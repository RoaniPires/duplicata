//! Rastreador passivo da área de transferência.
//!
//! `cargo run -p duplicata-win --example clipboard_trace`
//!
//! Imprime uma linha por `WM_CLIPBOARDUPDATE`: quando chegou, o número de
//! sequência, quem é o dono (classe da janela, pid e exe), quem está com a área
//! de transferência aberta naquele instante e quais formatos o dono anunciou.
//!
//! **Nunca abre a área de transferência** (sem `OpenClipboard`, sem
//! `GetClipboardData`, sem `EnumClipboardFormats`): os formatos vêm de
//! `GetUpdatedClipboardFormats`, que não exige a área aberta. Assim ele não
//! disputa o recurso com o duplicata nem com quem cola, e serve de testemunha
//! com o duplicata ligado ou desligado. Nenhum byte de conteúdo é lido nem
//! impresso.
//!
//! Sem timer: só o laço `GetMessageW`, que dorme até chegar mensagem. Ctrl+C
//! encerra o processo; o Windows remove o ouvinte junto com a janela.

#![cfg(windows)]

use std::io::Write;
use std::sync::OnceLock;
use std::time::Instant;

use windows::core::PCWSTR;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, LRESULT, WPARAM};
use windows::Win32::System::DataExchange::{
    AddClipboardFormatListener, GetClipboardFormatNameW, GetClipboardOwner,
    GetClipboardSequenceNumber, GetOpenClipboardWindow, GetUpdatedClipboardFormats,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::System::Threading::{
    OpenProcess, QueryFullProcessImageNameW, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
};
use windows::Win32::UI::WindowsAndMessaging::{
    CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClassNameW, GetMessageW,
    GetWindowThreadProcessId, RegisterClassW, TranslateMessage, CW_USEDEFAULT, HWND_MESSAGE, MSG,
    WINDOW_EX_STYLE, WINDOW_STYLE, WM_CLIPBOARDUPDATE, WNDCLASSW,
};

const CLASS_NAME: PCWSTR = windows::core::w!("duplicata_clipboard_trace");

/// Classe da janela que o OLE (`OleSetClipboard`) usa como dona.
const OLE_CLASS: &str = "CLIPBRDWNDCLASS";

static START: OnceLock<Instant> = OnceLock::new();

fn main() {
    START.get_or_init(Instant::now);

    // SAFETY: registro de classe com `wndproc` válido; o erro de "classe já
    // registrada" não ocorre (processo próprio) e, se ocorresse, o
    // `CreateWindowExW` abaixo falharia e encerraríamos com mensagem.
    let hwnd = unsafe {
        let Ok(hinstance) = GetModuleHandleW(None) else {
            eprintln!("erro: GetModuleHandleW falhou");
            std::process::exit(1);
        };
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: hinstance.into(),
            lpszClassName: CLASS_NAME,
            ..Default::default()
        };
        RegisterClassW(&wc);
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            PCWSTR::null(),
            WINDOW_STYLE::default(),
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            CW_USEDEFAULT,
            Some(HWND_MESSAGE),
            None,
            Some(hinstance.into()),
            None,
        )
    };
    let Ok(hwnd) = hwnd else {
        eprintln!("erro: não foi possível criar a janela message-only");
        std::process::exit(1);
    };

    // SAFETY: `hwnd` acabou de ser criada nesta thread e é válida.
    if unsafe { AddClipboardFormatListener(hwnd) }.is_err() {
        eprintln!("erro: AddClipboardFormatListener falhou");
        std::process::exit(1);
    }

    // SAFETY: só lê um contador global do sistema.
    let seq = unsafe { GetClipboardSequenceNumber() };
    emit(&format!(
        "# clipboard_trace: ouvindo (seq inicial={seq}); nunca abre a área de transferência; Ctrl+C encerra"
    ));

    let mut msg = MSG::default();
    // SAFETY: laço de mensagens padrão com um `MSG` local válido;
    // `GetMessageW` devolve 0 em `WM_QUIT` e -1 em erro (ambos encerram).
    unsafe {
        while GetMessageW(&mut msg, None, 0, 0).0 > 0 {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
}

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if msg == WM_CLIPBOARDUPDATE {
        emit(&trace_line());
        return LRESULT(0);
    }
    // SAFETY: repassa a mensagem não tratada ao procedimento padrão, com os
    // mesmos argumentos recebidos do sistema.
    unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
}

/// Escreve a linha e esvazia o buffer na hora: a saída costuma ir para um
/// pipe (`| Tee-Object`), e uma linha presa no buffer some no Ctrl+C.
fn emit(line: &str) {
    let mut out = std::io::stdout().lock();
    let _ = writeln!(out, "{line}");
    let _ = out.flush();
}

fn trace_line() -> String {
    let elapsed_ms = START.get().map_or(0, |s| s.elapsed().as_millis());
    // SAFETY: as três consultas só leem estado global da área de
    // transferência; nenhuma exige que ela esteja aberta nem a abre.
    let (seq, owner, open) = unsafe {
        (
            GetClipboardSequenceNumber(),
            GetClipboardOwner().ok(),
            GetOpenClipboardWindow().ok(),
        )
    };
    format!(
        "t={elapsed_ms}ms seq={seq} dono={} aberta_por={} formatos=[{}]",
        describe_window(owner),
        describe_window(open),
        describe_formats(&updated_formats()),
    )
}

