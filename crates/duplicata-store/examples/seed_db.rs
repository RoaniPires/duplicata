use std::error::Error;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use duplicata_core::{
    identity_of, unicode_text_format, CanonicalKind, CanonicalSelection, CaptureRecord,
    HistoryRepository, Timestamp,
};
use duplicata_store::{open, SqliteHistoryRepository};

const SPACING_MS: u64 = 60_000;
const BASE_OFFSET_MS: u64 = 3_600_000;

fn main() -> Result<(), Box<dyn Error>> {
    let n: u32 = match std::env::args().nth(1) {
        Some(arg) => arg
            .parse()
            .map_err(|_| format!("N inválido: {arg:?} (esperado inteiro > 0)"))?,
        None => return Err("uso: cargo run -p duplicata-store --example seed_db -- <N>".into()),
    };
    if n == 0 {
        return Err("N deve ser > 0".into());
    }

    let db_path = db_path()?;
    if let Some(parent) = db_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    println!("banco: {}", db_path.display());

    let conn = open(&db_path)?;
    let mut repo = SqliteHistoryRepository::new(conn);

    let now_ms = SystemTime::now().duration_since(UNIX_EPOCH)?.as_millis() as u64;

    for i in 1..=n {
        let text = format!("item de teste {i}");
        let captured_ms = now_ms - BASE_OFFSET_MS - u64::from(n - i) * SPACING_MS;

        let canonical = unicode_text_format(&text);
        let canonical_len = canonical.bytes.len() as u64;
        let identity = identity_of(&canonical.bytes);
        let formats = vec![canonical];
        let total_bytes = formats.iter().map(|f| f.bytes.len() as u64).sum();

        let record = CaptureRecord {
            identity,
            canonical: CanonicalSelection {
                format_id: 13,
                format_name: None,
                kind: CanonicalKind::UnicodeText,
                byte_len: canonical_len,
            },
            captured_at: Timestamp::from_millis(captured_ms),
            total_bytes,
            preview: Some(text),
            thumbnail: None,
            has_text: true,
            formats,
        };

        repo.upsert(&record)?;

        if i % 100 == 0 || i == n {
            println!("  {i}/{n}");
        }
    }

    println!("pronto: {n} itens inseridos (preview \"item de teste 1\"..\"item de teste {n}\").");
    Ok(())
}

fn db_path() -> Result<PathBuf, Box<dyn Error>> {
    let local_app_data =
        std::env::var_os("LOCALAPPDATA").ok_or("variável de ambiente LOCALAPPDATA não definida")?;
    Ok(PathBuf::from(local_app_data)
        .join("duplicata")
        .join("duplicata.db"))
}
