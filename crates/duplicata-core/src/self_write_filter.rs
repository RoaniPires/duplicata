use std::cell::Cell;

#[derive(Debug, Default)]
pub struct SelfWriteFilter {
    pending: Cell<bool>,
}

impl SelfWriteFilter {
    pub fn new() -> Self {
        SelfWriteFilter {
            pending: Cell::new(false),
        }
    }

    pub fn mark_pending(&self) {
        self.pending.set(true);
    }

    pub fn consume_if_pending(&self) -> bool {
        self.pending.replace(false)
    }
}
