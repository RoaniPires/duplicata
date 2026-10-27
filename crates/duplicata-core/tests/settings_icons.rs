//! Guarda dos ícones de "Configurações": o gerador em
//! `examples/settings_icons/gear.rs` tem de reproduzir, pixel a pixel, os `.ico`
//! versionados em `assets/`. Se falhar, regenere com
//! `cargo run -p duplicata-core --example settings_icons` (ou desfaça a edição à mão).

#[path = "../examples/settings_icons/gear.rs"]
mod gear;

use duplicata_core::menu_icon::{entries, SETTINGS_DARK_ICO, SETTINGS_LIGHT_ICO};
use gear::{
    gear_alpha, settings_ico, write_settings_icons, GearParams, SETTINGS_GEAR, SIZES, VARIANTS,
};

fn versioned(name: &str) -> &'static [u8] {
    match name {
        "settings_light.ico" => SETTINGS_LIGHT_ICO,
        "settings_dark.ico" => SETTINGS_DARK_ICO,
        other => panic!("variante desconhecida: {other}"),
    }
}

/// Cada entrada do `.ico` como (tamanho, pixels RGBA decodificados).
fn decoded(ico: &[u8]) -> Vec<(u32, image::RgbaImage)> {
    entries(ico)
        .into_iter()
        .map(|e| {
            let img = image::load_from_memory(&ico[e.offset..e.offset + e.len])
                .expect("entrada do .ico decodificável")
                .to_rgba8();
            (e.width, img)
        })
        .collect()
}

#[test]
fn the_gear_coverage_matches_the_alpha_of_the_versioned_icons() {
    for (name, _) in VARIANTS {
        for (size, img) in decoded(versioned(name)) {
            let alpha = gear_alpha(size, &SETTINGS_GEAR);
            let want: Vec<u8> = img.pixels().map(|p| p.0[3]).collect();
            if let Some(i) = (0..want.len()).find(|&i| alpha.get(i) != Some(&want[i])) {
                panic!(
                    "{name} {size}px: alfa diferente em ({}, {}): gerado {:?}, versionado {}",
                    i as u32 % size,
                    i as u32 / size,
                    alpha.get(i),
                    want[i]
                );
            }
            assert_eq!(alpha.len(), want.len(), "{name} {size}px: número de pixels");
        }
    }
}

/// Compara, entrada a entrada e pixel a pixel, um `.ico` gerado com o versionado `name`.
fn compare(name: &str, generated: &[u8], versioned: &[u8]) -> Result<(), String> {
    let got = decoded(generated);
    let want = decoded(versioned);
    let got_sizes: Vec<u32> = got.iter().map(|(s, _)| *s).collect();
    let want_sizes: Vec<u32> = want.iter().map(|(s, _)| *s).collect();
    if got_sizes != want_sizes {
        return Err(format!(
            "{name}: tamanhos gerados {got_sizes:?}, versionados {want_sizes:?}"
        ));
    }
    for ((size, g), (_, w)) in got.iter().zip(&want) {
        if g.dimensions() != w.dimensions() {
            return Err(format!(
                "{name} {size}px: dimensões geradas {:?}, versionadas {:?}",
                g.dimensions(),
                w.dimensions()
            ));
        }
        if let Some((x, y, gp)) = g
            .enumerate_pixels()
            .find(|(x, y, p)| *p != w.get_pixel(*x, *y))
        {
            return Err(format!(
                "{name} {size}px: primeiro pixel diferente em ({x}, {y}): gerado {:?}, versionado {:?}",
                gp.0,
                w.get_pixel(x, y).0
            ));
        }
    }
    Ok(())
}

