#![cfg(windows)]

use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use duplicata_core::settings_text as text;
use duplicata_core::{
    ByteBudgetQueue, CaptureQueue, Config, FakeHistoryRepository, HistoryReader, HotkeyCombo,
    SelfWriteFilter, StoreError, WorkItem,
};
use duplicata_win::history_window::{HistoryWindow, WM_APP_TOGGLE_ENCRYPTION};
use duplicata_win::hotkey_capture::{self, CaptureOwner};
use duplicata_win::settings_dialog::{
    self as dialog, ID_ADD, ID_ALTER, ID_APPLY, ID_CANCEL, ID_CONTENT, ID_LIST, ID_LIST_EDIT,
    ID_MAX_ITEMS, ID_MAX_PINNED, ID_PROTECTION, ID_REMOVE, ID_RETENTION, ID_SAVE, ID_SECRETS,
    ID_STARTUP,
};
use duplicata_win::system_appearance::SystemAppearance;
use windows::Win32::Foundation::{HWND, LPARAM, RECT, WPARAM};
use windows::Win32::UI::Controls::WM_MOUSEHOVER;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    GetFocus, IsWindowEnabled, SetFocus, VK_ESCAPE, VK_SPACE, VK_TAB,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, GetClientRect, GetDlgCtrlID, GetWindowRect, IsWindow, SendMessageW, BM_CLICK,
    BM_GETCHECK, LB_GETCOUNT, LB_SETCURSEL, MSG, WM_CLOSE, WM_DPICHANGED, WM_GETTEXT,
    WM_GETTEXTLENGTH, WM_KEYDOWN, WM_KEYUP, WM_MOUSEMOVE, WM_SETTEXT, WM_SETTINGCHANGE,
};

const DAY: u64 = 24 * 60 * 60;

struct Opened {
    cfg: Rc<RefCell<Config>>,
    path: PathBuf,
    queue: Arc<ByteBudgetQueue<WorkItem>>,
    _dir: tempfile::TempDir,
}

fn cfg_at(dir: &Path) -> Config {
    let mut c = Config::with_paths(dir.join("db"), dir.join("logs"));
    c.retention = Duration::from_secs(7 * DAY);
    c.max_items = 500;
    c.max_pinned = 25;
    c
}

fn open_with(
    history: HWND,
    registered: Option<HotkeyCombo>,
    edit: impl FnOnce(&mut Config),
) -> Opened {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.toml");
    let mut c = cfg_at(dir.path());
    edit(&mut c);
    let cfg = Rc::new(RefCell::new(c));
    let queue = Arc::new(ByteBudgetQueue::<WorkItem>::new());
    dialog::open(
        Rc::clone(&cfg),
        path.clone(),
        Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>,
        history,
        Rc::new(Cell::new(registered)),
    );
    assert!(
        dialog::window().is_some(),
        "a janela de Configurações abriu"
    );
    Opened {
        cfg,
        path,
        queue,
        _dir: dir,
    }
}

fn open() -> Opened {
    open_with(HWND::default(), Some(HotkeyCombo::DEFAULT), |_| {})
}

fn history_window() -> HistoryWindow {
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela deve funcionar numa sessão gráfica")
}

fn control(id: i32) -> HWND {
    dialog::control(id).unwrap_or_else(|| panic!("controle {id} não encontrado"))
}

fn click(id: i32) {
    // SAFETY: controle vivo desta thread; `BM_CLICK` entrega o clique de
    // forma síncrona.
    unsafe {
        SendMessageW(control(id), BM_CLICK, None, None);
    }
}

fn set_text(id: i32, value: &str) {
    let w: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: controle vivo; `w` NUL-terminado e vivo durante a chamada.
    unsafe {
        SendMessageW(
            control(id),
            WM_SETTEXT,
            None,
            Some(LPARAM(w.as_ptr() as isize)),
        );
    }
}

