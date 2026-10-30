//! Confirmação do Salvar (mudança irreversível ou do Win+V) e do botão de
//! reiniciar o Explorer.
//!
//! Janela owned pelas Configurações (que ficam desabilitadas enquanto ela
//! está aberta), com as linhas vindas do núcleo, o botão principal ("Salvar"
//! ou "Reiniciar") e "Voltar". O foco começa em "Voltar"; X e Esc também
//! voltam. O resultado chega às Configurações por mensagem, sem laço de
//! mensagens aninhado. Uma linha vazia separa blocos.

use std::cell::RefCell;

use duplicata_core::row_layout::scale_px;
use duplicata_core::settings_text as text;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateFontIndirectW, CreateSolidBrush, DeleteDC, DeleteObject,
    DrawTextW, EndPaint, FillRect, GetObjectW, SelectObject, SetBkMode, SetTextColor, DT_CALCRECT,
    DT_NOPREFIX, DT_WORDBREAK, HFONT, HGDIOBJ, LOGFONTW, PAINTSTRUCT, TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Controls::DRAWITEMSTRUCT;
use windows::Win32::UI::HiDpi::AdjustWindowRectExForDpi;
use windows::Win32::UI::Input::KeyboardAndMouse::{EnableWindow, SetFocus};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetWindow, GetWindowLongPtrW, GetWindowRect,
    LoadCursorW, RegisterClassW, SendMessageW, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    BN_CLICKED, BS_OWNERDRAW, CREATESTRUCTW, GWLP_USERDATA, GW_OWNER, HMENU, IDC_ARROW,
    SWP_NOACTIVATE, SWP_NOZORDER, SW_SHOW, WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLOSE, WM_COMMAND,
    WM_CREATE, WM_DRAWITEM, WM_ERASEBKGND, WM_NCCREATE, WM_NCDESTROY, WM_PAINT, WM_SETFONT,
    WNDCLASSW, WS_CAPTION, WS_CHILD, WS_EX_CONTROLPARENT, WS_EX_DLGMODALFRAME, WS_POPUP,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE,
};

use super::{controls, wide, WM_APP_CONFIRM_RESULT};
use crate::window_material;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_settings_confirm");
const ID_CONFIRM_SAVE: i32 = 301;
/// `IDCANCEL`: o Esc do `IsDialogMessageW` chega como "Voltar".
const ID_BACK: i32 = 2;
const WIDTH_PX: i32 = 440;
const MARGIN_PX: i32 = 20;
const GAP_PX: i32 = 12;
const BUTTON_W_PX: i32 = 96;
const BUTTON_H_PX: i32 = 28;

struct ConfirmState {
    lines: Vec<String>,
    primary: &'static str,
    font: HFONT,
    dpi: u32,
    /// Altura de cada linha na largura útil, medida na criação.
    heights: RefCell<Vec<i32>>,
}

fn style() -> WINDOW_STYLE {
    WS_POPUP | WS_CAPTION | WS_SYSMENU
}

fn ex_style() -> WINDOW_EX_STYLE {
    WS_EX_CONTROLPARENT | WS_EX_DLGMODALFRAME
}

/// Cópia da fonte das Configurações: a troca de tema ou de DPI apaga a
/// delas com a confirmação ainda aberta. Apagada no `WM_NCDESTROY`.
fn own_copy(font: HFONT) -> HFONT {
    let mut lf = LOGFONTW::default();
    // SAFETY: `font` é a fonte viva das Configurações; `GetObjectW` só
    // escreve o `LOGFONTW` local, do tamanho informado.
    let read = unsafe {
        GetObjectW(
            HGDIOBJ(font.0),
            core::mem::size_of::<LOGFONTW>() as i32,
            Some(&mut lf as *mut LOGFONTW as *mut core::ffi::c_void),
        )
    };
    if read == 0 {
        return HFONT::default();
    }
    // SAFETY: `lf` preenchido acima.
    unsafe { CreateFontIndirectW(&lf) }
}

/// Abre centrada sobre `owner`, com uma cópia própria de `font`.
pub(super) fn open(
    owner: HWND,
    lines: Vec<String>,
    font: HFONT,
    dpi: u32,
    primary: &'static str,
) -> Option<HWND> {
    let state = Box::new(ConfirmState {
        lines,
        primary,
        font: own_copy(font),
        dpi,
        heights: RefCell::new(Vec::new()),
    });
    let title = wide(text::CONFIRM_TITLE);
    // SAFETY: registro idempotente com `wndproc` válido; `owner` é a janela
    // das Configurações, viva; o `Box` passa a ser da janela no `WM_NCCREATE`
    // e volta no `WM_NCDESTROY`.
    unsafe {
        let hinstance = GetModuleHandleW(None).ok()?;
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: LoadCursorW(None, IDC_ARROW).unwrap_or_default(),
            ..Default::default()
        };
        RegisterClassW(&wc);
        let ptr = Box::into_raw(state);
        let Ok(hwnd) = CreateWindowExW(
            ex_style(),
            CLASS_NAME,
            windows::core::PCWSTR(title.as_ptr()),
            style(),
            0,
            0,
            0,
            0,
            Some(owner),
            None,
            Some(hinstance.into()),
            Some(ptr.cast()),
        ) else {
            // Como nas Configurações: vazar é o lado seguro, nunca liberar
            // duas vezes o que o `WM_NCDESTROY` já soltou.
            return None;
        };
        place(hwnd, owner);
        let _ = ShowWindow(hwnd, SW_SHOW);
        if let Ok(back) = windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(hwnd), ID_BACK) {
            let _ = SetFocus(Some(back));
        }
        Some(hwnd)
    }
}

