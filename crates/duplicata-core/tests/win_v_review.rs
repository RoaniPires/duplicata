//! Regras da primeira rodada de revisão: só um atalho com a janela de
//! conflito aberta, o `V` que não fica para trás quando o `config.toml` não
//! é gravado e as mensagens de erro que não podem ser só log.

use duplicata_core::settings_text as text;
use duplicata_core::testsupport::{FakeDisabledHotkeys, FakeHotkeySlots};
use duplicata_core::win_v::{
    enable, turned_off_error, undo_unsaved_enable, uninstall_release_message, RegistryError,
    UninstallRelease, WinVConfig, WinVStatus,
};
use duplicata_core::HotkeyCombo;

const RESERVE: HotkeyCombo = HotkeyCombo::DEFAULT;
const A: Option<u64> = Some(1_000);

mod one_hotkey {
    use super::*;

    #[test]
    fn only_win_v_in_use_stops_using_the_reserve() {
        assert!(!WinVStatus::Active.uses_reserve());
        for status in [
            WinVStatus::Off,
            WinVStatus::AwaitingRestart,
            WinVStatus::NotReleased,
            WinVStatus::OffAwaitingReturn,
        ] {
            assert!(status.uses_reserve(), "{status:?}");
        }
    }
}

mod unsaved_enable {
    use super::*;

    #[test]
    fn saving_only_the_win_v_fields_keeps_the_option_on() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        let mut saved = None;
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |c| {
            saved = Some(c);
            true
        });
        assert_eq!(undo, None, "gravado: nada a desfazer");
        assert_eq!(saved, Some(enabled.config), "grava os três campos");
        assert_eq!(store.value.as_deref(), Some("QRV"));
    }

    #[test]
    fn without_saving_at_all_our_v_is_taken_back_and_the_option_turns_off() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false)
            .expect("desfeito");
        assert_eq!(store.value.as_deref(), Some("QR"), "o V do duplicata sai");
        assert_eq!(undo.status, WinVStatus::Off);
        assert_eq!(undo.config, WinVConfig::default());
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!slots.both_at_once);
    }

    #[test]
    fn our_v_alone_is_deleted_when_undoing() {
        let mut store = FakeDisabledHotkeys::with(None);
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert_eq!(store.value.as_deref(), Some("V"));
        undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false);
        assert_eq!(store.value, None);
    }

    #[test]
    fn a_foreign_v_stays_and_win_v_gives_way_to_the_reserve() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert_eq!(enabled.status, WinVStatus::Active);
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false)
            .expect("desfeito");
        assert_eq!(store.value.as_deref(), Some("V"), "V alheio intocado");
        assert_eq!(store.writes, 0);
        assert!(!slots.win_v);
        assert_eq!(slots.reserve, Some(RESERVE));
        assert_eq!(undo.status, WinVStatus::Off);
        assert!(!slots.both_at_once);
    }

    #[test]
    fn a_taken_reserve_while_undoing_is_reported() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        slots.reserve_free = false;
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false)
            .expect("desfeito");
        assert!(undo.reserve_failed);
    }
}

mod visible_errors {
    use super::*;

    #[test]
    fn turning_off_without_saving_the_config_shows_the_save_failure() {
        assert_eq!(turned_off_error(true), None);
        assert_eq!(turned_off_error(false), Some(text::SAVE_FAILED));
    }

    #[test]
    fn an_uninstall_that_could_not_take_the_v_back_says_how_to_do_it_by_hand() {
        assert_eq!(uninstall_release_message(UninstallRelease::Untouched), None);
        assert_eq!(uninstall_release_message(UninstallRelease::Removed), None);
        assert_eq!(
            uninstall_release_message(UninstallRelease::Failed(RegistryError(5))),
            Some(text::WIN_V_UNINSTALL_FAILED)
        );
    }

    #[test]
    fn the_uninstall_message_points_to_the_manual_removal() {
        assert_eq!(
            text::WIN_V_UNINSTALL_FAILED,
            "Não foi possível devolver o Win+V ao Windows: ele continua desativado no valor \
             DisabledHotkeys do Windows Explorer. Para removê-lo à mão, veja a seção \"Abrir \
             com Win+V\" do README do duplicata (github.com/RoaniPires/duplicata)."
        );
    }
}

mod conflict_after_return {
    use duplicata_core::win_v::{open_conflict_after_return, Reaction, WinVConfig, WinVStatus};

    fn reaction(reserve_failed: bool, show_failure_window: bool) -> Reaction {
        Reaction {
            status: WinVStatus::AwaitingRestart,
            config: WinVConfig::default(),
            show_failure_window,
            close_failure_window: false,
            reserve_failed,
        }
    }

    #[test]
    fn a_taken_reserve_opens_the_conflict_window_when_none_is_open() {
        assert!(open_conflict_after_return(
            &reaction(true, false),
            false,
            false
        ));
    }

    #[test]
    fn an_open_conflict_window_is_left_alone_without_stealing_the_focus() {
        assert!(!open_conflict_after_return(
            &reaction(true, false),
            false,
            true
        ));
    }

    #[test]
    fn the_failure_window_opens_it_later_instead() {
        assert!(!open_conflict_after_return(
            &reaction(true, true),
            false,
            false
        ));
        assert!(!open_conflict_after_return(
            &reaction(true, false),
            true,
            false
        ));
    }

    #[test]
    fn a_registered_reserve_needs_no_conflict_window() {
        assert!(!open_conflict_after_return(
            &reaction(false, false),
            false,
            false
        ));
    }
}
