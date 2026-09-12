use duplicata_core::filter_strip::{
    adjacent_segment, segment_at_packed, segment_rects_packed, SEGMENT_PADDING_PX, STRIP_LEADING_PX,
};
use duplicata_core::list_view::ContentTypeFilter;

const LABELS: [i32; 4] = [30, 34, 42, 54];

fn rects() -> [duplicata_core::filter_strip::Rect; 4] {
    segment_rects_packed(&LABELS, SEGMENT_PADDING_PX, 34)
}

#[test]
fn each_segment_is_as_wide_as_its_own_label_plus_padding() {
    let r = rects();
    for (i, w) in LABELS.iter().enumerate() {
        assert_eq!(
            r[i].right - r[i].left,
            w + 2 * SEGMENT_PADDING_PX,
            "segmento {i}"
        );
        assert_eq!(r[i].top, 0);
        assert_eq!(r[i].bottom, 34);
    }
    assert_ne!(r[0].right - r[0].left, r[3].right - r[3].left);
}

#[test]
fn the_segments_are_packed_left_to_right_without_gaps() {
    let r = rects();
    assert_eq!(r[0].left, STRIP_LEADING_PX);
    for i in 1..4 {
        assert_eq!(r[i].left, r[i - 1].right, "buraco antes do segmento {i}");
    }
}

#[test]
fn the_strip_does_not_stretch_to_the_window_edge() {
    let r = rects();
    let used = r[3].right;
    assert!(
        used < 400,
        "os 4 segmentos ocupam {used}px — deveriam caber bem antes da borda"
    );
}

#[test]
fn segment_at_packed_maps_each_rect_to_its_segment() {
    let r = rects();
    for (i, seg) in ContentTypeFilter::SEGMENTS.iter().enumerate() {
        assert_eq!(
            segment_at_packed(r[i].left, &r),
            Some(*seg),
            "borda esq {i}"
        );
        assert_eq!(
            segment_at_packed(r[i].right - 1, &r),
            Some(*seg),
            "borda dir {i}"
        );
    }
}

#[test]
fn clicking_the_empty_space_after_the_last_segment_changes_nothing() {
    let r = rects();
    assert_eq!(segment_at_packed(r[3].right, &r), None);
    assert_eq!(segment_at_packed(r[3].right + 200, &r), None);
}

#[test]
fn segment_at_packed_returns_none_before_the_first_segment_and_outside() {
    let r = rects();
    assert_eq!(segment_at_packed(-1, &r), None);
    assert_eq!(segment_at_packed(0, &r), None);
    assert_eq!(segment_at_packed(9999, &r), None);
}

#[test]
fn a_degenerate_label_width_still_yields_a_clickable_segment() {
    let r = segment_rects_packed(&[0, 0, 0, 0], SEGMENT_PADDING_PX, 34);
    for (i, seg) in r.iter().enumerate() {
        assert!(seg.right > seg.left, "segmento {i} degenerou");
    }
    assert_eq!(
        segment_at_packed(r[2].left, &r),
        Some(ContentTypeFilter::Image)
    );
}

#[test]
fn negative_inputs_never_produce_an_inverted_rect() {
    let r = segment_rects_packed(&[-5, 10, -1, 3], -2, -9);
    for seg in &r {
        assert!(seg.right >= seg.left);
        assert!(seg.bottom >= seg.top);
    }
}

#[test]
fn adjacent_segment_advances_and_retreats() {
    assert_eq!(
        adjacent_segment(ContentTypeFilter::All, true),
        ContentTypeFilter::Text
    );
    assert_eq!(
        adjacent_segment(ContentTypeFilter::Text, true),
        ContentTypeFilter::Image
    );
    assert_eq!(
        adjacent_segment(ContentTypeFilter::Image, false),
        ContentTypeFilter::Text
    );
}

#[test]
fn adjacent_segment_clamps_at_the_ends_and_does_not_wrap() {
    assert_eq!(
        adjacent_segment(ContentTypeFilter::All, false),
        ContentTypeFilter::All
    );
    assert_eq!(
        adjacent_segment(ContentTypeFilter::Files, true),
        ContentTypeFilter::Files
    );
}

#[test]
fn canonical_segment_order_and_labels() {
    assert_eq!(
        ContentTypeFilter::SEGMENTS,
        [
            ContentTypeFilter::All,
            ContentTypeFilter::Text,
            ContentTypeFilter::Image,
            ContentTypeFilter::Files,
        ]
    );
    let labels: Vec<_> = ContentTypeFilter::SEGMENTS
        .iter()
        .map(|s| s.label())
        .collect();
    assert_eq!(labels, ["Tudo", "Texto", "Imagem", "Arquivos"]);
}
