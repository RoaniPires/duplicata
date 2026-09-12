use duplicata_core::hotkey::{should_suppress_reopen, TOGGLE_DEBOUNCE_MS};

#[test]
fn within_the_debounce_window_is_suppressed() {
    assert!(should_suppress_reopen(
        1_000,
        1_000 + TOGGLE_DEBOUNCE_MS - 1
    ));
}

#[test]
fn at_the_exact_boundary_is_not_suppressed() {
    assert!(!should_suppress_reopen(1_000, 1_000 + TOGGLE_DEBOUNCE_MS));
}

#[test]
fn well_outside_the_window_is_not_suppressed() {
    assert!(!should_suppress_reopen(
        1_000,
        1_000 + TOGGLE_DEBOUNCE_MS + 500
    ));
}

#[test]
fn now_before_last_hide_never_panics_and_is_treated_as_within_the_window() {
    assert!(should_suppress_reopen(1_000, 500));
}
