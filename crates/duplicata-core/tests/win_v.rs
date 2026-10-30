use duplicata_core::testsupport::{FakeDisabledHotkeys, FakeHotkeySlots};
use duplicata_core::win_v::{
    add_v, disable, enable, remove_v, startup, AddV, RegistryError, RemoveV, WinVConfig, WinVStatus,
};
use duplicata_core::HotkeyCombo;

const RESERVE: HotkeyCombo = HotkeyCombo::DEFAULT;
const EXPLORER_A: Option<u64> = Some(1_000);
const EXPLORER_B: Option<u64> = Some(2_000);

fn on(v_ours: bool, marker: Option<u64>) -> WinVConfig {
    WinVConfig {
        on: true,
        v_ours,
        marker,
    }
}

mod value_rule {
    use super::*;

    #[test]
    fn adding_keeps_the_other_characters_and_appends_v() {
        assert_eq!(add_v(Some("QR")), AddV::Write("QRV".into()));
    }

    #[test]
    fn adding_to_a_missing_value_creates_it_with_only_v() {
        assert_eq!(add_v(None), AddV::Write("V".into()));
    }

    #[test]
    fn an_existing_v_in_either_case_is_already_present() {
        for value in ["V", "QVR", "v", "qrv"] {
            assert_eq!(add_v(Some(value)), AddV::AlreadyPresent, "{value:?}");
        }
    }

    #[test]
    fn removing_takes_the_v_from_any_position_and_keeps_the_rest() {
        for value in ["QRV", "VQR", "QVR"] {
            assert_eq!(
                remove_v(Some(value)),
                RemoveV::Write("QR".into()),
                "{value:?}"
            );
        }
    }

    #[test]
    fn removing_the_only_character_deletes_the_value() {
        assert_eq!(remove_v(Some("V")), RemoveV::Delete);
    }

    #[test]
    fn removing_without_an_uppercase_v_does_nothing() {
        assert_eq!(remove_v(None), RemoveV::Nothing);
        assert_eq!(remove_v(Some("QR")), RemoveV::Nothing);
        assert_eq!(remove_v(Some("qv")), RemoveV::Nothing);
    }
}

mod at_startup {
    use super::*;

