//! Reprodução do issue #10: a cópia feita pelo `Set-Clipboard` do PowerShell
//! 5.1 (via OLE) some quando o duplicata captura no meio dela.
//!
//! O `OleSetClipboard` publica em três atualizações, medidas com o exemplo
//! `clipboard_trace`:
//!
//! - u1: só "DataObject" (com bytes) e "Ole Private Data" (adiado);
//! - u2 (+3 ms): acrescenta `CF_UNICODETEXT` e `CF_LOCALE` adiados;
//! - u3 (+12 ms, o flush): grava os bytes de verdade.
//!
//! Entre uma atualização e a seguinte a thread do PowerShell tenta reabrir a
//! área de transferência. Medido: com o duplicata lendo nesse meio-tempo, o
//! flush não aconteceu em 3 de 9 cópias e o texto sumiu.
//!
//! **O que estes testes provam, e o que não provam.** Eles provam a regra da
//! correção: com um dono OLE que não responde a tempo, o duplicata não abre a
//! área nem pede formato adiado, recusa o estado só de controle do OLE, e o
//! texto do flush chega inteiro ao histórico. Eles **não** reproduzem o
//! PowerShell real. O dono imitado é um modelo, com uma suposição:
//!
//! - **Suposição:** o PowerShell atende os pedidos de render dos outros
//!   programas da sessão e não atende o do duplicata a tempo. Ela vem só da
//!   medição (desligado, 16 de 16 cópias com o `msrdc.exe` do WSLg aberto, que
//!   lê a área assim que aparece texto; ligado, a perda), não de saber como o
//!   OLE trata cada pedido. Por isso a auxiliar **bombeia as mensagens
//!   enviadas** enquanto espera: renderiza o pedido de outro programa, não
//!   responde ao pedido feito pelo listener deste processo até ser "morta", e
//!   responde à sonda `WM_NULL` depois do prazo dela.
//!
//! A prova causal com o PowerShell real é o aceite manual (20 cópias no Claude
//! Code com o duplicata ligado e o exemplo `clipboard_trace` gravando junto).
//!
//! Os testes sobrescrevem a área de transferência real. Rode com o duplicata
//! encerrado:
//! `cargo test -p duplicata-win --test ole_flush_race_smoke -- --ignored --test-threads=1 --nocapture`

#![cfg(windows)]

use std::cell::Cell;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use duplicata_core::canonical::{CF_LOCALE, CF_UNICODETEXT};
use duplicata_core::{
    ByteBudgetQueue, CanonicalKind, CaptureQueue, Config, SelfWriteFilter, WorkItem,
};
use duplicata_win::ClipboardListener;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HANDLE, HGLOBAL, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, GetClipboardData, GetClipboardSequenceNumber,
    GetOpenClipboardWindow, OpenClipboard, RegisterClipboardFormatW, SetClipboardData,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW, GetWindowThreadProcessId,
    MsgWaitForMultipleObjects, PeekMessageW, RegisterClassW, HWND_MESSAGE, MSG, PM_NOREMOVE,
    PM_QS_SENDMESSAGE, PM_REMOVE, QS_ALLINPUT, QS_SENDMESSAGE, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_CLIPBOARDUPDATE, WM_NULL, WM_RENDERALLFORMATS, WM_RENDERFORMAT, WNDCLASSW,
};

const TEXTO: &str = "cargo build --workspace";

/// Pausa entre as tentativas de reabrir a área entre u1 e u2. Valor suposto:
/// imita a continuação do `OleSetClipboard`.
const REOPEN_PAUSE: Duration = Duration::from_millis(10);
/// Intervalo medido entre u2 e u3 antes da primeira tentativa do flush.
const FLUSH_FIRST_PAUSE: Duration = Duration::from_millis(12);
/// Pausa entre as tentativas do flush.
const FLUSH_PAUSE: Duration = Duration::from_millis(100);
/// Quando a auxiliar "é morta": o Claude Code mata o PowerShell 2 s depois de
/// iniciá-lo; 1,2 s basta para o teste e deixa a execução curta.
const KILL_AFTER: Duration = Duration::from_millis(1200);
/// Mais que o `OWNER_PROBE_TIMEOUT` de 20 ms: a sonda vê o dono calado.
const PROBE_ANSWER_DELAY: Duration = Duration::from_millis(60);
const DEADLINE: Duration = Duration::from_secs(10);

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

