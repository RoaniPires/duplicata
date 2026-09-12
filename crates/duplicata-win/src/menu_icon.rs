use duplicata_core::menu_icon::{best_entry, premultiply_buffer, settings_ico_for};
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Gdi::{
    CreateCompatibleDC, CreateDIBSection, DeleteDC, DeleteObject, GetDC, ReleaseDC, SelectObject,
    BITMAPINFO, BITMAPINFOHEADER, BI_RGB, DIB_RGB_COLORS, HBITMAP, HGDIOBJ,
};
use windows::Win32::UI::HiDpi::{GetDpiForWindow, GetSystemMetricsForDpi};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateIconFromResourceEx, DestroyIcon, DrawIconEx, DI_NORMAL, LR_DEFAULTCOLOR, SM_CXSMICON,
};

const DEFAULT_DPI: u32 = 96;

fn menu_icon_size(hwnd: HWND) -> i32 {
    // SAFETY: `hwnd` válido; `GetDpiForWindow` devolve 0 em erro.
    let dpi = unsafe { GetDpiForWindow(hwnd) };
    let dpi = if dpi == 0 { DEFAULT_DPI } else { dpi };
    // SAFETY: `SM_CXSMICON` é um índice de métrica válido; sem pré-condição.
    let size = unsafe { GetSystemMetricsForDpi(SM_CXSMICON, dpi) };
    if size <= 0 {
        16 * dpi as i32 / DEFAULT_DPI as i32
    } else {
        size
    }
}

pub fn settings_menu_bitmap(hwnd: HWND, dark: bool) -> Option<HBITMAP> {
    let size = menu_icon_size(hwnd);
    if size <= 0 {
        return None;
    }
    let ico = settings_ico_for(dark);
    let entry = best_entry(ico, size as u32)?;
    let image = ico.get(entry.offset..entry.offset + entry.len)?;

    // SAFETY: `image` aponta para uma imagem de ícone completa dentro do
    // `.ico` embutido (`best_entry` já validou os limites). `0x00030000` é a
    // versão de formato que a documentação exige para dados de recurso de
    // ícone. O `HICON` devolvido é nosso e é destruído antes de todo retorno
    // deste bloco.
    let hicon =
        unsafe { CreateIconFromResourceEx(image, true, 0x0003_0000, size, size, LR_DEFAULTCOLOR) }
            .ok()?;

    let bitmap = render_icon_to_bitmap(hicon, size);

    // SAFETY: `hicon` foi criado logo acima por `CreateIconFromResourceEx`
    // (não é `LR_SHARED`) e não é mais usado — o conteúdo já foi desenhado no
    // DIB.
    unsafe {
        let _ = DestroyIcon(hicon);
    }
    bitmap
}

fn render_icon_to_bitmap(
    hicon: windows::Win32::UI::WindowsAndMessaging::HICON,
    size: i32,
) -> Option<HBITMAP> {
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `GetDC(None)` pega o DC da tela só para `CreateDIBSection`
    // resolver o formato de pixel; `ReleaseDC` vem logo em seguida. `bmi`
    // descreve um DIB 32 bpp válido e `bits` recebe o buffer alocado.
    let (hbitmap, mem_dc) = unsafe {
        let screen_dc = GetDC(None);
        let hbitmap = CreateDIBSection(Some(screen_dc), &bmi, DIB_RGB_COLORS, &mut bits, None, 0);
        let mem_dc = CreateCompatibleDC(Some(screen_dc));
        ReleaseDC(None, screen_dc);
        (hbitmap, mem_dc)
    };

    let hbitmap = match hbitmap {
        Ok(b) if !bits.is_null() && !mem_dc.is_invalid() => b,
        other => {
            // SAFETY: limpa o que porventura tenha sido criado antes de
            // desistir — nenhum dos dois foi entregue a ninguém.
            unsafe {
                if let Ok(b) = other {
                    let _ = DeleteObject(HGDIOBJ(b.0));
                }
                if !mem_dc.is_invalid() {
                    let _ = DeleteDC(mem_dc);
                }
            }
            return None;
        }
    };

    // SAFETY: `mem_dc` é nosso e vive até o fim deste bloco; `hbitmap` é
    // selecionado nele só durante o desenho e o objeto anterior é restaurado.
    let drawn = unsafe {
        let old = SelectObject(mem_dc, HGDIOBJ(hbitmap.0));
        let ok = DrawIconEx(mem_dc, 0, 0, hicon, size, size, 0, None, DI_NORMAL).is_ok();
        SelectObject(mem_dc, old);
        let _ = DeleteDC(mem_dc);
        ok
    };

    if !drawn {
        // SAFETY: `hbitmap` é nosso e ainda não foi entregue.
        unsafe {
            let _ = DeleteObject(HGDIOBJ(hbitmap.0));
        }
        return None;
    }

    // SAFETY: `bits` aponta para `size * size * 4` bytes recém-alocados por
    // `CreateDIBSection` (32 bpp), ainda sob nossa posse — o `HBITMAP` só é
    // devolvido ao chamador depois deste bloco, e o `mem_dc` já foi destruído,
    // então ninguém mais lê o buffer agora.
    unsafe {
        let len = (size as usize) * (size as usize) * 4;
        let buf = std::slice::from_raw_parts_mut(bits.cast::<u8>(), len);
        premultiply_buffer(buf);
    }

    Some(hbitmap)
}
