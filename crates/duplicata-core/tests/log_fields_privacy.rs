use duplicata_core::LogFields;

#[test]
fn log_fields_has_exactly_the_allowed_fields() {
    let LogFields {
        code,
        kind,
        format_ids,
        byte_len,
        attempts,
        clip_id,
        requested,
        applied,
    } = LogFields::new("clipboard_busy")
        .kind("unicode_text")
        .format_ids([13u32, 49])
        .byte_len(4096)
        .attempts(3)
        .clip_id(7)
        .requested(0)
        .applied(1);

    let _: &'static str = code;
    let _: Option<&'static str> = kind;
    assert_eq!(code, "clipboard_busy");
    assert_eq!(kind, Some("unicode_text"));
    assert_eq!(format_ids, Some(vec![13, 49]));
    assert_eq!(byte_len, Some(4096));
    assert_eq!(attempts, Some(3));
    assert_eq!(clip_id, Some(7));
    assert_eq!(requested, Some(0));
    assert_eq!(applied, Some(1));
}

#[test]
fn debug_output_carries_only_ids_and_counts_no_free_text() {
    let synthetic = "SEGREDO-NAO-DEVE-VAZAR-a1b2c3";
    let _ = synthetic;

    let fields = LogFields::new("db_write_failed").clip_id(42).byte_len(128);
    let rendered = format!("{fields:?}");

    assert!(!rendered.contains(synthetic));
    assert!(rendered.contains("db_write_failed"));
    assert!(rendered.contains("42"));
}

#[test]
fn default_is_empty_with_static_code() {
    let f = LogFields::default();
    assert_eq!(f.code, "");
    assert!(f.kind.is_none());
    assert!(f.format_ids.is_none());
    assert!(f.byte_len.is_none());
    assert!(f.attempts.is_none());
    assert!(f.clip_id.is_none());
    assert!(f.requested.is_none());
    assert!(f.applied.is_none());
}
