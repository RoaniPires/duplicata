use std::cell::Cell;

use crate::identity::IdentityKey;

pub const DEDUP_WINDOW_MS: u64 = 300;

#[derive(Debug, Default)]
pub struct RecentCaptureGuard {
    last_accepted: Cell<Option<(IdentityKey, u64)>>,
}

impl RecentCaptureGuard {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_duplicate(&self, identity: IdentityKey, now_ms: u64) -> bool {
        if let Some((last_identity, last_ms)) = self.last_accepted.get() {
            if last_identity == identity && now_ms.saturating_sub(last_ms) < DEDUP_WINDOW_MS {
                return true;
            }
        }
        self.last_accepted.set(Some((identity, now_ms)));
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn id(byte: u8) -> IdentityKey {
        IdentityKey([byte; 32])
    }

    #[test]
    fn first_capture_is_never_a_duplicate() {
        let guard = RecentCaptureGuard::new();
        assert!(!guard.is_duplicate(id(1), 1_000));
    }

    #[test]
    fn same_identity_within_the_window_is_a_duplicate() {
        let guard = RecentCaptureGuard::new();
        assert!(!guard.is_duplicate(id(1), 1_000));
        assert!(guard.is_duplicate(id(1), 1_000 + DEDUP_WINDOW_MS - 1));
    }

    #[test]
    fn same_identity_at_or_after_the_window_is_not_a_duplicate() {
        let guard = RecentCaptureGuard::new();
        assert!(!guard.is_duplicate(id(1), 1_000));
        assert!(!guard.is_duplicate(id(1), 1_000 + DEDUP_WINDOW_MS));
    }

    #[test]
    fn different_identities_within_the_window_never_collapse() {
        let guard = RecentCaptureGuard::new();
        assert!(!guard.is_duplicate(id(1), 1_000));
        assert!(!guard.is_duplicate(id(2), 1_000));
    }

    #[test]
    fn a_suppressed_echo_never_slides_the_window() {
        let guard = RecentCaptureGuard::new();
        assert!(!guard.is_duplicate(id(1), 1_000));
        assert!(guard.is_duplicate(id(1), 1_000 + DEDUP_WINDOW_MS - 10));
        assert!(guard.is_duplicate(id(1), 1_000 + DEDUP_WINDOW_MS - 1));
    }
}
