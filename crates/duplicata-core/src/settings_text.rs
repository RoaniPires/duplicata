//! Textos da janela de Configurações e da confirmação do Salvar, num lugar
//! só para que o teste confira cada um contra a especificação.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OptionText {
    pub name: &'static str,
    pub description: &'static str,
    /// Detalhe mostrado na dica de ferramenta da linha.
    pub tip: &'static str,
}

pub const WINDOW_TITLE: &str = "Configurações — duplicata";
pub const CONFIRM_TITLE: &str = "Confirmar — duplicata";

pub const SECTION_GENERAL: &str = "Geral";
pub const SECTION_HISTORY: &str = "Histórico";
pub const SECTION_PRIVACY: &str = "Privacidade e segurança";

pub const HOTKEY: OptionText = OptionText {
    name: "Atalho global",
    description: "Abre o histórico de qualquer lugar.",
    tip: "Clique em Alterar e pressione a nova combinação. Esc cancela a captura.",
};
pub const STARTUP: OptionText = OptionText {
    name: "Iniciar com o Windows",
    description: "Abre o duplicata quando você entra no Windows.",
    tip: "Se o Gerenciador de Tarefas desativou a inicialização, ligar aqui remove esse \
          bloqueio ao salvar.",
};
pub const RETENTION: OptionText = OptionText {
    name: "Prazo de retenção",
    description: "Itens não fixados mais antigos que isso são apagados.",
    tip: "Mínimo de 1 dia. Itens fixados nunca são apagados pela retenção. Reduzir o prazo \
          apaga itens ao salvar.",
};
pub const MAX_ITEMS: OptionText = OptionText {
    name: "Quantidade máxima de itens",
    description: "Acima disso, os itens não fixados mais antigos saem.",
    tip: "Mínimo de 1. Com 1, cada cópia substitui a anterior. Itens fixados não contam. \
          Reduzir apaga itens ao salvar.",
};
pub const MAX_PINNED: OptionText = OptionText {
    name: "Teto de itens fixados",
    description: "Quantos itens podem ficar fixados ao mesmo tempo.",
    tip: "De 0 a 200. Com 0 não dá para fixar novos itens; os já fixados continuam fixados.",
};
pub const BLOCKLIST: OptionText = OptionText {
    name: "Aplicativos ignorados",
    description: "O que você copia nestes aplicativos não entra no histórico.",
    tip: "A identificação é só pelo nome do executável (ex.: keepass.exe). Protege contra \
          vazamento acidental, não contra um programa que use o mesmo nome.",
};
pub const SECRETS: OptionText = OptionText {
    name: "Descartar segredos",
    description: "Não guarda chaves privadas, tokens e números de cartão.",
    tip: "Quando algo é descartado, a bandeja avisa e permite recuperar.",
};
pub const PROTECTION: OptionText = OptionText {
    name: "Proteger com a conta do Windows",
    description: "Criptografa o banco contra outras contas e outros computadores.",
    tip: "Não protege contra outro programa na sua própria sessão. Ligar ou desligar \
          reescreve o banco ao salvar, o que pode demorar.",
};

/// As opções na ordem em que aparecem na janela.
pub const OPTIONS: [OptionText; 8] = [
    HOTKEY, STARTUP, RETENTION, MAX_ITEMS, MAX_PINNED, BLOCKLIST, SECRETS, PROTECTION,
];

pub const DAYS_UNIT: &str = "dias";

pub const BUTTON_SAVE: &str = "Salvar";
pub const BUTTON_CANCEL: &str = "Cancelar";
pub const BUTTON_APPLY: &str = "Aplicar";
pub const BUTTON_ALTER: &str = "Alterar";
pub const BUTTON_ADD: &str = "Adicionar";
pub const BUTTON_REMOVE: &str = "Remover";
pub const BUTTON_BACK: &str = "Voltar";

pub const HOTKEY_ARMED: &str = "Pressione a nova combinação de teclas... Esc cancela.";
pub const HOTKEY_STANDALONE: &str = "A configuração de atalho já está aberta na janela de \
                                     conflito — resolva-a ali (ela permanece sempre visível, \
                                     no topo da tela).";
pub const HOTKEY_PENDING: &str = "Nova combinação; passa a valer ao Aplicar ou Salvar.";
pub const STARTUP_BLOCKED: &str = "Desativado pelo Gerenciador de Tarefas do Windows.";
pub const SAVE_FAILED: &str = "Não foi possível salvar as configurações.";
pub const UNBLOCK_STARTUP_LINE: &str =
    "Ligar \"Iniciar com o Windows\" remove o bloqueio feito no Gerenciador de Tarefas.";
pub const CANNOT_UNDO: &str = "Isso não pode ser desfeito.";

pub fn hotkey_none_active(configured: &str) -> String {
    format!(
        "Nenhum atalho está ativo. A combinação configurada, \"{configured}\", não pôde ser \
         registrada."
    )
}

pub fn hotkey_taken(new: &str, current: &str) -> String {
    format!(
        "\"{new}\" ficou indisponível e não pôde ser registrada. O atalho continua \
         \"{current}\"."
    )
}

pub fn reduce_retention_line(before: u32, after: u32) -> String {
    format!(
        "Reduzir o prazo de retenção de {before} para {after} dias pode apagar itens antigos \
         do histórico."
    )
}

pub fn reduce_items_line(before: u32, after: u32) -> String {
    format!(
        "Reduzir a quantidade máxima de {before} para {after} itens pode apagar itens antigos \
         do histórico."
    )
}
