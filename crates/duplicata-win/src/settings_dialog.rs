//! Janela de Configurações.
//!
//! Casca: cria os controles, desenha com a paleta e a fonte do Histórico e
//! executa os efeitos que `duplicata_core::settings_session` devolve. O que é
//! reversível, o que o Aplicar, o Cancelar e o Salvar colocam em efeito e o
//! texto da confirmação vêm de lá; a geometria vem de
//! `duplicata_core::settings_layout`.
//!
//! Estrutura: a janela principal tem a área de conteúdo (filha com barra de
//! rolagem, `content`), a linha de erro e o rodapé. As teclas de navegação
//! (Tab, Shift+Tab, Esc) passam por [`pre_translate`], chamado pelo laço de
//! mensagens, que restringe `IsDialogMessageW` a esta janela e à
//! confirmação. Superfície opaca: os controles GDI não escrevem alfa.

mod confirm;
mod content;
pub(crate) mod controls;
mod tooltip;

use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use duplicata_core::filter_strip::Rect as CoreRect;
use duplicata_core::row_layout::scale_px;
use duplicata_core::settings_input::NUMERIC_MAX_DIGITS;
use duplicata_core::settings_layout::{settings_layout, LayoutMeasures, SettingsLayout};
use duplicata_core::settings_session::{
    startup_for_settings, Effect, HotkeyLineView, NumericField, SaveOutcome, SaveReport, SaveStep,
    SettingsSession,
};
use duplicata_core::settings_text as text;
use duplicata_core::theme;
use duplicata_core::win_v::{restart_error_line, ExplorerRestartOutcome};
use duplicata_core::{
    interpret_approval, log_event, CaptureQueue, Config, HotkeyCombo, LogFields, WorkItem,
};
use tracing::Level;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateFontIndirectW, CreateSolidBrush, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, FillRect, GetMonitorInfoW, GetTextMetricsW, InvalidateRect,
    MonitorFromWindow, RedrawWindow, SelectObject, SetBkMode, SetTextColor, DT_END_ELLIPSIS,
    DT_NOPREFIX, DT_WORDBREAK, FW_SEMIBOLD, HBRUSH, HDC, HFONT, HGDIOBJ, LOGFONTW, MONITORINFO,
    MONITOR_DEFAULTTONEAREST, PAINTSTRUCT, RDW_ALLCHILDREN, RDW_ERASE, RDW_INVALIDATE,
    RDW_UPDATENOW, TEXTMETRICW, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    SetWindowTheme, DRAWITEMSTRUCT, EM_SETLIMITTEXT, EM_SETSEL, WM_MOUSEHOVER, WM_MOUSELEAVE,
};
use windows::Win32::UI::HiDpi::{
    AdjustWindowRectExForDpi, GetDpiForWindow, GetSystemMetricsForDpi, SystemParametersInfoForDpi,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, GetFocus, SetFocus, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU,
    VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetCursorPos, GetDlgItem,
    GetWindowLongPtrW, GetWindowRect, IsChild, IsDialogMessageW, IsWindow, LoadCursorW,
    RegisterClassW, SendMessageW, SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    BM_GETCHECK, BM_SETCHECK, BN_CLICKED, BS_OWNERDRAW, CREATESTRUCTW, EN_CHANGE, ES_AUTOHSCROLL,
    ES_NUMBER, GWLP_USERDATA, HMENU, IDC_ARROW, LBS_NOINTEGRALHEIGHT, LBS_NOTIFY, LB_ADDSTRING,
    LB_GETCURSEL, LB_RESETCONTENT, MSG, NONCLIENTMETRICSW, SM_CXVSCROLL, SPI_GETNONCLIENTMETRICS,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_HIDE, SW_SHOW, WA_INACTIVE, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_ACTIVATE, WM_APP, WM_CLOSE, WM_COMMAND, WM_CREATE, WM_DPICHANGED, WM_DRAWITEM,
    WM_ERASEBKGND, WM_GETDLGCODE, WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN, WM_KILLFOCUS,
    WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETFONT, WM_SETTEXT,
    WM_SETTINGCHANGE, WM_SYSKEYDOWN, WM_THEMECHANGED, WNDCLASSW, WS_CAPTION, WS_CHILD,
    WS_CLIPCHILDREN, WS_EX_CONTROLPARENT, WS_POPUP, WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::app_icon;
use crate::dpi;
use crate::history_window::{
    ENCRYPTION_TOGGLE_IDLE, ENCRYPTION_TOGGLE_TO_ON, WM_APP_QUERY_ENCRYPTION_TOGGLE,
    WM_APP_SET_MAX_PINNED, WM_APP_TOGGLE_ENCRYPTION,
};
use crate::hotkey_capture::{self, CaptureOwner};
use crate::hotkey_dialog::{self, OnSwapFailure};
use crate::startup_registry;
use crate::system_appearance::{self, SystemAppearance, ThemePalette};
use crate::text_metrics;
use crate::win_v_runtime::WinVRuntime;
use crate::window_material;
use crate::{explorer_restart, tray};

use controls::ControlColors;
use tooltip::Tooltip;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_settings_dialog");
const SUBCLASS_HOTKEY_ID: usize = 1;
/// `WPARAM` = 1 se a confirmação do Salvar foi aceita.
const WM_APP_CONFIRM_RESULT: u32 = WM_APP + 40;
/// Reserva para a combinação do atalho: a mais larga que se espera.
const WIDEST_HOTKEY_SAMPLE: &str = "Ctrl+Shift+Alt+PageDown";

#[doc(hidden)]
pub const ID_CONTENT: i32 = 200;
#[doc(hidden)]
pub const ID_ALTER: i32 = 201;
#[doc(hidden)]
pub const ID_STARTUP: i32 = 202;
#[doc(hidden)]
pub const ID_RETENTION: i32 = 203;
#[doc(hidden)]
pub const ID_MAX_ITEMS: i32 = 204;
#[doc(hidden)]
pub const ID_MAX_PINNED: i32 = 205;
#[doc(hidden)]
pub const ID_LIST: i32 = 206;
#[doc(hidden)]
pub const ID_LIST_EDIT: i32 = 207;
#[doc(hidden)]
pub const ID_ADD: i32 = 208;
#[doc(hidden)]
pub const ID_REMOVE: i32 = 209;
#[doc(hidden)]
pub const ID_SECRETS: i32 = 210;
#[doc(hidden)]
pub const ID_PROTECTION: i32 = 211;
#[doc(hidden)]
pub const ID_SAVE: i32 = 212;
/// `IDCANCEL`: o Esc do `IsDialogMessageW` chega como o clique em Cancelar.
#[doc(hidden)]
pub const ID_CANCEL: i32 = 2;
#[doc(hidden)]
pub const ID_APPLY: i32 = 214;
#[doc(hidden)]
pub const ID_WIN_V: i32 = 215;
#[doc(hidden)]
pub const ID_RESTART_EXPLORER: i32 = 216;
const IDOK: usize = 1;

#[derive(Clone, Copy, Default)]
struct Controls {
    content: HWND,
    alter: HWND,
    startup: HWND,
    retention: HWND,
    items: HWND,
    pinned: HWND,
    list: HWND,
    list_edit: HWND,
    add: HWND,
    remove: HWND,
    secrets: HWND,
    protection: HWND,
    win_v: HWND,
    restart_explorer: HWND,
    save: HWND,
    cancel: HWND,
    apply: HWND,
}

impl Controls {
    fn numeric(&self) -> [(HWND, NumericField); 3] {
        [
            (self.retention, NumericField::RetentionDays),
            (self.items, NumericField::MaxItems),
            (self.pinned, NumericField::MaxPinned),
        ]
    }

    fn all(&self) -> [HWND; 16] {
        [
            self.alter,
            self.restart_explorer,
            self.win_v,
            self.startup,
            self.retention,
            self.items,
            self.pinned,
            self.list,
            self.list_edit,
            self.add,
            self.remove,
            self.secrets,
            self.protection,
            self.save,
            self.cancel,
            self.apply,
        ]
    }
}

/// Tema, fonte e pincéis no DPI atual. Recriado a cada troca de tema ou DPI.
struct Look {
    dark: bool,
    palette: ThemePalette,
    colors: ControlColors,
    dpi: u32,
    font: HFONT,
    title_font: HFONT,
    line_height: i32,
    surface_brush: HBRUSH,
    field_brush: HBRUSH,
}

/// Tema do sistema e as cores dos controles no DPI dado: as Configurações e
/// a janela de falha do Win+V usam as mesmas.
pub(crate) fn control_look(dpi: u32) -> (bool, ThemePalette, ControlColors) {
    let appearance = SystemAppearance::read(dpi, false);
    let palette = appearance.palette;
    let surface = system_appearance::to_core(palette.surface);
    let text = system_appearance::to_core(palette.text_primary);
    let field = system_appearance::from_core(theme::subtle_hover_fill(surface, text));
    let pressed = system_appearance::from_core(theme::composite_over(text, surface, 40));
    let colors = ControlColors {
        surface: palette.surface,
        text: palette.text_primary,
        dim: palette.text_dim,
        border: palette.border,
        field,
        pressed,
        accent: appearance.accent,
        on_accent: palette.surface,
        dpi,
    };
    (appearance.dark, palette, colors)
}

impl Look {
    fn load(dpi: u32) -> Look {
        let (dark, palette, colors) = control_look(dpi);
        let field = colors.field;
        let base = message_font(dpi);
        // SAFETY: `base` é um `LOGFONTW` preenchido; as fontes são nossas e
        // apagadas em `Look::release`.
        let font = unsafe { CreateFontIndirectW(&base) };
        let mut title = base;
        title.lfHeight = base.lfHeight.saturating_mul(5) / 4;
        title.lfWeight = FW_SEMIBOLD.0 as i32;
        // SAFETY: idem.
        let title_font = unsafe { CreateFontIndirectW(&title) };
        // SAFETY: pincéis nossos, apagados em `Look::release`.
        let (surface_brush, field_brush) =
            unsafe { (CreateSolidBrush(palette.surface), CreateSolidBrush(field)) };
        Look {
            dark,
            palette,
            colors,
            dpi,
            line_height: font_height(font),
            font,
            title_font,
            surface_brush,
            field_brush,
        }
    }

    /// Só depois que nenhum controle usa mais as fontes (a nova já foi
    /// entregue a eles) e a dica está escondida.
    fn release(&self) {
        for obj in [
            HGDIOBJ(self.font.0),
            HGDIOBJ(self.title_font.0),
            HGDIOBJ(self.surface_brush.0),
            HGDIOBJ(self.field_brush.0),
        ] {
            if !obj.is_invalid() {
                // SAFETY: objetos criados em `Look::load`, sem DC que os
                // tenha selecionados (as pinturas restauram o anterior).
                unsafe {
                    let _ = DeleteObject(obj);
                }
            }
        }
    }
}

/// Qual confirmação está aberta.
#[derive(Clone, Copy, PartialEq, Eq)]
enum ConfirmKind {
    Save,
    RestartExplorer,
}

struct SettingsCtx {
    win_v: Rc<WinVRuntime>,
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    queue: Arc<dyn CaptureQueue<WorkItem>>,
    history_hwnd: HWND,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
    session: RefCell<SettingsSession>,
    hwnd: Cell<HWND>,
    controls: Cell<Controls>,
    look: RefCell<Look>,
    layout: Cell<SettingsLayout>,
    scroll: Cell<i32>,
    hotkey_view: RefCell<HotkeyLineView>,
    error_line: Cell<Option<&'static str>>,
    hover_row: Cell<Option<usize>>,
    /// Último ponto do mouse (conteúdo) e a janela que o recebeu: o sistema
    /// repete o `WM_MOUSEMOVE` quando uma janela aparece sob o cursor parado.
    mouse_at: Cell<Option<(HWND, POINT)>>,
    tooltip: RefCell<Option<Tooltip>>,
    confirm: Cell<Option<HWND>>,
    confirm_kind: Cell<ConfirmKind>,
    last_focus: Cell<HWND>,
    /// Campo sendo reescrito pela janela: o `EN_CHANGE` dele não é do usuário.
    writing_fields: Cell<bool>,
    /// Textos da última pintura, para os testes.
    painted: RefCell<Vec<String>>,
}

thread_local! {
    static OPEN_HWND: Cell<Option<isize>> = const { Cell::new(None) };
}

#[derive(Clone, Copy)]
enum Placement {
    /// Centrada na área útil do monitor da janela.
    Center,
    /// Canto superior esquerdo pedido (o `RECT` sugerido do `WM_DPICHANGED`).
    At(i32, i32),
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Apply,
    Cancel,
    Save,
}

pub fn open(queue: Arc<dyn CaptureQueue<WorkItem>>, win_v: Rc<WinVRuntime>) {
    let config = Rc::clone(win_v.config());
    let config_path: PathBuf = win_v.config_path().clone();
    let history_hwnd = win_v.history_hwnd();
    let registered_hotkey = Rc::clone(win_v.registered_hotkey());
    if let Some((hwnd, ctx)) = open_window() {
        // Só traz para frente: o que estava pendente continua.
        // SAFETY: `hwnd` é a janela viva de `open_window`.
        unsafe {
            let _ = SetForegroundWindow(ctx.confirm.get().unwrap_or(hwnd));
        }
        refresh_hotkey_line(ctx);
        return;
    }

    let startup = startup_for_settings(
        startup_registry::run_entry_present(),
        interpret_approval(startup_registry::read_startup_approved_raw().as_deref()),
    );
    let mut session = SettingsSession::open(&config.borrow(), startup);
    session.sync_win_v(win_v.win_v_config(), win_v.status());
    if win_v.restart_in_progress() {
        session.explorer_restart_started();
    }
    // SAFETY: `SendMessageW` síncrono para o Histórico, desta thread; para um
    // `HWND` nulo (testes) devolve 0, que é "nenhuma reescrita".
    let toggle =
        unsafe { SendMessageW(history_hwnd, WM_APP_QUERY_ENCRYPTION_TOGGLE, None, None) }.0;
    if toggle != ENCRYPTION_TOGGLE_IDLE {
        session.protection_in_progress(toggle == ENCRYPTION_TOGGLE_TO_ON);
    }

    let ctx = Box::new(SettingsCtx {
        win_v,
        config,
        config_path,
        queue,
        history_hwnd,
        registered_hotkey,
        session: RefCell::new(session),
        hwnd: Cell::new(HWND::default()),
        controls: Cell::new(Controls::default()),
        look: RefCell::new(Look::load(system_appearance::DEFAULT_DPI)),
        layout: Cell::new(SettingsLayout::default()),
        scroll: Cell::new(0),
        hotkey_view: RefCell::new(HotkeyLineView {
            combo: String::new(),
            description: String::new(),
            alter_enabled: true,
            hint: None,
            status_line: None,
            restart_visible: false,
            restart_enabled: false,
        }),
        error_line: Cell::new(None),
        hover_row: Cell::new(None),
        mouse_at: Cell::new(None),
        tooltip: RefCell::new(None),
        confirm: Cell::new(None),
        confirm_kind: Cell::new(ConfirmKind::Save),
        last_focus: Cell::new(HWND::default()),
        writing_fields: Cell::new(false),
        painted: RefCell::new(Vec::new()),
    });

    let mut cursor = POINT::default();
    // SAFETY: `GetCursorPos` só escreve `cursor`; falhar deixa (0, 0).
    let _ = unsafe { GetCursorPos(&mut cursor) };
    // SAFETY: `cursor` é uma coordenada de tela.
    let (work, _) = unsafe { dpi::monitor_at(cursor) };

    // SAFETY: registro de classe idempotente com `wndproc` válido (mesmo
    // padrão das demais janelas); o erro de "classe já registrada" é
    // ignorado. A janela nasce no monitor do cursor, para já ter o DPI dele;
    // o `WM_CREATE` mede, redimensiona e centra.
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
        controls::register_toggle_class();
        content::register_class();

        let title = wide(text::WINDOW_TITLE);
        let ctx_ptr = Box::into_raw(ctx);
        let Ok(hwnd) = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            CLASS_NAME,
            PCWSTR(title.as_ptr()),
            main_style(),
            work.left + 8,
            work.top + 8,
            200,
            200,
            None,
            None,
            Some(hinstance.into()),
            Some(ctx_ptr.cast()),
        ) else {
            // Sem soltar o contexto: se a janela chegou a nascer, o
            // `WM_NCDESTROY` já o soltou; senão ele vaza, o lado seguro
            // (nunca liberado duas vezes).
            return;
        };

        OPEN_HWND.with(|c| c.set(Some(hwnd.0 as isize)));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
        if let Some(ctx) = ctx_of(hwnd) {
            let _ = SetFocus(Some(ctx.controls.get().alter));
        }
    }
}

