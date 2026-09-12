#![cfg(windows)]

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::Arc;

use duplicata_core::{
    ByteBudgetQueue, CaptureQueue, Config, FakeHistoryRepository, HistoryReader, HotkeyCombo,
    SelfWriteFilter, StoreError, WorkItem,
};
use duplicata_win::history_window::HistoryWindow;
use duplicata_win::hotkey_capture::{self, CaptureOwner};
use duplicata_win::settings_dialog;
use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, FindWindowW, GetDlgItem, IsWindow, SendMessageW, BM_CLICK, LB_SETCURSEL,
    WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN, WM_SETTEXT,
};

fn open_dialog(
    cfg: &Rc<RefCell<Config>>,
    config_path: &std::path::Path,
) -> Arc<ByteBudgetQueue<WorkItem>> {
    open_dialog_with_hotkey(cfg, config_path, Rc::new(Cell::new(None))).0
}

fn open_dialog_with_hotkey(
    cfg: &Rc<RefCell<Config>>,
    config_path: &std::path::Path,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
) -> (
    Arc<ByteBudgetQueue<WorkItem>>,
    Rc<Cell<Option<HotkeyCombo>>>,
) {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    settings_dialog::open(
        Rc::clone(cfg),
        config_path.to_path_buf(),
        Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>,
        windows::Win32::Foundation::HWND::default(),
        Rc::clone(&registered_hotkey),
    );
    (queue, registered_hotkey)
}

const ID_LISTBOX: i32 = 101;
const ID_EDIT: i32 = 102;
const ID_ADD: i32 = 103;
const ID_REMOVE: i32 = 104;
const ID_HEURISTIC_TOGGLE: i32 = 105;
const ID_RETENTION_DAYS_EDIT: i32 = 106;
const ID_MAX_ITEMS_EDIT: i32 = 107;
const ID_APPLY_RETENTION: i32 = 108;
const ID_MAX_PINNED_EDIT: i32 = 109;
const ID_HOTKEY_ALTER: i32 = 111;
const ID_HOTKEY_STATUS: i32 = 112;

fn new_test_window() -> HistoryWindow {
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        std::path::PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela deve funcionar numa sessão gráfica")
}

