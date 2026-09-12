use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::capture::CanonicalKind;
use crate::hotkey::HotkeyCombo;

pub const QUEUE_BYTE_BUDGET: usize = 128 * 1024 * 1024;

pub const BACKOFF_START: Duration = Duration::from_millis(10);

pub const BACKOFF_CAP_TOTAL: Duration = Duration::from_millis(250);

pub const LOG_FILE_MAX_BYTES: u64 = 5 * 1024 * 1024;

pub const LOG_FILE_KEEP: usize = 3;

const MIB: u64 = 1024 * 1024;

pub const DEFAULT_MAX_BYTES_TEXT: u64 = 2 * MIB;
pub const DEFAULT_MAX_BYTES_IMAGE: u64 = 64 * MIB;
pub const DEFAULT_MAX_BYTES_HDROP: u64 = MIB;
pub const DEFAULT_MAX_BYTES_CUSTOM: u64 = 8 * MIB;

pub const DEFAULT_RETENTION: Duration = Duration::from_secs(7 * 24 * 60 * 60);
pub const MIN_RETENTION_DAYS: u64 = 1;

pub const DEFAULT_MAX_ITEMS: u32 = 500;
pub const MIN_MAX_ITEMS: u32 = 1;
pub const DEFAULT_MAX_PINNED: u32 = 25;
pub const MAX_MAX_PINNED: u32 = 200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    pub max_bytes_text: u64,
    pub max_bytes_image: u64,
    pub max_bytes_hdrop: u64,
    pub max_bytes_custom: u64,
    pub retention: Duration,
    pub max_items: u32,
    pub max_pinned: u32,
    pub blocked_programs: Vec<String>,
    pub heuristic_secret_detection: bool,
    pub encryption_enabled: bool,
    pub db_path: PathBuf,
    pub log_dir: PathBuf,
    pub hotkey: HotkeyCombo,
}

impl Config {
    pub fn with_paths(db_path: PathBuf, log_dir: PathBuf) -> Self {
        Config {
            max_bytes_text: DEFAULT_MAX_BYTES_TEXT,
            max_bytes_image: DEFAULT_MAX_BYTES_IMAGE,
            max_bytes_hdrop: DEFAULT_MAX_BYTES_HDROP,
            max_bytes_custom: DEFAULT_MAX_BYTES_CUSTOM,
            retention: DEFAULT_RETENTION,
            max_items: DEFAULT_MAX_ITEMS,
            max_pinned: DEFAULT_MAX_PINNED,
            blocked_programs: Vec::new(),
            heuristic_secret_detection: true,
            encryption_enabled: false,
            db_path,
            log_dir,
            hotkey: HotkeyCombo::DEFAULT,
        }
    }

    pub fn limit_for(&self, kind: CanonicalKind) -> u64 {
        match kind {
            CanonicalKind::UnicodeText => self.max_bytes_text,
            CanonicalKind::Dib | CanonicalKind::DibV5 => self.max_bytes_image,
            CanonicalKind::HDrop => self.max_bytes_hdrop,
            CanonicalKind::Custom => self.max_bytes_custom,
        }
    }

    pub fn load(
        db_path: PathBuf,
        log_dir: PathBuf,
        toml_path: &Path,
    ) -> (Config, Vec<ConfigFallback>) {
        let cfg = Config::with_paths(db_path, log_dir);
        match std::fs::read_to_string(toml_path) {
            Ok(contents) => Self::from_str_over(cfg, &contents),
            Err(_) => (cfg, vec![ConfigFallback::whole_file()]),
        }
    }

