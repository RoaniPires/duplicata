use std::time::Duration;

use duplicata_core::{cutoff_ms, over_count_by, pin_cap_reached};

#[test]
fn cutoff_is_now_minus_retention() {
    assert_eq!(cutoff_ms(1_000, Duration::from_millis(300)), 700);
    let seven_days = Duration::from_secs(7 * 24 * 60 * 60);
    assert_eq!(
        cutoff_ms(10_000_000_000, seven_days),
        10_000_000_000 - 7 * 24 * 60 * 60 * 1000
    );
}

#[test]
fn cutoff_saturates_at_zero() {
    assert_eq!(cutoff_ms(100, Duration::from_millis(500)), 0);
}

#[test]
fn over_count_is_zero_within_the_limit() {
    assert_eq!(over_count_by(0, 500), 0);
    assert_eq!(over_count_by(499, 500), 0);
    assert_eq!(
        over_count_by(500, 500),
        0,
        "exatamente no limite não remove nada"
    );
}

#[test]
fn over_count_is_the_surplus_above_the_limit() {
    assert_eq!(over_count_by(501, 500), 1);
    assert_eq!(over_count_by(750, 500), 250);
    assert_eq!(over_count_by(3, 1), 2, "piso de 1 item: os outros 2 saem");
}

#[test]
fn pin_cap_is_reached_at_or_above_the_configured_ceiling() {
    assert!(!pin_cap_reached(24, 25), "abaixo do teto: há espaço");
    assert!(
        pin_cap_reached(25, 25),
        "exatamente no teto: sem espaço para mais um"
    );
    assert!(
        pin_cap_reached(26, 25),
        "acima do teto (FR-027 baixou o teto): sem espaço"
    );
}

#[test]
fn pin_cap_zero_blocks_pinning_entirely() {
    assert!(pin_cap_reached(0, 0));
}
