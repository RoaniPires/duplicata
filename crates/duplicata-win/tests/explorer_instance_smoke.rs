#![cfg(windows)]

use duplicata_core::win_v::ExplorerInstance;
use duplicata_win::explorer_instance::ShellProcess;

/// Só lê: o processo do shell desta sessão.
#[test]
fn the_same_shell_gives_the_same_start_time() {
    let first = ShellProcess.current();
    if first.is_none() {
        // Sessão sem shell (CI sem área de trabalho): nada a comparar.
        return;
    }
    assert_eq!(ShellProcess.current(), first);
    assert_ne!(first, Some(0));
}
