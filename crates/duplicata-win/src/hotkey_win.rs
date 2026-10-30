use duplicata_core::HotkeyCombo;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    RegisterHotKey, UnregisterHotKey, HOT_KEY_MODIFIERS, MOD_WIN,
};

pub const HOTKEY_ID: i32 = 1;
/// O Win+V da opção "Abrir com Win+V"; o `HOTKEY_ID` é a reserva.
pub const HOTKEY_ID_WIN_V: i32 = 2;
const VK_V: u32 = 0x56;

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
    // e de shutdown; `UnregisterHotKey` de um id não registrado só
    // devolve erro, sem efeito colateral.
    unsafe { UnregisterHotKey(Some(hwnd), HOTKEY_ID) }
}

pub fn register_win_v(hwnd: HWND) -> windows::core::Result<()> {
    // SAFETY: como em `register`; falha com o Win+V ainda do Explorer
    // (`ERROR_HOTKEY_ALREADY_REGISTERED`).
    unsafe { RegisterHotKey(Some(hwnd), HOTKEY_ID_WIN_V, MOD_WIN, VK_V) }
}

pub fn unregister_win_v(hwnd: HWND) -> windows::core::Result<()> {
    // SAFETY: como em `unregister`.
    unsafe { UnregisterHotKey(Some(hwnd), HOTKEY_ID_WIN_V) }
}
