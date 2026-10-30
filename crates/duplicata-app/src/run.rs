use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use duplicata_core::{
    log_event, run_worker, CaptureQueue, HotkeyCombo, InitError, LogFields, SelfWriteFilter,
};
use duplicata_win::disabled_hotkeys::RegistryDisabledHotkeys;
use duplicata_win::explorer_instance::ShellProcess;
use duplicata_win::history_window::HistoryWindow;
use duplicata_win::listener::ClipboardListener;
use duplicata_win::message_loop::SESSION_ENDING;
use duplicata_win::win_v_runtime::WinVRuntime;
use duplicata_win::{
    dialog, hotkey_dialog, message_loop, paths, settings_dialog, win_v_dialog, Tray,
};
use tracing::Level;

use crate::bootstrap::{self, InitErrorReporter, Paths};

const SESSION_ENDING_JOIN_TIMEOUT: Duration = Duration::from_secs(1);

struct DialogReporter;

impl InitErrorReporter for DialogReporter {
    fn report(&self, err: &InitError) {
        dialog::show_init_error(&err.to_string());
    }
}

pub fn run() -> i32 {
    duplicata_win::dpi::set_process_dpi_awareness_v2();

    let buffered_paint = duplicata_win::window_material::buffered_paint_init();

    let reporter = DialogReporter;

    let resolved = match resolve_paths() {
        Ok(p) => p,
        Err(e) => {
            reporter.report(&e);
            return 1;
        }
    };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);

    let services = match bootstrap::init(&resolved, now_ms, true, &reporter) {
        Ok(s) => s,
        Err(_) => return 1,
    };

    let queue = Arc::clone(&services.queue);
    let counters = Arc::clone(&services.counters);
    let worker_config = services.config.clone();
    let mut repo = services.repo;
    let _log_guard = services.log_guard;

    let (heuristic_tx, heuristic_rx) = mpsc::channel();

    let worker_queue = Arc::clone(&queue);
    let worker = thread::spawn(move || {
        run_worker(
            worker_queue.as_ref() as &dyn CaptureQueue<_>,
            &mut repo,
            &worker_config,
            counters.as_ref(),
            &heuristic_tx,
        );
        repo
    });

    // A mesma configuração das Configurações: o Aplicar/Salvar muda o que o
    // listener segue sem reiniciar.
    let shared_config = Rc::new(RefCell::new(services.config.clone()));
    let self_write_filter = Rc::new(SelfWriteFilter::new());
    let listener = match ClipboardListener::with_shared_config(
        Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>,
        Rc::clone(&shared_config),
        Rc::clone(&self_write_filter),
    ) {
        Ok(l) => l,
        Err(e) => {
            reporter.report(&e);
            queue.close();
            let _ = worker.join();
            return 1;
        }
    };

    let db_for_read = resolved.db_path.clone();
    let reopen_reader =
        move || -> Result<Box<dyn duplicata_core::HistoryReader>, duplicata_core::StoreError> {
            let path = duplicata_store::crypto::effective_read_path(&db_for_read);
            duplicata_store::SqliteHistoryReader::open(&path)
                .map(|r| Box::new(r) as Box<dyn duplicata_core::HistoryReader>)
        };
    let reader = match duplicata_store::SqliteHistoryReader::open(
        &duplicata_store::crypto::effective_read_path(&resolved.db_path),
    ) {
        Ok(r) => r,
        Err(e) => {
            reporter.report(&InitError::from(e));
            drop(listener);
            queue.close();
            let _ = worker.join();
            return 1;
        }
    };
    let db_for_probe = resolved.db_path.clone();
    let recreate_is_safe =
        move || -> bool { duplicata_store::crypto::recreate_is_safe_to_offer(&db_for_probe) };
    let history_window = match HistoryWindow::new(
        reader,
        reopen_reader,
        recreate_is_safe,
        Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>,
        resolved.db_path.clone(),
        Rc::clone(&self_write_filter),
    ) {
        Ok(w) => w,
        Err(e) => {
            reporter.report(&e);
            drop(listener);
            queue.close();
            let _ = worker.join();
            return 1;
        }
    };
    history_window.set_max_pinned(services.config.max_pinned);
    let config_path = resolved.data_dir.join("config.toml");
    let registered_hotkey: Rc<Cell<Option<HotkeyCombo>>> = Rc::new(Cell::new(None));

    // O Win+V ou a reserva: quem decide é o núcleo (`win_v::startup`).
    let win_v = WinVRuntime::new(
        history_window.hwnd(),
        Rc::clone(&shared_config),
        config_path,
        Rc::clone(&registered_hotkey),
        Box::new(RegistryDisabledHotkeys::explorer()),
        Box::new(ShellProcess),
    );
    let startup = win_v.startup();
    if startup.reserve_failed {
        log_event!(Level::WARN, LogFields::new("hotkey_register_failed"));
    }
    if startup.show_failure_window {
        // A janela de conflito da reserva, se for o caso, abre quando esta
        // fechar.
        win_v_dialog::open(Rc::clone(&win_v));
    } else if startup.reserve_failed {
        hotkey_dialog::open_after_conflict(
            history_window.hwnd(),
            win_v.reserve(),
            Rc::clone(&shared_config),
            win_v.config_path().clone(),
            Rc::clone(&registered_hotkey),
        );
    }

    let tray = match Tray::create(
        Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>,
        heuristic_rx,
        Rc::clone(&win_v),
        env!("CARGO_PKG_VERSION"),
    ) {
        Ok(t) => t,
        Err(e) => {
            reporter.report(&InitError::Tray);
            let _ = e;
            win_v.release_all();
            drop(history_window);
            drop(listener);
            queue.close();
            let _ = worker.join();
            return 1;
        }
    };

    message_loop::run();
    // Sair com as Configurações abertas equivale a Cancelar: antes de soltar
    // o atalho e destruir as janelas.
    settings_dialog::cancel_if_open();

    drop(listener);
    win_v.release_all();
    drop(history_window);
    tray.destroy();
    queue.close();
    if buffered_paint {
        duplicata_win::window_material::buffered_paint_uninit();
    }

    let repo = if SESSION_ENDING.load(Ordering::SeqCst) {
        join_worker_with_timeout(worker, SESSION_ENDING_JOIN_TIMEOUT)
    } else {
        worker.join().ok()
    };
    if let Some(repo) = repo {
        if let Err(e) = duplicata_store::checkpoint_truncate(repo.connection()) {
            log_event!(Level::WARN, LogFields::new("wal_checkpoint_failed"));
            let _ = e;
        }
        drop(repo);

        #[cfg(windows)]
        shutdown_reseal(&resolved.db_path);
    }
    0
}

pub fn join_worker_with_timeout<T: Send + 'static>(
    worker: thread::JoinHandle<T>,
    timeout: Duration,
) -> Option<T> {
    let (done_tx, done_rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = done_tx.send(worker.join());
    });

    match done_rx.recv_timeout(timeout) {
        Ok(joined) => joined.ok(),
        Err(_) => {
            log_event!(Level::WARN, LogFields::new("worker_join_timeout"));
            None
        }
    }
}

#[cfg(windows)]
pub fn shutdown_reseal(db_path: &std::path::Path) {
    if SESSION_ENDING.load(Ordering::SeqCst) {
        log_event!(
            Level::DEBUG,
            LogFields::new("shutdown_reseal_deferred_session_ending")
        );
        return;
    }
    if let Err(e) = duplicata_store::crypto::reseal_work_copy_if_present(db_path, &|_| {}) {
        log_event!(Level::WARN, LogFields::new("shutdown_reseal_failed"));
        let _ = e;
    }
}

fn resolve_paths() -> Result<Paths, InitError> {
    Ok(Paths {
        data_dir: paths::data_dir()?,
        db_path: paths::db_path()?,
        log_dir: paths::log_dir()?,
    })
}
