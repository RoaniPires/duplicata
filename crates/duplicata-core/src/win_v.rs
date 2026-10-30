//! Opção "Abrir com Win+V": o `V` no `DisabledHotkeys` do Explorer, qual
//! atalho fica registrado (o Win+V ou a reserva, nunca os dois) e o que
//! fazer quando o Explorer volta. A casca implementa as traits com o
//! registro, o `RegisterHotKey` e o processo do shell, e só executa.
//!
//! O reinício do Explorer é medido pela hora de início do processo do shell
//! (o "marcador"): o `TaskbarCreated` também chega sem reinício (mudança de
//! DPI ou de tema), e só a hora diz se o processo é outro.

use std::time::Duration;

use tracing::Level;

use crate::config::Config;
use crate::hotkey::HotkeyCombo;
use crate::log_event;
use crate::log_fields::LogFields;
use crate::settings_text as text;

/// Falha de leitura ou gravação no registro, com o código do Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegistryError(pub u32);

/// O valor `DisabledHotkeys` do Explorer.
pub trait DisabledHotkeysStore {
    /// `Ok(None)` quando o valor não existe.
    fn read(&self) -> Result<Option<String>, RegistryError>;
    fn write(&mut self, value: &str) -> Result<(), RegistryError>;
    fn delete(&mut self) -> Result<(), RegistryError>;
}

/// As duas vagas de atalho global: o Win+V e a reserva (o atalho do campo
/// "Alterar"). As funções deste módulo nunca deixam as duas ocupadas.
pub trait HotkeySlots {
    fn try_win_v(&mut self) -> bool;
    fn release_win_v(&mut self);
    fn register_reserve(&mut self, combo: HotkeyCombo) -> bool;
    fn release_reserve(&mut self);
}

/// O processo do shell que está rodando agora.
pub trait ExplorerInstance {
    /// Hora de criação do processo (FILETIME); `None` sem shell ou sem
    /// conseguir ler.
    fn current(&self) -> Option<u64>;
}

/// O Explorer não aceitou fechar normalmente.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Refused;

/// Reinício do Explorer pedido pelo botão. Nunca força o encerramento.
pub trait ExplorerRestarter {
    fn shutdown(&mut self) -> Result<(), Refused>;
    fn restart(&mut self);
}

/// O que `add_v` pede para o valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddV {
    /// Já tem `V` ou `v`: nada a gravar, e o `V` não é do duplicata.
    AlreadyPresent,
    Write(String),
}

/// O que `remove_v` pede para o valor.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RemoveV {
    Nothing,
    Write(String),
    /// Sobraria vazio: o valor é apagado.
    Delete,
}

fn has_v(value: &str) -> bool {
    value.contains(['V', 'v'])
}

/// Acrescenta `V` no fim, preservando o resto; ausente vira `"V"`.
pub fn add_v(current: Option<&str>) -> AddV {
    match current {
        Some(value) if has_v(value) => AddV::AlreadyPresent,
        Some(value) => AddV::Write(format!("{value}V")),
        None => AddV::Write("V".to_string()),
    }
}

