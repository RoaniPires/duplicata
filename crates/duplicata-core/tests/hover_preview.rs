use duplicata_core::filter_strip::Rect;
use duplicata_core::hover_preview::{
    content_width, fit_rows, hover_popup_rect, hover_popup_size, hover_preview, rows_for_height,
    HoverTracker, MoveEffect, RowFit, HOVER_PREVIEW_GAP_PX, HOVER_PREVIEW_MAX_LINES,
    HOVER_PREVIEW_MAX_WIDTH_PX, HOVER_PREVIEW_PADDING_PX,
};
use duplicata_core::preview::PREVIEW_MAX_CHARS;
use duplicata_core::ClipListItem;

fn item(kind: &str, preview: Option<&str>) -> ClipListItem {
    ClipListItem {
        id: 1,
        canonical_kind: kind.to_string(),
        preview: preview.map(str::to_string),
        thumbnail: None,
        has_text: preview.is_some(),
        last_activity_ms: 0,
        pinned: false,
    }
}

fn text(preview: &str) -> ClipListItem {
    item("unicode_text", Some(preview))
}

fn lines_of(preview: &str) -> Vec<String> {
    hover_preview(&text(preview), |_| true)
        .expect("deveria ter pré-visualização")
        .lines
}

fn rect(left: i32, top: i32, right: i32, bottom: i32) -> Rect {
    Rect {
        left,
        top,
        right,
        bottom,
    }
}

// ---- linhas lógicas ----

#[test]
fn crlf_lf_and_lone_cr_all_break_lines() {
    assert_eq!(lines_of("a\r\nb\nc\rd"), vec!["a", "b", "c", "d"]);
}

#[test]
fn unicode_line_terminators_break_lines_too() {
    assert_eq!(
        lines_of("a\u{2028}b\u{2029}c\u{0085}d"),
        vec!["a", "b", "c", "d"]
    );
}

#[test]
fn leading_spaces_and_tabs_are_preserved_verbatim() {
    let sql = "SELECT *\n  FROM t\n\tWHERE x = 1";
    assert_eq!(lines_of(sql), vec!["SELECT *", "  FROM t", "\tWHERE x = 1"]);
}

#[test]
fn blank_lines_in_the_middle_are_kept_but_trailing_ones_are_dropped() {
    assert_eq!(lines_of("a\n\nb\r\n\r\n  \n"), vec!["a", "", "b"]);
}

#[test]
fn a_short_preview_is_not_marked_as_cut() {
    let p = hover_preview(&text("a\nb"), |_| true).unwrap();
    assert!(!p.source_cut);
}

#[test]
fn a_preview_at_the_stored_limit_is_marked_as_cut() {
    let long = "x".repeat(PREVIEW_MAX_CHARS - 2);
    let p = hover_preview(&text(&format!("a\n{long}")), |_| true).unwrap();
    assert!(p.source_cut);
}

#[test]
fn a_single_line_at_the_stored_limit_gets_a_popup_even_if_it_seems_to_fit() {
    let long = "x".repeat(PREVIEW_MAX_CHARS);
    let p = hover_preview(&text(&long), |_| true).expect("há conteúdo além do preview");
    assert_eq!(p.lines, vec![long]);
    assert!(p.source_cut);
}

#[test]
fn the_stored_limit_counts_chars_not_bytes() {
    let just_under = "é".repeat(PREVIEW_MAX_CHARS - 1);
    let p = hover_preview(&text(&just_under), |_| false).unwrap();
    assert!(!p.source_cut, "{} chars < limite", PREVIEW_MAX_CHARS - 1);
}

#[test]
fn a_single_line_that_already_fits_in_the_row_gets_no_popup() {
    assert_eq!(hover_preview(&text("curto"), |_| true), None);
}

#[test]
fn a_single_line_wider_than_the_row_gets_a_popup() {
    let p = hover_preview(&text("linha longa demais"), |_| false).unwrap();
    assert_eq!(p.lines, vec!["linha longa demais"]);
    assert!(!p.source_cut);
}

