#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Timestamp(u64);

impl Timestamp {
    pub const fn from_millis(ms: u64) -> Self {
        Timestamp(ms)
    }

    pub const fn as_millis(self) -> u64 {
        self.0
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CapturedFormat {
    pub format_id: u32,
    pub format_name: Option<String>,
    pub bytes: Vec<u8>,
}

impl CapturedFormat {
    pub fn len(&self) -> usize {
        self.bytes.len()
    }

    pub fn is_empty(&self) -> bool {
        self.bytes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FormatInfo {
    pub format_id: u32,
    pub format_name: Option<String>,
    pub byte_len: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CanonicalKind {
    UnicodeText,
    Dib,
    DibV5,
    HDrop,
    Custom,
}

impl CanonicalKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            CanonicalKind::UnicodeText => "unicode_text",
            CanonicalKind::Dib => "dib",
            CanonicalKind::DibV5 => "dibv5",
            CanonicalKind::HDrop => "hdrop",
            CanonicalKind::Custom => "custom",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalSelection {
    pub format_id: u32,
    pub format_name: Option<String>,
    pub kind: CanonicalKind,
    pub byte_len: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawCapture {
    pub formats: Vec<CapturedFormat>,
    pub canonical: CanonicalSelection,
    pub captured_at: Timestamp,
    pub total_bytes: u64,
}

impl RawCapture {
    pub fn new(
        formats: Vec<CapturedFormat>,
        canonical: CanonicalSelection,
        captured_at: Timestamp,
    ) -> Self {
        let total_bytes = formats.iter().map(|f| f.bytes.len() as u64).sum();
        RawCapture {
            formats,
            canonical,
            captured_at,
            total_bytes,
        }
    }

    pub fn canonical_bytes(&self) -> &[u8] {
        self.formats
            .iter()
            .find(|f| f.format_id == self.canonical.format_id)
            .map(|f| f.bytes.as_slice())
            .unwrap_or(&[])
    }
}
