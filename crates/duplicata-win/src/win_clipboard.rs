use duplicata_core::canonical::{
    decide, CF_DIB, CF_DIBV5, CF_HDROP, CF_LOCALE, CF_OEMTEXT, CF_REGISTERED_FIRST, CF_TEXT,
    CF_UNICODETEXT,
};
use duplicata_core::{
    log_event, CaptureError, CaptureOutcome, CapturedFormat, ClipboardSource, Config, Decision,
    FormatInfo, LogFields,
};
use tracing::Level;
use windows::Win32::Foundation::{CloseHandle, HGLOBAL};
use windows::Win32::System::DataExchange::{
    CloseClipboard, EnumClipboardFormats, GetClipboardData, GetClipboardFormatNameW,
    GetClipboardOwner, OpenClipboard,
};
use windows::Win32::System::Memory::{GlobalLock, GlobalSize, GlobalUnlock};
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

const fn is_known_hglobal_format(format_id: u32) -> bool {
    matches!(
        format_id,
        CF_TEXT | CF_UNICODETEXT | CF_OEMTEXT | CF_DIB | CF_DIBV5 | CF_HDROP | CF_LOCALE
    ) || format_id >= CF_REGISTERED_FIRST
}

#[derive(Debug, Default, Clone, Copy)]
pub struct WinClipboard;

impl ClipboardSource for WinClipboard {
    fn try_capture(&self, cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
        // SAFETY: `OpenClipboard(None)` sem janela-dona; se falhar (outro
        // processo a mantém aberta) retornamos `Busy` sem chamar `CloseClipboard`.
        // Em sucesso, `CloseClipboard` é sempre chamado antes de retornar,
        // fechando a MESMA sessão em que as duas fases rodam.
        unsafe {
            if OpenClipboard(None).is_err() {
                return Err(CaptureError::Busy);
            }
            let outcome = capture_within_open_session(cfg);
            let _ = CloseClipboard();
            outcome
        }
    }
}

struct Handle {
    info: FormatInfo,
    hglobal: HGLOBAL,
}

/// # Safety
/// Só pode ser chamada com a área de transferência aberta (`OpenClipboard` OK).
unsafe fn capture_within_open_session(cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
    // SAFETY: fase 1 — enumera + GlobalSize, sem GlobalLock nem cópia; requer a
    // área de transferência aberta, garantido pelo chamador (`try_capture`).
    let handles = unsafe { enumerate_handles() };
    if handles.is_empty() {
        return Ok(CaptureOutcome::Empty);
    }

    let infos: Vec<FormatInfo> = handles.iter().map(|h| h.info.clone()).collect();
    // SAFETY: mesma sessão de OpenClipboard já aberta pelo chamador
    // (try_capture) — GetClipboardOwner só consulta, não precisa de mais nada.
    let source_program = unsafe { resolve_source_program() };
    match decide(&infos, source_program.as_deref(), cfg) {
        Decision::Reject(reason) => Ok(CaptureOutcome::Rejected(reason)),
        Decision::Copy { canonical_index } => {
            // SAFETY: fase 2 — só agora GlobalLock + cópia, só do que foi
            // aceito; a área de transferência ainda está aberta (mesma sessão).
            let formats = unsafe { copy_all(&handles) };
            Ok(CaptureOutcome::Copied {
                formats,
                canonical_index,
            })
        }
    }
}

