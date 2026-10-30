use duplicata_core::testsupport::FakeDisabledHotkeys;
use duplicata_core::win_v::{uninstall_release, UninstallRelease};

const OURS: &str = "open_with_win_v = true\nwin_v_added_v = true\n";

#[test]
fn our_v_is_removed_keeping_the_other_characters() {
    let mut store = FakeDisabledHotkeys::with(Some("QRV"));
    assert_eq!(
        uninstall_release(Some(OURS), &mut store),
        UninstallRelease::Removed
    );
    assert_eq!(store.value.as_deref(), Some("QR"));
}

#[test]
fn our_v_alone_deletes_the_value() {
    let mut store = FakeDisabledHotkeys::with(Some("V"));
    uninstall_release(Some(OURS), &mut store);
    assert_eq!(store.value, None);
}

#[test]
fn without_the_config_the_registry_is_untouched() {
    let mut store = FakeDisabledHotkeys::with(Some("V"));
    assert_eq!(
        uninstall_release(None, &mut store),
        UninstallRelease::Untouched
    );
    assert_eq!(store.value.as_deref(), Some("V"));
    assert_eq!(store.writes, 0);
}

#[test]
fn a_v_that_was_not_ours_is_untouched() {
    let mut store = FakeDisabledHotkeys::with(Some("V"));
    let config = "open_with_win_v = true\nwin_v_added_v = false\n";
    assert_eq!(
        uninstall_release(Some(config), &mut store),
        UninstallRelease::Untouched
    );
    assert_eq!(store.value.as_deref(), Some("V"));
}

#[test]
fn an_unreadable_flag_is_untouched() {
    let mut store = FakeDisabledHotkeys::with(Some("V"));
    let config = "win_v_added_v = talvez\n";
    assert_eq!(
        uninstall_release(Some(config), &mut store),
        UninstallRelease::Untouched
    );
    assert_eq!(store.writes, 0);
}

#[test]
fn a_registry_failure_is_reported() {
    let mut store = FakeDisabledHotkeys::with(Some("QRV"));
    store.fail_write = true;
    assert!(matches!(
        uninstall_release(Some(OURS), &mut store),
        UninstallRelease::Failed(_)
    ));
}
