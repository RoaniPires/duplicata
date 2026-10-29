//! As fases da captura: o que cada uma pode decidir, e com que informação.
//!
//! A ordem é anúncio -> filtros -> handles/tamanhos -> gate de tamanho ->
//! cópia. Cada teste aqui prende um pedaço dessa ordem.

use duplicata_core::canonical::{
    gate_size, screen, Decision, RejectReason, CF_DIB, CF_LOCALE, CF_UNICODETEXT,
    SENSITIVE_FORMAT_NAME,
};
use duplicata_core::capture::FormatAnnounce;
use duplicata_core::{
    copied_or_empty, CanonicalKind, CaptureOutcome, CapturedFormat, Config, OLE_DATA_OBJECT,
    OLE_PRIVATE_DATA,
};

const MIB: u64 = 1024 * 1024;

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn announce(id: u32) -> FormatAnnounce {
    FormatAnnounce {
        format_id: id,
        format_name: None,
    }
}

fn captured(id: u32, n: usize) -> CapturedFormat {
    CapturedFormat {
        format_id: id,
        format_name: None,
        bytes: vec![0u8; n],
    }
}

#[test]
fn screen_rejects_when_no_format_can_be_canonical_and_reports_the_ids() {
    let so_auxiliares = vec![announce(1), announce(7), announce(16)];
    assert_eq!(
        screen(&so_auxiliares, None, &cfg()),
        Decision::Reject(RejectReason::NoCanonicalFormat {
            format_ids: vec![1, 7, 16]
        })
    );
}

#[test]
fn screen_on_an_empty_announcement_rejects_without_panicking() {
    assert_eq!(
        screen(&[], None, &cfg()),
        Decision::Reject(RejectReason::NoCanonicalFormat { format_ids: vec![] })
    );
}

#[test]
fn gate_size_uses_the_limit_of_the_canonical_kind_not_a_single_limit() {
    // 3 MiB passa como imagem (limite 64 MiB) e não passa como texto (2 MiB).
    assert_eq!(gate_size(CF_DIB, 3 * MIB, &cfg()), None);
    assert_eq!(
        gate_size(CF_UNICODETEXT, 3 * MIB, &cfg()),
        Some(RejectReason::TooLarge {
            kind: CanonicalKind::UnicodeText,
            byte_len: 3 * MIB
        })
    );
}

#[test]
fn gate_size_accepts_exactly_the_limit() {
    assert_eq!(gate_size(CF_UNICODETEXT, 2 * MIB, &cfg()), None);
    assert!(gate_size(CF_UNICODETEXT, 2 * MIB + 1, &cfg()).is_some());
}

#[test]
fn copied_or_empty_reanchors_the_canonical_by_id_when_a_format_drops_out() {
    // O índice escolhido no anúncio valia para a lista ANUNCIADA. Se um formato
    // anterior não foi entregue, a lista copiada é mais curta e o índice antigo
    // apontaria para o formato errado — por isso a reancoragem é por id.
    //
    // O caso é montado para discriminar: anunciados [CF_TEXT, CF_UNICODETEXT],
    // `screen` escolheria índice 1; CF_TEXT não é entregue, e o índice correto
    // na lista copiada passa a ser 0. Um teste com um formato só não distingue
    // reancoragem de índice velho, porque 0 seria a resposta dos dois jeitos.
    let anunciados = vec![announce(1), announce(CF_UNICODETEXT)];
    assert_eq!(
        screen(&anunciados, None, &cfg()),
        Decision::Copy { canonical_index: 1 },
        "no anúncio o canônico está no índice 1"
    );

    let copiados = vec![captured(CF_UNICODETEXT, 6)];
    match copied_or_empty(copiados, CF_UNICODETEXT) {
        CaptureOutcome::Copied {
            formats,
            canonical_index,
        } => {
            assert_eq!(
                canonical_index, 0,
                "na lista copiada o canônico está no índice 0, não no 1 do anúncio"
            );
            assert_eq!(formats[canonical_index].format_id, CF_UNICODETEXT);
        }
        other => panic!("esperava Copied, obtido {other:?}"),
    }
}

#[test]
fn copied_or_empty_discards_everything_when_the_canonical_itself_is_missing() {
    // Degradação escolhida: silêncio em vez de afirmação errada. Sem o formato
    // canônico não há item honesto a guardar — guardar os auxiliares rotulados
    // como se fossem o conteúdo seria pior que não guardar nada.
    let so_auxiliares = vec![captured(1, 10), captured(16, 4)];
    assert_eq!(
        copied_or_empty(so_auxiliares, CF_UNICODETEXT),
        CaptureOutcome::Empty
    );
}

#[test]
fn copied_or_empty_picks_the_right_format_among_several() {
    let copiados = vec![
        captured(1, 10),
        captured(CF_UNICODETEXT, 6),
        captured(16, 4),
    ];
    match copied_or_empty(copiados, CF_UNICODETEXT) {
        CaptureOutcome::Copied {
            formats,
            canonical_index,
        } => {
            assert_eq!(canonical_index, 1);
            assert_eq!(formats.len(), 3, "os auxiliares continuam sendo guardados");
        }
        other => panic!("esperava Copied, obtido {other:?}"),
    }
}

fn named(id: u32, name: &str) -> FormatAnnounce {
    FormatAnnounce {
        format_id: id,
        format_name: Some(name.to_string()),
    }
}

/// A primeira atualização do `OleSetClipboard`: só formatos de controle.
fn ole_u1() -> Vec<FormatAnnounce> {
    vec![
        named(49161, OLE_DATA_OBJECT),
        named(49171, OLE_PRIVATE_DATA),
    ]
}

#[test]
fn screen_rejects_a_state_with_only_ole_bookkeeping_formats() {
    assert_eq!(
        screen(&ole_u1(), None, &cfg()),
        Decision::Reject(RejectReason::OleBookkeepingOnly)
    );
    let orphan = vec![
        named(49161, OLE_DATA_OBJECT),
        named(49171, OLE_PRIVATE_DATA),
        announce(CF_LOCALE),
    ];
    assert_eq!(
        screen(&orphan, None, &cfg()),
        Decision::Reject(RejectReason::OleBookkeepingOnly)
    );
}

#[test]
fn screen_still_copies_an_unknown_registered_format_alone() {
    // Fronteira com a etapa 2: só o filtro de "[sem prévia]" descarta isto.
    assert_eq!(
        screen(&[named(0xC0A1, "Formato Do App")], None, &cfg()),
        Decision::Copy { canonical_index: 0 }
    );
}

#[test]
fn a_sensitive_flag_is_rejected_even_with_ole_formats() {
    let formats = vec![
        named(49161, OLE_DATA_OBJECT),
        named(0xC300, SENSITIVE_FORMAT_NAME),
    ];
    assert_eq!(
        screen(&formats, None, &cfg()),
        Decision::Reject(RejectReason::SensitiveFlagged)
    );
}

#[test]
fn screen_reports_a_blocked_program_before_ole_bookkeeping() {
    let mut c = cfg();
    c.blocked_programs = vec!["powershell.exe".to_string()];
    assert_eq!(
        screen(&ole_u1(), Some("powershell.exe"), &c),
        Decision::Reject(RejectReason::BlockedProgram)
    );
}
