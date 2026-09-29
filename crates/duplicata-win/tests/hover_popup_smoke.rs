#![cfg(windows)]

use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;

use duplicata_core::filter_strip::Rect as CoreRect;
use duplicata_core::hover_preview::HoverPreview;
use duplicata_core::row_layout::ROW_HEIGHT_PX;
use duplicata_core::{
    ByteBudgetQueue, CaptureQueue, ClipListItem, FakeHistoryRepository, HistoryReader,
    SelfWriteFilter, StoreError, StoredClip, WorkItem,
};
use duplicata_win::history_window::HistoryWindow;
use duplicata_win::hover_popup::{HoverPopup, PopupColors, ShowRequest};
use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::HFONT;
use windows::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowRect, IsWindowVisible, SendMessageW, HTTRANSPARENT, WM_NCHITTEST,
};

// FILTER_STRIP_PX + SEARCH_BAR_PX (privados em history_window.rs).
const LIST_TOP_PX: i32 = 34 + 30;
const HOTKEY_ID: usize = 1;
const WM_MOUSEHOVER: u32 = 0x02A1;
const WM_MOUSELEAVE: u32 = 0x02A3;

struct Rows(Vec<ClipListItem>);
impl HistoryReader for Rows {
    fn list_for_display(&self, _limit: usize) -> Result<Vec<ClipListItem>, StoreError> {
        Ok(self.0.clone())
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

fn visible(hwnd: HWND) -> bool {
    // SAFETY: consulta sem pré-condição.
    unsafe { IsWindowVisible(hwnd) }.as_bool()
}

fn window_rect(hwnd: HWND) -> RECT {
    let mut r = RECT::default();
    // SAFETY: `hwnd` desta thread; escreve só `r`.
    let _ = unsafe { GetWindowRect(hwnd, &mut r) };
    r
}

fn send(hwnd: HWND, msg: u32, wparam: usize, lparam: isize) -> isize {
    // SAFETY: `hwnd` criada nesta thread; `SendMessageW` síncrono.
    unsafe { SendMessageW(hwnd, msg, Some(WPARAM(wparam)), Some(LPARAM(lparam))) }.0
}

fn client_point(x: i32, y: i32) -> isize {
    (((y as u32 & 0xFFFF) << 16) | (x as u32 & 0xFFFF)) as isize
}

fn history(rows: Vec<ClipListItem>) -> HistoryWindow {
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    HistoryWindow::new(
        Rows(rows),
        || -> Result<Box<dyn HistoryReader>, StoreError> {
            Ok(Box::new(FakeHistoryRepository::new()))
        },
        || true,
        queue,
        PathBuf::from("test.db"),
        Rc::new(SelfWriteFilter::new()),
    )
    .expect("criar a janela precisa de sessão gráfica")
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn the_popup_shows_without_taking_focus_inside_the_work_area_and_lets_the_mouse_through() {
    let owner = history(Vec::new());
    let popup = HoverPopup::new(owner.hwnd()).expect("criar o popup");
    let preview = HoverPreview {
        lines: vec!["SELECT *".into(), "  FROM t".into(), "\tWHERE x = 1".into()],
        source_cut: false,
    };
    let work = CoreRect {
        left: 0,
        top: 0,
        right: 800,
        bottom: 600,
    };

    // SAFETY: sem pré-condição.
    let foreground_before = unsafe { GetForegroundWindow() };
    let shown = popup.show(
        &preview,
        &ShowRequest {
            font: HFONT::default(),
            colors: PopupColors {
                surface: COLORREF(0x00FFFFFF),
                text: COLORREF(0),
                dim: COLORREF(0x00808080),
                border: COLORREF(0x00C0C0C0),
            },
            dpi: 96,
            // Janela encostada na direita: o popup precisa ir para a esquerda
            // e continuar dentro da área útil.
            window: CoreRect {
                left: 500,
                top: 100,
                right: 790,
                bottom: 500,
            },
            row_top: 580,
            work,
        },
    );
    assert!(shown);
    assert!(visible(popup.hwnd()));
    // SAFETY: sem pré-condição.
    let foreground_after = unsafe { GetForegroundWindow() };
    assert_eq!(
        foreground_after, foreground_before,
        "mostrar o popup não pode tirar o foco de ninguém — o histórico fecharia"
    );

    let r = window_rect(popup.hwnd());
    assert!(
        r.left >= work.left
            && r.right <= work.right
            && r.top >= work.top
            && r.bottom <= work.bottom,
        "popup fora da área útil: {r:?}"
    );
    assert!(
        r.right <= 500,
        "havia espaço à esquerda, não devia cobrir a janela: {r:?}"
    );

    let hit = send(
        popup.hwnd(),
        WM_NCHITTEST,
        0,
        client_point(r.left + 2, r.top + 2),
    );
    assert_eq!(hit, HTTRANSPARENT as isize, "o mouse atravessa o popup");

    popup.hide();
    assert!(!visible(popup.hwnd()));
    popup.hide();
    assert!(!visible(popup.hwnd()), "esconder duas vezes é inócuo");
}

#[test]
#[ignore = "precisa de sessão gráfica; cria janelas reais"]
fn hovering_a_row_shows_the_preview_and_every_dismiss_trigger_hides_it() {
    let sql = "SELECT id, nome\n  FROM cliente\n\tWHERE ativo = 1\nORDER BY nome";
    let window = history(vec![
        text_row(3, sql),
        text_row(2, "curto"),
        text_row(1, "outra linha\ncom quebra"),
    ]);
    let hwnd = window.hwnd();
    let popup = window.hover_popup_hwnd().expect("o histórico cria o popup");
    let row0 = client_point(80, LIST_TOP_PX + ROW_HEIGHT_PX / 2);
    let row1 = client_point(80, LIST_TOP_PX + ROW_HEIGHT_PX + ROW_HEIGHT_PX / 2);

    // Janela fechada: nenhum evento de mouse mostra nada.
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE,
        0,
        row0,
    );
    send(hwnd, WM_MOUSEHOVER, 0, row0);
    assert!(
        !visible(popup),
        "com o histórico fechado o popup nunca aparece"
    );

    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_HOTKEY,
        HOTKEY_ID,
        0,
    );
    assert!(visible(hwnd), "o atalho abre o histórico");

    let hover = |at: isize| {
        send(
            hwnd,
            windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE,
            0,
            at,
        );
        send(hwnd, WM_MOUSEHOVER, 0, at);
    };

    hover(row0);
    assert!(
        visible(popup),
        "hover sobre texto de várias linhas mostra o popup"
    );
    let r = window_rect(popup);
    assert!(
        r.bottom - r.top > ROW_HEIGHT_PX,
        "mostra mais de uma linha: {r:?}"
    );

    // Mudar de linha esconde; a linha nova só aparece depois do próprio hover.
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEMOVE,
        0,
        row1,
    );
    assert!(!visible(popup), "mudar de linha esconde o popup");
    send(hwnd, WM_MOUSEHOVER, 0, row1);
    assert!(
        !visible(popup),
        "item curto que cabe na linha não ganha popup"
    );