/// Quem atrasou o dono OLE.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Bloqueio {
    /// O listener deste processo: a área estava aberta sem janela (como o
    /// duplicata abre) enquanto o listener tratava uma notificação.
    DesteProcesso,
    /// Outro programa da sessão segurou a área. Não é o que este teste mede.
    OutroProcesso { pid: u32 },
    /// Aberta sem janela fora do tratamento de uma notificação do listener:
    /// não dá para dizer quem é (pode ser outro programa).
    SemJanela,
}

/// O que a auxiliar viu na execução corrente. Estático de processo: os testes
/// deste arquivo exigem `--test-threads=1`.
static BLOQUEIOS: Mutex<Vec<Bloqueio>> = Mutex::new(Vec::new());

/// Verdadeiro enquanto a thread do teste despacha um `WM_CLIPBOARDUPDATE` ao
/// listener: é o único código deste processo que abre a área sem janela
/// enquanto a auxiliar trabalha.
static IN_LISTENER: AtomicBool = AtomicBool::new(false);

/// Quem está com a área aberta agora.
///
/// O Windows não diz quem abriu sem janela. Aberta sem janela enquanto o
/// listener trata uma notificação conta como dele; fora disso, é
/// indeterminado. Resta uma ambiguidade estreita: outro programa abrindo sem
/// janela exatamente enquanto o listener trata uma notificação.
fn quem_segura_a_area() -> Bloqueio {
    // SAFETY: só consulta qual janela está com a área aberta.
    let Ok(window) = (unsafe { GetOpenClipboardWindow() }) else {
        return if IN_LISTENER.load(Ordering::SeqCst) {
            Bloqueio::DesteProcesso
        } else {
            Bloqueio::SemJanela
        };
    };
    let mut pid = 0u32;
    // SAFETY: `window` veio do sistema; se já morreu, `pid` fica 0.
    let _ = unsafe { GetWindowThreadProcessId(window, Some(&mut pid)) };
    if pid == std::process::id() {
        Bloqueio::DesteProcesso
    } else if pid == 0 {
        Bloqueio::SemJanela
    } else {
        Bloqueio::OutroProcesso { pid }
    }
}

thread_local! {
    /// Instante em que o processo imitado é morto (thread da auxiliar).
    static KILL_AT: Cell<Option<Instant>> = const { Cell::new(None) };
    /// Se o processo imitado já foi morto.
    static KILLED: Cell<bool> = const { Cell::new(false) };
}

fn killed() -> bool {
    KILLED.with(Cell::get) || KILL_AT.with(|k| k.get().is_some_and(|at| Instant::now() >= at))
}

/// Bytes que o dono imitado entrega para cada formato adiado.
fn rendered_bytes(format_id: u32) -> Option<Vec<u8>> {
    if format_id == CF_UNICODETEXT {
        Some(utf16le_with_nul(TEXTO))
    } else if format_id == CF_LOCALE {
        Some(0x0416u32.to_le_bytes().to_vec())
    } else if format_id == ole_private_data_id() {
        Some(vec![2, 0, 0, 0, 0, 0, 0, 0])
    } else {
        None
    }
}

