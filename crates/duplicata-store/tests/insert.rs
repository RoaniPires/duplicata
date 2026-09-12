mod support;

use duplicata_core::{HistoryRepository, UpsertOutcome};
use duplicata_store::{open, SqliteHistoryRepository};
use support::{image_record, text_record};

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

#[test]
fn new_identity_inserts_one_clip_and_one_row_per_format() {
    let (_dir, mut repo) = repo();
    let rec = text_record(0xAA, "olá mundo", 1_000);

    let out = repo.upsert(&rec).unwrap();
    let clip_id = match out {
        UpsertOutcome::Inserted { clip_id } => clip_id,
        other => panic!("esperava Inserted, veio {other:?}"),
    };

    let conn = repo.connection();
    let (fc, cid, kind, first, last, total, preview): (i64, i64, String, i64, i64, i64, String) =
        conn.query_row(
            "SELECT canonical_format_id, id, canonical_kind, first_captured_utc,
                    last_activity_utc, total_bytes, preview FROM clip",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                ))
            },
        )
        .unwrap();
    assert_eq!(cid, clip_id);
    assert_eq!(fc, 13);
    assert_eq!(kind, "unicode_text");
    assert_eq!(first, 1_000);
    assert_eq!(
        last, 1_000,
        "first_captured == last_activity numa entrada nova"
    );
    assert_eq!(total as u64, rec.total_bytes);
    assert_eq!(preview, "olá mundo");

    let n: i64 = conn
        .query_row(
            "SELECT count(*) FROM clip_format WHERE clip_id = ?1",
            [clip_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n, 2);

    let canon_bytes: Vec<u8> = conn
        .query_row(
            "SELECT bytes FROM clip_format WHERE clip_id = ?1 AND is_canonical = 1",
            [clip_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(canon_bytes, rec.canonical_bytes());

    let n_canon: i64 = conn
        .query_row(
            "SELECT count(*) FROM clip_format WHERE clip_id = ?1 AND is_canonical = 1",
            [clip_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(n_canon, 1, "exatamente um formato canônico");
}

#[test]
fn distinct_identities_create_distinct_clips() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "a", 10)).unwrap();
    repo.upsert(&text_record(2, "b", 20)).unwrap();
    repo.upsert(&text_record(3, "c", 30)).unwrap();

    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3);
}

#[test]
fn thumbnail_is_null_for_a_text_capture() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "x", 1)).unwrap();
    let null_count: i64 = repo
        .connection()
        .query_row(
            "SELECT count(*) FROM clip WHERE thumbnail IS NULL",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(null_count, 1);
}

#[test]
fn thumbnail_bytes_are_persisted_for_an_image_capture() {
    let (_dir, mut repo) = repo();
    let rec = image_record(1, vec![0u8; 40], Some(vec![1, 2, 3, 4]), 1);

    let clip_id = repo.upsert(&rec).unwrap().clip_id();
    let stored: Vec<u8> = repo
        .connection()
        .query_row("SELECT thumbnail FROM clip WHERE id = ?1", [clip_id], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(stored, vec![1, 2, 3, 4]);
}

#[test]
fn thumbnail_is_null_for_an_image_capture_when_decode_failed() {
    let (_dir, mut repo) = repo();
    let rec = image_record(1, vec![0u8; 40], None, 1);

    let clip_id = repo.upsert(&rec).unwrap().clip_id();
    let is_null: bool = repo
        .connection()
        .query_row(
            "SELECT thumbnail IS NULL FROM clip WHERE id = ?1",
            [clip_id],
            |r| r.get(0),
        )
        .unwrap();
    assert!(is_null);
}
