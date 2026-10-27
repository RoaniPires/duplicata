use duplicata_core::delete_button::{zone_at, RowZone};
use duplicata_core::filter_strip::Rect;
use duplicata_core::list_view::view_after_delete;
use duplicata_core::row_layout::{delete_button_rect, pin_button_rect};

#[test]
fn a_kept_position_past_the_end_is_pulled_back_to_the_last_row() {
    assert_eq!(view_after_delete(Some(9), 0, 0, 3, 5), Some((2, 0)));
}

#[test]
fn a_kept_position_above_the_scroll_pulls_the_scroll_up_to_it() {
    assert_eq!(view_after_delete(Some(2), 12, 10, 30, 5), Some((2, 2)));
}

#[test]
fn a_kept_position_below_the_page_pulls_the_scroll_down_to_show_it() {
    assert_eq!(view_after_delete(Some(20), 0, 0, 30, 5), Some((20, 16)));
}

#[test]
fn zero_rows_per_page_does_not_panic_and_keeps_the_selection_visible() {
    assert_eq!(view_after_delete(Some(4), 0, 0, 10, 0), Some((4, 4)));
}

#[test]
fn a_row_with_no_height_has_no_trash_zone() {
    let row = Rect {
        left: 0,
        top: 50,
        right: 300,
        bottom: 50,
    };
    let b = delete_button_rect(row, 96);
    assert_eq!((b.right - b.left, b.bottom - b.top), (0, 0));
    for x in [0, 150, 299] {
        assert_eq!(zone_at(x, 50, row, 96), RowZone::Body, "x={x}");
    }
}

#[test]
fn a_row_narrower_than_its_height_gives_the_trash_precedence_over_the_pin() {
    // As duas colunas cobrem a linha inteira; a lixeira, que é o que já
    // existia, ganha onde se sobrepõem.
    let row = Rect {
        left: 10,
        top: 0,
        right: 40,
        bottom: 28,
    };
    let pin = pin_button_rect(row, 96);
    let trash = delete_button_rect(row, 96);
    assert!(pin.right > trash.left, "o teste precisa de sobreposição");
    for x in [12, 25, 39] {
        assert_eq!(zone_at(x, 5, row, 96), RowZone::DeleteButton, "x={x}");
    }
    let wide = Rect {
        left: 10,
        top: 0,
        right: 200,
        bottom: 28,
    };
    assert_eq!(zone_at(12, 5, wide, 96), RowZone::PinButton);
}

#[test]
fn a_row_narrower_than_its_height_gives_the_trash_the_whole_row() {
    let row = Rect {
        left: 10,
        top: 0,
        right: 20,
        bottom: 28,
    };
    let b = delete_button_rect(row, 96);
    assert_eq!((b.left, b.right), (10, 20));
    assert_eq!(zone_at(10, 5, row, 96), RowZone::DeleteButton);
    assert_eq!(zone_at(19, 5, row, 96), RowZone::DeleteButton);
    assert_eq!(zone_at(9, 5, row, 96), RowZone::Body);
}

#[test]
fn the_pin_ignore_is_spent_by_one_double_click_and_the_next_one_acts() {
    use duplicata_core::delete_button::{ClickGuard, DoubleClickAction};
    let mut g = ClickGuard::default();
    g.on_down(RowZone::PinButton);
    assert_eq!(g.on_double_click(RowZone::Body), DoubleClickAction::Ignore);
    // Sem novo down: o duplo clique seguinte volta a agir normalmente.
    assert_eq!(g.on_double_click(RowZone::Body), DoubleClickAction::Paste);
    g.on_down(RowZone::PinButton);
    assert_eq!(
        g.on_double_click(RowZone::PinButton),
        DoubleClickAction::Ignore
    );
    assert_eq!(
        g.on_double_click(RowZone::PinButton),
        DoubleClickAction::TogglePin
    );
}

#[test]
fn the_pin_zone_follows_a_row_that_is_not_at_the_origin_and_stops_at_its_edges() {
    let row = Rect {
        left: 7,
        top: 100,
        right: 300,
        bottom: 128,
    };
    let p = pin_button_rect(row, 96);
    assert_eq!((p.left, p.top, p.bottom), (7, 100, 128));
    assert_eq!(p.right - p.left, 28);
    assert_eq!(zone_at(7, 100, row, 96), RowZone::PinButton);
    assert_eq!(zone_at(p.right - 1, 127, row, 96), RowZone::PinButton);
    assert_eq!(zone_at(p.right, 110, row, 96), RowZone::Body);
    assert_eq!(zone_at(6, 110, row, 96), RowZone::Body);
    assert_eq!(zone_at(10, 99, row, 96), RowZone::Body);
    assert_eq!(zone_at(10, 128, row, 96), RowZone::Body);
}
