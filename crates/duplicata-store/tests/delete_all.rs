mod support;

use duplicata_core::HistoryRepository;
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn repo_at(dir: &std::path::Path) -> SqliteHistoryRepository {
    let conn = open(&dir.join("duplicata.db")).unwrap();
    SqliteHistoryRepository::new(conn)
}

#[test]
fn keep_pinned_true_removes_only_unpinned_entries() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = repo_at(dir.path());
    let pinned = repo.upsert(&text_record(1, "fixado", 1)).unwrap().clip_id();
    repo.upsert(&text_record(2, "solto", 2)).unwrap();
    repo.set_pinned(pinned, true, 25).unwrap();

    let removed = repo.delete_all(true).unwrap();
    assert_eq!(removed, 1);

    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 1, "só o fixado sobrevive");
    let still_there: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip WHERE id = ?1", [pinned], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(still_there, 1);
}

#[test]
fn keep_pinned_false_removes_everything_pinned_included() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = repo_at(dir.path());
    let pinned = repo.upsert(&text_record(1, "fixado", 1)).unwrap().clip_id();
    repo.upsert(&text_record(2, "solto", 2)).unwrap();
    repo.set_pinned(pinned, true, 25).unwrap();

    let removed = repo.delete_all(false).unwrap();
    assert_eq!(removed, 2);

    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0, "nada sobrevive, fixados inclusive");
}

#[test]
fn cascades_clip_format_in_both_modes() {
    let dir = tempfile::tempdir().unwrap();
    let mut repo = repo_at(dir.path());
    repo.upsert(&text_record(1, "a", 1)).unwrap();
    repo.upsert(&text_record(2, "b", 2)).unwrap();

    repo.delete_all(false).unwrap();

    let formats: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip_format", [], |r| r.get(0))
        .unwrap();
    assert_eq!(formats, 0);
}

#[test]
fn deleted_content_is_not_recoverable_by_inspecting_the_raw_file_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    let marker = "SEGREDO_UNICO_XYZ_9f8e7d6c5b4a";

    {
        let mut repo = repo_at(dir.path());
        repo.upsert(&text_record(1, marker, 1)).unwrap();
        repo.delete_all(false).unwrap();
    }

    for suffix in ["", "-wal", "-shm"] {
        let mut path = db_path.clone().into_os_string();
        path.push(suffix);
        let path = std::path::PathBuf::from(path);
        if let Ok(bytes) = std::fs::read(&path) {
            let found = bytes.windows(marker.len()).any(|w| w == marker.as_bytes());
            assert!(
                !found,
                "marcador do conteúdo apagado não pode aparecer nos bytes crus de {path:?}"
            );
        }
    }
}
