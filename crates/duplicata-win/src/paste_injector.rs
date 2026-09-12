use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    SendInput, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_KEYUP, VIRTUAL_KEY,
    VK_CONTROL, VK_V,
};
use windows::Win32::UI::WindowsAndMessaging::{IsWindow, SetForegroundWindow};

pub fn send_to(prev_hwnd: HWND) {
    // SAFETY: `IsWindow` só consulta se o handle ainda é uma janela válida —
    // sem pré-condição além de um HWND (possivelmente obsoleto).
    if !unsafe { IsWindow(Some(prev_hwnd)) }.as_bool() {
        return;
    }
    // SAFETY: `prev_hwnd` validado acima; falha aqui é best-effort (ex.:
    // restrição do SO contra "roubar" o foco) — seguimos mesmo assim, o
    // `SendInput` abaixo só tem efeito visível se o foco realmente mudou.
    let _ = unsafe { SetForegroundWindow(prev_hwnd) };

    let inputs = [
        key_input(VK_CONTROL, false),
        key_input(VK_V, false),
        key_input(VK_V, true),
        key_input(VK_CONTROL, true),
    ];
    // SAFETY: `inputs` é um slice de `INPUT` totalmente inicializado (via
    // `key_input`, sem campos indeterminados); `cbSize` é o tamanho de um
    // único `INPUT`, exigido pela API.
    unsafe {
        SendInput(&inputs, core::mem::size_of::<INPUT>() as i32);
    }
}

fn key_input(vk: VIRTUAL_KEY, key_up: bool) -> INPUT {
    INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: vk,
                wScan: 0,
                dwFlags: if key_up {
                    KEYEVENTF_KEYUP
                } else {
                    Default::default()
                },
                time: 0,
                dwExtraInfo: 0,
            },
        },
    }
}
