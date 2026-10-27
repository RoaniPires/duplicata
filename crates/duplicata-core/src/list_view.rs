use crate::repository::ClipListItem;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ContentTypeFilter {
    #[default]
    All,
    Text,
    Image,
    Files,
}

impl ContentTypeFilter {
    pub const SEGMENTS: [ContentTypeFilter; 4] = [
        ContentTypeFilter::All,
        ContentTypeFilter::Text,
        ContentTypeFilter::Image,
        ContentTypeFilter::Files,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            ContentTypeFilter::All => "Tudo",
            ContentTypeFilter::Text => "Texto",
            ContentTypeFilter::Image => "Imagem",
            ContentTypeFilter::Files => "Arquivos",
        }
    }
}

pub fn category_of(canonical_kind: &str) -> Option<ContentTypeFilter> {
    match canonical_kind {
        "unicode_text" => Some(ContentTypeFilter::Text),
        "hdrop" => Some(ContentTypeFilter::Files),
        "dib" | "dibv5" => Some(ContentTypeFilter::Image),
        _ => None,
    }
}

fn passes_type(item: &ClipListItem, filter: ContentTypeFilter) -> bool {
    filter == ContentTypeFilter::All || category_of(&item.canonical_kind) == Some(filter)
}

pub fn text_matches(displayed_label: &str, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    displayed_label.to_lowercase().contains(needle)
}

fn labels_for_search(item: &ClipListItem) -> Vec<&str> {
    let preview = item.preview.as_deref().unwrap_or("");
    if item.canonical_kind == "hdrop" {
        preview.split('\n').collect()
    } else {
        vec![preview]
    }
}

fn item_matches_text(item: &ClipListItem, needle: &str) -> bool {
    if needle.is_empty() {
        return true;
    }
    labels_for_search(item)
        .iter()
        .any(|label| text_matches(label, needle))
}

pub fn arrange_for_display(items: &[ClipListItem]) -> Vec<usize> {
    let mut order: Vec<usize> = Vec::with_capacity(items.len());
    order.extend(
        items
            .iter()
            .enumerate()
            .filter(|(_, it)| it.pinned)
            .map(|(i, _)| i),
    );
    order.extend(
        items
            .iter()
            .enumerate()
            .filter(|(_, it)| !it.pinned)
            .map(|(i, _)| i),
    );
    order
}

pub fn visible_rows(
    items: &[ClipListItem],
    filter_text: &str,
    type_filter: ContentTypeFilter,
) -> Vec<usize> {
    let needle = filter_text.trim().to_lowercase();
    arrange_for_display(items)
        .into_iter()
        .filter(|&i| passes_type(&items[i], type_filter) && item_matches_text(&items[i], &needle))
        .collect()
}

pub fn pinned_group_boundary(items: &[ClipListItem], order: &[usize]) -> Option<usize> {
    let first_unpinned = order.iter().position(|&i| !items[i].pinned)?;
    if first_unpinned == 0 {
        return None;
    }
    Some(first_unpinned)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListDisplayState {
    ToggleBanner,
    ErrorBanner,
    NoMatch,
    EmptyHistory,
    Rows,
}

pub fn display_state(
    toggle_in_progress: bool,
    unreadable: bool,
    total_rows: usize,
    visible_rows: usize,
    filter_active: bool,
) -> ListDisplayState {
    if toggle_in_progress {
        return ListDisplayState::ToggleBanner;
    }
    if unreadable {
        return ListDisplayState::ErrorBanner;
    }
    if visible_rows > 0 {
        return ListDisplayState::Rows;
    }
    if filter_active {
        ListDisplayState::NoMatch
    } else if total_rows == 0 {
        ListDisplayState::EmptyHistory
    } else {
        ListDisplayState::Rows
    }
}

/// Seleção e scroll depois de remover um item da lista visível: a seleção
/// fica na mesma posição (o item seguinte ocupa o lugar do apagado), ou no
/// último se o apagado era o último; o scroll fica onde o usuário deixou,
/// só recuando o necessário para a página não passar do fim e para a
/// seleção continuar visível. `None` quando a lista ficou vazia.
pub fn view_after_removal(
    selected: usize,
    scroll: usize,
    visible_len: usize,
    rows_per_page: usize,
) -> Option<(usize, usize)> {
    let last = visible_len.checked_sub(1)?;
    let page = rows_per_page.max(1);
    let sel = selected.min(last);
    let mut top = scroll.min(visible_len.saturating_sub(page));
    if sel < top {
        top = sel;
    } else if sel >= top + page {
        top = sel + 1 - page;
    }
    Some((sel, top))
}

/// Seleção e scroll depois de apagar um item qualquer da lista visível.
/// `kept_pos` é a nova posição do item que estava selecionado, quando ele
/// continua na lista; `None` quando o apagado era o próprio selecionado (a
/// seleção fica em `prev_selected`, o item que ocupou o lugar dele). Depois,
/// o mesmo ajuste de [`view_after_removal`]. `None` quando a lista ficou vazia.
pub fn view_after_delete(
    kept_pos: Option<usize>,
    prev_selected: usize,
    scroll: usize,
    visible_len: usize,
    rows_per_page: usize,
) -> Option<(usize, usize)> {
    view_after_removal(
        kept_pos.unwrap_or(prev_selected),
        scroll,
        visible_len,
        rows_per_page,
    )
}

/// Seleção e scroll padrão ao abrir o histórico ou ao mudar os filtros: a
/// seleção vai para o item visível de maior recência (`last_activity_ms`,
/// desempate pelo maior `id`, o mesmo critério do store), fixado ou não. O
/// scroll parte do topo e rola o mínimo para a seleção aparecer, com o mesmo
/// ajuste de [`view_after_removal`].
///
/// `visible` são índices em `items`, na ordem de exibição (a de
/// [`visible_rows`]); o selecionado devolvido é a posição dentro de `visible`.
/// `rows_per_page` 0 conta como 1. `None` quando não há item visível.
///
/// # Panics
///
/// Se algum índice de `visible` não existir em `items`. Vindo de
/// [`visible_rows`] sobre o mesmo `items`, isso não acontece.
#[must_use]
pub fn default_view(
    items: &[ClipListItem],
    visible: &[usize],
    rows_per_page: usize,
) -> Option<(usize, usize)> {
    let (selected, _) = visible
        .iter()
        .enumerate()
        .max_by_key(|&(_, &i)| (items[i].last_activity_ms, items[i].id))?;
    view_after_removal(selected, 0, visible.len(), rows_per_page)
}
