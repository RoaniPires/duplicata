mod win_clock;
pub use win_clock::WinClock;

pub mod navigation;

#[cfg(windows)]
mod app_icon;
#[cfg(windows)]
pub mod clipboard_restore;
#[cfg(windows)]
pub mod dialog;
#[cfg(windows)]
pub mod dpi;
#[cfg(windows)]
pub mod error_banner;
#[cfg(windows)]
pub mod history_window;
#[cfg(windows)]
pub mod hotkey_capture;
#[cfg(windows)]
pub mod hotkey_dialog;
#[cfg(windows)]
pub mod hotkey_win;
#[cfg(windows)]
pub mod listener;
#[cfg(windows)]
pub mod menu_icon;
#[cfg(windows)]
pub mod message_loop;
#[cfg(windows)]
pub mod paste_injector;
#[cfg(windows)]
pub mod paths;
#[cfg(windows)]
pub mod settings_dialog;
#[cfg(windows)]
pub mod shutdown_request;
#[cfg(windows)]
pub mod startup_registry;
#[cfg(windows)]
pub mod system_appearance;
#[cfg(windows)]
pub mod text_metrics;
#[cfg(windows)]
pub mod thumbnail_gdi;
#[cfg(windows)]
pub mod tray;
#[cfg(windows)]
pub mod uninstall_dialog;
#[cfg(windows)]
pub mod win_clipboard;
#[cfg(windows)]
pub mod window_material;

#[cfg(windows)]
pub use history_window::HistoryWindow;
#[cfg(windows)]
pub use listener::ClipboardListener;
#[cfg(windows)]
pub use tray::Tray;
#[cfg(windows)]
pub use win_clipboard::WinClipboard;
