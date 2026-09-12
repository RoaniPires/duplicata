#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rgb {
    pub r: u8,
    pub g: u8,
    pub b: u8,
}

impl Rgb {
    pub const fn new(r: u8, g: u8, b: u8) -> Self {
        Rgb { r, g, b }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemeColors {
    pub surface: Rgb,
    pub text_primary: Rgb,
    pub text_dim: Rgb,
    pub border: Rgb,
}

pub const fn theme_colors(dark: bool) -> ThemeColors {
    if dark {
        ThemeColors {
            surface: Rgb::new(0x2B, 0x2B, 0x2B),
            text_primary: Rgb::new(0xF0, 0xF0, 0xF0),
            text_dim: Rgb::new(0xC0, 0xC0, 0xC0),
            border: Rgb::new(0x3F, 0x3F, 0x3F),
        }
    } else {
        ThemeColors {
            surface: Rgb::new(0xFA, 0xFA, 0xFA),
            text_primary: Rgb::new(0x1A, 0x1A, 0x1A),
            text_dim: Rgb::new(0x57, 0x57, 0x57),
            border: Rgb::new(0xD0, 0xD0, 0xD0),
        }
    }
}

pub const MIN_HINT_CONTRAST: f64 = 4.5;

pub fn relative_luminance(c: Rgb) -> f64 {
    fn channel(v: u8) -> f64 {
        let v = v as f64 / 255.0;
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    }
    0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b)
}

pub fn contrast_ratio(a: Rgb, b: Rgb) -> f64 {
    let (la, lb) = (relative_luminance(a), relative_luminance(b));
    let (hi, lo) = if la >= lb { (la, lb) } else { (lb, la) };
    (hi + 0.05) / (lo + 0.05)
}

pub fn composite_over(surface: Rgb, behind: Rgb, alpha: u8) -> Rgb {
    let a = alpha as u32;
    let mix = |s: u8, b: u8| (((s as u32 * a) + (b as u32 * (255 - a)) + 127) / 255) as u8;
    Rgb::new(
        mix(surface.r, behind.r),
        mix(surface.g, behind.g),
        mix(surface.b, behind.b),
    )
}

pub fn worst_case_contrast_translucent(text: Rgb, surface: Rgb, alpha: u8) -> f64 {
    let white = composite_over(surface, Rgb::new(0xFF, 0xFF, 0xFF), alpha);
    let black = composite_over(surface, Rgb::new(0, 0, 0), alpha);
    let (lw, lb) = (relative_luminance(white), relative_luminance(black));
    let (lo, hi) = if lw <= lb { (lw, lb) } else { (lb, lw) };
    let lt = relative_luminance(text);
    if lt >= lo && lt <= hi {
        return 1.0;
    }
    contrast_ratio(text, white).min(contrast_ratio(text, black))
}
