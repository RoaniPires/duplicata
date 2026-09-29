use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FailingHistoryRepository, FakeHistoryRepository, HistoryReader, HistoryRepository, RawCapture,
    StoreError, Timestamp, WorkItem, WorkerCounters,
};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn text_capture(s: &str, ts_ms: u64) -> RawCapture {
    let bytes = utf16le(s);
    RawCapture::new(
        vec![CapturedFormat {
            format_id: CF_UNICODETEXT,
            format_name: None,
            bytes: bytes.clone(),
        }],
        CanonicalSelection {
            format_id: CF_UNICODETEXT,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: bytes.len() as u64,
        },
        Timestamp::from_millis(ts_ms),
    )
}

#[test]
fn delete_one_removes_only_the_targeted_clip_and_is_not_counted_as_a_capture() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
        repo
    });

    let a = text_capture("fica", 100);
    let b = text_capture("sai", 200);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);
    // FIFO: as duas capturas já estão gravadas quando o DeleteOne chega; o
    // fake numera a partir de 1, então "sai" é o id 2.
    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::DeleteOne {
            clip_id: 2,
            done: done_tx,
        },
        0,
    );
    let (again_tx, again_rx) = mpsc::channel();
    queue.push(
        WorkItem::DeleteOne {
            clip_id: 2,
            done: again_tx,
        },
        0,
    );
    queue.close();

    assert_eq!(done_rx.recv().unwrap(), Ok(true));
    assert_eq!(
        again_rx.recv().unwrap(),
        Ok(false),
        "apagar o que já não existe é Ok(false), nunca erro"
    );

    let repo = worker.join().unwrap();
    let left = repo.list_for_display(10).unwrap();
    assert_eq!(left.len(), 1);
    assert_eq!(left[0].id, 1, "só o item alvo sai");
    assert_eq!(counters.processed(), 2, "DeleteOne não é uma captura");
}

#[test]
fn delete_one_reports_the_store_error_back_through_done() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FailingHistoryRepository::new(StoreError::Io);
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
    });

    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::DeleteOne {
            clip_id: 1,
            done: done_tx,
        },
        0,
    );
    queue.close();

    assert_eq!(done_rx.recv().unwrap(), Err(StoreError::Io));
    worker.join().unwrap();
}

#[test]
fn fake_delete_one_removes_pinned_clips_too() {
    let mut repo = FakeHistoryRepository::new();
    let (tx, _rx) = mpsc::channel();
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let cap = text_capture("fixado", 100);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.close();
    run_worker(&queue, &mut repo, &cfg(), &WorkerCounters::new(), &tx);

    repo.set_pinned(1, true, 25).unwrap();
    assert_eq!(repo.delete_one(1), Ok(true));
    assert!(repo.list_for_display(10).unwrap().is_empty());
}
