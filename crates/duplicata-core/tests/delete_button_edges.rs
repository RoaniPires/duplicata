use duplicata_core::delete_button::{zone_at, RowZone};
use duplicata_core::filter_strip::Rect;
use duplicata_core::list_view::view_after_delete;
use duplicata_core::row_layout::delete_button_rect;

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
