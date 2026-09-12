#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use duplicata_core::{
    ByteBudgetQueue, CaptureQueue, FakeHistoryRepository, HistoryReader, HotkeyCombo,
    SelfWriteFilter, StoreError, WorkItem,
};
use duplicata_win::history_window::HistoryWindow;
use duplicata_win::hotkey_win;

fn new_test_window() -> HistoryWindow {
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

#[test]
#[ignore = "precisa de sessão gráfica; cria HWNDs reais e registra um atalho global de verdade"]
fn a_combo_already_registered_elsewhere_fails_to_register_again() {
    let combo = HotkeyCombo::parse("ctrl+alt+shift+f13")
        .expect("combinação de teste deve parsear (sintaxe válida)");

    let occupier = new_test_window();
    hotkey_win::register(occupier.hwnd(), &combo)
        .expect("registrar a combinação pela primeira vez deve funcionar");

    let history_window = new_test_window();

    let result = hotkey_win::register(history_window.hwnd(), &combo);
    assert!(
        result.is_err(),
        "a mesma combinação já registrada pela janela ocupante deve falhar aqui (FR-004/FR-007)"
    );

    let _ = hotkey_win::unregister(occupier.hwnd());
}
