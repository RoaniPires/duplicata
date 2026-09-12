#![cfg(windows)]

use duplicata_core::canonical::{CF_BITMAP, CF_DIB, CF_UNICODETEXT};
use duplicata_core::{CaptureOutcome, ClipboardSource, Config, RejectReason};
use duplicata_win::WinClipboard;
use windows::Win32::Foundation::{HANDLE, HWND};
use windows::Win32::Graphics::Gdi::CreateBitmap;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WS_POPUP,
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
