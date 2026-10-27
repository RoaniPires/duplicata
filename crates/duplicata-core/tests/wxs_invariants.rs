//! Invariantes do `installer/duplicata.wxs`: o núcleo do comando
//! `cargo run -p duplicata-core --example wxs_invariants -- installer/duplicata.wxs`
//! e a guarda que aplica a mesma verificação ao arquivo versionado.

#[path = "../examples/wxs_invariants/check.rs"]
mod check;

use check::{check, Violation};

/// O `.wxs` versionado, que cumpre as seis regras.
const VERSIONED: &str = include_str!("../../../installer/duplicata.wxs");

/// O `.wxs` versionado com `from` trocado por `to`; falha se o trecho não existe
/// (senão a variação testaria o arquivo original sem perceber).
fn variant(from: &str, to: &str) -> String {
    let changed = VERSIONED.replace(from, to);
    assert_ne!(
        changed, VERSIONED,
        "o trecho {from:?} não está no .wxs versionado"
    );
    changed
}

fn violations(xml: &str) -> Vec<Violation> {
    check(xml).expect("o fixture é XML válido")
}

fn messages(xml: &str) -> Vec<String> {
    violations(xml).iter().map(ToString::to_string).collect()
}

fn only_message(xml: &str) -> String {
    let all = messages(xml);
    assert_eq!(all.len(), 1, "esperada uma violação, veio: {all:#?}");
    all.into_iter().next().expect("tamanho conferido acima")
}

// Regra 1: <Package> existe e Package/@Scope é perUser.

#[test]
fn the_versioned_wxs_has_no_violation() {
    assert_eq!(violations(VERSIONED), Vec::new());
}

