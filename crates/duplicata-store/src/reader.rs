use std::path::Path;
use std::time::Duration;

use duplicata_core::{ClipListItem, HistoryReader, StoreError, StoredClip};
use rusqlite::{Connection, OpenFlags};

use crate::error::map_sqlite_error;
use crate::repository::{count_pinned_impl, get_full_impl, list_for_display_impl};

const READ_BUSY_TIMEOUT: Duration = Duration::from_millis(50);

pub struct SqliteHistoryReader {
    conn: Connection,
}

impl SqliteHistoryReader {
    pub fn open(path: &Path) -> Result<Self, StoreError> {
        let conn = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
            .map_err(|e| map_sqlite_error(&e))?;
        conn.busy_timeout(READ_BUSY_TIMEOUT)
            .map_err(|e| map_sqlite_error(&e))?;
        Ok(SqliteHistoryReader { conn })
    }
}

impl HistoryReader for SqliteHistoryReader {
    fn list_for_display(&self, limit: usize) -> Result<Vec<ClipListItem>, StoreError> {
        list_for_display_impl(&self.conn, limit)
    }

    fn get_full(&self, clip_id: i64) -> Result<Option<StoredClip>, StoreError> {
        get_full_impl(&self.conn, clip_id)
    }

    fn count_pinned(&self) -> Result<u32, StoreError> {
        count_pinned_impl(&self.conn)
    }

    fn close(self: Box<Self>) -> Result<(), StoreError> {
        let this = *self;
        this.conn.close().map_err(|(_, e)| map_sqlite_error(&e))
    }
}
