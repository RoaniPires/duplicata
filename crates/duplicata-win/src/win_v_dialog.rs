//! Janela de falha do Win+V: "O Windows não liberou o Win+V...", com
//! "Desligar a opção" e "Manter". Abre na inicialização e a cada reinício
//! do Explorer enquanto o Win+V não é registrado (quem decide é
//! `duplicata_core::win_v`); fecha sozinha quando ele passa a ser.
//!
//! "Desligar a opção" desliga e grava sem a confirmação do Salvar; a mesma
//! janela troca o texto pelo aviso e fica só com "OK". Sem botão de
//! reiniciar o Explorer (ele fica só nas Configurações). Ao fechar, se a
//! reserva também não pôde ser registrada, abre a janela de conflito que já
//! existe para ela.
//!
//! Aparência das Configurações: tema claro ou escuro do sistema, fonte de
//! mensagens e botões desenhados por `settings_dialog::controls`, tudo no DPI
//! da janela. A geometria vem de `duplicata_core::win_v_layout`.

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::rc::Rc;

use duplicata_core::settings_text as text;
use duplicata_core::win_v::{turned_off_error, turned_off_notice, WinVStatus};
use duplicata_core::win_v_layout::{
    failure_text_width, failure_window_layout, FailureLayout, FailureMeasures,
};
use duplicata_core::{format_hotkey, log_event, LogFields};
use tracing::Level;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateFontIndirectW, CreateSolidBrush, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkMode, SetTextColor,
    DT_CALCRECT, DT_NOPREFIX, DT_WORDBREAK, HFONT, HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::HiDpi::{AdjustWindowRectExForDpi, GetDpiForWindow};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetCursorPos, GetDlgItem, GetWindowLongPtrW,
    GetWindowRect, IsWindow, LoadCursorW, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, BN_CLICKED, BS_OWNERDRAW, CREATESTRUCTW,
    GWLP_USERDATA, HMENU, IDC_ARROW, SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DPICHANGED, WM_DRAWITEM,
    WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETFONT, WM_SETTINGCHANGE,
    WM_THEMECHANGED, WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_TOPMOST, WS_POPUP, WS_SYSMENU,
    WS_TABSTOP, WS_VISIBLE,
};

use crate::app_icon;
use crate::hotkey_dialog;
use crate::settings_dialog::controls::{self, ControlColors};
use crate::settings_dialog::{self, control_look, font_height, message_font, work_area};
use crate::system_appearance::{self, ThemePalette};
use crate::text_metrics;
use crate::win_v_runtime::WinVRuntime;
use crate::{dpi, window_material};

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_win_v_dialog");

#[doc(hidden)]
pub const ID_TURN_OFF: i32 = 401;
#[doc(hidden)]
pub const ID_KEEP: i32 = 402;
#[doc(hidden)]
pub const ID_OK: i32 = 403;

/// Tema, fonte e cores no DPI atual da janela. Recriado a cada troca de
/// tema ou de DPI.
struct Look {
    dark: bool,
    palette: ThemePalette,
    colors: ControlColors,
    dpi: u32,
    font: HFONT,
    line_height: i32,
}

impl Look {
    fn load(dpi: u32) -> Look {
        let (dark, palette, colors) = control_look(dpi);
        let lf = message_font(dpi);
        // SAFETY: `lf` preenchido; a fonte é nossa e apagada em `release`.
        let font = unsafe { CreateFontIndirectW(&lf) };
        Look {
            dark,
            palette,
            colors,
            dpi,
            line_height: font_height(font),
            font,
        }
    }

    /// Só depois que os botões já receberam a fonte nova.
    fn release(&self) {
        if !self.font.is_invalid() {
            // SAFETY: fonte criada em `load`, sem DC que a tenha selecionada
            // (as pinturas restauram a anterior).
            unsafe {
                let _ = DeleteObject(HGDIOBJ(self.font.0));
            }
        }
    }
}

struct WinVDialogCtx {
    win_v: Rc<WinVRuntime>,
    message: RefCell<String>,
    error: Cell<Option<&'static str>>,
    look: RefCell<Look>,
    layout: Cell<FailureLayout>,
}

