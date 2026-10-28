use duplicata_core::settings_input::{
    digits_only, normalize_max_items, normalize_max_pinned, normalize_retention_days,
    NUMERIC_MAX_DIGITS,
};

#[test]
fn letters_and_symbols_are_dropped_from_a_numeric_field() {
    assert_eq!(digits_only("a5b"), "5");
    assert_eq!(digits_only(" 1-2,3 "), "123");
    assert_eq!(digits_only("abc"), "");
    assert_eq!(digits_only("\u{0663}"), "", "só dígitos ASCII");
}

#[test]
fn the_field_holds_as_many_digits_as_the_largest_value() {
    assert_eq!(NUMERIC_MAX_DIGITS, u32::MAX.to_string().len());
}

#[test]
fn retention_empty_returns_the_previous_value_zero_becomes_one_and_overflow_is_capped() {
    assert_eq!(normalize_retention_days("", 7), 7);
    assert_eq!(normalize_retention_days("0", 7), 1);
    assert_eq!(normalize_retention_days("30", 7), 30);
    assert_eq!(normalize_retention_days("9999999999", 7), u32::MAX);
    assert_eq!(
        normalize_retention_days("99999999999999999999999", 7),
        u32::MAX
    );
}

#[test]
fn max_items_has_a_floor_of_one_and_empty_returns_the_previous_value() {
    assert_eq!(normalize_max_items("", 500), 500);
    assert_eq!(normalize_max_items("0", 500), 1);
    assert_eq!(normalize_max_items("1000", 500), 1000);
    assert_eq!(normalize_max_items("9999999999", 500), u32::MAX);
}

#[test]
fn max_pinned_accepts_zero_and_caps_at_two_hundred() {
    assert_eq!(normalize_max_pinned("", 25), 25);
    assert_eq!(normalize_max_pinned("0", 25), 0);
    assert_eq!(normalize_max_pinned("40", 25), 40);
    assert_eq!(normalize_max_pinned("500", 25), 200);
    assert_eq!(normalize_max_pinned("9999999999", 25), 200);
}

#[test]
fn stray_non_digits_do_not_break_normalization() {
    assert_eq!(normalize_retention_days("a5b", 7), 5);
    assert_eq!(normalize_max_items("x", 500), 500);
}
