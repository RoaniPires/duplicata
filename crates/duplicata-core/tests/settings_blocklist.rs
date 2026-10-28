use duplicata_core::settings_session::{add_program, remove_selected};

#[test]
fn adding_trims_the_name_and_appends_it() {
    let mut list = vec!["a.exe".to_string()];
    assert!(add_program(&mut list, "  keepass.exe  "));
    assert_eq!(list, vec!["a.exe".to_string(), "keepass.exe".to_string()]);
}

#[test]
fn an_empty_or_blank_name_is_ignored() {
    let mut list = Vec::new();
    assert!(!add_program(&mut list, ""));
    assert!(!add_program(&mut list, "   "));
    assert!(list.is_empty());
}

#[test]
fn a_name_already_in_the_list_ignoring_case_is_ignored() {
    let mut list = vec!["KeePass.exe".to_string()];
    assert!(!add_program(&mut list, "keepass.EXE"));
    assert!(!add_program(&mut list, " KEEPASS.exe "));
    assert_eq!(list, vec!["KeePass.exe".to_string()]);
}

#[test]
fn removing_takes_out_the_selected_name() {
    let mut list = vec!["a.exe".to_string(), "b.exe".to_string()];
    assert!(remove_selected(&mut list, Some(0)));
    assert_eq!(list, vec!["b.exe".to_string()]);
}

#[test]
fn removing_without_a_selection_or_out_of_range_does_nothing() {
    let mut list = vec!["a.exe".to_string()];
    assert!(!remove_selected(&mut list, None));
    assert!(!remove_selected(&mut list, Some(1)));
    assert_eq!(list, vec!["a.exe".to_string()]);
}
