use std::sync::{mpsc, Arc};
use std::thread;
use std::time::Duration;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, RawCapture, Timestamp, WorkItem, WorkerCounters,
};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn text_capture(s: &str, ts: u64) -> RawCapture {
    let bytes = utf16le(s);
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

#[test]
fn close_with_pending_items_drains_them_all_before_pop_returns_none() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());
    let repo = Arc::new(std::sync::Mutex::new(FakeHistoryRepository::new()));

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker_repo = Arc::clone(&repo);
    let worker = thread::spawn(move || {
        let mut guard = worker_repo.lock().unwrap();
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut *guard,
            &cfg(),
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    thread::sleep(Duration::from_millis(20));

    let items = [
        text_capture("um", 1),
        text_capture("dois", 2),
        text_capture("tres", 3),
    ];
    for item in &items {
        queue.push(WorkItem::Capture(item.clone()), item.total_bytes as usize);
    }
    queue.close();

    worker
        .join()
        .expect("worker não deve travar nem entrar em pânico");

    assert_eq!(
        counters.processed(),
        3,
        "os 3 itens aceitos antes do close foram processados, nenhum perdido"
    );
    let repo = repo.lock().unwrap();
    assert_eq!(
        repo.len(),
        3,
        "os 3 itens aceitos antes do close aparecem no repositório"
    );
    assert!(repo
        .get(&duplicata_core::identity_of(&utf16le("um")).0)
        .is_some());
    assert!(repo
        .get(&duplicata_core::identity_of(&utf16le("dois")).0)
        .is_some());
    assert!(repo
        .get(&duplicata_core::identity_of(&utf16le("tres")).0)
        .is_some());
}

#[test]
fn close_before_any_push_makes_the_blocked_worker_return_immediately() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg(),
            worker_counters.as_ref(),
            &heuristic_tx,
        );
        repo
    });

    thread::sleep(Duration::from_millis(20));
    queue.close();

    let repo = worker
        .join()
        .expect("pop() bloqueado deve acordar e devolver None em vez de travar");
    assert_eq!(repo.len(), 0);
    assert_eq!(counters.processed(), 0);
}
