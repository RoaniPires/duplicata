//! Bordas da rodada 2: desfazer a ligação sem config, um atalho por status,
//! botão de reiniciar e a ordem dos efeitos no Salvar com falha.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

use duplicata_core::settings_session::{
    Effect, SaveOutcome, SaveReport, SaveStep, SettingsSession,
};
use duplicata_core::settings_text as text;
use duplicata_core::testsupport::{FakeDisabledHotkeys, FakeHotkeySlots};
use duplicata_core::win_v::{
    disable, enable, explorer_returned, startup, undo_unsaved_enable, WinVConfig, WinVStatus,
};
use duplicata_core::{Config, HotkeyCombo, StartupState, MOD_ALT, MOD_CONTROL};

const RESERVE: HotkeyCombo = HotkeyCombo::DEFAULT;
const CTRL_ALT_H: HotkeyCombo = HotkeyCombo {
    modifiers: MOD_CONTROL | MOD_ALT,
    vkey: 0x48,
};
const A: Option<u64> = Some(1_000);
const B: Option<u64> = Some(2_000);

const ALL: [WinVStatus; 5] = [
    WinVStatus::Off,
    WinVStatus::AwaitingRestart,
    WinVStatus::Active,
    WinVStatus::NotReleased,
    WinVStatus::OffAwaitingReturn,
];

mod undo_edges {
    use super::*;

    #[test]
    fn the_fields_are_offered_to_the_writer_exactly_once() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        let calls = Cell::new(0);
        undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| {
            calls.set(calls.get() + 1);
            false
        });
        assert_eq!(calls.get(), 1);
    }

    #[test]
    fn when_the_fields_are_saved_neither_the_registry_nor_the_hotkeys_move() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_reserve(true, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert_eq!(enabled.status, WinVStatus::Active);
        let writes = store.writes;
        let out = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| true);
        assert_eq!(out, None);
        assert_eq!(store.writes, writes);
        assert!(slots.win_v && slots.reserve.is_none());
    }

    #[test]
    fn an_unreadable_registry_still_turns_the_option_off_and_keeps_the_reserve() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        store.fail_read = true;
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false)
            .expect("desfeito");
        assert_eq!(undo.status, WinVStatus::Off);
        assert_eq!(undo.config, WinVConfig::default());
        assert_eq!(store.value.as_deref(), Some("QRV"), "nada foi gravado");
        assert_eq!(slots.reserve, Some(RESERVE));
        assert!(!undo.reserve_failed);
    }

    #[test]
    fn a_failing_write_while_undoing_still_turns_the_option_off() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        store.fail_write = true;
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false)
            .expect("desfeito");
        assert_eq!(undo.status, WinVStatus::Off);
        assert_eq!(store.value.as_deref(), Some("QRV"));
        assert_eq!(slots.reserve, Some(RESERVE));
    }

    #[test]
    fn a_v_that_is_gone_by_then_is_not_written_back() {
        let mut store = FakeDisabledHotkeys::with(Some("QR"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        store.value = Some("QR".into());
        let writes = store.writes;
        undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false).unwrap();
        assert_eq!(store.writes, writes);
        assert_eq!(store.value.as_deref(), Some("QR"));
    }

    #[test]
    fn a_foreign_v_waiting_for_the_restart_leaves_the_hotkeys_as_they_were() {
        let mut store = FakeDisabledHotkeys::with(Some("V"));
        let mut slots = FakeHotkeySlots::holding_reserve(false, RESERVE);
        let enabled = enable(RESERVE, &mut store, &mut slots, A).unwrap();
        assert_eq!(enabled.status, WinVStatus::AwaitingRestart);
        let attempts = slots.win_v_attempts;
        let undo = undo_unsaved_enable(enabled, RESERVE, &mut store, &mut slots, |_| false)
            .expect("desfeito");
        assert_eq!(undo.status, WinVStatus::Off);
        assert_eq!(store.value.as_deref(), Some("V"));
        assert_eq!(slots.reserve, Some(RESERVE));
        assert_eq!(slots.win_v_attempts, attempts);
        assert!(!undo.reserve_failed);
    }
}

mod one_hotkey_per_status {
    use super::*;

    /// `uses_reserve` diz se a reserva é o atalho registrado.
    fn assert_matches_slots(status: WinVStatus, slots: &FakeHotkeySlots, ctx: &str) {
        assert!(!slots.both_at_once, "{ctx}");
        assert_eq!(status.uses_reserve(), slots.reserve.is_some(), "{ctx}");
        assert_eq!(!status.uses_reserve(), slots.win_v, "{ctx}");
    }

