use std::path::Path;

#[cfg(windows)]
use duplicata_core::win_v::{
    uninstall_release, uninstall_release_message, DisabledHotkeysStore, UninstallRelease,
};

#[cfg(windows)]
pub fn uninstall_data() -> i32 {
    let Ok(data_dir) = duplicata_win::paths::data_dir() else {
        duplicata_win::dialog::show_uninstall_leftover(
            "Não foi possível localizar a pasta de dados do duplicata para remover o \
             histórico. Nada foi apagado.",
        );
        return 1;
    };

    let log_dir = std::env::temp_dir().join("duplicata-uninstall-log");
    let _guard = crate::logging::init(&log_dir).ok();
    tracing::warn!(data_dir = %data_dir.display(), "uninstall-data: data_dir resolvido");

    uninstall_data_at(
        &data_dir,
        &duplicata_win::uninstall_dialog::ask_delete_or_preserve,
        &mut duplicata_win::disabled_hotkeys::RegistryDisabledHotkeys::explorer(),
        &duplicata_win::dialog::show_uninstall_leftover,
    )
}

#[cfg(windows)]
pub fn uninstall_data_at(
    data_dir: &Path,
    ask: &dyn Fn(bool) -> bool,
    disabled_hotkeys: &mut dyn DisabledHotkeysStore,
    notify: &dyn Fn(&str),
) -> i32 {
    // O `V` do Win+V sai seja qual for a escolha, e antes dela: apagar os
    // dados leva junto o `config.toml`, que diz se o `V` é do duplicata.
    let config = std::fs::read_to_string(data_dir.join("config.toml")).ok();
    let released = uninstall_release(config.as_deref(), disabled_hotkeys);
    tracing::warn!(
        win_v = match released {
            UninstallRelease::Untouched => "intocado",
            UninstallRelease::Removed => "removido",
            UninstallRelease::Failed(_) => "falhou",
        },
        "uninstall-data: DisabledHotkeys"
    );
    // Erro visível, não só no log: o Win+V continua fora do Windows.
    let win_v_left = match uninstall_release_message(released) {
        Some(message) => {
            notify(message);
            1
        }
        None => 0,
    };

    let history_protected = data_dir.join("duplicata.work.db").exists();
    tracing::warn!(history_protected, "uninstall-data: protecao por conta");

    let delete = ask(history_protected);
    tracing::warn!(delete, "uninstall-data: escolha do usuario no dialogo");
    if !delete {
        return win_v_left;
    }

    let items = duplicata_core::removal_order(data_dir);
    tracing::warn!(
        item_count = items.len(),
        "uninstall-data: itens encontrados"
    );
    let outcome = duplicata_core::remove_all(
        items,
        &duplicata_core::remove_item,
        &duplicata_win::WinClock,
    );
    tracing::warn!(
        removed = outcome.removed.len(),
        remaining = outcome.remaining.len(),
        "uninstall-data: resultado da remocao"
    );

    match duplicata_core::leftover_message(&outcome) {
        Some(message) => {
            notify(&message);
            1
        }
        None => win_v_left,
    }
}