/// Dono OLE de mentira.
///
/// - `WM_RENDERFORMAT` de outro programa: renderiza (o PowerShell atende os
///   outros leitores da sessão).
/// - `WM_RENDERFORMAT` deste processo: não responde até ser morto, e então
///   volta sem renderizar — o impasse que o issue #10 mediu.
/// - `WM_NULL`: só responde depois de [`PROBE_ANSWER_DELAY`].
/// - `WM_RENDERALLFORMATS` da destruição: ignorado, como o processo morto.
extern "system" fn ole_owner_wndproc(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    match msg {
        WM_RENDERFORMAT => {
            let format_id = wparam.0 as u32;
            match quem_segura_a_area() {
                Bloqueio::OutroProcesso { .. } | Bloqueio::SemJanela => {
                    if let Some(bytes) = rendered_bytes(format_id) {
                        // SAFETY: resposta a `WM_RENDERFORMAT`: quem pediu já
                        // tem a área aberta, e o `HGLOBAL` passa ao sistema.
                        let _ = unsafe { SetClipboardData(format_id, Some(hglobal_copy(&bytes))) };
                    }
                }
                Bloqueio::DesteProcesso => {
                    BLOQUEIOS.lock().unwrap().push(Bloqueio::DesteProcesso);
                    while !killed() {
                        std::thread::sleep(REOPEN_PAUSE);
                    }
                    KILLED.with(|k| k.set(true));
                }
            }
            LRESULT(0)
        }
        WM_RENDERALLFORMATS => LRESULT(0),
        WM_NULL => {
            std::thread::sleep(PROBE_ANSWER_DELAY);
            // SAFETY: repassa ao handler padrão, com os argumentos recebidos.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: repassa o resto ao handler padrão, com os argumentos recebidos.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn create_ole_owner() -> HWND {
    let class = windows::core::w!("duplicata_teste_dono_ole");
    // SAFETY: registro de classe idempotente com wndproc válido (o erro de
    // "classe já registrada" dos testes seguintes é ignorado de propósito).
    unsafe {
        let hinstance = GetModuleHandleW(None).expect("GetModuleHandleW falhou");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(ole_owner_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE(0),
            class,
            PCWSTR::null(),
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
        .expect("CreateWindowExW do dono OLE falhou")
    }
}

fn registered(name: PCWSTR) -> u32 {
    // SAFETY: só registra/consulta um id junto ao sistema.
    let id = unsafe { RegisterClipboardFormatW(name) };
    assert!(id != 0, "RegisterClipboardFormatW falhou");
    id
}

fn data_object_id() -> u32 {
    registered(windows::core::w!("DataObject"))
}

fn ole_private_data_id() -> u32 {
    registered(windows::core::w!("Ole Private Data"))
}

fn hglobal_copy(bytes: &[u8]) -> HANDLE {
    // SAFETY: alocação nova, `GMEM_MOVEABLE` como o clipboard exige.
    let hglobal =
        unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }.expect("GlobalAlloc falhou");
    // SAFETY: `hglobal` recém-alocado, ainda não travado.
    let ptr = unsafe { GlobalLock(hglobal) };
    assert!(!ptr.is_null(), "GlobalLock falhou");
    // SAFETY: `ptr` aponta para pelo menos `bytes.len()` bytes.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len()) };
    // SAFETY: pareado com o `GlobalLock` acima.
    let _ = unsafe { GlobalUnlock(hglobal) };
    HANDLE(hglobal.0)
}

fn utf16le_with_nul(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .chain([0, 0])
        .collect()
}

/// Espera até `pause`, despachando só as mensagens **enviadas** a esta thread
/// (os `WM_RENDERFORMAT` e a sonda), nunca as postadas.
fn wait_serving_sent_messages(pause: Duration) {
    let end = Instant::now() + pause;
    loop {
        let mut msg = MSG::default();
        // SAFETY: `msg` local; com `PM_QS_SENDMESSAGE` a chamada só entrega as
        // mensagens enviadas ao wndproc e não tira nada da fila.
        let _ = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_NOREMOVE | PM_QS_SENDMESSAGE) };
        let left = end.saturating_duration_since(Instant::now());
        if left.is_zero() || killed() {
            return;
        }
        // SAFETY: nenhum handle; acorda com mensagem enviada ou no prazo.
        unsafe {
            MsgWaitForMultipleObjects(None, false, left.as_millis() as u32, QS_SENDMESSAGE);
        }
    }
}

