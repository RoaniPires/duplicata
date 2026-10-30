//! O estado da opção "Abrir com Win+V" enquanto o duplicata roda, dividido
//! entre a bandeja (que recebe o `TaskbarCreated`), as Configurações e a
//! janela de falha. Toda decisão vem de `duplicata_core::win_v`; aqui só
//! ficam as vagas de atalho reais, o registro, o processo do shell e a
//! gravação dos três campos no `config.toml`.

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc;

use duplicata_core::win_v::{
    self, DisabledHotkeysStore, ExplorerInstance, HotkeySlots, Reaction, RegistryError,
    StartupOutcome, Switched, WinVConfig, WinVStatus,
};
use duplicata_core::{log_event, Config, HotkeyCombo, LogFields};
use tracing::Level;
use windows::Win32::Foundation::HWND;

use crate::hotkey_dialog;
use crate::hotkey_win;

/// As vagas de atalho na janela do Histórico. `registered` é a reserva
/// registrada agora (o que as Configurações e a janela de conflito usam).
struct WindowSlots {
    hwnd: HWND,
    registered: Rc<Cell<Option<HotkeyCombo>>>,
}

impl HotkeySlots for WindowSlots {
    fn try_win_v(&mut self) -> bool {
        hotkey_win::register_win_v(self.hwnd).is_ok()
    }

    fn release_win_v(&mut self) {
        let _ = hotkey_win::unregister_win_v(self.hwnd);
    }

    fn register_reserve(&mut self, combo: HotkeyCombo) -> bool {
        let ok = hotkey_win::register(self.hwnd, &combo).is_ok();
        self.registered.set(ok.then_some(combo));
        ok
    }

    fn release_reserve(&mut self) {
        if self.registered.take().is_some() {
            let _ = hotkey_win::unregister(self.hwnd);
        }
    }
}

pub struct WinVRuntime {
    history_hwnd: HWND,
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
    store: RefCell<Box<dyn DisabledHotkeysStore>>,
    slots: RefCell<Box<dyn HotkeySlots>>,
    explorer: Box<dyn ExplorerInstance>,
    status: Cell<WinVStatus>,
    /// O Explorer que esta execução viu por último.
    last_seen: Cell<Option<u64>>,
    /// Aviso de que o Explorer voltou, para a thread do botão de reiniciar;
    /// só existe enquanto ela roda.
    returned: RefCell<Option<mpsc::Sender<()>>>,
}

impl WinVRuntime {
    /// Com as vagas reais na janela do Histórico.
    pub fn new(
        history_hwnd: HWND,
        config: Rc<RefCell<Config>>,
        config_path: PathBuf,
        registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
        store: Box<dyn DisabledHotkeysStore>,
        explorer: Box<dyn ExplorerInstance>,
    ) -> Rc<Self> {
        let slots = Box::new(WindowSlots {
            hwnd: history_hwnd,
            registered: Rc::clone(&registered_hotkey),
        });
        Self::with_slots(
            history_hwnd,
            config,
            config_path,
            registered_hotkey,
            store,
            slots,
            explorer,
        )
    }

    /// Com vagas próprias (os testes usam vagas em memória).
    pub fn with_slots(
        history_hwnd: HWND,
        config: Rc<RefCell<Config>>,
        config_path: PathBuf,
        registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
        store: Box<dyn DisabledHotkeysStore>,
        slots: Box<dyn HotkeySlots>,
        explorer: Box<dyn ExplorerInstance>,
    ) -> Rc<Self> {
        Rc::new(WinVRuntime {
            history_hwnd,
            config,
            config_path,
            registered_hotkey,
            store: RefCell::new(store),
            slots: RefCell::new(slots),
            explorer,
            status: Cell::new(WinVStatus::Off),
            last_seen: Cell::new(None),
            returned: RefCell::new(None),
        })
    }

    pub fn history_hwnd(&self) -> HWND {
        self.history_hwnd
    }

    pub fn config(&self) -> &Rc<RefCell<Config>> {
        &self.config
    }

    pub fn config_path(&self) -> &PathBuf {
        &self.config_path
    }

    pub fn registered_hotkey(&self) -> &Rc<Cell<Option<HotkeyCombo>>> {
        &self.registered_hotkey
    }

    pub fn status(&self) -> WinVStatus {
        self.status.get()
    }

    pub fn win_v_config(&self) -> WinVConfig {
        WinVConfig::of(&self.config.borrow())
    }

    pub fn reserve(&self) -> HotkeyCombo {
        self.config.borrow().hotkey
    }

    /// Com o Win+V em uso a reserva não pode ser registrada: a janela de
    /// conflito dela, se aberta, fecha (senão escolher uma combinação ali
    /// deixaria dois atalhos registrados).
    fn set_status(&self, status: WinVStatus) {
        self.status.set(status);
        if !status.uses_reserve() {
            hotkey_dialog::close_conflict();
        }
    }

