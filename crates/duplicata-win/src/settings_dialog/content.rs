//! Área de conteúdo das Configurações: a parte que rola, com os títulos das
//! seções, os nomes e descrições das opções e a moldura dos campos pintados
//! aqui, os controles filhos e a dica de ferramenta de cada linha.

use duplicata_core::filter_strip::Rect as CoreRect;
use duplicata_core::row_layout::scale_px;
use duplicata_core::settings_layout::{max_scroll, row_at, scroll_to_reveal};
use duplicata_core::settings_text as text;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, ClientToScreen, DrawTextW, EndPaint, FillRect, MapWindowPoints, SelectObject,
    SetBkColor, SetBkMode, SetTextColor, DRAW_TEXT_FORMAT, DT_END_ELLIPSIS, DT_NOPREFIX, DT_RIGHT,
    DT_SINGLELINE, DT_VCENTER, DT_WORDBREAK, HDC, HFONT, HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, HOVER_DEFAULT, WM_MOUSEHOVER, WM_MOUSELEAVE};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    TrackMouseEvent, TME_HOVER, TME_LEAVE, TRACKMOUSEEVENT,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetCursorPos, GetParent, GetScrollInfo, GetWindowRect, LoadCursorW,
    RegisterClassW, ScrollWindowEx, SendMessageW, IDC_ARROW, MSG, SB_BOTTOM, SB_LINEDOWN,
    SB_LINEUP, SB_PAGEDOWN, SB_PAGEUP, SB_THUMBPOSITION, SB_THUMBTRACK, SB_TOP, SB_VERT,
    SCROLLINFO, SIF_DISABLENOSCROLL, SIF_PAGE, SIF_POS, SIF_RANGE, SIF_TRACKPOS, SW_ERASE,
    SW_INVALIDATE, SW_SCROLLCHILDREN, WM_COMMAND, WM_CTLCOLOREDIT, WM_CTLCOLORLISTBOX, WM_DRAWITEM,
    WM_ERASEBKGND, WM_MOUSEMOVE, WM_MOUSEWHEEL, WM_PAINT, WM_VSCROLL, WNDCLASSW,
};

use super::{controls, ctx_of, hide_tooltip, win_rect, work_area, SettingsCtx};

pub(super) const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_settings_content");

pub(super) fn register_class() {
    // SAFETY: registro idempotente com `wndproc` válido; o erro de "classe
    // já registrada" é ignorado.
    unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return;
        };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

fn ctx_from_content(hwnd: HWND) -> Option<&'static SettingsCtx> {
    // SAFETY: `GetParent` só consulta; o pai é a janela das Configurações.
    let parent = unsafe { GetParent(hwnd) }.ok()?;
    ctx_of(parent)
}

fn viewport_height(ctx: &SettingsCtx) -> i32 {
    let v = ctx.layout.get().viewport;
    v.bottom - v.top
}

pub(super) fn update_scrollbar(ctx: &SettingsCtx) {
    let l = ctx.layout.get();
    let si = SCROLLINFO {
        cbSize: core::mem::size_of::<SCROLLINFO>() as u32,
        fMask: SIF_RANGE | SIF_PAGE | SIF_POS | SIF_DISABLENOSCROLL,
        nMin: 0,
        nMax: (l.content_height - 1).max(0),
        nPage: viewport_height(ctx).max(0) as u32,
        nPos: ctx.scroll.get(),
        nTrackPos: 0,
    };
    // SAFETY: `content` é desta janela; `si` local com `cbSize`.
    unsafe {
        windows::Win32::UI::Controls::SetScrollInfo(ctx.controls.get().content, SB_VERT, &si, true);
    }
}

/// Rola o conteúdo (e os controles filhos junto) até `pos`.
pub(super) fn set_scroll(ctx: &SettingsCtx, pos: i32) {
    let max = max_scroll(viewport_height(ctx), ctx.layout.get().content_height);
    let pos = pos.clamp(0, max);
    let old = ctx.scroll.get();
    if pos == old {
        return;
    }
    hide_tooltip(ctx);
    ctx.mouse_at.set(None);
    ctx.scroll.set(pos);
    // SAFETY: `content` é desta janela; desloca a área e os filhos.
    unsafe {
        ScrollWindowEx(
            ctx.controls.get().content,
            0,
            old - pos,
            None,
            None,
            None,
            None,
            SW_SCROLLCHILDREN | SW_INVALIDATE | SW_ERASE,
        );
    }
    update_scrollbar(ctx);
}

