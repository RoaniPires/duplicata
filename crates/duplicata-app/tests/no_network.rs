use std::path::PathBuf;

const BANNED: &[&str] = &[
    "reqwest",
    "hyper",
    "hyper-util",
    "h2",
    "ureq",
    "attohttpc",
    "isahc",
    "surf",
    "curl",
    "curl-sys",
    "tokio",
    "async-std",
    "smol",
    "native-tls",
    "openssl",
    "openssl-sys",
    "rustls",
];

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .and_then(|p| p.parent())
        .expect("crates/duplicata-app tem dois níveis até a raiz do workspace")
        .to_path_buf()
}

fn package_names_in_lock(lock: &str) -> Vec<&str> {
    lock.lines()
        .filter_map(|l| l.trim().strip_prefix("name = \""))
        .filter_map(|l| l.strip_suffix('"'))
        .collect()
}

#[test]
fn cargo_lock_has_no_networking_crate_anywhere_in_the_workspace() {
    let lock_path = workspace_root().join("Cargo.lock");
    let lock = std::fs::read_to_string(&lock_path)
        .unwrap_or_else(|e| panic!("não consegui ler {}: {e}", lock_path.display()));

    let names = package_names_in_lock(&lock);
    for banned in BANNED {
        assert!(
            !names.contains(banned),
            "{banned} apareceu em Cargo.lock — viola o Princípio I (local-first, sem rede)"
        );
    }
}

#[test]
fn deny_toml_bans_at_least_the_same_names_this_test_checks() {
    let deny_path = workspace_root().join("deny.toml");
    let deny = std::fs::read_to_string(&deny_path)
        .unwrap_or_else(|e| panic!("não consegui ler {}: {e}", deny_path.display()));
    for banned in BANNED {
        assert!(
            deny.contains(&format!("name = \"{banned}\"")),
            "{banned} está na lista deste teste mas não em deny.toml — as duas listas divergiram"
        );
    }
}
