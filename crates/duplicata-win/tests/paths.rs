#![cfg(windows)]

use duplicata_win::paths;

#[test]
fn data_dir_is_under_local_appdata_and_never_roaming() {
    let dir = paths::data_dir().expect("resolve %LocalAppData%");
    let s = dir.to_string_lossy().to_lowercase();

    assert!(s.ends_with("duplicata"));
    assert!(
        s.contains("appdata\\local") || s.contains("appdata/local"),
        "esperava caminho sob AppData\\Local, veio: {}",
        dir.display()
    );
    assert!(
        !s.contains("roaming"),
        "o banco NUNCA pode ficar em Roaming: {}",
        dir.display()
    );
}

#[test]
fn db_and_log_paths_hang_off_data_dir() {
    let data = paths::data_dir().unwrap();
    assert_eq!(paths::db_path().unwrap(), data.join("duplicata.db"));
    assert_eq!(paths::log_dir().unwrap(), data.join("logs"));
}
