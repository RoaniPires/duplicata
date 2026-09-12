use std::collections::HashMap;

use crate::canonical::CF_UNICODETEXT;
use crate::capture::{CanonicalSelection, CapturedFormat};
use crate::error::StoreError;
use crate::reader::HistoryReader;
use crate::repository::{
    CaptureRecord, ClipListItem, HistoryRepository, SetPinnedOutcome, StoredClip, UpsertOutcome,
};

pub fn utf16le(s: &str) -> Vec<u8> {
    s.encode_utf16()
        .flat_map(|u| u.to_le_bytes())
        .chain([0, 0])
        .collect()
}

pub fn unicode_text_format(s: &str) -> CapturedFormat {
    CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: utf16le(s),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FakeClip {
    pub id: i64,
    pub first_captured_ms: u64,
    pub last_activity_ms: u64,
    pub canonical: CanonicalSelection,
    pub total_bytes: u64,
    pub preview: Option<String>,
    pub has_text: bool,
    pub formats: Vec<CapturedFormat>,
    pub pinned: bool,
}

#[derive(Debug, Default)]
pub struct FakeHistoryRepository {
    by_hash: HashMap<[u8; 32], FakeClip>,
    next_id: i64,
    pub purge_older_than_calls: u32,
    pub purge_over_count_calls: u32,
    pub last_purge_over_count_max_items: Option<u32>,
    pub toggle_encryption_calls: u32,
    pub encryption_enabled: bool,
}

impl FakeHistoryRepository {
    pub fn new() -> Self {
        FakeHistoryRepository {
            next_id: 1,
            ..Default::default()
        }
    }

    pub fn len(&self) -> usize {
        self.by_hash.len()
    }

    pub fn is_empty(&self) -> bool {
        self.by_hash.is_empty()
    }

    pub fn get(&self, identity_hash: &[u8; 32]) -> Option<&FakeClip> {
        self.by_hash.get(identity_hash)
    }

    pub fn ids_by_recency(&self) -> Vec<i64> {
        let mut clips: Vec<&FakeClip> = self.by_hash.values().collect();
        clips.sort_by(|a, b| {
            b.last_activity_ms
                .cmp(&a.last_activity_ms)
                .then(b.id.cmp(&a.id))
        });
        clips.iter().map(|c| c.id).collect()
    }
}

#[derive(Debug, Default)]
pub struct FailingHistoryRepository {
    error: Option<StoreError>,
}

impl FailingHistoryRepository {
    pub fn new(error: StoreError) -> Self {
        FailingHistoryRepository { error: Some(error) }
    }
}

impl HistoryRepository for FailingHistoryRepository {
    fn upsert(&mut self, _rec: &CaptureRecord) -> Result<UpsertOutcome, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn purge_older_than(&mut self, _cutoff_ms: u64) -> Result<u64, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn purge_over_count(&mut self, _max_items: u32) -> Result<u64, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn set_pinned(
        &mut self,
        _clip_id: i64,
        _pinned: bool,
        _max_pinned: u32,
    ) -> Result<SetPinnedOutcome, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn delete_all(&mut self, _keep_pinned: bool) -> Result<u64, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn recreate(&mut self) -> Result<(), StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn toggle_encryption(
        &mut self,
        _enable: bool,
        _progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
}

impl HistoryReader for FailingHistoryRepository {
    fn list_for_display(&self, _limit: usize) -> Result<Vec<ClipListItem>, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn get_full(&self, _clip_id: i64) -> Result<Option<StoredClip>, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn count_pinned(&self) -> Result<u32, StoreError> {
        Err(self.error.clone().unwrap_or(StoreError::Query))
    }
    fn close(self: Box<Self>) -> Result<(), StoreError> {
        Ok(())
    }
}

impl HistoryRepository for FakeHistoryRepository {
    fn upsert(&mut self, rec: &CaptureRecord) -> Result<UpsertOutcome, StoreError> {
        let ts = rec.captured_at.as_millis();
        if let Some(clip) = self.by_hash.get_mut(&rec.identity.0) {
            clip.last_activity_ms = ts;
            clip.canonical = rec.canonical.clone();
            clip.total_bytes = rec.total_bytes;
            clip.preview = rec.preview.clone();
            clip.has_text = rec.has_text;
            clip.formats = rec.formats.clone();
            Ok(UpsertOutcome::Deduped { clip_id: clip.id })
        } else {
            let id = self.next_id;
            self.next_id += 1;
            self.by_hash.insert(
                rec.identity.0,
                FakeClip {
                    id,
                    first_captured_ms: ts,
                    last_activity_ms: ts,
                    canonical: rec.canonical.clone(),
                    total_bytes: rec.total_bytes,
                    preview: rec.preview.clone(),
                    has_text: rec.has_text,
                    formats: rec.formats.clone(),
                    pinned: false,
                },
            );
            Ok(UpsertOutcome::Inserted { clip_id: id })
        }
    }

    fn purge_older_than(&mut self, cutoff_ms: u64) -> Result<u64, StoreError> {
        self.purge_older_than_calls += 1;
        let before = self.by_hash.len();
        self.by_hash
            .retain(|_, c| c.pinned || c.last_activity_ms >= cutoff_ms);
        Ok((before - self.by_hash.len()) as u64)
    }

    fn purge_over_count(&mut self, max_items: u32) -> Result<u64, StoreError> {
        self.purge_over_count_calls += 1;
        self.last_purge_over_count_max_items = Some(max_items);
        let mut non_pinned: Vec<(i64, u64)> = self
            .by_hash
            .values()
            .filter(|c| !c.pinned)
            .map(|c| (c.id, c.last_activity_ms))
            .collect();
        if non_pinned.len() as u32 <= max_items {
            return Ok(0);
        }
        non_pinned.sort_by(|a, b| b.1.cmp(&a.1).then(b.0.cmp(&a.0)));
        let to_remove: Vec<i64> = non_pinned
            .split_off(max_items as usize)
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let removed = to_remove.len() as u64;
        self.by_hash.retain(|_, c| !to_remove.contains(&c.id));
        Ok(removed)
    }

    fn set_pinned(
        &mut self,
        clip_id: i64,
        pinned: bool,
        max_pinned: u32,
    ) -> Result<SetPinnedOutcome, StoreError> {
        let Some(current_pinned) = self
            .by_hash
            .values()
            .find(|c| c.id == clip_id)
            .map(|c| c.pinned)
        else {
            return Ok(SetPinnedOutcome::NotFound);
        };

        if !pinned {
            if let Some(clip) = self.by_hash.values_mut().find(|c| c.id == clip_id) {
                clip.pinned = false;
            }
            return Ok(SetPinnedOutcome::Applied);
        }

        if current_pinned {
            return Ok(SetPinnedOutcome::Applied);
        }

        let current = self.by_hash.values().filter(|c| c.pinned).count() as u32;
        if current >= max_pinned {
            return Ok(SetPinnedOutcome::PinCapReached {
                limit: max_pinned,
                current,
            });
        }

        if let Some(clip) = self.by_hash.values_mut().find(|c| c.id == clip_id) {
            clip.pinned = true;
        }
        Ok(SetPinnedOutcome::Applied)
    }

    fn delete_all(&mut self, keep_pinned: bool) -> Result<u64, StoreError> {
        let before = self.by_hash.len();
        if keep_pinned {
            self.by_hash.retain(|_, c| c.pinned);
        } else {
            self.by_hash.clear();
        }
        Ok((before - self.by_hash.len()) as u64)
    }

    fn recreate(&mut self) -> Result<(), StoreError> {
        self.by_hash.clear();
        self.next_id = 1;
        Ok(())
    }

    fn toggle_encryption(
        &mut self,
        enable: bool,
        progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        self.toggle_encryption_calls += 1;
        progress(1);
        progress(2);
        self.encryption_enabled = enable;
        Ok(())
    }
}

impl HistoryReader for FakeHistoryRepository {
    fn list_for_display(&self, limit: usize) -> Result<Vec<ClipListItem>, StoreError> {
        let mut clips: Vec<&FakeClip> = self.by_hash.values().collect();
        clips.sort_by(|a, b| {
            b.last_activity_ms
                .cmp(&a.last_activity_ms)
                .then(b.id.cmp(&a.id))
        });
        Ok(clips
            .into_iter()
            .take(limit)
            .map(|c| ClipListItem {
                id: c.id,
                canonical_kind: c.canonical.kind.as_str().to_string(),
                preview: c.preview.clone(),
                thumbnail: None,
                has_text: c.has_text,
                last_activity_ms: c.last_activity_ms,
                pinned: c.pinned,
            })
            .collect())
    }

    fn count_pinned(&self) -> Result<u32, StoreError> {
        Ok(self.by_hash.values().filter(|c| c.pinned).count() as u32)
    }

    fn get_full(&self, clip_id: i64) -> Result<Option<StoredClip>, StoreError> {
        Ok(self.by_hash.values().find(|c| c.id == clip_id).map(|c| {
            let text_format = c
                .has_text
                .then(|| {
                    c.formats
                        .iter()
                        .find(|f| f.format_id == CF_UNICODETEXT)
                        .map(|f| (f.format_id, f.bytes.clone()))
                })
                .flatten();
            StoredClip {
                formats: c
                    .formats
                    .iter()
                    .map(|f| (f.format_id, f.format_name.clone(), f.bytes.clone()))
                    .collect(),
                text_format,
            }
        }))
    }

    fn close(self: Box<Self>) -> Result<(), StoreError> {
        Ok(())
    }
}
