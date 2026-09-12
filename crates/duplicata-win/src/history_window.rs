use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use duplicata_core::config::DEFAULT_MAX_PINNED;
use duplicata_core::filter_strip::{
    self, adjacent_segment, segment_at_packed, segment_rects_packed, Rect as CoreRect,
};
use duplicata_core::hint_strip;
use duplicata_core::list_view::{self, ContentTypeFilter};
use duplicata_core::row_layout;
use duplicata_core::translucency;
use duplicata_core::{
    log_event, should_suppress_reopen, CaptureQueue, ClipListItem, Clock, HistoryReader, InitError,
    LogFields, SelfWriteFilter, SetPinnedOutcome, StoreError, WorkItem,
};
use tracing::Level;
use windows::core::HSTRING;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateFontIndirectW, CreateSolidBrush, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, FillRect, InvalidateRect, SelectObject, SetBkMode, SetBrushOrgEx,
    SetStretchBltMode, SetTextColor, StretchBlt, DRAW_TEXT_FORMAT, DT_CENTER, DT_END_ELLIPSIS,
    DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HALFTONE, HDC, HFONT, HGDIOBJ,
    PAINTSTRUCT, RGBQUAD, SRCCOPY, STRETCH_BLT_MODE, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{
    BeginBufferedPaint, BufferedPaintSetAlpha, CloseThemeData, DrawThemeTextEx, EndBufferedPaint,
    GetBufferedPaintBits, OpenThemeData, BPBF_TOPDOWNDIB, DTTOPTS, DTT_COMPOSITED, DTT_TEXTCOLOR,
    HTHEME,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetKeyState, VK_CONTROL, VK_DOWN, VK_END, VK_ESCAPE, VK_HOME, VK_LEFT, VK_NEXT, VK_P, VK_PRIOR,
    VK_RETURN, VK_RIGHT, VK_SHIFT, VK_TAB, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
    GetClientRect, GetCursorPos, GetForegroundWindow, GetGUIThreadInfo, GetWindowLongPtrW,
    GetWindowThreadProcessId, IsWindow, IsWindowVisible, LoadCursorW, MessageBoxW, PostMessageW,
    RegisterClassW, SetForegroundWindow, SetWindowLongPtrW, ShowWindow, TrackPopupMenu,
    CREATESTRUCTW, CS_DBLCLKS, CW_USEDEFAULT, GUITHREADINFO, GWLP_USERDATA, IDC_ARROW, IDNO, IDYES,
    MB_ICONINFORMATION, MB_ICONWARNING, MB_OK, MB_YESNO, MB_YESNOCANCEL, MF_SEPARATOR, MF_STRING,
    SW_HIDE, SW_SHOW, TPM_LEFTALIGN, TPM_RETURNCMD, TPM_RIGHTBUTTON, WA_INACTIVE, WM_ACTIVATE,
    WM_CHAR, WM_CLOSE, WM_CREATE, WM_DESTROY, WM_DPICHANGED, WM_ERASEBKGND, WM_HOTKEY, WM_KEYDOWN,
    WM_KILLFOCUS, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN, WM_NCCREATE, WM_PAINT, WM_PASTE, WM_RBUTTONUP,
    WM_SETTINGCHANGE, WM_THEMECHANGED, WNDCLASSW, WS_CAPTION, WS_EX_TOOLWINDOW, WS_POPUP,
    WS_SYSMENU, WS_THICKFRAME,
};

use crate::app_icon;
use crate::clipboard_restore;
use crate::dpi;
use crate::error_banner;
use crate::hotkey_win::HOTKEY_ID;
use crate::navigation::{next_selection_index, row_index_at, NavKey};
use crate::paste_injector;
use crate::system_appearance::SystemAppearance;
use crate::text_metrics;
use crate::thumbnail_gdi::ThumbnailCache;
use crate::win_clock::WinClock;
use crate::window_material;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_history_window");
const WINDOW_TITLE: PCWSTR = windows::core::w!("Histórico — duplicata");
const WINDOW_WIDTH: i32 = row_layout::WINDOW_WIDTH_PX;
const WINDOW_HEIGHT: i32 = row_layout::WINDOW_HEIGHT_PX;
const ROW_HEIGHT_PX: i32 = row_layout::ROW_HEIGHT_PX;
const FILTER_STRIP_PX: i32 = 34;
const SEARCH_BAR_PX: i32 = 30;
const LIST_TOP_PX: i32 = FILTER_STRIP_PX + SEARCH_BAR_PX;
const HINT_STRIP_PX: i32 = hint_strip::HINT_STRIP_PX;

pub const WM_APP_TOGGLE_ENCRYPTION: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 10;
const WM_TOGGLE_PROGRESS: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 11;
const WM_TOGGLE_DONE: u32 = windows::Win32::UI::WindowsAndMessaging::WM_APP + 12;

#[derive(Clone, Copy)]
struct ToggleUi {
    enable: bool,
    count: u64,
}
const MAX_ROWS: usize = 500;

struct HistoryWindowCtx {
    reader: RefCell<Option<Box<dyn HistoryReader>>>,
    reopen_reader: Box<dyn Fn() -> Result<Box<dyn HistoryReader>, StoreError>>,
    may_offer_recreate: Box<dyn Fn() -> bool>,
    queue: Arc<dyn CaptureQueue<WorkItem>>,
    db_path: PathBuf,
    error: RefCell<Option<StoreError>>,
    self_write_filter: Rc<SelfWriteFilter>,
    rows: RefCell<Vec<ClipListItem>>,
    selected: Cell<usize>,
    scroll_offset: Cell<usize>,
    glass: Cell<bool>,
    segment_rects: Cell<[CoreRect; 4]>,
    prev_hwnd: Cell<HWND>,
    thumbnail_cache: RefCell<ThumbnailCache>,
    clock: WinClock,
    last_hide_at_ms: Cell<u64>,
    toggle: Cell<Option<ToggleUi>>,
    appearance: RefCell<SystemAppearance>,
    ui_font: Cell<HFONT>,
    filter_text: RefCell<String>,
    type_filter: Cell<ContentTypeFilter>,
    strip_focused: Cell<bool>,
    max_pinned: Cell<u32>,
}

pub struct HistoryWindow {
    hwnd: HWND,
    _ctx: Box<HistoryWindowCtx>,
}

