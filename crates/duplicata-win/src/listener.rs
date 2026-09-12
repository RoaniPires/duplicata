use std::rc::Rc;
use std::sync::Arc;

use duplicata_core::backoff::BackoffPolicy;
use duplicata_core::{
    capture_and_enqueue, CaptureQueue, Config, InitError, RecentCaptureGuard, SelfWriteFilter,
    WorkItem,
};
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, RemoveClipboardFormatListener,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindowLongPtrW, RegisterClassW,
    SetWindowLongPtrW, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HWND_MESSAGE, WINDOW_EX_STYLE,
    WINDOW_STYLE, WM_CLIPBOARDUPDATE, WM_CREATE, WM_DESTROY, WM_NCCREATE, WNDCLASSW,
};

use crate::win_clipboard::WinClipboard;
use crate::win_clock::WinClock;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_clipboard_listener");

struct ListenerCtx {
    source: WinClipboard,
    clock: WinClock,
    policy: BackoffPolicy,
    config: Config,
    queue: Arc<dyn CaptureQueue<WorkItem>>,
    self_write_filter: Rc<SelfWriteFilter>,
    recent_capture_guard: RecentCaptureGuard,
}

pub struct ClipboardListener {
    hwnd: HWND,
    _ctx: Box<ListenerCtx>,
}

impl ClipboardListener {
    pub fn new(
        queue: Arc<dyn CaptureQueue<WorkItem>>,
        config: Config,
        self_write_filter: Rc<SelfWriteFilter>,
    ) -> Result<Self, InitError> {
        let mut ctx = Box::new(ListenerCtx {
            source: WinClipboard,
            clock: WinClock,
            policy: BackoffPolicy::production(),
            config,
            queue,
            self_write_filter,
            recent_capture_guard: RecentCaptureGuard::new(),
        });

        // SAFETY: registro de classe idempotente com `wndproc` válido; ignoramos
        // o erro de "classe já registrada".
        unsafe {
            let hinstance = GetModuleHandleW(None).map_err(|_| InitError::Listener)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let ctx_ptr: *mut ListenerCtx = ctx.as_mut();
            let hwnd = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                CLASS_NAME,
                PCWSTR::null(),
                WINDOW_STYLE::default(),
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                Some(HWND_MESSAGE),
                None,
                Some(hinstance.into()),
                Some(ctx_ptr.cast()),
            )
            .map_err(|_| InitError::Listener)?;

            if AddClipboardFormatListener(hwnd).is_err() {
                let _ = DestroyWindow(hwnd);
                return Err(InitError::Listener);
            }

            Ok(ClipboardListener { hwnd, _ctx: ctx })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }
}

impl Drop for ClipboardListener {
    fn drop(&mut self) {
        // SAFETY: `hwnd` foi criado por nós; remover o listener e destruir a
        // janela é seguro e idempotente o suficiente para o shutdown.
        unsafe {
            let _ = RemoveClipboardFormatListener(self.hwnd);
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE | WM_CREATE => {
            // SAFETY: no WM_(NC)CREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o nosso `*mut ListenerCtx`.
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CLIPBOARDUPDATE => {
            // SAFETY: `GWLP_USERDATA` guarda o `*mut ListenerCtx` posto no create;
            // a janela e o contexto vivem enquanto o `ClipboardListener` existir.
            let ctx = unsafe { &*(GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const ListenerCtx) };
            if ctx.self_write_filter.consume_if_pending() {
                return LRESULT(0);
            }
            capture_and_enqueue(
                &ctx.source,
                &ctx.clock,
                &ctx.policy,
                &ctx.config,
                ctx.queue.as_ref(),
                &ctx.recent_capture_guard,
            );
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
