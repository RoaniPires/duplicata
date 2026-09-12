#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::atomic::Ordering;
use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use duplicata_core::{
    run_worker, ByteBudgetQueue, CaptureQueue, Config, FakeHistoryRepository, HistoryReader,
    SelfWriteFilter, StoreError, WorkItem, WorkerCounters,
};
use duplicata_win::history_window::HistoryWindow;
use duplicata_win::listener::ClipboardListener;
use duplicata_win::message_loop::{self, MESSAGE_LOOP_ITERATIONS, MESSAGE_LOOP_TIMER_WAKEUPS};

const AMBIENT_WAKEUP_CEILING: u64 = 60;

fn assert_no_periodic_wakeups(
    what: &str,
    timer_before: u64,
    timer_after: u64,
    loop_before: u64,
    loop_after: u64,
) {
    let timers = timer_after - timer_before;
    let total = loop_after - loop_before;
    println!(
        "  [{what}] WM_TIMER: {timers} | despertares totais em 60s: {total} (teto ambiente {AMBIENT_WAKEUP_CEILING})"
    );
    assert_eq!(
        timers, 0,
        "{what}: chegaram {timers} WM_TIMER — alguém introduziu um temporizador. É exatamente o que o Princípio V proíbe."
    );
    assert!(
        total <= AMBIENT_WAKEUP_CEILING,
        "{what}: {total} despertares em 60s ociosos (teto {AMBIENT_WAKEUP_CEILING}). Nenhum WM_TIMER chegou, então não é `SetTimer` — procure um laço que posta mensagem para si mesmo. Atividade ambiente da máquina fica na casa de uma dezena."
    );
}

#[test]
#[ignore = "leva ~60s e precisa de sessão gráfica (AddClipboardFormatListener)"]
fn app_idle_for_60s_has_zero_message_loop_and_worker_wakeups() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let cfg = Config::with_paths("db".into(), "logs".into());
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg,
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    let listener_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>;
    let (ready_tx, ready_rx) = mpsc::channel();
    let loop_thread = thread::spawn(move || {
        let cfg = Config::with_paths("db".into(), "logs".into());
        let self_write_filter = Rc::new(SelfWriteFilter::new());
        let listener = ClipboardListener::new(listener_queue, cfg, self_write_filter)
            .expect("registrar o listener real precisa de sessão gráfica");
        let _ = ready_tx.send(message_loop::current_thread_id());
        message_loop::run();
        drop(listener);
    });
    let loop_thread_id = ready_rx.recv().expect("listener deve inicializar");

    let timer_before = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_before = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_before = counters.processed();

    thread::sleep(Duration::from_secs(60));

    let timer_after = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_after = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_after = counters.processed();

    message_loop::post_quit_to(loop_thread_id);
    loop_thread
        .join()
        .expect("thread da message loop não deve travar/entrar em pânico");
    queue.close();
    worker
        .join()
        .expect("worker não deve travar/entrar em pânico");

    assert_no_periodic_wakeups(
        "app_idle_for_60s_has_zero_message_loop_and_worker_wakeups",
        timer_before,
        timer_after,
        msgloop_before,
        msgloop_after,
    );
    assert_eq!(
        processed_after, processed_before,
        "a worker não pode acordar sozinha em 60s ociosos — nada foi empurrado pra fila (Condvar sem timeout)"
    );
}

