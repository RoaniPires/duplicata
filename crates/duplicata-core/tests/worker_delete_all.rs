use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, HistoryReader, RawCapture, Timestamp, WorkItem, WorkerCounters,
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
fn captures_queued_before_delete_all_are_processed_then_wiped_in_order() {
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

    let a = text_capture("um", 100);
    let b = text_capture("dois", 200);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);
    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::DeleteAll {
            keep_pinned: false,
            done: done_tx,
        },
        0,
    );
    queue.close();

    let removed = done_rx.recv().expect("done deve responder");
    assert_eq!(
        removed,
        Ok(2),
        "as 2 capturas foram gravadas ANTES do DeleteAll (FIFO) e então apagadas"
    );

    let repo = worker.join().unwrap();
    assert_eq!(
        repo.list_for_display(10).unwrap().len(),
        0,
        "nada em trânsito sobrevive a 'apagar tudo'"
    );
    assert_eq!(
        counters.processed(),
        2,
        "só as 2 capturas contam — DeleteAll não é uma captura"
    );
}

#[test]
fn delete_all_keeping_pinned_leaves_the_pinned_capture_and_removes_the_rest() {
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

    let a = text_capture("guardar", 100);
    let b = text_capture("descartar", 200);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);
    let (pin_tx, pin_rx) = mpsc::channel();
    queue.push(
        WorkItem::SetPinned {
            clip_id: 1,
            pinned: true,
            done: pin_tx,
        },
        0,
    );
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);
    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::DeleteAll {
            keep_pinned: true,
            done: done_tx,
        },
        0,
    );
    queue.close();

    pin_rx.recv().unwrap().unwrap();
    assert_eq!(
        done_rx.recv().unwrap(),
        Ok(1),
        "só o não fixado foi removido"
    );

    let repo = worker.join().unwrap();
    let previews: Vec<Option<String>> = repo
        .list_for_display(10)
        .unwrap()
        .into_iter()
        .map(|c| c.preview)
        .collect();
    assert_eq!(previews, vec![Some("guardar".to_string())]);
}
