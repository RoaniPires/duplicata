use duplicata_core::canonical::{
    gate_size, screen, CF_DIB, CF_DIBV5, CF_HDROP, CF_LOCALE, CF_OEMTEXT, CF_REGISTERED_FIRST,
    CF_TEXT, CF_UNICODETEXT,
};
use duplicata_core::{
    copied_or_empty, log_event, CaptureError, CaptureOutcome, CapturedFormat, ClipboardSource,
    Config, Decision, FormatAnnounce, LogFields, RejectReason,
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
        // fechando a MESMA sessão em que as três fases rodam.
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

/// # Safety
/// Só pode ser chamada com a área de transferência aberta (`OpenClipboard` OK).
///
/// As três fases são, nesta ordem e sem exceção:
///
/// 1. **Anúncio** — `EnumClipboardFormats` + `GetClipboardFormatNameW`. Nenhuma
///    dessas obtém handle, e portanto nenhuma faz o Windows mandar
///    `WM_RENDERFORMAT` ao dono.
/// 2. **Filtros** — `screen` decide só com id, nome e programa de origem.
/// 3. **Handles e bytes** — só aqui `GetClipboardData` aparece.
///
/// A ordem não é estética. Duas coisas dependem dela:
///
/// - **Medido.** Um dono com renderização adiada (`SetClipboardData(fmt,
///   NULL)`) só produz os bytes quando alguém chama `GetClipboardData`. Chamar
///   isso na fase de anúncio obrigava toda origem a materializar o conteúdo
///   antes de os filtros de privacidade opinarem — inclusive uma origem que
///   sinalizou `ExcludeClipboardContentFromMonitorProcessing` ou um programa da
///   lista de bloqueio. O teste
///   `a_rejected_capture_never_asks_a_delayed_render_owner_to_produce_the_bytes`
///   prende isso: com o desenho antigo saíam 3 pedidos de renderização numa
///   captura recusada; agora saem 0.
/// - **Contrato, não sintoma observado.** O Win32 não permite modificar o
///   clipboard durante a enumeração, e `GetClipboardData` dentro do laço de
///   `EnumClipboardFormats` fazia exatamente isso, via o `SetClipboardData` com
///   que o dono responde ao `WM_RENDERFORMAT`. Numa medição direta (ver
///   `a_delayed_render_owner_with_two_formats_is_captured_whole`) o desenho
///   antigo NÃO truncou a enumeração — então isto é conformidade com a regra,
///   não a explicação de um defeito reproduzido.
unsafe fn capture_within_open_session(cfg: &Config) -> Result<CaptureOutcome, CaptureError> {
    // SAFETY: fase 1 — só enumeração e nome; requer a área de transferência
    // aberta, garantido pelo chamador (`try_capture`).
    let announces = unsafe { announce_formats() };
    if announces.is_empty() {
        return Ok(CaptureOutcome::Empty);
    }

    // SAFETY: mesma sessão de OpenClipboard já aberta pelo chamador
    // (try_capture) — GetClipboardOwner só consulta, não precisa de mais nada.
    let source_program = unsafe { resolve_source_program() };

    // Fase 2 — filtros de exclusão + escolha do canônico, sem um único handle.
    let canonical_format_id = match screen(&announces, source_program.as_deref(), cfg) {
        Decision::Reject(reason) => return Ok(CaptureOutcome::Rejected(reason)),
        Decision::Copy { canonical_index } => announces[canonical_index].format_id,
    };

    // SAFETY: fase 3 — só agora os handles, com a enumeração já encerrada e os
    // filtros já aplicados; a área de transferência continua aberta (mesma
    // sessão).
    match unsafe { acquire_and_copy(&announces, canonical_format_id, cfg) } {
        Err(reason) => Ok(CaptureOutcome::Rejected(reason)),
        Ok(formats) => Ok(copied_or_empty(formats, canonical_format_id)),
    }
}

/// # Safety
/// Requer a área de transferência aberta (contrato de
/// [`capture_within_open_session`]).
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
///
/// **Nenhuma chamada daqui pode obter handle nem modificar o clipboard.**
/// `EnumClipboardFormats` e `GetClipboardFormatNameW` cumprem isso; o que
/// materializa bytes (`GetClipboardData`) pertence a [`acquire_handles`],
/// depois dos filtros.
unsafe fn announce_formats() -> Vec<FormatAnnounce> {
    let mut out = Vec::new();
    // SAFETY: clipboard aberto; `EnumClipboardFormats(0)` inicia a enumeração.
    let mut fmt = unsafe { EnumClipboardFormats(0) };
    while fmt != 0 {
        if is_known_hglobal_format(fmt) {
            out.push(FormatAnnounce {
                format_id: fmt,
                // SAFETY: nome só é consultado para formatos registrados.
                format_name: unsafe { registered_name(fmt) },
            });
        } else {
            log_event!(
                Level::DEBUG,
                LogFields::new("clipboard_format_ignored_non_hglobal").format_ids([fmt])
            );
        }
        // SAFETY: continua a enumeração a partir do formato anterior.
        fmt = unsafe { EnumClipboardFormats(fmt) };
    }
    out
}

/// Obtém, mede e copia cada formato — um de cada vez.
///
/// # Safety
/// Requer a área de transferência ainda aberta (mesma sessão de
/// [`announce_formats`]) e a enumeração já encerrada — um `SetClipboardData`
/// disparado por `WM_RENDERFORMAT` daqui não pode cair no meio dela.
///
/// **Nenhum handle atravessa outra chamada a `GetClipboardData`**, e é por isso
/// que obter e copiar são o mesmo passo. O handle devolvido pertence ao
/// clipboard, não a nós, e deixa de valer quando o dado daquele formato é
/// reposto — e `GetClipboardData` é justamente o que intima o dono a repor, via
/// `WM_RENDERFORMAT`. Colher todos os handles antes de copiar abriria essa
/// janela justamente sobre a classe de dono que esta função passou a atender.
///
/// O `Vec` sai na ordem de enunciação, que é a prioridade de formato que
/// `clipboard_restore` republica e que quem cola enxerga.
unsafe fn acquire_and_copy(
    announces: &[FormatAnnounce],
    canonical_format_id: u32,
    cfg: &Config,
) -> Result<Vec<CapturedFormat>, RejectReason> {
    let mut out = Vec::with_capacity(announces.len());
    let mut undelivered = Vec::new();
    for announce in announces {
        // SAFETY: `GetClipboardData` com a área aberta; `format_id` já passou
        // pela lista positiva em `announce_formats`, então o handle devolvido É
        // um HGLOBAL. Falha aqui é formato que o dono não entregou (renderização
        // adiada que não veio) — omitimos o formato.
        let Ok(handle) = (unsafe { GetClipboardData(announce.format_id) }) else {
            undelivered.push(announce.format_id);
            continue;
        };
        let hglobal = HGLOBAL(handle.0);
        // SAFETY: `GlobalSize` num HGLOBAL só consulta o tamanho, não trava
        // nem copia.
        let byte_len = unsafe { GlobalSize(hglobal) };

        // O limite roda com o handle na mão mas antes do `GlobalLock` do
        // canônico: um item grande demais é recusado sem que os bytes dele
        // sejam copiados para memória própria.
        if announce.format_id == canonical_format_id {
            if let Some(reason) = gate_size(canonical_format_id, byte_len as u64, cfg) {
                return Err(reason);
            }
        }

        // SAFETY: `hglobal` acabou de vir de `GetClipboardData` e nada entre
        // aquela chamada e esta pôde invalidá-lo.
        let Some(bytes) = (unsafe { copy_bytes(hglobal, byte_len) }) else {
            undelivered.push(announce.format_id);
            continue;
        };
        out.push(CapturedFormat {
            format_id: announce.format_id,
            format_name: announce.format_name.clone(),
            bytes,
        });
    }
    if !undelivered.is_empty() {
        log_event!(
            Level::WARN,
            LogFields::new("clipboard_format_not_delivered").format_ids(undelivered)
        );
    }
    Ok(out)
}

/// # Safety
/// `hglobal` deve vir da chamada a `GetClipboardData` imediatamente anterior,
/// na sessão de clipboard ainda aberta, para um `format_id` que passou por
/// [`is_known_hglobal_format`] (só esses chegam a [`FormatAnnounce`]) —
/// `hglobal` aqui É garantidamente um `HGLOBAL` de verdade, nunca um
/// `HBITMAP`/`HPALETTE`/etc. `size` deve ser o `GlobalSize` desse mesmo handle.
unsafe fn copy_bytes(hglobal: HGLOBAL, size: usize) -> Option<Vec<u8>> {
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