#[test]
#[ignore = "leva ~60s e precisa de sessão gráfica (AddClipboardFormatListener/CreateWindowExW)"]
fn history_window_existing_but_never_opened_adds_zero_wakeups_over_60s() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let cfg = Config::with_paths("db".into(), "logs".into());
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg,
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    let listener_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>;
    let history_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>;
    let (ready_tx, ready_rx) = mpsc::channel();
    let loop_thread = thread::spawn(move || {
        let cfg = Config::with_paths("db".into(), "logs".into());
        let self_write_filter = Rc::new(SelfWriteFilter::new());
        let listener = ClipboardListener::new(listener_queue, cfg, Rc::clone(&self_write_filter))
            .expect("registrar o listener real precisa de sessão gráfica");

        let history_window = HistoryWindow::new(
            FakeHistoryRepository::new(),
            || -> Result<Box<dyn HistoryReader>, StoreError> {
                Ok(Box::new(FakeHistoryRepository::new()))
            },
            || true,
            history_queue,
            PathBuf::from("db"),
            self_write_filter,
        )
        .expect("criar a janela oculta precisa de sessão gráfica");

        let _ = ready_tx.send(message_loop::current_thread_id());
        message_loop::run();
        drop(history_window);
        drop(listener);
    });
    let loop_thread_id = ready_rx.recv().expect("listener/janela devem inicializar");

    thread::sleep(Duration::from_secs(1));

    let timer_before = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_before = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_before = counters.processed();

    thread::sleep(Duration::from_secs(60));

    let timer_after = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_after = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_after = counters.processed();

    message_loop::post_quit_to(loop_thread_id);
    loop_thread
        .join()
        .expect("thread da message loop não deve travar/entrar em pânico");
    queue.close();
    worker
        .join()
        .expect("worker não deve travar/entrar em pânico");

    assert_no_periodic_wakeups(
        "history_window_existing_but_never_opened_adds_zero_wakeups_over_60s",
        timer_before,
        timer_after,
        msgloop_before,
        msgloop_after,
    );
    assert_eq!(
        processed_after, processed_before,
        "a HistoryWindow existir não pode acordar a worker sozinha em 60s ociosos"
    );
}

#[test]
#[ignore = "leva ~60s e precisa de sessão gráfica"]
fn fatia4_open_theme_switch_type_close_adds_no_periodic_wakeups() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        SendMessageW, WM_CHAR, WM_SETTINGCHANGE, WM_THEMECHANGED,
    };

    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let cfg = Config::with_paths("db".into(), "logs".into());
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg,
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    let listener_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>;
    let history_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>;
    let (ready_tx, ready_rx) = mpsc::channel();
    let loop_thread = thread::spawn(move || {
        let cfg = Config::with_paths("db".into(), "logs".into());
        let self_write_filter = Rc::new(SelfWriteFilter::new());
        let listener = ClipboardListener::new(listener_queue, cfg, Rc::clone(&self_write_filter))
            .expect("registrar o listener real precisa de sessão gráfica");

        let history_window = HistoryWindow::new(
            FakeHistoryRepository::new(),
            || -> Result<Box<dyn HistoryReader>, StoreError> {
                Ok(Box::new(FakeHistoryRepository::new()))
            },
            || true,
            history_queue,
            PathBuf::from("db"),
            self_write_filter,
        )
        .expect("criar a janela oculta precisa de sessão gráfica");

        let color_set: Vec<u16> = "ImmersiveColorSet\0".encode_utf16().collect();
        for _ in 0..3 {
            // SAFETY: `hwnd` é a janela recém-criada nesta thread.
            unsafe {
                SendMessageW(
                    history_window.hwnd(),
                    WM_SETTINGCHANGE,
                    Some(WPARAM(0)),
                    Some(LPARAM(color_set.as_ptr() as isize)),
                );
                SendMessageW(
                    history_window.hwnd(),
                    WM_THEMECHANGED,
                    Some(WPARAM(0)),
                    Some(LPARAM(0)),
                );
            }
        }

        for ch in "link".chars() {
            // SAFETY: `hwnd` desta thread.
            unsafe {
                SendMessageW(
                    history_window.hwnd(),
                    WM_CHAR,
                    Some(WPARAM(ch as usize)),
                    Some(LPARAM(0)),
                );
            }
        }

        let _ = ready_tx.send(message_loop::current_thread_id());
        message_loop::run();
        drop(history_window);
        drop(listener);
    });
    let loop_thread_id = ready_rx.recv().expect("listener/janela devem inicializar");

    thread::sleep(Duration::from_secs(1));

    let timer_before = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_before = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_before = counters.processed();

    thread::sleep(Duration::from_secs(60));

    let timer_after = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_after = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_after = counters.processed();

    message_loop::post_quit_to(loop_thread_id);
    loop_thread.join().expect("loop não deve travar");
    queue.close();
    worker.join().expect("worker não deve travar");

    assert_no_periodic_wakeups(
        "fatia4_open_theme_switch_type_close_adds_no_periodic_wakeups",
        timer_before,
        timer_after,
        msgloop_before,
        msgloop_after,
    );
    assert_eq!(
        processed_after, processed_before,
        "a worker não acorda por causa de uma troca de tema"
    );
}

