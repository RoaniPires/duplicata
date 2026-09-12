use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::{mpsc, Arc, OnceLock};
use std::thread;

use duplicata_core::heuristic_recovery::{recover_on_notification_click, RecoverOutcome};
use duplicata_core::{
    log_event, BackoffPolicy, CaptureQueue, Config, HotkeyCombo, InitError, LogFields, WorkItem,
};
use tracing::Level;
use windows::core::{HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::{DeleteObject, HGDIOBJ};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    SHQueryUserNotificationState, ShellExecuteW, Shell_NotifyIconW, NIF_ICON, NIF_INFO,
    NIF_MESSAGE, NIF_TIP, NIIF_NOSOUND, NIM_ADD, NIM_DELETE, NIM_MODIFY, NIN_BALLOONUSERCLICK,
    NOTIFYICONDATAW, QUERY_USER_NOTIFICATION_STATE, QUNS_ACCEPTS_NOTIFICATIONS, QUNS_APP,
    QUNS_BUSY, QUNS_NOT_PRESENT, QUNS_PRESENTATION_MODE, QUNS_QUIET_TIME,
    QUNS_RUNNING_D3D_FULL_SCREEN,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetCursorPos, GetWindowLongPtrW, LoadIconW, PostMessageW, RegisterClassW,
    RegisterWindowMessageW, SetForegroundWindow, SetMenuItemInfoW, SetWindowLongPtrW,
    TrackPopupMenu, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HICON, IDI_APPLICATION,
    MENUITEMINFOW, MF_SEPARATOR, MF_STRING, MIIM_BITMAP, SW_SHOWNORMAL, TPM_LEFTALIGN,
    TPM_RIGHTBUTTON, WM_APP, WM_COMMAND, WM_CREATE, WM_ENDSESSION, WM_LBUTTONUP, WM_NCCREATE,
    WM_QUERYENDSESSION, WM_RBUTTONUP, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

use crate::app_icon;

use crate::menu_icon;
use crate::message_loop;
use crate::settings_dialog;
use crate::system_appearance::SystemAppearance;
use crate::win_clipboard::WinClipboard;
use crate::WinClock;

pub(crate) const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_tray_icon");
const TRAY_UID: u32 = 1;
const WM_TRAYICON: u32 = WM_APP + 1;
const WM_HEURISTIC_REJECTED: u32 = WM_APP + 2;
pub(crate) const ID_TRAY_EXIT: usize = 2;
const ID_TRAY_SETTINGS: usize = 3;
const ID_TRAY_CHECK_UPDATE: usize = 4;

const RELEASES_URL: PCWSTR = windows::core::w!("https://github.com/RoaniPires/duplicata/releases");

fn wm_taskbarcreated() -> u32 {
    static ID: OnceLock<u32> = OnceLock::new();
    *ID.get_or_init(|| {
        // SAFETY: string estática válida; em caso de falha (nunca documentada,
        // mas o retorno é `u32`, não `Result`) o valor devolvido é 0, que não
        // colide com nenhuma mensagem real do sistema — o guard do wndproc
        // simplesmente nunca casaria.
        unsafe { RegisterWindowMessageW(windows::core::w!("TaskbarCreated")) }
    })
}

struct TrayCtx {
    icon: HICON,
    history_hwnd: HWND,
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    queue: Arc<dyn CaptureQueue<WorkItem>>,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
    version: &'static str,
}

pub struct Tray {
    hwnd: HWND,
    _ctx: Box<TrayCtx>,
}

impl Tray {
    pub fn create(
        history_hwnd: HWND,
        config: Rc<RefCell<Config>>,
        config_path: PathBuf,
        queue: Arc<dyn CaptureQueue<WorkItem>>,
        heuristic_rx: mpsc::Receiver<()>,
        registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
        version: &'static str,
    ) -> Result<Self, InitError> {
        let _ = wm_taskbarcreated();

        // SAFETY: registro de classe idempotente com `wndproc` válido; ignoramos
        // o erro de "classe já registrada" (mesmo padrão do listener de
        // clipboard). A janela é `WS_POPUP` sem `WS_VISIBLE` (nunca aparece) e
        // `WS_EX_TOOLWINDOW` (sem entrada na barra de tarefas/Alt+Tab) — mas
        // TOP-LEVEL (sem `HWND_MESSAGE`), condição necessária para receber
        // `TaskbarCreated` e `WM_QUERYENDSESSION`/`WM_ENDSESSION` (ver doc do
        // módulo).
        unsafe {
            let hinstance = GetModuleHandleW(None).map_err(|_| InitError::Tray)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let icon = app_icon::load_app_icon()
                .or_else(|| LoadIconW(None, IDI_APPLICATION).ok())
                .ok_or(InitError::Tray)?;
            let mut ctx = Box::new(TrayCtx {
                icon,
                history_hwnd,
                config,
                config_path,
                queue,
                registered_hotkey,
                version,
            });
            let ctx_ptr: *mut TrayCtx = ctx.as_mut();

            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS_NAME,
                PCWSTR::null(),
                WS_POPUP,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                None,
                None,
                Some(hinstance.into()),
                Some(ctx_ptr.cast()),
            )
            .map_err(|_| InitError::Tray)?;

            if !add_icon(hwnd, icon) {
                let _ = DestroyWindow(hwnd);
                return Err(InitError::Tray);
            }

            let _ = spawn_heuristic_bridge(hwnd, heuristic_rx);

            Ok(Tray { hwnd, _ctx: ctx })
        }
    }

    pub fn destroy(self) {}
}

