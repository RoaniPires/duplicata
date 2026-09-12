#![cfg(windows)]

use duplicata_win::shutdown_request::{request_shutdown, ShutdownOutcome};

#[test]
fn already_exited_when_no_tray_window_is_running() {
    assert_eq!(request_shutdown(), ShutdownOutcome::AlreadyExited);
}
