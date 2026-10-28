use std::path::PathBuf;
use std::sync::mpsc;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, RawCapture, Timestamp, WorkItem, WorkerCounters,
};

fn cfg(heuristic_on: bool) -> Config {
    let mut c = Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"));
    c.heuristic_secret_detection = heuristic_on;
    c
}

fn secret_text_capture(ts: u64) -> RawCapture {
    let bytes = utf16le("-----BEGIN PRIVATE KEY-----\nMIIEvQ...\n-----END PRIVATE KEY-----");
    let canonical = CanonicalSelection {
        format_id: CF_UNICODETEXT,
        format_name: None,
        kind: CanonicalKind::UnicodeText,
        byte_len: bytes.len() as u64,
    };
    RawCapture::new(
        vec![CapturedFormat {
            format_id: CF_UNICODETEXT,
            format_name: None,
            bytes,
        }],
        canonical,
        Timestamp::from_millis(ts),
    )
}

fn run(start_on: bool, items: Vec<WorkItem>) -> (FakeHistoryRepository, mpsc::Receiver<()>) {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    for item in items {
        queue.push(item, 1);
    }
    queue.close();
    let mut repo = FakeHistoryRepository::new();
    let (tx, rx) = mpsc::channel();
    run_worker(
        &queue,
        &mut repo,
        &cfg(start_on),
        &WorkerCounters::new(),
        &tx,
    );
    (repo, rx)
}

#[test]
fn turning_secret_detection_off_stores_the_next_secret_without_restarting() {
    let (repo, rx) = run(
        true,
        vec![
            WorkItem::SetSecretDetection { enabled: false },
            WorkItem::Capture(secret_text_capture(1)),
        ],
    );
    assert!(!repo.is_empty(), "o segredo entra no histórico");
    assert!(rx.try_recv().is_err(), "sem aviso de descarte");
}

#[test]
fn turning_secret_detection_on_discards_the_next_secret() {
    let (repo, rx) = run(
        false,
        vec![
            WorkItem::SetSecretDetection { enabled: true },
            WorkItem::Capture(secret_text_capture(1)),
        ],
    );
    assert!(repo.is_empty(), "o segredo é descartado");
    assert_eq!(rx.try_recv(), Ok(()));
}
