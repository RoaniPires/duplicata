use std::path::PathBuf;

use duplicata_core::capture::{CanonicalKind, CanonicalSelection, CapturedFormat, Timestamp};
use duplicata_core::{
    CaptureError, CaptureOutcome, CaptureRecord, ClipboardSource, Config, FakeClipboardSource,
    FakeHistoryRepository, HistoryRepository, IdentityKey, UpsertOutcome,
};

fn text_format(s: &str) -> CapturedFormat {
    CapturedFormat {
        format_id: 13,
        format_name: None,
        bytes: s.as_bytes().to_vec(),
    }
}

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

fn record(hash: u8, text: &str, ts_ms: u64) -> CaptureRecord {
    let bytes = text.as_bytes().to_vec();
    CaptureRecord {
        identity: IdentityKey([hash; 32]),
        canonical: CanonicalSelection {
            format_id: 13,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: bytes.len() as u64,
        },
        captured_at: Timestamp::from_millis(ts_ms),
        total_bytes: bytes.len() as u64,
        preview: Some(text.to_string()),
        thumbnail: None,
        has_text: true,
        formats: vec![text_format(text)],
    }
}

#[test]
fn fake_clipboard_source_fails_busy_n_times_then_succeeds() {
    let src = FakeClipboardSource::busy_then(2, vec![text_format("oi")]);
    assert_eq!(src.try_capture(&cfg()), Err(CaptureError::Busy));
    assert_eq!(src.try_capture(&cfg()), Err(CaptureError::Busy));
    match src.try_capture(&cfg()).unwrap() {
        CaptureOutcome::Copied {
            formats,
            canonical_index,
        } => {
            assert_eq!(formats, vec![text_format("oi")]);
            assert_eq!(canonical_index, 0);
        }
        other => panic!("esperava Copied, veio {other:?}"),
    }
    assert_eq!(src.calls(), 3);
    assert_eq!(src.copied_format_ids(), vec![13]);
}

#[test]
fn fake_clipboard_source_can_fail_non_retryable() {
    let src = FakeClipboardSource::failing(CaptureError::Unavailable);
    assert_eq!(src.try_capture(&cfg()), Err(CaptureError::Unavailable));
}

#[test]
fn fake_clipboard_source_always_and_busy_then_err() {
    let ok = FakeClipboardSource::always(vec![text_format("v")]);
    match ok.try_capture(&cfg()).unwrap() {
        CaptureOutcome::Copied { formats, .. } => assert_eq!(formats, vec![text_format("v")]),
        other => panic!("esperava Copied, veio {other:?}"),
    }
    assert_eq!(ok.calls(), 1);

    let src = FakeClipboardSource::busy_then_err(1, CaptureError::Empty);
    assert_eq!(src.try_capture(&cfg()), Err(CaptureError::Busy));
    assert_eq!(src.try_capture(&cfg()), Err(CaptureError::Empty));
}

#[test]
fn fake_repo_is_empty_when_new_and_get_misses() {
    let repo = FakeHistoryRepository::new();
    assert!(repo.is_empty());
    assert_eq!(repo.len(), 0);
    assert!(repo.get(&[0u8; 32]).is_none());
}

#[test]
fn fake_repo_inserts_then_dedupes_preserving_first_capture() {
    let mut repo = FakeHistoryRepository::new();

    let out = repo.upsert(&record(0xAA, "x", 100)).unwrap();
    let id = match out {
        UpsertOutcome::Inserted { clip_id } => clip_id,
        _ => panic!("esperava Inserted"),
    };

    let out2 = repo.upsert(&record(0xAA, "x", 500)).unwrap();
    assert_eq!(out2, UpsertOutcome::Deduped { clip_id: id });
    assert_eq!(repo.len(), 1);

    let clip = repo.get(&[0xAA; 32]).unwrap();
    assert_eq!(clip.first_captured_ms, 100, "first_captured é imutável");
    assert_eq!(clip.last_activity_ms, 500);
}

#[test]
fn fake_repo_recency_order_and_purge() {
    let mut repo = FakeHistoryRepository::new();
    repo.upsert(&record(1, "a", 10)).unwrap();
    repo.upsert(&record(2, "b", 20)).unwrap();
    repo.upsert(&record(3, "c", 30)).unwrap();
    repo.upsert(&record(1, "a", 40)).unwrap();

    assert_eq!(repo.ids_by_recency(), vec![1, 3, 2]);

    let removed = repo.purge_older_than(25).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(repo.ids_by_recency(), vec![1, 3]);
}
