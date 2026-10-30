use duplicata_core::filter_strip::Rect;
use duplicata_core::row_layout::scale_px;
use duplicata_core::settings_layout::{
    max_scroll, row_at, scroll_to_reveal, settings_layout, LayoutMeasures, SettingsLayout,
};

/// Medidas da fonte de mensagens do Windows (Segoe UI 9 pt) a 96 dpi,
/// escaladas para o DPI pedido.
fn measures(dpi: u32) -> LayoutMeasures {
    LayoutMeasures {
        dpi,
        line_height: scale_px(15, dpi),
        title_height: scale_px(19, dpi),
        widest_label: scale_px(360, dpi),
        hotkey_text_width: scale_px(110, dpi),
        days_width: scale_px(24, dpi),
        numeric_text_width: scale_px(70, dpi),
        button_text_width: scale_px(52, dpi),
        scrollbar_width: scale_px(17, dpi),
        ..Default::default()
    }
}

/// Altura útil de 768 px menos a barra de título e as bordas a 100%.
const CLIENT_AT_768: i32 = 768 - 40;

fn inside(inner: Rect, outer: Rect) -> bool {
    inner.left >= outer.left
        && inner.top >= outer.top
        && inner.right <= outer.right
        && inner.bottom <= outer.bottom
}

fn width(r: Rect) -> i32 {
    r.right - r.left
}

fn layout(dpi: u32, available: i32) -> SettingsLayout {
    settings_layout(&measures(dpi), available)
}

#[test]
fn at_100_percent_the_window_fits_768_px_and_the_content_scrolls() {
    let l = layout(96, CLIENT_AT_768);
    assert!(l.client_height > 0 && l.client_height <= CLIENT_AT_768);
    let viewport_h = l.viewport.bottom - l.viewport.top;
    assert!(
        l.content_height > viewport_h,
        "o conteúdo das três seções é maior que a área visível e rola"
    );
    assert!(max_scroll(viewport_h, l.content_height) > 0);
}

#[test]
fn the_footer_and_the_error_line_sit_below_the_scrolling_area() {
    for dpi in [96, 144, 192] {
        let l = layout(dpi, 2000);
        let client = Rect {
            left: 0,
            top: 0,
            right: l.client_width,
            bottom: l.client_height,
        };
        assert!(l.error_line.top >= l.viewport.bottom, "{dpi} dpi");
        for b in l.footer {
            assert!(b.top >= l.error_line.bottom, "{dpi} dpi");
            assert!(inside(b, client), "rodapé inteiro visível a {dpi} dpi");
        }
        let [save, cancel, apply] = l.footer;
        assert!(save.right <= cancel.left && cancel.right <= apply.left);
    }
}

#[test]
fn higher_dpi_scales_up_and_is_limited_by_the_available_height() {
    let at_96 = layout(96, 2000);
    for dpi in [144, 192] {
        let big = layout(dpi, 2000);
        assert!(big.client_width > at_96.client_width);
        assert!(big.content_height > at_96.content_height);
        let tight = layout(dpi, 700);
        assert!(tight.client_height <= 700, "{dpi} dpi");
        assert_eq!(tight.client_width, big.client_width);
        assert!(tight.footer[0].bottom <= tight.client_height);
    }
}

#[test]
fn the_widest_description_fits_in_one_line() {
    for dpi in [96, 144, 192] {
        let m = measures(dpi);
        let l = settings_layout(&m, 2000);
        for (i, row) in l.rows.iter().enumerate() {
            assert!(
                width(row.description) >= m.widest_label,
                "linha {i} a {dpi} dpi"
            );
            assert!(width(row.name) >= m.widest_label);
            assert_eq!(
                row.description.bottom - row.description.top,
                if i == 0 { 2 } else { 1 } * m.line_height,
                "só o atalho reserva uma segunda linha para as mensagens dele"
            );
            assert!(
                row.description.right <= row.control.left,
                "texto não invade o controle"
            );
        }
    }
}

#[test]
fn every_control_shares_the_same_right_edge() {
    for dpi in [96, 144, 192] {
        let l = layout(dpi, 2000);
        let right = l.rows[0].control.right;
        for row in &l.rows {
            assert_eq!(row.control.right, right, "{dpi} dpi");
        }
        assert_eq!(l.alter_button.right, right);
        assert_eq!(l.days_label.right, right);
        assert_eq!(l.numeric_edits[1].right, right);
        assert_eq!(l.numeric_edits[2].right, right);
        for t in l.toggles {
            assert_eq!(t.right, right);
        }
        assert_eq!(l.list.right, right);
        assert_eq!(l.add_button.right, right);
        assert_eq!(l.remove_button.right, right);
        assert!(right <= l.content_width);
        assert_eq!(
            l.content_width + measures(dpi).scrollbar_width,
            l.client_width
        );
    }
}

