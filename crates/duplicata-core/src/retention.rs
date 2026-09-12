use std::time::Duration;

pub fn cutoff_ms(now_ms: u64, retention: Duration) -> u64 {
    now_ms.saturating_sub(retention.as_millis() as u64)
}

pub fn over_count_by(current_non_pinned: u64, max_items: u32) -> u64 {
    current_non_pinned.saturating_sub(u64::from(max_items))
}

pub fn pin_cap_reached(current_pinned: u32, max_pinned: u32) -> bool {
    current_pinned >= max_pinned
}
