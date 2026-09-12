mod support;

use std::sync::{mpsc, Arc};
use std::thread;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, RawCapture, Timestamp, WorkItem, WorkerCounters,
};
use support::{with_captured_logs, LogBuffer};

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
fn inserts_one_clip_per_distinct_capture_and_counts_it() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
        repo
    });

    let a = text_capture("alpha", 100);
    let b = text_capture("beta", 200);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);
    queue.close();

    let repo = worker.join().unwrap();
    assert_eq!(repo.len(), 2);
    assert_eq!(counters.processed(), 2);
}

#[test]
fn clip_inserted_event_carries_the_numeric_format_ids() {
    let bytes = utf16le("com html");
    let canonical = CanonicalSelection {
        format_id: CF_UNICODETEXT,
        format_name: None,
        kind: CanonicalKind::UnicodeText,
        byte_len: bytes.len() as u64,
    };
    let cap = RawCapture::new(
        vec![
            CapturedFormat {
                format_id: CF_UNICODETEXT,
                format_name: None,
                bytes,
            },
            CapturedFormat {
                format_id: 0xC000,
                format_name: Some("HTML Format".into()),
                bytes: b"<p>com html</p>".to_vec(),
            },
        ],
        canonical,
        Timestamp::from_millis(1),
    );
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.close();

    let buf = LogBuffer::new();
    let counters = WorkerCounters::new();
    let mut repo = FakeHistoryRepository::new();
    let (heuristic_tx, _heuristic_rx) = mpsc::channel();
    with_captured_logs(&buf, || {
        run_worker(&queue, &mut repo, &cfg(), &counters, &heuristic_tx)
    });

    let logs = buf.contents();
    assert!(logs.contains("clip_inserted"), "{logs}");
    assert!(logs.contains("13"), "id do CF_UNICODETEXT no log: {logs}");
    assert!(
        logs.contains("49152"),
        "id do formato registrado no log: {logs}"
    );
    assert!(
        !logs.contains("format_ids=None"),
        "não pode mais ser None: {logs}"
    );
    assert!(
        !logs.contains("com html") && !logs.contains("HTML Format"),
        "{logs}"
    );
}

#[test]
fn worker_returns_when_queue_closes_empty() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    queue.close();
    let counters = WorkerCounters::new();
    let mut repo = FakeHistoryRepository::new();
    let (heuristic_tx, _heuristic_rx) = mpsc::channel();
    run_worker(&queue, &mut repo, &cfg(), &counters, &heuristic_tx);
    assert_eq!(counters.processed(), 0);
}

#[test]
fn repeated_capture_is_deduped_and_logged() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let a1 = text_capture("igual", 100);
    let a2 = text_capture("igual", 500);
    queue.push(WorkItem::Capture(a1.clone()), a1.total_bytes as usize);
    queue.push(WorkItem::Capture(a2.clone()), a2.total_bytes as usize);
    queue.close();

    let buf = LogBuffer::new();
    let counters = WorkerCounters::new();
    let mut repo = FakeHistoryRepository::new();
    let (heuristic_tx, _heuristic_rx) = mpsc::channel();
    with_captured_logs(&buf, || {
        run_worker(&queue, &mut repo, &cfg(), &counters, &heuristic_tx)
    });

    assert_eq!(repo.len(), 1, "mesma identidade → uma entrada");
    assert_eq!(counters.processed(), 2);
    let logs = buf.contents();
    assert!(logs.contains("clip_inserted"));
    assert!(logs.contains("clip_deduped"));
}

#[test]
fn db_write_failure_is_logged_and_the_loop_continues() {
    use duplicata_core::{FailingHistoryRepository, StoreError};

    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let a = text_capture("um", 1);
    let b = text_capture("dois", 2);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);
    queue.close();

    let buf = LogBuffer::new();
    let counters = WorkerCounters::new();
    let mut repo = FailingHistoryRepository::new(StoreError::Query);
    let (heuristic_tx, _heuristic_rx) = mpsc::channel();
    with_captured_logs(&buf, || {
        run_worker(&queue, &mut repo, &cfg(), &counters, &heuristic_tx)
    });

    assert_eq!(counters.processed(), 2);
    let logs = buf.contents();
    assert_eq!(logs.matches("db_write_failed").count(), 2, "{logs}");
}