#[test]
fn rows_follow_the_visual_order_with_the_list_under_its_row() {
    let l = layout(96, 2000);
    let [general, history, privacy] = l.titles;
    assert!(general.bottom <= l.rows[0].row.top);
    assert!(l.rows[2].row.bottom <= history.top);
    assert!(history.bottom <= l.rows[3].row.top);
    assert!(l.rows[5].row.bottom <= privacy.top);
    assert!(privacy.bottom <= l.rows[6].row.top);
    for pair in l.rows.windows(2) {
        assert!(pair[0].row.bottom <= pair[1].row.top);
    }
    assert!(l.rows[6].row.bottom <= l.list.top);
    assert!(l.list.bottom <= l.list_edit.top);
    assert_eq!(l.list_edit.top, l.add_button.top);
    assert!(l.list_edit.right <= l.add_button.left);
    assert!(l.add_button.bottom <= l.remove_button.top);
    assert!(l.remove_button.bottom <= l.rows[7].row.top);
    assert!(l.rows[8].row.bottom <= l.content_height);
    for (i, row) in l.rows.iter().enumerate() {
        let mid = (row.row.top + row.row.bottom) / 2;
        assert_eq!(row_at(&l, mid), Some(i));
    }
    assert_eq!(row_at(&l, l.list.top + 1), None, "a lista não é a linha");
}

#[test]
fn a_control_below_the_visible_area_scrolls_into_view() {
    assert_eq!(scroll_to_reveal(100, 500, 0, 150, 180), 80);
    assert_eq!(scroll_to_reveal(100, 500, 200, 150, 180), 150);
    assert_eq!(scroll_to_reveal(100, 500, 120, 150, 180), 120, "já visível");
    assert_eq!(
        scroll_to_reveal(100, 500, 0, 480, 520),
        400,
        "não passa do fim"
    );
    assert_eq!(max_scroll(100, 50), 0);
}

fn with_win_v(dpi: u32, status_line: bool, restart_button: bool) -> SettingsLayout {
    let m = LayoutMeasures {
        status_line,
        restart_button,
        restart_text_width: scale_px(210, dpi),
        ..measures(dpi)
    };
    settings_layout(&m, 2000)
}

#[test]
fn the_win_v_option_is_its_own_row_right_under_the_hotkey() {
    for dpi in [96, 144, 192] {
        let l = with_win_v(dpi, false, false);
        assert_eq!(l.rows[0].row.bottom, l.rows[1].row.top, "{dpi} dpi");
        assert_eq!(l.rows[1].control, l.toggles[3]);
        assert!(inside(l.toggles[3], l.rows[1].row));
        assert_eq!(l.toggles[3].right, l.rows[0].control.right);
    }
}

#[test]
fn without_anything_to_say_there_is_no_status_line_nor_button() {
    let l = with_win_v(96, false, false);
    assert_eq!(l.status_line, Rect::default());
    assert_eq!(l.restart_button, Rect::default());
}

#[test]
fn the_status_line_and_the_button_sit_between_the_hotkey_and_the_option() {
    for dpi in [96, 144, 192] {
        let m = measures(dpi);
        let l = with_win_v(dpi, true, true);
        let (status, button) = (l.status_line, l.restart_button);
        assert!(l.rows[0].row.bottom <= status.top, "{dpi} dpi");
        assert_eq!(status.bottom - status.top, 2 * m.line_height);
        assert_eq!(status.left, l.rows[0].name.left);
        assert_eq!(status.right, l.rows[0].control.right);
        assert!(status.bottom <= button.top);
        assert!(button.bottom <= l.rows[1].row.top);
        assert_eq!(button.left, status.left);
        assert!(width(button) >= scale_px(210, dpi), "cabe o texto do botão");
        assert_eq!(row_at(&l, (status.top + status.bottom) / 2), None);
        assert_eq!(row_at(&l, (button.top + button.bottom) / 2), None);

        let only_line = with_win_v(dpi, true, false);
        assert_eq!(only_line.restart_button, Rect::default());
        assert!(only_line.status_line.bottom <= only_line.rows[1].row.top);
        assert!(only_line.content_height < l.content_height);
        assert!(with_win_v(dpi, false, false).content_height < only_line.content_height);
    }
}

#[test]
fn the_error_line_has_room_for_two_lines() {
    for dpi in [96, 144, 192] {
        let m = measures(dpi);
        let l = settings_layout(&m, 2000);
        assert_eq!(
            l.error_line.bottom - l.error_line.top,
            2 * m.line_height,
            "as mensagens do reinício do Explorer são longas ({dpi} dpi)"
        );
    }
}
