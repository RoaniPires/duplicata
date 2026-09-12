#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::testsupport::{unicode_text_format, FakeHistoryRepository};
use duplicata_core::{
    ByteBudgetQueue, CaptureQueue, CaptureRecord, HistoryReader, HistoryRepository, IdentityKey,
    SelfWriteFilter, Timestamp, WorkItem,
};
use duplicata_win::history_window::HistoryWindow;
use duplicata_win::hotkey_win::HOTKEY_ID;
use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, SendMessageW, TranslateMessage, MSG, PM_REMOVE, WM_HOTKEY,
};

fn text_record(tag: u8, text: &str, ts: u64) -> CaptureRecord {
    let fmt = unicode_text_format(text);
    CaptureRecord {
        identity: IdentityKey([tag; 32]),
        canonical: CanonicalSelection {
            format_id: fmt.format_id,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: fmt.bytes.len() as u64,
        },
        captured_at: Timestamp::from_millis(ts),
        total_bytes: fmt.bytes.len() as u64,
        preview: Some(text.to_string()),
        thumbnail: None,
        has_text: true,
        formats: vec![fmt],
    }
}

fn main() {
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let mut repo = FakeHistoryRepository::new();
    for (i, t) in [
        "https://exemplo.com/um-link-que-eu-copiei-mais-cedo",
        "um trecho de texto qualquer copiado agora há pouco",
        "SELECT * FROM clip ORDER BY last_activity_utc DESC",
        "outra nota curta",
        "mais um item na lista",
        "penúltimo",
        "último",
    ]
    .iter()
    .enumerate()
    {
        repo.upsert(&text_record(i as u8 + 1, t, i as u64 + 1))
            .unwrap();
    }
    let _ = repo.set_pinned(1, true, 25);

    let window = HistoryWindow::new(
        repo,
        || -> Result<Box<dyn HistoryReader>, duplicata_core::StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("exemplo.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");

    // SAFETY: `hwnd` recém-criada nesta thread; simula o atalho global.
    unsafe {
        SendMessageW(
            window.hwnd(),
            WM_HOTKEY,
            Some(WPARAM(HOTKEY_ID as usize)),
            Some(LPARAM(0)),
        );
    }
    println!("janela aberta: hwnd={:?} — 12 s para olhar", window.hwnd());

    let deadline = Instant::now() + Duration::from_secs(12);
    while Instant::now() < deadline {
        let mut msg = MSG::default();
        // SAFETY: laço de mensagens padrão.
        unsafe {
            while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                let _ = TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        std::thread::sleep(Duration::from_millis(30));
    }
    println!("fim.");
}
