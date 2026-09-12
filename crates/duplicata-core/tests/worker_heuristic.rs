use std::path::PathBuf;
use std::sync::mpsc;

use duplicata_core::canonical::CF_UNICODETEXT;
use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
use duplicata_core::{
    run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat, Config,
    FakeHistoryRepository, RawCapture, Timestamp, WorkItem, WorkerCounters,
};

fn cfg(heuristic_on: bool) -> Config {
    let mut c = Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"));
    c.heuristic_secret_detection = heuristic_on;
    c
}

fn secret_text_capture(ts: u64) -> RawCapture {
    let bytes = utf16le("-----BEGIN PRIVATE KEY-----\nMIIEvQ...\n-----END PRIVATE KEY-----");
    let canonical = CanonicalSelection {
        format_id: CF_UNICODETEXT,
        format_name: None,
        kind: CanonicalKind::UnicodeText,
        byte_len: bytes.len() as u64,
    };
    RawCapture::new(
        vec![CapturedFormat {
            format_id: CF_UNICODETEXT,
            format_name: None,
            bytes,
        }],
        canonical,
        Timestamp::from_millis(ts),
    )
}

fn normal_text_capture(ts: u64) -> RawCapture {
    let bytes = utf16le("texto comum, sem nada de especial");
    let canonical = CanonicalSelection {
        format_id: CF_UNICODETEXT,
        format_name: None,
        kind: CanonicalKind::UnicodeText,
        byte_len: bytes.len() as u64,
    };
    RawCapture::new(
        vec![CapturedFormat {
            format_id: CF_UNICODETEXT,
            format_name: None,
            bytes,
        }],
        canonical,
        Timestamp::from_millis(ts),
    )
}

#[test]
fn a_secret_looking_capture_skips_upsert_and_signals_when_the_flag_is_on() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let cap = secret_text_capture(1);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.close();

    let mut repo = FakeHistoryRepository::new();
    let counters = WorkerCounters::new();
    let (heuristic_tx, heuristic_rx) = mpsc::channel();
    run_worker(&queue, &mut repo, &cfg(true), &counters, &heuristic_tx);

    assert!(repo.is_empty(), "conteúdo de segredo nunca é gravado");
    assert_eq!(
        counters.processed(),
        1,
        "ainda conta como processado — a worker tratou o item"
    );
    assert_eq!(
        heuristic_rx.try_recv(),
        Ok(()),
        "o sinal de rejeição heurística deve ter sido enviado"
    );
}

#[test]
fn the_same_capture_is_stored_normally_when_the_flag_is_off() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let cap = secret_text_capture(1);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.close();

    let mut repo = FakeHistoryRepository::new();
    let counters = WorkerCounters::new();
    let (heuristic_tx, heuristic_rx) = mpsc::channel();
    run_worker(&queue, &mut repo, &cfg(false), &counters, &heuristic_tx);

    assert_eq!(
        repo.len(),
        1,
        "com a detecção desligada, o conteúdo é gravado normalmente"
    );
    assert!(
        heuristic_rx.try_recv().is_err(),
        "nenhum sinal quando a detecção está desligada"
    );
}

#[test]
fn a_normal_capture_never_signals_even_with_the_flag_on() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let cap = normal_text_capture(1);
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.close();

    let mut repo = FakeHistoryRepository::new();
    let counters = WorkerCounters::new();
    let (heuristic_tx, heuristic_rx) = mpsc::channel();
    run_worker(&queue, &mut repo, &cfg(true), &counters, &heuristic_tx);

    assert_eq!(repo.len(), 1, "conteúdo comum é gravado normalmente");
    assert!(
        heuristic_rx.try_recv().is_err(),
        "captura normal nunca dispara o sinal heurístico"
    );
}

#[test]
fn recover_capture_always_bypasses_the_heuristic_even_with_the_flag_on() {
    let queue: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let cap = secret_text_capture(1);
    queue.push(
        WorkItem::RecoverCapture(cap.clone()),
        cap.total_bytes as usize,
    );
    queue.close();

    let mut repo = FakeHistoryRepository::new();
    let counters = WorkerCounters::new();
    let (heuristic_tx, heuristic_rx) = mpsc::channel();
    run_worker(&queue, &mut repo, &cfg(true), &counters, &heuristic_tx);

    assert_eq!(
        repo.len(),
        1,
        "RecoverCapture ignora a checagem heurística, mesmo com o mesmo conteúdo que a dispararia via Capture"
    );
    assert_eq!(
        counters.processed(),
        1,
        "RecoverCapture conta como uma captura processada, igual a Capture"
    );
    assert!(
        heuristic_rx.try_recv().is_err(),
        "RecoverCapture nunca dispara o sinal (a checagem nem roda)"
    );
}
