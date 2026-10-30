use duplicata_core::filter_strip::Rect;
use duplicata_core::row_layout::scale_px;
use duplicata_core::win_v_layout::{
    failure_text_width, failure_window_layout, FailureLayout, FailureMeasures,
};

/// Segoe UI 9 pt a 96 dpi, escalada para o DPI pedido.
fn measures(dpi: u32) -> FailureMeasures {
    FailureMeasures {
        dpi,
        line_height: scale_px(15, dpi),
        button_text_width: scale_px(95, dpi),
        text_height: scale_px(45, dpi),
    }
}

fn width(r: Rect) -> i32 {
    r.right - r.left
}

fn layout(dpi: u32) -> FailureLayout {
    failure_window_layout(&measures(dpi))
}

#[test]
fn the_window_scales_with_the_dpi() {
    let at_96 = layout(96);
    assert_eq!(at_96.client_width, 460);
    for dpi in [144, 192] {
        let l = layout(dpi);
        assert_eq!(l.client_width, scale_px(460, dpi), "{dpi} dpi");
        assert!(l.client_height > at_96.client_height, "{dpi} dpi");
        assert!(width(l.buttons[0]) > width(at_96.buttons[0]), "{dpi} dpi");
    }
}

#[test]
fn the_text_sits_on_top_with_the_measured_width_and_height() {
    for dpi in [96, 144, 192] {
        let m = measures(dpi);
        let l = failure_window_layout(&m);
        assert_eq!(width(l.text), failure_text_width(&m), "{dpi} dpi");
        assert_eq!(l.text.bottom - l.text.top, m.text_height);
        assert_eq!(l.text.left, scale_px(20, dpi), "margem");
        assert_eq!(l.text.right, l.client_width - scale_px(20, dpi));
    }
}

#[test]
fn the_buttons_sit_below_the_text_against_the_right_edge() {
    for dpi in [96, 144, 192] {
        let m = measures(dpi);
        let l = failure_window_layout(&m);
        let [left, right] = l.buttons;
        assert!(left.top >= l.text.bottom + scale_px(16, dpi), "{dpi} dpi");
        assert_eq!(left.top, right.top);
        assert_eq!(left.bottom, right.bottom);
        assert_eq!(right.right, l.client_width - scale_px(20, dpi));
        assert_eq!(left.right + scale_px(8, dpi), right.left);
        assert_eq!(width(left), width(right));
        assert!(width(left) >= m.button_text_width + 2 * scale_px(16, dpi));
        assert!(left.bottom - left.top >= m.line_height + scale_px(10, dpi));
        assert_eq!(l.client_height, right.bottom + scale_px(20, dpi));
    }
}

#[test]
fn long_button_texts_widen_the_window_instead_of_overlapping() {
    let m = FailureMeasures {
        button_text_width: 300,
        ..measures(96)
    };
    let l = failure_window_layout(&m);
    let [left, right] = l.buttons;
    assert!(left.left >= 20, "o botão da esquerda não sai da janela");
    assert!(width(left) >= 300 + 32 && width(right) >= 300 + 32);
    assert_eq!(width(l.text), failure_text_width(&m));
    assert!(l.client_width > 460);
}