#[test]
fn a_single_line_followed_by_a_newline_is_still_a_single_line() {
    assert_eq!(hover_preview(&text("curto\r\n"), |_| true), None);
}

#[test]
fn images_files_and_custom_formats_never_get_a_popup() {
    for kind in ["dib", "dibv5", "hdrop", "custom"] {
        assert_eq!(
            hover_preview(&item(kind, Some("a\nb")), |_| false),
            None,
            "{kind}"
        );
    }
}

#[test]
fn missing_or_blank_text_gets_no_popup() {
    assert_eq!(hover_preview(&item("unicode_text", None), |_| false), None);
    assert_eq!(hover_preview(&text(""), |_| false), None);
    assert_eq!(hover_preview(&text(" \r\n\t\n"), |_| false), None);
}

// ---- teto de linhas visuais ----

#[test]
fn lines_that_fit_are_shown_whole_without_continuation() {
    let fit = fit_rows(&[1, 3, 1], false, HOVER_PREVIEW_MAX_LINES);
    assert_eq!(
        fit,
        RowFit {
            rows_per_line: vec![1, 3, 1],
            truncated: false
        }
    );
    assert_eq!(fit.total_rows(), 5);
}

#[test]
fn exactly_the_ceiling_is_shown_whole_without_continuation() {
    let rows = vec![1; HOVER_PREVIEW_MAX_LINES];
    let fit = fit_rows(&rows, false, HOVER_PREVIEW_MAX_LINES);
    assert!(!fit.truncated);
    assert_eq!(fit.total_rows(), HOVER_PREVIEW_MAX_LINES);
}

#[test]
fn more_logical_lines_than_the_ceiling_are_capped_with_continuation() {
    let rows = vec![1; HOVER_PREVIEW_MAX_LINES + 5];
    let fit = fit_rows(&rows, false, HOVER_PREVIEW_MAX_LINES);
    assert!(fit.truncated);
    assert_eq!(fit.rows_per_line, vec![1; HOVER_PREVIEW_MAX_LINES]);
    assert_eq!(fit.total_rows(), HOVER_PREVIEW_MAX_LINES + 1);
}

#[test]
fn the_ceiling_counts_wrapped_rows_not_logical_lines() {
    // Uma linha de 400 caracteres quebrada em 7 linhas visuais, três vezes:
    // 7 + 7 = 14, a terceira só recebe 1 e aparece cortada.
    let fit = fit_rows(&[7, 7, 7], false, 15);
    assert_eq!(fit.rows_per_line, vec![7, 7, 1]);
    assert!(fit.truncated);
    assert_eq!(fit.total_rows(), 16);
}

#[test]
fn a_line_taller_than_the_whole_ceiling_is_shown_cut_with_continuation() {
    let fit = fit_rows(&[40], false, 15);
    assert_eq!(fit.rows_per_line, vec![15]);
    assert!(fit.truncated);
}

#[test]
fn a_line_that_starts_exactly_at_the_ceiling_is_dropped_not_given_zero_rows() {
    let fit = fit_rows(&[10, 5, 2], false, 15);
    assert_eq!(fit.rows_per_line, vec![10, 5]);
    assert!(fit.truncated);
}

#[test]
fn an_empty_line_still_takes_one_row() {
    let fit = fit_rows(&[1, 0, 1], false, 15);
    assert_eq!(fit.rows_per_line, vec![1, 1, 1]);
}

#[test]
fn a_cut_source_adds_continuation_even_when_everything_fits() {
    let fit = fit_rows(&[3], true, 15);
    assert_eq!(fit.rows_per_line, vec![3]);
    assert!(fit.truncated);
    assert_eq!(fit.total_rows(), 4);
}

#[test]
fn a_zero_ceiling_still_shows_one_row() {
    let fit = fit_rows(&[1, 1], false, 0);
    assert_eq!(fit.rows_per_line, vec![1]);
    assert!(fit.truncated);
}

