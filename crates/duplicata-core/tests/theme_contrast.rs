use duplicata_core::theme::{
    composite_over, contrast_ratio, relative_luminance, theme_colors,
    worst_case_contrast_translucent, Rgb, MIN_HINT_CONTRAST,
};
use duplicata_core::translucency::TRANSLUCENT_ALPHA;

#[test]
fn dim_text_over_translucency_clears_the_floor_against_the_worst_backdrop() {
    for dark in [false, true] {
        let c = theme_colors(dark);
        let ratio = worst_case_contrast_translucent(c.text_dim, c.surface, TRANSLUCENT_ALPHA);
        assert!(
            ratio >= MIN_HINT_CONTRAST,
            "tema {}: text_dim sobre região translúcida dá {ratio:.2}:1 no pior \
             caso (mínimo {MIN_HINT_CONTRAST}) — foi exatamente assim que os \
             segmentos inativos da faixa saíram ilegíveis na validação da Fase 12",
            if dark { "escuro" } else { "claro" }
        );
    }
}

#[test]
fn primary_text_over_translucency_clears_the_floor_too() {
    for dark in [false, true] {
        let c = theme_colors(dark);
        let ratio = worst_case_contrast_translucent(c.text_primary, c.surface, TRANSLUCENT_ALPHA);
        assert!(ratio >= MIN_HINT_CONTRAST, "tema dark={dark}: {ratio:.2}:1");
    }
}

#[test]
fn the_dim_tones_keep_headroom_over_the_floor() {
    for dark in [false, true] {
        let c = theme_colors(dark);
        let ratio = worst_case_contrast_translucent(c.text_dim, c.surface, TRANSLUCENT_ALPHA);
        assert!(
            ratio >= 4.8,
            "tema dark={dark}: {ratio:.2}:1 — sem margem sobre o piso"
        );
    }
}

#[test]
fn at_full_transparency_no_tone_could_ever_clear_the_floor() {
    for dark in [false, true] {
        let c = theme_colors(dark);
        assert_eq!(
            worst_case_contrast_translucent(c.text_dim, c.surface, 0),
            1.0
        );
        assert_eq!(
            worst_case_contrast_translucent(c.text_primary, c.surface, 0),
            1.0
        );
    }
}

#[test]
fn compositing_moves_the_surface_toward_whatever_is_behind() {
    let surface = Rgb::new(0x2B, 0x2B, 0x2B);
    let white = Rgb::new(0xFF, 0xFF, 0xFF);
    assert_eq!(
        composite_over(surface, white, 255),
        surface,
        "opaco não mistura"
    );
    assert_eq!(
        composite_over(surface, white, 0),
        white,
        "alfa 0 é só o fundo"
    );
    let partial = composite_over(surface, white, TRANSLUCENT_ALPHA);
    assert!(
        partial.r > surface.r && partial.r < white.r,
        "o composto fica entre os dois: {partial:?}"
    );
    assert!(
        (partial.r as i32 - surface.r as i32) < (white.r as i32 - partial.r as i32),
        "a superfície tem de pesar mais que o fundo no composto"
    );
}

#[test]
fn the_old_floor_still_holds_for_text_over_opaque_islands() {
    for dark in [false, true] {
        let c = theme_colors(dark);
        let ratio = contrast_ratio(c.text_dim, c.surface);
        assert!(
            ratio >= MIN_HINT_CONTRAST,
            "tema {}: text_dim contra surface dá {ratio:.2}:1, abaixo do piso de \
             {MIN_HINT_CONTRAST}:1 do FR-032a",
            if dark { "escuro" } else { "claro" }
        );
    }
}

#[test]
fn the_primary_text_clears_it_by_a_wide_margin_too() {
    for dark in [false, true] {
        let c = theme_colors(dark);
        assert!(contrast_ratio(c.text_primary, c.surface) >= 7.0);
    }
}

#[test]
fn contrast_ratio_is_symmetric_and_never_below_one() {
    let a = Rgb::new(0x12, 0x34, 0x56);
    let b = Rgb::new(0xEE, 0xDD, 0xCC);
    assert!((contrast_ratio(a, b) - contrast_ratio(b, a)).abs() < 1e-12);
    assert!(contrast_ratio(a, a) >= 1.0);
    assert!((contrast_ratio(a, a) - 1.0).abs() < 1e-12);
}

#[test]
fn black_on_white_is_the_wcag_maximum_of_twenty_one_to_one() {
    let ratio = contrast_ratio(Rgb::new(0, 0, 0), Rgb::new(0xFF, 0xFF, 0xFF));
    assert!((ratio - 21.0).abs() < 0.01, "veio {ratio}");
}

#[test]
fn relative_luminance_matches_the_wcag_reference_points() {
    assert!((relative_luminance(Rgb::new(0, 0, 0))).abs() < 1e-12);
    assert!((relative_luminance(Rgb::new(0xFF, 0xFF, 0xFF)) - 1.0).abs() < 1e-9);
    let dark = relative_luminance(Rgb::new(10, 10, 10));
    assert!(dark > 0.0 && dark < 0.01, "veio {dark}");
}

#[test]
fn the_two_themes_really_are_different_palettes() {
    let light = theme_colors(false);
    let dark = theme_colors(true);
    assert_ne!(light.surface, dark.surface);
    assert_ne!(light.text_dim, dark.text_dim);
    assert!(relative_luminance(light.surface) > relative_luminance(dark.surface));
}