/// Tenta abrir a área com `owner` até o processo imitado ser morto. Anota em
/// [`BLOQUEIOS`] quem segurava a área a cada recusa.
fn open_until_killed(owner: HWND, first_pause: Duration, pause: Duration) -> bool {
    wait_serving_sent_messages(first_pause);
    while !killed() {
        // SAFETY: `owner` é janela desta thread; falha só quer dizer ocupada.
        if unsafe { OpenClipboard(Some(owner)) }.is_ok() {
            return true;
        }
        BLOQUEIOS.lock().unwrap().push(quem_segura_a_area());
        wait_serving_sent_messages(pause);
    }
    false
}

/// Até onde o `Set-Clipboard` imitado chegou.
#[derive(Debug, PartialEq, Eq)]
enum Fim {
    Flushed,
    MortoAntesDeU1,
    MortoAntesDeU2,
    MortoNoFlush,
}

/// O que a auxiliar relata: até onde chegou e o número de sequência que a
/// última escrita dela deixou.
#[derive(Debug)]
struct Relato {
    fim: Fim,
    seq_final: u32,
}

fn sequence_now() -> u32 {
    // SAFETY: só lê um contador global do sistema.
    unsafe { GetClipboardSequenceNumber() }
}

/// Imita o `Set-Clipboard` do PowerShell 5.1: u1, u2 e o flush, nesta ordem.
/// Morto (ver [`KILL_AFTER`]), a janela é destruída sem renderizar e os
/// formatos adiados somem.
fn ole_set_clipboard_like_powershell() -> Relato {
    KILL_AT.with(|k| k.set(Some(Instant::now() + KILL_AFTER)));
    KILLED.with(|k| k.set(false));
    let data_object = data_object_id();
    let ole_private = ole_private_data_id();
    let owner = create_ole_owner();
    let morto = |fim: Fim| {
        // SAFETY: `owner` é janela desta thread; destruí-la sem renderizar é
        // o que o processo morto deixa para trás.
        let _ = unsafe { DestroyWindow(owner) };
        Relato {
            fim,
            seq_final: sequence_now(),
        }
    };

    if !open_until_killed(owner, Duration::ZERO, REOPEN_PAUSE) {
        return morto(Fim::MortoAntesDeU1);
    }
    // SAFETY: área aberta por `owner`; `EmptyClipboard` o torna dono, condição
    // do anúncio adiado (`SetClipboardData(fmt, None)`), cujo resultado (o
    // próprio handle nulo) não distingue erro e é ignorado.
    unsafe {
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(data_object, Some(hglobal_copy(&[1, 0, 0, 0, 0, 0, 0, 0])))
            .expect("SetClipboardData(DataObject) falhou");
        let _ = SetClipboardData(ole_private, None);
        let _ = CloseClipboard();
    }

    if !open_until_killed(owner, Duration::ZERO, REOPEN_PAUSE) {
        return morto(Fim::MortoAntesDeU2);
    }
    // SAFETY: área aberta por `owner`, dono desde o u1; dois anúncios adiados.
    unsafe {
        let _ = SetClipboardData(CF_UNICODETEXT, None);
        let _ = SetClipboardData(CF_LOCALE, None);
        let _ = CloseClipboard();
    }

    if !open_until_killed(owner, FLUSH_FIRST_PAUSE, FLUSH_PAUSE) {
        return morto(Fim::MortoNoFlush);
    }
    // SAFETY: área aberta por `owner`; os `HGLOBAL` passam ao sistema.
    unsafe {
        for format_id in [CF_UNICODETEXT, CF_LOCALE, ole_private] {
            let bytes = rendered_bytes(format_id).expect("formato adiado conhecido");
            SetClipboardData(format_id, Some(hglobal_copy(&bytes)))
                .expect("SetClipboardData do flush falhou");
        }
        let _ = CloseClipboard();
    }
    let seq_final = sequence_now();
    // SAFETY: `owner` é desta thread e todos os formatos já têm bytes.
    let _ = unsafe { DestroyWindow(owner) };
    Relato {
        fim: Fim::Flushed,
        seq_final,
    }
}

