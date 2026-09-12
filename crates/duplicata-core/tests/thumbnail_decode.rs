use duplicata_core::canonical::{CF_DIB, CF_DIBV5};
use duplicata_core::capture::{CanonicalKind, CanonicalSelection, CapturedFormat};
use duplicata_core::{build_thumbnail, MAX_THUMBNAIL_DIMENSION_PX};

fn synthetic_dib_24bpp(width: i32, height: i32) -> Vec<u8> {
    let row_unpadded = width as usize * 3;
    let row_bytes = row_unpadded.div_ceil(4) * 4;
    let pixel_data_size = row_bytes * height as usize;

    let mut dib = Vec::with_capacity(40 + pixel_data_size);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&width.to_le_bytes());
    dib.extend_from_slice(&height.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&24u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());

    let pixels: Vec<u8> = (0..pixel_data_size).map(|i| (i % 251) as u8).collect();
    dib.extend_from_slice(&pixels);
    dib
}

fn canonical_of(format_id: u32, kind: CanonicalKind) -> CanonicalSelection {
    CanonicalSelection {
        format_id,
        format_name: None,
        kind,
        byte_len: 0,
    }
}

#[test]
fn valid_small_dib_becomes_a_decodable_png_resized_to_the_max_dimension() {
    let dib = synthetic_dib_24bpp(300, 200);
    let formats = vec![CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: dib,
    }];
    let canonical = canonical_of(CF_DIB, CanonicalKind::Dib);

    let png = build_thumbnail(&canonical, &formats).expect("DIB válido deve gerar miniatura");
    let decoded = image::load_from_memory_with_format(&png, image::ImageFormat::Png)
        .expect("saída deve ser um PNG válido e decodificável");

    assert!(decoded.width() <= MAX_THUMBNAIL_DIMENSION_PX);
    assert!(decoded.height() <= MAX_THUMBNAIL_DIMENSION_PX);
    assert_eq!(
        decoded.width().max(decoded.height()),
        MAX_THUMBNAIL_DIMENSION_PX,
        "maior eixo deve bater exatamente no teto"
    );
}

#[test]
fn dibv5_kind_is_also_supported() {
    let dib = synthetic_dib_24bpp(10, 10);
    let formats = vec![CapturedFormat {
        format_id: CF_DIBV5,
        format_name: None,
        bytes: dib,
    }];
    let canonical = canonical_of(CF_DIBV5, CanonicalKind::DibV5);

    assert!(build_thumbnail(&canonical, &formats).is_some());
}

#[test]
fn corrupted_bytes_too_short_return_none_without_panic() {
    let formats = vec![CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: vec![0u8; 5],
    }];
    let canonical = canonical_of(CF_DIB, CanonicalKind::Dib);

    assert_eq!(build_thumbnail(&canonical, &formats), None);
}

#[test]
fn unsupported_header_size_returns_none_without_panic() {
    let mut dib = vec![0u8; 40];
    dib[0..4].copy_from_slice(&9999u32.to_le_bytes());
    let formats = vec![CapturedFormat {
        format_id: CF_DIB,
        format_name: None,
        bytes: dib,
    }];
    let canonical = canonical_of(CF_DIB, CanonicalKind::Dib);

    assert_eq!(build_thumbnail(&canonical, &formats), None);
}

#[test]
fn non_image_canonical_kind_returns_none() {
    let canonical = canonical_of(13, CanonicalKind::UnicodeText);
    let formats = vec![CapturedFormat {
        format_id: 13,
        format_name: None,
        bytes: b"hello".to_vec(),
    }];

    assert_eq!(build_thumbnail(&canonical, &formats), None);
}

#[test]
fn missing_canonical_format_in_formats_returns_none_without_panic() {
    let canonical = canonical_of(CF_DIB, CanonicalKind::Dib);
    let formats: Vec<CapturedFormat> = vec![];

    assert_eq!(build_thumbnail(&canonical, &formats), None);
}