#[test]
#[ignore = "leva ~60s e precisa de sessão gráfica"]
fn the_translucent_paint_path_adds_no_periodic_wakeups() {
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::Graphics::Gdi::{InvalidateRect, UpdateWindow};
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, ShowWindow, SW_HIDE, WM_HOTKEY};

    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let cfg = Config::with_paths("db".into(), "logs".into());
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg,
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    let buffered = duplicata_win::window_material::buffered_paint_init();
    println!("buffered paint disponível: {buffered}");

    let history_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>;
    let (ready_tx, ready_rx) = mpsc::channel();
    let loop_thread = thread::spawn(move || {
        let history_window = HistoryWindow::new(
            FakeHistoryRepository::new(),
            || -> Result<Box<dyn HistoryReader>, StoreError> {
                Ok(Box::new(FakeHistoryRepository::new()))
            },
            || true,
            history_queue,
            PathBuf::from("db"),
            Rc::new(SelfWriteFilter::new()),
        )
        .expect("criar a janela oculta precisa de sessão gráfica");

        // Abre e força repaints — cada um roda o par buffered paint inteiro.
        // SAFETY: `hwnd` é a janela recém-criada nesta thread.
        unsafe {
            SendMessageW(
                history_window.hwnd(),
                WM_HOTKEY,
                Some(WPARAM(1)),
                Some(LPARAM(0)),
            );
            for _ in 0..20 {
                let _ = InvalidateRect(Some(history_window.hwnd()), None, true);
                let _ = UpdateWindow(history_window.hwnd());
            }
            let _ = ShowWindow(history_window.hwnd(), SW_HIDE);
        }

        let _ = ready_tx.send(message_loop::current_thread_id());
        message_loop::run();
        drop(history_window);
    });
    let loop_thread_id = ready_rx.recv().expect("a janela deve inicializar");

    thread::sleep(Duration::from_secs(1));

    let timer_before = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_before = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_before = counters.processed();

    thread::sleep(Duration::from_secs(60));

    let timer_after = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_after = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_after = counters.processed();

    message_loop::post_quit_to(loop_thread_id);
    loop_thread.join().expect("loop não deve travar");
    if buffered {
        duplicata_win::window_material::buffered_paint_uninit();
    }
    queue.close();
    worker.join().expect("worker não deve travar");

    assert_no_periodic_wakeups(
        "the_translucent_paint_path_adds_no_periodic_wakeups",
        timer_before,
        timer_after,
        msgloop_before,
        msgloop_after,
    );
    assert_eq!(
        processed_after, processed_before,
        "a worker não acorda por causa de pintura"
    );
}