/// O estado que sobrou nas cópias perdidas: [DataObject, Ole Private Data,
/// CF_LOCALE], todos com bytes, e nenhum dono.
fn publish_orphan_ole_bookkeeping() -> Relato {
    KILL_AT.with(|k| k.set(Some(Instant::now() + KILL_AFTER)));
    KILLED.with(|k| k.set(false));
    let owner = create_ole_owner();
    assert!(
        open_until_killed(owner, Duration::ZERO, REOPEN_PAUSE),
        "OpenClipboard falhou por {KILL_AFTER:?} — outro processo com a área aberta?"
    );
    // SAFETY: área aberta por `owner`; os `HGLOBAL` passam ao sistema e a
    // janela é destruída em seguida, deixando a área sem dono.
    unsafe {
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            data_object_id(),
            Some(hglobal_copy(&[1, 0, 0, 0, 0, 0, 0, 0])),
        )
        .expect("SetClipboardData(DataObject) falhou");
        SetClipboardData(
            ole_private_data_id(),
            Some(hglobal_copy(&[2, 0, 0, 0, 0, 0, 0, 0])),
        )
        .expect("SetClipboardData(Ole Private Data) falhou");
        SetClipboardData(CF_LOCALE, Some(hglobal_copy(&0x0416u32.to_le_bytes())))
            .expect("SetClipboardData(CF_LOCALE) falhou");
        let _ = CloseClipboard();
    }
    let seq_final = sequence_now();
    // SAFETY: `owner` é desta thread e todos os formatos já têm bytes.
    let _ = unsafe { DestroyWindow(owner) };
    Relato {
        fim: Fim::Flushed,
        seq_final,
    }
}

/// Bombeia a thread do listener até a auxiliar terminar e o listener ter
/// tratado uma notificação já com a área no estado final que a auxiliar
/// deixou (`seq_final`). Com prazo de [`DEADLINE`]. Se a auxiliar foi morta,
/// para assim que ela termina: o resultado já é a falha.
fn pump_until_done(helper: JoinHandle<Relato>, listener: HWND) -> Relato {
    let start = Instant::now();
    let mut handled_seq: Option<u32> = None;
    let mut relato: Option<Relato> = None;
    let mut helper = Some(helper);
    loop {
        let mut msg = MSG::default();
        // SAFETY: `msg` local; fila da thread do teste.
        while unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool() {
            let is_update = msg.message == WM_CLIPBOARDUPDATE && msg.hwnd == listener;
            let seq = sequence_now();
            IN_LISTENER.store(is_update, Ordering::SeqCst);
            // SAFETY: `msg` acabou de ser preenchida por `PeekMessageW`.
            unsafe { DispatchMessageW(&msg) };
            IN_LISTENER.store(false, Ordering::SeqCst);
            if is_update {
                handled_seq = Some(handled_seq.map_or(seq, |s| s.max(seq)));
            }
        }
        if helper.as_ref().is_some_and(JoinHandle::is_finished) {
            let r = helper
                .take()
                .expect("acabou de ser conferida")
                .join()
                .expect("a thread auxiliar entrou em pânico");
            relato = Some(r);
        }
        if let Some(r) = &relato {
            if r.fim != Fim::Flushed || handled_seq.is_some_and(|s| s >= r.seq_final) {
                return relato.expect("acabou de ser conferido");
            }
        }
        assert!(
            start.elapsed() < DEADLINE,
            "em 10 s o listener não tratou o estado final (auxiliar: {relato:?}; última sequência tratada: {handled_seq:?})"
        );
        // SAFETY: nenhum handle; só espera a próxima mensagem, com prazo.
        unsafe {
            MsgWaitForMultipleObjects(None, false, 20, QS_ALLINPUT);
        }
    }
}

