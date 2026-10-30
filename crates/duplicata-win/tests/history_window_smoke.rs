#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use duplicata_core::capture::{CanonicalKind, CanonicalSelection, CapturedFormat};
use duplicata_core::testsupport::unicode_text_format;
use duplicata_core::{
    ByteBudgetQueue, CaptureQueue, CaptureRecord, ClipListItem, FakeHistoryRepository,
    HistoryReader, HistoryRepository, IdentityKey, SelfWriteFilter, SetPinnedOutcome, StoreError,
    StoredClip, Timestamp, UpsertOutcome, WorkItem,
};
use duplicata_win::history_window::HistoryWindow;

#[test]
#[ignore = "precisa de sessão gráfica; o resto do roteiro é manual (ver comentários abaixo)"]
fn creating_the_window_against_a_fake_reader_succeeds() {
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela oculta deve funcionar numa sessão gráfica");
    println!("HistoryWindow criada (oculta) com hwnd={:?}", window.hwnd());
}

#[test]
#[ignore = "precisa de sessão gráfica; julgamento visual é manual"]
fn creating_the_window_applies_the_native_appearance_attributes() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, WM_SETTINGCHANGE, WM_THEMECHANGED,
    };

    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela deve funcionar numa sessão gráfica");
    let hwnd = window.hwnd();

    let color_set: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
    // SAFETY: `hwnd` é a janela recém-criada; `SendMessageW` síncrono nesta
    // thread. `WM_SETTINGCHANGE` carrega a string em `lParam`.
    unsafe {
        SendMessageW(
            hwnd,
            WM_SETTINGCHANGE,
            Some(WPARAM(0)),
            Some(LPARAM(color_set.as_ptr() as isize)),
        );
        SendMessageW(hwnd, WM_THEMECHANGED, Some(WPARAM(0)), Some(LPARAM(0)));
    }

    assert_eq!(
        window.hwnd(),
        hwnd,
        "a janela não foi recriada na troca de tema"
    );
    println!("atributos DWM aplicados; troca de tema tratada sem recriar hwnd={hwnd:?}");
}

struct CountingReader {
    calls: Arc<AtomicUsize>,
    last_limit: Arc<AtomicUsize>,
    rows: Vec<ClipListItem>,
}
impl HistoryReader for CountingReader {
    fn list_for_display(&self, limit: usize) -> Result<Vec<ClipListItem>, StoreError> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.last_limit.store(limit, Ordering::SeqCst);
        Ok(self.rows.clone())
    }
    fn get_full(&self, _clip_id: i64) -> Result<Option<StoredClip>, StoreError> {
        Ok(None)
    }
    fn count_pinned(&self) -> Result<u32, StoreError> {
        Ok(0)
    }
    fn close(self: Box<Self>) -> Result<(), StoreError> {
        Ok(())
    }
}

fn text_row(id: i64, preview: &str) -> ClipListItem {
    ClipListItem {
        id,
        canonical_kind: "unicode_text".into(),
        preview: Some(preview.into()),
        thumbnail: None,
        has_text: true,
        last_activity_ms: id as u64,
        pinned: false,
    }
}

#[test]
#[ignore = "precisa de sessão gráfica (WM_HOTKEY/WM_CHAR)"]
fn typing_the_filter_never_hits_the_reader_again() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CHAR, WM_HOTKEY};

    let calls = Arc::new(AtomicUsize::new(0));
    let reader = CountingReader {
        calls: Arc::clone(&calls),
        last_limit: Arc::new(AtomicUsize::new(0)),
        rows: vec![
            text_row(3, "link para o exemplo"),
            text_row(2, "outra nota"),
            text_row(1, "mais um texto"),
        ],
    };
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");
    let hwnd = window.hwnd();

    // Abrir a janela (HOTKEY_ID = 1): 1 chamada a list_for_display.
    // SAFETY: `hwnd` recém-criada nesta thread.
    unsafe {
        SendMessageW(hwnd, WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
    }
    let after_open = calls.load(Ordering::SeqCst);
    assert_eq!(after_open, 1, "a abertura carrega a lista uma vez");

    for ch in "link".chars() {
        // SAFETY: idem.
        unsafe {
            SendMessageW(hwnd, WM_CHAR, Some(WPARAM(ch as usize)), Some(LPARAM(0)));
        }
    }
    // Backspace + mais um char.
    // SAFETY: `hwnd` desta thread.
    unsafe {
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0x08)), Some(LPARAM(0)));
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM('k' as usize)), Some(LPARAM(0)));
    }

    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "digitar no filtro NÃO pode disparar nova consulta ao banco"
    );
    println!("6 WM_CHAR processados, list_for_display chamado 1× no total");
}

#[test]
#[ignore = "precisa de sessão gráfica (WM_HOTKEY)"]
fn the_load_limit_includes_the_pin_cap() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_HOTKEY};

    let last_limit = Arc::new(AtomicUsize::new(0));
    let reader = CountingReader {
        calls: Arc::new(AtomicUsize::new(0)),
        last_limit: Arc::clone(&last_limit),
        rows: vec![text_row(1, "x")],
    };
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");

    let hotkey = |w: &HistoryWindow| {
        // SAFETY: `hwnd` desta thread; simula o atalho global (HOTKEY_ID = 1).
        unsafe {
            SendMessageW(w.hwnd(), WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
        }
    };
    let past_debounce = || std::thread::sleep(std::time::Duration::from_millis(260));

    hotkey(&window);
    assert_eq!(last_limit.load(Ordering::SeqCst), 500 + 25);

    window.set_max_pinned(200);
    past_debounce();
    hotkey(&window);
    past_debounce();
    hotkey(&window);

    assert_eq!(
        last_limit.load(Ordering::SeqCst),
        700,
        "o limit passa a MAX_ROWS + max_pinned = 500 + 200"
    );
    println!("limit: 525 com o default, 700 após set_max_pinned(200)");
}

#[test]
#[ignore = "precisa de sessão gráfica (WM_HOTKEY)"]
fn the_win_v_hotkey_opens_the_window_like_the_reserve() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{IsWindowVisible, SendMessageW, WM_HOTKEY};

    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");

    // SAFETY: `hwnd` desta thread; simula o Win+V (HOTKEY_ID_WIN_V = 2).
    unsafe {
        SendMessageW(
            window.hwnd(),
            WM_HOTKEY,
            Some(WPARAM(duplicata_win::hotkey_win::HOTKEY_ID_WIN_V as usize)),
            Some(LPARAM(0)),
        );
    }
    // SAFETY: só consulta a visibilidade da janela viva.
    assert!(unsafe { IsWindowVisible(window.hwnd()) }.as_bool());
}

