use crate::filter_strip::Rect;
use crate::list_view::{category_of, ContentTypeFilter};

pub const REFERENCE_DPI: u32 = 96;

pub const WINDOW_WIDTH_PX: i32 = 520;

pub const WINDOW_FRAME_WIDTH_PX: i32 = 16;

pub const WINDOW_FRAME_ALLOWANCE_PX: i32 = 40;

pub const ROW_HEIGHT_PX: i32 = 28;

pub const WINDOW_HEIGHT_PX: i32 = 560;

pub const WINDOW_NONCLIENT_HEIGHT_PX: i32 = 48;

pub const MIN_VISIBLE_ROWS: usize = 12;

pub fn rows_that_fit(client_height_px: i32, chrome_px: i32, row_height_px: i32) -> usize {
    if row_height_px <= 0 {
        return 0;
    }
    ((client_height_px - chrome_px).max(0) / row_height_px).max(0) as usize
}

pub fn client_height_at(dpi: u32, scaled_nonclient: bool) -> i32 {
    let nonclient = if scaled_nonclient {
        scale_px(WINDOW_NONCLIENT_HEIGHT_PX, dpi)
    } else {
        WINDOW_NONCLIENT_HEIGHT_PX
    };
    WINDOW_HEIGHT_PX - nonclient
}

pub const PIN_STRIPE_PX: i32 = 3;

pub const PIN_GUTTER_PX: i32 = 5;

pub const TYPE_BADGE_PX: i32 = 30;

pub const THUMBNAIL_MARGIN_PX: i32 = 2;

pub const THUMBNAIL_MAX_WIDTH_PX: i32 = 72;

pub const TEXT_GAP_PX: i32 = 8;

pub fn scale_px(logical_px: i32, dpi: u32) -> i32 {
    let dpi = if dpi == 0 { REFERENCE_DPI } else { dpi };
    ((logical_px as i64 * dpi as i64) / REFERENCE_DPI as i64) as i32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowBadge {
    Text,
    Image,
    Files,
    NoPreview,
}

impl RowBadge {
    pub const fn label(self) -> &'static str {
        match self {
            RowBadge::Text => "Txt",
            RowBadge::Image => "Img",
            RowBadge::Files => "Arq",
            RowBadge::NoPreview => "—",
        }
    }
}

pub fn badge_of(canonical_kind: &str) -> RowBadge {
    match category_of(canonical_kind) {
        Some(ContentTypeFilter::Text) => RowBadge::Text,
        Some(ContentTypeFilter::Image) => RowBadge::Image,
        Some(ContentTypeFilter::Files) => RowBadge::Files,
        Some(ContentTypeFilter::All) | None => RowBadge::NoPreview,
    }
}

pub fn pin_reserved_px(dpi: u32) -> i32 {
    scale_px(PIN_STRIPE_PX, dpi) + scale_px(PIN_GUTTER_PX, dpi)
}

pub fn thumbnail_target(row_height_px: i32, dpi: u32, src_w: u32, src_h: u32) -> (i32, i32) {
    if src_w == 0 || src_h == 0 {
        return (0, 0);
    }
    let margin = scale_px(THUMBNAIL_MARGIN_PX, dpi);
    let max_w = scale_px(THUMBNAIL_MAX_WIDTH_PX, dpi).max(1);
    let h = (row_height_px - 2 * margin).max(1);
    let (src_w, src_h) = (src_w as i64, src_h as i64);

    let w = ((h as i64 * src_w + src_h / 2) / src_h).max(1) as i32;
    if w <= max_w {
        return (w, h);
    }
    let clamped_h = ((max_w as i64 * src_h + src_w / 2) / src_w).max(1) as i32;
    (max_w, clamped_h)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowLayout {
    pub pin_stripe: Rect,
    pub type_badge: Rect,
    pub thumbnail: Rect,
    pub text: Rect,
}

pub fn row_layout(row: Rect, dpi: u32, thumbnail: Option<(u32, u32)>) -> RowLayout {
    let stripe_w = scale_px(PIN_STRIPE_PX, dpi);
    let gutter = scale_px(PIN_GUTTER_PX, dpi);
    let badge_w = scale_px(TYPE_BADGE_PX, dpi);
    let margin = scale_px(THUMBNAIL_MARGIN_PX, dpi);
    let gap = scale_px(TEXT_GAP_PX, dpi);
    let row_h = (row.bottom - row.top).max(0);

    let pin_stripe = Rect {
        left: row.left,
        top: row.top + margin,
        right: row.left + stripe_w,
        bottom: (row.bottom - margin).max(row.top + margin),
    };
    let badge_left = row.left + stripe_w + gutter;
    let type_badge = Rect {
        left: badge_left,
        top: row.top,
        right: badge_left + badge_w,
        bottom: row.bottom,
    };

    let mut cursor = type_badge.right;
    let empty = Rect {
        left: cursor,
        top: row.top,
        right: cursor,
        bottom: row.top,
    };
    let thumbnail = match thumbnail {
        Some((src_w, src_h)) => {
            let (w, h) = thumbnail_target(row_h, dpi, src_w, src_h);
            if w == 0 || h == 0 {
                empty
            } else {
                let top = row.top + (row_h - h) / 2;
                let left = cursor + margin;
                cursor = left + w;
                Rect {
                    left,
                    top,
                    right: cursor,
                    bottom: top + h,
                }
            }
        }
        None => empty,
    };

    let text_left = (cursor + gap).min(row.right);
    RowLayout {
        pin_stripe,
        type_badge,
        thumbnail,
        text: Rect {
            left: text_left,
            top: row.top,
            right: row.right,
            bottom: row.bottom,
        },
    }
}
