#![forbid(unsafe_code)]

pub mod backoff;
pub mod canonical;
pub mod capture;
pub mod clipboard;
pub mod clock;
pub mod config;
pub mod error;
pub mod filter_strip;
pub mod heuristic_recovery;
pub mod hint_strip;
pub mod hotkey;
pub mod identity;
pub mod list_view;
pub mod log_fields;
pub mod menu_icon;
pub mod pipeline;
pub mod preview;
pub mod queue;
pub mod reader;
pub mod recent_capture_guard;
pub mod repository;
pub mod retention;
pub mod row_layout;
pub mod secret_pattern;
pub mod self_write_filter;
pub mod size_limit;
pub mod startup;
pub mod testsupport;
pub mod theme;
pub mod thumbnail;
pub mod translucency;
pub mod uninstall_data;
pub mod work_item;
pub mod worker;

pub use backoff::{capture_with_retry, BackoffPolicy, RetryOutcome};
pub use canonical::{decide, select_canonical, select_canonical_from_info, Decision, RejectReason};
pub use capture::{
    CanonicalKind, CanonicalSelection, CapturedFormat, FormatInfo, RawCapture, Timestamp,
};
pub use clipboard::{CaptureOutcome, ClipboardSource, FakeClipboardSource};
pub use clock::{Clock, FakeClock};
pub use config::{Config, ConfigFallback};
pub use error::{CaptureError, InitError, StoreError, WorkerError};
pub use filter_strip::{adjacent_segment, segment_at_packed, segment_rects_packed};
pub use heuristic_recovery::{recover_on_notification_click, RecoverOutcome};
pub use hint_strip::{hint_line, hints, Hint, HINT_STRIP_PX};
pub use hotkey::{
    format_hotkey, should_suppress_reopen, validate_hotkey, HotkeyCombo, HotkeyRejection, MOD_ALT,
    MOD_CONTROL, MOD_SHIFT, MOD_WIN, TOGGLE_DEBOUNCE_MS,
};
pub use identity::{identity_of, IdentityKey};
pub use list_view::{
    arrange_for_display, category_of, display_state, pinned_group_boundary, text_matches,
    visible_rows, ContentTypeFilter, ListDisplayState,
};
pub use log_fields::LogFields;
pub use menu_icon::{best_entry, premultiply_bgra, settings_ico_for, IcoEntry};
pub use pipeline::{capture_and_enqueue, raw_capture_from_copied};
pub use preview::build_preview;
pub use queue::{ByteBudgetQueue, CaptureQueue, PushOutcome, QUEUE_BYTE_BUDGET};
pub use reader::HistoryReader;
pub use recent_capture_guard::{RecentCaptureGuard, DEDUP_WINDOW_MS};
pub use repository::{
    CaptureRecord, ClipListItem, HistoryRepository, SetPinnedOutcome, StoredClip, UpsertOutcome,
};
pub use retention::{cutoff_ms, over_count_by, pin_cap_reached};
pub use row_layout::{badge_of, row_layout, thumbnail_target, RowBadge, RowLayout};
pub use secret_pattern::{matches_secret_pattern, SecretPatternKind};
pub use self_write_filter::SelfWriteFilter;
pub use size_limit::check_size;
pub use startup::{interpret_approval, startup_state, Approval, StartupState};
pub use testsupport::{
    unicode_text_format, utf16le, FailingHistoryRepository, FakeHistoryRepository,
};
pub use theme::{
    composite_over, contrast_ratio, theme_colors, worst_case_contrast_translucent, Rgb, ThemeColors,
};
pub use thumbnail::{build_thumbnail, MAX_THUMBNAIL_DIMENSION_PX};
pub use translucency::{alpha_plan, AlphaOp, WindowRegions};
pub use uninstall_data::{
    leftover_message, removal_order, remove_all, remove_item, DataItem, RemovalOutcome, Sensitivity,
};
pub use work_item::{HeuristicRejectionSignal, WorkItem};
pub use worker::{run_worker, WorkerCounters};
