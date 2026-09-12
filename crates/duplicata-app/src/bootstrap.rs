use std::fs;
use std::path::PathBuf;
use std::sync::Arc;

use duplicata_core::{
    cutoff_ms, log_event, ByteBudgetQueue, Config, HistoryRepository, InitError, LogFields,
    WorkItem, WorkerCounters,
};
use duplicata_store::SqliteHistoryRepository;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;

use crate::logging;

#[derive(Debug, Clone)]
pub struct Paths {
    pub data_dir: PathBuf,
    pub db_path: PathBuf,
    pub log_dir: PathBuf,
}

pub trait InitErrorReporter {
    fn report(&self, err: &InitError);
}

pub struct Services {
    pub queue: Arc<ByteBudgetQueue<WorkItem>>,
    pub repo: SqliteHistoryRepository,
    pub counters: Arc<WorkerCounters>,
    pub config: Config,
    pub log_guard: Option<WorkerGuard>,
}

pub fn init(
    paths: &Paths,
    now_ms: u64,
    enable_logging: bool,
    reporter: &dyn InitErrorReporter,
) -> Result<Services, InitError> {
    fs::create_dir_all(&paths.data_dir).map_err(|_| fail(reporter, InitError::CreateDir))?;
    fs::create_dir_all(&paths.log_dir).map_err(|_| fail(reporter, InitError::CreateDir))?;

    let log_guard = if enable_logging {
        Some(logging::init(&paths.log_dir).map_err(|e| fail(reporter, e))?)
    } else {
        None
    };

    #[cfg(windows)]
    let effective_db_path = duplicata_store::crypto::reconcile_on_startup(&paths.db_path, &|_| {})
        .map_err(|e| fail(reporter, InitError::OpenDb(e)))?;
    #[cfg(not(windows))]
    let effective_db_path = paths.db_path.clone();

    let conn = duplicata_store::open(&effective_db_path)
        .map_err(|e| fail(reporter, InitError::OpenDb(e)))?;
    let mut repo = SqliteHistoryRepository::new_at(conn, paths.db_path.clone());

    let config_path = paths.data_dir.join("config.toml");
    let (mut config, fallbacks) =
        Config::load(paths.db_path.clone(), paths.log_dir.clone(), &config_path);
    config.encryption_enabled = effective_db_path != paths.db_path;
    for fb in &fallbacks {
        if let Some((requested, applied)) = fb.floor {
            log_event!(
                Level::WARN,
                LogFields::new("config_floor_applied")
                    .kind(fb.field)
                    .requested(requested)
                    .applied(applied)
            );
        } else if let Some((requested, applied)) = fb.ceiling {
            log_event!(
                Level::WARN,
                LogFields::new("config_ceiling_applied")
                    .kind(fb.field)
                    .requested(requested)
                    .applied(applied)
            );
        } else {
            log_event!(
                Level::WARN,
                LogFields::new("config_fallback").kind(fb.field)
            );
        }
    }

    let purged_by_age = match repo.purge_older_than(cutoff_ms(now_ms, config.retention)) {
        Ok(n) => n,
        Err(_) => {
            log_event!(Level::WARN, LogFields::new("retention_failed"));
            0
        }
    };
    let purged_by_count = match repo.purge_over_count(config.max_items) {
        Ok(n) => n,
        Err(_) => {
            log_event!(Level::WARN, LogFields::new("retention_failed"));
            0
        }
    };
    let purged = purged_by_age + purged_by_count;
    if purged > 0 {
        log_event!(
            Level::DEBUG,
            LogFields::new("retention_purged").byte_len(purged)
        );
    }

    Ok(Services {
        queue: Arc::new(ByteBudgetQueue::new()),
        repo,
        counters: Arc::new(WorkerCounters::new()),
        config,
        log_guard,
    })
}

fn fail(reporter: &dyn InitErrorReporter, err: InitError) -> InitError {
    reporter.report(&err);
    err
}