pub(super) fn font(hwnd: HWND) -> Option<HFONT> {
    state_ref(hwnd).map(|s| s.font)
}

pub(super) fn lines(hwnd: HWND) -> Vec<String> {
    state_ref(hwnd).map(|s| s.lines.clone()).unwrap_or_default()
}

fn state_ref(hwnd: HWND) -> Option<&'static ConfirmState> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut ConfirmState` posto no
    // `WM_NCCREATE`; válido até o `WM_NCDESTROY`, que o zera e solta.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const ConfirmState;
    // SAFETY: nulo ou o ponteiro acima.
    unsafe { ptr.as_ref() }
}

fn text_width(dpi: u32) -> i32 {
    scale_px(WIDTH_PX - 2 * MARGIN_PX, dpi)
}

fn measure(state: &ConfirmState) -> Vec<i32> {
    let w = text_width(state.dpi);
    // SAFETY: DC de memória criado e destruído aqui; a fonte volta a sair do
    // DC antes do `DeleteDC`; `DT_CALCRECT` só escreve nos `RECT` locais.
    unsafe {
        let dc = CreateCompatibleDC(None);
        let old = SelectObject(dc, HGDIOBJ(state.font.0));
        let heights = state
            .lines
            .iter()
            .map(|line| {
                // A linha vazia só separa blocos: a altura fica nos espaços.
                if line.is_empty() {
                    return 0;
                }
                let mut buf: Vec<u16> = line.encode_utf16().collect();
                let mut rc = RECT {
                    left: 0,
                    top: 0,
                    right: w,
                    bottom: 0,
                };
                DrawTextW(
                    dc,
                    &mut buf,
                    &mut rc,
                    DT_WORDBREAK | DT_NOPREFIX | DT_CALCRECT,
                );
                rc.bottom
            })
            .collect();
        SelectObject(dc, old);
        let _ = DeleteDC(dc);
        heights
    }
}

