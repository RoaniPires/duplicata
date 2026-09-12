use std::sync::{mpsc, Arc};
use std::thread;

use duplicata_core::backoff::BackoffPolicy;
use duplicata_core::canonical::{CF_DIB, CF_UNICODETEXT};
use duplicata_core::{
    capture_and_enqueue, run_worker, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeClipboardSource, FakeClock, RecentCaptureGuard, WorkItem, WorkerCounters,
};
use duplicata_store::{open, SqliteHistoryRepository};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

#[test]
fn oversized_image_leaves_no_row_and_no_queue_entry() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let conn = open(&db).unwrap();
        let mut repo = SqliteHistoryRepository::new(conn);
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
        repo
    });

    let policy = BackoffPolicy::production();
    let clock = FakeClock::new();

    let oversized = FakeClipboardSource::always(vec![CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: vec![0u8; 70 * 1024 * 1024],
    }]);
    capture_and_enqueue(
        &oversized,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    assert!(queue.is_empty(), "descarte não chega a ocupar a fila");

    let ok = FakeClipboardSource::always(vec![CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: vec![0u8, 0u8],
    }]);
    capture_and_enqueue(
        &ok,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    queue.close();
    let repo = worker.join().unwrap();

    assert_eq!(counters.processed(), 1, "só o item válido chegou à worker");
    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        n, 1,
        "nenhuma linha parcial do item descartado; só a cópia válida"
    );
}
