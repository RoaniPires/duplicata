use duplicata_core::delete_button::{
    shows_delete_button, zone_at, ClickGuard, DoubleClickAction, DownAction, RowZone,
};
use duplicata_core::filter_strip::Rect;
use duplicata_core::row_layout::{
    delete_button_rect, pin_button_rect, scale_px, ROW_HEIGHT_PX, TEXT_GAP_PX,
};

fn shown(selected: usize, hot: Option<usize>) -> Vec<usize> {
    (0..4)
        .filter(|&pos| shows_delete_button(pos, selected, hot))
        .collect()
}

#[test]
fn with_the_selection_on_a_and_the_mouse_over_c_only_a_and_c_show_the_trash() {
    assert_eq!(shown(0, Some(2)), vec![0, 2]);
}

#[test]
fn once_the_mouse_leaves_only_the_selected_row_shows_the_trash() {
    assert_eq!(shown(0, None), vec![0]);
}

#[test]
fn the_selected_row_under_the_mouse_shows_a_single_trash() {
    assert_eq!(shown(1, Some(1)), vec![1]);
}

const DPIS: [u32; 4] = [96, 120, 144, 192];

fn row_at(dpi: u32) -> Rect {
    Rect {
        left: 0,
        top: 40,
        right: scale_px(480, dpi),
        bottom: 40 + ROW_HEIGHT_PX,
    }
}

#[test]
fn the_trash_zone_starts_at_its_left_edge_and_ends_before_the_row_edge() {
    for dpi in DPIS {
        let r = row_at(dpi);
        let b = delete_button_rect(r, dpi);
        let mid_y = (r.top + r.bottom) / 2;
        assert_eq!(
            zone_at(b.left, mid_y, r, dpi),
            RowZone::DeleteButton,
            "dpi={dpi}"
        );
        assert_eq!(
            zone_at(b.right - 1, mid_y, r, dpi),
            RowZone::DeleteButton,
            "dpi={dpi}"
        );
        assert_eq!(
            zone_at(b.left - 1, mid_y, r, dpi),
            RowZone::Body,
            "dpi={dpi}"
        );
        assert_eq!(zone_at(b.right, mid_y, r, dpi), RowZone::Body, "dpi={dpi}");
    }
}

#[test]
fn the_trash_zone_covers_the_whole_row_height_and_nothing_outside_it() {
    let r = row_at(96);
    let b = delete_button_rect(r, 96);
    assert_eq!(zone_at(b.left + 2, r.top, r, 96), RowZone::DeleteButton);
    assert_eq!(
        zone_at(b.left + 2, r.bottom - 1, r, 96),
        RowZone::DeleteButton
    );
    assert_eq!(zone_at(b.left + 2, r.top - 1, r, 96), RowZone::Body);
    assert_eq!(zone_at(b.left + 2, r.bottom, r, 96), RowZone::Body);
}

#[test]
fn the_gap_between_text_and_trash_counts_as_the_row_body() {
    for dpi in DPIS {
        let r = row_at(dpi);
        let b = delete_button_rect(r, dpi);
        let mid_y = (r.top + r.bottom) / 2;
        let gap = scale_px(TEXT_GAP_PX, dpi);
        assert_eq!(
            zone_at(b.left - gap, mid_y, r, dpi),
            RowZone::Body,
            "dpi={dpi}"
        );
        assert_eq!(
            zone_at(pin_button_rect(r, dpi).right + 5, mid_y, r, dpi),
            RowZone::Body,
            "dpi={dpi}"
        );
    }
}

#[test]
fn the_pin_zone_covers_the_left_square_and_nothing_past_it() {
    for dpi in DPIS {
        let r = row_at(dpi);
        let p = pin_button_rect(r, dpi);
        let mid_y = (r.top + r.bottom) / 2;
        assert_eq!(
            zone_at(p.left, mid_y, r, dpi),
            RowZone::PinButton,
            "dpi={dpi}"
        );
        assert_eq!(
            zone_at(p.right - 1, mid_y, r, dpi),
            RowZone::PinButton,
            "dpi={dpi}"
        );
        assert_eq!(zone_at(p.right, mid_y, r, dpi), RowZone::Body, "dpi={dpi}");
        assert_eq!(
            zone_at(p.left - 1, mid_y, r, dpi),
            RowZone::Body,
            "dpi={dpi}"
        );
        assert_eq!(zone_at(p.left + 2, r.top, r, dpi), RowZone::PinButton);
        assert_eq!(
            zone_at(p.left + 2, r.bottom - 1, r, dpi),
            RowZone::PinButton
        );
        assert_eq!(zone_at(p.left + 2, r.top - 1, r, dpi), RowZone::Body);
        assert_eq!(zone_at(p.left + 2, r.bottom, r, dpi), RowZone::Body);
    }
}

