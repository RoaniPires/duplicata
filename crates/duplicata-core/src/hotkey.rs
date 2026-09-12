pub const MOD_ALT: u32 = 0x0001;
pub const MOD_CONTROL: u32 = 0x0002;
pub const MOD_SHIFT: u32 = 0x0004;
pub const MOD_WIN: u32 = 0x0008;

const VK_V: u32 = 0x56;
pub const VK_P: u32 = 0x50;
const VK_DELETE: u32 = 0x2E;
pub const VK_ESCAPE: u32 = 0x1B;
const VK_F1: u32 = 0x70;

const VK_BACK: u32 = 0x08;
pub const VK_TAB: u32 = 0x09;
pub const VK_RETURN: u32 = 0x0D;
const VK_SPACE: u32 = 0x20;
const VK_PRIOR: u32 = 0x21;
const VK_NEXT: u32 = 0x22;
const VK_END: u32 = 0x23;
const VK_HOME: u32 = 0x24;
const VK_LEFT: u32 = 0x25;
const VK_UP: u32 = 0x26;
const VK_RIGHT: u32 = 0x27;
const VK_DOWN: u32 = 0x28;
const VK_INSERT: u32 = 0x2D;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HotkeyCombo {
    pub modifiers: u32,
    pub vkey: u32,
}

impl HotkeyCombo {
    pub const DEFAULT: HotkeyCombo = HotkeyCombo {
        modifiers: MOD_CONTROL | MOD_SHIFT,
        vkey: VK_V,
    };

    pub fn parse(s: &str) -> Option<HotkeyCombo> {
        let mut modifiers = 0u32;
        let mut vkey: Option<u32> = None;
        for part in s.split('+') {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            match part.to_ascii_lowercase().as_str() {
                "ctrl" | "control" => modifiers |= MOD_CONTROL,
                "alt" => modifiers |= MOD_ALT,
                "shift" => modifiers |= MOD_SHIFT,
                "win" | "windows" => modifiers |= MOD_WIN,
                key => {
                    if vkey.is_some() {
                        return None;
                    }
                    vkey = Some(vkey_from_str(key)?);
                }
            }
        }
        Some(HotkeyCombo {
            modifiers,
            vkey: vkey.unwrap_or(0),
        })
    }

    pub fn format(&self) -> String {
        let mut parts = Vec::new();
        if self.modifiers & MOD_CONTROL != 0 {
            parts.push("ctrl".to_string());
        }
        if self.modifiers & MOD_ALT != 0 {
            parts.push("alt".to_string());
        }
        if self.modifiers & MOD_SHIFT != 0 {
            parts.push("shift".to_string());
        }
        if self.modifiers & MOD_WIN != 0 {
            parts.push("win".to_string());
        }
        if self.vkey != 0 {
            parts.push(vkey_to_str(self.vkey));
        }
        parts.join("+")
    }
}

pub fn format_hotkey(combo: &HotkeyCombo) -> String {
    let mut out = String::new();
    let mut push = |s: &str| {
        if !out.is_empty() {
            out.push('+');
        }
        out.push_str(s);
    };
    if combo.modifiers & MOD_CONTROL != 0 {
        push("Ctrl");
    }
    if combo.modifiers & MOD_SHIFT != 0 {
        push("Shift");
    }
    if combo.modifiers & MOD_ALT != 0 {
        push("Alt");
    }
    if combo.modifiers & MOD_WIN != 0 {
        push("Win");
    }
    if combo.vkey != 0 {
        push(&vkey_display_name(combo.vkey));
    }
    out
}

fn vkey_display_name(vkey: u32) -> String {
    if (0x41..=0x5A).contains(&vkey) {
        return ((b'A' + (vkey - 0x41) as u8) as char).to_string();
    }
    if (0x30..=0x39).contains(&vkey) {
        return ((b'0' + (vkey - 0x30) as u8) as char).to_string();
    }
    if (VK_F1..=VK_F1 + 23).contains(&vkey) {
        return format!("F{}", vkey - VK_F1 + 1);
    }
    match vkey {
        VK_RETURN => "Enter".to_string(),
        VK_ESCAPE => "Esc".to_string(),
        VK_DELETE => "Delete".to_string(),
        VK_SPACE => "Space".to_string(),
        VK_TAB => "Tab".to_string(),
        VK_BACK => "Backspace".to_string(),
        VK_HOME => "Home".to_string(),
        VK_END => "End".to_string(),
        VK_PRIOR => "PageUp".to_string(),
        VK_NEXT => "PageDown".to_string(),
        VK_LEFT => "Left".to_string(),
        VK_UP => "Up".to_string(),
        VK_RIGHT => "Right".to_string(),
        VK_DOWN => "Down".to_string(),
        VK_INSERT => "Insert".to_string(),
        other => format!("0x{other:X}"),
    }
}

fn vkey_from_str(key: &str) -> Option<u32> {
    match key {
        "delete" | "del" => return Some(VK_DELETE),
        "escape" | "esc" => return Some(VK_ESCAPE),
        _ => {}
    }
    if let Some(rest) = key.strip_prefix("0x") {
        return u32::from_str_radix(rest, 16).ok();
    }
    if let Some(rest) = key.strip_prefix('f') {
        if let Ok(n) = rest.parse::<u32>() {
            if (1..=24).contains(&n) {
                return Some(VK_F1 + (n - 1));
            }
        }
    }
    let mut chars = key.chars();
    let (Some(c), None) = (chars.next(), chars.next()) else {
        return None;
    };
    if c.is_ascii_alphabetic() {
        return Some(0x41 + (c.to_ascii_uppercase() as u32 - 'A' as u32));
    }
    if c.is_ascii_digit() {
        return Some(0x30 + (c as u32 - '0' as u32));
    }
    None
}

fn vkey_to_str(vkey: u32) -> String {
    if (0x41..=0x5A).contains(&vkey) {
        let c = (b'A' + (vkey - 0x41) as u8) as char;
        return c.to_ascii_lowercase().to_string();
    }
    if (0x30..=0x39).contains(&vkey) {
        return ((b'0' + (vkey - 0x30) as u8) as char).to_string();
    }
    if (VK_F1..=VK_F1 + 23).contains(&vkey) {
        return format!("f{}", vkey - VK_F1 + 1);
    }
    match vkey {
        VK_DELETE => "delete".to_string(),
        VK_ESCAPE => "escape".to_string(),
        other => format!("0x{other:x}"),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyRejection {
    NoModifier,
    ModifierOnly,
    ReservedSystemCombo,
}

pub fn validate_hotkey(combo: &HotkeyCombo) -> Result<(), HotkeyRejection> {
    if combo.modifiers == 0 {
        return Err(HotkeyRejection::NoModifier);
    }
    if combo.vkey == 0 {
        return Err(HotkeyRejection::ModifierOnly);
    }
    if combo.modifiers & MOD_WIN != 0 {
        return Err(HotkeyRejection::ReservedSystemCombo);
    }
    if combo.modifiers == (MOD_CONTROL | MOD_ALT) && combo.vkey == VK_DELETE {
        return Err(HotkeyRejection::ReservedSystemCombo);
    }
    if combo.modifiers == (MOD_CONTROL | MOD_SHIFT) && combo.vkey == VK_ESCAPE {
        return Err(HotkeyRejection::ReservedSystemCombo);
    }
    Ok(())
}

pub const TOGGLE_DEBOUNCE_MS: u64 = 200;

pub fn should_suppress_reopen(last_hide_at_ms: u64, now_ms: u64) -> bool {
    now_ms.saturating_sub(last_hide_at_ms) < TOGGLE_DEBOUNCE_MS
}
