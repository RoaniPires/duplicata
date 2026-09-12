use windows::core::HSTRING;
use windows::Win32::UI::WindowsAndMessaging::{
    MessageBoxW, MB_ICONERROR, MB_ICONWARNING, MB_OK, MB_SYSTEMMODAL,
};

pub fn show_init_error(message: &str) {
    let title = HSTRING::from("duplicata — erro de inicialização");
    let body = HSTRING::from(message);

    // SAFETY: `MessageBoxW` com strings wide válidas e terminadas em NUL
    // (garantido por `HSTRING`), sem janela-pai (modal de sistema). O valor de
    // retorno é 0 apenas em falha, caso em que reportamos por stderr.
    let result = unsafe { MessageBoxW(None, &body, &title, MB_ICONERROR | MB_OK | MB_SYSTEMMODAL) };

    if result.0 == 0 {
        eprintln!("duplicata: erro de inicialização: {message}");
    }
}

pub fn show_uninstall_leftover(message: &str) {
    let title = HSTRING::from("duplicata — desinstalação");
    let body = HSTRING::from(message);

    // SAFETY: mesma garantia de `show_init_error` — strings wide válidas via
    // `HSTRING`, sem janela-pai.
    let result =
        unsafe { MessageBoxW(None, &body, &title, MB_ICONWARNING | MB_OK | MB_SYSTEMMODAL) };

    if result.0 == 0 {
        eprintln!("duplicata: {message}");
    }
}
