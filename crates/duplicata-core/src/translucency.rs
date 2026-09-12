use crate::filter_strip::Rect;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AlphaOp {
    pub rect: Option<Rect>,
    pub alpha: u8,
}

pub const OPAQUE: u8 = 255;

pub const TRANSLUCENT_ALPHA: u8 = 220;

#[derive(Debug, Clone, Default)]
pub struct WindowRegions {
    pub client: Rect,
    pub banner: Option<Rect>,
    pub opaque_islands: Vec<Rect>,
}

fn is_empty(r: &Rect) -> bool {
    r.right <= r.left || r.bottom <= r.top
}

fn intersect(a: &Rect, b: &Rect) -> Option<Rect> {
    let r = Rect {
        left: a.left.max(b.left),
        top: a.top.max(b.top),
        right: a.right.min(b.right),
        bottom: a.bottom.min(b.bottom),
    };
    (!is_empty(&r)).then_some(r)
}

pub fn alpha_plan(regions: &WindowRegions) -> Vec<AlphaOp> {
    let mut plan = vec![AlphaOp {
        rect: None,
        alpha: OPAQUE,
    }];

    if regions.banner.is_some() || is_empty(&regions.client) {
        return plan;
    }

    plan.push(AlphaOp {
        rect: Some(regions.client),
        alpha: TRANSLUCENT_ALPHA,
    });

    for island in &regions.opaque_islands {
        if let Some(r) = intersect(island, &regions.client) {
            plan.push(AlphaOp {
                rect: Some(r),
                alpha: OPAQUE,
            });
        }
    }
    plan
}

pub fn opens_any_transparency(plan: &[AlphaOp]) -> bool {
    plan.iter().any(|op| op.alpha != OPAQUE)
}
