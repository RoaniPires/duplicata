use windows::core::PCWSTR;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    LoadImageW, HICON, IMAGE_ICON, LR_DEFAULTSIZE, LR_SHARED,
};

const APP_ICON_RESOURCE_ID: u16 = 1;

pub fn load_app_icon() -> Option<HICON> {
    // SAFETY: `GetModuleHandleW(None)` pede o módulo do processo atual — sem
    // pré-condição.
    let hinstance = unsafe { GetModuleHandleW(None) }.ok()?;
    // SAFETY: `hinstance` válido (linha acima). `APP_ICON_RESOURCE_ID` é um
    // nameID de recurso, não um ponteiro de string — `PCWSTR(id as _)` é o
    // idiom padrão de `MAKEINTRESOURCEW`, mesmo usado pelas constantes
    // `IDC_*` desta mesma crate `windows`. `LR_SHARED`: o handle devolvido é
    // gerenciado pelo sistema (mesmo contrato do `LoadIconW(None,
    // IDI_APPLICATION)` que esta função substitui) — nunca chamamos
    // `DestroyIcon` sobre ele, e chamar esta função de mais de um lugar
    // (bandeja + janela de histórico) devolve o mesmo handle compartilhado
    // em vez de vazar uma cópia por chamada.
    let handle = unsafe {
        LoadImageW(
            Some(hinstance.into()),
            PCWSTR(APP_ICON_RESOURCE_ID as _),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
    }
    .ok()?;
    Some(HICON(handle.0))
}
