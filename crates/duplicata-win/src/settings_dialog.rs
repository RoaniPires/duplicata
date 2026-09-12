use std::cell::{Cell, RefCell};
use std::ffi::c_void;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use duplicata_core::config::{MAX_MAX_PINNED, MIN_MAX_ITEMS, MIN_RETENTION_DAYS};
use duplicata_core::{
    format_hotkey, interpret_approval, startup_state, CaptureQueue, Config, HotkeyCombo,
    StartupState, WorkItem,
};
use windows::core::{BOOL, PCWSTR};
use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    CreateFontIndirectW, DeleteObject, GetStockObject, DEFAULT_GUI_FONT, HFONT, HGDIOBJ,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, SetFocus, VIRTUAL_KEY, VK_CONTROL, VK_ESCAPE, VK_LWIN, VK_MENU, VK_RWIN, VK_SHIFT,
};
use windows::Win32::UI::Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRect, CreateWindowExW, DefWindowProcW, DestroyWindow, EnumChildWindows,
    GetWindowLongPtrW, IsWindow, LoadCursorW, RegisterClassW, SendMessageW, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, SystemParametersInfoW, BM_GETCHECK, BM_SETCHECK,
    BS_AUTOCHECKBOX, BS_MULTILINE, CREATESTRUCTW, CW_USEDEFAULT, GWLP_USERDATA, HMENU, IDC_ARROW,
    LB_ADDSTRING, LB_GETCURSEL, LB_RESETCONTENT, NONCLIENTMETRICSW, SPI_GETNONCLIENTMETRICS,
    SWP_NOMOVE, SWP_NOZORDER, SW_HIDE, SW_SHOW, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, WINDOW_STYLE,
    WM_CLOSE, WM_COMMAND, WM_CREATE, WM_GETTEXT, WM_GETTEXTLENGTH, WM_KEYDOWN, WM_NCCREATE,
    WM_NCDESTROY, WM_SETFONT, WM_SETTEXT, WNDCLASSW, WS_BORDER, WS_CAPTION, WS_CHILD, WS_POPUP,
    WS_SYSMENU, WS_TABSTOP, WS_VISIBLE, WS_VSCROLL,
};

use crate::hotkey_capture::{self, CaptureOwner};
use crate::hotkey_dialog;
use crate::startup_registry;

const BST_CHECKED: usize = 1;
const BST_UNCHECKED: usize = 0;

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_settings_dialog");
const WINDOW_TITLE: PCWSTR = windows::core::w!("Configurações — duplicata");
const WINDOW_WIDTH: i32 = 480;
const WINDOW_HEIGHT_HINT: i32 = 680;
const MARGIN: i32 = 14;
const CONTENT_W: i32 = WINDOW_WIDTH - 2 * MARGIN - 4;

const BLOCKLIST_WARNING: PCWSTR = windows::core::w!(
    "A identificação é feita só pelo nome do arquivo executável (ex.: \"keepass.exe\"). \
     Isso é uma conveniência contra vazamento acidental por aplicativos legítimos — \
     NÃO é uma defesa contra um programa que se disfarce com o mesmo nome."
);

const HEURISTIC_TOGGLE_LABEL: PCWSTR = windows::core::w!(
    "Descartar automaticamente conteúdo que aparenta ser segredo (chave privada, token, cartão)"
);

const RETENTION_DAYS_LABEL: PCWSTR = windows::core::w!("Prazo de retenção (dias):");
const MAX_ITEMS_LABEL: PCWSTR = windows::core::w!("Quantidade máxima de itens não fixados:");
const MAX_PINNED_LABEL: PCWSTR = windows::core::w!("Teto de itens fixados ao mesmo tempo:");

const RETENTION_FLOOR_NOTE: PCWSTR = windows::core::w!(
    "Prazo e quantidade têm mínimo de 1 dia e 1 item — um valor menor é ajustado \
     para o mínimo ao aplicar. Com a quantidade máxima em 1, o histórico deixa de \
     reter mais de um item: cada nova captura substitui a anterior. Itens fixados \
     não contam para esse limite e nunca são removidos pela retenção automática. \
     O teto de fixados NÃO tem piso: 0 é válido e desliga a possibilidade de fixar \
     (itens já fixados continuam fixados). O teto tem, porém, um MÁXIMO absoluto \
     de 200 — um valor maior é ajustado para 200 ao aplicar (Fatia 4, FR-030i)."
);

const ENCRYPTION_TOGGLE_LABEL: PCWSTR =
    windows::core::w!("Proteger o arquivo do banco com a conta do Windows (criptografia DPAPI)");

const ENCRYPTION_DISCLOSURE: PCWSTR = windows::core::w!(
    "Protege o arquivo do banco contra alguém com acesso ao disco fora da sua conta \
     do Windows (por exemplo, se o computador for perdido, roubado ou acessado por \
     outra conta). NÃO protege contra outro programa em execução na sua própria \
     sessão do Windows enquanto você estiver logado. Ativar/desativar reescreve o \
     arquivo inteiro e pode demorar num banco grande; a janela continua respondendo."
);

const STARTUP_TOGGLE_LABEL: PCWSTR = windows::core::w!("Iniciar com o Windows");

const STARTUP_DISAPPROVED_TEXT: &str = "Desativado pelo Gerenciador de Tarefas do Windows.";

