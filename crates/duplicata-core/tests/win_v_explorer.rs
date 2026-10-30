use std::cell::Cell;
use std::time::Duration;

use duplicata_core::settings_text as text;
use duplicata_core::testsupport::{FakeExplorerRestarter, FakeHotkeySlots};
use duplicata_core::win_v::{
    explorer_returned, restart_error_line, run_explorer_restart, ExplorerRestartOutcome,
    WinVConfig, WinVStatus, EXPLORER_RETURN_TIMEOUT,
};
use duplicata_core::HotkeyCombo;

const RESERVE: HotkeyCombo = HotkeyCombo::DEFAULT;
const EXPLORER_A: Option<u64> = Some(1_000);
const EXPLORER_B: Option<u64> = Some(2_000);
const EXPLORER_C: Option<u64> = Some(3_000);

fn on(marker: Option<u64>) -> WinVConfig {
    WinVConfig {
        on: true,
        v_ours: true,
        marker,
    }
}

mod explorer_came_back {
    use super::*;

    #[test]
    fn awaiting_with_win_v_now_free_becomes_active_and_drops_the_reserve() {
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let mut last_seen = EXPLORER_A;
        let r = explorer_returned(
            WinVStatus::AwaitingRestart,
            on(EXPLORER_A),
            RESERVE,
            &mut last_seen,
            EXPLORER_B,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::Active);
        assert!(slots.win_v);
        assert_eq!(slots.reserve, None);
        assert!(!slots.both_at_once);
        assert!(!r.show_failure_window);
        assert_eq!(last_seen, EXPLORER_B);
    }

    #[test]
    fn a_real_restart_without_win_v_is_not_released_and_opens_the_window() {
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let mut last_seen = EXPLORER_A;
        let r = explorer_returned(
            WinVStatus::AwaitingRestart,
            on(EXPLORER_A),
            RESERVE,
            &mut last_seen,
            EXPLORER_B,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::NotReleased);
        assert!(r.show_failure_window);
        assert_eq!(slots.reserve, Some(RESERVE), "a reserva continua");
        assert!(!slots.both_at_once);
    }

    #[test]
    fn the_failure_window_comes_back_on_every_restart() {
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let mut last_seen = EXPLORER_B;
        let r = explorer_returned(
            WinVStatus::NotReleased,
            on(EXPLORER_A),
            RESERVE,
            &mut last_seen,
            EXPLORER_C,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::NotReleased);
        assert!(r.show_failure_window);
    }

    #[test]
    fn a_taskbar_created_without_restart_changes_nothing() {
        for (status, last) in [
            (WinVStatus::AwaitingRestart, EXPLORER_A),
            (WinVStatus::NotReleased, EXPLORER_B),
        ] {
            let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
            let mut last_seen = last;
            let r = explorer_returned(
                status,
                on(EXPLORER_A),
                RESERVE,
                &mut last_seen,
                last,
                &mut slots,
            );
            assert_eq!(r.status, status);
            assert!(!r.show_failure_window, "{status:?}");
            assert_eq!(slots.reserve, Some(RESERVE));
        }
    }

    #[test]
    fn an_unknown_explorer_changes_nothing() {
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let mut last_seen = EXPLORER_A;
        let r = explorer_returned(
            WinVStatus::AwaitingRestart,
            on(EXPLORER_A),
            RESERVE,
            &mut last_seen,
            None,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::AwaitingRestart);
        assert!(!r.show_failure_window);
        assert_eq!(last_seen, EXPLORER_A, "o último visto não se perde");
    }

    #[test]
    fn win_v_registered_closes_an_open_failure_window() {
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let mut last_seen = EXPLORER_B;
        let r = explorer_returned(
            WinVStatus::NotReleased,
            on(EXPLORER_A),
            RESERVE,
            &mut last_seen,
            EXPLORER_C,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::Active);
        assert!(r.close_failure_window);
    }

    #[test]
    fn off_awaiting_return_becomes_off_when_another_explorer_shows_up() {
        let saved = WinVConfig {
            on: false,
            v_ours: false,
            marker: EXPLORER_A,
        };
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let mut last_seen = EXPLORER_A;
        let r = explorer_returned(
            WinVStatus::OffAwaitingReturn,
            saved,
            RESERVE,
            &mut last_seen,
            EXPLORER_B,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::Off);
        assert_eq!(r.config, WinVConfig::default(), "marcador limpo");
        assert!(!slots.win_v, "desligada nunca tenta o Win+V");
    }

