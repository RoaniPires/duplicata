#![cfg(windows)]

use duplicata_win::menu_icon::settings_menu_bitmap;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{DeleteObject, GetObjectW, BITMAP, HGDIOBJ};

fn bitmap_info(h: windows::Win32::Graphics::Gdi::HBITMAP) -> BITMAP {
    let mut bm = BITMAP::default();
    // SAFETY: `h` é um `HBITMAP` válido; `GetObjectW` só escreve `bm`, cujo
    // tamanho é passado corretamente.
    unsafe {
        GetObjectW(
            HGDIOBJ(h.0),
            core::mem::size_of::<BITMAP>() as i32,
            Some(&mut bm as *mut BITMAP as *mut core::ffi::c_void),
        );
    }
    bm
}

#[test]
#[ignore = "precisa de sessão gráfica (GDI, CreateIconFromResourceEx)"]
fn both_theme_variants_produce_a_square_32bpp_bitmap() {
    for dark in [false, true] {
        let hbitmap =
            settings_menu_bitmap(HWND::default(), dark).expect("o .ico embutido tem de render");
        let bm = bitmap_info(hbitmap);
        println!(
            "tema {}: {}x{}, {} bpp, stride {}",
            if dark { "escuro" } else { "claro" },
            bm.bmWidth,
            bm.bmHeight,
            bm.bmBitsPixel,
            bm.bmWidthBytes
        );
        assert!(bm.bmWidth > 0 && bm.bmHeight > 0);
        assert_eq!(bm.bmWidth, bm.bmHeight, "ícone de menu tem de ser quadrado");
        assert_eq!(bm.bmBitsPixel, 32, "hbmpItem com alfa exige 32 bpp");
        assert_eq!(bm.bmWidthBytes, bm.bmWidth * 4);
        // SAFETY: nosso, e não está selecionado em nenhum DC.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        }
    }
}

#[test]
#[ignore = "precisa de sessão gráfica (GDI)"]
fn every_pixel_comes_back_premultiplied() {
    let hbitmap = settings_menu_bitmap(HWND::default(), false).expect("bitmap");
    let bm = bitmap_info(hbitmap);
    let len = (bm.bmWidth * bm.bmHeight * 4) as usize;
    assert!(!bm.bmBits.is_null(), "CreateDIBSection dá acesso aos bits");
    // SAFETY: `bmBits` aponta para `len` bytes do DIB que criamos, vivo
    // enquanto o `HBITMAP` existir; só leitura.
    let px = unsafe { std::slice::from_raw_parts(bm.bmBits.cast::<u8>(), len) };

    let mut transparent = 0usize;
    let mut opaque = 0usize;
    for p in px.chunks_exact(4) {
        let (b, g, r, a) = (p[0], p[1], p[2], p[3]);
        assert!(
            b <= a && g <= a && r <= a,
            "pixel não pré-multiplicado: BGRA {b},{g},{r},{a} — nenhum canal \
             pode passar do alfa, senão o menu pinta uma franja em volta do glifo"
        );
        if a == 0 {
            assert_eq!(
                (b, g, r),
                (0, 0, 0),
                "pixel transparente com cor sobrando — é exatamente a franja"
            );
            transparent += 1;
        } else if a == 255 {
            opaque += 1;
        }
    }
    println!("{len} bytes: {transparent} transparentes, {opaque} opacos");
    assert!(
        transparent > 0,
        "a engrenagem tem de ter fundo transparente"
    );
    assert!(opaque > 0, "a engrenagem tem de ter traço opaco");

    // SAFETY: nosso, não selecionado em DC nenhum.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
    }
}

#[test]
#[ignore = "precisa de sessão gráfica (GDI); mede tempo"]
fn building_the_icon_is_far_below_the_fifty_millisecond_budget() {
    use std::time::Instant;
    let t = Instant::now();
    let hbitmap = settings_menu_bitmap(HWND::default(), false).expect("bitmap");
    let elapsed = t.elapsed();
    println!("montar o ícone do menu: {:?}", elapsed);
    assert!(
        elapsed.as_millis() < 25,
        "montar o ícone levou {elapsed:?} — metade do orçamento de 50ms do \
         SC-031 já é muito, o `TrackPopupMenu` ainda vem depois"
    );
    // SAFETY: nosso.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(hbitmap.0));
    }
}
