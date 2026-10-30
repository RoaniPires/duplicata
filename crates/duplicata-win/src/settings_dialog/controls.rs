//! Controles desenhados pelas Configurações: a alternância no estilo do
//! Windows 11 (janela própria), os botões (`BS_OWNERDRAW`, pintados aqui) e a
//! moldura dos campos e da lista. As cores vêm de [`set_colors`], trocadas a
//! cada mudança de tema.

use std::cell::Cell;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, POINT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreatePen, CreateSolidBrush, DeleteObject, DrawTextW, EndPaint, FillRect,
    GetStockObject, InvalidateRect, PtInRect, RoundRect, SelectObject, SetBkMode, SetTextColor,
    DT_CENTER, DT_NOPREFIX, DT_SINGLELINE, DT_VCENTER, HDC, HFONT, HGDIOBJ, NULL_BRUSH,
    PAINTSTRUCT, PS_SOLID, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::{DRAWITEMSTRUCT, ODS_DISABLED, ODS_FOCUS, ODS_SELECTED};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetCapture, IsWindowEnabled, ReleaseCapture, SetCapture, SetFocus, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DefWindowProcW, GetClientRect, GetDlgCtrlID, GetParent, GetWindowLongPtrW, GetWindowTextW,
    LoadCursorW, RegisterClassW, SendMessageW, SetWindowLongPtrW, BM_CLICK, BM_GETCHECK,
    BM_SETCHECK, BN_CLICKED, GWLP_USERDATA, IDC_HAND, WM_COMMAND, WM_ENABLE, WM_ERASEBKGND,
    WM_KEYDOWN, WM_KEYUP, WM_KILLFOCUS, WM_LBUTTONDOWN, WM_LBUTTONUP, WM_PAINT, WM_SETFOCUS,
    WNDCLASSW,
};

use duplicata_core::row_layout::scale_px;

pub(super) const TOGGLE_CLASS: PCWSTR = windows::core::w!("duplicata_settings_toggle");

#[derive(Clone, Copy, Default)]
pub(crate) struct ControlColors {
    pub surface: COLORREF,
    pub text: COLORREF,
    pub dim: COLORREF,
    pub border: COLORREF,
    /// Fundo de campos, da lista e dos botões.
    pub field: COLORREF,
    /// Fundo do botão enquanto pressionado.
    pub pressed: COLORREF,
    pub accent: COLORREF,
    /// Bolinha sobre o trilho ligado.
    pub on_accent: COLORREF,
    pub dpi: u32,
}

thread_local! {
    static COLORS: Cell<ControlColors> = Cell::new(ControlColors::default());
}

pub(super) fn set_colors(colors: ControlColors) {
    COLORS.with(|c| c.set(colors));
}

pub(crate) fn colors() -> ControlColors {
    COLORS.with(Cell::get)
}

pub(super) fn register_toggle_class() {
    // SAFETY: registro de classe idempotente com `wndproc` válido; o erro de
    // "classe já registrada" é ignorado, como nas demais janelas.
    unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return;
        };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(toggle_wndproc),
            hInstance: hinstance.into(),
            lpszClassName: TOGGLE_CLASS,
            hCursor: LoadCursorW(None, IDC_HAND).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
    }
}

