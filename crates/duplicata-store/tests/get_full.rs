mod support;

use duplicata_core::{CapturedFormat, HistoryReader, HistoryRepository};
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

#[test]
fn all_formats_including_auxiliary_come_back_intact() {
    let (_dir, mut repo) = repo();
    let mut rec = text_record(1, "olá", 1);
    rec.formats.push(CapturedFormat {
        format_id: 0xC000,
        format_name: Some("HTML Format".into()),
        bytes: b"<p>ola</p>".to_vec(),
    });
    let clip_id = repo.upsert(&rec).unwrap().clip_id();

    let stored = repo.get_full(clip_id).unwrap().expect("clip existe");
    assert_eq!(stored.formats.len(), rec.formats.len());
    for original in &rec.formats {
        let found = stored
            .formats
            .iter()
            .find(|(id, _, _)| *id == original.format_id)
            .unwrap_or_else(|| panic!("formato {} ausente no get_full", original.format_id));
        assert_eq!(found.1, original.format_name);
        assert_eq!(found.2, original.bytes);
    }
}

#[test]
fn nonexistent_clip_id_is_none() {
    let (_dir, repo) = repo();
    assert_eq!(repo.get_full(999_999).unwrap(), None);
}