#[test]
#[ignore = "mexe no cursor do sistema; precisa de sessão gráfica"]
fn the_window_opens_near_the_cursor_inside_the_work_area() {
    use windows::Win32::Foundation::{LPARAM, POINT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{
        GetMonitorInfoW, MonitorFromPoint, MONITORINFO, MONITOR_DEFAULTTONEAREST,
    };
    use windows::Win32::UI::WindowsAndMessaging::{
        GetCursorPos, GetWindowRect, SendMessageW, SetCursorPos, WM_HOTKEY,
    };

    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");
    let hwnd = window.hwnd();

    let mut original = POINT::default();
    // SAFETY: só escreve `original`.
    unsafe {
        let _ = GetCursorPos(&mut original);
    }
    // SAFETY: coordenada válida; o monitor primário sempre existe em (0,0).
    let work = unsafe {
        let hmon = MonitorFromPoint(POINT { x: 0, y: 0 }, MONITOR_DEFAULTTONEAREST);
        let mut mi = MONITORINFO {
            cbSize: core::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        assert!(GetMonitorInfoW(hmon, &mut mi).as_bool());
        mi.rcWork
    };
    let cursor = POINT {
        x: work.right - 5,
        y: work.bottom - 5,
    };
    // SAFETY: coordenada dentro da tela.
    unsafe {
        let _ = SetCursorPos(cursor.x, cursor.y);
        SendMessageW(hwnd, WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    let mut wr = RECT::default();
    // SAFETY: `hwnd` válido.
    unsafe {
        let _ = GetWindowRect(hwnd, &mut wr);
    }
    // Área útil do monitor SOB A JANELA (pode ter mudado por WM_DPICHANGED).
    // SAFETY: `wr.left`/`wr.top` são coordenadas válidas.
    let win_work = unsafe {
        let hmon = MonitorFromPoint(
            POINT {
                x: wr.left,
                y: wr.top,
            },
            MONITOR_DEFAULTTONEAREST,
        );
        let mut mi = MONITORINFO {
            cbSize: core::mem::size_of::<MONITORINFO>() as u32,
            ..Default::default()
        };
        assert!(GetMonitorInfoW(hmon, &mut mi).as_bool());
        mi.rcWork
    };

    // restaura o cursor
    // SAFETY: coordenada válida.
    unsafe {
        let _ = SetCursorPos(original.x, original.y);
    }

    assert!(
        wr.left >= win_work.left
            && wr.top >= win_work.top
            && wr.right <= win_work.right
            && wr.bottom <= win_work.bottom,
        "a janela deve caber inteira em rcWork: janela={wr:?} rcWork={win_work:?}"
    );
    println!("janela em {wr:?}, dentro de rcWork {win_work:?} — cursor estava em {cursor:?}");
}

fn png(w: u32, h: u32) -> Vec<u8> {
    let img = image::RgbaImage::from_pixel(w, h, image::Rgba([80, 140, 220, 255]));
    let mut bytes = Vec::new();
    image::DynamicImage::ImageRgba8(img)
        .write_to(
            &mut std::io::Cursor::new(&mut bytes),
            image::ImageFormat::Png,
        )
        .expect("codificar o PNG de teste");
    bytes
}

fn image_row(id: i64, w: u32, h: u32, pinned: bool) -> ClipListItem {
    ClipListItem {
        id,
        canonical_kind: "dib".into(),
        preview: None,
        thumbnail: Some(png(w, h)),
        has_text: false,
        last_activity_ms: id as u64,
        pinned,
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; julgamento visual é manual"]
fn rows_show_a_proportional_thumbnail_and_paint_without_panicking_with_a_pinned_row() {
    use duplicata_core::row_layout;
    use duplicata_win::thumbnail_gdi::ThumbnailCache;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_HOTKEY};

    const ROW_HEIGHT_PX: i32 = 28;

    let rows = vec![
        image_row(5, 128, 72, true),
        image_row(4, 72, 128, false),
        image_row(3, 128, 8, false),
        text_row(2, "uma nota de texto qualquer"),
        ClipListItem {
            id: 1,
            canonical_kind: "hdrop".into(),
            preview: Some("relatorio.pdf\nfoto.png".into()),
            thumbnail: None,
            has_text: true,
            last_activity_ms: 1,
            pinned: false,
        },
    ];
    let reader = CountingReader {
        calls: Arc::new(AtomicUsize::new(0)),
        last_limit: Arc::new(AtomicUsize::new(0)),
        rows: rows.clone(),
    };
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");

    // Abre e pinta de verdade — se a pintura panicasse (decode, StretchBlt,
    // HALFTONE, glifo do alfinete na linha fixada e na selecionada), seria
    // aqui. `UpdateWindow` força o `WM_PAINT` agora, sem laço de mensagens.
    // SAFETY: `hwnd` recém-criada nesta thread.
    unsafe {
        SendMessageW(window.hwnd(), WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
        let _ = windows::Win32::Graphics::Gdi::UpdateWindow(window.hwnd());
    }

    let mut cache = ThumbnailCache::new();
    let margin = row_layout::THUMBNAIL_MARGIN_PX;
    let usable_h = ROW_HEIGHT_PX - 2 * margin;
    for item in rows.iter().filter(|i| i.thumbnail.is_some()) {
        let bytes = item.thumbnail.as_ref().unwrap();
        let (_, bw, bh) = cache
            .get_or_decode(item.id, bytes)
            .expect("o PNG de teste tem de decodificar");
        let (w, h) = row_layout::thumbnail_target(ROW_HEIGHT_PX, 96, bw, bh);
        assert!(w > 0 && h > 0, "id={} não gerou alvo", item.id);
        assert!(h <= usable_h, "id={}: {h}px passa da altura útil", item.id);
        let filled = h == usable_h;
        let clamped = w == row_layout::THUMBNAIL_MAX_WIDTH_PX;
        assert!(
            filled || clamped,
            "id={}: {w}x{h} não preenche a altura nem bate no teto de largura",
            item.id
        );
        let ratio_src = bw as f64 / bh as f64;
        let ratio_dst = w as f64 / h as f64;
        assert!(
            (ratio_src - ratio_dst).abs() / ratio_src <= 0.10,
            "id={}: proporção {ratio_dst:.3} contra {ratio_src:.3} da origem",
            item.id
        );
    }

    println!(
        "5 linhas pintadas (paisagem fixada, retrato, panorâmica, texto, arquivos); \
         miniaturas proporcionais à altura útil de {usable_h}px"
    );
}

struct RecordingRepo {
    inner: FakeHistoryRepository,
    pins: Arc<Mutex<Vec<(i64, bool, SetPinnedOutcome)>>>,
}
impl HistoryRepository for RecordingRepo {
    fn upsert(&mut self, record: &CaptureRecord) -> Result<UpsertOutcome, StoreError> {
        self.inner.upsert(record)
    }
    fn purge_older_than(&mut self, cutoff_ms: u64) -> Result<u64, StoreError> {
        self.inner.purge_older_than(cutoff_ms)
    }
    fn purge_over_count(&mut self, max_items: u32) -> Result<u64, StoreError> {
        self.inner.purge_over_count(max_items)
    }
    fn set_pinned(
        &mut self,
        clip_id: i64,
        pinned: bool,
        max_pinned: u32,
    ) -> Result<SetPinnedOutcome, StoreError> {
        let out = self.inner.set_pinned(clip_id, pinned, max_pinned);
        if let Ok(o) = &out {
            self.pins.lock().unwrap().push((clip_id, pinned, *o));
        }
        out
    }
    fn delete_all(&mut self, keep_pinned: bool) -> Result<u64, StoreError> {
        self.inner.delete_all(keep_pinned)
    }
    fn delete_one(&mut self, clip_id: i64) -> Result<bool, StoreError> {
        self.inner.delete_one(clip_id)
    }
    fn recreate(&mut self) -> Result<(), StoreError> {
        self.inner.recreate()
    }
    fn toggle_encryption(
        &mut self,
        enable: bool,
        progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        self.inner.toggle_encryption(enable, progress)
    }
}

fn seed_record(tag: u8) -> CaptureRecord {
    let fmt = unicode_text_format(&format!("semente {tag}"));
    CaptureRecord {
        identity: IdentityKey([tag; 32]),
        canonical: CanonicalSelection {
            format_id: fmt.format_id,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: fmt.bytes.len() as u64,
        },
        captured_at: Timestamp::from_millis(tag as u64),
        total_bytes: fmt.bytes.len() as u64,
        preview: Some(format!("semente {tag}")),
        thumbnail: None,
        has_text: true,
        formats: vec![fmt],
    }
}

struct HeldCtrl([u8; 256]);
impl HeldCtrl {
    fn press() -> Self {
        use windows::Win32::UI::Input::KeyboardAndMouse::{
            GetKeyboardState, SetKeyboardState, VK_CONTROL,
        };
        let mut saved = [0u8; 256];
        // SAFETY: `saved` tem os 256 bytes que a API exige.
        unsafe {
            let _ = GetKeyboardState(&mut saved);
        }
        let mut held = saved;
        held[VK_CONTROL.0 as usize] = 0x80; // bit alto = pressionada
                                            // SAFETY: idem; afeta só a tabela desta thread.
        unsafe {
            let _ = SetKeyboardState(&held);
        }
        HeldCtrl(saved)
    }
}
impl Drop for HeldCtrl {
    fn drop(&mut self) {
        use windows::Win32::UI::Input::KeyboardAndMouse::SetKeyboardState;
        // SAFETY: devolve exatamente a tabela lida em `press`.
        unsafe {
            let _ = SetKeyboardState(&self.0);
        }
    }
}

#[test]
#[ignore = "precisa de sessão gráfica; mexe na tabela de teclado da thread"]
fn ctrl_p_pins_through_the_same_path_as_the_context_menu() {
    use duplicata_core::{run_worker, Config, WorkerCounters};
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_P;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CHAR, WM_HOTKEY, WM_KEYDOWN};

    let pins = Arc::new(Mutex::new(Vec::new()));
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());

    let worker_queue = Arc::clone(&queue);
    let worker_pins = Arc::clone(&pins);
    let worker = thread::spawn(move || {
        let mut inner = FakeHistoryRepository::new();
        for i in 1..=3u8 {
            inner
                .upsert(&seed_record(i))
                .expect("semear o repositório do worker");
        }
        let mut repo = RecordingRepo {
            inner,
            pins: worker_pins,
        };
        let counters = WorkerCounters::default();
        let (heuristic_tx, _heuristic_rx) = std::sync::mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &Config::with_paths("db".into(), "logs".into()),
            &counters,
            &heuristic_tx,
        );
    });

    let reader = CountingReader {
        calls: Arc::new(AtomicUsize::new(0)),
        last_limit: Arc::new(AtomicUsize::new(0)),
        rows: vec![
            text_row(3, "o item mais recente"),
            text_row(2, "um do meio"),
            text_row(1, "o mais antigo"),
        ],
    };
    let window = HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        Arc::clone(&queue),
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");
    let hwnd = window.hwnd();

    // Abre: a seleção nasce no índice 0 = clip_id 3 (o mais recente).
    // SAFETY: `hwnd` recém-criada nesta thread.
    unsafe {
        SendMessageW(hwnd, WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    // (a) Sem Ctrl, `P` é só uma letra — vai para o filtro, não fixa nada.
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM('p' as usize)), Some(LPARAM(0)));
    }
    assert!(
        pins.lock().unwrap().is_empty(),
        "digitar `p` não pode fixar nada"
    );

    // (b) O caractere de controle que `Ctrl+P` gera (0x10) também não pode
    // fixar: o atalho chega por `WM_KEYDOWN`, nunca como caractere — é isso
    // que impede a colisão com a busca incremental.
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0x10)), Some(LPARAM(0)));
    }
    assert!(
        pins.lock().unwrap().is_empty(),
        "`Ctrl+P` não pode chegar como WM_CHAR"
    );

    // Limpa o filtro para a seleção voltar ao item 3.
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0x08)), Some(LPARAM(0)));
    }

    {
        let _ctrl = HeldCtrl::press();
        // SAFETY: idem.
        unsafe {
            SendMessageW(
                hwnd,
                WM_KEYDOWN,
                Some(WPARAM(VK_P.0 as usize)),
                Some(LPARAM(0)),
            );
        }
    }
    let recorded = pins.lock().unwrap().clone();
    assert_eq!(
        recorded,
        vec![(3, true, SetPinnedOutcome::Applied)],
        "Ctrl+P tem de enfileirar SetPinned para o item selecionado (clip 3), \
         fixando-o. `NotFound` aqui significa que o repositório do \
         worker não tem a linha — defeito do teste, não do atalho"
    );

    {
        let _ctrl = HeldCtrl::press();
        // SAFETY: idem.
        unsafe {
            SendMessageW(
                hwnd,
                WM_KEYDOWN,
                Some(WPARAM(VK_P.0 as usize)),
                Some(LPARAM(0)),
            );
        }
    }
    assert_eq!(
        pins.lock().unwrap().clone(),
        vec![
            (3, true, SetPinnedOutcome::Applied),
            (3, false, SetPinnedOutcome::Applied)
        ],
        "o segundo Ctrl+P tem de desafixar — `toggle_pin_selected` inverte o \
         `pinned` que a linha tem AGORA em `ctx.rows`, que é a mesma fonte de \
         onde o menu de contexto tira o rótulo Fixar/Desafixar"
    );

    {
        use duplicata_core::hint_strip::HINT_STRIP_PX;
        use duplicata_core::row_layout::{rows_that_fit, MIN_VISIBLE_ROWS, ROW_HEIGHT_PX};
        use windows::Win32::Foundation::RECT;
        use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

        let mut client = RECT::default();
        // SAFETY: `hwnd` válido.
        unsafe {
            let _ = GetClientRect(hwnd, &mut client);
        }
        let chrome = 34 + 30 + HINT_STRIP_PX;
        let fits = rows_that_fit(client.bottom - client.top, chrome, ROW_HEIGHT_PX);
        assert!(
            fits >= MIN_VISIBLE_ROWS,
            "cliente de {}px comporta só {fits} linhas com o chrome de {chrome}px \
             — abaixo do piso de {MIN_VISIBLE_ROWS}",
            client.bottom - client.top
        );
        println!(
            "cliente {}px → {fits} linhas de lista",
            client.bottom - client.top
        );
    }

    let line = duplicata_core::hint_strip::hint_line();
    assert!(line.contains("Tab+←→ tipo"), "rodapé: {line:?}");
    assert!(line.contains("Ctrl+P fixar"), "rodapé: {line:?}");
    println!("rodapé pintado: {line}");

    queue.close();
    worker.join().expect("a worker termina no close da fila");
}