impl HistoryWindow {
    pub fn new<R, F, G>(
        reader: R,
        reopen_reader: F,
        may_offer_recreate: G,
        queue: Arc<dyn CaptureQueue<WorkItem>>,
        db_path: PathBuf,
        self_write_filter: Rc<SelfWriteFilter>,
    ) -> Result<Self, InitError>
    where
        R: HistoryReader + 'static,
        F: Fn() -> Result<Box<dyn HistoryReader>, StoreError> + 'static,
        G: Fn() -> bool + 'static,
    {
        let mut ctx = Box::new(HistoryWindowCtx {
            reader: RefCell::new(Some(Box::new(reader))),
            reopen_reader: Box::new(reopen_reader),
            may_offer_recreate: Box::new(may_offer_recreate),
            queue,
            db_path,
            error: RefCell::new(None),
            self_write_filter,
            rows: RefCell::new(Vec::new()),
            selected: Cell::new(0),
            scroll_offset: Cell::new(0),
            glass: Cell::new(false),
            segment_rects: Cell::new(segment_rects_packed(
                &[30, 34, 42, 54],
                filter_strip::SEGMENT_PADDING_PX,
                FILTER_STRIP_PX,
            )),
            prev_hwnd: Cell::new(HWND::default()),
            thumbnail_cache: RefCell::new(ThumbnailCache::new()),
            clock: WinClock,
            last_hide_at_ms: Cell::new(0),
            toggle: Cell::new(None),
            appearance: RefCell::new(SystemAppearance::system_defaults()),
            ui_font: Cell::new(HFONT::default()),
            filter_text: RefCell::new(String::new()),
            type_filter: Cell::new(ContentTypeFilter::All),
            strip_focused: Cell::new(false),
            max_pinned: Cell::new(DEFAULT_MAX_PINNED),
        });

        // SAFETY: registro de classe idempotente com `wndproc` válido; mesmo
        // padrão do listener de clipboard e da bandeja (ignoramos o erro de
        // "classe já registrada"). A janela é WS_POPUP com borda/título/menu
        // de sistema normais (WS_CAPTION/WS_SYSMENU/WS_THICKFRAME) — mas SEM
        // WS_VISIBLE: só aparece via `ShowWindow` no ramo de abertura do
        // WM_HOTKEY. WS_EX_TOOLWINDOW evita entrada persistente na barra de
        // tarefas.
        unsafe {
            let hinstance = GetModuleHandleW(None).map_err(|_| InitError::HistoryWindow)?;
            let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
            let icon = app_icon::load_app_icon().unwrap_or_default();
            let wc = WNDCLASSW {
                style: CS_DBLCLKS,
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                hCursor: cursor,
                hIcon: icon,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let ctx_ptr: *mut HistoryWindowCtx = ctx.as_mut();
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW,
                CLASS_NAME,
                WINDOW_TITLE,
                WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                WINDOW_WIDTH,
                WINDOW_HEIGHT,
                None,
                None,
                Some(hinstance.into()),
                Some(ctx_ptr.cast()),
            )
            .map_err(|_| InitError::HistoryWindow)?;

            Ok(HistoryWindow { hwnd, _ctx: ctx })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn set_max_pinned(&self, max_pinned: u32) {
        self._ctx.max_pinned.set(max_pinned);
    }
}

impl Drop for HistoryWindow {
    fn drop(&mut self) {
        let font = self._ctx.ui_font.get();
        if !font.is_invalid() {
            // SAFETY: `font` veio de `CreateFontIndirectW` (nossa) e não está
            // mais selecionado em nenhum DC — o `paint` sempre restaura o
            // objeto anterior antes de retornar.
            unsafe {
                let _ = DeleteObject(HGDIOBJ(font.0));
            }
        }
        // SAFETY: `hwnd` foi criado por nós; o processo só chega aqui no
        // shutdown — a janela nunca é destruída/recriada em uso normal
        // (FR-031).
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn open_window(hwnd: HWND, ctx: &HistoryWindowCtx) {
    // SAFETY: sem pré-condição — só lê o handle da janela em primeiro plano
    // no momento do atalho.
    let prev = unsafe { GetForegroundWindow() };
    ctx.prev_hwnd.set(prev);

    refresh_rows(ctx);

    // (US3, FR-022/023/026) Reposiciona perto do cursor, no monitor certo,
    // dentro da área útil — ANTES de exibir. Só um `SetWindowPos`; a janela
    // continua criada uma única vez.
    // SAFETY: `hwnd` é a janela criada por `HistoryWindow::new`.
    unsafe {
        dpi::place_window_near_cursor(hwnd);
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
    invalidate(hwnd);
}

fn refresh_rows(ctx: &HistoryWindowCtx) {
    let limit = MAX_ROWS + ctx.max_pinned.get() as usize;
    let result = match ctx.reader.borrow().as_ref() {
        Some(r) => r.list_for_display(limit),
        None => Err(StoreError::Io),
    };
    match result {
        Ok(items) => {
            ctx.error.replace(None);
            *ctx.rows.borrow_mut() = items;
        }
        Err(e) => {
            log_event!(
                Level::WARN,
                LogFields::new("history_read_failed").kind(error_banner::store_error_kind(&e))
            );
            ctx.rows.borrow_mut().clear();
            ctx.error.replace(Some(e));
        }
    }
    ctx.selected.set(0);
    ctx.scroll_offset.set(0);
}

fn hide_window(hwnd: HWND, ctx: &HistoryWindowCtx) {
    // SAFETY: `hwnd` válido, criado por `HistoryWindow::new`.
    unsafe {
        let _ = ShowWindow(hwnd, SW_HIDE);
    }
    ctx.thumbnail_cache.borrow_mut().clear();
    ctx.last_hide_at_ms.set(ctx.clock.now_ms());
    ctx.filter_text.borrow_mut().clear();
    ctx.type_filter.set(ContentTypeFilter::All);
    ctx.strip_focused.set(false);
}

pub fn foreground_really_changed_away(hwnd: HWND, current_foreground: HWND) -> bool {
    current_foreground != hwnd
}

fn really_lost_foreground(hwnd: HWND) -> bool {
    // SAFETY: sem pré-condição — só lê o handle da janela em primeiro plano
    // no instante da chamada.
    let current = unsafe { GetForegroundWindow() };
    foreground_really_changed_away(hwnd, current)
}

fn handle_hotkey_toggle(hwnd: HWND, ctx: &HistoryWindowCtx) {
    if crate::hotkey_capture::is_armed() {
        return;
    }
    // SAFETY: `hwnd` válido, criado por `HistoryWindow::new`.
    let visible = unsafe { IsWindowVisible(hwnd) }.as_bool();
    if visible {
        hide_window(hwnd, ctx);
        return;
    }
    if should_suppress_reopen(ctx.last_hide_at_ms.get(), ctx.clock.now_ms()) {
        return;
    }
    open_window(hwnd, ctx);
}

fn confirm(hwnd: HWND, ctx: &HistoryWindowCtx, clip_id: i64) {
    let stored = match ctx.reader.borrow().as_ref().map(|r| r.get_full(clip_id)) {
        Some(Ok(Some(s))) => s,
        _ => return,
    };
    let prev_hwnd = ctx.prev_hwnd.get();

    clipboard_restore::write_all(&ctx.self_write_filter, &stored.formats);
    hide_window(hwnd, ctx);
    paste_injector::send_to(prev_hwnd);
}

fn confirm_text_only(hwnd: HWND, ctx: &HistoryWindowCtx, clip_id: i64) {
    let stored = match ctx.reader.borrow().as_ref().map(|r| r.get_full(clip_id)) {
        Some(Ok(Some(s))) => s,
        _ => return,
    };
    let Some(text_format) = stored.text_format else {
        return;
    };
    let prev_hwnd = ctx.prev_hwnd.get();

    clipboard_restore::write_text_only(&ctx.self_write_filter, &text_format);
    hide_window(hwnd, ctx);
    paste_text_via_wm_paste(prev_hwnd);
}

fn paste_text_via_wm_paste(prev_hwnd: HWND) {
    // SAFETY: `IsWindow` só consulta se o handle ainda é uma janela válida —
    // sem pré-condição além de um HWND (possivelmente obsoleto), mesma
    // guarda de `paste_injector::send_to`.
    if !unsafe { IsWindow(Some(prev_hwnd)) }.as_bool() {
        return;
    }
    // SAFETY: `prev_hwnd` validado acima; falha aqui é best-effort, mesma
    // postura de `paste_injector::send_to`.
    let _ = unsafe { SetForegroundWindow(prev_hwnd) };

    // SAFETY: `prev_hwnd` validado acima; `None` porque só precisamos do tid
    // (valor de retorno), não do pid.
    let tid = unsafe { GetWindowThreadProcessId(prev_hwnd, None) };
    let mut info = GUITHREADINFO {
        cbSize: core::mem::size_of::<GUITHREADINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `info.cbSize` já setado (exigido pela API antes da chamada);
    // `tid` obtido acima.
    let hwnd_focus =
        if unsafe { GetGUIThreadInfo(tid, &mut info) }.is_ok() && !info.hwndFocus.is_invalid() {
            info.hwndFocus
        } else {
            prev_hwnd
        };

    // SAFETY: `hwnd_focus` é `prev_hwnd` (validado acima via IsWindow) ou um
    // HWND devolvido por GetGUIThreadInfo dentro do mesmo processo; WM_PASTE
    // sem payload (wParam/lParam ambos 0), best-effort — se o controle não
    // aceitar a mensagem, não faz nada além disso.
    let _ = unsafe { PostMessageW(Some(hwnd_focus), WM_PASTE, WPARAM(0), LPARAM(0)) };
}

fn row_label(item: &ClipListItem) -> String {
    if let Some(text) = item.preview.as_deref() {
        if !text.is_empty() {
            return text.to_string();
        }
    }
    if item.thumbnail.is_some() {
        return String::new();
    }
    match item.canonical_kind.as_str() {
        "dib" | "dibv5" => "[imagem — sem prévia]",
        "hdrop" => "[arquivos]",
        _ => "[sem prévia]",
    }
    .to_string()
}

fn reload_appearance(hwnd: HWND, ctx: &HistoryWindowCtx) {
    let dark = SystemAppearance::read_dark();
    // SAFETY: `hwnd` é a janela de histórico desta thread.
    let backdrop_ok = unsafe { window_material::apply_attributes(hwnd, dark) };
    // SAFETY: idem.
    let appearance = unsafe { SystemAppearance::load(hwnd, backdrop_ok) };

    window_material::maybe_emit_material_diag(appearance.material_cause);

    let want_glass = appearance.material_available && window_material::buffered_paint_ready();
    if want_glass != ctx.glass.get() {
        let applied = if want_glass {
            // SAFETY: `hwnd` é a janela de histórico desta thread, e o
            // buffered paint está pronto (checado em `want_glass`).
            unsafe { window_material::apply_glass(hwnd, true) }
        } else {
            // SAFETY: idem; só zera as margens do frame estendido.
            unsafe { window_material::remove_glass(hwnd) };
            false
        };
        ctx.glass.set(applied);
    }

    // SAFETY: `message_font` foi preenchido por `SPI_GETNONCLIENTMETRICS`.
    let new_font = unsafe { CreateFontIndirectW(&appearance.message_font) };
    let old_font = ctx.ui_font.replace(if new_font.is_invalid() {
        HFONT::default()
    } else {
        new_font
    });
    if !old_font.is_invalid() {
        // SAFETY: `old_font` é nossa e não está selecionada em nenhum DC agora
        // — `paint` sempre restaura o objeto anterior antes de retornar.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(old_font.0));
        }
    }

    *ctx.appearance.borrow_mut() = appearance;
    invalidate(hwnd);
}

fn is_immersive_color_set(lparam: LPARAM) -> bool {
    if lparam.0 == 0 {
        return false;
    }
    // SAFETY: num `WM_SETTINGCHANGE`, `lParam` ou é 0 ou aponta para uma
    // string wide NUL-terminada fornecida pelo SO.
    let s = unsafe { PCWSTR(lparam.0 as *const u16).to_string() };
    s.map(|s| s == "ImmersiveColorSet").unwrap_or(false)
}

fn client_rect(hwnd: HWND) -> RECT {
    let mut client = RECT::default();
    // SAFETY: `hwnd` válido.
    let _ = unsafe { GetClientRect(hwnd, &mut client) };
    client
}

fn list_area(hwnd: HWND) -> RECT {
    let mut r = client_rect(hwnd);
    r.top += LIST_TOP_PX;
    r.bottom = (r.bottom - HINT_STRIP_PX).max(r.top);
    r
}

fn visible_row_count(hwnd: HWND) -> usize {
    let area = list_area(hwnd);
    (((area.bottom - area.top).max(0)) / ROW_HEIGHT_PX).max(1) as usize
}

fn visible_indices(ctx: &HistoryWindowCtx) -> Vec<usize> {
    let rows = ctx.rows.borrow();
    list_view::visible_rows(&rows, &ctx.filter_text.borrow(), ctx.type_filter.get())
}

fn selected_clip_id(ctx: &HistoryWindowCtx) -> Option<i64> {
    let visible = visible_indices(ctx);
    let row_idx = *visible.get(ctx.selected.get())?;
    ctx.rows.borrow().get(row_idx).map(|r| r.id)
}

fn refilter(hwnd: HWND, ctx: &HistoryWindowCtx) {
    ctx.selected.set(0);
    ctx.scroll_offset.set(0);
    invalidate(hwnd);
}

fn select_clip(hwnd: HWND, ctx: &HistoryWindowCtx, clip_id: i64) {
    let visible = visible_indices(ctx);
    let rows = ctx.rows.borrow();
    let pos = visible
        .iter()
        .position(|&i| rows[i].id == clip_id)
        .unwrap_or(0);
    drop(rows);
    ctx.selected.set(pos);
    let rows_per_page = visible_row_count(hwnd);
    let scroll = ctx.scroll_offset.get();
    if pos < scroll {
        ctx.scroll_offset.set(pos);
    } else if pos >= scroll + rows_per_page {
        ctx.scroll_offset.set(pos + 1 - rows_per_page);
    }
    invalidate(hwnd);
}

fn invalidate(hwnd: HWND) {
    // SAFETY: `hwnd` válido; `None` para `lprect` invalida a janela toda.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, true);
    }
}

struct PaintColors {
    surface: COLORREF,
    text_primary: COLORREF,
    text_dim: COLORREF,
    border: COLORREF,
    selection_bg: COLORREF,
    selection_fg: COLORREF,
    accent: COLORREF,
}

impl PaintColors {
    fn from(a: &SystemAppearance) -> Self {
        PaintColors {
            surface: a.palette.surface,
            text_primary: a.palette.text_primary,
            text_dim: a.palette.text_dim,
            border: a.palette.border,
            selection_bg: a.selection_bg,
            selection_fg: a.selection_fg,
            accent: a.accent,
        }
    }
}

fn core_rect(r: &RECT) -> CoreRect {
    CoreRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn win_rect(r: &CoreRect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

/// # Safety
/// `hdc` válido; `rect` dentro dos limites da janela.
unsafe fn fill_solid(hdc: HDC, rect: &RECT, color: COLORREF) {
    // SAFETY: `CreateSolidBrush` sem pré-condição; o brush é usado só nesta
    // chamada e liberado logo em seguida.
    unsafe {
        let brush = CreateSolidBrush(color);
        if !brush.is_invalid() {
            FillRect(hdc, rect, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));
        }
    }
}

struct TextRun {
    rect: RECT,
    text: Vec<u16>,
    color: COLORREF,
    flags: DRAW_TEXT_FORMAT,
}

#[derive(Default)]
struct PaintSink {
    runs: Vec<TextRun>,
    islands: Vec<CoreRect>,
}

impl PaintSink {
    fn text(&mut self, rect: RECT, text: &str, color: COLORREF, flags: DRAW_TEXT_FORMAT) {
        self.runs.push(TextRun {
            rect,
            text: text.encode_utf16().chain(std::iter::once(0)).collect(),
            color,
            flags,
        });
    }

    fn opaque(&mut self, rect: &RECT) {
        self.islands.push(core_rect(rect));
    }
}

fn flush_text(hdc: HDC, theme: Option<HTHEME>, sink: &mut PaintSink) {
    for run in sink.runs.iter_mut() {
        let mut rect = run.rect;
        match theme {
            Some(t) => {
                let opts = DTTOPTS {
                    dwSize: core::mem::size_of::<DTTOPTS>() as u32,
                    dwFlags: DTT_COMPOSITED | DTT_TEXTCOLOR,
                    crText: run.color,
                    ..Default::default()
                };
                let len = run.text.len().saturating_sub(1);
                // SAFETY: `t` é um `HTHEME` aberto e ainda não fechado; `hdc`
                // válido; `rect`/`opts` são locais vivos durante a chamada.
                unsafe {
                    let _ = DrawThemeTextEx(
                        t,
                        hdc,
                        0,
                        0,
                        &run.text[..len],
                        run.flags,
                        &mut rect,
                        Some(&opts),
                    );
                }
            }
            None => {
                // SAFETY: `hdc` válido; `run.text` é NUL-terminado e vive
                // durante a chamada; `rect` é local.
                unsafe {
                    SetTextColor(hdc, run.color);
                    DrawTextW(hdc, &mut run.text, &mut rect, run.flags);
                }
            }
        }
    }
}

/// # Safety
/// `hbp` MUST ser um handle vivo de `BeginBufferedPaint`.
unsafe fn apply_alpha_plan(hbp: isize, plan: &[translucency::AlphaOp]) {
    for op in plan {
        match op.rect {
            Some(r) => {
                let rect = win_rect(&r);
                // SAFETY: `hbp` vivo por contrato; `rect` é um local que vive
                // durante a chamada.
                unsafe {
                    let _ = BufferedPaintSetAlpha(hbp, Some(&rect), op.alpha);
                }
            }
            // SAFETY: `hbp` vivo por contrato; `prc` nulo = buffer inteiro.
            None => unsafe {
                let _ = BufferedPaintSetAlpha(hbp, None, op.alpha);
            },
        }
    }
}

fn paint(hwnd: HWND, ctx: &HistoryWindowCtx) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` padrão dentro do handler de
    // `WM_PAINT`; `hdc` devolvido é válido até o `EndPaint` no final desta
    // função.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };

    let mut client = RECT::default();
    // SAFETY: `hwnd` válido.
    let _ = unsafe { GetClientRect(hwnd, &mut client) };

    let appearance = ctx.appearance.borrow();
    let colors = PaintColors::from(&appearance);

    let mut hbp: isize = 0;
    let mut buf_hdc = HDC::default();
    if ctx.glass.get() {
        // SAFETY: `hdc` do `BeginPaint`; `client` e `buf_hdc` são locais vivos
        // durante a chamada. Handle pareado com `EndBufferedPaint` no fim.
        hbp = unsafe { BeginBufferedPaint(hdc, &client, BPBF_TOPDOWNDIB, None, &mut buf_hdc) };
        if hbp == 0 || buf_hdc.is_invalid() {
            hbp = 0;
            ctx.glass.set(false);
            // SAFETY: `hwnd` válido.
            unsafe { window_material::remove_glass(hwnd) };
            log_event!(
                Level::WARN,
                LogFields::new("material_unavailable").kind("buffered_paint_indisponivel")
            );
        }
    }
    let theme = if hbp != 0 {
        open_text_theme(hwnd)
    } else {
        None
    };
    let dc = if hbp != 0 { buf_hdc } else { hdc };

    let font = ctx.ui_font.get();
    let old_font: Option<HGDIOBJ> = if font.is_invalid() {
        None
    } else {
        // SAFETY: `dc` válido; `font` é nossa e vive até o Drop da janela.
        Some(unsafe { SelectObject(dc, HGDIOBJ(font.0)) })
    };

    // SAFETY: limpa a janela inteira com a superfície opaca do tema e fixa o
    // modo de fundo transparente para o texto por cima.
    unsafe {
        fill_solid(dc, &client, colors.surface);
        SetBkMode(dc, TRANSPARENT);
    }

    let mut sink = PaintSink::default();
    let mut banner: Option<CoreRect> = None;

    if let Some(t) = ctx.toggle.get() {
        paint_toggle_banner(dc, client, t, &colors);
        banner = Some(core_rect(&client));
    }

    if banner.is_none() {
        if let Some(err) = ctx.error.borrow().as_ref() {
            paint_error_banner(
                dc,
                client,
                err,
                &ctx.db_path,
                (ctx.may_offer_recreate)(),
                &colors,
            );
            banner = Some(core_rect(&client));
        }
    }

    if banner.is_some() {
        finish_paint(
            hwnd, hdc, dc, &ps, old_font, hbp, theme, banner, &client, &mut sink,
        );
        return;
    }

    paint_chrome(dc, client, ctx, &colors, &mut sink);
    paint_hint_strip(dc, client, &colors, &mut sink);

    let rows = ctx.rows.borrow();
    let visible: Vec<usize> =
        list_view::visible_rows(&rows, &ctx.filter_text.borrow(), ctx.type_filter.get());
    let selected = ctx.selected.get();
    let scroll = ctx.scroll_offset.get();
    let visible_rows = visible_row_count(hwnd);
    let list_top = client.top + LIST_TOP_PX;
    let dpi = appearance.dpi;

    let filter_active =
        !ctx.filter_text.borrow().is_empty() || ctx.type_filter.get() != ContentTypeFilter::All;
    let state = list_view::display_state(false, false, rows.len(), visible.len(), filter_active);
    if state == list_view::ListDisplayState::NoMatch {
        let msg_rect = RECT {
            left: client.left + 16,
            top: list_top + 8,
            right: client.right - 16,
            bottom: client.bottom - HINT_STRIP_PX - 8,
        };
        sink.text(
            msg_rect,
            "Nenhum item corresponde ao filtro",
            colors.text_dim,
            DT_WORDBREAK | DT_NOPREFIX,
        );
        drop(rows);
        drop(appearance);
        finish_paint(
            hwnd, hdc, dc, &ps, old_font, hbp, theme, banner, &client, &mut sink,
        );
        return;
    }

    for (visible_idx, &row_idx) in visible.iter().skip(scroll).take(visible_rows).enumerate() {
        let item = &rows[row_idx];
        let pos = scroll + visible_idx;
        let top = list_top + (visible_idx as i32) * ROW_HEIGHT_PX;
        let row_rect = RECT {
            left: client.left,
            top,
            right: client.right,
            bottom: top + ROW_HEIGHT_PX,
        };
        let is_selected = pos == selected;

        // Seleção: par de realce do sistema (FR-003), sem cálculo de contraste
        // próprio. Linhas não selecionadas ficam com a superfície do tema já
        // pintada na limpeza da janela inteira.
        // SAFETY: `dc` válido; `row_rect` dentro do cliente.
        unsafe {
            if is_selected {
                fill_solid(dc, &row_rect, colors.selection_bg);
            }
        }
        if is_selected {
            sink.opaque(&row_rect);
        }
        let row_fg = if is_selected {
            colors.selection_fg
        } else {
            colors.text_primary
        };

        let thumb = item.thumbnail.as_ref().and_then(|bytes| {
            ctx.thumbnail_cache
                .borrow_mut()
                .get_or_decode(item.id, bytes)
        });
        let layout =
            row_layout::row_layout(core_rect(&row_rect), dpi, thumb.map(|(_, bw, bh)| (bw, bh)));

        if item.pinned {
            let stripe = win_rect(&layout.pin_stripe);
            // SAFETY: `dc` válido; `stripe` dentro de `row_rect`.
            unsafe {
                fill_solid(
                    dc,
                    &stripe,
                    if is_selected {
                        colors.selection_fg
                    } else {
                        colors.accent
                    },
                );
            }
            sink.opaque(&stripe);
        }

        sink.text(
            win_rect(&layout.type_badge),
            row_layout::badge_of(&item.canonical_kind).label(),
            if is_selected {
                colors.selection_fg
            } else {
                colors.text_dim
            },
            DT_SINGLELINE | DT_VCENTER | DT_CENTER | DT_NOPREFIX,
        );

        if let Some((hbitmap, bw, bh)) = thumb {
            let t = layout.thumbnail;
            if t.right > t.left && t.bottom > t.top {
                // SAFETY: `hdc` válido; `hbitmap` foi decodificado e
                // cacheado por `ThumbnailCache` (ainda vivo, só liberado por
                // `hide_window`); `mem_dc` é criado, usado e destruído só
                // dentro deste bloco.
                unsafe {
                    let mem_dc = CreateCompatibleDC(Some(dc));
                    let old = SelectObject(mem_dc, hbitmap.into());
                    let old_mode = SetStretchBltMode(dc, HALFTONE);
                    let _ = SetBrushOrgEx(dc, 0, 0, None);
                    let _ = StretchBlt(
                        dc,
                        t.left,
                        t.top,
                        t.right - t.left,
                        t.bottom - t.top,
                        Some(mem_dc),
                        0,
                        0,
                        bw as i32,
                        bh as i32,
                        SRCCOPY,
                    );
                    if old_mode != 0 {
                        SetStretchBltMode(dc, STRETCH_BLT_MODE(old_mode));
                    }
                    SelectObject(mem_dc, old);
                    let _ = DeleteDC(mem_dc);
                }
                sink.opaque(&win_rect(&t));
            }
        }

        sink.text(
            win_rect(&layout.text),
            &row_label(item),
            row_fg,
            DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
        );
    }

    if let Some(boundary) = list_view::pinned_group_boundary(&rows, &visible) {
        if boundary > scroll && boundary < scroll + visible_rows {
            let y = list_top + ((boundary - scroll) as i32) * ROW_HEIGHT_PX;
            let sep = RECT {
                left: client.left + 6,
                top: y - 1,
                right: client.right - 6,
                bottom: y + 1,
            };
            // SAFETY: `dc` válido; `sep` dentro da área da lista.
            unsafe {
                fill_solid(dc, &sep, colors.text_dim);
            }
            sink.opaque(&sep);
        }
    }

    drop(rows);
    drop(appearance);
    finish_paint(
        hwnd, hdc, dc, &ps, old_font, hbp, theme, banner, &client, &mut sink,
    );
}

#[allow(clippy::too_many_arguments)]
fn finish_paint(
    hwnd: HWND,
    hdc: HDC,
    dc: HDC,
    ps: &PAINTSTRUCT,
    old_font: Option<HGDIOBJ>,
    hbp: isize,
    theme: Option<HTHEME>,
    banner: Option<CoreRect>,
    client: &RECT,
    sink: &mut PaintSink,
) {
    let translucent = hbp != 0 && theme.is_some();
    if translucent {
        let plan = translucency::alpha_plan(&translucency::WindowRegions {
            client: core_rect(client),
            banner,
            opaque_islands: std::mem::take(&mut sink.islands),
        });
        // SAFETY: `hbp` é o handle vivo de `BeginBufferedPaint` deste frame.
        unsafe { apply_alpha_plan(hbp, &plan) };
        // SAFETY: idem. Tem de vir DEPOIS do plano (é ele que define o alfa de
        // cada pixel) e ANTES do texto (o `DTT_COMPOSITED` já entrega pixels
        // pré-multiplicados; passar de novo escureceria os glifos).
        unsafe { premultiply_buffer_in_place(hbp, client.bottom - client.top) };
    } else if hbp != 0 {
        // Buffer existe mas sem tema: opaca tudo explicitamente. O GDI zerou o
        // alfa ao pintar, e sem este passo a janela sairia invisível.
        // SAFETY: idem.
        unsafe {
            let _ = BufferedPaintSetAlpha(hbp, None, translucency::OPAQUE);
        }
    }

    flush_text(dc, if translucent { theme } else { None }, sink);

    if let Some(t) = theme {
        // SAFETY: `t` foi aberto por `open_text_theme` neste frame.
        unsafe {
            let _ = CloseThemeData(t);
        }
    }
    if hbp != 0 {
        // SAFETY: pareado com o `BeginBufferedPaint` deste frame; `true`
        // manda o buffer para o DC da janela.
        unsafe {
            let _ = EndBufferedPaint(hbp, true);
        }
    }
    paint_cleanup(hwnd, hdc, ps, old_font);
}

/// # Safety
/// `hbp` MUST ser um handle vivo de `BeginBufferedPaint`, e `rows` a altura do
/// retângulo com que ele foi criado.
unsafe fn premultiply_buffer_in_place(hbp: isize, rows: i32) {
    let mut bits: *mut RGBQUAD = std::ptr::null_mut();
    let mut stride: i32 = 0;
    // SAFETY: `hbp` vivo por contrato; os dois ponteiros são locais.
    if unsafe { GetBufferedPaintBits(hbp, &mut bits, &mut stride) }.is_err() || bits.is_null() {
        return;
    }
    if rows <= 0 || stride <= 0 {
        return;
    }
    let rows = rows as usize;
    // SAFETY: `bits` aponta para `stride * rows` `RGBQUAD` (4 bytes cada) do
    // DIB que `BeginBufferedPaint` alocou, vivo até o `EndBufferedPaint` deste
    // frame; ninguém mais o lê agora.
    unsafe {
        let len = (stride as usize) * rows * 4;
        let buf = std::slice::from_raw_parts_mut(bits.cast::<u8>(), len);
        duplicata_core::menu_icon::premultiply_buffer(buf);
    }
}

fn open_text_theme(hwnd: HWND) -> Option<HTHEME> {
    // SAFETY: `hwnd` válido; a classe "Window" existe em qualquer tema visual.
    let t = unsafe { OpenThemeData(Some(hwnd), windows::core::w!("Window")) };
    (!t.is_invalid()).then_some(t)
}

fn paint_chrome(
    hdc: HDC,
    client: RECT,
    ctx: &HistoryWindowCtx,
    colors: &PaintColors,
    sink: &mut PaintSink,
) {
    let active = ctx.type_filter.get();
    let strip_focused = ctx.strip_focused.get();

    let mut label_widths = [0i32; 4];
    for (w, seg) in label_widths.iter_mut().zip(ContentTypeFilter::SEGMENTS) {
        // SAFETY: `hdc` do `BeginPaint`, com a fonte de interface já
        // selecionada por `paint`.
        *w = unsafe { text_metrics::width_in_dc(hdc, seg.label()) };
    }
    let seg_rects = segment_rects_packed(
        &label_widths,
        filter_strip::SEGMENT_PADDING_PX,
        FILTER_STRIP_PX,
    );
    ctx.segment_rects.set(seg_rects);

    for (i, seg) in ContentTypeFilter::SEGMENTS.iter().enumerate() {
        let r = RECT {
            left: client.left + seg_rects[i].left,
            top: client.top + seg_rects[i].top,
            right: client.left + seg_rects[i].right,
            bottom: client.top + seg_rects[i].bottom,
        };
        let is_active = *seg == active;
        if is_active {
            // SAFETY: `hdc` válido; `r` dentro do cliente.
            unsafe {
                fill_solid(hdc, &r, colors.selection_bg);
            }
            sink.opaque(&r);
        }
        sink.text(
            r,
            seg.label(),
            if is_active {
                colors.selection_fg
            } else {
                colors.text_dim
            },
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
    }
    let strip_border = RECT {
        left: client.left,
        top: client.top + FILTER_STRIP_PX - if strip_focused { 2 } else { 1 },
        right: client.right,
        bottom: client.top + FILTER_STRIP_PX,
    };
    // SAFETY: `hdc` válido; retângulo dentro do cliente.
    unsafe {
        fill_solid(
            hdc,
            &strip_border,
            if strip_focused {
                colors.selection_bg
            } else {
                colors.border
            },
        );
    }
    sink.opaque(&strip_border);

    let search_top = client.top + FILTER_STRIP_PX;
    let text_rect = RECT {
        left: client.left + 10,
        top: search_top,
        right: client.right - 10,
        bottom: search_top + SEARCH_BAR_PX,
    };
    let filter_text = ctx.filter_text.borrow();
    let (shown, color) = if filter_text.is_empty() {
        (
            "Digite para filtrar o histórico".to_string(),
            colors.text_dim,
        )
    } else {
        (filter_text.clone(), colors.text_primary)
    };
    drop(filter_text);
    sink.text(
        text_rect,
        &shown,
        color,
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
    let search_border = RECT {
        left: client.left,
        top: search_top + SEARCH_BAR_PX - 1,
        right: client.right,
        bottom: search_top + SEARCH_BAR_PX,
    };
    // SAFETY: `hdc` válido; retângulo dentro do cliente.
    unsafe {
        fill_solid(hdc, &search_border, colors.border);
    }
    sink.opaque(&search_border);
}

fn paint_hint_strip(hdc: HDC, client: RECT, colors: &PaintColors, sink: &mut PaintSink) {
    let top = client.bottom - HINT_STRIP_PX;
    let border = RECT {
        left: client.left,
        top,
        right: client.right,
        bottom: top + 1,
    };
    let text_rect = RECT {
        left: client.left + 10,
        top,
        right: client.right - 10,
        bottom: client.bottom,
    };
    // SAFETY: `hdc` válido; `border` dentro do cliente.
    unsafe {
        fill_solid(hdc, &border, colors.border);
    }
    sink.opaque(&border);
    sink.text(
        text_rect,
        &hint_strip::hint_line(),
        colors.text_dim,
        DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS | DT_NOPREFIX,
    );
}

fn paint_cleanup(hwnd: HWND, hdc: HDC, ps: &PAINTSTRUCT, old_font: Option<HGDIOBJ>) {
    if let Some(f) = old_font {
        // SAFETY: `hdc` ainda válido dentro do handler de `WM_PAINT`; `f` é o
        // objeto que estava selecionado antes.
        unsafe {
            SelectObject(hdc, f);
        }
    }
    // SAFETY: fecha o par iniciado por `BeginPaint`.
    unsafe {
        let _ = EndPaint(hwnd, ps);
    }
}

fn paint_error_banner(
    hdc: HDC,
    client: RECT,
    err: &StoreError,
    db_path: &Path,
    recreate_allowed: bool,
    colors: &PaintColors,
) {
    // (US1) O banner é pintado no mesmo `WM_PAINT` da janela e usa as mesmas
    // cores de `SystemAppearance` — herda o repintar de tema ao vivo (FR-006).
    // Fundo SEMPRE opaco (a superfície do tema): um erro não deve ficar
    // translúcido e difícil de ler.
    // SAFETY: `hdc` válido — chamador (`paint`) está dentro do par
    // `BeginPaint`/`EndPaint`.
    unsafe {
        fill_solid(hdc, &client, colors.surface);
    }

    let banner = error_banner::banner_for(err, db_path, recreate_allowed);
    let button =
        error_banner::recreate_button_rect(client.right - client.left, client.bottom - client.top);

    let mut text_rect = RECT {
        left: client.left + 16,
        top: client.top + 16,
        right: client.right - 16,
        bottom: if banner.offer_recreate {
            client.top + button.top
        } else {
            client.bottom - 16
        },
    };
    let mut wide: Vec<u16> = banner
        .lines
        .join("\n")
        .encode_utf16()
        .chain(std::iter::once(0))
        .collect();
    // SAFETY: `hdc` válido; `wide` NUL-terminado; `text_rect` local válida.
    unsafe {
        SetTextColor(hdc, colors.text_primary);
        DrawTextW(hdc, &mut wide, &mut text_rect, DT_WORDBREAK | DT_NOPREFIX);
    }

    if banner.offer_recreate {
        let mut button_rect = RECT {
            left: client.left + button.left,
            top: client.top + button.top,
            right: client.left + button.right,
            bottom: client.top + button.bottom,
        };
        let mut label: Vec<u16> = "Recriar histórico"
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `hdc` válido; `button_rect` dentro dos limites do cliente
        // (garantido por `recreate_button_rect`); `label` NUL-terminado.
        unsafe {
            fill_solid(hdc, &button_rect, colors.selection_bg);
            SetTextColor(hdc, colors.selection_fg);
            DrawTextW(
                hdc,
                &mut label,
                &mut button_rect,
                DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
            );
        }
    }
}

fn paint_toggle_banner(hdc: HDC, client: RECT, t: ToggleUi, colors: &PaintColors) {
    // (US1) Fundo opaco com a superfície do tema — mesmo motivo do banner de
    // erro: um estado de progresso não deve ficar translúcido.
    // SAFETY: `hdc` válido — chamador (`paint`) está dentro do par
    // `BeginPaint`/`EndPaint`.
    unsafe {
        fill_solid(hdc, &client, colors.surface);
    }
    let msg = if t.enable {
        "Aplicando proteção ao banco…"
    } else {
        "Removendo a proteção do banco…"
    };
    let body = format!(
        "{msg}\n\nO histórico fica indisponível até terminar. \
         A janela continua respondendo — você pode fechá-la.\n\nEtapas concluídas: {}",
        t.count
    );
    let mut rect = RECT {
        left: client.left + 16,
        top: client.top + 16,
        right: client.right - 16,
        bottom: client.bottom - 16,
    };
    let mut wide: Vec<u16> = body.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `hdc` válido; `wide` NUL-terminado; `rect` local válida.
    unsafe {
        SetTextColor(hdc, colors.text_primary);
        DrawTextW(hdc, &mut wide, &mut rect, DT_WORDBREAK | DT_NOPREFIX);
    }
}

fn start_encryption_toggle(hwnd: HWND, ctx: &HistoryWindowCtx, enable: bool) {
    if ctx.toggle.get().is_some() {
        return;
    }

    if let Some(reader) = ctx.reader.borrow_mut().take() {
        if let Err(e) = reader.close() {
            log_event!(
                Level::WARN,
                LogFields::new("history_reader_close_failed")
                    .kind(error_banner::store_error_kind(&e))
            );
        }
    }
    ctx.rows.borrow_mut().clear();
    ctx.error.replace(None);
    ctx.toggle.set(Some(ToggleUi { enable, count: 0 }));
    invalidate(hwnd);

    let (prog_tx, prog_rx) = mpsc::channel::<u64>();
    let (done_tx, done_rx) = mpsc::channel::<Result<(), StoreError>>();
    ctx.queue.push(
        WorkItem::ToggleEncryption {
            enable,
            progress: prog_tx,
            done: done_tx,
        },
        0,
    );

    let raw = hwnd.0 as isize;
    let _ = thread::Builder::new()
        .name("duplicata-encrypt-bridge".into())
        .spawn(move || {
            let target = HWND(raw as *mut core::ffi::c_void);
            while let Ok(n) = prog_rx.recv() {
                // SAFETY: `PostMessageW` é thread-safe para qualquer HWND,
                // mesmo já destruído (só devolve erro).
                let _ = unsafe {
                    PostMessageW(
                        Some(target),
                        WM_TOGGLE_PROGRESS,
                        WPARAM(n as usize),
                        LPARAM(0),
                    )
                };
            }
            let code = match done_rx.recv() {
                Ok(Ok(())) => 0usize,
                _ => 1usize,
            };
            // SAFETY: ver acima.
            let _ = unsafe { PostMessageW(Some(target), WM_TOGGLE_DONE, WPARAM(code), LPARAM(0)) };
        });
}

fn on_toggle_progress(hwnd: HWND, ctx: &HistoryWindowCtx, count: u64) {
    if let Some(t) = ctx.toggle.get() {
        ctx.toggle.set(Some(ToggleUi { count, ..t }));
        invalidate(hwnd);
    }
}

fn on_toggle_done(hwnd: HWND, ctx: &HistoryWindowCtx, failed: bool) {
    ctx.toggle.set(None);
    reopen_and_refresh(ctx);
    if failed {
        log_event!(
            Level::WARN,
            LogFields::new("history_encryption_toggle_failed")
        );
        if ctx.error.borrow().is_none() {
            ctx.error.replace(Some(StoreError::Io));
        }
    } else {
        log_event!(
            Level::INFO,
            LogFields::new("history_encryption_toggle_succeeded")
        );
    }
    invalidate(hwnd);
}

fn handle_char(hwnd: HWND, ctx: &HistoryWindowCtx, wparam: WPARAM) {
    let code = wparam.0 as u32;
    match code {
        0x08 => {
            ctx.filter_text.borrow_mut().pop();
        }
        0x7F => {
            ctx.filter_text.borrow_mut().clear();
        }
        c if c < 0x20 => return,
        c => {
            if let Some(ch) = char::from_u32(c) {
                ctx.filter_text.borrow_mut().push(ch);
            } else {
                return;
            }
        }
    }
    refilter(hwnd, ctx);
}

fn handle_keydown(hwnd: HWND, ctx: &HistoryWindowCtx, wparam: WPARAM) {
    let vk = wparam.0 as u16;

    if vk == VK_P.0 && ctrl_down() {
        toggle_pin_selected(hwnd, ctx);
        return;
    }

    if vk == VK_TAB.0 {
        ctx.strip_focused.set(!ctx.strip_focused.get());
        invalidate(hwnd);
        return;
    }

    if vk == VK_LEFT.0 || vk == VK_RIGHT.0 {
        if ctx.strip_focused.get() {
            let next = adjacent_segment(ctx.type_filter.get(), vk == VK_RIGHT.0);
            if next != ctx.type_filter.get() {
                ctx.type_filter.set(next);
                refilter(hwnd, ctx);
            }
        }
        return;
    }
    let nav_key = if vk == VK_UP.0 {
        Some(NavKey::Up)
    } else if vk == VK_DOWN.0 {
        Some(NavKey::Down)
    } else if vk == VK_HOME.0 {
        Some(NavKey::Home)
    } else if vk == VK_END.0 {
        Some(NavKey::End)
    } else if vk == VK_PRIOR.0 {
        Some(NavKey::PageUp)
    } else if vk == VK_NEXT.0 {
        Some(NavKey::PageDown)
    } else {
        None
    };

    if let Some(key) = nav_key {
        let total = visible_indices(ctx).len();
        let visible_rows = visible_row_count(hwnd);
        let new_idx = next_selection_index(ctx.selected.get(), total, key, visible_rows);
        ctx.selected.set(new_idx);

        let scroll = ctx.scroll_offset.get();
        if new_idx < scroll {
            ctx.scroll_offset.set(new_idx);
        } else if new_idx >= scroll + visible_rows {
            ctx.scroll_offset.set(new_idx + 1 - visible_rows);
        }

        invalidate(hwnd);
        return;
    }

    if vk == VK_RETURN.0 {
        if let Some(clip_id) = selected_clip_id(ctx) {
            // Shift+Enter é o controle distinto de "colar texto puro" (US3,
            // Assumption do spec.md — a confirmação padrão continua em Enter
            // puro/duplo clique, FR-014). Indisponível quando o item não tem
            // `has_text`: `confirm_text_only` já trata isso (sem efeito se
            // `stored.text_format` for `None`, FR-019).
            // SAFETY: sem pré-condição — só consulta o estado atual da tecla.
            let shift_down = (unsafe { GetKeyState(VK_SHIFT.0 as i32) } as u16 & 0x8000) != 0;
            if shift_down {
                confirm_text_only(hwnd, ctx, clip_id);
            } else {
                confirm(hwnd, ctx, clip_id);
            }
        }
        return;
    }

    if vk == VK_ESCAPE.0 {
        if !ctx.filter_text.borrow().is_empty() {
            ctx.filter_text.borrow_mut().clear();
            refilter(hwnd, ctx);
        } else {
            hide_window(hwnd, ctx);
        }
    }
}

fn click_y(lparam: LPARAM) -> i32 {
    ((lparam.0 >> 16) & 0xFFFF) as i16 as i32
}

fn click_x(lparam: LPARAM) -> i32 {
    (lparam.0 & 0xFFFF) as i16 as i32
}

enum ListClick {
    Segment(ContentTypeFilter),
    Row { visible_pos: usize, row_idx: usize },
    None,
}

fn list_click_target(hwnd: HWND, ctx: &HistoryWindowCtx, lparam: LPARAM) -> ListClick {
    let x = click_x(lparam);
    let y = click_y(lparam);
    let client = client_rect(hwnd);

    if (0..FILTER_STRIP_PX).contains(&y) {
        return match segment_at_packed(x - client.left, &ctx.segment_rects.get()) {
            Some(seg) => ListClick::Segment(seg),
            None => ListClick::None,
        };
    }
    if y < LIST_TOP_PX {
        return ListClick::None;
    }
    let visible = visible_indices(ctx);
    match row_index_at(
        y - LIST_TOP_PX,
        ROW_HEIGHT_PX,
        ctx.scroll_offset.get(),
        visible.len(),
    ) {
        Some(visible_pos) => ListClick::Row {
            visible_pos,
            row_idx: visible[visible_pos],
        },
        None => ListClick::None,
    }
}

fn handle_single_click(hwnd: HWND, ctx: &HistoryWindowCtx, lparam: LPARAM) {
    if ctx.error.borrow().is_some() {
        handle_banner_click(hwnd, ctx, lparam);
        return;
    }
    match list_click_target(hwnd, ctx, lparam) {
        ListClick::Segment(seg) => {
            ctx.strip_focused.set(true);
            if seg != ctx.type_filter.get() {
                ctx.type_filter.set(seg);
                refilter(hwnd, ctx);
            } else {
                invalidate(hwnd);
            }
        }
        ListClick::Row { visible_pos, .. } => {
            ctx.strip_focused.set(false);
            ctx.selected.set(visible_pos);
            invalidate(hwnd);
        }
        ListClick::None => {}
    }
}

fn handle_banner_click(hwnd: HWND, ctx: &HistoryWindowCtx, lparam: LPARAM) {
    let recreate_allowed = (ctx.may_offer_recreate)();
    let offers_recreate = ctx
        .error
        .borrow()
        .as_ref()
        .map(|e| error_banner::banner_for(e, &ctx.db_path, recreate_allowed).offer_recreate)
        .unwrap_or(false);
    if !offers_recreate {
        return;
    }
    let mut client = RECT::default();
    // SAFETY: `hwnd` válido.
    let _ = unsafe { GetClientRect(hwnd, &mut client) };
    let button =
        error_banner::recreate_button_rect(client.right - client.left, client.bottom - client.top);
    if error_banner::point_in_rect(click_x(lparam), click_y(lparam), &button) {
        trigger_recreate(hwnd, ctx);
    }
}

fn trigger_recreate(hwnd: HWND, ctx: &HistoryWindowCtx) {
    if !(ctx.may_offer_recreate)() {
        log_event!(
            Level::WARN,
            LogFields::new("history_recreate_blocked_protection_active")
        );
        return;
    }
    if !error_banner::confirm_recreate(hwnd) {
        return;
    }

    if let Some(old_reader) = ctx.reader.borrow_mut().take() {
        if let Err(e) = old_reader.close() {
            log_event!(
                Level::WARN,
                LogFields::new("history_reader_close_failed")
                    .kind(error_banner::store_error_kind(&e))
            );
        }
    }

    let (done_tx, done_rx) = mpsc::channel();
    ctx.queue
        .push(WorkItem::RecreateDatabase { done: done_tx }, 0);
    match done_rx.recv() {
        Ok(Ok(())) => {
            log_event!(Level::INFO, LogFields::new("history_recreate_succeeded"));
            reopen_and_refresh(ctx);
        }
        Ok(Err(e)) => {
            log_event!(
                Level::WARN,
                LogFields::new("history_recreate_failed").kind(error_banner::store_error_kind(&e))
            );
            ctx.error.replace(Some(e));
        }
        Err(_) => {
            log_event!(Level::WARN, LogFields::new("history_recreate_failed"));
            reopen_and_refresh(ctx);
        }
    }
    invalidate(hwnd);
}

fn reopen_and_refresh(ctx: &HistoryWindowCtx) {
    match (ctx.reopen_reader)() {
        Ok(new_reader) => {
            *ctx.reader.borrow_mut() = Some(new_reader);
            refresh_rows(ctx);
        }
        Err(e) => {
            log_event!(
                Level::WARN,
                LogFields::new("history_read_failed").kind(error_banner::store_error_kind(&e))
            );
            ctx.error.replace(Some(e));
        }
    }
}

fn handle_double_click(hwnd: HWND, ctx: &HistoryWindowCtx, lparam: LPARAM) {
    if let ListClick::Row { row_idx, .. } = list_click_target(hwnd, ctx, lparam) {
        let clip_id = ctx.rows.borrow().get(row_idx).map(|r| r.id);
        if let Some(clip_id) = clip_id {
            confirm(hwnd, ctx, clip_id);
        }
    }
}

const MENU_ID_PIN: usize = 1;
const MENU_ID_UNPIN: usize = 2;
const MENU_ID_DELETE_ALL: usize = 3;

fn handle_context_menu(hwnd: HWND, ctx: &HistoryWindowCtx, lparam: LPARAM) {
    if ctx.error.borrow().is_some() || ctx.toggle.get().is_some() {
        return;
    }
    let row = match list_click_target(hwnd, ctx, lparam) {
        ListClick::Row {
            visible_pos,
            row_idx,
        } => {
            ctx.selected.set(visible_pos);
            invalidate(hwnd);
            Some(row_idx)
        }
        _ => None,
    };

    // SAFETY: `CreatePopupMenu`/`AppendMenuW`/`TrackPopupMenu`/`DestroyMenu`
    // com um `HMENU` criado e destruído aqui mesmo; `SetForegroundWindow`
    // antes de `TrackPopupMenu` é exigido pela documentação da Microsoft para
    // o menu fechar ao perder o foco (mesmo idiom de `tray.rs`).
    // `TPM_RETURNCMD` devolve o id escolhido de forma síncrona — esta janela
    // não tem handler de `WM_COMMAND`.
    let chosen = unsafe {
        let _ = SetForegroundWindow(hwnd);
        let Ok(menu) = CreatePopupMenu() else {
            return;
        };
        if let Some(idx) = row {
            let (item_id, label) = if ctx.rows.borrow()[idx].pinned {
                (MENU_ID_UNPIN, windows::core::w!("Desafixar"))
            } else {
                (MENU_ID_PIN, windows::core::w!("Fixar"))
            };
            let _ = AppendMenuW(menu, MF_STRING, item_id, label);
            let _ = AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null());
        }
        let _ = AppendMenuW(
            menu,
            MF_STRING,
            MENU_ID_DELETE_ALL,
            windows::core::w!("Apagar todo o histórico…"),
        );
        let mut pt = POINT::default();
        let _ = GetCursorPos(&mut pt);
        let cmd = TrackPopupMenu(
            menu,
            TPM_RETURNCMD | TPM_RIGHTBUTTON | TPM_LEFTALIGN,
            pt.x,
            pt.y,
            Some(0),
            hwnd,
            None,
        );
        let _ = DestroyMenu(menu);
        cmd.0 as usize
    };

    match (chosen, row) {
        (MENU_ID_PIN, Some(idx)) => toggle_pin(hwnd, ctx, idx, true),
        (MENU_ID_UNPIN, Some(idx)) => toggle_pin(hwnd, ctx, idx, false),
        (MENU_ID_DELETE_ALL, _) => trigger_delete_all(hwnd, ctx),
        _ => {}
    }
}

fn toggle_pin(hwnd: HWND, ctx: &HistoryWindowCtx, idx: usize, pin: bool) {
    let Some(clip_id) = ctx.rows.borrow().get(idx).map(|r| r.id) else {
        return;
    };

    let (done_tx, done_rx) = mpsc::channel();
    ctx.queue.push(
        WorkItem::SetPinned {
            clip_id,
            pinned: pin,
            done: done_tx,
        },
        0,
    );

    match done_rx.recv() {
        Ok(Ok(SetPinnedOutcome::Applied)) => {
            if let Some(row) = ctx.rows.borrow_mut().get_mut(idx) {
                row.pinned = pin;
            }
            log_event!(
                Level::DEBUG,
                LogFields::new(if pin { "clip_pinned" } else { "clip_unpinned" }).clip_id(clip_id)
            );
            select_clip(hwnd, ctx, clip_id);
        }
        Ok(Ok(SetPinnedOutcome::PinCapReached { limit, current })) => {
            show_pin_cap_message(hwnd, limit, current);
        }
        Ok(Ok(SetPinnedOutcome::NotFound)) => {
            refresh_rows(ctx);
            invalidate(hwnd);
        }
        Ok(Err(e)) => {
            log_event!(
                Level::WARN,
                LogFields::new("clip_pin_failed").kind(error_banner::store_error_kind(&e))
            );
        }
        Err(_) => {
            log_event!(Level::WARN, LogFields::new("clip_pin_failed"));
        }
    }
}

fn ctrl_down() -> bool {
    // SAFETY: sem pré-condição — só consulta o estado atual da tecla.
    (unsafe { GetKeyState(VK_CONTROL.0 as i32) } as u16 & 0x8000) != 0
}

fn toggle_pin_selected(hwnd: HWND, ctx: &HistoryWindowCtx) {
    let visible = visible_indices(ctx);
    let Some(&row_idx) = visible.get(ctx.selected.get()) else {
        return;
    };
    let Some(pinned_now) = ctx.rows.borrow().get(row_idx).map(|r| r.pinned) else {
        return;
    };
    toggle_pin(hwnd, ctx, row_idx, !pinned_now);
}

fn show_pin_cap_message(owner: HWND, limit: u32, current: u32) {
    let title = HSTRING::from("Fixar item — duplicata");
    let body = HSTRING::from(format!(
        "Você já tem {current} {} fixado{}, e o teto configurado é {limit}.\n\n\
         Desafixe algum item, ou aumente o teto em Configurações, para fixar mais.",
        if current == 1 { "item" } else { "itens" },
        if current == 1 { "" } else { "s" },
    ));
    // SAFETY: `MessageBoxW` com strings wide NUL-terminadas (via `HSTRING`);
    // `owner` é a janela de histórico, visível quando este caminho ocorre.
    unsafe {
        MessageBoxW(Some(owner), &body, &title, MB_OK | MB_ICONINFORMATION);
    }
}

fn trigger_delete_all(hwnd: HWND, ctx: &HistoryWindowCtx) {
    let pinned = ctx
        .reader
        .borrow()
        .as_ref()
        .and_then(|r| r.count_pinned().ok())
        .unwrap_or(0);

    let title = HSTRING::from("Apagar todo o histórico — duplicata");
    let keep_pinned = if pinned == 0 {
        let body = HSTRING::from(
            "Isso apaga TODO o histórico — todos os itens copiados até agora. \
             Esta ação NÃO PODE ser desfeita.\n\nDeseja continuar?",
        );
        // SAFETY: strings wide NUL-terminadas (via `HSTRING`); `hwnd` é esta
        // janela, visível. Default = "Sim" (`MB_DEFBUTTON1`, implícito) — ver
        // doc da função: o comando de emergência honra o próprio nome para
        // quem confirma com pressa.
        let r = unsafe { MessageBoxW(Some(hwnd), &body, &title, MB_YESNO | MB_ICONWARNING) };
        if r != IDYES {
            return;
        }
        false
    } else {
        let body = HSTRING::from(format!(
            "Isso apaga o histórico. Você tem {pinned} {} fixado{}. \
             Esta ação NÃO PODE ser desfeita.\n\n\
             • Sim — apaga TUDO, inclusive os {pinned} fixados.\n\
             • Não — apaga só os não fixados; preserva os {pinned} fixados.\n\
             • Cancelar — não apaga nada.",
            if pinned == 1 { "item" } else { "itens" },
            if pinned == 1 { "" } else { "s" },
        ));
        // SAFETY: ver acima. Default = "Sim" (`MB_DEFBUTTON1`, implícito) —
        // "incluir itens fixados" MARCADA por padrão (FR-015a).
        let r = unsafe { MessageBoxW(Some(hwnd), &body, &title, MB_YESNOCANCEL | MB_ICONWARNING) };
        if r == IDYES {
            false
        } else if r == IDNO {
            true
        } else {
            return;
        }
    };

    let (done_tx, done_rx) = mpsc::channel();
    ctx.queue.push(
        WorkItem::DeleteAll {
            keep_pinned,
            done: done_tx,
        },
        0,
    );
    let failed_kind = match done_rx.recv() {
        Ok(Ok(removed)) => {
            log_event!(
                Level::INFO,
                LogFields::new("history_delete_all_succeeded").byte_len(removed)
            );
            refresh_rows(ctx);
            invalidate(hwnd);
            return;
        }
        Ok(Err(e)) => error_banner::store_error_kind(&e),
        Err(_) => "no_response",
    };
    log_event!(
        Level::WARN,
        LogFields::new("history_delete_all_failed").kind(failed_kind)
    );
    let body = HSTRING::from(
        "Não foi possível apagar o histórico. Nada foi removido. \
         Tente de novo; se persistir, consulte o log.",
    );
    // SAFETY: strings wide NUL-terminadas (via `HSTRING`); `hwnd` visível.
    unsafe {
        MessageBoxW(Some(hwnd), &body, &title, MB_OK | MB_ICONWARNING);
    }
}

fn ctx_ref(hwnd: HWND) -> Option<&'static HistoryWindowCtx> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut HistoryWindowCtx` posto no
    // create; a janela e o contexto vivem enquanto o `HistoryWindow` existir.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const HistoryWindowCtx;
    // SAFETY: `ptr` ou é nulo (janela ainda sem contexto) ou aponta para o
    // `HistoryWindowCtx` posto acima — `as_ref()` trata nulo como `None`.
    unsafe { ptr.as_ref() }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o nosso `*mut HistoryWindowCtx` (mesmo
            // padrão do listener de clipboard e da bandeja).
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            if let Some(ctx) = ctx_ref(hwnd) {
                reload_appearance(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_THEMECHANGED => {
            if let Some(ctx) = ctx_ref(hwnd) {
                reload_appearance(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_SETTINGCHANGE => {
            if is_immersive_color_set(lparam) {
                if let Some(ctx) = ctx_ref(hwnd) {
                    reload_appearance(hwnd, ctx);
                }
            }
            LRESULT(0)
        }
        WM_DPICHANGED => {
            if let Some(ctx) = ctx_ref(hwnd) {
                // SAFETY: `hwnd` válido; `lparam` de um `WM_DPICHANGED`.
                unsafe { dpi::apply_dpi_changed(hwnd, lparam) };
                reload_appearance(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_HOTKEY if wparam.0 == HOTKEY_ID as usize => {
            if let Some(ctx) = ctx_ref(hwnd) {
                handle_hotkey_toggle(hwnd, ctx);
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
        WM_CHAR => {
            if let Some(ctx) = ctx_ref(hwnd) {
                handle_char(hwnd, ctx, wparam);
            }
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            if let Some(ctx) = ctx_ref(hwnd) {
                handle_single_click(hwnd, ctx, lparam);
            }
            LRESULT(0)
        }
        WM_LBUTTONDBLCLK => {
            if let Some(ctx) = ctx_ref(hwnd) {
                handle_double_click(hwnd, ctx, lparam);
            }
            LRESULT(0)
        }
        WM_RBUTTONUP => {
            if let Some(ctx) = ctx_ref(hwnd) {
                handle_context_menu(hwnd, ctx, lparam);
            }
            LRESULT(0)
        }
        WM_APP_TOGGLE_ENCRYPTION => {
            if let Some(ctx) = ctx_ref(hwnd) {
                start_encryption_toggle(hwnd, ctx, wparam.0 != 0);
            }
            LRESULT(0)
        }
        WM_TOGGLE_PROGRESS => {
            if let Some(ctx) = ctx_ref(hwnd) {
                on_toggle_progress(hwnd, ctx, wparam.0 as u64);
            }
            LRESULT(0)
        }
        WM_TOGGLE_DONE => {
            if let Some(ctx) = ctx_ref(hwnd) {
                on_toggle_done(hwnd, ctx, wparam.0 != 0);
            }
            LRESULT(0)
        }
        WM_ACTIVATE => {
            if (wparam.0 as u32 & 0xFFFF) == WA_INACTIVE && really_lost_foreground(hwnd) {
                if let Some(ctx) = ctx_ref(hwnd) {
                    hide_window(hwnd, ctx);
                }
            }
            LRESULT(0)
        }
        WM_KILLFOCUS => {
            if really_lost_foreground(hwnd) {
                if let Some(ctx) = ctx_ref(hwnd) {
                    hide_window(hwnd, ctx);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            if let Some(ctx) = ctx_ref(hwnd) {
                hide_window(hwnd, ctx);
            }
            LRESULT(0)
        }
        WM_DESTROY => LRESULT(0),
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}

#[cfg(test)]
mod tests {
    use super::{row_label, ROW_HEIGHT_PX, WINDOW_WIDTH};
    use duplicata_core::filter_strip::Rect as CoreRect;
    use duplicata_core::row_layout::{self, RowBadge};
    use duplicata_core::ClipListItem;

    fn item(preview: Option<&str>, kind: &str, pinned: bool) -> ClipListItem {
        ClipListItem {
            id: 1,
            canonical_kind: kind.to_string(),
            preview: preview.map(str::to_string),
            thumbnail: None,
            has_text: preview.is_some(),
            last_activity_ms: 0,
            pinned,
        }
    }

    #[test]
    fn pinned_rows_are_marked_and_unpinned_rows_are_not() {
        assert_eq!(row_label(&item(Some("olá"), "unicode_text", true)), "olá");
        assert_eq!(row_label(&item(Some("olá"), "unicode_text", false)), "olá");

        let row = CoreRect {
            left: 0,
            top: 0,
            right: WINDOW_WIDTH,
            bottom: ROW_HEIGHT_PX,
        };
        let l = row_layout::row_layout(row, 96, None);
        assert!(l.pin_stripe.right > l.pin_stripe.left);
        assert!(l.pin_stripe.right <= l.type_badge.left);
    }

    #[test]
    fn the_no_preview_placeholders_carry_no_pin_prefix_and_the_type_is_a_badge() {
        assert_eq!(row_label(&item(None, "dib", true)), "[imagem — sem prévia]");
        assert_eq!(row_label(&item(None, "hdrop", false)), "[arquivos]");

        assert_eq!(row_layout::badge_of("dib"), RowBadge::Image);
        assert_eq!(row_layout::badge_of("hdrop"), RowBadge::Files);
        assert_eq!(row_layout::badge_of("unicode_text"), RowBadge::Text);
        assert_eq!(row_layout::badge_of("custom"), RowBadge::NoPreview);
    }

    #[test]
    fn a_row_with_a_thumbnail_shows_the_image_instead_of_a_label() {
        let mut it = item(None, "dib", false);
        it.thumbnail = Some(vec![0u8; 4]);
        assert_eq!(row_label(&it), "");

        let (w, h) = row_layout::thumbnail_target(ROW_HEIGHT_PX, 96, 128, 72);
        assert_eq!(h, ROW_HEIGHT_PX - 2 * row_layout::THUMBNAIL_MARGIN_PX);
        assert!(w > h, "uma paisagem 16:9 tem de sair mais larga que alta");
    }
}
