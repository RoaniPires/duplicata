use std::sync::Mutex;
use std::time::Duration;

pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;

    fn sleep(&self, dur: Duration);
}

#[derive(Debug)]
pub struct FakeClock {
    state: Mutex<FakeClockState>,
}

#[derive(Debug)]
struct FakeClockState {
    now_ms: u64,
    total_slept: Duration,
    sleep_calls: u32,
}

impl FakeClock {
    pub fn new() -> Self {
        Self::starting_at(0)
    }

    pub fn starting_at(now_ms: u64) -> Self {
        FakeClock {
            state: Mutex::new(FakeClockState {
                now_ms,
                total_slept: Duration::ZERO,
                sleep_calls: 0,
            }),
        }
    }

    pub fn total_slept(&self) -> Duration {
        self.state.lock().unwrap().total_slept
    }

    pub fn sleep_calls(&self) -> u32 {
        self.state.lock().unwrap().sleep_calls
    }

    pub fn advance(&self, dur: Duration) {
        let mut s = self.state.lock().unwrap();
        s.now_ms = s.now_ms.saturating_add(dur.as_millis() as u64);
    }
}

impl Default for FakeClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for FakeClock {
    fn now_ms(&self) -> u64 {
        self.state.lock().unwrap().now_ms
    }

    fn sleep(&self, dur: Duration) {
        let mut s = self.state.lock().unwrap();
        s.total_slept += dur;
        s.sleep_calls += 1;
        s.now_ms = s.now_ms.saturating_add(dur.as_millis() as u64);
    }
}
