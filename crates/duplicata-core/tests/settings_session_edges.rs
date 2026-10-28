//! Casos de borda da sessão de Configurações: vários Aplicar seguidos de
//! Cancelar, Salvar que deixa a janela aberta, limites dos campos e
//! inicialização bloqueada.

use std::path::PathBuf;
use std::time::Duration;

use duplicata_core::settings_session::{
    ChangeKind, Effect, NumericField, SaveOutcome, SaveReport, SaveStep, Setting, SettingsSession,
};
use duplicata_core::settings_text as text;
use duplicata_core::{Config, HotkeyCombo, StartupState, MOD_ALT, MOD_CONTROL, MOD_SHIFT};

const DAY: u64 = 24 * 60 * 60;

const A: HotkeyCombo = HotkeyCombo {
    modifiers: MOD_CONTROL | MOD_ALT,
    vkey: 0x48,
};
const B: HotkeyCombo = HotkeyCombo {
    modifiers: MOD_CONTROL | MOD_SHIFT,
    vkey: 0x4B,
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

fn type_in(s: &mut SettingsSession, f: NumericField, t: &str) {
    s.set_numeric_text(f, t);
}

#[test]
fn cancel_after_several_applies_undoes_from_the_last_applied_back_to_the_opening_values() {
    // Quebra: Cancelar desfaz só o último Aplicar (usa o penúltimo estado)
    // em vez do estado de abertura.
    let mut s = session();
    type_in(&mut s, NumericField::RetentionDays, "30");
    s.set_pending_hotkey(A);
    assert!(!s.apply().is_empty());
    type_in(&mut s, NumericField::RetentionDays, "60");
    s.set_pending_hotkey(B);
    s.add_program("keepass.exe");
    assert!(!s.apply().is_empty());
    let effects = s.cancel();
    assert!(effects.contains(&Effect::SwapHotkey {
        from: B,
        to: HotkeyCombo::DEFAULT
    }));
    assert!(effects.contains(&Effect::SetRetention {
        days: 7,
        max_items: 500
    }));
    assert!(effects.contains(&Effect::SetCaptureFilters {
        blocked_programs: vec![],
        secret_detection: true
    }));
    assert!(!effects.contains(&Effect::Persist));
    assert_eq!(s.in_effect(), s.saved());
    assert_eq!(s.draft(), s.saved());
    assert_eq!(s.numeric_text(NumericField::RetentionDays), "7");
}

#[test]
fn save_that_stays_open_makes_what_it_applied_the_new_start_so_cancel_undoes_nothing() {
    // Quebra: o Cancelar depois de um Salvar com falha desfaz o que o Salvar pôs em efeito.
    let mut s = session();
    s.add_program("keepass.exe");
    s.set_secret_detection(false);
    let SaveStep::Proceed(effects) = s.begin_save() else {
        panic!("sem confirmação");
    };
    assert_eq!(effects.last(), Some(&Effect::Persist));
    let out = s.finish_save(SaveReport {
        hotkey_failed: false,
        persist_failed: true,
    });
    assert!(matches!(
        out,
        SaveOutcome::StayOpen {
            error_line: Some(_)
        }
    ));
    assert!(s.cancel().is_empty());
    assert_eq!(s.saved().blocked_programs, vec!["keepass.exe".to_string()]);
    assert!(!s.saved().secret_detection);
}

#[test]
fn reducing_after_applying_an_increase_is_measured_against_the_saved_value() {
    // Quebra: "antes" vira o valor aplicado (30) em vez do salvo (7).
    let mut s = session();
    type_in(&mut s, NumericField::RetentionDays, "30");
    s.apply();
    type_in(&mut s, NumericField::RetentionDays, "3");
    let SaveStep::Confirm(lines) = s.begin_save() else {
        panic!("esperava confirmação");
    };
    assert_eq!(lines[0], text::reduce_retention_line(7, 3));
    assert_eq!(lines.last().map(String::as_str), Some(text::CANNOT_UNDO));
}

#[test]
fn going_back_to_the_saved_value_after_an_applied_increase_is_reversible_not_a_reduction() {
    // Quebra: 30 -> 7 conta como redução (compara com o aplicado) e pede confirmação.
    let mut s = session();
    type_in(&mut s, NumericField::RetentionDays, "30");
    s.apply();
    type_in(&mut s, NumericField::RetentionDays, "7");
    assert_eq!(
        s.classify(),
        vec![(Setting::RetentionDays, ChangeKind::Reversible)]
    );
    assert!(s.can_apply());
    assert!(matches!(s.begin_save(), SaveStep::Proceed(_)));
}

#[test]
fn an_irreversible_reduction_pending_does_not_stop_apply_from_taking_the_reversible_ones() {
    // Quebra: o Aplicar leva a redução junto, ou o prazo vai com a quantidade.
    let mut s = session();
    type_in(&mut s, NumericField::RetentionDays, "3");
    type_in(&mut s, NumericField::MaxItems, "1000");
    let effects = s.apply();
    assert_eq!(
        effects,
        vec![Effect::SetRetention {
            days: 7,
            max_items: 1000
        }]
    );
    assert_eq!(s.in_effect().retention_days, 7);
    assert_eq!(s.draft().retention_days, 3);
    assert!(!s.can_apply());
}

#[test]
fn confirmation_has_one_line_per_irreversible_change_and_ends_with_cannot_undo() {
    // Quebra: uma linha faltando ou repetida, ou "Isso não pode ser desfeito." fora do fim.
    let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
    s.set_startup(true);
    type_in(&mut s, NumericField::MaxItems, "100");
    type_in(&mut s, NumericField::RetentionDays, "3");
    let SaveStep::Confirm(lines) = s.begin_save() else {
        panic!("esperava confirmação");
    };
    let (last, body) = lines.split_last().expect("linhas");
    assert_eq!(last, text::CANNOT_UNDO);
    let mut body = body.to_vec();
    body.sort();
    let mut want = vec![
        text::reduce_retention_line(7, 3),
        text::reduce_items_line(500, 100),
        text::UNBLOCK_STARTUP_LINE.to_string(),
    ];
    want.sort();
    assert_eq!(body, want);
}

#[test]
fn pin_ceiling_zero_is_kept_200_is_kept_and_201_becomes_200() {
    // Quebra: teto 0 vira 1, ou 200/201 não ajustados.
    let mut s = session();
    type_in(&mut s, NumericField::MaxPinned, "0");
    assert_eq!(s.draft().max_pinned, 0);
    type_in(&mut s, NumericField::MaxPinned, "200");
    assert_eq!(s.draft().max_pinned, 200);
    type_in(&mut s, NumericField::MaxPinned, "201");
    assert_eq!(s.draft().max_pinned, 200);
    assert!(s.can_apply());
    let effects = s.apply();
    assert_eq!(effects, vec![Effect::SetMaxPinned(200)]);
    assert_eq!(s.numeric_text(NumericField::MaxPinned), "200");
}

#[test]
fn a_ten_digit_value_above_u32_is_adjusted_to_the_maximum_and_shown_that_way() {
    // Quebra: estouro vira 0 ou pânico em vez de saturar.
    let mut s = session();
    type_in(&mut s, NumericField::RetentionDays, "9999999999");
    assert_eq!(s.draft().retention_days, u32::MAX);
    type_in(&mut s, NumericField::MaxItems, "4294967296");
    assert_eq!(s.draft().max_items, u32::MAX);
    s.apply();
    assert_eq!(s.numeric_text(NumericField::RetentionDays), "4294967295");
    assert_eq!(s.numeric_text(NumericField::MaxItems), "4294967295");
}

#[test]
fn an_empty_field_restores_the_value_the_field_showed_and_enables_apply_until_rewritten() {
    // Quebra: vazio vira 0/1 em vez de voltar ao anterior, ou não habilita o Aplicar.
    let mut s = session();
    type_in(&mut s, NumericField::MaxItems, "");
    assert_eq!(s.draft().max_items, 500);
    assert!(s.can_apply());
    assert!(s.apply().is_empty());
    assert_eq!(s.numeric_text(NumericField::MaxItems), "500");
    assert!(!s.can_apply());
}

#[test]
fn a_blocked_startup_turned_on_and_off_again_leaves_nothing_pending() {
    // Quebra: ligar e desligar deixa mudança irreversível pendente.
    let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
    s.set_startup(true);
    assert_eq!(
        s.classify(),
        vec![(Setting::Startup, ChangeKind::Irreversible)]
    );
    s.set_startup(false);
    assert!(s.classify().is_empty());
    assert!(matches!(s.begin_save(), SaveStep::Proceed(_)));
}

#[test]
fn a_blocked_startup_confirmed_then_failed_write_no_longer_shows_the_block() {
    // Quebra: após o Salvar que liga e deixa a janela aberta, a descrição continua "Desativado...".
    let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
    s.set_startup(true);
    assert!(matches!(s.begin_save(), SaveStep::Confirm(_)));
    let effects = s.confirm_save();
    assert!(effects.contains(&Effect::SetStartup {
        enable: true,
        clear_block: true
    }));
    s.finish_save(SaveReport {
        hotkey_failed: false,
        persist_failed: true,
    });
    assert!(!s.startup_blocked());
    assert_eq!(s.startup_description(), text::STARTUP.description);
}

#[test]
fn a_blocked_startup_pending_is_dropped_by_cancel_without_touching_the_system() {
    // Quebra: Cancelar emite SetStartup para um ligar que nunca foi aplicado.
    let mut s = SettingsSession::open(&cfg(), StartupState::DesligadoPeloGerenciador);
    s.set_startup(true);
    s.apply();
    assert!(s.cancel().is_empty());
    assert!(!s.draft().startup);
}

#[test]
fn the_protection_toggle_ignores_clicks_while_a_rewrite_runs() {
    // Quebra: o rascunho muda durante a reescrita, gerando outra reescrita ao Salvar.
    let mut s = session();
    s.protection_in_progress(true);
    s.set_protection(false);
    assert!(s.draft().protection);
    assert!(s.classify().is_empty());
    assert!(matches!(s.begin_save(), SaveStep::Proceed(e) if e == vec![Effect::Persist]));
    s.protection_finished();
    s.set_protection(false);
    assert_eq!(
        s.classify(),
        vec![(Setting::Protection, ChangeKind::SaveOnly)]
    );
}

#[test]
fn applying_never_touches_the_protection_and_cancel_does_not_rewrite_it() {
    // Quebra: Aplicar ou Cancelar emitem ToggleProtection.
    let mut s = session();
    s.set_protection(true);
    s.add_program("a.exe");
    let applied = s.apply();
    assert!(!applied
        .iter()
        .any(|e| matches!(e, Effect::ToggleProtection(_))));
    assert!(s.in_effect().protection == s.saved().protection);
    let cancelled = s.cancel();
    assert!(!cancelled
        .iter()
        .any(|e| matches!(e, Effect::ToggleProtection(_))));
}

#[test]
fn startup_state_for_settings_is_blocked_exactly_when_the_approval_is_disapproved() {
    // Quebra: desaprovação sem entrada em Run vira "Desligado" (sem bloqueio), ou
    // aprovado/indeterminado com entrada deixa de ser "Ligado".
    use duplicata_core::settings_session::startup_for_settings;
    use duplicata_core::Approval::{Aprovado, Desaprovado, Indeterminado};
    for entry in [false, true] {
        assert_eq!(
            startup_for_settings(entry, Desaprovado),
            StartupState::DesligadoPeloGerenciador
        );
    }
    assert_eq!(
        startup_for_settings(false, Aprovado),
        StartupState::Desligado
    );
    assert_eq!(
        startup_for_settings(false, Indeterminado),
        StartupState::Desligado
    );
    assert_eq!(startup_for_settings(true, Aprovado), StartupState::Ligado);
    assert_eq!(
        startup_for_settings(true, Indeterminado),
        StartupState::Ligado
    );
}

#[test]
fn a_disapproval_without_entry_turned_on_waits_for_save_and_asks_before_clearing_it() {
    // Quebra: o Aplicar liga a inicialização sem confirmação nesse estado.
    use duplicata_core::settings_session::startup_for_settings;
    use duplicata_core::Approval::Desaprovado;
    let mut s = SettingsSession::open(&cfg(), startup_for_settings(false, Desaprovado));
    s.set_startup(true);
    assert!(!s.can_apply());
    assert!(s.apply().is_empty());
    assert!(matches!(s.begin_save(), SaveStep::Confirm(l) if l[0] == text::UNBLOCK_STARTUP_LINE));
}
