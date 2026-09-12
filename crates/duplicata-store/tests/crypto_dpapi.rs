#![cfg(windows)]

mod support;

use duplicata_core::{HistoryReader, HistoryRepository};
use duplicata_store::crypto;
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn seeded_db(dir: &std::path::Path, marker: &str) -> std::path::PathBuf {
    let db = dir.join("duplicata.db");
    let conn = open(&db).unwrap();
    let mut repo = SqliteHistoryRepository::new(conn);
    repo.upsert(&text_record(1, marker, 1_000)).unwrap();
    duplicata_store::checkpoint_truncate(repo.connection()).unwrap();
    drop(repo);
    db
}

#[test]
#[ignore = "DPAPI real, vinculado à conta do Windows"]
fn protect_then_unprotect_recovers_identical_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let src = dir.path().join("plain.bin");
    let original = b"SQLite format 3\0 ... conteudo qualquer, 12345, \xde\xad\xbe\xef".to_vec();
    std::fs::write(&src, &original).unwrap();

    let blob = dir.path().join("blob.bin");
    let back = dir.path().join("back.bin");
    let ticks = std::cell::RefCell::new(Vec::new());
    let progress = |n: u64| ticks.borrow_mut().push(n);

    crypto::protect_file(&src, &blob, &crypto::tmp_path(&blob), &progress).unwrap();
    assert!(
        crypto::is_dpapi_blob(&blob),
        "o resultado não é um SQLite plano"
    );
    assert_ne!(std::fs::read(&blob).unwrap(), original, "os bytes mudaram");

    crypto::unprotect_file(&blob, &back, &crypto::tmp_path(&back), &progress).unwrap();
    assert_eq!(
        std::fs::read(&back).unwrap(),
        original,
        "ida e volta recupera os bytes idênticos"
    );
    assert!(!ticks.borrow().is_empty(), "progress foi chamado");
    assert!(!crypto::tmp_path(&blob).exists(), "temporário consumido");
}

#[test]
#[ignore = "DPAPI real"]
fn unprotect_of_a_non_dpapi_blob_fails_cleanly() {
    let dir = tempfile::tempdir().unwrap();
    let garbage = dir.path().join("garbage.bin");
    std::fs::write(&garbage, b"isto nunca passou por CryptProtectData").unwrap();

    let out = dir.path().join("out.bin");
    let err = crypto::unprotect_file(&garbage, &out, &crypto::tmp_path(&out), &|_| {})
        .expect_err("um blob alheio/corrompido deve falhar");
    assert_eq!(
        err,
        duplicata_core::StoreError::Corrupted,
        "falha limpa como Corrupted, nunca pânico"
    );
    assert!(!out.exists(), "nada foi escrito no destino");
}

#[test]
#[ignore = "DPAPI real"]
fn toggle_encryption_round_trips_and_never_leaves_the_placeholder() {
    let dir = tempfile::tempdir().unwrap();
    let marker = "MARCADOR_TOGGLE_a1b2c3";
    let db = seeded_db(dir.path(), marker);
    let work = crypto::work_copy_path(&db);

    let conn = open(&db).unwrap();
    let mut repo = SqliteHistoryRepository::new_at(conn, db.clone());
    repo.toggle_encryption(true, &|_| {}).unwrap();

    assert!(crypto::is_dpapi_blob(&db), "o arquivo real é um blob agora");
    assert!(work.exists(), "a cópia de trabalho desta sessão existe");
    repo.upsert(&text_record(2, "durante-protegido", 2_000))
        .unwrap();
    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2, "o item novo foi gravado na cópia de trabalho");

    repo.toggle_encryption(false, &|_| {}).unwrap();
    assert!(!crypto::is_dpapi_blob(&db), "voltou a ser SQLite plano");
    assert!(!work.exists(), "a cópia de trabalho foi promovida (rename)");
    repo.upsert(&text_record(3, "depois-desprotegido", 3_000))
        .unwrap();
    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 3, "tudo preservado na ida e volta");

    let survived: i64 = repo
        .connection()
        .query_row(
            "SELECT count(*) FROM clip WHERE preview = ?1",
            [marker],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(survived, 1);
}

