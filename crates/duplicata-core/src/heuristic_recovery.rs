use crate::backoff::{capture_with_retry, BackoffPolicy, RetryOutcome};
use crate::capture::Timestamp;
use crate::{
    raw_capture_from_copied, CaptureError, CaptureOutcome, CaptureQueue, ClipboardSource, Clock,
    Config, WorkItem,
};

#[derive(Debug)]
pub enum RecoverOutcome {
    Failed(CaptureError),
    Recovered,
}

pub fn recover_on_notification_click(
    source: &dyn ClipboardSource,
    clock: &dyn Clock,
    policy: &BackoffPolicy,
    cfg: &Config,
    queue: &dyn CaptureQueue<WorkItem>,
) -> RecoverOutcome {
    match capture_with_retry(source, clock, policy, cfg) {
        Ok((
            CaptureOutcome::Copied {
                formats,
                canonical_index,
            },
            _attempts,
        )) => {
            let raw = raw_capture_from_copied(
                formats,
                canonical_index,
                Timestamp::from_millis(clock.now_ms()),
            );
            let size = raw.total_bytes as usize;
            queue.push(WorkItem::RecoverCapture(raw), size);
            RecoverOutcome::Recovered
        }
        Ok((CaptureOutcome::Rejected(_), _)) => RecoverOutcome::Recovered,
        Ok((CaptureOutcome::Empty, _)) => RecoverOutcome::Recovered,
        Err(RetryOutcome::Exhausted { .. }) => RecoverOutcome::Failed(CaptureError::Busy),
        Err(RetryOutcome::Fatal(e)) => RecoverOutcome::Failed(e),
    }
}
