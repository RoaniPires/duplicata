use duplicata_core::canonical::{
    select_canonical, CF_BITMAP, CF_DIB, CF_DIBV5, CF_HDROP, CF_LOCALE, CF_OEMTEXT, CF_TEXT,
    CF_UNICODETEXT,
};
use duplicata_core::capture::FormatInfo;
use duplicata_core::{select_canonical_from_info, CanonicalKind};

fn info_named(id: u32, name: &str, byte_len: u64) -> FormatInfo {
    FormatInfo {
        format_id: id,
        format_name: Some(name.into()),
        byte_len,
    }
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
fn largest_registered_format_wins_when_no_standard_canonical() {
    let formats = vec![
        f(CF_LOCALE, 4),
        f(CF_OEMTEXT, 20),
        named(0xC010, "App Data A", 100),
        named(0xC020, "App Data B", 250),
        named(0xC030, "App Data C", 100),
    ];
    let c = select_canonical(&formats).unwrap();
    assert_eq!(c.kind, CanonicalKind::Custom);
    assert_eq!(c.format_id, 0xC020);
    assert_eq!(c.byte_len, 250);
}

#[test]
fn custom_size_tie_breaks_by_smallest_format_id() {
    let formats = vec![
        named(0xC0FF, "big id", 500),
        named(0xC001, "small id", 500),
        named(0xC080, "mid id", 500),
    ];
    assert_eq!(select_canonical(&formats).unwrap().format_id, 0xC001);
}

#[test]
fn custom_size_tie_break_from_info_matches_the_post_copy_result() {
    let infos = vec![
        info_named(0xC0FF, "big id", 500),
        info_named(0xC001, "small id", 500),
        info_named(0xC080, "mid id", 500),
    ];
    let (index, selection) = select_canonical_from_info(&infos).unwrap();
    assert_eq!(index, 1);
    assert_eq!(selection.format_id, 0xC001);
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
}

#[test]
fn none_when_empty() {
    assert!(select_canonical(&[]).is_none());
}
