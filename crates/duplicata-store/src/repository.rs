use std::path::PathBuf;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::{
    CaptureRecord, ClipListItem, HistoryReader, HistoryRepository, SetPinnedOutcome, StoreError,
    StoredClip, UpsertOutcome,
};
use rusqlite::{params, Connection, OptionalExtension, Transaction};

use crate::checkpoint_truncate;
use crate::error::map_sqlite_error;

pub struct SqliteHistoryRepository {
    conn: Connection,
    real_db_path: PathBuf,
}

impl SqliteHistoryRepository {
    pub fn new(conn: Connection) -> Self {
        let real_db_path = conn.path().map(PathBuf::from).unwrap_or_default();
        SqliteHistoryRepository { conn, real_db_path }
    }

    pub fn new_at(conn: Connection, real_db_path: std::path::PathBuf) -> Self {
        SqliteHistoryRepository { conn, real_db_path }
    }

    pub fn connection(&self) -> &Connection {
        &self.conn
    }

    #[cfg(windows)]
    fn toggle_encryption_win(
        &mut self,
        enable: bool,
        progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        use crate::crypto;

        checkpoint_truncate(&self.conn)?;

        let placeholder = Connection::open_in_memory().map_err(|e| map_sqlite_error(&e))?;
        let old = std::mem::replace(&mut self.conn, placeholder);
        old.close().map_err(|(_, e)| map_sqlite_error(&e))?;

        let real = self.real_db_path.clone();
        let tmp = crypto::tmp_path(&real);
        let work = crypto::work_copy_path(&real);

        let outcome: Result<PathBuf, (PathBuf, StoreError)> = if enable {
            match crypto::protect_file(&real, &real, &tmp, progress) {
                Err(e) => Err((real.clone(), e)),
                Ok(()) => {
                    let mut last = Ok(());
                    for attempt in 0..3 {
                        match crypto::unprotect_file(&real, &work, &tmp, progress) {
                            Ok(()) => {
                                last = Ok(());
                                break;
                            }
                            Err(e) => {
                                last = Err(e);
                                if attempt < 2 {
                                    std::thread::sleep(std::time::Duration::from_millis(50));
                                }
                            }
                        }
                    }
                    match last {
                        Ok(()) => Ok(work.clone()),
                        Err(e) => Err((real.clone(), e)),
                    }
                }
            }
        } else {
            match std::fs::rename(&work, &real) {
                Ok(()) => Ok(real.clone()),
                Err(_) => Err((work.clone(), StoreError::Io)),
            }
        };

        match outcome {
            Ok(effective) => {
                self.conn = crate::open(&effective)?;
                Ok(())
            }
            Err((fallback, err)) => {
                self.conn = crate::open(&fallback)?;
                Err(err)
            }
        }
    }

    pub fn list_recent(&self, limit: usize) -> Result<Vec<ClipSummary>, StoreError> {
        let mut stmt = self
            .conn
            .prepare(
                "SELECT id, canonical_kind, total_bytes, first_captured_utc,
                        last_activity_utc, preview
                   FROM clip
                  ORDER BY last_activity_utc DESC, id DESC
                  LIMIT ?1",
            )
            .map_err(|e| map_sqlite_error(&e))?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                Ok(ClipSummary {
                    id: r.get(0)?,
                    canonical_kind: r.get(1)?,
                    total_bytes: r.get::<_, i64>(2)? as u64,
                    first_captured_ms: r.get::<_, i64>(3)? as u64,
                    last_activity_ms: r.get::<_, i64>(4)? as u64,
                    preview: r.get(5)?,
                })
            })
            .map_err(|e| map_sqlite_error(&e))?;
        rows.collect::<Result<Vec<_>, _>>()
            .map_err(|e| map_sqlite_error(&e))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipSummary {
    pub id: i64,
    pub canonical_kind: String,
    pub total_bytes: u64,
    pub first_captured_ms: u64,
    pub last_activity_ms: u64,
    pub preview: Option<String>,
}

pub(crate) fn list_for_display_impl(
    conn: &Connection,
    limit: usize,
) -> Result<Vec<ClipListItem>, StoreError> {
    let mut stmt = conn
        .prepare(
            "SELECT id, canonical_kind, preview, thumbnail, has_text, last_activity_utc, pinned
               FROM clip
              ORDER BY pinned DESC, last_activity_utc DESC, id DESC
              LIMIT ?1",
        )
        .map_err(|e| map_sqlite_error(&e))?;
    let rows = stmt
        .query_map([limit as i64], |r| {
            Ok(ClipListItem {
                id: r.get(0)?,
                canonical_kind: r.get(1)?,
                preview: r.get(2)?,
                thumbnail: r.get(3)?,
                has_text: r.get::<_, i64>(4)? != 0,
                last_activity_ms: r.get::<_, i64>(5)? as u64,
                pinned: r.get::<_, i64>(6)? != 0,
            })
        })
        .map_err(|e| map_sqlite_error(&e))?;
    rows.collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_error(&e))
}

