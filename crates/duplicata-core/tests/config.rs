use std::path::PathBuf;
use std::time::Duration;

use duplicata_core::capture::CanonicalKind;
use duplicata_core::config::{
    BACKOFF_CAP_TOTAL, BACKOFF_START, DEFAULT_MAX_BYTES_CUSTOM, DEFAULT_MAX_BYTES_HDROP,
    DEFAULT_MAX_BYTES_IMAGE, DEFAULT_MAX_BYTES_TEXT, DEFAULT_MAX_ITEMS, DEFAULT_MAX_PINNED,
    LOG_FILE_KEEP, LOG_FILE_MAX_BYTES, MAX_MAX_PINNED, MIN_MAX_ITEMS, MIN_RETENTION_DAYS,
    QUEUE_BYTE_BUDGET,
};
use duplicata_core::hotkey::HotkeyCombo;
use duplicata_core::Config;

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

#[test]
fn with_paths_uses_the_documented_defaults() {
    let c = cfg();
    assert_eq!(c.max_bytes_text, 2 * 1024 * 1024);
    assert_eq!(c.max_bytes_image, 64 * 1024 * 1024);
    assert_eq!(c.max_bytes_hdrop, 1024 * 1024);
    assert_eq!(c.max_bytes_custom, 8 * 1024 * 1024);
    assert_eq!(c.retention, Duration::from_secs(7 * 24 * 60 * 60));
    assert_eq!(c.max_items, DEFAULT_MAX_ITEMS);
    assert_eq!(c.max_pinned, DEFAULT_MAX_PINNED);
    assert!(c.blocked_programs.is_empty());
    assert!(c.heuristic_secret_detection);
    assert_eq!(c.db_path, PathBuf::from("db"));
    assert_eq!(c.log_dir, PathBuf::from("logs"));
    assert_eq!(c.hotkey, HotkeyCombo::DEFAULT);
}

#[test]
fn limit_for_maps_each_canonical_kind_including_both_dib_variants() {
    let c = cfg();
    assert_eq!(c.limit_for(CanonicalKind::UnicodeText), c.max_bytes_text);
    assert_eq!(c.limit_for(CanonicalKind::Dib), c.max_bytes_image);
    assert_eq!(c.limit_for(CanonicalKind::DibV5), c.max_bytes_image);
    assert_eq!(c.limit_for(CanonicalKind::HDrop), c.max_bytes_hdrop);
    assert_eq!(c.limit_for(CanonicalKind::Custom), c.max_bytes_custom);
}

#[test]
fn limit_for_reflects_overridden_fields() {
    let mut c = cfg();
    c.max_bytes_image = 1234;
    assert_eq!(c.limit_for(CanonicalKind::DibV5), 1234);
    assert_eq!(
        c.limit_for(CanonicalKind::UnicodeText),
        DEFAULT_MAX_BYTES_TEXT
    );
}

#[test]
fn missing_file_falls_back_to_all_defaults_with_one_fallback_event() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nao-existe.toml");
    let (c, fallbacks) =
        duplicata_core::Config::load(PathBuf::from("db"), PathBuf::from("logs"), &missing);
    assert_eq!(c.max_bytes_text, DEFAULT_MAX_BYTES_TEXT);
    assert_eq!(c.max_bytes_image, DEFAULT_MAX_BYTES_IMAGE);
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "*");
}

#[test]
fn well_formed_file_overrides_every_field() {
    let toml = "\
        max_bytes_text = 111\n\
        max_bytes_image = 222\n\
        max_bytes_hdrop = 333\n\
        max_bytes_custom = 444\n\
        retention_days = 3\n\
        max_items = 50\n\
        max_pinned = 5\n\
        heuristic_secret_detection = false\n\
        blocked_program = \"a.exe\"\n\
        blocked_program = \"b.exe\"\n\
        hotkey = \"ctrl+alt+k\"\n\
    ";
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), toml);
    assert!(fallbacks.is_empty());
    assert_eq!(c.max_bytes_text, 111);
    assert_eq!(c.max_bytes_image, 222);
    assert_eq!(c.max_bytes_hdrop, 333);
    assert_eq!(c.max_bytes_custom, 444);
    assert_eq!(c.retention, Duration::from_secs(3 * 24 * 60 * 60));
    assert_eq!(c.max_items, 50);
    assert_eq!(c.max_pinned, 5);
    assert!(!c.heuristic_secret_detection);
    assert_eq!(
        c.blocked_programs,
        vec!["a.exe".to_string(), "b.exe".to_string()]
    );
    assert_eq!(c.hotkey, HotkeyCombo::parse("ctrl+alt+k").unwrap());
}

