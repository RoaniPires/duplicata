use duplicata_core::StoreError;
use rusqlite::ffi::ErrorCode;
use rusqlite::Error as SqliteError;

pub fn map_sqlite_error(err: &SqliteError) -> StoreError {
    match err {
        SqliteError::SqliteFailure(ffi, _) => match ffi.code {
            ErrorCode::DatabaseCorrupt | ErrorCode::NotADatabase => StoreError::Corrupted,
            ErrorCode::CannotOpen
            | ErrorCode::PermissionDenied
            | ErrorCode::ReadOnly
            | ErrorCode::DiskFull
            | ErrorCode::SystemIoFailure
            | ErrorCode::DatabaseBusy
            | ErrorCode::DatabaseLocked => StoreError::Io,
            _ => StoreError::Query,
        },
        SqliteError::InvalidPath(_) => StoreError::Io,
        SqliteError::SqliteSingleThreadedMode => StoreError::Io,
        _ => StoreError::Query,
    }
}
