//! Textos da janela de Configurações e da confirmação do Salvar, num lugar
//! só para que o teste confira cada um contra a especificação.

use crate::hotkey::{format_hotkey, HotkeyCombo, HotkeyRejection};

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
pub const OPTIONS: [OptionText; 9] = [
    HOTKEY, WIN_V, STARTUP, RETENTION, MAX_ITEMS, MAX_PINNED, BLOCKLIST, SECRETS, PROTECTION,
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

pub const EXPLORER_RESTART_FAILED: &str = "Não foi possível reiniciar o Windows Explorer. \
     Reinicie pelo Gerenciador de Tarefas ou reinicie o computador.";
pub const EXPLORER_NOT_RETURNED: &str = "O Windows Explorer não voltou. Abra o Gerenciador de \
     Tarefas (Ctrl+Shift+Esc), Executar nova tarefa, e digite explorer.";

/// Recusa da combinação pressionada no campo "Alterar".
pub fn hotkey_rejection(candidate: &HotkeyCombo, rejection: HotkeyRejection) -> String {
    let reason = match rejection {
        HotkeyRejection::WinReserved => return WIN_RESERVED.to_string(),
        HotkeyRejection::NoModifier => "nenhum modificador (Ctrl/Alt/Shift) foi pressionado.",
        HotkeyRejection::ModifierOnly => "só modificadores foram pressionados, falta uma tecla.",
        HotkeyRejection::ReservedSystemCombo => "essa combinação é reservada pelo sistema.",
    };
    format!(
        "\"{}\": {reason} Pressione outra combinação.",
        format_hotkey(candidate)
    )
}

/// O `<reinício>` da especificação, como literal para o `concat!`.
macro_rules! restart {
    () => {
        "reiniciar o Windows Explorer, o computador ou sair e entrar na conta"
    };
}

pub const RESTART_PHRASE: &str = restart!();

pub const WIN_V: OptionText = OptionText {
    name: "Abrir com Win+V",
    description: "Substitui o histórico do Windows.",
    tip: concat!(
        "Vale ao salvar. O Win+V passa a abrir o duplicata depois de ",
        restart!(),
        "."
    ),
};

pub const WIN_RESERVED: &str = "O Windows reserva as combinações com Win. Para usar Win+V, \
                                ligue a opção \"Abrir com Win+V\".";
pub const WIN_V_HINT: &str = "Para usar Win+V, ligue a opção \"Abrir com Win+V\"";
pub const WIN_V_OFF_AWAITING_RETURN: &str = concat!(
    "O Win+V volta ao histórico do Windows depois de ",
    restart!(),
    ". Até lá, o Win+V não faz nada."
);
pub const WIN_V_ENABLE_PANEL_LINE: &str =
    "O painel de histórico do Windows deixa de abrir pelo Win+V.";
pub const WIN_V_DISABLE_LINE: &str = concat!(
    "O Win+V para de abrir o duplicata agora e volta ao histórico do Windows depois de ",
    restart!(),
    ". Até lá, o Win+V não faz nada."
);
pub const WIN_V_DISABLE_NOW_LINE: &str = "O Win+V para de abrir o duplicata agora.";
pub const WIN_V_WAS_DISABLED_BEFORE: &str =
    "O Win+V já estava desativado no Windows antes do duplicata e continua assim.";
pub const WIN_V_REGISTRY_FAILED: &str = "Não foi possível alterar o atalho do Windows; nada mudou.";
pub const BUTTON_RESTART_EXPLORER: &str = "Reiniciar o Windows Explorer agora";
pub const RESTART_EXPLORER_CONFIRM: &str = "O Windows Explorer será reiniciado. As janelas de \
                                            pasta abertas serão fechadas e cópias de arquivo em \
                                            andamento nelas serão canceladas.";
pub const BUTTON_RESTART: &str = "Reiniciar";
pub const WIN_V_FAILURE_TITLE: &str = "Win+V não liberado — duplicata";
pub const WIN_V_RETURNS_AFTER: &str = concat!(
    "O Win+V volta ao histórico do Windows depois de ",
    restart!(),
    "."
);
pub const BUTTON_TURN_OFF: &str = "Desligar a opção";
pub const BUTTON_KEEP: &str = "Manter";
pub const BUTTON_OK: &str = "OK";

pub fn win_v_awaiting(reserve: &str) -> String {
    format!("O Win+V passa a valer depois de {RESTART_PHRASE}. Até lá: {reserve}.")
}

pub fn win_v_active(reserve: &str) -> String {
    format!("Em uso: Win+V (reserva: {reserve})")
}

pub fn win_v_not_released(reserve: &str) -> String {
    format!("O Windows não liberou o Win+V. Em uso: {reserve}.")
}

pub fn win_v_enable_when(reserve: &str) -> String {
    format!(
        "O Win+V passa a abrir o duplicata depois de {RESTART_PHRASE}. Até lá, continua \
         valendo {reserve}."
    )
}

pub fn win_v_failure(reserve: &str) -> String {
    format!("O Windows não liberou o Win+V. O duplicata continua abrindo com {reserve}.")
}

pub const WIN_V_UNINSTALL_FAILED: &str = "Não foi possível devolver o Win+V ao Windows: ele \
                                          continua desativado no valor DisabledHotkeys do \
                                          Windows Explorer. Para removê-lo à mão, veja a seção \
                                          \"Abrir com Win+V\" do README do duplicata \
                                          (github.com/RoaniPires/duplicata).";
