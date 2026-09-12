mod support;

use duplicata_core::{HistoryRepository, UpsertOutcome};
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

#[test]
fn repeated_identity_updates_recency_keeps_first_capture_no_new_row() {
    let (_dir, mut repo) = repo();

    let first = repo.upsert(&text_record(0xAA, "x", 100)).unwrap();
    let id = match first {
        UpsertOutcome::Inserted { clip_id } => clip_id,
        other => panic!("{other:?}"),
    };

    let again = repo.upsert(&text_record(0xAA, "x", 900)).unwrap();
    assert_eq!(again, UpsertOutcome::Deduped { clip_id: id });

    let conn = repo.connection();
    let n: i64 = conn
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "nenhuma linha nova");

    let (first_ms, last_ms): (i64, i64) = conn
        .query_row(
            "SELECT first_captured_utc, last_activity_utc FROM clip WHERE id = ?1",
            [id],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(first_ms, 100, "first_captured imutável");
    assert_eq!(last_ms, 900, "last_activity atualizado");
}

#[test]
fn dedupe_replaces_formats_with_the_most_recent_copy() {
    let (_dir, mut repo) = repo();

    let mut r1 = text_record(0xBB, "codigo", 1);
    r1.formats.push(duplicata_core::CapturedFormat {
        format_id: 0xC000,
        format_name: Some("HTML Format".into()),
        bytes: b"<b>antigo</b>".to_vec(),
    });
    repo.upsert(&r1).unwrap();
    let id: i64 = repo
        .connection()
        .query_row("SELECT id FROM clip", [], |r| r.get(0))
        .unwrap();

    let mut r2 = text_record(0xBB, "codigo", 2);
    r2.formats.push(duplicata_core::CapturedFormat {
        format_id: 0xC000,
        format_name: Some("HTML Format".into()),
        bytes: b"<i>novo</i>".to_vec(),
    });
    repo.upsert(&r2).unwrap();

    let conn = repo.connection();
    let html: Vec<u8> = conn
        .query_row(
            "SELECT bytes FROM clip_format WHERE clip_id = ?1 AND format_id = 49152",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        html, b"<i>novo</i>",
        "formatos passam a ser os da cópia mais recente"
    );

    let count: i64 = conn
        .query_row(
            "SELECT count(*) FROM clip_format WHERE clip_id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 3, "sem acumular formatos antigos");
}
