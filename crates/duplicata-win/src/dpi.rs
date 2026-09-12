use windows::Win32::Foundation::{HWND, LPARAM, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MonitorFromPoint, HMONITOR, MONITORINFO, MONITOR_DEFAULTTONEAREST,
};
use windows::Win32::UI::HiDpi::{
    GetDpiForMonitor, SetProcessDpiAwarenessContext, DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    MDT_EFFECTIVE_DPI,
};
use windows::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOSIZE, SWP_NOZORDER,
};

const DEFAULT_DPI: u32 = 96;

/// # Safety
/// Nenhuma pré-condição além de `pt` ser uma coordenada de tela válida.
unsafe fn monitor_at(pt: POINT) -> (RECT, u32) {
    // SAFETY: `MONITOR_DEFAULTTONEAREST` sempre devolve um `HMONITOR` válido.
    let hmon: HMONITOR = unsafe { MonitorFromPoint(pt, MONITOR_DEFAULTTONEAREST) };
    let mut mi = MONITORINFO {
        cbSize: core::mem::size_of::<MONITORINFO>() as u32,
        ..Default::default()
    };
    // SAFETY: `cbSize` preenchido (exigido pela API); a chamada só escreve `mi`.
    let work = if unsafe { GetMonitorInfoW(hmon, &mut mi) }.as_bool() {
        mi.rcWork
    } else {
        RECT {
            left: pt.x - 400,
            top: pt.y - 300,
            right: pt.x + 400,
            bottom: pt.y + 300,
        }
    };
    let mut dx = DEFAULT_DPI;
    let mut dy = DEFAULT_DPI;
    // SAFETY: `hmon` válido; escreve só `dx`/`dy`.
    let _ = unsafe { GetDpiForMonitor(hmon, MDT_EFFECTIVE_DPI, &mut dx, &mut dy) };
    (work, if dx == 0 { DEFAULT_DPI } else { dx })
}

/// # Safety
/// `hwnd` válido.
unsafe fn window_size(hwnd: HWND) -> (i32, i32) {
    let mut wr = RECT::default();
    // SAFETY: `hwnd` válido; escreve só `wr`.
    let _ = unsafe { GetWindowRect(hwnd, &mut wr) };
    (wr.right - wr.left, wr.bottom - wr.top)
}

/// # Safety
/// `hwnd` MUST ser uma janela válida desta thread.
pub unsafe fn place_window_near_cursor(hwnd: HWND) {
    let mut pt = POINT::default();
    // SAFETY: `GetCursorPos` só escreve `pt`.
    if unsafe { GetCursorPos(&mut pt) }.is_err() {
        return;
    }
    // SAFETY: `pt` é uma coordenada de tela válida.
    let (work, dpi) = unsafe { monitor_at(pt) };
    // SAFETY: `hwnd` válido.
    let (w, h) = unsafe { window_size(hwnd) };
    let offset = (8 * dpi as i32 / DEFAULT_DPI as i32).max(1);

    let target = place_near_cursor(pt, work, w, h, offset);
    // SAFETY: `hwnd` válido; move sem redimensionar, sem ativar, sem mexer no
    // z-order.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            target.left,
            target.top,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

/// # Safety
/// `hwnd` válido; `lparam` de uma mensagem `WM_DPICHANGED`.
pub unsafe fn apply_dpi_changed(hwnd: HWND, lparam: LPARAM) {
    if lparam.0 == 0 {
        return;
    }
    // SAFETY: num `WM_DPICHANGED`, `lParam` aponta para um `RECT` sugerido pelo
    // SO, válido durante o handler.
    let suggested = unsafe { *(lparam.0 as *const RECT) };
    let w = suggested.right - suggested.left;
    let h = suggested.bottom - suggested.top;
    let pt = POINT {
        x: suggested.left,
        y: suggested.top,
    };
    // SAFETY: `pt` é uma coordenada de tela válida.
    let (work, _) = unsafe { monitor_at(pt) };
    let target = place_near_cursor(pt, work, w, h, 0);
    // SAFETY: `hwnd` válido; aplica posição + tamanho sugeridos.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            target.left,
            target.top,
            w,
            h,
            SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

pub fn set_process_dpi_awareness_v2() {
    // SAFETY: chamada única no início do processo; o único efeito é fixar o
    // modo de DPI awareness, que é uma propriedade global do processo.
    unsafe {
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }
}

pub fn place_near_cursor(
    cursor: POINT,
    work_area: RECT,
    window_width: i32,
    window_height: i32,
    offset_px: i32,
) -> RECT {
    let work_w = (work_area.right - work_area.left).max(0);
    let work_h = (work_area.bottom - work_area.top).max(0);
    let fit_w = window_width.min(work_w);
    let fit_h = window_height.min(work_h);

    let mut left = cursor.x + offset_px;
    let mut top = cursor.y + offset_px;

    left = left.min(work_area.right - fit_w);
    top = top.min(work_area.bottom - fit_h);
    left = left.max(work_area.left);
    top = top.max(work_area.top);

    RECT {
        left,
        top,
        right: left + window_width,
        bottom: top + window_height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn work() -> RECT {
        RECT {
            left: 0,
            top: 0,
            right: 1920,
            bottom: 1040,
        }
    }

    const W: i32 = 480;
    const H: i32 = 520;
    const OFF: i32 = 8;

    #[test]
    fn cursor_in_the_middle_places_the_window_near_it() {
        let r = place_near_cursor(POINT { x: 900, y: 400 }, work(), W, H, OFF);
        assert_eq!(r.left, 908);
        assert_eq!(r.top, 408);
        assert_eq!(r.right, 908 + W);
        assert_eq!(r.bottom, 408 + H);
    }

    #[test]
    fn cursor_near_the_bottom_right_corner_is_clamped_inside_rcwork() {
        let r = place_near_cursor(POINT { x: 1910, y: 1035 }, work(), W, H, OFF);
        assert_eq!(r.right, 1920);
        assert_eq!(r.bottom, 1040);
        assert_eq!(r.left, 1920 - W);
        assert_eq!(r.top, 1040 - H);
    }

    #[test]
    fn cursor_near_the_top_left_corner_is_clamped_to_the_work_origin() {
        let r = place_near_cursor(POINT { x: -50, y: -50 }, work(), W, H, OFF);
        assert_eq!(r.left, 0);
        assert_eq!(r.top, 0);
    }

    #[test]
    fn works_on_a_secondary_monitor_with_a_non_zero_origin() {
        let secondary = RECT {
            left: 1920,
            top: 0,
            right: 3520,
            bottom: 860,
        };
        let r = place_near_cursor(POINT { x: 3510, y: 850 }, secondary, W, H, OFF);
        assert!(r.left >= 1920 && r.right <= 3520);
        assert!(r.top >= 0 && r.bottom <= 860);
        assert_eq!(r.right, 3520);
        assert_eq!(r.bottom, 860);
    }

    #[test]
    fn a_window_larger_than_the_work_area_pins_to_the_top_left() {
        let tiny = RECT {
            left: 100,
            top: 100,
            right: 400,
            bottom: 300,
        };
        let r = place_near_cursor(POINT { x: 200, y: 200 }, tiny, W, H, OFF);
        assert_eq!(r.left, 100);
        assert_eq!(r.top, 100);
    }
}
