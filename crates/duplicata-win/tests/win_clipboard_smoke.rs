#![cfg(windows)]

use std::sync::atomic::{AtomicU32, Ordering};

use duplicata_core::canonical::{CF_BITMAP, CF_DIB, CF_UNICODETEXT};
use duplicata_core::{CaptureOutcome, ClipboardSource, Config, RejectReason};
use duplicata_win::WinClipboard;
use windows::Win32::Foundation::{HANDLE, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::Graphics::Gdi::CreateBitmap;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, IsClipboardFormatAvailable, OpenClipboard,
    RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, HWND_MESSAGE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_RENDERALLFORMATS, WM_RENDERFORMAT, WNDCLASSW, WNDPROC, WS_POPUP,
};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn cfg_with_blocked(name: &str) -> Config {
    let mut c = cfg();
    c.blocked_programs = vec![name.to_string()];
    c
}

fn create_message_window() -> HWND {
    // SAFETY: classe do sistema ("STATIC"), sem WNDPROC customizado — só
    // precisamos de um HWND válido, nunca processamos mensagens nele.
    unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            windows::core::w!("STATIC"),
            windows::core::w!(""),
            WS_POPUP,
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            None,
            None,
        )
    }
    .expect("CreateWindowExW(STATIC) falhou")
}

fn minimal_dib_bytes() -> Vec<u8> {
    let mut dib = Vec::with_capacity(43);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&1i32.to_le_bytes());
    dib.extend_from_slice(&1i32.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&24u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&[0u8, 0, 0]);
    dib
}

fn hglobal_copy(bytes: &[u8]) -> Option<isize> {
    // SAFETY: alocação nova, `GMEM_MOVEABLE` exigido pelo clipboard para
    // dados que o sistema vai possuir depois de `SetClipboardData`.
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }.ok()?;
    // SAFETY: `hglobal` recém-alocado, ainda não travado.
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` aponta para pelo menos `bytes.len()` bytes válidos.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len()) };
    // SAFETY: pareado com o `GlobalLock` acima.
    let _ = unsafe { GlobalUnlock(hglobal) };
    Some(hglobal.0 as isize)
}

fn utf16le_with_nul(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .chain([0, 0])
        .collect()
}

/// Quantas vezes o dono sintético foi intimado a renderizar.
///
/// Estático de processo: os testes que o leem exigem `--test-threads=1`, que é
/// como o AGENTS manda rodar os `#[ignore]` deste crate.
static RENDER_REQUESTS: AtomicU32 = AtomicU32::new(0);

/// Dono de clipboard que anuncia com renderização adiada e nunca renderiza.
///
/// Só conta os pedidos: é a instrumentação do teste. Responder ao
/// `WM_RENDERFORMAT` não interessa aqui — o que se mede é SE ele chega.
extern "system" fn counting_owner_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_RENDERFORMAT || msg == WM_RENDERALLFORMATS {
        RENDER_REQUESTS.fetch_add(1, Ordering::SeqCst);
        return LRESULT(0);
    }
    // SAFETY: repassa o resto ao handler padrão.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// O texto que o dono que RENDERIZA de verdade entrega quando intimado.
const TEXTO_RENDERIZADO: &str = "conteudo que so existe quando pedido";

/// Id do segundo formato adiado, registrado em [`publish_two_delayed_formats_that_render`].
static SEGUNDO_FORMATO: AtomicU32 = AtomicU32::new(0);

/// Dono de clipboard que anuncia com renderização adiada e **entrega** quando
/// intimado — o comportamento de uma ponte WSL/RDP que funciona.
///
/// Responder ao `WM_RENDERFORMAT` é `SetClipboardData` de dentro do handler,
/// sem abrir o clipboard (quem pediu já o tem aberto). É esse
/// `SetClipboardData` que, no desenho antigo, caía no meio do laço de
/// `EnumClipboardFormats` de quem estava lendo.
extern "system" fn rendering_owner_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if msg == WM_RENDERFORMAT {
        RENDER_REQUESTS.fetch_add(1, Ordering::SeqCst);
        let format_id = wparam.0 as u32;
        let bytes = if format_id == CF_UNICODETEXT {
            utf16le_with_nul(TEXTO_RENDERIZADO)
        } else {
            b"carga auxiliar".to_vec()
        };
        if let Some(hglobal) = hglobal_copy(&bytes) {
            // SAFETY: resposta ao WM_RENDERFORMAT — `SetClipboardData` aqui é
            // feito SEM abrir o clipboard, como a API exige do dono adiado.
            unsafe {
                let _ =
                    SetClipboardData(format_id, Some(HANDLE(hglobal as *mut core::ffi::c_void)));
            }
        }
        return LRESULT(0);
    }
    if msg == WM_RENDERALLFORMATS {
        RENDER_REQUESTS.fetch_add(1, Ordering::SeqCst);
        return LRESULT(0);
    }
    // SAFETY: repassa o resto ao handler padrão.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Cria uma janela message-only da classe dada, registrando-a se preciso.
