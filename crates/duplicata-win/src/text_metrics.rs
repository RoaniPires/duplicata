use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, DeleteDC, GetDC, GetTextExtentPoint32W, ReleaseDC, SelectObject, HDC,
    HFONT, HGDIOBJ,
};

/// # Safety
/// `hdc` MUST ser um DC válido com a fonte de interface selecionada.
pub unsafe fn width_in_dc(hdc: HDC, text: &str) -> i32 {
    let wide: Vec<u16> = text.encode_utf16().collect();
    if wide.is_empty() {
        return 0;
    }
    let mut size = SIZE::default();
    // SAFETY: `wide` vive durante a chamada; `size` é local; `hdc` é válido por
    // pré-condição.
    let ok = unsafe { GetTextExtentPoint32W(hdc, &wide, &mut size) }.as_bool();
    if ok {
        size.cx
    } else {
        0
    }
}

pub fn width_with_font(font: HFONT, text: &str) -> i32 {
    // SAFETY: `GetDC(None)` devolve o DC da tela; `ReleaseDC` é chamado antes
    // de todo retorno, e o `mem_dc` criado a partir dele é destruído aqui
    // mesmo.
    unsafe {
        let screen_dc = GetDC(None);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        ReleaseDC(None, screen_dc);
        if mem_dc.is_invalid() {
            return 0;
        }
        let old = if font.is_invalid() {
            None
        } else {
            Some(SelectObject(mem_dc, HGDIOBJ(font.0)))
        };
        let w = width_in_dc(mem_dc, text);
        if let Some(old) = old {
            SelectObject(mem_dc, old);
        }
        let _ = DeleteDC(mem_dc);
        w
    }
}
