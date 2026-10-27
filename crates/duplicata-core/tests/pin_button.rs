use duplicata_core::delete_button::DELETE_GLYPH;
use duplicata_core::pin_button::{
    pin_glyph, pin_tone, shows_pin_button, PinTone, PINNED_GLYPH, PIN_GLYPH,
};

#[test]
fn the_glyphs_are_pin_and_pinned_fill_from_the_system_icon_font() {
    assert_eq!(PIN_GLYPH, '\u{E718}', "Pin");
    assert_eq!(PINNED_GLYPH, '\u{E842}', "PinnedFill");
    assert_ne!(PIN_GLYPH, DELETE_GLYPH);
}

#[test]
fn a_pinned_item_shows_the_filled_pin_and_an_unpinned_one_the_outline() {
    assert_eq!(pin_glyph(true), PINNED_GLYPH);
    assert_eq!(pin_glyph(false), PIN_GLYPH);
}

/// Linhas (de 0 a 3) que mostram o alfinete, com os fixados dados.
fn shown(selected: usize, hot: Option<usize>, pinned: &[usize]) -> Vec<usize> {
    (0..4)
        .filter(|&pos| shows_pin_button(pos, selected, hot, pinned.contains(&pos)))
        .collect()
}

#[test]
fn with_the_selection_on_a_and_the_mouse_away_only_a_shows_the_outline_pin() {
    // Aceite 1: A, B, C não fixados; seleção em A; mouse fora da lista.
    assert_eq!(shown(0, None, &[]), vec![0]);
}

#[test]
fn with_the_mouse_over_c_a_and_c_show_the_outline_pin() {
    // Aceite 2.
    assert_eq!(shown(0, Some(2), &[]), vec![0, 2]);
}

#[test]
fn a_pinned_row_always_shows_its_pin_even_away_from_mouse_and_selection() {
    // Aceite 3: F fixado (linha 0) e A não fixado selecionado (linha 1).
    assert_eq!(shown(1, None, &[0]), vec![0, 1]);
    assert_eq!(shown(1, Some(3), &[0, 2]), vec![0, 1, 2, 3]);
}

#[test]
fn the_selected_row_under_the_mouse_shows_a_single_pin() {
    assert_eq!(shown(1, Some(1), &[]), vec![1]);
}

#[test]
fn a_pinned_pin_uses_the_accent_and_an_unpinned_one_the_trash_colour() {
    assert_eq!(pin_tone(true, false), PinTone::Accent);
    assert_eq!(pin_tone(false, false), PinTone::Dim);
}

#[test]
fn on_the_selected_row_any_pin_uses_the_selection_text_colour() {
    assert_eq!(pin_tone(true, true), PinTone::SelectionFg);
    assert_eq!(pin_tone(false, true), PinTone::SelectionFg);
}