#[derive(Clone, Copy)]
enum Placement {
    /// Centrada na área útil do monitor da janela.
    Center,
    /// Canto superior esquerdo pedido (o `RECT` sugerido do `WM_DPICHANGED`).
    At(i32, i32),
}

thread_local! {
    static OPEN_HWND: Cell<Option<isize>> = const { Cell::new(None) };
}

fn open_window() -> Option<HWND> {
    let bits = OPEN_HWND.with(Cell::get)?;
    let hwnd = HWND(bits as *mut c_void);
    // SAFETY: `IsWindow` só consulta, mesmo com um handle já destruído.
    unsafe { IsWindow(Some(hwnd)) }.as_bool().then_some(hwnd)
}

pub fn is_open() -> bool {
    open_window().is_some()
}

#[doc(hidden)]
pub fn window() -> Option<HWND> {
    open_window()
}

fn style() -> WINDOW_STYLE {
    WS_POPUP | WS_CAPTION | WS_SYSMENU
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Abre a janela; aberta, só a traz para a frente (nunca duas).
pub fn open(win_v: Rc<WinVRuntime>) {
    if let Some(hwnd) = open_window() {
        // SAFETY: `hwnd` é a janela viva.
        unsafe {
            let _ = SetForegroundWindow(hwnd);
        }
        return;
    }
    let message = text::win_v_failure(&format_hotkey(&win_v.reserve()));
    let ctx = Box::new(WinVDialogCtx {
        win_v,
        message: RefCell::new(message),
        error: Cell::new(None),
        look: RefCell::new(Look::load(system_appearance::DEFAULT_DPI)),
        layout: Cell::new(FailureLayout::default()),
    });
    let title = wide(text::WIN_V_FAILURE_TITLE);

    let mut cursor = POINT::default();
    // SAFETY: `GetCursorPos` só escreve `cursor`; falhar deixa (0, 0).
    let _ = unsafe { GetCursorPos(&mut cursor) };
    // SAFETY: `cursor` é uma coordenada de tela.
    let (work, _) = unsafe { dpi::monitor_at(cursor) };

    // SAFETY: registro de classe idempotente com `wndproc` válido (mesmo
    // padrão das Configurações); a janela nasce no monitor do cursor, para
    // já ter o DPI dele, e o `WM_CREATE` mede, redimensiona e centra. O
    // `Box` passa a ser da janela no `WM_NCCREATE` e volta no `WM_NCDESTROY`.
    unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return;
        };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            hIcon: app_icon::load_app_icon().unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let ptr = Box::into_raw(ctx);
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_TOPMOST,
            CLASS_NAME,
            PCWSTR(title.as_ptr()),
            style(),
            work.left + 8,
            work.top + 8,
            200,
            200,
            None,
            None,
            Some(hinstance.into()),
            Some(ptr.cast()),
        ) else {
            // Vazar é o lado seguro: nunca liberar duas vezes o que o
            // `WM_NCDESTROY` já soltou.
            return;
        };
        OPEN_HWND.with(|c| c.set(Some(hwnd.0 as isize)));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// O Win+V passou a ser registrado: a janela fecha sozinha, sem abrir a
/// de conflito.
pub fn close() {
    if let Some(hwnd) = open_window() {
        // SAFETY: `hwnd` é a janela viva; o contexto sai no `WM_NCDESTROY`.
        unsafe {
            let _ = DestroyWindow(hwnd);
        }
    }
}

/// Texto que a janela mostra agora (aviso e linha de erro).
#[doc(hidden)]
pub fn shown_text() -> Option<String> {
    let ctx = ctx_ref(open_window()?)?;
    Some(painted_text(ctx))
}

#[doc(hidden)]
pub fn current_layout() -> Option<FailureLayout> {
    ctx_ref(open_window()?).map(|ctx| ctx.layout.get())
}

#[doc(hidden)]
pub fn current_dpi() -> Option<u32> {
    ctx_ref(open_window()?).map(|ctx| ctx.look.borrow().dpi)
}

#[doc(hidden)]
pub fn is_dark() -> Option<bool> {
    ctx_ref(open_window()?).map(|ctx| ctx.look.borrow().dark)
}

fn painted_text(ctx: &WinVDialogCtx) -> String {
    let mut out = ctx.message.borrow().clone();
    if let Some(error) = ctx.error.get() {
        out.push_str("\n\n");
        out.push_str(error);
    }
    out
}

