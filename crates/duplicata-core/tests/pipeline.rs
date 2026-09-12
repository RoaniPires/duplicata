mod support;

use std::path::PathBuf;

use duplicata_core::backoff::BackoffPolicy;
use duplicata_core::canonical::{CF_LOCALE, CF_UNICODETEXT};
use duplicata_core::{
    capture_and_enqueue, ByteBudgetQueue, CaptureError, CaptureQueue, CapturedFormat, Config,
    FakeClipboardSource, FakeClock, RecentCaptureGuard, WorkItem,
};
use support::{with_captured_logs, LogBuffer};

fn raw_text_format(s: &str) -> CapturedFormat {
    CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: s.as_bytes().to_vec(),
    }
}

fn policy() -> BackoffPolicy {
    BackoffPolicy::production()
}

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

#[test]
fn successful_capture_is_enqueued_with_the_clock_timestamp() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![raw_text_format("oi")]);
    let clock = FakeClock::starting_at(4_242);

    capture_and_enqueue(
        &src,
        &clock,
        &policy(),
        &cfg(),
        &q,
        &RecentCaptureGuard::new(),
    );

    q.close();
    let WorkItem::Capture(raw) = q.pop().unwrap() else {
        panic!("esperava WorkItem::Capture")
    };
    assert_eq!(raw.formats, vec![raw_text_format("oi")]);
    assert_eq!(raw.captured_at.as_millis(), 4_242);
    assert!(q.pop().is_none());
}

#[test]
fn exhausted_backoff_logs_clipboard_busy_and_enqueues_nothing() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::failing(CaptureError::Busy);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(q.is_empty());
    let logs = buf.contents();
    assert!(logs.contains("clipboard_busy"), "{logs}");
    assert!(logs.contains("attempts"), "{logs}");
}

#[test]
fn unavailable_logs_and_empty_is_silent() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::new();

    let buf1 = LogBuffer::new();
    with_captured_logs(&buf1, || {
        capture_and_enqueue(
            &FakeClipboardSource::failing(CaptureError::Unavailable),
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });
    assert!(buf1.contents().contains("clipboard_unavailable"));

    let buf2 = LogBuffer::new();
    with_captured_logs(&buf2, || {
        capture_and_enqueue(
            &FakeClipboardSource::failing(CaptureError::Empty),
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });
    assert!(
        buf2.contents().trim().is_empty(),
        "clipboard vazio não gera log: {}",
        buf2.contents()
    );
    assert!(q.is_empty());
}

#[test]
fn no_canonical_format_is_logged_and_nothing_is_enqueued() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![CapturedFormat {
        format_id: CF_LOCALE,
        format_name: None,
        bytes: b"SEGREDO-NAO-VAZAR".to_vec(),
    }]);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(q.is_empty(), "nada enfileirado sem formato canônico");
    let logs = buf.contents();
    assert!(logs.contains("no_canonical_format"), "{logs}");
    assert!(
        !logs.contains("SEGREDO-NAO-VAZAR"),
        "log sem conteúdo: {logs}"
    );
}

#[test]
fn queue_eviction_emits_one_log_per_discarded_item_with_its_byte_len() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::with_budget(100);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        for i in 0..4u8 {
            let src = FakeClipboardSource::always(vec![raw_text_format(&"x".repeat(60))]);
            let _ = i;
            capture_and_enqueue(
                &src,
                &clock,
                &policy(),
                &cfg(),
                &q,
                &RecentCaptureGuard::new(),
            );
        }
    });

    let logs = buf.contents();
    let evictions = logs.matches("queue_evicted").count();
    assert_eq!(evictions, 3, "3 itens descartados: {logs}");
    assert_eq!(
        logs.matches("byte_len=60").count(),
        3,
        "cada queue_evicted traz byte_len=60: {logs}"
    );
    assert_eq!(q.len(), 1);
}