    #[test]
    fn off_without_a_marker_registers_only_the_reserve() {
        let store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::new(true);
        let out = startup(
            WinVConfig::default(),
            RESERVE,
            &store,
            &mut slots,
            EXPLORER_A,
        );
        assert_eq!(out.status, WinVStatus::Off);
        assert_eq!(out.config, WinVConfig::default());
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!slots.win_v, "desligada nunca tenta o Win+V");
        assert!(!out.show_failure_window);
    }

    #[test]
    fn on_without_v_in_the_registry_turns_off_and_touches_nothing() {
        for value in [None, Some("QR")] {
            let store = FakeDisabledHotkeys::with(value);
            let mut slots = FakeHotkeySlots::new(true);
            let out = startup(
                on(true, EXPLORER_A),
                RESERVE,
                &store,
                &mut slots,
                EXPLORER_A,
            );
            assert_eq!(out.status, WinVStatus::Off, "{value:?}");
            assert_eq!(out.config, WinVConfig::default(), "grava a opção desligada");
            assert_eq!(slots.reserve, Some(RESERVE));
            assert!(!slots.win_v);
            assert_eq!(store.writes, 0, "registro intocado");
        }
    }

    #[test]
    fn an_unreadable_registry_does_not_turn_off_and_still_tries_win_v() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        store.fail_read = true;
        let mut slots = FakeHotkeySlots::new(true);
        let out = startup(
            on(true, EXPLORER_A),
            RESERVE,
            &store,
            &mut slots,
            EXPLORER_B,
        );
        assert_eq!(out.status, WinVStatus::Active);
        assert!(out.config.on);
    }

    #[test]
    fn win_v_registered_uses_only_win_v() {
        let store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::new(true);
        let out = startup(
            on(true, EXPLORER_A),
            RESERVE,
            &store,
            &mut slots,
            EXPLORER_B,
        );
        assert_eq!(out.status, WinVStatus::Active);
        assert!(slots.win_v);
        assert_eq!(slots.reserve, None, "nunca dois atalhos ativos");
        assert!(!slots.both_at_once);
        assert!(!out.show_failure_window);
    }

    #[test]
    fn same_explorer_as_the_save_waits_without_a_window() {
        let store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::new(false);
        let out = startup(
            on(true, EXPLORER_A),
            RESERVE,
            &store,
            &mut slots,
            EXPLORER_A,
        );
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!out.show_failure_window);
        assert!(!slots.both_at_once);
    }

    #[test]
    fn another_explorer_since_the_save_is_not_released_with_the_window() {
        let store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::new(false);
        let out = startup(
            on(true, EXPLORER_A),
            RESERVE,
            &store,
            &mut slots,
            EXPLORER_B,
        );
        assert_eq!(out.status, WinVStatus::NotReleased);
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(out.show_failure_window);
        assert_eq!(
            out.config,
            on(true, EXPLORER_A),
            "o marcador do Salvar fica"
        );
    }

    #[test]
    fn an_unknown_explorer_is_treated_as_awaiting_without_a_window() {
        for (saved, now) in [(None, EXPLORER_A), (EXPLORER_A, None)] {
            let store = FakeDisabledHotkeys::with(Some("V"));
            let mut slots = FakeHotkeySlots::new(false);
            let out = startup(on(true, saved), RESERVE, &store, &mut slots, now);
            assert_eq!(out.status, WinVStatus::AwaitingRestart, "{saved:?} {now:?}");
            assert!(!out.show_failure_window);
        }
    }

    #[test]
    fn a_taken_reserve_is_reported_for_the_conflict_window() {
        let store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::new(false);
        slots.reserve_free = false;
        let out = startup(
            on(true, EXPLORER_A),
            RESERVE,
            &store,
            &mut slots,
            EXPLORER_B,
        );
        assert!(out.show_failure_window);
        assert!(out.reserve_failed);
    }

    #[test]
    fn off_awaiting_return_survives_a_restart_of_the_app_with_the_same_explorer() {
        let store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::new(true);
        let saved = WinVConfig {
            on: false,
            v_ours: false,
            marker: EXPLORER_A,
        };
        let out = startup(saved, RESERVE, &store, &mut slots, EXPLORER_A);
        assert_eq!(out.status, WinVStatus::OffAwaitingReturn);
        assert_eq!(out.config, saved);
        assert_eq!(slots.reserve, Some(RESERVE));
    }

    #[test]
    fn off_with_another_explorer_since_the_save_clears_the_marker() {
        let store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::new(true);
        let saved = WinVConfig {
            on: false,
            v_ours: false,
            marker: EXPLORER_A,
        };
        let out = startup(saved, RESERVE, &store, &mut slots, EXPLORER_B);
        assert_eq!(out.status, WinVStatus::Off);
        assert_eq!(out.config, WinVConfig::default());
    }
}

mod turning_on {
    use super::*;