    #[test]
    fn startup_leaves_the_slots_the_status_says() {
        for on in [false, true] {
            for win_v_free in [false, true] {
                for marker in [None, A, B] {
                    for now in [None, A, B] {
                        let store = FakeDisabledHotkeys::with(Some("QRV"));
                        let mut slots = FakeHotkeySlots::new(win_v_free);
                        let saved = WinVConfig {
                            on,
                            v_ours: true,
                            marker,
                        };
                        let out = startup(saved, RESERVE, &store, &mut slots, now);
                        let ctx = format!("{saved:?} free={win_v_free} now={now:?}");
                        assert_matches_slots(out.status, &slots, &ctx);
                    }
                }
            }
        }
    }

    #[test]
    fn enable_leaves_the_slots_the_status_says() {
        for value in [None, Some("QR"), Some("V")] {
            for win_v_free in [false, true] {
                let mut store = FakeDisabledHotkeys::with(value);
                let mut slots = FakeHotkeySlots::holding_reserve(win_v_free, RESERVE);
                let out = enable(RESERVE, &mut store, &mut slots, A).unwrap();
                assert_matches_slots(out.status, &slots, &format!("{value:?} {win_v_free}"));
            }
        }
    }

    #[test]
    fn disable_leaves_the_slots_the_status_says() {
        for from in ALL {
            for v_ours in [false, true] {
                let mut store = FakeDisabledHotkeys::with(Some("QRV"));
                let mut slots = if from == WinVStatus::Active {
                    FakeHotkeySlots::holding_win_v()
                } else {
                    FakeHotkeySlots::holding_reserve(false, RESERVE)
                };
                let saved = WinVConfig {
                    on: true,
                    v_ours,
                    marker: A,
                };
                let out = disable(saved, from, RESERVE, &mut store, &mut slots, B).unwrap();
                assert_matches_slots(out.status, &slots, &format!("{from:?} {v_ours}"));
            }
        }
    }

    #[test]
    fn explorer_returned_leaves_the_slots_the_status_says() {
        for from in ALL {
            for win_v_free in [false, true] {
                for now in [None, A, B] {
                    let mut slots = if from == WinVStatus::Active {
                        FakeHotkeySlots::holding_win_v()
                    } else {
                        FakeHotkeySlots::holding_reserve(win_v_free, RESERVE)
                    };
                    let saved = WinVConfig {
                        on: !matches!(from, WinVStatus::Off | WinVStatus::OffAwaitingReturn),
                        v_ours: true,
                        marker: A,
                    };
                    let mut last = A;
                    let r = explorer_returned(from, saved, RESERVE, &mut last, now, &mut slots);
                    let ctx = format!("{from:?} free={win_v_free} now={now:?}");
                    assert_matches_slots(r.status, &slots, &ctx);
                    assert_eq!(
                        r.close_failure_window,
                        r.status == WinVStatus::Active && from != WinVStatus::Active,
                        "{ctx}"
                    );
                }
            }
        }
    }
}

fn cfg() -> Config {
    let mut c = Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"));
    c.retention = Duration::from_secs(7 * 86_400);
    c.max_items = 500;
    c.max_pinned = 25;
    c
}

fn opened(config: WinVConfig, status: WinVStatus) -> SettingsSession {
    let mut c = cfg();
    config.write_into(&mut c);
    let mut s = SettingsSession::open(&c, StartupState::Desligado);
    s.sync_win_v(config, status);
    s
}

mod restart_button_rule {
    use super::*;

    #[test]
    fn it_follows_the_status_after_every_outside_change() {
        let ours = WinVConfig {
            on: true,
            v_ours: true,
            marker: A,
        };
        for status in ALL {
            let s = opened(ours, status);
            assert_eq!(
                s.begin_explorer_restart().is_some(),
                status.awaits_restart(),
                "{status:?}"
            );
            let view = s.hotkey_line(Some(RESERVE), false, false);
            assert_eq!(view.restart_visible, status.awaits_restart(), "{status:?}");
        }
    }

