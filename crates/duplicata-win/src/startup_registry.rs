use core::ffi::c_void;

use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::ERROR_FILE_NOT_FOUND;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ,
    RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
};

const VALUE_NAME: PCWSTR = w!("duplicata");
const RUN_KEY: PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const STARTUP_APPROVED_KEY: PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");

pub fn run_entry_present() -> bool {
    // SAFETY: só consulta se o valor existe (pvData/pcbData nulos — idiom
    // documentado do RegGetValueW para checar presença sem ler conteúdo);
    // não escreve nada. `RRF_RT_REG_SZ` restringe ao tipo que o instalador
    // e este módulo sempre gravam.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            None,
        )
    };
    err.is_ok()
}

pub fn write_run_entry(exe_path: &str) -> bool {
    let wide: Vec<u16> = exe_path.encode_utf16().chain(std::iter::once(0)).collect();
    let bytes = (wide.len() * 2) as u32;
    // SAFETY: `wide` é um buffer UTF-16 terminado em NUL, vivo durante esta
    // chamada síncrona; `bytes` é o tamanho exato em bytes (incluindo o
    // terminador), como `REG_SZ` exige. `RegSetKeyValueW` cria a chave
    // `Run` se ela ainda não existir.
    let err = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            REG_SZ.0,
            Some(wide.as_ptr() as *const c_void),
            bytes,
        )
    };
    err.is_ok()
}

pub fn remove_run_entry() -> bool {
    // SAFETY: `RegDeleteKeyValueW` é seguro mesmo se o valor não existir —
    // devolve `ERROR_FILE_NOT_FOUND`, tratado como sucesso abaixo.
    let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, RUN_KEY, VALUE_NAME) };
    err.is_ok() || err == ERROR_FILE_NOT_FOUND
}

pub fn read_startup_approved_raw() -> Option<Vec<u8>> {
    let mut size: u32 = 0;
    // SAFETY: primeira chamada só descobre o tamanho (pvData=None é o
    // idiom documentado do RegGetValueW para isso); não lê conteúdo ainda.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            STARTUP_APPROVED_KEY,
            VALUE_NAME,
            RRF_RT_REG_BINARY,
            None,
            None,
            Some(&mut size),
        )
    };
    if !err.is_ok() || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: `buf` tem exatamente `size` bytes de capacidade, informado a
    // `pcbData`; a API escreve no máximo isso, nunca além do buffer.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            STARTUP_APPROVED_KEY,
            VALUE_NAME,
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr() as *mut c_void),
            Some(&mut size),
        )
    };
    err.is_ok().then_some(buf)
}

pub fn clear_startup_approval() -> bool {
    // SAFETY: idempotente — devolve `ERROR_FILE_NOT_FOUND` se já não
    // existir, tratado como sucesso abaixo.
    let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, STARTUP_APPROVED_KEY, VALUE_NAME) };
    err.is_ok() || err == ERROR_FILE_NOT_FOUND
}