#[test]
#[ignore = "DPAPI real"]
fn the_read_connection_opens_the_work_copy_when_protection_is_active() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded_db(dir.path(), "LEITURA_PROTEGIDA");

    let conn = open(&db).unwrap();
    let mut repo = SqliteHistoryRepository::new_at(conn, db.clone());
    repo.toggle_encryption(true, &|_| {}).unwrap();
    duplicata_store::checkpoint_truncate(repo.connection()).unwrap();
    drop(repo);

    let bad = duplicata_store::SqliteHistoryReader::open(&db).unwrap();
    assert_eq!(
        bad.list_for_display(10).unwrap_err(),
        duplicata_core::StoreError::Corrupted,
        "ler duplicata.db cru (blob) parece corrupção"
    );

    let effective = duplicata_store::crypto::effective_read_path(&db);
    assert_eq!(effective, duplicata_store::crypto::work_copy_path(&db));
    let reader = duplicata_store::SqliteHistoryReader::open(&effective)
        .expect("a cópia de trabalho é um SQLite normal");
    let rows = reader.list_for_display(10).unwrap();
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].preview.as_deref(), Some("LEITURA_PROTEGIDA"));

    assert!(
        !duplicata_store::crypto::recreate_is_safe_to_offer(&db),
        "recriar seria destrutivo com dados íntegros — nunca oferecido"
    );
}

#[test]
#[ignore = "DPAPI real"]
fn reconcile_discards_a_stale_tmp_crypto_and_leaves_a_plain_db_untouched() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded_db(dir.path(), "PLANO_INTACTO");
    let before = std::fs::read(&db).unwrap();

    std::fs::write(crypto::tmp_path(&db), b"lixo de uma troca interrompida").unwrap();

    let effective = crypto::reconcile_on_startup(&db, &|_| {}).unwrap();
    assert_eq!(effective, db, "banco plano: abre direto no caminho real");
    assert!(
        !crypto::tmp_path(&db).exists(),
        ".tmp-crypto órfão descartado"
    );
    assert_eq!(
        std::fs::read(&db).unwrap(),
        before,
        "o banco plano não foi tocado"
    );
}

#[test]
#[ignore = "DPAPI real"]
fn reconcile_reassumes_and_reseals_an_orphan_work_copy_before_opening() {
    let dir = tempfile::tempdir().unwrap();
    let db = seeded_db(dir.path(), "ORIGINAL_NO_BLOB");
    let work = crypto::work_copy_path(&db);

    crypto::protect_file(&db, &db, &crypto::tmp_path(&db), &|_| {}).unwrap();
    {
        let conn = open(&work).unwrap();
        let mut r = SqliteHistoryRepository::new(conn);
        r.upsert(&text_record(1, "ORIGINAL_NO_BLOB", 1_000))
            .unwrap();
        r.upsert(&text_record(2, "SO_NA_WORK_COPY", 2_000)).unwrap();
        duplicata_store::checkpoint_truncate(r.connection()).unwrap();
        drop(r);
    }

    let effective = crypto::reconcile_on_startup(&db, &|_| {}).unwrap();

    assert!(
        crypto::is_dpapi_blob(&db),
        "duplicata.db é o blob atualizado"
    );
    assert_eq!(effective, work, "a sessão abre a cópia de trabalho nova");

    let conn = open(&effective).unwrap();
    let both: i64 = conn
        .query_row(
            "SELECT count(*) FROM clip WHERE preview IN ('ORIGINAL_NO_BLOB','SO_NA_WORK_COPY')",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        both, 2,
        "nenhum dado da work copy órfã foi perdido (FR-018f)"
    );
}