/// # Safety
/// `hwnd` deve ser um `HWND` de controle válido.
unsafe fn window_text(hwnd: HWND) -> String {
    // SAFETY: `hwnd` válido (contrato desta função).
    let len = unsafe { SendMessageW(hwnd, WM_GETTEXTLENGTH, None, None) }.0 as usize;
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` tem espaço para o NUL que `WM_GETTEXT` sempre escreve.
    unsafe {
        SendMessageW(
            hwnd,
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        );
    }
    String::from_utf16_lossy(&buf[..len])
}

fn find_settings_window() -> HWND {
    // SAFETY: `FindWindowW` só consulta janelas top-level já existentes, sem
    // pré-condição.
    unsafe {
        FindWindowW(
            windows::core::w!("duplicata_settings_dialog"),
            windows::core::w!("Configurações — duplicata"),
        )
    }
    .expect("janela de configurações não encontrada — settings_dialog::open falhou?")
}

fn cfg_at(dir: &std::path::Path) -> Config {
    Config::with_paths(dir.join("db"), dir.join("logs"))
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn adding_and_removing_via_the_ui_round_trips_through_config_toml() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));

    let _queue = open_dialog(&cfg, &config_path);
    let hwnd = find_settings_window();

    // SAFETY: `hwnd` acabou de ser localizado por `find_settings_window`;
    // `GetDlgItem` só consulta a árvore de filhos já criada em `WM_CREATE`.
    let edit = unsafe { GetDlgItem(Some(hwnd), ID_EDIT) }.expect("EDIT não encontrado");
    // SAFETY: idem acima.
    let add_btn =
        unsafe { GetDlgItem(Some(hwnd), ID_ADD) }.expect("botão Adicionar não encontrado");
    // SAFETY: idem acima.
    let remove_btn =
        unsafe { GetDlgItem(Some(hwnd), ID_REMOVE) }.expect("botão Remover não encontrado");
    // SAFETY: idem acima.
    let listbox = unsafe { GetDlgItem(Some(hwnd), ID_LISTBOX) }.expect("LISTBOX não encontrado");

    let text: Vec<u16> = "app-de-teste.exe\0".encode_utf16().collect();
    // SAFETY: `edit`/`add_btn` válidos (obtidos acima); `text` NUL-terminado,
    // ponteiro válido pela duração desta chamada síncrona. `BM_CLICK` simula
    // um clique real — dispara `WM_COMMAND`/`BN_CLICKED` para `hwnd`
    // sincronamente, dentro do próprio `SendMessageW`.
    unsafe {
        SendMessageW(edit, WM_SETTEXT, None, Some(LPARAM(text.as_ptr() as isize)));
        SendMessageW(add_btn, BM_CLICK, None, None);
    }

    assert_eq!(
        cfg.borrow().blocked_programs,
        vec!["app-de-teste.exe".to_string()],
        "o clique em Adicionar deve refletir no Config compartilhado"
    );
    let on_disk = std::fs::read_to_string(&config_path).expect("config.toml deveria existir");
    let (reparsed, _) = Config::from_str_over(cfg_at(dir.path()), &on_disk);
    assert_eq!(
        reparsed.blocked_programs,
        vec!["app-de-teste.exe".to_string()],
        "e persistir corretamente em config.toml (round-trip via from_str_over)"
    );

    // --- Remover, pela UI de verdade --------------------------------------
    // SAFETY: `listbox`/`remove_btn` válidos. Seleciona o item 0 (o único).
    unsafe {
        SendMessageW(listbox, LB_SETCURSEL, Some(WPARAM(0)), None);
        SendMessageW(remove_btn, BM_CLICK, None, None);
    }

    assert!(
        cfg.borrow().blocked_programs.is_empty(),
        "o clique em Remover deve refletir no Config compartilhado"
    );
    let on_disk2 = std::fs::read_to_string(&config_path).expect("config.toml deveria existir");
    let (reparsed2, _) = Config::from_str_over(cfg_at(dir.path()), &on_disk2);
    assert!(
        reparsed2.blocked_programs.is_empty(),
        "remoção também persiste em config.toml"
    );

    // SAFETY: `hwnd` ainda válido — limpa para não interferir com outro
    // teste deste arquivo rodado depois na mesma sessão manual.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn adding_a_duplicate_name_case_insensitively_is_a_silent_no_op() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));

    let _queue = open_dialog(&cfg, &config_path);
    let hwnd = find_settings_window();
    // SAFETY: `hwnd` acabou de ser localizado; `GetDlgItem` só consulta.
    let edit = unsafe { GetDlgItem(Some(hwnd), ID_EDIT) }.expect("EDIT não encontrado");
    // SAFETY: idem acima.
    let add_btn =
        unsafe { GetDlgItem(Some(hwnd), ID_ADD) }.expect("botão Adicionar não encontrado");

    let first: Vec<u16> = "dup.exe\0".encode_utf16().collect();
    // SAFETY: `edit`/`add_btn` válidos; `first` NUL-terminado, ponteiro
    // válido pela duração desta chamada síncrona.
    unsafe {
        SendMessageW(
            edit,
            WM_SETTEXT,
            None,
            Some(LPARAM(first.as_ptr() as isize)),
        );
        SendMessageW(add_btn, BM_CLICK, None, None);
    }

    let second: Vec<u16> = "DUP.EXE\0".encode_utf16().collect();
    // SAFETY: idem acima.
    unsafe {
        SendMessageW(
            edit,
            WM_SETTEXT,
            None,
            Some(LPARAM(second.as_ptr() as isize)),
        );
        SendMessageW(add_btn, BM_CLICK, None, None);
    }

    assert_eq!(
        cfg.borrow().blocked_programs,
        vec!["dup.exe".to_string()],
        "a segunda tentativa (mesmo nome, case diferente) não deve duplicar a entrada"
    );

    // SAFETY: `hwnd` ainda válido.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn opening_twice_reuses_the_same_window_instead_of_creating_a_second_one() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));

    let _queue = open_dialog(&cfg, &config_path);
    let first = find_settings_window();

    let _queue = open_dialog(&cfg, &config_path);
    let second = find_settings_window();

    assert_eq!(
        first.0 as isize, second.0 as isize,
        "a segunda chamada a open() deve reusar a janela já aberta (SetForegroundWindow), \
         não criar uma segunda"
    );

    // SAFETY: `first`/`second` são o mesmo HWND, ainda válido.
    unsafe {
        let _ = DestroyWindow(first);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn escape_sent_directly_to_the_parent_closes_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));

    let _queue = open_dialog(&cfg, &config_path);
    let hwnd = find_settings_window();

    // Envia WM_KEYDOWN(VK_ESCAPE) direto para a janela-PAI — exercita
    // exatamente o caminho que o handler cobre (ver doc do módulo: só
    // funciona quando nenhum controle filho tem o foco de teclado; mandar
    // direto ao pai simula esse caso sem depender de qual controle o
    // Windows deu foco inicial ao criar a janela).
    // SAFETY: `hwnd` válido, obtido acima.
    unsafe {
        SendMessageW(hwnd, WM_KEYDOWN, Some(WPARAM(VK_ESCAPE.0 as usize)), None);
    }

    // SAFETY: `IsWindow` só consulta, seguro mesmo se `hwnd` já não existir
    // mais (é exatamente o que este teste espera confirmar).
    let still_a_window = unsafe { IsWindow(Some(hwnd)) }.as_bool();
    assert!(
        !still_a_window,
        "Esc enviado ao pai (nenhum filho com foco) deve fechar a janela"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn toggling_the_heuristic_checkbox_updates_and_persists_config() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));
    assert!(
        cfg.borrow().heuristic_secret_detection,
        "pré-condição: default é ligado (FR-007)"
    );

    let _queue = open_dialog(&cfg, &config_path);
    let hwnd = find_settings_window();
    // SAFETY: `hwnd` acabou de ser localizado; `GetDlgItem` só consulta.
    let checkbox = unsafe { GetDlgItem(Some(hwnd), ID_HEURISTIC_TOGGLE) }
        .expect("checkbox da heurística não encontrado");

    // BS_AUTOCHECKBOX alterna sozinho o estado visual a cada BM_CLICK — o
    // handler só precisa ler o novo estado depois (ver toggle_heuristic).
    // SAFETY: `checkbox` válido.
    unsafe {
        SendMessageW(checkbox, BM_CLICK, None, None);
    }
    assert!(
        !cfg.borrow().heuristic_secret_detection,
        "primeiro clique desliga a detecção"
    );
    let on_disk = std::fs::read_to_string(&config_path).expect("config.toml deveria existir");
    let (reparsed, _) = Config::from_str_over(cfg_at(dir.path()), &on_disk);
    assert!(
        !reparsed.heuristic_secret_detection,
        "e persiste corretamente em config.toml"
    );

    // SAFETY: idem.
    unsafe {
        SendMessageW(checkbox, BM_CLICK, None, None);
    }
    assert!(
        cfg.borrow().heuristic_secret_detection,
        "segundo clique religa a detecção"
    );

    // SAFETY: `hwnd` ainda válido.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn applying_retention_and_pin_ceiling_persists_clamps_and_enqueues_apply_settings() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));

    let queue = open_dialog(&cfg, &config_path);
    let hwnd = find_settings_window();
    // SAFETY: `hwnd` localizado; `GetDlgItem` só consulta a árvore de filhos.
    let days_edit = unsafe { GetDlgItem(Some(hwnd), ID_RETENTION_DAYS_EDIT) }
        .expect("EDIT do prazo não encontrado");
    // SAFETY: idem.
    let items_edit = unsafe { GetDlgItem(Some(hwnd), ID_MAX_ITEMS_EDIT) }
        .expect("EDIT da quantidade não encontrado");
    // SAFETY: idem.
    let pinned_edit = unsafe { GetDlgItem(Some(hwnd), ID_MAX_PINNED_EDIT) }
        .expect("EDIT do teto de fixados não encontrado");
    // SAFETY: idem.
    let apply_btn = unsafe { GetDlgItem(Some(hwnd), ID_APPLY_RETENTION) }
        .expect("botão Aplicar não encontrado");

    let zero: Vec<u16> = "0\0".encode_utf16().collect();
    // SAFETY: os HWNDs de EDIT/BUTTON são válidos; `zero` NUL-terminado.
    unsafe {
        SendMessageW(
            days_edit,
            WM_SETTEXT,
            None,
            Some(LPARAM(zero.as_ptr() as isize)),
        );
        SendMessageW(
            items_edit,
            WM_SETTEXT,
            None,
            Some(LPARAM(zero.as_ptr() as isize)),
        );
        SendMessageW(
            pinned_edit,
            WM_SETTEXT,
            None,
            Some(LPARAM(zero.as_ptr() as isize)),
        );
        SendMessageW(apply_btn, BM_CLICK, None, None);
    }

    assert_eq!(cfg.borrow().max_items, 1, "max_items abaixo do piso vira 1");
    assert_eq!(
        cfg.borrow().retention.as_secs(),
        24 * 60 * 60,
        "prazo abaixo do piso vira 1 dia"
    );
    assert_eq!(
        cfg.borrow().max_pinned,
        0,
        "max_pinned = 0 é aceito como escrito (sem piso, FR-025)"
    );
    let on_disk = std::fs::read_to_string(&config_path).expect("config.toml deveria existir");
    let (reparsed, _) = Config::from_str_over(cfg_at(dir.path()), &on_disk);
    assert_eq!(reparsed.max_items, 1);
    assert_eq!(reparsed.retention.as_secs(), 24 * 60 * 60);
    assert_eq!(reparsed.max_pinned, 0);

    queue.close();
    match queue.pop() {
        Some(WorkItem::ApplySettings {
            retention_ms,
            max_items,
            max_pinned,
            ..
        }) => {
            assert_eq!(max_items, 1);
            assert_eq!(retention_ms, 24 * 60 * 60 * 1000);
            assert_eq!(max_pinned, 0);
        }
        other => panic!("esperava WorkItem::ApplySettings na fila, veio {other:?}"),
    }

    // SAFETY: `hwnd` ainda válido.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria uma janela real"]
fn a_pin_ceiling_above_200_is_clamped_to_200_by_the_ui_too() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));

    let queue = open_dialog(&cfg, &config_path);
    let hwnd = find_settings_window();
    // SAFETY: `hwnd` localizado; `GetDlgItem` só consulta a árvore de filhos.
    let pinned_edit = unsafe { GetDlgItem(Some(hwnd), ID_MAX_PINNED_EDIT) }
        .expect("EDIT do teto de fixados não encontrado");
    // SAFETY: idem.
    let apply_btn = unsafe { GetDlgItem(Some(hwnd), ID_APPLY_RETENTION) }
        .expect("botão Aplicar não encontrado");

    let above_ceiling: Vec<u16> = "250\0".encode_utf16().collect();
    // SAFETY: HWNDs válidos; `above_ceiling` NUL-terminado.
    unsafe {
        SendMessageW(
            pinned_edit,
            WM_SETTEXT,
            None,
            Some(LPARAM(above_ceiling.as_ptr() as isize)),
        );
        SendMessageW(apply_btn, BM_CLICK, None, None);
    }

    assert_eq!(
        cfg.borrow().max_pinned,
        200,
        "250 acima do teto de 200 deve ser ajustado para 200 (FR-030i)"
    );
    let on_disk = std::fs::read_to_string(&config_path).expect("config.toml deveria existir");
    let (reparsed, _) = Config::from_str_over(cfg_at(dir.path()), &on_disk);
    assert_eq!(
        reparsed.max_pinned, 200,
        "e persistir já clampado em config.toml"
    );

    queue.close();
    match queue.pop() {
        Some(WorkItem::ApplySettings { max_pinned, .. }) => {
            assert_eq!(max_pinned, 200, "a worker recebe o valor JÁ clampado");
        }
        other => panic!("esperava WorkItem::ApplySettings na fila, veio {other:?}"),
    }

    // SAFETY: `hwnd` ainda válido.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn the_section_is_inactive_while_the_standalone_dialog_holds_the_capture() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));
    let history_window = new_test_window();

    hotkey_capture::arm(CaptureOwner::StandaloneDialog);

    let _queue = open_dialog_with_hotkey(
        &cfg,
        &config_path,
        Rc::new(std::cell::Cell::new(Some(HotkeyCombo::DEFAULT))),
    );
    let hwnd = find_settings_window();
    // SAFETY: `hwnd` localizado; `GetDlgItem` só consulta a árvore de filhos.
    let status = unsafe { GetDlgItem(Some(hwnd), ID_HOTKEY_STATUS) }
        .expect("STATIC do atalho não encontrado");
    // SAFETY: idem.
    let alter_btn =
        unsafe { GetDlgItem(Some(hwnd), ID_HOTKEY_ALTER) }.expect("botão Alterar não encontrado");

    // SAFETY: `status` válido.
    let text = unsafe { window_text(status) };
    assert!(
        text.to_lowercase().contains("aberta"),
        "motivo deve identificar que a configuração está aberta em outro lugar — texto: {text:?}"
    );
    // SAFETY: `alter_btn` válido — `IsWindowEnabled` só consulta.
    let enabled =
        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(alter_btn) }
            .as_bool();
    assert!(
        !enabled,
        "o botão \"Alterar\" deve estar desabilitado enquanto inativa"
    );

    hotkey_capture::release(CaptureOwner::StandaloneDialog);
    settings_dialog::open(
        Rc::clone(&cfg),
        config_path.clone(),
        Arc::new(ByteBudgetQueue::new()) as Arc<dyn CaptureQueue<WorkItem>>,
        history_window.hwnd(),
        Rc::new(std::cell::Cell::new(Some(HotkeyCombo::DEFAULT))),
    );

    // SAFETY: `status`/`alter_btn` continuam válidos (mesma janela reusada).
    let text_after = unsafe { window_text(status) };
    assert!(
        !text_after.to_lowercase().contains("aberta"),
        "a seção deve voltar a ficar ativa depois da detenção liberada — texto: {text_after:?}"
    );
    // SAFETY: `alter_btn` ainda válido — `IsWindowEnabled` só consulta.
    let enabled_after =
        unsafe { windows::Win32::UI::Input::KeyboardAndMouse::IsWindowEnabled(alter_btn) }
            .as_bool();
    assert!(
        enabled_after,
        "o botão \"Alterar\" deve voltar a ficar habilitado"
    );

    // SAFETY: `hwnd` ainda válido.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn arming_the_capture_intercepts_keys_and_escape_disarms_without_closing() {
    let dir = tempfile::tempdir().unwrap();
    let config_path = dir.path().join("config.toml");
    let cfg = Rc::new(RefCell::new(cfg_at(dir.path())));
    let history_window = new_test_window();
    let registered_hotkey = Rc::new(std::cell::Cell::new(Some(HotkeyCombo::DEFAULT)));

    settings_dialog::open(
        Rc::clone(&cfg),
        config_path.clone(),
        Arc::new(ByteBudgetQueue::new()) as Arc<dyn CaptureQueue<WorkItem>>,
        history_window.hwnd(),
        Rc::clone(&registered_hotkey),
    );
    let hwnd = find_settings_window();
    // SAFETY: `hwnd` localizado; `GetDlgItem` só consulta a árvore de filhos.
    let alter_btn =
        unsafe { GetDlgItem(Some(hwnd), ID_HOTKEY_ALTER) }.expect("botão Alterar não encontrado");
    // SAFETY: idem.
    let status = unsafe { GetDlgItem(Some(hwnd), ID_HOTKEY_STATUS) }
        .expect("STATIC do atalho não encontrado");

    assert!(!hotkey_capture::is_armed());
    // SAFETY: `alter_btn` válido — `BM_CLICK` simula um clique real.
    unsafe { SendMessageW(alter_btn, BM_CLICK, None, None) };
    assert!(
        hotkey_capture::is_armed(),
        "clicar em \"Alterar\" deve armar a captura"
    );
    assert_eq!(hotkey_capture::owner(), CaptureOwner::SettingsSection);

    // Uma tecla comum, sem nenhum modificador físico realmente pressionado
    // (o estado de uma sessão de teste automatizada) — deve ser recusada
    // (FR-006 NoModifier) SEM desarmar (FR-047: sem "OK" separado, tenta de
    // novo na hora).
    // SAFETY: `alter_btn` tem o subclass instalado; entrega direto a ele,
    // como o foco real entregaria.
    unsafe { SendMessageW(alter_btn, WM_KEYDOWN, Some(WPARAM(0x41)), None) };
    assert!(
        hotkey_capture::is_armed(),
        "uma recusa NÃO deve desarmar a captura (mesmo estilo de hotkey_dialog.rs)"
    );
    // SAFETY: `status` válido.
    let refusal_text = unsafe { window_text(status) };
    assert!(
        refusal_text.to_lowercase().contains("modificador"),
        "a recusa deve nomear o motivo (FR-048b) — texto: {refusal_text:?}"
    );

    // Esc desarma e NÃO fecha o diálogo (FR-047a).
    // SAFETY: idem acima.
    unsafe {
        SendMessageW(
            alter_btn,
            WM_KEYDOWN,
            Some(WPARAM(VK_ESCAPE.0 as usize)),
            None,
        )
    };
    assert!(!hotkey_capture::is_armed(), "Esc deve desarmar");
    // SAFETY: `IsWindow` só consulta.
    let still_open = unsafe { IsWindow(Some(hwnd)) }.as_bool();
    assert!(
        still_open,
        "o diálogo de Configurações deve continuar aberto depois do Esc"
    );
    assert_eq!(
        registered_hotkey.get(),
        Some(HotkeyCombo::DEFAULT),
        "nenhuma recusa nem o Esc alteram a combinação em vigor"
    );

    // SAFETY: `hwnd` ainda válido — este `DestroyWindow` também prova
    // FR-049a: a detenção não pode sobreviver a ele.
    unsafe {
        let _ = DestroyWindow(hwnd);
    }
    assert_eq!(
        hotkey_capture::owner(),
        CaptureOwner::None,
        "destruir a janela deve liberar a detenção (FR-049a)"
    );
}
