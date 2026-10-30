use std::path::PathBuf;
use std::time::Duration;

use duplicata_core::settings_session::{
    ChangeKind, Effect, NumericField, SaveOutcome, SaveReport, SaveStep, Setting, SettingsSession,
    SettingsValues,
};
use duplicata_core::settings_text as text;
use duplicata_core::{Config, HotkeyCombo, StartupState, MOD_ALT, MOD_CONTROL};

const DAY: u64 = 24 * 60 * 60;

const CTRL_ALT_H: HotkeyCombo = HotkeyCombo {
    modifiers: MOD_CONTROL | MOD_ALT,
    vkey: 0x48,
};

fn cfg() -> Config {
    let mut c = Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"));
    c.retention = Duration::from_secs(7 * DAY);
    c.max_items = 500;
    c.max_pinned = 25;
    c
}

fn session() -> SettingsSession {
    SettingsSession::open(&cfg(), StartupState::Desligado)
}

fn kinds(s: &SettingsSession) -> Vec<(Setting, ChangeKind)> {
    s.classify()
}

#[test]
fn values_round_trip_through_the_config_without_touching_the_hotkey() {
    let mut c = cfg();
    c.blocked_programs = vec!["a.exe".into()];
    c.encryption_enabled = true;
    let v = SettingsValues::from_config(&c, StartupState::Ligado);
    assert_eq!(v.retention_days, 7);
    assert_eq!(v.max_items, 500);
    assert_eq!(v.max_pinned, 25);
    assert_eq!(v.blocked_programs, vec!["a.exe".to_string()]);
    assert!(v.secret_detection);
    assert!(v.protection);
    assert!(v.startup);
    assert_eq!(v.hotkey, HotkeyCombo::DEFAULT);

    let mut other = SettingsValues {
        hotkey: CTRL_ALT_H,
        retention_days: 30,
        max_items: 1000,
        max_pinned: 40,
        blocked_programs: vec!["b.exe".into()],
        secret_detection: false,
        protection: false,
        ..v
    };
    other.startup = false;
    let mut target = cfg();
    other.write_into(&mut target);
    assert_eq!(target.retention, Duration::from_secs(30 * DAY));
    assert_eq!(target.max_items, 1000);
    assert_eq!(target.max_pinned, 40);
    assert_eq!(target.blocked_programs, vec!["b.exe".to_string()]);
    assert!(!target.heuristic_secret_detection);
    assert!(!target.encryption_enabled);
    assert_eq!(
        target.hotkey,
        HotkeyCombo::DEFAULT,
        "o atalho só muda pela troca bem-sucedida, não pela gravação dos valores"
    );
}

#[test]
fn startup_blocked_by_the_task_manager_shows_off_with_its_description() {
    let s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
    assert!(!s.draft().startup);
    assert!(s.startup_blocked());
    assert_eq!(s.startup_description(), text::STARTUP_BLOCKED);
    let free = session();
    assert!(!free.startup_blocked());
    assert_eq!(free.startup_description(), text::STARTUP.description);
}

mod classificacao {
    use super::*;

    #[test]
    fn nothing_changed_means_nothing_to_apply() {
        let s = session();
        assert!(kinds(&s).is_empty());
        assert!(!s.can_apply());
    }

