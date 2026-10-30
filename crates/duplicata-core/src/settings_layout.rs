//! Geometria da janela de Configurações: largura que cabe a descrição mais
//! larga numa linha, coluna de controles com a mesma borda direita, área que
//! rola e rodapé fixo. A casca mede os textos com a fonte do DPI atual e só
//! posiciona o que vem daqui.

use crate::filter_strip::Rect;
use crate::row_layout::scale_px;

/// Medidas em pixels físicos, já no DPI de `dpi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LayoutMeasures {
    pub dpi: u32,
    pub line_height: i32,
    pub title_height: i32,
    /// O mais largo entre os nomes e as descrições das opções.
    pub widest_label: i32,
    /// Reserva para a combinação do atalho ao lado de "Alterar".
    pub hotkey_text_width: i32,
    pub days_width: i32,
    /// Dez dígitos na fonte dos campos.
    pub numeric_text_width: i32,
    /// O mais largo entre os textos dos botões.
    pub button_text_width: i32,
    pub scrollbar_width: i32,
    /// Há linha de estado do Win+V abaixo do atalho.
    pub status_line: bool,
    /// O botão de reiniciar o Explorer está visível.
    pub restart_button: bool,
    pub restart_text_width: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct OptionRow {
    pub row: Rect,
    pub name: Rect,
    pub description: Rect,
    /// O grupo de controles da linha; vazio na de aplicativos ignorados.
    pub control: Rect,
}

/// Retângulos da área que rola em coordenadas do conteúdo (antes da
/// rolagem); rodapé e linha de erro em coordenadas da área cliente.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct SettingsLayout {
    pub client_width: i32,
    pub client_height: i32,
    /// A área que rola, com a barra de rolagem, na área cliente.
    pub viewport: Rect,
    /// Largura útil do conteúdo (sem a barra de rolagem).
    pub content_width: i32,
    pub content_height: i32,
    pub titles: [Rect; 3],
    /// Na ordem de [`crate::settings_text::OPTIONS`].
    pub rows: [OptionRow; 9],
    pub hotkey_text: Rect,
    pub alter_button: Rect,
    /// Prazo, quantidade e teto.
    pub numeric_edits: [Rect; 3],
    pub days_label: Rect,
    /// Inicialização, segredos, proteção e Win+V.
    pub toggles: [Rect; 4],
    pub list: Rect,
    pub list_edit: Rect,
    pub add_button: Rect,
    pub remove_button: Rect,
    /// Linha de estado do Win+V; vazia quando não há.
    pub status_line: Rect,
    /// Vazio quando o botão não aparece.
    pub restart_button: Rect,
    pub error_line: Rect,
    /// Salvar, Cancelar e Aplicar, da esquerda para a direita.
    pub footer: [Rect; 3],
}

const MARGIN_X: i32 = 20;
const TOP_PAD: i32 = 12;
const SECTION_GAP: i32 = 20;
const TITLE_GAP: i32 = 6;
const ROW_PAD: i32 = 8;
const NAME_GAP: i32 = 2;
const COLUMN_GAP: i32 = 24;
const CONTROL_GAP: i32 = 8;
const CONTROL_MIN_H: i32 = 28;
const CONTROL_TEXT_PAD: i32 = 10;
const TOGGLE_W: i32 = 40;
const TOGGLE_H: i32 = 20;
const FIELD_PAD: i32 = 8;
const BUTTON_PAD: i32 = 16;
const BUTTON_MIN_W: i32 = 88;
const LIST_LINES: i32 = 5;
const BLOCK_GAP: i32 = 8;
const BOTTOM_PAD: i32 = 16;
const FOOTER_PAD: i32 = 12;
/// Altura preferida da área que rola: a janela cabe em 768 px a 100%.
const PREFERRED_VIEWPORT: i32 = 480;
/// O atalho troca a descrição por mensagens mais longas (recusa, conflito).
const HOTKEY_DESCRIPTION_LINES: i32 = 2;
/// A linha de estado do Win+V cita o `<reinício>` e a reserva: duas linhas.
const STATUS_LINES: i32 = 2;
/// A linha de erro do rodapé cabe as mensagens do reinício do Explorer.
const ERROR_LINES: i32 = 2;