/// Sair do duplicata com a janela aberta equivale a Cancelar.
pub fn cancel_if_open() {
    if let Some((hwnd, ctx)) = open_window() {
        cancel(hwnd, ctx);
    }
}

/// O estado do Win+V mudou fora da janela (reinício do Explorer, janela de
/// falha): a linha de estado, o botão e a alternância acompanham.
pub fn refresh_if_open() {
    if let Some((_, ctx)) = open_window() {
        sync_win_v(ctx);
        sync_toggles(ctx);
        refresh_hotkey_line(ctx);
    }
}

/// O reinício pedido pelo botão terminou. Com as Configurações fechadas, um
/// erro aparece numa caixa de mensagem.
pub fn explorer_restart_finished(outcome: ExplorerRestartOutcome) {
    let error = restart_error_line(outcome);
    match open_window() {
        Some((_, ctx)) => {
            ctx.session.borrow_mut().explorer_restart_finished();
            sync_win_v(ctx);
            ctx.error_line.set(error);
            refresh_hotkey_line(ctx);
            redraw_all(ctx);
        }
        None => {
            if let Some(message) = error {
                crate::dialog::show_settings_error(message);
            }
        }
    }
}

fn sync_win_v(ctx: &SettingsCtx) {
    let (config, status) = (ctx.win_v.win_v_config(), ctx.win_v.status());
    ctx.session.borrow_mut().sync_win_v(config, status);
}

