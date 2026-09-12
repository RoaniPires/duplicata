#![deny(unsafe_code)]

#[cfg(windows)]
#[allow(unsafe_code)]
pub mod crypto;
mod error;
pub mod migrations;
mod open;
mod reader;
mod repository;

pub use error::map_sqlite_error;
pub use open::{checkpoint_truncate, open};
pub use reader::SqliteHistoryReader;
pub use repository::{ClipSummary, SqliteHistoryRepository};

pub const SCHEMA_VERSION: i64 = 3;
