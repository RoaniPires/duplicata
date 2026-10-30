//! Sessão da janela de Configurações: o que foi salvo quando ela abriu, o
//! que está em efeito e o rascunho dos controles. Decide o que é reversível,
//! o que o Aplicar, o Cancelar e o Salvar colocam em efeito e o texto da
//! confirmação; a casca só executa os [`Effect`] devolvidos.

use std::time::Duration;

use crate::config::Config;
use crate::hotkey::{format_hotkey, HotkeyCombo};
use crate::settings_input::{
    digits_only, normalize_max_items, normalize_max_pinned, normalize_retention_days,
};
use crate::settings_text as text;
use crate::startup::{Approval, StartupState};
use crate::win_v::{WinVConfig, WinVStatus};

const SECS_PER_DAY: u64 = 24 * 60 * 60;

/// Nome sem espaços nas pontas; vazio ou repetido (sem diferenciar
/// maiúsculas) não entra.
pub fn add_program(list: &mut Vec<String>, text: &str) -> bool {
    let name = text.trim();
    if name.is_empty() || list.iter().any(|p| p.eq_ignore_ascii_case(name)) {
        return false;
    }
    list.push(name.to_string());
    true
}

pub fn remove_selected(list: &mut Vec<String>, index: Option<usize>) -> bool {
    match index {
        Some(i) if i < list.len() => {
            list.remove(i);
            true
        }
        _ => false,
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettingsValues {
    pub hotkey: HotkeyCombo,
    pub startup: bool,
    pub retention_days: u32,
    pub max_items: u32,
    pub max_pinned: u32,
    pub blocked_programs: Vec<String>,
    pub secret_detection: bool,
    pub protection: bool,
    /// "Abrir com Win+V"; muda só pelo Salvar confirmado.
    pub win_v: bool,
}

impl SettingsValues {
    pub fn from_config(cfg: &Config, startup: StartupState) -> Self {
        let days = cfg.retention.as_secs() / SECS_PER_DAY;
        SettingsValues {
            hotkey: cfg.hotkey,
            startup: startup == StartupState::Ligado,
            retention_days: u32::try_from(days).unwrap_or(u32::MAX),
            max_items: cfg.max_items,
            max_pinned: cfg.max_pinned,
            blocked_programs: cfg.blocked_programs.clone(),
            secret_detection: cfg.heuristic_secret_detection,
            protection: cfg.encryption_enabled,
            win_v: cfg.open_with_win_v,
        }
    }

    /// Fora ficam a inicialização (vive no registro) e o atalho: ele só muda
    /// na configuração quando a troca dá certo, e o diálogo de conflito
    /// também o grava com a janela aberta.
    pub fn write_into(&self, cfg: &mut Config) {
        cfg.retention = Duration::from_secs(u64::from(self.retention_days) * SECS_PER_DAY);
        cfg.max_items = self.max_items;
        cfg.max_pinned = self.max_pinned;
        cfg.blocked_programs = self.blocked_programs.clone();
        cfg.heuristic_secret_detection = self.secret_detection;
        cfg.encryption_enabled = self.protection;
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Setting {
    Hotkey,
    Startup,
    RetentionDays,
    MaxItems,
    MaxPinned,
    BlockedPrograms,
    SecretDetection,
    Protection,
    WinV,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChangeKind {
    /// Vale no Aplicar e o Cancelar desfaz.
    Reversible,
    /// Só no Salvar, sem confirmação (a proteção: reversível, mas pesada).
    SaveOnly,
    /// Só no Salvar, com confirmação.
    Irreversible,
    /// Só no Salvar, com confirmação, mas reversível (o Win+V): a
    /// confirmação não diz que não pode ser desfeito.
    SaveOnlyConfirmed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NumericField {
    RetentionDays,
    MaxItems,
    MaxPinned,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Effect {
    SwapHotkey {
        from: HotkeyCombo,
        to: HotkeyCombo,
    },
    SetStartup {
        enable: bool,
        /// Ligar removendo a desativação feita no Gerenciador de Tarefas.
        clear_block: bool,
    },
    SetCaptureFilters {
        blocked_programs: Vec<String>,
        secret_detection: bool,
    },
    SetRetention {
        days: u32,
        max_items: u32,
    },
    SetMaxPinned(u32),
    ToggleProtection(bool),
    /// Liga o Win+V (registro, atalhos e campos do `config.toml`).
    EnableWinV,
    DisableWinV,
    /// Troca a reserva sem registrar: o Win+V está em uso.
    SetReserve(HotkeyCombo),
    /// Gravar o arquivo de configuração; sempre o último efeito do Salvar.
    Persist,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveStep {
    /// Linhas da confirmação: as irreversíveis terminam em
    /// [`text::CANNOT_UNDO`]; o bloco do Win+V vem por último, separado por
    /// uma linha vazia quando há irreversíveis.
    Confirm(Vec<String>),
    Proceed(Vec<Effect>),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SaveReport {
    pub hotkey_failed: bool,
    pub persist_failed: bool,
    /// O `DisabledHotkeys` não pôde ser gravado.
    pub win_v_failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SaveOutcome {
    Close,
    StayOpen { error_line: Option<&'static str> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HotkeyLineView {
    pub combo: String,
    pub description: String,
    pub alter_enabled: bool,
    /// Dica do Win+V enquanto o campo espera a combinação.
    pub hint: Option<&'static str>,
    /// Linha de estado do Win+V, abaixo do atalho.
    pub status_line: Option<String>,
    pub restart_visible: bool,
    pub restart_enabled: bool,
}

/// Texto do campo e o último valor que a janela escreveu nele (o "anterior"
/// para onde um campo vazio volta).
#[derive(Debug, Clone)]
struct NumericInput {
    text: String,
    shown: u32,
}

impl NumericInput {
    fn new(value: u32) -> Self {
        NumericInput {
            text: value.to_string(),
            shown: value,
        }
    }

    fn reset(&mut self, value: u32) {
        *self = NumericInput::new(value);
    }
}

#[derive(Debug, Clone)]
pub struct SettingsSession {
    saved: SettingsValues,
    in_effect: SettingsValues,
    draft: SettingsValues,
    retention: NumericInput,
    items: NumericInput,
    pinned: NumericInput,
    startup_blocked: bool,
    protection_busy: bool,
    hotkey_message: Option<String>,
    last_swap: Option<(HotkeyCombo, HotkeyCombo)>,
    win_v_status: WinVStatus,
    win_v_ours: bool,
    restart_in_progress: bool,
}

impl SettingsSession {
    pub fn open(cfg: &Config, startup: StartupState) -> Self {
        let values = SettingsValues::from_config(cfg, startup);
        SettingsSession {
            retention: NumericInput::new(values.retention_days),
            items: NumericInput::new(values.max_items),
            pinned: NumericInput::new(values.max_pinned),
            saved: values.clone(),
            in_effect: values.clone(),
            draft: values,
            startup_blocked: startup == StartupState::DesligadoPeloGerenciador,
            protection_busy: false,
            hotkey_message: None,
            last_swap: None,
            win_v_status: WinVStatus::Off,
            win_v_ours: cfg.win_v_added_v,
            restart_in_progress: false,
        }
    }

    pub fn saved(&self) -> &SettingsValues {
        &self.saved
    }

    pub fn in_effect(&self) -> &SettingsValues {
        &self.in_effect
    }

    pub fn draft(&self) -> &SettingsValues {
        &self.draft
    }

    fn input(&self, field: NumericField) -> &NumericInput {
        match field {
            NumericField::RetentionDays => &self.retention,
            NumericField::MaxItems => &self.items,
            NumericField::MaxPinned => &self.pinned,
        }
    }

    pub fn numeric_text(&self, field: NumericField) -> &str {
        &self.input(field).text
    }

    pub fn set_numeric_text(&mut self, field: NumericField, text: &str) {
        let text = digits_only(text);
        match field {
            NumericField::RetentionDays => {
                self.draft.retention_days = normalize_retention_days(&text, self.retention.shown);
                self.retention.text = text;
            }
            NumericField::MaxItems => {
                self.draft.max_items = normalize_max_items(&text, self.items.shown);
                self.items.text = text;
            }
            NumericField::MaxPinned => {
                self.draft.max_pinned = normalize_max_pinned(&text, self.pinned.shown);
                self.pinned.text = text;
            }
        }
    }

    /// Reescreve os campos com o valor ajustado do rascunho.
    fn rewrite_fields(&mut self) {
        self.retention.reset(self.draft.retention_days);
        self.items.reset(self.draft.max_items);
        self.pinned.reset(self.draft.max_pinned);
    }

    fn fields_need_rewrite(&self) -> bool {
        [
            (&self.retention, self.draft.retention_days),
            (&self.items, self.draft.max_items),
            (&self.pinned, self.draft.max_pinned),
        ]
        .iter()
        .any(|(input, value)| input.text != value.to_string())
    }

    pub fn set_startup(&mut self, on: bool) {
        self.draft.startup = on;
    }

    pub fn set_secret_detection(&mut self, on: bool) {
        self.draft.secret_detection = on;
    }

    pub fn set_protection(&mut self, on: bool) {
        if !self.protection_busy {
            self.draft.protection = on;
        }
    }

    pub fn add_program(&mut self, text: &str) -> bool {
        add_program(&mut self.draft.blocked_programs, text)
    }

    pub fn remove_program(&mut self, index: Option<usize>) -> bool {
        remove_selected(&mut self.draft.blocked_programs, index)
    }

    pub fn startup_blocked(&self) -> bool {
        self.startup_blocked
    }

    pub fn startup_description(&self) -> &'static str {
        if self.startup_blocked {
            text::STARTUP_BLOCKED
        } else {
            text::STARTUP.description
        }
    }

    /// Cada configuração em que o rascunho difere do que está em efeito, na
    /// ordem da janela. Reduzir é medido contra o valor salvo.
    pub fn classify(&self) -> Vec<(Setting, ChangeKind)> {
        use ChangeKind::*;
        let (d, e, s) = (&self.draft, &self.in_effect, &self.saved);
        let mut out = Vec::new();
        if d.hotkey != e.hotkey {
            out.push((Setting::Hotkey, Reversible));
        }
        if d.startup != e.startup {
            let kind = if d.startup && self.startup_blocked {
                Irreversible
            } else {
                Reversible
            };
            out.push((Setting::Startup, kind));
        }
        let reduce = |draft: u32, effect: u32, saved: u32| {
            if draft < saved {
                Some(Irreversible)
            } else if draft != effect {
                Some(Reversible)
            } else {
                None
            }
        };
        if let Some(kind) = reduce(d.retention_days, e.retention_days, s.retention_days) {
            out.push((Setting::RetentionDays, kind));
        }
        if let Some(kind) = reduce(d.max_items, e.max_items, s.max_items) {
            out.push((Setting::MaxItems, kind));
        }
        if d.max_pinned != e.max_pinned {
            out.push((Setting::MaxPinned, Reversible));
        }
        if d.blocked_programs != e.blocked_programs {
            out.push((Setting::BlockedPrograms, Reversible));
        }
        if d.secret_detection != e.secret_detection {
            out.push((Setting::SecretDetection, Reversible));
        }
        if d.protection != e.protection {
            out.push((Setting::Protection, SaveOnly));
        }
        if d.win_v != e.win_v {
            out.push((Setting::WinV, SaveOnlyConfirmed));
        }
        out
    }

    pub fn can_apply(&self) -> bool {
        self.fields_need_rewrite()
            || self
                .classify()
                .iter()
                .any(|(_, kind)| *kind == ChangeKind::Reversible)
    }

    /// Efeitos que levam `from` a `to`, na ordem da janela.
    fn transition(&mut self, from: &SettingsValues, to: &SettingsValues) -> Vec<Effect> {
        let mut out = Vec::new();
        if from.hotkey != to.hotkey {
            if self.win_v_status == WinVStatus::Active {
                // Com o Win+V em uso a reserva não fica registrada.
                out.push(Effect::SetReserve(to.hotkey));
            } else {
                self.last_swap = Some((from.hotkey, to.hotkey));
                out.push(Effect::SwapHotkey {
                    from: from.hotkey,
                    to: to.hotkey,
                });
            }
        }
        if from.startup != to.startup {
            out.push(Effect::SetStartup {
                enable: to.startup,
                clear_block: to.startup && self.startup_blocked,
            });
        }
        if from.blocked_programs != to.blocked_programs
            || from.secret_detection != to.secret_detection
        {
            out.push(Effect::SetCaptureFilters {
                blocked_programs: to.blocked_programs.clone(),
                secret_detection: to.secret_detection,
            });
        }
        if from.retention_days != to.retention_days || from.max_items != to.max_items {
            out.push(Effect::SetRetention {
                days: to.retention_days,
                max_items: to.max_items,
            });
        }
        if from.max_pinned != to.max_pinned {
            out.push(Effect::SetMaxPinned(to.max_pinned));
        }
        if from.protection != to.protection {
            out.push(Effect::ToggleProtection(to.protection));
        }
        if from.win_v != to.win_v {
            out.push(if to.win_v {
                Effect::EnableWinV
            } else {
                Effect::DisableWinV
            });
        }
        out
    }

    /// Coloca em efeito só as mudanças reversíveis; as só-no-Salvar
    /// continuam no rascunho. Não grava.
    pub fn apply(&mut self) -> Vec<Effect> {
        let mut next = self.in_effect.clone();
        for (setting, kind) in self.classify() {
            if kind != ChangeKind::Reversible {
                continue;
            }
            let d = &self.draft;
            match setting {
                Setting::Hotkey => next.hotkey = d.hotkey,
                Setting::Startup => next.startup = d.startup,
                Setting::RetentionDays => next.retention_days = d.retention_days,
                Setting::MaxItems => next.max_items = d.max_items,
                Setting::MaxPinned => next.max_pinned = d.max_pinned,
                Setting::BlockedPrograms => next.blocked_programs = d.blocked_programs.clone(),
                Setting::SecretDetection => next.secret_detection = d.secret_detection,
                Setting::Protection | Setting::WinV => {}
            }
        }
        self.rewrite_fields();
        let current = self.in_effect.clone();
        let effects = self.transition(&current, &next);
        self.in_effect = next;
        effects
    }

    /// Volta ao salvo o que foi aplicado; o que nunca foi aplicado não gera
    /// efeito. Não grava.
    pub fn cancel(&mut self) -> Vec<Effect> {
        let (current, saved) = (self.in_effect.clone(), self.saved.clone());
        let effects = self.transition(&current, &saved);
        self.in_effect = saved.clone();
        self.draft = saved;
        self.rewrite_fields();
        effects
    }

    pub fn begin_save(&mut self) -> SaveStep {
        self.rewrite_fields();
        let irreversible: Vec<Setting> = self
            .classify()
            .into_iter()
            .filter(|(_, kind)| *kind == ChangeKind::Irreversible)
            .map(|(setting, _)| setting)
            .collect();
        // Na ordem da confirmação: prazo, quantidade, inicialização.
        let mut lines = Vec::new();
        if irreversible.contains(&Setting::RetentionDays) {
            lines.push(text::reduce_retention_line(
                self.saved.retention_days,
                self.draft.retention_days,
            ));
        }
        if irreversible.contains(&Setting::MaxItems) {
            lines.push(text::reduce_items_line(
                self.saved.max_items,
                self.draft.max_items,
            ));
        }
        if irreversible.contains(&Setting::Startup) {
            lines.push(text::UNBLOCK_STARTUP_LINE.to_string());
        }
        if !lines.is_empty() {
            lines.push(text::CANNOT_UNDO.to_string());
        }
        // O Win+V é reversível: vem depois, separado, sem a frase acima.
        let win_v = self.win_v_lines();
        if !win_v.is_empty() {
            if !lines.is_empty() {
                lines.push(String::new());
            }
            lines.extend(win_v);
        }
        if lines.is_empty() {
            return SaveStep::Proceed(self.confirm_save());
        }
        SaveStep::Confirm(lines)
    }

    fn win_v_lines(&self) -> Vec<String> {
        let (draft, effect) = (self.draft.win_v, self.in_effect.win_v);
        if draft == effect {
            Vec::new()
        } else if draft {
            vec![
                text::WIN_V_ENABLE_PANEL_LINE.to_string(),
                text::win_v_enable_when(&format_hotkey(&self.draft.hotkey)),
            ]
        } else if self.win_v_ours {
            vec![text::WIN_V_DISABLE_LINE.to_string()]
        } else {
            vec![
                text::WIN_V_DISABLE_NOW_LINE.to_string(),
                text::WIN_V_WAS_DISABLED_BEFORE.to_string(),
            ]
        }
    }

    /// Tudo o que está pendente, com a gravação por último.
    pub fn confirm_save(&mut self) -> Vec<Effect> {
        let (current, draft) = (self.in_effect.clone(), self.draft.clone());
        let mut effects = self.transition(&current, &draft);
        effects.push(Effect::Persist);
        if effects.contains(&Effect::ToggleProtection(draft.protection)) {
            self.protection_busy = true;
        }
        if draft.startup {
            self.startup_blocked = false;
        }
        self.in_effect = draft;
        effects
    }

    /// "Voltar" na confirmação: nada muda.
    pub fn back(&mut self) {}

    pub fn finish_save(&mut self, report: SaveReport) -> SaveOutcome {
        if report.hotkey_failed {
            self.hotkey_effect_failed();
        }
        if report.win_v_failed {
            // O registro não mudou: a opção volta ao que era.
            self.in_effect.win_v = self.saved.win_v;
            self.draft.win_v = self.saved.win_v;
        }
        if !report.hotkey_failed && !report.persist_failed && !report.win_v_failed {
            return SaveOutcome::Close;
        }
        // O que o Salvar pôs em efeito vira o ponto de partida: um Cancelar
        // depois só fecha.
        self.saved = self.in_effect.clone();
        let error_line = if report.persist_failed {
            Some(text::SAVE_FAILED)
        } else if report.win_v_failed {
            Some(text::WIN_V_REGISTRY_FAILED)
        } else {
            None
        };
        SaveOutcome::StayOpen { error_line }
    }

    pub fn hotkey_line(
        &self,
        registered: Option<HotkeyCombo>,
        armed: bool,
        standalone_open: bool,
    ) -> HotkeyLineView {
        let combo = format_hotkey(&self.draft.hotkey);
        let (description, alter_enabled) = if standalone_open {
            (text::HOTKEY_STANDALONE.to_string(), false)
        } else if let Some(message) = &self.hotkey_message {
            (message.clone(), true)
        } else if armed {
            (text::HOTKEY_ARMED.to_string(), true)
        } else if self.has_pending_hotkey() {
            (text::HOTKEY_PENDING.to_string(), true)
        } else if registered.is_none() && self.win_v_status != WinVStatus::Active {
            (text::hotkey_none_active(&combo), true)
        } else {
            (text::HOTKEY.description.to_string(), true)
        };
        HotkeyLineView {
            combo,
            description,
            alter_enabled,
            hint: (armed && !standalone_open).then_some(text::WIN_V_HINT),
            status_line: self.win_v_status_line(),
            restart_visible: self.win_v_status.awaits_restart(),
            restart_enabled: self.win_v_status.awaits_restart() && !self.restart_in_progress,
        }
    }

    fn win_v_status_line(&self) -> Option<String> {
        let reserve = format_hotkey(&self.in_effect.hotkey);
        match self.win_v_status {
            WinVStatus::Off => None,
            WinVStatus::AwaitingRestart => Some(text::win_v_awaiting(&reserve)),
            WinVStatus::Active => Some(text::win_v_active(&reserve)),
            WinVStatus::NotReleased => Some(text::win_v_not_released(&reserve)),
            WinVStatus::OffAwaitingReturn => Some(text::WIN_V_OFF_AWAITING_RETURN.to_string()),
        }
    }

    pub fn hotkey_armed(&mut self) {
        self.hotkey_message = None;
    }

    pub fn hotkey_disarmed(&mut self) {
        self.hotkey_message = None;
    }

    /// Recusa da combinação pressionada; nada fica pendente.
    pub fn set_hotkey_message(&mut self, message: String) {
        self.hotkey_message = Some(message);
    }

    /// Só depois de a casca conferir que a combinação está livre.
    pub fn set_pending_hotkey(&mut self, combo: HotkeyCombo) {
        self.draft.hotkey = combo;
        self.hotkey_message = None;
    }

    pub fn has_pending_hotkey(&self) -> bool {
        self.draft.hotkey != self.in_effect.hotkey
    }

    /// A troca pedida no último Aplicar/Salvar falhou: a anterior continua.
    pub fn hotkey_effect_failed(&mut self) {
        if let Some((from, to)) = self.last_swap.take() {
            self.in_effect.hotkey = from;
            self.draft.hotkey = from;
            self.hotkey_message = Some(text::hotkey_taken(
                &format_hotkey(&to),
                &format_hotkey(&from),
            ));
        }
    }

    /// Reescrita do banco em andamento ao abrir: a alternância mostra o
    /// destino e fica travada até [`Self::protection_finished`].
    pub fn protection_in_progress(&mut self, target: bool) {
        self.saved.protection = target;
        self.in_effect.protection = target;
        self.draft.protection = target;
        self.protection_busy = true;
    }

    pub fn protection_finished(&mut self) {
        self.protection_busy = false;
    }

    pub fn protection_busy(&self) -> bool {
        self.protection_busy
    }

    pub fn set_win_v(&mut self, on: bool) {
        self.draft.win_v = on;
    }

    /// O estado do Win+V mudou fora da janela (inicialização, reinício do
    /// Explorer, janela de falha) ou depois de um Salvar.
    pub fn sync_win_v(&mut self, config: WinVConfig, status: WinVStatus) {
        if self.draft.win_v == self.in_effect.win_v {
            self.draft.win_v = config.on;
        }
        self.saved.win_v = config.on;
        self.in_effect.win_v = config.on;
        self.win_v_ours = config.v_ours;
        self.win_v_status = status;
    }

    pub fn win_v_status(&self) -> WinVStatus {
        self.win_v_status
    }

    /// Confirmação do botão "Reiniciar o Windows Explorer agora"; `None`
    /// quando o botão não vale agora.
    pub fn begin_explorer_restart(&self) -> Option<Vec<String>> {
        (self.win_v_status.awaits_restart() && !self.restart_in_progress)
            .then(|| vec![text::RESTART_EXPLORER_CONFIRM.to_string()])
    }

    pub fn explorer_restart_started(&mut self) {
        self.restart_in_progress = true;
    }

    pub fn explorer_restart_finished(&mut self) {
        self.restart_in_progress = false;
    }
}

/// Estado da inicialização como as Configurações o tratam: a desativação
/// gravada pelo Gerenciador de Tarefas bloqueia mesmo sem a entrada de
/// execução (ligar a recriaria ainda desativada).
pub fn startup_for_settings(entry_present: bool, approval: Approval) -> StartupState {
    if approval == Approval::Desaprovado {
        return StartupState::DesligadoPeloGerenciador;
    }
    crate::startup::startup_state(entry_present, approval)
}
