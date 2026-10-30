#![cfg(windows)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;

use duplicata_core::settings_text as text;
use duplicata_core::testsupport::{FakeDisabledHotkeys, FixedExplorer};
use duplicata_core::{Config, HotkeyCombo};
use duplicata_win::win_v_dialog::{self, ID_KEEP, ID_OK, ID_TURN_OFF};
use duplicata_win::win_v_runtime::WinVRuntime;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{
    GetDlgItem, IsWindowVisible, SendMessageW, BM_CLICK,
};

struct Opened {
    store: Rc<RefCell<FakeDisabledHotkeys>>,
    config: Rc<RefCell<Config>>,
    _dir: tempfile::TempDir,
}

/// Opção ligada com o `V` do duplicata, registro em memória (nunca o
/// `DisabledHotkeys` real) e a reserva dada como registrada.
fn open(value: &str, fail_write: bool) -> Opened {
    open_with(value, fail_write, false)
}

/// `config_is_a_folder`: o caminho do `config.toml` é uma pasta, e a
/// gravação falha.
fn open_with(value: &str, fail_write: bool, config_is_a_folder: bool) -> Opened {
    let dir = tempfile::tempdir().unwrap();
    let config_path = if config_is_a_folder {
        dir.path().to_path_buf()
    } else {
        dir.path().join("config.toml")
    };
    let mut cfg = Config::with_paths(dir.path().join("db"), dir.path().join("logs"));
    cfg.open_with_win_v = true;
    cfg.win_v_added_v = true;
    cfg.win_v_explorer = Some(1);
    let config = Rc::new(RefCell::new(cfg));
    let store = Rc::new(RefCell::new(FakeDisabledHotkeys::with(Some(value))));
    store.borrow_mut().fail_write = fail_write;
    let runtime = WinVRuntime::new(
        HWND::default(),
        Rc::clone(&config),
        config_path,
        Rc::new(Cell::new(Some(HotkeyCombo::DEFAULT))),
        Box::new(Rc::clone(&store)),
        Box::new(FixedExplorer(Some(2))),
    );
    win_v_dialog::open(runtime);
    assert!(win_v_dialog::is_open());
    Opened {
        store,
        config,
        _dir: dir,
    }
}

fn button(id: i32) -> HWND {
    let window = win_v_dialog::window().expect("janela aberta");
    // SAFETY: só consulta os filhos da janela viva.
    unsafe { GetDlgItem(Some(window), id) }.expect("botão")
}

fn click(id: i32) {
    // SAFETY: botão vivo desta thread; o clique é síncrono.
    unsafe {
        SendMessageW(button(id), BM_CLICK, None, None);
    }
}

