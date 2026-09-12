#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogFields {
    pub code: &'static str,
    pub kind: Option<&'static str>,
    pub format_ids: Option<Vec<u32>>,
    pub byte_len: Option<u64>,
    pub attempts: Option<u32>,
    pub clip_id: Option<i64>,
    pub requested: Option<u64>,
    pub applied: Option<u64>,
}

impl LogFields {
    pub fn new(code: &'static str) -> Self {
        LogFields {
            code,
            ..Default::default()
        }
    }

    pub fn kind(mut self, kind: &'static str) -> Self {
        self.kind = Some(kind);
        self
    }

    pub fn format_ids(mut self, ids: impl IntoIterator<Item = u32>) -> Self {
        self.format_ids = Some(ids.into_iter().collect());
        self
    }

    pub fn byte_len(mut self, n: u64) -> Self {
        self.byte_len = Some(n);
        self
    }

    pub fn attempts(mut self, n: u32) -> Self {
        self.attempts = Some(n);
        self
    }

    pub fn clip_id(mut self, id: i64) -> Self {
        self.clip_id = Some(id);
        self
    }

    pub fn requested(mut self, n: u64) -> Self {
        self.requested = Some(n);
        self
    }

    pub fn applied(mut self, n: u64) -> Self {
        self.applied = Some(n);
        self
    }
}

#[macro_export]
macro_rules! log_event {
    ($level:expr, $fields:expr $(,)?) => {{
        let __f: $crate::log_fields::LogFields = $fields;
        ::tracing::event!(
            $level,
            code = __f.code,
            kind = __f.kind,
            format_ids = ?__f.format_ids,
            byte_len = __f.byte_len,
            attempts = __f.attempts,
            clip_id = __f.clip_id,
            requested = __f.requested,
            applied = __f.applied,
        );
    }};
}
