mod support;

use duplicata_core::{HistoryReader, HistoryRepository};
use duplicata_store::{open, SqliteHistoryReader, SqliteHistoryRepository};
use support::text_record;

#[test]
fn closing_the_old_reader_before_recreate_actually_clears_the_data() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");

    let mut repo = SqliteHistoryRepository::new(open(&db_path).unwrap());
    repo.upsert(&text_record(1, "antes de recriar", 1)).unwrap();

    let old_reader = SqliteHistoryReader::open(&db_path).unwrap();
    assert_eq!(
        old_reader.list_for_display(10).unwrap().len(),
        1,
        "sanity check: o item inserido antes deve aparecer"
    );

    Box::new(old_reader)
        .close()
        .expect("fechar a conexão antiga explicitamente deve funcionar");

    repo.recreate().unwrap();

    let new_reader = SqliteHistoryReader::open(&db_path)
        .expect("reabrir depois de fechar a conexão antiga e recriar deve funcionar");
    let items = new_reader
        .list_for_display(10)
        .expect("listar no banco recém-recriado deve funcionar");
    assert!(
        items.is_empty(),
        "banco recriado deve estar vazio, sem o item de antes"
    );
}

#[test]
fn recreate_with_a_reader_still_open_fails_loudly_instead_of_leaving_stale_data() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");

    let mut repo = SqliteHistoryRepository::new(open(&db_path).unwrap());
    repo.upsert(&text_record(1, "antes de recriar", 1)).unwrap();

    let old_reader = SqliteHistoryReader::open(&db_path).unwrap();
    assert_eq!(old_reader.list_for_display(10).unwrap().len(), 1);

    let result = repo.recreate();
    assert_eq!(
        result,
        Err(duplicata_core::StoreError::Io),
        "recreate() deve reportar a falha de remoção do -shm, não devolver \
         Ok silenciosamente com dado obsoleto por trás"
    );

    assert_eq!(
        repo.connection().path(),
        Some(db_path.to_str().unwrap()),
        "repo deve continuar apontando para o arquivo real, nunca para o \
         placeholder em memória usado internamente por recreate()"
    );
    repo.upsert(&text_record(2, "depois do Err, ainda funciona", 2))
        .unwrap();

    drop(old_reader);
}
