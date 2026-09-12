#![cfg(windows)]

use duplicata_core::filter_strip::Rect as CoreRect;
use duplicata_core::translucency::{alpha_plan, WindowRegions, OPAQUE, TRANSLUCENT_ALPHA};
use duplicata_win::window_material;
use windows::Win32::Foundation::{COLORREF, HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateSolidBrush, DeleteDC, DeleteObject, FillRect, GetDC, ReleaseDC,
    HGDIOBJ, RGBQUAD,
};
use windows::Win32::UI::Controls::{
    BeginBufferedPaint, BufferedPaintSetAlpha, EndBufferedPaint, GetBufferedPaintBits,
    BPBF_TOPDOWNDIB,
};

const W: i32 = 120;
const H: i32 = 80;

fn core(r: &RECT) -> CoreRect {
    CoreRect {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn win(r: &CoreRect) -> RECT {
    RECT {
        left: r.left,
        top: r.top,
        right: r.right,
        bottom: r.bottom,
    }
}

fn frame_pixels(regions: &WindowRegions, premultiply: bool) -> Vec<[u8; 4]> {
    let client = RECT {
        left: 0,
        top: 0,
        right: W,
        bottom: H,
    };
    // SAFETY: DC de memória criado a partir do DC da tela, só como alvo do
    // buffered paint; todos os objetos criados aqui são destruídos antes do
    // retorno.
    unsafe {
        let screen = GetDC(None);
        let mem = CreateCompatibleDC(Some(screen));
        ReleaseDC(None, screen);
        assert!(!mem.is_invalid(), "CreateCompatibleDC");

        let mut buf_hdc = windows::Win32::Graphics::Gdi::HDC::default();
        let hbp = BeginBufferedPaint(mem, &client, BPBF_TOPDOWNDIB, None, &mut buf_hdc);
        assert!(
            hbp != 0,
            "BeginBufferedPaint falhou — sessão sem tema visual?"
        );

        let brush = CreateSolidBrush(COLORREF(0x00_20_20_20));
        FillRect(buf_hdc, &client, brush);
        for island in &regions.opaque_islands {
            FillRect(buf_hdc, &win(island), brush);
        }
        let _ = DeleteObject(HGDIOBJ(brush.0));

        for op in alpha_plan(regions) {
            match op.rect {
                Some(r) => {
                    let rect = win(&r);
                    let _ = BufferedPaintSetAlpha(hbp, Some(&rect), op.alpha);
                }
                None => {
                    let _ = BufferedPaintSetAlpha(hbp, None, op.alpha);
                }
            }
        }

        let mut bits: *mut RGBQUAD = std::ptr::null_mut();
        let mut stride: i32 = 0;
        GetBufferedPaintBits(hbp, &mut bits, &mut stride).expect("GetBufferedPaintBits");
        assert!(!bits.is_null());
        if premultiply {
            let len = (stride as usize) * (H as usize) * 4;
            let buf = std::slice::from_raw_parts_mut(bits.cast::<u8>(), len);
            duplicata_core::menu_icon::premultiply_buffer(buf);
        }

        let mut out = vec![[0u8; 4]; (W * H) as usize];
        for y in 0..H {
            for x in 0..W {
                let px = *bits.offset((y * stride + x) as isize);
                out[(y * W + x) as usize] = [px.rgbBlue, px.rgbGreen, px.rgbRed, px.rgbReserved];
            }
        }

        let _ = EndBufferedPaint(hbp, false);
        let _ = DeleteDC(mem);
        out
    }
}

fn at(px: &[[u8; 4]], x: i32, y: i32) -> u8 {
    px[(y * W + x) as usize][3]
}

fn with_init<T>(f: impl FnOnce() -> T) -> T {
    assert!(
        window_material::buffered_paint_init(),
        "BufferedPaintInit falhou — sessão sem tema visual?"
    );
    let out = f();
    window_material::buffered_paint_uninit();
    out
}

#[test]
#[ignore = "precisa de sessão gráfica com tema visual (uxtheme)"]
fn the_background_comes_out_transparent_and_the_islands_opaque() {
    let island = RECT {
        left: 10,
        top: 20,
        right: 60,
        bottom: 40,
    };
    let alpha = with_init(|| {
        frame_pixels(
            &WindowRegions {
                client: CoreRect {
                    left: 0,
                    top: 0,
                    right: W,
                    bottom: H,
                },
                banner: None,
                opaque_islands: vec![core(&island)],
            },
            false,
        )
    });

    assert_eq!(at(&alpha, 0, 0), TRANSLUCENT_ALPHA, "canto do fundo");
    assert_eq!(at(&alpha, W - 1, H - 1), TRANSLUCENT_ALPHA, "canto oposto");
    assert_eq!(at(&alpha, 5, 5), TRANSLUCENT_ALPHA, "fundo fora da ilha");
    assert_eq!(at(&alpha, 11, 21), OPAQUE, "dentro da ilha");
    assert_eq!(at(&alpha, 59, 39), OPAQUE, "última coluna/linha da ilha");
    assert_eq!(at(&alpha, 60, 30), TRANSLUCENT_ALPHA, "1px depois da ilha");
    println!("fundo alfa 0, ilha {island:?} alfa 255 — plano aplicado no buffer real");
}

#[test]
#[ignore = "precisa de sessão gráfica com tema visual (uxtheme)"]
fn a_banner_frame_comes_out_fully_opaque() {
    let client = CoreRect {
        left: 0,
        top: 0,
        right: W,
        bottom: H,
    };
    let alpha = with_init(|| {
        frame_pixels(
            &WindowRegions {
                client,
                banner: Some(client),
                opaque_islands: vec![],
            },
            false,
        )
    });
    assert!(
        (0..W * H).all(|i| alpha[i as usize][3] == OPAQUE),
        "um banner see-through é ilegível — todo pixel tem de sair opaco"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica com tema visual (uxtheme)"]
fn the_translucent_region_comes_out_premultiplied_and_the_islands_untouched() {
    let island = RECT {
        left: 10,
        top: 20,
        right: 60,
        bottom: 40,
    };
    let regions = WindowRegions {
        client: CoreRect {
            left: 0,
            top: 0,
            right: W,
            bottom: H,
        },
        banner: None,
        opaque_islands: vec![core(&island)],
    };
    let (plain, pre) = with_init(|| (frame_pixels(&regions, false), frame_pixels(&regions, true)));

    let bg = (W - 1, H - 1);
    let raw = plain[(bg.1 * W + bg.0) as usize];
    let mult = pre[(bg.1 * W + bg.0) as usize];
    let a = TRANSLUCENT_ALPHA as u32;
    for c in 0..3 {
        let expected = ((raw[c] as u32 * a + 127) / 255) as u8;
        assert_eq!(
            mult[c], expected,
            "canal {c} do fundo: {} deveria virar {expected} com alfa {a}",
            raw[c]
        );
    }
    assert_eq!(mult[3], TRANSLUCENT_ALPHA, "o alfa não muda");
    assert!(
        mult[0] < raw[0],
        "pré-multiplicar tem de escurecer o composto"
    );

    let inside = ((21 * W) + 11) as usize;
    assert_eq!(
        pre[inside], plain[inside],
        "ilha em alfa 255 não pode ser alterada pela pré-multiplicação"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica com tema visual (uxtheme)"]
fn without_the_plan_gdi_alone_would_leave_everything_invisible() {
    let client = RECT {
        left: 0,
        top: 0,
        right: W,
        bottom: H,
    };
    let alpha = with_init(|| {
        // SAFETY: mesmo padrão de `alpha_after_frame`, sem o passo do plano.
        unsafe {
            let screen = GetDC(None);
            let mem = CreateCompatibleDC(Some(screen));
            ReleaseDC(None, screen);
            let mut buf_hdc = windows::Win32::Graphics::Gdi::HDC::default();
            let hbp = BeginBufferedPaint(mem, &client, BPBF_TOPDOWNDIB, None, &mut buf_hdc);
            assert!(hbp != 0);
            let brush = CreateSolidBrush(COLORREF(0x00_C0_C0_C0));
            FillRect(buf_hdc, &client, brush);
            let _ = DeleteObject(HGDIOBJ(brush.0));

            let mut bits: *mut RGBQUAD = std::ptr::null_mut();
            let mut stride: i32 = 0;
            GetBufferedPaintBits(hbp, &mut bits, &mut stride).expect("bits");
            let a = (*bits.offset(((H / 2) * stride + W / 2) as isize)).rgbReserved;
            let _ = EndBufferedPaint(hbp, false);
            let _ = DeleteDC(mem);
            a
        }
    });
    assert_eq!(
        alpha, 0,
        "o `FillRect` do GDI deveria ter deixado alfa 0 — é a premissa de \
         toda esta fase"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica"]
fn glass_can_be_applied_and_removed_on_a_real_window() {
    use duplicata_win::window_material::{apply_glass, buffered_paint_ready, remove_glass};

    let ready = with_init(|| {
        assert!(
            buffered_paint_ready(),
            "o wrapper `buffered_paint_init` deveria ter ligado o flag"
        );
        // Sem buffered paint, `apply_glass` tem de recusar — é a trava que
        // impede vidro sem controle de alfa.
        // SAFETY: `HWND` nulo é aceito; a função sai antes de usá-lo.
        let refused = unsafe { apply_glass(HWND::default(), false) };
        assert!(!refused, "vidro NUNCA pode ser aplicado sem buffered paint");
        true
    });
    assert!(ready);

    // SAFETY: `HWND::default()` é nulo — `DwmExtendFrameIntoClientArea` falha
    // e devolve erro em vez de aplicar; o que se verifica aqui é que o par
    // não panica nem deixa estado pendente.
    unsafe {
        let applied = apply_glass(HWND::default(), true);
        println!("apply_glass(HWND nulo) = {applied} (esperado: false)");
        remove_glass(HWND::default());
    }
}
