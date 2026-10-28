#![cfg(windows)]

use duplicata_win::hotkey_capture::{arm, disarm, is_armed, owner, release, CaptureOwner};

#[test]
fn at_rest_nothing_is_armed_and_nobody_owns_it() {
    assert!(!is_armed());
    assert_eq!(owner(), CaptureOwner::None);
}

#[test]
fn arm_disarm_release_is_the_full_lifecycle() {
    arm(CaptureOwner::SettingsSection);
    assert!(is_armed());
    assert_eq!(owner(), CaptureOwner::SettingsSection);

    disarm();
    assert!(!is_armed());
    assert_eq!(owner(), CaptureOwner::SettingsSection);

    release(CaptureOwner::SettingsSection);
    assert!(!is_armed());
    assert_eq!(owner(), CaptureOwner::None);
}

#[test]
fn owner_is_exclusive_while_armed() {
    arm(CaptureOwner::StandaloneDialog);
    assert_eq!(owner(), CaptureOwner::StandaloneDialog);
    assert_ne!(owner(), CaptureOwner::SettingsSection);
    release(CaptureOwner::StandaloneDialog);
}

#[test]
fn owner_reverts_to_none_after_destruction_regardless_of_which_exit_path() {
    for _ in 0..4 {
        arm(CaptureOwner::StandaloneDialog);
        release(CaptureOwner::StandaloneDialog);
        assert_eq!(owner(), CaptureOwner::None);
        assert!(!is_armed());
    }
}

#[test]
fn a_release_that_does_not_match_the_current_owner_is_a_no_op() {
    arm(CaptureOwner::SettingsSection);
    release(CaptureOwner::StandaloneDialog);
    assert_eq!(owner(), CaptureOwner::SettingsSection);
    assert!(is_armed());
    release(CaptureOwner::SettingsSection);
}

mod troca_do_atalho {
    use duplicata_core::{HotkeyCombo, MOD_ALT, MOD_CONTROL, MOD_SHIFT};
    use duplicata_win::hotkey_dialog::{probe_hotkey, swap_hotkey, OnSwapFailure};
    use duplicata_win::hotkey_win;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WS_POPUP,
    };

    const ALL: u32 = MOD_CONTROL | MOD_ALT | MOD_SHIFT;
    /// F20 a F22: combinações que nenhum programa costuma usar.
    const X: HotkeyCombo = HotkeyCombo {
        modifiers: ALL,
        vkey: 0x83,
    };
    const Y: HotkeyCombo = HotkeyCombo {
        modifiers: ALL,
        vkey: 0x84,
    };
    const Z: HotkeyCombo = HotkeyCombo {
        modifiers: ALL,
        vkey: 0x85,
    };

    fn window() -> HWND {
        // SAFETY: classe do sistema, janela message-only só para registrar
        // atalhos; destruída no fim de cada teste.
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("STATIC"),
                windows::core::w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .expect("CreateWindowExW(STATIC) falhou")
    }

    fn destroy(hwnds: &[HWND]) {
        for h in hwnds {
            let _ = hotkey_win::unregister(*h);
            // SAFETY: janelas criadas por `window()` neste teste.
            let _ = unsafe { DestroyWindow(*h) };
        }
    }

    fn is_free(probe: HWND, combo: HotkeyCombo) -> bool {
        let free = hotkey_win::register(probe, &combo).is_ok();
        if free {
            let _ = hotkey_win::unregister(probe);
        }
        free
    }

    #[test]
    #[ignore = "registra atalhos globais reais na sessão"]
    fn probing_tells_a_taken_combination_apart_without_keeping_it() {
        let (probe, other) = (window(), window());
        hotkey_win::register(other, &X).expect("X deveria estar livre");
        assert!(probe_hotkey(probe, None, X).is_err(), "X está tomada");
        assert_eq!(probe_hotkey(probe, None, Y), Ok(()));
        assert!(is_free(probe, Y), "a sondagem não fica com Y");
        destroy(&[probe, other]);
    }

    #[test]
    #[ignore = "registra atalhos globais reais na sessão"]
    fn swapping_moves_the_registration_and_a_failure_keeps_or_drops_the_previous() {
        let (history, other, probe) = (window(), window(), window());
        hotkey_win::register(history, &X).expect("X deveria estar livre");
        hotkey_win::register(other, &Z).expect("Z deveria estar livre");

        assert_eq!(
            swap_hotkey(history, Some(X), Y, OnSwapFailure::KeepPrevious),
            Ok(())
        );
        assert!(is_free(probe, X), "a anterior foi solta");
        assert!(!is_free(probe, Y), "a nova ficou registrada");

        assert!(swap_hotkey(history, Some(Y), Z, OnSwapFailure::KeepPrevious).is_err());
        assert!(!is_free(probe, Y), "falhou: Y continua");

        assert!(swap_hotkey(history, Some(Y), Z, OnSwapFailure::LeaveNone).is_err());
        assert!(is_free(probe, Y), "falhou no Cancelar: nenhuma fica");
        destroy(&[history, other, probe]);
    }
}