pub(super) fn wheel(ctx: &SettingsCtx, wparam: WPARAM) {
    let delta = ((wparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32;
    let line = ctx.look.borrow().line_height;
    // Três linhas por entalhe, como o padrão do Windows.
    set_scroll(ctx, ctx.scroll.get() - delta * 3 * line / 120);
}

fn on_vscroll(ctx: &SettingsCtx, wparam: WPARAM) {
    let line = ctx.look.borrow().line_height;
    let page = viewport_height(ctx);
    let s = ctx.scroll.get();
    let code =
        windows::Win32::UI::WindowsAndMessaging::SCROLLBAR_COMMAND((wparam.0 & 0xFFFF) as i32);
    let target = match code {
        SB_LINEUP => s - 2 * line,
        SB_LINEDOWN => s + 2 * line,
        SB_PAGEUP => s - page,
        SB_PAGEDOWN => s + page,
        SB_TOP => 0,
        SB_BOTTOM => i32::MAX,
        SB_THUMBTRACK | SB_THUMBPOSITION => {
            let mut si = SCROLLINFO {
                cbSize: core::mem::size_of::<SCROLLINFO>() as u32,
                fMask: SIF_TRACKPOS,
                ..Default::default()
            };
            // SAFETY: `content` é desta janela; `si` local com `cbSize`.
            let _ = unsafe { GetScrollInfo(ctx.controls.get().content, SB_VERT, &mut si) };
            si.nTrackPos
        }
        _ => s,
    };
    set_scroll(ctx, target);
}

/// Traz `focus` (filho do conteúdo) para dentro da área visível.
pub(super) fn reveal(ctx: &SettingsCtx, focus: HWND) {
    let content = ctx.controls.get().content;
    let mut r = RECT::default();
    // SAFETY: `focus` é filho de `content`; só lê e converte coordenadas.
    unsafe {
        let _ = GetWindowRect(focus, &mut r);
        let mut pts = [
            POINT {
                x: r.left,
                y: r.top,
            },
            POINT {
                x: r.right,
                y: r.bottom,
            },
        ];
        MapWindowPoints(None, Some(content), &mut pts);
        r.top = pts[0].y;
        r.bottom = pts[1].y;
    }
    let pad = scale_px(8, ctx.look.borrow().dpi);
    let s = ctx.scroll.get();
    let target = scroll_to_reveal(
        viewport_height(ctx),
        ctx.layout.get().content_height,
        s,
        r.top + s - pad,
        r.bottom + s + pad,
    );
    set_scroll(ctx, target);
}

fn track_mouse(hwnd: HWND) {
    let mut tme = TRACKMOUSEEVENT {
        cbSize: core::mem::size_of::<TRACKMOUSEEVENT>() as u32,
        dwFlags: TME_HOVER | TME_LEAVE,
        hwndTrack: hwnd,
        dwHoverTime: HOVER_DEFAULT,
    };
    // SAFETY: `tme` local com `cbSize`; `hwnd` desta thread. Falhar só deixa
    // de mostrar a dica, o lado seguro.
    let _ = unsafe { TrackMouseEvent(&mut tme) };
}

fn row_under(ctx: &SettingsCtx, pt: POINT) -> Option<usize> {
    row_at(&ctx.layout.get(), pt.y + ctx.scroll.get())
}

fn on_move(ctx: &SettingsCtx, track: HWND, pt: POINT) {
    // Movimento repetido sem sair do lugar (a dica que acabou de aparecer
    // gera um): rearmar o hover aqui mostraria a dica de novo em ciclo, com
    // o mouse parado.
    if ctx.mouse_at.replace(Some((track, pt))) == Some((track, pt)) {
        return;
    }
    let row = row_under(ctx, pt);
    if row != ctx.hover_row.get() {
        hide_tooltip(ctx);
        ctx.hover_row.set(row);
    }
    track_mouse(track);
}

fn on_hover(ctx: &SettingsCtx, pt: POINT) {
    let row = row_under(ctx, pt);
    ctx.hover_row.set(row);
    let Some(i) = row else {
        hide_tooltip(ctx);
        return;
    };
    let content = ctx.controls.get().content;
    let r = ctx.layout.get().rows[i].row;
    let s = ctx.scroll.get();
    let mut top_left = POINT {
        x: r.left,
        y: r.top - s,
    };
    let mut bottom_right = POINT {
        x: r.right,
        y: r.bottom - s,
    };
    // SAFETY: `content` é desta janela; só converte os pontos locais.
    unsafe {
        let _ = ClientToScreen(content, &mut top_left);
        let _ = ClientToScreen(content, &mut bottom_right);
    }
    let screen_row = RECT {
        left: top_left.x,
        top: top_left.y,
        right: bottom_right.x,
        bottom: bottom_right.y,
    };
    let (font, dpi) = {
        let look = ctx.look.borrow();
        (look.font, look.dpi)
    };
    let work = work_area(ctx.hwnd.get());
    if let Some(tip) = ctx.tooltip.borrow().as_ref() {
        tip.show(text::OPTIONS[i].tip, font, dpi, screen_row, work);
    }
}

fn on_leave(ctx: &SettingsCtx) {
    ctx.mouse_at.set(None);
    let content = ctx.controls.get().content;
    let mut cursor = POINT::default();
    let mut r = RECT::default();
    // SAFETY: só consulta o cursor e o retângulo da janela.
    let inside = unsafe {
        GetCursorPos(&mut cursor).is_ok()
            && GetWindowRect(content, &mut r).is_ok()
            && cursor.x >= r.left
            && cursor.x < r.right
            && cursor.y >= r.top
            && cursor.y < r.bottom
    };
    // Saiu para um controle filho: o próximo movimento decide a linha.
    if !inside {
        hide_tooltip(ctx);
        ctx.hover_row.set(None);
    }
}

fn point_of(lparam: LPARAM) -> POINT {
    POINT {
        x: (lparam.0 & 0xFFFF) as u16 as i16 as i32,
        y: ((lparam.0 >> 16) & 0xFFFF) as u16 as i16 as i32,
    }
}

/// Movimento, parada e saída do mouse sobre um controle filho contam para a
/// linha embaixo dele.
pub(super) fn forward_mouse(ctx: &SettingsCtx, msg: &MSG) {
    if msg.message == WM_MOUSELEAVE {
        on_leave(ctx);
        return;
    }
    let mut pts = [point_of(msg.lParam)];
    // SAFETY: `msg.hwnd` é filho de `content`; só converte o ponto local.
    unsafe { MapWindowPoints(Some(msg.hwnd), Some(ctx.controls.get().content), &mut pts) };
    if msg.message == WM_MOUSEMOVE {
        on_move(ctx, msg.hwnd, pts[0]);
    } else {
        on_hover(ctx, pts[0]);
    }
}

struct Painter<'a> {
    hdc: HDC,
    painted: &'a mut Vec<String>,
}

impl Painter<'_> {
    fn text(&mut self, s: &str, r: RECT, font: HFONT, color: COLORREF, flags: DRAW_TEXT_FORMAT) {
        self.painted.push(s.to_string());
        // Texto vazio não vai ao `DrawTextW` (o buffer vazio não tem
        // ponteiro válido).
        if s.is_empty() {
            return;
        }
        let mut buf: Vec<u16> = s.encode_utf16().collect();
        let mut rc = r;
        // SAFETY: `hdc` do `BeginPaint`; a fonte anterior volta em seguida.
        unsafe {
            let old = SelectObject(self.hdc, HGDIOBJ(font.0));
            SetTextColor(self.hdc, color);
            DrawTextW(self.hdc, &mut buf, &mut rc, flags | DT_NOPREFIX);
            SelectObject(self.hdc, old);
        }
    }
}

