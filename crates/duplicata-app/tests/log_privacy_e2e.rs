use std::path::Path;
use std::time::Duration;

use duplicata_app::logging;
use duplicata_core::{
    capture_and_enqueue, run_worker, unicode_text_format, BackoffPolicy, ByteBudgetQueue,
    CaptureQueue, Config, FailingHistoryRepository, FakeClipboardSource, FakeClock,
    RecentCaptureGuard, StoreError, WorkItem, WorkerCounters,
};

const SEGREDO: &str = "SEGREDO-E2E-NAO-PODE-VAZAR-CLIPBOARD-4f9c2a";
const SOURCE_PROGRAM: &str = "gerenciador-de-senha-e2e.exe";

fn cfg(dir: &Path) -> Config {
    Config::with_paths(dir.join("db"), dir.join("logs"))
}

#[test]
fn synthetic_content_never_reaches_any_log_file_even_on_a_forced_write_failure() {
    let tmp = tempfile::tempdir().unwrap();
    let log_dir = tmp.path().join("logs");

    let guard = logging::init(&log_dir).expect("logging real deve inicializar num tempdir vazio");

    let src = FakeClipboardSource::always(vec![unicode_text_format(SEGREDO)]);
    let clock = FakeClock::new();
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let config = cfg(tmp.path());

    capture_and_enqueue(
        &src,
        &clock,
        &BackoffPolicy::production(),
        &config,
        &queue,
        &RecentCaptureGuard::new(),
    );

    let src2 = FakeClipboardSource::always(vec![unicode_text_format(
        "texto comum, capturado normalmente",
    )]);
    src2.set_source_program(Some(SOURCE_PROGRAM));
    capture_and_enqueue(
        &src2,
        &clock,
        &BackoffPolicy::production(),
        &config,
        &queue,
        &RecentCaptureGuard::new(),
    );

    queue.close();

    let mut repo = FailingHistoryRepository::new(StoreError::Query);
    let counters = WorkerCounters::new();
    let (heuristic_tx, _heuristic_rx) = std::sync::mpsc::channel();
    run_worker(&queue, &mut repo, &config, &counters, &heuristic_tx);

    drop(guard);
    std::thread::sleep(Duration::from_millis(50));

    let mut checked_any_file = false;
    for entry in std::fs::read_dir(&log_dir).expect("logs\\ deve existir após logging::init") {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        checked_any_file = true;
        let content = std::fs::read_to_string(entry.path()).unwrap_or_default();
        assert!(
            !content.contains(SEGREDO),
            "conteúdo sintético vazou em {}: {content}",
            entry.path().display()
        );
        assert!(
            !content.contains("NAO-PODE-VAZAR"),
            "fragmento do conteúdo sintético vazou em {}: {content}",
            entry.path().display()
        );
        assert!(
            !content.contains(SOURCE_PROGRAM),
            "nome do executável de origem vazou em {}: {content}",
            entry.path().display()
        );
        assert!(
            !content.contains("gerenciador-de-senha"),
            "fragmento do nome do executável vazou em {}: {content}",
            entry.path().display()
        );
    }
    assert!(
        checked_any_file,
        "esperava pelo menos 1 arquivo de log em {}",
        log_dir.display()
    );
}