pub(crate) fn count_pinned_impl(conn: &Connection) -> Result<u32, StoreError> {
    conn.query_row("SELECT COUNT(*) FROM clip WHERE pinned = 1", [], |r| {
        r.get::<_, i64>(0)
    })
    .map(|n| n as u32)
    .map_err(|e| map_sqlite_error(&e))
}

pub(crate) fn get_full_impl(
    conn: &Connection,
    clip_id: i64,
) -> Result<Option<StoredClip>, StoreError> {
    let exists: bool = conn
        .query_row("SELECT 1 FROM clip WHERE id = ?1", [clip_id], |_| Ok(()))
        .optional()
        .map_err(|e| map_sqlite_error(&e))?
        .is_some();
    if !exists {
        return Ok(None);
    }

    let mut stmt = conn
        .prepare("SELECT format_id, format_name, bytes FROM clip_format WHERE clip_id = ?1")
        .map_err(|e| map_sqlite_error(&e))?;
    let rows = stmt
        .query_map([clip_id], |r| {
            Ok((
                r.get::<_, u32>(0)?,
                r.get::<_, Option<String>>(1)?,
                r.get::<_, Vec<u8>>(2)?,
            ))
        })
        .map_err(|e| map_sqlite_error(&e))?;
    let formats = rows
        .collect::<Result<Vec<_>, _>>()
        .map_err(|e| map_sqlite_error(&e))?;

    let text_format = formats
        .iter()
        .find(|(format_id, _, _)| *format_id == CF_UNICODETEXT)
        .map(|(format_id, _, bytes)| (*format_id, bytes.clone()));

    Ok(Some(StoredClip {
        formats,
        text_format,
    }))
}

impl HistoryReader for SqliteHistoryRepository {
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

impl HistoryRepository for SqliteHistoryRepository {
    fn upsert(&mut self, rec: &CaptureRecord) -> Result<UpsertOutcome, StoreError> {
        let tx = self.conn.transaction().map_err(|e| map_sqlite_error(&e))?;

        let identity = &rec.identity.as_bytes()[..];
        let existing: Option<i64> = tx
            .query_row(
                "SELECT id FROM clip WHERE identity_hash = ?1",
                [identity],
                |r| r.get(0),
            )
            .optional()
            .map_err(|e| map_sqlite_error(&e))?;

        let ts = rec.captured_at.as_millis() as i64;
        let kind = rec.canonical.kind.as_str();

        let outcome = match existing {
            Some(clip_id) => {
                tx.execute(
                    "UPDATE clip
                        SET last_activity_utc = ?1, total_bytes = ?2,
                            canonical_format_id = ?3, canonical_format_name = ?4,
                            canonical_kind = ?5, preview = ?6, thumbnail = ?7,
                            has_text = ?8
                      WHERE id = ?9",
                    params![
                        ts,
                        rec.total_bytes as i64,
                        rec.canonical.format_id,
                        rec.canonical.format_name,
                        kind,
                        rec.preview,
                        rec.thumbnail,
                        rec.has_text as i64,
                        clip_id,
                    ],
                )
                .map_err(|e| map_sqlite_error(&e))?;

                tx.execute("DELETE FROM clip_format WHERE clip_id = ?1", [clip_id])
                    .map_err(|e| map_sqlite_error(&e))?;
                insert_formats(&tx, clip_id, rec)?;

                UpsertOutcome::Deduped { clip_id }
            }
            None => {
                tx.execute(
                    "INSERT INTO clip(identity_hash, canonical_format_id, canonical_format_name,
                                      canonical_kind, first_captured_utc, last_activity_utc,
                                      total_bytes, preview, thumbnail, has_text)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?5, ?6, ?7, ?8, ?9)",
                    params![
                        identity,
                        rec.canonical.format_id,
                        rec.canonical.format_name,
                        kind,
                        ts,
                        rec.total_bytes as i64,
                        rec.preview,
                        rec.thumbnail,
                        rec.has_text as i64,
                    ],
                )
                .map_err(|e| map_sqlite_error(&e))?;

                let clip_id = tx.last_insert_rowid();
                insert_formats(&tx, clip_id, rec)?;

                UpsertOutcome::Inserted { clip_id }
            }
        };

