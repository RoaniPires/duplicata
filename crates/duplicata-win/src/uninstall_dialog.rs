use windows::core::{w, HSTRING, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, DispatchMessageW,
    GetMessageW, GetWindowLongPtrW, LoadCursorW, PostQuitMessage, RegisterClassW, SendMessageW,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, TranslateMessage,
    BM_GETCHECK, BM_SETCHECK, BS_AUTORADIOBUTTON, BS_MULTILINE, CREATESTRUCTW, CW_USEDEFAULT,
    GWLP_USERDATA, HMENU, IDC_ARROW, MSG, SWP_NOMOVE, SWP_NOZORDER, SW_SHOW, WINDOW_STYLE,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DESTROY, WM_NCCREATE, WM_NCDESTROY, WNDCLASSW, WS_CAPTION,
    WS_CHILD, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

const CLASS_NAME: PCWSTR = w!("duplicata_uninstall_choice");
const ID_RADIO_DELETE: usize = 1;
const ID_RADIO_PRESERVE: usize = 2;
const ID_CONTINUE: usize = 3;
const BST_CHECKED: usize = 1;
const BST_UNCHECKED: usize = 0;

const WINDOW_WIDTH: i32 = 460;
const MARGIN: i32 = 14;
const CONTENT_W: i32 = WINDOW_WIDTH - 2 * MARGIN - 4;

const BODY_TEXT: PCWSTR = w!(
    "Ao desinstalar, você decide o que acontece com os dados guardados fora \
     da pasta de instalação: o histórico de tudo que foi copiado, a \
     configuração e os registros de diagnóstico.\r\n\r\n\
     Apagar: remove tudo, sem deixar rastro.\r\n\
     Preservar: mantém os dados no disco, para uma instalação futura recuperá-los."
);

const PROTECTED_WARNING: PCWSTR = w!(
    "O histórico está protegido pela sua conta do Windows: se você preservar, \
     ele só volta a ser legível entrando novamente com esta mesma conta."
);

struct Ctx {
    radio_delete: HWND,
    radio_preserve: HWND,
    history_protected: bool,
}

thread_local! {
    static RESULT_DELETE: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn ask_delete_or_preserve(history_protected: bool) -> bool {
    RESULT_DELETE.with(|r| r.set(false));

    // SAFETY: registro de classe idempotente com wndproc válido;
    // `GetModuleHandleW(None)` pede o módulo do processo atual.
    let created = unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return false;
        };
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: cursor,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let ctx = Box::new(Ctx {
            radio_delete: HWND::default(),
            radio_preserve: HWND::default(),
            history_protected,
        });
        let ctx_ptr = Box::into_raw(ctx);

        let title = HSTRING::from("Desinstalar duplicata");
        let hwnd = CreateWindowExW(
            WS_EX_TOPMOST,
            CLASS_NAME,
            &title,
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            360,
            None,
            None,
            Some(hinstance.into()),
            Some(ctx_ptr.cast()),
        );
        match hwnd {
            Ok(hwnd) => {
                let _ = ShowWindow(hwnd, SW_SHOW);
                let _ = SetForegroundWindow(hwnd);
                true
            }
            Err(_) => {
                drop(Box::from_raw(ctx_ptr));
                false
            }
        }
    };
    if !created {
        return false;
    }

    let mut msg = MSG::default();
    // SAFETY: `GetMessageW`/`TranslateMessage`/`DispatchMessageW` com um
    // `MSG` válido — laço local só desta janela, termina quando ela posta
    // `WM_QUIT` (`WM_DESTROY` abaixo).
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }

    RESULT_DELETE.with(|r| r.get())
}

const fn duplicata_style(bs: i32) -> WINDOW_STYLE {
    WINDOW_STYLE(bs as u32)
}

