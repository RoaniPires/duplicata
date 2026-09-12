use std::time::{Duration, Instant};

use duplicata_core::{Clock, FakeClock};

#[test]
fn starts_at_zero_or_at_a_given_instant() {
    assert_eq!(FakeClock::new().now_ms(), 0);
    assert_eq!(
        FakeClock::starting_at(1_700_000_000_000).now_ms(),
        1_700_000_000_000
    );
}

#[test]
fn sleep_is_instantaneous_but_advances_virtual_time() {
    let clock = FakeClock::new();
    let wall = Instant::now();

    clock.sleep(Duration::from_millis(10));
    clock.sleep(Duration::from_millis(20));
    clock.sleep(Duration::from_millis(40));

    assert!(wall.elapsed() < Duration::from_millis(50));
    assert_eq!(clock.now_ms(), 70);
    assert_eq!(clock.total_slept(), Duration::from_millis(70));
    assert_eq!(clock.sleep_calls(), 3);
}

#[test]
fn advance_moves_time_without_counting_as_sleep() {
    let clock = FakeClock::starting_at(100);
    clock.advance(Duration::from_millis(5));
    assert_eq!(clock.now_ms(), 105);
    assert_eq!(clock.sleep_calls(), 0);
    assert_eq!(clock.total_slept(), Duration::ZERO);
}
