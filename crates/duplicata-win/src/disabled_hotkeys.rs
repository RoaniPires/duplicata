//! O valor `DisabledHotkeys` do Explorer (REG_SZ em
//! `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced`).
//! Só lê, grava e apaga; quem decide o conteúdo é `duplicata_core::win_v`.
//! Valor de outro tipo é erro de leitura: o núcleo não mexe no que não
//! entende.

use core::ffi::c_void;

use duplicata_core::win_v::{DisabledHotkeysStore, RegistryError};
use windows::core::{w, PCWSTR};
use windows::Win32::Foundation::{ERROR_FILE_NOT_FOUND, WIN32_ERROR};
use windows::Win32::System::Registry::{
    RegDeleteKeyValueW, RegGetValueW, RegSetKeyValueW, HKEY_CURRENT_USER, REG_SZ, RRF_RT_REG_SZ,
};

/// A subchave do Explorer onde o valor mora.
pub const ADVANCED_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Explorer\Advanced";
const VALUE_NAME: PCWSTR = w!("DisabledHotkeys");

/// O valor dentro de `subkey` do HKCU. A real é [`ADVANCED_KEY`]; os testes
/// usam uma descartável.
pub struct RegistryDisabledHotkeys {
    subkey: Vec<u16>,
}

impl RegistryDisabledHotkeys {
    pub fn new(subkey: &str) -> Self {
        RegistryDisabledHotkeys {
            subkey: subkey.encode_utf16().chain(std::iter::once(0)).collect(),
        }
    }

    /// O do Explorer.
    pub fn explorer() -> Self {
        Self::new(ADVANCED_KEY)
    }

    fn key(&self) -> PCWSTR {
        PCWSTR(self.subkey.as_ptr())
    }
}

fn error(err: WIN32_ERROR) -> RegistryError {
    RegistryError(err.0)
}

impl DisabledHotkeysStore for RegistryDisabledHotkeys {
    fn read(&self) -> Result<Option<String>, RegistryError> {
        let mut size: u32 = 0;
        // SAFETY: `self.key()` aponta para o `Vec` NUL-terminado vivo em
        // `self`; primeira chamada só descobre o tamanho (pvData=None, idiom
        // documentado do RegGetValueW); não escreve no registro.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                self.key(),
                VALUE_NAME,
                RRF_RT_REG_SZ,
                None,
                None,
                Some(&mut size),
            )
        };
        if err == ERROR_FILE_NOT_FOUND {
            return Ok(None);
        }
        if err.is_err() {
            return Err(error(err));
        }
        let mut buf = vec![0u16; (size as usize).div_ceil(2)];
        // SAFETY: `buf` tem pelo menos `size` bytes, informado a `pcbData`;
        // a API escreve no máximo isso e termina a cadeia em NUL.
        let err = unsafe {
            RegGetValueW(
                HKEY_CURRENT_USER,
                self.key(),
                VALUE_NAME,
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr() as *mut c_void),
                Some(&mut size),
            )
        };
        if err.is_err() {
            return Err(error(err));
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        Ok(Some(String::from_utf16_lossy(&buf[..len])))
    }

    fn write(&mut self, value: &str) -> Result<(), RegistryError> {
        let wide: Vec<u16> = value.encode_utf16().chain(std::iter::once(0)).collect();
        let bytes = u32::try_from(wide.len() * 2).map_err(|_| RegistryError(0))?;
        // SAFETY: `wide` é UTF-16 NUL-terminado, vivo durante a chamada;
        // `bytes` é o tamanho exato com o terminador, como `REG_SZ` exige.
        // `RegSetKeyValueW` cria a subchave se não existir.
        let err = unsafe {
            RegSetKeyValueW(
                HKEY_CURRENT_USER,
                self.key(),
                VALUE_NAME,
                REG_SZ.0,
                Some(wide.as_ptr() as *const c_void),
                bytes,
            )
        };
        if err.is_err() {
            return Err(error(err));
        }
        Ok(())
    }

    fn delete(&mut self) -> Result<(), RegistryError> {
        // SAFETY: apaga só o valor; sem ele (ou sem a subchave) devolve
        // `ERROR_FILE_NOT_FOUND`, tratado como sucesso.
        let err = unsafe { RegDeleteKeyValueW(HKEY_CURRENT_USER, self.key(), VALUE_NAME) };
        if err.is_ok() || err == ERROR_FILE_NOT_FOUND {
            Ok(())
        } else {
            Err(error(err))
        }
    }
}
