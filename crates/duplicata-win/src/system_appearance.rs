use core::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Dwm::DwmGetColorizationColor;
use windows::Win32::Graphics::Gdi::{GetSysColor, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT, LOGFONTW};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_DWORD};
use windows::Win32::UI::HiDpi::GetDpiForWindow;
use windows::Win32::UI::WindowsAndMessaging::{
    SystemParametersInfoW, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
    SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS,
};

pub use crate::window_material::MaterialUnavailableCause;
use crate::window_material::{composition_enabled, material_state};

const PERSONALIZE: PCWSTR = w!("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize");

pub const DEFAULT_DPI: u32 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ThemePalette {
    pub surface: COLORREF,
    pub text_primary: COLORREF,
    pub text_dim: COLORREF,
    pub border: COLORREF,
}

const fn rgb(r: u8, g: u8, b: u8) -> COLORREF {
    COLORREF((r as u32) | ((g as u32) << 8) | ((b as u32) << 16))
}

const fn from_core(c: duplicata_core::theme::Rgb) -> COLORREF {
    rgb(c.r, c.g, c.b)
}

pub const fn theme_palette(dark: bool) -> ThemePalette {
    let c = duplicata_core::theme::theme_colors(dark);
    ThemePalette {
        surface: from_core(c.surface),
        text_primary: from_core(c.text_primary),
        text_dim: from_core(c.text_dim),
        border: from_core(c.border),
    }
}

#[derive(Clone)]
pub struct SystemAppearance {
    pub dark: bool,

    pub selection_bg: COLORREF,
    pub selection_fg: COLORREF,

    pub palette: ThemePalette,

    pub accent: COLORREF,

    pub transparency_enabled: bool,
    pub composition_enabled: bool,
    pub material_available: bool,
    pub material_cause: Option<MaterialUnavailableCause>,

    pub message_font: LOGFONTW,
    pub dpi: u32,
}

impl SystemAppearance {
    /// # Safety
    /// `hwnd` MUST ser uma janela válida desta thread.
    pub unsafe fn load(hwnd: HWND, backdrop_supported: bool) -> Self {
        // SAFETY: `hwnd` válido (contrato). `0` = janela sem DPI associado
        // ainda; cai para 100%.
        let raw_dpi = unsafe { GetDpiForWindow(hwnd) };
        let dpi = if raw_dpi == 0 { DEFAULT_DPI } else { raw_dpi };
        Self::read(dpi, backdrop_supported)
    }

    pub fn read(dpi: u32, backdrop_supported: bool) -> Self {
        let dark = read_hkcu_dword(PERSONALIZE, w!("AppsUseLightTheme"))
            .map(|v| v == 0)
            .unwrap_or(false);
        let transparency_enabled = read_hkcu_dword(PERSONALIZE, w!("EnableTransparency"))
            .map(|v| v != 0)
            .unwrap_or(true);
        let composition = composition_enabled();
        let (material_available, material_cause) =
            material_state(backdrop_supported, composition, transparency_enabled);

        // SAFETY: os índices são constantes válidas de SYS_COLOR_INDEX.
        let (selection_bg, selection_fg) = unsafe {
            (
                COLORREF(GetSysColor(COLOR_HIGHLIGHT)),
                COLORREF(GetSysColor(COLOR_HIGHLIGHTTEXT)),
            )
        };

        SystemAppearance {
            dark,
            selection_bg,
            selection_fg,
            palette: theme_palette(dark),
            accent: read_accent(),
            transparency_enabled,
            composition_enabled: composition,
            material_available,
            material_cause,
            message_font: read_message_font(),
            dpi,
        }
    }

    pub fn read_dark() -> bool {
        read_hkcu_dword(PERSONALIZE, w!("AppsUseLightTheme"))
            .map(|v| v == 0)
            .unwrap_or(false)
    }

    pub fn system_defaults() -> Self {
        Self::read(DEFAULT_DPI, false)
    }
}

fn read_hkcu_dword(subkey: PCWSTR, value: PCWSTR) -> Option<u32> {
    let mut data: u32 = 0;
    let mut size = core::mem::size_of::<u32>() as u32;
    // SAFETY: `pvdata`/`pcbdata` apontam para valores vivos durante a chamada;
    // `RRF_RT_REG_DWORD` restringe a escrita a um único DWORD.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            subkey,
            value,
            RRF_RT_REG_DWORD,
            None,
            Some(&mut data as *mut u32 as *mut c_void),
            Some(&mut size),
        )
    };
    err.is_ok().then_some(data)
}

fn read_accent() -> COLORREF {
    let mut argb: u32 = 0;
    let mut opaque = windows::core::BOOL::from(false);
    // SAFETY: ambos os ponteiros apontam para valores vivos durante a chamada.
    let ok = unsafe { DwmGetColorizationColor(&mut argb, &mut opaque) }.is_ok();
    if ok {
        let r = (argb >> 16) & 0xFF;
        let g = (argb >> 8) & 0xFF;
        let b = argb & 0xFF;
        COLORREF(r | (g << 8) | (b << 16))
    } else {
        rgb(0x00, 0x78, 0xD4)
    }
}

fn read_message_font() -> LOGFONTW {
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: core::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    // SAFETY: `cbSize` preenchido (exigido pela API); a chamada só lê para o
    // buffer que passamos. Mesmo padrão de `settings_dialog::ui_font`.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    if ok {
        ncm.lfMessageFont
    } else {
        LOGFONTW::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_palette_differs_between_light_and_dark() {
        let light = theme_palette(false);
        let dark = theme_palette(true);
        assert_ne!(light.surface, dark.surface);
        assert_ne!(light.text_primary, dark.text_primary);
        assert_ne!(light.text_dim, dark.text_dim);
        assert_ne!(light.border, dark.border);
    }

    #[test]
    fn dark_surface_is_darker_than_light_surface() {
        let sum = |c: COLORREF| (c.0 & 0xFF) + ((c.0 >> 8) & 0xFF) + ((c.0 >> 16) & 0xFF);
        assert!(sum(theme_palette(true).surface) < sum(theme_palette(false).surface));
        assert!(sum(theme_palette(true).text_primary) > sum(theme_palette(false).text_primary));
    }

    #[test]
    fn rgb_helper_packs_into_colorref_byte_order() {
        assert_eq!(rgb(0x12, 0x34, 0x56).0, 0x0056_3412);
    }
}
