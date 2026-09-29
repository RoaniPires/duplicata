mod support;

use duplicata_core::HistoryRepository;
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn repo_at(dir: &std::path::Path) -> SqliteHistoryRepository {
    let conn = open(&dir.join("duplicata.db")).unwrap();
    SqliteHistoryRepository::new(conn)
}

fn count(repo: &SqliteHistoryRepository, sql: &str) -> i64 {
    repo.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn removes_only_the_targeted_clip_and_its_formats() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = repo_at(dir.path());
    let alvo = repo.upsert(&text_record(1, "alvo", 1)).unwrap().clip_id();
    let vizinho = repo
        .upsert(&text_record(2, "vizinho", 2))
        .unwrap()
        .clip_id();

    assert_eq!(repo.delete_one(alvo), Ok(true));

    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 1);
    let sobrou: i64 = repo
        .connection()
        .query_row("SELECT id FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(sobrou, vizinho, "o vizinho não pode ser tocado");
    let formatos_do_alvo: i64 = repo
        .connection()
        .query_row(
            "SELECT count(*) FROM clip_format WHERE clip_id = ?1",
            [alvo],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(formatos_do_alvo, 0, "clip_format cai em cascata");
}

#[test]
fn removes_a_pinned_clip_too() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = repo_at(dir.path());
    let fixado = repo.upsert(&text_record(1, "fixado", 1)).unwrap().clip_id();
    repo.set_pinned(fixado, true, 25).unwrap();

    assert_eq!(repo.delete_one(fixado), Ok(true));
    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 0);
}

#[test]
fn a_missing_id_is_not_an_error_and_touches_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = repo_at(dir.path());
    let id = repo.upsert(&text_record(1, "fica", 1)).unwrap().clip_id();

    assert_eq!(repo.delete_one(id + 1000), Ok(false));
    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 1);
}

#[test]
fn deleted_content_is_not_recoverable_by_inspecting_the_raw_file_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    let marker = "SEGREDO_ITEM_UNICO_1a2b3c4d5e6f";
    let files = || {
        ["", "-wal", "-shm"].map(|suffix| {
            let mut path = db_path.clone().into_os_string();
            path.push(suffix);
            std::fs::read(std::path::PathBuf::from(path)).unwrap_or_default()
        })
    };
    let marker_on_disk = |all: [Vec<u8>; 3]| {
        all.iter()
            .any(|b| b.windows(marker.len()).any(|w| w == marker.as_bytes()))
    };

    {
        let mut repo = repo_at(dir.path());
        repo.upsert(&text_record(1, "outro item fica", 1)).unwrap();
        let id = repo.upsert(&text_record(2, marker, 2)).unwrap().clip_id();
        assert!(
            marker_on_disk(files()),
            "controle positivo: antes de apagar, o marcador tem de estar nos bytes crus"
        );
        repo.delete_one(id).unwrap();
    }

    for suffix in ["", "-wal", "-shm"] {
        let mut path = db_path.clone().into_os_string();
        path.push(suffix);
        let path = std::path::PathBuf::from(path);
        if let Ok(bytes) = std::fs::read(&path) {
            let found = bytes.windows(marker.len()).any(|w| w == marker.as_bytes());
            assert!(
                !found,
                "marcador do item apagado não pode aparecer nos bytes crus de {path:?}"
            );
        }
    }
}
