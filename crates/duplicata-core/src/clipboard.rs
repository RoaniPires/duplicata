use std::sync::Mutex;

use tracing::Level;

use crate::canonical::{gate_size, screen, Decision, RejectReason};
use crate::capture::{CapturedFormat, FormatAnnounce};
use crate::config::Config;
use crate::error::CaptureError;
use crate::log_event;
use crate::log_fields::LogFields;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureOutcome {
    Copied {
        formats: Vec<CapturedFormat>,
        canonical_index: usize,
    },
    Empty,
    Rejected(RejectReason),
}

pub fn copied_or_empty(formats: Vec<CapturedFormat>, canonical_format_id: u32) -> CaptureOutcome {
    match formats
        .iter()
        .position(|f| f.format_id == canonical_format_id)
    {
        Some(canonical_index) => CaptureOutcome::Copied {
            formats,
            canonical_index,
        },
        None => {
            log_event!(
                Level::WARN,
                LogFields::new("canonical_format_unavailable").format_ids([canonical_format_id])
            );
            CaptureOutcome::Empty
        }
    }
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
                let source_program = s.source_program.clone();
                let outcome = fake_capture(formats, source_program.as_deref(), cfg);
                s.copied_format_ids = match &outcome {
                    CaptureOutcome::Copied { formats, .. } => {
                        formats.iter().map(|f| f.format_id).collect()
                    }
                    _ => Vec::new(),
                };
                Ok(outcome)
            }
        }
    }
}

fn fake_capture(
    formats: &[CapturedFormat],
    source_program: Option<&str>,
    cfg: &Config,
) -> CaptureOutcome {
    if formats.is_empty() {
        return CaptureOutcome::Empty;
    }

    let announces: Vec<FormatAnnounce> = formats
        .iter()
        .map(|f| FormatAnnounce {
            format_id: f.format_id,
            format_name: f.format_name.clone(),
        })
        .collect();

    let canonical_index = match screen(&announces, source_program, cfg) {
        Decision::Reject(reason) => return CaptureOutcome::Rejected(reason),
        Decision::Copy { canonical_index } => canonical_index,
    };

    let canonical = &formats[canonical_index];
    if let Some(reason) = gate_size(canonical.format_id, canonical.bytes.len() as u64, cfg) {
        return CaptureOutcome::Rejected(reason);
    }

    copied_or_empty(formats.to_vec(), canonical.format_id)
}
