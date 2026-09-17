#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;

use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
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
#[ignore = "precisa de sessão gráfica; julgamento visual é manual (quickstart.md §1)"]
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
        "digitar no filtro NÃO pode disparar nova consulta ao banco (FR-019)"
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
        "o limit passa a MAX_ROWS + max_pinned = 500 + 200 (FR-030h/030i)"
    );
    println!("limit: 525 com o default, 700 após set_max_pinned(200)");
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
#[ignore = "precisa de sessão gráfica; julgamento visual é manual (quickstart.md §6)"]
fn rows_show_a_proportional_thumbnail_a_type_badge_and_a_visual_pin_marker() {
    use duplicata_core::row_layout::{self, RowBadge};
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
    // HALFTONE), seria aqui.
    // SAFETY: `hwnd` recém-criada nesta thread.
    unsafe {
        SendMessageW(window.hwnd(), WM_HOTKEY, Some(WPARAM(1)), Some(LPARAM(0)));
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

    assert_eq!(row_layout::badge_of("dib"), RowBadge::Image);
    assert_eq!(row_layout::badge_of("unicode_text"), RowBadge::Text);
    assert_eq!(row_layout::badge_of("hdrop"), RowBadge::Files);

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
    // fixar: o atalho chega por `WM_KEYDOWN`, nunca como caractere (research
    // R10) — é isso que impede a colisão com a busca incremental.
    // SAFETY: idem.
    unsafe {
        SendMessageW(hwnd, WM_CHAR, Some(WPARAM(0x10)), Some(LPARAM(0)));
    }
    assert!(
        pins.lock().unwrap().is_empty(),
        "`Ctrl+P` não pode chegar como WM_CHAR (FR-034a, research R10)"
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
         fixando-o (FR-034a). `NotFound` aqui significa que o repositório do \
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
             — abaixo do piso de {MIN_VISIBLE_ROWS} (FR-033a)",
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
    let click_lparam = LPARAM(((LIST_TOP_PX as u32) << 16) as isize);
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
         (FR-032). Suba WINDOW_WIDTH_PX para pelo menos {minimum}."
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
        "levou {elapsed:?} do atalho até aceitar a primeira tecla — o teto do \
         SC-010/FR-038 é 100ms"
    );
}