    #[test]
    fn reducing_the_retention_is_irreversible_and_does_not_enable_apply() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        assert_eq!(
            kinds(&s),
            vec![(Setting::RetentionDays, ChangeKind::Irreversible)]
        );
        assert!(!s.can_apply(), "7→3 deixa o Aplicar desabilitado");
    }

    #[test]
    fn reducing_then_going_back_to_the_original_is_not_a_reduction() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        s.set_numeric_text(NumericField::RetentionDays, "7");
        assert!(kinds(&s).is_empty());
        assert!(!s.can_apply());
    }

    #[test]
    fn increasing_retention_or_items_is_reversible() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "30");
        s.set_numeric_text(NumericField::MaxItems, "1000");
        assert_eq!(
            kinds(&s),
            vec![
                (Setting::RetentionDays, ChangeKind::Reversible),
                (Setting::MaxItems, ChangeKind::Reversible),
            ]
        );
        assert!(s.can_apply());
    }

    #[test]
    fn the_pin_ceiling_is_reversible_both_ways() {
        let mut s = session();
        s.set_numeric_text(NumericField::MaxPinned, "10");
        assert_eq!(
            kinds(&s),
            vec![(Setting::MaxPinned, ChangeKind::Reversible)]
        );
        s.set_numeric_text(NumericField::MaxPinned, "40");
        assert_eq!(
            kinds(&s),
            vec![(Setting::MaxPinned, ChangeKind::Reversible)]
        );
    }

    #[test]
    fn the_protection_alone_is_save_only_and_does_not_enable_apply() {
        let mut s = session();
        s.set_protection(true);
        assert_eq!(kinds(&s), vec![(Setting::Protection, ChangeKind::SaveOnly)]);
        assert!(!s.can_apply());
    }

    #[test]
    fn turning_on_a_blocked_startup_is_irreversible_and_does_not_enable_apply() {
        let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
        s.set_startup(true);
        assert_eq!(
            kinds(&s),
            vec![(Setting::Startup, ChangeKind::Irreversible)]
        );
        assert!(!s.can_apply());
        assert_eq!(
            s.startup_description(),
            text::STARTUP_BLOCKED,
            "a descrição do bloqueio fica até o Salvar"
        );
    }

    #[test]
    fn startup_without_a_block_is_reversible() {
        let mut s = session();
        s.set_startup(true);
        assert_eq!(kinds(&s), vec![(Setting::Startup, ChangeKind::Reversible)]);
        assert!(s.can_apply());
    }

    #[test]
    fn blocklist_and_secrets_are_reversible() {
        let mut s = session();
        assert!(s.add_program("keepass.exe"));
        s.set_secret_detection(false);
        assert_eq!(
            kinds(&s),
            vec![
                (Setting::BlockedPrograms, ChangeKind::Reversible),
                (Setting::SecretDetection, ChangeKind::Reversible),
            ]
        );
        assert!(s.can_apply());
    }

    #[test]
    fn a_pending_hotkey_is_reversible() {
        let mut s = session();
        s.set_pending_hotkey(CTRL_ALT_H);
        assert!(s.has_pending_hotkey());
        assert_eq!(kinds(&s), vec![(Setting::Hotkey, ChangeKind::Reversible)]);
        assert!(s.can_apply());
    }

    #[test]
    fn a_field_whose_text_differs_from_its_adjusted_value_enables_apply() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "");
        assert!(kinds(&s).is_empty(), "vazio volta ao valor anterior");
        assert!(s.can_apply(), "o campo vazio precisa ser reescrito");
        let mut s = session();
        s.set_numeric_text(NumericField::MaxPinned, "025");
        assert!(s.can_apply());
    }

    #[test]
    fn zero_items_is_adjusted_to_one_and_counts_as_a_reduction() {
        let mut s = session();
        s.set_numeric_text(NumericField::MaxItems, "0");
        assert_eq!(s.draft().max_items, 1);
        assert_eq!(
            kinds(&s),
            vec![(Setting::MaxItems, ChangeKind::Irreversible)]
        );
    }

    #[test]
    fn letters_never_reach_the_field_text() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "a5b");
        assert_eq!(s.numeric_text(NumericField::RetentionDays), "5");
    }
}

mod aplicar {
    use super::*;

    #[test]
    fn apply_puts_every_reversible_change_in_effect_without_persisting() {
        let mut s = session();
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_startup(true);
        assert!(s.add_program("keepass.exe"));
        s.set_numeric_text(NumericField::RetentionDays, "30");
        s.set_numeric_text(NumericField::MaxPinned, "40");
        let effects = s.apply();
        assert_eq!(
            effects,
            vec![
                Effect::SwapHotkey {
                    from: HotkeyCombo::DEFAULT,
                    to: CTRL_ALT_H
                },
                Effect::SetStartup {
                    enable: true,
                    clear_block: false
                },
                Effect::SetCaptureFilters {
                    blocked_programs: vec!["keepass.exe".into()],
                    secret_detection: true
                },
                Effect::SetRetention {
                    days: 30,
                    max_items: 500
                },
                Effect::SetMaxPinned(40),
            ]
        );
        assert!(!effects.contains(&Effect::Persist));
        assert_eq!(s.in_effect(), s.draft());
        assert_eq!(s.saved().retention_days, 7, "Aplicar não muda o salvo");
        assert!(!s.can_apply(), "Aplicar volta a ficar desabilitado");
    }