#[test]
fn per_machine_scope_is_reported_with_found_and_expected_values() {
    let xml = variant(r#"Scope="perUser""#, r#"Scope="perMachine""#);
    assert_eq!(
        violations(&xml),
        vec![Violation::ScopeWrong {
            found: Some("perMachine".into())
        }]
    );
    let msg = only_message(&xml);
    for part in ["Package/@Scope", "'perMachine'", "'perUser'", "elevação"] {
        assert!(msg.contains(part), "{msg:?} não cita {part:?}");
    }
}

#[test]
fn scope_comparison_is_case_sensitive() {
    let xml = variant(r#"Scope="perUser""#, r#"Scope="PerUser""#);
    assert_eq!(
        violations(&xml),
        vec![Violation::ScopeWrong {
            found: Some("PerUser".into())
        }]
    );
}

#[test]
fn missing_scope_counts_as_wrong_value_and_says_absent() {
    let xml = variant(r#"Scope="perUser""#, "");
    assert_eq!(
        violations(&xml),
        vec![Violation::ScopeWrong { found: None }]
    );
    let msg = only_message(&xml);
    assert!(
        msg.contains("Package/@Scope está ausente, esperado 'perUser'"),
        "{msg:?}"
    );
    assert!(msg.contains("'perUser'"), "{msg:?}");
}

#[test]
fn missing_package_is_reported() {
    // Mesmo namespace, outro nome: o resto do documento continua igual.
    let xml = variant("<Package", "<Pacote").replace("</Package>", "</Pacote>");
    assert_eq!(violations(&xml), vec![Violation::PackageMissing]);
    let msg = only_message(&xml);
    assert!(msg.contains("<Package> não encontrado"), "{msg:?}");
}

// Regra 2: nenhum <ServiceInstall> nem <ServiceControl>.

const MAIN_COMPONENT: &str = r#"<Component Id="MainExecutable" Guid="*">"#;
const SERVICE_INSTALL: &str = r#"<ServiceInstall Id="Svc" Name="duplicata" Type="ownProcess" Start="auto" ErrorControl="normal" />"#;
const SERVICE_CONTROL: &str = r#"<ServiceControl Id="SvcCtl" Name="duplicata" Start="install" />"#;

#[test]
fn service_install_inside_a_component_is_reported() {
    let xml = variant(
        MAIN_COMPONENT,
        &format!("{MAIN_COMPONENT}{SERVICE_INSTALL}"),
    );
    assert_eq!(
        violations(&xml),
        vec![Violation::ServiceElement {
            tag: "ServiceInstall"
        }]
    );
    let msg = only_message(&xml);
    assert!(msg.contains("<ServiceInstall>"), "{msg:?}");
    assert!(msg.contains("serviço do Windows"), "{msg:?}");
}

#[test]
fn service_control_is_reported() {
    let xml = variant(
        MAIN_COMPONENT,
        &format!("{MAIN_COMPONENT}{SERVICE_CONTROL}"),
    );
    assert_eq!(
        violations(&xml),
        vec![Violation::ServiceElement {
            tag: "ServiceControl"
        }]
    );
    let msg = only_message(&xml);
    assert!(msg.contains("<ServiceControl>"), "{msg:?}");
}

#[test]
fn service_install_is_reported_before_service_control() {
    // ServiceControl vem primeiro no documento; a ordem do relatório é a das regras.
    let xml = variant(
        MAIN_COMPONENT,
        &format!("{MAIN_COMPONENT}{SERVICE_CONTROL}{SERVICE_INSTALL}"),
    );
    assert_eq!(
        violations(&xml),
        vec![
            Violation::ServiceElement {
                tag: "ServiceInstall"
            },
            Violation::ServiceElement {
                tag: "ServiceControl"
            },
        ]
    );
}

// Regra 3: <Property Id="MSIRESTARTMANAGERCONTROL" Value="Disable">.

const RESTART_MANAGER: &str = r#"<Property Id="MSIRESTARTMANAGERCONTROL" Value="Disable" />"#;
const RESTART_MANAGER_COMMON: &str = "Property Id=MSIRESTARTMANAGERCONTROL Value=Disable ausente";

#[test]
fn restart_manager_disable_shutdown_is_reported_with_found_value() {
    let xml = variant(
        RESTART_MANAGER,
        r#"<Property Id="MSIRESTARTMANAGERCONTROL" Value="DisableShutdown" />"#,
    );
    assert_eq!(
        violations(&xml),
        vec![Violation::RestartManagerNotDisabled {
            found: Some("DisableShutdown".into())
        }]
    );
    let msg = only_message(&xml);
    assert!(msg.contains(RESTART_MANAGER_COMMON), "{msg:?}");
    assert!(
        msg.contains("(encontrado Value='DisableShutdown')"),
        "{msg:?}"
    );
    assert!(msg.contains("texto claro"), "{msg:?}");
}

#[test]
fn restart_manager_property_missing_is_reported_without_found_value() {
    let xml = variant(RESTART_MANAGER, "");
    assert_eq!(
        violations(&xml),
        vec![Violation::RestartManagerNotDisabled { found: None }]
    );
    let msg = only_message(&xml);
    assert!(msg.contains(RESTART_MANAGER_COMMON), "{msg:?}");
    assert!(!msg.contains("encontrado"), "{msg:?}");
}

#[test]
fn restart_manager_passes_when_any_property_disables_it() {
    // Como no script anterior: basta uma Property com Id e Value certos.
    let xml = variant(
        RESTART_MANAGER,
        &format!(
            r#"<Property Id="MSIRESTARTMANAGERCONTROL" Value="DisableShutdown" />{RESTART_MANAGER}"#
        ),
    );
    assert_eq!(violations(&xml), Vec::new());
}

// Regra 4: <MajorUpgrade> existe e MajorUpgrade/@AllowDowngrades é no.

const ALLOW_DOWNGRADES: &str = r#"AllowDowngrades="no""#;

#[test]
fn allow_downgrades_yes_is_reported_with_found_and_expected_values() {
    let xml = variant(ALLOW_DOWNGRADES, r#"AllowDowngrades="yes""#);
    assert_eq!(
        violations(&xml),
        vec![Violation::AllowDowngradesWrong {
            found: Some("yes".into())
        }]
    );
    let msg = only_message(&xml);
    for part in [
        "MajorUpgrade/@AllowDowngrades",
        "'yes'",
        "'no'",
        "histórico protegido",
    ] {
        assert!(msg.contains(part), "{msg:?} não cita {part:?}");
    }
}

#[test]
fn allow_downgrades_comparison_is_case_sensitive() {
    let xml = variant(ALLOW_DOWNGRADES, r#"AllowDowngrades="No""#);
    assert_eq!(
        violations(&xml),
        vec![Violation::AllowDowngradesWrong {
            found: Some("No".into())
        }]
    );
}

#[test]
fn missing_allow_downgrades_counts_as_wrong_value_and_says_absent() {
    let xml = variant(ALLOW_DOWNGRADES, "");
    assert_eq!(
        violations(&xml),
        vec![Violation::AllowDowngradesWrong { found: None }]
    );
    let msg = only_message(&xml);
    assert!(
        msg.contains("MajorUpgrade/@AllowDowngrades está ausente, esperado 'no'"),
        "{msg:?}"
    );
}

#[test]
fn missing_major_upgrade_is_reported() {
    let xml = variant("<MajorUpgrade", "<MajorUpgradeRemovido");
    assert_eq!(violations(&xml), vec![Violation::MajorUpgradeMissing]);
    let msg = only_message(&xml);
    assert!(msg.contains("<MajorUpgrade> ausente"), "{msg:?}");
}

// Regra 5: <Media> existe e Media/@EmbedCab é yes.

const MEDIA: &str = r#"<Media Id="1" Cabinet="duplicata.cab" EmbedCab="yes" />"#;

#[test]
fn media_without_embed_cab_is_reported_as_absent() {
    let xml = variant(MEDIA, r#"<Media Id="1" Cabinet="duplicata.cab" />"#);
    assert_eq!(
        violations(&xml),
        vec![Violation::EmbedCabWrong { found: None }]
    );
    let msg = only_message(&xml);
    for part in [
        "Media/@EmbedCab está ausente, esperado 'yes'",
        "'yes'",
        "só o .msi",
    ] {
        assert!(msg.contains(part), "{msg:?} não cita {part:?}");
    }
}

#[test]
fn embed_cab_no_is_reported_with_found_value() {
    let xml = variant(
        MEDIA,
        r#"<Media Id="1" Cabinet="duplicata.cab" EmbedCab="no" />"#,
    );
    assert_eq!(
        violations(&xml),
        vec![Violation::EmbedCabWrong {
            found: Some("no".into())
        }]
    );
    let msg = only_message(&xml);
    assert!(msg.contains("Media/@EmbedCab é 'no'"), "{msg:?}");
}

#[test]
fn missing_media_is_reported() {
    let xml = variant(MEDIA, "");
    assert_eq!(violations(&xml), vec![Violation::MediaMissing]);
    let msg = only_message(&xml);
    assert!(msg.contains("<Media> ausente"), "{msg:?}");
    assert!(msg.contains("só o .msi"), "{msg:?}");
}

// Regra 6: Property ARPPRODUCTICON com Value não vazio, Id de algum <Icon>.

const ARP_ICON: &str = r#"<Property Id="ARPPRODUCTICON" Value="DuplicataIcon.ico" />"#;

#[test]
fn missing_arp_icon_property_is_reported() {
    let xml = variant(ARP_ICON, "");
    assert_eq!(violations(&xml), vec![Violation::ArpIconMissing]);
    let msg = only_message(&xml);
    assert!(msg.contains("ARPPRODUCTICON ausente"), "{msg:?}");
    assert!(
        msg.contains("ícone genérico em Aplicativos instalados"),
        "{msg:?}"
    );
}

#[test]
fn empty_arp_icon_value_counts_as_missing() {
    let xml = variant(ARP_ICON, r#"<Property Id="ARPPRODUCTICON" Value="" />"#);
    assert_eq!(violations(&xml), vec![Violation::ArpIconMissing]);
}

#[test]
fn arp_icon_pointing_to_an_undeclared_icon_is_reported() {
    let xml = variant(
        ARP_ICON,
        r#"<Property Id="ARPPRODUCTICON" Value="outro.ico" />"#,
    )
    .replace(
        r#"<Icon Id="DuplicataIcon.ico""#,
        r#"<Icon Id="duplicata.ico""#,
    );
    assert_eq!(
        violations(&xml),
        vec![Violation::ArpIconUndeclared {
            value: "outro.ico".into()
        }]
    );
    let msg = only_message(&xml);
    assert!(msg.contains("'outro.ico'"), "{msg:?}");
    assert!(msg.contains("nenhum <Icon> declarado"), "{msg:?}");
}

// Relatório: todas as regras quebradas, na ordem das regras.

#[test]
fn several_broken_rules_are_all_reported_in_rule_order() {
    let xml = variant(r#"Scope="perUser""#, r#"Scope="perMachine""#)
        .replace(r#"EmbedCab="yes""#, r#"EmbedCab="no""#)
        .replace(
            MAIN_COMPONENT,
            &format!("{MAIN_COMPONENT}{SERVICE_INSTALL}"),
        );
    assert_eq!(
        violations(&xml),
        vec![
            Violation::ScopeWrong {
                found: Some("perMachine".into())
            },
            Violation::ServiceElement {
                tag: "ServiceInstall"
            },
            Violation::EmbedCabWrong {
                found: Some("no".into())
            },
        ]
    );
}

fn every_rule_missing() -> Vec<Violation> {
    vec![
        Violation::PackageMissing,
        Violation::RestartManagerNotDisabled { found: None },
        Violation::MajorUpgradeMissing,
        Violation::MediaMissing,
        Violation::ArpIconMissing,
    ]
}

#[test]
fn a_document_without_the_wix_v4_namespace_fails_every_rule_that_needs_an_element() {
    assert_eq!(
        violations(r#"<Wix><Package Scope="perUser"/></Wix>"#),
        every_rule_missing()
    );
}

#[test]
fn a_wix_v3_document_fails_every_rule_that_needs_an_element() {
    let xml = VERSIONED.replace(
        r#"xmlns="http://wixtoolset.org/schemas/v4/wxs""#,
        r#"xmlns="http://schemas.microsoft.com/wix/2006/wi""#,
    );
    assert_ne!(xml, VERSIONED);
    assert_eq!(violations(&xml), every_rule_missing());
}

#[test]
fn no_message_cites_requirement_ids_or_research_notes() {
    let all = [
        Violation::PackageMissing,
        Violation::ScopeWrong { found: None },
        Violation::ScopeWrong {
            found: Some("perMachine".into()),
        },
        Violation::ServiceElement {
            tag: "ServiceInstall",
        },
        Violation::ServiceElement {
            tag: "ServiceControl",
        },
        Violation::RestartManagerNotDisabled { found: None },
        Violation::RestartManagerNotDisabled {
            found: Some("DisableShutdown".into()),
        },
        Violation::MajorUpgradeMissing,
        Violation::AllowDowngradesWrong { found: None },
        Violation::AllowDowngradesWrong {
            found: Some("yes".into()),
        },
        Violation::MediaMissing,
        Violation::EmbedCabWrong { found: None },
        Violation::EmbedCabWrong {
            found: Some("no".into()),
        },
        Violation::ArpIconMissing,
        Violation::ArpIconUndeclared {
            value: "outro.ico".into(),
        },
    ];
    for violation in all {
        let msg = violation.to_string();
        assert!(!msg.is_empty(), "{violation:?} sem mensagem");
        assert!(msg.contains(" — "), "{msg:?} não explica por que importa");
        assert!(!msg.contains("FR-"), "{msg:?}");
        assert!(!msg.contains("research.md"), "{msg:?}");
    }
}

// Comando: argumentos, leitura e relatório.

use check::{run, Report, OK_LINE};
use std::ffi::OsString;
use std::path::Path;

const USAGE: &str =
    "::error::uso: cargo run -p duplicata-core --example wxs_invariants -- <caminho para o .wxs>";

fn run_on(path: &Path) -> Report {
    run([path.as_os_str().to_owned()])
}

fn written(dir: &tempfile::TempDir, content: &str) -> std::path::PathBuf {
    let path = dir.path().join("duplicata.wxs");
    std::fs::write(&path, content).expect("grava o .wxs temporário");
    path
}

#[test]
fn without_arguments_it_prints_the_usage_and_fails() {
    let report = run(Vec::<OsString>::new());
    assert_eq!(report.lines, vec![USAGE.to_owned()]);
    assert!(!report.ok);
}

#[test]
fn with_two_arguments_it_prints_the_usage_and_fails() {
    let report = run([OsString::from("a.wxs"), OsString::from("b.wxs")]);
    assert_eq!(report.lines, vec![USAGE.to_owned()]);
    assert!(!report.ok);
}

#[test]
fn a_missing_file_is_reported_with_its_path_and_fails() {
    let dir = tempfile::tempdir().expect("pasta temporária");
    let report = run_on(&dir.path().join("nao-existe.wxs"));
    assert!(!report.ok);
    assert_eq!(report.lines.len(), 1, "{:#?}", report.lines);
    let line = &report.lines[0];
    assert!(
        line.starts_with("::error::não foi possível ler '"),
        "{line:?}"
    );
    assert!(line.contains("nao-existe.wxs"), "{line:?}");
    assert!(line.ends_with("': arquivo não encontrado"), "{line:?}");
}

#[test]
fn a_truncated_xml_is_reported_as_invalid_and_fails() {
    let dir = tempfile::tempdir().expect("pasta temporária");
    let path = written(&dir, "<Wix><Package");
    let report = run_on(&path);
    assert!(!report.ok);
    assert_eq!(report.lines.len(), 1, "{:#?}", report.lines);
    let line = &report.lines[0];
    assert!(line.starts_with("::error::'"), "{line:?}");
    assert!(
        line.contains("duplicata.wxs' não é XML válido: "),
        "{line:?}"
    );
}

#[test]
fn an_invalid_xml_error_carries_the_position_given_by_the_reader() {
    let dir = tempfile::tempdir().expect("pasta temporária");
    let path = written(&dir, "<Wix>\n  <Package></Wix>");
    let report = run_on(&path);
    assert!(!report.ok);
    assert_eq!(report.lines.len(), 1, "{:#?}", report.lines);
    let line = &report.lines[0];
    assert!(line.contains("não é XML válido: "), "{line:?}");
    // O roxmltree informa a posição como linha:coluna.
    assert!(line.contains(" at 2:"), "{line:?}");
}

#[test]
fn each_violation_becomes_one_error_annotation_and_the_command_fails() {
    let dir = tempfile::tempdir().expect("pasta temporária");
    let xml = variant(r#"Scope="perUser""#, r#"Scope="perMachine""#)
        .replace(r#"EmbedCab="yes""#, r#"EmbedCab="no""#);
    let expected: Vec<String> = violations(&xml)
        .iter()
        .map(|v| format!("::error::{v}"))
        .collect();
    assert_eq!(expected.len(), 2);
    let report = run_on(&written(&dir, &xml));
    assert_eq!(report.lines, expected);
    assert!(!report.ok);
}

#[test]
fn a_valid_file_prints_only_the_ok_line_listing_the_six_rules() {
    let dir = tempfile::tempdir().expect("pasta temporária");
    let report = run_on(&written(&dir, VERSIONED));
    assert_eq!(report.lines, vec![OK_LINE.to_owned()]);
    assert!(report.ok);
    assert!(
        !OK_LINE.contains('\n'),
        "{OK_LINE:?} ocupa mais de uma linha"
    );
    for part in [
        "OK: ",
        "Package/@Scope=perUser",
        "ServiceInstall/ServiceControl",
        "Restart Manager",
        "MajorUpgrade",
        "cabinet embutido",
        "ARPPRODUCTICON",
    ] {
        assert!(OK_LINE.contains(part), "{OK_LINE:?} não cita {part:?}");
    }
}

// Guarda: o `.wxs` versionado, lido do disco como o comando do CI o lê.

#[test]
fn the_versioned_installer_keeps_the_six_invariants() {
    let wxs = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("installer")
        .join("duplicata.wxs");
    let report = run_on(&wxs);
    assert!(report.ok, "{}", report.lines.join("\n"));
}

// Casos de borda do leitor de XML.

#[test]
fn service_install_inside_an_xml_comment_is_not_reported() {
    let xml = variant(
        "<Feature ",
        "<!-- <ServiceInstall Name=\"x\"/> --><Feature ",
    );
    assert_eq!(violations(&xml), Vec::new());
}

#[test]
fn an_element_in_another_namespace_does_not_count_as_the_wix_element() {
    let xml = variant(
        r#"<Media Id="1" Cabinet="duplicata.cab" EmbedCab="yes" />"#,
        r#"<x:Media xmlns:x="urn:outro" EmbedCab="yes" />"#,
    );
    assert_eq!(violations(&xml), vec![Violation::MediaMissing]);
}

#[test]
fn a_namespaced_attribute_does_not_count_as_the_plain_attribute() {
    let xml = variant(r#"Scope="perUser""#, r#"xmlns:q="urn:q" q:Scope="perUser""#);
    assert_eq!(
        violations(&xml),
        vec![Violation::ScopeWrong { found: None }]
    );
}

#[test]
fn a_well_formed_wxs_with_a_doctype_is_checked_like_any_other() {
    // O ElementTree do script anterior aceitava DOCTYPE; só XML mal formado é recusado.
    let xml = variant(
        r#"<?xml version="1.0" encoding="utf-8"?>"#,
        r#"<?xml version="1.0" encoding="utf-8"?><!DOCTYPE Wix>"#,
    );
    assert_eq!(check(&xml), Ok(Vec::new()));
}

#[test]
fn the_root_element_itself_is_not_searched() {
    // Como `.//w:Package` no script anterior: só os descendentes da raiz contam.
    let xml = format!(r#"<Package xmlns="{}" Scope="perUser"/>"#, check::WIX_V4_NS);
    assert_eq!(violations(&xml), every_rule_missing());
}