#[test]
fn one_invalid_field_falls_back_only_for_that_field() {
    let toml = "\
        max_bytes_text = nao-e-numero\n\
        max_bytes_image = 999\n\
    ";
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), toml);
    assert_eq!(
        c.max_bytes_text, DEFAULT_MAX_BYTES_TEXT,
        "default só neste campo"
    );
    assert_eq!(
        c.max_bytes_image, 999,
        "os demais campos válidos são aplicados"
    );
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "max_bytes_text");
}

#[test]
fn zero_is_rejected_as_invalid_not_as_a_literal_zero_limit() {
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "max_bytes_hdrop = 0\n");
    assert_eq!(c.max_bytes_hdrop, DEFAULT_MAX_BYTES_HDROP);
    assert_eq!(fallbacks.len(), 1);
}

#[test]
fn comments_and_unknown_keys_are_ignored() {
    let toml = "\
        # comentário\n\
        max_bytes_text = 123 # outro comentário\n\
        chave_desconhecida = 999\n\
    ";
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), toml);
    assert_eq!(c.max_bytes_text, 123);
    assert!(fallbacks.is_empty(), "chave desconhecida não é fallback");
}

#[test]
fn hotkey_key_parses_into_the_default_combo() {
    let (c, fallbacks) =
        duplicata_core::Config::from_str_over(cfg(), "hotkey = \"ctrl+shift+v\"\n");
    assert!(fallbacks.is_empty());
    assert_eq!(c.hotkey, HotkeyCombo::DEFAULT);
}

#[test]
fn invalid_hotkey_string_falls_back_only_for_that_field() {
    let toml = "\
        hotkey = \"isso-nao-e-um-atalho\"\n\
        max_bytes_text = 999\n\
    ";
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), toml);
    assert_eq!(
        c.hotkey,
        HotkeyCombo::DEFAULT,
        "default só neste campo, herdado de cfg()"
    );
    assert_eq!(
        c.max_bytes_text, 999,
        "os demais campos válidos são aplicados"
    );
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "hotkey");
}

#[test]
fn to_toml_string_round_trips_through_from_str_over() {
    let mut c = cfg();
    c.hotkey = HotkeyCombo::parse("alt+f7").unwrap();
    let serialized = c.to_toml_string();

    let (reparsed, fallbacks) = duplicata_core::Config::from_str_over(cfg(), &serialized);
    assert!(
        fallbacks.is_empty(),
        "serialização própria nunca cai em fallback"
    );
    assert_eq!(reparsed.hotkey, c.hotkey);
    assert_eq!(reparsed.max_bytes_text, c.max_bytes_text);
    assert_eq!(reparsed.max_bytes_image, c.max_bytes_image);
    assert_eq!(reparsed.max_bytes_hdrop, c.max_bytes_hdrop);
    assert_eq!(reparsed.max_bytes_custom, c.max_bytes_custom);
    assert_eq!(reparsed.retention, c.retention);
}

#[test]
fn retention_days_below_the_floor_is_clamped_to_the_floor_not_a_fallback() {
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "retention_days = 0\n");
    assert_eq!(
        c.retention,
        Duration::from_secs(MIN_RETENTION_DAYS * 24 * 60 * 60),
        "0 é um pedido válido, tratado como o piso — não é o default de 7 dias"
    );
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "retention_days");
    assert_eq!(fallbacks[0].floor, Some((0, MIN_RETENTION_DAYS)));
}

#[test]
fn retention_days_within_the_floor_reports_no_notice() {
    let (_, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "retention_days = 7\n");
    assert!(
        fallbacks.is_empty(),
        "clamping só é reportado quando de fato ajusta o valor pedido"
    );
}

#[test]
fn max_items_below_the_floor_is_clamped_to_the_floor_not_a_fallback() {
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "max_items = 0\n");
    assert_eq!(c.max_items, MIN_MAX_ITEMS);
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "max_items");
    assert_eq!(
        fallbacks[0].floor,
        Some((0, MIN_MAX_ITEMS as u64)),
        "piso aplicado é reportado (com pedido/aplicado), não silencioso — mas é distinto de um fallback-para-default"
    );
}

#[test]
fn max_items_within_the_floor_reports_no_notice() {
    let (_, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "max_items = 500\n");
    assert!(
        fallbacks.is_empty(),
        "clamping só é reportado quando de fato ajusta o valor pedido"
    );
}

#[test]
fn max_items_invalid_value_falls_back_to_the_default() {
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "max_items = nao-e-numero\n");
    assert_eq!(c.max_items, DEFAULT_MAX_ITEMS);
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "max_items");
}

#[test]
fn max_pinned_zero_is_accepted_without_any_floor() {
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "max_pinned = 0\n");
    assert_eq!(c.max_pinned, 0);
    assert!(fallbacks.is_empty());
}

