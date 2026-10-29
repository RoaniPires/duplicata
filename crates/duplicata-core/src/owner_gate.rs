//! O que o duplicata decide antes de abrir a área de transferência quando o
//! dono dela é o OLE (`OleSetClipboard`).
//!
//! O `OleSetClipboard` publica em três atualizações: primeiro só os formatos de
//! controle ("DataObject" e "Ole Private Data"), depois os formatos de conteúdo
//! adiados, e por fim o flush, com os bytes. Entre elas, a thread do dono tenta
//! reabrir a área sem bombear mensagens: quem a segura e pede um formato adiado
//! espera por um dono que não pode responder, e o flush não acontece.

use crate::canonical::CF_LOCALE;
use crate::capture::FormatAnnounce;

/// Formato de controle que o OLE publica junto com toda cópia.
pub const OLE_DATA_OBJECT: &str = "DataObject";
/// Outro formato de controle do OLE.
pub const OLE_PRIVATE_DATA: &str = "Ole Private Data";

/// Verdadeiro quando a área só tem formatos de controle do OLE (e, no máximo,
/// `CF_LOCALE`): não há conteúdo para guardar.
///
/// O OLE se reconhece pelo nome do formato, não pelo id: os ids registrados
/// mudam de sessão para sessão. `CF_LOCALE` sozinho não conta, e um formato
/// registrado qualquer também não (esse continua sendo conteúdo).
pub fn is_ole_bookkeeping_only(announces: &[FormatAnnounce]) -> bool {
    let has_ole = announces.iter().any(is_ole_format);
    has_ole
        && announces
            .iter()
            .all(|f| is_ole_format(f) || f.format_id == CF_LOCALE)
}

fn is_ole_format(f: &FormatAnnounce) -> bool {
    matches!(
        f.format_name.as_deref(),
        Some(OLE_DATA_OBJECT) | Some(OLE_PRIVATE_DATA)
    )
}

/// Verdadeiro quando o dono é o OLE e precisa responder antes de a área ser
/// aberta.
pub fn owner_must_answer_first(announces: &[FormatAnnounce]) -> bool {
    announces
        .iter()
        .any(|f| f.format_name.as_deref() == Some(OLE_DATA_OBJECT))
}

/// Como o dono respondeu à sonda.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OwnerState {
    Responsive,
    NotResponding,
    Unknown,
}

/// Se a área pode ser aberta agora.
///
/// Só recusa o dono que precisava responder e não respondeu. Sem saber
/// (`Unknown`: sem dono, janela já destruída, erro da sonda), abre como antes:
/// na dúvida, capturar.
pub fn may_open(needs_probe: bool, state: OwnerState) -> bool {
    !(needs_probe && state == OwnerState::NotResponding)
}
