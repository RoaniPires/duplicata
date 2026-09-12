mod support;

use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    CaptureRecord, CapturedFormat, HistoryReader, HistoryRepository, IdentityKey,
};
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

const CF_HDROP: u32 = 15;

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

fn hdrop_record_with_preview(tag: u8) -> CaptureRecord {
    let bytes = b"C:\\arquivo.txt".to_vec();
    let formats = vec![CapturedFormat {
        format_id: CF_HDROP,
        format_name: None,
        bytes: bytes.clone(),
    }];
    CaptureRecord {
        identity: IdentityKey([tag; 32]),
        canonical: CanonicalSelection {
            format_id: CF_HDROP,
            format_name: None,
            kind: CanonicalKind::HDrop,
            byte_len: bytes.len() as u64,
        },
        captured_at: duplicata_core::Timestamp::from_millis(1),
        total_bytes: bytes.len() as u64,
        preview: Some("arquivo.txt".to_string()),
        thumbnail: None,
        has_text: false,
        formats,
    }
}

#[test]
fn text_capture_has_has_text_true() {
    let (_dir, mut repo) = repo();
    let rec = text_record(1, "olá", 1);
    let clip_id = repo.upsert(&rec).unwrap().clip_id();

    let items = repo.list_for_display(10).unwrap();
    let item = items.iter().find(|i| i.id == clip_id).unwrap();
    assert!(item.has_text);
}

#[test]
fn hdrop_capture_with_a_preview_but_no_unicodetext_has_has_text_false() {
    let (_dir, mut repo) = repo();
    let rec = hdrop_record_with_preview(2);
    assert!(
        rec.preview.is_some(),
        "pré-condição do teste: precisa ter preview"
    );
    let clip_id = repo.upsert(&rec).unwrap().clip_id();

    let items = repo.list_for_display(10).unwrap();
    let item = items.iter().find(|i| i.id == clip_id).unwrap();
    assert!(
        !item.has_text,
        "has_text não pode ser inferido de um preview presente"
    );
}
