#![cfg(windows)]

use duplicata_core::backoff::BackoffPolicy;
use duplicata_core::heuristic_recovery::{recover_on_notification_click, RecoverOutcome};
use duplicata_core::{ByteBudgetQueue, CaptureQueue, Config, WorkItem};
use duplicata_win::{WinClipboard, WinClock};
use windows::Win32::Foundation::HANDLE;
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

const CF_UNICODETEXT: u32 = 13;

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn utf16le_with_nul(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .chain([0, 0])
        .collect()
}

fn hglobal_copy(bytes: &[u8]) -> Option<isize> {
    // SAFETY: alocação nova, GMEM_MOVEABLE — mesmo padrão de
    // win_clipboard_smoke.rs (reimplementado aqui, função privada lá).
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }.ok()?;
    // SAFETY: hglobal recém-alocado, ainda não travado.
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: ptr aponta para pelo menos bytes.len() bytes válidos.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len()) };
    // SAFETY: pareado com o GlobalLock acima.
    let _ = unsafe { GlobalUnlock(hglobal) };
    Some(hglobal.0 as isize)
}

fn set_clipboard_text(text: &str) {
    let hglobal = hglobal_copy(&utf16le_with_nul(text)).expect("GlobalAlloc falhou");
    // SAFETY: OpenClipboard(None) sem janela-dona; CloseClipboard sempre
    // chamado antes de retornar.
    unsafe {
        OpenClipboard(None).expect("OpenClipboard falhou — outro processo com o clipboard aberto?");
        EmptyClipboard().expect("EmptyClipboard falhou");
        SetClipboardData(
            CF_UNICODETEXT,
            Some(HANDLE(hglobal as *mut core::ffi::c_void)),
        )
        .expect("SetClipboardData falhou");
        let _ = CloseClipboard();
    }
}

fn queue_text(item: &WorkItem) -> String {
    let WorkItem::RecoverCapture(raw) = item else {
        panic!("esperava RecoverCapture, veio {item:?}")
    };
    let bytes = &raw.formats[0].bytes;
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn clicking_right_after_the_rejection_recovers_the_item_still_on_the_clipboard() {
    set_clipboard_text("-----BEGIN PRIVATE KEY-----\nconteudo\n-----END PRIVATE KEY-----");

    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let outcome = recover_on_notification_click(
        &WinClipboard,
        &WinClock,
        &BackoffPolicy::production(),
        &cfg(),
        &queue,
    );

    assert!(matches!(outcome, RecoverOutcome::Recovered));
    let item = queue.pop().expect("um item deveria ter sido enfileirado");
    assert!(queue_text(&item).contains("PRIVATE KEY"));
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn clicking_after_copying_something_else_captures_the_new_content_not_the_original() {
    set_clipboard_text("texto comum copiado depois da rejeição");

    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let outcome = recover_on_notification_click(
        &WinClipboard,
        &WinClock,
        &BackoffPolicy::production(),
        &cfg(),
        &queue,
    );

    assert!(matches!(outcome, RecoverOutcome::Recovered));
    let item = queue.pop().expect("um item deveria ter sido enfileirado");
    let text = queue_text(&item);
    assert_eq!(text, "texto comum copiado depois da rejeição");
    assert!(
        !text.contains("PRIVATE KEY"),
        "nunca recupera o item originalmente rejeitado — só o que está lá agora"
    );
}
