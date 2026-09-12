use duplicata_store::migrations;
use rusqlite::Connection;

const SCHEMA_V1: &str = include_str!("../src/schema_v1.sql");
const MIGRATION_V2: &str = include_str!("../src/migration_v2.sql");

fn v2_only_connection() -> Connection {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(SCHEMA_V1).unwrap();
    conn.execute_batch(MIGRATION_V2).unwrap();
    conn
}

#[test]
fn migrating_a_v2_database_adds_pinned_with_default_zero_for_old_rows() {
    let conn = v2_only_connection();
    assert_eq!(
        migrations::schema_version(&conn).unwrap(),
        Some(2),
        "pré-condição do teste: precisa começar no schema v2"
    );

    conn.execute(
        "INSERT INTO clip(identity_hash, canonical_format_id, canonical_kind,
                          first_captured_utc, last_activity_utc, total_bytes, has_text)
         VALUES (x'00', 13, 'unicode_text', 1, 1, 1, 0)",
        [],
    )
    .unwrap();

    migrations::apply(&conn).expect("migração v2->v3 deve suceder");

    let pinned: i64 = conn
        .query_row("SELECT pinned FROM clip", [], |r| r.get(0))
        .expect("coluna pinned deve existir e a linha antiga deve ter default 0");
    assert_eq!(pinned, 0);
    assert_eq!(migrations::schema_version(&conn).unwrap(), Some(3));
}

#[test]
fn migration_is_idempotent_when_applied_twice() {
    let conn = v2_only_connection();
    migrations::apply(&conn).expect("primeira aplicação deve suceder");
    migrations::apply(&conn).expect("segunda aplicação deve ser no-op, não erro");
    assert_eq!(migrations::schema_version(&conn).unwrap(), Some(3));
}
