use core::ffi::c_void;
use std::sync::atomic::{AtomicBool, Ordering};

use duplicata_core::{log_event, LogFields};
use tracing::Level;
use windows::core::BOOL;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Dwm::{
    DwmExtendFrameIntoClientArea, DwmIsCompositionEnabled, DwmSetWindowAttribute,
    DWMSBT_TRANSIENTWINDOW, DWMWA_SYSTEMBACKDROP_TYPE, DWMWA_USE_IMMERSIVE_DARK_MODE,
    DWMWA_WINDOW_CORNER_PREFERENCE, DWMWCP_ROUNDSMALL, DWM_SYSTEMBACKDROP_TYPE,
    DWM_WINDOW_CORNER_PREFERENCE,
};
use windows::Win32::UI::Controls::{BufferedPaintInit, BufferedPaintUnInit, MARGINS};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterialUnavailableCause {
    SemMaterialNaBuild,
    EfeitosDesabilitados,
    ComposicaoIndisponivel,
}

impl MaterialUnavailableCause {
    pub const fn code(self) -> &'static str {
        match self {
            MaterialUnavailableCause::SemMaterialNaBuild => "sem_material_na_build",
            MaterialUnavailableCause::EfeitosDesabilitados => "efeitos_desabilitados",
            MaterialUnavailableCause::ComposicaoIndisponivel => "composicao_indisponivel",
        }
    }
}

pub fn material_state(
    backdrop_supported: bool,
    composition_enabled: bool,
    transparency_enabled: bool,
) -> (bool, Option<MaterialUnavailableCause>) {
    if !backdrop_supported {
        return (false, Some(MaterialUnavailableCause::SemMaterialNaBuild));
    }
    if !composition_enabled {
        return (
            false,
            Some(MaterialUnavailableCause::ComposicaoIndisponivel),
        );
    }
    if !transparency_enabled {
        return (false, Some(MaterialUnavailableCause::EfeitosDesabilitados));
    }
    (true, None)
}

fn claim_once(flag: &AtomicBool) -> bool {
    flag.compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
        .is_ok()
}

static MATERIAL_DIAG_EMITTED: AtomicBool = AtomicBool::new(false);

pub fn maybe_emit_material_diag(cause: Option<MaterialUnavailableCause>) {
    if let Some(c) = cause {
        if claim_once(&MATERIAL_DIAG_EMITTED) {
            log_event!(
                Level::INFO,
                LogFields::new("material_unavailable").kind(c.code())
            );
        }
    }
}

pub fn composition_enabled() -> bool {
    // SAFETY: sem pré-condição; lê um único BOOL.
    unsafe { DwmIsCompositionEnabled() }
        .map(|b| b.as_bool())
        .unwrap_or(true)
}

/// # Safety
/// `hwnd` MUST ser uma janela válida desta thread.
pub unsafe fn apply_attributes(hwnd: HWND, dark: bool) -> bool {
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    // SAFETY: `hwnd` válido (contrato); `pvattribute` aponta para um valor do
    // tipo/tamanho que o atributo exige e vive durante toda a chamada.
    let backdrop_ok = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_SYSTEMBACKDROP_TYPE,
            &backdrop as *const DWM_SYSTEMBACKDROP_TYPE as *const c_void,
            core::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
        )
    }
    .is_ok();

    let corner = DWMWCP_ROUNDSMALL;
    // SAFETY: idem.
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_WINDOW_CORNER_PREFERENCE,
            &corner as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
            core::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
        )
    };

    let dark_flag = BOOL::from(dark);
    // SAFETY: idem; `DWMWA_USE_IMMERSIVE_DARK_MODE` espera um `BOOL` (i32).
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_flag as *const BOOL as *const c_void,
            core::mem::size_of::<BOOL>() as u32,
        )
    };

    backdrop_ok
}

#[derive(Debug, Clone, Copy)]
pub struct DwmDiag {
    pub backdrop: i32,
    pub corner: i32,
    pub dark_mode: i32,
    pub composition_enabled: bool,
}

/// # Safety
/// `hwnd` MUST ser uma janela válida desta thread.
pub unsafe fn apply_attributes_diag(hwnd: HWND, dark: bool) -> DwmDiag {
    fn hr(r: windows::core::Result<()>) -> i32 {
        match r {
            Ok(()) => 0,
            Err(e) => e.code().0,
        }
    }
    let backdrop = DWMSBT_TRANSIENTWINDOW;
    let corner = DWMWCP_ROUNDSMALL;
    let dark_flag = BOOL::from(dark);
    // SAFETY: `hwnd` válido (contrato); cada `pvattribute` aponta para um valor
    // do tipo/tamanho certo, vivo durante a chamada.
    unsafe {
        DwmDiag {
            backdrop: hr(DwmSetWindowAttribute(
                hwnd,
                DWMWA_SYSTEMBACKDROP_TYPE,
                &backdrop as *const DWM_SYSTEMBACKDROP_TYPE as *const c_void,
                core::mem::size_of::<DWM_SYSTEMBACKDROP_TYPE>() as u32,
            )),
            corner: hr(DwmSetWindowAttribute(
                hwnd,
                DWMWA_WINDOW_CORNER_PREFERENCE,
                &corner as *const DWM_WINDOW_CORNER_PREFERENCE as *const c_void,
                core::mem::size_of::<DWM_WINDOW_CORNER_PREFERENCE>() as u32,
            )),
            dark_mode: hr(DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                &dark_flag as *const BOOL as *const c_void,
                core::mem::size_of::<BOOL>() as u32,
            )),
            composition_enabled: composition_enabled(),
        }
    }
}

