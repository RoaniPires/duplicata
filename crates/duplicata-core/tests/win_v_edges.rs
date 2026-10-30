//! Bordas do Win+V que os outros arquivos `win_v*` não exercitam.

use duplicata_core::testsupport::{FakeDisabledHotkeys, FakeHotkeySlots};
use duplicata_core::win_v::{
    add_v, disable, enable, explorer_returned, remove_v, startup, uninstall_release, AddV,
    RegistryError, RemoveV, UninstallRelease, WinVConfig, WinVStatus,
};
use duplicata_core::HotkeyCombo;

const RESERVE: HotkeyCombo = HotkeyCombo::DEFAULT;
const A: Option<u64> = Some(1_000);
const B: Option<u64> = Some(2_000);

mod value_rule_edges {
    use super::*;

    #[test]
    fn an_empty_string_gets_a_v_appended() {
        assert_eq!(add_v(Some("")), AddV::Write("V".into()));
    }

    #[test]
    fn removing_from_an_empty_string_does_nothing() {
        assert_eq!(remove_v(Some("")), RemoveV::Nothing);
    }

    #[test]
    fn non_ascii_characters_are_kept_when_adding_and_removing() {
        assert_eq!(add_v(Some("é")), AddV::Write("éV".into()));
        assert_eq!(remove_v(Some("éVã")), RemoveV::Write("éã".into()));
        assert_eq!(remove_v(Some("Vé")), RemoveV::Write("é".into()));
    }

    #[test]
    fn removing_takes_only_one_v_when_there_are_several() {
        assert_eq!(remove_v(Some("VV")), RemoveV::Write("V".into()));
        assert_eq!(remove_v(Some("QVRV")), RemoveV::Write("QRV".into()));
    }

    #[test]
    fn removing_with_both_cases_takes_the_uppercase_and_keeps_the_lowercase() {
        assert_eq!(remove_v(Some("vV")), RemoveV::Write("v".into()));
        assert_eq!(remove_v(Some("Vv")), RemoveV::Write("v".into()));
    }

    #[test]
    fn adding_with_both_cases_or_several_vs_is_already_present() {
        for value in ["vV", "VV", "vv"] {
            assert_eq!(add_v(Some(value)), AddV::AlreadyPresent, "{value:?}");
        }
    }
}

mod startup_invariants {
    use super::*;

