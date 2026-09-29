//! Popup de pré-visualização do item sob o mouse na janela de histórico.
//!
//! Casca: mede texto e desenha. Quais linhas, onde cortar, tamanho e posição
//! vêm de `duplicata_core::hover_preview`.
//!
//! Postura: janela *owned* pelo histórico (fica acima dele sem topmost),
//! nunca ativada (`WS_EX_NOACTIVATE`, `SWP_NOACTIVATE`, `MA_NOACTIVATE`) — se
//! tomasse o foco, o histórico receberia `WM_KILLFOCUS` e fecharia — e
//! transparente ao mouse (`HTTRANSPARENT`): o cursor sobre o popup continua
//! sendo do histórico, sem `WM_MOUSELEAVE` espúrio. Opaca: sem vidro, o
//! contraste é o do par da paleta. Criada uma vez, só mostrada e escondida.

use std::cell::{Cell, RefCell};

use duplicata_core::filter_strip::Rect as CoreRect;
use duplicata_core::hover_preview::{
    content_width, fit_rows, hover_popup_rect, hover_popup_size, rows_for_height, HoverPreview,
    CONTINUATION, HOVER_PREVIEW_MAX_LINES, HOVER_PREVIEW_MAX_WIDTH_PX, HOVER_PREVIEW_PADDING_PX,
};
use duplicata_core::row_layout::scale_px;
use duplicata_core::InitError;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, DrawTextW, EndPaint,
    FillRect, FrameRect, GetDC, GetTextMetricsW, InvalidateRect, ReleaseDC, SelectObject,
    SetBkMode, SetTextColor, DRAW_TEXT_FORMAT, DT_CALCRECT, DT_EDITCONTROL, DT_EXPANDTABS,
    DT_NOPREFIX, DT_SINGLELINE, DT_WORDBREAK, HDC, HFONT, HGDIOBJ, PAINTSTRUCT, TEXTMETRICW,
    TRANSPARENT,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, GetClientRect, GetWindowLongPtrW,
    IsWindowVisible, RegisterClassW, SetWindowLongPtrW, SetWindowPos, ShowWindow, CREATESTRUCTW,
    GWLP_USERDATA, HTTRANSPARENT, HWND_TOP, MA_NOACTIVATE, SWP_NOACTIVATE, SWP_SHOWWINDOW, SW_HIDE,
    WM_ERASEBKGND, WM_MOUSEACTIVATE, WM_NCCREATE, WM_NCHITTEST, WM_PAINT, WNDCLASSW,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
};

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_hover_preview");

/// Quebra por palavra, e no meio da palavra quando ela sozinha não cabe
/// (URL, hash); tabulação expandida; `&` literal.
const TEXT_FLAGS: DRAW_TEXT_FORMAT =
    DRAW_TEXT_FORMAT(DT_WORDBREAK.0 | DT_EDITCONTROL.0 | DT_EXPANDTABS.0 | DT_NOPREFIX.0);

#[derive(Clone, Copy, Default)]
pub struct PopupColors {
    pub surface: COLORREF,
    pub text: COLORREF,
    pub dim: COLORREF,
    pub border: COLORREF,
}

/// Tudo que o popup precisa saber do histórico para se posicionar e
/// desenhar. Coordenadas de tela.
pub struct ShowRequest {
    /// Emprestada do histórico e guardada até o `hide`. Quem trocar ou
    /// apagar a fonte do histórico precisa esconder o popup antes (hoje:
    /// `reload_appearance` e o `Drop` do histórico).
    pub font: HFONT,
    pub colors: PopupColors,
    pub dpi: u32,
    pub window: CoreRect,
    pub row_top: i32,
    pub work: CoreRect,
}

#[derive(Default)]
struct PopupState {
    /// Linha lógica em UTF-16 (sem NUL) e as linhas visuais concedidas.
    lines: RefCell<Vec<(Vec<u16>, usize)>>,
    truncated: Cell<bool>,
    line_height: Cell<i32>,
    padding: Cell<i32>,
    font: Cell<HFONT>,
    colors: Cell<PopupColors>,
}

pub struct HoverPopup {
    hwnd: HWND,
    state: Box<PopupState>,
}

