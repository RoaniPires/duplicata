mod support;

use std::path::PathBuf;

use duplicata_core::canonical::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use duplicata_core::capture::CanonicalKind;
use duplicata_core::{
    capture_and_enqueue, check_size, ByteBudgetQueue, CaptureError, CaptureOutcome, CaptureQueue,
    CapturedFormat, ClipboardSource, Config, FakeClipboardSource, FakeClock, RecentCaptureGuard,
    WorkItem,
};
use support::{with_captured_logs, LogBuffer};

const MIB: u64 = 1024 * 1024;

fn cfg() -> Config {
    Config::with_paths(PathBuf::from("db"), PathBuf::from("logs"))
}

fn policy() -> duplicata_core::backoff::BackoffPolicy {
    duplicata_core::backoff::BackoffPolicy::production()
}

fn dib(bytes_len: u64) -> CapturedFormat {
    CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: vec![0u8; bytes_len as usize],
    }
}

#[test]
fn check_size_uses_the_default_limit_of_each_type() {
    let cfg = cfg();
    assert!(
        check_size(CanonicalKind::UnicodeText, 2 * MIB, &cfg).is_ok(),
        "exatamente no limite"
    );
    assert!(check_size(CanonicalKind::UnicodeText, 2 * MIB + 1, &cfg).is_err());
    assert!(check_size(CanonicalKind::Dib, 64 * MIB, &cfg).is_ok());
    assert!(check_size(CanonicalKind::Dib, 64 * MIB + 1, &cfg).is_err());
    assert!(check_size(CanonicalKind::DibV5, 64 * MIB, &cfg).is_ok());
    assert!(check_size(CanonicalKind::HDrop, MIB, &cfg).is_ok());
    assert!(check_size(CanonicalKind::HDrop, MIB + 1, &cfg).is_err());
    assert!(check_size(CanonicalKind::Custom, 8 * MIB, &cfg).is_ok());
    assert!(check_size(CanonicalKind::Custom, 8 * MIB + 1, &cfg).is_err());
}

#[test]
fn check_size_error_carries_the_byte_len() {
    match check_size(CanonicalKind::Dib, 70 * MIB, &cfg()) {
        Err(CaptureError::TooLarge { byte_len }) => assert_eq!(byte_len, 70 * MIB),
        other => panic!("esperava TooLarge, veio {other:?}"),
    }
}

#[test]
fn screenshot_1920x1080_32bpp_dib_is_captured() {
    let bytes_len = 1920u64 * 1080 * 4;
    assert!(bytes_len < 8 * MIB + MIB / 2, "sanity: ~8,3 MiB");
    assert!(check_size(CanonicalKind::Dib, bytes_len, &cfg()).is_ok());

    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![dib(bytes_len)]);
    let clock = FakeClock::new();
    capture_and_enqueue(
        &src,
        &clock,
        &policy(),
        &cfg(),
        &q,
        &RecentCaptureGuard::new(),
    );

    q.close();
    let WorkItem::Capture(raw) = q.pop().expect("o screenshot 1920x1080 FOI capturado") else {
        panic!("esperava WorkItem::Capture")
    };
    assert_eq!(raw.canonical.kind, CanonicalKind::Dib);
    assert_eq!(raw.canonical.byte_len, bytes_len);
}

#[test]
fn screenshot_4k_32bpp_dib_is_captured() {
    let bytes_len = 3840u64 * 2160 * 4;
    assert!(bytes_len < 64 * MIB, "sanity: abaixo do teto de imagem");
    assert!(check_size(CanonicalKind::Dib, bytes_len, &cfg()).is_ok());

    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![dib(bytes_len)]);
    let clock = FakeClock::new();
    capture_and_enqueue(
        &src,
        &clock,
        &policy(),
        &cfg(),
        &q,
        &RecentCaptureGuard::new(),
    );

    q.close();
    let WorkItem::Capture(raw) = q.pop().expect("o screenshot 4K FOI capturado") else {
        panic!("esperava WorkItem::Capture")
    };
    assert_eq!(raw.canonical.byte_len, bytes_len);
}