/// # Safety
/// Requer a área de transferência aberta (mesma sessão de `enumerate_handles`).
unsafe fn resolve_source_program() -> Option<String> {
    // SAFETY: clipboard aberto (contrato desta função) — GetClipboardOwner só
    // consulta o HWND dono.
    let owner = unsafe { GetClipboardOwner() }.ok()?;

    let mut pid: u32 = 0;
    // SAFETY: `owner` é um HWND válido (GetClipboardOwner só devolve Ok para
    // um HWND não-nulo); `&mut pid` é um ponteiro local válido.
    let _tid = unsafe { GetWindowThreadProcessId(owner, Some(&mut pid)) };
    if pid == 0 {
        return None;
    }

    // SAFETY: `pid` resolvido acima; PROCESS_QUERY_LIMITED_INFORMATION é o
    // direito mínimo que `QueryFullProcessImageNameW` exige.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;

    let mut buf = [0u16; 260];
    let mut len = buf.len() as u32;
    // SAFETY: `process` recém-aberto acima, ainda válido; `buf` tem
    // capacidade `len` informada corretamente à API.
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: fecha o handle aberto por OpenProcess acima, sempre — mesmo se
    // a consulta acima falhou.
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;

    let full_path = String::from_utf16_lossy(&buf[..len as usize]);
    std::path::Path::new(&full_path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
}

/// # Safety
/// Requer a área de transferência aberta.
unsafe fn enumerate_handles() -> Vec<Handle> {
    let mut out = Vec::new();
    // SAFETY: clipboard aberto; `EnumClipboardFormats(0)` inicia a enumeração.
    let mut fmt = unsafe { EnumClipboardFormats(0) };
    while fmt != 0 {
        if !is_known_hglobal_format(fmt) {
            log_event!(
                Level::DEBUG,
                LogFields::new("clipboard_format_ignored_non_hglobal").format_ids([fmt])
            );
            // SAFETY: continua a enumeração a partir do formato anterior.
            fmt = unsafe { EnumClipboardFormats(fmt) };
            continue;
        }
        // SAFETY: `GetClipboardData` com a área aberta; `fmt` já passou pela
        // lista positiva acima, então o handle devolvido É um HGLOBAL — NULL
        // aqui só significa delayed-render que falhou, e omite o formato.
        if let Ok(handle) = unsafe { GetClipboardData(fmt) } {
            let hglobal = HGLOBAL(handle.0);
            // SAFETY: `GlobalSize` num HGLOBAL só consulta o tamanho, não trava
            // nem copia.
            let byte_len = unsafe { GlobalSize(hglobal) } as u64;
            out.push(Handle {
                info: FormatInfo {
                    format_id: fmt,
                    // SAFETY: nome só é consultado para formatos registrados.
                    format_name: unsafe { registered_name(fmt) },
                    byte_len,
                },
                hglobal,
            });
        }
        // SAFETY: continua a enumeração a partir do formato anterior.
        fmt = unsafe { EnumClipboardFormats(fmt) };
    }
    out
}

/// # Safety
/// Requer a área de transferência ainda aberta (mesma sessão de
/// `enumerate_handles`) e os handles ainda válidos.
unsafe fn copy_all(handles: &[Handle]) -> Vec<CapturedFormat> {
    handles
        .iter()
        // SAFETY: `h.hglobal` veio de `GetClipboardData` nesta mesma sessão,
        // ainda aberta.
        .filter_map(|h| unsafe { copy_bytes(h.hglobal) }.map(|bytes| (h, bytes)))
        .map(|(h, bytes)| CapturedFormat {
            format_id: h.info.format_id,
            format_name: h.info.format_name.clone(),
            bytes,
        })
        .collect()
}

/// # Safety
/// `hglobal` deve vir de `GetClipboardData` na sessão de clipboard ainda
/// aberta, para um `format_id` que passou por [`is_known_hglobal_format`]
/// (só esses chegam a [`Handle`]/`enumerate_handles`) — `hglobal` aqui É
/// garantidamente um `HGLOBAL` de verdade, nunca um `HBITMAP`/`HPALETTE`/etc.
unsafe fn copy_bytes(hglobal: HGLOBAL) -> Option<Vec<u8>> {
    // SAFETY: `GlobalSize` num HGLOBAL de verdade (ver contrato da função).
    let size = unsafe { GlobalSize(hglobal) };
    if size == 0 {
        return Some(Vec::new());
    }
    // SAFETY: `GlobalLock` num HGLOBAL; NULL se não for lockável.
    let ptr = unsafe { GlobalLock(hglobal) };
    if ptr.is_null() {
        return None;
    }
    // SAFETY: `ptr` aponta para `size` bytes válidos enquanto travado.
    let bytes = unsafe { std::slice::from_raw_parts(ptr.cast::<u8>(), size) }.to_vec();
    // SAFETY: pareado com o `GlobalLock` acima.
    let _ = unsafe { GlobalUnlock(hglobal) };
    Some(bytes)
}

/// # Safety
/// `fmt` deve ser um id de formato válido.
unsafe fn registered_name(fmt: u32) -> Option<String> {
    if fmt < CF_REGISTERED_FIRST {
        return None;
    }
    let mut buf = [0u16; 256];
    // SAFETY: buffer local com capacidade informada; a função escreve no máximo
    // `buf.len()` unidades e devolve quantas escreveu (0 em erro).
    let written = unsafe { GetClipboardFormatNameW(fmt, &mut buf) };
    if written <= 0 {
        return None;
    }
    Some(String::from_utf16_lossy(&buf[..written as usize]))
}

#[cfg(test)]
mod tests {
    use super::is_known_hglobal_format;
    use duplicata_core::canonical::{
        CF_DIB, CF_DIBV5, CF_HDROP, CF_LOCALE, CF_OEMTEXT, CF_REGISTERED_FIRST, CF_TEXT,
        CF_UNICODETEXT,
    };

    const CF_BITMAP: u32 = 2;
    const CF_METAFILEPICT: u32 = 3;
    const CF_PALETTE: u32 = 9;
    const CF_ENHMETAFILE: u32 = 14;

    #[test]
    fn all_seven_standard_hglobal_formats_are_recognized() {
        for id in [
            CF_TEXT,
            CF_UNICODETEXT,
            CF_OEMTEXT,
            CF_DIB,
            CF_DIBV5,
            CF_HDROP,
            CF_LOCALE,
        ] {
            assert!(is_known_hglobal_format(id), "id {id} deveria ser HGLOBAL");
        }
    }

    #[test]
    fn known_non_hglobal_standard_formats_are_rejected() {
        for id in [CF_BITMAP, CF_METAFILEPICT, CF_PALETTE, CF_ENHMETAFILE] {
            assert!(
                !is_known_hglobal_format(id),
                "id {id} NÃO é HGLOBAL — nunca deve ir a GlobalSize/GlobalLock"
            );
        }
    }

    #[test]
    fn any_registered_format_id_is_treated_as_hglobal() {
        assert!(is_known_hglobal_format(CF_REGISTERED_FIRST));
        assert!(is_known_hglobal_format(CF_REGISTERED_FIRST + 1));
        assert!(is_known_hglobal_format(u32::MAX));
    }

    #[test]
    fn an_unknown_standard_format_below_the_registered_range_is_rejected() {
        assert!(!is_known_hglobal_format(11));
        assert!(!is_known_hglobal_format(CF_REGISTERED_FIRST - 1));
    }
}
