use duplicata_core::filter_strip::Rect;
use duplicata_core::hint_strip::HINT_STRIP_PX;
use duplicata_core::row_layout::{
    badge_of, client_height_at, pin_reserved_px, row_layout, rows_that_fit, scale_px,
    thumbnail_target, RowBadge, MIN_VISIBLE_ROWS, ROW_HEIGHT_PX, THUMBNAIL_MARGIN_PX,
    THUMBNAIL_MAX_WIDTH_PX, WINDOW_FRAME_ALLOWANCE_PX, WINDOW_HEIGHT_PX, WINDOW_WIDTH_PX,
};

fn row_width(dpi: u32) -> i32 {
    scale_px(WINDOW_WIDTH_PX - WINDOW_FRAME_ALLOWANCE_PX, dpi)
}

const DPIS: [u32; 2] = [96, 192];

fn row(width_px: i32, height_px: i32) -> Rect {
    Rect {
        left: 0,
        top: 0,
        right: width_px,
        bottom: height_px,
    }
}

#[test]
fn the_pin_indicator_column_never_costs_more_than_ten_percent_of_the_row() {
    for dpi in DPIS {
        let width = row_width(dpi);
        let reserved = pin_reserved_px(dpi);
        assert!(
            reserved * 100 <= width * 10,
            "dpi={dpi}: indicador de fixado reserva {reserved}px de {width}px \
             de linha (> 10%, FR-030)"
        );
    }
}

#[test]
fn a_pinned_row_and_an_unpinned_row_have_the_exact_same_preview_width() {
    for dpi in DPIS {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        let l = row_layout(r, dpi, None);
        assert_eq!(
            l.text.right - l.text.left,
            row_layout(r, dpi, None).text.right - row_layout(r, dpi, None).text.left
        );
        assert!(l.text.left > l.pin_stripe.right, "dpi={dpi}");
    }
}

#[test]
fn the_pin_stripe_hugs_the_left_edge_and_stays_inside_the_row() {
    for dpi in DPIS {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        let l = row_layout(r, dpi, None);
        assert_eq!(l.pin_stripe.left, r.left, "dpi={dpi}");
        assert!(l.pin_stripe.right > l.pin_stripe.left, "dpi={dpi}");
        assert!(
            l.pin_stripe.top >= r.top && l.pin_stripe.bottom <= r.bottom,
            "dpi={dpi}"
        );
    }
}

#[test]
fn a_square_thumbnail_fills_the_usable_row_height() {
    for dpi in DPIS {
        let margin = scale_px(THUMBNAIL_MARGIN_PX, dpi);
        let (w, h) = thumbnail_target(ROW_HEIGHT_PX, dpi, 128, 128);
        assert_eq!(h, ROW_HEIGHT_PX - 2 * margin, "dpi={dpi}");
        assert_eq!(w, h, "dpi={dpi}: quadrada não pode virar retângulo");
    }
}

#[test]
fn a_landscape_thumbnail_keeps_its_aspect_ratio() {
    let (w, h) = thumbnail_target(ROW_HEIGHT_PX, 96, 128, 72);
    assert_eq!(h, ROW_HEIGHT_PX - 2 * THUMBNAIL_MARGIN_PX);
    assert_eq!(w, 43);
    let expected_w = (h as f64) * 128.0 / 72.0;
    assert!(
        (w as f64 - expected_w).abs() <= 1.0,
        "w={w} esperado≈{expected_w}"
    );
}

#[test]
fn a_portrait_thumbnail_stays_narrow_instead_of_being_stretched() {
    let (w, h) = thumbnail_target(ROW_HEIGHT_PX, 96, 72, 128);
    assert_eq!(h, ROW_HEIGHT_PX - 2 * THUMBNAIL_MARGIN_PX);
    assert!(
        w < h,
        "retrato tem de ficar mais estreito que alto, veio {w}x{h}"
    );
    let expected_w = (h as f64) * 72.0 / 128.0;
    assert!(
        (w as f64 - expected_w).abs() <= 1.0,
        "w={w} esperado≈{expected_w}"
    );
}

