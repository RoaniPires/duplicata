#![cfg(windows)]

use windows::core::w;
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_BINARY,
    RRF_RT_REG_BINARY, RRF_RT_REG_SZ,
};

use duplicata_win::startup_registry::{
    clear_startup_approval, read_startup_approved_raw, remove_run_entry, run_entry_present,
    write_run_entry,
};

const RUN_KEY: windows::core::PCWSTR = w!(r"Software\Microsoft\Windows\CurrentVersion\Run");
const STARTUP_APPROVED_KEY: windows::core::PCWSTR =
    w!(r"Software\Microsoft\Windows\CurrentVersion\Explorer\StartupApproved\Run");
const VALUE_NAME: windows::core::PCWSTR = w!("duplicata");

fn raw_run_value_for_fixture() -> Option<Vec<u16>> {
    let mut size: u32 = 0;
    // SAFETY: só descobre o tamanho, não escreve nada.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            None,
            Some(&mut size),
        )
    };
    if !err.is_ok() || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size as usize];
    // SAFETY: `buf` tem exatamente `size` bytes, informado a `pcbData`.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            RRF_RT_REG_SZ,
            None,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut size),
        )
    };
    if !err.is_ok() {
        return None;
    }
    Some(
        buf.chunks_exact(2)
            .map(|c| u16::from_le_bytes([c[0], c[1]]))
            .collect(),
    )
}

fn restore_raw_run_value(value: &[u16]) {
    let bytes = value.len() * 2;
    // SAFETY: `value` (UTF-16, já NUL-terminado por vir direto do
    // registro) vivo durante a chamada síncrona.
    let err = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            RUN_KEY,
            VALUE_NAME,
            windows::Win32::System::Registry::REG_SZ.0,
            Some(value.as_ptr() as *const core::ffi::c_void),
            bytes as u32,
        )
    };
    assert!(err.is_ok(), "fixture: falha ao restaurar Run");
}

fn raw_startup_approved_for_fixture() -> Option<Vec<u8>> {
    let mut size: u32 = 0;
    // SAFETY: só descobre o tamanho, não escreve nada.
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
    // SAFETY: `buf` tem exatamente `size` bytes, informado a `pcbData`.
    let err = unsafe {
        RegGetValueW(
            HKEY_CURRENT_USER,
            STARTUP_APPROVED_KEY,
            VALUE_NAME,
            RRF_RT_REG_BINARY,
            None,
            Some(buf.as_mut_ptr() as *mut core::ffi::c_void),
            Some(&mut size),
        )
    };
    err.is_ok().then_some(buf)
}

fn write_raw_startup_approved_for_fixture(bytes: &[u8]) {
    // SAFETY: `bytes` vivo durante a chamada síncrona.
    let err = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            STARTUP_APPROVED_KEY,
            VALUE_NAME,
            REG_BINARY.0,
            Some(bytes.as_ptr() as *const core::ffi::c_void),
            bytes.len() as u32,
        )
    };
    assert!(
        err.is_ok(),
        "fixture: falha ao escrever StartupApproved\\Run"
    );
}

struct RestoreRegistry {
    run_before: Option<Vec<u16>>,
    startup_approved_before: Option<Vec<u8>>,
}

impl RestoreRegistry {
    fn capture() -> Self {
        Self {
            run_before: raw_run_value_for_fixture(),
            startup_approved_before: raw_startup_approved_for_fixture(),
        }
    }
}

impl Drop for RestoreRegistry {
    fn drop(&mut self) {
        match &self.run_before {
            Some(value) => restore_raw_run_value(value),
            None => {
                let _ = remove_run_entry();
            }
        }
        match &self.startup_approved_before {
            Some(bytes) => write_raw_startup_approved_for_fixture(bytes),
            None => {
                // SAFETY: idempotente — já tratado como sucesso se ausente.
                let _ = unsafe {
                    RegDeleteKeyValueW(HKEY_CURRENT_USER, STARTUP_APPROVED_KEY, VALUE_NAME)
                };
            }
        }
    }
}

#[test]
#[ignore = "toca o HKEY_CURRENT_USER real desta conta"]
fn write_then_remove_run_entry_round_trips_presence() {
    let _restore = RestoreRegistry::capture();

    assert!(write_run_entry(r"C:\fixture\duplicata.exe"));
    assert!(
        run_entry_present(),
        "Run deveria existir depois de escrever"
    );

    assert!(remove_run_entry());
    assert!(
        !run_entry_present(),
        "Run não deveria existir depois de remover"
    );

    assert!(remove_run_entry());
}

#[test]
#[ignore = "toca o HKEY_CURRENT_USER real desta conta"]
fn read_startup_approved_raw_matches_what_was_really_written() {
    let _restore = RestoreRegistry::capture();

    // Nenhum valor: None.
    // SAFETY: idempotente.
    let _ = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, STARTUP_APPROVED_KEY, VALUE_NAME) };
    assert_eq!(read_startup_approved_raw(), None);

    let disapproved = vec![0x03u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
    write_raw_startup_approved_for_fixture(&disapproved);
    assert_eq!(read_startup_approved_raw(), Some(disapproved.clone()));
    assert_eq!(
        duplicata_core::interpret_approval(read_startup_approved_raw().as_deref()),
        duplicata_core::Approval::Desaprovado,
        "os bytes lidos de verdade do registro devem compor com o módulo puro"
    );
}

#[test]
#[ignore = "toca o HKEY_CURRENT_USER real desta conta"]
fn clear_startup_approval_erases_the_value_fr010b() {
    let _restore = RestoreRegistry::capture();

    write_raw_startup_approved_for_fixture(&[0x03, 0, 0, 0]);
    assert!(read_startup_approved_raw().is_some());

    assert!(clear_startup_approval());
    assert_eq!(read_startup_approved_raw(), None);
    assert_eq!(
        duplicata_core::interpret_approval(None),
        duplicata_core::Approval::Indeterminado
    );

    assert!(clear_startup_approval());
}