    #[test]
    fn every_combination_ends_with_at_most_one_hotkey_and_an_untouched_registry() {
        let values = [None, Some(""), Some("QR"), Some("QRV"), Some("v")];
        let markers = [None, A, B];
        let nows = [None, A, B];
        let mut checked = 0;
        for on in [false, true] {
            for v_ours in [false, true] {
                for marker in markers {
                    for now in nows {
                        for value in values {
                            for win_v_free in [false, true] {
                                for reserve_free in [false, true] {
                                    for fail_read in [false, true] {
                                        let mut store = FakeDisabledHotkeys::with(value);
                                        store.fail_read = fail_read;
                                        let mut slots = FakeHotkeySlots::new(win_v_free);
                                        slots.reserve_free = reserve_free;
                                        let saved = WinVConfig { on, v_ours, marker };
                                        let out = startup(saved, RESERVE, &store, &mut slots, now);
                                        let ctx = format!(
                                            "{saved:?} {value:?} now={now:?} wv={win_v_free} \
                                             res={reserve_free} fail_read={fail_read}"
                                        );
                                        assert!(!slots.both_at_once, "dois atalhos: {ctx}");
                                        assert_eq!(store.writes, 0, "registro tocado: {ctx}");
                                        if out.status == WinVStatus::Active {
                                            assert!(
                                                slots.win_v && slots.reserve.is_none(),
                                                "{ctx}"
                                            );
                                            assert!(out.config.on, "{ctx}");
                                        } else {
                                            assert!(!slots.win_v, "{ctx}");
                                            assert_eq!(
                                                slots.reserve.is_some(),
                                                reserve_free,
                                                "reserva: {ctx}"
                                            );
                                            assert_eq!(out.reserve_failed, !reserve_free, "{ctx}");
                                        }
                                        if out.show_failure_window {
                                            assert_eq!(
                                                out.status,
                                                WinVStatus::NotReleased,
                                                "{ctx}"
                                            );
                                        }
                                        if !on {
                                            assert!(!out.config.on && !out.show_failure_window);
                                            assert_eq!(slots.win_v_attempts, 0, "{ctx}");
                                        }
                                        checked += 1;
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        assert_eq!(checked, 2 * 2 * 3 * 3 * 5 * 2 * 2 * 2);
    }

    #[test]
    fn an_empty_value_or_a_lowercase_v_is_read_like_the_spec_says() {
        // "" não tem V: a opção ligada vira desligada e o Win+V nem é tentado.
        let store = FakeDisabledHotkeys::with(Some(""));
        let mut slots = FakeHotkeySlots::new(true);
        let saved = WinVConfig {
            on: true,
            v_ours: true,
            marker: A,
        };
        let out = startup(saved, RESERVE, &store, &mut slots, A);
        assert_eq!(out.status, WinVStatus::Off);
        assert_eq!(slots.win_v_attempts, 0);

        // `v` minúsculo conta como `V` presente: o Win+V é tentado.
        let store = FakeDisabledHotkeys::with(Some("v"));
        let mut slots = FakeHotkeySlots::new(true);
        let out = startup(saved, RESERVE, &store, &mut slots, B);
        assert_eq!(out.status, WinVStatus::Active);
    }

    #[test]
    fn a_marker_saved_with_an_unknown_explorer_now_waits_without_a_window() {
        let store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::new(false);
        let saved = WinVConfig {
            on: true,
            v_ours: true,
            marker: A,
        };
        let out = startup(saved, RESERVE, &store, &mut slots, None);
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
        assert!(!out.show_failure_window);
        assert_eq!(slots.reserve, Some(RESERVE));
    }
}

mod turning_on_edges {
    use super::*;

    #[test]
    fn an_unreadable_registry_changes_neither_the_registry_nor_the_hotkeys() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        store.fail_read = true;
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let out = enable(RESERVE, &mut store, &mut slots, A);
        assert_eq!(out, Err(RegistryError(5)));
        assert_eq!(store.writes, 0);
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!slots.win_v);
        assert_eq!(slots.win_v_attempts, 0);
    }

    #[test]
    fn a_lowercase_v_already_there_is_not_ours_and_nothing_is_written() {
        let mut store = FakeDisabledHotkeys::with(Some("qv"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let out = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert!(!out.config.v_ours);
        assert_eq!(store.writes, 0);
        assert_eq!(store.value.as_deref(), Some("qv"));
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
        assert_eq!(slots.reserve, Some(RESERVE));
    }

    #[test]
    fn a_foreign_v_with_win_v_free_wins_even_when_the_reserve_was_taken() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::new(true);
        slots.reserve_free = false;
        let out = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert_eq!(out.status, WinVStatus::Active);
        assert!(!out.reserve_failed);
        assert!(slots.win_v && slots.reserve.is_none());
    }

    #[test]
    fn a_foreign_v_with_win_v_and_the_reserve_both_unavailable_reports_the_reserve() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::new(false);
        slots.reserve_free = false;
        let out = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
        assert!(out.reserve_failed);
        assert!(!slots.win_v && slots.reserve.is_none());
    }

    #[test]
    fn an_unknown_explorer_is_saved_as_no_marker() {
        let mut store = FakeDisabledHotkeys::with(None);
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let out = enable(RESERVE, &mut store, &mut slots, None).unwrap();
        assert_eq!(out.config.marker, None);
        assert_eq!(out.status, WinVStatus::AwaitingRestart);
    }
}

mod turning_off_edges {
    use super::*;

    fn ours(marker: Option<u64>) -> WinVConfig {
        WinVConfig {
            on: true,
            v_ours: true,
            marker,
        }
    }

    #[test]
    fn an_unreadable_registry_fails_and_keeps_win_v_and_the_registry() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        store.fail_read = true;
        let mut slots = FakeHotkeySlots::holding_win_v();
        let out = disable(
            ours(A),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            B,
        );
        assert_eq!(out, Err(RegistryError(5)));
        assert!(slots.win_v, "o Win+V em uso não é solto");
        assert_eq!(slots.reserve, None);
        assert_eq!(store.value.as_deref(), Some("QRV"));
    }

    #[test]
    fn a_foreign_v_never_reads_nor_writes_the_registry() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        store.fail_read = true;
        store.fail_write = true;
        let mut slots = FakeHotkeySlots::holding_win_v();
        let saved = WinVConfig {
            on: true,
            v_ours: false,
            marker: A,
        };
        let out = disable(
            saved,
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            B,
        )
        .expect("com V alheio o registro nem é consultado");
        assert_eq!(out.status, WinVStatus::Off);
        assert_eq!(out.config, WinVConfig::default());
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!slots.win_v);
    }

    #[test]
    fn our_v_with_an_unknown_explorer_now_goes_to_awaiting_return_without_marker() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let out = disable(
            ours(A),
            WinVStatus::AwaitingRestart,
            RESERVE,
            &mut store,
            &mut slots,
            None,
        )
        .unwrap();
        assert_eq!(out.status, WinVStatus::OffAwaitingReturn);
        assert_eq!(out.config.marker, None);
        assert_eq!(store.value.as_deref(), Some("QR"));
    }

    #[test]
    fn our_v_with_an_unknown_saved_marker_goes_to_awaiting_return_with_the_current_one() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_win_v();
        let out = disable(
            ours(None),
            WinVStatus::Active,
            RESERVE,
            &mut store,
            &mut slots,
            B,
        )
        .unwrap();
        assert_eq!(out.status, WinVStatus::OffAwaitingReturn);
        assert_eq!(out.config.marker, B);
        assert_eq!(store.value, None, "o V sozinho apaga o valor");
    }

    #[test]
    fn our_v_missing_from_the_registry_is_not_written_back() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let out = disable(
            ours(A),
            WinVStatus::AwaitingRestart,
            RESERVE,
            &mut store,
            &mut slots,
            B,
        )
        .unwrap();
        assert_eq!(store.writes, 0);
        assert_eq!(store.value.as_deref(), Some("QR"));
        assert!(!out.config.on);
    }

