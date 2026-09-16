use duplicata_core::canonical::{
    screen, select_canonical, Decision, CF_BITMAP, CF_DIB, CF_DIBV5, CF_HDROP, CF_LOCALE,
    CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT,
};
use duplicata_core::capture::FormatAnnounce;
use duplicata_core::{CanonicalKind, Config};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn f(id: u32, n: usize) -> duplicata_core::CapturedFormat {
    duplicata_core::CapturedFormat {
        format_id: id,
        format_name: None,
        bytes: vec![0u8; n],
    }
}

fn named(id: u32, name: &str, n: usize) -> duplicata_core::CapturedFormat {
    duplicata_core::CapturedFormat {
        format_id: id,
        format_name: Some(name.into()),
        bytes: vec![0u8; n],
    }
}

fn announced(formats: &[duplicata_core::CapturedFormat]) -> Vec<FormatAnnounce> {
    formats
        .iter()
        .map(|f| FormatAnnounce {
            format_id: f.format_id,
            format_name: f.format_name.clone(),
        })
        .collect()
}

fn screened_index(formats: &[duplicata_core::CapturedFormat]) -> Option<usize> {
    match screen(&announced(formats), None, &cfg()) {
        Decision::Copy { canonical_index } => Some(canonical_index),
        Decision::Reject(_) => None,
    }
}

#[test]
fn unicode_text_wins_over_everything() {
    let formats = vec![
        f(CF_TEXT, 10),
        named(0xC000, "HTML Format", 5000),
        f(CF_DIB, 9999),
        f(CF_UNICODETEXT, 6),
    ];
    let c = select_canonical(&formats).unwrap();
    assert_eq!(c.kind, CanonicalKind::UnicodeText);
    assert_eq!(c.format_id, CF_UNICODETEXT);
    assert_eq!(c.byte_len, 6);
}

#[test]
fn dibv5_beats_dib_when_enumerated_first() {
    let formats = vec![f(CF_DIBV5, 100), f(CF_DIB, 100)];
    assert_eq!(
        select_canonical(&formats).unwrap().kind,
        CanonicalKind::DibV5
    );
}

#[test]
fn dib_beats_dibv5_when_enumerated_first() {
    let formats = vec![f(CF_DIB, 100), f(CF_DIBV5, 100)];
    assert_eq!(select_canonical(&formats).unwrap().kind, CanonicalKind::Dib);
}

#[test]
fn falls_back_to_single_dib_variant() {
    assert_eq!(
        select_canonical(&[f(CF_DIB, 1)]).unwrap().kind,
        CanonicalKind::Dib
    );
    assert_eq!(
        select_canonical(&[f(CF_DIBV5, 1)]).unwrap().kind,
        CanonicalKind::DibV5
    );
}

#[test]
fn hdrop_is_next_after_text_and_image() {
    let formats = vec![named(0xC001, "Shell IDList Array", 999), f(CF_HDROP, 40)];
    let c = select_canonical(&formats).unwrap();
    assert_eq!(c.kind, CanonicalKind::HDrop);
    assert_eq!(c.format_id, CF_HDROP);
}

#[test]
fn first_registered_format_wins_when_no_standard_canonical() {
    let formats = vec![
        f(CF_LOCALE, 4),
        f(CF_OEMTEXT, 20),
        named(0xC010, "App Data A", 100),
        named(0xC020, "App Data B", 250),
        named(0xC030, "App Data C", 100),
    ];
    let c = select_canonical(&formats).unwrap();
    assert_eq!(c.kind, CanonicalKind::Custom);
    assert_eq!(
        c.format_id, 0xC010,
        "vence o primeiro registrado enunciado, não o maior (o critério antigo \
         teria escolhido 0xC020, de 250 bytes)"
    );
}

#[test]
fn registered_format_choice_never_depends_on_size() {
    let crescente = vec![named(0xC010, "A", 1), named(0xC020, "B", 9999)];
    let decrescente = vec![named(0xC010, "A", 9999), named(0xC020, "B", 1)];

    assert_eq!(
        select_canonical(&crescente).unwrap().format_id,
        select_canonical(&decrescente).unwrap().format_id
    );
    assert_eq!(select_canonical(&crescente).unwrap().format_id, 0xC010);
}

#[test]
fn screen_and_select_canonical_always_agree_on_the_same_format() {
    let casos = vec![
        vec![
            f(CF_TEXT, 10),
            named(0xC000, "HTML Format", 5000),
            f(CF_DIB, 9999),
            f(CF_UNICODETEXT, 6),
        ],
        vec![f(CF_DIBV5, 100), f(CF_DIB, 100)],
        vec![f(CF_DIB, 100), f(CF_DIBV5, 100)],
        vec![named(0xC001, "Shell IDList Array", 999), f(CF_HDROP, 40)],
        vec![
            f(CF_LOCALE, 4),
            named(0xC010, "App Data A", 100),
            named(0xC020, "App Data B", 250),
        ],
    ];

    for formats in casos {
        let esperado = select_canonical(&formats).unwrap();
        let index = screened_index(&formats).expect("screen deveria aprovar");
        assert_eq!(
            formats[index].format_id,
            esperado.format_id,
            "screen e select_canonical divergiram em {:?}",
            formats.iter().map(|f| f.format_id).collect::<Vec<_>>()
        );
    }
}

#[test]
fn none_when_only_auxiliary_formats() {
    let formats = vec![
        f(CF_TEXT, 10),
        f(CF_OEMTEXT, 10),
        f(CF_LOCALE, 4),
        f(CF_BITMAP, 8),
    ];
    assert!(select_canonical(&formats).is_none());
    assert!(screened_index(&formats).is_none());
}

#[test]
fn none_when_empty() {
    assert!(select_canonical(&[]).is_none());
}
