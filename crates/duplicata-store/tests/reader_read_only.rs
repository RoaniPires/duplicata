mod support;

use std::time::{Duration, Instant};

use duplicata_core::{HistoryReader, HistoryRepository, StoreError};
use duplicata_store::{open, SqliteHistoryReader, SqliteHistoryRepository};
use rusqlite::Connection;
use support::text_record;

#[test]
fn nonexistent_path_fails_to_open_without_creating_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("nao-existe.db");

    let result = SqliteHistoryReader::open(&db_path);

    assert!(
        result.is_err(),
        "sem SQLITE_OPEN_CREATE, abrir um caminho inexistente deve falhar"
    );
    assert!(
        !db_path.exists(),
        "o lado de leitura nunca deve criar um arquivo de banco"
    );
}

#[test]
fn a_valid_database_is_readable_and_returns_what_was_written() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    {
        let conn = open(&db_path).unwrap();
        let mut repo = SqliteHistoryRepository::new(conn);
        repo.upsert(&text_record(1, "olá", 1)).unwrap();
    }

    let reader = SqliteHistoryReader::open(&db_path).expect("banco válido deve abrir");
    let items = reader.list_for_display(10).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].preview.as_deref(), Some("olá"));
}

#[test]
fn a_held_exclusive_lock_times_out_via_busy_timeout_and_maps_to_store_error_io() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    {
        let conn = open(&db_path).unwrap();
        SqliteHistoryRepository::new(conn)
            .upsert(&text_record(1, "x", 1))
            .unwrap();
    }

    let writer = Connection::open(&db_path).unwrap();
    writer
        .pragma_update(None, "locking_mode", "exclusive")
        .unwrap();
    writer
        .execute(
            "INSERT INTO clip(identity_hash, canonical_format_id, canonical_kind,
                              first_captured_utc, last_activity_utc, total_bytes)
             VALUES (x'02', 13, 'unicode_text', 2, 2, 1)",
            [],
        )
        .unwrap();

    let started = Instant::now();
    let result: Result<(), StoreError> = SqliteHistoryReader::open(&db_path)
        .and_then(|reader| reader.list_for_display(10))
        .map(|_| ());
    let elapsed = started.elapsed();

    assert!(
        matches!(result, Err(StoreError::Io)),
        "lock exclusivo mantido deveria estourar o busy_timeout como StoreError::Io, veio {result:?}"
    );
    assert!(
        elapsed < Duration::from_millis(1_000),
        "busy_timeout curto (~50ms) não deve bloquear por muito tempo; levou {elapsed:?}"
    );

    drop(writer);
}
