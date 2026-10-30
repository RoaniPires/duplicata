//! Botão "Reiniciar o Windows Explorer agora": pede ao Explorer, pelo
//! Restart Manager, que feche normalmente e o deixa voltar. Nunca força: sem
//! a flag de encerramento forçado, um Explorer que recusa fechar faz o
//! `RmShutdown` devolver erro, e nada muda.
//!
//! Roda numa thread criada pelo clique em "Reiniciar", que só existe durante
//! a ação: ela espera o aviso de que o Explorer voltou (o `TaskbarCreated`
//! chega à bandeja, que avisa pelo canal) por no máximo
//! `duplicata_core::win_v::EXPLORER_RETURN_TIMEOUT`, manda o resultado à janela da bandeja por
//! `PostMessageW` e termina. Sem `WM_TIMER`: a message loop continua
//! dormindo em `GetMessageW`.

use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use duplicata_core::win_v::{
    run_explorer_restart, ExplorerRestartOutcome, ExplorerRestarter, Refused,
};
use duplicata_core::{log_event, LogFields};
use tracing::Level;
use windows::core::PWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::RestartManager::{
    RmEndSession, RmRegisterResources, RmRestart, RmShutdown, RmStartSession, CCH_RM_SESSION_KEY,
    RM_UNIQUE_PROCESS,
};
use windows::Win32::System::SystemInformation::GetWindowsDirectoryW;
use windows::Win32::UI::WindowsAndMessaging::PostMessageW;

use crate::explorer_instance;

/// Sessão do Restart Manager com o processo do shell registrado; encerrada
/// no `Drop`.
#[derive(Default)]
pub struct RmExplorerRestarter {
    session: Option<u32>,
}

impl RmExplorerRestarter {
    fn open_session(&mut self) -> Option<u32> {
        let (pid, created) = explorer_instance::shell_process()?;
        let mut handle = 0u32;
        let mut key = [0u16; CCH_RM_SESSION_KEY as usize + 1];
        // SAFETY: `handle` local; `key` tem os `CCH_RM_SESSION_KEY + 1`
        // caracteres que a API exige.
        let err = unsafe { RmStartSession(&mut handle, None, PWSTR(key.as_mut_ptr())) };
        if err.is_err() {
            log_event!(
                Level::WARN,
                LogFields::new("explorer_restart_session_failed")
            );
            return None;
        }
        self.session = Some(handle);
        let process = RM_UNIQUE_PROCESS {
            dwProcessId: pid,
            ProcessStartTime: created,
        };
        // SAFETY: `handle` aberto acima; o slice vive durante a chamada.
        let err = unsafe { RmRegisterResources(handle, None, Some(&[process]), None) };
        if err.is_err() {
            log_event!(
                Level::WARN,
                LogFields::new("explorer_restart_register_failed")
            );
            return None;
        }
        Some(handle)
    }
}

impl ExplorerRestarter for RmExplorerRestarter {
    fn shutdown(&mut self) -> Result<(), Refused> {
        let handle = self.open_session().ok_or(Refused)?;
        // SAFETY: `handle` é a sessão aberta acima. `0` pede o encerramento
        // normal: o Explorer pode recusar, e então a chamada devolve erro.
        let err = unsafe { RmShutdown(handle, 0, None) };
        if err.is_err() {
            log_event!(Level::WARN, LogFields::new("explorer_restart_refused"));
            return Err(Refused);
        }
        Ok(())
    }

    fn restart(&mut self) {
        let Some(handle) = self.session else {
            return;
        };
        // SAFETY: `handle` é a sessão em que o `RmShutdown` deu certo.
        let err = unsafe { RmRestart(handle, None, None) };
        if err.is_err() {
            // O prazo e o relançamento cobrem o Explorer que não voltar.
            log_event!(
                Level::WARN,
                LogFields::new("explorer_restart_rmrestart_failed")
            );
        }
    }
}

impl Drop for RmExplorerRestarter {
    fn drop(&mut self) {
        if let Some(handle) = self.session.take() {
            // SAFETY: fecha a sessão aberta por este valor, uma vez.
            let _ = unsafe { RmEndSession(handle) };
        }
    }
}

/// `%WINDIR%\explorer.exe`, pelo caminho completo (nunca pelo PATH).
fn explorer_exe() -> Option<PathBuf> {
    let mut buf = [0u16; 260];
    // SAFETY: `buf` local; a API escreve no máximo o tamanho do slice.
    let len = unsafe { GetWindowsDirectoryW(Some(&mut buf)) } as usize;
    if len == 0 || len >= buf.len() {
        return None;
    }
    let dir = String::from_utf16(&buf[..len]).ok()?;
    Some(PathBuf::from(dir).join("explorer.exe"))
}

/// Abre o Explorer de novo. O único programa que o duplicata executa, e só
/// quando, passado o prazo, não há shell.
fn relaunch() -> bool {
    let Some(exe) = explorer_exe() else {
        return false;
    };
    let launched = std::process::Command::new(exe).spawn().is_ok();
    log_event!(
        Level::WARN,
        LogFields::new("explorer_restart_relaunch").kind(if launched { "ok" } else { "failed" })
    );
    launched
}

pub fn outcome_to_wparam(outcome: ExplorerRestartOutcome) -> usize {
    match outcome {
        ExplorerRestartOutcome::Refused => 0,
        ExplorerRestartOutcome::Returned => 1,
        ExplorerRestartOutcome::NotReturnedRelaunched => 2,
        ExplorerRestartOutcome::NotReturnedRelaunchFailed => 3,
    }
}

pub fn outcome_from_wparam(wparam: usize) -> ExplorerRestartOutcome {
    match wparam {
        1 => ExplorerRestartOutcome::Returned,
        2 => ExplorerRestartOutcome::NotReturnedRelaunched,
        3 => ExplorerRestartOutcome::NotReturnedRelaunchFailed,
        _ => ExplorerRestartOutcome::Refused,
    }
}

/// Começa o reinício numa thread própria. O `Sender` devolvido é o aviso de
/// que o Explorer voltou; o resultado chega a `notify` como `message`, com o
/// resultado em `WPARAM` ([`outcome_from_wparam`]).
pub fn spawn(notify: HWND, message: u32) -> mpsc::Sender<()> {
    let (returned_tx, returned_rx) = mpsc::channel::<()>();
    let raw = notify.0 as isize;
    thread::spawn(move || {
        let mut restarter = RmExplorerRestarter::default();
        let outcome = run_explorer_restart(
            &mut restarter,
            |timeout| returned_rx.recv_timeout(timeout).is_ok(),
            explorer_instance::shell_present,
            relaunch,
        );
        drop(restarter);
        let hwnd = HWND(raw as *mut core::ffi::c_void);
        // SAFETY: `PostMessageW` aceita qualquer `HWND`, mesmo destruído (só
        // devolve erro).
        let posted = unsafe {
            PostMessageW(
                Some(hwnd),
                message,
                WPARAM(outcome_to_wparam(outcome)),
                LPARAM(0),
            )
        };
        if posted.is_err() {
            log_event!(Level::WARN, LogFields::new("explorer_restart_post_failed"));
        }
    });
    returned_tx
}
