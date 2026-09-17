use duplicata_win::navigation::{
    next_selection_index, row_index_at, scroll_offset_after_wheel, NavKey,
};

#[test]
fn up_and_down_move_by_one() {
    assert_eq!(next_selection_index(5, 10, NavKey::Up, 3), 4);
    assert_eq!(next_selection_index(5, 10, NavKey::Down, 3), 6);
}

#[test]
fn up_at_index_zero_stays_at_zero_no_wrap() {
    assert_eq!(next_selection_index(0, 10, NavKey::Up, 3), 0);
}

#[test]
fn down_at_the_last_index_stays_there_no_wrap() {
    assert_eq!(next_selection_index(9, 10, NavKey::Down, 3), 9);
}

#[test]
fn home_and_end_jump_to_the_edges() {
    assert_eq!(next_selection_index(5, 10, NavKey::Home, 3), 0);
    assert_eq!(next_selection_index(5, 10, NavKey::End, 3), 9);
}

#[test]
fn page_up_and_page_down_move_by_the_page_size() {
    assert_eq!(next_selection_index(5, 20, NavKey::PageUp, 4), 1);
    assert_eq!(next_selection_index(5, 20, NavKey::PageDown, 4), 9);
}

#[test]
fn page_up_past_the_start_clamps_to_zero_no_wrap() {
    assert_eq!(next_selection_index(2, 20, NavKey::PageUp, 4), 0);
}

#[test]
fn page_down_past_the_end_clamps_to_the_last_index_no_wrap() {
    assert_eq!(next_selection_index(18, 20, NavKey::PageDown, 4), 19);
}

#[test]
fn total_zero_always_yields_zero_for_every_key() {
    for key in [
        NavKey::Up,
        NavKey::Down,
        NavKey::Home,
        NavKey::End,
        NavKey::PageUp,
        NavKey::PageDown,
    ] {
        assert_eq!(next_selection_index(0, 0, key, 5), 0, "{key:?}");
    }
}

#[test]
fn page_size_larger_than_total_clamps_like_home_and_end() {
    assert_eq!(next_selection_index(3, 5, NavKey::PageDown, 1000), 4);
    assert_eq!(next_selection_index(3, 5, NavKey::PageUp, 1000), 0);
}

#[test]
fn page_size_zero_still_moves_by_at_least_one() {
    assert_eq!(next_selection_index(5, 10, NavKey::PageDown, 0), 6);
    assert_eq!(next_selection_index(5, 10, NavKey::PageUp, 0), 4);
}

#[test]
fn wheel_scrolls_up_and_down_by_the_given_number_of_lines() {
    assert_eq!(scroll_offset_after_wheel(5, 3, 20, 4), 8);
    assert_eq!(scroll_offset_after_wheel(5, -3, 20, 4), 2);
}

#[test]
fn wheel_up_past_the_top_clamps_to_zero_no_wrap() {
    assert_eq!(scroll_offset_after_wheel(2, -5, 20, 4), 0);
}

#[test]
fn wheel_down_past_the_last_page_clamps_so_the_view_stays_full() {
    assert_eq!(scroll_offset_after_wheel(15, 10, 20, 4), 16);
}

#[test]
fn wheel_with_total_not_exceeding_a_page_never_scrolls() {
    assert_eq!(scroll_offset_after_wheel(0, 5, 3, 10), 0);
}

#[test]
fn wheel_with_zero_lines_does_not_move_the_offset() {
    assert_eq!(scroll_offset_after_wheel(4, 0, 20, 4), 4);
}

#[test]
fn wheel_with_extreme_line_counts_never_panics_and_clamps() {
    assert_eq!(scroll_offset_after_wheel(4, isize::MIN, 20, 4), 0);
    assert_eq!(scroll_offset_after_wheel(4, isize::MAX, 20, 4), 16);
}

#[test]
fn click_on_the_first_visible_row_hits_index_zero_of_the_scroll_offset() {
    assert_eq!(row_index_at(0, 28, 0, 10), Some(0));
    assert_eq!(row_index_at(27, 28, 0, 10), Some(0));
}

#[test]
fn click_on_a_later_row_accounts_for_scroll_offset() {
    assert_eq!(row_index_at(56, 28, 5, 20), Some(7));
}

#[test]
fn click_above_the_client_area_hits_nothing() {
    assert_eq!(row_index_at(-1, 28, 0, 10), None);
}

#[test]
fn click_below_the_last_item_hits_nothing_even_within_a_drawn_row() {
    assert_eq!(row_index_at(84, 28, 0, 3), None);
}

#[test]
fn click_with_zero_total_items_always_hits_nothing() {
    assert_eq!(row_index_at(0, 28, 0, 0), None);
}

#[test]
fn zero_or_negative_row_height_never_panics_and_hits_nothing() {
    assert_eq!(row_index_at(10, 0, 0, 10), None);
    assert_eq!(row_index_at(10, -5, 0, 10), None);
}
