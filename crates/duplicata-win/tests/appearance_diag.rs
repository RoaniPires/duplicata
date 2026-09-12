#![cfg(windows)]

use duplicata_win::system_appearance::{theme_palette, SystemAppearance};
use duplicata_win::window_material::apply_attributes_diag;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{COLORREF, HWND};
use windows::Win32::Graphics::Gdi::{GetSysColor, COLOR_HIGHLIGHT, COLOR_HIGHLIGHTTEXT};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, CW_USEDEFAULT, WNDCLASSW,
    WS_CAPTION, WS_POPUP, WS_SYSMENU, WS_THICKFRAME,
};

extern "system" fn wp(
    h: HWND,
    m: u32,
    w: windows::Win32::Foundation::WPARAM,
    l: windows::Win32::Foundation::LPARAM,
) -> windows::Win32::Foundation::LRESULT {
    // SAFETY: repassa tudo ao handler padrão.
    unsafe { DefWindowProcW(h, m, w, l) }
}

fn hex(c: COLORREF) -> String {
    let v = c.0;
    format!(
        "0x{:06X}  (R={:>3} G={:>3} B={:>3})",
        v & 0xFF_FFFF,
        v & 0xFF,
        (v >> 8) & 0xFF,
        (v >> 16) & 0xFF
    )
}

#[test]
#[ignore = "diagnóstico visual — precisa de sessão gráfica; --nocapture"]
fn dump_system_appearance_and_dwm_results() {
    let class = PCWSTR(windows::core::w!("duplicata_diag_win").as_ptr());
    // SAFETY: registro de classe idempotente; janela criada e destruída aqui.
    let hwnd = unsafe {
        let hinst = GetModuleHandleW(None).unwrap();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wp),
            hInstance: hinst.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            Default::default(),
            class,
            windows::core::w!("diag"),
            WS_POPUP | WS_CAPTION | WS_SYSMENU | WS_THICKFRAME,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            400,
            300,
            None,
            None,
            Some(hinst.into()),
            None,
        )
        .unwrap()
    };

    let dark = SystemAppearance::read_dark();
    // SAFETY: `hwnd` acabou de ser criado nesta thread.
    let diag = unsafe { apply_attributes_diag(hwnd, dark) };
    // SAFETY: idem.
    let a = unsafe { SystemAppearance::load(hwnd, diag.backdrop == 0) };

    // SAFETY: índices constantes válidos.
    let (raw_hl, raw_hlt) = unsafe {
        (
            GetSysColor(COLOR_HIGHLIGHT),
            GetSysColor(COLOR_HIGHLIGHTTEXT),
        )
    };

    let light = theme_palette(false);
    let darkp = theme_palette(true);

    println!("\n========== SystemAppearance DIAG ==========");
    println!("dark (AppsUseLightTheme==0) .......... {dark}");
    println!(
        "transparency_enabled ................. {}",
        a.transparency_enabled
    );
    println!(
        "composition_enabled ................. {}",
        a.composition_enabled
    );
    println!(
        "material_available .................. {}",
        a.material_available
    );
    println!(
        "material_cause ..................... {:?}",
        a.material_cause
    );
    println!("dpi ................................ {}", a.dpi);
    println!("--- par de realce do sistema (usado na SELEÇÃO) ---");
    println!("GetSysColor(COLOR_HIGHLIGHT) raw u32 . 0x{raw_hl:08X}");
    println!("GetSysColor(COLOR_HIGHLIGHTTEXT) raw . 0x{raw_hlt:08X}");
    println!(
        "a.selection_bg .................... {}",
        hex(a.selection_bg)
    );
    println!(
        "a.selection_fg .................... {}",
        hex(a.selection_fg)
    );
    println!("--- paleta EM VIGOR (a.palette, dark={dark}) ---");
    println!(
        "a.palette.surface ................ {}",
        hex(a.palette.surface)
    );
    println!(
        "a.palette.text_primary ........... {}",
        hex(a.palette.text_primary)
    );
    println!(
        "a.palette.text_dim ............... {}",
        hex(a.palette.text_dim)
    );
    println!(
        "a.palette.border ................. {}",
        hex(a.palette.border)
    );
    println!("a.accent ......................... {}", hex(a.accent));
    println!("--- paleta constante (referência) ---");
    println!("theme_palette(false).surface ..... {}", hex(light.surface));
    println!(
        "theme_palette(false).text_primary  {}",
        hex(light.text_primary)
    );
    println!("theme_palette(true).surface ...... {}", hex(darkp.surface));
    println!(
        "theme_palette(true).text_primary . {}",
        hex(darkp.text_primary)
    );
    println!("--- HRESULT das chamadas do DWM (0 = S_OK) ---");
    println!(
        "DWMWA_SYSTEMBACKDROP_TYPE ......... 0x{:08X}",
        diag.backdrop
    );
    println!("DWMWA_WINDOW_CORNER_PREFERENCE .... 0x{:08X}", diag.corner);
    println!(
        "DWMWA_USE_IMMERSIVE_DARK_MODE ..... 0x{:08X}",
        diag.dark_mode
    );
    println!("--- o que PaintColors passa ao FillRect/SetTextColor ---");
    println!(
        "fundo (surface, SEMPRE opaco) .... {}",
        hex(a.palette.surface)
    );
    println!(
        "texto não selecionado ........... {}",
        hex(a.palette.text_primary)
    );
    println!("fundo seleção ................... {}", hex(a.selection_bg));
    println!("texto seleção .................. {}", hex(a.selection_fg));
    println!("==========================================\n");

    // SAFETY: janela criada acima.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}
