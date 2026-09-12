use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection, CapturedFormat, Timestamp};
use duplicata_core::{
    utf16le, CaptureRecord, FailingHistoryRepository, FakeHistoryRepository, HistoryReader,
    HistoryRepository, IdentityKey, SetPinnedOutcome, StoreError, UpsertOutcome,
};

fn text_record(tag: u8, text: &str, ts_ms: u64) -> CaptureRecord {
    let bytes = utf16le(text);
    CaptureRecord {
        identity: IdentityKey([tag; 32]),
        canonical: CanonicalSelection {
            format_id: CF_UNICODETEXT,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: bytes.len() as u64,
        },
        captured_at: Timestamp::from_millis(ts_ms),
        total_bytes: bytes.len() as u64,
        preview: Some(text.to_string()),
        thumbnail: None,
        has_text: true,
        formats: vec![CapturedFormat {
            format_id: CF_UNICODETEXT,
            format_name: None,
            bytes,
        }],
    }
}

#[test]
fn fake_recreate_clears_entries_and_resets_the_id_sequence() {
    let mut repo = FakeHistoryRepository::new();
    repo.upsert(&text_record(1, "um", 1)).unwrap();
    repo.upsert(&text_record(2, "dois", 2)).unwrap();
    assert_eq!(repo.len(), 2);

    repo.recreate().unwrap();
    assert!(repo.is_empty());

    let outcome = repo.upsert(&text_record(3, "três", 3)).unwrap();
    match outcome {
        UpsertOutcome::Inserted { clip_id } => assert_eq!(clip_id, 1),
        other => panic!("esperava Inserted{{clip_id: 1}}, veio {other:?}"),
    }
}

#[test]
fn fake_list_for_display_orders_by_recency_and_respects_limit() {
    let mut repo = FakeHistoryRepository::new();
    repo.upsert(&text_record(1, "primeiro", 100)).unwrap();
    repo.upsert(&text_record(2, "segundo", 300)).unwrap();
    repo.upsert(&text_record(3, "terceiro", 200)).unwrap();

    let items = repo.list_for_display(10).unwrap();
    let previews: Vec<_> = items.iter().map(|i| i.preview.clone()).collect();
    assert_eq!(
        previews,
        vec![
            Some("segundo".to_string()),
            Some("terceiro".to_string()),
            Some("primeiro".to_string()),
        ]
    );
    assert!(
        items.iter().all(|i| i.thumbnail.is_none()),
        "fake nunca escreve thumbnail — mesmo contrato do lado real antes da US1b gerar uma"
    );
    assert!(items.iter().all(|i| i.has_text));

    assert_eq!(
        repo.list_for_display(2).unwrap().len(),
        2,
        "respeita o limite"
    );
}

#[test]
fn fake_get_full_returns_all_formats_and_the_text_format_when_has_text() {
    let mut repo = FakeHistoryRepository::new();
    let outcome = repo.upsert(&text_record(1, "olá", 1)).unwrap();
    let UpsertOutcome::Inserted { clip_id } = outcome else {
        panic!("esperava Inserted");
    };

    let stored = repo
        .get_full(clip_id)
        .unwrap()
        .expect("clip recém-inserido deve existir");
    assert_eq!(stored.formats.len(), 1);
    assert_eq!(stored.formats[0].0, CF_UNICODETEXT);
    assert!(
        stored.text_format.is_some(),
        "has_text=true deve produzir text_format"
    );
}

#[test]
fn fake_get_full_is_none_for_a_nonexistent_id() {
    let repo = FakeHistoryRepository::new();
    assert_eq!(repo.get_full(999).unwrap(), None);
}

#[test]
fn fake_close_always_succeeds_no_real_connection_to_release() {
    let boxed: Box<dyn HistoryReader> = Box::new(FakeHistoryRepository::new());
    boxed
        .close()
        .expect("fake não tem conexão real, close() nunca falha");
}

#[test]
fn fake_set_pinned_pins_an_unpinned_item_and_reports_applied() {
    let mut repo = FakeHistoryRepository::new();
    let clip_id = repo.upsert(&text_record(1, "x", 1)).unwrap().clip_id();

    assert_eq!(
        repo.set_pinned(clip_id, true, 25).unwrap(),
        SetPinnedOutcome::Applied
    );
    assert!(repo.get(&[1; 32]).unwrap().pinned);
    assert_eq!(repo.count_pinned().unwrap(), 1);
}

#[test]
fn fake_set_pinned_pin_is_idempotent_and_does_not_double_count_against_the_cap() {
    let mut repo = FakeHistoryRepository::new();
    let clip_id = repo.upsert(&text_record(1, "x", 1)).unwrap().clip_id();

    repo.set_pinned(clip_id, true, 1).unwrap();
    assert_eq!(
        repo.set_pinned(clip_id, true, 1).unwrap(),
        SetPinnedOutcome::Applied,
        "fixar algo já fixado é no-op, nunca PinCapReached mesmo no teto"
    );
    assert_eq!(repo.count_pinned().unwrap(), 1);
}

#[test]
fn fake_set_pinned_unpin_is_always_applied_even_when_already_unpinned() {
    let mut repo = FakeHistoryRepository::new();
    let clip_id = repo.upsert(&text_record(1, "x", 1)).unwrap().clip_id();

    assert_eq!(
        repo.set_pinned(clip_id, false, 25).unwrap(),
        SetPinnedOutcome::Applied,
        "desafixar algo que já não estava fixado é sempre Applied, nunca erro"
    );
}

