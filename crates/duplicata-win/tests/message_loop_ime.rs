#![cfg(windows)]

use duplicata_win::message_loop::is_system_ime_class;

#[test]
fn the_system_ime_windows_are_told_apart_from_the_app_windows() {
    assert!(is_system_ime_class("MSCTFIME UI"));
    assert!(is_system_ime_class("IME"));
    for ours in [
        "duplicata_settings_dialog",
        "duplicata_history_window",
        "Edit",
        "Button",
    ] {
        assert!(!is_system_ime_class(ours), "{ours}");
    }
}
