#![cfg(windows)]

use duplicata_core::win_v::{add_v, AddV, DisabledHotkeysStore};
use duplicata_win::disabled_hotkeys::RegistryDisabledHotkeys;
use windows::core::{w, PCWSTR};
use windows::Win32::System::Registry::{
    RegDeleteKeyW, RegDeleteTreeW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_DWORD,
};

/// Subchave descartável deste processo; nunca a `Advanced` real.
struct Scratch(String);

impl Scratch {
    fn new(name: &str) -> Self {
        Scratch(format!(
            r"Software\duplicata-test\{}-{name}",
            std::process::id()
        ))
    }

    fn wide(&self) -> Vec<u16> {
        self.0.encode_utf16().chain(std::iter::once(0)).collect()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let key = self.wide();
        // SAFETY: apaga só a subchave descartável criada pelo teste.
        let _ = unsafe { RegDeleteTreeW(HKEY_CURRENT_USER, PCWSTR(key.as_ptr())) };
        // A pai só sai vazia: com outro teste ainda usando a dele, o
        // `RegDeleteKeyW` falha e ela fica.
        // SAFETY: só a chave-pai descartável dos testes.
        let _ = unsafe { RegDeleteKeyW(HKEY_CURRENT_USER, w!(r"Software\duplicata-test")) };
    }
}

#[test]
#[ignore = "toca o HKEY_CURRENT_USER real desta conta"]
fn a_missing_value_reads_as_none_and_round_trips_through_write_and_delete() {
    let scratch = Scratch::new("round-trip");
    let mut store = RegistryDisabledHotkeys::new(&scratch.0);
    assert_eq!(store.read(), Ok(None));
    store.write("QRV").unwrap();
    assert_eq!(store.read(), Ok(Some("QRV".into())));
    assert_eq!(
        add_v(store.read().unwrap().as_deref()),
        AddV::AlreadyPresent
    );
    store.delete().unwrap();
    assert_eq!(store.read(), Ok(None));
    store.delete().expect("apagar o que não existe não é erro");
}

#[test]
#[ignore = "toca o HKEY_CURRENT_USER real desta conta"]
fn a_value_of_another_type_is_a_read_error() {
    let scratch = Scratch::new("tipo");
    let key = scratch.wide();
    let value: u32 = 7;
    // SAFETY: grava um DWORD na subchave descartável; `value` vive durante a
    // chamada e tem o tamanho informado.
    let err = unsafe {
        RegSetKeyValueW(
            HKEY_CURRENT_USER,
            PCWSTR(key.as_ptr()),
            windows::core::w!("DisabledHotkeys"),
            REG_DWORD.0,
            Some(&value as *const u32 as *const core::ffi::c_void),
            4,
        )
    };
    assert!(err.is_ok());
    let store = RegistryDisabledHotkeys::new(&scratch.0);
    assert!(store.read().is_err());
}