/// Texto em `CF_UNICODETEXT` na área agora, ou `None`.
fn clipboard_text() -> Option<String> {
    let deadline = Instant::now() + Duration::from_secs(2);
    // SAFETY: abre sem dono só para ler; `CloseClipboard` sempre ao fim. O
    // handle é lido e destravado dentro da mesma sessão.
    unsafe {
        while OpenClipboard(None).is_err() {
            assert!(Instant::now() < deadline, "OpenClipboard falhou por 2 s");
            std::thread::yield_now();
        }
        let text = GetClipboardData(CF_UNICODETEXT).ok().and_then(|h| {
            let hglobal = HGLOBAL(h.0);
            let ptr = GlobalLock(hglobal).cast::<u16>();
            if ptr.is_null() {
                return None;
            }
            let mut len = 0;
            while *ptr.add(len) != 0 {
                len += 1;
            }
            let s = String::from_utf16_lossy(std::slice::from_raw_parts(ptr, len));
            let _ = GlobalUnlock(hglobal);
            Some(s)
        });
        let _ = CloseClipboard();
        text
    }
}

/// (tipo do canônico, id do canônico, texto se for texto) de cada captura.
fn drain_captures(queue: &ByteBudgetQueue<WorkItem>) -> Vec<(CanonicalKind, u32, Option<String>)> {
    // `pop` bloqueia com a fila vazia até ela ser fechada.
    queue.close();
    let mut out = Vec::new();
    while let Some(item) = queue.pop() {
        if let WorkItem::Capture(raw) = item {
            let text = (raw.canonical.kind == CanonicalKind::UnicodeText).then(|| {
                let units: Vec<u16> = raw
                    .canonical_bytes()
                    .chunks_exact(2)
                    .map(|c| u16::from_le_bytes([c[0], c[1]]))
                    .take_while(|&u| u != 0)
                    .collect();
                String::from_utf16_lossy(&units)
            });
            out.push((raw.canonical.kind, raw.canonical.format_id, text));
        }
    }
    out
}

type Capturas = Vec<(CanonicalKind, u32, Option<String>)>;