// ---- tamanho ----

#[test]
fn size_is_the_widest_line_plus_padding_and_the_rows_plus_padding() {
    let pad = HOVER_PREVIEW_PADDING_PX;
    assert_eq!(
        hover_popup_size(100, 3, 16, 96),
        (100 + 2 * pad, 48 + 2 * pad)
    );
}

#[test]
fn width_is_capped_at_the_maximum() {
    let (w, _) = hover_popup_size(10_000, 1, 16, 96);
    assert_eq!(w, HOVER_PREVIEW_MAX_WIDTH_PX);
}

#[test]
fn size_scales_padding_and_cap_with_dpi() {
    let (w, h) = hover_popup_size(10_000, 2, 32, 192);
    assert_eq!(w, HOVER_PREVIEW_MAX_WIDTH_PX * 2);
    assert_eq!(h, 64 + 4 * HOVER_PREVIEW_PADDING_PX);
}

#[test]
fn negative_inputs_never_produce_a_negative_size() {
    let pad = HOVER_PREVIEW_PADDING_PX;
    assert_eq!(hover_popup_size(-5, 2, -1, 96), (2 * pad, 2 * pad));
}

#[test]
fn absurd_inputs_saturate_instead_of_overflowing() {
    let (w, h) = hover_popup_size(i32::MAX, usize::MAX, i32::MAX, 96);
    assert_eq!(w, HOVER_PREVIEW_MAX_WIDTH_PX);
    assert_eq!(h, i32::MAX);
}

// ---- posição ----

const WORK: Rect = Rect {
    left: 0,
    top: 0,
    right: 1920,
    bottom: 1040,
};

fn overlaps(a: Rect, b: Rect) -> bool {
    a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom
}

#[test]
fn goes_to_the_right_of_the_window_aligned_with_the_row() {
    let window = rect(400, 200, 920, 760);
    let r = hover_popup_rect(window, 300, (300, 200), WORK, 96);
    let left = 920 + HOVER_PREVIEW_GAP_PX;
    assert_eq!(r, rect(left, 300, left + 300, 500));
}

#[test]
fn flips_to_the_left_when_the_right_side_has_no_room() {
    let window = rect(1500, 200, 1900, 760);
    let r = hover_popup_rect(window, 300, (300, 200), WORK, 96);
    assert_eq!(r.right, 1500 - HOVER_PREVIEW_GAP_PX);
    assert_eq!(r.left, r.right - 300);
}

#[test]
fn with_no_room_on_either_side_it_hugs_the_right_edge_inside_the_screen() {
    let window = rect(100, 0, 1820, 900);
    let r = hover_popup_rect(window, 10, (480, 100), WORK, 96);
    assert_eq!((r.left, r.right), (1920 - 480, 1920));
}

#[test]
fn a_row_near_the_bottom_pushes_the_popup_up_to_stay_inside() {
    let window = rect(400, 500, 920, 1040);
    let r = hover_popup_rect(window, 1000, (300, 400), WORK, 96);
    assert_eq!((r.top, r.bottom), (640, 1040));
}

#[test]
fn a_popup_taller_or_wider_than_the_work_area_is_shrunk_and_pinned_to_it() {
    let small = rect(0, 0, 200, 150);
    let r = hover_popup_rect(rect(10, 10, 100, 100), 50, (480, 400), small, 96);
    assert_eq!(r, small);
}

#[test]
fn a_degenerate_work_area_yields_an_empty_rect_at_its_origin() {
    let r = hover_popup_rect(
        rect(0, 0, 100, 100),
        10,
        (300, 200),
        rect(50, 50, 40, 40),
        96,
    );
    assert_eq!(r, rect(50, 50, 50, 50));
}

#[test]
fn works_on_a_secondary_monitor_with_a_negative_origin() {
    let work = rect(-1920, -200, 0, 880);
    let window = rect(-1000, -100, -480, 460);
    let r = hover_popup_rect(window, -150, (300, 200), work, 96);
    let left = -480 + HOVER_PREVIEW_GAP_PX;
    assert_eq!(r, rect(left, -150, left + 300, 50));
}