fn paint(hwnd: HWND, ctx: &SettingsCtx) {
    let l = ctx.layout.get();
    let s = ctx.scroll.get();
    let shift = |r: CoreRect| {
        win_rect(CoreRect {
            top: r.top - s,
            bottom: r.bottom - s,
            ..r
        })
    };
    let one_line = DT_SINGLELINE | DT_VCENTER | DT_END_ELLIPSIS;
    let look = ctx.look.borrow();
    let session = ctx.session.borrow();
    let hotkey = ctx.hotkey_view.borrow();
    let mut painted = ctx.painted.borrow_mut();
    painted.clear();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` no `WM_PAINT`; pincéis do `Look`,
    // vivos enquanto ele existir.
    let hdc = unsafe { BeginPaint(hwnd, &mut ps) };
    // SAFETY: `hdc` acabou de ser obtido.
    unsafe {
        FillRect(hdc, &ps.rcPaint, look.surface_brush);
        SetBkMode(hdc, TRANSPARENT);
    }
    let mut p = Painter {
        hdc,
        painted: &mut painted,
    };
    let (primary, dim) = (look.palette.text_primary, look.palette.text_dim);
    for (i, title) in [
        text::SECTION_GENERAL,
        text::SECTION_HISTORY,
        text::SECTION_PRIVACY,
    ]
    .iter()
    .enumerate()
    {
        p.text(
            title,
            shift(l.titles[i]),
            look.title_font,
            primary,
            one_line,
        );
    }
    for (i, row) in l.rows.iter().enumerate() {
        let option = text::OPTIONS[i];
        p.text(option.name, shift(row.name), look.font, primary, one_line);
        match i {
            0 => p.text(
                &hotkey.description,
                shift(row.description),
                look.font,
                dim,
                DT_WORDBREAK | DT_END_ELLIPSIS,
            ),
            1 => p.text(
                session.startup_description(),
                shift(row.description),
                look.font,
                dim,
                one_line,
            ),
            _ => p.text(
                option.description,
                shift(row.description),
                look.font,
                dim,
                one_line,
            ),
        }
    }
    p.text(
        &hotkey.combo,
        shift(l.hotkey_text),
        look.font,
        primary,
        one_line | DT_RIGHT,
    );
    p.text(
        text::DAYS_UNIT,
        shift(l.days_label),
        look.font,
        dim,
        one_line,
    );
    for frame in l.numeric_edits.iter().chain([&l.list_edit, &l.list]) {
        // SAFETY: `hdc` do `BeginPaint`.
        unsafe { controls::paint_field_box(hdc, &shift(*frame)) };
    }
    // SAFETY: fecha o par aberto acima.
    unsafe {
        let _ = EndPaint(hwnd, &ps);
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    let ctx = ctx_from_content(hwnd);
    match (msg, ctx) {
        (WM_ERASEBKGND, _) => LRESULT(1),
        (WM_PAINT, Some(ctx)) => {
            paint(hwnd, ctx);
            LRESULT(0)
        }
        (WM_COMMAND, Some(ctx)) => {
            // Os controles avisam o pai direto; quem decide é a janela.
            // SAFETY: janela principal desta thread, entrega síncrona.
            unsafe { SendMessageW(ctx.hwnd.get(), msg, Some(wparam), Some(lparam)) }
        }
        (WM_DRAWITEM, Some(ctx)) => {
            let font = ctx.look.borrow().font;
            // SAFETY: num `WM_DRAWITEM`, `lParam` aponta para o
            // `DRAWITEMSTRUCT` do botão, válido durante o handler.
            unsafe { controls::draw_button(&*(lparam.0 as *const DRAWITEMSTRUCT), font) };
            LRESULT(1)
        }
        (WM_CTLCOLOREDIT | WM_CTLCOLORLISTBOX, Some(ctx)) => {
            let look = ctx.look.borrow();
            let hdc = HDC(wparam.0 as *mut core::ffi::c_void);
            // SAFETY: `wParam` é o DC do controle durante a mensagem.
            unsafe {
                SetTextColor(hdc, look.palette.text_primary);
                SetBkColor(hdc, look.colors.field);
            }
            LRESULT(look.field_brush.0 as isize)
        }
        (WM_VSCROLL, Some(ctx)) => {
            on_vscroll(ctx, wparam);
            LRESULT(0)
        }
        (WM_MOUSEWHEEL, Some(ctx)) => {
            wheel(ctx, wparam);
            LRESULT(0)
        }
        (WM_MOUSEMOVE, Some(ctx)) => {
            on_move(ctx, hwnd, point_of(lparam));
            LRESULT(0)
        }
        (WM_MOUSEHOVER, Some(ctx)) => {
            on_hover(ctx, point_of(lparam));
            LRESULT(0)
        }
        (WM_MOUSELEAVE, Some(ctx)) => {
            on_leave(ctx);
            LRESULT(0)
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
