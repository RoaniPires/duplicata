use duplicata_core::settings_text::{self as text, OptionText};

#[test]
fn the_three_sections_have_the_titles_in_order() {
    assert_eq!(text::SECTION_GENERAL, "Geral");
    assert_eq!(text::SECTION_HISTORY, "Histórico");
    assert_eq!(text::SECTION_PRIVACY, "Privacidade e segurança");
}

#[test]
fn each_option_has_the_name_description_and_tip_of_the_specification() {
    let expected = [
        (
            "Atalho global",
            "Abre o histórico de qualquer lugar.",
            "Clique em Alterar e pressione a nova combinação. Esc cancela a captura.",
        ),
        (
            "Abrir com Win+V",
            "Substitui o histórico do Windows.",
            "Vale ao salvar. O Win+V passa a abrir o duplicata depois de reiniciar o Windows Explorer, o computador ou sair e entrar na conta.",
        ),
        (
            "Iniciar com o Windows",
            "Abre o duplicata quando você entra no Windows.",
            "Se o Gerenciador de Tarefas desativou a inicialização, ligar aqui remove esse bloqueio ao salvar.",
        ),
        (
            "Prazo de retenção",
            "Itens não fixados mais antigos que isso são apagados.",
            "Mínimo de 1 dia. Itens fixados nunca são apagados pela retenção. Reduzir o prazo apaga itens ao salvar.",
        ),
        (
            "Quantidade máxima de itens",
            "Acima disso, os itens não fixados mais antigos saem.",
            "Mínimo de 1. Com 1, cada cópia substitui a anterior. Itens fixados não contam. Reduzir apaga itens ao salvar.",
        ),
        (
            "Teto de itens fixados",
            "Quantos itens podem ficar fixados ao mesmo tempo.",
            "De 0 a 200. Com 0 não dá para fixar novos itens; os já fixados continuam fixados.",
        ),
        (
            "Aplicativos ignorados",
            "O que você copia nestes aplicativos não entra no histórico.",
            "A identificação é só pelo nome do executável (ex.: keepass.exe). Protege contra vazamento acidental, não contra um programa que use o mesmo nome.",
        ),
        (
            "Descartar segredos",
            "Não guarda chaves privadas, tokens e números de cartão.",
            "Quando algo é descartado, a bandeja avisa e permite recuperar.",
        ),
        (
            "Proteger com a conta do Windows",
            "Criptografa o banco contra outras contas e outros computadores.",
            "Não protege contra outro programa na sua própria sessão. Ligar ou desligar reescreve o banco ao salvar, o que pode demorar.",
        ),
    ];
    let named = [
        text::HOTKEY,
        text::WIN_V,
        text::STARTUP,
        text::RETENTION,
        text::MAX_ITEMS,
        text::MAX_PINNED,
        text::BLOCKLIST,
        text::SECRETS,
        text::PROTECTION,
    ];
    for (i, (name, description, tip)) in expected.iter().enumerate() {
        let want = OptionText {
            name,
            description,
            tip,
        };
        assert_eq!(named[i], want, "opção {i}");
        assert_eq!(
            text::OPTIONS[i],
            want,
            "OPTIONS segue a ordem visual, opção {i}"
        );
    }
}

#[test]
fn buttons_and_unit_use_the_specified_words() {
    assert_eq!(text::BUTTON_SAVE, "Salvar");
    assert_eq!(text::BUTTON_CANCEL, "Cancelar");
    assert_eq!(text::BUTTON_APPLY, "Aplicar");
    assert_eq!(text::BUTTON_ALTER, "Alterar");
    assert_eq!(text::BUTTON_ADD, "Adicionar");
    assert_eq!(text::BUTTON_REMOVE, "Remover");
    assert_eq!(text::BUTTON_BACK, "Voltar");
    assert_eq!(text::DAYS_UNIT, "dias");
    assert_eq!(text::WINDOW_TITLE, "Configurações — duplicata");
}

#[test]
fn status_messages_are_copied_from_the_specification() {
    assert_eq!(
        text::HOTKEY_ARMED,
        "Pressione a nova combinação de teclas... Esc cancela."
    );
    assert_eq!(
        text::HOTKEY_STANDALONE,
        "A configuração de atalho já está aberta na janela de conflito — resolva-a ali (ela permanece sempre visível, no topo da tela)."
    );
    assert_eq!(
        text::hotkey_none_active("Ctrl+Shift+V"),
        "Nenhum atalho está ativo. A combinação configurada, \"Ctrl+Shift+V\", não pôde ser registrada."
    );
    assert_eq!(
        text::STARTUP_BLOCKED,
        "Desativado pelo Gerenciador de Tarefas do Windows."
    );
    assert_eq!(
        text::SAVE_FAILED,
        "Não foi possível salvar as configurações."
    );
    assert_eq!(
        text::HOTKEY_PENDING,
        "Nova combinação; passa a valer ao Aplicar ou Salvar."
    );
    assert_eq!(
        text::hotkey_taken("Ctrl+Alt+H", "Ctrl+Shift+V"),
        "\"Ctrl+Alt+H\" ficou indisponível e não pôde ser registrada. O atalho continua \"Ctrl+Shift+V\"."
    );
}

