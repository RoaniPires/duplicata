//! A decisão tomada antes de abrir a área de transferência quando o dono é o
//! OLE. As listas abaixo são as medidas com o exemplo `clipboard_trace` numa
//! cópia do Claude Code (`Set-Clipboard` do PowerShell 5.1).

use duplicata_core::canonical::{CF_LOCALE, CF_OEMTEXT, CF_TEXT, CF_UNICODETEXT};
use duplicata_core::capture::FormatAnnounce;
use duplicata_core::{
    is_ole_bookkeeping_only, may_open, owner_must_answer_first, OwnerState, OLE_DATA_OBJECT,
    OLE_PRIVATE_DATA,
};

const DATA_OBJECT_ID: u32 = 49161;
const OLE_PRIVATE_ID: u32 = 49171;

fn std_fmt(id: u32) -> FormatAnnounce {
    FormatAnnounce {
        format_id: id,
        format_name: None,
    }
}

fn named(id: u32, name: &str) -> FormatAnnounce {
    FormatAnnounce {
        format_id: id,
        format_name: Some(name.to_string()),
    }
}

fn data_object() -> FormatAnnounce {
    named(DATA_OBJECT_ID, OLE_DATA_OBJECT)
}

fn ole_private() -> FormatAnnounce {
    named(OLE_PRIVATE_ID, OLE_PRIVATE_DATA)
}

/// u1: só os dois formatos de controle.
fn u1() -> Vec<FormatAnnounce> {
    vec![data_object(), ole_private()]
}

/// u2 e u3 têm a mesma lista; muda só se os bytes são adiados ou reais.
fn u2() -> Vec<FormatAnnounce> {
    vec![
        data_object(),
        std_fmt(CF_UNICODETEXT),
        ole_private(),
        std_fmt(CF_LOCALE),
        std_fmt(CF_TEXT),
        std_fmt(CF_OEMTEXT),
    ]
}

/// O que sobrou nas cópias perdidas, sem dono.
fn orphan() -> Vec<FormatAnnounce> {
    vec![data_object(), ole_private(), std_fmt(CF_LOCALE)]
}

/// A escrita do Windows Terminal (OSC 52), sem OLE.
fn windows_terminal() -> Vec<FormatAnnounce> {
    vec![
        std_fmt(CF_UNICODETEXT),
        std_fmt(CF_LOCALE),
        std_fmt(CF_TEXT),
        std_fmt(CF_OEMTEXT),
    ]
}

#[test]
fn the_first_ole_update_is_bookkeeping_only() {
    assert!(is_ole_bookkeeping_only(&u1()));
}

#[test]
fn the_orphan_state_left_after_a_lost_flush_is_bookkeeping_only() {
    assert!(is_ole_bookkeeping_only(&orphan()));
}

#[test]
fn a_state_with_text_is_content_even_with_ole_formats() {
    assert!(!is_ole_bookkeeping_only(&u2()));
}

#[test]
fn a_write_without_ole_formats_is_content() {
    assert!(!is_ole_bookkeeping_only(&windows_terminal()));
}

#[test]
fn an_unknown_registered_format_alone_is_not_ole_bookkeeping() {
    // Fronteira com a etapa 2: um formato registrado qualquer continua sendo
    // capturado até o filtro de "[sem prévia]" existir.
    assert!(!is_ole_bookkeeping_only(&[named(0xC0A1, "Formato Do App")]));
    assert!(!is_ole_bookkeeping_only(&[std_fmt(0xC0A1)]));
}

#[test]
fn locale_alone_is_not_ole_bookkeeping() {
    assert!(!is_ole_bookkeeping_only(&[std_fmt(CF_LOCALE)]));
}

#[test]
fn an_empty_list_is_not_ole_bookkeeping() {
    assert!(!is_ole_bookkeeping_only(&[]));
}

#[test]
fn ole_names_are_recognized_by_name_not_by_id() {
    // Os ids registrados mudam de sessão para sessão; o nome não.
    assert!(is_ole_bookkeeping_only(&[named(0xC123, OLE_DATA_OBJECT)]));
    assert!(!is_ole_bookkeeping_only(&[std_fmt(DATA_OBJECT_ID)]));
}

#[test]
fn an_ole_owner_must_answer_before_the_clipboard_is_opened() {
    assert!(owner_must_answer_first(&u2()));
    assert!(owner_must_answer_first(&u1()));
}

#[test]
fn a_non_ole_owner_is_read_without_asking() {
    assert!(!owner_must_answer_first(&windows_terminal()));
    assert!(!owner_must_answer_first(&[
        ole_private(),
        std_fmt(CF_UNICODETEXT)
    ]));
    assert!(!owner_must_answer_first(&[]));
}

#[test]
fn a_similar_name_does_not_count_as_ole() {
    assert!(!owner_must_answer_first(&[
        named(0xC200, "DataObjectX"),
        named(0xC201, "dataobject"),
        std_fmt(CF_UNICODETEXT),
    ]));
}

#[test]
fn may_open_only_refuses_an_ole_owner_that_does_not_answer() {
    use OwnerState::{NotResponding, Responsive, Unknown};
    let table = [
        (true, Responsive, true),
        (true, NotResponding, false),
        // Sem saber (sem dono, janela morta, erro): abre como antes.
        (true, Unknown, true),
        (false, Responsive, true),
        (false, NotResponding, true),
        (false, Unknown, true),
    ];
    for (needs_probe, state, expected) in table {
        assert_eq!(
            may_open(needs_probe, state),
            expected,
            "may_open({needs_probe}, {state:?})"
        );
    }
}
