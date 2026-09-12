#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct IdentityKey(pub [u8; 32]);

impl IdentityKey {
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }
}

impl core::fmt::Debug for IdentityKey {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "IdentityKey(")?;
        for b in self.0 {
            write!(f, "{b:02x}")?;
        }
        write!(f, ")")
    }
}

pub fn identity_of(canonical_bytes: &[u8]) -> IdentityKey {
    IdentityKey(*blake3::hash(canonical_bytes).as_bytes())
}
