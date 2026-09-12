use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::clock::Clock;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Sensitivity {
    CopiaDeTrabalho,
    Historico,
    Configuracao,
    Diagnostico,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DataItem {
    pub path: PathBuf,
    pub sensitivity: Sensitivity,
}

pub fn removal_order(data_dir: &Path) -> Vec<DataItem> {
    let mut items = Vec::new();

    for name in [
        "duplicata.work.db",
        "duplicata.work.db-wal",
        "duplicata.work.db-shm",
        "duplicata.db.tmp-crypto",
    ] {
        push_if_exists(&mut items, data_dir, name, Sensitivity::CopiaDeTrabalho);
    }
    for name in ["duplicata.db", "duplicata.db-wal", "duplicata.db-shm"] {
        push_if_exists(&mut items, data_dir, name, Sensitivity::Historico);
    }
    push_if_exists(
        &mut items,
        data_dir,
        "config.toml",
        Sensitivity::Configuracao,
    );
    let logs = data_dir.join("logs");
    if logs.exists() {
        items.push(DataItem {
            path: logs,
            sensitivity: Sensitivity::Diagnostico,
        });
    }

    items
}

fn push_if_exists(items: &mut Vec<DataItem>, dir: &Path, name: &str, sensitivity: Sensitivity) {
    let path = dir.join(name);
    if path.exists() {
        items.push(DataItem { path, sensitivity });
    }
}

pub fn remove_item(item: &DataItem) -> std::io::Result<()> {
    match fs::metadata(&item.path) {
        Ok(meta) if meta.is_dir() => fs::remove_dir_all(&item.path),
        Ok(_) => fs::remove_file(&item.path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RemovalOutcome {
    pub removed: Vec<PathBuf>,
    pub remaining: Vec<DataItem>,
}

impl RemovalOutcome {
    pub fn is_complete(&self) -> bool {
        self.remaining.is_empty()
    }
}

const RETRY_DELAY: Duration = Duration::from_millis(250);

pub fn remove_all(
    items: Vec<DataItem>,
    remove: &dyn Fn(&DataItem) -> std::io::Result<()>,
    clock: &dyn Clock,
) -> RemovalOutcome {
    let mut removed = Vec::new();

    let mut remaining = attempt_ordered_prefix(items, remove, &mut removed);
    if !remaining.is_empty() {
        clock.sleep(RETRY_DELAY);
        remaining = attempt_ordered_prefix(remaining, remove, &mut removed);
    }

    RemovalOutcome { removed, remaining }
}

fn attempt_ordered_prefix(
    items: Vec<DataItem>,
    remove: &dyn Fn(&DataItem) -> std::io::Result<()>,
    removed: &mut Vec<PathBuf>,
) -> Vec<DataItem> {
    let mut iter = items.into_iter();
    for item in iter.by_ref() {
        match remove(&item) {
            Ok(()) => removed.push(item.path),
            Err(_) => return std::iter::once(item).chain(iter).collect(),
        }
    }
    Vec::new()
}

pub fn leftover_message(outcome: &RemovalOutcome) -> Option<String> {
    if outcome.is_complete() {
        return None;
    }
    let mut msg = String::from(
        "Não foi possível remover todo o histórico de tudo que foi copiado. \
         Os caminhos a seguir precisam ser apagados manualmente:\n",
    );
    for item in &outcome.remaining {
        msg.push_str("  - ");
        msg.push_str(&item.path.display().to_string());
        msg.push('\n');
    }
    Some(msg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::clock::FakeClock;

    fn touch(dir: &Path, name: &str) {
        fs::write(dir.join(name), b"x").unwrap();
    }

    #[test]
    fn removal_order_only_includes_paths_that_exist() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "config.toml");
        let items = removal_order(dir.path());
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].sensitivity, Sensitivity::Configuracao);
    }

    #[test]
    fn removal_order_is_sorted_from_most_to_least_sensitive() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "duplicata.work.db",
            "duplicata.db.tmp-crypto",
            "duplicata.db",
            "duplicata.db-wal",
            "config.toml",
        ] {
            touch(dir.path(), name);
        }
        fs::create_dir(dir.path().join("logs")).unwrap();

        let items = removal_order(dir.path());
        for pair in items.windows(2) {
            assert!(
                pair[0].sensitivity <= pair[1].sensitivity,
                "ordem violada: {:?} veio antes de {:?}",
                pair[0].sensitivity,
                pair[1].sensitivity
            );
        }
    }

    #[test]
    fn every_prefix_removed_leaves_only_less_or_equally_sensitive_items() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "duplicata.work.db",
            "duplicata.work.db-wal",
            "duplicata.work.db-shm",
            "duplicata.db.tmp-crypto",
            "duplicata.db",
            "duplicata.db-wal",
            "duplicata.db-shm",
            "config.toml",
        ] {
            touch(dir.path(), name);
        }
        fs::create_dir(dir.path().join("logs")).unwrap();

        let items = removal_order(dir.path());
        assert!(
            items.len() >= 4,
            "fixture fraca demais para testar prefixos"
        );

        for cut in 0..=items.len() {
            let removed_max = items[..cut].iter().map(|i| i.sensitivity).max();
            let remaining_min = items[cut..].iter().map(|i| i.sensitivity).min();
            if let (Some(r_max), Some(rem_min)) = (removed_max, remaining_min) {
                assert!(
                    r_max <= rem_min,
                    "interrompendo em {cut}: restou item mais sensível que algo já removido"
                );
            }
        }
    }

    #[test]
    fn remove_item_deletes_a_file() {
        let dir = tempfile::tempdir().unwrap();
        touch(dir.path(), "config.toml");
        let item = DataItem {
            path: dir.path().join("config.toml"),
            sensitivity: Sensitivity::Configuracao,
        };
        remove_item(&item).unwrap();
        assert!(!item.path.exists());
    }

    #[test]
    fn remove_item_deletes_a_directory() {
        let dir = tempfile::tempdir().unwrap();
        let logs = dir.path().join("logs");
        fs::create_dir(&logs).unwrap();
        fs::write(logs.join("a.log"), b"x").unwrap();
        let item = DataItem {
            path: logs.clone(),
            sensitivity: Sensitivity::Diagnostico,
        };
        remove_item(&item).unwrap();
        assert!(!logs.exists());
    }

    #[test]
    fn remove_item_on_an_already_absent_path_is_ok() {
        let dir = tempfile::tempdir().unwrap();
        let item = DataItem {
            path: dir.path().join("nao-existe.db"),
            sensitivity: Sensitivity::Historico,
        };
        assert!(remove_item(&item).is_ok());
    }

    struct FlakyRemove {
        fail_times: std::cell::RefCell<std::collections::HashMap<PathBuf, u32>>,
    }

    impl FlakyRemove {
        fn new(flaky_paths: impl IntoIterator<Item = (PathBuf, u32)>) -> Self {
            Self {
                fail_times: std::cell::RefCell::new(flaky_paths.into_iter().collect()),
            }
        }

        fn call(&self, item: &DataItem) -> std::io::Result<()> {
            let mut map = self.fail_times.borrow_mut();
            match map.get_mut(&item.path) {
                Some(remaining) if *remaining > 0 => {
                    *remaining -= 1;
                    Err(std::io::Error::other("fixture: bloqueio simulado"))
                }
                _ => Ok(()),
            }
        }
    }

    #[test]
    fn remove_all_does_not_sleep_when_everything_removes_on_the_first_pass() {
        let items = vec![
            DataItem {
                path: PathBuf::from("a"),
                sensitivity: Sensitivity::Historico,
            },
            DataItem {
                path: PathBuf::from("b"),
                sensitivity: Sensitivity::Configuracao,
            },
        ];
        let flaky = FlakyRemove::new([]);
        let clock = FakeClock::new();

        let outcome = remove_all(items, &|i| flaky.call(i), &clock);

        assert!(outcome.is_complete());
        assert_eq!(outcome.removed.len(), 2);
        assert_eq!(
            clock.sleep_calls(),
            0,
            "não deveria pausar quando nada falhou"
        );
    }

    #[test]
    fn remove_all_retries_once_after_a_transient_failure_and_recovers() {
        let flaky_path = PathBuf::from("cópia-de-trabalho");
        let items = vec![
            DataItem {
                path: flaky_path.clone(),
                sensitivity: Sensitivity::CopiaDeTrabalho,
            },
            DataItem {
                path: PathBuf::from("config"),
                sensitivity: Sensitivity::Configuracao,
            },
        ];
        let flaky = FlakyRemove::new([(flaky_path.clone(), 1)]);
        let clock = FakeClock::new();

        let outcome = remove_all(items, &|i| flaky.call(i), &clock);

        assert!(
            outcome.is_complete(),
            "a segunda tentativa deveria ter recuperado o item flaky"
        );
        assert!(outcome.removed.contains(&flaky_path));
        assert_eq!(
            clock.sleep_calls(),
            1,
            "exatamente uma pausa antes da nova tentativa"
        );
    }

    #[test]
    fn remove_all_reports_remaining_when_the_retry_also_fails() {
        let stuck_path = PathBuf::from("preso-para-sempre");
        let config_path = PathBuf::from("config");
        let items = vec![
            DataItem {
                path: stuck_path.clone(),
                sensitivity: Sensitivity::CopiaDeTrabalho,
            },
            DataItem {
                path: config_path.clone(),
                sensitivity: Sensitivity::Configuracao,
            },
        ];
        let flaky = FlakyRemove::new([(stuck_path.clone(), 100)]);
        let clock = FakeClock::new();

        let outcome = remove_all(items, &|i| flaky.call(i), &clock);

        assert!(!outcome.is_complete());
        assert_eq!(outcome.remaining.len(), 2);
        assert_eq!(outcome.remaining[0].path, stuck_path);
        assert_eq!(outcome.remaining[1].path, config_path);
        assert!(outcome.removed.is_empty());
        assert_eq!(
            clock.sleep_calls(),
            1,
            "só uma pausa — FR-015c não é um laço ilimitado"
        );
    }

    #[test]
    fn a_stuck_sensitive_item_never_lets_a_later_less_sensitive_item_be_removed_first() {
        let stuck_path = PathBuf::from("cópia-de-trabalho-presa");
        let config_path = PathBuf::from("config");
        let items = vec![
            DataItem {
                path: stuck_path.clone(),
                sensitivity: Sensitivity::CopiaDeTrabalho,
            },
            DataItem {
                path: config_path.clone(),
                sensitivity: Sensitivity::Configuracao,
            },
        ];
        let flaky = FlakyRemove::new([(stuck_path.clone(), 100)]);
        let clock = FakeClock::new();

        let outcome = remove_all(items, &|i| flaky.call(i), &clock);

        let config_removed = outcome.removed.contains(&config_path);
        let stuck_remaining = outcome.remaining.iter().any(|i| i.path == stuck_path);
        assert!(
            !(config_removed && stuck_remaining),
            "config foi removido com a cópia de trabalho ainda presa — inverteu a ordem de sensibilidade"
        );
    }

    #[test]
    fn is_complete_only_when_remaining_is_empty() {
        let complete = RemovalOutcome::default();
        assert!(complete.is_complete());

        let incomplete = RemovalOutcome {
            removed: vec![],
            remaining: vec![DataItem {
                path: PathBuf::from("x"),
                sensitivity: Sensitivity::Historico,
            }],
        };
        assert!(!incomplete.is_complete());
    }

    #[test]
    fn leftover_message_is_none_when_complete() {
        assert!(leftover_message(&RemovalOutcome::default()).is_none());
    }

    #[test]
    fn leftover_message_names_history_and_lists_exact_paths_when_incomplete() {
        let outcome = RemovalOutcome {
            removed: vec![],
            remaining: vec![DataItem {
                path: PathBuf::from(r"C:\Users\x\AppData\Local\duplicata\duplicata.db"),
                sensitivity: Sensitivity::Historico,
            }],
        };
        let msg = leftover_message(&outcome).expect("deveria haver mensagem");
        assert!(
            msg.contains("histórico"),
            "mensagem deve nomear o histórico: {msg}"
        );
        assert!(
            msg.contains("duplicata.db"),
            "mensagem deve listar o caminho exato: {msg}"
        );
    }

    #[test]
    fn leftover_message_is_never_the_generic_forbidden_text() {
        let outcome = RemovalOutcome {
            removed: vec![],
            remaining: vec![DataItem {
                path: PathBuf::from("x"),
                sensitivity: Sensitivity::Diagnostico,
            }],
        };
        let msg = leftover_message(&outcome).unwrap();
        assert!(
            !msg.to_lowercase()
                .contains("alguns arquivos não puderam ser removidos"),
            "mensagem genérica proibida pelo FR-015d: {msg}"
        );
    }
}