fn ctx_ref(hwnd: HWND) -> Option<&'static WinVDialogCtx> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut WinVDialogCtx` posto no
    // `WM_NCCREATE`; válido até o `WM_NCDESTROY`, que o zera e solta.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const WinVDialogCtx;
    // SAFETY: nulo ou o ponteiro acima.
    unsafe { ptr.as_ref() }
}

fn button(hwnd: HWND, id: i32) -> Option<HWND> {
    // SAFETY: só consulta os filhos desta janela.
    unsafe { GetDlgItem(Some(hwnd), id) }.ok()
}

/// # Safety
/// Dentro do `WM_CREATE` de `hwnd`.
unsafe fn create_buttons(hwnd: HWND) {
    // SAFETY: `hwnd` é o pai, no próprio `WM_CREATE`; o tamanho e a fonte
    // chegam no `relayout`. O Tab segue a ordem de criação.
    unsafe {
        let hinstance = GetModuleHandleW(None).ok();
        for (id, label, visible) in [
            (ID_TURN_OFF, text::BUTTON_TURN_OFF, true),
            (ID_KEEP, text::BUTTON_KEEP, true),
            (ID_OK, text::BUTTON_OK, false),
        ] {
            let label = wide(label);
            let style = WS_CHILD
                | WS_TABSTOP
                | WINDOW_STYLE(BS_OWNERDRAW as u32)
                | if visible { WS_VISIBLE } else { WINDOW_STYLE(0) };
            let _ = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::w!("BUTTON"),
                PCWSTR(label.as_ptr()),
                style,
                0,
                0,
                0,
                0,
                Some(hwnd),
                Some(HMENU(id as usize as *mut c_void)),
                hinstance.map(Into::into),
                None,
            );
        }
    }
}

/// Altura de `s` quebrado na largura `width`, com a fonte da janela.
fn text_height(font: HFONT, s: &str, width: i32) -> i32 {
    let mut buf: Vec<u16> = s.encode_utf16().collect();
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: width,
        bottom: 0,
    };
    // SAFETY: DC de memória criado e destruído aqui; a fonte volta a sair do
    // DC antes do `DeleteDC`; `DT_CALCRECT` só escreve no `RECT` local.
    unsafe {
        let dc = CreateCompatibleDC(None);
        let old = SelectObject(dc, HGDIOBJ(font.0));
        DrawTextW(
            dc,
            &mut buf,
            &mut rc,
            DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
        );
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
    }
    rc.bottom
}