/// Chamado pelo Histórico quando a reescrita da proteção termina.
pub fn on_protection_finished() {
    if let Some((_, ctx)) = open_window() {
        ctx.session.borrow_mut().protection_finished();
        sync_toggles(ctx);
    }
}

/// Navegação de teclado (Tab, Shift+Tab, Esc) e a dica de ferramenta sobre
/// os controles. Devolve `true` quando a mensagem já foi tratada e não deve
/// ser despachada.
pub fn pre_translate(msg: &MSG) -> bool {
    let Some((hwnd, ctx)) = open_window() else {
        return false;
    };
    let content_hwnd = ctx.controls.get().content;
    if matches!(msg.message, WM_MOUSEMOVE | WM_MOUSEHOVER | WM_MOUSELEAVE)
        && msg.hwnd != content_hwnd
        // SAFETY: `IsChild` só consulta a árvore de janelas.
        && unsafe { IsChild(content_hwnd, msg.hwnd) }.as_bool()
    {
        content::forward_mouse(ctx, msg);
    }
    if let Some(confirm) = ctx.confirm.get() {
        // SAFETY: `confirm` é válida ou já destruída: o resultado que ela manda
        // logo depois de se destruir limpa `ctx.confirm`, e um `HWND` morto só
        // faz o `IsDialogMessageW` devolver `false`; `msg` é válido.
        return unsafe { IsDialogMessageW(confirm, msg) }.as_bool();
    }
    // SAFETY: `hwnd` é a janela viva de `open_window`; `IsDialogMessageW` só
    // trata mensagens dela e das filhas e devolve `false` para as demais.
    let handled = unsafe { IsDialogMessageW(hwnd, msg) }.as_bool();
    // A janela pode ter fechado dentro do `IsDialogMessageW` (Esc).
    if handled {
        if let Some((_, ctx)) = open_window() {
            reveal_focus(ctx);
        }
    }
    handled
}

fn open_window() -> Option<(HWND, &'static SettingsCtx)> {
    let bits = OPEN_HWND.with(Cell::get)?;
    let hwnd = HWND(bits as *mut c_void);
    // SAFETY: `IsWindow` só consulta, mesmo com um handle já destruído.
    if !unsafe { IsWindow(Some(hwnd)) }.as_bool() {
        return None;
    }
    ctx_of(hwnd).map(|ctx| (hwnd, ctx))
}

fn ctx_of(hwnd: HWND) -> Option<&'static SettingsCtx> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut SettingsCtx` posto em
    // `WM_NCCREATE`; válido até `WM_NCDESTROY`, que o desaloca e o zera.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const SettingsCtx;
    // SAFETY: nulo ou o ponteiro acima.
    unsafe { ptr.as_ref() }
}

fn main_style() -> WINDOW_STYLE {
    WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_CLIPCHILDREN
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

pub(crate) fn message_font(dpi: u32) -> LOGFONTW {
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: core::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    // SAFETY: `cbSize` preenchido (exigido pela API); a chamada só escreve
    // no buffer passado, com a fonte já no DPI pedido.
    let ok = unsafe {
        SystemParametersInfoForDpi(
            SPI_GETNONCLIENTMETRICS.0,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut c_void),
            0,
            dpi,
        )
    }
    .is_ok();
    if ok {
        ncm.lfMessageFont
    } else {
        LOGFONTW {
            lfHeight: -scale_px(12, dpi),
            ..Default::default()
        }
    }
}

pub(crate) fn font_height(font: HFONT) -> i32 {
    // SAFETY: DC de memória criado e destruído aqui; a fonte volta a sair do
    // DC antes do `DeleteDC`.
    unsafe {
        let dc = CreateCompatibleDC(None);
        if dc.is_invalid() {
            return 16;
        }
        let old = (!font.is_invalid()).then(|| SelectObject(dc, HGDIOBJ(font.0)));
        let mut tm = TEXTMETRICW::default();
        let h = if GetTextMetricsW(dc, &mut tm).as_bool() {
            tm.tmHeight.max(1)
        } else {
            16
        };
        if let Some(old) = old {
            SelectObject(dc, old);
        }
        let _ = DeleteDC(dc);
        h
    }
}

fn win_rect(r: CoreRect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn measures(look: &Look, view: &HotkeyLineView) -> LayoutMeasures {
    let font = look.font;
    let width = |s: &str| text_metrics::width_with_font(font, s);
    let mut widest_label = 0;
    for o in text::OPTIONS {
        widest_label = widest_label.max(width(o.name)).max(width(o.description));
    }
    for s in [
        text::STARTUP_BLOCKED,
        text::HOTKEY_ARMED,
        text::HOTKEY_PENDING,
        text::WIN_V_HINT,
    ] {
        widest_label = widest_label.max(width(s));
    }
    let button_text_width = [
        text::BUTTON_SAVE,
        text::BUTTON_CANCEL,
        text::BUTTON_APPLY,
        text::BUTTON_ALTER,
        text::BUTTON_ADD,
        text::BUTTON_REMOVE,
    ]
    .iter()
    .map(|s| width(s))
    .max()
    .unwrap_or(0);
    LayoutMeasures {
        dpi: look.dpi,
        line_height: look.line_height,
        title_height: font_height(look.title_font),
        widest_label,
        hotkey_text_width: width(WIDEST_HOTKEY_SAMPLE),
        days_width: width(text::DAYS_UNIT),
        numeric_text_width: width(&"0".repeat(NUMERIC_MAX_DIGITS)),
        button_text_width,
        // SAFETY: sem pré-condição.
        scrollbar_width: unsafe { GetSystemMetricsForDpi(SM_CXVSCROLL, look.dpi) },
        status_line: view.status_line.is_some(),
        restart_button: view.restart_visible,
        restart_text_width: width(text::BUTTON_RESTART_EXPLORER),
    }
}

/// Retângulo do campo de texto dentro da moldura desenhada.
fn field_inner(frame: CoreRect, line_height: i32, dpi: u32) -> CoreRect {
    let pad = scale_px(8, dpi);
    let top = frame.top + (frame.bottom - frame.top - line_height) / 2;
    CoreRect {
        left: frame.left + pad,
        top,
        right: frame.right - pad,
        bottom: top + line_height,
    }
}

fn list_inner(frame: CoreRect, dpi: u32) -> CoreRect {
    let pad = scale_px(3, dpi);
    CoreRect {
        left: frame.left + pad,
        top: frame.top + pad,
        right: frame.right - pad,
        bottom: frame.bottom - pad,
    }
}

/// # Safety
/// `parent` é a janela que recebe o controle, desta thread.
unsafe fn child(parent: HWND, class: PCWSTR, label: &str, style: WINDOW_STYLE, id: i32) -> HWND {
    let label = wide(label);
    // SAFETY: `GetModuleHandleW(None)` não tem pré-condição; `parent` válido
    // (contrato); `label` vive durante a chamada.
    unsafe {
        let hinstance = GetModuleHandleW(None).ok();
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            class,
            PCWSTR(label.as_ptr()),
            WS_CHILD | WS_VISIBLE | style,
            0,
            0,
            0,
            0,
            Some(parent),
            Some(HMENU(id as usize as *mut c_void)),
            hinstance.map(Into::into),
            None,
        )
        .unwrap_or_default()
    }
}