    pub fn from_str_over(mut cfg: Config, contents: &str) -> (Config, Vec<ConfigFallback>) {
        let mut fallbacks = Vec::new();
        for raw_line in contents.lines() {
            let line = raw_line.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim();
            let value = value.trim().trim_matches('"');

            match key {
                "max_bytes_text" => set_field(
                    &mut cfg.max_bytes_text,
                    "max_bytes_text",
                    value,
                    &mut fallbacks,
                ),
                "max_bytes_image" => set_field(
                    &mut cfg.max_bytes_image,
                    "max_bytes_image",
                    value,
                    &mut fallbacks,
                ),
                "max_bytes_hdrop" => set_field(
                    &mut cfg.max_bytes_hdrop,
                    "max_bytes_hdrop",
                    value,
                    &mut fallbacks,
                ),
                "max_bytes_custom" => set_field(
                    &mut cfg.max_bytes_custom,
                    "max_bytes_custom",
                    value,
                    &mut fallbacks,
                ),
                "retention_days" => match value.parse::<u64>() {
                    Ok(days) => {
                        let applied = days.max(MIN_RETENTION_DAYS);
                        if applied != days {
                            fallbacks.push(ConfigFallback::floor_applied(
                                "retention_days",
                                days,
                                applied,
                            ));
                        }
                        cfg.retention = Duration::from_secs(applied * 24 * 60 * 60);
                    }
                    Err(_) => fallbacks.push(ConfigFallback::field("retention_days")),
                },
                "max_items" => match value.parse::<u32>() {
                    Ok(n) => {
                        let applied = n.max(MIN_MAX_ITEMS);
                        if applied != n {
                            fallbacks.push(ConfigFallback::floor_applied(
                                "max_items",
                                n as u64,
                                applied as u64,
                            ));
                        }
                        cfg.max_items = applied;
                    }
                    Err(_) => fallbacks.push(ConfigFallback::field("max_items")),
                },
                "max_pinned" => match value.parse::<u32>() {
                    Ok(n) => {
                        let applied = n.min(MAX_MAX_PINNED);
                        if applied != n {
                            fallbacks.push(ConfigFallback::ceiling_applied(
                                "max_pinned",
                                n as u64,
                                applied as u64,
                            ));
                        }
                        cfg.max_pinned = applied;
                    }
                    Err(_) => fallbacks.push(ConfigFallback::field("max_pinned")),
                },
                "heuristic_secret_detection" => match value {
                    "true" => cfg.heuristic_secret_detection = true,
                    "false" => cfg.heuristic_secret_detection = false,
                    _ => fallbacks.push(ConfigFallback::field("heuristic_secret_detection")),
                },
                "blocked_program" => {
                    if !value.is_empty() {
                        cfg.blocked_programs.push(value.to_string());
                    }
                }
                "hotkey" => match HotkeyCombo::parse(value) {
                    Some(combo) => cfg.hotkey = combo,
                    None => fallbacks.push(ConfigFallback::field("hotkey")),
                },
                _ => {}
            }
        }
        (cfg, fallbacks)
    }

    pub fn to_toml_string(&self) -> String {
        let mut s = format!(
            "max_bytes_text = {}\n\
             max_bytes_image = {}\n\
             max_bytes_hdrop = {}\n\
             max_bytes_custom = {}\n\
             retention_days = {}\n\
             max_items = {}\n\
             max_pinned = {}\n\
             heuristic_secret_detection = {}\n\
             hotkey = \"{}\"\n",
            self.max_bytes_text,
            self.max_bytes_image,
            self.max_bytes_hdrop,
            self.max_bytes_custom,
            self.retention.as_secs() / (24 * 60 * 60),
            self.max_items,
            self.max_pinned,
            self.heuristic_secret_detection,
            self.hotkey.format(),
        );
        for prog in &self.blocked_programs {
            s.push_str(&format!("blocked_program = \"{prog}\"\n"));
        }
        s
    }

    pub fn persist(&self, path: &Path) -> std::io::Result<()> {
        std::fs::write(path, self.to_toml_string())
    }
}

fn set_field(field: &mut u64, key: &'static str, value: &str, fallbacks: &mut Vec<ConfigFallback>) {
    match value.parse::<u64>() {
        Ok(n) if n > 0 => *field = n,
        _ => fallbacks.push(ConfigFallback::field(key)),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConfigFallback {
    pub field: &'static str,
    pub floor: Option<(u64, u64)>,
    pub ceiling: Option<(u64, u64)>,
}

impl ConfigFallback {
    fn field(name: &'static str) -> Self {
        ConfigFallback {
            field: name,
            floor: None,
            ceiling: None,
        }
    }

    fn whole_file() -> Self {
        ConfigFallback {
            field: "*",
            floor: None,
            ceiling: None,
        }
    }

    fn floor_applied(field: &'static str, requested: u64, applied: u64) -> Self {
        ConfigFallback {
            field,
            floor: Some((requested, applied)),
            ceiling: None,
        }
    }

    fn ceiling_applied(field: &'static str, requested: u64, applied: u64) -> Self {
        ConfigFallback {
            field,
            floor: None,
            ceiling: Some((requested, applied)),
        }
    }
}
