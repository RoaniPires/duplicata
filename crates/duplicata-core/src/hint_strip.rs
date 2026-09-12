use crate::hotkey::{
    format_hotkey, HotkeyCombo, MOD_CONTROL, MOD_SHIFT, VK_ESCAPE, VK_P, VK_RETURN, VK_TAB,
};

pub const HINT_STRIP_PX: i32 = 24;

pub const SEPARATOR: &str = " · ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Hint {
    pub keys: String,
    pub action: &'static str,
}

fn combo(modifiers: u32, vkey: u32) -> String {
    format_hotkey(&HotkeyCombo { modifiers, vkey })
}

pub fn hints() -> Vec<Hint> {
    vec![
        Hint {
            keys: "↑↓".to_string(),
            action: "navegar",
        },
        Hint {
            keys: combo(0, VK_RETURN),
            action: "colar",
        },
        Hint {
            keys: combo(MOD_SHIFT, VK_RETURN),
            action: "colar texto",
        },
        Hint {
            keys: combo(MOD_CONTROL, VK_P),
            action: "fixar",
        },
        Hint {
            keys: format!("{}+←→", combo(0, VK_TAB)),
            action: "tipo",
        },
        Hint {
            keys: combo(0, VK_ESCAPE),
            action: "fechar",
        },
    ]
}

pub fn hint_line() -> String {
    hints()
        .iter()
        .map(|h| format!("{} {}", h.keys, h.action))
        .collect::<Vec<_>>()
        .join(SEPARATOR)
}

pub const MEASURED_HINT_LINE_PX: i32 = 458;

pub const SIDE_PADDING_PX: i32 = 20;

pub const HINT_LINE_SLACK_PX: i32 = 24;

pub const MAX_HINT_SLACK_PX: i32 = 120;

pub const fn min_client_width_px() -> i32 {
    MEASURED_HINT_LINE_PX + SIDE_PADDING_PX + HINT_LINE_SLACK_PX
}