fn run_with_listener(helper: fn() -> Relato) -> (Relato, Vec<Bloqueio>, Capturas) {
    let queue = Arc::new(ByteBudgetQueue::<WorkItem>::new());
    let listener = ClipboardListener::new(
        Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>,
        cfg(),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("registrar o listener precisa de sessão gráfica");

    BLOQUEIOS.lock().unwrap().clear();
    let relato = pump_until_done(std::thread::spawn(helper), listener.hwnd());
    drop(listener);
    let bloqueios = BLOQUEIOS.lock().unwrap().clone();
    (relato, bloqueios, drain_captures(&queue))
}

#[test]
#[ignore = "precisa de sessão gráfica; sobrescreve a área de transferência"]
fn a_powershell_set_clipboard_keeps_its_text_while_the_listener_runs() {
    let (relato, bloqueios, captures) = run_with_listener(ole_set_clipboard_like_powershell);

    assert!(
        !bloqueios.contains(&Bloqueio::DesteProcesso),
        "o listener segurou a área ou pediu um formato adiado enquanto o dono OLE esperava (auxiliar: {relato:?}; bloqueios: {bloqueios:?})"
    );
    assert_eq!(
        relato.fim,
        Fim::Flushed,
        "inconclusivo: o dono OLE foi morto sem o listener ter segurado a área; outro programa, ou alguém sem janela, a segurou ({bloqueios:?})"
    );
    assert_eq!(
        clipboard_text().as_deref(),
        Some(TEXTO),
        "o texto tem que sobreviver na área de transferência"
    );
    assert_eq!(
        captures,
        vec![(
            CanonicalKind::UnicodeText,
            CF_UNICODETEXT,
            Some(TEXTO.to_string())
        )],
        "exatamente uma captura, de texto, com o texto copiado, e nenhuma do controle do OLE"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; sobrescreve a área de transferência"]
fn an_orphan_ole_bookkeeping_state_is_not_captured() {
    let (_, _, captures) = run_with_listener(publish_orphan_ole_bookkeeping);
    assert!(
        captures.is_empty(),
        "só formatos de controle do OLE não são conteúdo; vieram {captures:?}"
    );
}

/// Dono OLE numa thread que não responde a tempo: anuncia "DataObject" com
/// bytes e `CF_UNICODETEXT` adiado e fica esperando `release` (ou 3 s),
/// atendendo só os outros programas da sessão.
fn park_silent_ole_owner(
    announced: std::sync::mpsc::Sender<()>,
    release: std::sync::mpsc::Receiver<()>,
) {
    KILL_AT.with(|k| k.set(Some(Instant::now() + Duration::from_secs(3))));
    KILLED.with(|k| k.set(false));
    let owner = create_ole_owner();
    assert!(
        open_until_killed(owner, Duration::ZERO, REOPEN_PAUSE),
        "OpenClipboard falhou — outro processo com a área aberta?"
    );
    // SAFETY: área aberta por `owner`; `EmptyClipboard` o torna dono, condição
    // do anúncio adiado, cujo resultado (handle nulo) é ignorado.
    unsafe {
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            data_object_id(),
            Some(hglobal_copy(&[1, 0, 0, 0, 0, 0, 0, 0])),
        )
        .expect("SetClipboardData(DataObject) falhou");
        let _ = SetClipboardData(CF_UNICODETEXT, None);
        let _ = CloseClipboard();
    }
    let _ = announced.send(());
    while release.try_recv().is_err() && !killed() {
        wait_serving_sent_messages(REOPEN_PAUSE);
    }
    // SAFETY: `owner` é desta thread; destruí-la sem renderizar deixa a área
    // sem o texto adiado, como o processo morto.
    let _ = unsafe { DestroyWindow(owner) };
}

#[test]
#[ignore = "precisa de sessão gráfica; sobrescreve a área de transferência"]
fn the_capture_does_not_open_the_clipboard_while_an_ole_owner_does_not_answer() {
    use duplicata_core::{CaptureError, ClipboardSource};
    use duplicata_win::WinClipboard;

    let (announced_tx, announced_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let helper = std::thread::spawn(move || park_silent_ole_owner(announced_tx, release_rx));
    announced_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("o dono OLE não anunciou em 5 s");

    let started = Instant::now();
    let result = WinClipboard.try_capture_if_owner_ready(&cfg());
    let took = started.elapsed();
    let _ = release_tx.send(());
    helper.join().expect("a thread do dono entrou em pânico");

    assert_eq!(
        result,
        Err(CaptureError::OwnerNotResponding),
        "com o dono OLE calado, a captura não abre a área (levou {took:?})"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; sobrescreve a área de transferência"]
fn an_ole_owner_that_answers_is_read_as_before() {
    use duplicata_core::{CaptureOutcome, ClipboardSource};
    use duplicata_win::WinClipboard;

    // Dono nesta thread: a sonda vira chamada direta ao wndproc, que demora
    // `PROBE_ANSWER_DELAY` mas responde, porque na mesma thread o prazo da
    // sonda não se aplica. É o dono que bombeia.
    KILL_AT.with(|k| k.set(Some(Instant::now() + KILL_AFTER)));
    KILLED.with(|k| k.set(false));
    let owner = create_ole_owner();
    assert!(open_until_killed(owner, Duration::ZERO, REOPEN_PAUSE));
    // SAFETY: área aberta por `owner`; os `HGLOBAL` passam ao sistema.
    unsafe {
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            data_object_id(),
            Some(hglobal_copy(&[1, 0, 0, 0, 0, 0, 0, 0])),
        )
        .expect("SetClipboardData(DataObject) falhou");
        SetClipboardData(CF_UNICODETEXT, Some(hglobal_copy(&utf16le_with_nul(TEXTO))))
            .expect("SetClipboardData(CF_UNICODETEXT) falhou");
        let _ = CloseClipboard();
    }

    let result = WinClipboard.try_capture_if_owner_ready(&cfg());
    // SAFETY: `owner` é desta thread e todos os formatos já têm bytes.
    let _ = unsafe { DestroyWindow(owner) };

    match result {
        Ok(CaptureOutcome::Copied {
            formats,
            canonical_index,
        }) => assert_eq!(formats[canonical_index].format_id, CF_UNICODETEXT),
        other => panic!("esperava a cópia do texto, veio {other:?}"),
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; sobrescreve a área de transferência"]
fn a_blocked_program_is_reported_before_ole_bookkeeping_without_opening() {
    use duplicata_core::{CaptureOutcome, ClipboardSource, RejectReason};
    use duplicata_win::WinClipboard;

    // Mesma ordem do `screen`: programa bloqueado antes do controle do OLE.
    KILL_AT.with(|k| k.set(Some(Instant::now() + KILL_AFTER)));
    KILLED.with(|k| k.set(false));
    let owner = create_ole_owner();
    assert!(open_until_killed(owner, Duration::ZERO, REOPEN_PAUSE));
    // SAFETY: área aberta por `owner`; os `HGLOBAL` passam ao sistema.
    unsafe {
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            data_object_id(),
            Some(hglobal_copy(&[1, 0, 0, 0, 0, 0, 0, 0])),
        )
        .expect("SetClipboardData(DataObject) falhou");
        SetClipboardData(
            ole_private_data_id(),
            Some(hglobal_copy(&[2, 0, 0, 0, 0, 0, 0, 0])),
        )
        .expect("SetClipboardData(Ole Private Data) falhou");
        let _ = CloseClipboard();
    }
    let this_exe = std::env::current_exe()
        .expect("current_exe() falhou")
        .file_name()
        .expect("current_exe() sem nome de arquivo")
        .to_string_lossy()
        .into_owned();
    let mut blocked = cfg();
    blocked.blocked_programs = vec![this_exe];

    // Outra thread segura a área durante a chamada: se a captura tentasse
    // abrir, o resultado seria `Err(Busy)`. Só a decisão tomada antes de
    // abrir devolve o motivo.
    let (holding_tx, holding_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel::<()>();
    let holder = std::thread::spawn(move || {
        // Com janela: aberta sem janela por outra thread deste processo, a
        // área não recusa o `OpenClipboard(NULL)` da captura (medido).
        let window = create_ole_owner();
        let deadline = Instant::now() + Duration::from_secs(2);
        // SAFETY: abre com uma janela desta thread só para segurar, sem
        // `EmptyClipboard` (o dono não muda); fecha sempre ao fim.
        while unsafe { OpenClipboard(Some(window)) }.is_err() {
            assert!(Instant::now() < deadline, "OpenClipboard falhou por 2 s");
            std::thread::sleep(REOPEN_PAUSE);
        }
        let _ = holding_tx.send(());
        let _ = release_rx.recv_timeout(Duration::from_secs(5));
        // SAFETY: fecha a sessão aberta acima e destrói a janela desta thread,
        // que nunca foi dona da área.
        unsafe {
            let _ = CloseClipboard();
            let _ = DestroyWindow(window);
        }
    });
    holding_rx
        .recv_timeout(Duration::from_secs(5))
        .expect("a thread que segura a área não abriu em 5 s");

    let result = WinClipboard.try_capture_if_owner_ready(&blocked);
    let _ = release_tx.send(());
    holder
        .join()
        .expect("a thread que segura a área entrou em pânico");
    // SAFETY: `owner` é desta thread e todos os formatos já têm bytes.
    let _ = unsafe { DestroyWindow(owner) };

    assert_eq!(
        result,
        Ok(CaptureOutcome::Rejected(RejectReason::BlockedProgram)),
        "decidido antes de abrir: com a área ocupada, abrir daria Err(Busy)"
    );
}
