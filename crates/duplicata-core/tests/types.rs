use duplicata_core::capture::{
    CanonicalKind, CanonicalSelection, CapturedFormat, RawCapture, Timestamp,
};
use duplicata_core::{
    CaptureError, IdentityKey, InitError, StoreError, UpsertOutcome, WorkerError,
};

#[test]
fn timestamp_roundtrips_millis() {
    let t = Timestamp::from_millis(1_700_000_123_456);
    assert_eq!(t.as_millis(), 1_700_000_123_456);
    assert!(Timestamp::from_millis(1) < Timestamp::from_millis(2));
}

#[test]
fn canonical_kind_strings_match_the_db_column() {
    assert_eq!(CanonicalKind::UnicodeText.as_str(), "unicode_text");
    assert_eq!(CanonicalKind::Dib.as_str(), "dib");
    assert_eq!(CanonicalKind::DibV5.as_str(), "dibv5");
    assert_eq!(CanonicalKind::HDrop.as_str(), "hdrop");
    assert_eq!(CanonicalKind::Custom.as_str(), "custom");
}

#[test]
fn captured_format_len_and_empty() {
    let f = CapturedFormat {
        format_id: 13,
        format_name: None,
        bytes: vec![1, 2, 3],
    };
    assert_eq!(f.len(), 3);
    assert!(!f.is_empty());

    let e = CapturedFormat {
        format_id: 1,
        format_name: Some("x".into()),
        bytes: vec![],
    };
    assert!(e.is_empty());
}

#[test]
fn raw_capture_new_sums_total_bytes() {
    let canonical = CanonicalSelection {
        format_id: 13,
        format_name: None,
        kind: CanonicalKind::UnicodeText,
        byte_len: 10,
    };
    let cap = RawCapture::new(
        vec![
            CapturedFormat {
                format_id: 13,
                format_name: None,
                bytes: vec![0; 10],
            },
            CapturedFormat {
                format_id: 49,
                format_name: Some("HTML Format".into()),
                bytes: vec![0; 25],
            },
        ],
        canonical,
        Timestamp::from_millis(1),
    );
    assert_eq!(cap.total_bytes, 35);
    assert_eq!(cap.formats.len(), 2);
    assert_eq!(cap.captured_at.as_millis(), 1);
}

#[test]
fn identity_key_debug_is_hex_and_as_bytes_roundtrips() {
    let mut bytes = [0u8; 32];
    bytes[0] = 0xAB;
    bytes[31] = 0x0F;
    let key = IdentityKey(bytes);
    assert_eq!(key.as_bytes(), &bytes);
    let dbg = format!("{key:?}");
    assert!(dbg.starts_with("IdentityKey(ab"));
    assert!(dbg.ends_with("0f)"));
    assert_eq!(dbg.len(), "IdentityKey(".len() + 64 + 1);
}

#[test]
fn upsert_outcome_clip_id_is_branch_agnostic() {
    assert_eq!(UpsertOutcome::Inserted { clip_id: 7 }.clip_id(), 7);
    assert_eq!(UpsertOutcome::Deduped { clip_id: 9 }.clip_id(), 9);
}

#[test]
fn capture_record_canonical_bytes_finds_the_canonical_format() {
    use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
    use duplicata_core::CaptureRecord;

    let rec = CaptureRecord {
        identity: IdentityKey([0; 32]),
        canonical: CanonicalSelection {
            format_id: 13,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: 3,
        },
        captured_at: Timestamp::from_millis(0),
        total_bytes: 5,
        preview: None,
        thumbnail: None,
        has_text: true,
        formats: vec![
            CapturedFormat {
                format_id: 1,
                format_name: None,
                bytes: vec![9, 9],
            },
            CapturedFormat {
                format_id: 13,
                format_name: None,
                bytes: vec![1, 2, 3],
            },
        ],
    };
    assert_eq!(rec.canonical_bytes(), &[1, 2, 3]);
}

#[test]
fn error_display_and_conversions() {
    assert_eq!(
        CaptureError::Busy.to_string(),
        "área de transferência ocupada"
    );
    assert_eq!(
        CaptureError::TooLarge { byte_len: 42 }.to_string(),
        "captura acima do limite (42 bytes)"
    );
    assert_eq!(
        CaptureError::Unavailable.to_string(),
        "área de transferência indisponível"
    );
    assert_eq!(
        CaptureError::Empty.to_string(),
        "área de transferência vazia"
    );

    let we: WorkerError = StoreError::Corrupted.into();
    assert_eq!(we, WorkerError::Store(StoreError::Corrupted));
    assert!(we.to_string().contains("banco corrompido"));
    assert_eq!(
        WorkerError::NoCanonicalFormat.to_string(),
        "captura sem formato canônico"
    );

    let ie: InitError = StoreError::Io.into();
    assert_eq!(ie, InitError::OpenDb(StoreError::Io));
    assert!(ie.to_string().contains("falha de I/O"));

    for e in [
        StoreError::Corrupted,
        StoreError::Io,
        StoreError::Migration,
        StoreError::Query,
    ] {
        assert!(!e.to_string().is_empty());
    }
    for e in [
        InitError::Paths,
        InitError::CreateDir,
        InitError::Logging,
        InitError::Listener,
        InitError::Tray,
        InitError::HistoryWindow,
    ] {
        assert!(!e.to_string().is_empty());
    }
}