#[test]
fn the_gap_scales_with_dpi() {
    let window = rect(400, 200, 920, 760);
    let r = hover_popup_rect(window, 300, (300, 200), WORK, 192);
    assert_eq!(r.left, 920 + 2 * HOVER_PREVIEW_GAP_PX);
}

#[test]
fn extreme_coordinates_do_not_overflow() {
    let window = rect(i32::MAX - 10, 0, i32::MAX, 10);
    let work = rect(i32::MIN, i32::MIN, i32::MAX, i32::MAX);
    let _ = hover_popup_rect(window, 0, (300, 200), work, 96);
}

#[test]
fn every_placement_stays_inside_and_beside_the_window_when_there_is_room() {
    for wx in (-200..2100).step_by(137) {
        for row_top in (-100..1200).step_by(113) {
            for (w, h) in [(50, 40), (480, 300), (2500, 1500)] {
                let window = rect(wx, 100, wx + 520, 660);
                let r = hover_popup_rect(window, row_top, (w, h), WORK, 96);
                let ctx = format!("wx={wx} row_top={row_top} size=({w},{h}) → {r:?}");
                assert!(
                    r.left >= WORK.left
                        && r.right <= WORK.right
                        && r.top >= WORK.top
                        && r.bottom <= WORK.bottom,
                    "fora da área útil: {ctx}"
                );
                let gap = HOVER_PREVIEW_GAP_PX;
                let room_right = window.right + gap + w <= WORK.right;
                let room_left = window.left - gap - w >= WORK.left;
                if room_right || room_left {
                    assert!(
                        !overlaps(r, window),
                        "cobriu a janela havendo espaço: {ctx}"
                    );
                }
            }
        }
    }
}

// ---- estado do hover ----

const ARM: MoveEffect = MoveEffect {
    hide: false,
    arm: true,
};
const NOTHING: MoveEffect = MoveEffect {
    hide: false,
    arm: false,
};
const HIDE: MoveEffect = MoveEffect {
    hide: true,
    arm: false,
};
const HIDE_AND_ARM: MoveEffect = MoveEffect {
    hide: true,
    arm: true,
};

#[test]
fn a_closed_window_never_arms_the_hover_notice() {
    let mut t = HoverTracker::default();
    for _ in 0..5 {
        assert_eq!(t.on_move(false, Some(3)), NOTHING);
    }
    assert_eq!(t.on_hover(false, Some(3)), None);
}

#[test]
fn moving_onto_a_row_arms_once_and_further_moves_on_it_do_not_rearm() {
    let mut t = HoverTracker::default();
    assert_eq!(t.on_move(true, Some(2)), ARM);
    assert_eq!(t.on_move(true, Some(2)), NOTHING);
    assert_eq!(t.on_move(true, Some(2)), NOTHING);
}

#[test]
fn moving_over_chrome_or_empty_space_arms_nothing() {
    let mut t = HoverTracker::default();
    assert_eq!(t.on_move(true, None), NOTHING);
}

#[test]
fn the_hover_on_the_same_row_shows_it() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(4));
    assert_eq!(t.on_hover(true, Some(4)), Some(4));
    t.mark_shown();
    assert!(t.is_shown());
}

#[test]
fn after_the_hover_fired_moves_on_the_same_row_do_not_rearm_even_without_popup() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(4));
    assert_eq!(t.on_hover(true, Some(4)), Some(4));
    // A casca decidiu não mostrar (item curto): não rearma sobre a mesma linha.
    assert_eq!(t.on_move(true, Some(4)), NOTHING);
    assert!(!t.is_shown());
}

#[test]
fn moving_to_another_row_hides_the_popup_and_rearms() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    t.on_hover(true, Some(1));
    t.mark_shown();
    assert_eq!(t.on_move(true, Some(2)), HIDE_AND_ARM);
    assert!(!t.is_shown());
}