/// # Safety
/// `hdc` válido; os objetos selecionados aqui são restaurados e apagados.
unsafe fn fill_round(hdc: HDC, r: &RECT, radius: i32, fill: COLORREF, edge: COLORREF, pen_w: i32) {
    // SAFETY: contrato da função; caneta e pincel são nossos e voltam a ser
    // desselecionados antes do `DeleteObject`.
    unsafe {
        let brush = CreateSolidBrush(fill);
        let pen = CreatePen(PS_SOLID, pen_w.max(1), edge);
        let old_brush = SelectObject(hdc, HGDIOBJ(brush.0));
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(brush.0));
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// # Safety
/// `hdc` válido.
unsafe fn outline_round(hdc: HDC, r: &RECT, radius: i32, edge: COLORREF, pen_w: i32) {
    // SAFETY: contrato da função; o pincel nulo é de estoque e não se apaga.
    unsafe {
        let pen = CreatePen(PS_SOLID, pen_w.max(1), edge);
        let old_brush = SelectObject(hdc, GetStockObject(NULL_BRUSH));
        let old_pen = SelectObject(hdc, HGDIOBJ(pen.0));
        let _ = RoundRect(hdc, r.left, r.top, r.right, r.bottom, radius, radius);
        SelectObject(hdc, old_brush);
        SelectObject(hdc, old_pen);
        let _ = DeleteObject(HGDIOBJ(pen.0));
    }
}

/// Moldura de um campo de texto ou da lista: fundo de campo e borda.
///
/// # Safety
/// `hdc` válido.
pub(super) unsafe fn paint_field_box(hdc: HDC, r: &RECT) {
    let c = colors();
    let radius = scale_px(6, c.dpi);
    // SAFETY: contrato da função.
    unsafe { fill_round(hdc, r, radius, c.field, c.border, 1) };
}

/// Pinta um botão `BS_OWNERDRAW` a partir do `WM_DRAWITEM`. O foco é
/// sempre desenhado, com ou sem as dicas de teclado do sistema.
///
/// # Safety
/// `dis` é o `DRAWITEMSTRUCT` do `WM_DRAWITEM`, com `hDC` válido; `font` é
/// válida ou inválida (a do DC fica).
pub(crate) unsafe fn draw_button(dis: &DRAWITEMSTRUCT, font: HFONT) {
    // SAFETY: contrato da função.
    unsafe { draw_button_with(dis, font, colors()) }
}

/// [`draw_button`] com cores próprias: a janela de falha do Win+V tem o DPI
/// dela, e as cores das Configurações (de outro DPI) ficam intocadas.
///
/// # Safety
/// O mesmo de [`draw_button`].
pub(crate) unsafe fn draw_button_with(dis: &DRAWITEMSTRUCT, font: HFONT, c: ControlColors) {
    let hdc = dis.hDC;
    let r = dis.rcItem;
    let disabled = dis.itemState.0 & ODS_DISABLED.0 != 0;
    let pressed = dis.itemState.0 & ODS_SELECTED.0 != 0;
    let focused = dis.itemState.0 & ODS_FOCUS.0 != 0;
    let radius = scale_px(8, c.dpi);
    let mut text = [0u16; 64];
    // SAFETY: `hwndItem` é o botão que pediu o desenho; o buffer é local.
    let len = unsafe { GetWindowTextW(dis.hwndItem, &mut text) }.max(0) as usize;
    // SAFETY: `hdc` do `WM_DRAWITEM`, válido durante o handler; a fonte
    // anterior volta antes de sair.
    unsafe {
        let bg = CreateSolidBrush(c.surface);
        FillRect(hdc, &r, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));
        let fill = if pressed { c.pressed } else { c.field };
        fill_round(hdc, &r, radius, fill, c.border, 1);
        if focused {
            let inset = scale_px(2, c.dpi);
            let ring = RECT {
                left: r.left + inset,
                top: r.top + inset,
                right: r.right - inset,
                bottom: r.bottom - inset,
            };
            outline_round(hdc, &ring, radius, c.text, scale_px(2, c.dpi));
        }
        let old = (!font.is_invalid()).then(|| SelectObject(hdc, HGDIOBJ(font.0)));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, if disabled { c.border } else { c.text });
        let mut label = text[..len.min(text.len())].to_vec();
        let mut rc = r;
        DrawTextW(
            hdc,
            &mut label,
            &mut rc,
            DT_CENTER | DT_VCENTER | DT_SINGLELINE | DT_NOPREFIX,
        );
        if let Some(old) = old {
            SelectObject(hdc, old);
        }
    }
}

/// Bits do `GWLP_USERDATA` da alternância.
const CHECKED: isize = 1;
/// Espaço apertado com o foco nela (só o soltar depois disso conta).
const SPACE_DOWN: isize = 2;

fn bits(hwnd: HWND) -> isize {
    // SAFETY: `GWLP_USERDATA` desta janela guarda só os bits acima.
    unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) }
}

fn set_bit(hwnd: HWND, bit: isize, on: bool) {
    let v = if on {
        bits(hwnd) | bit
    } else {
        bits(hwnd) & !bit
    };
    // SAFETY: `hwnd` é uma alternância desta thread; o valor são os bits.
    unsafe {
        SetWindowLongPtrW(hwnd, GWLP_USERDATA, v);
    }
}

fn is_checked(hwnd: HWND) -> bool {
    bits(hwnd) & CHECKED != 0
}

fn set_checked(hwnd: HWND, on: bool) {
    set_bit(hwnd, CHECKED, on);
    // SAFETY: `hwnd` é uma alternância desta thread.
    unsafe {
        let _ = InvalidateRect(Some(hwnd), None, false);
    }
}

fn enabled(hwnd: HWND) -> bool {
    // SAFETY: só consulta.
    unsafe { IsWindowEnabled(hwnd) }.as_bool()
}

