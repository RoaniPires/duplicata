//! Regrava `assets/settings_light.ico` e `assets/settings_dark.ico` a partir de
//! `gear.rs`: `cargo run -p duplicata-core --example settings_icons`.

mod gear;

use std::error::Error;
use std::path::Path;
use std::process::ExitCode;

fn main() -> ExitCode {
    // crates/duplicata-core → raiz do workspace. Sem "../..", o caminho na
    // mensagem de erro sai limpo.
    let Some(root) = Path::new(env!("CARGO_MANIFEST_DIR")).ancestors().nth(2) else {
        eprintln!("erro: pasta do pacote fora de um workspace");
        return ExitCode::FAILURE;
    };
    let assets = root.join("assets");
    match gear::write_settings_icons(&assets) {
        Ok(written) => {
            for (name, sizes) in written {
                println!("gravado {name} {sizes:?}");
            }
            ExitCode::SUCCESS
        }
        Err(e) => {
            match e.source() {
                Some(cause) => eprintln!("erro: {e}: {cause}"),
                None => eprintln!("erro: {e}"),
            }
            ExitCode::FAILURE
        }
    }
}
