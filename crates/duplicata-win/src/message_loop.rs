use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetClassNameW, GetMessageW, PostQuitMessage, PostThreadMessageW,
    TranslateMessage, MSG, WM_QUIT, WM_TIMER,
};

pub static MESSAGE_LOOP_ITERATIONS: AtomicU64 = AtomicU64::new(0);

pub static MESSAGE_LOOP_TIMER_WAKEUPS: AtomicU64 = AtomicU64::new(0);

/// `WM_TIMER` das janelas de IME do sistema, contados à parte: não vêm de
/// temporizador do aplicativo.
pub static MESSAGE_LOOP_IME_TIMER_WAKEUPS: AtomicU64 = AtomicU64::new(0);

/// Janelas de IME que o sistema cria na thread (Text Services Framework):
/// os `WM_TIMER` delas não são do aplicativo.
pub fn is_system_ime_class(class_name: &str) -> bool {
    matches!(class_name, "MSCTFIME UI" | "IME")
}

pub static SESSION_ENDING: AtomicBool = AtomicBool::new(false);

pub fn run() {
    let mut msg = MSG::default();
    // SAFETY: `GetMessageW`/`TranslateMessage`/`DispatchMessageW` com um `MSG`
    // válido; `GetMessageW` devolve 0 em `WM_QUIT` e -1 em erro.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            MESSAGE_LOOP_ITERATIONS.fetch_add(1, Ordering::Relaxed);
            if msg.message == WM_TIMER {
                let counter = if is_system_ime_class(&class_name(msg.hwnd)) {
                    &MESSAGE_LOOP_IME_TIMER_WAKEUPS
                } else {
                    &MESSAGE_LOOP_TIMER_WAKEUPS
                };
                counter.fetch_add(1, Ordering::Relaxed);
            }
            if crate::settings_dialog::pre_translate(&msg) {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

fn class_name(hwnd: HWND) -> String {
    let mut name = [0u16; 64];
    // SAFETY: só lê o nome da classe para o buffer local; um `HWND` inválido
    // devolve 0.
    let len = unsafe { GetClassNameW(hwnd, &mut name) }.max(0) as usize;
    String::from_utf16_lossy(&name[..len.min(name.len())])
}

pub fn post_quit() {
    // SAFETY: `PostQuitMessage` é seguro de qualquer thread com fila de mensagens.
    unsafe { PostQuitMessage(0) };
}

pub fn current_thread_id() -> u32 {
    // SAFETY: sem pré-condições — sempre seguro.
    unsafe { GetCurrentThreadId() }
}

pub fn post_quit_to(thread_id: u32) {
    // SAFETY: `PostThreadMessageW` com um id de thread e `WM_QUIT` (sem
    // payload); erro é seguro de ignorar.
    let _ = unsafe { PostThreadMessageW(thread_id, WM_QUIT, WPARAM(0), LPARAM(0)) };
}