const HOTKEY_SECTION_LABEL: PCWSTR = windows::core::w!("Atalho global:");
const HOTKEY_ALTER_LABEL: PCWSTR = windows::core::w!("Alterar");
const SUBCLASS_HOTKEY_ID: usize = 1;

const ID_LISTBOX: usize = 101;
const ID_EDIT: usize = 102;
const ID_ADD: usize = 103;
const ID_REMOVE: usize = 104;
const ID_HEURISTIC_TOGGLE: usize = 105;
const ID_RETENTION_DAYS_EDIT: usize = 106;
const ID_MAX_ITEMS_EDIT: usize = 107;
const ID_APPLY_RETENTION: usize = 108;
const ID_MAX_PINNED_EDIT: usize = 109;
const ID_ENCRYPTION_TOGGLE: usize = 110;
const ID_HOTKEY_ALTER: usize = 111;
const ID_HOTKEY_STATUS: usize = 112;
const ID_STARTUP_TOGGLE: usize = 113;
const ID_STARTUP_STATUS: usize = 114;

struct SettingsDialogCtx {
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    queue: Arc<dyn CaptureQueue<WorkItem>>,
    history_hwnd: HWND,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
    dialog_hwnd: HWND,
    listbox: HWND,
    edit: HWND,
    heuristic_checkbox: HWND,
    retention_days_edit: HWND,
    max_items_edit: HWND,
    max_pinned_edit: HWND,
    encryption_checkbox: HWND,
    startup_checkbox: HWND,
    startup_status: HWND,
    hotkey_status: HWND,
    hotkey_alter: HWND,
    font: HFONT,
    font_owned: bool,
}

thread_local! {
    static OPEN_HWND: Cell<Option<isize>> = const { Cell::new(None) };
}

pub fn open(
    config: Rc<RefCell<Config>>,
    config_path: PathBuf,
    queue: Arc<dyn CaptureQueue<WorkItem>>,
    history_hwnd: HWND,
    registered_hotkey: Rc<Cell<Option<HotkeyCombo>>>,
) {
    if let Some(bits) = OPEN_HWND.with(Cell::get) {
        let existing = HWND(bits as *mut core::ffi::c_void);
        // SAFETY: `IsWindow` só consulta — seguro mesmo se `existing` for um
        // handle de uma janela já destruída (devolve `false`, não UB).
        if unsafe { IsWindow(Some(existing)) }.as_bool() {
            // SAFETY: acabamos de confirmar que `existing` é uma janela viva;
            // `ctx_ref` lê o mesmo `GWLP_USERDATA` que `WM_NCCREATE` põe nela.
            unsafe {
                let _ = SetForegroundWindow(existing);
                if let Some(ctx) = ctx_ref(existing) {
                    refresh_hotkey_status(ctx);
                    refresh_startup_status(ctx);
                }
            }
            return;
        }
    }

    // SAFETY: registro de classe idempotente com `wndproc` válido (mesmo
    // padrão de `history_window.rs`/`hotkey_dialog.rs`/`tray.rs`); ignoramos
    // o erro de "classe já registrada". `GetModuleHandleW(None)` pede o
    // módulo do processo atual, sem pré-condição além disso.
    unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            return;
        };
        let cursor = LoadCursorW(None, IDC_ARROW).unwrap_or_default();
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            hCursor: cursor,
            ..Default::default()
        };
        RegisterClassW(&wc);

        let (font, font_owned) = ui_font();
        let ctx = Box::new(SettingsDialogCtx {
            config,
            config_path,
            queue,
            history_hwnd,
            registered_hotkey,
            dialog_hwnd: HWND::default(),
            listbox: HWND::default(),
            edit: HWND::default(),
            heuristic_checkbox: HWND::default(),
            retention_days_edit: HWND::default(),
            max_items_edit: HWND::default(),
            max_pinned_edit: HWND::default(),
            encryption_checkbox: HWND::default(),
            startup_checkbox: HWND::default(),
            startup_status: HWND::default(),
            hotkey_status: HWND::default(),
            hotkey_alter: HWND::default(),
            font,
            font_owned,
        });
        let ctx_ptr = Box::into_raw(ctx);

        let Ok(hwnd) = CreateWindowExW(
            Default::default(),
            CLASS_NAME,
            WINDOW_TITLE,
            WS_POPUP | WS_CAPTION | WS_SYSMENU,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            WINDOW_WIDTH,
            WINDOW_HEIGHT_HINT,
            None,
            None,
            Some(hinstance.into()),
            Some(ctx_ptr.cast()),
        ) else {
            return;
        };

        OPEN_HWND.with(|c| c.set(Some(hwnd.0 as isize)));
        let _ = ShowWindow(hwnd, SW_SHOW);
        let _ = SetForegroundWindow(hwnd);
    }
}

