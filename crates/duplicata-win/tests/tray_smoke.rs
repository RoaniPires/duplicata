#![cfg(windows)]

use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::{mpsc, Arc};

use duplicata_core::{CaptureQueue, Config, WorkItem};
use duplicata_win::Tray;

#[test]
#[ignore = "precisa de sessão gráfica com Explorer/bandeja"]
fn create_adds_the_icon_with_no_taskbar_entry_and_destroy_removes_it() {
    let config = Rc::new(RefCell::new(Config::with_paths(
        PathBuf::from("test.db"),
        PathBuf::from("."),
    )));
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(duplicata_core::ByteBudgetQueue::new());
    let (_heuristic_tx, heuristic_rx) = mpsc::channel();
    let tray = Tray::create(
        windows::Win32::Foundation::HWND::default(),
        config,
        PathBuf::from("config.toml"),
        queue,
        heuristic_rx,
        Rc::new(Cell::new(None)),
        "0.0.0-teste",
    )
    .expect("Shell_NotifyIcon(NIM_ADD) deve funcionar numa sessão gráfica");
    println!("ícone de bandeja criado — confira visualmente a bandeja do sistema (e que nada aparece no Alt+Tab)");

    tray.destroy();
    println!("ícone de bandeja destruído (Shell_NotifyIcon(NIM_DELETE)) — confira que sumiu");
}
