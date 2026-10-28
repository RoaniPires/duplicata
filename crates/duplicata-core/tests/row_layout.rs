use duplicata_core::filter_strip::Rect;
use duplicata_core::hint_strip::HINT_STRIP_PX;
use duplicata_core::row_layout::{
    client_height_at, delete_button_rect, pin_button_rect, row_layout, row_layout_with_delete,
    rows_that_fit, scale_px, thumbnail_target, MIN_VISIBLE_ROWS, ROW_HEIGHT_PX, TEXT_GAP_PX,
    THUMBNAIL_MARGIN_PX, THUMBNAIL_MAX_WIDTH_PX, WINDOW_FRAME_ALLOWANCE_PX, WINDOW_HEIGHT_PX,
    WINDOW_WIDTH_PX,
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
fn the_pin_column_never_costs_more_than_ten_percent_of_the_row() {
    for dpi in ALL_SCALES {
        let width = row_width(dpi);
        let r = row(width, ROW_HEIGHT_PX);
        let l = row_layout(r, dpi, None);
        let reserved = l.pin_button.right - l.pin_button.left;
        assert!(reserved > 0, "dpi={dpi}: a coluna do alfinete existe");
        assert!(
            reserved * 100 <= width * 10,
            "dpi={dpi}: o alfinete reserva {reserved}px de {width}px \
             de linha (> 10%)"
        );
    }
}

#[test]
fn the_pin_button_is_a_square_as_tall_as_the_row_hugging_the_left_edge() {
    for dpi in ALL_SCALES {
        let r = Rect {
            left: 4,
            top: 56,
            right: 4 + row_width(dpi),
            bottom: 56 + ROW_HEIGHT_PX,
        };
        let b = pin_button_rect(r, dpi);
        assert_eq!(b.left, r.left, "dpi={dpi}: encostado na borda esquerda");
        assert_eq!(b.right - b.left, ROW_HEIGHT_PX, "dpi={dpi}: quadrado");
        assert_eq!(b.top, r.top, "dpi={dpi}");
        assert_eq!(b.bottom, r.bottom, "dpi={dpi}");
        for show_delete in [false, true] {
            let l = row_layout_with_delete(r, dpi, Some((128, 72)), show_delete);
            assert_eq!(l.pin_button, b, "dpi={dpi} lixeira={show_delete}");
        }
    }
}

#[test]
fn a_row_narrower_than_its_height_cuts_the_pin_button_at_the_right_edge() {
    let r = Rect {
        left: 10,
        top: 0,
        right: 20,
        bottom: ROW_HEIGHT_PX,
    };
    let b = pin_button_rect(r, 96);
    assert_eq!((b.left, b.right), (10, 20));
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
    assert!(l.pin_button.left <= l.pin_button.right);
    assert!(l.pin_button.top <= l.pin_button.bottom);
    assert!(l.pin_button.right <= l.text.left);
    assert!(l.text.left <= l.text.right);
}

#[test]
fn a_row_with_negative_height_gets_an_empty_pin_button_at_its_left_edge() {
    let r = Rect {
        left: 0,
        top: 50,
        right: 300,
        bottom: 20,
    };
    let p = pin_button_rect(r, 96);
    assert_eq!((p.left, p.right), (0, 0), "sem largura, nunca negativa");
    let l = row_layout(r, 96, None);
    assert_eq!(l.pin_button.right, l.pin_button.left);
    assert!(l.pin_button.right <= l.text.left);
}

#[test]
fn a_row_narrower_than_two_buttons_leaves_the_pin_only_what_the_trash_does_not_cover() {
    // A zona da lixeira ganha o clique onde os quadrados se sobrepõem (com
    // ou sem a lixeira à mostra): o alfinete desenhado para antes dela.
    let r = row(40, ROW_HEIGHT_PX);
    let trash = delete_button_rect(r, 96);
    for show_delete in [false, true] {
        let l = row_layout_with_delete(r, 96, None, show_delete);
        assert_eq!(l.pin_button.left, r.left, "lixeira={show_delete}");
        assert_eq!(l.pin_button.right, trash.left, "lixeira={show_delete}");
    }
    let tiny = row(20, ROW_HEIGHT_PX);
    let l = row_layout(tiny, 96, None);
    assert_eq!(
        l.pin_button.right, l.pin_button.left,
        "a lixeira cobre a linha toda: alfinete vazio"
    );
}

#[test]
fn a_row_with_inverted_edges_gets_an_empty_pin_button_at_its_left_edge() {
    let r = Rect {
        left: 100,
        top: 0,
        right: 50,
        bottom: ROW_HEIGHT_PX,
    };
    let p = pin_button_rect(r, 96);
    assert_eq!(
        (p.left, p.right),
        (100, 100),
        "nunca termina antes de começar"
    );
    for show_delete in [false, true] {
        let l = row_layout_with_delete(r, 96, None, show_delete);
        assert_eq!(
            (l.pin_button.left, l.pin_button.right),
            (100, 100),
            "layout com lixeira={show_delete}"
        );
        assert!(l.text.left <= l.text.right, "lixeira={show_delete}");
    }
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
            assert!(l.pin_button.right > l.pin_button.left, "dpi={dpi}");
            assert!(l.pin_button.right <= l.text.left, "dpi={dpi} {thumb:?}");
            assert!(l.text.left <= l.text.right, "dpi={dpi} {thumb:?}");
            if thumb.is_some() {
                assert!(l.pin_button.right <= l.thumbnail.left, "dpi={dpi}");
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
fn the_pin_column_is_reserved_on_every_row_so_the_text_lines_up() {
    for dpi in DPIS {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        let a = row_layout(r, dpi, None);
        let b = row_layout(r, dpi, Some((128, 72)));
        let c = row_layout_with_delete(r, dpi, None, true);
        assert_eq!(a.pin_button, b.pin_button, "dpi={dpi}");
        assert_eq!(a.pin_button, c.pin_button, "dpi={dpi}");
        assert!(a.pin_button.right > a.pin_button.left, "dpi={dpi}");
        assert_eq!(
            a.text.left,
            a.pin_button.right + scale_px(TEXT_GAP_PX, dpi),
            "dpi={dpi}: o texto começa uma fresta depois do alfinete"
        );
        assert_eq!(a.text.left, c.text.left, "dpi={dpi}");
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
             {CHROME_PX}px) — abaixo do piso de {MIN_VISIBLE_ROWS}. \
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

#[test]
fn the_delete_button_is_a_square_as_tall_as_the_row_at_every_scale() {
    for dpi in ALL_SCALES {
        let r = Rect {
            left: 4,
            top: 56,
            right: 4 + row_width(dpi),
            bottom: 56 + ROW_HEIGHT_PX,
        };
        let b = delete_button_rect(r, dpi);
        assert_eq!(b.bottom - b.top, ROW_HEIGHT_PX, "dpi={dpi}");
        assert_eq!(b.right - b.left, ROW_HEIGHT_PX, "dpi={dpi}: quadrado");
        assert_eq!(b.top, r.top, "dpi={dpi}");
        assert_eq!(b.bottom, r.bottom, "dpi={dpi}");
    }
}

#[test]
fn the_delete_button_hugs_the_right_edge() {
    let r = row(row_width(96), ROW_HEIGHT_PX);
    assert_eq!(delete_button_rect(r, 96).right, r.right);
}

#[test]
fn only_the_row_showing_the_delete_button_gets_a_shorter_text() {
    for dpi in ALL_SCALES {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        for thumb in [None, Some((128u32, 72u32))] {
            let with = row_layout_with_delete(r, dpi, thumb, true);
            let without = row_layout_with_delete(r, dpi, thumb, false);
            let button = with.delete_button.expect("linha com lixeira");
            assert_eq!(button, delete_button_rect(r, dpi), "dpi={dpi}");
            assert!(with.text.right < without.text.right, "dpi={dpi} {thumb:?}");
            assert_eq!(with.text.left, without.text.left, "dpi={dpi} {thumb:?}");
            assert_eq!(without.text.right, r.right, "dpi={dpi}: largura de hoje");
            assert_eq!(without.delete_button, None);
        }
    }
}

#[test]
fn the_text_stops_a_gap_before_the_delete_button() {
    for dpi in ALL_SCALES {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        let l = row_layout_with_delete(r, dpi, None, true);
        let button = l.delete_button.expect("linha com lixeira");
        assert_eq!(
            l.text.right,
            button.left - scale_px(TEXT_GAP_PX, dpi),
            "dpi={dpi}"
        );
    }
}

#[test]
fn a_narrow_row_with_the_delete_button_never_gets_a_negative_text_width() {
    for width in [0, 10, 40, 60, 80] {
        let r = row(width, ROW_HEIGHT_PX);
        let l = row_layout_with_delete(r, 96, Some((128, 72)), true);
        assert!(l.text.left <= l.text.right, "largura {width}: {:?}", l.text);
    }
}

#[test]
fn the_layout_without_the_delete_button_is_exactly_row_layout() {
    for dpi in ALL_SCALES {
        let r = row(row_width(dpi), ROW_HEIGHT_PX);
        for thumb in [None, Some((128u32, 72u32)), Some((0, 10))] {
            assert_eq!(
                row_layout_with_delete(r, dpi, thumb, false),
                row_layout(r, dpi, thumb)
            );
        }
    }
}

#[test]
fn every_point_of_the_drawn_pin_button_clicks_the_pin_and_the_rest_of_its_square_is_the_trash() {
    use duplicata_core::delete_button::{zone_at, RowZone};
    for height in -3..=40 {
        for width in -3..=80 {
            let r = Rect {
                left: 7,
                top: 5,
                right: 7 + width,
                bottom: 5 + height,
            };
            let square = pin_button_rect(r, 96);
            for show_delete in [false, true] {
                let l = row_layout_with_delete(r, 96, None, show_delete);
                let p = l.pin_button;
                assert_eq!(p.left, square.left, "{r:?}");
                assert!(
                    p.left <= p.right && p.right <= square.right,
                    "{r:?}: alfinete {p:?} fora do quadrado {square:?}"
                );
                for y in (r.top - 1)..=(r.bottom + 1) {
                    for x in (r.left - 1)..=(r.right + 1) {
                        let drawn = x >= p.left && x < p.right && y >= p.top && y < p.bottom;
                        let in_square = x >= square.left
                            && x < square.right
                            && y >= square.top
                            && y < square.bottom;
                        let zone = zone_at(x, y, r, 96);
                        if drawn {
                            assert_eq!(zone, RowZone::PinButton, "{r:?} ({x},{y}) desenhado");
                        } else if in_square {
                            assert_eq!(zone, RowZone::DeleteButton, "{r:?} ({x},{y}) coberto");
                        }
                    }
                }
            }
        }
    }
}

#[test]
fn the_pin_button_never_has_negative_width_nor_crosses_the_trash_for_any_row_geometry() {
    for left in [-20, 0, 7, 100] {
        for right in [-30, 0, 5, 50, 100, 400] {
            for (top, bottom) in [(0, 0), (10, 4), (0, 1), (0, 28), (3, 60)] {
                let r = Rect {
                    left,
                    top,
                    right,
                    bottom,
                };
                let trash_left = delete_button_rect(r, 96).left;
                for show_delete in [false, true] {
                    let l = row_layout_with_delete(r, 96, None, show_delete);
                    let p = l.pin_button;
                    let ctx = format!("{r:?} lixeira={show_delete}");
                    assert_eq!(p.left, r.left, "{ctx}");
                    assert!(p.left <= p.right, "{ctx}");
                    assert!(p.right <= trash_left.max(r.left), "{ctx}");
                    assert!(l.text.left <= l.text.right, "{ctx}");
                }
            }
        }
    }
}
