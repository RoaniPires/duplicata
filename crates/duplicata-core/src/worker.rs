use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use tracing::Level;

use crate::canonical::CF_UNICODETEXT;
use crate::capture::CanonicalKind;
use crate::config::Config;
use crate::identity::identity_of;
use crate::log_event;
use crate::preview::build_preview;
use crate::queue::CaptureQueue;
use crate::repository::{CaptureRecord, HistoryRepository, UpsertOutcome};
use crate::retention::cutoff_ms;
use crate::secret_pattern::matches_secret_pattern;
use crate::thumbnail::build_thumbnail;
use crate::work_item::{HeuristicRejectionSignal, WorkItem};
use crate::{LogFields, RawCapture};

#[derive(Debug, Default)]
pub struct WorkerCounters {
    processed: AtomicU64,
}

impl WorkerCounters {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn processed(&self) -> u64 {
        self.processed.load(Ordering::Relaxed)
    }
}

pub fn run_worker(
    queue: &dyn CaptureQueue<WorkItem>,
    repo: &mut dyn HistoryRepository,
    cfg: &Config,
    counters: &WorkerCounters,
    heuristic_signal: &HeuristicRejectionSignal,
) {
    let mut cfg = cfg.clone();
    while let Some(item) = queue.pop() {
        match item {
            WorkItem::Capture(raw) => {
                process_one(&raw, repo, &cfg, heuristic_signal, false);
                counters.processed.fetch_add(1, Ordering::Relaxed);
            }
            WorkItem::RecoverCapture(raw) => {
                process_one(&raw, repo, &cfg, heuristic_signal, true);
                counters.processed.fetch_add(1, Ordering::Relaxed);
            }
            WorkItem::RecreateDatabase { done } => {
                let _ = done.send(repo.recreate());
            }
            WorkItem::ApplySettings {
                now_ms,
                retention_ms,
                max_items,
                max_pinned,
            } => {
                cfg.retention = Duration::from_millis(retention_ms);
                cfg.max_items = max_items;
                cfg.max_pinned = max_pinned;
                run_retention_purges(repo, now_ms, &cfg);
            }
            WorkItem::SetPinned {
                clip_id,
                pinned,
                done,
            } => {
                let _ = done.send(repo.set_pinned(clip_id, pinned, cfg.max_pinned));
            }
            WorkItem::DeleteAll { keep_pinned, done } => {
                let _ = done.send(repo.delete_all(keep_pinned));
            }
            WorkItem::ToggleEncryption {
                enable,
                progress,
                done,
            } => {
                let result = repo.toggle_encryption(enable, &|n| {
                    let _ = progress.send(n);
                });
                let _ = done.send(result);
            }
        }
    }
}

fn run_retention_purges(repo: &mut dyn HistoryRepository, now_ms: u64, cfg: &Config) {
    let mut removed = 0u64;
    let mut failed = false;
    match repo.purge_older_than(cutoff_ms(now_ms, cfg.retention)) {
        Ok(n) => removed += n,
        Err(_) => failed = true,
    }
    match repo.purge_over_count(cfg.max_items) {
        Ok(n) => removed += n,
        Err(_) => failed = true,
    }
    if removed > 0 {
        log_event!(
            Level::DEBUG,
            LogFields::new("retention_purged").byte_len(removed)
        );
    }
    if failed {
        log_event!(Level::WARN, LogFields::new("retention_failed"));
    }
}

fn process_one(
    raw: &RawCapture,
    repo: &mut dyn HistoryRepository,
    cfg: &Config,
    heuristic_signal: &HeuristicRejectionSignal,
    bypass_heuristic: bool,
) {
    let canonical_bytes = raw.canonical_bytes();

    if !bypass_heuristic
        && cfg.heuristic_secret_detection
        && raw.canonical.kind == CanonicalKind::UnicodeText
    {
        if let Some(kind) = matches_secret_pattern(canonical_bytes) {
            log_event!(
                Level::WARN,
                LogFields::new("heuristic_secret_rejected").kind(kind.as_str())
            );
            if heuristic_signal.send(()).is_err() {
                log_event!(Level::WARN, LogFields::new("heuristic_signal_send_failed"));
            }
            return;
        }
    }

    let identity = identity_of(canonical_bytes);
    let preview = build_preview(&raw.canonical, &raw.formats);
    let thumbnail = build_thumbnail(&raw.canonical, &raw.formats);
    if thumbnail.is_none()
        && matches!(
            raw.canonical.kind,
            CanonicalKind::Dib | CanonicalKind::DibV5
        )
    {
        log_event!(
            Level::DEBUG,
            LogFields::new("thumbnail_skipped").kind(raw.canonical.kind.as_str())
        );
    }

    let has_text = raw.formats.iter().any(|f| f.format_id == CF_UNICODETEXT);

    let record = CaptureRecord {
        identity,
        canonical: raw.canonical.clone(),
        captured_at: raw.captured_at,
        total_bytes: raw.total_bytes,
        preview,
        thumbnail,
        has_text,
        formats: raw.formats.clone(),
    };

    let format_ids = || record.formats.iter().map(|f| f.format_id);

    let outcome = repo.upsert(&record);
    match &outcome {
        Ok(UpsertOutcome::Inserted { clip_id }) => {
            log_event!(
                Level::DEBUG,
                LogFields::new("clip_inserted")
                    .clip_id(*clip_id)
                    .format_ids(format_ids())
            );
        }
        Ok(UpsertOutcome::Deduped { clip_id }) => {
            log_event!(
                Level::DEBUG,
                LogFields::new("clip_deduped")
                    .clip_id(*clip_id)
                    .format_ids(format_ids())
            );
        }
        Err(e) => {
            log_event!(
                Level::WARN,
                LogFields::new("db_write_failed")
                    .byte_len(record.total_bytes)
                    .format_ids(format_ids())
            );
            let _ = e;
        }
    }

    if outcome.is_ok() {
        run_retention_purges(repo, raw.captured_at.as_millis(), cfg);
    }
}