/// # Safety
/// `hwnd` deve ser a janela recém-criada recebendo seu próprio `WM_CREATE`.
unsafe fn create_children(hwnd: HWND, ctx: &mut SettingsDialogCtx) {
    ctx.dialog_hwnd = hwnd;

    // SAFETY: `GetModuleHandleW(None)` pede o módulo do processo atual, sem
    // pré-condição além disso.
    let hinstance = unsafe { GetModuleHandleW(None) }.ok();

    let mk = |class: PCWSTR,
              text: PCWSTR,
              extra: WINDOW_STYLE,
              x: i32,
              y: i32,
              w: i32,
              h: i32,
              id: usize|
     -> HWND {
        let menu = if id == 0 {
            None
        } else {
            Some(HMENU(id as *mut core::ffi::c_void))
        };
        // SAFETY: classe predefinida pelo sistema; `hwnd` é o pai, válido
        // (dentro do próprio `WM_CREATE` dele).
        unsafe {
            CreateWindowExW(
                Default::default(),
                class,
                text,
                WS_CHILD | WS_VISIBLE | extra,
                x,
                y,
                w,
                h,
                Some(hwnd),
                menu,
                hinstance.map(Into::into),
                None,
            )
        }
        .unwrap_or_default()
    };

    let plain = WINDOW_STYLE(0);
    let m = MARGIN;
    let cw = CONTENT_W;
    let mut y = 12;

    mk(
        windows::core::w!("STATIC"),
        HOTKEY_SECTION_LABEL,
        plain,
        m,
        y + 3,
        cw - 110,
        18,
        0,
    );
    let hotkey_status = mk(
        windows::core::w!("STATIC"),
        PCWSTR::null(),
        plain,
        m,
        y + 22,
        cw,
        36,
        ID_HOTKEY_STATUS,
    );
    let hotkey_alter = mk(
        windows::core::w!("BUTTON"),
        HOTKEY_ALTER_LABEL,
        WS_TABSTOP,
        m + cw - 90,
        y,
        90,
        24,
        ID_HOTKEY_ALTER,
    );
    y += 22 + 36 + 16;

    // SAFETY: `hotkey_alter` acabou de ser criado por `mk` acima; `ctx`
    // aponta para o `SettingsDialogCtx` vivo até `WM_NCDESTROY` — o mesmo
    // endereço que `GWLP_USERDATA` guarda (`ctx` É o ponteiro reconstruído a
    // partir dele em `ctx_ref`), então repassá-lo como `dwRefData` é válido
    // por toda a vida do `BUTTON` (destruído junto com o pai, sempre ANTES
    // do `WM_NCDESTROY` do pai que dropa o `Box` — ver doc do subclass).
    unsafe {
        let ctx_addr = ctx as *mut SettingsDialogCtx as usize;
        let _ = SetWindowSubclass(
            hotkey_alter,
            Some(hotkey_alter_subclass),
            SUBCLASS_HOTKEY_ID,
            ctx_addr,
        );
    }

    mk(
        windows::core::w!("STATIC"),
        BLOCKLIST_WARNING,
        plain,
        m,
        y,
        cw,
        78,
        0,
    );
    y += 78 + 10;

    let listbox = mk(
        windows::core::w!("LISTBOX"),
        PCWSTR::null(),
        WS_BORDER | WS_VSCROLL | WS_TABSTOP,
        m,
        y,
        cw,
        116,
        ID_LISTBOX,
    );
    y += 116 + 6;

    let edit = mk(
        windows::core::w!("EDIT"),
        PCWSTR::null(),
        WS_BORDER | WS_TABSTOP,
        m,
        y,
        cw - 116,
        24,
        ID_EDIT,
    );
    mk(
        windows::core::w!("BUTTON"),
        windows::core::w!("Adicionar"),
        WS_TABSTOP,
        m + cw - 108,
        y,
        108,
        24,
        ID_ADD,
    );
    y += 24 + 4;

    mk(
        windows::core::w!("BUTTON"),
        windows::core::w!("Remover selecionado"),
        WS_TABSTOP,
        m,
        y,
        180,
        24,
        ID_REMOVE,
    );
    y += 24 + 16;

    let heuristic_checkbox = mk(
        windows::core::w!("BUTTON"),
        HEURISTIC_TOGGLE_LABEL,
        WS_TABSTOP | duplicata_win_style(BS_AUTOCHECKBOX) | duplicata_win_style(BS_MULTILINE),
        m,
        y,
        cw,
        40,
        ID_HEURISTIC_TOGGLE,
    );
    // SAFETY: `heuristic_checkbox` acabou de ser criado; estado inicial
    // reflete a configuração atual (FR-007 default ligado).
    unsafe {
        let initial = if ctx.config.borrow().heuristic_secret_detection {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        };
        SendMessageW(heuristic_checkbox, BM_SETCHECK, Some(WPARAM(initial)), None);
    }
    y += 40 + 16;

    mk(
        windows::core::w!("STATIC"),
        RETENTION_DAYS_LABEL,
        plain,
        m,
        y + 3,
        cw - 90,
        18,
        0,
    );
    let retention_days_edit = mk(
        windows::core::w!("EDIT"),
        PCWSTR::null(),
        WS_BORDER | WS_TABSTOP,
        m + cw - 80,
        y,
        80,
        22,
        ID_RETENTION_DAYS_EDIT,
    );
    y += 22 + 8;

    mk(
        windows::core::w!("STATIC"),
        MAX_ITEMS_LABEL,
        plain,
        m,
        y,
        cw,
        18,
        0,
    );
    y += 18 + 2;
    let max_items_edit = mk(
        windows::core::w!("EDIT"),
        PCWSTR::null(),
        WS_BORDER | WS_TABSTOP,
        m,
        y,
        80,
        22,
        ID_MAX_ITEMS_EDIT,
    );
    y += 22 + 8;

    mk(
        windows::core::w!("STATIC"),
        MAX_PINNED_LABEL,
        plain,
        m,
        y,
        cw,
        18,
        0,
    );
    y += 18 + 2;
    let max_pinned_edit = mk(
        windows::core::w!("EDIT"),
        PCWSTR::null(),
        WS_BORDER | WS_TABSTOP,
        m,
        y,
        80,
        22,
        ID_MAX_PINNED_EDIT,
    );
    y += 22 + 12;

    mk(
        windows::core::w!("BUTTON"),
        windows::core::w!("Aplicar retenção e teto de fixados"),
        WS_TABSTOP,
        m,
        y,
        300,
        26,
        ID_APPLY_RETENTION,
    );
    y += 26 + 12;

    mk(
        windows::core::w!("STATIC"),
        RETENTION_FLOOR_NOTE,
        plain,
        m,
        y,
        cw,
        132,
        0,
    );
    y += 132 + 16;

    let encryption_checkbox = mk(
        windows::core::w!("BUTTON"),
        ENCRYPTION_TOGGLE_LABEL,
        WS_TABSTOP | duplicata_win_style(BS_AUTOCHECKBOX) | duplicata_win_style(BS_MULTILINE),
        m,
        y,
        cw,
        36,
        ID_ENCRYPTION_TOGGLE,
    );
    // SAFETY: `encryption_checkbox` acabou de ser criado; estado inicial =
    // estado real da proteção (derivado pelo bootstrap, data-model.md §A).
    unsafe {
        let initial = if ctx.config.borrow().encryption_enabled {
            BST_CHECKED
        } else {
            BST_UNCHECKED
        };
        SendMessageW(
            encryption_checkbox,
            BM_SETCHECK,
            Some(WPARAM(initial)),
            None,
        );
    }
    y += 36 + 4;
    mk(
        windows::core::w!("STATIC"),
        ENCRYPTION_DISCLOSURE,
        plain,
        m,
        y,
        cw,
        110,
        0,
    );
    y += 110 + m;

    let startup_checkbox = mk(
        windows::core::w!("BUTTON"),
        STARTUP_TOGGLE_LABEL,
        WS_TABSTOP | duplicata_win_style(BS_AUTOCHECKBOX),
        m,
        y,
        cw,
        22,
        ID_STARTUP_TOGGLE,
    );
    y += 22 + 2;
    let startup_status = mk(
        windows::core::w!("STATIC"),
        PCWSTR::null(),
        plain,
        m,
        y,
        cw,
        18,
        ID_STARTUP_STATUS,
    );
    y += 18 + m;

    ctx.listbox = listbox;
    ctx.edit = edit;
    ctx.heuristic_checkbox = heuristic_checkbox;
    ctx.retention_days_edit = retention_days_edit;
    ctx.max_items_edit = max_items_edit;
    ctx.max_pinned_edit = max_pinned_edit;
    ctx.encryption_checkbox = encryption_checkbox;
    ctx.startup_checkbox = startup_checkbox;
    ctx.startup_status = startup_status;
    ctx.hotkey_status = hotkey_status;
    ctx.hotkey_alter = hotkey_alter;

    // Aplica a fonte de UI a TODOS os controles filhos — sem isto usam a
    // fonte "System" bitmap larga, que corta os textos longos.
    // SAFETY: `set_child_font` é um callback `extern "system"` válido; o
    // `HFONT` (`ctx.font`) vive até `WM_NCDESTROY`.
    unsafe {
        let _ = EnumChildWindows(
            Some(hwnd),
            Some(set_child_font),
            LPARAM(ctx.font.0 as isize),
        );
    }

    let mut rc = RECT {
        left: 0,
        top: 0,
        right: WINDOW_WIDTH,
        bottom: y,
    };
    // SAFETY: `AdjustWindowRect` só calcula sobre `rc` (válido, local).
    unsafe {
        let _ = AdjustWindowRect(&mut rc, WS_POPUP | WS_CAPTION | WS_SYSMENU, false);
    }
    // SAFETY: `hwnd` é a janela desta função; `SWP_NOMOVE|SWP_NOZORDER`
    // mantêm posição e ordem-Z, só o tamanho muda.
    unsafe {
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            rc.right - rc.left,
            rc.bottom - rc.top,
            SWP_NOMOVE | SWP_NOZORDER,
        );
    }

    // SAFETY: `ctx.listbox`/os três EDIT acabaram de ser criados, válidos.
    unsafe { refresh_listbox(ctx) };
    // SAFETY: idem — os três EDIT de retenção/fixados acabaram de ser criados.
    unsafe { refresh_retention_edits(ctx) };
    // SAFETY: idem — `ctx.hotkey_status`/`ctx.hotkey_alter` acabaram de ser
    // criados.
    unsafe { refresh_hotkey_status(ctx) };
    // SAFETY: idem — `ctx.startup_checkbox`/`ctx.startup_status` acabaram
    // de ser criados.
    unsafe { refresh_startup_status(ctx) };
}