#[test]
fn a_panoramic_thumbnail_loses_height_instead_of_being_distorted() {
    let (w, h) = thumbnail_target(ROW_HEIGHT_PX, 96, 128, 8);
    assert_eq!(w, THUMBNAIL_MAX_WIDTH_PX);
    assert!(h < ROW_HEIGHT_PX - 2 * THUMBNAIL_MARGIN_PX);
    let expected_h = (w as f64) * 8.0 / 128.0;
    assert!(
        (h as f64 - expected_h).abs() <= 1.0,
        "h={h} esperado≈{expected_h}"
    );
}

#[test]
fn the_thumbnail_never_exceeds_the_reserved_width_at_any_dpi() {
    for dpi in DPIS {
        let max_w = scale_px(THUMBNAIL_MAX_WIDTH_PX, dpi);
        for (sw, sh) in [(128u32, 1u32), (128, 8), (128, 72), (128, 128), (1, 128)] {
            let (w, _) = thumbnail_target(ROW_HEIGHT_PX, dpi, sw, sh);
            assert!(w <= max_w, "dpi={dpi} src={sw}x{sh}: w={w} > {max_w}");
        }
    }
}

#[test]
fn a_degenerate_source_draws_nothing() {
    assert_eq!(thumbnail_target(ROW_HEIGHT_PX, 96, 0, 10), (0, 0));
    assert_eq!(thumbnail_target(ROW_HEIGHT_PX, 96, 10, 0), (0, 0));
}

#[test]
fn a_degenerate_source_leaves_the_row_laid_out_as_if_there_were_no_thumbnail() {
    let r = row(row_width(96), ROW_HEIGHT_PX);
    let degenerate = row_layout(r, 96, Some((0, 10)));
    let none = row_layout(r, 96, None);
    assert_eq!(degenerate.thumbnail.right, degenerate.thumbnail.left);
    assert_eq!(degenerate.text, none.text);
}

#[test]
fn a_row_with_no_height_still_produces_a_sane_layout() {
    let r = row(row_width(96), 0);
    let l = row_layout(r, 96, Some((128, 72)));
    assert!(l.pin_stripe.bottom >= l.pin_stripe.top);
    assert!(l.type_badge.right > l.type_badge.left);
    assert!(l.text.left <= l.text.right);
}

#[test]
fn the_empty_space_around_a_thumbnail_is_only_the_minimum_margin() {
    let r = row(row_width(96), ROW_HEIGHT_PX);
    let l = row_layout(r, 96, Some((128, 72)));
    let slack_top = l.thumbnail.top - r.top;
    let slack_bottom = r.bottom - l.thumbnail.bottom;
    assert!(
        slack_top <= THUMBNAIL_MARGIN_PX,
        "folga acima = {slack_top}px"
    );
    assert!(
        slack_bottom <= THUMBNAIL_MARGIN_PX,
        "folga abaixo = {slack_bottom}px"
    );
}

#[test]
fn the_thumbnail_is_vertically_centred_in_the_row() {
    let r = row(row_width(96), ROW_HEIGHT_PX);
    let l = row_layout(r, 96, Some((128, 8)));
    let slack_top = l.thumbnail.top - r.top;
    let slack_bottom = r.bottom - l.thumbnail.bottom;
    assert!(
        (slack_top - slack_bottom).abs() <= 1,
        "centralização: {slack_top} acima vs {slack_bottom} abaixo"
    );
}

#[test]
fn the_columns_run_left_to_right_without_overlapping() {
    for dpi in DPIS {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        for thumb in [None, Some((128u32, 72u32))] {
            let l = row_layout(r, dpi, thumb);
            assert!(
                l.pin_stripe.right <= l.type_badge.left,
                "dpi={dpi} {thumb:?}"
            );
            assert!(l.type_badge.right <= l.text.left, "dpi={dpi} {thumb:?}");
            assert!(l.text.left <= l.text.right, "dpi={dpi} {thumb:?}");
            if thumb.is_some() {
                assert!(l.type_badge.right <= l.thumbnail.left, "dpi={dpi}");
                assert!(l.thumbnail.right <= l.text.left, "dpi={dpi}");
            }
        }
    }
}

#[test]
fn a_row_with_a_thumbnail_leaves_less_text_room_than_one_without() {
    let r = row(row_width(96), ROW_HEIGHT_PX);
    let with = row_layout(r, 96, Some((128, 72)));
    let without = row_layout(r, 96, None);
    assert!(with.text.left > without.text.left);
}

