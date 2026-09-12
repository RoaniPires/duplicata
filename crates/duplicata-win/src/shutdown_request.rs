use windows::Win32::Foundation::{CloseHandle, WPARAM};
use windows::Win32::System::Threading::{
    OpenProcess, WaitForSingleObject, INFINITE, PROCESS_SYNCHRONIZE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowW, GetWindowThreadProcessId, PostMessageW, WM_COMMAND,
};

use crate::tray::{CLASS_NAME, ID_TRAY_EXIT};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ShutdownOutcome {
    AlreadyExited,
    Exited,
}

pub fn request_shutdown() -> ShutdownOutcome {
    // SAFETY: `FindWindowW` só consulta janelas top-level já existentes, sem
    // pré-condição; devolve `Err` se nenhuma janela dessa classe existir.
    let Ok(hwnd) = (unsafe { FindWindowW(CLASS_NAME, None) }) else {
        return ShutdownOutcome::AlreadyExited;
    };

    let mut pid: u32 = 0;
    // SAFETY: `hwnd` acabou de ser devolvido por `FindWindowW` como válido;
    // `&mut pid` é um ponteiro local válido para a chamada preencher.
    let _tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };

    // SAFETY: `pid` resolvido acima (ou `0`, que `OpenProcess` recusa
    // normalmente); `PROCESS_SYNCHRONIZE` é o único direito que
    // `WaitForSingleObject` exige.
    let Ok(process) = (unsafe { OpenProcess(PROCESS_SYNCHRONIZE, false, pid) }) else {
        return ShutdownOutcome::AlreadyExited;
    };

    // Passo 2: posta a MESMA mensagem que "Encerrar" do menu posta.
    // SAFETY: `PostMessageW` é seguro de qualquer processo/thread para
    // qualquer HWND; um `hwnd` já destruído só faz a chamada devolver erro,
    // que ignoramos (o efeito equivalente ao passo 1 já teria sido pego
    // acima, pelo `OpenProcess`).
    let _ = unsafe {
        PostMessageW(
            Some(hwnd),
            WM_COMMAND,
            WPARAM(ID_TRAY_EXIT),
            Default::default(),
        )
    };

    // Passo 3: espera o processo sair, sem teto (FR-024a).
    // SAFETY: `process` tem `PROCESS_SYNCHRONIZE`, obtido acima; `INFINITE`
    // é o único valor aceito para "sem teto" nesta API.
    unsafe { WaitForSingleObject(process, INFINITE) };
    // SAFETY: fecha o handle aberto por `OpenProcess` acima, sempre.
    let _ = unsafe { CloseHandle(process) };

    ShutdownOutcome::Exited
}
