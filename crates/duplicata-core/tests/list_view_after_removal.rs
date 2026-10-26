use duplicata_core::list_view::{view_after_delete, view_after_removal};

// Lista ABCD = posições 0..4; página larga o bastante para não rolar.

#[test]
fn deleting_another_row_below_keeps_the_selection_on_the_same_item() {
    // seleção em A, apaga C → A continua na posição 0
    assert_eq!(view_after_delete(Some(0), 0, 0, 3, 10), Some((0, 0)));
}

#[test]
fn deleting_a_row_above_moves_the_selection_up_with_its_item() {
    // seleção em C (2), apaga A → C passa para a posição 1
    assert_eq!(view_after_delete(Some(1), 2, 0, 3, 10), Some((1, 0)));
}

#[test]
fn deleting_the_selected_row_selects_the_item_that_took_its_place() {
    // seleção em B (1), apaga B → C ocupa a posição 1
    assert_eq!(view_after_delete(None, 1, 0, 3, 10), Some((1, 0)));
}

#[test]
fn deleting_the_selected_last_row_selects_the_new_last() {
    // ABC, seleção em C (2), apaga C → B (1)
    assert_eq!(view_after_delete(None, 2, 0, 2, 10), Some((1, 0)));
}

#[test]
fn deleting_the_only_item_leaves_nothing_to_select() {
    assert_eq!(view_after_delete(None, 0, 0, 0, 10), None);
}

#[test]
fn the_kept_selection_stays_inside_the_page_after_the_scroll_backs_off() {
    // 20 visíveis, página 5, topo 15; apagou uma linha acima da seleção (18)
    // → a seleção vai para 17 e o topo recua para 14 para a página caber.
    assert_eq!(view_after_delete(Some(17), 18, 15, 19, 5), Some((17, 14)));
}

#[test]
fn without_a_kept_position_it_matches_view_after_removal() {
    for len in 0..12usize {
        for sel in 0..14usize {
            for scroll in 0..14usize {
                assert_eq!(
                    view_after_delete(None, sel, scroll, len, 4),
                    view_after_removal(sel, scroll, len, 4)
                );
            }
        }
    }
}

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
