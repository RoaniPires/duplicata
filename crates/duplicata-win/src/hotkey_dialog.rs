use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use duplicata_core::{
    format_hotkey, log_event, validate_hotkey, Config, HotkeyCombo, HotkeyRejection, LogFields,
};
use tracing::Level;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect, InvalidateRect,
    SetBkMode, SetTextColor, DT_CENTER, DT_NOPREFIX, DT_WORDBREAK, HGDIOBJ, PAINTSTRUCT,
    TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VIRTUAL_KEY, VK_CONTROL, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW, LoadCursorW,
    RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, CREATESTRUCTW,
    CW_USEDEFAULT, GWLP_USERDATA, IDC_ARROW, SW_SHOW, WM_CLOSE, WM_CREATE, WM_KEYDOWN, WM_NCCREATE,
    WM_NCDESTROY, WM_PAINT, WNDCLASSW, WS_CAPTION, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU,
};

use crate::app_icon;
use crate::hotkey_capture::{self, CaptureOwner};
use crate::hotkey_win;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_hotkey_dialog");
const WINDOW_TITLE: PCWSTR = windows::core::w!("Atalho em uso — duplicata");
const WINDOW_WIDTH: i32 = 420;
const WINDOW_HEIGHT: i32 = 180;

struct HotkeyDialogCtx {
    history_hwnd: HWND,
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
    message: RefCell<String>,
}

pub fn open_after_conflict(
    history_hwnd: HWND,
    rejected: HotkeyCombo,
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
) {
    let message = format!(
        "\"{}\" já está em uso por outro programa.\n\nPressione a nova combinação de teclas.",
        format_hotkey(&rejected)
    );

    let ctx = Box::new(HotkeyDialogCtx {
        history_hwnd,
        config,
        config_path,
        registered_hotkey,
        message: RefCell::new(message),
    });
    let ctx_ptr = Box::into_raw(ctx);

    // SAFETY: registro de classe idempotente com `wndproc` válido (mesmo
    // padrão de `history_window.rs`/`tray.rs`); ignoramos erro de "classe já
    // registrada". `GetModuleHandleW(None)` pede o módulo do processo atual —
    // sem pré-condição além disso.
    unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return;
        };
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let icon = app_icon::load_app_icon().unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: cursor,
            hIcon: icon,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST,
            CLASS_NAME,
            WINDOW_TITLE,
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT,
            None,
            None,
            Some(hinstance.into()),
            Some(ctx_ptr.cast()),
        ) else {
            return;
        };

        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }

    hotkey_capture::arm(CaptureOwner::StandaloneDialog);
}

fn handle_keydown(hwnd: HWND, ctx: &HotkeyDialogCtx, wparam: WPARAM) {
    let vk = VIRTUAL_KEY(wparam.0 as u16);
    if matches!(vk, VK_CONTROL | VK_SHIFT | VK_MENU | VK_LWIN | VK_RWIN) {
        return;
    }
    let combo = HotkeyCombo {
        modifiers: current_modifiers(),
        vkey: vk.0 as u32,
    };
    try_combo(hwnd, ctx, combo);
}

pub(crate) fn current_modifiers() -> u32 {
    let mut modifiers = 0u32;
    if key_is_down(VK_CONTROL) {
        modifiers |= duplicata_core::MOD_CONTROL;
    }
    if key_is_down(VK_SHIFT) {
        modifiers |= duplicata_core::MOD_SHIFT;
    }
    if key_is_down(VK_MENU) {
        modifiers |= duplicata_core::MOD_ALT;
    }
    if key_is_down(VK_LWIN) || key_is_down(VK_RWIN) {
        modifiers |= duplicata_core::MOD_WIN;
    }
    modifiers
}

fn key_is_down(vk: VIRTUAL_KEY) -> bool {
    // SAFETY: `GetKeyState` não tem pré-condição além de um código de tecla
    // virtual válido.
    (unsafe { GetKeyState(vk.0 as i32) } as u16 & 0x8000) != 0
}

pub(crate) enum ExchangeOutcome {
    Accepted(HotkeyCombo),
    Refused { message: String },
}