    #[test]
    fn writes_v_keeping_the_others_and_waits_with_the_reserve() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let out = enable(RESERVE, &mut store, &mut slots, EXPLORER_A).unwrap();
        assert_eq!(store.value.as_deref(), Some("QRV"));
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
        assert_eq!(out.config, on(true, EXPLORER_A));
        assert_eq!(
            slots.reserve,
            Some(RESERVE),
            "a reserva continua registrada"
        );
        assert!(!slots.win_v);
    }

    #[test]
    fn a_missing_value_is_created_with_v() {
        let mut store = FakeDisabledHotkeys::with(None);
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        enable(RESERVE, &mut store, &mut slots, EXPLORER_A).unwrap();
        assert_eq!(store.value.as_deref(), Some("V"));
    }

    #[test]
    fn a_write_failure_changes_nothing() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        store.fail_write = true;
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        assert_eq!(
            enable(RESERVE, &mut store, &mut slots, EXPLORER_A),
            Err(RegistryError(5))
        );
        assert_eq!(store.value.as_deref(), Some("QR"));
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!slots.win_v);
    }

    #[test]
    fn an_existing_v_is_not_ours_and_win_v_is_tried_at_once() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let out = enable(RESERVE, &mut store, &mut slots, EXPLORER_A).unwrap();
        assert_eq!(store.writes, 0, "nada gravado");
        assert_eq!(out.status, WinVStatus::Active);
        assert_eq!(out.config, on(false, EXPLORER_A));
        assert!(slots.win_v);
        assert_eq!(slots.reserve, None, "solta a reserva");
        assert!(!slots.both_at_once);
    }

    #[test]
    fn an_existing_v_without_win_v_free_keeps_the_reserve_and_waits() {
        let mut store = FakeDisabledHotkeys::with(Some("v"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let out = enable(RESERVE, &mut store, &mut slots, EXPLORER_A).unwrap();
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
        assert!(!out.config.v_ours);
        assert_eq!(slots.reserve, Some(RESERVE));
        assert_eq!(store.writes, 0);
    }
}

mod turning_off {
    use super::*;

    #[test]
    fn our_v_is_removed_and_win_v_gives_way_to_the_reserve_at_once() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::holding_win_v();
        let out = disable(
            on(true, EXPLORER_A),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            EXPLORER_B,
        )
        .unwrap();
        assert_eq!(store.value.as_deref(), Some("QR"));
        assert!(!slots.win_v);
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!slots.both_at_once);
        assert_eq!(out.status, WinVStatus::OffAwaitingReturn);
        assert_eq!(
            out.config,
            WinVConfig {
                on: false,
                v_ours: false,
                marker: EXPLORER_B
            }
        );
    }

    #[test]
    fn our_v_alone_deletes_the_value() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_win_v();
        disable(
            on(true, EXPLORER_A),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            EXPLORER_B,
        )
        .unwrap();
        assert_eq!(store.value, None);
    }

    #[test]
    fn a_v_that_was_not_ours_stays_and_the_option_is_simply_off() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_win_v();
        let out = disable(
            on(false, EXPLORER_A),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            EXPLORER_A,
        )
        .unwrap();
        assert_eq!(store.value.as_deref(), Some("V"));
        assert_eq!(store.writes, 0);
        assert_eq!(out.status, WinVStatus::Off);
        assert_eq!(out.config, WinVConfig::default());
        assert_eq!(slots.reserve, Some(RESERVE));
    }

    #[test]
    fn without_an_explorer_restart_since_turning_on_it_goes_straight_to_off() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let out = disable(
            on(true, EXPLORER_A),
            WinVStatus::AwaitingRestart,
            RESERVE,
            &mut store,
            &mut slots,
            EXPLORER_A,
        )
        .unwrap();
        assert_eq!(store.value.as_deref(), Some("QR"));
        assert_eq!(out.status, WinVStatus::Off);
        assert_eq!(out.config, WinVConfig::default());
        assert_eq!(slots.reserve, Some(RESERVE), "a reserva continua");
    }

    #[test]
    fn a_write_failure_keeps_the_hotkey_in_use() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        store.fail_write = true;
        let mut slots = FakeHotkeySlots::holding_win_v();
        let result = disable(
            on(true, EXPLORER_A),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            EXPLORER_B,
        );
        assert_eq!(result, Err(RegistryError(5)));
        assert!(slots.win_v, "o Win+V continua");
        assert_eq!(slots.reserve, None);
        assert_eq!(store.value.as_deref(), Some("QRV"));
    }

    #[test]
    fn a_reserve_taken_by_another_program_leaves_no_hotkey() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::holding_win_v();
        slots.reserve_free = false;
        let out = disable(
            on(true, EXPLORER_A),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            EXPLORER_B,
        )
        .unwrap();
        assert!(out.reserve_failed);
        assert!(!slots.win_v);
        assert_eq!(slots.reserve, None);
    }
}
