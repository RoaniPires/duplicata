use duplicata_core::list_view::{default_view, visible_rows, ContentTypeFilter};
use duplicata_core::ClipListItem;

// Recência em ordem crescente: A < B < C < D. Os ids seguem a ordem de
// criação (A=1, B=2, ...), a não ser quando o teste diz outra coisa.

fn item(id: i64, kind: &str, preview: &str, last_activity_ms: u64, pinned: bool) -> ClipListItem {
    ClipListItem {
        id,
        canonical_kind: kind.to_owned(),
        preview: Some(preview.to_owned()),
        thumbnail: None,
        has_text: kind == "unicode_text",
        last_activity_ms,
        pinned,
    }
}

fn text(id: i64, preview: &str, last_activity_ms: u64, pinned: bool) -> ClipListItem {
    item(id, "unicode_text", preview, last_activity_ms, pinned)
}

const A: u64 = 1_000;
const B: u64 = 2_000;
const C: u64 = 3_000;
const D: u64 = 4_000;

fn view(
    items: &[ClipListItem],
    filter: &str,
    kind: ContentTypeFilter,
    page: usize,
) -> Option<(usize, usize)> {
    let visible = visible_rows(items, filter, kind);
    default_view(items, &visible, page)
}

fn view_all(items: &[ClipListItem]) -> Option<(usize, usize)> {
    view(items, "", ContentTypeFilter::All, 10)
}

#[test]
fn opens_on_the_most_recent_unpinned_below_an_older_pinned() {
    // A │ D C B → D (posição 1)
    let items = [
        text(1, "a", A, true),
        text(4, "d", D, false),
        text(3, "c", C, false),
        text(2, "b", B, false),
    ];
    assert_eq!(view_all(&items), Some((1, 0)));
}

#[test]
fn opens_on_the_pinned_when_it_is_the_most_recent() {
    // D │ C B A → D (posição 0)
    let items = [
        text(4, "d", D, true),
        text(3, "c", C, false),
        text(2, "b", B, false),
        text(1, "a", A, false),
    ];
    assert_eq!(view_all(&items), Some((0, 0)));
}

#[test]
fn opens_after_every_older_pinned() {
    // B A │ D C → D (posição 2)
    let items = [
        text(2, "b", B, true),
        text(1, "a", A, true),
        text(4, "d", D, false),
        text(3, "c", C, false),
    ];
    assert_eq!(view_all(&items), Some((2, 0)));
}

#[test]
fn opens_on_the_single_recent_below_a_pinned() {
    // A │ D → D (posição 1)
    let items = [text(1, "a", A, true), text(4, "d", D, false)];
    assert_eq!(view_all(&items), Some((1, 0)));
}

#[test]
fn only_pinned_opens_on_the_most_recent_pinned() {
    // B A → B (posição 0)
    let items = [text(2, "b", B, true), text(1, "a", A, true)];
    assert_eq!(view_all(&items), Some((0, 0)));
}

#[test]
fn a_pinned_that_kept_its_older_recency_stays_behind_the_most_recent_copy() {
    // Depois de colar A pelo histórico a recência dele continua a antiga
    // (quem garante é paste_keeps_recency_smoke.rs, no duplicata-win).
    // A │ D C → D
    let items = [
        text(1, "a", A, true),
        text(4, "d", D, false),
        text(3, "c", C, false),
    ];
    assert_eq!(view_all(&items), Some((1, 0)));
}

#[test]
fn recopying_a_pinned_outside_the_app_makes_it_the_selected_one() {
    // A copiado de novo em outro app: recência acima de D. A │ D C → A
    let items = [
        text(1, "a", D + 500, true),
        text(4, "d", D, false),
        text(3, "c", C, false),
    ];
    assert_eq!(view_all(&items), Some((0, 0)));
}

#[test]
fn text_filter_selects_the_most_recent_among_the_matches() {
    // A("senha wifi") │ D("relatório") C("senha banco"); "senha" → A │ C, C
    let items = [
        text(1, "senha wifi", A, true),
        text(4, "relatório", D, false),
        text(3, "senha banco", C, false),
    ];
    assert_eq!(
        visible_rows(&items, "senha", ContentTypeFilter::All),
        vec![0, 2]
    );
    assert_eq!(
        view(&items, "senha", ContentTypeFilter::All, 10),
        Some((1, 0))
    );
}

#[test]
fn type_filter_selects_the_most_recent_among_the_matches() {
    // A(texto) │ D(imagem) C(texto); Tipo = Texto → A │ C, C
    let items = [
        text(1, "a", A, true),
        item(4, "dib", "", D, false),
        text(3, "c", C, false),
    ];
    assert_eq!(view(&items, "", ContentTypeFilter::Text, 10), Some((1, 0)));
}

#[test]
fn type_filter_keeps_the_pinned_selected_when_it_is_the_most_recent() {
    // D(texto) │ C(texto); Tipo = Texto → D (posição 0)
    let items = [text(4, "d", D, true), text(3, "c", C, false)];
    assert_eq!(view(&items, "", ContentTypeFilter::Text, 10), Some((0, 0)));
}

#[test]
fn many_pinned_scroll_the_minimum_so_the_selected_is_the_last_visible_row() {
    // F12..F1 fixados, R mais recente de todos, página de 8 → R (12), topo 5
    let mut items: Vec<ClipListItem> = (1..=12)
        .rev()
        .map(|n| text(n, &format!("f{n}"), u64::try_from(n).unwrap() * 10, true))
        .collect();
    items.push(text(13, "r", 1_000, false));
    assert_eq!(view(&items, "", ContentTypeFilter::All, 8), Some((12, 5)));
}

#[test]
fn empty_history_has_no_selection() {
    assert_eq!(view_all(&[]), None);
}

#[test]
fn filter_without_matches_has_no_selection() {
    let items = [text(1, "a", A, true), text(4, "d", D, false)];
    assert_eq!(view(&items, "zzz", ContentTypeFilter::All, 10), None);
}

#[test]
fn equal_recency_is_won_by_the_greater_id() {
    // mesmo last_activity: o de id 7 vence os de id 5, em qualquer posição
    let first = [text(7, "y", D, false), text(5, "x", D, false)];
    assert_eq!(view_all(&first), Some((0, 0)));
    let last = [text(5, "x", D, false), text(7, "y", D, false)];
    assert_eq!(view_all(&last), Some((1, 0)));
}

#[test]
fn page_size_zero_counts_as_one() {
    // A │ D C B com página 0 → D (1), topo 1 (página de uma linha)
    let items = [
        text(1, "a", A, true),
        text(4, "d", D, false),
        text(3, "c", C, false),
        text(2, "b", B, false),
    ];
    assert_eq!(view(&items, "", ContentTypeFilter::All, 0), Some((1, 1)));
}