/// # Safety
/// Dentro do `WM_CREATE` de `hwnd`, com `ctx` o contexto dela.
unsafe fn build(hwnd: HWND, ctx: &SettingsCtx) {
    ctx.hwnd.set(hwnd);
    // SAFETY: `hwnd` válido (contrato).
    let dpi = match unsafe { GetDpiForWindow(hwnd) } {
        0 => system_appearance::DEFAULT_DPI,
        d => d,
    };
    let old = ctx.look.replace(Look::load(dpi));
    old.release();
    let dark = ctx.look.borrow().dark;
    controls::set_colors(ctx.look.borrow().colors);
    // SAFETY: `hwnd` é esta janela, desta thread.
    unsafe {
        let _ = window_material::apply_attributes(hwnd, dark);
    }

    let button = WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32);
    let numeric = WS_TABSTOP | WINDOW_STYLE((ES_NUMBER | ES_AUTOHSCROLL) as u32);
    // SAFETY: `hwnd` é o pai, dentro do próprio `WM_CREATE`; a área de
    // conteúdo é criada antes dos filhos dela, na ordem do Tab.
    let c = unsafe {
        let content = CreateWindowExW(
            WS_EX_CONTROLPARENT,
            content::CLASS_NAME,
            PCWSTR::null(),
            WS_CHILD | WS_VISIBLE | WS_VSCROLL | WS_CLIPCHILDREN,
            0,
            0,
            0,
            0,
            Some(hwnd),
            Some(HMENU(ID_CONTENT as usize as *mut c_void)),
            GetModuleHandleW(None).ok().map(Into::into),
            None,
        )
        .unwrap_or_default();
        let b = windows::core::w!("BUTTON");
        let e = windows::core::w!("EDIT");
        let t = controls::TOGGLE_CLASS;
        Controls {
            content,
            alter: child(content, b, text::BUTTON_ALTER, button, ID_ALTER),
            restart_explorer: child(
                content,
                b,
                text::BUTTON_RESTART_EXPLORER,
                button,
                ID_RESTART_EXPLORER,
            ),
            win_v: child(content, t, text::WIN_V.name, WS_TABSTOP, ID_WIN_V),
            startup: child(content, t, text::STARTUP.name, WS_TABSTOP, ID_STARTUP),
            retention: child(content, e, "", numeric, ID_RETENTION),
            items: child(content, e, "", numeric, ID_MAX_ITEMS),
            pinned: child(content, e, "", numeric, ID_MAX_PINNED),
            list: child(
                content,
                windows::core::w!("LISTBOX"),
                "",
                WS_TABSTOP | WS_VSCROLL | WINDOW_STYLE((LBS_NOTIFY | LBS_NOINTEGRALHEIGHT) as u32),
                ID_LIST,
            ),
            list_edit: child(
                content,
                e,
                "",
                WS_TABSTOP | WINDOW_STYLE(ES_AUTOHSCROLL as u32),
                ID_LIST_EDIT,
            ),
            add: child(content, b, text::BUTTON_ADD, button, ID_ADD),
            remove: child(content, b, text::BUTTON_REMOVE, button, ID_REMOVE),
            secrets: child(content, t, text::SECRETS.name, WS_TABSTOP, ID_SECRETS),
            protection: child(content, t, text::PROTECTION.name, WS_TABSTOP, ID_PROTECTION),
            save: child(hwnd, b, text::BUTTON_SAVE, button, ID_SAVE),
            cancel: child(hwnd, b, text::BUTTON_CANCEL, button, ID_CANCEL),
            apply: child(hwnd, b, text::BUTTON_APPLY, button, ID_APPLY),
        }
    };
    ctx.controls.set(c);

    for (edit, _) in c.numeric() {
        // SAFETY: `edit` acabou de ser criado.
        unsafe {
            SendMessageW(
                edit,
                EM_SETLIMITTEXT,
                Some(WPARAM(NUMERIC_MAX_DIGITS)),
                None,
            );
        }
    }
    // SAFETY: `c.alter` acabou de ser criado; `ctx` vive até o
    // `WM_NCDESTROY` da janela principal, que vem depois do de todos os
    // filhos (o subclass sai no `WM_NCDESTROY` do botão).
    unsafe {
        let _ = SetWindowSubclass(
            c.alter,
            Some(alter_subclass),
            SUBCLASS_HOTKEY_ID,
            ctx as *const SettingsCtx as usize,
        );
    }
    *ctx.tooltip.borrow_mut() = Tooltip::new(hwnd);

    apply_fonts(ctx);
    apply_control_themes(ctx);
    sync_controls(ctx);
    // A linha do atalho antes do layout: a linha de estado e o botão de
    // reiniciar mudam a altura.
    refresh_hotkey_line(ctx);
    relayout(ctx, Placement::Center);
    refresh_apply(ctx);
}