#[test]
#[ignore = "precisa de sessão gráfica; mexe na tabela de teclado da thread"]
fn mouse_wheel_scrolls_the_list_without_moving_the_selection() {
    use duplicata_core::hint_strip::HINT_STRIP_PX;
    use duplicata_core::row_layout::{rows_that_fit, ROW_HEIGHT_PX};
    use duplicata_core::{run_worker, Config, WorkerCounters};
    use windows::Win32::Foundation::{LPARAM, RECT, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_P;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetClientRect, SendMessageW, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDOWN, WM_MOUSEWHEEL,
    };

    const TOTAL: i64 = 30;
    const LIST_TOP_PX: i32 = 34 + 30; // FILTER_STRIP_PX + SEARCH_BAR_PX (privados em history_window.rs)

    let pins = Arc::new(Mutex::new(Vec::new()));
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());

    let worker_queue = Arc::clone(&queue);
    let worker_pins = Arc::clone(&pins);
    let worker = thread::spawn(move || {
        let mut inner = FakeHistoryRepository::new();
        for tag in 1..=TOTAL as u8 {
            inner
                .upsert(&seed_record(tag))
                .expect("semear o repositório do worker");
        }
        let mut repo = RecordingRepo {
            inner,
            pins: worker_pins,
        };
        let counters = WorkerCounters::default();
        let (heuristic_tx, _heuristic_rx) = std::sync::mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &Config::with_paths("db".into(), "logs".into()),
            &counters,
            &heuristic_tx,
        );
    });

    // rows[0] = clip_id TOTAL (o mais recente), decrescendo — mesma convenção
    // de "o item mais recente" no teste de Ctrl+P acima.
    let rows: Vec<ClipListItem> = (1..=TOTAL)
        .rev()
        .map(|id| text_row(id, &format!("item {id}")))
        .collect();
    let reader = CountingReader {
        calls: Arc::new(AtomicUsize::new(0)),
        last_limit: Arc::new(AtomicUsize::new(0)),
        rows,
    };
    let window = HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        Arc::clone(&queue),
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");
    let hwnd = window.hwnd();

    // SAFETY: `hwnd` recém-criada nesta thread.
    unsafe {
        SendMessageW(hwnd, WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
    }

    // A seleção nasce no índice 0 = clip_id TOTAL (o mais recente). Fixa para
    // ter um jeito observável de perguntar "quem está selecionado agora".
    {
        let _ctrl = HeldCtrl::press();
        // SAFETY: idem.
        unsafe {
            SendMessageW(
                hwnd,
                WM_KEYDOWN,
                Some(WPARAM(VK_P.0 as usize)),
                Some(LPARAM(0)),
            );
        }
    }
    assert_eq!(
        pins.lock().unwrap().clone(),
        vec![(TOTAL, true, SetPinnedOutcome::Applied)],
        "antes de rolar, Ctrl+P tem de fixar o item selecionado por padrão \
         (o mais recente)"
    );

    let mut client = RECT::default();
    // SAFETY: `hwnd` válido.
    unsafe {
        let _ = GetClientRect(hwnd, &mut client);
    }
    let chrome = LIST_TOP_PX + HINT_STRIP_PX;
    let fits = rows_that_fit(client.bottom - client.top, chrome, ROW_HEIGHT_PX) as i64;
    assert!(
        fits + 6 < TOTAL,
        "o teste precisa de mais itens do que cabem na tela para rolar de fato \
         (cabem {fits}, tem {TOTAL})"
    );

    // Duas "notches" para trás (para o usuário), sinal negativo — WHEEL_DELTA
    // (120) por notch, WHEEL_LINES_PER_NOTCH (3) linhas por notch em
    // history_window.rs: 2 * 3 = 6 linhas para baixo.
    let wheel_wparam = WPARAM((((-240i16) as u16 as u32) << 16) as usize);
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_MOUSEWHEEL, Some(wheel_wparam), Some(LPARAM(0)));
    }

    // A rolagem sozinha não pode mexer em `ctx.selected`: Ctrl+P ainda tem de
    // mirar no clip TOTAL, agora desfixando (a segunda fixação alterna).
    {
        let _ctrl = HeldCtrl::press();
        // SAFETY: idem.
        unsafe {
            SendMessageW(
                hwnd,
                WM_KEYDOWN,
                Some(WPARAM(VK_P.0 as usize)),
                Some(LPARAM(0)),
            );
        }
    }
    assert_eq!(
        pins.lock().unwrap().clone(),
        vec![
            (TOTAL, true, SetPinnedOutcome::Applied),
            (TOTAL, false, SetPinnedOutcome::Applied)
        ],
        "WM_MOUSEWHEEL rolou a lista, mas não pode ter tocado em `ctx.selected` \
         — Ctrl+P continua mirando o clip {TOTAL}"
    );

    // O Ctrl+P acima chama `select_clip`, que recentraliza a rolagem no item
    // ainda selecionado (índice 0) — de propósito, é o que mantém o item
    // recém-fixado visível. Isso zera `scroll_offset` outra vez, então rola
    // mais uma vez antes de testar o clique.
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_MOUSEWHEEL, Some(wheel_wparam), Some(LPARAM(0)));
    }

    // Clicar no topo da área de lista agora tem de acertar o item que ficou
    // visível ali depois da rolagem (offset 6 → clip TOTAL - 6), provando que
    // `ctx.scroll_offset` avançou o número certo de linhas.
    let click_lparam = LPARAM((((LIST_TOP_PX as u32) << 16) | 80) as isize);
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_LBUTTONDOWN, Some(WPARAM(0)), Some(click_lparam));
    }
    {
        let _ctrl = HeldCtrl::press();
        // SAFETY: idem.
        unsafe {
            SendMessageW(
                hwnd,
                WM_KEYDOWN,
                Some(WPARAM(VK_P.0 as usize)),
                Some(LPARAM(0)),
            );
        }
    }
    assert_eq!(
        pins.lock().unwrap().last().copied(),
        Some((TOTAL - 6, true, SetPinnedOutcome::Applied)),
        "depois de rolar 2 notches (6 linhas) e clicar no topo da lista, o item \
         fixado tem de ser o clip {}, confirmando o deslocamento de scroll_offset",
        TOTAL - 6
    );

    queue.close();
    worker.join().expect("a worker termina no close da fila");
}