#[test]
fn fake_set_pinned_refuses_to_pin_beyond_the_cap() {
    let mut repo = FakeHistoryRepository::new();
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
fn fake_set_pinned_lowering_the_cap_never_unpins_existing_items() {
    let mut repo = FakeHistoryRepository::new();
    let a = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    let b = repo.upsert(&text_record(2, "b", 2)).unwrap().clip_id();
    repo.set_pinned(a, true, 25).unwrap();
    repo.set_pinned(b, true, 25).unwrap();

    assert_eq!(repo.count_pinned().unwrap(), 2);
    assert_eq!(
        repo.set_pinned(a, true, 1).unwrap(),
        SetPinnedOutcome::Applied,
        "refixar um já fixado nunca é bloqueado pelo teto"
    );
}

#[test]
fn fake_set_pinned_on_a_nonexistent_id_is_not_found() {
    let mut repo = FakeHistoryRepository::new();
    assert_eq!(
        repo.set_pinned(999, true, 25).unwrap(),
        SetPinnedOutcome::NotFound
    );
}

#[test]
fn fake_purge_over_count_removes_only_the_oldest_unpinned_entries_above_the_limit() {
    let mut repo = FakeHistoryRepository::new();
    let old = repo.upsert(&text_record(1, "old", 100)).unwrap().clip_id();
    repo.upsert(&text_record(2, "mid", 200)).unwrap();
    repo.upsert(&text_record(3, "new", 300)).unwrap();
    repo.set_pinned(old, true, 25).unwrap();

    let removed = repo.purge_over_count(1).unwrap();
    assert_eq!(removed, 1, "só 'mid' é removido — 'old' está fixado, 'new' é o mais recente que sobra dentro do limite de 1 não-fixado");
    assert_eq!(
        repo.len(),
        2,
        "'old' (fixado) + 'new' (mais recente não-fixado) sobrevivem"
    );
    assert!(
        repo.get(&[1; 32]).is_some(),
        "fixado nunca é removido por esta via"
    );
    assert!(repo.get(&[3; 32]).is_some());
}

#[test]
fn fake_purge_over_count_within_the_limit_removes_nothing() {
    let mut repo = FakeHistoryRepository::new();
    repo.upsert(&text_record(1, "a", 1)).unwrap();
    assert_eq!(repo.purge_over_count(10).unwrap(), 0);
    assert_eq!(repo.len(), 1);
}

#[test]
fn fake_delete_all_keep_pinned_true_preserves_only_pinned() {
    let mut repo = FakeHistoryRepository::new();
    let pinned = repo.upsert(&text_record(1, "p", 1)).unwrap().clip_id();
    repo.upsert(&text_record(2, "np", 2)).unwrap();
    repo.set_pinned(pinned, true, 25).unwrap();

    let removed = repo.delete_all(true).unwrap();
    assert_eq!(removed, 1);
    assert_eq!(repo.len(), 1);
    assert!(repo.get(&[1; 32]).is_some());
}

#[test]
fn fake_delete_all_keep_pinned_false_removes_everything_pinned_included() {
    let mut repo = FakeHistoryRepository::new();
    let pinned = repo.upsert(&text_record(1, "p", 1)).unwrap().clip_id();
    repo.upsert(&text_record(2, "np", 2)).unwrap();
    repo.set_pinned(pinned, true, 25).unwrap();

    let removed = repo.delete_all(false).unwrap();
    assert_eq!(removed, 2);
    assert!(repo.is_empty());
}

#[test]
fn fake_count_pinned_reflects_pin_and_unpin() {
    let mut repo = FakeHistoryRepository::new();
    let a = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    assert_eq!(repo.count_pinned().unwrap(), 0);
    repo.set_pinned(a, true, 25).unwrap();
    assert_eq!(repo.count_pinned().unwrap(), 1);
    repo.set_pinned(a, false, 25).unwrap();
    assert_eq!(repo.count_pinned().unwrap(), 0);
}

#[test]
fn fake_list_for_display_exposes_the_pinned_flag() {
    let mut repo = FakeHistoryRepository::new();
    let a = repo.upsert(&text_record(1, "a", 1)).unwrap().clip_id();
    repo.upsert(&text_record(2, "b", 2)).unwrap();
    repo.set_pinned(a, true, 25).unwrap();

    let items = repo.list_for_display(10).unwrap();
    let a_item = items.iter().find(|i| i.id == a).unwrap();
    assert!(a_item.pinned);
    let b_item = items.iter().find(|i| i.id != a).unwrap();
    assert!(!b_item.pinned);
}

#[test]
fn failing_repository_reports_the_configured_error_on_every_read_and_write_method() {
    let mut repo = FailingHistoryRepository::new(StoreError::Corrupted);

    assert_eq!(
        repo.upsert(&text_record(1, "x", 1)),
        Err(StoreError::Corrupted)
    );
    assert_eq!(repo.purge_older_than(0), Err(StoreError::Corrupted));
    assert_eq!(repo.purge_over_count(0), Err(StoreError::Corrupted));
    assert_eq!(repo.set_pinned(1, true, 25), Err(StoreError::Corrupted));
    assert_eq!(repo.delete_all(true), Err(StoreError::Corrupted));
    assert_eq!(repo.recreate(), Err(StoreError::Corrupted));

    let reader: &dyn HistoryReader = &repo;
    assert_eq!(reader.list_for_display(10), Err(StoreError::Corrupted));
    assert_eq!(reader.get_full(1), Err(StoreError::Corrupted));
    assert_eq!(reader.count_pinned(), Err(StoreError::Corrupted));
}

#[test]
fn failing_repository_close_always_succeeds_no_real_connection_to_release() {
    let boxed: Box<dyn HistoryReader> = Box::new(FailingHistoryRepository::new(StoreError::Io));
    boxed
        .close()
        .expect("fake não tem conexão real, close() nunca falha");
}
