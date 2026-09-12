#![cfg(windows)]

mod support;

use std::thread;
use std::time::{Duration, Instant};

use duplicata_app::run::join_worker_with_timeout;
use support::{with_captured_logs, LogBuffer};

#[test]
fn a_stuck_worker_does_not_block_shutdown_and_logs_worker_join_timeout() {
    let worker = thread::spawn(|| loop {
        thread::sleep(Duration::from_secs(3600));
    });

    let buf = LogBuffer::new();
    let start = Instant::now();
    with_captured_logs(&buf, || {
        join_worker_with_timeout(worker, Duration::from_millis(50));
    });
    let elapsed = start.elapsed();

    assert!(
        elapsed < Duration::from_secs(2),
        "join_worker_with_timeout não pode ficar bloqueado além do timeout \
         configurado, mesmo com a worker travada para sempre: levou {elapsed:?}"
    );
    let logs = buf.contents();
    assert!(
        logs.contains("worker_join_timeout"),
        "deve logar worker_join_timeout quando a worker não retorna a tempo: {logs}"
    );
}

#[test]
fn a_worker_that_returns_in_time_does_not_log_worker_join_timeout() {
    let worker = thread::spawn(|| 42);

    let buf = LogBuffer::new();
    with_captured_logs(&buf, || {
        join_worker_with_timeout(worker, Duration::from_secs(5));
    });

    assert!(
        !buf.contents().contains("worker_join_timeout"),
        "não deve logar timeout quando a worker terminou dentro do prazo: {}",
        buf.contents()
    );
}