/// Tamanho pelo texto medido, centrada sobre `owner`; botões à direita.
///
/// # Safety
/// `hwnd` é esta janela, já com os botões criados; `owner` válido.
unsafe fn place(hwnd: HWND, owner: HWND) {
    let Some(state) = state_ref(hwnd) else {
        return;
    };
    let s = |px| scale_px(px, state.dpi);
    let heights = measure(state);
    let text_h: i32 = heights.iter().sum::<i32>() + s(GAP_PX) * (heights.len() as i32 - 1).max(0);
    *state.heights.borrow_mut() = heights;
    let client_w = s(WIDTH_PX);
    let buttons_top = s(MARGIN_PX) + text_h + s(GAP_PX) * 2;
    let client_h = buttons_top + s(BUTTON_H_PX) + s(MARGIN_PX);
    let mut frame = RECT {
        left: 0,
        top: 0,
        right: client_w,
        bottom: client_h,
    };
    let mut o = RECT::default();
    // SAFETY: contrato da função; só cálculos e movimentos de janelas nossas.
    unsafe {
        let _ = AdjustWindowRectExForDpi(&mut frame, style(), false, ex_style(), state.dpi);
        let _ = GetWindowRect(owner, &mut o);
        let (w, h) = (frame.right - frame.left, frame.bottom - frame.top);
        let x = o.left + (o.right - o.left - w) / 2;
        let y = o.top + (o.bottom - o.top - h) / 2;
        let _ = SetWindowPos(hwnd, None, x, y, w, h, SWP_NOZORDER | SWP_NOACTIVATE);
        let (bw, bh) = (s(BUTTON_W_PX), s(BUTTON_H_PX));
        let back_left = client_w - s(MARGIN_PX) - bw;
        let save_left = back_left - s(8) - bw;
        for (id, left) in [(ID_CONFIRM_SAVE, save_left), (ID_BACK, back_left)] {
            if let Ok(b) = windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(hwnd), id) {
                let _ = SetWindowPos(
                    b,
                    None,
                    left,
                    buttons_top,
                    bw,
                    bh,
                    SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
    }
}

/// # Safety
/// Dentro do `WM_CREATE` de `hwnd`.
unsafe fn create_buttons(hwnd: HWND, font: HFONT, primary: &str) {
    // SAFETY: `hwnd` é o pai, no próprio `WM_CREATE`; o Tab segue a ordem
    // de criação (Salvar, Voltar).
    unsafe {
        let hinstance = GetModuleHandleW(None).ok();
        for (id, label) in [(ID_CONFIRM_SAVE, primary), (ID_BACK, text::BUTTON_BACK)] {
            let label = wide(label);
            let b = CreateWindowExW(
                WINDOW_EX_STYLE::default(),
                windows::core::w!("BUTTON"),
                windows::core::PCWSTR(label.as_ptr()),
                WS_CHILD | WS_VISIBLE | WS_TABSTOP | WINDOW_STYLE(BS_OWNERDRAW as u32),
                0,
                0,
                0,
                0,
                Some(hwnd),
                Some(HMENU(id as usize as *mut core::ffi::c_void)),
                hinstance.map(Into::into),
                None,
            )
            .unwrap_or_default();
            SendMessageW(
                b,
                WM_SETFONT,
                Some(WPARAM(font.0 as usize)),
                Some(LPARAM(1)),
            );
        }
    }
}

/// Reabilita as Configurações, fecha e avisa o resultado.
fn finish(hwnd: HWND, confirmed: bool) {
    // SAFETY: `hwnd` é esta janela. O dono é reabilitado antes de ela sumir
    // (assim a ativação volta a ele); depois do `DestroyWindow` só o `HWND`
    // copiado do dono é usado.
    unsafe {
        let owner = GetWindow(hwnd, GW_OWNER).unwrap_or_default();
        let _ = EnableWindow(owner, true);
        let _ = DestroyWindow(hwnd);
        SendMessageW(
            owner,
            WM_APP_CONFIRM_RESULT,
            Some(WPARAM(confirmed as usize)),
            None,
        );
    }
}

fn paint(hwnd: HWND, state: &ConfirmState) {
    let c = controls::colors();
    let s = |px| scale_px(px, state.dpi);
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint`; pincel criado e apagado aqui; a
    // fonte anterior volta antes do `EndPaint`.
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let bg = CreateSolidBrush(c.surface);
        FillRect(hdc, &ps.rcPaint, bg);
        let _ = DeleteObject(HGDIOBJ(bg.0));
        let old = SelectObject(hdc, HGDIOBJ(state.font.0));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, c.text);
        let mut y = s(MARGIN_PX);
        for (line, h) in state.lines.iter().zip(state.heights.borrow().iter()) {
            if line.is_empty() {
                y += s(GAP_PX);
                continue;
            }
            let mut buf: Vec<u16> = line.encode_utf16().collect();
            let mut rc = RECT {
                left: s(MARGIN_PX),
                top: y,
                right: s(MARGIN_PX) + text_width(state.dpi),
                bottom: y + h,
            };
            DrawTextW(hdc, &mut buf, &mut rc, DT_WORDBREAK | DT_NOPREFIX);
            y += h + s(GAP_PX);
        }
        SelectObject(hdc, old);
        let _ = EndPaint(hwnd, &ps);
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o `*mut ConfirmState` de `open()`.
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            let dark = crate::system_appearance::SystemAppearance::read_dark();
            // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
            unsafe {
                let _ = window_material::apply_attributes(hwnd, dark);
                if let Some(state) = state_ref(hwnd) {
                    create_buttons(hwnd, state.font, state.primary);
                }
            }
            LRESULT(0)
        }
        WM_ERASEBKGND => LRESULT(1),
        WM_PAINT => {
            if let Some(state) = state_ref(hwnd) {
                paint(hwnd, state);
            }
            LRESULT(0)
        }
        WM_DRAWITEM => {
            if let Some(state) = state_ref(hwnd) {
                // SAFETY: num `WM_DRAWITEM`, `lParam` aponta para o
                // `DRAWITEMSTRUCT` do botão, válido durante o handler.
                unsafe { controls::draw_button(&*(lparam.0 as *const DRAWITEMSTRUCT), state.font) };
            }
            LRESULT(1)
        }
        WM_COMMAND => {
            let id = (wparam.0 & 0xFFFF) as i32;
            let code = ((wparam.0 >> 16) & 0xFFFF) as u32;
            if code == BN_CLICKED {
                match id {
                    ID_CONFIRM_SAVE => finish(hwnd, true),
                    ID_BACK => finish(hwnd, false),
                    _ => {}
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            finish(hwnd, false);
            LRESULT(0)
        }
        WM_NCDESTROY => {
            // SAFETY: zera e solta o `Box` de `open()`; nada mais o lê.
            let ptr = unsafe { SetWindowLongPtrW(hwnd, GWLP_USERDATA, 0) } as *mut ConfirmState;
            if !ptr.is_null() {
                // SAFETY: `ptr` veio de `Box::into_raw` em `open()`, uma vez.
                let state = unsafe { Box::from_raw(ptr) };
                if !state.font.is_invalid() {
                    // SAFETY: fonte nossa (`own_copy`); os botões, que a usavam,
                    // já foram destruídos antes do `WM_NCDESTROY` do pai.
                    unsafe {
                        let _ = DeleteObject(HGDIOBJ(state.font.0));
                    }
                }
            }
            // SAFETY: repassa ao handler padrão.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
