//! Núcleo da verificação das invariantes do `installer/duplicata.wxs`.
//!
//! Só `run` lê arquivo; o resto trabalha sobre o texto. Os elementos são
//! procurados no namespace do WiX v4 em qualquer profundidade, e os atributos
//! sem namespace, com comparação exata (sensível a maiúsculas).

use std::ffi::OsString;
use std::path::Path;
use std::{fmt, fs, io};

use roxmltree::{Document, Node, ParsingOptions};

/// Namespace do WiX v4.
pub const WIX_V4_NS: &str = "http://wixtoolset.org/schemas/v4/wxs";

/// Linha impressa quando as seis regras passam.
pub const OK_LINE: &str = "OK: Package/@Scope=perUser, nenhum ServiceInstall/ServiceControl, \
     Restart Manager desabilitado, MajorUpgrade recusa downgrade, \
     cabinet embutido no .msi, ARPPRODUCTICON aponta para <Icon> declarado.";

/// Uma regra quebrada.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Violation {
    PackageMissing,
    ScopeWrong { found: Option<String> },
    ServiceElement { tag: &'static str },
    RestartManagerNotDisabled { found: Option<String> },
    MajorUpgradeMissing,
    AllowDowngradesWrong { found: Option<String> },
    MediaMissing,
    EmbedCabWrong { found: Option<String> },
    ArpIconMissing,
    ArpIconUndeclared { value: String },
}

/// `é 'valor'`, ou `está ausente` quando o atributo não existe.
struct Found<'a>(&'a Option<String>);

impl fmt::Display for Found<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.0 {
            Some(value) => write!(f, "é '{value}'"),
            None => f.write_str("está ausente"),
        }
    }
}

impl fmt::Display for Violation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::PackageMissing => f.write_str(
                "elemento <Package> não encontrado — sem ele não há como garantir \
                 a instalação por usuário, sem elevação",
            ),
            Self::ScopeWrong { found } => write!(
                f,
                "Package/@Scope {}, esperado 'perUser' — a instalação deixaria de \
                 ser por usuário e passaria a pedir elevação de administrador",
                Found(found)
            ),
            Self::ServiceElement { tag } => write!(
                f,
                "elemento <{tag}> encontrado — o duplicata não pode ser registrado \
                 como serviço do Windows"
            ),
            Self::RestartManagerNotDisabled { found } => {
                f.write_str("Property Id=MSIRESTARTMANAGERCONTROL Value=Disable ausente")?;
                if let Some(value) = found {
                    write!(f, " (encontrado Value='{value}')")?;
                }
                f.write_str(
                    " — com o Restart Manager ligado, o instalador fecha o duplicata \
                     como fim de sessão, ele pula a re-encriptação e a cópia de \
                     trabalho do histórico fica em texto claro no disco",
                )
            }
            Self::MajorUpgradeMissing => f.write_str(
                "elemento <MajorUpgrade> ausente — sem ele nada impede instalar uma \
                 versão mais antiga por cima e deixar o histórico protegido ilegível",
            ),
            Self::AllowDowngradesWrong { found } => write!(
                f,
                "MajorUpgrade/@AllowDowngrades {}, esperado 'no' — um downgrade \
                 acidental pode deixar o histórico protegido ilegível",
                Found(found)
            ),
            Self::MediaMissing => f.write_str(
                "elemento <Media> ausente — sem EmbedCab=\"yes\" o cabinet sai fora \
                 do .msi, e quem baixar só o .msi da release não consegue instalar",
            ),
            Self::EmbedCabWrong { found } => write!(
                f,
                "Media/@EmbedCab {}, esperado 'yes' — o instalador geraria um \
                 cabinet externo (ex.: cab1.cab), e quem baixar só o .msi da release \
                 não consegue instalar",
                Found(found)
            ),
            Self::ArpIconMissing => f.write_str(
                "Property Id=ARPPRODUCTICON ausente — o duplicata apareceria com ícone \
                 genérico em Aplicativos instalados e em Programas e Recursos",
            ),
            Self::ArpIconUndeclared { value } => write!(
                f,
                "ARPPRODUCTICON aponta para '{value}', que não é o Id de nenhum <Icon> \
                 declarado — o MSI não teria o ícone, e o duplicata apareceria com \
                 ícone genérico em Aplicativos instalados"
            ),
        }
    }
}

/// Todos os elementos `name` do namespace WiX v4, em ordem de documento.
fn elements<'a, 'input>(
    doc: &'a Document<'input>,
    name: &'static str,
) -> impl Iterator<Item = Node<'a, 'input>> + 'a {
    // Como `.//w:X`: só os descendentes da raiz, não ela mesma.
    doc.root_element()
        .descendants()
        .skip(1)
        .filter(move |n| n.tag_name().namespace() == Some(WIX_V4_NS) && n.tag_name().name() == name)
}

fn first<'a, 'input>(doc: &'a Document<'input>, name: &'static str) -> Option<Node<'a, 'input>> {
    elements(doc, name).next()
}

