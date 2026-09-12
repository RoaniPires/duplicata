mod support;

use std::time::Instant;

use duplicata_core::{HistoryReader, HistoryRepository};
use duplicata_store::{open, SqliteHistoryReader, SqliteHistoryRepository};
use support::text_record;

#[test]
fn list_for_display_on_the_read_connection_stays_under_30ms_with_500_items() {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");

    {
        let mut repo = SqliteHistoryRepository::new(open(&db_path).unwrap());
        for i in 0..500u32 {
            let mut tag = [0u8; 32];
            tag[..4].copy_from_slice(&i.to_le_bytes());
            let rec = text_record_with_tag(tag, &format!("item {i}"), i as u64);
            repo.upsert(&rec).unwrap();
        }
    }

    let reader = SqliteHistoryReader::open(&db_path).expect("banco populado deve abrir");

    let started = Instant::now();
    let items = reader
        .list_for_display(500)
        .expect("listar 500 itens deve funcionar");
    let elapsed = started.elapsed();

    assert_eq!(items.len(), 500);
    assert!(
        elapsed.as_millis() <= 30,
        "list_for_display na conexão de leitura deveria ficar em ≤30ms (SC-010); levou {elapsed:?}"
    );
}

fn text_record_with_tag(
    identity: [u8; 32],
    text: &str,
    ts_ms: u64,
) -> duplicata_core::CaptureRecord {
    let mut rec = text_record(0, text, ts_ms);
    rec.identity = duplicata_core::IdentityKey(identity);
    rec
}