/// `hwnd=0x… classe="…"[ (OLE)] pid=… exe=…`, ou `nenhuma`.
fn describe_window(hwnd: Option<HWND>) -> String {
    let Some(hwnd) = hwnd.filter(|h| !h.is_invalid()) else {
        return "nenhuma".to_string();
    };
    let class = class_name(hwnd).unwrap_or_else(|| "?".to_string());
    let ole = if class == OLE_CLASS { " (OLE)" } else { "" };
    let mut pid: u32 = 0;
    // SAFETY: `hwnd` não-nulo vindo do sistema; `&mut pid` é local. Se a
    // janela já morreu, a função devolve 0 e `pid` fica 0.
    let _tid = unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
    let exe = if pid == 0 {
        "?".to_string()
    } else {
        exe_name(pid).unwrap_or_else(|| "?".to_string())
    };
    format!(
        "hwnd={:#x} classe=\"{class}\"{ole} pid={pid} exe={exe}",
        hwnd.0 as usize
    )
}

fn class_name(hwnd: HWND) -> Option<String> {
    let mut buf = [0u16; 256];
    // SAFETY: buffer local com a capacidade informada pelo slice; devolve
    // quantas unidades escreveu (0 em erro).
    let written = unsafe { GetClassNameW(hwnd, &mut buf) };
    let written = usize::try_from(written).ok().filter(|&n| n > 0)?;
    Some(String::from_utf16_lossy(&buf[..written]))
}

/// Nome do exe do processo, pela mesma técnica de `resolve_source_program`
/// em `win_clipboard.rs`.
fn exe_name(pid: u32) -> Option<String> {
    // SAFETY: PROCESS_QUERY_LIMITED_INFORMATION é o direito mínimo que
    // `QueryFullProcessImageNameW` exige; falha vira `None`.
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) }.ok()?;
    let mut buf = [0u16; 260];
    let mut len = buf.len() as u32;
    // SAFETY: `process` recém-aberto e válido; `buf` tem a capacidade `len`
    // informada à API.
    let result = unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            windows::core::PWSTR(buf.as_mut_ptr()),
            &mut len,
        )
    };
    // SAFETY: fecha o handle aberto acima, sempre, mesmo se a consulta falhou.
    let _ = unsafe { CloseHandle(process) };
    result.ok()?;
    let full_path = String::from_utf16_lossy(&buf[..len as usize]);
    std::path::Path::new(&full_path)
        .file_name()
        .map(|f| f.to_string_lossy().into_owned())
}

/// Ids dos formatos da área de transferência, sem abri-la.
fn updated_formats() -> Vec<u32> {
    let mut buf = vec![0u32; 64];
    loop {
        let mut count: u32 = 0;
        // SAFETY: `buf` é um slice válido com a capacidade informada; `count`
        // é local. Com buffer pequeno, a função falha e põe em `count` quantos
        // formatos existem, e tentamos de novo com esse tamanho.
        let result = unsafe { GetUpdatedClipboardFormats(&mut buf, &mut count) };
        let count = count as usize;
        match result {
            Ok(()) => {
                buf.truncate(count.min(buf.len()));
                return buf;
            }
            Err(_) if count > buf.len() => buf.resize(count, 0),
            Err(_) => return Vec::new(),
        }
    }
}

fn describe_formats(ids: &[u32]) -> String {
    ids.iter()
        .map(|&id| match format_name(id) {
            Some(name) => format!("{id} {name}"),
            None => id.to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_name(id: u32) -> Option<String> {
    let standard = match id {
        1 => "CF_TEXT",
        2 => "CF_BITMAP",
        3 => "CF_METAFILEPICT",
        4 => "CF_SYLK",
        5 => "CF_DIF",
        6 => "CF_TIFF",
        7 => "CF_OEMTEXT",
        8 => "CF_DIB",
        9 => "CF_PALETTE",
        10 => "CF_PENDATA",
        11 => "CF_RIFF",
        12 => "CF_WAVE",
        13 => "CF_UNICODETEXT",
        14 => "CF_ENHMETAFILE",
        15 => "CF_HDROP",
        16 => "CF_LOCALE",
        17 => "CF_DIBV5",
        0x80 => "CF_OWNERDISPLAY",
        0x81 => "CF_DSPTEXT",
        0x82 => "CF_DSPBITMAP",
        0x83 => "CF_DSPMETAFILEPICT",
        0x8E => "CF_DSPENHMETAFILE",
        0x200..=0x2FF => "CF_PRIVATE",
        0x300..=0x3FF => "CF_GDIOBJ",
        _ => {
            let mut buf = [0u16; 256];
            // SAFETY: buffer local com a capacidade informada pelo slice; só
            // consulta a tabela de formatos registrados, sem abrir a área de
            // transferência. Devolve 0 para id não registrado.
            let written = unsafe { GetClipboardFormatNameW(id, &mut buf) };
            let written = usize::try_from(written).ok().filter(|&n| n > 0)?;
            return Some(format!("\"{}\"", String::from_utf16_lossy(&buf[..written])));
        }
    };
    Some(standard.to_string())
}