/// Os `<Property>` com o `Id` dado, em ordem de documento.
fn properties<'a, 'input>(
    doc: &'a Document<'input>,
    id: &'static str,
) -> impl Iterator<Item = Node<'a, 'input>> + 'a {
    elements(doc, "Property").filter(move |p| plain(*p, "Id") == Some(id))
}

/// O atributo `name` sem namespace. O `attribute(name)` do roxmltree também
/// aceitaria `q:name` de outro namespace; o ElementTree do script anterior não.
fn plain<'a>(node: Node<'a, '_>, name: &str) -> Option<&'a str> {
    node.attributes()
        .find(|a| a.namespace().is_none() && a.name() == name)
        .map(|a| a.value())
}

fn attr(node: Node<'_, '_>, name: &str) -> Option<String> {
    plain(node, name).map(str::to_owned)
}

/// Confere as seis regras e devolve as quebradas, na ordem das regras.
pub fn check(xml: &str) -> Result<Vec<Violation>, roxmltree::Error> {
    // DOCTYPE é aceito como no script anterior; o roxmltree limita a expansão de entidades.
    let options = ParsingOptions {
        allow_dtd: true,
        ..ParsingOptions::default()
    };
    let doc = Document::parse_with_options(xml, options)?;
    let mut violations = Vec::new();

    match first(&doc, "Package") {
        None => violations.push(Violation::PackageMissing),
        Some(package) => {
            let scope = attr(package, "Scope");
            if scope.as_deref() != Some("perUser") {
                violations.push(Violation::ScopeWrong { found: scope });
            }
        }
    }

    for tag in ["ServiceInstall", "ServiceControl"] {
        if first(&doc, tag).is_some() {
            violations.push(Violation::ServiceElement { tag });
        }
    }

    // Basta uma Property certa; o valor citado no erro é o da primeira com esse Id.
    let restart_manager: Vec<Node> = properties(&doc, "MSIRESTARTMANAGERCONTROL").collect();
    if !restart_manager
        .iter()
        .any(|p| plain(*p, "Value") == Some("Disable"))
    {
        violations.push(Violation::RestartManagerNotDisabled {
            found: restart_manager.first().and_then(|p| attr(*p, "Value")),
        });
    }

    match first(&doc, "MajorUpgrade") {
        None => violations.push(Violation::MajorUpgradeMissing),
        Some(upgrade) => {
            let allow = attr(upgrade, "AllowDowngrades");
            if allow.as_deref() != Some("no") {
                violations.push(Violation::AllowDowngradesWrong { found: allow });
            }
        }
    }

    match first(&doc, "Media") {
        None => violations.push(Violation::MediaMissing),
        Some(media) => {
            let embed = attr(media, "EmbedCab");
            if embed.as_deref() != Some("yes") {
                violations.push(Violation::EmbedCabWrong { found: embed });
            }
        }
    }

    // Value vazio conta como ausente.
    let arp_icon = properties(&doc, "ARPPRODUCTICON")
        .next()
        .and_then(|p| plain(p, "Value"))
        .filter(|value| !value.is_empty());
    match arp_icon {
        None => violations.push(Violation::ArpIconMissing),
        Some(value) => {
            if !elements(&doc, "Icon").any(|icon| plain(icon, "Id") == Some(value)) {
                violations.push(Violation::ArpIconUndeclared {
                    value: value.to_owned(),
                });
            }
        }
    }

    Ok(violations)
}

/// O que o comando imprime e se terminou bem.
#[derive(Debug)]
pub struct Report {
    pub lines: Vec<String>,
    pub ok: bool,
}

/// Uso correto do comando.
pub const USAGE: &str =
    "uso: cargo run -p duplicata-core --example wxs_invariants -- <caminho para o .wxs>";

impl Report {
    fn failure(lines: Vec<String>) -> Self {
        Self { lines, ok: false }
    }
}

fn annotation(message: impl fmt::Display) -> String {
    format!("::error::{message}")
}

/// Lê o `.wxs` indicado pelo único argumento e monta o relatório: uma linha
/// `::error::` por problema, ou só [`OK_LINE`]. Arquivo ilegível ou XML
/// inválido param antes das regras.
pub fn run(args: impl IntoIterator<Item = OsString>) -> Report {
    let mut args = args.into_iter();
    let (Some(arg), None) = (args.next(), args.next()) else {
        return Report::failure(vec![annotation(USAGE)]);
    };
    let path = Path::new(&arg);

    let xml = match fs::read_to_string(path) {
        Ok(xml) => xml,
        Err(e) => {
            let reason = match e.kind() {
                io::ErrorKind::NotFound => "arquivo não encontrado".to_owned(),
                _ => e.to_string(),
            };
            return Report::failure(vec![annotation(format_args!(
                "não foi possível ler '{}': {reason}",
                path.display()
            ))]);
        }
    };

    match check(&xml) {
        Err(e) => Report::failure(vec![annotation(format_args!(
            "'{}' não é XML válido: {e}",
            path.display()
        ))]),
        Ok(violations) if violations.is_empty() => Report {
            lines: vec![OK_LINE.to_owned()],
            ok: true,
        },
        Ok(violations) => Report::failure(violations.iter().map(annotation).collect()),
    }
}