#[test]
fn confirmation_lines_are_copied_from_the_specification() {
    assert_eq!(
        text::reduce_retention_line(7, 3),
        "Reduzir o prazo de retenção de 7 para 3 dias pode apagar itens antigos do histórico."
    );
    assert_eq!(
        text::reduce_items_line(500, 1),
        "Reduzir a quantidade máxima de 500 para 1 itens pode apagar itens antigos do histórico."
    );
    assert_eq!(
        text::UNBLOCK_STARTUP_LINE,
        "Ligar \"Iniciar com o Windows\" remove o bloqueio feito no Gerenciador de Tarefas."
    );
    assert_eq!(text::CANNOT_UNDO, "Isso não pode ser desfeito.");
}

#[test]
fn no_text_cites_documents_requirement_codes_or_technical_names() {
    let mut all: Vec<String> = [
        text::WINDOW_TITLE,
        text::CONFIRM_TITLE,
        text::SECTION_GENERAL,
        text::SECTION_HISTORY,
        text::SECTION_PRIVACY,
        text::DAYS_UNIT,
        text::BUTTON_SAVE,
        text::BUTTON_CANCEL,
        text::BUTTON_APPLY,
        text::BUTTON_ALTER,
        text::BUTTON_ADD,
        text::BUTTON_REMOVE,
        text::BUTTON_BACK,
        text::HOTKEY_ARMED,
        text::HOTKEY_STANDALONE,
        text::HOTKEY_PENDING,
        text::STARTUP_BLOCKED,
        text::SAVE_FAILED,
        text::UNBLOCK_STARTUP_LINE,
        text::CANNOT_UNDO,
    ]
    .into_iter()
    .map(str::to_string)
    .collect();
    for o in text::OPTIONS {
        all.extend([o.name, o.description, o.tip].map(str::to_string));
    }
    all.push(text::hotkey_none_active("Ctrl+Shift+V"));
    all.push(text::hotkey_taken("Ctrl+Alt+H", "Ctrl+Shift+V"));
    all.push(text::reduce_retention_line(7, 3));
    all.push(text::reduce_items_line(500, 100));
    all.extend(
        [
            text::WIN_RESERVED,
            text::WIN_V_HINT,
            text::WIN_V_OFF_AWAITING_RETURN,
            text::WIN_V_ENABLE_PANEL_LINE,
            text::WIN_V_DISABLE_LINE,
            text::WIN_V_DISABLE_NOW_LINE,
            text::WIN_V_WAS_DISABLED_BEFORE,
            text::WIN_V_REGISTRY_FAILED,
            text::BUTTON_RESTART_EXPLORER,
            text::RESTART_EXPLORER_CONFIRM,
            text::BUTTON_RESTART,
            text::WIN_V_FAILURE_TITLE,
            text::WIN_V_RETURNS_AFTER,
            text::BUTTON_TURN_OFF,
            text::BUTTON_KEEP,
            text::BUTTON_OK,
            text::EXPLORER_RESTART_FAILED,
            text::EXPLORER_NOT_RETURNED,
        ]
        .map(str::to_string),
    );
    for f in [
        text::win_v_awaiting,
        text::win_v_active,
        text::win_v_not_released,
        text::win_v_enable_when,
        text::win_v_failure,
    ] {
        all.push(f("Ctrl+Shift+V"));
    }

    assert!(all.iter().all(|t| !t.is_empty()), "nenhum texto vazio");
    for t in &all {
        for banned in ["FR-", "Fatia", "DPAPI", ".md", "especifica"] {
            assert!(!t.contains(banned), "{t:?} contém {banned:?}");
        }
    }
}

mod hotkey_rejection {
    use duplicata_core::{HotkeyCombo, HotkeyRejection};

    use super::text;

    #[test]
    fn a_combination_with_win_points_to_the_option() {
        let combo = HotkeyCombo::parse("win+f8").unwrap();
        assert_eq!(
            text::hotkey_rejection(&combo, HotkeyRejection::WinReserved),
            "O Windows reserva as combinações com Win. Para usar Win+V, ligue a opção \
             \"Abrir com Win+V\"."
        );
    }

