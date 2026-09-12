#![cfg(windows)]

use std::fs;

use duplicata_app::uninstall_cli::uninstall_data_at;

fn seed(dir: &std::path::Path, name: &str) {
    fs::write(dir.join(name), b"conteudo de fixture").unwrap();
}

#[test]
fn choosing_delete_removes_everything_and_reports_complete() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "duplicata.db");
    seed(dir.path(), "config.toml");

    let exit = uninstall_data_at(dir.path(), &|_history_protected| true);

    assert_eq!(exit, 0, "remoção completa deve devolver 0");
    assert!(!dir.path().join("duplicata.db").exists());
    assert!(!dir.path().join("config.toml").exists());
}

#[test]
fn choosing_preserve_leaves_everything_untouched() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "duplicata.db");
    seed(dir.path(), "config.toml");

    let exit = uninstall_data_at(dir.path(), &|_history_protected| false);

    assert_eq!(exit, 0, "preservar também devolve 0 — nada deu errado");
    assert!(dir.path().join("duplicata.db").exists());
    assert!(dir.path().join("config.toml").exists());
}

#[test]
fn history_protected_flag_reflects_the_work_copy_presence() {
    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "duplicata.work.db");

    let seen = std::cell::Cell::new(None);
    let exit = uninstall_data_at(dir.path(), &|history_protected| {
        seen.set(Some(history_protected));
        false
    });

    assert_eq!(exit, 0);
    assert_eq!(
        seen.get(),
        Some(true),
        "duplicata.work.db presente deveria sinalizar história protegida"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria o diálogo real"]
fn end_to_end_with_the_real_dialog_deletes_when_the_user_confirms() {
    use std::thread;
    use std::time::Duration;
    use windows::core::w;
    use windows::Win32::UI::WindowsAndMessaging::{
        FindWindowW, GetDlgItem, SendMessageW, BM_CLICK,
    };

    let dir = tempfile::tempdir().unwrap();
    seed(dir.path(), "duplicata.db");

    let clicker = thread::spawn(|| {
        let hwnd = loop {
            // SAFETY: só consulta janelas top-level já existentes.
            if let Ok(hwnd) = unsafe { FindWindowW(w!("duplicata_uninstall_choice"), None) } {
                break hwnd;
            }
            thread::sleep(Duration::from_millis(10));
        };
        // SAFETY: `hwnd`/`GetDlgItem` válidos — clica Continuar com o
        // padrão (apagar) já selecionado.
        unsafe {
            let continue_btn = GetDlgItem(Some(hwnd), 3).expect("Continuar não encontrado");
            SendMessageW(continue_btn, BM_CLICK, None, None);
        }
    });

    let exit = uninstall_data_at(
        dir.path(),
        &duplicata_win::uninstall_dialog::ask_delete_or_preserve,
    );
    clicker.join().unwrap();

    assert_eq!(exit, 0);
    assert!(!dir.path().join("duplicata.db").exists());
}
