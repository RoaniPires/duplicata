#![cfg(windows)]

use std::sync::atomic::Ordering;

use duplicata_app::run::shutdown_reseal;
use duplicata_win::message_loop::SESSION_ENDING;

struct ResetSessionEnding;

impl Drop for ResetSessionEnding {
    fn drop(&mut self) {
        SESSION_ENDING.store(false, Ordering::SeqCst);
    }
}

#[test]
fn session_ending_defers_reseal_without_touching_the_work_copy() {
    let _reset = ResetSessionEnding;
    SESSION_ENDING.store(true, Ordering::SeqCst);

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    let work_path = dir.path().join("duplicata.work.db");
    std::fs::write(
        &work_path,
        b"isca -- se shutdown_reseal tocasse aqui, isto sumiria",
    )
    .unwrap();

    shutdown_reseal(&db_path);

    assert!(
        work_path.exists(),
        "com SESSION_ENDING marcado, shutdown_reseal MUST NOT tentar resealar (a operação é \
         adiada para a reconciliação do próximo login, research.md R7 passo 4)"
    );
}

fn seeded_work_copy(dir: &std::path::Path) -> std::path::PathBuf {
    let work = dir.join("duplicata.work.db");
    let conn = duplicata_store::open(&work).unwrap();
    duplicata_store::checkpoint_truncate(&conn).unwrap();
    drop(conn);
    work
}

#[test]
#[ignore = "DPAPI real, vinculado à conta do Windows"]
fn reseal_completes_synchronously_and_consumes_the_work_copy_when_not_session_ending() {
    let _reset = ResetSessionEnding;
    SESSION_ENDING.store(false, Ordering::SeqCst);

    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("duplicata.db");
    let work_path = seeded_work_copy(dir.path());
    assert!(work_path.exists(), "fixture: cópia de trabalho criada");

    shutdown_reseal(&db_path);

    assert!(
        !work_path.exists(),
        "a cópia de trabalho deveria ter sido consumida pelo reseal (renomeada para o blob)"
    );
    assert!(
        duplicata_store::crypto::is_dpapi_blob(&db_path),
        "duplicata.db deveria ser o blob protegido depois do reseal"
    );
}
