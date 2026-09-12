use crate::canonical::{CF_HDROP, CF_UNICODETEXT};
use crate::capture::{CanonicalKind, CanonicalSelection, CapturedFormat};

pub const PREVIEW_MAX_CHARS: usize = 400;

pub fn build_preview(canonical: &CanonicalSelection, formats: &[CapturedFormat]) -> Option<String> {
    match canonical.kind {
        CanonicalKind::UnicodeText => {
            let bytes = format_bytes(formats, CF_UNICODETEXT)?;
            let text = decode_utf16le_nul_terminated(bytes);
            Some(truncate_chars(&text, PREVIEW_MAX_CHARS))
        }
        CanonicalKind::HDrop => {
            let bytes = format_bytes(formats, CF_HDROP)?;
            let names = parse_hdrop(bytes);
            Some(names.join("\n"))
        }
        CanonicalKind::Dib | CanonicalKind::DibV5 | CanonicalKind::Custom => None,
    }
}

fn format_bytes(formats: &[CapturedFormat], id: u32) -> Option<&[u8]> {
    formats
        .iter()
        .find(|f| f.format_id == id)
        .map(|f| f.bytes.as_slice())
}

fn truncate_chars(s: &str, max: usize) -> String {
    match s.char_indices().nth(max) {
        Some((idx, _)) => s[..idx].to_string(),
        None => s.to_string(),
    }
}

pub(crate) fn decode_utf16le_nul_terminated(bytes: &[u8]) -> String {
    let units: Vec<u16> = bytes
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .take_while(|&u| u != 0)
        .collect();
    String::from_utf16_lossy(&units)
}

pub fn parse_hdrop(bytes: &[u8]) -> Vec<String> {
    if bytes.len() < 20 {
        return Vec::new();
    }
    let p_files = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]) as usize;
    let f_wide = i32::from_le_bytes([bytes[16], bytes[17], bytes[18], bytes[19]]) != 0;
    if p_files > bytes.len() {
        return Vec::new();
    }
    let list = &bytes[p_files..];

    if f_wide {
        split_wide_double_nul(list)
    } else {
        split_ansi_double_nul(list)
    }
}

fn split_wide_double_nul(list: &[u8]) -> Vec<String> {
    let units: Vec<u16> = list
        .chunks_exact(2)
        .map(|c| u16::from_le_bytes([c[0], c[1]]))
        .collect();
    let mut out = Vec::new();
    let mut cur: Vec<u16> = Vec::new();
    for u in units {
        if u == 0 {
            if cur.is_empty() {
                break;
            }
            out.push(String::from_utf16_lossy(&cur));
            cur.clear();
        } else {
            cur.push(u);
        }
    }
    out
}

fn split_ansi_double_nul(list: &[u8]) -> Vec<String> {
    let mut out = Vec::new();
    let mut cur: Vec<u8> = Vec::new();
    for &b in list {
        if b == 0 {
            if cur.is_empty() {
                break;
            }
            out.push(String::from_utf8_lossy(&cur).into_owned());
            cur.clear();
        } else {
            cur.push(b);
        }
    }
    out
}
