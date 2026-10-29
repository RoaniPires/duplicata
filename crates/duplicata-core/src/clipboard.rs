use std::collections::VecDeque;
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
    /// A área mudou enquanto o dono não respondia: nada foi lido, e a
    /// notificação da mudança trata o conteúdo novo.
    Superseded,
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
    /// Abre a área e captura, sem perguntar nada ao dono antes.
    fn try_capture(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError>;

    /// Como [`ClipboardSource::try_capture`], mas antes de abrir a área recusa
    /// o estado só de controle do OLE e, se o dono é o OLE, só abre se ele
    /// responder ([`CaptureError::OwnerNotResponding`] se não).
    fn try_capture_if_owner_ready(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
        self.try_capture(cfg)
    }

    /// Número de sequência da área de transferência, se a fonte o conhece.
    fn sequence(&self) -> Option<u32> {
        None
    }
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
    /// Falhas que só a captura com sonda vê, na ordem, antes de capturar.
    probe_script: VecDeque<CaptureError>,
    owner_never_responds: bool,
    probe_calls: u32,
    unprobed_reads: u32,
    /// Valores de [`ClipboardSource::sequence`] em ordem; o último se repete.
    sequences: VecDeque<u32>,
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
                probe_script: VecDeque::new(),
                owner_never_responds: false,
                probe_calls: 0,
                unprobed_reads: 0,
                sequences: VecDeque::new(),
                calls: 0,
                copied_format_ids: Vec::new(),
                source_program: None,
            }),
        }
    }

    /// O dono não responde às `n` primeiras sondas; depois captura.
    pub fn owner_not_responding_then(n: u32, formats: Vec<CapturedFormat>) -> Self {
        Self::probe_script_then(
            (0..n).map(|_| CaptureError::OwnerNotResponding).collect(),
            formats,
        )
    }

    /// O dono nunca responde à sonda; a leitura sem sonda captura.
    pub fn owner_never_responds(formats: Vec<CapturedFormat>) -> Self {
        Self::always(formats).with_owner_never_responding()
    }

    /// Este fake passa a ter um dono que nunca responde à sonda; a leitura
    /// sem sonda segue o roteiro de antes (ocupada, erro ou formatos).
    #[must_use]
    pub fn with_owner_never_responding(self) -> Self {
        self.state.lock().unwrap().owner_never_responds = true;
        self
    }

    /// As sondas falham com `failures`, na ordem; depois captura.
    pub fn probe_script_then(failures: Vec<CaptureError>, formats: Vec<CapturedFormat>) -> Self {
        let fake = Self::always(formats);
        fake.state.lock().unwrap().probe_script = failures.into();
        fake
    }

    /// Valores de `sequence()` em ordem; o último se repete. Vazio: `None`.
    pub fn set_sequences(&self, values: &[u32]) {
        self.state.lock().unwrap().sequences = values.iter().copied().collect();
    }

    /// Chamadas a `try_capture_if_owner_ready`.
    pub fn probe_calls(&self) -> u32 {
        self.state.lock().unwrap().probe_calls
    }

    /// Chamadas diretas a `try_capture` (leitura sem sonda).
    pub fn unprobed_reads(&self) -> u32 {
        self.state.lock().unwrap().unprobed_reads
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
        self.state.lock().unwrap().unprobed_reads += 1;
        self.capture(cfg)
    }

    fn try_capture_if_owner_ready(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
        {
            let mut s = self.state.lock().unwrap();
            s.probe_calls += 1;
            if let Some(failure) = s.probe_script.pop_front() {
                return Err(failure);
            }
            if s.owner_never_responds {
                return Err(CaptureError::OwnerNotResponding);
            }
        }
        self.capture(cfg)
    }

    fn sequence(&self) -> Option<u32> {
        let mut s = self.state.lock().unwrap();
        if s.sequences.len() > 1 {
            s.sequences.pop_front()
        } else {
            s.sequences.front().copied()
        }
    }
}

impl FakeClipboardSource {
    fn capture(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
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
