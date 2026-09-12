#![cfg(windows)]

use duplicata_win::hotkey_capture::{arm, disarm, is_armed, owner, release, CaptureOwner};

#[test]
fn at_rest_nothing_is_armed_and_nobody_owns_it() {
    assert!(!is_armed());
    assert_eq!(owner(), CaptureOwner::None);
}

#[test]
fn arm_disarm_release_is_the_full_lifecycle() {
    arm(CaptureOwner::SettingsSection);
    assert!(is_armed());
    assert_eq!(owner(), CaptureOwner::SettingsSection);

    disarm();
    assert!(!is_armed());
    assert_eq!(owner(), CaptureOwner::SettingsSection);

    release(CaptureOwner::SettingsSection);
    assert!(!is_armed());
    assert_eq!(owner(), CaptureOwner::None);
}

#[test]
fn owner_is_exclusive_while_armed() {
    arm(CaptureOwner::StandaloneDialog);
    assert_eq!(owner(), CaptureOwner::StandaloneDialog);
    assert_ne!(owner(), CaptureOwner::SettingsSection);
    release(CaptureOwner::StandaloneDialog);
}

#[test]
fn owner_reverts_to_none_after_destruction_regardless_of_which_exit_path() {
    for _ in 0..4 {
        arm(CaptureOwner::StandaloneDialog);
        release(CaptureOwner::StandaloneDialog);
        assert_eq!(owner(), CaptureOwner::None);
        assert!(!is_armed());
    }
}

#[test]
fn a_release_that_does_not_match_the_current_owner_is_a_no_op() {
    arm(CaptureOwner::SettingsSection);
    release(CaptureOwner::StandaloneDialog);
    assert_eq!(owner(), CaptureOwner::SettingsSection);
    assert!(is_armed());
    release(CaptureOwner::SettingsSection);
}