fn text_of(id: i32) -> String {
    let h = control(id);
    // SAFETY: controle vivo desta thread.
    let len = unsafe { SendMessageW(h, WM_GETTEXTLENGTH, None, None) }.0 as usize;
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` tem espaço para o NUL.
    unsafe {
        SendMessageW(
            h,
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        );
    }
    String::from_utf16_lossy(&buf[..len])
}

fn enabled(id: i32) -> bool {
    // SAFETY: só consulta.
    unsafe { IsWindowEnabled(control(id)) }.as_bool()
}

fn checked(id: i32) -> bool {
    // SAFETY: alternância viva desta thread.
    unsafe { SendMessageW(control(id), BM_GETCHECK, None, None) }.0 != 0
}

fn is_open() -> bool {
    dialog::window().is_some()
}

fn close_if_open() {
    if let Some(h) = dialog::window() {
        // SAFETY: janela viva desta thread.
        unsafe {
            let _ = DestroyWindow(h);
        }
    }
}

fn key(hwnd: HWND, msg: u32, vk: u16) -> bool {
    let m = MSG {
        hwnd,
        message: msg,
        wParam: WPARAM(vk as usize),
        ..Default::default()
    };
    dialog::pre_translate(&m)
}

fn drained(queue: &ByteBudgetQueue<WorkItem>) -> Vec<WorkItem> {
    queue.close();
    std::iter::from_fn(|| queue.pop()).collect()
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn opens_with_the_three_sections_their_options_and_only_the_footer_buttons_enabled() {
    let _o = open();
    let painted = dialog::painted_texts();
    let mut expected = vec![
        text::SECTION_GENERAL,
        text::SECTION_HISTORY,
        text::SECTION_PRIVACY,
    ];
    for o in text::OPTIONS {
        expected.push(o.name);
    }
    for o in &text::OPTIONS[1..] {
        expected.push(o.description);
    }
    expected.push(text::HOTKEY.description);
    for want in expected {
        assert!(
            painted.iter().any(|p| p == want),
            "faltou {want:?} em {painted:?}"
        );
    }
    let order: Vec<usize> = [
        text::SECTION_GENERAL,
        text::SECTION_HISTORY,
        text::SECTION_PRIVACY,
    ]
    .iter()
    .map(|s| painted.iter().position(|p| p == s).unwrap())
    .collect();
    assert!(
        order[0] < order[1] && order[1] < order[2],
        "seções na ordem"
    );
    for p in &painted {
        for banned in ["FR-", "Fatia", "DPAPI"] {
            assert!(!p.contains(banned), "{p:?}");
        }
    }
    assert!(!enabled(ID_APPLY), "sem mudança, Aplicar desabilitado");
    assert!(enabled(ID_SAVE) && enabled(ID_CANCEL));
    assert_eq!(text_of(ID_SAVE), text::BUTTON_SAVE);
    assert_eq!(text_of(ID_CANCEL), text::BUTTON_CANCEL);
    assert_eq!(text_of(ID_APPLY), text::BUTTON_APPLY);
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn follows_the_system_theme_and_repaints_on_a_live_change() {
    let _o = open();
    assert_eq!(dialog::is_dark(), Some(SystemAppearance::read_dark()));
    let name: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    // SAFETY: janela viva; `name` vive durante a chamada síncrona.
    unsafe {
        SendMessageW(
            dialog::window().unwrap(),
            WM_SETTINGCHANGE,
            None,
            Some(LPARAM(name.as_ptr() as isize)),
        );
    }
    assert!(is_open());
    assert_eq!(dialog::is_dark(), Some(SystemAppearance::read_dark()));
    assert!(!dialog::painted_texts().is_empty());
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_dpi_change_rescales_the_window_and_keeps_the_footer_inside() {
    let _o = open();
    let hwnd = dialog::window().unwrap();
    let mut before = RECT::default();
    // SAFETY: janela viva; só lê o retângulo.
    unsafe {
        let _ = GetWindowRect(hwnd, &mut before);
    }
    let suggested = before;
    // SAFETY: `WM_DPICHANGED` sintético para 200%; `suggested` vive durante
    // a chamada.
    unsafe {
        SendMessageW(
            hwnd,
            WM_DPICHANGED,
            Some(WPARAM((192 << 16) | 192)),
            Some(LPARAM(&suggested as *const RECT as isize)),
        );
    }
    assert_eq!(dialog::current_dpi(), Some(192));
    let mut after = RECT::default();
    let mut client = RECT::default();
    let mut apply = RECT::default();
    // SAFETY: janelas vivas; só lê retângulos.
    unsafe {
        let _ = GetWindowRect(hwnd, &mut after);
        let _ = GetClientRect(hwnd, &mut client);
        let _ = GetWindowRect(control(ID_APPLY), &mut apply);
    }
    assert!(after.right - after.left > before.right - before.left);
    assert!(apply.right <= after.right && apply.bottom <= after.bottom);
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn applying_a_new_ignored_app_takes_effect_without_writing_and_cancel_reverts_it() {
    let o = open();
    set_text(ID_LIST_EDIT, "  keepass.exe ");
    click(ID_ADD);
    assert_eq!(text_of(ID_LIST_EDIT), "", "o campo é limpo");
    assert!(
        o.cfg.borrow().blocked_programs.is_empty(),
        "nada vale antes do Aplicar"
    );
    assert!(enabled(ID_APPLY));
    click(ID_APPLY);
    assert_eq!(
        o.cfg.borrow().blocked_programs,
        vec!["keepass.exe".to_string()]
    );
    assert!(!o.path.exists(), "Aplicar não grava o arquivo");
    assert!(!enabled(ID_APPLY), "Aplicar volta a ficar desabilitado");
    click(ID_CANCEL);
    assert!(!is_open(), "Cancelar fecha");
    assert!(
        o.cfg.borrow().blocked_programs.is_empty(),
        "Cancelar desfaz o aplicado"
    );
    assert!(!o.path.exists());
    let items = drained(&o.queue);
    assert_eq!(
        items.len(),
        2,
        "um aviso à worker no Aplicar e um no Cancelar: {items:?}"
    );
    assert!(items
        .iter()
        .all(|i| matches!(i, WorkItem::SetSecretDetection { enabled: true })));
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_repeated_name_or_a_remove_without_selection_does_nothing() {
    let _o = open_with(HWND::default(), Some(HotkeyCombo::DEFAULT), |c| {
        c.blocked_programs = vec!["Dup.exe".into()]
    });
    set_text(ID_LIST_EDIT, "dup.EXE");
    click(ID_ADD);
    let count = || {
        // SAFETY: LISTBOX viva desta thread.
        unsafe { SendMessageW(control(ID_LIST), LB_GETCOUNT, None, None) }.0
    };
    assert_eq!(count(), 1);
    assert_eq!(
        text_of(ID_LIST_EDIT),
        "dup.EXE",
        "nada acontece: o campo fica"
    );
    click(ID_REMOVE);
    assert_eq!(count(), 1, "sem seleção, Remover não faz nada");
    // SAFETY: LISTBOX viva; seleciona o primeiro.
    unsafe {
        SendMessageW(control(ID_LIST), LB_SETCURSEL, Some(WPARAM(0)), None);
    }
    click(ID_REMOVE);
    assert_eq!(count(), 0);
    assert!(enabled(ID_APPLY));
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn numeric_fields_keep_only_digits_and_show_the_adjusted_value_after_apply() {
    let o = open();
    set_text(ID_RETENTION, "a5b");
    assert_eq!(text_of(ID_RETENTION), "5", "letras não entram");
    set_text(ID_RETENTION, "");
    set_text(ID_MAX_PINNED, "500");
    assert!(enabled(ID_APPLY));
    click(ID_APPLY);
    assert_eq!(text_of(ID_RETENTION), "7", "vazio volta ao anterior");
    assert_eq!(text_of(ID_MAX_PINNED), "200", "acima de 200 vira 200");
    assert_eq!(o.cfg.borrow().max_pinned, 200);
    assert!(!enabled(ID_APPLY));
    let items = drained(&o.queue);
    assert!(
        items.iter().any(|i| matches!(
            i,
            WorkItem::ApplySettings {
                max_pinned: 200,
                max_items: 500,
                ..
            }
        )),
        "a worker recebe o teto novo: {items:?}"
    );
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_reduction_asks_for_confirmation_back_keeps_everything_and_confirming_saves() {
    let o = open();
    set_text(ID_RETENTION, "3");
    assert!(
        !enabled(ID_APPLY),
        "só mudança irreversível: Aplicar desabilitado"
    );
    click(ID_SAVE);
    assert_eq!(
        dialog::confirm_lines(),
        vec![
            text::reduce_retention_line(7, 3),
            text::CANNOT_UNDO.to_string()
        ]
    );
    let confirm = dialog::confirm_window().expect("confirmação aberta");
    // SAFETY: só consulta.
    assert!(!unsafe { IsWindowEnabled(dialog::window().unwrap()) }.as_bool());
    // SAFETY: só consulta o foco desta thread.
    let focus = unsafe { GetFocus() };
    // SAFETY: só consulta.
    assert_eq!(unsafe { GetDlgCtrlID(focus) }, 2, "foco inicial em Voltar");
    assert!(key(focus, WM_KEYDOWN, VK_ESCAPE.0), "Esc na confirmação");
    // SAFETY: só consulta.
    assert!(!unsafe { IsWindow(Some(confirm)) }.as_bool());
    assert!(is_open(), "Voltar deixa as Configurações abertas");
    assert_eq!(text_of(ID_RETENTION), "3");
    assert!(!o.path.exists(), "nada gravado");
    assert_eq!(o.cfg.borrow().retention, Duration::from_secs(7 * DAY));

    click(ID_SAVE);
    let confirm = dialog::confirm_window().expect("confirmação aberta de novo");
    // SAFETY: botão Salvar da confirmação (id 301), vivo.
    unsafe {
        let save = windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(confirm), 301).unwrap();
        SendMessageW(save, BM_CLICK, None, None);
    }
    assert!(!is_open(), "confirmar salva e fecha");
    let (saved, _) = Config::from_str_over(
        cfg_at(o._dir.path()),
        &std::fs::read_to_string(&o.path).expect("config.toml gravado"),
    );
    assert_eq!(saved.retention, Duration::from_secs(3 * DAY));
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn space_on_a_toggle_flips_it_and_saving_without_reduction_writes_and_closes() {
    let o = open();
    assert!(checked(ID_SECRETS));
    // SAFETY: alternância viva desta thread.
    unsafe {
        let _ = SetFocus(Some(control(ID_SECRETS)));
        for msg in [WM_KEYDOWN, WM_KEYUP] {
            SendMessageW(
                control(ID_SECRETS),
                msg,
                Some(WPARAM(VK_SPACE.0 as usize)),
                None,
            );
        }
    }
    assert!(!checked(ID_SECRETS));
    assert!(enabled(ID_APPLY));
    click(ID_SAVE);
    assert!(!is_open(), "sem irreversível: salva e fecha sem perguntar");
    let items = drained(&o.queue);
    assert!(
        items
            .iter()
            .any(|i| matches!(i, WorkItem::SetSecretDetection { enabled: false })),
        "a worker passa a guardar segredos sem reiniciar: {items:?}"
    );
    let on_disk = std::fs::read_to_string(&o.path).expect("config.toml gravado");
    assert!(on_disk.contains("heuristic_secret_detection = false"));
    assert!(!o.cfg.borrow().heuristic_secret_detection);
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_failed_write_keeps_the_window_open_with_the_message_and_the_change_in_effect() {
    let dir = tempfile::tempdir().unwrap();
    let o = open();
    // Um diretório no lugar do arquivo: a gravação falha.
    let blocked = dir.path().to_path_buf();
    drop(o);
    close_if_open();
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));
    dialog::open(
        Rc::clone(&cfg),
        blocked,
        Arc::new(ByteBudgetQueue::new()) as Arc<dyn CaptureQueue<WorkItem>>,
        HWND::default(),
        Rc::new(Cell::new(Some(HotkeyCombo::DEFAULT))),
    );
    set_text(ID_RETENTION, "10");
    click(ID_SAVE);
    assert!(is_open(), "a janela continua aberta");
    assert!(dialog::painted_texts()
        .iter()
        .any(|t| t == text::SAVE_FAILED));
    assert_eq!(
        cfg.borrow().retention,
        Duration::from_secs(10 * DAY),
        "em efeito"
    );
    click(ID_CANCEL);
    assert!(!is_open());
    assert_eq!(
        cfg.borrow().retention,
        Duration::from_secs(10 * DAY),
        "nada é desfeito: o Salvar virou o ponto de partida"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn escape_and_the_close_button_cancel_what_was_applied() {
    for close_with_x in [false, true] {
        let o = open();
        set_text(ID_MAX_PINNED, "40");
        click(ID_APPLY);
        assert_eq!(o.cfg.borrow().max_pinned, 40);
        if close_with_x {
            // SAFETY: janela viva desta thread.
            unsafe {
                SendMessageW(dialog::window().unwrap(), WM_CLOSE, None, None);
            }
        } else {
            assert!(key(control(ID_RETENTION), WM_KEYDOWN, VK_ESCAPE.0));
        }
        assert!(!is_open(), "fecha sem perguntar");
        assert_eq!(o.cfg.borrow().max_pinned, 25, "o aplicado volta");
        assert!(!o.path.exists());
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn opening_again_brings_the_same_window_with_the_pending_changes() {
    let o = open();
    let first = dialog::window().unwrap();
    set_text(ID_MAX_ITEMS, "1000");
    dialog::open(
        Rc::clone(&o.cfg),
        o.path.clone(),
        Arc::clone(&o.queue) as Arc<dyn CaptureQueue<WorkItem>>,
        HWND::default(),
        Rc::new(Cell::new(Some(HotkeyCombo::DEFAULT))),
    );
    assert_eq!(dialog::window(), Some(first));
    assert_eq!(text_of(ID_MAX_ITEMS), "1000");
    assert!(enabled(ID_APPLY));
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn the_hotkey_line_is_inactive_while_the_standalone_dialog_holds_the_capture() {
    hotkey_capture::arm(CaptureOwner::StandaloneDialog);
    let _o = open();
    assert_eq!(
        dialog::hotkey_description().as_deref(),
        Some(text::HOTKEY_STANDALONE)
    );
    assert!(!enabled(ID_ALTER));
    hotkey_capture::release(CaptureOwner::StandaloneDialog);
    let _again = open_with(HWND::default(), Some(HotkeyCombo::DEFAULT), |_| {});
    assert_eq!(
        dialog::hotkey_description().as_deref(),
        Some(text::HOTKEY.description)
    );
    assert!(enabled(ID_ALTER));
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn arming_refusing_and_escape_only_disarm_without_closing() {
    let history = history_window();
    let _o = open_with(history.hwnd(), Some(HotkeyCombo::DEFAULT), |_| {});
    click(ID_ALTER);
    assert!(hotkey_capture::is_armed());
    assert_eq!(
        dialog::hotkey_description().as_deref(),
        Some(text::HOTKEY_ARMED)
    );
    let alter = control(ID_ALTER);
    // Sem modificador físico pressionado: recusada, continua armada.
    // SAFETY: botão vivo com o subclass da captura.
    unsafe { SendMessageW(alter, WM_KEYDOWN, Some(WPARAM(0x41)), None) };
    assert!(hotkey_capture::is_armed());
    let refusal = dialog::hotkey_description().unwrap();
    assert!(refusal.contains("modificador"), "{refusal:?}");
    assert!(!enabled(ID_APPLY), "nada pendente");
    assert!(
        key(alter, WM_KEYDOWN, VK_ESCAPE.0),
        "o Esc vai para a captura"
    );
    assert!(!hotkey_capture::is_armed());
    assert!(is_open(), "Esc com a captura armada não fecha");
    close_if_open();
    assert_eq!(hotkey_capture::owner(), CaptureOwner::None);
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn hovering_a_row_shows_its_tip_and_moving_to_another_row_hides_it() {
    let _o = open();
    let content = control(ID_CONTENT);
    let at = |row: usize| {
        let p = dialog::row_point(row).unwrap();
        LPARAM(((p.y as u16 as isize) << 16) | (p.x as u16 as isize))
    };
    // SAFETY: área de conteúdo viva; mensagens síncronas com pontos dela.
    unsafe {
        SendMessageW(content, WM_MOUSEMOVE, None, Some(at(4)));
        SendMessageW(content, WM_MOUSEHOVER, None, Some(at(4)));
    }
    assert_eq!(
        dialog::tooltip_text().as_deref(),
        Some(text::MAX_PINNED.tip)
    );
    // SAFETY: idem.
    unsafe { SendMessageW(content, WM_MOUSEMOVE, None, Some(at(3))) };
    assert_eq!(dialog::tooltip_text(), None);
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn a_rewrite_in_progress_shows_the_target_and_locks_the_toggle_until_it_ends() {
    let history = history_window();
    // SAFETY: Histórico desta thread; sem worker, a reescrita fica em
    // andamento.
    unsafe {
        SendMessageW(
            history.hwnd(),
            WM_APP_TOGGLE_ENCRYPTION,
            Some(WPARAM(1)),
            None,
        );
    }
    let _o = open_with(history.hwnd(), Some(HotkeyCombo::DEFAULT), |_| {});
    assert!(checked(ID_PROTECTION), "mostra o destino");
    assert!(!enabled(ID_PROTECTION), "travada até terminar");
    dialog::on_protection_finished();
    assert!(enabled(ID_PROTECTION));
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn tab_follows_the_visual_order_skips_the_disabled_apply_and_scrolls_into_view() {
    let _o = open();
    // SAFETY: controle vivo desta thread.
    unsafe {
        let _ = SetFocus(Some(control(ID_ALTER)));
    }
    let mut order = Vec::new();
    for _ in 0..13 {
        // SAFETY: só consulta o foco.
        let focus = unsafe { GetFocus() };
        // SAFETY: só consulta.
        order.push(unsafe { GetDlgCtrlID(focus) });
        assert!(
            key(focus, WM_KEYDOWN, VK_TAB.0),
            "Tab passa pelo IsDialogMessage"
        );
        if order.len() == 11 {
            assert!(
                dialog::scroll_position().unwrap() > 0,
                "o fim do conteúdo rolou"
            );
        }
    }
    assert_eq!(
        order,
        vec![
            ID_ALTER,
            ID_STARTUP,
            ID_RETENTION,
            ID_MAX_ITEMS,
            ID_MAX_PINNED,
            ID_LIST,
            ID_LIST_EDIT,
            ID_ADD,
            ID_REMOVE,
            ID_SECRETS,
            ID_PROTECTION,
            ID_SAVE,
            ID_CANCEL,
        ]
    );
    // SAFETY: só consulta.
    let last = unsafe { GetDlgCtrlID(GetFocus()) };
    assert_eq!(last, ID_ALTER, "Aplicar desabilitado fica de fora");
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn quitting_with_the_window_open_cancels_what_was_applied() {
    let o = open();
    set_text(ID_MAX_PINNED, "40");
    click(ID_APPLY);
    assert_eq!(o.cfg.borrow().max_pinned, 40);
    dialog::cancel_if_open();
    assert!(!is_open());
    assert_eq!(o.cfg.borrow().max_pinned, 25);
    assert!(!o.path.exists());
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn saving_the_protection_starts_the_rewrite_in_the_history_and_apply_does_not() {
    use duplicata_win::history_window::{
        ENCRYPTION_TOGGLE_IDLE, ENCRYPTION_TOGGLE_TO_ON, WM_APP_QUERY_ENCRYPTION_TOGGLE,
    };
    let history = history_window();
    let query = || {
        // SAFETY: Histórico vivo desta thread; só consulta.
        unsafe { SendMessageW(history.hwnd(), WM_APP_QUERY_ENCRYPTION_TOGGLE, None, None) }.0
    };
    let _o = open_with(history.hwnd(), Some(HotkeyCombo::DEFAULT), |_| {});
    click(ID_PROTECTION);
    assert!(
        !enabled(ID_APPLY),
        "a proteção sozinha não habilita o Aplicar"
    );
    assert_eq!(
        query(),
        ENCRYPTION_TOGGLE_IDLE,
        "nada reescrito antes do Salvar"
    );
    click(ID_SAVE);
    assert!(!is_open(), "sem confirmação: salva e fecha");
    assert_eq!(
        query(),
        ENCRYPTION_TOGGLE_TO_ON,
        "o Histórico começa a reescrita"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn going_back_from_a_zero_typed_as_a_reduction_leaves_apply_disabled() {
    let _o = open();
    set_text(ID_MAX_ITEMS, "0");
    assert!(enabled(ID_APPLY), "o campo precisa ser reescrito");
    click(ID_SAVE);
    assert_eq!(text_of(ID_MAX_ITEMS), "1", "o campo mostra o ajustado");
    assert!(!enabled(ID_APPLY), "só irreversível pendente");
    // SAFETY: só consulta o foco desta thread.
    let focus = unsafe { GetFocus() };
    assert!(key(focus, WM_KEYDOWN, VK_ESCAPE.0), "Voltar pelo Esc");
    assert!(dialog::confirm_window().is_none());
    assert!(!enabled(ID_APPLY), "depois do Voltar continua desabilitado");
    close_if_open();
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_theme_change_with_the_confirmation_open_keeps_its_font_alive() {
    use windows::Win32::Graphics::Gdi::{GetObjectType, HGDIOBJ};
    let _o = open();
    set_text(ID_RETENTION, "3");
    click(ID_SAVE);
    let font = dialog::confirm_font().expect("confirmação aberta");
    let name: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    // SAFETY: janela viva; `name` vive durante a chamada síncrona.
    unsafe {
        SendMessageW(
            dialog::window().unwrap(),
            WM_SETTINGCHANGE,
            None,
            Some(LPARAM(name.as_ptr() as isize)),
        );
    }
    // SAFETY: só consulta o tipo do objeto GDI; 0 = já apagado.
    let kind = unsafe { GetObjectType(HGDIOBJ(font.0)) };
    assert_ne!(kind, 0, "a fonte da confirmação continua viva");
    close_if_open();
}

struct LimitReader(Arc<std::sync::atomic::AtomicUsize>);
impl HistoryReader for LimitReader {
    fn list_for_display(
        &self,
        limit: usize,
    ) -> Result<Vec<duplicata_core::ClipListItem>, StoreError> {
        self.0.store(limit, std::sync::atomic::Ordering::SeqCst);
        Ok(Vec::new())
    }
    fn get_full(&self, _clip_id: i64) -> Result<Option<duplicata_core::StoredClip>, StoreError> {
        Ok(None)
    }
    fn count_pinned(&self) -> Result<u32, StoreError> {
        Ok(0)
    }
    fn close(self: Box<Self>) -> Result<(), StoreError> {
        Ok(())
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn applying_the_pin_ceiling_reaches_the_history_list() {
    use windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY;
    let limit = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let history = HistoryWindow::new(
        LimitReader(Arc::clone(&limit)),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        Arc::new(ByteBudgetQueue::new()) as Arc<dyn CaptureQueue<WorkItem>>,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");
    let _o = open_with(history.hwnd(), Some(HotkeyCombo::DEFAULT), |_| {});
    set_text(ID_MAX_PINNED, "40");
    click(ID_APPLY);
    close_if_open();
    // SAFETY: Histórico desta thread; simula o atalho global (id 1).
    unsafe {
        SendMessageW(history.hwnd(), WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
    }
    assert_eq!(
        limit.load(std::sync::atomic::Ordering::SeqCst),
        500 + 40,
        "o Histórico usa o teto aplicado"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_toggle_changes_only_when_the_press_started_on_it() {
    use windows::Win32::UI::WindowsAndMessaging::{WM_LBUTTONDOWN, WM_LBUTTONUP};
    let _o = open();
    let toggle = control(ID_SECRETS);
    let inside = Some(LPARAM((5 << 16) | 5));
    // SAFETY: alternância viva desta thread; mensagens síncronas.
    unsafe { SendMessageW(toggle, WM_LBUTTONUP, None, inside) };
    assert!(
        checked(ID_SECRETS),
        "soltar o clique sem ter apertado nela não muda"
    );
    // SAFETY: idem.
    unsafe { SendMessageW(toggle, WM_KEYUP, Some(WPARAM(VK_SPACE.0 as usize)), None) };
    assert!(
        checked(ID_SECRETS),
        "soltar o Espaço sem ter apertado nela não muda"
    );
    // SAFETY: idem.
    unsafe {
        SendMessageW(toggle, WM_LBUTTONDOWN, None, inside);
        SendMessageW(toggle, WM_LBUTTONUP, None, inside);
    }
    assert!(!checked(ID_SECRETS), "apertar e soltar nela muda");
    // SAFETY: idem.
    unsafe {
        SendMessageW(toggle, WM_KEYDOWN, Some(WPARAM(VK_SPACE.0 as usize)), None);
        SendMessageW(toggle, WM_KEYUP, Some(WPARAM(VK_SPACE.0 as usize)), None);
    }
    assert!(checked(ID_SECRETS), "Espaço apertado e solto nela muda");
    close_if_open();
}
