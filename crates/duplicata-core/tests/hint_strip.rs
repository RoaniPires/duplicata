use duplicata_core::hint_strip::{hint_line, hints, HINT_STRIP_PX, SEPARATOR};

#[test]
fn every_combination_is_written_in_the_fr034d_convention() {
    let line = hint_line();
    assert!(line.contains("Ctrl+P"), "faltou Ctrl+P em {line:?}");
    assert!(
        line.contains("Shift+Enter"),
        "faltou Shift+Enter em {line:?}"
    );
    assert!(!line.contains("ctrl+"), "grafia minúscula em {line:?}");
    assert!(!line.contains("shift+"), "grafia minúscula em {line:?}");
    assert!(!line.contains(" + "), "`+` com espaços em {line:?}");
}

#[test]
fn the_modifier_order_is_ctrl_then_shift_then_alt() {
    for h in hints() {
        if let (Some(c), Some(s)) = (h.keys.find("Ctrl"), h.keys.find("Shift")) {
            assert!(c < s, "Shift antes de Ctrl em {:?}", h.keys);
        }
        if let (Some(s), Some(a)) = (h.keys.find("Shift"), h.keys.find("Alt")) {
            assert!(s < a, "Alt antes de Shift em {:?}", h.keys);
        }
    }
}

#[test]
fn the_type_filter_is_advertised_as_tab_plus_arrows_never_tab_alone() {
    let type_hint = hints()
        .into_iter()
        .find(|h| h.action == "tipo")
        .expect("o rodapé tem de anunciar o filtro de tipo");
    assert_eq!(type_hint.keys, "Tab+←→");
    assert_ne!(
        type_hint.keys, "Tab",
        "`Tab` sozinho não filtra nada — ele só leva o foco à faixa (FR-034)"
    );
    assert!(
        hint_line().contains("Tab+←→ tipo"),
        "a linha do rodapé tem de trazer as duas teclas: {:?}",
        hint_line()
    );
}

#[test]
fn no_hint_advertises_a_bare_tab_for_the_type_filter() {
    for h in hints() {
        assert_ne!(h.keys, "Tab", "dica com `Tab` sozinho: {h:?}");
    }
}

#[test]
fn the_strip_lists_the_six_actions_of_the_contract_in_order() {
    let actions: Vec<&str> = hints().iter().map(|h| h.action).collect();
    assert_eq!(
        actions,
        vec!["navegar", "colar", "colar texto", "fixar", "tipo", "fechar"]
    );
}

#[test]
fn the_arrow_keys_of_navigation_are_shown_as_a_pair() {
    let nav = hints().into_iter().find(|h| h.action == "navegar").unwrap();
    assert_eq!(nav.keys, "↑↓");
}

#[test]
fn the_line_joins_every_hint_with_the_middle_dot_separator() {
    let line = hint_line();
    assert_eq!(line.matches(SEPARATOR).count(), hints().len() - 1);
    assert!(line.starts_with("↑↓ navegar"));
    assert!(line.ends_with("Esc fechar"));
}

#[test]
fn deleting_the_whole_history_is_not_in_the_strip_and_has_no_shortcut() {
    let line = hint_line().to_lowercase();
    for forbidden in ["apagar", "excluir", "limpar", "delete", "del"] {
        assert!(
            !line.contains(forbidden),
            "\"Apagar todo o histórico\" não pode ter atalho nem aparecer no \
             rodapé (FR-034c) — achei {forbidden:?} em {line:?}"
        );
    }
}

#[test]
fn the_strip_has_a_fixed_positive_height() {
    const {
        assert!(HINT_STRIP_PX > 0);
        assert!(HINT_STRIP_PX < duplicata_core::row_layout::ROW_HEIGHT_PX);
    }
}

#[test]
fn the_window_is_wide_enough_for_the_whole_hint_line() {
    use duplicata_core::hint_strip::min_client_width_px;
    use duplicata_core::row_layout::{WINDOW_FRAME_WIDTH_PX, WINDOW_WIDTH_PX};

    let client = WINDOW_WIDTH_PX - WINDOW_FRAME_WIDTH_PX;
    let needed = min_client_width_px();
    assert!(
        client >= needed,
        "a linha do rodapé precisa de {needed}px de cliente e a janela de \
         {WINDOW_WIDTH_PX}px só oferece {client}px — ela sairia cortada por \
         `DT_END_ELLIPSIS`, escondendo o fim (`Tab+←→ tipo · Esc fechar`) e \
         violando o \"legível e alinhado à janela\" do FR-032."
    );
}

#[test]
fn the_slack_after_the_hint_line_lands_in_the_intended_band() {
    use duplicata_core::hint_strip::{
        HINT_LINE_SLACK_PX, MAX_HINT_SLACK_PX, MEASURED_HINT_LINE_PX, SIDE_PADDING_PX,
    };
    use duplicata_core::row_layout::{WINDOW_FRAME_WIDTH_PX, WINDOW_WIDTH_PX};

    let text_area = WINDOW_WIDTH_PX - WINDOW_FRAME_WIDTH_PX - SIDE_PADDING_PX;
    let slack = text_area - MEASURED_HINT_LINE_PX;
    assert!(
        slack >= HINT_LINE_SLACK_PX,
        "só {slack}px de folga depois do rodapé (mínimo {HINT_LINE_SLACK_PX}px) \
         — uma escala de texto do Windows já cortaria o fim da linha"
    );
    assert!(
        slack <= MAX_HINT_SLACK_PX,
        "{slack}px de folga é faixa vazia demais (teto {MAX_HINT_SLACK_PX}px)"
    );
}

#[test]
fn the_window_is_not_wider_than_the_hint_line_actually_needs() {
    use duplicata_core::hint_strip::{min_client_width_px, MAX_HINT_SLACK_PX};
    use duplicata_core::row_layout::{WINDOW_FRAME_WIDTH_PX, WINDOW_WIDTH_PX};

    let client = WINDOW_WIDTH_PX - WINDOW_FRAME_WIDTH_PX;
    let slack = client - min_client_width_px();
    assert!(
        slack <= MAX_HINT_SLACK_PX,
        "sobram {slack}px de folga além do que o rodapé precisa — a janela \
         está mais larga que o conteúdo (o defeito da largura estimada)."
    );
}

#[test]
fn the_measured_width_is_consistent_with_the_line_it_measures() {
    use duplicata_core::hint_strip::{MEASURED_HINT_LINE_PX, SIDE_PADDING_PX};
    let chars = hint_line().chars().count() as i32;
    assert!(
        MEASURED_HINT_LINE_PX >= chars * 3,
        "medida de {MEASURED_HINT_LINE_PX}px para {chars} caracteres é estreita \
         demais para qualquer fonte de interface — remedir"
    );
    assert!(
        MEASURED_HINT_LINE_PX <= chars * 9,
        "medida de {MEASURED_HINT_LINE_PX}px para {chars} caracteres é larga \
         demais — remedir"
    );
    const {
        assert!(SIDE_PADDING_PX > 0);
    }
}
