mod support;

use duplicata_core::HistoryRepository;
use duplicata_store::{migrations, open, SqliteHistoryRepository, SCHEMA_VERSION};
use rusqlite::Connection;
use support::text_record;

#[test]
fn corrupted_file_becomes_a_new_empty_database_at_the_current_schema_version() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");

    let conn = open(&db_path).unwrap();
    let mut repo = SqliteHistoryRepository::new(conn);
    repo.upsert(&text_record(1, "antes de corromper", 1))
        .unwrap();

    repo.recreate().expect("recriar deve suceder");

    let conn2 = repo.connection();
    assert_eq!(
        migrations::schema_version(conn2).unwrap(),
        Some(SCHEMA_VERSION)
    );
    let clip_count: i64 = conn2
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(clip_count, 0, "histórico anterior perdido — banco novo");

    repo.upsert(&text_record(2, "depois de recriar", 2))
        .unwrap();
    let clip_count2: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(clip_count2, 1);
}

#[test]
fn recreate_removes_stale_wal_and_shm_sidecar_files() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    let wal_path = dir.path().join("duplicata.db-wal");
    let shm_path = dir.path().join("duplicata.db-shm");

    {
        let conn = open(&db_path).unwrap();
        SqliteHistoryRepository::new(conn)
            .upsert(&text_record(1, "x", 1))
            .unwrap();
    }

    let forged = vec![0xABu8; 1024 * 1024];
    std::fs::write(&wal_path, &forged).unwrap();
    std::fs::write(&shm_path, &forged).unwrap();

    let raw_conn = Connection::open(&db_path).unwrap();
    let mut repo = SqliteHistoryRepository::new(raw_conn);

    repo.recreate()
        .expect("recriar deve suceder mesmo com sidecars forjados");

    assert!(db_path.exists(), "o arquivo principal existe de novo");
    if let Ok(meta) = std::fs::metadata(&wal_path) {
        assert!(
            meta.len() < forged.len() as u64,
            "o -wal forjado (1 MiB) deveria ter sido apagado antes da reabertura, \
             não só sobrescrito no início; ficou com {} bytes",
            meta.len()
        );
    }
    if let Ok(meta) = std::fs::metadata(&shm_path) {
        assert!(
            meta.len() < forged.len() as u64,
            "o -shm forjado (1 MiB) deveria ter sido apagado antes da reabertura, \
             não só sobrescrito no início; ficou com {} bytes",
            meta.len()
        );
    }
}