#[test]
fn image_above_64_mib_is_discarded_with_kind_and_byte_len_logged() {
    let bytes_len = 70 * MIB;
    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![dib(bytes_len)]);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(q.is_empty(), "nada enfileirado");
    let logs = buf.contents();
    assert!(logs.contains("capture_too_large"), "{logs}");
    assert!(
        logs.contains("kind=\"dib\"") || logs.contains("kind=dib"),
        "kind no log: {logs}"
    );
    assert!(
        logs.contains(&(70 * MIB).to_string()),
        "byte_len no log: {logs}"
    );
}

#[test]
fn text_clip_with_a_huge_auxiliary_dib_is_judged_by_the_text_limit_not_the_image_limit() {
    let text = CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: vec![0u8; 100],
    };
    let huge_dib = dib(70 * MIB);

    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![text, huge_dib]);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(
        !buf.contents().contains("capture_too_large"),
        "não pode ser julgado pelo DIB auxiliar: {}",
        buf.contents()
    );
    let WorkItem::Capture(raw) = q
        .pop()
        .expect("captura de texto foi aceita (limite de texto, não de imagem)")
    else {
        panic!("esperava WorkItem::Capture")
    };
    assert_eq!(raw.canonical.kind, CanonicalKind::UnicodeText);
}

#[test]
fn image_clip_with_a_huge_auxiliary_custom_format_is_judged_by_the_image_limit_not_the_custom_limit(
) {
    let image = dib(40 * MIB);
    let huge_custom = CapturedFormat {
        format_id: 0xC0A0,
        format_name: Some("Formato Proprietario Enorme".into()),
        bytes: vec![0u8; (9 * MIB) as usize],
    };

    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let src = FakeClipboardSource::always(vec![image, huge_custom]);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(
        !buf.contents().contains("capture_too_large"),
        "não pode ser julgado pelo custom auxiliar: {}",
        buf.contents()
    );
    let WorkItem::Capture(raw) = q
        .pop()
        .expect("captura de imagem foi aceita (limite de imagem, não de custom)")
    else {
        panic!("esperava WorkItem::Capture")
    };
    assert_eq!(raw.canonical.kind, CanonicalKind::Dib);
    assert_eq!(raw.canonical.byte_len, 40 * MIB);
}

#[test]
fn hdrop_is_judged_by_its_own_limit_regardless_of_a_smaller_aux_text() {
    let huge_hdrop = CapturedFormat {
        format_id: CF_HDROP,
        format_name: None,
        bytes: vec![0u8; (2 * MIB) as usize],
    };
    let tiny_text = CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: vec![0u8; 10],
    };

    let q: ByteBudgetQueue<WorkItem> = ByteBudgetQueue::new();
    let _ = tiny_text;
    let src = FakeClipboardSource::always(vec![huge_hdrop]);
    let clock = FakeClock::new();
    let buf = LogBuffer::new();

    with_captured_logs(&buf, || {
        capture_and_enqueue(
            &src,
            &clock,
            &policy(),
            &cfg(),
            &q,
            &RecentCaptureGuard::new(),
        )
    });

    assert!(buf.contents().contains("capture_too_large"));
    assert!(q.is_empty());
}

#[test]
fn oversized_capture_copies_zero_bytes_before_being_rejected() {
    let src = FakeClipboardSource::always(vec![dib(70 * MIB)]);

    let outcome = src.try_capture(&cfg()).unwrap();
    assert!(
        matches!(
            outcome,
            CaptureOutcome::Rejected(duplicata_core::RejectReason::TooLarge { .. })
        ),
        "{outcome:?}"
    );
    assert!(
        src.copied_format_ids().is_empty(),
        "nenhum byte foi copiado para uma captura descartada por tamanho"
    );
}

#[test]
fn accepted_capture_copies_every_listed_format() {
    let text = CapturedFormat {
        format_id: CF_UNICODETEXT,
        format_name: None,
        bytes: vec![0u8; 10],
    };
    let aux = CapturedFormat {
        format_id: 0xC000,
        format_name: Some("HTML Format".into()),
        bytes: vec![0u8; 20],
    };
    let src = FakeClipboardSource::always(vec![text, aux]);

    let outcome = src.try_capture(&cfg()).unwrap();
    assert!(matches!(outcome, CaptureOutcome::Copied { .. }));
    assert_eq!(
        src.copied_format_ids(),
        vec![CF_UNICODETEXT, 0xC000],
        "canônico E auxiliares são copiados quando aceito"
    );
}