fn visible(id: i32) -> bool {
    // SAFETY: só consulta a visibilidade.
    unsafe { IsWindowVisible(button(id)) }.as_bool()
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn the_window_shows_the_failure_with_turn_off_and_keep() {
    let _o = open("QRV", false);
    assert_eq!(
        win_v_dialog::shown_text().as_deref(),
        Some(text::win_v_failure("Ctrl+Shift+V").as_str())
    );
    assert!(visible(ID_TURN_OFF) && visible(ID_KEEP) && !visible(ID_OK));
    click(ID_KEEP);
    assert!(!win_v_dialog::is_open(), "Manter fecha");
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn turning_off_removes_our_v_and_swaps_the_text_for_the_notice_with_ok() {
    let o = open("QRV", false);
    click(ID_TURN_OFF);
    assert_eq!(o.store.borrow().value.as_deref(), Some("QR"));
    assert!(!o.config.borrow().open_with_win_v);
    assert_eq!(
        win_v_dialog::shown_text().as_deref(),
        Some(text::WIN_V_RETURNS_AFTER)
    );
    assert!(!visible(ID_TURN_OFF) && !visible(ID_KEEP) && visible(ID_OK));
    click(ID_OK);
    assert!(!win_v_dialog::is_open());
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_registry_failure_shows_the_error_and_keeps_the_option() {
    let o = open("QRV", true);
    click(ID_TURN_OFF);
    assert_eq!(o.store.borrow().value.as_deref(), Some("QRV"));
    assert!(o.config.borrow().open_with_win_v);
    let shown = win_v_dialog::shown_text().unwrap();
    assert!(shown.ends_with(text::WIN_V_REGISTRY_FAILED), "{shown:?}");
    assert!(visible(ID_TURN_OFF) && visible(ID_KEEP));
    win_v_dialog::close();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn the_window_follows_the_theme_and_the_settings_buttons() {
    use duplicata_win::system_appearance::SystemAppearance;
    use windows::Win32::UI::WindowsAndMessaging::{GetWindowLongPtrW, BS_OWNERDRAW, GWL_STYLE};

    let _o = open("QRV", false);
    assert_eq!(win_v_dialog::is_dark(), Some(SystemAppearance::read_dark()));
    for id in [ID_TURN_OFF, ID_KEEP, ID_OK] {
        // SAFETY: só lê o estilo de um botão vivo.
        let style = unsafe { GetWindowLongPtrW(button(id), GWL_STYLE) } as u32;
        assert_eq!(
            style & 0xF,
            BS_OWNERDRAW as u32,
            "botão {id} desenhado como nas Configurações"
        );
    }
    win_v_dialog::close();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_dpi_change_rescales_the_window_and_the_buttons() {
    use duplicata_core::row_layout::scale_px;
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{GetClientRect, GetWindowRect, WM_DPICHANGED};

    let _o = open("QRV", false);
    let window = win_v_dialog::window().unwrap();
    let before = win_v_dialog::current_layout().unwrap();
    // No DPI real da janela, a área cliente é a do layout. (O `WM_DPICHANGED`
    // simulado abaixo não muda o DPI do monitor, então a moldura do Windows
    // não acompanha; ali só o layout e os botões são conferidos.)
    // SAFETY: só consulta o DPI da janela viva.
    let real_dpi = unsafe { windows::Win32::UI::HiDpi::GetDpiForWindow(window) };
    assert_eq!(win_v_dialog::current_dpi(), Some(real_dpi));
    let mut client = RECT::default();
    // SAFETY: só lê a área cliente.
    let _ = unsafe { GetClientRect(window, &mut client) };
    assert_eq!(client.right - client.left, before.client_width);
    assert_eq!(client.bottom - client.top, before.client_height);
    let mut r = RECT::default();
    // SAFETY: janela viva desta thread; o `RECT` sugerido vive durante a
    // chamada síncrona.
    unsafe {
        let _ = GetWindowRect(window, &mut r);
        let dpi = 144usize;
        SendMessageW(
            window,
            WM_DPICHANGED,
            Some(WPARAM((dpi << 16) | dpi)),
            Some(LPARAM(&r as *const RECT as isize)),
        );
    }
    assert_eq!(win_v_dialog::current_dpi(), Some(144));
    let after = win_v_dialog::current_layout().unwrap();
    assert!(after.client_width >= scale_px(460, 144));
    assert_eq!(
        after.buttons[1].bottom - after.buttons[1].top,
        scale_px(28, 144).max(before.buttons[1].bottom - before.buttons[1].top),
        "o botão cresce com o DPI"
    );
    let mut b = RECT::default();
    // SAFETY: só lê o retângulo do botão.
    let _ = unsafe { GetWindowRect(button(ID_KEEP), &mut b) };
    assert_eq!(
        b.right - b.left,
        after.buttons[1].right - after.buttons[1].left,
        "o botão segue o layout"
    );
    win_v_dialog::close();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn turning_off_without_saving_the_config_shows_the_error_with_the_notice() {
    let o = open_with("QRV", false, true);
    click(ID_TURN_OFF);
    assert_eq!(
        o.store.borrow().value.as_deref(),
        Some("QR"),
        "o registro mudou"
    );
    let shown = win_v_dialog::shown_text().unwrap();
    assert!(shown.starts_with(text::WIN_V_RETURNS_AFTER), "{shown:?}");
    assert!(shown.ends_with(text::SAVE_FAILED), "{shown:?}");
    assert!(visible(ID_OK));
    click(ID_OK);
}