fn rect(left: i32, top: i32, width: i32, height: i32) -> Rect {
    Rect {
        left,
        top,
        right: left + width,
        bottom: top + height,
    }
}

/// Retângulo de `width`×`height` encostado em `right`, centrado em `row`.
fn right_aligned(right: i32, row: Rect, width: i32, height: i32) -> Rect {
    let top = row.top + (row.bottom - row.top - height) / 2;
    rect(right - width, top, width, height)
}

pub fn settings_layout(m: &LayoutMeasures, available_client_height: i32) -> SettingsLayout {
    let s = |px| scale_px(px, m.dpi);
    let line = m.line_height.max(1);
    let control_h = s(CONTROL_MIN_H).max(line + s(CONTROL_TEXT_PAD));
    let button_w = s(BUTTON_MIN_W).max(m.button_text_width + 2 * s(BUTTON_PAD));
    let edit_w = m.numeric_text_width + 2 * s(FIELD_PAD);
    let (toggle_w, toggle_h) = (s(TOGGLE_W), s(TOGGLE_H));
    let gap = s(CONTROL_GAP);

    let control_col = (m.hotkey_text_width + gap + button_w)
        .max(edit_w + gap + m.days_width)
        .max(toggle_w);
    let margin = s(MARGIN_X);
    let text_col = m.widest_label;
    let content_width = 2 * margin + text_col + s(COLUMN_GAP) + control_col;
    let right = content_width - margin;

    let mut l = SettingsLayout {
        content_width,
        client_width: content_width + m.scrollbar_width,
        ..Default::default()
    };

    let mut y = s(TOP_PAD);
    let title = |l: &mut SettingsLayout, i: usize, y: &mut i32| {
        if i > 0 {
            *y += s(SECTION_GAP);
        }
        l.titles[i] = rect(margin, *y, right - margin, m.title_height);
        *y += m.title_height + s(TITLE_GAP);
    };
    let option = |l: &mut SettingsLayout, i: usize, y: &mut i32| {
        let lines = if i == 0 { HOTKEY_DESCRIPTION_LINES } else { 1 };
        let text_h = line + s(NAME_GAP) + lines * line;
        let row_h = 2 * s(ROW_PAD) + text_h.max(control_h);
        let row = rect(0, *y, content_width, row_h);
        let top = row.top + (row_h - text_h) / 2;
        let name = rect(margin, top, text_col, line);
        let description = rect(margin, name.bottom + s(NAME_GAP), text_col, lines * line);
        l.rows[i] = OptionRow {
            row,
            name,
            description,
            control: Rect {
                left: right,
                top: row.top,
                right,
                bottom: row.top,
            },
        };
        *y += row_h;
        row
    };

    title(&mut l, 0, &mut y);
    let row = option(&mut l, 0, &mut y);
    l.alter_button = right_aligned(right, row, button_w, control_h);
    l.hotkey_text = right_aligned(l.alter_button.left - gap, row, m.hotkey_text_width, line);
    l.rows[0].control = Rect {
        left: l.hotkey_text.left,
        top: l.alter_button.top,
        ..l.alter_button
    };
    if m.status_line {
        l.status_line = rect(margin, y, right - margin, STATUS_LINES * line);
        y = l.status_line.bottom + s(BLOCK_GAP);
    }
    if m.restart_button {
        let w = s(BUTTON_MIN_W).max(m.restart_text_width + 2 * s(BUTTON_PAD));
        l.restart_button = rect(margin, y, w, control_h);
        y = l.restart_button.bottom + s(BLOCK_GAP);
    }
    let row = option(&mut l, 1, &mut y);
    l.toggles[3] = right_aligned(right, row, toggle_w, toggle_h);
    l.rows[1].control = l.toggles[3];
    let row = option(&mut l, 2, &mut y);
    l.toggles[0] = right_aligned(right, row, toggle_w, toggle_h);
    l.rows[2].control = l.toggles[0];

    title(&mut l, 1, &mut y);
    let row = option(&mut l, 3, &mut y);
    l.days_label = right_aligned(right, row, m.days_width, line);
    l.numeric_edits[0] = right_aligned(l.days_label.left - gap, row, edit_w, control_h);
    l.rows[3].control = Rect {
        left: l.numeric_edits[0].left,
        top: l.numeric_edits[0].top,
        right,
        bottom: l.numeric_edits[0].bottom,
    };
    for (i, edit) in [(4, 1), (5, 2)] {
        let row = option(&mut l, i, &mut y);
        l.numeric_edits[edit] = right_aligned(right, row, edit_w, control_h);
        l.rows[i].control = l.numeric_edits[edit];
    }

    title(&mut l, 2, &mut y);
    option(&mut l, 6, &mut y);
    l.list = rect(margin, y, right - margin, LIST_LINES * line + s(4));
    y = l.list.bottom + s(BLOCK_GAP);
    l.add_button = rect(right - button_w, y, button_w, control_h);
    l.list_edit = Rect {
        left: margin,
        top: y,
        right: l.add_button.left - gap,
        bottom: y + control_h,
    };
    y = l.add_button.bottom + s(BLOCK_GAP);
    l.remove_button = rect(right - button_w, y, button_w, control_h);
    y = l.remove_button.bottom + s(BLOCK_GAP);
    for (i, toggle) in [(7, 1), (8, 2)] {
        let row = option(&mut l, i, &mut y);
        l.toggles[toggle] = right_aligned(right, row, toggle_w, toggle_h);
        l.rows[i].control = l.toggles[toggle];
    }
    l.content_height = y + s(BOTTOM_PAD);

    let footer_pad = s(FOOTER_PAD);
    let error_h = ERROR_LINES * line;
    let footer_h = footer_pad + error_h + gap + control_h + footer_pad;
    let mut viewport_h = l.content_height.min(s(PREFERRED_VIEWPORT));
    if viewport_h + footer_h > available_client_height {
        viewport_h = (available_client_height - footer_h).max(control_h);
    }
    l.client_height = viewport_h + footer_h;
    l.viewport = rect(0, 0, l.client_width, viewport_h);

    let client_margin = margin;
    l.error_line = Rect {
        left: client_margin,
        top: viewport_h + footer_pad,
        right: l.client_width - client_margin,
        bottom: viewport_h + footer_pad + error_h,
    };
    let buttons_top = l.error_line.bottom + gap;
    let mut x = l.client_width - client_margin;
    for i in (0..3).rev() {
        l.footer[i] = rect(x - button_w, buttons_top, button_w, control_h);
        x -= button_w + gap;
    }
    l
}

pub fn max_scroll(viewport_height: i32, content_height: i32) -> i32 {
    (content_height - viewport_height).max(0)
}

/// Rolagem que deixa `top..bottom` visível mexendo o mínimo.
pub fn scroll_to_reveal(
    viewport_height: i32,
    content_height: i32,
    scroll: i32,
    top: i32,
    bottom: i32,
) -> i32 {
    let target = if top < scroll {
        top
    } else if bottom > scroll + viewport_height {
        bottom - viewport_height
    } else {
        scroll
    };
    target.clamp(0, max_scroll(viewport_height, content_height))
}

/// Opção cuja linha contém `y` (coordenadas do conteúdo).
pub fn row_at(layout: &SettingsLayout, y: i32) -> Option<usize> {
    layout
        .rows
        .iter()
        .position(|r| y >= r.row.top && y < r.row.bottom)
}
