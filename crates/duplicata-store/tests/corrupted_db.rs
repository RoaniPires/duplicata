use std::fs;

use duplicata_core::StoreError;
use duplicata_store::open;

#[test]
fn garbage_file_opens_as_corrupted_not_io() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");
    fs::write(
        &db,
        b"isto definitivamente nao e um banco sqlite valido \x00\x01\x02",
    )
    .unwrap();

    match open(&db) {
        Err(StoreError::Corrupted) => {}
        other => panic!("esperava StoreError::Corrupted, veio {other:?}"),
    }
}

#[test]
fn truncated_sqlite_header_is_corrupted() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");
    fs::write(
        &db,
        b"SQLite format 3\x00 lixo lixo lixo lixo lixo lixo lixo",
    )
    .unwrap();

    assert!(matches!(open(&db), Err(StoreError::Corrupted)));
}
