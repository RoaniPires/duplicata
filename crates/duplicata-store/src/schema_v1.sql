-- duplicata — esquema SQLite v1 (DDL apenas; os pragmas de conexão são
-- aplicados por open.rs). Idempotente.

CREATE TABLE IF NOT EXISTS schema_meta (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS clip (
    id                    INTEGER PRIMARY KEY AUTOINCREMENT,
    identity_hash         BLOB    NOT NULL UNIQUE,
    canonical_format_id   INTEGER NOT NULL,
    canonical_format_name TEXT,
    canonical_kind        TEXT    NOT NULL,
    first_captured_utc    INTEGER NOT NULL,
    last_activity_utc     INTEGER NOT NULL,
    total_bytes           INTEGER NOT NULL,
    preview               TEXT,
    thumbnail             BLOB
);

CREATE INDEX IF NOT EXISTS ix_clip_last_activity ON clip(last_activity_utc DESC);

CREATE TABLE IF NOT EXISTS clip_format (
    id           INTEGER PRIMARY KEY AUTOINCREMENT,
    clip_id      INTEGER NOT NULL REFERENCES clip(id) ON DELETE CASCADE,
    format_id    INTEGER NOT NULL,
    format_name  TEXT,
    is_canonical INTEGER NOT NULL DEFAULT 0 CHECK (is_canonical IN (0, 1)),
    byte_len     INTEGER NOT NULL,
    bytes        BLOB    NOT NULL
);

CREATE INDEX IF NOT EXISTS ix_clip_format_clip ON clip_format(clip_id);

INSERT OR IGNORE INTO schema_meta(key, value) VALUES ('schema_version', '1');
