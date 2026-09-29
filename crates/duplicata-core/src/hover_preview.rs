//! Pré-visualização flutuante de um item do histórico ao parar o mouse sobre
//! ele. Só texto: imagem e arquivos ficam fora (a linha já mostra miniatura
//! ou nomes). O conteúdo vem do `preview` já carregado na lista — nunca dos
//! bytes do item —, pela mesma razão da busca: nada de I/O nem decriptação
//! na thread da interface por causa de um movimento de mouse.
//!
//! O fluxo tem duas etapas porque a quebra de linha depende de medir texto,
//! que só a casca sabe fazer: `hover_preview` separa as linhas lógicas; a
//! casca mede quantas linhas visuais cada uma ocupa na largura máxima; e
//! `fit_rows` aplica o teto sobre linhas visuais e decide a continuação.

use crate::filter_strip::Rect;
use crate::preview::PREVIEW_MAX_CHARS;
use crate::repository::ClipListItem;
use crate::row_layout::scale_px;

/// Teto de linhas visuais de conteúdo; a de continuação vem além dele.
pub const HOVER_PREVIEW_MAX_LINES: usize = 15;

pub const CONTINUATION: &str = "…";

pub const HOVER_PREVIEW_MAX_WIDTH_PX: i32 = 480;

pub const HOVER_PREVIEW_PADDING_PX: i32 = 8;

pub const HOVER_PREVIEW_GAP_PX: i32 = 6;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HoverPreview {
    /// Linhas lógicas, na ordem, com a indentação original.
    pub lines: Vec<String>,
    /// O `preview` salvo atingiu `PREVIEW_MAX_CHARS`: há conteúdo além dele.
    pub source_cut: bool,
}

/// Decide se o item ganha pré-visualização e separa suas linhas lógicas.
///
/// `line_fits` mede se uma linha cabe inteira na linha da lista (a casca
/// sabe medir texto). Só é chamada para item de uma linha só, com a linha já
/// separada aqui — a casca nunca decide o que é "a primeira linha". Item de
/// uma linha só que já aparece inteiro não ganha popup: repetiria o que está
/// na tela.
///
/// Quebram linha: `\r\n`, `\n`, `\r` sozinho e os terminadores Unicode
/// U+0085, U+2028 e U+2029 (que o GDI desenharia como caixa). Espaços e
/// tabulações iniciais ficam intactos. Linhas em branco no fim são
/// descartadas — são o `\n` final, não conteúdo.
///
/// O `preview` salvo é cortado em `PREVIEW_MAX_CHARS`. Quando ele chega a
/// esse tamanho, `source_cut` liga a continuação mesmo que tudo caiba:
/// omitir afirmaria que o conteúdo acaba ali. O único falso positivo é um
/// texto de exatamente esse tamanho, e "…" a mais engana menos que "…" a
/// menos.
pub fn hover_preview(
    item: &ClipListItem,
    line_fits: impl FnOnce(&str) -> bool,
) -> Option<HoverPreview> {
    if item.canonical_kind != "unicode_text" {
        return None;
    }
    let text = item.preview.as_deref()?;
    if text.trim().is_empty() {
        return None;
    }

    let mut lines = split_lines(text);
    while lines.last().is_some_and(|l| l.trim().is_empty()) {
        lines.pop();
    }

    let source_cut = text.chars().count() >= PREVIEW_MAX_CHARS;
    if lines.len() == 1 && !source_cut && line_fits(&lines[0]) {
        return None;
    }
    Some(HoverPreview { lines, source_cut })
}

