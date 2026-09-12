mod support;

use std::path::PathBuf;
use std::sync::mpsc;

use duplicata_core::{
    capture_and_enqueue, identity_of, run_worker, unicode_text_format, ByteBudgetQueue,
    CaptureQueue, CaptureRecord, Config, FakeClipboardSource, FakeClock, FakeHistoryRepository,
    HistoryRepository, RecentCaptureGuard, WorkItem, WorkerCounters, DEDUP_WINDOW_MS,
};

fn policy() -> duplicata_core::backoff::BackoffPolicy {
    duplicata_core::backoff::BackoffPolicy::production()
}

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

fn cfg_with_heuristic() -> Config {
    let mut c = cfg();
    c.heuristic_secret_detection = true;
    c
}

#[test]
fn the_same_copy_in_a_burst_produces_a_single_queue_entry() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::starting_at(10_000);
    let guard = RecentCaptureGuard::new();

    let src1 = FakeClipboardSource::always(vec![unicode_text_format("segredo-do-usuario")]);
    capture_and_enqueue(&src1, &clock, &policy(), &cfg(), &q, &guard);
    let src2 = FakeClipboardSource::always(vec![unicode_text_format("segredo-do-usuario")]);
    capture_and_enqueue(&src2, &clock, &policy(), &cfg(), &q, &guard);

    q.close();
    assert!(
        q.pop().is_some(),
        "a primeira captura da rajada É enfileirada"
    );
    assert!(
        q.pop().is_none(),
        "o eco (mesma identidade, mesmo instante) nunca chega a enfileirar de novo"
    );
}

#[test]
fn the_same_copy_after_the_window_produces_two_entries_and_still_bumps_recency() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::starting_at(10_000);
    let guard = RecentCaptureGuard::new();

    let src1 =
        FakeClipboardSource::always(vec![unicode_text_format("conteudo repetido de proposito")]);
    capture_and_enqueue(&src1, &clock, &policy(), &cfg(), &q, &guard);

    clock.advance(std::time::Duration::from_millis(DEDUP_WINDOW_MS));
    let src2 =
        FakeClipboardSource::always(vec![unicode_text_format("conteudo repetido de proposito")]);
    capture_and_enqueue(&src2, &clock, &policy(), &cfg(), &q, &guard);

    q.close();
    let WorkItem::Capture(first) = q.pop().expect("primeira captura enfileirada") else {
        panic!("esperava WorkItem::Capture")
    };
    let WorkItem::Capture(second) = q
        .pop()
        .expect("segunda captura, fora da janela, TAMBÉM é enfileirada")
    else {
        panic!("esperava WorkItem::Capture")
    };
    assert!(q.pop().is_none());
    assert_eq!(first.captured_at.as_millis(), 10_000);
    assert_eq!(second.captured_at.as_millis(), 10_000 + DEDUP_WINDOW_MS);

    let mut repo = FakeHistoryRepository::new();
    for raw in [&first, &second] {
        repo.upsert(&CaptureRecord {
            identity: identity_of(raw.canonical_bytes()),
            canonical: raw.canonical.clone(),
            captured_at: raw.captured_at,
            total_bytes: raw.total_bytes,
            preview: None,
            thumbnail: None,
            has_text: true,
            formats: raw.formats.clone(),
        })
        .unwrap();
    }
    assert_eq!(repo.len(), 1, "mesmo hash — uma única entrada no histórico");
    let entry = repo
        .get(identity_of(first.canonical_bytes()).as_bytes())
        .expect("entrada deve existir");
    assert_eq!(entry.first_captured_ms, 10_000, "primeira captura imutável");
    assert_eq!(
        entry.last_activity_ms,
        10_000 + DEDUP_WINDOW_MS,
        "recência avança para a segunda cópia, fora da janela"
    );
}

#[test]
fn different_copies_in_a_burst_never_collapse_into_each_other() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::starting_at(10_000);
    let guard = RecentCaptureGuard::new();

    let src1 = FakeClipboardSource::always(vec![unicode_text_format("primeiro conteudo")]);
    capture_and_enqueue(&src1, &clock, &policy(), &cfg(), &q, &guard);
    let src2 =
        FakeClipboardSource::always(vec![unicode_text_format("segundo conteudo, bem diferente")]);
    capture_and_enqueue(&src2, &clock, &policy(), &cfg(), &q, &guard);

    q.close();
    assert!(q.pop().is_some(), "primeira captura enfileirada");
    assert!(
        q.pop().is_some(),
        "identidade diferente nunca é tratada como eco, mesmo no mesmo instante"
    );
    assert!(q.pop().is_none());
}

#[test]
fn a_heuristic_rejection_burst_signals_only_once() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::starting_at(10_000);
    let guard = RecentCaptureGuard::new();
    let pem = "-----BEGIN PRIVATE KEY-----\nMIIEvQ...\n-----END PRIVATE KEY-----";

    let src1 = FakeClipboardSource::always(vec![unicode_text_format(pem)]);
    capture_and_enqueue(&src1, &clock, &policy(), &cfg_with_heuristic(), &q, &guard);
    let src2 = FakeClipboardSource::always(vec![unicode_text_format(pem)]);
    capture_and_enqueue(&src2, &clock, &policy(), &cfg_with_heuristic(), &q, &guard);
    q.close();

    let mut repo = FakeHistoryRepository::new();
    let counters = WorkerCounters::new();
    let (heuristic_tx, heuristic_rx) = mpsc::channel();
    run_worker(
        &q,
        &mut repo,
        &cfg_with_heuristic(),
        &counters,
        &heuristic_tx,
    );

    assert!(repo.is_empty(), "conteúdo de segredo nunca é gravado");
    assert_eq!(
        counters.processed(),
        1,
        "só UM item chegou à worker — o eco nunca foi enfileirado"
    );
    assert_eq!(
        heuristic_rx.try_recv(),
        Ok(()),
        "primeiro (e único) sinal recebido"
    );
    assert!(
        heuristic_rx.try_recv().is_err(),
        "nenhum segundo sinal — a rajada inteira virou um único evento"
    );
}
