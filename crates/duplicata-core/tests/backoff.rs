use std::path::PathBuf;
use std::time::Duration;

use duplicata_core::backoff::{capture_with_retry, BackoffPolicy, RetryOutcome};
use duplicata_core::config::OWNER_PROBE_TIMEOUT;
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

fn assert_copied(outcome: &CaptureOutcome) {
    match outcome {
        CaptureOutcome::Copied { formats, .. } => assert_eq!(formats, &one_format()),
        other => panic!("esperava Copied, veio {other:?}"),
    }
}

#[test]
fn an_owner_that_answers_after_two_probes_is_read_after_exponential_sleeps() {
    let src = FakeClipboardSource::owner_not_responding_then(2, one_format());
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_copied(&outcome);
    assert_eq!(attempts, 3);
    assert_eq!(src.probe_calls(), 3, "toda tentativa passa pela sonda");
    assert_eq!(src.unprobed_reads(), 0, "não precisou da leitura sem sonda");
    assert_eq!(clock.total_slept(), Duration::from_millis(30));
}

#[test]
fn an_owner_that_never_answers_gets_exactly_one_final_read_without_probe() {
    let src = FakeClipboardSource::owner_never_responds(one_format());
    let clock = FakeClock::new();
    let policy = BackoffPolicy::production();
    let (outcome, attempts) = capture_with_retry(&src, &clock, &policy, &cfg()).unwrap();

    assert_copied(&outcome);
    // Sonos de 10, 20, 40 e 80 ms cabem nos 250 ms; o de 160 não.
    assert_eq!(src.probe_calls(), 5);
    assert_eq!(src.unprobed_reads(), 1, "uma leitura final, como antes");
    assert_eq!(attempts, 6);
    assert!(clock.total_slept() <= policy.cap_total);
}

#[test]
fn busy_and_owner_not_responding_share_the_same_backoff() {
    let src = FakeClipboardSource::probe_script_then(
        vec![
            CaptureError::Busy,
            CaptureError::OwnerNotResponding,
            CaptureError::Busy,
        ],
        one_format(),
    );
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_copied(&outcome);
    assert_eq!(attempts, 4);
    assert_eq!(src.unprobed_reads(), 0);
    assert_eq!(clock.total_slept(), Duration::from_millis(70));
}

#[test]
fn a_budget_that_ends_on_busy_is_exhausted_without_the_final_read() {
    let src = FakeClipboardSource::probe_script_then(
        vec![
            CaptureError::OwnerNotResponding,
            CaptureError::OwnerNotResponding,
            CaptureError::OwnerNotResponding,
            CaptureError::OwnerNotResponding,
            CaptureError::Busy,
        ],
        one_format(),
    );
    let clock = FakeClock::new();
    match capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()) {
        Err(RetryOutcome::Exhausted { attempts }) => assert_eq!(attempts, 5),
        other => panic!("esperava Exhausted, veio {other:?}"),
    }
    assert_eq!(src.unprobed_reads(), 0, "ocupada: abrir à força não ajuda");
}

#[test]
fn the_worst_case_of_the_handler_with_an_owner_that_never_answers_stays_under_400_ms() {
    let src = FakeClipboardSource::owner_never_responds(one_format());
    let clock = FakeClock::new();
    capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    let worst = OWNER_PROBE_TIMEOUT * src.probe_calls() + clock.total_slept();
    assert!(
        worst <= Duration::from_millis(400),
        "{} sondas de {OWNER_PROBE_TIMEOUT:?} + {:?} de sono = {worst:?}",
        src.probe_calls(),
        clock.total_slept()
    );
}

#[test]
fn a_new_clipboard_sequence_supersedes_the_wait_without_reading_or_sleeping() {
    let src = FakeClipboardSource::owner_never_responds(one_format());
    src.set_sequences(&[7, 8]);
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_eq!(outcome, CaptureOutcome::Superseded);
    assert_eq!(attempts, 1);
    assert_eq!(src.unprobed_reads(), 0, "a área mudou: nada é lido");
    assert_eq!(clock.sleep_calls(), 0, "nem espera");
}

#[test]
fn the_same_sequence_keeps_waiting_for_the_owner() {
    let src = FakeClipboardSource::owner_not_responding_then(1, one_format());
    src.set_sequences(&[7]);
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_copied(&outcome);
    assert_eq!(attempts, 2);
    assert_eq!(clock.total_slept(), Duration::from_millis(10));
}

#[test]
fn without_a_sequence_the_owner_wait_is_never_superseded() {
    let src = FakeClipboardSource::owner_not_responding_then(1, one_format());
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_copied(&outcome);
    assert_eq!(attempts, 2);
}

#[test]
fn a_busy_clipboard_is_not_superseded_by_a_new_sequence() {
    let src = FakeClipboardSource::busy_then(2, one_format());
    src.set_sequences(&[1, 2, 3, 4]);
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_copied(&outcome);
    assert_eq!(attempts, 3);
}

#[test]
fn a_final_read_that_finds_the_clipboard_busy_is_exhausted() {
    let src = FakeClipboardSource::failing(CaptureError::Busy).with_owner_never_responding();
    let clock = FakeClock::new();
    match capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()) {
        Err(RetryOutcome::Exhausted { attempts }) => assert_eq!(attempts, 6),
        other => panic!("esperava Exhausted, veio {other:?}"),
    }
    assert_eq!(src.unprobed_reads(), 1);
}

#[test]
fn a_final_read_that_fails_reports_the_error() {
    let src = FakeClipboardSource::failing(CaptureError::Unavailable).with_owner_never_responding();
    let clock = FakeClock::new();
    match capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()) {
        Err(RetryOutcome::Fatal(e)) => assert_eq!(e, CaptureError::Unavailable),
        other => panic!("esperava Fatal(Unavailable), veio {other:?}"),
    }
    assert_eq!(src.unprobed_reads(), 1);
}

#[test]
fn a_fatal_error_from_the_probed_read_stops_without_sleeping_or_a_final_read() {
    let src = FakeClipboardSource::probe_script_then(
        vec![CaptureError::OwnerNotResponding, CaptureError::Unavailable],
        one_format(),
    );
    let clock = FakeClock::new();
    match capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()) {
        Err(RetryOutcome::Fatal(e)) => assert_eq!(e, CaptureError::Unavailable),
        other => panic!("esperava Fatal(Unavailable), veio {other:?}"),
    }
    assert_eq!(src.probe_calls(), 2);
    assert_eq!(
        src.unprobed_reads(),
        0,
        "erro fatal não leva à leitura final"
    );
    assert_eq!(clock.total_slept(), Duration::from_millis(10));
}

#[test]
fn a_sequence_change_after_earlier_waits_supersedes_without_a_final_read() {
    let src = FakeClipboardSource::owner_never_responds(one_format());
    // 1ª leitura da sequência no início; as seguintes, uma por falha do dono.
    src.set_sequences(&[7, 7, 7, 9]);
    let clock = FakeClock::new();
    let (outcome, attempts) =
        capture_with_retry(&src, &clock, &BackoffPolicy::production(), &cfg()).unwrap();

    assert_eq!(outcome, CaptureOutcome::Superseded);
    assert_eq!(attempts, 3);
    assert_eq!(src.unprobed_reads(), 0, "a área mudou: nada é lido");
    assert_eq!(clock.total_slept(), Duration::from_millis(30));
}