    #[test]
    fn the_other_refusals_name_the_combination_and_ask_for_another() {
        let cases = [
            (
                "ctrl+alt+delete",
                HotkeyRejection::ReservedSystemCombo,
                "\"Ctrl+Alt+Delete\": essa combinação é reservada pelo sistema. Pressione outra \
                 combinação.",
            ),
            (
                "p",
                HotkeyRejection::NoModifier,
                "\"P\": nenhum modificador (Ctrl/Alt/Shift) foi pressionado. Pressione outra \
                 combinação.",
            ),
            (
                "ctrl",
                HotkeyRejection::ModifierOnly,
                "\"Ctrl\": só modificadores foram pressionados, falta uma tecla. Pressione outra \
                 combinação.",
            ),
        ];
        for (combo, rejection, want) in cases {
            let combo = HotkeyCombo::parse(combo).unwrap();
            assert_eq!(text::hotkey_rejection(&combo, rejection), want);
        }
    }
}

mod win_v {
    use super::text;

    const RESTART: &str = "reiniciar o Windows Explorer, o computador ou sair e entrar na conta";

    #[test]
    fn the_status_lines_follow_the_specification() {
        assert_eq!(text::RESTART_PHRASE, RESTART);
        assert_eq!(
            text::win_v_awaiting("Ctrl+Shift+V"),
            format!("O Win+V passa a valer depois de {RESTART}. Até lá: Ctrl+Shift+V.")
        );
        assert_eq!(
            text::win_v_active("Ctrl+Alt+V"),
            "Em uso: Win+V (reserva: Ctrl+Alt+V)"
        );
        assert_eq!(
            text::win_v_not_released("Ctrl+Shift+V"),
            "O Windows não liberou o Win+V. Em uso: Ctrl+Shift+V."
        );
        assert_eq!(
            text::WIN_V_OFF_AWAITING_RETURN,
            format!(
                "O Win+V volta ao histórico do Windows depois de {RESTART}. Até lá, o Win+V não faz nada."
            )
        );
    }

    #[test]
    fn the_hint_and_the_refusal_point_to_the_option() {
        assert_eq!(
            text::WIN_V_HINT,
            "Para usar Win+V, ligue a opção \"Abrir com Win+V\""
        );
        assert_eq!(
            text::WIN_RESERVED,
            "O Windows reserva as combinações com Win. Para usar Win+V, ligue a opção \"Abrir com Win+V\"."
        );
    }

    #[test]
    fn the_confirmations_follow_the_specification() {
        assert_eq!(
            text::WIN_V_ENABLE_PANEL_LINE,
            "O painel de histórico do Windows deixa de abrir pelo Win+V."
        );
        assert_eq!(
            text::win_v_enable_when("Ctrl+Shift+V"),
            format!(
                "O Win+V passa a abrir o duplicata depois de {RESTART}. Até lá, continua valendo Ctrl+Shift+V."
            )
        );
        assert_eq!(
            text::WIN_V_DISABLE_LINE,
            format!(
                "O Win+V para de abrir o duplicata agora e volta ao histórico do Windows depois de {RESTART}. Até lá, o Win+V não faz nada."
            )
        );
        assert_eq!(
            text::WIN_V_DISABLE_NOW_LINE,
            "O Win+V para de abrir o duplicata agora."
        );
        assert_eq!(
            text::WIN_V_WAS_DISABLED_BEFORE,
            "O Win+V já estava desativado no Windows antes do duplicata e continua assim."
        );
        assert_eq!(
            text::WIN_V_REGISTRY_FAILED,
            "Não foi possível alterar o atalho do Windows; nada mudou."
        );
    }

    #[test]
    fn the_restart_button_and_its_confirmation_follow_the_specification() {
        assert_eq!(
            text::BUTTON_RESTART_EXPLORER,
            "Reiniciar o Windows Explorer agora"
        );
        assert_eq!(
            text::RESTART_EXPLORER_CONFIRM,
            "O Windows Explorer será reiniciado. As janelas de pasta abertas serão fechadas e cópias de arquivo em andamento nelas serão canceladas."
        );
        assert_eq!(text::BUTTON_RESTART, "Reiniciar");
    }

    #[test]
    fn the_failure_window_follows_the_specification() {
        assert_eq!(
            text::win_v_failure("Ctrl+Shift+V"),
            "O Windows não liberou o Win+V. O duplicata continua abrindo com Ctrl+Shift+V."
        );
        assert_eq!(
            text::WIN_V_RETURNS_AFTER,
            format!("O Win+V volta ao histórico do Windows depois de {RESTART}.")
        );
        assert_eq!(text::BUTTON_TURN_OFF, "Desligar a opção");
        assert_eq!(text::BUTTON_KEEP, "Manter");
        assert_eq!(text::BUTTON_OK, "OK");
        assert!(!text::WIN_V_FAILURE_TITLE.is_empty());
    }
}