    #[test]
    fn save_only_changes_stay_pending_after_apply() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        s.set_numeric_text(NumericField::MaxPinned, "40");
        s.set_protection(true);
        let effects = s.apply();
        assert_eq!(effects, vec![Effect::SetMaxPinned(40)]);
        assert_eq!(s.in_effect().retention_days, 7);
        assert!(!s.in_effect().protection);
        assert_eq!(s.draft().retention_days, 3);
        assert!(s.draft().protection);
        assert_eq!(
            kinds(&s),
            vec![
                (Setting::RetentionDays, ChangeKind::Irreversible),
                (Setting::Protection, ChangeKind::SaveOnly),
            ]
        );
        assert!(!s.can_apply());
    }

    #[test]
    fn apply_rewrites_each_field_with_its_adjusted_value() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "");
        s.set_numeric_text(NumericField::MaxPinned, "500");
        let effects = s.apply();
        assert_eq!(effects, vec![Effect::SetMaxPinned(200)]);
        assert_eq!(s.numeric_text(NumericField::RetentionDays), "7");
        assert_eq!(s.numeric_text(NumericField::MaxPinned), "200");
        assert!(!s.can_apply());
    }

    #[test]
    fn an_empty_field_after_apply_returns_to_the_last_adjusted_value() {
        let mut s = session();
        s.set_numeric_text(NumericField::MaxItems, "1000");
        let _ = s.apply();
        s.set_numeric_text(NumericField::MaxItems, "");
        assert_eq!(s.draft().max_items, 1000);
    }

    #[test]
    fn retention_is_compared_with_the_saved_value_not_the_applied_one() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "30");
        let _ = s.apply();
        s.set_numeric_text(NumericField::RetentionDays, "10");
        assert_eq!(
            kinds(&s),
            vec![(Setting::RetentionDays, ChangeKind::Reversible)],
            "10 continua acima dos 7 salvos"
        );
        assert_eq!(
            s.apply(),
            vec![Effect::SetRetention {
                days: 10,
                max_items: 500
            }]
        );
    }

    #[test]
    fn a_blocked_startup_turned_on_is_not_applied() {
        let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
        s.set_startup(true);
        assert!(s.apply().is_empty());
        assert!(!s.in_effect().startup);
        assert!(s.draft().startup);
    }
}

mod cancelar {
    use super::*;

    #[test]
    fn cancel_undoes_what_was_applied_without_persisting() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "30");
        let _ = s.apply();
        assert_eq!(
            s.cancel(),
            vec![Effect::SetRetention {
                days: 7,
                max_items: 500
            }]
        );
        assert_eq!(s.in_effect(), s.saved());
    }

    #[test]
    fn cancel_without_anything_applied_has_no_effect() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "30");
        s.set_protection(true);
        assert!(s.add_program("keepass.exe"));
        assert!(s.cancel().is_empty());
    }

    #[test]
    fn cancel_restores_every_applied_setting_in_order() {
        let mut s = SettingsSession::open(
            &Config {
                blocked_programs: vec!["keepass.exe".into()],
                ..cfg()
            },
            StartupState::Desligado,
        );
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_startup(true);
        assert!(s.remove_program(Some(0)));
        s.set_secret_detection(false);
        s.set_numeric_text(NumericField::MaxPinned, "10");
        let _ = s.apply();
        assert_eq!(
            s.cancel(),
            vec![
                Effect::SwapHotkey {
                    from: CTRL_ALT_H,
                    to: HotkeyCombo::DEFAULT
                },
                Effect::SetStartup {
                    enable: false,
                    clear_block: false
                },
                Effect::SetCaptureFilters {
                    blocked_programs: vec!["keepass.exe".into()],
                    secret_detection: true
                },
                Effect::SetMaxPinned(25),
            ]
        );
    }
}

mod salvar {
    use super::*;