fn create_owner_window(class_name: windows::core::PCWSTR, wndproc: WNDPROC) -> HWND {
    // SAFETY: registro de classe idempotente com wndproc válido; ignoramos o
    // erro de "classe já registrada" porque vários testes deste arquivo podem
    // registrá-la.
    unsafe {
        let hinstance = GetModuleHandleW(None).expect("GetModuleHandleW falhou");
        let wc = WNDCLASSW {
            lpfnWndProc: wndproc,
            hInstance: hinstance.into(),
            lpszClassName: class_name,
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class_name,
            windows::core::w!(""),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance.into()),
            None,
        )
        .expect("CreateWindowExW do dono sintético falhou")
    }
}

/// Anuncia os formatos com renderização adiada, com `hwnd` como dono.
///
/// `SetClipboardData(fmt, NULL)` é o anúncio sem dados: os bytes só existem
/// quando alguém chama `GetClipboardData` e o dono responde ao
/// `WM_RENDERFORMAT`. É assim que uma ponte de clipboard entre máquinas (RDP,
/// WSL) publica — e é o caso que a ordem das fases precisa respeitar.
fn announce_delayed(hwnd: HWND, formats: &[u32]) {
    // SAFETY: `OpenClipboard(Some(hwnd))` + `EmptyClipboard` tornam esta janela
    // a DONA do clipboard, que é o que faz o Windows mandar `WM_RENDERFORMAT`
    // para ela. `SetClipboardData` com handle nulo é o anúncio adiado — o
    // Result é ignorado de propósito: o valor de retorno de um anúncio adiado é
    // o próprio handle nulo, então "erro" aqui não distingue falha de sucesso;
    // quem confirma a publicação é o `IsClipboardFormatAvailable` abaixo.
    unsafe {
        OpenClipboard(Some(hwnd)).expect("OpenClipboard falhou — outro processo com o clipboard?");
        EmptyClipboard().expect("EmptyClipboard falhou");
        for format_id in formats {
            let _ = SetClipboardData(*format_id, None);
        }
        let _ = CloseClipboard();
    }

    for format_id in formats {
        // SAFETY: consulta de disponibilidade, sem clipboard aberto e sem
        // handle — não obtém dado nenhum, logo não dispara renderização.
        let anunciado = unsafe { IsClipboardFormatAvailable(*format_id) }.is_ok();
        assert!(anunciado, "formato adiado {format_id} não foi anunciado");
    }
    RENDER_REQUESTS.store(0, Ordering::SeqCst);
}

/// Dono que anuncia CF_UNICODETEXT adiado e nunca renderiza.
fn publish_delayed_render_text() -> HWND {
    let hwnd = create_owner_window(
        windows::core::w!("duplicata_teste_dono_render_adiado"),
        Some(counting_owner_wndproc),
    );
    announce_delayed(hwnd, &[CF_UNICODETEXT]);
    hwnd
}

/// Dono que anuncia DOIS formatos adiados e renderiza os dois quando intimado.
///
/// Dois formatos, e não um, porque é isso que torna o teste discriminante: com
/// um formato só, a enumeração acabaria logo depois do primeiro
/// `GetClipboardData` e a modificação do clipboard no meio dela não teria
/// consequência observável.
fn publish_two_delayed_formats_that_render() -> HWND {
    // SAFETY: `RegisterClipboardFormatW` só registra/consulta um id junto ao
    // sistema — nenhuma pré-condição de clipboard aberto.
    let segundo =
        unsafe { RegisterClipboardFormatW(windows::core::w!("duplicata Teste Formato Adiado")) };
    assert!(segundo != 0, "RegisterClipboardFormatW falhou");
    SEGUNDO_FORMATO.store(segundo, Ordering::SeqCst);

    let hwnd = create_owner_window(
        windows::core::w!("duplicata_teste_dono_que_renderiza"),
        Some(rendering_owner_wndproc),
    );
    announce_delayed(hwnd, &[CF_UNICODETEXT, segundo]);
    hwnd
}

