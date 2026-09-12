use std::path::Path;

use duplicata_core::StoreError;
use rusqlite::Connection;

use crate::error::map_sqlite_error;
use crate::migrations;

pub fn open(path: &Path) -> Result<Connection, StoreError> {
    let conn = Connection::open(path).map_err(|e| map_sqlite_error(&e))?;
    apply_pragmas(&conn)?;
    migrations::apply(&conn)?;
    Ok(conn)
}

fn apply_pragmas(conn: &Connection) -> Result<(), StoreError> {
    let mode: String = conn
        .query_row("PRAGMA journal_mode = WAL", [], |row| row.get(0))
        .map_err(|e| map_sqlite_error(&e))?;
    if !mode.eq_ignore_ascii_case("wal") && !mode.eq_ignore_ascii_case("memory") {
        return Err(StoreError::Io);
    }

    conn.pragma_update(None, "synchronous", "NORMAL")
        .map_err(|e| map_sqlite_error(&e))?;
    conn.pragma_update(None, "foreign_keys", true)
        .map_err(|e| map_sqlite_error(&e))?;
    conn.pragma_update(None, "secure_delete", true)
        .map_err(|e| map_sqlite_error(&e))?;
    Ok(())
}

pub fn checkpoint_truncate(conn: &Connection) -> Result<(), StoreError> {
    conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
        .map_err(|e| map_sqlite_error(&e))
}
