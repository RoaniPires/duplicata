use std::io::Cursor;

use image::ImageFormat;

use crate::capture::{CanonicalKind, CanonicalSelection, CapturedFormat};

pub const MAX_THUMBNAIL_DIMENSION_PX: u32 = 128;

const BI_BITFIELDS: u32 = 3;

pub fn build_thumbnail(
    canonical: &CanonicalSelection,
    formats: &[CapturedFormat],
) -> Option<Vec<u8>> {
    if !matches!(canonical.kind, CanonicalKind::Dib | CanonicalKind::DibV5) {
        return None;
    }
    let dib = formats
        .iter()
        .find(|f| f.format_id == canonical.format_id)?
        .bytes
        .as_slice();

    let bmp_bytes = synthesize_bmp(dib)?;
    let img = image::load_from_memory_with_format(&bmp_bytes, ImageFormat::Bmp).ok()?;
    let thumb = img.thumbnail(MAX_THUMBNAIL_DIMENSION_PX, MAX_THUMBNAIL_DIMENSION_PX);

    let mut png_bytes = Vec::new();
    thumb
        .write_to(&mut Cursor::new(&mut png_bytes), ImageFormat::Png)
        .ok()?;
    Some(png_bytes)
}

fn synthesize_bmp(dib: &[u8]) -> Option<Vec<u8>> {
    if dib.len() < 40 {
        return None;
    }
    let header_size = u32::from_le_bytes(dib[0..4].try_into().ok()?);
    let bit_count = u16::from_le_bytes(dib[14..16].try_into().ok()?);
    let compression = u32::from_le_bytes(dib[16..20].try_into().ok()?);
    let colors_used = u32::from_le_bytes(dib[32..36].try_into().ok()?);

    if header_size as usize > dib.len() {
        return None;
    }

    let palette_colors = if colors_used != 0 {
        colors_used
    } else if bit_count <= 8 {
        1u32 << bit_count
    } else {
        0
    };
    let masks_bytes: u32 = if compression == BI_BITFIELDS && header_size < 108 {
        12
    } else {
        0
    };
    let palette_bytes = palette_colors.saturating_mul(4);

    let off_bits = 14u32
        .saturating_add(header_size)
        .saturating_add(masks_bytes)
        .saturating_add(palette_bytes);
    let file_size = 14u32.saturating_add(dib.len() as u32);

    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&file_size.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0]);
    out.extend_from_slice(&off_bits.to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::{synthesize_bmp, BI_BITFIELDS};

    fn dib_header(header_size: u32, bit_count: u16, compression: u32) -> Vec<u8> {
        let mut dib = vec![0u8; header_size as usize];
        dib[0..4].copy_from_slice(&header_size.to_le_bytes());
        dib[14..16].copy_from_slice(&bit_count.to_le_bytes());
        dib[16..20].copy_from_slice(&compression.to_le_bytes());
        dib
    }

    fn off_bits_of(bmp: &[u8]) -> u32 {
        u32::from_le_bytes(bmp[10..14].try_into().unwrap())
    }

    #[test]
    fn classic_bitmapinfoheader_with_bitfields_appends_the_12_mask_bytes() {
        let dib = dib_header(40, 32, BI_BITFIELDS);
        let bmp = synthesize_bmp(&dib).expect("cabeçalho de 40 bytes é válido");
        assert_eq!(off_bits_of(&bmp), 14 + 40 + 12);
    }

    #[test]
    fn bitmapv4header_already_embeds_the_masks_no_extra_12_bytes() {
        let dib = dib_header(108, 32, BI_BITFIELDS);
        let bmp = synthesize_bmp(&dib).expect("cabeçalho V4 de 108 bytes é válido");
        assert_eq!(off_bits_of(&bmp), 14 + 108);
    }

    #[test]
    fn bitmapv5header_already_embeds_the_masks_no_extra_12_bytes() {
        let dib = dib_header(124, 32, BI_BITFIELDS);
        let bmp = synthesize_bmp(&dib).expect("cabeçalho V5 de 124 bytes é válido");
        assert_eq!(off_bits_of(&bmp), 14 + 124);
    }

    #[test]
    fn non_bitfields_compression_never_adds_mask_bytes_regardless_of_header_size() {
        let dib = dib_header(40, 24, 0);
        let bmp = synthesize_bmp(&dib).expect("cabeçalho de 40 bytes é válido");
        assert_eq!(off_bits_of(&bmp), 14 + 40);
    }

    #[test]
    fn explicit_colors_used_sizes_the_palette_directly() {
        let mut dib = dib_header(40, 8, 0);
        dib[32..36].copy_from_slice(&2u32.to_le_bytes());
        let bmp = synthesize_bmp(&dib).expect("cabeçalho de 40 bytes é válido");
        assert_eq!(
            off_bits_of(&bmp),
            14 + 40 + 2 * 4,
            "paleta de 2 cores (RGBQUAD, 4 bytes cada) — nunca a paleta cheia de 256"
        );
    }

    #[test]
    fn zero_colors_used_with_8bpp_or_less_defaults_to_the_full_palette() {
        let dib = dib_header(40, 8, 0);
        let bmp = synthesize_bmp(&dib).expect("cabeçalho de 40 bytes é válido");
        assert_eq!(
            off_bits_of(&bmp),
            14 + 40 + 256 * 4,
            "8bpp sem biClrUsed explícito deve assumir a paleta cheia (2^8 = 256 cores)"
        );
    }

    #[test]
    fn zero_colors_used_above_8bpp_has_no_palette() {
        let dib = dib_header(40, 16, 0);
        let bmp = synthesize_bmp(&dib).expect("cabeçalho de 40 bytes é válido");
        assert_eq!(off_bits_of(&bmp), 14 + 40);
    }
}
