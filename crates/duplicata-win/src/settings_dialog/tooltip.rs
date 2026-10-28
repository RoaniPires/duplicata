//! Dica de ferramenta das linhas das Configurações.
//!
//! Mesma postura do popup do Histórico (`hover_popup`): janela própria,
//! owned pelas Configurações, nunca ativada (`WS_EX_NOACTIVATE`) e
//! transparente ao mouse (`HTTRANSPARENT`), criada uma vez e só mostrada e
//! escondida. Sem `TOOLTIPS_CLASS` e sem temporizador: quem chama decide
//! quando mostrar a partir do `WM_MOUSEHOVER`.

use std::cell::{Cell, RefCell};

use duplicata_core::row_layout::scale_px;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FillRect, FrameRect, GetDC, InvalidateRect, ReleaseDC, SelectObject, SetBkMode, SetTextColor,
    DT_CALCRECT, DT_NOPREFIX, DT_WORDBREAK, HFONT, HGDIOBJ, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    IsWindowVisible, RegisterClassW, SetWindowLongPtrW, SetWindowPos, ShowWindow, CREATESTRUCTW,
    GWLP_USERDATA, HTTRANSPARENT, HWND_TOP, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

use super::controls;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_settings_tooltip");
const MAX_WIDTH_PX: i32 = 360;
const PADDING_PX: i32 = 8;

#[derive(Default)]
struct TipState {
    text: RefCell<Vec<u16>>,
    font: Cell<HFONT>,
    padding: Cell<i32>,
}

pub(super) struct Tooltip {
    hwnd: HWND,
    state: Box<TipState>,
}

impl Tooltip {
    pub(super) fn new(owner: HWND) -> Option<Self> {
        let mut state = Box::<TipState>::default();
        // SAFETY: registro idempotente com `wndproc` válido; sem WS_VISIBLE.
        // `owner` são as Configurações, que destroem a dica antes de soltar
        // o contexto.
        unsafe {
            let hinstance = GetModuleHandleW(None).ok()?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc);
            let state_ptr: *mut TipState = state.as_mut();
            let hwnd = CreateWindowExW(
                WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                CLASS_NAME,
                PCWSTR::null(),
                WS_POPUP,
                0,
                0,
                0,
                0,
                Some(owner),
                None,
                Some(hinstance.into()),
                Some(state_ptr.cast()),
            )
            .ok()?;
            Some(Tooltip { hwnd, state })
        }
    }

    pub(super) fn is_visible(&self) -> bool {
        // SAFETY: `hwnd` nosso, vivo enquanto `self` existir.
        unsafe { IsWindowVisible(self.hwnd) }.as_bool()
    }

    pub(super) fn text(&self) -> String {
        String::from_utf16_lossy(&self.state.text.borrow())
    }

    /// Logo abaixo de `row` (tela), dentro de `work`. `font` é emprestada das
    /// Configurações até o próximo `hide`.
    pub(super) fn show(&self, text: &str, font: HFONT, dpi: u32, row: RECT, work: RECT) {
        let pad = scale_px(PADDING_PX, dpi);
        let max_w = scale_px(MAX_WIDTH_PX, dpi);
        let wide: Vec<u16> = text.encode_utf16().collect();
        let (w, h) = measure(font, &wide, max_w - 2 * pad);
        let (w, h) = (w + 2 * pad, h + 2 * pad);
        let left = (row.left + scale_px(20, dpi))
            .min(work.right - w)
            .max(work.left);
        let mut top = row.bottom;
        if top + h > work.bottom {
            top = (row.top - h).max(work.top);
        }
        *self.state.text.borrow_mut() = wide;
        self.state.font.set(font);
        self.state.padding.set(pad);
        // SAFETY: `hwnd` nosso; `SWP_NOACTIVATE` mantém o foco onde está.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                left,
                top,
                w,
                h,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
    }

    pub(super) fn hide(&self) {
        if self.is_visible() {
            // SAFETY: `hwnd` nosso.
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
        self.state.text.borrow_mut().clear();
        self.state.font.set(HFONT::default());
    }
}

impl Drop for Tooltip {
    fn drop(&mut self) {
        // SAFETY: `hwnd` nosso; destruído antes de o estado sair de escopo.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

fn measure(font: HFONT, text: &[u16], max_w: i32) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    let mut buf = text.to_vec();
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: max_w.max(1),
        bottom: 0,
    };
    // SAFETY: DC de memória criado e destruído aqui; a fonte anterior volta
    // antes do `DeleteDC`; `DT_CALCRECT` só escreve no `RECT` local.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        if dc.is_invalid() {
            return (max_w, 0);
        }
        let old = (!font.is_invalid()).then(|| SelectObject(dc, HGDIOBJ(font.0)));
        DrawTextW(
            dc,
            &mut buf,
            &mut rc,
            DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
        );
        if let Some(old) = old {
            SelectObject(dc, old);
        }
        let _ = DeleteDC(dc);
    }
    (rc.right.min(max_w), rc.bottom)
}

fn paint(hwnd: HWND, state: &TipState) {
    let c = controls::colors();
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint`; pincéis criados e apagados aqui; a
    // fonte (emprestada, viva enquanto a dica aparece) volta a sair do DC.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let surface = CreateSolidBrush(c.field);
        FillRect(hdc, &client, surface);
        let _ = DeleteObject(HGDIOBJ(surface.0));
        let border = CreateSolidBrush(c.border);
        FrameRect(hdc, &client, border);
        let _ = DeleteObject(HGDIOBJ(border.0));
        let font = state.font.get();
        let old = (!font.is_invalid()).then(|| SelectObject(hdc, HGDIOBJ(font.0)));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, c.text);
        let pad = state.padding.get();
        let mut rc = RECT {
            left: client.left + pad,
            top: client.top + pad,
            right: client.right - pad,
            bottom: client.bottom - pad,
        };
        let mut text = state.text.borrow().clone();
        if !text.is_empty() {
            DrawTextW(hdc, &mut text, &mut rc, DT_WORDBREAK | DT_NOPREFIX);
        }
        if let Some(old) = old {
            SelectObject(hdc, old);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

fn state_ref(hwnd: HWND) -> Option<&'static TipState> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut TipState` posto no create; o
    // estado vive no `Box` do `Tooltip`, que destrói a janela antes.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const TipState;
    // SAFETY: nulo (antes do WM_NCCREATE) ou o ponteiro acima.
    unsafe { ptr.as_ref() }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o nosso `*mut TipState`.
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_NCHITTEST => LRESULT(HTTRANSPARENT as isize),
        WM_MOUSEACTIVATE => LRESULT(MA_NOACTIVATE as isize),
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => match state_ref(hwnd) {
            Some(state) => {
                paint(hwnd, state);
                LRESULT(0)
            }
            // SAFETY: repassa ao handler padrão (valida a região).
            None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        },
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
