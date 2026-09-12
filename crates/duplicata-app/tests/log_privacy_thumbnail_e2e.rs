#![cfg(windows)]

use std::path::Path;
use std::time::Duration;

use duplicata_app::logging;
use duplicata_core::canonical::CF_DIB;
use duplicata_core::{
    capture_and_enqueue, run_worker, BackoffPolicy, ByteBudgetQueue, CaptureQueue, CapturedFormat,
    Config, FailingHistoryRepository, FakeClipboardSource, FakeClock, RecentCaptureGuard,
    StoreError, WorkItem, WorkerCounters,
};

fn cfg(dir: &Path) -> Config {
    Config::with_paths(dir.join("db"), dir.join("logs"))
}

fn synthetic_dib_with_canary() -> Vec<u8> {
    const CANARY: &[u8] = b"CANARIO-THUMBNAIL-NAO-PODE-VAZAR-7d3f";
    let (width, height) = (8i32, 8i32);
    let row_bytes = (width as usize * 3).div_ceil(4) * 4;
    let pixel_data_size = row_bytes * height as usize;

    let mut dib = Vec::with_capacity(40 + pixel_data_size);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&width.to_le_bytes());
    dib.extend_from_slice(&height.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&24u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());

    let mut pixels = vec![0u8; pixel_data_size];
    pixels[..CANARY.len()].copy_from_slice(CANARY);
    dib.extend_from_slice(&pixels);
    dib
}

#[test]
fn thumbnail_bytes_from_a_real_image_capture_never_reach_any_log_file() {
    let tmp = tempfile::tempdir().unwrap();
    let log_dir = tmp.path().join("logs");

    let guard = logging::init(&log_dir).expect("logging real deve inicializar num tempdir vazio");

    let dib_bytes = synthetic_dib_with_canary();
    let format = CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: dib_bytes,
    };
    let src = FakeClipboardSource::always(vec![format]);
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
    queue.close();

    let mut repo = FailingHistoryRepository::new(StoreError::Query);
    let counters = WorkerCounters::new();
    let (heuristic_tx, _heuristic_rx) = std::sync::mpsc::channel();
    run_worker(&queue, &mut repo, &config, &counters, &heuristic_tx);

    drop(guard);
    std::thread::sleep(Duration::from_millis(50));

    let mut all_content = String::new();
    let mut checked_any_file = false;
    for entry in std::fs::read_dir(&log_dir).expect("logs\\ deve existir após logging::init") {
        let entry = entry.unwrap();
        if !entry.file_type().unwrap().is_file() {
            continue;
        }
        checked_any_file = true;
        let content = std::fs::read_to_string(entry.path()).unwrap_or_default();

        assert!(
            !content.contains("CANARIO-THUMBNAIL"),
            "marcador dos pixels sintéticos vazou em {}: {content}",
            entry.path().display()
        );
        assert!(
            !content.contains("40, 0, 0, 0"),
            "um Vec<u8> bruto (formatado via Debug) parece ter vazado em {}: {content}",
            entry.path().display()
        );
        all_content.push_str(&content);
    }
    assert!(
        checked_any_file,
        "esperava pelo menos 1 arquivo de log em {}",
        log_dir.display()
    );
    assert!(
        all_content.contains("db_write_failed"),
        "esperava ver o evento db_write_failed no log — a captura de imagem \
         deveria ter chegado à falha de gravação forçada"
    );
}
