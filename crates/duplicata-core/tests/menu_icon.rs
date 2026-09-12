use duplicata_core::menu_icon::{
    best_entry, entries, premultiply_bgra, premultiply_buffer, settings_ico_for, SETTINGS_DARK_ICO,
    SETTINGS_LIGHT_ICO,
};

#[test]
fn a_dark_menu_gets_the_light_stroke_and_a_light_menu_the_dark_one() {
    assert_eq!(settings_ico_for(true), SETTINGS_DARK_ICO);
    assert_eq!(settings_ico_for(false), SETTINGS_LIGHT_ICO);
    assert_ne!(SETTINGS_DARK_ICO, SETTINGS_LIGHT_ICO);
}

#[test]
fn both_variants_are_real_icon_files_with_the_same_sizes() {
    let l = entries(SETTINGS_LIGHT_ICO);
    let d = entries(SETTINGS_DARK_ICO);
    assert!(!l.is_empty() && !d.is_empty());
    let ls: Vec<u32> = l.iter().map(|e| e.width).collect();
    let ds: Vec<u32> = d.iter().map(|e| e.width).collect();
    assert_eq!(ls, ds, "as duas variantes têm de cobrir os mesmos tamanhos");
    for want in [16, 20, 24, 32] {
        assert!(ls.contains(&want), "falta o tamanho {want} no .ico");
    }
    for e in l.iter().chain(d.iter()) {
        assert_eq!(e.width, e.height, "ícone não quadrado: {e:?}");
    }
}

#[test]
fn the_chosen_size_is_the_smallest_one_at_or_above_what_the_dpi_asks() {
    for (asked, want) in [(16, 16), (18, 20), (20, 20), (24, 24), (32, 32)] {
        let e = best_entry(SETTINGS_LIGHT_ICO, asked).expect("tem de escolher algo");
        assert_eq!(e.width, want, "pedido {asked}px");
    }
}

#[test]
fn asking_for_more_than_the_file_has_falls_back_to_the_largest() {
    let e = best_entry(SETTINGS_LIGHT_ICO, 512).unwrap();
    assert_eq!(e.width, 32);
}

#[test]
fn the_chosen_entry_points_inside_the_file() {
    for asked in [16, 24, 32, 999] {
        let e = best_entry(SETTINGS_DARK_ICO, asked).unwrap();
        assert!(e.offset + e.len <= SETTINGS_DARK_ICO.len());
        assert!(e.len > 0);
    }
}

#[test]
fn a_malformed_file_yields_no_entries_instead_of_panicking() {
    assert!(entries(&[]).is_empty());
    assert!(entries(&[0, 0, 1, 0]).is_empty());
    assert!(entries(&[1, 2, 3, 4, 5, 6]).is_empty());
    assert!(entries(&[0, 0, 1, 0, 3, 0]).is_empty());
    assert!(best_entry(&[], 16).is_none());
}

#[test]
fn an_entry_pointing_outside_the_buffer_is_dropped() {
    let mut ico = vec![0, 0, 1, 0, 1, 0];
    ico.extend_from_slice(&[16, 16, 0, 0, 1, 0, 32, 0]);
    ico.extend_from_slice(&9999u32.to_le_bytes());
    assert!(entries(&ico).is_empty());
    assert!(best_entry(&ico, 16).is_none());
}

#[test]
fn a_fully_opaque_pixel_is_unchanged() {
    let mut px = [0x40, 0x80, 0xC0, 0xFF];
    premultiply_bgra(&mut px);
    assert_eq!(px, [0x40, 0x80, 0xC0, 0xFF]);
}

#[test]
fn a_fully_transparent_pixel_loses_all_colour() {
    let mut px = [0xFF, 0xFF, 0xFF, 0x00];
    premultiply_bgra(&mut px);
    assert_eq!(px, [0, 0, 0, 0]);
}

#[test]
fn a_half_transparent_pixel_gets_half_its_colour() {
    let mut px = [0xFF, 0x80, 0x00, 0x80];
    premultiply_bgra(&mut px);
    assert_eq!(px[0], 0x80);
    assert_eq!(px[1], 0x40);
    assert_eq!(px[2], 0x00);
    assert_eq!(px[3], 0x80, "o alfa não muda");
}

#[test]
fn premultiplying_is_idempotent_only_at_the_extremes_and_never_overflows() {
    for a in 0u16..=255 {
        for c in [0u8, 1, 127, 128, 254, 255] {
            let mut px = [c, c, c, a as u8];
            premultiply_bgra(&mut px);
            assert!(px[0] <= c, "pré-multiplicar nunca aumenta o canal");
            assert!(px[0] as u16 <= a, "canal não pode passar do alfa");
            assert_eq!(px[3], a as u8);
        }
    }
}

#[test]
fn the_buffer_version_matches_the_pixel_version_and_ignores_a_short_tail() {
    let mut buf = vec![0xFF, 0x80, 0x00, 0x80, 0x10, 0x20, 0x30, 0x40, 0xAA, 0xBB];
    premultiply_buffer(&mut buf);
    let mut a = [0xFF, 0x80, 0x00, 0x80];
    let mut b = [0x10, 0x20, 0x30, 0x40];
    premultiply_bgra(&mut a);
    premultiply_bgra(&mut b);
    assert_eq!(&buf[0..4], &a);
    assert_eq!(&buf[4..8], &b);
    assert_eq!(&buf[8..], &[0xAA, 0xBB]);
}
