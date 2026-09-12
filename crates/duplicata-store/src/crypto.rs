#![cfg(windows)]

use std::ffi::c_void;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

use duplicata_core::StoreError;
use rusqlite::Connection;
use windows::core::PCWSTR;
use windows::Win32::Foundation::{LocalFree, HLOCAL};
use windows::Win32::Security::Cryptography::{
    CryptProtectData, CryptUnprotectData, CRYPTPROTECT_UI_FORBIDDEN, CRYPT_INTEGER_BLOB,
};

const TMP_SUFFIX: &str = ".tmp-crypto";
const WORK_FILE_NAME: &str = "duplicata.work.db";
const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

pub fn tmp_path(db_path: &Path) -> PathBuf {
    let mut s = db_path.as_os_str().to_os_string();
    s.push(TMP_SUFFIX);
    PathBuf::from(s)
}

pub fn work_copy_path(db_path: &Path) -> PathBuf {
    match db_path.parent() {
        Some(dir) => dir.join(WORK_FILE_NAME),
        None => PathBuf::from(WORK_FILE_NAME),
    }
}

pub fn is_dpapi_blob(path: &Path) -> bool {
    let mut head = [0u8; 16];
    match fs::File::open(path).and_then(|mut f| f.read(&mut head)) {
        Ok(n) => n < SQLITE_MAGIC.len() || &head[..SQLITE_MAGIC.len()] != SQLITE_MAGIC,
        Err(_) => false,
    }
}

pub fn effective_read_path(db_path: &Path) -> PathBuf {
    let work = work_copy_path(db_path);
    if work.exists() {
        work
    } else {
        db_path.to_path_buf()
    }
}

pub fn recreate_is_safe_to_offer(db_path: &Path) -> bool {
    !work_copy_path(db_path).exists() && !is_dpapi_blob(db_path)
}

pub fn cleanup_stale_swap_temp(db_path: &Path) {
    let _ = fs::remove_file(tmp_path(db_path));
}

fn with_suffix(path: &Path, suffix: &str) -> PathBuf {
    let mut s = path.as_os_str().to_os_string();
    s.push(suffix);
    PathBuf::from(s)
}

pub fn reconcile_on_startup(db_path: &Path, progress: &dyn Fn(u64)) -> Result<PathBuf, StoreError> {
    cleanup_stale_swap_temp(db_path);

    reseal_work_copy_if_present(db_path, progress)?;

    let work_path = work_copy_path(db_path);
    if is_dpapi_blob(db_path) {
        unprotect_file(db_path, &work_path, &tmp_path(db_path), progress)?;
        Ok(work_path)
    } else {
        Ok(db_path.to_path_buf())
    }
}

pub fn reseal_work_copy_if_present(
    db_path: &Path,
    progress: &dyn Fn(u64),
) -> Result<bool, StoreError> {
    let work_path = work_copy_path(db_path);
    if !work_path.exists() {
        return Ok(false);
    }
    let c = Connection::open(&work_path).map_err(|e| crate::map_sqlite_error(&e))?;
    crate::checkpoint_truncate(&c)?;
    c.close().map_err(|(_, e)| crate::map_sqlite_error(&e))?;

    protect_file(&work_path, db_path, &tmp_path(db_path), progress)?;

    let _ = fs::remove_file(&work_path);
    let _ = fs::remove_file(with_suffix(&work_path, "-wal"));
    let _ = fs::remove_file(with_suffix(&work_path, "-shm"));
    Ok(true)
}

pub fn protect_file(
    src: &Path,
    dst: &Path,
    tmp_path: &Path,
    progress: &dyn Fn(u64),
) -> Result<(), StoreError> {
    swap_via_tmp(src, dst, tmp_path, progress, Direction::Protect)
}

pub fn unprotect_file(
    src: &Path,
    dst: &Path,
    tmp_path: &Path,
    progress: &dyn Fn(u64),
) -> Result<(), StoreError> {
    swap_via_tmp(src, dst, tmp_path, progress, Direction::Unprotect)
}

#[derive(Clone, Copy)]
enum Direction {
    Protect,
    Unprotect,
}

fn swap_via_tmp(
    src: &Path,
    dst: &Path,
    tmp_path: &Path,
    progress: &dyn Fn(u64),
    dir: Direction,
) -> Result<(), StoreError> {
    let input = fs::read(src).map_err(|_| StoreError::Io)?;
    progress(1);

    let output = match dir {
        Direction::Protect => dpapi_protect(&input)?,
        Direction::Unprotect => dpapi_unprotect(&input)?,
    };
    progress(2);

    fs::write(tmp_path, &output).map_err(|_| StoreError::Io)?;
    progress(3);
    fs::rename(tmp_path, dst).map_err(|_| StoreError::Io)?;
    progress(4);
    Ok(())
}