/// # Safety
/// `hwnd` deve ser a janela recém-criada recebendo seu próprio `WM_CREATE`.
unsafe fn create_children(hwnd: HWND, ctx: &mut Ctx) {
    // SAFETY: pede o módulo do processo atual, sem pré-condição além disso.
    let hinstance = unsafe { GetModuleHandleW(None) }.ok();

    let mk =
        |class: PCWSTR, text: PCWSTR, extra: WINDOW_STYLE, y: i32, h: i32, id: usize| -> HWND {
            let menu = if id == 0 {
                None
            } else {
                Some(HMENU(id as *mut core::ffi::c_void))
            };
            // SAFETY: classe predefinida pelo sistema; `hwnd` é o pai, válido
            // (dentro do próprio WM_CREATE dele).
            unsafe {
                CreateWindowExW(
                    Default::default(),
                    class,
                    text,
                    WS_CHILD | WS_VISIBLE | extra,
                    MARGIN,
                    y,
                    CONTENT_W,
                    h,
                    Some(hwnd),
                    menu,
                    hinstance.map(Into::into),
                    None,
                )
                .unwrap_or_default()
            }
        };

    let mut y = MARGIN;
    mk(
        windows::core::w!("STATIC"),
        BODY_TEXT,
        duplicata_style(BS_MULTILINE),
        y,
        110,
        0,
    );
    y += 110 + 8;

    if ctx.history_protected {
        mk(
            windows::core::w!("STATIC"),
            PROTECTED_WARNING,
            duplicata_style(BS_MULTILINE),
            y,
            50,
            0,
        );
        y += 50 + 8;
    }

    let radio_delete = mk(
        windows::core::w!("BUTTON"),
        windows::core::w!("Apagar os dados"),
        WS_TABSTOP | duplicata_style(BS_AUTORADIOBUTTON),
        y,
        22,
        ID_RADIO_DELETE,
    );
    y += 22 + 4;
    let radio_preserve = mk(
        windows::core::w!("BUTTON"),
        windows::core::w!("Preservar os dados"),
        WS_TABSTOP | duplicata_style(BS_AUTORADIOBUTTON),
        y,
        22,
        ID_RADIO_PRESERVE,
    );
    y += 22 + 16;

    // SAFETY: os dois rádios acabaram de ser criados; "apagar" começa
    // marcado (FR-013 — apagar pré-selecionado).
    unsafe {
        SendMessageW(radio_delete, BM_SETCHECK, Some(WPARAM(BST_CHECKED)), None);
        SendMessageW(
            radio_preserve,
            BM_SETCHECK,
            Some(WPARAM(BST_UNCHECKED)),
            None,
        );
    }

    mk(
        windows::core::w!("BUTTON"),
        windows::core::w!("Continuar"),
        WS_TABSTOP,
        y,
        26,
        ID_CONTINUE,
    );
    y += 26 + MARGIN;

    ctx.radio_delete = radio_delete;
    ctx.radio_preserve = radio_preserve;

    let mut rc = RECT {
        left: 0,
        top: 0,
        right: WINDOW_WIDTH,
        bottom: y,
    };
    // SAFETY: só calcula sobre `rc`, local e válido.
    unsafe {
        let _ = AdjustWindowRect(&mut rc, WS_POPUP | WS_CAPTION | WS_SYSMENU, false);
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_NOMOVE | SWP_NOZORDER,
        );
    }
}

fn ctx_ref(hwnd: HWND) -> Option<&'static mut Ctx> {
    // SAFETY: `GWLP_USERDATA` só é lido depois de `WM_NCCREATE` já ter
    // posto o ponteiro válido (vindo de `Box::into_raw` em
    // `ask_delete_or_preserve`), e só até `WM_NCDESTROY` liberá-lo.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Ctx;
    if ptr.is_null() {
        None
    } else {
        // SAFETY: `ptr` não nulo, veio de `Box::into_raw` (contrato desta
        // função, ver comentário acima).
        Some(unsafe { &mut *ptr })
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um
            // CREATESTRUCTW cujo `lpCreateParams` é o nosso `*mut Ctx`.
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                if !cs.lpCreateParams.is_null() {
                    SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                }
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            if let Some(ctx) = ctx_ref(hwnd) {
                // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
                unsafe { create_children(hwnd, ctx) };
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            if let Some(ctx) = ctx_ref(hwnd) {
                if id == ID_RADIO_DELETE {
                    // SAFETY: rádios já criados (só chegamos aqui depois
                    // de WM_CREATE).
                    unsafe {
                        SendMessageW(
                            ctx.radio_delete,
                            BM_SETCHECK,
                            Some(WPARAM(BST_CHECKED)),
                            None,
                        );
                        SendMessageW(
                            ctx.radio_preserve,
                            BM_SETCHECK,
                            Some(WPARAM(BST_UNCHECKED)),
                            None,
                        );
                    }
                } else if id == ID_RADIO_PRESERVE {
                    // SAFETY: idem.
                    unsafe {
                        SendMessageW(
                            ctx.radio_delete,
                            BM_SETCHECK,
                            Some(WPARAM(BST_UNCHECKED)),
                            None,
                        );
                        SendMessageW(
                            ctx.radio_preserve,
                            BM_SETCHECK,
                            Some(WPARAM(BST_CHECKED)),
                            None,
                        );
                    }
                } else if id == ID_CONTINUE {
                    // SAFETY: `ctx.radio_delete` válido.
                    let checked = unsafe { SendMessageW(ctx.radio_delete, BM_GETCHECK, None, None) }
                        .0 as usize
                        == BST_CHECKED;
                    RESULT_DELETE.with(|r| r.set(checked));
                    // SAFETY: `hwnd` é o desta janela.
                    unsafe {
                        let _ = DestroyWindow(hwnd);
                    }
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // X/Alt+F4/menu de sistema: RESULT_DELETE fica no valor
            // inicial (`false`, preservar) — fechar sem confirmar nunca
            // apaga (Princípio III-A).
            // SAFETY: `hwnd` é o desta janela.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_DESTROY => {
            // SAFETY: encerra o laço LOCAL desta janela (não a message
            // loop compartilhada do app principal — este processo não tem
            // uma).
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: só lê o valor bruto do ponteiro; a validade dele é
            // checada abaixo antes de qualquer desreferência.
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut Ctx;
            if !ptr.is_null() {
                // SAFETY: `ptr` veio de `Box::into_raw`, nunca liberado
                // antes; nenhum código lê `GWLP_USERDATA` depois disto.
                drop(unsafe { Box::from_raw(ptr) });
            }
            // SAFETY: repassa ao handler padrão.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