#[test]
#[ignore = "leva ~60s e precisa de sessão gráfica"]
fn the_fatia3_worker_triggers_do_not_add_periodic_wakeups() {
    use duplicata_core::canonical::CF_UNICODETEXT;
    use duplicata_core::capture::{CanonicalKind, CanonicalSelection};
    use duplicata_core::{utf16le, CapturedFormat, RawCapture, Timestamp};

    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let cfg = Config::with_paths("db".into(), "logs".into());
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg,
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    let listener_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>;
    let (ready_tx, ready_rx) = mpsc::channel();
    let loop_thread = thread::spawn(move || {
        let cfg = Config::with_paths("db".into(), "logs".into());
        let self_write_filter = Rc::new(SelfWriteFilter::new());
        let listener = ClipboardListener::new(listener_queue, cfg, self_write_filter)
            .expect("registrar o listener real precisa de sessão gráfica");
        let _ = ready_tx.send(message_loop::current_thread_id());
        message_loop::run();
        drop(listener);
    });
    let loop_thread_id = ready_rx.recv().expect("listener deve inicializar");

    let bytes = utf16le("um item");
    let cap = RawCapture::new(
        vec![CapturedFormat {
            format_id: CF_UNICODETEXT,
            format_name: None,
            bytes: bytes.clone(),
        }],
        CanonicalSelection {
            format_id: CF_UNICODETEXT,
            format_name: None,
            kind: CanonicalKind::UnicodeText,
            byte_len: bytes.len() as u64,
        },
        Timestamp::from_millis(1_000),
    );
    queue.push(WorkItem::Capture(cap.clone()), cap.total_bytes as usize);
    queue.push(
        WorkItem::ApplySettings {
            now_ms: 1_000,
            retention_ms: 7 * 24 * 60 * 60 * 1000,
            max_items: 500,
            max_pinned: 25,
        },
        0,
    );
    let (pin_tx, pin_rx) = mpsc::channel();
    queue.push(
        WorkItem::SetPinned {
            clip_id: 1,
            pinned: true,
            done: pin_tx,
        },
        0,
    );
    let (enc_prog, _p) = mpsc::channel();
    let (enc_done_tx, enc_done_rx) = mpsc::channel();
    queue.push(
        WorkItem::ToggleEncryption {
            enable: true,
            progress: enc_prog,
            done: enc_done_tx,
        },
        0,
    );
    let _ = pin_rx.recv();
    let _ = enc_done_rx.recv();
    thread::sleep(Duration::from_millis(200));

    let timer_before = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_before = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_before = counters.processed();
    assert_eq!(processed_before, 1, "só o Capture conta como processado");

    thread::sleep(Duration::from_secs(60));

    let timer_after = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_after = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_after = counters.processed();

    message_loop::post_quit_to(loop_thread_id);
    loop_thread.join().expect("loop não deve travar");
    queue.close();
    worker.join().expect("worker não deve travar");

    assert_no_periodic_wakeups(
        "the_fatia3_worker_triggers_do_not_add_periodic_wakeups",
        timer_before,
        timer_after,
        msgloop_before,
        msgloop_after,
    );
    assert_eq!(
        processed_after, processed_before,
        "a worker volta a dormir depois dos disparos — retenção/ApplySettings/etc. não agendam follow-up"
    );
}