    #[test]
    fn disabling_never_leaves_two_hotkeys_registered() {
        for status in [
            WinVStatus::AwaitingRestart,
            WinVStatus::Active,
            WinVStatus::NotReleased,
        ] {
            let mut store = FakeDisabledHotkeys::with(Some("QRV"));
            let mut slots = if status == WinVStatus::Active {
                FakeHotkeySlots::holding_win_v()
            } else {
                FakeHotkeySlots::holding_reserve(false, RESERVE)
            };
            disable(ours(A), status, RESERVE, &mut store, &mut slots, B).unwrap();
            assert!(!slots.both_at_once, "{status:?}");
            assert!(!slots.win_v && slots.reserve == Some(RESERVE), "{status:?}");
        }
    }
}

mod explorer_returned_edges {
    use super::*;

    fn on(marker: Option<u64>) -> WinVConfig {
        WinVConfig {
            on: true,
            v_ours: true,
            marker,
        }
    }

    #[test]
    fn a_repeated_taskbar_created_opens_the_window_only_once_per_restart() {
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let mut last_seen = A;
        let first = explorer_returned(
            WinVStatus::AwaitingRestart,
            on(A),
            RESERVE,
            &mut last_seen,
            B,
            &mut slots,
        );
        assert_eq!(first.status, WinVStatus::NotReleased);
        assert!(first.show_failure_window);
        for _ in 0..3 {
            let again =
                explorer_returned(first.status, on(A), RESERVE, &mut last_seen, B, &mut slots);
            assert_eq!(again.status, WinVStatus::NotReleased);
            assert!(!again.show_failure_window);
            assert!(!again.close_failure_window);
            assert_eq!(slots.reserve, Some(RESERVE));
            assert!(!slots.both_at_once);
        }
    }

