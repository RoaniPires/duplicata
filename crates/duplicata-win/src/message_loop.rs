use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, PostQuitMessage, PostThreadMessageW, TranslateMessage, MSG,
    WM_QUIT, WM_TIMER,
};

pub static MESSAGE_LOOP_ITERATIONS: AtomicU64 = AtomicU64::new(0);

pub static MESSAGE_LOOP_TIMER_WAKEUPS: AtomicU64 = AtomicU64::new(0);

pub static SESSION_ENDING: AtomicBool = AtomicBool::new(false);

pub fn run() {
    let mut msg = MSG::default();
    // SAFETY: `GetMessageW`/`TranslateMessage`/`DispatchMessageW` com um `MSG`
    // válido; `GetMessageW` devolve 0 em `WM_QUIT` e -1 em erro.
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            MESSAGE_LOOP_ITERATIONS.fetch_add(1, Ordering::Relaxed);
            if msg.message == WM_TIMER {
                MESSAGE_LOOP_TIMER_WAKEUPS.fetch_add(1, Ordering::Relaxed);
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
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
