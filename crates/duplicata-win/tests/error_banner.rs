#![cfg(windows)]

use std::path::Path;

use duplicata_core::StoreError;
use duplicata_win::error_banner::{
    banner_for, point_in_rect, recreate_button_rect, store_error_kind,
};

#[test]
fn corrupted_offers_recreate_and_names_the_path_when_allowed() {
    let banner = banner_for(
        &StoreError::Corrupted,
        Path::new(r"C:\dup\duplicata.db"),
        true,
    );
    assert!(banner.offer_recreate);
    assert!(banner
        .lines
        .iter()
        .any(|l| l.contains(r"C:\dup\duplicata.db")));
    assert!(banner.lines.iter().any(|l| l.contains("corromp")));
}

#[test]
fn corrupted_never_offers_recreate_when_the_caller_says_it_is_not_safe() {
    let banner = banner_for(&StoreError::Corrupted, Path::new("db.sqlite"), false);
    assert!(!banner.offer_recreate, "recriar nunca é oferecido aqui");
    assert!(
        banner
            .lines
            .iter()
            .any(|l| l.contains("NÃO foram perdidos")),
        "o texto tranquiliza sobre os dados: {:?}",
        banner.lines
    );
    assert!(
        !banner.lines.iter().any(|l| l.contains("apaga TODO")),
        "nenhuma menção a apagar tudo"
    );
}

#[test]
fn io_does_not_offer_recreate() {
    assert!(!banner_for(&StoreError::Io, Path::new("db.sqlite"), true).offer_recreate);
}

#[test]
fn migration_and_query_also_do_not_offer_recreate() {
    for err in [StoreError::Migration, StoreError::Query] {
        let banner = banner_for(&err, Path::new("db.sqlite"), true);
        assert!(
            !banner.offer_recreate,
            "{err:?} não deveria oferecer recriar"
        );
    }
}

#[test]
fn corrupted_and_io_banners_have_different_text() {
    let corrupted = banner_for(&StoreError::Corrupted, Path::new("db.sqlite"), true);
    let io = banner_for(&StoreError::Io, Path::new("db.sqlite"), true);
    assert_ne!(corrupted.lines, io.lines);
}

#[test]
fn store_error_kind_is_a_distinct_short_code_per_variant() {
    let kinds = [
        store_error_kind(&StoreError::Corrupted),
        store_error_kind(&StoreError::Io),
        store_error_kind(&StoreError::Migration),
        store_error_kind(&StoreError::Query),
    ];
    for k in kinds {
        assert!(!k.is_empty());
    }
    let unique: std::collections::HashSet<_> = kinds.iter().collect();
    assert_eq!(
        unique.len(),
        kinds.len(),
        "cada variant precisa de um código só seu"
    );
}

#[test]
fn recreate_button_sits_in_the_bottom_strip_with_margins() {
    let r = recreate_button_rect(420, 480);
    assert!(r.top > 0 && r.top < 480);
    assert_eq!(r.bottom, 480);
    assert!(r.left > 0);
    assert!(r.right < 420);
    assert!(r.right > r.left);
}

#[test]
fn point_inside_the_button_hits() {
    let r = recreate_button_rect(420, 480);
    let mid_x = (r.left + r.right) / 2;
    let mid_y = (r.top + r.bottom) / 2;
    assert!(point_in_rect(mid_x, mid_y, &r));
}

#[test]
fn point_above_the_button_misses() {
    let r = recreate_button_rect(420, 480);
    assert!(!point_in_rect((r.left + r.right) / 2, r.top - 1, &r));
}

#[test]
fn point_outside_left_or_right_misses() {
    let r = recreate_button_rect(420, 480);
    let y = (r.top + r.bottom) / 2;
    assert!(!point_in_rect(r.left - 1, y, &r));
    assert!(!point_in_rect(r.right, y, &r));
}

#[test]
fn degenerate_tiny_client_size_never_panics_and_stays_a_valid_rect() {
    for (w, h) in [(0, 0), (1, 1), (5, 10), (-3, -3)] {
        let r = recreate_button_rect(w, h);
        assert!(r.right >= r.left);
        assert!(r.bottom >= r.top);
    }
}