impl HoverPopup {
    pub fn new(owner: HWND) -> Result<Self, InitError> {
        let mut state = Box::<PopupState>::default();
        // SAFETY: registro de classe idempotente com `wndproc` válido (erro
        // de "classe já registrada" ignorado, como nas demais janelas). Sem
        // WS_VISIBLE: só aparece por `show`. `owner` é a janela de histórico,
        // viva enquanto este popup existir (o `HistoryWindow` derruba o popup
        // antes de destruir a si mesmo).
        unsafe {
            let hinstance = GetModuleHandleW(None).map_err(|_| InitError::HistoryWindow)?;
            let wc = WNDCLASSW {
                lpfnWndProc: Some(wndproc),
                hInstance: hinstance.into(),
                lpszClassName: CLASS_NAME,
                ..Default::default()
            };
            RegisterClassW(&wc);

            let state_ptr: *mut PopupState = state.as_mut();
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
            .map_err(|_| InitError::HistoryWindow)?;
            Ok(HoverPopup { hwnd, state })
        }
    }

    pub fn hwnd(&self) -> HWND {
        self.hwnd
    }

    pub fn is_visible(&self) -> bool {
        // SAFETY: `hwnd` criado por nós e vivo enquanto `self` existir.
        unsafe { IsWindowVisible(self.hwnd) }.as_bool()
    }

