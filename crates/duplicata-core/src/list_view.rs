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
