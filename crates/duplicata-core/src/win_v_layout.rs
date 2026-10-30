//! Geometria da janela de falha do Win+V no DPI dela: o texto em cima, os
//! dois botões encostados à direita embaixo. A casca mede o texto com a
//! fonte do DPI atual na largura de [`failure_text_width`] e só posiciona o
//! que vem daqui.

use crate::filter_strip::Rect;
use crate::row_layout::scale_px;

/// Medidas em pixels físicos, já no DPI de `dpi`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FailureMeasures {
    pub dpi: u32,
    pub line_height: i32,
    /// O mais largo entre os textos dos botões.
    pub button_text_width: i32,
    /// Altura do texto mais alto que a janela mostra, medido na largura de
    /// [`failure_text_width`].
    pub text_height: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FailureLayout {
    pub client_width: i32,
    pub client_height: i32,
    pub text: Rect,
    /// Da esquerda para a direita: "Desligar a opção" e "Manter" (o "OK"
    /// ocupa o da direita).
    pub buttons: [Rect; 2],
}

const WIDTH: i32 = 460;
const MARGIN: i32 = 20;
const TEXT_GAP: i32 = 16;
const BUTTON_GAP: i32 = 8;
const BUTTON_PAD: i32 = 16;
const BUTTON_MIN_W: i32 = 88;
const CONTROL_MIN_H: i32 = 28;
const CONTROL_TEXT_PAD: i32 = 10;

fn button_width(m: &FailureMeasures) -> i32 {
    scale_px(BUTTON_MIN_W, m.dpi).max(m.button_text_width + 2 * scale_px(BUTTON_PAD, m.dpi))
}

/// Largura da área cliente: a preferida, ou a que cabe os dois botões.
fn client_width(m: &FailureMeasures) -> i32 {
    let s = |px| scale_px(px, m.dpi);
    s(WIDTH).max(2 * s(MARGIN) + 2 * button_width(m) + s(BUTTON_GAP))
}

/// Largura em que o texto é medido e desenhado.
pub fn failure_text_width(m: &FailureMeasures) -> i32 {
    client_width(m) - 2 * scale_px(MARGIN, m.dpi)
}

pub fn failure_window_layout(m: &FailureMeasures) -> FailureLayout {
    let s = |px| scale_px(px, m.dpi);
    let client_width = client_width(m);
    let margin = s(MARGIN);
    let text = Rect {
        left: margin,
        top: margin,
        right: client_width - margin,
        bottom: margin + m.text_height,
    };
    let (w, h) = (
        button_width(m),
        s(CONTROL_MIN_H).max(m.line_height + s(CONTROL_TEXT_PAD)),
    );
    let top = text.bottom + s(TEXT_GAP);
    let right = Rect {
        left: client_width - margin - w,
        top,
        right: client_width - margin,
        bottom: top + h,
    };
    let left = Rect {
        left: right.left - s(BUTTON_GAP) - w,
        right: right.left - s(BUTTON_GAP),
        ..right
    };
    FailureLayout {
        client_width,
        client_height: right.bottom + margin,
        text,
        buttons: [left, right],
    }
}
