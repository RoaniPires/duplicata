use std::sync::Mutex;

use crate::canonical::{decide, RejectReason};
use crate::capture::{CapturedFormat, FormatInfo};
use crate::config::Config;
use crate::error::CaptureError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    Copied {
        formats: Vec<CapturedFormat>,
        canonical_index: usize,
    },
    Empty,
    Rejected(RejectReason),
}

pub trait ClipboardSource: Send {
    fn try_capture(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError>;
}

#[derive(Debug)]
pub struct FakeClipboardSource {
    state: Mutex<FakeState>,
}

#[derive(Debug)]
enum FakeOutcome {
    Formats(Vec<CapturedFormat>),
    Fatal(CaptureError),
}

#[derive(Debug)]
struct FakeState {
    busy_remaining: u32,
    outcome: FakeOutcome,
    calls: u32,
    copied_format_ids: Vec<u32>,
    source_program: Option<String>,
}

impl FakeClipboardSource {
    pub fn always(formats: Vec<CapturedFormat>) -> Self {
        Self::new(0, FakeOutcome::Formats(formats))
    }

    pub fn busy_then(n: u32, formats: Vec<CapturedFormat>) -> Self {
        Self::new(n, FakeOutcome::Formats(formats))
    }

    pub fn busy_then_err(n: u32, err: CaptureError) -> Self {
        Self::new(n, FakeOutcome::Fatal(err))
    }

    pub fn failing(err: CaptureError) -> Self {
        Self::new(0, FakeOutcome::Fatal(err))
    }

    fn new(busy_remaining: u32, outcome: FakeOutcome) -> Self {
        FakeClipboardSource {
            state: Mutex::new(FakeState {
                busy_remaining,
                outcome,
                calls: 0,
                copied_format_ids: Vec::new(),
                source_program: None,
            }),
        }
    }

    pub fn set_source_program(&self, name: Option<&str>) {
        self.state.lock().unwrap().source_program = name.map(String::from);
    }

    pub fn calls(&self) -> u32 {
        self.state.lock().unwrap().calls
    }

    pub fn copied_format_ids(&self) -> Vec<u32> {
        self.state.lock().unwrap().copied_format_ids.clone()
    }
}

impl ClipboardSource for FakeClipboardSource {
    fn try_capture(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
        let mut s = self.state.lock().unwrap();
        s.calls += 1;
        if s.busy_remaining > 0 {
            s.busy_remaining -= 1;
            return Err(CaptureError::Busy);
        }

        match &s.outcome {
            FakeOutcome::Fatal(e) => Err(e.clone()),
            FakeOutcome::Formats(formats) => {
                if formats.is_empty() {
                    s.copied_format_ids.clear();
                    return Ok(CaptureOutcome::Empty);
                }
                let infos: Vec<FormatInfo> = formats
                    .iter()
                    .map(|f| FormatInfo {
                        format_id: f.format_id,
                        format_name: f.format_name.clone(),
                        byte_len: f.bytes.len() as u64,
                    })
                    .collect();
                let source_program = s.source_program.clone();
                match decide(&infos, source_program.as_deref(), cfg) {
                    crate::canonical::Decision::Copy { canonical_index } => {
                        let formats = formats.clone();
                        s.copied_format_ids = formats.iter().map(|f| f.format_id).collect();
                        Ok(CaptureOutcome::Copied {
                            formats,
                            canonical_index,
                        })
                    }
                    crate::canonical::Decision::Reject(reason) => {
                        s.copied_format_ids.clear();
                        Ok(CaptureOutcome::Rejected(reason))
                    }
                }
            }
        }
    }
}
