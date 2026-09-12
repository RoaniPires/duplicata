use crate::capture::{CanonicalKind, CanonicalSelection, CapturedFormat, FormatInfo};
use crate::config::Config;
use crate::error::CaptureError;
use crate::size_limit::check_size;

pub const CF_TEXT: u32 = 1;
pub const CF_BITMAP: u32 = 2;
pub const CF_OEMTEXT: u32 = 7;
pub const CF_DIB: u32 = 8;
pub const CF_UNICODETEXT: u32 = 13;
pub const CF_LOCALE: u32 = 16;
pub const CF_HDROP: u32 = 15;
pub const CF_DIBV5: u32 = 17;
pub const CF_REGISTERED_FIRST: u32 = 0xC000;

pub const SENSITIVE_FORMAT_NAME: &str = "ExcludeClipboardContentFromMonitorProcessing";

pub const fn kind_of(format_id: u32) -> CanonicalKind {
    match format_id {
        CF_UNICODETEXT => CanonicalKind::UnicodeText,
        CF_DIB => CanonicalKind::Dib,
        CF_DIBV5 => CanonicalKind::DibV5,
        CF_HDROP => CanonicalKind::HDrop,
        _ => CanonicalKind::Custom,
    }
}

fn select_index(formats: &[(u32, u64)]) -> Option<usize> {
    if let Some(idx) = formats.iter().position(|(id, _)| *id == CF_UNICODETEXT) {
        return Some(idx);
    }

    let dib_pos = formats.iter().position(|(id, _)| *id == CF_DIB);
    let dibv5_pos = formats.iter().position(|(id, _)| *id == CF_DIBV5);
    match (dibv5_pos, dib_pos) {
        (Some(v5), Some(d)) => return Some(if v5 <= d { v5 } else { d }),
        (Some(v5), None) => return Some(v5),
        (None, Some(d)) => return Some(d),
        (None, None) => {}
    }

    if let Some(idx) = formats.iter().position(|(id, _)| *id == CF_HDROP) {
        return Some(idx);
    }

    formats
        .iter()
        .enumerate()
        .filter(|(_, (id, _))| *id >= CF_REGISTERED_FIRST)
        .max_by(|(_, (id_a, size_a)), (_, (id_b, size_b))| size_a.cmp(size_b).then(id_b.cmp(id_a)))
        .map(|(idx, _)| idx)
}

pub fn select_canonical(formats: &[CapturedFormat]) -> Option<CanonicalSelection> {
    let keys: Vec<(u32, u64)> = formats
        .iter()
        .map(|f| (f.format_id, f.bytes.len() as u64))
        .collect();
    let idx = select_index(&keys)?;
    let f = &formats[idx];
    Some(CanonicalSelection {
        format_id: f.format_id,
        format_name: f.format_name.clone(),
        kind: kind_of(f.format_id),
        byte_len: f.bytes.len() as u64,
    })
}

pub fn select_canonical_from_info(formats: &[FormatInfo]) -> Option<(usize, CanonicalSelection)> {
    let keys: Vec<(u32, u64)> = formats.iter().map(|f| (f.format_id, f.byte_len)).collect();
    let idx = select_index(&keys)?;
    let f = &formats[idx];
    let selection = CanonicalSelection {
        format_id: f.format_id,
        format_name: f.format_name.clone(),
        kind: kind_of(f.format_id),
        byte_len: f.byte_len,
    };
    Some((idx, selection))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RejectReason {
    SensitiveFlagged,
    BlockedProgram,
    NoCanonicalFormat { format_ids: Vec<u32> },
    TooLarge { kind: CanonicalKind, byte_len: u64 },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Copy { canonical_index: usize },
    Reject(RejectReason),
}

pub fn decide(formats: &[FormatInfo], source_program: Option<&str>, cfg: &Config) -> Decision {
    let sensitive_flagged = formats
        .iter()
        .any(|f| f.format_name.as_deref() == Some(SENSITIVE_FORMAT_NAME));
    if sensitive_flagged {
        return Decision::Reject(RejectReason::SensitiveFlagged);
    }

    let blocked = source_program.is_some_and(|name| {
        cfg.blocked_programs
            .iter()
            .any(|blocked| blocked.eq_ignore_ascii_case(name))
    });
    if blocked {
        return Decision::Reject(RejectReason::BlockedProgram);
    }

    match select_canonical_from_info(formats) {
        None => Decision::Reject(RejectReason::NoCanonicalFormat {
            format_ids: formats.iter().map(|f| f.format_id).collect(),
        }),
        Some((canonical_index, selection)) => {
            match check_size(selection.kind, selection.byte_len, cfg) {
                Ok(()) => Decision::Copy { canonical_index },
                Err(CaptureError::TooLarge { byte_len }) => {
                    Decision::Reject(RejectReason::TooLarge {
                        kind: selection.kind,
                        byte_len,
                    })
                }
                Err(_) => unreachable!("check_size só devolve Ok ou TooLarge"),
            }
        }
    }
}