/// # Safety
/// `hwnd` MUST ser uma janela válida desta thread.
pub unsafe fn apply_dark_titlebar(hwnd: HWND, dark: bool) {
    let dark_flag = BOOL::from(dark);
    // SAFETY: `hwnd` válido (contrato); o atributo espera um `BOOL` (i32).
    let _ = unsafe {
        DwmSetWindowAttribute(
            hwnd,
            DWMWA_USE_IMMERSIVE_DARK_MODE,
            &dark_flag as *const BOOL as *const c_void,
            core::mem::size_of::<BOOL>() as u32,
        )
    };
}

/// # Safety
/// `hwnd` MUST ser uma janela válida.
pub unsafe fn apply_glass(hwnd: HWND, buffered_paint_ready: bool) -> bool {
    if !buffered_paint_ready {
        return false;
    }
    let margins = MARGINS {
        cxLeftWidth: -1,
        cxRightWidth: -1,
        cyTopHeight: -1,
        cyBottomHeight: -1,
    };
    // SAFETY: `hwnd` válido por contrato; `margins` vive durante a chamada.
    unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) }.is_ok()
}

/// # Safety
/// `hwnd` MUST ser uma janela válida.
pub unsafe fn remove_glass(hwnd: HWND) {
    let margins = MARGINS {
        cxLeftWidth: 0,
        cxRightWidth: 0,
        cyTopHeight: 0,
        cyBottomHeight: 0,
    };
    // SAFETY: idem `apply_glass`.
    let _ = unsafe { DwmExtendFrameIntoClientArea(hwnd, &margins) };
}

static BUFFERED_PAINT_READY: AtomicBool = AtomicBool::new(false);

pub fn buffered_paint_init() -> bool {
    // SAFETY: sem pré-condição; pareada com `buffered_paint_uninit` no
    // encerramento.
    let ok = unsafe { BufferedPaintInit() }.is_ok();
    BUFFERED_PAINT_READY.store(ok, Ordering::Relaxed);
    ok
}

pub fn buffered_paint_ready() -> bool {
    BUFFERED_PAINT_READY.load(Ordering::Relaxed)
}

pub fn buffered_paint_uninit() {
    BUFFERED_PAINT_READY.store(false, Ordering::Relaxed);
    // SAFETY: só desfaz o `BufferedPaintInit` acima.
    unsafe {
        let _ = BufferedPaintUnInit();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn material_is_available_only_when_every_condition_holds() {
        assert_eq!(material_state(true, true, true), (true, None));
    }

    #[test]
    fn each_failing_condition_maps_to_its_cause_with_the_documented_precedence() {
        assert_eq!(
            material_state(false, true, true),
            (false, Some(MaterialUnavailableCause::SemMaterialNaBuild))
        );
        assert_eq!(
            material_state(true, false, true),
            (
                false,
                Some(MaterialUnavailableCause::ComposicaoIndisponivel)
            )
        );
        assert_eq!(
            material_state(true, true, false),
            (false, Some(MaterialUnavailableCause::EfeitosDesabilitados))
        );
        assert_eq!(
            material_state(false, false, false),
            (false, Some(MaterialUnavailableCause::SemMaterialNaBuild))
        );
        assert_eq!(
            material_state(true, false, false),
            (
                false,
                Some(MaterialUnavailableCause::ComposicaoIndisponivel)
            )
        );
    }

    #[test]
    fn the_diagnostic_flag_fires_exactly_once() {
        let flag = AtomicBool::new(false);
        assert!(claim_once(&flag), "primeira vez emite");
        assert!(!claim_once(&flag), "segunda vez não");
        assert!(!claim_once(&flag), "e nunca mais");
    }

    #[test]
    fn cause_codes_are_stable_and_content_free() {
        assert_eq!(
            MaterialUnavailableCause::SemMaterialNaBuild.code(),
            "sem_material_na_build"
        );
        assert_eq!(
            MaterialUnavailableCause::EfeitosDesabilitados.code(),
            "efeitos_desabilitados"
        );
        assert_eq!(
            MaterialUnavailableCause::ComposicaoIndisponivel.code(),
            "composicao_indisponivel"
        );
    }
}
