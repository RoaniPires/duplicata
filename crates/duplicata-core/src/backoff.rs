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

pub fn capture_with_retry(
    source: &dyn ClipboardSource,
    clock: &dyn Clock,
    policy: &BackoffPolicy,
    cfg: &Config,
) -> Result<(CaptureOutcome, u32), RetryOutcome> {
    let mut attempts: u32 = 0;
    let mut slept = Duration::ZERO;
    let mut delay = policy.start;

    loop {
        attempts += 1;
        match source.try_capture(cfg) {
            Ok(outcome) => return Ok((outcome, attempts)),
            Err(CaptureError::Busy) => {
                if slept + delay > policy.cap_total {
                    return Err(RetryOutcome::Exhausted { attempts });
                }
                clock.sleep(delay);
                slept += delay;
                delay = delay.saturating_mul(2);
            }
            Err(other) => return Err(RetryOutcome::Fatal(other)),
        }
    }
}