    #[test]
    fn the_explorer_coming_back_by_another_way_hides_the_button_and_refuses_the_click() {
        let mut s = opened(
            WinVConfig {
                on: true,
                v_ours: true,
                marker: A,
            },
            WinVStatus::AwaitingRestart,
        );
        assert!(s.begin_explorer_restart().is_some());
        s.sync_win_v(
            WinVConfig {
                on: true,
                v_ours: true,
                marker: A,
            },
            WinVStatus::Active,
        );
        assert_eq!(s.begin_explorer_restart(), None);
        assert!(!s.hotkey_line(None, false, false).restart_visible);

        let mut s = opened(
            WinVConfig {
                on: false,
                v_ours: false,
                marker: A,
            },
            WinVStatus::OffAwaitingReturn,
        );
        assert!(s.begin_explorer_restart().is_some());
        s.sync_win_v(WinVConfig::default(), WinVStatus::Off);
        assert_eq!(s.begin_explorer_restart(), None);
    }

    #[test]
    fn a_restart_in_progress_blocks_a_second_click_even_if_the_status_still_waits() {
        let mut s = opened(
            WinVConfig {
                on: true,
                v_ours: true,
                marker: A,
            },
            WinVStatus::AwaitingRestart,
        );
        s.explorer_restart_started();
        assert_eq!(s.begin_explorer_restart(), None);
        s.explorer_restart_finished();
        s.sync_win_v(
            WinVConfig {
                on: true,
                v_ours: true,
                marker: A,
            },
            WinVStatus::NotReleased,
        );
        assert_eq!(
            s.begin_explorer_restart(),
            None,
            "NotReleased não mostra o botão"
        );
    }
}

mod save_with_a_persist_failure {
    use super::*;

    fn confirm(s: &mut SettingsSession) -> Vec<Effect> {
        match s.begin_save() {
            SaveStep::Confirm(_) => {}
            SaveStep::Proceed(_) => panic!("o Win+V pede confirmação"),
        }
        s.confirm_save()
    }

    #[test]
    fn the_reserve_goes_first_then_win_v_and_the_write_is_always_last() {
        let mut s = opened(WinVConfig::default(), WinVStatus::Off);
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_win_v(true);
        assert_eq!(
            confirm(&mut s),
            vec![
                Effect::SwapHotkey {
                    from: RESERVE,
                    to: CTRL_ALT_H
                },
                Effect::EnableWinV,
                Effect::Persist
            ]
        );
    }

    #[test]
    fn a_persist_failure_after_enabling_stays_open_with_only_the_save_failure() {
        let mut s = opened(WinVConfig::default(), WinVStatus::Off);
        s.set_win_v(true);
        confirm(&mut s);
        let out = s.finish_save(SaveReport {
            persist_failed: true,
            ..Default::default()
        });
        assert_eq!(
            out,
            SaveOutcome::StayOpen {
                error_line: Some(text::SAVE_FAILED)
            }
        );
    }

    #[test]
    fn undoing_the_enable_afterwards_leaves_nothing_pending_and_cancel_does_nothing() {
        let mut s = opened(WinVConfig::default(), WinVStatus::Off);
        s.set_win_v(true);
        confirm(&mut s);
        s.finish_save(SaveReport {
            persist_failed: true,
            ..Default::default()
        });
        // A casca desfaz a ligação e informa o estado desligado.
        s.sync_win_v(WinVConfig::default(), WinVStatus::Off);
        assert!(!s.draft().win_v && !s.in_effect().win_v && !s.saved().win_v);
        assert!(s.classify().is_empty());
        assert!(!s.can_apply());
        assert!(s.cancel().is_empty());
        assert_eq!(s.begin_explorer_restart(), None);
    }

    #[test]
    fn a_persist_failure_with_a_swapped_reserve_keeps_the_new_reserve_as_the_starting_point() {
        let mut s = opened(WinVConfig::default(), WinVStatus::Off);
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_win_v(true);
        confirm(&mut s);
        s.finish_save(SaveReport {
            persist_failed: true,
            ..Default::default()
        });
        s.sync_win_v(WinVConfig::default(), WinVStatus::Off);
        assert_eq!(s.saved().hotkey, CTRL_ALT_H);
        assert!(s.cancel().is_empty(), "Cancelar só fecha");
    }

    #[test]
    fn a_persist_failure_when_turning_off_keeps_the_option_off_in_the_session() {
        let ours = WinVConfig {
            on: true,
            v_ours: true,
            marker: A,
        };
        let mut s = opened(ours, WinVStatus::Active);
        s.set_win_v(false);
        let effects = confirm(&mut s);
        assert_eq!(effects, vec![Effect::DisableWinV, Effect::Persist]);
        let out = s.finish_save(SaveReport {
            persist_failed: true,
            ..Default::default()
        });
        assert_eq!(
            out,
            SaveOutcome::StayOpen {
                error_line: Some(text::SAVE_FAILED)
            }
        );
        assert!(!s.saved().win_v && !s.in_effect().win_v);
    }
}