    /// Mede, encaixa e exibe sem ativar. Devolve `false` se não havia o que
    /// mostrar — nesse caso o popup continua escondido.
    pub fn show(&self, preview: &HoverPreview, req: &ShowRequest) -> bool {
        let pad = scale_px(HOVER_PREVIEW_PADDING_PX, req.dpi);
        let max_text_w = (scale_px(HOVER_PREVIEW_MAX_WIDTH_PX, req.dpi) - 2 * pad).max(1);

        let wide: Vec<Vec<u16>> = preview
            .lines
            .iter()
            .map(|l| l.encode_utf16().collect())
            .collect();
        let Some(measured) = measure(req.font, &wide, max_text_w) else {
            return false;
        };

        let fit = fit_rows(&measured.rows, preview.source_cut, HOVER_PREVIEW_MAX_LINES);
        if fit.rows_per_line.is_empty() {
            return false;
        }
        let widest = content_width(measured.widest, measured.continuation_w, &fit);
        let size = hover_popup_size(widest, fit.total_rows(), measured.line_height, req.dpi);
        let r = hover_popup_rect(req.window, req.row_top, size, req.work, req.dpi);

        *self.state.lines.borrow_mut() = wide
            .into_iter()
            .zip(fit.rows_per_line.iter().copied())
            .collect();
        self.state.truncated.set(fit.truncated);
        self.state.line_height.set(measured.line_height);
        self.state.padding.set(pad);
        self.state.font.set(req.font);
        self.state.colors.set(req.colors);

        // SAFETY: `hwnd` nosso. `HWND_TOP` entre as não-topmost basta: o
        // popup é owned pelo histórico e fica acima dele. `SWP_NOACTIVATE`
        // mantém o foco onde está.
        unsafe {
            let _ = SetWindowPos(
                self.hwnd,
                Some(HWND_TOP),
                r.left,
                r.top,
                r.right - r.left,
                r.bottom - r.top,
                SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
            let _ = InvalidateRect(Some(self.hwnd), None, false);
        }
        true
    }

    /// Esconde e descarta o texto. Idempotente.
    pub fn hide(&self) {
        if self.is_visible() {
            // SAFETY: `hwnd` nosso.
            unsafe {
                let _ = ShowWindow(self.hwnd, SW_HIDE);
            }
        }
        self.state.lines.borrow_mut().clear();
        self.state.truncated.set(false);
        self.state.font.set(HFONT::default());
    }
}

impl Drop for HoverPopup {
    fn drop(&mut self) {
        // SAFETY: `hwnd` criado por nós; destruído uma vez, antes do estado
        // apontado por `GWLP_USERDATA` sair de escopo.
        unsafe {
            let _ = DestroyWindow(self.hwnd);
        }
    }
}

struct Measured {
    rows: Vec<usize>,
    widest: i32,
    line_height: i32,
    continuation_w: i32,
}

/// Linhas visuais que cada linha ocupa na largura `max_text_w`, a mais larga
/// delas e a altura de linha da fonte. `None` se não houver DC.
fn measure(font: HFONT, lines: &[Vec<u16>], max_text_w: i32) -> Option<Measured> {
    // SAFETY: DC de memória compatível com a tela, criado e destruído aqui;
    // a fonte original é restaurada antes do `DeleteDC`. `DrawTextW` com
    // `DT_CALCRECT` só escreve no `RECT` local.
    unsafe {
        let screen = GetDC(None);
        let dc = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        if dc.is_invalid() {
            return None;
        }
        let old = (!font.is_invalid()).then(|| SelectObject(dc, HGDIOBJ(font.0)));

        let mut tm = TEXTMETRICW::default();
        let line_height = if GetTextMetricsW(dc, &mut tm).as_bool() {
            tm.tmHeight.max(1)
        } else {
            16
        };

        let mut rows = Vec::with_capacity(lines.len());
        let mut widest = 0;
        for line in lines {
            let (w, h) = calc(dc, line, max_text_w, TEXT_FLAGS);
            widest = widest.max(w);
            rows.push(rows_for_height(h, line_height));
        }
        let continuation: Vec<u16> = CONTINUATION.encode_utf16().collect();
        let (continuation_w, _) = calc(dc, &continuation, max_text_w, DT_SINGLELINE | DT_NOPREFIX);

        if let Some(old) = old {
            SelectObject(dc, old);
        }
        let _ = DeleteDC(dc);
        Some(Measured {
            rows,
            widest,
            line_height,
            continuation_w,
        })
    }
}

/// # Safety
/// `dc` válido com a fonte já selecionada.
unsafe fn calc(dc: HDC, text: &[u16], max_w: i32, flags: DRAW_TEXT_FORMAT) -> (i32, i32) {
    if text.is_empty() {
        return (0, 0);
    }
    let mut buf = text.to_vec();
    let mut rc = RECT {
        left: 0,
        top: 0,
        right: max_w,
        bottom: 0,
    };
    // SAFETY: `dc` válido por pré-condição; `buf` e `rc` locais.
    unsafe { DrawTextW(dc, &mut buf, &mut rc, flags | DT_CALCRECT) };
    (rc.right.min(max_w), rc.bottom)
}

fn paint(hwnd: HWND, state: &PopupState) {
    let mut ps = PAINTSTRUCT::default();
    // SAFETY: par `BeginPaint`/`EndPaint` no handler de `WM_PAINT`; brushes
    // criados e liberados aqui; a fonte anterior do DC é restaurada antes do
    // `EndPaint`. A fonte é a do histórico, viva enquanto o popup estiver
    // visível (toda recarga de aparência esconde o popup antes de trocá-la).
    unsafe {
        let hdc = BeginPaint(hwnd, &mut ps);
        let mut client = RECT::default();
        let _ = GetClientRect(hwnd, &mut client);
        let colors = state.colors.get();

        let surface = CreateSolidBrush(colors.surface);
        if !surface.is_invalid() {
            FillRect(hdc, &client, surface);
            let _ = DeleteObject(HGDIOBJ(surface.0));
        }
        let border = CreateSolidBrush(colors.border);
        if !border.is_invalid() {
            FrameRect(hdc, &client, border);
            let _ = DeleteObject(HGDIOBJ(border.0));
        }

        let font = state.font.get();
        let old = (!font.is_invalid()).then(|| SelectObject(hdc, HGDIOBJ(font.0)));
        SetBkMode(hdc, TRANSPARENT);
        SetTextColor(hdc, colors.text);

        let pad = state.padding.get();
        let line_h = state.line_height.get();
        let mut y = client.top + pad;
        for (text, rows) in state.lines.borrow().iter() {
            let h = (*rows as i32) * line_h;
            if !text.is_empty() {
                let mut buf = text.clone();
                // Sem DT_NOCLIP: a última linha, se recebeu menos linhas
                // visuais do que precisava, sai cortada embaixo.
                let mut rc = RECT {
                    left: client.left + pad,
                    top: y,
                    right: client.right - pad,
                    bottom: y + h,
                };
                DrawTextW(hdc, &mut buf, &mut rc, TEXT_FLAGS);
            }
            y += h;
        }
        if state.truncated.get() {
            SetTextColor(hdc, colors.dim);
            let mut buf: Vec<u16> = CONTINUATION.encode_utf16().collect();
            let mut rc = RECT {
                left: client.left + pad,
                top: y,
                right: client.right - pad,
                bottom: y + line_h,
            };
            DrawTextW(hdc, &mut buf, &mut rc, DT_SINGLELINE | DT_NOPREFIX);
        }

        if let Some(old) = old {
            SelectObject(hdc, old);
        }
        let _ = EndPaint(hwnd, &ps);
    }
}

fn state_ref(hwnd: HWND) -> Option<&'static PopupState> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut PopupState` posto no create; o
    // estado vive no `Box` do `HoverPopup`, que destrói a janela antes de
    // soltá-lo.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *const PopupState;
    // SAFETY: nulo (antes do WM_NCCREATE) ou o ponteiro acima.
    unsafe { ptr.as_ref() }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o nosso `*mut PopupState`.
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
            // Sem estado não há o que pintar; o padrão valida a região, sem
            // laço de WM_PAINT.
            // SAFETY: repassa ao handler padrão.
            None => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        },
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