fn dpapi_protect(input: &[u8]) -> Result<Vec<u8>, StoreError> {
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    // SAFETY: `in_blob` aponta para `input` (vivo durante a chamada);
    // `out_blob` é um blob zerado que a API preenche; os `None` são para
    // parâmetros opcionais (descrição, entropia, reservado, prompt).
    let r = unsafe {
        CryptProtectData(
            &in_blob,
            PCWSTR::null(),
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    r.map_err(|_| StoreError::Io)?;
    Ok(take_blob(&out_blob))
}

fn dpapi_unprotect(input: &[u8]) -> Result<Vec<u8>, StoreError> {
    let in_blob = CRYPT_INTEGER_BLOB {
        cbData: input.len() as u32,
        pbData: input.as_ptr() as *mut u8,
    };
    let mut out_blob = CRYPT_INTEGER_BLOB::default();
    // SAFETY: ver `dpapi_protect`. O `None` extra aqui é `ppszdatadescr`
    // (descrição devolvida — não queremos).
    let r = unsafe {
        CryptUnprotectData(
            &in_blob,
            None,
            None,
            None,
            None,
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut out_blob,
        )
    };
    r.map_err(|_| StoreError::Corrupted)?;
    Ok(take_blob(&out_blob))
}

fn take_blob(blob: &CRYPT_INTEGER_BLOB) -> Vec<u8> {
    // SAFETY: `blob.pbData`/`blob.cbData` vêm de uma chamada DPAPI
    // bem-sucedida — a região é válida e do tamanho informado. Copiamos ANTES
    // de liberar.
    let out = unsafe {
        if blob.pbData.is_null() {
            Vec::new()
        } else {
            std::slice::from_raw_parts(blob.pbData, blob.cbData as usize).to_vec()
        }
    };
    if !blob.pbData.is_null() {
        // SAFETY: `blob.pbData` foi alocado pela DPAPI com `LocalAlloc`; a
        // documentação exige `LocalFree`. `HLOCAL` só encapsula o ponteiro.
        unsafe {
            let _ = LocalFree(Some(HLOCAL(blob.pbData as *mut c_void)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tmp_and_work_paths_sit_next_to_the_db() {
        let db = Path::new("C:\\dir\\duplicata.db");
        assert_eq!(tmp_path(db), Path::new("C:\\dir\\duplicata.db.tmp-crypto"));
        assert_eq!(work_copy_path(db), Path::new("C:\\dir\\duplicata.work.db"));
    }

    #[test]
    fn is_dpapi_blob_distinguishes_sqlite_header_from_anything_else() {
        let dir = tempfile::tempdir().unwrap();
        let sqlite = dir.path().join("a.db");
        fs::write(&sqlite, b"SQLite format 3\0rest of a real header...").unwrap();
        assert!(!is_dpapi_blob(&sqlite));

        let blob = dir.path().join("b.db");
        fs::write(&blob, b"\x01\x00\x00\x00\xd0\x8c\x9d\xdf random blob bytes").unwrap();
        assert!(is_dpapi_blob(&blob));

        let short = dir.path().join("c.db");
        fs::write(&short, b"tiny").unwrap();
        assert!(is_dpapi_blob(&short), "arquivo curto demais para o magic");

        assert!(
            !is_dpapi_blob(&dir.path().join("missing.db")),
            "ausente conta como 'não blob' — o open reporta o erro real"
        );
    }

    #[test]
    fn cleanup_stale_swap_temp_is_a_noop_when_there_is_nothing() {
        let dir = tempfile::tempdir().unwrap();
        cleanup_stale_swap_temp(&dir.path().join("duplicata.db"));
    }

    #[test]
    fn effective_read_path_prefers_the_work_copy_when_it_exists() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("duplicata.db");

        assert_eq!(effective_read_path(&db), db);

        let work = work_copy_path(&db);
        fs::write(&work, b"SQLite format 3\0...").unwrap();
        assert_eq!(effective_read_path(&db), work);
    }

    #[test]
    fn recreate_is_only_safe_to_offer_when_there_is_no_protection_signal() {
        let dir = tempfile::tempdir().unwrap();
        let db = dir.path().join("duplicata.db");

        assert!(recreate_is_safe_to_offer(&db));
        fs::write(&db, b"SQLite format 3\0real corruption comes later").unwrap();
        assert!(recreate_is_safe_to_offer(&db));

        fs::write(&db, b"blob DPAPI, no SQLite header here at all").unwrap();
        assert!(!recreate_is_safe_to_offer(&db));

        fs::write(&db, b"SQLite format 3\0").unwrap();
        fs::write(work_copy_path(&db), b"SQLite format 3\0").unwrap();
        assert!(!recreate_is_safe_to_offer(&db));
    }
}
