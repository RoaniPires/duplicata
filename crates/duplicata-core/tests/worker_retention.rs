mod support;

use std::sync::mpsc;
use std::time::Duration;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, HistoryReader, RawCapture, Timestamp, WorkItem, WorkerCounters,
};

fn cfg_with(retention: Duration, max_items: u32) -> Config {
    let mut c = Config::with_paths("db".into(), "logs".into());
    c.retention = retention;
    c.max_items = max_items;
    c
}

fn text_capture(s: &str, ts_ms: u64) -> RawCapture {
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
        Timestamp::from_millis(ts_ms),
    )
}

fn drain(queue: &ByteBudgetQueue<WorkItem>, repo: &mut FakeHistoryRepository) -> WorkerCounters {
    let counters = WorkerCounters::new();
    let (heuristic_tx, _rx) = mpsc::channel();
    let cfg = Config::with_paths("db".into(), "logs".into());
    queue.close();
    run_worker(queue, repo, &cfg, &counters, &heuristic_tx);
    counters
}

#[test]
fn every_successful_capture_triggers_both_retention_purges() {
    let queue = ByteBudgetQueue::new();
    let a = text_capture("alpha", 100);
    let b = text_capture("beta", 200);
    queue.push(WorkItem::Capture(a.clone()), a.total_bytes as usize);
    queue.push(WorkItem::Capture(b.clone()), b.total_bytes as usize);

    let mut repo = FakeHistoryRepository::new();
    drain(&queue, &mut repo);

    assert_eq!(
        repo.purge_older_than_calls, 2,
        "uma purga por idade após cada upsert bem-sucedido (FR-011, pós-upsert)"
    );
    assert_eq!(
        repo.purge_over_count_calls, 2,
        "e uma purga por quantidade após cada upsert bem-sucedido"
    );
}

#[test]
fn post_upsert_purge_removes_stale_entries_using_the_worker_config_snapshot() {
    let queue = ByteBudgetQueue::new();
    let old = text_capture("velho", 10);
    let fresh = text_capture("novo", 1_000);
    queue.push(WorkItem::Capture(old.clone()), old.total_bytes as usize);
    queue.push(WorkItem::Capture(fresh.clone()), fresh.total_bytes as usize);

    let mut repo = FakeHistoryRepository::new();
    let counters = WorkerCounters::new();
    let (heuristic_tx, _rx) = mpsc::channel();
    queue.close();
    run_worker(
        &queue,
        &mut repo,
        &cfg_with(Duration::from_millis(100), 500),
        &counters,
        &heuristic_tx,
    );

    let previews: Vec<Option<String>> = repo
        .list_for_display(10)
        .unwrap()
        .into_iter()
        .map(|c| c.preview)
        .collect();
    assert_eq!(previews, vec![Some("novo".to_string())], "só 'novo' sobra");
}

#[test]
fn apply_retention_updates_the_snapshot_purges_now_and_is_not_a_processed_capture() {
    let queue = ByteBudgetQueue::new();
    for (i, ts) in [("um", 100), ("dois", 200), ("tres", 300)] {
        let cap = text_capture(i, ts);
        queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    }
    queue.push(
        WorkItem::ApplySettings {
            now_ms: 300,
            retention_ms: 1_000_000,
            max_items: 1,
            max_pinned: 25,
        },
        0,
    );

    let mut repo = FakeHistoryRepository::new();
    let counters = drain(&queue, &mut repo);

    assert_eq!(
        counters.processed(),
        3,
        "ApplySettings não conta como captura processada — só os 3 Capture"
    );
    assert_eq!(
        repo.last_purge_over_count_max_items,
        Some(1),
        "a purga por quantidade usou o max_items RECÉM aplicado, não o snapshot antigo (500)"
    );
    assert_eq!(
        repo.list_for_display(10).unwrap().len(),
        1,
        "histórico cortado para 1 item na hora em que a configuração foi salva (FR-011)"
    );
}

#[test]
fn apply_retention_survives_a_config_where_nothing_needs_removing() {
    let queue = ByteBudgetQueue::new();
    let cap = text_capture("único", 100);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.push(
        WorkItem::ApplySettings {
            now_ms: 100,
            retention_ms: 1_000_000,
            max_items: 500,
            max_pinned: 25,
        },
        0,
    );

    let mut repo = FakeHistoryRepository::new();
    drain(&queue, &mut repo);

    assert_eq!(
        repo.list_for_display(10).unwrap().len(),
        1,
        "nada a remover"
    );
    assert!(repo.purge_over_count_calls >= 1, "mas a purga foi tentada");
}
