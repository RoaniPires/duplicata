//! Alfinete de um clique na ponta esquerda da linha do histórico: qual glifo,
//! em quais linhas aparece e em que tom. A zona do clique e o duplo clique
//! ficam em [`crate::delete_button`], junto com os da lixeira.

use crate::delete_button::shows_delete_button;

/// Alfinete de contorno (Pin) da fonte de ícones do sistema: item não fixado.
pub const PIN_GLYPH: char = '\u{E718}';

/// Alfinete preenchido (PinnedFill) do mesmo catálogo: item fixado.
pub const PINNED_GLYPH: char = '\u{E842}';

/// Glifo do alfinete para o estado do item.
pub fn pin_glyph(pinned: bool) -> char {
    if pinned {
        PINNED_GLYPH
    } else {
        PIN_GLYPH
    }
}

/// A linha `pos` mostra o alfinete quando o item é fixado (sempre) ou, não
/// fixado, quando é a selecionada ou a sob o mouse (a regra da lixeira).
pub fn shows_pin_button(pos: usize, selected: usize, hot: Option<usize>, pinned: bool) -> bool {
    pinned || shows_delete_button(pos, selected, hot)
}

/// Cor do alfinete, em termos do tema.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PinTone {
    /// Cor de destaque do tema: fixado fora da seleção.
    Accent,
    /// Texto atenuado, a cor da lixeira: não fixado fora da seleção.
    Dim,
    /// Texto da seleção: qualquer alfinete na linha selecionada.
    SelectionFg,
}

/// Tom do alfinete para o estado do item e da linha.
pub fn pin_tone(pinned: bool, selected: bool) -> PinTone {
    match (pinned, selected) {
        (_, true) => PinTone::SelectionFg,
        (true, false) => PinTone::Accent,
        (false, false) => PinTone::Dim,
    }
}