    hover(row0);
    assert!(visible(popup));
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_MOUSEWHEEL,
        (-120i16 as u16 as usize) << 16,
        0,
    );
    assert!(!visible(popup), "rolar a lista esconde o popup");

    hover(row0);
    assert!(visible(popup));
    send(hwnd, WM_MOUSELEAVE, 0, 0);
    assert!(!visible(popup), "o mouse sair da janela esconde o popup");

    hover(row0);
    assert!(visible(popup));
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_KEYDOWN,
        windows::Win32::UI::Input::KeyboardAndMouse::VK_DOWN.0 as usize,
        0,
    );
    assert!(!visible(popup), "navegar pelo teclado esconde o popup");

    hover(row0);
    assert!(visible(popup));
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_CHAR,
        'x' as usize,
        0,
    );
    assert!(!visible(popup), "digitar no filtro esconde o popup");
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_CHAR,
        0x7F,
        0,
    );

    hover(row0);
    assert!(visible(popup));
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_LBUTTONDOWN,
        0,
        row1,
    );
    assert!(!visible(popup), "clicar esconde o popup");

    hover(row0);
    assert!(visible(popup));
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_THEMECHANGED,
        0,
        0,
    );
    assert!(
        !visible(popup),
        "trocar o tema esconde o popup antes de a fonte emprestada ser apagada"
    );

    hover(row0);
    assert!(visible(popup));
    send(
        hwnd,
        windows::Win32::UI::WindowsAndMessaging::WM_CLOSE,
        0,
        0,
    );
    assert!(!visible(hwnd));
    assert!(!visible(popup), "fechar o histórico esconde o popup");

    // Um aviso de hover atrasado, com a janela já fechada, não reabre nada.
    send(hwnd, WM_MOUSEHOVER, 0, row0);
    assert!(!visible(popup));
}