fn ui_font() -> (HFONT, bool) {
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: core::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    // SAFETY: `cbSize` preenchido (exigido pela API); `SystemParametersInfoW`
    // com `SPI_GETNONCLIENTMETRICS` só lê para o buffer que passamos.
    let ok = unsafe {
        SystemParametersInfoW(
            SPI_GETNONCLIENTMETRICS,
            ncm.cbSize,
            Some(&mut ncm as *mut _ as *mut c_void),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok();
    if ok {
        // SAFETY: `ncm.lfMessageFont` foi preenchido pela chamada acima.
        let f = unsafe { CreateFontIndirectW(&ncm.lfMessageFont) };
        if !f.is_invalid() {
            return (f, true);
        }
    }
    // SAFETY: `GetStockObject` sem pré-condição; `DEFAULT_GUI_FONT` é um
    // stock font válido (Tahoma/MS Sans Serif).
    let stock: HGDIOBJ = unsafe { GetStockObject(DEFAULT_GUI_FONT) };
    (HFONT(stock.0), false)
}

unsafe extern "system" fn set_child_font(child: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: contrato do callback; `1` no `LPARAM` pede redesenho imediato.
    unsafe {
        SendMessageW(
            child,
            WM_SETFONT,
            Some(WPARAM(lparam.0 as usize)),
            Some(LPARAM(1)),
        );
    }
    BOOL(1)
}

/// # Safety
/// `ctx.retention_days_edit`/`ctx.max_items_edit`/`ctx.max_pinned_edit` devem
/// ser `HWND`s de EDIT válidos.
unsafe fn refresh_retention_edits(ctx: &SettingsDialogCtx) {
    let (days, max_items, max_pinned) = {
        let cfg = ctx.config.borrow();
        (
            cfg.retention.as_secs() / 86_400,
            cfg.max_items,
            cfg.max_pinned,
        )
    };
    // SAFETY: contrato desta função.
    unsafe {
        set_edit_text(ctx.retention_days_edit, &days.to_string());
        set_edit_text(ctx.max_items_edit, &max_items.to_string());
        set_edit_text(ctx.max_pinned_edit, &max_pinned.to_string());
    }
}

/// # Safety
/// `edit` deve ser um `HWND` de controle válido.
unsafe fn set_edit_text(edit: HWND, text: &str) {
    let wide: Vec<u16> = text.encode_utf16().chain(std::iter::once(0)).collect();
    // SAFETY: `edit` válido (contrato); `wide` NUL-terminado, ponteiro válido
    // pela duração desta chamada síncrona.
    unsafe {
        SendMessageW(edit, WM_SETTEXT, None, Some(LPARAM(wide.as_ptr() as isize)));
    }
}

const fn duplicata_win_style(bs: i32) -> windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE {
    windows::Win32::UI::WindowsAndMessaging::WINDOW_STYLE(bs as u32)
}

/// # Safety
/// `ctx.listbox` deve ser um `HWND` de LISTBOX válido.
unsafe fn refresh_listbox(ctx: &SettingsDialogCtx) {
    // SAFETY: `ctx.listbox` válido (contrato desta função).
    unsafe {
        SendMessageW(ctx.listbox, LB_RESETCONTENT, None, None);
    }
    for prog in &ctx.config.borrow().blocked_programs {
        let wide: Vec<u16> = prog.encode_utf16().chain(std::iter::once(0)).collect();
        // SAFETY: `ctx.listbox` válido; `wide` é NUL-terminado, ponteiro
        // válido pela duração desta chamada síncrona.
        unsafe {
            SendMessageW(
                ctx.listbox,
                LB_ADDSTRING,
                None,
                Some(LPARAM(wide.as_ptr() as isize)),
            );
        }
    }
}

/// # Safety
/// `edit` deve ser um `HWND` de EDIT válido.
unsafe fn edit_text_of(edit: HWND) -> String {
    // SAFETY: `edit` válido (contrato desta função).
    let len = unsafe { SendMessageW(edit, WM_GETTEXTLENGTH, None, None) }.0 as usize;
    if len == 0 {
        return String::new();
    }
    let mut buf = vec![0u16; len + 1];
    // SAFETY: `buf` tem capacidade `len + 1` (espaço para o NUL que
    // `WM_GETTEXT` sempre escreve); `wParam` informa essa capacidade.
    unsafe {
        SendMessageW(
            edit,
            WM_GETTEXT,
            Some(WPARAM(buf.len())),
            Some(LPARAM(buf.as_mut_ptr() as isize)),
        );
    }
    String::from_utf16_lossy(&buf[..len])
}

/// # Safety
/// `ctx.edit`/`ctx.listbox` devem ser `HWND`s válidos.
unsafe fn add_from_edit(ctx: &SettingsDialogCtx) {
    // SAFETY: contrato desta função.
    let text = unsafe { edit_text_of(ctx.edit) };
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return;
    }
    let already_there = ctx
        .config
        .borrow()
        .blocked_programs
        .iter()
        .any(|p| p.eq_ignore_ascii_case(trimmed));
    if already_there {
        return;
    }
    ctx.config
        .borrow_mut()
        .blocked_programs
        .push(trimmed.to_string());
    let _ = ctx.config.borrow().persist(&ctx.config_path);

    // SAFETY: `ctx.edit` válido — limpa o campo depois de adicionar de verdade.
    unsafe {
        let _ = SendMessageW(ctx.edit, WM_SETTEXT, None, Some(LPARAM(0)));
    }
    // SAFETY: `ctx.listbox` válido.
    unsafe { refresh_listbox(ctx) };
}

/// # Safety
/// `ctx.listbox` deve ser um `HWND` de LISTBOX válido.
unsafe fn remove_selected(ctx: &SettingsDialogCtx) {
    // SAFETY: `ctx.listbox` válido (contrato desta função). `LB_GETCURSEL`
    // devolve -1 se nada estiver selecionado.
    let index = unsafe { SendMessageW(ctx.listbox, LB_GETCURSEL, None, None) }.0 as i32;
    if index < 0 {
        return;
    }
    let mut cfg = ctx.config.borrow_mut();
    if (index as usize) < cfg.blocked_programs.len() {
        cfg.blocked_programs.remove(index as usize);
    }
    drop(cfg);
    let _ = ctx.config.borrow().persist(&ctx.config_path); // FR-005, best-effort
                                                           // SAFETY: `ctx.listbox` válido.
    unsafe { refresh_listbox(ctx) };
}

/// # Safety
/// `ctx.heuristic_checkbox` deve ser um `HWND` de `BUTTON` válido.
unsafe fn toggle_heuristic(ctx: &SettingsDialogCtx) {
    // SAFETY: `ctx.heuristic_checkbox` válido (contrato desta função).
    let checked = unsafe { SendMessageW(ctx.heuristic_checkbox, BM_GETCHECK, None, None) }.0
        as usize
        == BST_CHECKED;
    ctx.config.borrow_mut().heuristic_secret_detection = checked;
    let _ = ctx.config.borrow().persist(&ctx.config_path);
}

/// # Safety
/// `ctx.encryption_checkbox` deve ser um `HWND` de `BUTTON` válido e
/// `ctx.history_hwnd` a janela de histórico.
unsafe fn toggle_encryption(ctx: &SettingsDialogCtx) {
    // SAFETY: `ctx.encryption_checkbox` válido (contrato desta função).
    let enable = unsafe { SendMessageW(ctx.encryption_checkbox, BM_GETCHECK, None, None) }.0
        as usize
        == BST_CHECKED;
    ctx.config.borrow_mut().encryption_enabled = enable;
    // SAFETY: `ctx.history_hwnd` é a janela de histórico, na mesma thread;
    // `SendMessageW` entrega o `WM_APP_TOGGLE_ENCRYPTION` sincronamente.
    unsafe {
        SendMessageW(
            ctx.history_hwnd,
            crate::history_window::WM_APP_TOGGLE_ENCRYPTION,
            Some(WPARAM(enable as usize)),
            None,
        );
    }
}

/// # Safety
/// `ctx.startup_checkbox`/`ctx.startup_status` devem ser `HWND`s válidos.
unsafe fn toggle_startup(ctx: &SettingsDialogCtx) {
    // SAFETY: `ctx.startup_checkbox` válido (contrato desta função).
    let checked = unsafe { SendMessageW(ctx.startup_checkbox, BM_GETCHECK, None, None) }.0 as usize
        == BST_CHECKED;
    if checked {
        if let Ok(exe) = std::env::current_exe() {
            if let Some(path) = exe.to_str() {
                let _ = startup_registry::write_run_entry(path);
            }
        }
        let _ = startup_registry::clear_startup_approval();
    } else {
        let _ = startup_registry::remove_run_entry();
    }
    // SAFETY: contrato desta função.
    unsafe { refresh_startup_status(ctx) };
}

/// # Safety
/// `ctx.retention_days_edit`/`ctx.max_items_edit`/`ctx.max_pinned_edit` devem
/// ser `HWND`s de EDIT válidos.
unsafe fn apply_retention(ctx: &SettingsDialogCtx) {
    // SAFETY: contrato desta função.
    let (days_text, items_text, pinned_text) = unsafe {
        (
            edit_text_of(ctx.retention_days_edit),
            edit_text_of(ctx.max_items_edit),
            edit_text_of(ctx.max_pinned_edit),
        )
    };

    let (days, max_items, max_pinned) = {
        let cfg = ctx.config.borrow();
        let current_days = cfg.retention.as_secs() / 86_400;
        let days = days_text
            .trim()
            .parse::<u64>()
            .unwrap_or(current_days)
            .max(MIN_RETENTION_DAYS);
        let max_items = items_text
            .trim()
            .parse::<u32>()
            .unwrap_or(cfg.max_items)
            .max(MIN_MAX_ITEMS);
        let max_pinned = pinned_text
            .trim()
            .parse::<u32>()
            .unwrap_or(cfg.max_pinned)
            .min(MAX_MAX_PINNED);
        (days, max_items, max_pinned)
    };

    {
        let mut cfg = ctx.config.borrow_mut();
        cfg.retention = Duration::from_secs(days * 86_400);
        cfg.max_items = max_items;
        cfg.max_pinned = max_pinned;
    }
    let _ = ctx.config.borrow().persist(&ctx.config_path);

    // Reescreve os campos com o valor efetivo (o usuário vê o ajuste ao piso).
    // SAFETY: contrato desta função.
    unsafe { refresh_retention_edits(ctx) };

    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    ctx.queue.push(
        WorkItem::ApplySettings {
            now_ms,
            retention_ms: days.saturating_mul(86_400_000),
            max_items,
            max_pinned,
        },
        0,
    );
}

/// # Safety
/// `ctx.hotkey_status`/`ctx.hotkey_alter` devem ser HWNDs válidos.
unsafe fn refresh_hotkey_status(ctx: &SettingsDialogCtx) {
    if hotkey_capture::owner() == CaptureOwner::StandaloneDialog {
        // (FR-049) O diálogo autônomo de conflito detém a captura — a seção
        // fica inativa, com o motivo E o caminho para resolver (não só "não
        // dá aqui").
        // SAFETY: contrato desta função.
        unsafe {
            set_edit_text(
                ctx.hotkey_status,
                "A configuração de atalho já está aberta na janela de conflito — resolva-a \
                 ali (ela permanece sempre visível, no topo da tela).",
            );
            let _ = EnableWindow(ctx.hotkey_alter, false);
        }
        return;
    }
    // SAFETY: contrato desta função.
    unsafe {
        let _ = EnableWindow(ctx.hotkey_alter, true);
    }

    if hotkey_capture::is_armed() {
        // (FR-047a) Estado armado visualmente indicado.
        // SAFETY: contrato desta função.
        unsafe {
            set_edit_text(
                ctx.hotkey_status,
                "Pressione a nova combinação de teclas... Esc cancela.",
            );
        }
        return;
    }

    let text = match ctx.registered_hotkey.get() {
        Some(combo) => format_hotkey(&combo),
        None => format!(
            "Nenhum atalho está ativo. A combinação configurada, \"{}\", não pôde ser registrada.",
            format_hotkey(&ctx.config.borrow().hotkey)
        ),
    };
    // SAFETY: contrato desta função.
    unsafe { set_edit_text(ctx.hotkey_status, &text) };
}

/// # Safety
/// `ctx.hotkey_alter`/`ctx.hotkey_status` devem ser HWNDs válidos.
unsafe fn on_hotkey_alter_clicked(ctx: &SettingsDialogCtx) {
    if hotkey_capture::owner() == CaptureOwner::StandaloneDialog {
        return;
    }
    if !hotkey_capture::is_armed() {
        hotkey_capture::arm(CaptureOwner::SettingsSection);
    }
    // SAFETY: contrato desta função.
    unsafe {
        let _ = SetFocus(Some(ctx.hotkey_alter));
        refresh_hotkey_status(ctx);
    }
}

/// # Safety
/// `ctx.startup_checkbox`/`ctx.startup_status` devem ser `HWND`s válidos.
unsafe fn refresh_startup_status(ctx: &SettingsDialogCtx) {
    let entry_present = startup_registry::run_entry_present();
    let approval_raw = startup_registry::read_startup_approved_raw();
    let approval = interpret_approval(approval_raw.as_deref());
    let state = startup_state(entry_present, approval);

    let checked = matches!(state, StartupState::Ligado);
    // SAFETY: `ctx.startup_checkbox` válido (contrato desta função).
    unsafe {
        SendMessageW(
            ctx.startup_checkbox,
            BM_SETCHECK,
            Some(WPARAM(if checked { BST_CHECKED } else { BST_UNCHECKED })),
            None,
        );
    }

    // SAFETY: `ctx.startup_status` válido (contrato desta função).
    unsafe {
        match state {
            StartupState::DesligadoPeloGerenciador => {
                set_edit_text(ctx.startup_status, STARTUP_DISAPPROVED_TEXT);
                let _ = ShowWindow(ctx.startup_status, SW_SHOW);
            }
            StartupState::Ligado | StartupState::Desligado => {
                let _ = ShowWindow(ctx.startup_status, SW_HIDE);
            }
        }
    }
}

/// # Safety
/// `ctx.hotkey_status` deve ser um HWND válido; `ctx.dialog_hwnd` deve ser a
/// janela de Configurações já criada.
unsafe fn handle_hotkey_keydown(ctx: &SettingsDialogCtx, wparam: WPARAM) {
    let vk = VIRTUAL_KEY(wparam.0 as u16);
    if matches!(vk, VK_CONTROL | VK_SHIFT | VK_MENU | VK_LWIN | VK_RWIN) {
        return;
    }
    if vk == VK_ESCAPE {
        hotkey_capture::disarm();
        // SAFETY: contrato desta função.
        unsafe { refresh_hotkey_status(ctx) };
        return;
    }

    let candidate = HotkeyCombo {
        modifiers: hotkey_dialog::current_modifiers(),
        vkey: vk.0 as u32,
    };
    let outcome = hotkey_dialog::try_exchange(
        ctx.dialog_hwnd,
        ctx.history_hwnd,
        ctx.registered_hotkey.get(),
        candidate,
        &ctx.config,
        &ctx.config_path,
    );
    match outcome {
        hotkey_dialog::ExchangeOutcome::Accepted(combo) => {
            ctx.registered_hotkey.set(Some(combo));
            hotkey_capture::disarm();
        }
        hotkey_dialog::ExchangeOutcome::Refused { message } => {
            // SAFETY: contrato desta função.
            unsafe { set_edit_text(ctx.hotkey_status, &message) };
            return;
        }
    }
    // SAFETY: contrato desta função.
    unsafe { refresh_hotkey_status(ctx) };
}

unsafe extern "system" fn hotkey_alter_subclass(
    hwnd: HWND,
    msg: u32,
    wparam: WPARAM,
    lparam: LPARAM,
    uidsubclass: usize,
    dwrefdata: usize,
) -> LRESULT {
    if msg == WM_NCDESTROY {
        // Recomendação da própria Microsoft: remover o subclass ao ver
        // WM_NCDESTROY, antes de repassar ao handler padrão.
        // SAFETY: `hwnd`/`uidsubclass` identificam este mesmo subclass,
        // instalado em `create_children`.
        unsafe {
            let _ = RemoveWindowSubclass(hwnd, Some(hotkey_alter_subclass), uidsubclass);
        }
    } else if msg == WM_KEYDOWN && hotkey_capture::is_armed() {
        let ctx_ptr = dwrefdata as *const SettingsDialogCtx;
        // SAFETY: `dwrefdata` é o ponteiro posto por `create_children` — ver
        // doc desta função sobre por que ele continua válido aqui.
        if let Some(ctx) = unsafe { ctx_ptr.as_ref() } {
            // SAFETY: `ctx.hotkey_status`/`ctx.dialog_hwnd` válidos (postos
            // antes deste subclass ser instalado, na mesma `create_children`).
            unsafe { handle_hotkey_keydown(ctx, wparam) };
        }
        return LRESULT(0);
    }
    // SAFETY: repassa ao handler padrão do subclass.
    unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
}

fn ctx_ref(hwnd: HWND) -> Option<&'static mut SettingsDialogCtx> {
    // SAFETY: `GWLP_USERDATA` guarda o `*mut SettingsDialogCtx` posto em
    // `WM_NCCREATE`; o ponteiro é válido até `WM_NCDESTROY`, que o desaloca.
    let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsDialogCtx;
    // SAFETY: `ptr` ou é nulo ou aponta para o `SettingsDialogCtx` posto
    // acima — `as_mut()` trata nulo como `None` corretamente.
    unsafe { ptr.as_mut() }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_NCCREATE => {
            // SAFETY: no WM_NCCREATE, `lParam` aponta para um CREATESTRUCTW
            // cujo `lpCreateParams` é o `*mut SettingsDialogCtx` de `open()`
            // (mesmo padrão de `history_window.rs`/`hotkey_dialog.rs`).
            unsafe {
                let cs = &*(lparam.0 as *const CREATESTRUCTW);
                SetWindowLongPtrW(hwnd, GWLP_USERDATA, cs.lpCreateParams as isize);
                DefWindowProcW(hwnd, msg, wparam, lparam)
            }
        }
        WM_CREATE => {
            // (Fatia 4, FR-006a) Adota o tema do Windows na ABERTURA — só a
            // barra de título / frame; não repinta ao vivo (limitação conhecida
            // aceita: o diálogo pode ficar aberto por minutos editando a
            // denylist durante uma troca de tema).
            // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
            unsafe {
                crate::window_material::apply_dark_titlebar(
                    hwnd,
                    crate::system_appearance::SystemAppearance::read_dark(),
                );
            }
            if let Some(ctx) = ctx_ref(hwnd) {
                // SAFETY: dentro do próprio WM_CREATE de `hwnd`.
                unsafe { create_children(hwnd, ctx) };
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            let id = wparam.0 & 0xFFFF;
            if let Some(ctx) = ctx_ref(hwnd) {
                if id == ID_ADD {
                    // SAFETY: `ctx` válido (obtido acima); controles filhos já
                    // criados (só chegamos em WM_COMMAND depois de WM_CREATE).
                    unsafe { add_from_edit(ctx) };
                } else if id == ID_REMOVE {
                    // SAFETY: idem.
                    unsafe { remove_selected(ctx) };
                } else if id == ID_HEURISTIC_TOGGLE {
                    // SAFETY: idem.
                    unsafe { toggle_heuristic(ctx) };
                } else if id == ID_APPLY_RETENTION {
                    // SAFETY: idem — os EDIT de retenção já criados em WM_CREATE.
                    unsafe { apply_retention(ctx) };
                } else if id == ID_ENCRYPTION_TOGGLE {
                    // SAFETY: idem — o checkbox da proteção já criado.
                    unsafe { toggle_encryption(ctx) };
                } else if id == ID_STARTUP_TOGGLE {
                    // SAFETY: idem — o checkbox de inicialização já criado.
                    unsafe { toggle_startup(ctx) };
                } else if id == ID_HOTKEY_ALTER {
                    // SAFETY: idem — a seção "Atalho global" já criada.
                    unsafe { on_hotkey_alter_clicked(ctx) };
                }
            }
            LRESULT(0)
        }
        WM_KEYDOWN => {
            if wparam.0 as u16 == VK_ESCAPE.0 {
                // SAFETY: `hwnd` é o desta janela.
                unsafe {
                    let _ = DestroyWindow(hwnd);
                }
            }
            LRESULT(0)
        }
        WM_CLOSE => {
            // Não-modal, sem estado não salvo (cada mudança já persiste
            // imediatamente) — fechar é só destruir, mesmo padrão de
            // `hotkey_dialog.rs`.
            // SAFETY: `hwnd` é o desta janela.
            unsafe {
                let _ = DestroyWindow(hwnd);
            }
            LRESULT(0)
        }
        WM_NCDESTROY => {
            hotkey_capture::release(CaptureOwner::SettingsSection);
            // SAFETY: reconstrói e dropa o `Box` alocado em `open()` — nenhum
            // código lê `GWLP_USERDATA` de novo depois desta mensagem.
            let ptr = unsafe { GetWindowLongPtrW(hwnd, GWLP_USERDATA) } as *mut SettingsDialogCtx;
            if !ptr.is_null() {
                // SAFETY: `ptr` veio de `Box::into_raw` em `open()` e nunca
                // foi liberado antes.
                let ctx = unsafe { Box::from_raw(ptr) };
                if ctx.font_owned {
                    // SAFETY: `ctx.font` veio de `CreateFontIndirectW` em
                    // `ui_font()` (`font_owned == true`); nenhum controle a usa
                    // mais depois de a janela ser destruída.
                    unsafe {
                        let _ = DeleteObject(ctx.font.into());
                    }
                }
                drop(ctx);
            }
            // SAFETY: repassa ao handler padrão.
            unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
        }
        // SAFETY: repassa mensagens não tratadas ao handler padrão.
        _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
    }
}
