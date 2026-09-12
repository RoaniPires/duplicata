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
fn within_the_limit_removes_nothing() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "a", 1)).unwrap();
    repo.upsert(&text_record(2, "b", 2)).unwrap();
    assert_eq!(repo.purge_over_count(10).unwrap(), 0);

    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2);
}

#[test]
fn above_the_limit_removes_the_oldest_non_pinned_entries_first() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "velho", 100)).unwrap();
    repo.upsert(&text_record(2, "meio", 200)).unwrap();
    repo.upsert(&text_record(3, "novo", 300)).unwrap();

    let removed = repo.purge_over_count(2).unwrap();
    assert_eq!(removed, 1);

    let conn = repo.connection();
    let previews: Vec<String> = {
        let mut stmt = conn
            .prepare("SELECT preview FROM clip ORDER BY last_activity_utc DESC")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(previews, vec!["novo", "meio"], "'velho' foi o removido");
}

#[test]
fn pinned_entries_are_never_removed_regardless_of_count() {
    let (_dir, mut repo) = repo();
    let pinned_id = repo
        .upsert(&text_record(1, "velho-fixado", 100))
        .unwrap()
        .clip_id();
    repo.set_pinned(pinned_id, true, 25).unwrap();
    repo.upsert(&text_record(2, "meio", 200)).unwrap();
    repo.upsert(&text_record(3, "novo", 300)).unwrap();

    let removed = repo.purge_over_count(1).unwrap();
    assert_eq!(removed, 1);

    let n: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 2, "fixado + o não-fixado mais recente sobrevivem");

    let still_pinned: i64 = repo
        .connection()
        .query_row(
            "SELECT count(*) FROM clip WHERE id = ?1",
            [pinned_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(still_pinned, 1, "o fixado nunca é removido por esta via");
}

#[test]
fn retention_by_count_is_untouched_by_the_slice4_ordering_change() {
    let (_dir, mut repo) = repo();
    let p_old = repo
        .upsert(&text_record(1, "fixado-antigo", 10))
        .unwrap()
        .clip_id();
    let p_new = repo
        .upsert(&text_record(2, "fixado-recente", 900))
        .unwrap()
        .clip_id();
    repo.set_pinned(p_old, true, 25).unwrap();
    repo.set_pinned(p_new, true, 25).unwrap();
    for (tag, ts) in [(3u8, 100u64), (4, 200), (5, 300), (6, 400)] {
        repo.upsert(&text_record(tag, &format!("nao-fixado-{ts}"), ts))
            .unwrap();
    }

    assert_eq!(repo.purge_over_count(2).unwrap(), 2);

    let previews: Vec<String> = {
        let conn = repo.connection();
        let mut stmt = conn
            .prepare("SELECT preview FROM clip WHERE pinned = 0 ORDER BY last_activity_utc DESC")
            .unwrap();
        let rows = stmt.query_map([], |r| r.get::<_, String>(0)).unwrap();
        rows.map(Result::unwrap).collect()
    };
    assert_eq!(previews, vec!["nao-fixado-400", "nao-fixado-300"]);

    let pinned_count: i64 = repo
        .connection()
        .query_row("SELECT count(*) FROM clip WHERE pinned = 1", [], |r| {
            r.get(0)
        })
        .unwrap();
    assert_eq!(
        pinned_count, 2,
        "os dois fixados sobrevivem, independentemente da data"
    );
}

#[test]
fn cascades_clip_format_for_removed_entries() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "velho", 100)).unwrap();
    repo.upsert(&text_record(2, "novo", 200)).unwrap();

    repo.purge_over_count(1).unwrap();

    let conn = repo.connection();
    let formats: i64 = conn
        .query_row("SELECT count(*) FROM clip_format", [], |r| r.get(0))
        .unwrap();
    assert_eq!(formats, 2, "só os 2 formatos da entrada que sobrou");
}
