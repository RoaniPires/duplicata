use std::time::{Duration, Instant};

use duplicata_core::Clock;
use duplicata_win::WinClock;

#[test]
fn now_ms_is_after_2020() {
    const Y2020: u64 = 1_577_836_800_000;
    assert!(WinClock.now_ms() > Y2020);
}

#[test]
fn sleep_actually_waits() {
    let start = Instant::now();
    WinClock.sleep(Duration::from_millis(20));
    assert!(start.elapsed() >= Duration::from_millis(15));
}
