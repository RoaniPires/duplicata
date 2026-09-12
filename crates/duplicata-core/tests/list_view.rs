use std::time::Instant;

use duplicata_core::list_view::{
    arrange_for_display, category_of, pinned_group_boundary, text_matches, visible_rows,
    ContentTypeFilter,
};
use duplicata_core::ClipListItem;

fn item(id: i64, kind: &str, preview: Option<&str>, pinned: bool) -> ClipListItem {
    ClipListItem {
        id,
        canonical_kind: kind.to_string(),
        preview: preview.map(str::to_string),
        thumbnail: None,
        has_text: preview.is_some(),
        last_activity_ms: 0,
        pinned,
    }
}

#[test]
fn category_of_maps_each_known_kind() {
    assert_eq!(category_of("unicode_text"), Some(ContentTypeFilter::Text));
    assert_eq!(category_of("hdrop"), Some(ContentTypeFilter::Files));
    assert_eq!(category_of("dib"), Some(ContentTypeFilter::Image));
    assert_eq!(category_of("dibv5"), Some(ContentTypeFilter::Image));
}

#[test]
fn category_of_is_none_for_custom_and_unknown() {
    assert_eq!(category_of("custom"), None);
    assert_eq!(category_of("algo_que_nao_existe"), None);
    assert_eq!(category_of(""), None);
}

#[test]
fn text_matches_is_case_insensitive_substring() {
    assert!(text_matches("Um Link Importante", "link"));
    assert!(text_matches("UPPER", "upper"));
    assert!(text_matches("meio da string", "io da str"));
    assert!(!text_matches("nada a ver", "link"));
}

#[test]
fn text_matches_empty_needle_is_always_true_even_against_empty_label() {
    assert!(text_matches("", ""));
    assert!(text_matches("qualquer", ""));
}

#[test]
fn text_matches_nonempty_needle_never_matches_empty_label() {
    assert!(!text_matches("", "x"));
}

#[test]
fn hdrop_matches_each_name_isolated_and_the_separator_is_not_matchable() {
    let items = vec![item(
        1,
        "hdrop",
        Some("relatorio.pdf\nfoto_ferias.jpg"),
        false,
    )];

    assert_eq!(
        visible_rows(&items, "ferias", ContentTypeFilter::All),
        vec![0]
    );
    assert_eq!(
        visible_rows(&items, "relatorio", ContentTypeFilter::All),
        vec![0]
    );

    assert!(visible_rows(&items, "pdffoto", ContentTypeFilter::All).is_empty());
    assert!(visible_rows(&items, "pdf\nfoto", ContentTypeFilter::All).is_empty());
}

#[test]
fn image_without_preview_never_matches_a_text_filter() {
    let items = vec![item(1, "dib", None, false)];
    assert!(visible_rows(&items, "foto", ContentTypeFilter::All).is_empty());
    assert_eq!(visible_rows(&items, "", ContentTypeFilter::Image), vec![0]);
}

#[test]
fn arrange_puts_pinned_first_preserving_input_order_in_each_group() {
    let items = vec![
        item(10, "unicode_text", Some("a"), false),
        item(11, "unicode_text", Some("b"), true),
        item(12, "unicode_text", Some("c"), false),
        item(13, "unicode_text", Some("d"), true),
    ];
    assert_eq!(arrange_for_display(&items), vec![1, 3, 0, 2]);
}

#[test]
fn arrange_is_a_permutation_and_handles_homogeneous_and_empty() {
    let all_pinned = vec![item(1, "unicode_text", Some("x"), true); 3];
    assert_eq!(arrange_for_display(&all_pinned), vec![0, 1, 2]);

    let none_pinned = vec![item(1, "unicode_text", Some("x"), false); 3];
    assert_eq!(arrange_for_display(&none_pinned), vec![0, 1, 2]);

    let empty: Vec<ClipListItem> = Vec::new();
    assert!(arrange_for_display(&empty).is_empty());
}

fn mixed_history() -> Vec<ClipListItem> {
    vec![
        item(1, "unicode_text", Some("nota antiga fixada"), true),
        item(2, "hdrop", Some("planilha.xlsx"), true),
        item(3, "unicode_text", Some("link recente"), false),
        item(4, "dib", None, false),
        item(5, "custom", None, false),
        item(6, "unicode_text", Some("outra nota"), false),
    ]
}

#[test]
fn all_plus_empty_text_equals_arrange_for_display() {
    let items = mixed_history();
    assert_eq!(
        visible_rows(&items, "", ContentTypeFilter::All),
        arrange_for_display(&items)
    );
}