mod gravacao_do_atalho {
    use std::path::PathBuf;
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

    fn read(path: &PathBuf, dir: &std::path::Path) -> Config {
        let text = std::fs::read_to_string(path).expect("config.toml gravado");
        Config::from_str_over(base(dir), &text).0
    }

    #[test]
    fn the_conflict_dialog_writes_only_the_hotkey_over_the_saved_values() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut saved = base(dir.path());
        saved.retention = Duration::from_secs(7 * 86_400);
        saved.persist(&path).unwrap();

        // Em memória, o que as Configurações só aplicaram.
        let mut shared = saved.clone();
        shared.blocked_programs = vec!["keepass.exe".into()];
        shared.retention = Duration::from_secs(30 * 86_400);
        shared.heuristic_secret_detection = false;

        persist_hotkey_only(&path, &shared, NEW).unwrap();
        let on_disk = read(&path, dir.path());
        assert_eq!(on_disk.hotkey, NEW);
        assert!(
            on_disk.blocked_programs.is_empty(),
            "o aplicado não vai ao disco"
        );
        assert_eq!(on_disk.retention, Duration::from_secs(7 * 86_400));
        assert!(on_disk.heuristic_secret_detection);
    }

    #[test]
    fn without_a_file_the_hotkey_is_written_over_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut shared = base(dir.path());
        shared.blocked_programs = vec!["keepass.exe".into()];

        persist_hotkey_only(&path, &shared, NEW).unwrap();
        let on_disk = read(&path, dir.path());
        assert_eq!(on_disk.hotkey, NEW);
        assert!(on_disk.blocked_programs.is_empty());
    }
}

mod troca_pelo_dialogo_de_conflito {
    use std::cell::RefCell;
    use std::rc::Rc;
    use std::time::Duration;

    use duplicata_core::{Config, HotkeyCombo, MOD_ALT, MOD_CONTROL, MOD_SHIFT};
    use duplicata_win::hotkey_dialog::{try_exchange, ExchangeOutcome};
    use duplicata_win::hotkey_win;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, HWND_MESSAGE, WINDOW_EX_STYLE, WS_POPUP,
    };

    /// F23: fora das usadas pelos outros testes deste arquivo.
    const NEW: HotkeyCombo = HotkeyCombo {
        modifiers: MOD_CONTROL | MOD_ALT | MOD_SHIFT,
        vkey: 0x86,
    };

    fn window() -> HWND {
        // SAFETY: classe do sistema, janela message-only só para registrar
        // atalhos; destruída no fim do teste.
        unsafe {
            CreateWindowExW(
                WINDOW_EX_STYLE(0),
                windows::core::w!("STATIC"),
                windows::core::w!(""),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(HWND_MESSAGE),
                None,
                None,
                None,
            )
        }
        .expect("CreateWindowExW(STATIC) falhou")
    }

    #[test]
    #[ignore = "registra atalhos globais reais na sessão"]
    fn resolving_the_conflict_writes_only_the_hotkey_not_what_settings_applied() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut saved = Config::with_paths(dir.path().join("db"), dir.path().join("logs"));
        saved.retention = Duration::from_secs(7 * 86_400);
        saved.persist(&path).unwrap();
        // Em memória, o que as Configurações só aplicaram.
        let mut applied = saved.clone();
        applied.blocked_programs = vec!["keepass.exe".into()];
        applied.retention = Duration::from_secs(30 * 86_400);
        let shared = Rc::new(RefCell::new(applied));

        let (probe, history) = (window(), window());
        let outcome = try_exchange(probe, history, None, NEW, &shared, &path);
        let _ = hotkey_win::unregister(history);
        // SAFETY: janelas criadas por `window()` neste teste.
        unsafe {
            let _ = DestroyWindow(probe);
            let _ = DestroyWindow(history);
        }
        assert!(matches!(outcome, ExchangeOutcome::Accepted(c) if c == NEW));
        assert_eq!(
            shared.borrow().hotkey,
            NEW,
            "a memória passa a ter o atalho novo"
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let on_disk = Config::from_str_over(saved.clone(), &text).0;
        assert_eq!(on_disk.hotkey, NEW);
        assert!(
            on_disk.blocked_programs.is_empty(),
            "o aplicado não vai ao disco"
        );
        assert_eq!(on_disk.retention, Duration::from_secs(7 * 86_400));
    }
}
