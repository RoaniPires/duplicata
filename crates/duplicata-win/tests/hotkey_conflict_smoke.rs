#![cfg(windows)]

//! Janela de conflito da reserva: uma só, e fechada quando o Win+V passa a
//! valer (senão escolher uma combinação nela registraria a reserva junto do
//! Win+V). Vagas e registro em memória: nenhum atalho real é registrado.

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use duplicata_core::testsupport::{FakeDisabledHotkeys, FakeHotkeySlots};
use duplicata_core::win_v::WinVStatus;
use duplicata_core::{Config, HotkeyCombo};
use duplicata_win::hotkey_dialog;
use duplicata_win::win_v_runtime::WinVRuntime;
use windows::core::w;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::FindWindowExW;

fn conflict_windows() -> usize {
    let mut count = 0;
    let mut after = None;
    // SAFETY: só enumera janelas top-level da classe da janela de conflito.
    while let Ok(hwnd) = unsafe { FindWindowExW(None, after, w!("duplicata_hotkey_dialog"), None) }
    {
        count += 1;
        after = Some(hwnd);
    }
    count
}

fn open_conflict(config: &Rc<RefCell<Config>>, dir: &std::path::Path) {
    hotkey_dialog::open_after_conflict(
        HWND::default(),
        HotkeyCombo::DEFAULT,
        Rc::clone(config),
        dir.join("config.toml"),
        Rc::new(Cell::new(None)),
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn opening_the_conflict_window_again_keeps_a_single_one() {
    let dir = tempfile::tempdir().unwrap();
    let config = Rc::new(RefCell::new(Config::with_paths(
        dir.path().join("db"),
        dir.path().join("logs"),
    )));
    open_conflict(&config, dir.path());
    let first = hotkey_dialog::conflict_window().expect("aberta");
    open_conflict(&config, dir.path());
    open_conflict(&config, dir.path());
    assert_eq!(hotkey_dialog::conflict_window(), Some(first));
    assert_eq!(conflict_windows(), 1);
    hotkey_dialog::close_conflict();
    assert_eq!(hotkey_dialog::conflict_window(), None);
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn win_v_becoming_active_closes_the_conflict_window() {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::with_paths(dir.path().join("db"), dir.path().join("logs"));
    cfg.open_with_win_v = true;
    cfg.win_v_added_v = true;
    cfg.win_v_explorer = Some(1);
    let config = Rc::new(RefCell::new(cfg));
    let slots = Rc::new(RefCell::new(FakeHotkeySlots::new(false)));
    slots.borrow_mut().reserve_free = false;
    let explorer = Rc::new(Cell::new(Some(1)));
    let runtime = WinVRuntime::with_slots(
        HWND::default(),
        Rc::clone(&config),
        dir.path().join("config.toml"),
        Rc::new(Cell::new(None)),
        Box::new(FakeDisabledHotkeys::with(Some("QRV"))),
        Box::new(Rc::clone(&slots)),
        Box::new(Rc::clone(&explorer)),
    );
    let started = runtime.startup();
    assert_eq!(started.status, WinVStatus::AwaitingRestart);
    assert!(started.reserve_failed, "a reserva está tomada");
    open_conflict(&config, dir.path());
    assert!(hotkey_dialog::conflict_window().is_some());

    // O Explorer reinicia e libera o Win+V com a janela de conflito aberta.
    slots.borrow_mut().win_v_free = true;
    explorer.set(Some(2));
    let reaction = runtime.explorer_returned();

    assert_eq!(reaction.status, WinVStatus::Active);
    assert_eq!(
        hotkey_dialog::conflict_window(),
        None,
        "nenhuma combinação pode ser registrada junto do Win+V"
    );
    assert!(!slots.borrow().both_at_once);
}
