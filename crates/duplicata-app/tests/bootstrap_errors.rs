use std::sync::Mutex;

use duplicata_app::bootstrap::{init, InitErrorReporter, Paths};
use duplicata_core::{InitError, StoreError};

#[derive(Default)]
struct CapturingReporter {
    errs: Mutex<Vec<InitError>>,
}

impl InitErrorReporter for CapturingReporter {
    fn report(&self, err: &InitError) {
        self.errs.lock().unwrap().push(err.clone());
    }
}

impl CapturingReporter {
    fn reported(&self) -> Vec<InitError> {
        self.errs.lock().unwrap().clone()
    }
}

fn paths_in(dir: &std::path::Path) -> Paths {
    Paths {
        data_dir: dir.to_path_buf(),
        db_path: dir.join("duplicata.db"),
        log_dir: dir.join("logs"),
    }
}

#[test]
fn corrupted_db_is_reported_as_opendb_corrupted() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    std::fs::write(&paths.db_path, b"isto nao e um banco sqlite").unwrap();
    let reporter = CapturingReporter::default();

    let err = match init(&paths, 0, false, &reporter) {
        Err(e) => e,
        Ok(_) => panic!("esperava erro de banco corrompido"),
    };
    assert_eq!(err, InitError::OpenDb(StoreError::Corrupted));
    assert_eq!(reporter.reported(), vec![err], "reportado de forma visível");
}

#[test]
fn undcreatable_log_dir_is_reported_as_createdir() {
    let dir = tempfile::tempdir().unwrap();
    let a_file = dir.path().join("nao-e-diretorio");
    std::fs::write(&a_file, b"x").unwrap();
    let mut paths = paths_in(dir.path());
    paths.log_dir = a_file.join("logs");
    let reporter = CapturingReporter::default();

    let err = match init(&paths, 0, false, &reporter) {
        Err(e) => e,
        Ok(_) => panic!("esperava falha ao criar o diretório de logs"),
    };
    assert_eq!(err, InitError::CreateDir);
    assert_eq!(reporter.reported(), vec![InitError::CreateDir]);
}

#[test]
#[cfg(windows)]
fn a_reconcile_failure_surfaces_as_a_visible_init_error() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    std::fs::write(dir.path().join("duplicata.work.db"), b"nao e um banco").unwrap();
    let reporter = CapturingReporter::default();

    let err = match init(&paths, 0, false, &reporter) {
        Err(e) => e,
        Ok(_) => panic!("esperava falha da reconciliação"),
    };
    assert!(
        matches!(err, InitError::OpenDb(_)),
        "reportado como InitError::OpenDb, veio {err:?}"
    );
    assert_eq!(reporter.reported(), vec![err], "reportado de forma visível");
}

#[test]
fn happy_path_produces_services_and_reports_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let paths = paths_in(dir.path());
    let reporter = CapturingReporter::default();

    let services = init(&paths, 1_700_000_000_000, false, &reporter).unwrap();
    assert!(reporter.reported().is_empty());
    assert!(services.queue.is_empty());
    assert_eq!(services.counters.processed(), 0);
    let n: i64 = services
        .repo
        .connection()
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(n, 0);
}
