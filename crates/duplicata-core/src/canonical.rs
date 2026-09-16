use crate::capture::{CanonicalKind, CanonicalSelection, CapturedFormat, FormatAnnounce};
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

fn select_index(format_ids: &[u32]) -> Option<usize> {
    if let Some(idx) = format_ids.iter().position(|id| *id == CF_UNICODETEXT) {
        return Some(idx);
    }

    if let Some(idx) = format_ids
        .iter()
        .position(|id| *id == CF_DIBV5 || *id == CF_DIB)
    {
        return Some(idx);
    }

    if let Some(idx) = format_ids.iter().position(|id| *id == CF_HDROP) {
        return Some(idx);
    }

    format_ids.iter().position(|id| *id >= CF_REGISTERED_FIRST)
}

pub fn select_canonical(formats: &[CapturedFormat]) -> Option<CanonicalSelection> {
    let ids: Vec<u32> = formats.iter().map(|f| f.format_id).collect();
    let idx = select_index(&ids)?;
    let f = &formats[idx];
    Some(CanonicalSelection {
        format_id: f.format_id,
        format_name: f.format_name.clone(),
        kind: kind_of(f.format_id),
        byte_len: f.bytes.len() as u64,
    })
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

pub fn screen(formats: &[FormatAnnounce], source_program: Option<&str>, cfg: &Config) -> Decision {
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

    let format_ids: Vec<u32> = formats.iter().map(|f| f.format_id).collect();
    match select_index(&format_ids) {
        Some(canonical_index) => Decision::Copy { canonical_index },
        None => Decision::Reject(RejectReason::NoCanonicalFormat { format_ids }),
    }
}

pub fn gate_size(canonical_format_id: u32, byte_len: u64, cfg: &Config) -> Option<RejectReason> {
    let kind = kind_of(canonical_format_id);
    match check_size(kind, byte_len, cfg) {
        Ok(()) => None,
        Err(CaptureError::TooLarge { byte_len }) => Some(RejectReason::TooLarge { kind, byte_len }),
        Err(_) => unreachable!("check_size só devolve Ok ou TooLarge"),
    }
}