#[test]
fn max_pinned_invalid_value_falls_back_to_the_default() {
    let (c, fallbacks) =
        duplicata_core::Config::from_str_over(cfg(), "max_pinned = nao-e-numero\n");
    assert_eq!(c.max_pinned, DEFAULT_MAX_PINNED);
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "max_pinned");
}

#[test]
fn max_pinned_above_the_ceiling_is_clamped_to_the_ceiling_with_a_notice() {
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), "max_pinned = 250\n");
    assert_eq!(c.max_pinned, MAX_MAX_PINNED);
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "max_pinned");
    assert_eq!(fallbacks[0].floor, None, "é um teto, não um piso");
    assert_eq!(
        fallbacks[0].ceiling,
        Some((250, MAX_MAX_PINNED as u64)),
        "teto aplicado é reportado com pedido/aplicado, distinto de um fallback-para-default"
    );
}

#[test]
fn max_pinned_at_or_below_the_ceiling_reports_no_notice() {
    for v in ["max_pinned = 200\n", "max_pinned = 25\n"] {
        let (_, fallbacks) = duplicata_core::Config::from_str_over(cfg(), v);
        assert!(fallbacks.is_empty(), "sem ajuste ⇒ sem evento ({v:?})");
    }
    assert_eq!(MAX_MAX_PINNED, 200);
}

#[test]
fn heuristic_secret_detection_parses_true_and_false() {
    let (c_true, f_true) =
        duplicata_core::Config::from_str_over(cfg(), "heuristic_secret_detection = true\n");
    assert!(c_true.heuristic_secret_detection);
    assert!(f_true.is_empty());

    let (c_false, f_false) =
        duplicata_core::Config::from_str_over(cfg(), "heuristic_secret_detection = false\n");
    assert!(!c_false.heuristic_secret_detection);
    assert!(f_false.is_empty());
}

#[test]
fn heuristic_secret_detection_invalid_value_falls_back_to_the_default_true() {
    let (c, fallbacks) =
        duplicata_core::Config::from_str_over(cfg(), "heuristic_secret_detection = talvez\n");
    assert!(c.heuristic_secret_detection, "default é ligado");
    assert_eq!(fallbacks.len(), 1);
    assert_eq!(fallbacks[0].field, "heuristic_secret_detection");
}

#[test]
fn repeated_blocked_program_lines_accumulate_in_order() {
    let toml = "\
        blocked_program = \"keepass.exe\"\n\
        blocked_program = \"1password.exe\"\n\
        blocked_program = \"bitwarden.exe\"\n\
    ";
    let (c, fallbacks) = duplicata_core::Config::from_str_over(cfg(), toml);
    assert_eq!(
        c.blocked_programs,
        vec!["keepass.exe", "1password.exe", "bitwarden.exe"]
    );
    assert!(fallbacks.is_empty(), "chave repetida nunca é um fallback");
}

#[test]
fn to_toml_string_round_trips_the_new_fatia_3_fields() {
    let mut c = cfg();
    c.max_items = 42;
    c.max_pinned = 3;
    c.heuristic_secret_detection = false;
    c.blocked_programs = vec!["a.exe".to_string(), "b.exe".to_string()];
    let serialized = c.to_toml_string();

    let (reparsed, fallbacks) = duplicata_core::Config::from_str_over(cfg(), &serialized);
    assert!(
        fallbacks.is_empty(),
        "serialização própria nunca cai em fallback"
    );
    assert_eq!(reparsed.max_items, 42);
    assert_eq!(reparsed.max_pinned, 3);
    assert!(!reparsed.heuristic_secret_detection);
    assert_eq!(
        reparsed.blocked_programs,
        vec!["a.exe".to_string(), "b.exe".to_string()]
    );
}

#[test]
fn compile_time_constants_have_the_revised_values() {
    assert_eq!(QUEUE_BYTE_BUDGET, 128 * 1024 * 1024);
    assert_eq!(BACKOFF_START, Duration::from_millis(10));
    assert_eq!(BACKOFF_CAP_TOTAL, Duration::from_millis(250));
    assert_eq!(LOG_FILE_MAX_BYTES, 5 * 1024 * 1024);
    assert_eq!(LOG_FILE_KEEP, 3);
    assert_eq!(DEFAULT_MAX_BYTES_TEXT, 2 * 1024 * 1024);
    assert_eq!(DEFAULT_MAX_BYTES_IMAGE, 64 * 1024 * 1024);
    assert_eq!(DEFAULT_MAX_BYTES_HDROP, 1024 * 1024);
    assert_eq!(DEFAULT_MAX_BYTES_CUSTOM, 8 * 1024 * 1024);
}