    #[test]
    fn saving_without_irreversible_changes_proceeds_with_everything_and_persists_last() {
        let mut s = session();
        s.set_secret_detection(false);
        s.set_protection(true);
        s.set_numeric_text(NumericField::MaxPinned, "10");
        let SaveStep::Proceed(effects) = s.begin_save() else {
            panic!("sem irreversível não há confirmação");
        };
        assert_eq!(
            effects,
            vec![
                Effect::SetCaptureFilters {
                    blocked_programs: vec![],
                    secret_detection: false
                },
                Effect::SetMaxPinned(10),
                Effect::ToggleProtection(true),
                Effect::Persist,
            ]
        );
        assert_eq!(s.in_effect(), s.draft());
        assert_eq!(s.finish_save(SaveReport::default()), SaveOutcome::Close);
    }

    #[test]
    fn saving_a_reduction_asks_for_confirmation_with_one_line_per_change() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        s.set_numeric_text(NumericField::MaxItems, "100");
        assert_eq!(
            s.begin_save(),
            SaveStep::Confirm(vec![
                text::reduce_retention_line(7, 3),
                text::reduce_items_line(500, 100),
                text::CANNOT_UNDO.to_string(),
            ])
        );
    }

    #[test]
    fn going_back_from_the_confirmation_changes_nothing() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        let before = s.clone();
        let _ = s.begin_save();
        s.back();
        assert_eq!(s.in_effect(), before.in_effect());
        assert_eq!(s.draft(), before.draft());
        assert_eq!(s.saved(), before.saved());
        assert_eq!(s.numeric_text(NumericField::RetentionDays), "3");
    }

    #[test]
    fn confirming_emits_the_reduction_and_persists() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        let _ = s.begin_save();
        assert_eq!(
            s.confirm_save(),
            vec![
                Effect::SetRetention {
                    days: 3,
                    max_items: 500
                },
                Effect::Persist
            ]
        );
    }

    #[test]
    fn zero_items_is_shown_as_one_and_confirmed_as_a_reduction() {
        let mut s = session();
        s.set_numeric_text(NumericField::MaxItems, "0");
        assert_eq!(
            s.begin_save(),
            SaveStep::Confirm(vec![
                text::reduce_items_line(500, 1),
                text::CANNOT_UNDO.to_string(),
            ])
        );
        assert_eq!(s.numeric_text(NumericField::MaxItems), "1");
    }

    #[test]
    fn reducing_and_returning_to_the_original_saves_without_confirmation() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        s.set_numeric_text(NumericField::RetentionDays, "7");
        assert_eq!(s.begin_save(), SaveStep::Proceed(vec![Effect::Persist]));
    }

    #[test]
    fn turning_on_a_blocked_startup_asks_and_then_clears_the_block() {
        let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
        s.set_startup(true);
        assert_eq!(
            s.begin_save(),
            SaveStep::Confirm(vec![
                text::UNBLOCK_STARTUP_LINE.to_string(),
                text::CANNOT_UNDO.to_string(),
            ])
        );
        assert_eq!(
            s.confirm_save(),
            vec![
                Effect::SetStartup {
                    enable: true,
                    clear_block: true
                },
                Effect::Persist
            ]
        );
        assert!(!s.startup_blocked());
    }

    #[test]
    fn a_failed_write_keeps_the_window_open_and_cancel_then_only_closes() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "10");
        let _ = s.begin_save();
        assert_eq!(
            s.finish_save(SaveReport {
                hotkey_failed: false,
                persist_failed: true,
                ..Default::default()
            }),
            SaveOutcome::StayOpen {
                error_line: Some(text::SAVE_FAILED)
            }
        );
        assert_eq!(s.in_effect().retention_days, 10, "nada é desfeito");
        assert!(s.cancel().is_empty(), "o Salvar virou o ponto de partida");
    }

    #[test]
    fn a_hotkey_taken_before_saving_keeps_the_window_open_with_the_message() {
        let mut s = session();
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_numeric_text(NumericField::MaxPinned, "40");
        let SaveStep::Proceed(effects) = s.begin_save() else {
            panic!("sem irreversível");
        };
        assert!(effects.contains(&Effect::SwapHotkey {
            from: HotkeyCombo::DEFAULT,
            to: CTRL_ALT_H
        }));
        assert_eq!(
            s.finish_save(SaveReport {
                hotkey_failed: true,
                persist_failed: false,
                ..Default::default()
            }),
            SaveOutcome::StayOpen { error_line: None }
        );
        assert_eq!(s.in_effect().hotkey, HotkeyCombo::DEFAULT);
        assert_eq!(s.draft().hotkey, HotkeyCombo::DEFAULT);
        assert_eq!(s.in_effect().max_pinned, 40, "as demais foram salvas");
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert_eq!(
            line.description,
            text::hotkey_taken("Ctrl+Alt+H", "Ctrl+Shift+V")
        );
        assert!(s.cancel().is_empty());
    }
}

