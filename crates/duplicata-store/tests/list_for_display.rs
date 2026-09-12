mod support;

use std::time::Instant;

use duplicata_core::{HistoryReader, HistoryRepository, SetPinnedOutcome};
use duplicata_store::{open, SqliteHistoryRepository};
use support::{image_record, text_record};

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

#[test]
fn orders_by_last_activity_utc_desc() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "primeiro", 100)).unwrap();
    repo.upsert(&text_record(2, "segundo", 300)).unwrap();
    repo.upsert(&text_record(3, "terceiro", 200)).unwrap();

    let items = repo.list_for_display(10).unwrap();
    let previews: Vec<Option<String>> = items.iter().map(|i| i.preview.clone()).collect();
    assert_eq!(
        previews,
        vec![
            Some("segundo".to_string()),
            Some("terceiro".to_string()),
            Some("primeiro".to_string()),
        ]
    );
}

#[test]
fn a_pinned_old_item_survives_the_limit_and_leads_the_result() {
    let (_dir, mut repo) = repo();
    let old = repo
        .upsert(&text_record(0, "ancora", 10))
        .unwrap()
        .clip_id();
    for i in 1..=4u8 {
        repo.upsert(&text_record(i, &format!("recente{i}"), 100 + i as u64))
            .unwrap();
    }
    assert!(matches!(
        repo.set_pinned(old, true, 25).unwrap(),
        SetPinnedOutcome::Applied
    ));

    let items = repo.list_for_display(3).unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0].preview.as_deref(), Some("ancora"));
    assert!(items[0].pinned);
    assert_eq!(items[1].preview.as_deref(), Some("recente4"));
    assert_eq!(items[2].preview.as_deref(), Some("recente3"));
    assert!(!items[1].pinned && !items[2].pinned);
}

#[test]
fn without_any_pinned_item_the_order_is_still_pure_recency() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "a", 100)).unwrap();
    repo.upsert(&text_record(2, "b", 300)).unwrap();
    repo.upsert(&text_record(3, "c", 200)).unwrap();
    let previews: Vec<_> = repo
        .list_for_display(10)
        .unwrap()
        .iter()
        .map(|i| i.preview.clone().unwrap())
        .collect();
    assert_eq!(previews, ["b", "c", "a"]);
}

#[test]
fn thumbnail_is_none_for_a_text_item() {
    let (_dir, mut repo) = repo();
    repo.upsert(&text_record(1, "x", 1)).unwrap();

    let items = repo.list_for_display(10).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].thumbnail, None);
}

#[test]
fn thumbnail_bytes_come_back_for_an_image_item() {
    let (_dir, mut repo) = repo();
    repo.upsert(&image_record(1, vec![0u8; 40], Some(vec![9, 8, 7]), 1))
        .unwrap();

    let items = repo.list_for_display(10).unwrap();
    assert_eq!(items.len(), 1);
    assert_eq!(items[0].thumbnail, Some(vec![9, 8, 7]));
}

#[test]
fn respects_the_limit() {
    let (_dir, mut repo) = repo();
    for i in 0..5u8 {
        repo.upsert(&text_record(i, &format!("item{i}"), i as u64))
            .unwrap();
    }
    let items = repo.list_for_display(2).unwrap();
    assert_eq!(items.len(), 2);
}

#[test]
fn large_clip_format_bytes_do_not_affect_result_or_time() {
    let (_dir, mut repo) = repo();
    let huge = "x".repeat(1024 * 1024);
    for i in 0..20u8 {
        let mut rec = text_record(i, &format!("item{i}"), i as u64);
        rec.formats.push(duplicata_core::CapturedFormat {
            format_id: 0xC000 + i as u32,
            format_name: Some("Formato Enorme".into()),
            bytes: huge.as_bytes().to_vec(),
        });
        repo.upsert(&rec).unwrap();
    }

    let started = Instant::now();
    let items = repo.list_for_display(500).unwrap();
    let elapsed = started.elapsed();

    assert_eq!(items.len(), 20);
    assert!(
        elapsed.as_millis() < 100,
        "list_for_display não deveria ler clip_format.bytes; levou {elapsed:?}"
    );
}