/// Tira o primeiro `V` maiúsculo, em qualquer posição, preservando o resto.
pub fn remove_v(current: Option<&str>) -> RemoveV {
    let Some(value) = current else {
        return RemoveV::Nothing;
    };
    let Some(at) = value.find('V') else {
        return RemoveV::Nothing;
    };
    let rest = format!("{}{}", &value[..at], &value[at + 1..]);
    if rest.is_empty() {
        RemoveV::Delete
    } else {
        RemoveV::Write(rest)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WinVStatus {
    Off,
    /// Ligada; o Win+V passa a valer quando o Explorer reiniciar.
    AwaitingRestart,
    /// O Win+V está registrado; a reserva, não.
    Active,
    /// O Explorer reiniciou e o Win+V continuou indisponível.
    NotReleased,
    /// Desligada com o `V` recém-removido: o Win+V não faz nada até o
    /// Explorer reiniciar.
    OffAwaitingReturn,
}

impl WinVStatus {
    /// A reserva é o atalho que vale (ou deveria valer): com o Win+V em uso,
    /// nada pode registrá-la, nem a janela de conflito.
    pub fn uses_reserve(self) -> bool {
        self != WinVStatus::Active
    }

    /// Estados em que reiniciar o Explorer muda alguma coisa.
    pub fn awaits_restart(self) -> bool {
        matches!(
            self,
            WinVStatus::AwaitingRestart | WinVStatus::OffAwaitingReturn
        )
    }
}

/// Os três campos da opção no `config.toml`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct WinVConfig {
    pub on: bool,
    /// O `V` foi incluído pelo duplicata.
    pub v_ours: bool,
    /// Explorer do último Salvar da opção.
    pub marker: Option<u64>,
}

impl WinVConfig {
    pub fn of(cfg: &Config) -> Self {
        WinVConfig {
            on: cfg.open_with_win_v,
            v_ours: cfg.win_v_added_v,
            marker: cfg.win_v_explorer,
        }
    }

    pub fn write_into(self, cfg: &mut Config) {
        cfg.open_with_win_v = self.on;
        cfg.win_v_added_v = self.v_ours;
        cfg.win_v_explorer = self.marker;
    }
}

/// Resultado de uma mudança: o estado, os campos a gravar e se a reserva
/// devia ficar registrada e não ficou.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Switched {
    pub status: WinVStatus,
    pub config: WinVConfig,
    pub reserve_failed: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartupOutcome {
    pub status: WinVStatus,
    /// Gravar quando difere do salvo (opção desligada pelo item 14,
    /// marcador limpo).
    pub config: WinVConfig,
    pub show_failure_window: bool,
    /// A reserva devia ficar registrada e está em uso por outro programa.
    pub reserve_failed: bool,
}

/// Solta a reserva e tenta o Win+V; sem ele, a reserva volta.
fn take_win_v(slots: &mut dyn HotkeySlots, reserve: HotkeyCombo) -> Result<(), bool> {
    slots.release_reserve();
    if slots.try_win_v() {
        Ok(())
    } else {
        Err(!slots.register_reserve(reserve))
    }
}

/// Mesmo Explorer de quando o marcador foi gravado.
fn same_explorer(marker: Option<u64>, now: Option<u64>) -> Option<bool> {
    match (marker, now) {
        (Some(saved), Some(now)) => Some(saved == now),
        _ => None,
    }
}

/// Inicialização: qual atalho registrar e se a janela de falha abre.
pub fn startup(
    saved: WinVConfig,
    reserve: HotkeyCombo,
    store: &dyn DisabledHotkeysStore,
    slots: &mut dyn HotkeySlots,
    explorer_now: Option<u64>,
) -> StartupOutcome {
    let reserve_only = |slots: &mut dyn HotkeySlots, status, config| StartupOutcome {
        status,
        config,
        show_failure_window: false,
        reserve_failed: !slots.register_reserve(reserve),
    };

    if !saved.on {
        let (status, config) = match saved.marker {
            None => (WinVStatus::Off, saved),
            Some(_) => match same_explorer(saved.marker, explorer_now) {
                Some(false) => (
                    WinVStatus::Off,
                    WinVConfig {
                        marker: None,
                        ..saved
                    },
                ),
                Some(true) => (WinVStatus::OffAwaitingReturn, saved),
                None => {
                    log_event!(Level::WARN, LogFields::new("win_v_explorer_unknown"));
                    (WinVStatus::OffAwaitingReturn, saved)
                }
            },
        };
        return reserve_only(slots, status, config);
    }

    match store.read() {
        Ok(value) if !value.as_deref().is_some_and(has_v) => {
            // Reinstalação com dados preservados ou `V` removido à mão.
            log_event!(Level::WARN, LogFields::new("win_v_v_missing_turned_off"));
            return reserve_only(slots, WinVStatus::Off, WinVConfig::default());
        }
        Ok(_) => {}
        Err(_) => log_event!(Level::WARN, LogFields::new("win_v_registry_read_failed")),
    }

    if slots.try_win_v() {
        return StartupOutcome {
            status: WinVStatus::Active,
            config: saved,
            show_failure_window: false,
            reserve_failed: false,
        };
    }
    let reserve_failed = !slots.register_reserve(reserve);
    let (status, show_failure_window) = match same_explorer(saved.marker, explorer_now) {
        Some(true) => (WinVStatus::AwaitingRestart, false),
        Some(false) => (WinVStatus::NotReleased, true),
        None => {
            log_event!(Level::WARN, LogFields::new("win_v_explorer_unknown"));
            (WinVStatus::AwaitingRestart, false)
        }
    };
    StartupOutcome {
        status,
        config: saved,
        show_failure_window,
        reserve_failed,
    }
}

/// Ligar (Salvar confirmado). Grava antes de mexer nos atalhos: com erro
/// no registro, nada muda.
pub fn enable(
    reserve: HotkeyCombo,
    store: &mut dyn DisabledHotkeysStore,
    slots: &mut dyn HotkeySlots,
    explorer_now: Option<u64>,
) -> Result<Switched, RegistryError> {
    let current = store.read()?;
    let v_ours = match add_v(current.as_deref()) {
        AddV::Write(value) => {
            store.write(&value)?;
            true
        }
        AddV::AlreadyPresent => false,
    };
    let config = WinVConfig {
        on: true,
        v_ours,
        marker: explorer_now,
    };
    let awaiting = Switched {
        status: WinVStatus::AwaitingRestart,
        config,
        reserve_failed: false,
    };
    if v_ours {
        // O Explorer que está rodando ainda é o dono do Win+V.
        return Ok(awaiting);
    }
    Ok(match take_win_v(slots, reserve) {
        Ok(()) => Switched {
            status: WinVStatus::Active,
            ..awaiting
        },
        Err(reserve_failed) => Switched {
            reserve_failed,
            ..awaiting
        },
    })
}

/// Desligar (Salvar confirmado ou "Desligar a opção" na janela de falha).
/// O registro vem antes dos atalhos: com erro nele, o atalho em uso não
/// muda.
pub fn disable(
    saved: WinVConfig,
    status: WinVStatus,
    reserve: HotkeyCombo,
    store: &mut dyn DisabledHotkeysStore,
    slots: &mut dyn HotkeySlots,
    explorer_now: Option<u64>,
) -> Result<Switched, RegistryError> {
    if saved.v_ours {
        match remove_v(store.read()?.as_deref()) {
            RemoveV::Nothing => {}
            RemoveV::Write(value) => store.write(&value)?,
            RemoveV::Delete => store.delete()?,
        }
    }
    let mut reserve_failed = false;
    if status == WinVStatus::Active {
        slots.release_win_v();
        reserve_failed = !slots.register_reserve(reserve);
    }
    // O Win+V só some do Windows se o Explorer reiniciou desde que o `V`
    // entrou; sem saber, vale o "aguardando".
    let status = if saved.v_ours && same_explorer(saved.marker, explorer_now) != Some(true) {
        WinVStatus::OffAwaitingReturn
    } else {
        WinVStatus::Off
    };
    let marker = (status == WinVStatus::OffAwaitingReturn)
        .then_some(explorer_now)
        .flatten();
    Ok(Switched {
        status,
        config: WinVConfig {
            on: false,
            v_ours: false,
            marker,
        },
        reserve_failed,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Reaction {
    pub status: WinVStatus,
    pub config: WinVConfig,
    pub show_failure_window: bool,
    pub close_failure_window: bool,
    pub reserve_failed: bool,
}

/// O `TaskbarCreated` chegou. `last_seen` é o Explorer que esta execução
/// viu por último; é atualizado aqui, para um `TaskbarCreated` sem
/// reinício não reabrir a janela de falha.
pub fn explorer_returned(
    status: WinVStatus,
    saved: WinVConfig,
    reserve: HotkeyCombo,
    last_seen: &mut Option<u64>,
    explorer_now: Option<u64>,
    slots: &mut dyn HotkeySlots,
) -> Reaction {
    let restarted =
        explorer_now.is_some() && explorer_now != *last_seen && explorer_now != saved.marker;
    if explorer_now.is_some() {
        *last_seen = explorer_now;
    }
    let mut reaction = Reaction {
        status,
        config: saved,
        show_failure_window: false,
        close_failure_window: false,
        reserve_failed: false,
    };
    match status {
        WinVStatus::AwaitingRestart | WinVStatus::NotReleased => match take_win_v(slots, reserve) {
            Ok(()) => {
                reaction.status = WinVStatus::Active;
                reaction.close_failure_window = true;
            }
            Err(reserve_failed) => {
                reaction.reserve_failed = reserve_failed;
                if restarted {
                    reaction.status = WinVStatus::NotReleased;
                    reaction.show_failure_window = true;
                }
            }
        },
        WinVStatus::OffAwaitingReturn => {
            if explorer_now.is_some() && explorer_now != saved.marker {
                reaction.status = WinVStatus::Off;
                reaction.config.marker = None;
            }
        }
        WinVStatus::Active | WinVStatus::Off => {}
    }
    reaction
}

/// Prazo para o Explorer voltar depois de fechar pelo botão.
pub const EXPLORER_RETURN_TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExplorerRestartOutcome {
    /// O Explorer recusou fechar; nada mudou.
    Refused,
    Returned,
    /// Não voltou no prazo; o duplicata o abriu de novo.
    NotReturnedRelaunched,
    NotReturnedRelaunchFailed,
}

/// Linha de erro das Configurações para o resultado do botão.
pub fn restart_error_line(outcome: ExplorerRestartOutcome) -> Option<&'static str> {
    match outcome {
        ExplorerRestartOutcome::Returned => None,
        ExplorerRestartOutcome::Refused | ExplorerRestartOutcome::NotReturnedRelaunched => {
            Some(text::EXPLORER_RESTART_FAILED)
        }
        ExplorerRestartOutcome::NotReturnedRelaunchFailed => Some(text::EXPLORER_NOT_RETURNED),
    }
}

/// O botão "Reiniciar o Windows Explorer agora", fora da thread da
/// interface. `wait_returned` espera o aviso de que o Explorer voltou por
/// até o prazo; só relança quando, passado o prazo, não há shell.
pub fn run_explorer_restart(
    restarter: &mut dyn ExplorerRestarter,
    wait_returned: impl FnOnce(Duration) -> bool,
    shell_present: impl FnOnce() -> bool,
    relaunch: impl FnOnce() -> bool,
) -> ExplorerRestartOutcome {
    if restarter.shutdown().is_err() {
        return ExplorerRestartOutcome::Refused;
    }
    restarter.restart();
    if wait_returned(EXPLORER_RETURN_TIMEOUT) || shell_present() {
        return ExplorerRestartOutcome::Returned;
    }
    if relaunch() {
        ExplorerRestartOutcome::NotReturnedRelaunched
    } else {
        ExplorerRestartOutcome::NotReturnedRelaunchFailed
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UninstallRelease {
    /// Sem saber que o `V` é do duplicata, ou sem `V`: registro intocado.
    Untouched,
    Removed,
    Failed(RegistryError),
}

/// Desinstalação: tira o `V` só quando o `config.toml` diz, sem fallback,
/// que foi o duplicata que o incluiu.
pub fn uninstall_release(
    config_text: Option<&str>,
    store: &mut dyn DisabledHotkeysStore,
) -> UninstallRelease {
    let Some(contents) = config_text else {
        return UninstallRelease::Untouched;
    };
    let base = Config::with_paths(Default::default(), Default::default());
    // Valor inválido cai no padrão (não incluído): também fica intocado.
    let (cfg, _) = Config::from_str_over(base, contents);
    if !cfg.win_v_added_v {
        return UninstallRelease::Untouched;
    }
    let result = store
        .read()
        .and_then(|current| match remove_v(current.as_deref()) {
            RemoveV::Nothing => Ok(false),
            RemoveV::Write(value) => store.write(&value).map(|()| true),
            RemoveV::Delete => store.delete().map(|()| true),
        });
    match result {
        Ok(true) => UninstallRelease::Removed,
        Ok(false) => UninstallRelease::Untouched,
        Err(e) => UninstallRelease::Failed(e),
    }
}

/// Texto da janela de falha depois de "Desligar a opção".
pub fn turned_off_notice(v_was_ours: bool) -> &'static str {
    if v_was_ours {
        text::WIN_V_RETURNS_AFTER
    } else {
        text::WIN_V_WAS_DISABLED_BEFORE
    }
}

/// Linha de erro da janela de falha depois de "Desligar a opção": o registro
/// mudou, mas o `config.toml` não foi gravado.
pub fn turned_off_error(persisted: bool) -> Option<&'static str> {
    (!persisted).then_some(text::SAVE_FAILED)
}

/// Mensagem da desinstalação quando o `V` do duplicata não pôde ser tirado.
pub fn uninstall_release_message(outcome: UninstallRelease) -> Option<&'static str> {
    matches!(outcome, UninstallRelease::Failed(_)).then_some(text::WIN_V_UNINSTALL_FAILED)
}

/// Ligou, mas o `config.toml` não foi gravado: sem `win_v_added_v`, o `V`
/// passaria a ser tratado como alheio e ficaria para trás. Primeiro tenta
/// gravar só os campos do Win+V (`persist_fields`); se também falhar, desfaz
/// a ligação (tira o `V` que acabou de entrar, volta à reserva) e devolve o
/// estado desligado. `None`: os campos foram gravados, nada muda.
pub fn undo_unsaved_enable(
    enabled: Switched,
    reserve: HotkeyCombo,
    store: &mut dyn DisabledHotkeysStore,
    slots: &mut dyn HotkeySlots,
    persist_fields: impl FnOnce(WinVConfig) -> bool,
) -> Option<Switched> {
    if persist_fields(enabled.config) {
        return None;
    }
    if enabled.config.v_ours {
        // Só o `V` que acabou de entrar; sem conseguir tirá-lo, ele fica
        // como alheio (o lado que não apaga nada que não seja nosso).
        let removed = store
            .read()
            .and_then(|current| match remove_v(current.as_deref()) {
                RemoveV::Nothing => Ok(()),
                RemoveV::Write(value) => store.write(&value),
                RemoveV::Delete => store.delete(),
            });
        if removed.is_err() {
            log_event!(Level::WARN, LogFields::new("win_v_undo_enable_failed"));
        }
    }
    let mut reserve_failed = enabled.reserve_failed;
    if enabled.status == WinVStatus::Active {
        slots.release_win_v();
        reserve_failed = !slots.register_reserve(reserve);
    }
    Some(Switched {
        status: WinVStatus::Off,
        config: WinVConfig::default(),
        reserve_failed,
    })
}

/// Depois do `TaskbarCreated`: abrir a janela de conflito da reserva? Só
/// quando a reserva não registrou, a janela de falha não vai abrir nem está
/// aberta (ela abre a de conflito ao fechar) e a de conflito ainda não está
/// aberta (trazê-la de novo à frente roubaria o foco a cada troca de DPI ou
/// de tema).
pub fn open_conflict_after_return(
    reaction: &Reaction,
    failure_window_open: bool,
    conflict_window_open: bool,
) -> bool {
    reaction.reserve_failed
        && !reaction.show_failure_window
        && !failure_window_open
        && !conflict_window_open
}
