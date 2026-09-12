use duplicata_core::HotkeyCombo;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS,
};

pub const HOTKEY_ID: i32 = 1;

pub fn register(hwnd: HWND, combo: &HotkeyCombo) -> windows::core::Result<()> {
    // SAFETY: `hwnd` é uma janela válida do processo (dona da message loop);
    // `RegisterHotKey` não tem pré-condição além disso.
    unsafe {
        RegisterHotKey(
            Some(hwnd),
            HOTKEY_ID,
            HOT_KEY_MODIFIERS(combo.modifiers),
            combo.vkey,
        )
    }
}

pub fn unregister(hwnd: HWND) -> windows::core::Result<()> {
    // SAFETY: idempotente o suficiente para os caminhos de troca de atalho
    // (US4) e de shutdown; `UnregisterHotKey` de um id não registrado só
    // devolve erro, sem efeito colateral.
    unsafe { UnregisterHotKey(Some(hwnd), HOTKEY_ID) }
}