    #[test]
    fn a_repeated_taskbar_created_with_win_v_now_free_activates_once_and_stays_active() {
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let mut last_seen = A;
        let first = explorer_returned(
            WinVStatus::AwaitingRestart,
            on(A),
            RESERVE,
            &mut last_seen,
            B,
            &mut slots,
        );
        assert_eq!(first.status, WinVStatus::Active);
        let attempts = slots.win_v_attempts;
        let again = explorer_returned(first.status, on(A), RESERVE, &mut last_seen, B, &mut slots);
        assert_eq!(again.status, WinVStatus::Active);
        assert_eq!(slots.win_v_attempts, attempts, "Ativa não tenta de novo");
        assert!(slots.win_v && slots.reserve.is_none());
    }

    #[test]
    fn an_unknown_explorer_never_creates_a_window_in_any_status() {
        for status in [
            WinVStatus::Off,
            WinVStatus::AwaitingRestart,
            WinVStatus::Active,
            WinVStatus::NotReleased,
            WinVStatus::OffAwaitingReturn,
        ] {
            let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
            let mut last_seen = A;
            let r = explorer_returned(status, on(A), RESERVE, &mut last_seen, None, &mut slots);
            assert!(!r.show_failure_window, "{status:?}");
            assert_eq!(r.config.marker, A, "{status:?}: marcador preservado");
        }
    }

    #[test]
    fn a_taken_reserve_during_a_taskbar_created_is_reported() {
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        slots.reserve_free = false;
        let mut last_seen = A;
        let r = explorer_returned(
            WinVStatus::AwaitingRestart,
            on(A),
            RESERVE,
            &mut last_seen,
            B,
            &mut slots,
        );
        assert!(r.reserve_failed);
        assert!(r.show_failure_window);
    }
}

mod uninstall_edges {
    use super::*;

    const OURS: &str = "open_with_win_v = true\nwin_v_added_v = true\n";

    #[test]
    fn a_lowercase_v_is_not_removed_even_when_the_v_was_ours() {
        let mut store = FakeDisabledHotkeys::with(Some("qv"));
        assert_eq!(
            uninstall_release(Some(OURS), &mut store),
            UninstallRelease::Untouched
        );
        assert_eq!(store.writes, 0);
        assert_eq!(store.value.as_deref(), Some("qv"));
    }

    #[test]
    fn a_missing_value_is_left_missing() {
        let mut store = FakeDisabledHotkeys::with(None);
        assert_eq!(
            uninstall_release(Some(OURS), &mut store),
            UninstallRelease::Untouched
        );
        assert_eq!(store.writes, 0);
    }

    #[test]
    fn an_unreadable_registry_is_reported_as_failed_and_nothing_is_written() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        store.fail_read = true;
        assert_eq!(
            uninstall_release(Some(OURS), &mut store),
            UninstallRelease::Failed(RegistryError(5))
        );
        assert_eq!(store.writes, 0);
    }

    #[test]
    fn other_invalid_fields_in_the_config_do_not_hide_our_flag() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        let config = "max_items = banana\nretention_days = -4\nwin_v_added_v = true\n";
        assert_eq!(
            uninstall_release(Some(config), &mut store),
            UninstallRelease::Removed
        );
        assert_eq!(store.value.as_deref(), Some("QR"));
    }

    #[test]
    fn an_empty_or_garbage_config_leaves_the_registry_alone() {
        for config in ["", "\u{0}\u{1}\u{2}", "[[[", "win_v_added_v", "= true"] {
            let mut store = FakeDisabledHotkeys::with(Some("V"));
            assert_eq!(
                uninstall_release(Some(config), &mut store),
                UninstallRelease::Untouched,
                "{config:?}"
            );
            assert_eq!(store.writes, 0, "{config:?}");
            assert_eq!(store.value.as_deref(), Some("V"), "{config:?}");
        }
    }

    #[test]
    fn the_flag_without_the_option_on_still_removes_our_v() {
        let mut store = FakeDisabledHotkeys::with(Some("QRV"));
        let config = "open_with_win_v = false\nwin_v_added_v = true\n";
        assert_eq!(
            uninstall_release(Some(config), &mut store),
            UninstallRelease::Removed
        );
        assert_eq!(store.value.as_deref(), Some("QR"));
    }
}