impl Drop for Tray {
    fn drop(&mut self) {
        // SAFETY: `remove_icon`/`DestroyWindow` num `hwnd` criado por nós;
        // idempotente o suficiente para o shutdown (o ícone pode já ter sido
        // removido por `WM_ENDSESSION` — `Shell_NotifyIconW(NIM_DELETE)` numa
        // entrada inexistente só devolve `FALSE`, que ignoramos).
        unsafe {
            remove_icon(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn add_icon(hwnd: HWND, icon: HICON) -> bool {
    let mut nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_ICON | NIF_MESSAGE | NIF_TIP,
        uCallbackMessage: WM_TRAYICON,
        hIcon: icon,
        ..Default::default()
    };
    set_tip(&mut nid, "duplicata");
    // SAFETY: `nid` está totalmente inicializado (campos não citados vêm do
    // `Default` zerado do `windows` crate, válido para `NOTIFYICONDATAW`) e
    // `cbSize` reflete seu tamanho real.
    unsafe { Shell_NotifyIconW(NIM_ADD, &nid).as_bool() }
}

/// # Safety
/// `hwnd` deve ter sido criado por [`Tray::create`] (mesmo `uID`/`hWnd`
/// usados no `NIM_ADD`).
unsafe fn remove_icon(hwnd: HWND) {
    let nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        ..Default::default()
    };
    // SAFETY: `nid` inicializado, `cbSize` correto — ver contrato da função.
    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &nid) };
}

fn set_tip(nid: &mut NOTIFYICONDATAW, tip: &str) {
    let wide: Vec<u16> = tip.encode_utf16().chain(core::iter::once(0)).collect();
    let n = wide.len().min(nid.szTip.len());
    nid.szTip[..n].copy_from_slice(&wide[..n]);
}

const HEURISTIC_BALLOON_TITLE: &str = "duplicata";
const HEURISTIC_BALLOON_INFO: &str = "Um item copiado foi descartado por parecer um segredo \
     (chave privada, token de acesso ou número de cartão). Clique aqui para recuperá-lo, se \
     ainda estiver na área de transferência.";

fn spawn_heuristic_bridge(hwnd: HWND, rx: mpsc::Receiver<()>) -> thread::JoinHandle<()> {
    let raw_hwnd = hwnd.0 as isize;
    thread::spawn(move || {
        while rx.recv().is_ok() {
            let hwnd = HWND(raw_hwnd as *mut core::ffi::c_void);
            // SAFETY: `PostMessageW` é thread-safe para qualquer HWND, mesmo
            // um já destruído (ver nota sobre ordem acima) — só devolve erro
            // nesse caso, nunca UB.
            let posted =
                unsafe { PostMessageW(Some(hwnd), WM_HEURISTIC_REJECTED, WPARAM(0), LPARAM(0)) };
            if posted.is_err() {
                log_event!(
                    Level::WARN,
                    LogFields::new("heuristic_bridge_postmessage_failed")
                );
            }
        }
    })
}

/// # Safety
/// `hwnd` deve ter sido criado por [`Tray::create`] (mesmo `uID`/`hWnd`
/// usados no `NIM_ADD`).
unsafe fn show_heuristic_balloon(hwnd: HWND) {
    // SAFETY: sem pré-condição além de shell32 estar carregada (sempre está).
    let state = unsafe { SHQueryUserNotificationState() };
    log_event!(
        Level::DEBUG,
        LogFields::new("heuristic_balloon_user_state").kind(user_notification_state_str(state))
    );

    let mut nid = NOTIFYICONDATAW {
        cbSize: core::mem::size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_UID,
        uFlags: NIF_INFO,
        dwInfoFlags: NIIF_NOSOUND,
        ..Default::default()
    };
    set_info(&mut nid, HEURISTIC_BALLOON_TITLE, HEURISTIC_BALLOON_INFO);
    // SAFETY: `nid` inicializado, `cbSize` correto — ver contrato da função.
    let shown = unsafe { Shell_NotifyIconW(NIM_MODIFY, &nid) };
    if shown.as_bool() {
        log_event!(
            Level::DEBUG,
            LogFields::new("heuristic_balloon_shell_notify_ok")
        );
    } else {
        log_event!(
            Level::WARN,
            LogFields::new("heuristic_balloon_shell_notify_failed")
        );
    }
}

