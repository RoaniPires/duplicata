mod support;

use std::path::PathBuf;

use duplicata_core::canonical::{screen, Decision, RejectReason, CF_UNICODETEXT};
use duplicata_core::capture::FormatAnnounce;
use duplicata_core::{
    capture_and_enqueue, identity_of, unicode_text_format, ByteBudgetQueue, CaptureQueue,
    CaptureRecord, Config, FakeClipboardSource, FakeClock, FakeHistoryRepository,
    HistoryRepository, RecentCaptureGuard, WorkItem,
};
use support::{with_captured_logs, LogBuffer};

fn cfg_with_blocked(blocked: &[&str]) -> Config {
    let mut c = Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"));
    c.blocked_programs = blocked.iter().map(|s| s.to_string()).collect();
    c
}

fn policy() -> duplicata_core::backoff::BackoffPolicy {
    duplicata_core::backoff::BackoffPolicy::production()
}

fn text_info() -> Vec<FormatAnnounce> {
    vec![FormatAnnounce {
        format_id: CF_UNICODETEXT,
        format_name: None,
    }]
}

#[test]
fn screen_rejects_with_blocked_program_when_the_name_matches() {
    let cfg = cfg_with_blocked(&["keepass.exe"]);
    assert_eq!(
        screen(&text_info(), Some("keepass.exe"), &cfg),
        Decision::Reject(RejectReason::BlockedProgram)
    );
}

#[test]
fn screen_matches_case_insensitively() {
    let cfg = cfg_with_blocked(&["keepass.exe"]);
    assert_eq!(
        screen(&text_info(), Some("KeePass.EXE"), &cfg),
        Decision::Reject(RejectReason::BlockedProgram)
    );
    assert_eq!(
        screen(
            &text_info(),
            Some("Notepad.EXE"),
            &cfg_with_blocked(&["notepad.exe"])
        ),
        Decision::Reject(RejectReason::BlockedProgram)
    );
}

#[test]
fn screen_does_not_block_a_program_not_in_the_list() {
    let cfg = cfg_with_blocked(&["keepass.exe"]);
    assert_eq!(
        screen(&text_info(), Some("notepad.exe"), &cfg),
        Decision::Copy { canonical_index: 0 }
    );
}

#[test]
fn screen_with_no_source_program_never_blocks_by_precaution() {
    let cfg = cfg_with_blocked(&["keepass.exe"]);
    assert_eq!(
        screen(&text_info(), None, &cfg),
        Decision::Copy { canonical_index: 0 }
    );
}

#[test]
fn screen_checks_the_sensitive_flag_before_the_blocked_program() {
    let flagged_and_blocked = vec![
        FormatAnnounce {
            format_id: CF_UNICODETEXT,
            format_name: None,
        },
        FormatAnnounce {
            format_id: 0xC001,
            format_name: Some("ExcludeClipboardContentFromMonitorProcessing".to_string()),
        },
    ];
    assert_eq!(
        screen(
            &flagged_and_blocked,
            Some("keepass.exe"),
            &cfg_with_blocked(&["keepass.exe"])
        ),
        Decision::Reject(RejectReason::SensitiveFlagged)
    );
}

#[test]
fn a_blocked_program_is_rejected_as_blocked_even_when_the_content_is_too_large() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::new();
    let cfg = cfg_with_blocked(&["keepass.exe"]);
    let buf = LogBuffer::new();
    let huge = "a".repeat(3 * 1024 * 1024);
    let src = FakeClipboardSource::always(vec![unicode_text_format(&huge)]);
    src.set_source_program(Some("keepass.exe"));

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg,
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(
        q.is_empty(),
        "captura de programa bloqueado nunca é enfileirada"
    );
    let logs = buf.contents();
    assert!(logs.contains("blocked_program"), "{logs}");
    assert!(
        !logs.contains("capture_too_large"),
        "o gate de tamanho não deveria nem ter rodado: {logs}"
    );
}

#[test]
fn blocked_program_capture_is_rejected_removing_from_the_list_restores_capture() {
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::new();
    let mut cfg = cfg_with_blocked(&["keepass.exe"]);

    let src = FakeClipboardSource::always(vec![unicode_text_format("senha")]);
    src.set_source_program(Some("keepass.exe"));
    capture_and_enqueue(
        &src,
        &clock,
        &policy(),
        &cfg,
        &q,
        &RecentCaptureGuard::new(),
    );
    assert!(
        q.is_empty(),
        "captura de programa bloqueado nunca é enfileirada"
    );

    cfg.blocked_programs.clear();
    capture_and_enqueue(
        &src,
        &clock,
        &policy(),
        &cfg,
        &q,
        &RecentCaptureGuard::new(),
    );
    assert!(
        !q.is_empty(),
        "depois de removido da lista, a captura volta ao normal"
    );
}

#[test]
fn same_content_from_different_source_programs_collapses_into_one_entry() {
    let mut repo = FakeHistoryRepository::new();
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let clock = FakeClock::new();
    let cfg = Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"));

    let src_a = FakeClipboardSource::always(vec![unicode_text_format("conteudo identico")]);
    src_a.set_source_program(Some("notepad.exe"));
    capture_and_enqueue(
        &src_a,
        &clock,
        &policy(),
        &cfg,
        &q,
        &RecentCaptureGuard::new(),
    );

    let src_b = FakeClipboardSource::always(vec![unicode_text_format("conteudo identico")]);
    src_b.set_source_program(Some("chrome.exe"));
    capture_and_enqueue(
        &src_b,
        &clock,
        &policy(),
        &cfg,
        &q,
        &RecentCaptureGuard::new(),
    );

    q.close();
    while let Some(WorkItem::Capture(raw)) = q.pop() {
        let canonical_bytes = raw
            .formats
            .iter()
            .find(|f| f.format_id == raw.canonical.format_id)
            .map(|f| f.bytes.as_slice())
            .unwrap_or(&[]);
        repo.upsert(&CaptureRecord {
            identity: identity_of(canonical_bytes),
            canonical: raw.canonical.clone(),
            captured_at: raw.captured_at,
            total_bytes: raw.total_bytes,
            preview: None,
            thumbnail: None,
            has_text: true,
            formats: raw.formats,
        })
        .unwrap();
    }

    assert_eq!(
        repo.len(),
        1,
        "source_program nunca compõe a chave de identidade — mesmo conteúdo de \
         dois programas diferentes colapsa numa única entrada (dedupe por hash, Fatia 1)"
    );
}
