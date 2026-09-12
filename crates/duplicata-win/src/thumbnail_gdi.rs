use std::collections::HashMap;

use windows::Win32::Graphics::Gdi::{
    CreateDIBSection, DeleteObject, GetDC, ReleaseDC, BITMAPINFO, BITMAPINFOHEADER, BI_RGB,
    DIB_RGB_COLORS, HBITMAP,
};

pub fn decode(png_bytes: &[u8]) -> Option<(HBITMAP, u32, u32)> {
    let img = image::load_from_memory_with_format(png_bytes, image::ImageFormat::Png).ok()?;
    let rgba = img.to_rgba8();
    let (width, height) = rgba.dimensions();
    if width == 0 || height == 0 {
        return None;
    }

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: core::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width as i32,
            biHeight: -(height as i32),
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    // SAFETY: `GetDC(None)` pega o DC da tela, só para `CreateDIBSection`
    // resolver o formato de pixel — não é a janela de destino do
    // `StretchBlt` (isso é decidido por quem chama `decode`, no `WM_PAINT`);
    // `ReleaseDC` sempre chamado antes de qualquer retorno.
    let screen_dc = unsafe { GetDC(None) };
    // SAFETY: `bmi` descreve um bitmap 32bpp top-down válido; `bits_ptr`
    // recebe o ponteiro para o buffer que `CreateDIBSection` aloca — só é
    // lido/escrito depois que a chamada tiver sucesso.
    let hbitmap = unsafe {
        CreateDIBSection(
            Some(screen_dc),
            &bmi,
            DIB_RGB_COLORS,
            &mut bits_ptr,
            None,
            0,
        )
    };
    // SAFETY: libera o DC pego acima, único uso dele.
    unsafe { ReleaseDC(None, screen_dc) };

    let hbitmap = hbitmap.ok()?;
    if bits_ptr.is_null() {
        return None;
    }

    // SAFETY: `bits_ptr` aponta para `width * height * 4` bytes recém-
    // alocados por `CreateDIBSection` (32bpp), ainda sob nossa posse (só
    // devolvida ao chamador depois deste bloco); `image` entrega RGBA, GDI
    // espera BGRA — troca R/B por pixel.
    unsafe {
        let dst =
            std::slice::from_raw_parts_mut(bits_ptr.cast::<u8>(), (width * height * 4) as usize);
        for (i, px) in rgba.pixels().enumerate() {
            let [r, g, b, a] = px.0;
            let o = i * 4;
            dst[o] = b;
            dst[o + 1] = g;
            dst[o + 2] = r;
            dst[o + 3] = a;
        }
    }

    Some((hbitmap, width, height))
}

#[derive(Debug, Default)]
pub struct ThumbnailCache {
    entries: HashMap<i64, (HBITMAP, u32, u32)>,
}

impl ThumbnailCache {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn get_or_decode(&mut self, clip_id: i64, png_bytes: &[u8]) -> Option<(HBITMAP, u32, u32)> {
        if let Some(&entry) = self.entries.get(&clip_id) {
            return Some(entry);
        }
        let entry = decode(png_bytes)?;
        self.entries.insert(clip_id, entry);
        Some(entry)
    }

    pub fn clear(&mut self) {
        for (_, (hbitmap, _, _)) in self.entries.drain() {
            // SAFETY: `hbitmap` foi criado por `decode` (`CreateDIBSection`)
            // e ainda não foi liberado — só entra no mapa uma vez, e o mapa
            // inteiro é drenado aqui de uma só vez.
            unsafe {
                let _ = DeleteObject(hbitmap.into());
            }
        }
    }
}

impl Drop for ThumbnailCache {
    fn drop(&mut self) {
        self.clear();
    }
}