mod atalho_e_protecao {
    use super::*;

    #[test]
    fn the_line_shows_the_active_combination_and_its_description() {
        let s = session();
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert_eq!(line.combo, "Ctrl+Shift+V");
        assert_eq!(line.description, text::HOTKEY.description);
        assert!(line.alter_enabled);
    }

    #[test]
    fn armed_the_line_asks_for_the_new_combination() {
        let mut s = session();
        s.hotkey_armed();
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), true, false);
        assert_eq!(line.description, text::HOTKEY_ARMED);
    }

    #[test]
    fn a_refusal_shows_its_message_and_leaves_nothing_pending() {
        let mut s = session();
        s.hotkey_armed();
        s.set_hotkey_message("recusada".into());
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), true, false);
        assert_eq!(line.description, "recusada");
        assert!(!s.has_pending_hotkey());
        assert!(!s.can_apply());
        s.hotkey_disarmed();
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert_eq!(line.description, text::HOTKEY.description);
    }

    #[test]
    fn a_pending_combination_is_shown_with_its_description() {
        let mut s = session();
        s.hotkey_armed();
        s.set_pending_hotkey(CTRL_ALT_H);
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert_eq!(line.combo, "Ctrl+Alt+H");
        assert_eq!(line.description, text::HOTKEY_PENDING);
    }

    #[test]
    fn choosing_the_active_combination_again_clears_the_pending_one() {
        let mut s = session();
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_pending_hotkey(HotkeyCombo::DEFAULT);
        assert!(!s.has_pending_hotkey());
    }

    #[test]
    fn with_the_standalone_dialog_open_the_line_is_inactive() {
        let s = session();
        let line = s.hotkey_line(None, true, true);
        assert_eq!(line.description, text::HOTKEY_STANDALONE);
        assert!(!line.alter_enabled);
    }

    #[test]
    fn without_any_active_hotkey_the_line_says_so() {
        let s = session();
        let line = s.hotkey_line(None, false, false);
        assert_eq!(line.description, text::hotkey_none_active("Ctrl+Shift+V"));
    }

    #[test]
    fn a_failed_swap_on_apply_returns_to_the_active_combination_with_the_message() {
        let mut s = session();
        s.set_pending_hotkey(CTRL_ALT_H);
        let _ = s.apply();
        s.hotkey_effect_failed();
        assert_eq!(s.in_effect().hotkey, HotkeyCombo::DEFAULT);
        assert_eq!(s.draft().hotkey, HotkeyCombo::DEFAULT);
        assert!(!s.can_apply());
        let line = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert_eq!(
            line.description,
            text::hotkey_taken("Ctrl+Alt+H", "Ctrl+Shift+V")
        );
        assert!(s.cancel().is_empty(), "nada trocado para desfazer");
    }

    #[test]
    fn a_rewrite_in_progress_shows_the_target_and_locks_the_toggle() {
        let mut s = session();
        s.protection_in_progress(true);
        assert!(s.protection_busy());
        assert!(s.draft().protection);
        s.set_protection(false);
        assert!(s.draft().protection, "travada até terminar");
        assert!(kinds(&s).is_empty());
        s.protection_finished();
        assert!(!s.protection_busy());
        s.set_protection(false);
        assert!(!s.draft().protection);
    }

    #[test]
    fn saving_the_protection_locks_the_toggle_until_the_rewrite_ends() {
        let mut s = session();
        s.set_protection(true);
        let _ = s.begin_save();
        assert!(s.protection_busy());
    }
}

