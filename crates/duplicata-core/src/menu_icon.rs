pub const SETTINGS_LIGHT_ICO: &[u8] = include_bytes!("../../../assets/settings_light.ico");

pub const SETTINGS_DARK_ICO: &[u8] = include_bytes!("../../../assets/settings_dark.ico");

pub const fn settings_ico_for(dark: bool) -> &'static [u8] {
    if dark {
        SETTINGS_DARK_ICO
    } else {
        SETTINGS_LIGHT_ICO
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IcoEntry {
    pub width: u32,
    pub height: u32,
    pub offset: usize,
    pub len: usize,
}

pub fn entries(ico: &[u8]) -> Vec<IcoEntry> {
    if ico.len() < 6 || ico[0] != 0 || ico[1] != 0 || ico[2] != 1 || ico[3] != 0 {
        return Vec::new();
    }
    let count = u16::from_le_bytes([ico[4], ico[5]]) as usize;
    let mut out = Vec::with_capacity(count);
    for i in 0..count {
        let e = 6 + 16 * i;
        if e + 16 > ico.len() {
            break;
        }
        let width = if ico[e] == 0 { 256 } else { ico[e] as u32 };
        let height = if ico[e + 1] == 0 {
            256
        } else {
            ico[e + 1] as u32
        };
        let len = u32::from_le_bytes([ico[e + 8], ico[e + 9], ico[e + 10], ico[e + 11]]) as usize;
        let offset =
            u32::from_le_bytes([ico[e + 12], ico[e + 13], ico[e + 14], ico[e + 15]]) as usize;
        if len == 0 || offset.saturating_add(len) > ico.len() {
            continue;
        }
        out.push(IcoEntry {
            width,
            height,
            offset,
            len,
        });
    }
    out
}

pub fn best_entry(ico: &[u8], desired_px: u32) -> Option<IcoEntry> {
    let all = entries(ico);
    all.iter()
        .filter(|e| e.width >= desired_px)
        .min_by_key(|e| e.width)
        .or_else(|| all.iter().max_by_key(|e| e.width))
        .copied()
}

pub fn premultiply_bgra(px: &mut [u8; 4]) {
    let a = px[3] as u32;
    for c in px.iter_mut().take(3) {
        *c = ((*c as u32 * a + 127) / 255) as u8;
    }
}

pub fn premultiply_buffer(bgra: &mut [u8]) {
    for px in bgra.chunks_exact_mut(4) {
        let a = px[3] as u32;
        for c in px.iter_mut().take(3) {
            *c = ((*c as u32 * a + 127) / 255) as u8;
        }
    }
}