fn user_notification_state_str(
    state: windows::core::Result<QUERY_USER_NOTIFICATION_STATE>,
) -> &'static str {
    match state {
        Err(_) => "query_failed",
        Ok(QUNS_NOT_PRESENT) => "not_present",
        Ok(QUNS_BUSY) => "busy",
        Ok(QUNS_RUNNING_D3D_FULL_SCREEN) => "fullscreen_d3d",
        Ok(QUNS_PRESENTATION_MODE) => "presentation_mode",
        Ok(QUNS_ACCEPTS_NOTIFICATIONS) => "accepts_notifications",
        Ok(QUNS_QUIET_TIME) => "quiet_time_focus_assist",
        Ok(QUNS_APP) => "app_quiet_time",
        Ok(_) => "unknown",
    }
}

fn set_info(nid: &mut NOTIFYICONDATAW, title: &str, info: &str) {
    let title_w: Vec<u16> = title.encode_utf16().chain(core::iter::once(0)).collect();
    let n = title_w.len().min(nid.szInfoTitle.len());
    nid.szInfoTitle[..n].copy_from_slice(&title_w[..n]);

    let info_w: Vec<u16> = info.encode_utf16().chain(core::iter::once(0)).collect();
    let n = info_w.len().min(nid.szInfo.len());
    nid.szInfo[..n].copy_from_slice(&info_w[..n]);
}

fn handle_heuristic_recovery_click(hwnd: HWND) {
    let Some(ctx) = tray_ctx(hwnd) else {
        return;
    };
    let outcome = recover_on_notification_click(
        &WinClipboard,
        &WinClock,
        &BackoffPolicy::production(),
        &ctx.config.borrow(),
        ctx.queue.as_ref(),
    );
    if let RecoverOutcome::Failed(_) = outcome {
        log_event!(Level::WARN, LogFields::new("heuristic_recovery_failed"));
    }
}

