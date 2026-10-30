//! Qual processo do shell está rodando: a hora de criação do processo dono
//! da janela do shell. Outra hora quer dizer que o Explorer reiniciou (o
//! `TaskbarCreated` sozinho não diz isso: também chega na troca de DPI ou de
//! tema). Só consulta.

use duplicata_core::win_v::ExplorerInstance;
use windows::core::w;
use windows::Win32::Foundation::{CloseHandle, FILETIME, HWND};
use windows::Win32::System::Threading::{
    GetProcessTimes, OpenProcess, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetShellWindow, GetWindowThreadProcessId,
};

/// O shell da sessão atual.
pub struct ShellProcess;

impl ExplorerInstance for ShellProcess {
    fn current(&self) -> Option<u64> {
        shell_started_at()
    }
}

fn shell_window() -> Option<HWND> {
    // SAFETY: sem pré-condição; devolve nulo sem shell.
    let shell = unsafe { GetShellWindow() };
    if !shell.is_invalid() {
        return Some(shell);
    }
    // SAFETY: só procura uma janela top-level pela classe da barra.
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }.ok()
}

/// Há shell agora.
pub fn shell_present() -> bool {
    shell_window().is_some()
}

/// Hora de criação (FILETIME) do processo do shell; `None` sem shell ou sem
/// acesso.
pub fn shell_started_at() -> Option<u64> {
    let (_, created) = shell_process()?;
    Some((u64::from(created.dwHighDateTime) << 32) | u64::from(created.dwLowDateTime))
}

/// Identificador e hora de criação do processo do shell: o par que o
/// Restart Manager usa para não confundir com outro processo que reaproveite
/// o mesmo número.
pub fn shell_process() -> Option<(u32, FILETIME)> {
    let hwnd = shell_window()?;
    let mut pid = 0u32;
    // SAFETY: `hwnd` veio de uma consulta acima; se já morreu, a chamada
    // devolve 0 e `pid` fica 0.
    unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }
    // SAFETY: abre só para consulta limitada; o handle é fechado abaixo.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let (mut created, mut exited, mut kernel, mut user) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    // SAFETY: `process` aberto acima com o direito exigido; as saídas são
    // locais.
    let read =
        unsafe { GetProcessTimes(process, &mut created, &mut exited, &mut kernel, &mut user) };
    // SAFETY: fecha o handle aberto acima, uma vez.
    unsafe {
        let _ = CloseHandle(process);
    }
    read.ok()?;
    Some((pid, created))
}