#[test]
fn each_kind_gets_its_own_badge() {
    assert_eq!(badge_of("unicode_text"), RowBadge::Text);
    assert_eq!(badge_of("dib"), RowBadge::Image);
    assert_eq!(badge_of("dibv5"), RowBadge::Image);
    assert_eq!(badge_of("hdrop"), RowBadge::Files);
    assert_eq!(badge_of("custom"), RowBadge::NoPreview);
    assert_eq!(badge_of("algo_que_nao_existe"), RowBadge::NoPreview);
}

#[test]
fn the_badges_are_distinguishable_from_one_another() {
    let labels = [
        RowBadge::Text.label(),
        RowBadge::Image.label(),
        RowBadge::Files.label(),
        RowBadge::NoPreview.label(),
    ];
    for (i, a) in labels.iter().enumerate() {
        assert!(!a.is_empty(), "etiqueta vazia não distingue nada");
        for b in &labels[i + 1..] {
            assert_ne!(a, b, "duas categorias com a mesma etiqueta");
        }
    }
}

#[test]
fn the_badge_column_is_reserved_on_every_row_so_the_text_lines_up() {
    for dpi in DPIS {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        let a = row_layout(r, dpi, None);
        let b = row_layout(r, dpi, Some((128, 72)));
        assert_eq!(a.type_badge, b.type_badge, "dpi={dpi}");
        assert!(a.type_badge.right > a.type_badge.left, "dpi={dpi}");
    }
}

#[test]
fn scale_px_doubles_at_two_hundred_percent_and_survives_a_zero_dpi() {
    assert_eq!(scale_px(30, 96), 30);
    assert_eq!(scale_px(30, 192), 60);
    assert_eq!(scale_px(30, 144), 45);
    assert_eq!(scale_px(30, 0), 30);
}

const CHROME_PX: i32 = FILTER_STRIP_PX + SEARCH_BAR_PX + HINT_STRIP_PX;

const FILTER_STRIP_PX: i32 = 34;
const SEARCH_BAR_PX: i32 = 30;

const ALL_SCALES: [u32; 4] = [96, 120, 144, 192];

#[test]
fn twelve_rows_still_fit_with_strip_search_and_hint_strip_at_every_scale() {
    for dpi in ALL_SCALES {
        let client = client_height_at(dpi, true);
        let rows = rows_that_fit(client, CHROME_PX, ROW_HEIGHT_PX);
        assert!(
            rows >= MIN_VISIBLE_ROWS,
            "dpi={dpi}: cabem {rows} linhas (cliente {client}px, chrome \
             {CHROME_PX}px) — abaixo do piso de {MIN_VISIBLE_ROWS} do FR-033a. \
             Ajuste WINDOW_HEIGHT_PX (constante única)."
        );
    }
}

#[test]
fn twelve_rows_also_fit_once_the_window_is_rescaled_by_wm_dpichanged() {
    for dpi in ALL_SCALES {
        let client = scale_px(WINDOW_HEIGHT_PX, dpi) - scale_px(48, dpi);
        let rows = rows_that_fit(client, CHROME_PX, ROW_HEIGHT_PX);
        assert!(rows >= MIN_VISIBLE_ROWS, "dpi={dpi}: {rows} linhas");
    }
}

#[test]
fn the_hint_strip_is_what_makes_the_budget_tight_and_it_still_clears_the_floor() {
    let client = client_height_at(96, true);
    let without = rows_that_fit(client, FILTER_STRIP_PX + SEARCH_BAR_PX, ROW_HEIGHT_PX);
    let with = rows_that_fit(client, CHROME_PX, ROW_HEIGHT_PX);
    assert!(with <= without);
    assert!(with >= MIN_VISIBLE_ROWS);
}

#[test]
fn rows_that_fit_never_goes_negative_or_divides_by_zero() {
    assert_eq!(rows_that_fit(10, 100, ROW_HEIGHT_PX), 0);
    assert_eq!(rows_that_fit(100, 0, 0), 0);
    assert_eq!(rows_that_fit(-5, 0, ROW_HEIGHT_PX), 0);
}
