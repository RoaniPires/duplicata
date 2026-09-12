use duplicata_store::{migrations, SCHEMA_VERSION};
use rusqlite::Connection;

const SCHEMA_V1: &str = include_str!("../src/schema_v1.sql");

fn v1_only_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn
}

#[test]
fn migrating_a_v1_database_adds_has_text_with_default_zero_for_old_rows() {
    let conn = v1_only_connection();
    assert_eq!(
        migrations::schema_version(&conn).unwrap(),
        Some(1),
        "pré-condição do teste: precisa começar no schema v1"
    );

    conn.execute(
        "INSERT INTO clip(identity_hash, canonical_format_id, canonical_kind,
                          first_captured_utc, last_activity_utc, total_bytes)
         VALUES (x'00', 13, 'unicode_text', 1, 1, 1)",
        [],
    )
    .unwrap();

    migrations::apply(&conn).expect("migração v1->v2 deve suceder");

    let has_text: i64 = conn
        .query_row("SELECT has_text FROM clip", [], |r| r.get(0))
        .expect("coluna has_text deve existir e a linha antiga deve ter default 0");
    assert_eq!(has_text, 0);
    assert_eq!(
        migrations::schema_version(&conn).unwrap(),
        Some(SCHEMA_VERSION)
    );
}

#[test]
fn migration_is_idempotent_when_applied_twice() {
    let conn = v1_only_connection();
    migrations::apply(&conn).expect("primeira aplicação deve suceder");
    migrations::apply(&conn).expect("segunda aplicação deve ser no-op, não erro");
    assert_eq!(
        migrations::schema_version(&conn).unwrap(),
        Some(SCHEMA_VERSION)
    );
}
