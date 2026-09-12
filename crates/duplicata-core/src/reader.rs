use crate::error::StoreError;
use crate::repository::{ClipListItem, StoredClip};

pub trait HistoryReader: Send {
    fn list_for_display(&self, limit: usize) -> Result<Vec<ClipListItem>, StoreError>;

    fn get_full(&self, clip_id: i64) -> Result<Option<StoredClip>, StoreError>;

    fn count_pinned(&self) -> Result<u32, StoreError>;

    fn close(self: Box<Self>) -> Result<(), StoreError>;
}
