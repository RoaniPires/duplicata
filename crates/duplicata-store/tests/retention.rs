mod support;

use duplicata_core::HistoryRepository;
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

#[test]
fn purge_removes_entries_older_than_cutoff_and_cascades_formats() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "velho", 10)).unwrap();
    repo.upsert(&text_record(2, "meio", 50)).unwrap();
    repo.upsert(&text_record(3, "novo", 100)).unwrap();

    let removed = repo.purge_older_than(60).unwrap();
    assert_eq!(removed, 2);

    let conn = repo.connection();
    let clips: i64 = conn
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(clips, 1);

    let formats: i64 = conn
        .query_row("SELECT count(*) FROM clip_format", [], |r| r.get(0))
        .unwrap();
    assert_eq!(formats, 2, "só os 2 formatos da entrada que sobrou");
}

#[test]
fn purge_with_nothing_to_remove_returns_zero() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "a", 100)).unwrap();
    assert_eq!(repo.purge_older_than(50).unwrap(), 0);
}

#[test]
fn pinned_entries_survive_the_age_cut_while_old_unpinned_ones_are_removed() {
    let (_dir, mut repo) = repo();
    let pinned_id = repo
        .upsert(&text_record(1, "velho-fixado", 10))
        .unwrap()
        .clip_id();
    repo.set_pinned(pinned_id, true, 25).unwrap();
    repo.upsert(&text_record(2, "velho-solto", 20)).unwrap();
    repo.upsert(&text_record(3, "recente", 100)).unwrap();

    let removed = repo.purge_older_than(60).unwrap();
    assert_eq!(removed, 1, "só a entrada antiga NÃO fixada");

    let previews: Vec<String> = {
        let conn = repo.connection();
        let mut stmt = conn
            .prepare("SELECT preview FROM clip ORDER BY last_activity_utc")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(previews, vec!["velho-fixado", "recente"]);
}