#[test]
fn each_generated_icon_has_four_rgba_png_entries_in_the_theme_color() {
    for (name, rgb) in VARIANTS {
        let ico = settings_ico(rgb, &SETTINGS_GEAR);
        let es = entries(&ico);
        let sizes: Vec<(u32, u32)> = es.iter().map(|e| (e.width, e.height)).collect();
        assert_eq!(
            sizes,
            vec![(16, 16), (20, 20), (24, 24), (32, 32)],
            "{name}"
        );
        for e in es {
            let blob = &ico[e.offset..e.offset + e.len];
            assert_eq!(
                &blob[..8],
                b"\x89PNG\r\n\x1a\n",
                "{name} {}px: não é PNG",
                e.width
            );
            assert_eq!(&blob[12..16], b"IHDR", "{name} {}px", e.width);
            assert_eq!(blob[24], 8, "{name} {}px: profundidade de bits", e.width);
            assert_eq!(blob[25], 6, "{name} {}px: tipo de cor RGBA", e.width);
            let img = image::load_from_memory(blob).unwrap().to_rgba8();
            assert!(
                img.pixels().all(|p| p.0[..3] == rgb),
                "{name} {}px: RGB fora da cor do tema",
                e.width
            );
        }
    }
}

#[test]
fn the_generated_icons_match_the_versioned_ones_pixel_by_pixel() {
    for (name, rgb) in VARIANTS {
        let generated = settings_ico(rgb, &SETTINGS_GEAR);
        if let Err(msg) = compare(name, &generated, versioned(name)) {
            panic!("{msg}\nregenere com `cargo run -p duplicata-core --example settings_icons`");
        }
    }
}

#[test]
fn a_single_pixel_edited_by_hand_in_the_dark_icon_is_caught() {
    let name = "settings_dark.ico";
    let mut images: Vec<(u32, Vec<u8>)> = Vec::new();
    for (size, mut img) in decoded(SETTINGS_DARK_ICO) {
        if size == 24 {
            let p = img.get_pixel_mut(5, 7);
            p.0[3] = p.0[3].wrapping_add(1);
        }
        images.push((size, gear::encode_png(size, img.as_raw())));
    }
    let tampered = gear::build_ico(&images);
    let generated = settings_ico([0xF0, 0xF0, 0xF2], &SETTINGS_GEAR);

    let err = compare(name, &generated, &tampered).expect_err("o pixel alterado passou");
    assert!(err.contains("settings_dark.ico"), "{err}");
    assert!(err.contains("24px"), "{err}");
    assert!(err.contains("(5, 7)"), "{err}");
}

#[test]
fn a_larger_tip_radius_without_regenerating_is_caught() {
    let wider = GearParams {
        tip: 0.440,
        ..SETTINGS_GEAR
    };
    let diverges = VARIANTS
        .iter()
        .any(|(name, rgb)| compare(name, &settings_ico(*rgb, &wider), versioned(name)).is_err());
    assert!(
        diverges,
        "raio da ponta 0.440 não divergiu dos .ico versionados"
    );
}

#[test]
fn writing_saves_both_icons_and_reports_their_sizes() {
    let dir = tempfile::tempdir().unwrap();
    let written = write_settings_icons(dir.path()).expect("gravar numa pasta temporária");
    assert_eq!(
        written,
        vec![
            ("settings_light.ico", SIZES.to_vec()),
            ("settings_dark.ico", SIZES.to_vec()),
        ]
    );
    for (name, rgb) in VARIANTS {
        let on_disk = std::fs::read(dir.path().join(name)).unwrap();
        assert_eq!(on_disk, settings_ico(rgb, &SETTINGS_GEAR), "{name}");
    }
}

#[test]
fn a_read_only_icon_fails_with_its_path() {
    let dir = tempfile::tempdir().unwrap();
    let target = dir.path().join("settings_light.ico");
    std::fs::write(&target, b"antigo").unwrap();
    set_read_only(&target, true);

    let result = write_settings_icons(dir.path());
    // Devolve a escrita antes de checar: o TempDir não apaga arquivo só leitura no Windows.
    set_read_only(&target, false);

    let err = result.expect_err("gravou por cima de arquivo só leitura");
    assert_eq!(err.path, target);
    assert!(
        err.to_string().contains(&target.display().to_string()),
        "a mensagem não cita o caminho: {err}"
    );
    assert!(
        std::error::Error::source(&err).is_some(),
        "sem a causa do sistema"
    );
}

fn set_read_only(path: &std::path::Path, read_only: bool) {
    let mut perm = std::fs::metadata(path).unwrap().permissions();
    perm.set_readonly(read_only);
    std::fs::set_permissions(path, perm).unwrap();
}
