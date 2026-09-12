use duplicata_core::SelfWriteFilter;
use windows::Win32::Foundation::{HANDLE, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};

pub fn write_all(
    self_write_filter: &SelfWriteFilter,
    formats: &[(u32, Option<String>, Vec<u8>)],
) -> bool {
    self_write_filter.mark_pending();
    write_open_close(self_write_filter, formats)
}

pub fn write_text_only(self_write_filter: &SelfWriteFilter, text_format: &(u32, Vec<u8>)) -> bool {
    self_write_filter.mark_pending();
    let (format_id, bytes) = text_format;
    let single = [(*format_id, None, bytes.clone())];
    write_open_close(self_write_filter, &single)
}

fn write_open_close(
    self_write_filter: &SelfWriteFilter,
    formats: &[(u32, Option<String>, Vec<u8>)],
) -> bool {
    // SAFETY: `OpenClipboard(None)` sem janela-dona; `CloseClipboard` sempre
    // chamado antes de retornar quando a abertura teve sucesso, fechando a
    // MESMA sessão em que `EmptyClipboard`/`SetClipboardData` rodam.
    let ok = unsafe {
        if OpenClipboard(None).is_err() {
            self_write_filter.consume_if_pending();
            return false;
        }
        let ok = write_within_open_session(formats);
        let _ = CloseClipboard();
        ok
    };
    if !ok {
        self_write_filter.consume_if_pending();
    }
    ok
}

/// # Safety
/// Só pode ser chamada com a área de transferência aberta (`OpenClipboard` OK).
unsafe fn write_within_open_session(formats: &[(u32, Option<String>, Vec<u8>)]) -> bool {
    // SAFETY: clipboard aberto (contrato desta função).
    if unsafe { EmptyClipboard() }.is_err() {
        return false;
    }
    for (format_id, _name, bytes) in formats {
        // SAFETY: `hglobal_copy` produz um bloco isolado (cópia própria dos
        // bytes) que passa a ser propriedade do sistema assim que
        // `SetClipboardData` tiver sucesso.
        let Some(hglobal) = (unsafe { hglobal_copy(bytes) }) else {
            return false;
        };
        // SAFETY: clipboard aberto; `hglobal` foi alocado com GMEM_MOVEABLE e
        // já foi destravado (GlobalUnlock) antes desta chamada, como a API exige.
        if unsafe { SetClipboardData(*format_id, Some(HANDLE(hglobal.0))) }.is_err() {
            return false;
        }
    }
    true
}

/// # Safety
/// O `HGLOBAL` devolvido, se `Some`, deve ser entregue a `SetClipboardData`
/// (que passa a possuí-lo) — chamador não deve liberá-lo por conta própria.
unsafe fn hglobal_copy(bytes: &[u8]) -> Option<HGLOBAL> {
    // SAFETY: `dwbytes` vem do próprio `bytes` (mínimo 1 — um bloco de 0
    // bytes não é válido para GlobalLock); `GMEM_MOVEABLE` é exigido pelo
    // clipboard do Windows para dados que o sistema vai possuir depois.
    let hglobal = unsafe { GlobalAlloc(GMEM_MOVEABLE, bytes.len().max(1)) }.ok()?;
    // SAFETY: `hglobal` recém-alocado por nós, ainda não travado.
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` aponta para pelo menos `bytes.len()` bytes válidos
    // enquanto travado — garantido pelo tamanho pedido em `GlobalAlloc` acima.
    unsafe { std::ptr::copy_nonoverlapping(bytes.as_ptr(), ptr.cast::<u8>(), bytes.len()) };
    // SAFETY: pareado com o `GlobalLock` acima.
    let _ = unsafe { GlobalUnlock(hglobal) };
    Some(hglobal)
}
