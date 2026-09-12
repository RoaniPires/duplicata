use tracing::Level;

use crate::backoff::{capture_with_retry, BackoffPolicy, RetryOutcome};
use crate::canonical::{kind_of, RejectReason};
use crate::capture::{CanonicalSelection, CapturedFormat, Timestamp};
use crate::clipboard::{CaptureOutcome, ClipboardSource};
use crate::clock::Clock;
use crate::config::Config;
use crate::error::CaptureError;
use crate::identity::identity_of;
use crate::log_event;
use crate::queue::CaptureQueue;
use crate::recent_capture_guard::RecentCaptureGuard;
use crate::work_item::WorkItem;
use crate::{LogFields, RawCapture};

pub fn raw_capture_from_copied(
    formats: Vec<CapturedFormat>,
    canonical_index: usize,
    captured_at: Timestamp,
) -> RawCapture {
    let f = &formats[canonical_index];
    let canonical = CanonicalSelection {
        format_id: f.format_id,
        format_name: f.format_name.clone(),
        kind: kind_of(f.format_id),
        byte_len: f.bytes.len() as u64,
    };
    RawCapture::new(formats, canonical, captured_at)
}

pub fn capture_and_enqueue(
    source: &dyn ClipboardSource,
    clock: &dyn Clock,
    policy: &BackoffPolicy,
    config: &Config,
    queue: &dyn CaptureQueue<WorkItem>,
    recent_capture_guard: &RecentCaptureGuard,
) {
    match capture_with_retry(source, clock, policy, config) {
        Ok((
            CaptureOutcome::Copied {
                formats,
                canonical_index,
            },
            _attempts,
        )) => {
            let now_ms = clock.now_ms();
            let raw =
                raw_capture_from_copied(formats, canonical_index, Timestamp::from_millis(now_ms));

            let identity = identity_of(raw.canonical_bytes());
            if recent_capture_guard.is_duplicate(identity, now_ms) {
                log_event!(
                    Level::DEBUG,
                    LogFields::new("duplicate_notification_suppressed")
                );
                return;
            }

            let size = raw.total_bytes as usize;
            let outcome = queue.push(WorkItem::Capture(raw), size);
            for discarded in outcome.evicted_bytes {
                log_event!(
                    Level::WARN,
                    LogFields::new("queue_evicted").byte_len(discarded as u64)
                );
            }
        }
        Ok((CaptureOutcome::Empty, _)) => {}
        Ok((CaptureOutcome::Rejected(RejectReason::SensitiveFlagged), _)) => {
            log_event!(Level::WARN, LogFields::new("sensitive_flagged"));
        }
        Ok((CaptureOutcome::Rejected(RejectReason::BlockedProgram), _)) => {
            log_event!(Level::WARN, LogFields::new("blocked_program"));
        }
        Ok((CaptureOutcome::Rejected(RejectReason::NoCanonicalFormat { format_ids }), _)) => {
            log_event!(
                Level::WARN,
                LogFields::new("no_canonical_format").format_ids(format_ids)
            );
        }
        Ok((CaptureOutcome::Rejected(RejectReason::TooLarge { kind, byte_len }), _)) => {
            log_event!(
                Level::WARN,
                LogFields::new("capture_too_large")
                    .kind(kind.as_str())
                    .byte_len(byte_len)
            );
        }
        Err(RetryOutcome::Exhausted { attempts }) => {
            log_event!(
                Level::WARN,
                LogFields::new("clipboard_busy").attempts(attempts)
            );
        }
        Err(RetryOutcome::Fatal(CaptureError::Empty)) => {}
        Err(RetryOutcome::Fatal(_)) => {
            log_event!(Level::WARN, LogFields::new("clipboard_unavailable"));
        }
    }
}
