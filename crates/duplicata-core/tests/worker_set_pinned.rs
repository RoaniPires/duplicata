use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, HistoryReader, RawCapture, SetPinnedOutcome, Timestamp, WorkItem,
    WorkerCounters,
};

fn cfg_with_max_pinned(max_pinned: u32) -> Config {
    let mut c = Config::with_paths("db".into(), "logs".into());
    c.max_pinned = max_pinned;
    c
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
fn set_pinned_via_run_worker_pins_the_clip_and_answers_done_without_counting_a_capture() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            q.as_ref(),
            &mut repo,
            &cfg_with_max_pinned(25),
            c.as_ref(),
            &heuristic_tx,
        );
        repo
    });

    let cap = text_capture("preservar", 100);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);

    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::SetPinned {
            clip_id: 1,
            pinned: true,
            done: done_tx,
        },
        0,
    );
    queue.close();

    let outcome = done_rx
        .recv()
        .expect("run_worker deve responder no canal done");
    assert_eq!(outcome, Ok(SetPinnedOutcome::Applied));

    let repo = worker.join().unwrap();
    assert_eq!(
        repo.count_pinned().unwrap(),
        1,
        "o clip ficou fixado de verdade"
    );
    assert_eq!(
        counters.processed(),
        1,
        "só o Capture conta — SetPinned não é uma captura"
    );
}

#[test]
fn set_pinned_uses_the_worker_config_ceiling_and_reports_pin_cap_reached() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            q.as_ref(),
            &mut repo,
            &cfg_with_max_pinned(0),
            c.as_ref(),
            &heuristic_tx,
        );
    });

    let cap = text_capture("tentar fixar", 100);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::SetPinned {
            clip_id: 1,
            pinned: true,
            done: done_tx,
        },
        0,
    );
    queue.close();

    let outcome = done_rx.recv().expect("done deve responder");
    assert_eq!(
        outcome,
        Ok(SetPinnedOutcome::PinCapReached {
            limit: 0,
            current: 0,
        }),
        "o teto veio de cfg.max_pinned da worker, não do WorkItem"
    );
    worker.join().expect("worker não deve travar");
}

#[test]
fn set_pinned_after_apply_settings_uses_the_freshly_saved_ceiling() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            q.as_ref(),
            &mut repo,
            &cfg_with_max_pinned(25),
            c.as_ref(),
            &heuristic_tx,
        );
        repo
    });

    for (s, ts) in [("a", 100), ("b", 200)] {
        let cap = text_capture(s, ts);
        queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    }
    let (tx1, rx1) = mpsc::channel();
    queue.push(
        WorkItem::SetPinned {
            clip_id: 1,
            pinned: true,
            done: tx1,
        },
        0,
    );
    queue.push(
        WorkItem::ApplySettings {
            now_ms: 1_000,
            retention_ms: 7 * 24 * 60 * 60 * 1000,
            max_items: 500,
            max_pinned: 1,
        },
        0,
    );
    let (tx2, rx2) = mpsc::channel();
    queue.push(
        WorkItem::SetPinned {
            clip_id: 2,
            pinned: true,
            done: tx2,
        },
        0,
    );
    queue.close();

    assert_eq!(rx1.recv().unwrap(), Ok(SetPinnedOutcome::Applied));
    assert_eq!(
        rx2.recv().unwrap(),
        Ok(SetPinnedOutcome::PinCapReached {
            limit: 1,
            current: 1,
        }),
        "o teto recém-salvo (1) já vale para a fixação seguinte, sem restart"
    );

    let repo = worker.join().unwrap();
    assert_eq!(
        repo.count_pinned().unwrap(),
        1,
        "o já-fixado não foi desafixado (FR-027)"
    );
}