pub(crate) fn try_exchange(
    probe_hwnd: HWND,
    history_hwnd: HWND,
    registered: Option<HotkeyCombo>,
    candidate: HotkeyCombo,
    config: &Rc<RefCell<Config>>,
    config_path: &Path,
) -> ExchangeOutcome {
    if let Err(rejection) = validate_hotkey(&candidate) {
        return ExchangeOutcome::Refused {
            message: format!(
                "\"{}\": {} Pressione outra combinação.",
                format_hotkey(&candidate),
                rejection_reason(rejection)
            ),
        };
    }

    if already_current(registered, candidate) {
        return ExchangeOutcome::Refused {
            message: format!(
                "\"{}\" já é a combinação em uso. Pressione outra.",
                format_hotkey(&candidate)
            ),
        };
    }

    if hotkey_win::register(probe_hwnd, &candidate).is_err() {
        return ExchangeOutcome::Refused {
            message: format!(
                "\"{}\" já está em uso por outro programa. Pressione outra combinação.",
                format_hotkey(&candidate)
            ),
        };
    }
    let _ = hotkey_win::unregister(probe_hwnd);

    if registered.is_some() {
        let _ = hotkey_win::unregister(history_hwnd);
    }
    if hotkey_win::register(history_hwnd, &candidate).is_err() {
        if let Some(previous) = registered {
            let _ = hotkey_win::register(history_hwnd, &previous);
        }
        return ExchangeOutcome::Refused {
            message: format!(
                "\"{}\" ficou indisponível durante o registro. Pressione outra combinação.",
                format_hotkey(&candidate)
            ),
        };
    }

    log_event!(Level::INFO, LogFields::new("hotkey_reconfigured"));
    config.borrow_mut().hotkey = candidate;
    let _ = config.borrow().persist(config_path);

    ExchangeOutcome::Accepted(candidate)
}

fn try_combo(hwnd: HWND, ctx: &HotkeyDialogCtx, combo: HotkeyCombo) {
    let outcome = try_exchange(
        hwnd,
        ctx.history_hwnd,
        ctx.registered_hotkey.get(),
        combo,
        &ctx.config,
        &ctx.config_path,
    );
    let accepted = match outcome {
        ExchangeOutcome::Accepted(combo) => combo,
        ExchangeOutcome::Refused { message } => {
            show_message(hwnd, ctx, message);
            return;
        }
    };
    ctx.registered_hotkey.set(Some(accepted));

    // SAFETY: `hwnd` foi criado por `open_after_conflict`. Depois desta
    // chamada, `ctx` não pode mais ser tocado nesta função nem por quem a
    // chamou — `WM_NCDESTROY` desaloca o `Box` por trás dele de forma
    // síncrona, antes de `DestroyWindow` retornar (ver doc do módulo e o
    // handler de `WM_NCDESTROY` abaixo). Nenhum código depois desta linha lê
    // `ctx`.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn already_current(registered: Option<HotkeyCombo>, candidate: HotkeyCombo) -> bool {
    registered == Some(candidate)
}

fn rejection_reason(rejection: HotkeyRejection) -> &'static str {
    match rejection {
        HotkeyRejection::NoModifier => "nenhum modificador (Ctrl/Alt/Shift) foi pressionado.",
        HotkeyRejection::ModifierOnly => "só modificadores foram pressionados, falta uma tecla.",
        HotkeyRejection::ReservedSystemCombo => "essa combinação é reservada pelo sistema.",
    }
}

