use std::time::Duration;

use crate::clipboard::{CaptureOutcome, ClipboardSource};
use crate::clock::Clock;
use crate::config::{Config, BACKOFF_CAP_TOTAL, BACKOFF_START};
use crate::error::CaptureError;

#[derive(Debug, Clone, Copy)]
pub struct BackoffPolicy {
    pub start: Duration,
    pub cap_total: Duration,
}

impl BackoffPolicy {
    pub const fn production() -> Self {
        BackoffPolicy {
            start: BACKOFF_START,
            cap_total: BACKOFF_CAP_TOTAL,
        }
    }
}

impl Default for BackoffPolicy {
    fn default() -> Self {
        Self::production()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RetryOutcome {
    Fatal(CaptureError),
    Exhausted { attempts: u32 },
}

/// Captura com backoff exponencial, sem segurar a área entre as tentativas.
///
/// Cada tentativa passa pela sonda do dono
/// ([`ClipboardSource::try_capture_if_owner_ready`]). Área ocupada e dono OLE
/// que não responde esperam o mesmo backoff. Enquanto o dono não responde, se
/// a área mudou desde a primeira tentativa, a espera cede à notificação nova
/// ([`CaptureOutcome::Superseded`]) sem ler nada. Esgotado o orçamento com o
/// dono ainda calado, faz uma leitura final sem sonda, como antes desta regra:
/// na dúvida, capturar.
pub fn capture_with_retry(
    source: &dyn ClipboardSource,
    clock: &dyn Clock,
    policy: &BackoffPolicy,
    cfg: &Config,
) -> Result<(CaptureOutcome, u32), RetryOutcome> {
    let first_sequence = source.sequence();
    let mut attempts: u32 = 0;
    let mut slept = Duration::ZERO;
    let mut delay = policy.start;

    loop {
        attempts += 1;
        let owner_silent = match source.try_capture_if_owner_ready(cfg) {
            Ok(outcome) => return Ok((outcome, attempts)),
            Err(CaptureError::Busy) => false,
            Err(CaptureError::OwnerNotResponding) => {
                if sequence_changed(first_sequence, source.sequence()) {
                    return Ok((CaptureOutcome::Superseded, attempts));
                }
                true
            }
            Err(other) => return Err(RetryOutcome::Fatal(other)),
        };
        if slept + delay > policy.cap_total {
            if owner_silent {
                return final_read_without_probe(source, cfg, attempts + 1);
            }
            return Err(RetryOutcome::Exhausted { attempts });
        }
        clock.sleep(delay);
        slept += delay;
        delay = delay.saturating_mul(2);
    }
}

fn sequence_changed(first: Option<u32>, now: Option<u32>) -> bool {
    match (first, now) {
        (Some(first), Some(now)) => first != now,
        _ => false,
    }
}

fn final_read_without_probe(
    source: &dyn ClipboardSource,
    cfg: &Config,
    attempts: u32,
) -> Result<(CaptureOutcome, u32), RetryOutcome> {
    match source.try_capture(cfg) {
        Ok(outcome) => Ok((outcome, attempts)),
        Err(CaptureError::Busy) => Err(RetryOutcome::Exhausted { attempts }),
        Err(other) => Err(RetryOutcome::Fatal(other)),
    }
}