        tx.commit().map_err(|e| map_sqlite_error(&e))?;
        Ok(outcome)
    }

    fn purge_older_than(&mut self, cutoff_ms: u64) -> Result<u64, StoreError> {
        let removed = self
            .conn
            .execute(
                "DELETE FROM clip WHERE pinned = 0 AND last_activity_utc < ?1",
                [cutoff_ms as i64],
            )
            .map_err(|e| map_sqlite_error(&e))?;
        checkpoint_truncate(&self.conn)?;
        Ok(removed as u64)
    }

    fn purge_over_count(&mut self, max_items: u32) -> Result<u64, StoreError> {
        let removed = self
            .conn
            .execute(
                "DELETE FROM clip WHERE pinned = 0 AND id NOT IN (
                    SELECT id FROM clip WHERE pinned = 0
                     ORDER BY last_activity_utc DESC, id DESC
                     LIMIT ?1
                 )",
                [max_items as i64],
            )
            .map_err(|e| map_sqlite_error(&e))?;
        checkpoint_truncate(&self.conn)?;
        Ok(removed as u64)
    }

    fn set_pinned(
        &mut self,
        clip_id: i64,
        pinned: bool,
        max_pinned: u32,
    ) -> Result<SetPinnedOutcome, StoreError> {
        let tx = self.conn.transaction().map_err(|e| map_sqlite_error(&e))?;

        let current_pinned: Option<i64> = tx
            .query_row("SELECT pinned FROM clip WHERE id = ?1", [clip_id], |r| {
                r.get(0)
            })
            .optional()
            .map_err(|e| map_sqlite_error(&e))?;

        let outcome = match current_pinned {
            None => SetPinnedOutcome::NotFound,
            Some(_) if !pinned => {
                tx.execute("UPDATE clip SET pinned = 0 WHERE id = ?1", [clip_id])
                    .map_err(|e| map_sqlite_error(&e))?;
                SetPinnedOutcome::Applied
            }
            Some(already) if already != 0 => SetPinnedOutcome::Applied,
            Some(_) => {
                let current: i64 = tx
                    .query_row("SELECT COUNT(*) FROM clip WHERE pinned = 1", [], |r| {
                        r.get(0)
                    })
                    .map_err(|e| map_sqlite_error(&e))?;
                if current as u32 >= max_pinned {
                    SetPinnedOutcome::PinCapReached {
                        limit: max_pinned,
                        current: current as u32,
                    }
                } else {
                    tx.execute("UPDATE clip SET pinned = 1 WHERE id = ?1", [clip_id])
                        .map_err(|e| map_sqlite_error(&e))?;
                    SetPinnedOutcome::Applied
                }
            }
        };

        tx.commit().map_err(|e| map_sqlite_error(&e))?;
        Ok(outcome)
    }

    fn delete_all(&mut self, keep_pinned: bool) -> Result<u64, StoreError> {
        let removed = if keep_pinned {
            self.conn.execute("DELETE FROM clip WHERE pinned = 0", [])
        } else {
            self.conn.execute("DELETE FROM clip", [])
        }
        .map_err(|e| map_sqlite_error(&e))?;
        checkpoint_truncate(&self.conn)?;
        Ok(removed as u64)
    }

    #[cfg(windows)]
    fn toggle_encryption(
        &mut self,
        enable: bool,
        progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        self.toggle_encryption_win(enable, progress)
    }

    #[cfg(not(windows))]
    fn toggle_encryption(
        &mut self,
        _enable: bool,
        _progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        Err(StoreError::Io)
    }

    fn recreate(&mut self) -> Result<(), StoreError> {
        let path: PathBuf = self.conn.path().map(PathBuf::from).ok_or(StoreError::Io)?;

        let placeholder = Connection::open_in_memory().map_err(|e| map_sqlite_error(&e))?;
        let old = std::mem::replace(&mut self.conn, placeholder);
        old.close().map_err(|(_, e)| map_sqlite_error(&e))?;

        let mut remove_failed = false;
        for suffix in ["", "-wal", "-shm"] {
            let mut with_suffix = path.clone().into_os_string();
            with_suffix.push(suffix);
            match std::fs::remove_file(PathBuf::from(with_suffix)) {
                Ok(()) => {}
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
                Err(_) => remove_failed = true,
            }
        }

        self.conn = crate::open(&path)?;

        if remove_failed {
            return Err(StoreError::Io);
        }
        Ok(())
    }
}

fn insert_formats(tx: &Transaction, clip_id: i64, rec: &CaptureRecord) -> Result<(), StoreError> {
    let mut stmt = tx
        .prepare(
            "INSERT INTO clip_format(clip_id, format_id, format_name, is_canonical, byte_len, bytes)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
        )
        .map_err(|e| map_sqlite_error(&e))?;

    let mut canonical_marked = false;
    for f in &rec.formats {
        let is_canonical = !canonical_marked && f.format_id == rec.canonical.format_id;
        if is_canonical {
            canonical_marked = true;
        }
        stmt.execute(params![
            clip_id,
            f.format_id,
            f.format_name,
            is_canonical as i64,
            f.bytes.len() as i64,
            f.bytes,
        ])
        .map_err(|e| map_sqlite_error(&e))?;
    }
    Ok(())
}
