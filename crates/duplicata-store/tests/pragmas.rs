use duplicata_store::{migrations, open, SCHEMA_VERSION};

#[test]
fn open_applies_wal_secure_delete_and_foreign_keys() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");
    let conn = open(&db).expect("abre o banco");

    let journal: String = conn
        .pragma_query_value(None, "journal_mode", |r| r.get(0))
        .unwrap();
    assert_eq!(journal.to_ascii_lowercase(), "wal");

    let foreign_keys: i64 = conn
        .pragma_query_value(None, "foreign_keys", |r| r.get(0))
        .unwrap();
    assert_eq!(foreign_keys, 1);

    let secure_delete: i64 = conn
        .pragma_query_value(None, "secure_delete", |r| r.get(0))
        .unwrap();
    assert_eq!(secure_delete, 1);

    let synchronous: i64 = conn
        .pragma_query_value(None, "synchronous", |r| r.get(0))
        .unwrap();
    assert_eq!(synchronous, 1);
}

#[test]
fn open_creates_schema_v1_and_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");

    {
        let conn = open(&db).unwrap();
        assert_eq!(
            migrations::schema_version(&conn).unwrap(),
            Some(SCHEMA_VERSION)
        );

        let tables: Vec<String> = {
            let mut stmt = conn
                .prepare("SELECT name FROM sqlite_master WHERE type='table' ORDER BY name")
                .unwrap();
            let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
            rows.map(Result::unwrap).collect()
        };
        assert!(tables.contains(&"clip".to_string()));
        assert!(tables.contains(&"clip_format".to_string()));
        assert!(tables.contains(&"schema_meta".to_string()));
    }

    let conn2 = open(&db).unwrap();
    let clip_count: i64 = conn2
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(clip_count, 0);
    let meta_count: i64 = conn2
        .query_row("SELECT count(*) FROM schema_meta", [], |r| r.get(0))
        .unwrap();
    assert_eq!(meta_count, 1);
}

#[test]
fn foreign_key_cascade_is_active() {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();

    conn.execute(
        "INSERT INTO clip(identity_hash, canonical_format_id, canonical_kind,
                          first_captured_utc, last_activity_utc, total_bytes)
         VALUES (x'00', 13, 'unicode_text', 1, 1, 1)",
        [],
    )
    .unwrap();
    let clip_id = conn.last_insert_rowid();
    conn.execute(
        "INSERT INTO clip_format(clip_id, format_id, is_canonical, byte_len, bytes)
         VALUES (?1, 13, 1, 1, x'61')",
        [clip_id],
    )
    .unwrap();

    conn.execute("DELETE FROM clip WHERE id = ?1", [clip_id])
        .unwrap();
    let orphans: i64 = conn
        .query_row("SELECT count(*) FROM clip_format", [], |r| r.get(0))
        .unwrap();
    assert_eq!(orphans, 0, "ON DELETE CASCADE via foreign_keys=ON");
}
