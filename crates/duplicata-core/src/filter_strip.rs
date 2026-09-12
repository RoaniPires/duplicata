use crate::list_view::ContentTypeFilter;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Rect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub const SEGMENT_PADDING_PX: i32 = 14;

pub const STRIP_LEADING_PX: i32 = 6;

pub fn segment_rects_packed(label_widths: &[i32; 4], padding_px: i32, height_px: i32) -> [Rect; 4] {
    let h = height_px.max(0);
    let pad = padding_px.max(0);
    let mut rects = [Rect {
        left: 0,
        top: 0,
        right: 0,
        bottom: h,
    }; 4];
    let mut x = STRIP_LEADING_PX;
    for (r, &label_w) in rects.iter_mut().zip(label_widths.iter()) {
        r.left = x;
        r.right = x + label_w.max(0) + 2 * pad;
        x = r.right;
    }
    rects
}

pub fn segment_at_packed(x: i32, rects: &[Rect; 4]) -> Option<ContentTypeFilter> {
    rects
        .iter()
        .position(|r| x >= r.left && x < r.right)
        .map(|i| ContentTypeFilter::SEGMENTS[i])
}

pub fn adjacent_segment(current: ContentTypeFilter, forward: bool) -> ContentTypeFilter {
    let segs = ContentTypeFilter::SEGMENTS;
    let idx = segs.iter().position(|&s| s == current).unwrap_or(0);
    let next = if forward {
        (idx + 1).min(segs.len() - 1)
    } else {
        idx.saturating_sub(1)
    };
    segs[next]
}
