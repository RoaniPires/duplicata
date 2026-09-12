use std::collections::VecDeque;
use std::sync::{Condvar, Mutex};

pub use crate::config::QUEUE_BYTE_BUDGET;

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct PushOutcome {
    pub evicted_bytes: Vec<usize>,
}

impl PushOutcome {
    pub fn evicted_count(&self) -> usize {
        self.evicted_bytes.len()
    }

    pub fn is_clean(&self) -> bool {
        self.evicted_bytes.is_empty()
    }
}

pub trait CaptureQueue<T>: Send + Sync {
    fn push(&self, item: T, size: usize) -> PushOutcome;

    fn pop(&self) -> Option<T>;

    fn close(&self);
}

#[derive(Debug)]
pub struct ByteBudgetQueue<T> {
    inner: Mutex<State<T>>,
    not_empty: Condvar,
    budget: usize,
}

#[derive(Debug)]
struct State<T> {
    items: VecDeque<(T, usize)>,
    bytes: usize,
    closed: bool,
    evicted_total: u64,
}

impl<T> ByteBudgetQueue<T> {
    pub fn new() -> Self {
        Self::with_budget(QUEUE_BYTE_BUDGET)
    }

    pub fn with_budget(budget: usize) -> Self {
        ByteBudgetQueue {
            inner: Mutex::new(State {
                items: VecDeque::new(),
                bytes: 0,
                closed: false,
                evicted_total: 0,
            }),
            not_empty: Condvar::new(),
            budget,
        }
    }

    pub fn len(&self) -> usize {
        self.inner.lock().unwrap().items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().unwrap().items.is_empty()
    }

    pub fn byte_len(&self) -> usize {
        self.inner.lock().unwrap().bytes
    }

    pub fn evicted_total(&self) -> u64 {
        self.inner.lock().unwrap().evicted_total
    }
}

impl<T> Default for ByteBudgetQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T: Send> CaptureQueue<T> for ByteBudgetQueue<T> {
    fn push(&self, item: T, size: usize) -> PushOutcome {
        let mut st = self.inner.lock().unwrap();
        let mut evicted_bytes = Vec::new();
        while !st.items.is_empty() && st.bytes + size > self.budget {
            if let Some((_, sz)) = st.items.pop_front() {
                st.bytes -= sz;
                evicted_bytes.push(sz);
            }
        }
        st.items.push_back((item, size));
        st.bytes += size;
        st.evicted_total += evicted_bytes.len() as u64;
        drop(st);
        self.not_empty.notify_one();
        PushOutcome { evicted_bytes }
    }

    fn pop(&self) -> Option<T> {
        let mut st = self.inner.lock().unwrap();
        loop {
            if let Some((item, sz)) = st.items.pop_front() {
                st.bytes -= sz;
                return Some(item);
            }
            if st.closed {
                return None;
            }
            st = self.not_empty.wait(st).unwrap();
        }
    }

    fn close(&self) {
        let mut st = self.inner.lock().unwrap();
        st.closed = true;
        drop(st);
        self.not_empty.notify_all();
    }
}