fn open_releases_page() {
    // SAFETY: `ShellExecuteW` com strings estáticas válidas; `None` de
    // `hwnd` (sem janela-pai) e de parâmetros/diretório é aceito pela API
    // para abrir uma URL. O `HINSTANCE` de retorno (>32 = sucesso) é
    // descartado de propósito — nada a fazer com uma falha aqui.
    unsafe {
        let _ = ShellExecuteW(
            None,
            windows::core::w!("open"),
            RELEASES_URL,
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        );
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE | WM_CREATE => {
            // SAFETY: no WM_(NC)CREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o nosso `*mut TrayCtx` (mesmo padrão do
            // listener de clipboard).
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_TRAYICON => {
            let event = (lparam.0 as usize & 0xFFFF) as u32;
            if event == WM_RBUTTONUP || event == WM_LBUTTONUP {
                show_context_menu(hwnd);
            } else if event == NIN_BALLOONUSERCLICK {
                handle_heuristic_recovery_click(hwnd);
            }
            LRESULT(0)
        }
        WM_HEURISTIC_REJECTED => {
            // SAFETY: `hwnd` é o desta janela, criado por `Tray::create`.
            unsafe { show_heuristic_balloon(hwnd) };
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            if id == ID_TRAY_EXIT {
                message_loop::post_quit();
            } else if id == ID_TRAY_SETTINGS {
                if let Some(ctx) = tray_ctx(hwnd) {
                    settings_dialog::open(
                        Rc::clone(&ctx.config),
                        ctx.config_path.clone(),
                        Arc::clone(&ctx.queue),
                        ctx.history_hwnd,
                        Rc::clone(&ctx.registered_hotkey),
                    );
                }
            } else if id == ID_TRAY_CHECK_UPDATE {
                open_releases_page();
            }
            LRESULT(0)
        }
        WM_QUERYENDSESSION => LRESULT(1),
        WM_ENDSESSION => {
            if wparam.0 != 0 {
                // SAFETY: `hwnd` é o desta janela, criado por `Tray::create`.
                unsafe { remove_icon(hwnd) };
                message_loop::SESSION_ENDING.store(true, Ordering::SeqCst);
                message_loop::post_quit();
            }
            LRESULT(0)
        }
        m if m == wm_taskbarcreated() => {
            let ctx = tray_ctx(hwnd);
            if let Some(ctx) = ctx {
                let _ = add_icon(hwnd, ctx.icon);
            }
            LRESULT(0)
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

fn tray_ctx(hwnd: HWND) -> Option<&'static TrayCtx> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut TrayCtx` posto em `WM_NCCREATE`;
    // a janela e o contexto vivem enquanto o `Tray` existir.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const TrayCtx;
    // SAFETY: `ptr` ou é nulo (janela ainda sem contexto) ou aponta para o
    // `TrayCtx` posto acima — `as_ref()` trata nulo como `None` corretamente.
    unsafe { ptr.as_ref() }
}

fn show_context_menu(hwnd: HWND) {
    let dark = SystemAppearance::read_dark();
    let icon = menu_icon::settings_menu_bitmap(hwnd, dark);

    let update_label = HSTRING::from(format!(
        "Verificar atualizações (instalada: v{})",
        tray_ctx(hwnd).map_or("?", |ctx| ctx.version)
    ));

    // SAFETY: `CreatePopupMenu`/`AppendMenuW`/`TrackPopupMenu`/`DestroyMenu` com
    // um `HMENU` criado e destruído aqui mesmo (não persiste entre chamadas).
    // `SetForegroundWindow` antes de `TrackPopupMenu` é exigido pela
    // documentação da Microsoft para o menu fechar corretamente ao perder o
    // foco (senão ele fica preso aberto até outro clique).
    unsafe {
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let _ = SetForegroundWindow(hwnd);

        if let Ok(hmenu) = CreatePopupMenu() {
            let _ = AppendMenuW(
                hmenu,
                MF_STRING,
                ID_TRAY_SETTINGS,
                windows::core::w!("Configurações"),
            );
            if let Some(hbitmap) = icon {
                let info = MENUITEMINFOW {
                    cbSize: core::mem::size_of::<MENUITEMINFOW>() as u32,
                    fMask: MIIM_BITMAP,
                    hbmpItem: hbitmap,
                    ..Default::default()
                };
                let _ = SetMenuItemInfoW(hmenu, ID_TRAY_SETTINGS as u32, false, &info);
            }
            let _ = AppendMenuW(hmenu, MF_STRING, ID_TRAY_CHECK_UPDATE, &update_label);
            let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, PCWSTR::null());
            let _ = AppendMenuW(
                hmenu,
                MF_STRING,
                ID_TRAY_EXIT,
                windows::core::w!("Encerrar"),
            );
            let _ = TrackPopupMenu(
                hmenu,
                TPM_RIGHTBUTTON | TPM_LEFTALIGN,
                pt.x,
                pt.y,
                None,
                hwnd,
                None,
            );
            let _ = DestroyMenu(hmenu);
        }
    }

    if let Some(hbitmap) = icon {
        // SAFETY: `hbitmap` foi criado por `menu_icon::settings_menu_bitmap`
        // (nosso), o menu que o referenciava já foi destruído, e ele não é
        // guardado em lugar nenhum.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    fn join_with_timeout(handle: thread::JoinHandle<()>, timeout: Duration) -> thread::Result<()> {
        let (done_tx, done_rx) = mpsc::channel();
        thread::spawn(move || {
            let _ = done_tx.send(handle.join());
        });
        done_rx
            .recv_timeout(timeout)
            .expect("a thread-ponte deveria ter terminado dentro do teto — travou?")
    }

    #[test]
    fn heuristic_bridge_terminates_promptly_after_the_sender_is_dropped() {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = spawn_heuristic_bridge(HWND::default(), rx);

        drop(tx);

        join_with_timeout(handle, Duration::from_secs(2))
            .expect("a thread-ponte não deve entrar em pânico");
    }

    #[test]
    fn heuristic_bridge_stays_blocked_while_the_sender_is_still_alive() {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = spawn_heuristic_bridge(HWND::default(), rx);

        thread::sleep(Duration::from_millis(150));
        assert!(
            !handle.is_finished(),
            "não deve terminar sozinha enquanto o Sender ainda existe e nada foi enviado — \
             provaria polling/busy-loop em vez de um bloqueio real (Princípio V)"
        );

        drop(tx);
        join_with_timeout(handle, Duration::from_secs(2))
            .expect("a thread-ponte não deve entrar em pânico");
    }

    #[test]
    fn heuristic_bridge_terminates_promptly_even_after_delivering_a_signal() {
        let (tx, rx) = mpsc::channel::<()>();
        let handle = spawn_heuristic_bridge(HWND::default(), rx);

        tx.send(()).expect("receptor ainda vivo");
        drop(tx);

        join_with_timeout(handle, Duration::from_secs(2))
            .expect("a thread-ponte não deve entrar em pânico mesmo após um PostMessageW inválido");
    }
}