mod ordem_e_bloqueio {
    use super::*;
    use duplicata_core::settings_session::startup_for_settings;
    use duplicata_core::Approval;

    #[test]
    fn the_confirmation_lists_retention_then_items_then_startup() {
        let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
        s.set_startup(true);
        s.set_numeric_text(NumericField::MaxItems, "100");
        s.set_numeric_text(NumericField::RetentionDays, "3");
        assert_eq!(
            s.begin_save(),
            SaveStep::Confirm(vec![
                text::reduce_retention_line(7, 3),
                text::reduce_items_line(500, 100),
                text::UNBLOCK_STARTUP_LINE.to_string(),
                text::CANNOT_UNDO.to_string(),
            ])
        );
    }

    #[test]
    fn a_disapproval_left_without_the_run_entry_still_counts_as_blocked() {
        let state = startup_for_settings(false, Approval::Desaprovado);
        assert_eq!(state, StartupState::DesligadoPeloGerenciador);
        let mut s = SettingsSession::open(&cfg(), state);
        assert!(!s.draft().startup);
        assert_eq!(s.startup_description(), text::STARTUP_BLOCKED);
        s.set_startup(true);
        assert_eq!(
            kinds(&s),
            vec![(Setting::Startup, ChangeKind::Irreversible)]
        );
        assert!(!s.can_apply());
        let _ = s.begin_save();
        assert_eq!(
            s.confirm_save(),
            vec![
                Effect::SetStartup {
                    enable: true,
                    clear_block: true
                },
                Effect::Persist
            ]
        );
    }

    #[test]
    fn without_a_disapproval_the_startup_state_is_the_usual_one() {
        assert_eq!(
            startup_for_settings(false, Approval::Indeterminado),
            StartupState::Desligado
        );
        assert_eq!(
            startup_for_settings(true, Approval::Indeterminado),
            StartupState::Ligado
        );
        assert_eq!(
            startup_for_settings(true, Approval::Desaprovado),
            StartupState::DesligadoPeloGerenciador
        );
    }
}

mod win_v {
    use duplicata_core::win_v::{WinVConfig, WinVStatus};

    use super::*;

    const ON_OURS: WinVConfig = WinVConfig {
        on: true,
        v_ours: true,
        marker: Some(1),
    };
    const ON_NOT_OURS: WinVConfig = WinVConfig {
        on: true,
        v_ours: false,
        marker: Some(1),
    };

    fn opened_with(config: WinVConfig, status: WinVStatus) -> SettingsSession {
        let mut c = cfg();
        config.write_into(&mut c);
        let mut s = SettingsSession::open(&c, StartupState::Desligado);
        s.sync_win_v(config, status);
        s
    }

    fn lines(step: SaveStep) -> Vec<String> {
        match step {
            SaveStep::Confirm(lines) => lines,
            SaveStep::Proceed(_) => panic!("o Win+V sempre pede confirmação"),
        }
    }

    #[test]
    fn the_option_is_read_from_the_config() {
        let mut c = cfg();
        ON_OURS.write_into(&mut c);
        let s = SettingsSession::open(&c, StartupState::Desligado);
        assert!(s.saved().win_v && s.in_effect().win_v && s.draft().win_v);
        assert!(!session().draft().win_v);
    }

    #[test]
    fn turning_it_on_does_not_enable_apply_and_apply_ignores_it() {
        let mut s = session();
        s.set_win_v(true);
        assert_eq!(
            kinds(&s),
            vec![(Setting::WinV, ChangeKind::SaveOnlyConfirmed)]
        );
        assert!(!s.can_apply());
        assert!(s.apply().is_empty());
        assert!(s.draft().win_v, "continua no rascunho");
        assert!(!s.in_effect().win_v);
    }

    #[test]
    fn cancel_drops_the_draft_without_touching_the_registry() {
        let mut s = session();
        s.set_win_v(true);
        assert!(s.cancel().is_empty());
        assert!(!s.draft().win_v);
    }

