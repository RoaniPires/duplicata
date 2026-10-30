#![cfg(windows)]

use duplicata_core::win_v::{run_explorer_restart, ExplorerRestartOutcome, ExplorerRestarter};
use duplicata_win::explorer_instance::{shell_present, shell_started_at};
use duplicata_win::explorer_restart::RmExplorerRestarter;

/// Pede ao Explorer real que feche normalmente e o deixa voltar: as janelas
/// de pasta abertas fecham. Rodar à mão, com a sessão livre.
#[test]
#[ignore = "reinicia o Windows Explorer real"]
fn the_explorer_closes_normally_and_comes_back_as_another_process() {
    let before = shell_started_at().expect("há shell nesta sessão");
    let mut restarter = RmExplorerRestarter::default();
    let mut came_back = None;
    let outcome = run_explorer_restart(
        &mut restarter as &mut dyn ExplorerRestarter,
        |timeout| {
            // Sem a bandeja deste teste: confere o shell até o prazo, pela
            // própria condição (nunca uma espera fixa como sincronização).
            let deadline = std::time::Instant::now() + timeout;
            while std::time::Instant::now() < deadline {
                if let Some(now) = shell_started_at().filter(|&t| t != before) {
                    came_back = Some(now);
                    return true;
                }
                std::thread::sleep(std::time::Duration::from_millis(100));
            }
            false
        },
        shell_present,
        || false,
    );
    assert_eq!(outcome, ExplorerRestartOutcome::Returned);
    assert_ne!(came_back, Some(before));
}
