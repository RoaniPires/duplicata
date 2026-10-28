//! Bordas da gravação só do atalho pelo diálogo de conflito: arquivo
//! ilegível, valores salvos mantidos.

use std::time::Duration;

use duplicata_core::{Config, HotkeyCombo, MOD_ALT, MOD_CONTROL};
use duplicata_win::hotkey_dialog::persist_hotkey_only;

const NEW: HotkeyCombo = HotkeyCombo {
    modifiers: MOD_CONTROL | MOD_ALT,
    vkey: 0x48,
};

fn base(dir: &std::path::Path) -> Config {
    Config::with_paths(dir.join("db"), dir.join("logs"))
}

#[test]
fn an_unreadable_file_is_left_byte_for_byte_and_the_error_is_returned() {
    // Quebra: ao falhar a leitura, grava os padrões/aplicado por cima do arquivo do usuário.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let bytes: Vec<u8> = vec![0xff, 0xfe, 0x00, b'r', 0xc3, 0x28];
    std::fs::write(&path, &bytes).unwrap();
    let mut shared = base(dir.path());
    shared.blocked_programs = vec!["keepass.exe".into()];

    let result = persist_hotkey_only(&path, &shared, NEW);
    assert!(result.is_err());
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn the_saved_values_stay_and_the_applied_ones_do_not_reach_the_file() {
    // Quebra: o valor aplicado (em memória) vai ao disco, ou o salvo se perde.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(
        &path,
        "# meu comentário\nretention_days = 9\nmax_items = 123\n",
    )
    .unwrap();
    let mut shared = base(dir.path());
    shared.retention = Duration::from_secs(30 * 86_400);
    shared.max_items = 999;

    persist_hotkey_only(&path, &shared, NEW).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let on_disk = Config::from_str_over(base(dir.path()), &text).0;
    assert_eq!(on_disk.hotkey, NEW);
    assert_eq!(on_disk.retention, Duration::from_secs(9 * 86_400));
    assert_eq!(on_disk.max_items, 123);
}

#[test]
fn writing_twice_keeps_the_last_hotkey_and_the_saved_values() {
    // Quebra: a segunda gravação parte do compartilhado e não do arquivo.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    std::fs::write(&path, "max_items = 123\n").unwrap();
    let shared = base(dir.path());
    persist_hotkey_only(&path, &shared, NEW).unwrap();
    persist_hotkey_only(&path, &shared, HotkeyCombo::DEFAULT).unwrap();
    let text = std::fs::read_to_string(&path).unwrap();
    let on_disk = Config::from_str_over(base(dir.path()), &text).0;
    assert_eq!(on_disk.hotkey, HotkeyCombo::DEFAULT);
    assert_eq!(on_disk.max_items, 123);
}