#[test]
#[ignore = "precisa de sessão gráfica (mede a fonte de interface real)"]
fn the_hint_strip_fits_without_wasting_width() {
    use duplicata_core::filter_strip::SEGMENT_PADDING_PX;
    use duplicata_core::hint_strip::{
        hint_line, HINT_LINE_SLACK_PX, MAX_HINT_SLACK_PX, MEASURED_HINT_LINE_PX, SIDE_PADDING_PX,
    };
    use duplicata_core::list_view::ContentTypeFilter;
    use duplicata_core::row_layout::{WINDOW_FRAME_WIDTH_PX, WINDOW_WIDTH_PX};
    use duplicata_win::text_metrics::width_with_font;
    use windows::Win32::Foundation::RECT;
    use windows::Win32::Graphics::Gdi::{CreateFontIndirectW, DeleteObject, HGDIOBJ};
    use windows::Win32::UI::WindowsAndMessaging::GetClientRect;

    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");

    let appearance = duplicata_win::system_appearance::SystemAppearance::system_defaults();
    // SAFETY: `message_font` veio de `SPI_GETNONCLIENTMETRICS`.
    let font = unsafe { CreateFontIndirectW(&appearance.message_font) };

    let line = hint_line();
    let measured = width_with_font(font, &line);
    assert!(measured > 0, "GetTextExtentPoint32W não mediu a linha");

    let mut client = RECT::default();
    // SAFETY: `hwnd` válido.
    unsafe {
        let _ = GetClientRect(window.hwnd(), &mut client);
    }
    let client_w = client.right - client.left;
    let text_area = client_w - SIDE_PADDING_PX;
    let slack = text_area - measured;
    let nonclient = WINDOW_WIDTH_PX - client_w;
    let minimum = measured + SIDE_PADDING_PX + HINT_LINE_SLACK_PX + nonclient;

    println!("--- largura do rodapé (fonte de interface real) ---");
    println!("  linha             : {line}");
    println!("  medida            : {measured}px  (constante: {MEASURED_HINT_LINE_PX}px)");
    println!("  janela            : {WINDOW_WIDTH_PX}px  (moldura medida: {nonclient}px)");
    println!("  cliente           : {client_w}px  → área de texto {text_area}px");
    println!(
        "  folga             : {slack}px  (mínimo {HINT_LINE_SLACK_PX}, teto {MAX_HINT_SLACK_PX})"
    );
    println!(
        "  WINDOW_WIDTH_PX mínimo: {minimum}px  (atual {WINDOW_WIDTH_PX}px — \
         estar ACIMA do mínimo é o esperado, não uma divergência)"
    );
    assert_eq!(
        nonclient, WINDOW_FRAME_WIDTH_PX,
        "a moldura real ({nonclient}px) não bate com WINDOW_FRAME_WIDTH_PX \
         ({WINDOW_FRAME_WIDTH_PX}px) — a conta do gate do core está calibrada \
         para outro valor e vai divergir deste"
    );

    print!("  rótulos da faixa  :");
    for seg in ContentTypeFilter::SEGMENTS {
        print!(
            " {}={}px(+{}) ",
            seg.label(),
            width_with_font(font, seg.label()),
            2 * SEGMENT_PADDING_PX
        );
    }
    println!();

    // SAFETY: `font` foi criada aqui e não está selecionada em nenhum DC.
    unsafe {
        let _ = DeleteObject(HGDIOBJ(font.0));
    }

    assert!(
        measured <= text_area,
        "o rodapé mede {measured}px e a área de texto tem {text_area}px — sairia \
         cortado por `DT_END_ELLIPSIS`, escondendo `Tab+←→ tipo · Esc fechar` \
         Suba WINDOW_WIDTH_PX para pelo menos {minimum}."
    );
    assert!(
        slack >= HINT_LINE_SLACK_PX,
        "só {slack}px de folga depois do rodapé (mínimo {HINT_LINE_SLACK_PX}px) \
         — uma fonte de interface maior (escala de texto do Windows, outro \
         idioma) cortaria o fim da linha. Suba WINDOW_WIDTH_PX para pelo menos \
         {minimum}."
    );
    assert!(
        slack <= MAX_HINT_SLACK_PX,
        "sobram {slack}px de faixa vazia à direita do rodapé (teto \
         {MAX_HINT_SLACK_PX}px) — a janela está mais larga que o conteúdo pede. \
         Baixe WINDOW_WIDTH_PX para perto de {minimum}."
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; mede tempo de abertura"]
fn opening_with_a_full_history_accepts_the_first_keystroke_within_100ms() {
    use std::time::Instant;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_CHAR, WM_HOTKEY};

    let rows: Vec<ClipListItem> = (0..500)
        .map(|i| {
            let id = 500 - i;
            let mut r = text_row(
                id,
                &format!("item numero {id} com um texto de tamanho realista para a lista"),
            );
            r.pinned = id % 50 == 0;
            r
        })
        .collect();

    let calls = Arc::new(AtomicUsize::new(0));
    let reader = CountingReader {
        calls: Arc::clone(&calls),
        last_limit: Arc::new(AtomicUsize::new(0)),
        rows,
    };
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela");
    let hwnd = window.hwnd();

    let t0 = Instant::now();
    // SAFETY: `hwnd` recém-criada nesta thread.
    unsafe {
        SendMessageW(hwnd, WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM('i' as usize)), Some(LPARAM(0)));
    }
    let elapsed = t0.elapsed();

    println!("abertura com 500 itens até aceitar a 1ª tecla: {elapsed:?}");
    assert_eq!(
        calls.load(Ordering::SeqCst),
        1,
        "a lista é carregada UMA vez"
    );
    assert!(
        elapsed.as_millis() <= 100,
        "levou {elapsed:?} do atalho até aceitar a primeira tecla — o teto é \
         100ms"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica; o julgamento visual do X é manual (issue #3)"]
fn the_window_uses_the_standard_caption_and_stays_off_the_taskbar() {
    use windows::Win32::UI::WindowsAndMessaging::{
        GetWindow, GetWindowLongPtrW, IsWindowVisible, GWL_EXSTYLE, GWL_STYLE, GW_OWNER,
        WS_EX_APPWINDOW, WS_EX_TOOLWINDOW, WS_MAXIMIZEBOX, WS_MINIMIZEBOX,
    };

    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let window = HistoryWindow::new(
        FakeHistoryRepository::new(),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela deve funcionar numa sessão gráfica");
    let hwnd = window.hwnd();

    // SAFETY: `hwnd` é a janela recém-criada, viva até o fim do teste.
    let (style, ex_style, owner) = unsafe {
        (
            GetWindowLongPtrW(hwnd, GWL_STYLE) as u32,
            GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32,
            GetWindow(hwnd, GW_OWNER),
        )
    };
    assert_eq!(
        ex_style & WS_EX_TOOLWINDOW.0,
        0,
        "WS_EX_TOOLWINDOW faz o DWM desenhar o X vermelho de ferramenta"
    );
    assert_eq!(
        ex_style & WS_EX_APPWINDOW.0,
        0,
        "WS_EX_APPWINDOW poria a janela na barra de tarefas mesmo com dono"
    );
    assert!(
        owner.as_ref().is_ok_and(|o| !o.is_invalid()),
        "sem dono a janela ganha botão na barra de tarefas"
    );
    let owner = owner.expect("dono checado acima");
    // SAFETY: `owner` é o dono vivo da janela recém-criada.
    let (owner_visible, owner_ex_style) = unsafe {
        (
            IsWindowVisible(owner).as_bool(),
            GetWindowLongPtrW(owner, GWL_EXSTYLE) as u32,
        )
    };
    assert!(
        !owner_visible,
        "dono visível ganharia o botão na barra de tarefas"
    );
    assert_ne!(
        owner_ex_style & WS_EX_TOOLWINDOW.0,
        0,
        "sem WS_EX_TOOLWINDOW o dono entra na barra de tarefas se for exibido"
    );
    assert_eq!(
        style & (WS_MINIMIZEBOX.0 | WS_MAXIMIZEBOX.0),
        0,
        "o título mostra só o X, sem minimizar/maximizar"
    );
}

// --- Lixeira na linha (tarefa 01) ---------------------------------------
//
// O repositório é um só, dividido entre o worker (que apaga) e a janela (que
// relê a lista): o que a tela mostra depois do clique é o que está gravado.

mod trash {
    use super::*;
    use std::cell::RefCell;
    use std::thread::JoinHandle;

    use duplicata_core::list_view::ListDisplayState;
    use duplicata_core::row_layout::ROW_HEIGHT_PX;
    use duplicata_core::{run_worker, Config, WorkerCounters};
    use std::sync::atomic::AtomicBool;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, RECT, WPARAM};
    use windows::Win32::Graphics::Gdi::{GetUpdateRect, UpdateWindow};
    use windows::Win32::System::Threading::GetCurrentThreadId;
    use windows::Win32::UI::WindowsAndMessaging::{
        CallNextHookEx, GetClassNameW, GetClientRect, GetWindowTextW, IsWindowVisible,
        PostMessageW, SendMessageW, SetWindowsHookExW, UnhookWindowsHookEx, HCBT_ACTIVATE, HHOOK,
        WH_CBT, WM_CHAR, WM_CLOSE, WM_HOTKEY, WM_KEYDOWN, WM_LBUTTONDBLCLK, WM_LBUTTONDOWN,
        WM_MOUSEMOVE,
    };

    const LIST_TOP_PX: i32 = 34 + 30; // FILTER_STRIP_PX + SEARCH_BAR_PX (privados)
    const WM_MOUSEHOVER: u32 = 0x02A1;
    const WM_MOUSELEAVE: u32 = 0x02A3;

    #[derive(Clone)]
    struct SharedRepo {
        inner: Arc<Mutex<FakeHistoryRepository>>,
        fail_delete: bool,
        /// Liga a falha na leitura da lista (faixa de erro na próxima recarga).
        fail_list: Arc<AtomicBool>,
        get_full_calls: Arc<AtomicUsize>,
        /// O último `clip_id` que a janela tentou colar.
        last_get_full: Arc<Mutex<Option<i64>>>,
    }

    impl HistoryRepository for SharedRepo {
        fn upsert(&mut self, record: &CaptureRecord) -> Result<UpsertOutcome, StoreError> {
            self.inner.lock().unwrap().upsert(record)
        }
        fn purge_older_than(&mut self, cutoff_ms: u64) -> Result<u64, StoreError> {
            self.inner.lock().unwrap().purge_older_than(cutoff_ms)
        }
        fn purge_over_count(&mut self, max_items: u32) -> Result<u64, StoreError> {
            self.inner.lock().unwrap().purge_over_count(max_items)
        }
        fn set_pinned(
            &mut self,
            clip_id: i64,
            pinned: bool,
            max_pinned: u32,
        ) -> Result<SetPinnedOutcome, StoreError> {
            self.inner
                .lock()
                .unwrap()
                .set_pinned(clip_id, pinned, max_pinned)
        }
        fn delete_all(&mut self, keep_pinned: bool) -> Result<u64, StoreError> {
            self.inner.lock().unwrap().delete_all(keep_pinned)
        }
        fn delete_one(&mut self, clip_id: i64) -> Result<bool, StoreError> {
            if self.fail_delete {
                return Err(StoreError::Io);
            }
            self.inner.lock().unwrap().delete_one(clip_id)
        }
        fn recreate(&mut self) -> Result<(), StoreError> {
            self.inner.lock().unwrap().recreate()
        }
        fn toggle_encryption(
            &mut self,
            enable: bool,
            progress: &dyn Fn(u64),
        ) -> Result<(), StoreError> {
            self.inner
                .lock()
                .unwrap()
                .toggle_encryption(enable, progress)
        }
    }

    impl HistoryReader for SharedRepo {
        fn list_for_display(&self, limit: usize) -> Result<Vec<ClipListItem>, StoreError> {
            if self.fail_list.load(Ordering::SeqCst) {
                return Err(StoreError::Io);
            }
            self.inner.lock().unwrap().list_for_display(limit)
        }
        fn get_full(&self, clip_id: i64) -> Result<Option<StoredClip>, StoreError> {
            // Contado e nunca devolvido: colar de verdade mexeria na área de
            // transferência da máquina. Uma chamada aqui é a tentativa de colar.
            self.get_full_calls.fetch_add(1, Ordering::SeqCst);
            *self.last_get_full.lock().unwrap() = Some(clip_id);
            Ok(None)
        }
        fn count_pinned(&self) -> Result<u32, StoreError> {
            self.inner.lock().unwrap().count_pinned()
        }
        fn close(self: Box<Self>) -> Result<(), StoreError> {
            Ok(())
        }
    }

    fn record(tag: u8, preview: &str) -> CaptureRecord {
        let fmt = unicode_text_format(preview);
        CaptureRecord {
            identity: IdentityKey([tag; 32]),
            canonical: CanonicalSelection {
                format_id: fmt.format_id,
                format_name: None,
                kind: CanonicalKind::UnicodeText,
                byte_len: fmt.bytes.len() as u64,
            },
            captured_at: Timestamp::from_millis(tag as u64),
            total_bytes: fmt.bytes.len() as u64,
            preview: Some(preview.to_string()),
            thumbnail: None,
            has_text: true,
            formats: vec![fmt],
        }
    }

    #[derive(Default)]
    struct Setup<'a> {
        pinned: &'a [&'a str],
        /// Prévias semeadas como imagem (`dib`) em vez de texto.
        images: &'a [&'a str],
        fail_delete: bool,
        /// Teto de fixados do worker; `None` deixa o padrão da configuração.
        max_pinned: Option<u32>,
    }

    /// Um item de imagem: a prévia só serve para o teste achar o item.
    fn image_record(tag: u8, preview: &str) -> CaptureRecord {
        let mut rec = record(tag, preview);
        let dib = CapturedFormat {
            format_id: 8, // CF_DIB
            format_name: None,
            bytes: vec![tag; 40],
        };
        rec.canonical = CanonicalSelection {
            format_id: dib.format_id,
            format_name: None,
            kind: CanonicalKind::Dib,
            byte_len: dib.bytes.len() as u64,
        };
        rec.total_bytes = dib.bytes.len() as u64;
        rec.has_text = false;
        rec.formats = vec![dib];
        rec
    }

    struct Fixture {
        window: HistoryWindow,
        queue: Arc<dyn CaptureQueue<WorkItem>>,
        worker: Option<JoinHandle<()>>,
        repo: Arc<Mutex<FakeHistoryRepository>>,
        get_full_calls: Arc<AtomicUsize>,
        last_get_full: Arc<Mutex<Option<i64>>>,
        fail_list: Arc<AtomicBool>,
        /// Id de cada prévia, para os testes falarem em "A", "B", "C".
        ids: Vec<(String, i64)>,
    }

    impl Fixture {
        /// Lista na ordem da tela (o primeiro é o mais recente).
        fn new(previews: &[&str]) -> Self {
            Self::with(previews, Setup::default())
        }

        fn with(previews: &[&str], setup: Setup<'_>) -> Self {
            let mut inner = FakeHistoryRepository::new();
            let mut ids = Vec::new();
            for (i, p) in previews.iter().rev().enumerate() {
                let tag = (i + 1) as u8;
                let rec = if setup.images.contains(p) {
                    image_record(tag, p)
                } else {
                    record(tag, p)
                };
                let id = match inner.upsert(&rec).expect("semear") {
                    UpsertOutcome::Inserted { clip_id } | UpsertOutcome::Deduped { clip_id } => {
                        clip_id
                    }
                };
                ids.push((p.to_string(), id));
            }
            for p in setup.pinned {
                let id = ids.iter().find(|(n, _)| n == p).expect("fixado existe").1;
                inner.set_pinned(id, true, 25).expect("fixar");
            }
            let repo = Arc::new(Mutex::new(inner));
            let get_full_calls = Arc::new(AtomicUsize::new(0));
            let fail_list = Arc::new(AtomicBool::new(false));
            let last_get_full = Arc::new(Mutex::new(None));
            let shared = SharedRepo {
                inner: Arc::clone(&repo),
                fail_delete: setup.fail_delete,
                fail_list: Arc::clone(&fail_list),
                get_full_calls: Arc::clone(&get_full_calls),
                last_get_full: Arc::clone(&last_get_full),
            };

            let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
            let worker_queue = Arc::clone(&queue);
            let mut worker_repo = shared.clone();
            let mut config = Config::with_paths("db".into(), "logs".into());
            if let Some(max) = setup.max_pinned {
                config.max_pinned = max;
            }
            let worker = thread::spawn(move || {
                let counters = WorkerCounters::default();
                let (heuristic_tx, _heuristic_rx) = std::sync::mpsc::channel();
                run_worker(
                    worker_queue.as_ref(),
                    &mut worker_repo,
                    &config,
                    &counters,
                    &heuristic_tx,
                );
            });

            let window = HistoryWindow::new(
                shared,
                || -> Result<Box<dyn HistoryReader>, StoreError> {
                    Ok(Box::new(FakeHistoryRepository::new()))
                },
                || false,
                Arc::clone(&queue),
                PathBuf::from("test.db"),
                Rc::new(SelfWriteFilter::new()),
            )
            .expect("criar a janela");
            let f = Fixture {
                window,
                queue,
                worker: Some(worker),
                repo,
                get_full_calls,
                last_get_full,
                fail_list,
                ids,
            };
            f.send(WM_HOTKEY, 1, 0);
            assert!(f.visible(), "o atalho abre o histórico");
            f
        }

        fn hwnd(&self) -> HWND {
            self.window.hwnd()
        }

        fn visible(&self) -> bool {
            // SAFETY: consulta sem pré-condição.
            unsafe { IsWindowVisible(self.hwnd()) }.as_bool()
        }

        fn send(&self, msg: u32, wparam: usize, lparam: isize) {
            // SAFETY: `hwnd` criada nesta thread; `SendMessageW` síncrono.
            unsafe {
                SendMessageW(self.hwnd(), msg, Some(WPARAM(wparam)), Some(LPARAM(lparam)));
            }
        }

        fn id(&self, preview: &str) -> i64 {
            self.ids
                .iter()
                .find(|(p, _)| p == preview)
                .expect("prévia semeada")
                .1
        }

        fn ids_of(&self, previews: &[&str]) -> Vec<i64> {
            previews.iter().map(|p| self.id(p)).collect()
        }

        fn width(&self) -> i32 {
            let mut client = RECT::default();
            // SAFETY: `hwnd` válido.
            unsafe {
                let _ = GetClientRect(self.hwnd(), &mut client);
            }
            client.right - client.left
        }

        fn row_y(&self, row: usize) -> i32 {
            LIST_TOP_PX + row as i32 * ROW_HEIGHT_PX + ROW_HEIGHT_PX / 2
        }

        /// Centro da lixeira da linha `row` (lado = altura da linha, na
        /// borda direita).
        fn trash(&self, row: usize) -> isize {
            point(self.width() - ROW_HEIGHT_PX / 2, self.row_y(row))
        }

        fn text(&self, row: usize) -> isize {
            point(80, self.row_y(row))
        }

        /// Centro do alfinete da linha `row` (lado = altura da linha, na
        /// borda esquerda).
        fn pin(&self, row: usize) -> isize {
            point(ROW_HEIGHT_PX / 2, self.row_y(row))
        }

        /// Ids gravados como fixados, do mais recente ao mais antigo.
        fn pinned_ids(&self) -> Vec<i64> {
            self.repo
                .lock()
                .unwrap()
                .list_for_display(1000)
                .expect("listar")
                .into_iter()
                .filter(|i| i.pinned)
                .map(|i| i.id)
                .collect()
        }

        /// Um pixel à esquerda da lixeira: a fresta entre o texto e ela.
        fn gap(&self, row: usize) -> isize {
            point(self.width() - ROW_HEIGHT_PX - 1, self.row_y(row))
        }

        fn click(&self, at: isize) {
            self.send(WM_LBUTTONDOWN, 0, at);
        }

        fn stored_ids(&self) -> Vec<i64> {
            self.repo.lock().unwrap().ids_by_recency()
        }

        fn has_update_region(&self) -> bool {
            // SAFETY: `hwnd` válido; `None` só consulta se há região inválida.
            unsafe { GetUpdateRect(self.hwnd(), None, false) }.as_bool()
        }

        /// O último item que a janela tentou colar.
        fn pasted(&self) -> Option<i64> {
            *self.last_get_full.lock().unwrap()
        }

        fn key(&self, vk: u16) {
            self.send(WM_KEYDOWN, vk as usize, 0);
        }

        fn type_text(&self, text: &str) {
            for ch in text.chars() {
                self.send(WM_CHAR, ch as usize, 0);
            }
        }

        /// Linhas que a lista mostra por página. Repete a conta de
        /// `visible_row_count` (privada em `history_window.rs`): a área da
        /// lista vai de `LIST_TOP_PX` (faixa de tipo + busca) até a faixa de
        /// dicas (`HINT_STRIP_PX`), em linhas inteiras de `ROW_HEIGHT_PX`, no
        /// mínimo uma. Se a produção mudar essa conta, mude aqui também.
        fn rows_per_page(&self) -> usize {
            use duplicata_core::hint_strip::HINT_STRIP_PX;
            let mut c = RECT::default();
            // SAFETY: `hwnd` válido.
            unsafe {
                let _ = GetClientRect(self.hwnd(), &mut c);
            }
            (((c.bottom - c.top - LIST_TOP_PX - HINT_STRIP_PX).max(0)) / ROW_HEIGHT_PX).max(1)
                as usize
        }

        /// Copia de novo, fora do app, o conteúdo semeado como `preview`: o
        /// dedupe sobe a recência dele acima de todas as outras.
        fn recopy(&self, preview: &str) {
            // `with` semeia do mais antigo ao mais recente: a etiqueta (e a
            // identidade) de cada prévia é a posição dela em `ids` mais um.
            let pos = self
                .ids
                .iter()
                .position(|(p, _)| p == preview)
                .expect("semeado");
            let tag = u8::try_from(pos + 1).expect("etiqueta cabe em u8");
            let mut rec = record(tag, preview);
            rec.captured_at = Timestamp::from_millis(10_000);
            let outcome = self.repo.lock().unwrap().upsert(&rec).expect("recopiar");
            assert_eq!(
                outcome,
                UpsertOutcome::Deduped {
                    clip_id: self.id(preview)
                },
                "a recópia cai no mesmo item"
            );
        }

        /// Esconde a janela e a reabre pelo atalho. O atalho logo depois de
        /// esconder é ignorado de propósito (debounce no relógio real), então
        /// repete até ela abrir, com prazo.
        fn reopen(&self) {
            self.send(WM_HOTKEY, 1, 0);
            assert!(!self.visible(), "o atalho esconde o histórico aberto");
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
            while !self.visible() {
                assert!(
                    std::time::Instant::now() < deadline,
                    "o histórico não reabriu"
                );
                std::thread::yield_now();
                self.send(WM_HOTKEY, 1, 0);
            }
        }

        fn paint_now(&self) {
            // SAFETY: `hwnd` desta thread; pinta e valida a janela agora.
            unsafe {
                let _ = UpdateWindow(self.hwnd());
            }
        }
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.queue.close();
            if let Some(w) = self.worker.take() {
                let _ = w.join();
            }
        }
    }

    fn point(x: i32, y: i32) -> isize {
        (((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF)) as isize
    }

    thread_local! {
        static DIALOG_TITLES: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
    }

    /// Fecha toda caixa de diálogo que abrir nesta thread (o aviso é modal e
    /// travaria o teste) e anota o título dela.
    struct DialogCloser(HHOOK);

    impl DialogCloser {
        fn install() -> Self {
            DIALOG_TITLES.with(|t| t.borrow_mut().clear());
            // SAFETY: gancho só desta thread, retirado no `Drop`.
            let hook = unsafe {
                SetWindowsHookExW(WH_CBT, Some(close_dialogs), None, GetCurrentThreadId())
            }
            .expect("instalar o gancho CBT");
            DialogCloser(hook)
        }

        fn titles(&self) -> Vec<String> {
            DIALOG_TITLES.with(|t| t.borrow().clone())
        }
    }

    impl Drop for DialogCloser {
        fn drop(&mut self) {
            // SAFETY: o gancho instalado em `install`.
            unsafe {
                let _ = UnhookWindowsHookEx(self.0);
            }
        }
    }

    unsafe extern "system" fn close_dialogs(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        if code == HCBT_ACTIVATE as i32 {
            let hwnd = HWND(wparam.0 as *mut core::ffi::c_void);
            let mut class = [0u16; 64];
            // SAFETY: `hwnd` é a janela sendo ativada; buffers locais.
            let n = unsafe { GetClassNameW(hwnd, &mut class) };
            if String::from_utf16_lossy(&class[..n.max(0) as usize]) == "#32770" {
                let mut title = [0u16; 256];
                // SAFETY: idem.
                let n = unsafe { GetWindowTextW(hwnd, &mut title) };
                let title = String::from_utf16_lossy(&title[..n.max(0) as usize]);
                DIALOG_TITLES.with(|t| t.borrow_mut().push(title));
                // SAFETY: fecha o diálogo assim que o laço modal dele rodar.
                unsafe {
                    let _ = PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0));
                }
            }
        }
        // SAFETY: repassa ao próximo gancho, como a API pede.
        unsafe { CallNextHookEx(None, code, wparam, lparam) }
    }

    const ABCD: [&str; 4] = ["A", "B", "C", "D"];

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn the_trash_of_another_row_deletes_it_and_the_selection_stays_on_its_item() {
        let f = Fixture::new(&ABCD);
        assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
        f.click(f.trash(2));
        assert_eq!(f.stored_ids(), f.ids_of(&["A", "B", "D"]));
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "B", "D"]));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
        assert!(f.visible(), "a janela continua aberta");
        assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0, "nada colado");
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn deleting_a_row_above_the_selection_keeps_the_selection_on_its_item() {
        let f = Fixture::new(&ABCD);
        f.click(f.text(2));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
        f.click(f.trash(0));
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["B", "C", "D"]));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn deleting_the_selected_row_selects_the_item_that_took_its_place() {
        let f = Fixture::new(&ABCD);
        f.click(f.text(1));
        f.click(f.trash(1));
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "C", "D"]));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn deleting_the_selected_last_row_selects_the_new_last() {
        let f = Fixture::new(&["A", "B", "C"]);
        f.click(f.text(2));
        f.click(f.trash(2));
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "B"]));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("B")));
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn the_trash_deletes_a_pinned_item_too() {
        let f = Fixture::with(
            &["P", "A", "B"],
            Setup {
                pinned: &["P"],
                ..Setup::default()
            },
        );
        assert_eq!(f.window.visible_clip_ids()[0], f.id("P"), "fixado no topo");
        assert_eq!(f.window.pinned_boundary(), Some(1), "faixa de fixados");
        f.click(f.trash(0));
        assert_eq!(f.stored_ids(), f.ids_of(&["A", "B"]));
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "B"]));
        assert_eq!(f.window.pinned_boundary(), None, "P era o único fixado");
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn the_trash_deletes_the_last_item_matching_the_filter() {
        let f = Fixture::new(&["outra", "minha senha", "mais uma"]);
        for ch in "senha".chars() {
            f.send(WM_CHAR, ch as usize, 0);
        }
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["minha senha"]));
        f.click(f.trash(0));
        assert_eq!(f.window.visible_clip_ids(), Vec::<i64>::new());
        assert_eq!(f.stored_ids(), f.ids_of(&["outra", "mais uma"]));
        assert_eq!(
            f.window.list_display_state(),
            ListDisplayState::NoMatch,
            "aparece \"Nenhum item corresponde ao filtro\""
        );
        f.paint_now(); // "Nenhum item corresponde ao filtro", sem lixeira
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn deleting_the_only_item_leaves_the_empty_list() {
        let f = Fixture::new(&["Z"]);
        f.click(f.trash(0));
        assert!(f.stored_ids().is_empty());
        assert!(f.window.visible_clip_ids().is_empty());
        assert_eq!(f.window.selected_clip_id(), None);
        assert_eq!(
            f.window.list_display_state(),
            ListDisplayState::EmptyHistory
        );
        f.paint_now();
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; o aviso modal é fechado por um gancho CBT"]
    fn a_failed_delete_rereads_the_list_and_warns() {
        let f = Fixture::with(
            &["A", "B", "C"],
            Setup {
                fail_delete: true,
                ..Setup::default()
            },
        );
        let closer = DialogCloser::install();
        f.click(f.trash(1));
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "B", "C"]));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
        assert_eq!(closer.titles(), vec!["Apagar item — duplicata".to_string()]);
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn a_click_on_the_text_or_on_the_gap_only_selects() {
        let f = Fixture::new(&ABCD);
        f.click(f.text(1));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("B")));
        f.click(f.gap(2));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
        assert_eq!(f.stored_ids(), f.ids_of(&ABCD), "nada foi apagado");
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn with_the_error_banner_a_click_on_the_trash_spot_deletes_nothing() {
        let f = Fixture::new(&["A", "B"]);
        // A recarga depois de apagar B falha: a faixa de erro toma a tela.
        f.fail_list.store(true, Ordering::SeqCst);
        f.click(f.trash(1));
        assert_eq!(f.stored_ids(), f.ids_of(&["A"]));

        f.click(f.trash(0));
        f.send(WM_LBUTTONDBLCLK, 0, f.trash(0));
        assert_eq!(f.stored_ids(), f.ids_of(&["A"]), "nada mais foi apagado");
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn a_double_click_on_the_trash_deletes_exactly_one_item_and_pastes_nothing() {
        let f = Fixture::new(&["A", "B", "C"]);
        f.send(WM_LBUTTONDOWN, 0, f.trash(0));
        f.send(WM_LBUTTONDBLCLK, 0, f.trash(0));
        assert_eq!(f.stored_ids(), f.ids_of(&["B", "C"]), "B continua gravado");
        assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["B", "C"]));
        assert_eq!(f.window.selected_clip_id(), Some(f.id("B")));
        assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0, "nada colado");
        assert!(f.visible());
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn a_double_click_on_the_text_still_pastes() {
        let f = Fixture::new(&["A", "B", "C"]);
        f.send(WM_LBUTTONDOWN, 0, f.text(1));
        f.send(WM_LBUTTONDBLCLK, 0, f.text(1));
        assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 1, "tentou colar");
        assert_eq!(f.stored_ids(), f.ids_of(&["A", "B", "C"]));
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn the_preview_hides_over_the_trash_and_does_not_reopen_there() {
        let long = "SELECT id, nome\n  FROM cliente\n\tWHERE ativo = 1\nORDER BY nome";
        let f = Fixture::new(&[long, "curto"]);
        let popup = f
            .window
            .hover_popup_hwnd()
            .expect("o histórico cria o popup");
        // SAFETY: consulta sem pré-condição.
        let popup_visible = || unsafe { IsWindowVisible(popup) }.as_bool();

        f.send(WM_MOUSEMOVE, 0, f.text(0));
        f.send(WM_MOUSEHOVER, 0, f.text(0));
        assert!(popup_visible(), "no texto a pré-visualização abre");

        f.send(WM_MOUSEMOVE, 0, f.trash(0));
        assert!(!popup_visible(), "entrar na lixeira esconde");
        f.send(WM_MOUSEHOVER, 0, f.trash(0));
        assert!(!popup_visible(), "parado na lixeira não reabre");
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; redimensiona uma janela real"]
    fn the_strip_below_the_last_drawn_row_is_not_a_row() {
        use duplicata_core::hint_strip::HINT_STRIP_PX;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        };

        let names: Vec<String> = (0..40).map(|i| format!("item {i:02}")).collect();
        let previews: Vec<&str> = names.iter().map(String::as_str).collect();
        let f = Fixture::new(&previews);

        let client_h = || {
            let mut c = RECT::default();
            // SAFETY: `hwnd` válido.
            unsafe {
                let _ = GetClientRect(f.hwnd(), &mut c);
            }
            c.bottom - c.top
        };
        let leftover = |h: i32| (h - LIST_TOP_PX - HINT_STRIP_PX).max(0) % ROW_HEIGHT_PX;
        if leftover(client_h()) == 0 {
            // Sem sobra: cresce a janela alguns pixels para abrir a faixa.
            let mut w = RECT::default();
            // SAFETY: `hwnd` desta thread; só escreve locais e redimensiona.
            unsafe {
                let _ = GetWindowRect(f.hwnd(), &mut w);
                let _ = SetWindowPos(
                    f.hwnd(),
                    None,
                    0,
                    0,
                    w.right - w.left,
                    w.bottom - w.top + 5,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        let h = client_h();
        assert!(
            leftover(h) > 0,
            "o teste precisa de sobra abaixo das linhas"
        );
        let rows = (h - LIST_TOP_PX - HINT_STRIP_PX) / ROW_HEIGHT_PX;
        assert!((rows as usize) < previews.len(), "mais itens que a página");

        // Primeiro pixel abaixo da última linha desenhada, onde a lixeira da
        // linha seguinte (não desenhada) estaria.
        let below = point(
            f.width() - ROW_HEIGHT_PX / 2,
            LIST_TOP_PX + rows * ROW_HEIGHT_PX,
        );
        let before = f.stored_ids();
        f.click(below);
        f.send(WM_LBUTTONDBLCLK, 0, below);
        assert_eq!(f.stored_ids(), before, "nada foi apagado");
        assert_eq!(f.window.selected_clip_id(), Some(f.id("item 00")));
    }

    /// Garante sobra abaixo das linhas (cresce a janela se preciso) e devolve
    /// quantas linhas a pintura desenha.
    fn drawn_rows_with_leftover(f: &Fixture) -> i32 {
        use duplicata_core::hint_strip::HINT_STRIP_PX;
        use windows::Win32::UI::WindowsAndMessaging::{
            GetWindowRect, SetWindowPos, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOZORDER,
        };
        let client_h = || {
            let mut c = RECT::default();
            // SAFETY: `hwnd` válido.
            unsafe {
                let _ = GetClientRect(f.hwnd(), &mut c);
            }
            c.bottom - c.top
        };
        let leftover = |h: i32| (h - LIST_TOP_PX - HINT_STRIP_PX).max(0) % ROW_HEIGHT_PX;
        if leftover(client_h()) == 0 {
            let mut w = RECT::default();
            // SAFETY: `hwnd` desta thread; só escreve locais e redimensiona.
            unsafe {
                let _ = GetWindowRect(f.hwnd(), &mut w);
                let _ = SetWindowPos(
                    f.hwnd(),
                    None,
                    0,
                    0,
                    w.right - w.left,
                    w.bottom - w.top + 5,
                    SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE,
                );
            }
        }
        let h = client_h();
        assert!(
            leftover(h) > 0,
            "o teste precisa de sobra abaixo das linhas"
        );
        (h - LIST_TOP_PX - HINT_STRIP_PX) / ROW_HEIGHT_PX
    }

    fn forty() -> Vec<String> {
        (0..40).map(|i| format!("item {i:02}")).collect()
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; redimensiona uma janela real"]
    fn the_last_drawn_row_trash_deletes_that_row_with_leftover_below() {
        let names = forty();
        let previews: Vec<&str> = names.iter().map(String::as_str).collect();
        let f = Fixture::new(&previews);
        let rows = drawn_rows_with_leftover(&f);
        let before = f.stored_ids();
        f.click(f.trash((rows - 1) as usize));
        let deleted: Vec<i64> = before
            .iter()
            .copied()
            .filter(|id| !f.stored_ids().contains(id))
            .collect();
        assert_eq!(deleted, vec![f.id(&format!("item {:02}", rows - 1))]);
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; redimensiona uma janela real"]
    fn scrolled_to_the_end_the_last_drawn_row_trash_deletes_the_last_item() {
        use windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN;
        const VK_END: usize = 0x23;
        let names = forty();
        let previews: Vec<&str> = names.iter().map(String::as_str).collect();
        let f = Fixture::new(&previews);
        let rows = drawn_rows_with_leftover(&f);
        f.send(WM_KEYDOWN, VK_END, 0);
        assert_eq!(f.window.selected_clip_id(), Some(f.id("item 39")));

        let before = f.stored_ids();
        f.click(f.trash((rows - 1) as usize));
        let after = f.stored_ids();
        assert_eq!(after.len(), before.len() - 1);
        assert!(
            !after.contains(&f.id("item 39")),
            "a última linha desenhada é a última"
        );
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; cria janelas reais"]
    fn below_a_list_shorter_than_the_page_the_trash_spot_hits_nothing() {
        let f = Fixture::new(&["A", "B", "C"]);
        let below = f.trash(3);
        let before = f.stored_ids();
        f.click(below);
        f.send(WM_LBUTTONDBLCLK, 0, below);
        assert_eq!(f.stored_ids(), before);
        assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
    }

    #[test]
    #[ignore = "precisa de sessão gráfica; pinta a janela de verdade"]
    fn the_mouse_repaints_only_when_the_hot_row_or_zone_changes() {
        let f = Fixture::new(&ABCD);
        f.paint_now();
        assert!(!f.has_update_region());

        // Seleção em A, mouse sobre o texto de C: pinta A e C com lixeira.
        f.send(WM_MOUSEMOVE, 0, f.text(2));
        assert!(f.has_update_region(), "a linha sob o mouse mudou");
        f.paint_now();

        f.send(WM_MOUSEMOVE, 0, point(120, f.row_y(2)));
        assert!(
            !f.has_update_region(),
            "mesma linha, mesma zona: nada a pintar"
        );

        f.send(WM_MOUSEMOVE, 0, f.trash(2));
        assert!(f.has_update_region(), "entrou na lixeira: fundo do hover");
        f.paint_now();

        f.send(WM_MOUSELEAVE, 0, 0);
        assert!(f.has_update_region(), "o mouse saiu: a lixeira de C some");
        f.paint_now();

        f.send(WM_MOUSELEAVE, 0, 0);
        assert!(!f.has_update_region(), "saída repetida não repinta");
    }

    mod pin_icon {
        use super::*;

        const VK_TAB: u16 = 0x09;
        const VK_RIGHT: u16 = 0x27;

        fn sorted(mut ids: Vec<i64>) -> Vec<i64> {
            ids.sort_unstable();
            ids
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn the_pin_of_another_row_pins_it_and_the_selection_stays_on_its_item() {
            // Aceite 4: A, B, C não fixados; seleção em A; ícone de C.
            let f = Fixture::new(&["A", "B", "C"]);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
            f.click(f.pin(2));
            assert_eq!(f.pinned_ids(), f.ids_of(&["C"]), "C gravado como fixado");
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["C", "A", "B"]));
            assert_eq!(f.window.pinned_boundary(), Some(1), "C no grupo do topo");
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
            assert!(f.visible(), "a janela continua aberta");
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0, "nada colado");
            f.paint_now();
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn the_pin_of_the_selected_row_pins_it_and_keeps_it_selected() {
            // Aceite 5.
            let f = Fixture::new(&["A", "B", "C"]);
            f.click(f.text(2));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
            f.click(f.pin(2));
            assert_eq!(f.pinned_ids(), f.ids_of(&["C"]));
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["C", "A", "B"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
            assert!(f.visible());
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0);
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn the_pin_of_a_pinned_row_unpins_it_back_to_its_date_position() {
            // Aceite 6.
            let f = Fixture::with(
                &["A", "B", "C"],
                Setup {
                    pinned: &["C"],
                    ..Setup::default()
                },
            );
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["C", "A", "B"]));
            let selected = f.window.selected_clip_id();
            f.click(f.pin(0));
            assert!(f.pinned_ids().is_empty(), "C deixou de ser fixado");
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "B", "C"]));
            assert_eq!(f.window.pinned_boundary(), None);
            assert_eq!(f.window.selected_clip_id(), selected, "seleção intacta");
            assert!(f.visible());
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0);
            f.paint_now();
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; o aviso modal é fechado por um gancho CBT"]
        fn with_the_pin_cap_reached_the_icon_warns_and_pins_nothing() {
            // Aceite 8: teto 2, já com 2 fixados.
            let f = Fixture::with(
                &["P", "Q", "A", "B"],
                Setup {
                    pinned: &["P", "Q"],
                    max_pinned: Some(2),
                    ..Setup::default()
                },
            );
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["P", "Q", "A", "B"]));
            let selected = f.window.selected_clip_id();
            let closer = DialogCloser::install();
            f.click(f.pin(2));
            assert_eq!(closer.titles(), vec!["Fixar item — duplicata".to_string()]);
            assert_eq!(sorted(f.pinned_ids()), sorted(f.ids_of(&["P", "Q"])));
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["P", "Q", "A", "B"]));
            assert_eq!(f.window.selected_clip_id(), selected);
            // Visibilidade não conferida: o aviso modal toma o primeiro plano
            // e o `WM_ACTIVATE` da janela a esconde, como já acontece pelo
            // menu (mesmo `show_pin_cap_message`). Pendência registrada.
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0);
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn with_the_image_filter_the_pinned_image_stays_in_the_filter() {
            // Aceite 9.
            let f = Fixture::with(
                &["T", "I1", "I2"],
                Setup {
                    images: &["I1", "I2"],
                    ..Setup::default()
                },
            );
            f.key(VK_TAB);
            f.key(VK_RIGHT);
            f.key(VK_RIGHT);
            assert_eq!(
                f.window.visible_clip_ids(),
                f.ids_of(&["I1", "I2"]),
                "filtro Imagem"
            );
            let selected = f.window.selected_clip_id();
            f.click(f.pin(1));
            assert_eq!(f.pinned_ids(), f.ids_of(&["I2"]));
            assert_eq!(
                f.window.visible_clip_ids(),
                f.ids_of(&["I2", "I1"]),
                "continua no filtro Imagem"
            );
            assert_eq!(f.window.selected_clip_id(), selected);
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0);
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        /// Cobre o conjunto de proteções com a faixa de erro aberta, não uma
        /// só: a lista é esvaziada na falha de leitura, `row_hit` não acha
        /// linha com a faixa, o clique simples vai para a faixa e o segundo
        /// clique é ignorado pelo `ClickGuard` armado pela lixeira. Nenhum
        /// efeito da faixa é observável aqui (`may_offer_recreate` é falso no
        /// `Fixture`), por isso o teste só confere que nada foi fixado.
        fn with_the_error_banner_no_click_on_the_pin_spot_pins_anything() {
            let f = Fixture::new(&["A", "B"]);
            // A recarga depois de apagar B falha: a faixa de erro toma a tela.
            f.fail_list.store(true, Ordering::SeqCst);
            f.click(f.trash(1));
            assert_eq!(f.stored_ids(), f.ids_of(&["A"]));

            f.click(f.pin(0));
            f.send(WM_LBUTTONDBLCLK, 0, f.pin(0));
            assert!(f.pinned_ids().is_empty(), "nada foi fixado");
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0);
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; o gancho CBT anota qualquer aviso"]
        fn the_pin_of_an_item_already_deleted_rereads_the_list_without_warning() {
            let f = Fixture::new(&["A", "B", "C"]);
            let gone = f.id("B");
            assert!(f.repo.lock().unwrap().delete_one(gone).expect("apagar"));
            let closer = DialogCloser::install();
            f.click(f.pin(1));
            assert!(closer.titles().is_empty(), "nenhum aviso");
            assert!(f.pinned_ids().is_empty(), "nada foi fixado");
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "C"]));
            assert!(f.visible());
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn a_double_click_on_the_pin_pins_once_and_pastes_nothing() {
            // Aceite 7: o segundo clique cai sobre a linha que tomou o lugar
            // de C e não pode colar, fixar nem apagar.
            let f = Fixture::new(&["B", "C", "A"]);
            f.send(WM_LBUTTONDOWN, 0, f.pin(1));
            f.send(WM_LBUTTONDBLCLK, 0, f.pin(1));
            assert_eq!(f.pinned_ids(), f.ids_of(&["C"]), "só C, uma vez");
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["C", "B", "A"]));
            assert_eq!(f.stored_ids().len(), 3, "nada apagado");
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 0, "nada colado");
            assert_eq!(f.window.selected_clip_id(), Some(f.id("B")));
            assert!(f.visible());
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn a_double_click_on_the_text_still_pastes_with_the_pin_column() {
            let f = Fixture::new(&["A", "B", "C"]);
            f.send(WM_LBUTTONDOWN, 0, f.text(1));
            f.send(WM_LBUTTONDBLCLK, 0, f.text(1));
            assert_eq!(f.get_full_calls.load(Ordering::SeqCst), 1, "tentou colar");
            assert_eq!(f.pasted(), Some(f.id("B")));
            assert!(f.pinned_ids().is_empty());
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn the_preview_hides_over_the_pin_and_does_not_reopen_there() {
            let long = "SELECT id, nome\n  FROM cliente\n\tWHERE ativo = 1\nORDER BY nome";
            let f = Fixture::new(&[long, "curto"]);
            let popup = f
                .window
                .hover_popup_hwnd()
                .expect("o histórico cria o popup");
            // SAFETY: consulta sem pré-condição.
            let popup_visible = || unsafe { IsWindowVisible(popup) }.as_bool();

            f.send(WM_MOUSEMOVE, 0, f.text(0));
            f.send(WM_MOUSEHOVER, 0, f.text(0));
            assert!(popup_visible(), "no texto a pré-visualização abre");

            f.send(WM_MOUSEMOVE, 0, f.pin(0));
            assert!(!popup_visible(), "entrar no alfinete esconde");
            f.send(WM_MOUSEHOVER, 0, f.pin(0));
            assert!(!popup_visible(), "parado no alfinete não reabre");
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; pinta a janela de verdade"]
        fn the_mouse_repaints_when_it_enters_the_pin_icon() {
            let f = Fixture::new(&ABCD);
            f.send(WM_MOUSEMOVE, 0, f.text(2));
            f.paint_now();
            assert!(!f.has_update_region());

            f.send(WM_MOUSEMOVE, 0, f.pin(2));
            assert!(f.has_update_region(), "entrou no alfinete: fundo do hover");
            f.paint_now();

            f.send(WM_MOUSEMOVE, 0, point(ROW_HEIGHT_PX / 2 + 1, f.row_y(2)));
            assert!(!f.has_update_region(), "mesmo alfinete: nada a pintar");

            f.send(WM_MOUSEMOVE, 0, f.text(2));
            assert!(f.has_update_region(), "saiu do alfinete");
            f.paint_now();
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; mexe na tabela de teclado da thread"]
        fn ctrl_p_and_the_icon_toggle_the_same_item_state() {
            use windows::Win32::UI::Input::KeyboardAndMouse::VK_P;
            let ctrl_p = |f: &Fixture| {
                let _ctrl = super::super::HeldCtrl::press();
                f.key(VK_P.0);
            };

            let f = Fixture::new(&["A", "B", "C"]);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
            ctrl_p(&f);
            assert_eq!(f.pinned_ids(), f.ids_of(&["A"]), "Ctrl+P fixou A");
            f.click(f.pin(0));
            assert!(f.pinned_ids().is_empty(), "o ícone desafixou A");
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
            f.click(f.pin(0));
            assert_eq!(f.pinned_ids(), f.ids_of(&["A"]), "o ícone fixou A");
            ctrl_p(&f);
            assert!(f.pinned_ids().is_empty(), "Ctrl+P desafixou A");
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "B", "C"]));
        }
    }

    mod default_selection {
        use super::*;

        const VK_RETURN: u16 = 0x0D;
        const VK_HOME: u16 = 0x24;
        const VK_UP: u16 = 0x26;
        const VK_TAB: u16 = 0x09;
        const VK_RIGHT: u16 = 0x27;

        /// A(texto, fixado) │ D(imagem) C(texto).
        fn pinned_text_recent_image() -> Fixture {
            Fixture::with(
                &["D", "C", "A"],
                Setup {
                    pinned: &["A"],
                    images: &["D"],
                    ..Setup::default()
                },
            )
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; pinta a janela de verdade"]
        fn clicking_the_text_segment_selects_the_most_recent_text() {
            let f = pinned_text_recent_image();
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "D", "C"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
            // As caixas dos segmentos só existem depois da primeira pintura.
            // Varre a faixa da esquerda para a direita: o primeiro clique que
            // muda a lista é o do segmento logo depois de "Tudo", o "Texto".
            f.paint_now();
            let all = f.window.visible_clip_ids();
            let y = 34 / 2; // meio da faixa de tipo (FILTER_STRIP_PX, privado)
            let changed = (0..f.width()).any(|x| {
                f.click(point(x, y));
                f.window.visible_clip_ids() != all
            });
            assert!(changed, "nenhum segmento mudou a lista");
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "C"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn tab_then_right_to_text_selects_the_most_recent_text() {
            let f = pinned_text_recent_image();
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
            f.key(VK_TAB);
            f.key(VK_RIGHT);
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "C"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("C")));
        }

        /// Recência crescente A < B < C < D; `previews` do mais recente ao
        /// mais antigo, como em `Fixture::new`.
        fn open(previews: &[&str], pinned: &[&str]) -> Fixture {
            Fixture::with(
                previews,
                Setup {
                    pinned,
                    ..Setup::default()
                },
            )
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn opens_on_the_most_recent_copy_below_an_older_pinned() {
            let f = open(&["D", "C", "B", "A"], &["A"]);
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "D", "C", "B"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn opens_on_the_pinned_when_it_is_the_most_recent_copy() {
            let f = open(&["D", "C", "B", "A"], &["D"]);
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["D", "C", "B", "A"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn up_from_the_opened_recent_selects_the_last_pinned() {
            let f = open(&["D", "C", "B", "A"], &["B", "A"]);
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["B", "A", "D", "C"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
            f.key(VK_UP);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn home_from_the_opened_recent_selects_the_first_pinned() {
            let f = open(&["D", "A"], &["A"]);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
            f.key(VK_HOME);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn only_pinned_opens_on_the_most_recent_pinned() {
            let f = open(&["B", "A"], &["B", "A"]);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("B")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn a_click_on_the_pinned_then_enter_pastes_the_pinned() {
            let f = open(&["D", "C", "B", "A"], &["A"]);
            f.click(f.text(0));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
            f.key(VK_RETURN);
            assert_eq!(f.pasted(), Some(f.id("A")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn enter_right_after_opening_pastes_the_most_recent_copy() {
            let f = open(&["D", "C", "B", "A"], &["A"]);
            f.key(VK_RETURN);
            assert_eq!(f.pasted(), Some(f.id("D")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn recopying_the_pinned_outside_the_app_selects_it_on_the_next_opening() {
            let f = open(&["D", "C", "A"], &["A"]);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
            f.recopy("A");
            f.reopen();
            assert_eq!(f.window.visible_clip_ids(), f.ids_of(&["A", "D", "C"]));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("A")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn many_pinned_scroll_just_enough_for_the_selected_to_be_the_last_visible_row() {
            // 24 fixados (o teto da Fixture é 25) passam da página em qualquer
            // escala; R é a cópia mais recente e vem depois de todos.
            let pinned_names: Vec<String> = (1..=24).rev().map(|n| format!("F{n:02}")).collect();
            let mut previews: Vec<&str> = vec!["R"];
            previews.extend(pinned_names.iter().map(String::as_str));
            let pinned: Vec<&str> = pinned_names.iter().map(String::as_str).collect();
            let f = open(&previews, &pinned);

            let total = previews.len();
            let page = f.rows_per_page();
            assert!(
                page < total,
                "a página ({page}) precisa ser menor que a lista"
            );
            let visible = f.window.visible_clip_ids();
            assert_eq!(visible[total - 1], f.id("R"), "R vem depois dos fixados");
            assert_eq!(f.window.selected_clip_id(), Some(f.id("R")));

            // R é a última linha desenhada: a rolagem é total - page.
            f.click(f.text(page - 1));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("R")));
            f.click(f.text(0));
            assert_eq!(f.window.selected_clip_id(), Some(visible[total - page]));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn an_empty_history_opens_without_selection() {
            let f = Fixture::new(&[]);
            assert_eq!(f.window.selected_clip_id(), None);
            assert_eq!(
                f.window.list_display_state(),
                ListDisplayState::EmptyHistory
            );
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn typing_the_filter_selects_the_most_recent_match() {
            let f = open(&["relatório", "senha banco", "senha wifi"], &["senha wifi"]);
            f.type_text("senha");
            assert_eq!(
                f.window.visible_clip_ids(),
                f.ids_of(&["senha wifi", "senha banco"])
            );
            assert_eq!(f.window.selected_clip_id(), Some(f.id("senha banco")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn clearing_the_filter_with_backspace_selects_the_most_recent_copy() {
            let f = open(&["relatório", "senha banco", "senha wifi"], &["senha wifi"]);
            f.type_text("senha");
            f.click(f.text(0));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("senha wifi")));
            for _ in "senha".chars() {
                f.send(WM_CHAR, 0x08, 0);
            }
            assert_eq!(f.window.visible_clip_ids().len(), 3);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("relatório")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn a_filter_without_matches_has_no_selection() {
            let f = open(&["D", "C", "A"], &["A"]);
            f.type_text("zzz");
            assert_eq!(f.window.selected_clip_id(), None);
            assert_eq!(f.window.list_display_state(), ListDisplayState::NoMatch);
        }

        const VK_ESCAPE: u16 = 0x1B;

        /// Filtro "senha" com o fixado selecionado por clique; devolve a
        /// fixture para a ação que limpa o filtro.
        fn filtered_with_the_pinned_clicked() -> Fixture {
            let f = open(&["relatório", "senha banco", "senha wifi"], &["senha wifi"]);
            f.type_text("senha");
            f.click(f.text(0));
            assert_eq!(f.window.selected_clip_id(), Some(f.id("senha wifi")));
            f
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn escape_clearing_the_filter_selects_the_most_recent_copy_and_keeps_the_window() {
            let f = filtered_with_the_pinned_clicked();
            f.key(VK_ESCAPE);
            assert!(f.visible(), "o primeiro Esc só limpa o filtro");
            assert_eq!(f.window.visible_clip_ids().len(), 3);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("relatório")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn the_delete_char_clearing_the_filter_selects_the_most_recent_copy() {
            let f = filtered_with_the_pinned_clicked();
            f.send(WM_CHAR, 0x7F, 0);
            assert_eq!(f.window.visible_clip_ids().len(), 3);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("relatório")));
        }

        #[test]
        #[ignore = "precisa de sessão gráfica; cria janelas reais"]
        fn an_unreadable_history_opens_with_the_error_banner_and_no_selection() {
            let f = open(&["D", "C", "A"], &["A"]);
            assert_eq!(f.window.selected_clip_id(), Some(f.id("D")));
            f.fail_list.store(true, Ordering::SeqCst);
            f.reopen();
            assert_eq!(f.window.list_display_state(), ListDisplayState::ErrorBanner);
            assert_eq!(f.window.selected_clip_id(), None);
        }
    }
}

fn counting_window(last_limit: &Arc<AtomicUsize>) -> HistoryWindow {
    let reader = CountingReader {
        calls: Arc::new(AtomicUsize::new(0)),
        last_limit: Arc::clone(last_limit),
        rows: vec![text_row(1, "x")],
    };
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    HistoryWindow::new(
        reader,
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela")
}

#[test]
#[ignore = "precisa de sessão gráfica (WM_HOTKEY)"]
fn the_settings_message_changes_the_pin_cap_used_by_the_list() {
    use duplicata_win::history_window::WM_APP_SET_MAX_PINNED;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, WM_HOTKEY};

    let last_limit = Arc::new(AtomicUsize::new(0));
    let window = counting_window(&last_limit);
    // SAFETY: `hwnd` desta thread; o teto novo chega como as Configurações
    // mandam, e o atalho global (HOTKEY_ID = 1) abre a lista.
    unsafe {
        SendMessageW(
            window.hwnd(),
            WM_APP_SET_MAX_PINNED,
            Some(WPARAM(40)),
            Some(LPARAM(0)),
        );
        SendMessageW(window.hwnd(), WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
    }
    assert_eq!(
        last_limit.load(Ordering::SeqCst),
        500 + 40,
        "o limite da lista passa a usar o teto novo"
    );
}

#[test]
#[ignore = "precisa de sessão gráfica"]
fn the_encryption_toggle_query_reports_the_target_while_it_runs() {
    use duplicata_win::history_window::{
        ENCRYPTION_TOGGLE_IDLE, ENCRYPTION_TOGGLE_TO_ON, WM_APP_QUERY_ENCRYPTION_TOGGLE,
        WM_APP_TOGGLE_ENCRYPTION,
    };
    use windows::Win32::Foundation::WPARAM;
    use windows::Win32::UI::WindowsAndMessaging::SendMessageW;

    let window = counting_window(&Arc::new(AtomicUsize::new(0)));
    // SAFETY: `hwnd` desta thread; sem worker, a reescrita pedida fica em
    // andamento até o fim do teste.
    let (before, during) = unsafe {
        let before = SendMessageW(window.hwnd(), WM_APP_QUERY_ENCRYPTION_TOGGLE, None, None);
        SendMessageW(
            window.hwnd(),
            WM_APP_TOGGLE_ENCRYPTION,
            Some(WPARAM(1)),
            None,
        );
        let during = SendMessageW(window.hwnd(), WM_APP_QUERY_ENCRYPTION_TOGGLE, None, None);
        (before.0, during.0)
    };
    assert_eq!(before, ENCRYPTION_TOGGLE_IDLE);
    assert_eq!(during, ENCRYPTION_TOGGLE_TO_ON);
}