fn split_lines(text: &str) -> Vec<String> {
    text.replace("\r\n", "\n")
        .split(['\n', '\r', '\u{0085}', '\u{2028}', '\u{2029}'])
        .map(str::to_string)
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowFit {
    /// Linhas visuais concedidas a cada linha lógica exibida, na ordem. A
    /// última pode receber menos do que precisa: aparece cortada embaixo.
    pub rows_per_line: Vec<usize>,
    /// Desenhar a linha de continuação. A casca usa este campo, nunca
    /// compara texto com `CONTINUATION` (o conteúdo pode ter um "…" sozinho).
    pub truncated: bool,
}

impl RowFit {
    /// Linhas visuais do popup, contando a de continuação.
    pub fn total_rows(&self) -> usize {
        self.rows_per_line.iter().sum::<usize>() + usize::from(self.truncated)
    }
}

/// Encaixa as linhas lógicas no teto de linhas visuais. `visual_rows[i]` é
/// quantas linhas visuais a linha lógica `i` ocupa na largura máxima (linha
/// vazia conta uma). A linha que estoura o teto entra com o que sobra, para
/// mostrar o máximo possível, e a continuação liga.
pub fn fit_rows(visual_rows: &[usize], source_cut: bool, max_rows: usize) -> RowFit {
    let max_rows = max_rows.max(1);
    let mut used = 0usize;
    let mut rows_per_line = Vec::with_capacity(visual_rows.len());
    let mut overflow = false;
    for &rows in visual_rows {
        let rows = rows.max(1);
        let left = max_rows - used;
        if rows > left {
            if left > 0 {
                rows_per_line.push(left);
            }
            overflow = true;
            break;
        }
        rows_per_line.push(rows);
        used += rows;
    }
    RowFit {
        rows_per_line,
        truncated: overflow || source_cut,
    }
}

/// Linhas visuais que um texto medido com altura `height_px` ocupa, com
/// linhas de `line_height_px`. Arredonda para cima; nunca menos de uma.
pub fn rows_for_height(height_px: i32, line_height_px: i32) -> usize {
    let line = i64::from(line_height_px.max(1));
    let rows = (i64::from(height_px.max(0)) + line - 1) / line;
    usize::try_from(rows).unwrap_or(usize::MAX).max(1)
}

/// Largura do conteúdo: a linha mais larga, ou o "…" se ele for desenhado e
/// for mais largo.
pub fn content_width(widest_line_px: i32, continuation_px: i32, fit: &RowFit) -> i32 {
    if fit.truncated {
        widest_line_px.max(continuation_px)
    } else {
        widest_line_px
    }
}

/// Tamanho do popup: largura da linha mais larga, limitada ao máximo; altura
/// das linhas visuais. Ambos com a margem interna, nunca negativos.
pub fn hover_popup_size(
    widest_line_px: i32,
    total_rows: usize,
    line_height_px: i32,
    dpi: u32,
) -> (i32, i32) {
    let pad2 = scale_px(HOVER_PREVIEW_PADDING_PX, dpi).saturating_mul(2);
    let max_w = scale_px(HOVER_PREVIEW_MAX_WIDTH_PX, dpi);
    let w = widest_line_px.max(0).saturating_add(pad2).min(max_w);
    let rows = i32::try_from(total_rows).unwrap_or(i32::MAX);
    let h = rows
        .saturating_mul(line_height_px.max(0))
        .saturating_add(pad2);
    (w.max(0), h.max(0))
}

/// Onde o popup aparece, em coordenadas de tela: ao lado da janela do
/// histórico (direita, ou esquerda se não couber), com o topo alinhado à
/// linha sob o mouse, sempre dentro de `work` (a área útil do monitor).
/// Ao lado e não por cima: por cima esconderia as linhas vizinhas que o
/// usuário está percorrendo. Se não couber de nenhum lado, encosta na borda
/// direita da área útil e sobrepõe a janela — a casca garante que o popup
/// não intercepta o mouse. Com `work` degenerada (largura ou altura ≤ 0) o
/// resultado é um retângulo vazio em sua origem.
pub fn hover_popup_rect(
    window: Rect,
    row_top: i32,
    size: (i32, i32),
    work: Rect,
    dpi: u32,
) -> Rect {
    let gap = scale_px(HOVER_PREVIEW_GAP_PX, dpi);
    let w = size.0.clamp(0, work.right.saturating_sub(work.left).max(0));
    let h = size.1.clamp(0, work.bottom.saturating_sub(work.top).max(0));

    let right_side = window.right.saturating_add(gap);
    let left_side = window.left.saturating_sub(gap).saturating_sub(w);
    let preferred = if right_side.saturating_add(w) <= work.right {
        right_side
    } else if left_side >= work.left {
        left_side
    } else {
        work.right - w
    };
    // A janela pode ter sido arrastada para fora da área útil; qualquer lado
    // escolhido é trazido de volta para dentro.
    let left = preferred.min(work.right - w).max(work.left);
    let top = row_top.min(work.bottom - h).max(work.top);

    Rect {
        left,
        top,
        right: left + w,
        bottom: top + h,
    }
}

/// O que a casca faz depois de um `WM_MOUSEMOVE`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MoveEffect {
    /// Esconder o popup (o mouse mudou de linha ou a janela não está aberta).
    pub hide: bool,
    /// Pedir ao sistema um único `WM_MOUSEHOVER` (e o `WM_MOUSELEAVE`).
    pub arm: bool,
}

/// Estado do hover na lista. Recebe os eventos de mouse já traduzidos pela
/// casca em "qual linha visível está sob o cursor" e decide quando armar o
/// aviso de hover, quando mostrar e quando esconder.
///
/// O aviso só é armado com a janela aberta e sobre uma linha, e no máximo
/// uma vez por linha: depois que ele dispara — com ou sem popup —, mexer o
/// mouse na mesma linha não arma de novo. Assim o sistema nunca é chamado a
/// medir tempo fora de uso, nem repetidamente sobre um item curto.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct HoverTracker {
    row: Option<usize>,
    armed: bool,
    fired: bool,
    shown: bool,
}

impl HoverTracker {
    pub fn on_move(&mut self, window_open: bool, row: Option<usize>) -> MoveEffect {
        if !window_open {
            return MoveEffect {
                hide: self.dismiss(),
                arm: false,
            };
        }
        let mut hide = false;
        if row != self.row {
            hide = self.shown;
            *self = HoverTracker {
                row,
                ..HoverTracker::default()
            };
        }
        let arm = self.row.is_some() && !self.armed && !self.fired;
        if arm {
            self.armed = true;
        }
        MoveEffect { hide, arm }
    }

    /// O aviso de hover chegou com o cursor sobre `row`. Devolve a linha a
    /// pré-visualizar, ou `None` se o aviso ficou velho (a lista rolou, o
    /// cursor está em outra linha) ou a janela não está aberta.
    pub fn on_hover(&mut self, window_open: bool, row: Option<usize>) -> Option<usize> {
        self.armed = false;
        if !window_open || self.shown || row.is_none() || row != self.row {
            return None;
        }
        self.fired = true;
        row
    }

    /// A casca conseguiu exibir o popup para a linha devolvida por
    /// `on_hover`.
    pub fn mark_shown(&mut self) {
        self.shown = true;
    }

    /// O mouse saiu da janela. Devolve se o popup precisa ser escondido.
    pub fn on_leave(&mut self) -> bool {
        self.dismiss()
    }

    /// Qualquer outro motivo para sumir: rolagem, teclado, clique, menu,
    /// filtro, recarga, janela escondida. Devolve se o popup estava visível.
    /// O próximo movimento sobre uma linha arma de novo.
    pub fn dismiss(&mut self) -> bool {
        let was_shown = self.shown;
        *self = HoverTracker::default();
        was_shown
    }

    pub fn is_shown(&self) -> bool {
        self.shown
    }
}
