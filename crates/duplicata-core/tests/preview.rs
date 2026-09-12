use duplicata_core::canonical::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::preview::{build_preview, parse_hdrop, PREVIEW_MAX_CHARS};
use duplicata_core::{utf16le, CapturedFormat};

fn canonical(id: u32, kind: CanonicalKind, len: u64) -> CanonicalSelection {
    CanonicalSelection {
        format_id: id,
        format_name: None,
        kind,
        byte_len: len,
    }
}

#[test]
fn text_preview_is_the_first_chars_of_unicodetext() {
    let text = "conteúdo copiado";
    let fmt = CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: utf16le(text),
    };
    let c = canonical(
        CF_UNICODETEXT,
        CanonicalKind::UnicodeText,
        fmt.bytes.len() as u64,
    );
    assert_eq!(build_preview(&c, &[fmt]).as_deref(), Some(text));
}

#[test]
fn text_preview_is_truncated_to_the_max() {
    let long = "a".repeat(1000);
    let fmt = CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: utf16le(&long),
    };
    let c = canonical(
        CF_UNICODETEXT,
        CanonicalKind::UnicodeText,
        fmt.bytes.len() as u64,
    );
    let p = build_preview(&c, &[fmt]).unwrap();
    assert_eq!(p.chars().count(), PREVIEW_MAX_CHARS);
}

#[test]
fn text_preview_max_is_400_for_new_captures() {
    assert_eq!(PREVIEW_MAX_CHARS, 400);

    let long = format!("{}CENOURA{}", "a".repeat(210), "b".repeat(500));
    let fmt = CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: utf16le(&long),
    };
    let c = canonical(
        CF_UNICODETEXT,
        CanonicalKind::UnicodeText,
        fmt.bytes.len() as u64,
    );
    let p = build_preview(&c, &[fmt]).unwrap();
    assert_eq!(p.chars().count(), 400);
    assert!(
        p.contains("CENOURA"),
        "termo no char ~210 entra no preview de 400; ficaria de fora no de 200"
    );
}

#[test]
fn image_has_no_preview() {
    let fmt = CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: vec![0u8; 128],
    };
    let c = canonical(CF_DIB, CanonicalKind::Dib, 128);
    assert_eq!(build_preview(&c, &[fmt]), None);
}

#[test]
fn hdrop_preview_lists_file_names() {
    let blob = dropfiles_wide(&["C:\\a\\um.txt", "C:\\a\\dois.txt"]);
    let fmt = CapturedFormat {
        format_id: CF_HDROP,
        format_name: None,
        bytes: blob.clone(),
    };
    let c = canonical(CF_HDROP, CanonicalKind::HDrop, blob.len() as u64);
    let p = build_preview(&c, &[fmt]).unwrap();
    assert!(p.contains("um.txt"));
    assert!(p.contains("dois.txt"));
}

#[test]
fn parse_hdrop_reads_the_wide_path_list() {
    let blob = dropfiles_wide(&["X:\\foo", "X:\\bar", "X:\\baz"]);
    assert_eq!(parse_hdrop(&blob), vec!["X:\\foo", "X:\\bar", "X:\\baz"]);
}

#[test]
fn parse_hdrop_handles_empty_list() {
    let blob = dropfiles_wide(&[]);
    assert!(parse_hdrop(&blob).is_empty());
}

#[test]
fn parse_hdrop_reads_the_ansi_path_list() {
    let mut out = Vec::new();
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(b"C:\\a\0C:\\b\0\0");
    assert_eq!(parse_hdrop(&out), vec!["C:\\a", "C:\\b"]);
}

#[test]
fn parse_hdrop_rejects_garbage() {
    assert!(parse_hdrop(&[]).is_empty());
    assert!(parse_hdrop(b"curto").is_empty());
    let mut bad = vec![0xFFu8, 0xFF, 0xFF, 0xFF];
    bad.extend_from_slice(&[0u8; 16]);
    assert!(parse_hdrop(&bad).is_empty());
}

fn dropfiles_wide(paths: &[&str]) -> Vec<u8> {
    let header_len = 20u32;
    let mut out = Vec::new();
    out.extend_from_slice(&header_len.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&1i32.to_le_bytes());
    for p in paths {
        for u in p.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    out
}
