#![cfg(windows)]

use std::thread;
use std::time::Duration;

use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetDlgItem, PostMessageW, SendMessageW, BM_CLICK, WM_CLOSE,
};

use duplicata_win::uninstall_dialog::ask_delete_or_preserve;

const ID_RADIO_PRESERVE: i32 = 2;
const ID_CONTINUE: i32 = 3;

fn find_dialog() -> HWND {
    for _ in 0..200 {
        // SAFETY: `FindWindowW` só consulta janelas top-level já existentes.
        if let Ok(hwnd) = unsafe { FindWindowW(w!("duplicata_uninstall_choice"), None) } {
            return hwnd;
        }
        thread::sleep(Duration::from_millis(10));
    }
    panic!("diálogo de desinstalação não apareceu a tempo");
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn clicking_continue_with_the_default_selection_means_delete() {
    let clicker = thread::spawn(|| {
        let hwnd = find_dialog();
        // SAFETY: `hwnd`/`GetDlgItem` válidos; clica "Continuar" sem mexer
        // nos rádios — "apagar" já vem marcado por padrão (FR-013).
        unsafe {
            let continue_btn =
                GetDlgItem(Some(hwnd), ID_CONTINUE).expect("botão Continuar não encontrado");
            SendMessageW(continue_btn, BM_CLICK, None, None);
        }
    });

    let delete = ask_delete_or_preserve(false);
    clicker.join().unwrap();

    assert!(
        delete,
        "apagar é o padrão — clicar Continuar sem mexer deve devolver true"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn selecting_preserve_then_continuing_means_preserve() {
    let clicker = thread::spawn(|| {
        let hwnd = find_dialog();
        // SAFETY: `hwnd`/`GetDlgItem` válidos.
        unsafe {
            let preserve =
                GetDlgItem(Some(hwnd), ID_RADIO_PRESERVE).expect("rádio Preservar não encontrado");
            SendMessageW(preserve, BM_CLICK, None, None);
            let continue_btn =
                GetDlgItem(Some(hwnd), ID_CONTINUE).expect("botão Continuar não encontrado");
            SendMessageW(continue_btn, BM_CLICK, None, None);
        }
    });

    let delete = ask_delete_or_preserve(false);
    clicker.join().unwrap();

    assert!(
        !delete,
        "selecionar Preservar e continuar deve devolver false"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn closing_the_window_without_confirming_never_deletes() {
    let closer = thread::spawn(|| {
        let hwnd = find_dialog();
        // SAFETY: `hwnd` válido; `WM_CLOSE` é o mesmo que o X/Alt+F4 posta.
        let _ =
            unsafe { PostMessageW(Some(hwnd), WM_CLOSE, Default::default(), Default::default()) };
    });

    let delete = ask_delete_or_preserve(false);
    closer.join().unwrap();

    assert!(
        !delete,
        "fechar sem confirmar nunca deve apagar, mesmo com apagar pré-selecionado"
    );
}