#[test]
fn type_filter_only() {
    let items = mixed_history();
    assert_eq!(
        visible_rows(&items, "", ContentTypeFilter::Text),
        vec![0, 2, 5]
    );
    assert_eq!(visible_rows(&items, "", ContentTypeFilter::Image), vec![3]);
    assert_eq!(visible_rows(&items, "", ContentTypeFilter::Files), vec![1]);
}

#[test]
fn custom_item_appears_only_under_all() {
    let items = mixed_history();
    assert!(visible_rows(&items, "", ContentTypeFilter::All).contains(&4));
    for f in [
        ContentTypeFilter::Text,
        ContentTypeFilter::Image,
        ContentTypeFilter::Files,
    ] {
        assert!(!visible_rows(&items, "", f).contains(&4));
    }
}

#[test]
fn type_and_text_intersect_and_keep_the_partition() {
    let items = mixed_history();
    assert_eq!(
        visible_rows(&items, "nota", ContentTypeFilter::Text),
        vec![0, 5]
    );
    assert!(visible_rows(&items, "zzz", ContentTypeFilter::All).is_empty());
    assert_eq!(
        visible_rows(&items, "  LINK  ", ContentTypeFilter::All),
        vec![2]
    );
}

#[test]
fn boundary_is_the_first_unpinned_index_in_the_order() {
    let items = mixed_history();
    let order = arrange_for_display(&items);
    assert_eq!(pinned_group_boundary(&items, &order), Some(2));
}

#[test]
fn boundary_is_none_when_a_group_is_empty_or_a_filter_removes_all_pinned() {
    let items = mixed_history();

    let unpinned_only: Vec<usize> = vec![2, 3, 4, 5];
    assert_eq!(pinned_group_boundary(&items, &unpinned_only), None);

    let pinned_only: Vec<usize> = vec![0, 1];
    assert_eq!(pinned_group_boundary(&items, &pinned_only), None);

    let order = visible_rows(&items, "outra", ContentTypeFilter::All);
    assert_eq!(order, vec![5]);
    assert_eq!(pinned_group_boundary(&items, &order), None);

    assert_eq!(pinned_group_boundary(&items, &[]), None);
}

#[test]
fn visible_rows_over_700_items_stays_well_under_a_frame() {
    let mut items = Vec::with_capacity(700);
    for i in 0..700i64 {
        let pinned = i < 200;
        let kind = if i % 3 == 0 { "unicode_text" } else { "hdrop" };
        let preview =
            format!("item numero {i} com um texto de preview razoavelmente longo\noutro.txt");
        items.push(item(i, kind, Some(&preview), pinned));
    }

    let _ = visible_rows(&items, "numero 42", ContentTypeFilter::Text);

    let iters = 50;
    let started = Instant::now();
    for _ in 0..iters {
        let rows = visible_rows(&items, "preview", ContentTypeFilter::Text);
        assert!(!rows.is_empty());
    }
    let per_call = started.elapsed() / iters;
    assert!(
        per_call.as_millis() < 16,
        "visible_rows sobre 700 itens levou {per_call:?}/chamada — orçamento SC-004 é 16 ms; \
         suspeitar de regressão algorítmica"
    );
}

#[test]
fn sc013_a_type_filter_shows_all_of_its_category_and_nothing_else() {
    let items = vec![
        item(9, "unicode_text", Some("texto um"), false),
        item(8, "dib", None, false),
        item(7, "hdrop", Some("a.pdf"), false),
        item(6, "custom", None, false),
        item(5, "unicode_text", Some("texto dois"), true),
        item(4, "dibv5", None, false),
        item(3, "custom", Some("algo"), false),
        item(2, "hdrop", Some("b.png\nc.txt"), false),
        item(1, "unicode_text", Some("texto três"), false),
    ];

    for (filter, kinds) in [
        (ContentTypeFilter::Text, vec!["unicode_text"]),
        (ContentTypeFilter::Image, vec!["dib", "dibv5"]),
        (ContentTypeFilter::Files, vec!["hdrop"]),
    ] {
        let visible = visible_rows(&items, "", filter);
        let esperados: Vec<usize> = items
            .iter()
            .enumerate()
            .filter(|(_, it)| kinds.contains(&it.canonical_kind.as_str()))
            .map(|(i, _)| i)
            .collect();

        for i in &esperados {
            assert!(
                visible.contains(i),
                "{filter:?}: item {} ({}) sumiu",
                items[*i].id,
                items[*i].canonical_kind
            );
        }
        for i in &visible {
            assert!(
                kinds.contains(&items[*i].canonical_kind.as_str()),
                "{filter:?}: entrou item {} de tipo {}",
                items[*i].id,
                items[*i].canonical_kind
            );
        }
        assert_eq!(visible.len(), esperados.len());
    }
}

