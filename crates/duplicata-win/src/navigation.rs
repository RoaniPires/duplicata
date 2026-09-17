#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NavKey {
    Up,
    Down,
    Home,
    End,
    PageUp,
    PageDown,
}

pub fn next_selection_index(current: usize, total: usize, key: NavKey, page_size: usize) -> usize {
    if total == 0 {
        return 0;
    }
    let last = total - 1;
    let step = page_size.max(1);
    let raw = match key {
        NavKey::Up => current.saturating_sub(1),
        NavKey::Down => current.saturating_add(1),
        NavKey::Home => 0,
        NavKey::End => last,
        NavKey::PageUp => current.saturating_sub(step),
        NavKey::PageDown => current.saturating_add(step),
    };
    raw.min(last)
}

pub fn scroll_offset_after_wheel(
    current_offset: usize,
    lines: isize,
    total: usize,
    page_size: usize,
) -> usize {
    let max_offset = total.saturating_sub(page_size.max(1));
    let shifted = if lines >= 0 {
        current_offset.saturating_add(lines as usize)
    } else {
        current_offset.saturating_sub(lines.unsigned_abs())
    };
    shifted.min(max_offset)
}

pub fn row_index_at(
    y: i32,
    row_height_px: i32,
    scroll_offset: usize,
    total: usize,
) -> Option<usize> {
    if y < 0 || row_height_px <= 0 {
        return None;
    }
    let visible_idx = (y / row_height_px) as usize;
    let idx = scroll_offset.saturating_add(visible_idx);
    if idx < total {
        Some(idx)
    } else {
        None
    }
}
