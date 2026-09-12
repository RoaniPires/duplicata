use std::sync::mpsc;

use crate::capture::RawCapture;
use crate::error::StoreError;
use crate::repository::SetPinnedOutcome;

#[derive(Debug)]
pub enum WorkItem {
    Capture(RawCapture),
    RecoverCapture(RawCapture),
    RecreateDatabase {
        done: mpsc::Sender<Result<(), StoreError>>,
    },
    ApplySettings {
        now_ms: u64,
        retention_ms: u64,
        max_items: u32,
        max_pinned: u32,
    },
    SetPinned {
        clip_id: i64,
        pinned: bool,
        done: mpsc::Sender<Result<SetPinnedOutcome, StoreError>>,
    },
    ToggleEncryption {
        enable: bool,
        progress: mpsc::Sender<u64>,
        done: mpsc::Sender<Result<(), StoreError>>,
    },
    DeleteAll {
        keep_pinned: bool,
        done: mpsc::Sender<Result<u64, StoreError>>,
    },
}

pub type HeuristicRejectionSignal = mpsc::Sender<()>;
