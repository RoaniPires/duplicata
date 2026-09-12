mod support;

use std::path::PathBuf;

use duplicata_core::canonical::{decide, Decision, RejectReason, CF_UNICODETEXT};
use duplicata_core::capture::FormatInfo;
use duplicata_core::{
    capture_and_enqueue, unicode_text_format, ByteBudgetQueue, CaptureQueue, CapturedFormat,
    Config, FakeClipboardSource, FakeClock, FakeHistoryRepository, HistoryRepository,
    RecentCaptureGuard, WorkItem,
};
use support::{with_captured_logs, LogBuffer};

const SENSITIVE_FORMAT_NAME: &str = "ExcludeClipboardContentFromMonitorProcessing";

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

fn policy() -> duplicata_core::backoff::BackoffPolicy {
    duplicata_core::backoff::BackoffPolicy::production()
}

#[test]
fn decide_rejects_with_sensitive_flagged_when_the_sentinel_format_is_present() {
    let formats = vec![
        FormatInfo {
            format_id: 13,
            format_name: None,
            byte_len: 6,
        },
        FormatInfo {
            format_id: 0xC001,
            format_name: Some(SENSITIVE_FORMAT_NAME.to_string()),
            byte_len: 0,
        },
    ];
    assert_eq!(
        decide(&formats, None, &cfg()),
        Decision::Reject(RejectReason::SensitiveFlagged)
    );
}

#[test]
fn decide_checks_the_sensitive_flag_before_size_or_canonical_selection() {
    let huge_text = FormatInfo {
        format_id: CF_UNICODETEXT,
        format_name: None,
        byte_len: 10 * 1024 * 1024,
    };
    let sentinel = FormatInfo {
        format_id: 0xC001,
        format_name: Some(SENSITIVE_FORMAT_NAME.to_string()),
        byte_len: 0,
    };
    assert_eq!(
        decide(&[huge_text, sentinel], None, &cfg()),
        Decision::Reject(RejectReason::SensitiveFlagged)
    );
}

#[test]
fn decide_without_the_sentinel_format_proceeds_normally() {
    let formats = vec![FormatInfo {
        format_id: CF_UNICODETEXT,
        format_name: None,
        byte_len: 6,
    }];
    assert_eq!(
        decide(&formats, None, &cfg()),
        Decision::Copy { canonical_index: 0 }
    );
}

#[test]
fn sensitive_flagged_rejection_logs_only_the_code_no_other_field() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let sentinel = CapturedFormat {
        format_id: 0xC001,
        format_name: Some(SENSITIVE_FORMAT_NAME.to_string()),
        bytes: vec![],
    };
    let src = FakeClipboardSource::always(vec![unicode_text_format("segredo"), sentinel]);
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

    assert!(q.is_empty(), "nada enfileirado");
    let logs = buf.contents();
    assert!(logs.contains("sensitive_flagged"), "{logs}");
    assert!(!logs.contains(SENSITIVE_FORMAT_NAME), "{logs}");
    assert!(!logs.contains("segredo"), "{logs}");
    assert!(
        !logs.contains("49152"),
        "id do formato (0xC001) não deve aparecer: {logs}"
    );
}

#[test]
fn flagged_content_never_reaches_upsert_the_next_unflagged_copy_is_captured_normally() {
    let mut repo = FakeHistoryRepository::new();
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::new();

    let sentinel = CapturedFormat {
        format_id: 0xC001,
        format_name: Some(SENSITIVE_FORMAT_NAME.to_string()),
        bytes: vec![],
    };
    let flagged_src =
        FakeClipboardSource::always(vec![unicode_text_format("senha-do-gerenciador"), sentinel]);
    capture_and_enqueue(
        &flagged_src,
        &clock,
        &policy(),
        &cfg(),
        &q,
        &RecentCaptureGuard::new(),
    );
    assert!(q.is_empty(), "cópia sinalizada nunca é enfileirada");

    let normal_src = FakeClipboardSource::always(vec![unicode_text_format("texto comum")]);
    capture_and_enqueue(
        &normal_src,
        &clock,
        &policy(),
        &cfg(),
        &q,
        &RecentCaptureGuard::new(),
    );

    q.close();
    let WorkItem::Capture(raw) = q.pop().expect("a segunda cópia foi enfileirada") else {
        panic!("esperava WorkItem::Capture")
    };
    let canonical_bytes = raw
        .formats
        .iter()
        .find(|f| f.format_id == raw.canonical.format_id)
        .map(|f| f.bytes.as_slice())
        .unwrap_or(&[]);
    let outcome = repo.upsert(&duplicata_core::CaptureRecord {
        identity: duplicata_core::identity_of(canonical_bytes),
        canonical: raw.canonical.clone(),
        captured_at: raw.captured_at,
        total_bytes: raw.total_bytes,
        preview: None,
        thumbnail: None,
        has_text: true,
        formats: raw.formats,
    });
    assert!(outcome.is_ok());
    assert_eq!(repo.len(), 1, "só a cópia não sinalizada chegou ao banco");
}
