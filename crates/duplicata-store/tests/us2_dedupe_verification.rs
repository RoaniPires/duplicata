mod support;

use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    identity_of, CaptureRecord, CapturedFormat, HistoryRepository, UpsertOutcome,
};
use duplicata_store::{open, SqliteHistoryRepository};
use support::{cf_html, utf16le};

const CF_TEXT: u32 = 1;
const CF_UNICODETEXT: u32 = 13;
const CF_LOCALE: u32 = 16;
const CF_HTML: u32 = 0xC000;
const CF_OBJDESC: u32 = 0xC001;
const CF_SOURCE_URL: u32 = 0xC002;

fn repo() -> (tempfile::TempDir, SqliteHistoryRepository) {
    let dir = tempfile::tempdir().unwrap();
    let conn = open(&dir.path().join("duplicata.db")).unwrap();
    (dir, SqliteHistoryRepository::new(conn))
}

fn rec(text: &str, ts: u64, aux: Vec<CapturedFormat>) -> CaptureRecord {
    let unicode = utf16le(text);
    let identity = identity_of(&unicode);
    let mut formats = vec![CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: unicode.clone(),
    }];
    formats.extend(aux);
    let total = formats.iter().map(|f| f.bytes.len() as u64).sum();
    CaptureRecord {
        identity,
        canonical: CanonicalSelection {
            format_id: CF_UNICODETEXT,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: unicode.len() as u64,
        },
        captured_at: duplicata_core::Timestamp::from_millis(ts),
        total_bytes: total,
        preview: Some(text.to_string()),
        thumbnail: None,
        has_text: true,
        formats,
    }
}

fn aux(id: u32, name: &str, bytes: &[u8]) -> CapturedFormat {
    CapturedFormat {
        format_id: id,
        format_name: Some(name.into()),
        bytes: bytes.to_vec(),
    }
}

