use duplicata_core::backoff::BackoffPolicy;
use duplicata_core::heuristic_recovery::{recover_on_notification_click, RecoverOutcome};
use duplicata_core::{
    ByteBudgetQueue, CaptureError, CaptureQueue, Clock, Config, FakeClipboardSource, FakeClock,
    WorkItem,
};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

#[test]
fn a_reread_that_stays_busy_past_the_backoff_is_reported_as_failed() {
    let source = FakeClipboardSource::busy_then_err(u32::MAX, CaptureError::Busy);
    let clock = FakeClock::new();
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();

    let outcome = recover_on_notification_click(
        &source,
        &clock,
        &BackoffPolicy::production(),
        &cfg(),
        &queue,
    );

    assert!(
        matches!(outcome, RecoverOutcome::Failed(CaptureError::Busy)),
        "esperava Failed(Busy), veio {outcome:?}"
    );
    assert!(
        queue.is_empty(),
        "nada pode ser enfileirado quando a releitura falha (FR-008e)"
    );
}

#[test]
fn the_backoff_gives_up_after_five_attempts_and_about_150ms() {
    let source = FakeClipboardSource::busy_then_err(u32::MAX, CaptureError::Busy);
    let clock = FakeClock::new();
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();

    let _ = recover_on_notification_click(
        &source,
        &clock,
        &BackoffPolicy::production(),
        &cfg(),
        &queue,
    );

    assert_eq!(source.calls(), 5, "tentativas de leitura");
    assert_eq!(
        clock.now_ms(),
        150,
        "sono acumulado do backoff — se este número subir acima do tempo que o \
         smoke segura o clipboard, aquele teste passa a ver `Recovered` por \
         tempo, não por defeito"
    );
}

#[test]
fn a_fatal_error_is_reported_immediately_without_retrying() {
    let source = FakeClipboardSource::failing(CaptureError::Unavailable);
    let clock = FakeClock::new();
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();

    let outcome = recover_on_notification_click(
        &source,
        &clock,
        &BackoffPolicy::production(),
        &cfg(),
        &queue,
    );

    assert!(
        matches!(outcome, RecoverOutcome::Failed(CaptureError::Unavailable)),
        "veio {outcome:?}"
    );
    assert_eq!(source.calls(), 1, "erro fatal não tem retry");
    assert!(queue.is_empty(), "erro fatal não enfileira nada");
}

#[test]
fn a_clipboard_that_frees_up_within_the_backoff_recovers_normally() {
    let text = duplicata_core::unicode_text_format("conteudo recuperado");
    let source = FakeClipboardSource::busy_then(3, vec![text]);
    let clock = FakeClock::new();
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();

    let outcome = recover_on_notification_click(
        &source,
        &clock,
        &BackoffPolicy::production(),
        &cfg(),
        &queue,
    );

    assert!(matches!(outcome, RecoverOutcome::Recovered), "{outcome:?}");
    assert!(
        !queue.is_empty(),
        "o item recuperado tem de ter sido enfileirado"
    );
    let item = queue.pop().expect("acabou de ser conferido acima");
    assert!(
        matches!(item, WorkItem::RecoverCapture(_)),
        "é RecoverCapture (pula só a heurística), não Capture — veio {item:?}"
    );
}