#[test]
fn sc013_all_includes_the_items_with_no_category() {
    let items = vec![
        item(3, "unicode_text", Some("texto"), false),
        item(2, "custom", None, false),
        item(1, "algo_desconhecido", Some("x"), false),
    ];
    let visible = visible_rows(&items, "", ContentTypeFilter::All);
    assert_eq!(visible.len(), 3, "'Tudo' não pode esconder nada");

    for f in [
        ContentTypeFilter::Text,
        ContentTypeFilter::Image,
        ContentTypeFilter::Files,
    ] {
        let v = visible_rows(&items, "", f);
        assert!(
            !v.contains(&1) && !v.contains(&2),
            "{f:?} não pode incluir item sem categoria"
        );
    }
}

#[test]
fn sc016_pinned_are_all_above_unpinned_and_each_group_stays_by_recency() {
    let items = vec![
        item(9, "unicode_text", Some("nota nove"), false),
        item(8, "unicode_text", Some("nota oito"), true),
        item(7, "unicode_text", Some("nota sete"), false),
        item(6, "unicode_text", Some("nota seis"), true),
        item(5, "unicode_text", Some("outra coisa"), false),
        item(4, "unicode_text", Some("nota quatro"), true),
        item(3, "unicode_text", Some("nota três"), false),
    ];

    for needle in ["", "nota"] {
        let visible = visible_rows(&items, needle, ContentTypeFilter::All);
        let ids: Vec<i64> = visible.iter().map(|&i| items[i].id).collect();
        let pinned: Vec<bool> = visible.iter().map(|&i| items[i].pinned).collect();

        let primeiro_nao_fixado = pinned.iter().position(|p| !p);
        if let Some(k) = primeiro_nao_fixado {
            assert!(
                pinned[k..].iter().all(|p| !p),
                "needle={needle:?}: fixado abaixo de não fixado — {ids:?} / {pinned:?}"
            );
        }

        let fixados: Vec<i64> = visible
            .iter()
            .filter(|&&i| items[i].pinned)
            .map(|&i| items[i].id)
            .collect();
        let soltos: Vec<i64> = visible
            .iter()
            .filter(|&&i| !items[i].pinned)
            .map(|&i| items[i].id)
            .collect();
        for grupo in [&fixados, &soltos] {
            assert!(
                grupo.windows(2).all(|w| w[0] > w[1]),
                "needle={needle:?}: grupo fora da recência: {grupo:?}"
            );
        }
    }
}

#[test]
fn sc033_the_three_empty_states_are_distinct_from_one_another() {
    use duplicata_core::list_view::{display_state, ListDisplayState};

    let no_match = display_state(false, false, 42, 0, true);
    let empty = display_state(false, false, 0, 0, false);
    let unreadable = display_state(false, true, 0, 0, false);

    assert_eq!(no_match, ListDisplayState::NoMatch);
    assert_eq!(empty, ListDisplayState::EmptyHistory);
    assert_eq!(unreadable, ListDisplayState::ErrorBanner);
    assert_ne!(no_match, empty);
    assert_ne!(no_match, unreadable);
    assert_ne!(empty, unreadable);
}

#[test]
fn sc033_an_unreadable_history_wins_over_any_list_state() {
    use duplicata_core::list_view::{display_state, ListDisplayState};
    for (total, visiveis, filtro) in [(0, 0, false), (0, 0, true), (10, 3, false), (10, 0, true)] {
        assert_eq!(
            display_state(false, true, total, visiveis, filtro),
            ListDisplayState::ErrorBanner
        );
    }
}

#[test]
fn sc033_a_toggle_in_progress_wins_over_everything_including_the_error_banner() {
    use duplicata_core::list_view::{display_state, ListDisplayState};
    assert_eq!(
        display_state(true, true, 10, 5, true),
        ListDisplayState::ToggleBanner
    );
}

#[test]
fn sc033_having_anything_visible_always_paints_the_list() {
    use duplicata_core::list_view::{display_state, ListDisplayState};
    for filtro in [false, true] {
        assert_eq!(
            display_state(false, false, 10, 1, filtro),
            ListDisplayState::Rows
        );
    }
}