#[test]
#[ignore = "leva ~60s e precisa de sessão gráfica"]
fn opening_settings_arming_and_disarming_the_hotkey_capture_adds_no_periodic_wakeups() {
    use std::cell::Cell;
    use std::sync::mpsc as std_mpsc;
    use windows::Win32::Foundation::{LPARAM, WPARAM};
    use windows::Win32::UI::Input::KeyboardAndMouse::VK_ESCAPE;
    use windows::Win32::UI::WindowsAndMessaging::{SendMessageW, BM_CLICK};

    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let worker_queue = Arc::clone(&queue);
    let worker_counters = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let cfg = Config::with_paths("db".into(), "logs".into());
        let (heuristic_tx, _rx) = mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut repo,
            &cfg,
            worker_counters.as_ref(),
            &heuristic_tx,
        );
    });

    let listener_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<_>>;
    let history_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>;
    let tray_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>;
    let settings_queue = Arc::clone(&queue) as Arc<dyn CaptureQueue<WorkItem>>;
    let (ready_tx, ready_rx) = mpsc::channel();
    let loop_thread = thread::spawn(move || {
        let cfg = Config::with_paths("db".into(), "logs".into());
        let self_write_filter = Rc::new(SelfWriteFilter::new());
        let listener =
            ClipboardListener::new(listener_queue, cfg.clone(), Rc::clone(&self_write_filter))
                .expect("registrar o listener real precisa de sessão gráfica");

        let history_window = HistoryWindow::new(
            FakeHistoryRepository::new(),
            || -> Result<Box<dyn HistoryReader>, StoreError> {
                Ok(Box::new(FakeHistoryRepository::new()))
            },
            || true,
            history_queue,
            PathBuf::from("db"),
            self_write_filter,
        )
        .expect("criar a janela oculta precisa de sessão gráfica");

        let registered_hotkey: Rc<Cell<Option<duplicata_core::HotkeyCombo>>> =
            Rc::new(Cell::new(Some(duplicata_core::HotkeyCombo::DEFAULT)));
        let (_heuristic_tx2, heuristic_rx2) = std_mpsc::channel();
        let tray = duplicata_win::Tray::create(
            history_window.hwnd(),
            Rc::new(std::cell::RefCell::new(cfg.clone())),
            PathBuf::from("config.toml"),
            tray_queue,
            heuristic_rx2,
            Rc::clone(&registered_hotkey),
            "0.0.0-teste",
        )
        .expect("criar o ícone de bandeja precisa de sessão gráfica");

        duplicata_win::settings_dialog::open(
            Rc::new(std::cell::RefCell::new(cfg)),
            PathBuf::from("config.toml"),
            settings_queue,
            history_window.hwnd(),
            registered_hotkey,
        );
        let settings_hwnd = unsafe {
            windows::Win32::UI::WindowsAndMessaging::FindWindowW(
                windows::core::w!("duplicata_settings_dialog"),
                windows::core::w!("Configurações — duplicata"),
            )
        }
        .expect("janela de Configurações deveria existir logo após open()");
        let alter_btn = unsafe {
            windows::Win32::UI::WindowsAndMessaging::GetDlgItem(Some(settings_hwnd), 111)
        }
        .expect("botão \"Alterar\" não encontrado");

        // armar → desarmar (Esc) — síncrono, mesma thread.
        // SAFETY: `alter_btn` acabou de ser localizado, válido.
        unsafe {
            SendMessageW(alter_btn, BM_CLICK, None, None);
            SendMessageW(
                alter_btn,
                windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
                Some(WPARAM(VK_ESCAPE.0 as usize)),
                Some(LPARAM(0)),
            );
        }
        // SAFETY: `settings_hwnd` válido — fecha antes de medir (fechar tudo,
        // como o cenário do T076 pede).
        unsafe {
            let _ = windows::Win32::UI::WindowsAndMessaging::DestroyWindow(settings_hwnd);
        }

        let _ = ready_tx.send(message_loop::current_thread_id());
        message_loop::run();
        drop(history_window);
        drop(listener);
        tray.destroy();
    });
    let loop_thread_id = ready_rx.recv().expect("listener/janela devem inicializar");

    thread::sleep(Duration::from_secs(1));

    let timer_before = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_before = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_before = counters.processed();

    thread::sleep(Duration::from_secs(60));

    let timer_after = MESSAGE_LOOP_TIMER_WAKEUPS.load(Ordering::Relaxed);
    let msgloop_after = MESSAGE_LOOP_ITERATIONS.load(Ordering::Relaxed);
    let processed_after = counters.processed();

    message_loop::post_quit_to(loop_thread_id);
    loop_thread.join().expect("loop não deve travar");
    queue.close();
    worker.join().expect("worker não deve travar");

    assert_no_periodic_wakeups(
        "opening_settings_arming_and_disarming_the_hotkey_capture_adds_no_periodic_wakeups",
        timer_before,
        timer_after,
        msgloop_before,
        msgloop_after,
    );
    assert_eq!(
        processed_after, processed_before,
        "nada disso acorda a worker"
    );
}