/// Inverte o estado e avisa o pai como um botão (`BN_CLICKED`).
fn flip(hwnd: HWND) {
    if !enabled(hwnd) {
        return;
    }
    set_checked(hwnd, !is_checked(hwnd));
    // SAFETY: `hwnd` é filha desta thread; o pai trata o `WM_COMMAND` de
    // forma síncrona e não destrói a alternância no meio.
    unsafe {
        let id = GetDlgCtrlID(hwnd) as usize & 0xFFFF;
        if let Ok(parent) = GetParent(hwnd) {
            SendMessageW(
                parent,
                WM_COMMAND,
                Some(WPARAM(id | ((BN_CLICKED as usize) << 16))),
                Some(LPARAM(hwnd.0 as isize)),
            );
        }
    }
}

fn paint_toggle(hwnd: HWND) {
    let c = colors();
    let on = is_checked(hwnd);
    let active = enabled(hwnd);
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` no `WM_PAINT`; os objetos GDI são
    // criados e apagados em `fill_round`/`outline_round`.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let bg = CreateSolidBrush(c.surface);
        FillRect(hdc, &client, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));

        let focus_pad = scale_px(3, c.dpi);
        let track = RECT {
            left: client.left + focus_pad,
            top: client.top + focus_pad,
            right: client.right - focus_pad,
            bottom: client.bottom - focus_pad,
        };
        let h = track.bottom - track.top;
        let (track_fill, track_edge, thumb) = match (active, on) {
            (true, true) => (c.accent, c.accent, c.on_accent),
            (true, false) => (c.surface, c.dim, c.dim),
            (false, true) => (c.border, c.border, c.surface),
            (false, false) => (c.surface, c.border, c.border),
        };
        fill_round(hdc, &track, h, track_fill, track_edge, 1);
        let knob_pad = (h / 4).max(2);
        let d = h - 2 * knob_pad;
        let left = if on {
            track.right - knob_pad - d
        } else {
            track.left + knob_pad
        };
        let knob = RECT {
            left,
            top: track.top + knob_pad,
            right: left + d,
            bottom: track.top + knob_pad + d,
        };
        fill_round(hdc, &knob, d, thumb, thumb, 1);
        if windows::Win32::UI::Input::KeyboardAndMouse::GetFocus() == hwnd {
            outline_round(hdc, &client, h + focus_pad, c.text, scale_px(2, c.dpi));
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

extern "system" fn toggle_wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            paint_toggle(hwnd);
            LRESULT(0)
        }
        WM_LBUTTONDOWN => {
            // SAFETY: `hwnd` é a alternância; captura até soltar o botão.
            unsafe {
                let _ = SetFocus(Some(hwnd));
                SetCapture(hwnd);
            }
            LRESULT(0)
        }
        WM_LBUTTONUP => {
            let pt = POINT {
                x: (lparam.0 & 0xFFFF) as i16 as i32,
                y: ((lparam.0 >> 16) & 0xFFFF) as i16 as i32,
            };
            let mut client = RECT::default();
            // SAFETY: `hwnd` válido; a captura posta no `WM_LBUTTONDOWN` é solta.
            let inside = unsafe {
                // Sem a captura, o clique não começou aqui: não conta.
                let pressed_here = GetCapture() == hwnd;
                let _ = ReleaseCapture();
                if !pressed_here {
                    return LRESULT(0);
                }
                let _ = GetClientRect(hwnd, &mut client);
                PtInRect(&client, pt).as_bool()
            };
            // Como num botão: vale ao soltar dentro do controle.
            if inside {
                flip(hwnd);
            }
            LRESULT(0)
        }
        WM_KEYDOWN if wparam.0 == VK_SPACE.0 as usize => {
            set_bit(hwnd, SPACE_DOWN, true);
            LRESULT(0)
        }
        WM_KEYUP if wparam.0 == VK_SPACE.0 as usize => {
            if bits(hwnd) & SPACE_DOWN != 0 {
                set_bit(hwnd, SPACE_DOWN, false);
                flip(hwnd);
            }
            LRESULT(0)
        }
        BM_CLICK => {
            flip(hwnd);
            LRESULT(0)
        }
        BM_GETCHECK => LRESULT(is_checked(hwnd) as isize),
        BM_SETCHECK => {
            set_checked(hwnd, wparam.0 != 0);
            LRESULT(0)
        }
        WM_SETFOCUS | WM_KILLFOCUS | WM_ENABLE => {
            set_bit(hwnd, SPACE_DOWN, false);
            // SAFETY: redesenha o foco ou o estado desabilitado.
            unsafe {
                let _ = InvalidateRect(Some(hwnd), None, false);
            }
            LRESULT(0)
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
