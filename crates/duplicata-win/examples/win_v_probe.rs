//! Sonda do Win+V: diz se este processo consegue registrar o Win+V agora.
//!
//! `cargo run -p duplicata-win --example win_v_probe [-- --esperar]`
//!
//! Serve à verificação manual que precede a opção "Abrir com Win+V": incluir
//! `V` no `DisabledHotkeys` do Explorer, reiniciar e conferir se o
//! `RegisterHotKey` passa a aceitar o Win+V.
//!
//! 1. Imprime o valor atual de `DisabledHotkeys` em
//!    `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced`
//!    (ou "ausente"). **Só lê**: nunca grava no registro.
//! 2. Chama `RegisterHotKey(None, 1, MOD_WIN, 'V')` e imprime "ok" ou o código
//!    de erro (1409 = `ERROR_HOTKEY_ALREADY_REGISTERED`, o Explorer é o dono).
//! 3. Com `--esperar`, e só se o registro deu certo, dorme em `GetMessageW` até
//!    chegar um `WM_HOTKEY`, imprime "Win+V recebido", desregistra e sai.
//!
//! Sem timer e sem gancho de teclado: só o `GetMessageW`, que dorme até chegar
//! mensagem. Ctrl+C encerra; o Windows solta o atalho junto com o processo.

#![cfg(windows)]

use core::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{RegGetValueW, HKEY_CURRENT_USER, RRF_RT_REG_SZ};
use windows::Win32::UI::Input::KeyboardAndMouse::{RegisterHotKey, UnregisterHotKey, MOD_WIN};
use windows::Win32::UI::WindowsAndMessaging::{GetMessageW, MSG, WM_HOTKEY};

const ADVANCED_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced");
const DISABLED_HOTKEYS: PCWSTR = w!("DisabledHotkeys");
const HOTKEY_ID: i32 = 1;
const VK_V: u32 = b'V' as u32;

fn main() {
    let esperar = std::env::args().skip(1).any(|a| a == "--esperar");

    println!("DisabledHotkeys: {}", disabled_hotkeys());

    // SAFETY: sem janela (`None`), o `WM_HOTKEY` vai para a fila desta thread;
    // `RegisterHotKey` não tem outra pré-condição.
    let registered = unsafe { RegisterHotKey(None, HOTKEY_ID, MOD_WIN, VK_V) };
    match registered {
        Ok(()) => println!("RegisterHotKey(Win+V): ok"),
        Err(e) => {
            let code = WIN32_ERROR::from_error(&e).map_or(e.code().0 as u32, |c| c.0);
            println!(
                "RegisterHotKey(Win+V): falhou, erro {code} ({})",
                e.message()
            );
            std::process::exit(1);
        }
    }

    if esperar {
        println!("aguardando Win+V (Ctrl+C encerra)...");
        if wait_hotkey() {
            println!("Win+V recebido");
        } else {
            eprintln!("erro: o laço de mensagens terminou sem receber o Win+V");
        }
    }

    // SAFETY: desregistra o id registrado acima por esta mesma thread.
    if unsafe { UnregisterHotKey(None, HOTKEY_ID) }.is_err() {
        eprintln!("aviso: UnregisterHotKey falhou; o Windows solta o atalho ao sair");
    }
}

/// Dorme em `GetMessageW` até um `WM_HOTKEY` do nosso id. `false` se o laço
/// terminar antes (`WM_QUIT` ou erro).
fn wait_hotkey() -> bool {
    let mut msg = MSG::default();
    // SAFETY: laço de mensagens com um `MSG` local válido; `GetMessageW`
    // devolve 0 em `WM_QUIT` e -1 em erro (ambos encerram).
    while unsafe { GetMessageW(&mut msg, None, 0, 0) }.0 > 0 {
        if msg.message == WM_HOTKEY && msg.wParam.0 == HOTKEY_ID as usize {
            return true;
        }
    }
    false
}

/// O valor entre aspas, `ausente`, ou o código de erro da leitura.
fn disabled_hotkeys() -> String {
    let mut size: u32 = 0;
    // SAFETY: primeira chamada só descobre o tamanho (pvData=None, idiom
    // documentado do RegGetValueW); não lê conteúdo nem escreve no registro.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            ADVANCED_KEY,
            DISABLED_HOTKEYS,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if err == ERROR_FILE_NOT_FOUND {
        return "ausente".to_string();
    }
    if err.is_err() {
        return format!("erro de leitura {}", err.0);
    }
    let mut buf = vec![0u16; (size as usize).div_ceil(2)];
    // SAFETY: `buf` tem pelo menos `size` bytes de capacidade, informado a
    // `pcbData`; a API escreve no máximo isso e termina a cadeia em NUL.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            ADVANCED_KEY,
            DISABLED_HOTKEYS,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    if err.is_err() {
        return format!("erro de leitura {}", err.0);
    }
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    format!("\"{}\"", String::from_utf16_lossy(&buf[..len]))
}
