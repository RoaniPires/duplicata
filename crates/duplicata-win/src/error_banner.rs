use std::path::Path;

use duplicata_core::StoreError;
use windows::core::HSTRING;
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::WindowsAndMessaging::{MessageBoxW, IDYES, MB_ICONWARNING, MB_YESNO};

const RECREATE_BUTTON_HEIGHT_PX: i32 = 40;
const RECREATE_BUTTON_MARGIN_PX: i32 = 16;

pub struct ErrorBanner {
    pub lines: Vec<String>,
    pub offer_recreate: bool,
}

pub fn banner_for(err: &StoreError, db_path: &Path, recreate_allowed: bool) -> ErrorBanner {
    match err {
        StoreError::Corrupted if recreate_allowed => ErrorBanner {
            lines: vec![
                "O histórico está corrompido e não pode ser lido.".to_string(),
                format!("Arquivo: {}", db_path.display()),
                String::new(),
                "Recriar apaga TODO o histórico atual — não pode ser desfeito.".to_string(),
            ],
            offer_recreate: true,
        },
        StoreError::Corrupted => ErrorBanner {
            lines: vec![
                "O histórico não pôde ser lido agora.".to_string(),
                format!("Arquivo: {}", db_path.display()),
                String::new(),
                "Se você ativou a proteção do banco, reinicie o app — os dados NÃO foram perdidos."
                    .to_string(),
            ],
            offer_recreate: false,
        },
        StoreError::Io | StoreError::Migration | StoreError::Query => ErrorBanner {
            lines: vec![
                "Não foi possível acessar o arquivo do histórico agora.".to_string(),
                format!("Arquivo: {}", db_path.display()),
                String::new(),
                "A captura de novos itens e o Ctrl+V continuam funcionando normalmente."
                    .to_string(),
            ],
            offer_recreate: false,
        },
    }
}

pub fn store_error_kind(err: &StoreError) -> &'static str {
    match err {
        StoreError::Corrupted => "corrupted",
        StoreError::Io => "io",
        StoreError::Migration => "migration",
        StoreError::Query => "query",
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ClickRect {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

pub fn recreate_button_rect(client_width: i32, client_height: i32) -> ClickRect {
    let width = client_width.max(0);
    let height = client_height.max(0);
    let top = (height - RECREATE_BUTTON_HEIGHT_PX).max(0);
    let left = RECREATE_BUTTON_MARGIN_PX.min(width);
    let right = (width - RECREATE_BUTTON_MARGIN_PX).max(left);
    ClickRect {
        left,
        top,
        right,
        bottom: height,
    }
}

pub fn point_in_rect(x: i32, y: i32, r: &ClickRect) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

pub fn confirm_recreate(owner: HWND) -> bool {
    let title = HSTRING::from("Recriar histórico — duplicata");
    let body = HSTRING::from(
        "Isso apaga TODO o histórico atual — todos os itens copiados \
         anteriormente. Esta ação NÃO PODE ser desfeita.\n\nDeseja continuar?",
    );
    // SAFETY: `MessageBoxW` com strings wide válidas e terminadas em NUL
    // (garantido por `HSTRING`); `owner` é a janela de histórico, já criada e
    // visível quando este banner pode aparecer.
    let result = unsafe { MessageBoxW(Some(owner), &body, &title, MB_ICONWARNING | MB_YESNO) };
    result == IDYES
}
