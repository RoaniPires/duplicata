use std::path::PathBuf;

use duplicata_core::menu_icon::{entries, SETTINGS_DARK_ICO, SETTINGS_LIGHT_ICO};
use duplicata_core::theme::{contrast_ratio, Rgb};

const LIGHT_BG: Rgb = Rgb::new(0xF3, 0xF3, 0xF3);
const DARK_BG: Rgb = Rgb::new(0x20, 0x20, 0x20);

const MIN_RATIO: f64 = 3.0;

const MIN_BAND_FRACTION: f64 = 0.25;

fn asset(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets")
        .join(name)
}

struct Image {
    w: usize,
    h: usize,
    px: Vec<[u8; 4]>,
}

fn decode(bytes: &[u8]) -> Image {
    let img = image::load_from_memory(bytes)
        .expect("as imagens dos .ico do projeto são PNG")
        .to_rgba8();
    let (w, h) = img.dimensions();
    Image {
        w: w as usize,
        h: h as usize,
        px: img.pixels().map(|p| p.0).collect(),
    }
}

impl Image {
    fn solid(&self, x: isize, y: isize) -> bool {
        if x < 0 || y < 0 || x >= self.w as isize || y >= self.h as isize {
            return false;
        }
        self.px[y as usize * self.w + x as usize][3] >= 128
    }

    fn band(&self, n: isize) -> Vec<Rgb> {
        let mut out = Vec::new();
        for y in 0..self.h as isize {
            for x in 0..self.w as isize {
                if !self.solid(x, y) {
                    continue;
                }
                let near_edge = (-n..=n).any(|dy| (-n..=n).any(|dx| !self.solid(x + dx, y + dy)));
                if near_edge {
                    let p = self.px[y as usize * self.w + x as usize];
                    out.push(Rgb::new(p[0], p[1], p[2]));
                }
            }
        }
        out
    }
}

fn fraction_over(px: &[Rgb], bg: Rgb) -> f64 {
    if px.is_empty() {
        return 0.0;
    }
    px.iter()
        .filter(|p| contrast_ratio(**p, bg) >= MIN_RATIO)
        .count() as f64
        / px.len() as f64
}

fn measure(ico: &[u8]) -> Vec<(u32, f64, f64)> {
    entries(ico)
        .into_iter()
        .map(|e| {
            let img = decode(&ico[e.offset..e.offset + e.len]);
            let n = ((img.w / 8).max(2)) as isize;
            let band = img.band(n);
            (
                e.width,
                fraction_over(&band, LIGHT_BG),
                fraction_over(&band, DARK_BG),
            )
        })
        .collect()
}

#[test]
fn the_app_icon_has_contrast_of_its_own_against_both_taskbars() {
    let ico = std::fs::read(asset("duplicata.ico")).expect("assets/duplicata.ico");
    let rows = measure(&ico);
    assert!(!rows.is_empty(), "o .ico não tem imagens legíveis");
    for (size, light, dark) in rows {
        println!(
            "duplicata.ico {size}px: banda >=3:1 → claro {:.1}%  escuro {:.1}%",
            light * 100.0,
            dark * 100.0
        );
        assert!(
            light >= MIN_BAND_FRACTION,
            "{size}px: só {:.1}% da banda externa cruza {MIN_RATIO}:1 contra a barra \
             CLARA (mínimo {:.0}%) — o ícone sumiria no tema claro",
            light * 100.0,
            MIN_BAND_FRACTION * 100.0
        );
        assert!(
            dark >= MIN_BAND_FRACTION,
            "{size}px: só {:.1}% da banda externa cruza {MIN_RATIO}:1 contra a barra \
             ESCURA (mínimo {:.0}%)",
            dark * 100.0,
            MIN_BAND_FRACTION * 100.0
        );
    }
}

#[test]
fn the_app_icon_covers_the_sizes_the_taskbar_asks_for() {
    let ico = std::fs::read(asset("duplicata.ico")).unwrap();
    let sizes: Vec<u32> = entries(&ico).iter().map(|e| e.width).collect();
    for want in [16, 32, 48] {
        assert!(
            sizes.contains(&want),
            "falta {want}px em duplicata.ico: {sizes:?}"
        );
    }
}

#[test]
fn each_settings_variant_contrasts_with_the_menu_it_is_for() {
    for (name, ico, bg, tema) in [
        ("settings_light", SETTINGS_LIGHT_ICO, LIGHT_BG, "claro"),
        ("settings_dark", SETTINGS_DARK_ICO, DARK_BG, "escuro"),
    ] {
        for e in entries(ico) {
            let img = decode(&ico[e.offset..e.offset + e.len]);
            let all: Vec<Rgb> = img
                .px
                .iter()
                .filter(|p| p[3] >= 128)
                .map(|p| Rgb::new(p[0], p[1], p[2]))
                .collect();
            let frac = fraction_over(&all, bg);
            println!(
                "{name} {}px no menu {tema}: {:.1}% >=3:1",
                e.width,
                frac * 100.0
            );
            assert!(
                frac >= 0.99,
                "{name} {}px: só {:.1}% do glifo cruza {MIN_RATIO}:1 contra o menu \
                 {tema} — a variante existe justamente para isso (FR-053)",
                e.width,
                frac * 100.0
            );
        }
    }
}

#[test]
fn the_settings_glyph_leaves_a_margin_so_it_does_not_touch_the_menu_text() {
    for (name, ico) in [
        ("settings_light", SETTINGS_LIGHT_ICO),
        ("settings_dark", SETTINGS_DARK_ICO),
    ] {
        for e in entries(ico) {
            let img = decode(&ico[e.offset..e.offset + e.len]);
            let touches_border = (0..img.w)
                .any(|x| img.solid(x as isize, 0) || img.solid(x as isize, img.h as isize - 1))
                || (0..img.h)
                    .any(|y| img.solid(0, y as isize) || img.solid(img.w as isize - 1, y as isize));
            assert!(
                !touches_border,
                "{name} {}px encosta na borda do canvas — sem margem interna o \
                 glifo cola no rótulo do item de menu",
                e.width
            );
        }
    }
}