fn apply_fonts(ctx: &SettingsCtx) {
    let font = ctx.look.borrow().font;
    for h in ctx.controls.get().all() {
        // SAFETY: `h` é filho desta janela; a fonte vive até o próximo
        // `Look::release`, que só acontece depois de outra fonte entregue.
        unsafe {
            SendMessageW(
                h,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }
}

/// Barra de rolagem e lista no tema do sistema. Sem os controles comuns v6
/// no manifesto, a barra pode continuar clara.
fn apply_control_themes(ctx: &SettingsCtx) {
    let c = ctx.controls.get();
    let theme = if ctx.look.borrow().dark {
        windows::core::w!("DarkMode_Explorer")
    } else {
        windows::core::w!("Explorer")
    };
    for h in [
        c.content,
        c.list,
        c.list_edit,
        c.retention,
        c.items,
        c.pinned,
    ] {
        // SAFETY: `h` é desta janela; falhar só deixa o visual padrão.
        unsafe {
            let _ = SetWindowTheme(h, theme, PCWSTR::null());
        }
    }
}

/// Monitor e moldura da janela no DPI atual.
pub(crate) fn work_area(hwnd: HWND) -> RECT {
    // SAFETY: `hwnd` válido; `MONITOR_DEFAULTTONEAREST` sempre devolve um
    // monitor; a chamada só escreve `mi`.
    unsafe {
        let hmon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: core::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        if GetMonitorInfoW(hmon, &mut mi).as_bool() {
            mi.rcWork
        } else {
            RECT {
                left: 0,
                top: 0,
                right: 1024,
                bottom: 768,
            }
        }
    }
}

fn frame_size(dpi: u32) -> (i32, i32) {
    let mut r = RECT::default();
    // SAFETY: só calcula sobre o `RECT` local.
    let _ =
        unsafe { AdjustWindowRectExForDpi(&mut r, main_style(), false, WS_EX_CONTROLPARENT, dpi) };
    (r.right - r.left, r.bottom - r.top)
}

fn relayout(ctx: &SettingsCtx, placement: Placement) {
    let hwnd = ctx.hwnd.get();
    let (m, dpi) = {
        let look = ctx.look.borrow();
        (measures(&look, &ctx.hotkey_view.borrow()), look.dpi)
    };
    let work = work_area(hwnd);
    let (fw, fh) = frame_size(dpi);
    let layout = settings_layout(&m, work.bottom - work.top - fh);
    ctx.layout.set(layout);
    let (w, h) = (layout.client_width + fw, layout.client_height + fh);
    let (x, y) = match placement {
        Placement::Center => (
            work.left + (work.right - work.left - w) / 2,
            work.top + (work.bottom - work.top - h) / 2,
        ),
        Placement::At(x, y) => (x, y),
    };
    let x = x.min(work.right - w).max(work.left);
    let y = y.min(work.bottom - h).max(work.top);
    // SAFETY: `hwnd` é esta janela; só posição e tamanho mudam.
    unsafe {
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
    }
    let c = ctx.controls.get();
    let v = layout.viewport;
    // SAFETY: `c.content` é filha desta janela.
    unsafe {
        let _ = SetWindowPos(
            c.content,
            None,
            v.left,
            v.top,
            v.right - v.left,
            v.bottom - v.top,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
    let max = duplicata_core::settings_layout::max_scroll(v.bottom - v.top, layout.content_height);
    ctx.scroll.set(ctx.scroll.get().min(max));
    place_children(ctx);
    content::update_scrollbar(ctx);
    redraw_all(ctx);
}

/// Posiciona os controles da área de conteúdo (descontada a rolagem) e os
/// do rodapé.
fn place_children(ctx: &SettingsCtx) {
    let l = ctx.layout.get();
    let c = ctx.controls.get();
    let (line, dpi) = {
        let look = ctx.look.borrow();
        (look.line_height, look.dpi)
    };
    let s = ctx.scroll.get();
    let shift = |r: CoreRect| CoreRect {
        top: r.top - s,
        bottom: r.bottom - s,
        ..r
    };
    let place = |h: HWND, r: CoreRect| {
        // SAFETY: `h` é filho desta janela, desta thread.
        unsafe {
            let _ = SetWindowPos(
                h,
                None,
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOZORDER | SWP_NOACTIVATE,
            );
        }
    };
    place(c.alter, shift(l.alter_button));
    place(c.startup, shift(l.toggles[0]));
    place(c.secrets, shift(l.toggles[1]));
    place(c.protection, shift(l.toggles[2]));
    place(c.win_v, shift(l.toggles[3]));
    place(c.restart_explorer, shift(l.restart_button));
    for (i, (edit, _)) in c.numeric().into_iter().enumerate() {
        place(edit, shift(field_inner(l.numeric_edits[i], line, dpi)));
    }
    place(c.list, shift(list_inner(l.list, dpi)));
    place(c.list_edit, shift(field_inner(l.list_edit, line, dpi)));
    place(c.add, shift(l.add_button));
    place(c.remove, shift(l.remove_button));
    place(c.save, l.footer[0]);
    place(c.cancel, l.footer[1]);
    place(c.apply, l.footer[2]);
}

fn redraw_all(ctx: &SettingsCtx) {
    // SAFETY: `hwnd` é esta janela; invalida ela e todos os filhos.
    unsafe {
        let _ = RedrawWindow(
            Some(ctx.hwnd.get()),
            None,
            None,
            RDW_INVALIDATE | RDW_ERASE | RDW_ALLCHILDREN,
        );
    }
}

fn reload_look(ctx: &SettingsCtx, dpi: u32) {
    hide_tooltip(ctx);
    let old = ctx.look.replace(Look::load(dpi));
    controls::set_colors(ctx.look.borrow().colors);
    let dark = ctx.look.borrow().dark;
    // SAFETY: `hwnd` é esta janela, desta thread.
    unsafe {
        let _ = window_material::apply_attributes(ctx.hwnd.get(), dark);
    }
    apply_fonts(ctx);
    old.release();
    apply_control_themes(ctx);
}

/// # Safety
/// `edit` deve ser um controle válido.
unsafe fn window_text(edit: HWND) -> String {
    // SAFETY: `edit` válido (contrato).
    let len = unsafe { SendMessageW(edit, WM_GETTEXTLENGTH, None, None) }.0 as usize;
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` tem espaço para o NUL que `WM_GETTEXT` escreve.
    unsafe {
        SendMessageW(
            edit,
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        );
    }
    String::from_utf16_lossy(&buf[..len])
}

/// Reescreve um campo sem que o `EN_CHANGE` conte como edição.
fn write_field(ctx: &SettingsCtx, edit: HWND, value: &str) {
    // SAFETY: `edit` é filho desta janela.
    if unsafe { window_text(edit) } == value {
        return;
    }
    let w = wide(value);
    ctx.writing_fields.set(true);
    // SAFETY: `edit` válido; `w` NUL-terminado e vivo durante a chamada.
    unsafe {
        SendMessageW(edit, WM_SETTEXT, None, Some(LPARAM(w.as_ptr() as isize)));
    }
    ctx.writing_fields.set(false);
}

fn set_check(h: HWND, on: bool) {
    // SAFETY: `h` é uma alternância desta janela.
    unsafe {
        SendMessageW(h, BM_SETCHECK, Some(WPARAM(on as usize)), None);
    }
}

fn is_checked(h: HWND) -> bool {
    // SAFETY: `h` é uma alternância desta janela.
    unsafe { SendMessageW(h, BM_GETCHECK, None, None) }.0 != 0
}

fn sync_controls(ctx: &SettingsCtx) {
    sync_toggles(ctx);
    sync_fields(ctx);
    sync_list(ctx);
}

fn sync_toggles(ctx: &SettingsCtx) {
    let c = ctx.controls.get();
    let (startup, secrets, protection, busy, win_v) = {
        let s = ctx.session.borrow();
        let d = s.draft();
        (
            d.startup,
            d.secret_detection,
            d.protection,
            s.protection_busy(),
            d.win_v,
        )
    };
    set_check(c.win_v, win_v);
    set_check(c.startup, startup);
    set_check(c.secrets, secrets);
    set_check(c.protection, protection);
    // SAFETY: `c.protection` é desta janela.
    unsafe {
        let _ = EnableWindow(c.protection, !busy);
    }
    invalidate_content(ctx);
}

fn sync_fields(ctx: &SettingsCtx) {
    let c = ctx.controls.get();
    for (edit, field) in c.numeric() {
        let value = ctx.session.borrow().numeric_text(field).to_string();
        write_field(ctx, edit, &value);
    }
}

fn sync_list(ctx: &SettingsCtx) {
    let list = ctx.controls.get().list;
    let names = ctx.session.borrow().draft().blocked_programs.clone();
    // SAFETY: `list` é a LISTBOX desta janela; cada `w` vive durante a
    // chamada síncrona.
    unsafe {
        SendMessageW(list, LB_RESETCONTENT, None, None);
        for name in names {
            let w = wide(&name);
            SendMessageW(list, LB_ADDSTRING, None, Some(LPARAM(w.as_ptr() as isize)));
        }
    }
}

fn refresh_apply(ctx: &SettingsCtx) {
    let c = ctx.controls.get();
    let can = ctx.session.borrow().can_apply();
    // SAFETY: `c.apply`/`c.save` são desta janela; o foco não fica num botão
    // desabilitado.
    unsafe {
        if !can && GetFocus() == c.apply {
            let _ = SetFocus(Some(c.save));
        }
        let _ = EnableWindow(c.apply, can);
    }
}

fn refresh_hotkey_line(ctx: &SettingsCtx) {
    let owner = hotkey_capture::owner();
    let armed = hotkey_capture::is_armed() && owner == CaptureOwner::SettingsSection;
    let view = ctx.session.borrow().hotkey_line(
        ctx.registered_hotkey.get(),
        armed,
        owner == CaptureOwner::StandaloneDialog,
    );
    let c = ctx.controls.get();
    // SAFETY: `alter` e o botão de reiniciar são desta janela; o foco não
    // fica num botão escondido ou desabilitado.
    unsafe {
        let _ = EnableWindow(c.alter, view.alter_enabled);
        if !view.restart_enabled && GetFocus() == c.restart_explorer {
            let _ = SetFocus(Some(c.alter));
        }
        let _ = EnableWindow(c.restart_explorer, view.restart_enabled);
        let _ = ShowWindow(
            c.restart_explorer,
            if view.restart_visible {
                SW_SHOW
            } else {
                SW_HIDE
            },
        );
    }
    let shape = |v: &HotkeyLineView| (v.status_line.is_some(), v.restart_visible);
    let reshaped = shape(&view) != shape(&ctx.hotkey_view.borrow());
    *ctx.hotkey_view.borrow_mut() = view;
    if reshaped && ctx.layout.get().client_width > 0 {
        let mut r = RECT::default();
        // SAFETY: `hwnd` é esta janela; só lê a posição.
        let _ = unsafe { GetWindowRect(ctx.hwnd.get(), &mut r) };
        relayout(ctx, Placement::At(r.left, r.top));
    }
    invalidate_content(ctx);
}

fn invalidate_content(ctx: &SettingsCtx) {
    // SAFETY: `content` é desta janela (ou nula antes de criada: no-op).
    unsafe {
        let _ = InvalidateRect(Some(ctx.controls.get().content), None, false);
    }
}

fn hide_tooltip(ctx: &SettingsCtx) {
    if let Some(t) = ctx.tooltip.borrow().as_ref() {
        t.hide();
    }
}

/// Rola até o controle com o foco, se ele estiver na área de conteúdo.
fn reveal_focus(ctx: &SettingsCtx) {
    let c = ctx.controls.get();
    // SAFETY: só consulta o foco desta thread.
    let focus = unsafe { GetFocus() };
    if focus == ctx.last_focus.get() {
        return;
    }
    ctx.last_focus.set(focus);
    // SAFETY: `IsChild` só consulta.
    if focus.is_invalid() || !unsafe { IsChild(c.content, focus) }.as_bool() {
        return;
    }
    content::reveal(ctx, focus);
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Executa os efeitos na ordem. Empréstimos curtos: nenhum atravessa um
/// `SendMessageW`. Na falha da troca do atalho, a direção é a do núcleo: no
/// Aplicar/Salvar a anterior continua; no Cancelar nenhuma fica (como na
/// inicialização quando o registro falha).
fn execute(ctx: &SettingsCtx, effects: &[Effect], mode: Mode) -> SaveReport {
    let mut report = SaveReport::default();
    let mut push_settings = false;
    // Ligado neste Salvar: se o `config.toml` falhar, o `V` não pode ficar
    // sem a anotação de que é do duplicata.
    let mut enabled = None;
    for effect in effects {
        match effect {
            Effect::SwapHotkey { to, .. } => {
                let on_failure = if mode == Mode::Cancel {
                    OnSwapFailure::LeaveNone
                } else {
                    OnSwapFailure::KeepPrevious
                };
                let registered = ctx.registered_hotkey.get();
                match hotkey_dialog::swap_hotkey(ctx.history_hwnd, registered, *to, on_failure) {
                    Ok(()) => {
                        ctx.registered_hotkey.set(Some(*to));
                        ctx.config.borrow_mut().hotkey = *to;
                    }
                    Err(_) if mode == Mode::Cancel => {
                        ctx.registered_hotkey.set(None);
                        ctx.config.borrow_mut().hotkey = *to;
                    }
                    Err(_) => {
                        report.hotkey_failed = true;
                        if mode == Mode::Apply {
                            ctx.session.borrow_mut().hotkey_effect_failed();
                        }
                    }
                }
            }
            Effect::SetStartup {
                enable,
                clear_block,
            } => {
                // Falha no registro fica no log; a janela segue (como antes).
                if *enable {
                    let written = std::env::current_exe()
                        .ok()
                        .and_then(|p| p.to_str().map(str::to_string))
                        .is_some_and(|path| startup_registry::write_run_entry(&path));
                    if !written {
                        log_event!(Level::WARN, LogFields::new("startup_entry_write_failed"));
                    }
                    if *clear_block && !startup_registry::clear_startup_approval() {
                        log_event!(Level::WARN, LogFields::new("startup_approval_clear_failed"));
                    }
                } else if !startup_registry::remove_run_entry() {
                    log_event!(Level::WARN, LogFields::new("startup_entry_remove_failed"));
                }
            }
            Effect::SetCaptureFilters {
                blocked_programs,
                secret_detection,
            } => {
                {
                    let mut cfg = ctx.config.borrow_mut();
                    cfg.blocked_programs = blocked_programs.clone();
                    cfg.heuristic_secret_detection = *secret_detection;
                }
                ctx.queue.push(
                    WorkItem::SetSecretDetection {
                        enabled: *secret_detection,
                    },
                    0,
                );
            }
            Effect::SetRetention { days, max_items } => {
                let mut cfg = ctx.config.borrow_mut();
                cfg.retention = std::time::Duration::from_secs(u64::from(*days) * 86_400);
                cfg.max_items = *max_items;
                push_settings = true;
            }
            Effect::SetMaxPinned(n) => {
                ctx.config.borrow_mut().max_pinned = *n;
                // SAFETY: Histórico desta thread; entrega síncrona.
                unsafe {
                    SendMessageW(
                        ctx.history_hwnd,
                        WM_APP_SET_MAX_PINNED,
                        Some(WPARAM(*n as usize)),
                        None,
                    );
                }
                push_settings = true;
            }
            Effect::ToggleProtection(on) => {
                ctx.config.borrow_mut().encryption_enabled = *on;
                // SAFETY: Histórico desta thread; a reescrita segue pela
                // worker e o fim chega por `on_protection_finished`.
                unsafe {
                    SendMessageW(
                        ctx.history_hwnd,
                        WM_APP_TOGGLE_ENCRYPTION,
                        Some(WPARAM(*on as usize)),
                        None,
                    );
                }
            }
            Effect::SetReserve(to) => {
                // O Win+V está em uso: a reserva só muda na configuração.
                ctx.config.borrow_mut().hotkey = *to;
            }
            Effect::EnableWinV => match ctx.win_v.enable() {
                Ok(switched) => enabled = Some(switched),
                Err(_) => {
                    log_event!(Level::WARN, LogFields::new("win_v_enable_failed"));
                    report.win_v_failed = true;
                }
            },
            Effect::DisableWinV => {
                if ctx.win_v.disable().is_err() {
                    log_event!(Level::WARN, LogFields::new("win_v_disable_failed"));
                    report.win_v_failed = true;
                }
            }
            Effect::Persist => {
                ctx.session
                    .borrow()
                    .in_effect()
                    .write_into(&mut ctx.config.borrow_mut());
                report.persist_failed = ctx.config.borrow().persist(&ctx.config_path).is_err();
                if let (true, Some(switched)) = (report.persist_failed, enabled) {
                    // Só os campos do Win+V, ou a ligação desfeita; a tela
                    // mostra só a falha do Salvar.
                    report.win_v_failed = ctx.win_v.undo_unsaved_enable(switched);
                }
            }
        }
    }
    if push_settings {
        let cfg = ctx.config.borrow();
        ctx.queue.push(
            WorkItem::ApplySettings {
                now_ms: now_ms(),
                retention_ms: cfg.retention.as_millis() as u64,
                max_items: cfg.max_items,
                max_pinned: cfg.max_pinned,
            },
            0,
        );
    }
    report
}

fn on_apply(ctx: &SettingsCtx) {
    if !ctx.session.borrow().can_apply() {
        return;
    }
    let effects = ctx.session.borrow_mut().apply();
    execute(ctx, &effects, Mode::Apply);
    sync_fields(ctx);
    refresh_hotkey_line(ctx);
    refresh_apply(ctx);
}

fn cancel(hwnd: HWND, ctx: &SettingsCtx) {
    let effects = ctx.session.borrow_mut().cancel();
    execute(ctx, &effects, Mode::Cancel);
    // SAFETY: `hwnd` é esta janela. Depois desta linha `ctx` não é mais
    // tocado: o `WM_NCDESTROY` solta o contexto dentro do `DestroyWindow`.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn on_save(hwnd: HWND, ctx: &SettingsCtx) {
    let step = ctx.session.borrow_mut().begin_save();
    // Os campos mostram o ajustado; o Aplicar segue o que sobrou pendente.
    sync_fields(ctx);
    refresh_apply(ctx);
    match step {
        SaveStep::Proceed(effects) => finish_save(hwnd, ctx, &effects),
        SaveStep::Confirm(lines) => open_confirm(hwnd, ctx, lines, ConfirmKind::Save),
    }
}

fn open_confirm(hwnd: HWND, ctx: &SettingsCtx, lines: Vec<String>, kind: ConfirmKind) {
    let (font, dpi) = {
        let look = ctx.look.borrow();
        (look.font, look.dpi)
    };
    let primary = match kind {
        ConfirmKind::Save => text::BUTTON_SAVE,
        ConfirmKind::RestartExplorer => text::BUTTON_RESTART,
    };
    hide_tooltip(ctx);
    if let Some(confirm) = confirm::open(hwnd, lines, font, dpi, primary) {
        ctx.confirm.set(Some(confirm));
        ctx.confirm_kind.set(kind);
        // SAFETY: `hwnd` é esta janela; a confirmação reabilita antes de se
        // destruir.
        unsafe {
            let _ = EnableWindow(hwnd, false);
        }
    }
}

fn on_restart_explorer(hwnd: HWND, ctx: &SettingsCtx) {
    let lines = ctx.session.borrow().begin_explorer_restart();
    if let Some(lines) = lines {
        open_confirm(hwnd, ctx, lines, ConfirmKind::RestartExplorer);
    }
}

/// "Reiniciar" confirmado: a thread do reinício avisa a bandeja no fim.
fn start_explorer_restart(ctx: &SettingsCtx) {
    // O Explorer pode ter voltado por outro caminho com a confirmação
    // aberta: aí o botão já não vale e nada é reiniciado.
    sync_win_v(ctx);
    if ctx.session.borrow().begin_explorer_restart().is_none() {
        refresh_hotkey_line(ctx);
        return;
    }
    let Some(notify) = tray::window() else {
        ctx.error_line
            .set(restart_error_line(ExplorerRestartOutcome::Refused));
        redraw_all(ctx);
        return;
    };
    ctx.session.borrow_mut().explorer_restart_started();
    let returned = explorer_restart::spawn(notify, tray::WM_EXPLORER_RESTART_DONE);
    ctx.win_v.restart_started(returned);
    ctx.error_line.set(None);
    refresh_hotkey_line(ctx);
    redraw_all(ctx);
}

fn finish_save(hwnd: HWND, ctx: &SettingsCtx, effects: &[Effect]) {
    let report = execute(ctx, effects, Mode::Save);
    sync_win_v(ctx);
    let outcome = ctx.session.borrow_mut().finish_save(report);
    match outcome {
        SaveOutcome::Close => {}
        SaveOutcome::StayOpen { error_line } => {
            ctx.error_line.set(error_line);
            sync_toggles(ctx);
            refresh_hotkey_line(ctx);
            refresh_apply(ctx);
            redraw_all(ctx);
            return;
        }
    }
    // SAFETY: `hwnd` é esta janela; `ctx` não é tocado depois.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

fn on_confirm_result(hwnd: HWND, ctx: &SettingsCtx, confirmed: bool) {
    ctx.confirm.set(None);
    // SAFETY: `hwnd` é esta janela (a confirmação já a reabilitou).
    unsafe {
        let _ = EnableWindow(hwnd, true);
        let _ = SetForegroundWindow(hwnd);
    }
    let kind = ctx.confirm_kind.get();
    match (kind, confirmed) {
        (ConfirmKind::Save, true) => {
            let effects = ctx.session.borrow_mut().confirm_save();
            finish_save(hwnd, ctx, &effects);
        }
        (ConfirmKind::Save, false) => {
            ctx.session.borrow_mut().back();
            refresh_apply(ctx);
            // SAFETY: `hwnd` é esta janela, de volta ao primeiro plano.
            unsafe {
                let _ = SetFocus(Some(ctx.controls.get().save));
            }
        }
        (ConfirmKind::RestartExplorer, true) => start_explorer_restart(ctx),
        // "Voltar": nada muda.
        (ConfirmKind::RestartExplorer, false) => {
            // SAFETY: `hwnd` é esta janela, de volta ao primeiro plano.
            unsafe {
                let _ = SetFocus(Some(ctx.controls.get().restart_explorer));
            }
        }
    }
}

fn on_numeric_change(ctx: &SettingsCtx, edit: HWND, field: NumericField) {
    if ctx.writing_fields.get() {
        return;
    }
    // SAFETY: `edit` é um campo desta janela.
    let typed = unsafe { window_text(edit) };
    let digits = duplicata_core::settings_input::digits_only(&typed);
    if digits != typed {
        // Colagem com letras: o campo fica só com os dígitos.
        write_field(ctx, edit, &digits);
        let end = digits.encode_utf16().count();
        // SAFETY: `edit` válido; cursor no fim do texto.
        unsafe {
            SendMessageW(
                edit,
                EM_SETSEL,
                Some(WPARAM(end)),
                Some(LPARAM(end as isize)),
            );
        }
    }
    ctx.session.borrow_mut().set_numeric_text(field, &digits);
    refresh_apply(ctx);
}

fn on_add(ctx: &SettingsCtx) {
    let c = ctx.controls.get();
    // SAFETY: `c.list_edit` é desta janela.
    let typed = unsafe { window_text(c.list_edit) };
    if ctx.session.borrow_mut().add_program(&typed) {
        write_field(ctx, c.list_edit, "");
        sync_list(ctx);
        refresh_apply(ctx);
    }
}

fn on_remove(ctx: &SettingsCtx) {
    let list = ctx.controls.get().list;
    // SAFETY: `list` é a LISTBOX desta janela; -1 = nada selecionado.
    let index = unsafe { SendMessageW(list, LB_GETCURSEL, None, None) }.0;
    let selected = usize::try_from(index).ok();
    if ctx.session.borrow_mut().remove_program(selected) {
        sync_list(ctx);
        refresh_apply(ctx);
    }
}

fn on_alter(ctx: &SettingsCtx) {
    if hotkey_capture::owner() == CaptureOwner::StandaloneDialog {
        return;
    }
    hotkey_capture::arm(CaptureOwner::SettingsSection);
    ctx.session.borrow_mut().hotkey_armed();
    // SAFETY: `alter` é desta janela; o foco fica nele enquanto armado.
    unsafe {
        let _ = SetFocus(Some(ctx.controls.get().alter));
    }
    refresh_hotkey_line(ctx);
}

fn settings_armed() -> bool {
    hotkey_capture::is_armed() && hotkey_capture::owner() == CaptureOwner::SettingsSection
}

fn disarm(ctx: &SettingsCtx) {
    hotkey_capture::disarm();
    ctx.session.borrow_mut().hotkey_disarmed();
    refresh_hotkey_line(ctx);
}

fn on_hotkey_key(ctx: &SettingsCtx, wparam: WPARAM) {
    let vk = VIRTUAL_KEY(wparam.0 as u16);
    if matches!(vk, VK_CONTROL | VK_SHIFT | VK_MENU | VK_LWIN | VK_RWIN) {
        return;
    }
    if vk == VK_ESCAPE {
        disarm(ctx);
        return;
    }
    let candidate = HotkeyCombo {
        modifiers: hotkey_dialog::current_modifiers(),
        vkey: vk.0 as u32,
    };
    let registered = ctx.registered_hotkey.get();
    let pending = ctx.session.borrow().has_pending_hotkey();
    let outcome = if pending && registered == Some(candidate) {
        // Voltar à combinação que já vale desfaz a pendente.
        Ok(())
    } else {
        hotkey_dialog::probe_hotkey(ctx.hwnd.get(), registered, candidate)
    };
    match outcome {
        Ok(()) => {
            ctx.session.borrow_mut().set_pending_hotkey(candidate);
            hotkey_capture::disarm();
            refresh_hotkey_line(ctx);
            refresh_apply(ctx);
        }
        Err(message) => {
            ctx.session.borrow_mut().set_hotkey_message(message);
            refresh_hotkey_line(ctx);
        }
    }
}

unsafe extern "system" fn alter_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    uidsubclass: usize,
    dwrefdata: usize,
) -> LRESULT {
    // SAFETY: `dwrefdata` é o `SettingsCtx` posto em `build`, vivo enquanto
    // o botão existir (ver `build`).
    let ctx = unsafe { (dwrefdata as *const SettingsCtx).as_ref() };
    match (msg, ctx) {
        (WM_NCDESTROY, _) => {
            // SAFETY: `hwnd`/`uidsubclass` identificam este subclass.
            unsafe {
                let _ = RemoveWindowSubclass(hwnd, Some(alter_subclass), uidsubclass);
            }
        }
        (WM_GETDLGCODE, Some(_)) if settings_armed() => {
            // Armado, todas as teclas vêm para cá: Tab e Esc também.
            return LRESULT(windows::Win32::UI::WindowsAndMessaging::DLGC_WANTALLKEYS as isize);
        }
        (WM_KEYDOWN | WM_SYSKEYDOWN, Some(ctx)) if settings_armed() => {
            on_hotkey_key(ctx, wparam);
            return LRESULT(0);
        }
        (WM_KILLFOCUS, Some(ctx)) if settings_armed() => {
            disarm(ctx);
        }
        _ => {}
    }
    // SAFETY: repassa ao handler padrão do subclass.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn on_command(hwnd: HWND, ctx: &SettingsCtx, wparam: WPARAM) {
    let id = (wparam.0 & 0xFFFF) as i32;
    let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
    let c = ctx.controls.get();
    if code == EN_CHANGE {
        let [retention, items, pinned] = c.numeric();
        let target = match id {
            ID_RETENTION => Some(retention),
            ID_MAX_ITEMS => Some(items),
            ID_MAX_PINNED => Some(pinned),
            _ => None,
        };
        if let Some((edit, field)) = target {
            on_numeric_change(ctx, edit, field);
        }
        return;
    }
    if code != BN_CLICKED || wparam.0 & 0xFFFF == IDOK {
        return;
    }
    match id {
        ID_ALTER => on_alter(ctx),
        ID_RESTART_EXPLORER => on_restart_explorer(hwnd, ctx),
        ID_WIN_V => {
            // Só no Salvar: não habilita o Aplicar.
            ctx.session.borrow_mut().set_win_v(is_checked(c.win_v));
            refresh_apply(ctx);
        }
        ID_STARTUP => {
            ctx.session.borrow_mut().set_startup(is_checked(c.startup));
            refresh_apply(ctx);
        }
        ID_SECRETS => {
            ctx.session
                .borrow_mut()
                .set_secret_detection(is_checked(c.secrets));
            refresh_apply(ctx);
        }
        ID_PROTECTION => {
            ctx.session
                .borrow_mut()
                .set_protection(is_checked(c.protection));
            sync_toggles(ctx);
            refresh_apply(ctx);
        }
        ID_ADD => on_add(ctx),
        ID_REMOVE => on_remove(ctx),
        ID_APPLY => on_apply(ctx),
        ID_SAVE => on_save(hwnd, ctx),
        ID_CANCEL => cancel(hwnd, ctx),
        _ => {}
    }
}

fn paint(hwnd: HWND, ctx: &SettingsCtx) {
    let l = ctx.layout.get();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` no `WM_PAINT`; o pincel do tema
    // vive no `Look`; a fonte anterior volta antes do `EndPaint`.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let look = ctx.look.borrow();
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        FillRect(hdc, &client, look.surface_brush);
        let line = RECT {
            left: 0,
            top: l.viewport.bottom,
            right: client.right,
            bottom: l.viewport.bottom + 1,
        };
        fill(hdc, &line, look.palette.border);
        if let Some(message) = ctx.error_line.get() {
            let old = SelectObject(hdc, HGDIOBJ(look.font.0));
            SetBkMode(hdc, TRANSPARENT);
            SetTextColor(hdc, look.palette.text_primary);
            let mut buf: Vec<u16> = message.encode_utf16().collect();
            let mut rc = win_rect(l.error_line);
            DrawTextW(
                hdc,
                &mut buf,
                &mut rc,
                DT_WORDBREAK | DT_NOPREFIX | DT_END_ELLIPSIS,
            );
            SelectObject(hdc, old);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

/// # Safety
/// `hdc` válido.
unsafe fn fill(hdc: HDC, r: &RECT, color: COLORREF) {
    // SAFETY: pincel criado e apagado aqui; `hdc` válido (contrato).
    unsafe {
        let brush = CreateSolidBrush(color);
        FillRect(hdc, r, brush);
        let _ = DeleteObject(HGDIOBJ(brush.0));
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o `*mut SettingsCtx` de `open()`.
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            if let Some(ctx) = ctx_of(hwnd) {
                // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
                unsafe { build(hwnd, ctx) };
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(ctx) = ctx_of(hwnd) {
                paint(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            if let Some(ctx) = ctx_of(hwnd) {
                on_command(hwnd, ctx, wparam);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            if let Some(ctx) = ctx_of(hwnd) {
                let font = ctx.look.borrow().font;
                // SAFETY: num `WM_DRAWITEM`, `lParam` aponta para o
                // `DRAWITEMSTRUCT` do botão, válido durante o handler.
                unsafe { controls::draw_button(&*(lparam.0 as *const DRAWITEMSTRUCT), font) };
            }
            LRESULT(1)
        }
        WM_MOUSEWHEEL => {
            if let Some(ctx) = ctx_of(hwnd) {
                content::wheel(ctx, wparam);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if let Some(ctx) = ctx_of(hwnd) {
                if (wparam.0 & 0xFFFF) as u32 == WA_INACTIVE {
                    hide_tooltip(ctx);
                    // SAFETY: só consulta o foco desta thread.
                    let focus = unsafe { GetFocus() };
                    if !focus.is_invalid() {
                        ctx.last_focus.set(focus);
                    }
                } else {
                    let target = ctx.last_focus.get();
                    // SAFETY: `IsChild` só consulta; o foco volta ao último
                    // controle desta janela, ou ao primeiro.
                    unsafe {
                        let target = if IsChild(hwnd, target).as_bool() {
                            target
                        } else {
                            ctx.controls.get().alter
                        };
                        let _ = SetFocus(Some(target));
                    }
                }
            }
            LRESULT(0)
        }
        WM_THEMECHANGED => {
            if let Some(ctx) = ctx_of(hwnd) {
                let dpi = ctx.look.borrow().dpi;
                reload_look(ctx, dpi);
                redraw_all(ctx);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if system_appearance::is_immersive_color_set(lparam) {
                if let Some(ctx) = ctx_of(hwnd) {
                    let dpi = ctx.look.borrow().dpi;
                    reload_look(ctx, dpi);
                    redraw_all(ctx);
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(ctx) = ctx_of(hwnd) {
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
                reload_look(ctx, if dpi == 0 { ctx.look.borrow().dpi } else { dpi });
                relayout(ctx, Placement::At(at.0, at.1));
            }
            LRESULT(0)
        }
        WM_APP_CONFIRM_RESULT => {
            if let Some(ctx) = ctx_of(hwnd) {
                on_confirm_result(hwnd, ctx, wparam.0 != 0);
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(ctx) = ctx_of(hwnd) {
                cancel(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            hotkey_capture::release(CaptureOwner::SettingsSection);
            OPEN_HWND.with(|c| c.set(None));
            // SAFETY: reconstrói e solta o `Box` de `open()`; zera o
            // `GWLP_USERDATA` para que nada mais o leia.
            let ptr = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut SettingsCtx;
            if !ptr.is_null() {
                // SAFETY: `ptr` veio de `Box::into_raw` em `open()` e nunca
                // foi liberado antes.
                let ctx = unsafe { Box::from_raw(ptr) };
                // A dica antes das fontes que ela empresta.
                drop(ctx.tooltip.borrow_mut().take());
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

#[doc(hidden)]
pub fn window() -> Option<HWND> {
    open_window().map(|(hwnd, _)| hwnd)
}

/// Controle pelo id, na área de conteúdo ou no rodapé.
#[doc(hidden)]
pub fn control(id: i32) -> Option<HWND> {
    let (hwnd, ctx) = open_window()?;
    let content = ctx.controls.get().content;
    // SAFETY: `GetDlgItem` só consulta a árvore de filhos.
    unsafe {
        GetDlgItem(Some(content), id)
            .or_else(|_| GetDlgItem(Some(hwnd), id))
            .ok()
    }
}

/// Repinta agora e devolve os textos que a janela desenhou (títulos, nomes,
/// descrições, combinação, unidade e a linha de erro).
#[doc(hidden)]
pub fn painted_texts() -> Vec<String> {
    let Some((hwnd, ctx)) = open_window() else {
        return Vec::new();
    };
    // SAFETY: `hwnd` é a janela viva; pinta de forma síncrona.
    unsafe {
        let _ = RedrawWindow(
            Some(hwnd),
            None,
            None,
            RDW_INVALIDATE | RDW_UPDATENOW | RDW_ALLCHILDREN,
        );
    }
    let mut out = ctx.painted.borrow().clone();
    if let Some(e) = ctx.error_line.get() {
        out.push(e.to_string());
    }
    out
}

#[doc(hidden)]
pub fn tooltip_text() -> Option<String> {
    let (_, ctx) = open_window()?;
    let tip = ctx.tooltip.borrow();
    let tip = tip.as_ref()?;
    tip.is_visible().then(|| tip.text())
}

/// Ponto no meio da linha `row`, nas coordenadas da área de conteúdo.
#[doc(hidden)]
pub fn row_point(row: usize) -> Option<POINT> {
    let (_, ctx) = open_window()?;
    let r = ctx.layout.get().rows.get(row)?.row;
    Some(POINT {
        x: (r.left + r.right) / 2,
        y: (r.top + r.bottom) / 2 - ctx.scroll.get(),
    })
}

#[doc(hidden)]
pub fn is_dark() -> Option<bool> {
    open_window().map(|(_, ctx)| ctx.look.borrow().dark)
}

#[doc(hidden)]
pub fn current_dpi() -> Option<u32> {
    open_window().map(|(_, ctx)| ctx.look.borrow().dpi)
}

#[doc(hidden)]
pub fn scroll_position() -> Option<i32> {
    open_window().map(|(_, ctx)| ctx.scroll.get())
}

#[doc(hidden)]
pub fn hotkey_description() -> Option<String> {
    open_window().map(|(_, ctx)| ctx.hotkey_view.borrow().description.clone())
}

#[doc(hidden)]
pub fn win_v_status_line() -> Option<String> {
    open_window().and_then(|(_, ctx)| ctx.hotkey_view.borrow().status_line.clone())
}

#[doc(hidden)]
pub fn error_line() -> Option<&'static str> {
    open_window().and_then(|(_, ctx)| ctx.error_line.get())
}

#[doc(hidden)]
pub fn confirm_window() -> Option<HWND> {
    open_window().and_then(|(_, ctx)| ctx.confirm.get())
}

#[doc(hidden)]
pub fn confirm_lines() -> Vec<String> {
    confirm_window().map(confirm::lines).unwrap_or_default()
}

/// Fonte que a confirmação aberta usa para desenhar.
#[doc(hidden)]
pub fn confirm_font() -> Option<HFONT> {
    confirm_window().and_then(confirm::font)
}