#[test]
fn moving_from_a_shown_row_to_chrome_hides_without_arming() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    t.on_hover(true, Some(1));
    t.mark_shown();
    assert_eq!(t.on_move(true, None), HIDE);
}

#[test]
fn a_stale_hover_for_another_row_is_ignored() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    assert_eq!(t.on_hover(true, Some(7)), None);
    assert_eq!(t.on_hover(true, None), None);
}

#[test]
fn a_hover_arriving_after_the_window_closed_is_ignored() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    assert_eq!(t.on_hover(false, Some(1)), None);
}

#[test]
fn a_second_hover_while_shown_does_not_show_again() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    t.on_hover(true, Some(1));
    t.mark_shown();
    assert_eq!(t.on_hover(true, Some(1)), None);
}

#[test]
fn leaving_the_window_hides_and_the_next_move_rearms() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    t.on_hover(true, Some(1));
    t.mark_shown();
    assert!(t.on_leave());
    assert!(!t.is_shown());
    assert_eq!(t.on_move(true, Some(1)), ARM);
}

#[test]
fn dismissing_hides_only_if_shown_and_forgets_the_row() {
    let mut t = HoverTracker::default();
    assert!(!t.dismiss());
    t.on_move(true, Some(5));
    assert!(!t.dismiss());
    t.on_move(true, Some(5));
    t.on_hover(true, Some(5));
    t.mark_shown();
    assert!(t.dismiss());
    // Depois de uma rolagem o item sob o cursor mudou: um aviso velho não
    // mostra nada, e o próximo movimento arma de novo.
    assert_eq!(t.on_hover(true, Some(5)), None);
    assert_eq!(t.on_move(true, Some(5)), ARM);
}

#[test]
fn a_move_with_the_window_closed_hides_a_shown_popup() {
    let mut t = HoverTracker::default();
    t.on_move(true, Some(1));
    t.on_hover(true, Some(1));
    t.mark_shown();
    assert_eq!(t.on_move(false, Some(1)), HIDE);
}

#[test]
fn the_single_line_measured_is_the_one_the_core_extracted_without_terminators() {
    for src in ["curto\r", "curto\u{2028}", "curto\r\n\r\n", "curto\u{0085}"] {
        let mut seen = String::new();
        let p = hover_preview(&text(src), |l| {
            seen = l.to_string();
            true
        });
        assert_eq!(p, None, "{src:?}");
        assert_eq!(seen, "curto", "{src:?}");
    }
}

#[test]
fn multi_line_items_never_ask_for_a_measurement() {
    let p = hover_preview(&text("a\nb"), |_| panic!("não deveria medir"));
    assert!(p.is_some());
}

#[test]
fn a_cut_single_line_never_asks_for_a_measurement() {
    let long = "x".repeat(PREVIEW_MAX_CHARS);
    assert!(hover_preview(&text(&long), |_| panic!("não deveria medir")).is_some());
}

#[test]
fn rows_for_height_rounds_up_and_never_returns_zero() {
    assert_eq!(rows_for_height(0, 16), 1);
    assert_eq!(rows_for_height(16, 16), 1);
    assert_eq!(rows_for_height(17, 16), 2);
    assert_eq!(rows_for_height(48, 16), 3);
    assert_eq!(rows_for_height(-5, 16), 1);
    assert_eq!(rows_for_height(10, 0), 10);
    assert_eq!(rows_for_height(i32::MAX, 16), (i32::MAX / 16) as usize + 1);
}

#[test]
fn content_width_counts_the_continuation_only_when_it_is_drawn() {
    let cut = fit_rows(&[1], true, 15);
    let whole = fit_rows(&[1], false, 15);
    assert_eq!(content_width(10, 30, &cut), 30);
    assert_eq!(content_width(50, 30, &cut), 50);
    assert_eq!(content_width(10, 30, &whole), 10);
}
