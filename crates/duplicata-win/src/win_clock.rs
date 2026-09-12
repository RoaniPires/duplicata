use std::time::{Duration, SystemTime, UNIX_EPOCH};

use duplicata_core::Clock;

#[derive(Debug, Default, Clone, Copy)]
pub struct WinClock;

impl Clock for WinClock {
    fn now_ms(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn sleep(&self, dur: Duration) {
        std::thread::sleep(dur);
    }
}