    #[test]
    fn saving_it_on_asks_with_the_two_lines_then_enables_before_persisting() {
        let mut s = session();
        s.set_win_v(true);
        assert_eq!(
            lines(s.begin_save()),
            vec![
                text::WIN_V_ENABLE_PANEL_LINE.to_string(),
                text::win_v_enable_when("Ctrl+Shift+V"),
            ]
        );
        assert_eq!(s.confirm_save(), vec![Effect::EnableWinV, Effect::Persist]);
    }

    #[test]
    fn irreversible_lines_come_first_then_cannot_undo_then_the_win_v_block() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "3");
        s.set_win_v(true);
        assert_eq!(
            lines(s.begin_save()),
            vec![
                text::reduce_retention_line(7, 3),
                text::CANNOT_UNDO.to_string(),
                String::new(),
                text::WIN_V_ENABLE_PANEL_LINE.to_string(),
                text::win_v_enable_when("Ctrl+Shift+V"),
            ]
        );
    }

    #[test]
    fn the_enable_line_names_the_reserve_being_saved() {
        let mut s = session();
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_win_v(true);
        assert!(lines(s.begin_save()).contains(&text::win_v_enable_when("Ctrl+Alt+H")));
    }

    #[test]
    fn saving_it_off_with_our_v_says_it_returns_after_a_restart() {
        let mut s = opened_with(ON_OURS, WinVStatus::Active);
        s.set_win_v(false);
        assert_eq!(
            lines(s.begin_save()),
            vec![text::WIN_V_DISABLE_LINE.to_string()]
        );
        assert_eq!(s.confirm_save(), vec![Effect::DisableWinV, Effect::Persist]);
    }

    #[test]
    fn saving_it_off_with_a_v_that_was_not_ours_says_it_stays_disabled() {
        let mut s = opened_with(ON_NOT_OURS, WinVStatus::Active);
        s.set_win_v(false);
        assert_eq!(
            lines(s.begin_save()),
            vec![
                text::WIN_V_DISABLE_NOW_LINE.to_string(),
                text::WIN_V_WAS_DISABLED_BEFORE.to_string(),
            ]
        );
    }

    #[test]
    fn back_in_the_confirmation_keeps_the_option_in_the_draft() {
        let mut s = session();
        s.set_win_v(true);
        let _ = s.begin_save();
        s.back();
        assert!(s.draft().win_v);
        assert!(!s.in_effect().win_v);
    }

    #[test]
    fn a_registry_failure_stays_open_reverts_the_option_and_keeps_the_rest() {
        let mut s = session();
        s.set_numeric_text(NumericField::RetentionDays, "14");
        s.set_win_v(true);
        let _ = s.begin_save();
        let _ = s.confirm_save();
        assert_eq!(
            s.finish_save(SaveReport {
                win_v_failed: true,
                ..Default::default()
            }),
            SaveOutcome::StayOpen {
                error_line: Some(text::WIN_V_REGISTRY_FAILED)
            }
        );
        assert!(!s.draft().win_v, "a opção volta a desligada");
        assert!(!s.in_effect().win_v);
        assert_eq!(s.saved().retention_days, 14, "o resto do Salvar vale");
        assert!(s.cancel().is_empty());
    }

    #[test]
    fn a_registry_and_config_failure_show_only_the_save_failure() {
        let mut s = opened_with(ON_OURS, WinVStatus::Active);
        s.set_win_v(false);
        let _ = s.begin_save();
        let _ = s.confirm_save();
        assert_eq!(
            s.finish_save(SaveReport {
                win_v_failed: true,
                persist_failed: true,
                ..Default::default()
            }),
            SaveOutcome::StayOpen {
                error_line: Some(text::SAVE_FAILED)
            }
        );
        assert!(s.draft().win_v, "a opção volta a ligada");
    }

    #[test]
    fn with_win_v_active_the_reserve_changes_without_registering() {
        let mut s = opened_with(ON_OURS, WinVStatus::Active);
        s.set_pending_hotkey(CTRL_ALT_H);
        assert_eq!(s.apply(), vec![Effect::SetReserve(CTRL_ALT_H)]);
        assert_eq!(s.cancel(), vec![Effect::SetReserve(HotkeyCombo::DEFAULT)]);
    }

    #[test]
    fn turning_it_off_sets_the_new_reserve_before_disabling() {
        let mut s = opened_with(ON_OURS, WinVStatus::Active);
        s.set_pending_hotkey(CTRL_ALT_H);
        s.set_win_v(false);
        let _ = s.begin_save();
        assert_eq!(
            s.confirm_save(),
            vec![
                Effect::SetReserve(CTRL_ALT_H),
                Effect::DisableWinV,
                Effect::Persist
            ]
        );
    }

    #[test]
    fn each_status_has_its_line_and_only_the_awaiting_ones_show_the_button() {
        let reserve = "Ctrl+Shift+V";
        let cases = [
            (WinVStatus::Off, None, false),
            (
                WinVStatus::AwaitingRestart,
                Some(text::win_v_awaiting(reserve)),
                true,
            ),
            (WinVStatus::Active, Some(text::win_v_active(reserve)), false),
            (
                WinVStatus::NotReleased,
                Some(text::win_v_not_released(reserve)),
                false,
            ),
            (
                WinVStatus::OffAwaitingReturn,
                Some(text::WIN_V_OFF_AWAITING_RETURN.to_string()),
                true,
            ),
        ];
        for (status, line, button) in cases {
            let s = opened_with(ON_OURS, status);
            let view = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
            assert_eq!(view.status_line, line, "{status:?}");
            assert_eq!(view.restart_visible, button, "{status:?}");
            assert_eq!(view.restart_enabled, button, "{status:?}");
            assert_eq!(view.combo, reserve, "ao lado de Alterar continua a reserva");
        }
    }

    #[test]
    fn with_win_v_active_an_unregistered_reserve_is_not_reported_as_missing() {
        let s = opened_with(ON_OURS, WinVStatus::Active);
        let view = s.hotkey_line(None, false, false);
        assert_eq!(view.description, text::HOTKEY.description);
    }

    #[test]
    fn the_hint_shows_only_while_the_field_waits_for_the_combination() {
        let mut s = session();
        s.hotkey_armed();
        let armed = s.hotkey_line(Some(HotkeyCombo::DEFAULT), true, false);
        assert_eq!(armed.hint, Some(text::WIN_V_HINT));
        s.hotkey_disarmed();
        let idle = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert_eq!(idle.hint, None);
    }

    #[test]
    fn the_restart_button_asks_for_confirmation_and_is_disabled_while_running() {
        let mut s = opened_with(ON_OURS, WinVStatus::AwaitingRestart);
        assert_eq!(
            s.begin_explorer_restart(),
            Some(vec![text::RESTART_EXPLORER_CONFIRM.to_string()])
        );
        s.explorer_restart_started();
        assert_eq!(s.begin_explorer_restart(), None);
        let view = s.hotkey_line(Some(HotkeyCombo::DEFAULT), false, false);
        assert!(view.restart_visible && !view.restart_enabled);
        s.explorer_restart_finished();
        assert!(s.begin_explorer_restart().is_some());
        assert_eq!(session().begin_explorer_restart(), None, "desligada");
    }

    #[test]
    fn the_restart_no_longer_applies_once_the_explorer_came_back_by_another_way() {
        let mut s = opened_with(ON_OURS, WinVStatus::AwaitingRestart);
        assert!(s.begin_explorer_restart().is_some());
        // O Explorer voltou com a confirmação aberta: "Reiniciar" não vale mais.
        s.sync_win_v(ON_OURS, WinVStatus::Active);
        assert_eq!(s.begin_explorer_restart(), None);
    }

    #[test]
    fn an_outside_change_follows_the_option_unless_the_draft_has_its_own() {
        let mut s = opened_with(ON_OURS, WinVStatus::NotReleased);
        s.sync_win_v(WinVConfig::default(), WinVStatus::OffAwaitingReturn);
        assert!(!s.saved().win_v && !s.in_effect().win_v && !s.draft().win_v);
        assert!(s.classify().is_empty());

        let mut s = session();
        s.set_win_v(true);
        s.sync_win_v(WinVConfig::default(), WinVStatus::Off);
        assert!(s.draft().win_v, "o rascunho do usuário continua");
    }
}
