#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    #[cfg(windows)]
    std::process::exit(run_windows());

    #[cfg(not(windows))]
    eprintln!("duplicata só roda no Windows.");
}

#[cfg(windows)]
fn run_windows() -> i32 {
    match std::env::args().nth(1).as_deref() {
        Some("--request-shutdown") => {
            duplicata_win::shutdown_request::request_shutdown();
            0
        }
        Some("--version") => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            0
        }
        Some("--uninstall-data") => duplicata_app::uninstall_cli::uninstall_data(),
        Some("--clear-run-entry") => {
            i32::from(!duplicata_win::startup_registry::remove_run_entry())
        }
        _ => duplicata_app::run::run(),
    }
}
