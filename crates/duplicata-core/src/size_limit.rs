use crate::capture::CanonicalKind;
use crate::config::Config;
use crate::error::CaptureError;

pub fn check_size(kind: CanonicalKind, byte_len: u64, cfg: &Config) -> Result<(), CaptureError> {
    let limit = cfg.limit_for(kind);
    if byte_len > limit {
        Err(CaptureError::TooLarge { byte_len })
    } else {
        Ok(())
    }
}
