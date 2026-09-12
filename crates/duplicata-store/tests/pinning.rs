mod support;

use duplicata_core::{HistoryReader, HistoryRepository, SetPinnedOutcome};
use duplicata_store::{open, SqliteHistoryRepository};
use support::text_record;

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

#[test]
fn count_pinned_is_zero_when_nothing_is_pinned() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "a", 1)).unwrap();
    assert_eq!(repo.count_pinned().unwrap(), 0);
}

#[test]
fn count_pinned_reflects_pin_and_unpin() {
    let (_dir, mut repo) = repo();
    let id = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    repo.upsert(&text_record(2, "b", 2)).unwrap();

    repo.set_pinned(id, true, 25).unwrap();
    assert_eq!(repo.count_pinned().unwrap(), 1);

    repo.set_pinned(id, false, 25).unwrap();
    assert_eq!(repo.count_pinned().unwrap(), 0);
}

#[test]
fn set_pinned_pins_an_unpinned_item() {
    let (_dir, mut repo) = repo();
    let id = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();

    assert_eq!(
        repo.set_pinned(id, true, 25).unwrap(),
        SetPinnedOutcome::Applied
    );

    let pinned: i64 = repo
        .connection()
        .query_row("SELECT pinned FROM clip WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(pinned, 1);
}

#[test]
fn set_pinned_pin_is_idempotent_and_does_not_double_count_against_the_cap() {
    let (_dir, mut repo) = repo();
    let id = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();

    repo.set_pinned(id, true, 1).unwrap();
    assert_eq!(
        repo.set_pinned(id, true, 1).unwrap(),
        SetPinnedOutcome::Applied,
        "fixar algo já fixado é no-op, nunca PinCapReached mesmo no teto"
    );
    assert_eq!(repo.count_pinned().unwrap(), 1);
}

#[test]
fn set_pinned_unpin_always_succeeds_even_when_already_unpinned() {
    let (_dir, mut repo) = repo();
    let id = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();

    assert_eq!(
        repo.set_pinned(id, false, 25).unwrap(),
        SetPinnedOutcome::Applied,
        "desafixar algo que já não estava fixado nunca é PinCapReached, nunca erro"
    );
}

#[test]
fn set_pinned_refuses_to_pin_beyond_the_cap() {
    let (_dir, mut repo) = repo();
    let a = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    let b = repo.upsert(&text_record(2, "b", 2)).unwrap().clip_id();

    repo.set_pinned(a, true, 1).unwrap();
    assert_eq!(
        repo.set_pinned(b, true, 1).unwrap(),
        SetPinnedOutcome::PinCapReached {
            limit: 1,
            current: 1
        }
    );
    assert_eq!(repo.count_pinned().unwrap(), 1, "b não foi fixado");
}

#[test]
fn set_pinned_max_pinned_zero_refuses_every_new_pin() {
    let (_dir, mut repo) = repo();
    let id = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();

    assert_eq!(
        repo.set_pinned(id, true, 0).unwrap(),
        SetPinnedOutcome::PinCapReached {
            limit: 0,
            current: 0
        },
        "max_pinned=0 é um valor válido sem piso — bloqueia toda nova fixação"
    );
}

#[test]
fn lowering_the_cap_never_unpins_existing_items_only_blocks_new_ones() {
    let (_dir, mut repo) = repo();
    let a = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    let b = repo.upsert(&text_record(2, "b", 2)).unwrap().clip_id();
    repo.set_pinned(a, true, 25).unwrap();
    repo.set_pinned(b, true, 25).unwrap();
    assert_eq!(repo.count_pinned().unwrap(), 2);

    assert_eq!(
        repo.set_pinned(a, true, 1).unwrap(),
        SetPinnedOutcome::Applied
    );
    assert_eq!(repo.count_pinned().unwrap(), 2, "nada foi desafixado");

    let c = repo.upsert(&text_record(3, "c", 3)).unwrap().clip_id();
    assert_eq!(
        repo.set_pinned(c, true, 1).unwrap(),
        SetPinnedOutcome::PinCapReached {
            limit: 1,
            current: 2
        }
    );
}

#[test]
fn set_pinned_on_a_nonexistent_id_is_not_found() {
    let (_dir, mut repo) = repo();
    assert_eq!(
        repo.set_pinned(999, true, 25).unwrap(),
        SetPinnedOutcome::NotFound
    );
}

#[test]
fn upsert_never_touches_the_pinned_column_on_a_deduped_update() {
    let (_dir, mut repo) = repo();
    let id = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    repo.set_pinned(id, true, 25).unwrap();

    repo.upsert(&text_record(1, "a", 2)).unwrap();

    let pinned: i64 = repo
        .connection()
        .query_row("SELECT pinned FROM clip WHERE id = ?1", [id], |r| r.get(0))
        .unwrap();
    assert_eq!(pinned, 1, "upsert nunca toca em pinned");
}
