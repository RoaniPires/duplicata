use std::path::PathBuf;

use duplicata_core::InitError;
use windows::core::PWSTR;
use windows::Win32::System::Com::CoTaskMemFree;
use windows::Win32::UI::Shell::{FOLDERID_LocalAppData, SHGetKnownFolderPath, KF_FLAG_DEFAULT};

pub fn data_dir() -> Result<PathBuf, InitError> {
    Ok(local_app_data()?.join("duplicata"))
}

pub fn db_path() -> Result<PathBuf, InitError> {
    Ok(data_dir()?.join("duplicata.db"))
}

pub fn log_dir() -> Result<PathBuf, InitError> {
    Ok(data_dir()?.join("logs"))
}

fn local_app_data() -> Result<PathBuf, InitError> {
    // SAFETY: `FOLDERID_LocalAppData` é um KNOWNFOLDERID válido; passamos flags
    // padrão e nenhum token de usuário. Em caso de sucesso, `SHGetKnownFolderPath`
    // aloca a string com CoTaskMemAlloc e nós a liberamos com `CoTaskMemFree`
    // logo após copiá-la, exatamente como a documentação exige.
    let path = unsafe {
        let pwstr: PWSTR = SHGetKnownFolderPath(&FOLDERID_LocalAppData, KF_FLAG_DEFAULT, None)
            .map_err(|_| InitError::Paths)?;
        let s = pwstr.to_string().map_err(|_| InitError::Paths);
        CoTaskMemFree(Some(pwstr.0 as *const _));
        s?
    };

    let dir = PathBuf::from(path);
    if dir
        .components()
        .any(|c| c.as_os_str().eq_ignore_ascii_case("Roaming"))
    {
        return Err(InitError::Paths);
    }
    Ok(dir)
}