    #[test]
    fn off_awaiting_return_with_the_same_explorer_changes_nothing() {
        let saved = WinVConfig {
            on: false,
            v_ours: false,
            marker: EXPLORER_A,
        };
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let mut last_seen = EXPLORER_A;
        let r = explorer_returned(
            WinVStatus::OffAwaitingReturn,
            saved,
            RESERVE,
            &mut last_seen,
            EXPLORER_A,
            &mut slots,
        );
        assert_eq!(r.status, WinVStatus::OffAwaitingReturn);
        assert_eq!(r.config, saved);
    }

    #[test]
    fn active_and_off_ignore_the_explorer() {
        for status in [WinVStatus::Active, WinVStatus::Off] {
            let mut slots = FakeHotkeySlots::new(true);
            let mut last_seen = EXPLORER_A;
            let r = explorer_returned(
                status,
                on(EXPLORER_A),
                RESERVE,
                &mut last_seen,
                EXPLORER_B,
                &mut slots,
            );
            assert_eq!(r.status, status);
            assert_eq!(slots.win_v_attempts, 0, "{status:?}");
        }
    }
}

mod restart_button {
    use super::*;

    #[test]
    fn the_wait_is_ten_seconds() {
        assert_eq!(EXPLORER_RETURN_TIMEOUT, Duration::from_secs(10));
    }

    #[test]
    fn a_refusal_changes_nothing_and_never_waits() {
        let mut restarter = FakeExplorerRestarter {
            refuse: true,
            ..Default::default()
        };
        let out = run_explorer_restart(
            &mut restarter,
            |_| panic!("não espera depois de uma recusa"),
            || panic!("não confere o shell"),
            || panic!("não relança"),
        );
        assert_eq!(out, ExplorerRestartOutcome::Refused);
        assert_eq!(restarter.restarts, 0);
    }

    #[test]
    fn the_explorer_coming_back_in_time_is_returned_without_relaunch() {
        let mut restarter = FakeExplorerRestarter::default();
        let waited = Cell::new(None);
        let out = run_explorer_restart(
            &mut restarter,
            |timeout| {
                waited.set(Some(timeout));
                true
            },
            || panic!("não confere o shell"),
            || panic!("não relança"),
        );
        assert_eq!(out, ExplorerRestartOutcome::Returned);
        assert_eq!(waited.get(), Some(EXPLORER_RETURN_TIMEOUT));
        assert_eq!((restarter.shutdowns, restarter.restarts), (1, 1));
    }

    #[test]
    fn a_shell_present_after_the_wait_is_not_relaunched() {
        let mut restarter = FakeExplorerRestarter::default();
        let out = run_explorer_restart(
            &mut restarter,
            |_| false,
            || true,
            || panic!("não relança com shell presente"),
        );
        assert_eq!(out, ExplorerRestartOutcome::Returned);
    }

    #[test]
    fn no_shell_after_the_wait_relaunches() {
        let mut restarter = FakeExplorerRestarter::default();
        let out = run_explorer_restart(&mut restarter, |_| false, || false, || true);
        assert_eq!(out, ExplorerRestartOutcome::NotReturnedRelaunched);
        let out = run_explorer_restart(&mut restarter, |_| false, || false, || false);
        assert_eq!(out, ExplorerRestartOutcome::NotReturnedRelaunchFailed);
    }

    #[test]
    fn each_outcome_has_its_error_line() {
        assert_eq!(restart_error_line(ExplorerRestartOutcome::Returned), None);
        assert_eq!(
            restart_error_line(ExplorerRestartOutcome::Refused),
            Some(
                "Não foi possível reiniciar o Windows Explorer. Reinicie pelo Gerenciador de \
                 Tarefas ou reinicie o computador."
            )
        );
        assert_eq!(
            restart_error_line(ExplorerRestartOutcome::NotReturnedRelaunched),
            Some(text::EXPLORER_RESTART_FAILED)
        );
        assert_eq!(
            restart_error_line(ExplorerRestartOutcome::NotReturnedRelaunchFailed),
            Some(
                "O Windows Explorer não voltou. Abra o Gerenciador de Tarefas \
                 (Ctrl+Shift+Esc), Executar nova tarefa, e digite explorer."
            )
        );
    }
}

mod failure_window {
    use duplicata_core::settings_text as text;
    use duplicata_core::win_v::turned_off_notice;

    #[test]
    fn turning_off_our_v_says_it_returns_after_a_restart() {
        assert_eq!(turned_off_notice(true), text::WIN_V_RETURNS_AFTER);
    }

    #[test]
    fn turning_off_a_v_that_was_not_ours_says_it_stays_disabled() {
        assert_eq!(turned_off_notice(false), text::WIN_V_WAS_DISABLED_BEFORE);
    }
}
