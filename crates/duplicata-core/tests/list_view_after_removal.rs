use duplicata_core::list_view::view_after_removal;

#[test]
fn removing_from_the_middle_keeps_the_same_position_selected() {
    assert_eq!(view_after_removal(3, 0, 9, 5), Some((3, 0)));
}

#[test]
fn removing_the_last_item_selects_the_new_last() {
    assert_eq!(view_after_removal(4, 0, 4, 5), Some((3, 0)));
}

#[test]
fn removing_the_only_item_leaves_nothing_selected() {
    assert_eq!(view_after_removal(0, 0, 0, 5), None);
}

#[test]
fn the_scroll_the_user_left_is_preserved_when_it_still_fits() {
    assert_eq!(view_after_removal(12, 10, 29, 5), Some((12, 10)));
}

#[test]
fn the_scroll_backs_off_only_enough_to_keep_the_page_full_at_the_end() {
    // 20 visíveis, página de 5, estava no topo 15 vendo 15..20; apagou o
    // último → 19 visíveis, o topo recua para 14 e a seleção vai para 18.
    assert_eq!(view_after_removal(19, 15, 19, 5), Some((18, 14)));
}

#[test]
fn a_short_list_never_scrolls() {
    assert_eq!(view_after_removal(1, 3, 3, 5), Some((1, 0)));
}

#[test]
fn the_selection_is_always_inside_the_page() {
    for len in 1..30usize {
        for page in 0..8usize {
            for sel in 0..32usize {
                for scroll in 0..32usize {
                    let (s, top) = view_after_removal(sel, scroll, len, page).unwrap();
                    let page = page.max(1);
                    assert!(s < len, "seleção fora da lista");
                    assert!(top <= s && s < top + page, "seleção fora da página");
                    assert!(top + page <= len.max(page), "página passou do fim");
                }
            }
        }
    }
}