#[test]
fn a_down_on_the_pin_toggles() {
    let mut g = ClickGuard::default();
    assert_eq!(g.on_down(RowZone::PinButton), DownAction::TogglePin);
}

#[test]
fn a_double_click_on_the_pin_toggles_exactly_once() {
    // WM_LBUTTONDOWN (alterna), depois WM_LBUTTONDBLCLK no mesmo ponto.
    let mut g = ClickGuard::default();
    let mut toggles = 0;
    if g.on_down(RowZone::PinButton) == DownAction::TogglePin {
        toggles += 1;
    }
    let second = g.on_double_click(RowZone::PinButton);
    assert_eq!(second, DoubleClickAction::Ignore);
    assert_eq!(toggles, 1);
}

#[test]
fn the_second_click_after_a_pin_down_is_ignored_even_over_the_body() {
    let mut g = ClickGuard::default();
    g.on_down(RowZone::PinButton);
    assert_eq!(g.on_double_click(RowZone::Body), DoubleClickAction::Ignore);
    g.on_down(RowZone::PinButton);
    assert_eq!(
        g.on_double_click(RowZone::DeleteButton),
        DoubleClickAction::Ignore
    );
}

#[test]
fn a_double_click_landing_on_the_pin_after_a_body_down_toggles() {
    let mut g = ClickGuard::default();
    g.on_down(RowZone::Body);
    assert_eq!(
        g.on_double_click(RowZone::PinButton),
        DoubleClickAction::TogglePin
    );
}

#[test]
fn a_click_on_the_trash_deletes_and_a_click_on_the_body_selects() {
    let mut g = ClickGuard::default();
    assert_eq!(g.on_down(RowZone::DeleteButton), DownAction::Delete);
    assert_eq!(g.on_down(RowZone::Body), DownAction::Select);
}

#[test]
fn a_double_click_on_the_trash_deletes_only_once() {
    // WM_LBUTTONDOWN (apaga), depois WM_LBUTTONDBLCLK no mesmo ponto.
    let mut g = ClickGuard::default();
    assert_eq!(g.on_down(RowZone::DeleteButton), DownAction::Delete);
    assert_eq!(
        g.on_double_click(RowZone::DeleteButton),
        DoubleClickAction::Ignore
    );
}

#[test]
fn a_double_click_on_the_text_pastes() {
    let mut g = ClickGuard::default();
    assert_eq!(g.on_down(RowZone::Body), DownAction::Select);
    assert_eq!(g.on_double_click(RowZone::Body), DoubleClickAction::Paste);
}

#[test]
fn a_double_click_on_the_trash_after_a_click_on_the_body_deletes() {
    let mut g = ClickGuard::default();
    g.on_down(RowZone::Body);
    assert_eq!(
        g.on_double_click(RowZone::DeleteButton),
        DoubleClickAction::Delete
    );
}

#[test]
fn a_double_click_on_the_body_right_after_a_click_on_the_trash_does_nothing() {
    let mut g = ClickGuard::default();
    g.on_down(RowZone::DeleteButton);
    assert_eq!(g.on_double_click(RowZone::Body), DoubleClickAction::Ignore);
}

#[test]
fn two_double_clicks_in_a_row_on_the_trash_delete_exactly_two_items() {
    let mut g = ClickGuard::default();
    let mut deletes = 0;
    for _ in 0..2 {
        if g.on_down(RowZone::DeleteButton) == DownAction::Delete {
            deletes += 1;
        }
        if g.on_double_click(RowZone::DeleteButton) == DoubleClickAction::Delete {
            deletes += 1;
        }
    }
    assert_eq!(deletes, 2);
}