fn show_message(hwnd: HWND, ctx: &HotkeyDialogCtx, message: String) {
    *ctx.message.borrow_mut() = message;
    // SAFETY: `hwnd` válido (chamado só a partir do próprio wndproc desta
    // janela); força um `WM_PAINT` com o texto atualizado.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

fn paint(hwnd: HWND, ctx: &HotkeyDialogCtx) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` padrão dentro do handler de
    // `WM_PAINT`; `hdc` devolvido é válido até o `EndPaint` no final desta
    // função.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

    let mut client = RECT::default();
    // SAFETY: `hwnd` válido.
    let _ = unsafe { GetClientRect(hwnd, &mut client) };

    let palette = crate::system_appearance::theme_palette(
        crate::system_appearance::SystemAppearance::read_dark(),
    );
    // SAFETY: `hdc` do `BeginPaint` acima; o brush é usado e liberado aqui.
    unsafe {
        let brush = CreateSolidBrush(palette.surface);
        FillRect(hdc, &client, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, palette.text_primary);
    }

    let mut text_rect = RECT {
        left: client.left + 16,
        top: client.top + 16,
        right: client.right - 16,
        bottom: client.bottom - 16,
    };

    let mut wide: Vec<u16> = ctx
        .message
        .borrow()
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();

    // SAFETY: `wide` é um buffer NUL-terminado válido pelo tempo da chamada;
    // `text_rect` aponta para uma `RECT` local válida.
    unsafe {
        DrawTextW(
            hdc,
            &mut wide,
            &mut text_rect,
            DT_CENTER | DT_WORDBREAK | DT_NOPREFIX,
        );
    }

    // SAFETY: fecha o par iniciado por `BeginPaint` acima.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

fn ctx_ref(hwnd: HWND) -> Option<&'static HotkeyDialogCtx> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut HotkeyDialogCtx` posto no
    // `WM_NCCREATE`; o ponteiro é válido até `WM_NCDESTROY`, que o desaloca.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const HotkeyDialogCtx;
    // SAFETY: `ptr` ou é nulo ou aponta para o `HotkeyDialogCtx` posto acima.
    unsafe { ptr.as_ref() }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o `*mut HotkeyDialogCtx` de `open()`
            // (mesmo padrão de `history_window.rs`/`tray.rs`).
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            // (Fatia 4, FR-006a) Adota o tema do Windows na ABERTURA — só a
            // barra de título / frame; não repinta ao vivo.
            // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
            unsafe {
                crate::window_material::apply_dark_titlebar(
                    hwnd,
                    crate::system_appearance::SystemAppearance::read_dark(),
                );
            }
            LRESULT(0)
        }
        WM_PAINT => {
            if let Some(ctx) = ctx_ref(hwnd) {
                paint(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if let Some(ctx) = ctx_ref(hwnd) {
                handle_keydown(hwnd, ctx, wparam);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Ao contrário da HistoryWindow (FR-031, nunca destruída), este
            // diálogo é descartável: o botão de fechar (X)/Alt+F4/menu de
            // sistema dispensa sem escolher um atalho, deixando o app sem
            // nenhum atalho global nesta sessão — estado visível e aceitável
            // (Princípio VII), não um crash.
            // SAFETY: `hwnd` é o desta janela.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            hotkey_capture::release(CaptureOwner::StandaloneDialog);
            // SAFETY: reconstrói e dropa o `Box` alocado em `open()` — nenhum
            // código lê `GWLP_USERDATA` de novo depois desta mensagem (é a
            // última que a janela recebe).
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut HotkeyDialogCtx;
            if !ptr.is_null() {
                // SAFETY: `ptr` veio de `Box::into_raw` em `open()` e nunca
                // foi liberado antes (esta é a única mensagem que libera).
                unsafe { drop(Box::from_raw(ptr)) };
            }
            // SAFETY: repassa ao handler padrão.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cfg() -> Rc<RefCell<Config>> {
        Rc::new(RefCell::new(Config::with_paths(
            PathBuf::from("test.db"),
            PathBuf::from("."),
        )))
    }

    #[test]
    fn already_current_is_true_only_when_something_is_registered_and_it_matches() {
        let combo = HotkeyCombo::DEFAULT;
        let other = HotkeyCombo {
            modifiers: duplicata_core::MOD_CONTROL,
            vkey: 0x41,
        };
        assert!(already_current(Some(combo), combo));
        assert!(!already_current(Some(other), combo));
        assert!(
            !already_current(None, combo),
            "FR-048a: nada registrado nunca é \"já é a atual\", mesmo que a \
             candidata seja igual à Config configurada"
        );
    }

    #[test]
    fn a_candidate_without_any_modifier_is_refused_before_touching_win32() {
        let candidate = HotkeyCombo {
            modifiers: 0,
            vkey: 0x41,
        };
        let outcome = try_exchange(
            HWND::default(),
            HWND::default(),
            None,
            candidate,
            &cfg(),
            Path::new("config.toml"),
        );
        let ExchangeOutcome::Refused { message } = outcome else {
            panic!("candidata sem modificador deveria ser recusada (FR-006)");
        };
        assert!(
            message.contains(&format_hotkey(&candidate)),
            "FR-048b: a recusa deve NOMEAR a combinação recusada — mensagem: {message:?}"
        );
        assert!(
            message.to_lowercase().contains("pressione outra"),
            "FR-048b: a recusa deve oferecer a ação de escolher outra — mensagem: {message:?}"
        );
    }

    #[test]
    fn a_candidate_equal_to_the_registered_combo_is_refused_as_already_current() {
        let combo = HotkeyCombo::DEFAULT;
        let outcome = try_exchange(
            HWND::default(),
            HWND::default(),
            Some(combo),
            combo,
            &cfg(),
            Path::new("config.toml"),
        );
        let ExchangeOutcome::Refused { message } = outcome else {
            panic!("a mesma combinação já registrada deveria ser recusada (FR-048)");
        };
        assert!(message.contains(&format_hotkey(&combo)));
        assert!(message.to_lowercase().contains("já é a combinação"));
    }

    #[test]
    fn refusal_messages_use_the_display_convention_not_the_serialization_one() {
        let candidate = HotkeyCombo {
            modifiers: 0,
            vkey: 0x50,
        };
        let outcome = try_exchange(
            HWND::default(),
            HWND::default(),
            None,
            candidate,
            &cfg(),
            Path::new("config.toml"),
        );
        let ExchangeOutcome::Refused { message } = outcome else {
            panic!("sem modificador deveria ser recusada");
        };
        assert!(
            message.contains("\"P\""),
            "esperado \"P\" (FR-034d) em {message:?}"
        );
        assert!(
            !message.contains(&format!("\"{}\"", candidate.format())),
            "NÃO deveria conter a grafia de serialização entre aspas (\"p\", minúscula) em \
             {message:?}"
        );
    }
}
