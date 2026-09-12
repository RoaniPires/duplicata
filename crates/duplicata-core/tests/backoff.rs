use std::path::PathBuf;
use std::time::Duration;

use duplicata_core::backoff::{capture_with_retry, BackoffPolicy, RetryOutcome};
use duplicata_core::{
    CaptureError, CaptureOutcome, CapturedFormat, Config, FakeClipboardSource, FakeClock,
};

fn one_format() -> Vec<CapturedFormat> {
    vec![CapturedFormat {
        format_id: 13,
        format_name: None,
        bytes: b"oi".to_vec(),
    }]
}

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

#[test]
fn succeeds_on_first_try_without_sleeping() {
    let src = FakeClipboardSource::always(one_format());
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();
    match outcome {
        CaptureOutcome::Copied { formats, .. } => assert_eq!(formats, one_format()),
        other => panic!("esperava Copied, veio {other:?}"),
    }
    assert_eq!(attempts, 1);
    assert_eq!(clock.sleep_calls(), 0);
}

#[test]
fn retries_busy_then_succeeds_with_exponential_backoff() {
    let src = FakeClipboardSource::busy_then(2, one_format());
    let clock = FakeClock::new();
    let (_, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_eq!(attempts, 3);
    assert_eq!(src.calls(), 3);
    assert_eq!(clock.sleep_calls(), 2);
    assert_eq!(clock.total_slept(), Duration::from_millis(30));
}

#[test]
fn exhausts_the_cap_and_never_sleeps_past_it() {
    let src = FakeClipboardSource::failing(CaptureError::Busy);
    let clock = FakeClock::new();
    let policy = BackoffPolicy::production();

    match capture_with_retry(&src, &clock, &policy, &cfg()) {
        Err(RetryOutcome::Exhausted { attempts }) => {
            assert!(attempts >= 2, "tentou mais de uma vez");
        }
        other => panic!("esperava Exhausted, veio {other:?}"),
    }
    assert!(
        clock.total_slept() <= policy.cap_total,
        "nunca dorme além do teto de {:?} (dormiu {:?})",
        policy.cap_total,
        clock.total_slept()
    );
}

#[test]
fn unavailable_and_empty_return_immediately_without_retry() {
    for err in [CaptureError::Unavailable, CaptureError::Empty] {
        let src = FakeClipboardSource::failing(err.clone());
        let clock = FakeClock::new();
        match capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()) {
            Err(RetryOutcome::Fatal(e)) => assert_eq!(e, err),
            other => panic!("esperava Fatal({err:?}), veio {other:?}"),
        }
        assert_eq!(clock.sleep_calls(), 0);
        assert_eq!(src.calls(), 1);
    }
}

#[test]
fn a_custom_small_cap_bounds_the_attempts() {
    let src = FakeClipboardSource::failing(CaptureError::Busy);
    let clock = FakeClock::new();
    let policy = BackoffPolicy {
        start: Duration::from_millis(10),
        cap_total: Duration::from_millis(10),
    };
    match capture_with_retry(&src, &clock, &policy, &cfg()) {
        Err(RetryOutcome::Exhausted { attempts }) => assert_eq!(attempts, 2),
        other => panic!("veio {other:?}"),
    }
    assert_eq!(clock.total_slept(), Duration::from_millis(10));
}
