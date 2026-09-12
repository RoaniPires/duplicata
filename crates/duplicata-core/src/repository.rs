use crate::capture::{CanonicalSelection, CapturedFormat, Timestamp};
use crate::error::StoreError;
use crate::identity::IdentityKey;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureRecord {
    pub identity: IdentityKey,
    pub canonical: CanonicalSelection,
    pub captured_at: Timestamp,
    pub total_bytes: u64,
    pub preview: Option<String>,
    pub thumbnail: Option<Vec<u8>>,
    pub has_text: bool,
    pub formats: Vec<CapturedFormat>,
}

impl CaptureRecord {
    pub fn canonical_bytes(&self) -> &[u8] {
        self.formats
            .iter()
            .find(|f| f.format_id == self.canonical.format_id)
            .map(|f| f.bytes.as_slice())
            .unwrap_or(&[])
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpsertOutcome {
    Inserted { clip_id: i64 },
    Deduped { clip_id: i64 },
}

impl UpsertOutcome {
    pub fn clip_id(self) -> i64 {
        match self {
            UpsertOutcome::Inserted { clip_id } | UpsertOutcome::Deduped { clip_id } => clip_id,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SetPinnedOutcome {
    Applied,
    PinCapReached { limit: u32, current: u32 },
    NotFound,
}

pub trait HistoryRepository: Send {
    fn upsert(&mut self, rec: &CaptureRecord) -> Result<UpsertOutcome, StoreError>;

    fn purge_older_than(&mut self, cutoff_ms: u64) -> Result<u64, StoreError>;

    fn purge_over_count(&mut self, max_items: u32) -> Result<u64, StoreError>;

    fn set_pinned(
        &mut self,
        clip_id: i64,
        pinned: bool,
        max_pinned: u32,
    ) -> Result<SetPinnedOutcome, StoreError>;

    fn delete_all(&mut self, keep_pinned: bool) -> Result<u64, StoreError>;

    fn recreate(&mut self) -> Result<(), StoreError>;

    fn toggle_encryption(&mut self, enable: bool, progress: &dyn Fn(u64))
        -> Result<(), StoreError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClipListItem {
    pub id: i64,
    pub canonical_kind: String,
    pub preview: Option<String>,
    pub thumbnail: Option<Vec<u8>>,
    pub has_text: bool,
    pub last_activity_ms: u64,
    pub pinned: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredClip {
    pub formats: Vec<(u32, Option<String>, Vec<u8>)>,
    pub text_format: Option<(u32, Vec<u8>)>,
}