/// Solta a posse do clipboard antes de destruir a janela dona, para que o
/// Windows não peça `WM_RENDERALLFORMATS` na destruição e contamine a contagem
/// de um teste seguinte.
fn release_clipboard_and_window(hwnd: HWND) {
    // SAFETY: `OpenClipboard(None)` + `EmptyClipboard` zeram a posse; depois a
    // janela pode ser destruída sem obrigação de renderizar.
    unsafe {
        if OpenClipboard(None).is_ok() {
            let _ = EmptyClipboard();
            let _ = CloseClipboard();
        }
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn a_rejected_capture_never_asks_a_delayed_render_owner_to_produce_the_bytes() {
    // Este é o invariante da correção. Com o desenho antigo, `GetClipboardData`
    // rodava na fase de enumeração — antes de qualquer filtro — e obrigava o
    // dono a materializar os bytes mesmo quando a captura ia ser recusada.
    //
    // Com as fases separadas, uma recusa por programa bloqueado acontece só com
    // id, nome e dono: ZERO pedidos de renderização.
    let hwnd = publish_delayed_render_text();
    let this_exe = std::env::current_exe().expect("current_exe() falhou");
    let this_exe_name = this_exe
        .file_name()
        .expect("current_exe() sem nome de arquivo")
        .to_string_lossy()
        .into_owned();

    let outcome = WinClipboard.try_capture(&cfg_with_blocked(&this_exe_name));

    let pedidos = RENDER_REQUESTS.load(Ordering::SeqCst);
    release_clipboard_and_window(hwnd);

    match outcome {
        Ok(CaptureOutcome::Rejected(RejectReason::BlockedProgram)) => {}
        other => panic!("esperava Rejected(BlockedProgram), obtido {other:?}"),
    }
    assert_eq!(
        pedidos, 0,
        "captura recusada não pode disparar WM_RENDERFORMAT — os filtros rodam \
         antes de qualquer byte ser materializado"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn an_approved_capture_does_ask_the_owner_for_the_bytes() {
    // A contraparte do teste acima: aprovada a captura, a fase 3 pede os bytes.
    // Sem isso, "zero pedidos" seria satisfeito por um app que simplesmente não
    // captura mais nada.
    //
    // O dono sintético não responde, então nenhum formato é entregue e o
    // desfecho é `Empty`.
    let hwnd = publish_delayed_render_text();

    let outcome = WinClipboard.try_capture(&cfg());

    let pedidos = RENDER_REQUESTS.load(Ordering::SeqCst);
    release_clipboard_and_window(hwnd);

    assert!(
        pedidos >= 1,
        "captura aprovada precisa pedir os bytes na fase 3; obtido {pedidos} pedidos"
    );
    match outcome {
        Ok(CaptureOutcome::Empty) => {}
        other => panic!(
            "dono que não renderiza não entrega formato nenhum: esperava Empty, obtido {other:?}"
        ),
    }
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn a_delayed_render_owner_with_two_formats_is_captured_whole() {
    // Teste de CARACTERIZAÇÃO, não de regressão — e a distinção foi medida, não
    // suposta.
    //
    // Um dono anuncia dois formatos adiados e renderiza os dois quando
    // intimado: é a forma de uma ponte WSL/RDP que funciona. Rodando este teste
    // contra o desenho ANTIGO (só os `src` revertidos, este arquivo mantido),
    // ele PASSA. Ou seja: o `SetClipboardData` que caía no meio do laço de
    // `EnumClipboardFormats` não truncou a enumeração neste Windows, e este
    // teste não reproduz o defeito do usuário.
    //
    // O que ele prende é o caminho feliz completo do dono adiado: os dois
    // formatos chegam, e os bytes do canônico são os que o dono renderizou. Se
    // uma mudança futura quebrar a captura de renderização adiada, este teste
    // acusa.
    let hwnd = publish_two_delayed_formats_that_render();
    let segundo = SEGUNDO_FORMATO.load(Ordering::SeqCst);

    let outcome = WinClipboard.try_capture(&cfg());

    let pedidos = RENDER_REQUESTS.load(Ordering::SeqCst);
    release_clipboard_and_window(hwnd);

    let Ok(CaptureOutcome::Copied {
        formats,
        canonical_index,
    }) = outcome
    else {
        panic!("esperava Copied de um dono adiado que renderiza, obtido {outcome:?}");
    };

    assert_eq!(
        formats[canonical_index].format_id, CF_UNICODETEXT,
        "o canônico é o texto"
    );
    assert_eq!(
        formats[canonical_index].bytes,
        utf16le_with_nul(TEXTO_RENDERIZADO),
        "os bytes capturados têm de ser os que o dono renderizou"
    );
    assert!(
        formats.iter().any(|f| f.format_id == segundo),
        "o segundo formato adiado também tem de ser capturado — a enumeração \
         não pode ter sido truncada pelo render do primeiro"
    );
    assert!(
        pedidos >= 2,
        "os dois formatos precisam ter sido pedidos; obtido {pedidos}"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn sensitive_flagged_sentinel_format_is_recognized_and_rejects_the_capture() {
    // SAFETY: `RegisterClipboardFormatW` só registra/consulta um id junto ao
    // sistema — nenhuma pré-condição de clipboard aberto.
    let sentinel_id = unsafe {
        RegisterClipboardFormatW(windows::core::w!(
            "ExcludeClipboardContentFromMonitorProcessing"
        ))
    };
    assert!(sentinel_id != 0, "RegisterClipboardFormatW falhou");

    let text_hglobal = hglobal_copy(&utf16le_with_nul("não deveria ser capturado"))
        .expect("GlobalAlloc do texto sintético falhou");
    let sentinel_hglobal = hglobal_copy(&[]).expect("GlobalAlloc vazio do sentinela falhou");

    // SAFETY: `OpenClipboard(None)` sem janela-dona; `CloseClipboard` sempre
    // chamado antes de retornar, mesma disciplina do teste BUG1 acima.
    unsafe {
        OpenClipboard(None).expect("OpenClipboard falhou — outro processo com o clipboard aberto?");
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            CF_UNICODETEXT,
            Some(HANDLE(text_hglobal as *mut core::ffi::c_void)),
        )
        .expect("SetClipboardData(CF_UNICODETEXT) falhou");
        SetClipboardData(
            sentinel_id,
            Some(HANDLE(sentinel_hglobal as *mut core::ffi::c_void)),
        )
        .expect("SetClipboardData(sentinela) falhou");
        let _ = CloseClipboard();
    }

    match WinClipboard.try_capture(&cfg()) {
        Ok(CaptureOutcome::Rejected(RejectReason::SensitiveFlagged)) => {}
        other => panic!("esperava Rejected(SensitiveFlagged), obtido {other:?}"),
    }
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn source_program_resolves_to_the_current_process_and_blocking_works_end_to_end() {
    let hwnd = create_message_window();
    let this_exe = std::env::current_exe().expect("current_exe() falhou");
    let this_exe_name = this_exe
        .file_name()
        .expect("current_exe() sem nome de arquivo")
        .to_string_lossy()
        .into_owned();

    let text_hglobal = hglobal_copy(&utf16le_with_nul("copiado por este processo de teste"))
        .expect("GlobalAlloc do texto sintético falhou");

    // SAFETY: `OpenClipboard(Some(hwnd))` — DIFERENTE do padrão
    // `OpenClipboard(None)` dos outros testes deste arquivo: aqui
    // precisamos que ESTE processo vire o dono do clipboard (é isso que
    // `EmptyClipboard` faz, associando a posse ao HWND passado a
    // `OpenClipboard`), para que `GetClipboardOwner` dentro de
    // `WinClipboard::try_capture` resolva de volta para este processo.
    unsafe {
        OpenClipboard(Some(hwnd))
            .expect("OpenClipboard falhou — outro processo com o clipboard aberto?");
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            CF_UNICODETEXT,
            Some(HANDLE(text_hglobal as *mut core::ffi::c_void)),
        )
        .expect("SetClipboardData(CF_UNICODETEXT) falhou");
        let _ = CloseClipboard();
    }

    match WinClipboard.try_capture(&cfg_with_blocked(&this_exe_name)) {
        Ok(CaptureOutcome::Rejected(RejectReason::BlockedProgram)) => {}
        other => panic!(
            "esperava Rejected(BlockedProgram) com '{this_exe_name}' bloqueado, obtido {other:?}"
        ),
    }

    match WinClipboard.try_capture(&cfg_with_blocked("programa-que-nao-e-este-processo.exe")) {
        Ok(CaptureOutcome::Copied { .. }) => {}
        other => panic!("esperava Copied com outro nome bloqueado, obtido {other:?}"),
    }

    // SAFETY: janela message-only criada só para este teste.
    let _ = unsafe { DestroyWindow(hwnd) };
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn source_program_resolution_is_none_without_panicking_when_there_is_no_clipboard_owner() {
    // SAFETY: `OpenClipboard(None)` — sem janela dona — seguido de
    // `EmptyClipboard` deixa o clipboard sem NENHUM dono associado.
    unsafe {
        OpenClipboard(None).expect("OpenClipboard falhou — outro processo com o clipboard aberto?");
        EmptyClipboard().expect("EmptyClipboard falhou");
        let _ = CloseClipboard();
    }

    match WinClipboard.try_capture(&cfg()) {
        Ok(CaptureOutcome::Empty) => {}
        other => panic!("esperava Empty (clipboard vazio, sem dono), obtido {other:?}"),
    }
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard"]
fn try_capture_reads_current_clipboard_or_reports_a_known_outcome() {
    match WinClipboard.try_capture(&cfg()) {
        Ok(CaptureOutcome::Copied {
            formats,
            canonical_index,
        }) => {
            println!(
                "capturados {} formatos (canônico = índice {canonical_index}):",
                formats.len()
            );
            for f in &formats {
                println!(
                    "  id={} nome={:?} bytes={}",
                    f.format_id,
                    f.format_name,
                    f.bytes.len()
                );
            }
            assert!(!formats.is_empty());
        }
        Ok(CaptureOutcome::Empty) => println!("área de transferência vazia — ok"),
        Ok(CaptureOutcome::Rejected(reason)) => {
            println!("recusado por tamanho/sem canônico: {reason:?} — ok")
        }
        Err(e) => panic!("erro inesperado: {e:?}"),
    }
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn bug1_non_hglobal_cf_bitmap_alongside_cf_dib_is_ignored_without_crashing() {
    // SAFETY: `CreateBitmap` monocromático 1x1 sem dados iniciais — só para
    // ter um `HBITMAP` real (não-HGLOBAL) para publicar como CF_BITMAP.
    let hbitmap = unsafe { CreateBitmap(1, 1, 1, 1, None) };
    assert!(!hbitmap.is_invalid(), "CreateBitmap falhou");

    let dib_hglobal =
        hglobal_copy(&minimal_dib_bytes()).expect("GlobalAlloc do CF_DIB sintético falhou");

    // SAFETY: `OpenClipboard(None)` sem janela-dona; `CloseClipboard` sempre
    // chamado antes de qualquer retorno desta função (via bloco abaixo).
    unsafe {
        OpenClipboard(None).expect("OpenClipboard falhou — outro processo com o clipboard aberto?");
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(CF_BITMAP, Some(HANDLE(hbitmap.0)))
            .expect("SetClipboardData(CF_BITMAP) falhou");
        SetClipboardData(CF_DIB, Some(HANDLE(dib_hglobal as *mut core::ffi::c_void)))
            .expect("SetClipboardData(CF_DIB) falhou");
        let _ = CloseClipboard();
    }

    match WinClipboard.try_capture(&cfg()) {
        Ok(CaptureOutcome::Copied {
            formats,
            canonical_index,
        }) => {
            assert!(
                !formats.iter().any(|f| f.format_id == CF_BITMAP),
                "CF_BITMAP (não-HGLOBAL) não deveria aparecer entre os formatos capturados"
            );
            assert!(
                formats.iter().any(|f| f.format_id == CF_DIB),
                "CF_DIB deveria continuar sendo capturado normalmente"
            );
            assert_eq!(
                formats[canonical_index].format_id, CF_DIB,
                "canônico deveria ser CF_DIB mesmo com CF_BITMAP ausente da lista positiva"
            );
        }
        other => panic!("esperado Copied{{..}}, obtido {other:?}"),
    }
}