fn count(repo: &SqliteHistoryRepository, sql: &str) -> i64 {
    repo.connection().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn us2_1_repeated_content_reuses_the_entry_and_bumps_recency() {
    let (_d, mut repo) = repo();
    let first = repo.upsert(&rec("comando repetido", 100, vec![])).unwrap();
    let id = first.clip_id();
    assert!(matches!(first, UpsertOutcome::Inserted { .. }));

    let again = repo.upsert(&rec("comando repetido", 900, vec![])).unwrap();
    assert_eq!(again, UpsertOutcome::Deduped { clip_id: id });
    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 1);

    let last: i64 = repo
        .connection()
        .query_row(
            "SELECT last_activity_utc FROM clip WHERE id = ?1",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(last, 900);
}

#[test]
fn us2_2_and_sc002_ten_repeats_intercalated_yield_one_entry_that_is_most_recent() {
    let (_d, mut repo) = repo();
    let x = "conteudo X";

    let mut ts = 0u64;
    let mut tick = || {
        ts += 10;
        ts
    };

    repo.upsert(&rec(x, tick(), vec![])).unwrap();
    for i in 0..10 {
        repo.upsert(&rec(&format!("ruído {i}"), tick(), vec![]))
            .unwrap();
        repo.upsert(&rec(x, tick(), vec![])).unwrap();
    }

    let x_hash = identity_of(&utf16le(x)).0;
    let x_rows: i64 = repo
        .connection()
        .query_row(
            "SELECT count(*) FROM clip WHERE identity_hash = ?1",
            [&x_hash[..]],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(x_rows, 1, "10 repetições de X = 1 entrada (SC-002)");

    let newest_hash: Vec<u8> = repo
        .connection()
        .query_row(
            "SELECT identity_hash FROM clip ORDER BY last_activity_utc DESC LIMIT 1",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        newest_hash, x_hash,
        "X é a mais recente após a última repetição"
    );
}

#[test]
fn us2_3_any_byte_difference_in_the_canonical_creates_a_new_entry() {
    let (_d, mut repo) = repo();
    repo.upsert(&rec("linha\r\nfim", 1, vec![])).unwrap();
    repo.upsert(&rec("linha\nfim", 2, vec![])).unwrap();
    repo.upsert(&rec("linha fim", 3, vec![])).unwrap();
    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 3);
}

#[test]
fn us2_4_first_capture_preserved_and_formats_replaced_by_most_recent() {
    let (_d, mut repo) = repo();
    repo.upsert(&rec(
        "codigo",
        100,
        vec![aux(CF_HTML, "HTML Format", b"<b>v1</b>")],
    ))
    .unwrap();
    let id: i64 = count(&repo, "SELECT id FROM clip");

    repo.upsert(&rec(
        "codigo",
        700,
        vec![aux(CF_HTML, "HTML Format", b"<i>v2</i>")],
    ))
    .unwrap();

    let (first, last): (i64, i64) = repo
        .connection()
        .query_row(
            "SELECT first_captured_utc, last_activity_utc FROM clip",
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap();
    assert_eq!(first, 100, "first_captured imutável (FR-007)");
    assert_eq!(last, 700);

    let html: Vec<u8> = repo
        .connection()
        .query_row(
            "SELECT bytes FROM clip_format WHERE clip_id = ?1 AND format_id = 49152",
            [id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(html, b"<i>v2</i>", "formatos = os da cópia mais recente");
}

#[test]
fn us2_5_browser_copy_then_editor_copy_collapse_to_one_entry() {
    let (_d, mut repo) = repo();

    let html = cf_html("<p>mesmo texto</p>", "https://exemplo.test/pagina");
    assert!(
        html.starts_with(b"Version:0.9\r\nStartHTML:"),
        "fixture precisa do cabeçalho real do CF_HTML"
    );
    repo.upsert(&rec(
        "mesmo texto",
        100,
        vec![
            aux(CF_HTML, "HTML Format", &html),
            aux(CF_OBJDESC, "Object Descriptor", b"\x01\x02\x03"),
            aux(CF_SOURCE_URL, "SourceURL", b"https://exemplo.test/pagina"),
            aux(CF_LOCALE, "", &1033u32.to_le_bytes()),
        ],
    ))
    .unwrap();

    let out = repo.upsert(&rec("mesmo texto", 500, vec![])).unwrap();

    assert!(
        matches!(out, UpsertOutcome::Deduped { .. }),
        "auxiliares não influenciam a identidade"
    );
    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 1);
    assert_eq!(count(&repo, "SELECT count(*) FROM clip_format"), 1);
}

#[test]
fn us2_6_cf_html_with_different_fragment_offsets_is_still_one_entry() {
    let (_d, mut repo) = repo();
    let html_a = cf_html("<p>trecho</p>", "https://site-a.test/artigo");
    let html_b = cf_html(
        "<p>trecho</p>",
        "https://outro-site.test/materia-mais-longa",
    );
    assert_ne!(
        html_a, html_b,
        "os offsets realmente divergem entre as cópias"
    );

    repo.upsert(&rec(
        "trecho",
        1,
        vec![aux(CF_HTML, "HTML Format", &html_a)],
    ))
    .unwrap();
    let out = repo
        .upsert(&rec(
            "trecho",
            2,
            vec![aux(CF_HTML, "HTML Format", &html_b)],
        ))
        .unwrap();
    assert!(matches!(out, UpsertOutcome::Deduped { .. }));
    assert_eq!(
        count(&repo, "SELECT count(*) FROM clip"),
        1,
        "offsets do CF_HTML são ruído"
    );

    let stored: Vec<u8> = repo
        .connection()
        .query_row(
            "SELECT bytes FROM clip_format WHERE format_id = 49152",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(stored, html_b);
}

#[test]
fn list_recent_orders_by_recency_after_a_dedupe_bump() {
    let (_d, mut repo) = repo();
    repo.upsert(&rec("a", 10, vec![])).unwrap();
    repo.upsert(&rec("b", 20, vec![])).unwrap();
    repo.upsert(&rec("c", 30, vec![])).unwrap();
    repo.upsert(&rec("a", 40, vec![])).unwrap();

    let recent = repo.list_recent(10).unwrap();
    let previews: Vec<Option<String>> = recent.iter().map(|s| s.preview.clone()).collect();
    assert_eq!(
        previews,
        vec![Some("a".into()), Some("c".into()), Some("b".into())]
    );
    assert_eq!(recent[0].last_activity_ms, 40);
    assert_eq!(recent[0].first_captured_ms, 10, "first_captured preservado");
}

#[test]
fn fr006b_any_set_of_auxiliary_formats_never_affects_identity() {
    let (_d, mut repo) = repo();
    repo.upsert(&rec(
        "X",
        1,
        vec![
            aux(CF_TEXT, "", b"X"),
            aux(CF_LOCALE, "", &0u32.to_le_bytes()),
        ],
    ))
    .unwrap();
    repo.upsert(&rec(
        "X",
        2,
        vec![
            aux(
                0xC0FF,
                "Formato Proprietario",
                b"lixo binario arbitrario aqui",
            ),
            aux(CF_HTML, "HTML Format", b"<div>X</div>"),
        ],
    ))
    .unwrap();
    assert_eq!(count(&repo, "SELECT count(*) FROM clip"), 1);
}
