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
fn captures_around_a_toggle_are_all_processed_in_order_and_the_toggle_is_not_a_capture() {
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

    let a = text_capture("antes", 100);
    let b = text_capture("depois", 200);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);

    let (prog_tx, prog_rx) = mpsc::channel();
    let (done_tx, done_rx) = mpsc::channel();
    queue.push(
        WorkItem::ToggleEncryption {
            enable: true,
            progress: prog_tx,
            done: done_tx,
        },
        0,
    );
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);
    queue.close();

    assert_eq!(done_rx.recv().unwrap(), Ok(()), "toggle concluído");
    assert!(
        prog_rx.try_recv().is_ok(),
        "progresso foi reportado (FR-018b)"
    );

    let repo = worker.join().unwrap();
    let previews: Vec<Option<String>> = {
        let mut v: Vec<_> = repo
            .list_for_display(10)
            .unwrap()
            .into_iter()
            .map(|c| c.preview)
            .collect();
        v.sort();
        v
    };
    assert_eq!(
        previews,
        vec![Some("antes".to_string()), Some("depois".to_string())],
        "as duas capturas (antes e depois do toggle) foram gravadas"
    );
    assert_eq!(
        counters.processed(),
        2,
        "só as 2 capturas contam — ToggleEncryption não é uma captura"
    );
    assert!(
        repo.encryption_enabled,
        "o estado simulado alternou para ativo"
    );
    assert_eq!(repo.toggle_encryption_calls, 1);
}
