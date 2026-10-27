//! Confere as invariantes do instalador e anota cada regra quebrada no formato
//! do GitHub Actions:
//! `cargo run -p duplicata-core --example wxs_invariants -- installer/duplicata.wxs`.
//!
//! As regras estão em `check.rs`; o teste `tests/wxs_invariants.rs` aplica a
//! mesma verificação ao `.wxs` versionado no `cargo test`.

mod check;

use std::process::ExitCode;

fn main() -> ExitCode {
    let report = check::run(std::env::args_os().skip(1));
    for line in &report.lines {
        println!("{line}");
    }
    if report.ok {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
