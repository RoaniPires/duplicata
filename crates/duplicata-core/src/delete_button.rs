//! Lixeira de um clique na linha do histórico: em quais linhas aparece, onde
//! fica o clique e como o duplo clique sobre ela vira um único apagar.

use crate::filter_strip::Rect;
use crate::row_layout::delete_button_rect;

/// Lixeira da fonte de ícones do sistema (Segoe Fluent Icons / MDL2 Assets).
pub const DELETE_GLYPH: char = '\u{E74D}';

/// Altura lógica do glifo, a 96 DPI.
pub const DELETE_GLYPH_PX: i32 = 16;

/// A linha `pos` mostra a lixeira quando é a selecionada ou a sob o mouse.
pub fn shows_delete_button(pos: usize, selected: usize, hot: Option<usize>) -> bool {
    pos == selected || hot == Some(pos)
}

/// Parte da linha sob um ponto.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RowZone {
    /// Qualquer ponto fora da lixeira, inclusive a fresta antes dela.
    Body,
    DeleteButton,
}

/// Em que parte da linha `row` cai o ponto (`x`, `y`).
pub fn zone_at(x: i32, y: i32, row: Rect, dpi: u32) -> RowZone {
    let b = delete_button_rect(row, dpi);
    if x >= b.left && x < b.right && y >= b.top && y < b.bottom {
        RowZone::DeleteButton
    } else {
        RowZone::Body
    }
}

/// O que fazer com o botão esquerdo descendo sobre a linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DownAction {
    Select,
    Delete,
}

/// O que fazer com o duplo clique sobre a linha.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DoubleClickAction {
    Paste,
    Delete,
    Ignore,
}

/// Lembra onde caiu o último clique, para o segundo clique de um duplo
/// clique na lixeira não colar nem apagar o item que subiu para o lugar.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ClickGuard {
    last_down_on_button: bool,
}

impl ClickGuard {
    /// Botão esquerdo desceu: na lixeira apaga, no corpo seleciona.
    pub fn on_down(&mut self, zone: RowZone) -> DownAction {
        self.last_down_on_button = zone == RowZone::DeleteButton;
        match zone {
            RowZone::DeleteButton => DownAction::Delete,
            RowZone::Body => DownAction::Select,
        }
    }

    /// Duplo clique: o segundo clique de um duplo clique cuja primeira metade
    /// caiu na lixeira não faz nada (a linha sob ele já é outro item).
    pub fn on_double_click(&mut self, zone: RowZone) -> DoubleClickAction {
        if std::mem::take(&mut self.last_down_on_button) {
            return DoubleClickAction::Ignore;
        }
        match zone {
            RowZone::DeleteButton => DoubleClickAction::Delete,
            RowZone::Body => DoubleClickAction::Paste,
        }
    }
}