/// Mede no DPI atual, entrega a fonte aos botões, redimensiona e posiciona.
/// A altura é a do texto mais alto que a janela pode mostrar, para não
/// mudar de tamanho ao trocar o texto.
fn relayout(hwnd: HWND, ctx: &WinVDialogCtx, placement: Placement) {
    let (font, dpi, line_height) = {
        let look = ctx.look.borrow();
        (look.font, look.dpi, look.line_height)
    };
    let mut m = FailureMeasures {
        dpi,
        line_height,
        button_text_width: [text::BUTTON_TURN_OFF, text::BUTTON_KEEP, text::BUTTON_OK]
            .iter()
            .map(|s| text_metrics::width_with_font(font, s))
            .max()
            .unwrap_or(0),
        text_height: 0,
    };
    let width = failure_text_width(&m);
    let failure = ctx.message.borrow().clone();
    m.text_height = [
        format!("{failure}\n\n{}", text::WIN_V_REGISTRY_FAILED),
        failure,
        text::WIN_V_RETURNS_AFTER.to_string(),
        text::WIN_V_WAS_DISABLED_BEFORE.to_string(),
    ]
    .iter()
    .map(|s| text_height(font, s, width))
    .max()
    .unwrap_or(line_height);
    let layout = failure_window_layout(&m);
    ctx.layout.set(layout);

    let mut frame = RECT {
        left: 0,
        top: 0,
        right: layout.client_width,
        bottom: layout.client_height,
    };
    // SAFETY: só calcula sobre o `RECT` local.
    let _ = unsafe { AdjustWindowRectExForDpi(&mut frame, style(), false, WS_EX_TOPMOST, dpi) };
    let (w, h) = (frame.right - frame.left, frame.bottom - frame.top);
    let work = work_area(hwnd);
    let (x, y) = match placement {
        Placement::Center => (
            work.left + (work.right - work.left - w) / 2,
            work.top + (work.bottom - work.top - h) / 2,
        ),
        Placement::At(x, y) => (x, y),
    };
    let x = x.min(work.right - w).max(work.left);
    let y = y.min(work.bottom - h).max(work.top);
    // SAFETY: `hwnd` é esta janela e os botões são filhos dela; a fonte vive
    // até o próximo `Look::release`, que só vem depois de outra entregue.
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        for (id, slot) in [(ID_TURN_OFF, 0), (ID_KEEP, 1), (ID_OK, 1)] {
            let Some(b) = button(hwnd, id) else {
                continue;
            };
            let r = layout.buttons[slot];
            SendMessageW(
                b,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
            let _ = SetWindowPos(
                b,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// Troca de tema ou de DPI: carrega o visual novo e solta o anterior.
fn reload_look(hwnd: HWND, ctx: &WinVDialogCtx, dpi: u32) {
    let old = ctx.look.replace(Look::load(dpi));
    let dark = ctx.look.borrow().dark;
    // SAFETY: `hwnd` é esta janela, desta thread.
    unsafe { window_material::apply_dark_titlebar(hwnd, dark) };
    // A fonte nova vai aos botões antes de a velha ser apagada.
    let font = ctx.look.borrow().font;
    for id in [ID_TURN_OFF, ID_KEEP, ID_OK] {
        if let Some(b) = button(hwnd, id) {
            // SAFETY: `b` é filho desta janela.
            unsafe {
                SendMessageW(
                    b,
                    WM_SETFONT,
                    Some(WPARAM(font.0 as usize)),
                    Some(LPARAM(1)),
                );
            }
        }
    }
    old.release();
}

fn on_turn_off(hwnd: HWND, ctx: &WinVDialogCtx) {
    let was_ours = ctx.win_v.win_v_config().v_ours;
    match ctx.win_v.turn_off_and_persist() {
        Ok(persisted) => {
            *ctx.message.borrow_mut() = turned_off_notice(was_ours).to_string();
            // O registro mudou; sem o `config.toml` gravado, a linha de erro
            // fica junto do aviso.
            ctx.error.set(turned_off_error(persisted));
            for (id, show) in [(ID_TURN_OFF, false), (ID_KEEP, false), (ID_OK, true)] {
                if let Some(b) = button(hwnd, id) {
                    // SAFETY: `b` é filho desta janela.
                    unsafe {
                        let _ = ShowWindow(b, if show { SW_SHOW } else { SW_HIDE });
                    }
                }
            }
            settings_dialog::refresh_if_open();
        }
        Err(_) => {
            log_event!(Level::WARN, LogFields::new("win_v_disable_failed"));
            ctx.error.set(Some(text::WIN_V_REGISTRY_FAILED));
        }
    }
    // SAFETY: `hwnd` é esta janela; repinta com o texto novo.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

/// Fecha pelo usuário ("Manter", "OK" ou X). Sem nenhum atalho registrado,
/// a janela de conflito da reserva abre em seguida.
fn close_by_user(hwnd: HWND, ctx: &WinVDialogCtx) {
    let win_v = Rc::clone(&ctx.win_v);
    // SAFETY: `hwnd` é esta janela; `ctx` não é tocado depois desta linha
    // (o `WM_NCDESTROY` o solta dentro do `DestroyWindow`).
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    if win_v.status() != WinVStatus::Active && win_v.registered_hotkey().get().is_none() {
        hotkey_dialog::open_after_conflict(
            win_v.history_hwnd(),
            win_v.reserve(),
            Rc::clone(win_v.config()),
            win_v.config_path().clone(),
            Rc::clone(win_v.registered_hotkey()),
        );
    }
}

fn paint(hwnd: HWND, ctx: &WinVDialogCtx) {
    let look = ctx.look.borrow();
    let layout = ctx.layout.get();
    let mut wide: Vec<u16> = painted_text(ctx).encode_utf16().collect();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` no `WM_PAINT`; o pincel é criado e
    // apagado aqui; a fonte anterior volta antes do `EndPaint`; `wide` vive
    // durante a chamada.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let brush = CreateSolidBrush(look.palette.surface);
        FillRect(hdc, &ps.rcPaint, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let old = SelectObject(hdc, HGDIOBJ(look.font.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, look.palette.text_primary);
        let mut rc = RECT {
            left: layout.text.left,
            top: layout.text.top,
            right: layout.text.right,
            bottom: layout.text.bottom,
        };
        DrawTextW(hdc, &mut wide, &mut rc, DT_WORDBREAK | DT_NOPREFIX);
        SelectObject(hdc, old);
        let _ = EndPaint(hwnd, &ps);
    }
}

fn window_dpi(hwnd: HWND) -> u32 {
    // SAFETY: `hwnd` válido.
    match unsafe { GetDpiForWindow(hwnd) } {
        0 => system_appearance::DEFAULT_DPI,
        d => d,
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o `*mut WinVDialogCtx` de `open()`.
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            if let Some(ctx) = ctx_ref(hwnd) {
                // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
                unsafe { create_buttons(hwnd) };
                reload_look(hwnd, ctx, window_dpi(hwnd));
                relayout(hwnd, ctx, Placement::Center);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(ctx) = ctx_ref(hwnd) {
                paint(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            if let Some(ctx) = ctx_ref(hwnd) {
                let (font, colors) = {
                    let look = ctx.look.borrow();
                    (look.font, look.colors)
                };
                // SAFETY: num `WM_DRAWITEM`, `lParam` aponta para o
                // `DRAWITEMSTRUCT` do botão, válido durante o handler.
                unsafe {
                    controls::draw_button_with(&*(lparam.0 as *const DRAWITEMSTRUCT), font, colors)
                };
            }
            LRESULT(1)
        }
        WM_THEMECHANGED => {
            if let Some(ctx) = ctx_ref(hwnd) {
                let dpi = ctx.look.borrow().dpi;
                reload_look(hwnd, ctx, dpi);
                // SAFETY: `hwnd` é esta janela; repinta com as cores novas.
                unsafe {
                    let _ = InvalidateRect(Some(hwnd), None, true);
                }
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if system_appearance::is_immersive_color_set(lparam) {
                if let Some(ctx) = ctx_ref(hwnd) {
                    let dpi = ctx.look.borrow().dpi;
                    reload_look(hwnd, ctx, dpi);
                    // SAFETY: `hwnd` é esta janela; repinta com as cores novas.
                    unsafe {
                        let _ = InvalidateRect(Some(hwnd), None, true);
                    }
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(ctx) = ctx_ref(hwnd) {
                let dpi = ((wparam.0 >> 16) & 0xFFFF) as u32;
                let at = if lparam.0 == 0 {
                    let mut r = RECT::default();
                    // SAFETY: `hwnd` válido; só escreve `r`.
                    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
                    (r.left, r.top)
                } else {
                    // SAFETY: num `WM_DPICHANGED`, `lParam` aponta para o
                    // `RECT` sugerido pelo SO, válido durante o handler.
                    let r = unsafe { *(lparam.0 as *const RECT) };
                    (r.left, r.top)
                };
                let dpi = if dpi == 0 { ctx.look.borrow().dpi } else { dpi };
                reload_look(hwnd, ctx, dpi);
                relayout(hwnd, ctx, Placement::At(at.0, at.1));
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if let (BN_CLICKED, Some(ctx)) = (code, ctx_ref(hwnd)) {
                match id {
                    ID_TURN_OFF => on_turn_off(hwnd, ctx),
                    ID_KEEP | ID_OK => close_by_user(hwnd, ctx),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(ctx) = ctx_ref(hwnd) {
                close_by_user(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            OPEN_HWND.with(|c| {
                if c.get() == Some(hwnd.0 as isize) {
                    c.set(None);
                }
            });
            // SAFETY: zera e solta o `Box` de `open()`; nada mais o lê.
            let ptr = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut WinVDialogCtx;
            if !ptr.is_null() {
                // SAFETY: `ptr` veio de `Box::into_raw` em `open()`, uma vez;
                // os botões, que usavam a fonte, já foram destruídos.
                let ctx = unsafe { Box::from_raw(ptr) };
                ctx.look.borrow().release();
                drop(ctx);
            }
            // SAFETY: repassa ao handler padrão.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
