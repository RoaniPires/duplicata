use duplicata_core::StoreError;
use rusqlite::Connection;

use crate::error::map_sqlite_error;

const SCHEMA_V1: &str = include_str!("schema_v1.sql");
const MIGRATION_V2: &str = include_str!("migration_v2.sql");
const MIGRATION_V3: &str = include_str!("migration_v3.sql");

pub fn apply(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch(SCHEMA_V1)
        .map_err(|e| match map_sqlite_error(&e) {
            StoreError::Query => StoreError::Migration,
            other => other,
        })?;

    if schema_version(conn)?.unwrap_or(1) < 2 {
        conn.execute_batch(MIGRATION_V2)
            .map_err(|e| match map_sqlite_error(&e) {
                StoreError::Query => StoreError::Migration,
                other => other,
            })?;
    }

    if schema_version(conn)?.unwrap_or(1) < 3 {
        conn.execute_batch(MIGRATION_V3)
            .map_err(|e| match map_sqlite_error(&e) {
                StoreError::Query => StoreError::Migration,
                other => other,
            })?;
    }
    Ok(())
}

pub fn schema_version(conn: &Connection) -> Result<Option<i64>, StoreError> {
    conn.query_row(
        "SELECT value FROM schema_meta WHERE key = 'schema_version'",
        [],
        |row| row.get::<_, String>(0),
    )
    .map(|s| s.parse::<i64>().ok())
    .or_else(|e| match e {
        rusqlite::Error::QueryReturnedNoRows => Ok(None),
        other => Err(map_sqlite_error(&other)),
    })
}
