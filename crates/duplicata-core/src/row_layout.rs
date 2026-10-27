use crate::filter_strip::Rect;

pub const REFERENCE_DPI: u32 = 96;

pub const WINDOW_WIDTH_PX: i32 = 520;

pub const WINDOW_FRAME_WIDTH_PX: i32 = 16;

pub const WINDOW_FRAME_ALLOWANCE_PX: i32 = 40;

pub const ROW_HEIGHT_PX: i32 = 28;

pub const WINDOW_HEIGHT_PX: i32 = 560;

pub const WINDOW_NONCLIENT_HEIGHT_PX: i32 = 48;

pub const MIN_VISIBLE_ROWS: usize = 12;

pub fn rows_that_fit(client_height_px: i32, chrome_px: i32, row_height_px: i32) -> usize {
    if row_height_px <= 0 {
        return 0;
    }
    ((client_height_px - chrome_px).max(0) / row_height_px).max(0) as usize
}

pub fn client_height_at(dpi: u32, scaled_nonclient: bool) -> i32 {
    let nonclient = if scaled_nonclient {
        scale_px(WINDOW_NONCLIENT_HEIGHT_PX, dpi)
    } else {
        WINDOW_NONCLIENT_HEIGHT_PX
    };
    WINDOW_HEIGHT_PX - nonclient
}

pub const THUMBNAIL_MARGIN_PX: i32 = 2;

pub const THUMBNAIL_MAX_WIDTH_PX: i32 = 72;

pub const TEXT_GAP_PX: i32 = 8;

pub fn scale_px(logical_px: i32, dpi: u32) -> i32 {
    let dpi = if dpi == 0 { REFERENCE_DPI } else { dpi };
    ((logical_px as i64 * dpi as i64) / REFERENCE_DPI as i64) as i32
}

pub fn thumbnail_target(row_height_px: i32, dpi: u32, src_w: u32, src_h: u32) -> (i32, i32) {
    if src_w == 0 || src_h == 0 {
        return (0, 0);
    }
    let margin = scale_px(THUMBNAIL_MARGIN_PX, dpi);
    let max_w = scale_px(THUMBNAIL_MAX_WIDTH_PX, dpi).max(1);
    let h = (row_height_px - 2 * margin).max(1);
    let (src_w, src_h) = (src_w as i64, src_h as i64);

    let w = ((h as i64 * src_w + src_h / 2) / src_h).max(1) as i32;
    if w <= max_w {
        return (w, h);
    }
    let clamped_h = ((max_w as i64 * src_h + src_w / 2) / src_w).max(1) as i32;
    (max_w, clamped_h)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowLayout {
    /// Coluna do alfinete na ponta esquerda, reservada em toda linha (vazia
    /// quando o ícone não aparece), para o texto começar sempre no mesmo ponto.
    /// Numa linha estreita, para onde começa a zona da lixeira.
    pub pin_button: Rect,
    pub thumbnail: Rect,
    pub text: Rect,
    /// Lixeira na ponta direita, só nas linhas que a mostram.
    pub delete_button: Option<Rect>,
}

/// Quadrado da lixeira: lado igual à altura da linha, encostado na borda
/// direita e ocupando a altura toda da linha.
///
/// `dpi` não entra na conta: o lado é a altura real de `row`, que já vem na
/// escala da janela (só o glifo escala com o DPI). O parâmetro fica para a
/// assinatura acompanhar a de [`row_layout`] e a de quem a chama.
pub fn delete_button_rect(row: Rect, dpi: u32) -> Rect {
    let _ = dpi;
    let side = (row.bottom - row.top).max(0);
    Rect {
        left: (row.right - side).max(row.left),
        top: row.top,
        right: row.right,
        bottom: row.bottom,
    }
}

/// Quadrado do alfinete: lado igual à altura da linha, encostado na borda
/// esquerda, ocupando a altura toda e cortado na borda direita de `row`
/// (espelho de [`delete_button_rect`]).
///
/// Como na lixeira, `dpi` não entra na conta: o lado é a altura real de
/// `row`, já na escala da janela (só o glifo escala com o DPI).
pub fn pin_button_rect(row: Rect, dpi: u32) -> Rect {
    let _ = dpi;
    // Altura negativa ou borda direita antes da esquerda: o piso em
    // `row.left` deixa o quadrado vazio, nunca com largura negativa.
    let side = row.bottom - row.top;
    Rect {
        left: row.left,
        top: row.top,
        right: (row.left + side).min(row.right).max(row.left),
        bottom: row.bottom,
    }
}

pub fn row_layout(row: Rect, dpi: u32, thumbnail: Option<(u32, u32)>) -> RowLayout {
    row_layout_with_delete(row, dpi, thumbnail, false)
}

/// Como [`row_layout`], mas com a lixeira quando `show_delete`: o texto
/// termina uma fresta antes dela, nunca antes do próprio começo.
pub fn row_layout_with_delete(
    row: Rect,
    dpi: u32,
    thumbnail: Option<(u32, u32)>,
    show_delete: bool,
) -> RowLayout {
    let margin = scale_px(THUMBNAIL_MARGIN_PX, dpi);
    let gap = scale_px(TEXT_GAP_PX, dpi);
    let row_h = (row.bottom - row.top).max(0);

    // Onde os dois quadrados se sobrepõem (linha estreita), o clique é da
    // lixeira, mesmo escondida (`zone_at`): o alfinete visível para antes.
    // Sem piso aqui: a lixeira nunca começa antes de `row.left`.
    let trash_left = delete_button_rect(row, dpi).left;
    let mut pin_button = pin_button_rect(row, dpi);
    pin_button.right = pin_button.right.min(trash_left);
    let mut cursor = pin_button.right;
    let empty = Rect {
        left: cursor,
        top: row.top,
        right: cursor,
        bottom: row.top,
    };
    let thumbnail = match thumbnail {
        Some((src_w, src_h)) => {
            let (w, h) = thumbnail_target(row_h, dpi, src_w, src_h);
            if w == 0 || h == 0 {
                empty
            } else {
                let top = row.top + (row_h - h) / 2;
                let left = cursor + margin;
                cursor = left + w;
                Rect {
                    left,
                    top,
                    right: cursor,
                    bottom: top + h,
                }
            }
        }
        None => empty,
    };

    let text_left = (cursor + gap).min(row.right);
    let delete_button = show_delete.then(|| delete_button_rect(row, dpi));
    let text_right = match delete_button {
        Some(button) => (button.left - gap).max(text_left),
        None => row.right,
    };
    RowLayout {
        pin_button,
        thumbnail,
        text: Rect {
            left: text_left,
            top: row.top,
            right: text_right,
            bottom: row.bottom,
        },
        delete_button,
    }
}