    /// Escreve os campos na configuração compartilhada e, se `persist`, só
    /// eles no arquivo. `false` só quando a gravação pedida falhou.
    fn adopt(&self, config: WinVConfig, persist: bool) -> bool {
        if WinVConfig::of(&self.config.borrow()) == config {
            return true;
        }
        config.write_into(&mut self.config.borrow_mut());
        if !persist {
            return true;
        }
        let written = self.persist_fields(config);
        if !written {
            log_event!(Level::WARN, LogFields::new("win_v_persist_failed"));
        }
        written
    }

    /// Grava só os três campos do Win+V no arquivo.
    fn persist_fields(&self, config: WinVConfig) -> bool {
        hotkey_dialog::persist_only(&self.config_path, &self.config.borrow(), |saved| {
            config.write_into(saved)
        })
        .is_ok()
    }

    /// Registra o Win+V ou a reserva na inicialização.
    pub fn startup(&self) -> StartupOutcome {
        let now = self.explorer.current();
        self.last_seen.set(now);
        let outcome = win_v::startup(
            self.win_v_config(),
            self.reserve(),
            self.store.borrow().as_ref(),
            self.slots.borrow_mut().as_mut(),
            now,
        );
        self.set_status(outcome.status);
        self.adopt(outcome.config, true);
        outcome
    }

    /// Ligar pelo Salvar. Não grava o arquivo: o `Persist` do Salvar grava
    /// (e, se ele falhar, [`Self::undo_unsaved_enable`]).
    pub fn enable(&self) -> Result<Switched, RegistryError> {
        let switched = win_v::enable(
            self.reserve(),
            self.store.borrow_mut().as_mut(),
            self.slots.borrow_mut().as_mut(),
            self.explorer.current(),
        )?;
        self.set_status(switched.status);
        self.adopt(switched.config, false);
        Ok(switched)
    }

    /// O `config.toml` do Salvar falhou depois de ligar: grava só os campos
    /// do Win+V ou desfaz a ligação. `true` quando desfez.
    pub fn undo_unsaved_enable(&self, enabled: Switched) -> bool {
        let undone = win_v::undo_unsaved_enable(
            enabled,
            self.reserve(),
            self.store.borrow_mut().as_mut(),
            self.slots.borrow_mut().as_mut(),
            |config| self.persist_fields(config),
        );
        let Some(undone) = undone else {
            return false;
        };
        log_event!(Level::WARN, LogFields::new("win_v_enable_undone"));
        self.set_status(undone.status);
        self.adopt(undone.config, false);
        true
    }

    fn disable_switch(&self) -> Result<Switched, RegistryError> {
        let switched = win_v::disable(
            self.win_v_config(),
            self.status.get(),
            self.reserve(),
            self.store.borrow_mut().as_mut(),
            self.slots.borrow_mut().as_mut(),
            self.explorer.current(),
        )?;
        self.set_status(switched.status);
        Ok(switched)
    }

    /// Desligar pelo Salvar: o `Persist` dele grava.
    pub fn disable(&self) -> Result<Switched, RegistryError> {
        let switched = self.disable_switch()?;
        self.adopt(switched.config, false);
        Ok(switched)
    }

    /// "Desligar a opção" na janela de falha: grava só os campos do Win+V.
    /// `Ok(false)` quando o registro mudou mas o arquivo não foi gravado.
    pub fn turn_off_and_persist(&self) -> Result<bool, RegistryError> {
        let switched = self.disable_switch()?;
        Ok(self.adopt(switched.config, true))
    }

    /// O `TaskbarCreated` chegou à bandeja.
    pub fn explorer_returned(&self) -> Reaction {
        if let Some(tx) = self.returned.borrow().as_ref() {
            let _ = tx.send(());
        }
        let mut last_seen = self.last_seen.get();
        let reaction = win_v::explorer_returned(
            self.status.get(),
            self.win_v_config(),
            self.reserve(),
            &mut last_seen,
            self.explorer.current(),
            self.slots.borrow_mut().as_mut(),
        );
        self.last_seen.set(last_seen);
        self.set_status(reaction.status);
        self.adopt(reaction.config, true);
        reaction
    }

    pub fn restart_in_progress(&self) -> bool {
        self.returned.borrow().is_some()
    }

    /// O botão começou um reinício; `returned` recebe o aviso de volta.
    pub fn restart_started(&self, returned: mpsc::Sender<()>) {
        *self.returned.borrow_mut() = Some(returned);
    }

    pub fn restart_finished(&self) {
        self.returned.borrow_mut().take();
    }

    /// Solta os dois atalhos no encerramento.
    pub fn release_all(&self) {
        let mut slots = self.slots.borrow_mut();
        slots.release_win_v();
        slots.release_reserve();
    }
}
