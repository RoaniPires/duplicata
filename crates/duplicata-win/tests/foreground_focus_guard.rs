#![cfg(windows)]

use duplicata_win::history_window::foreground_really_changed_away;
use windows::Win32::Foundation::HWND;

fn fake_hwnd(v: isize) -> HWND {
    HWND(v as *mut std::ffi::c_void)
}

#[test]
fn same_hwnd_as_current_foreground_is_not_a_real_loss() {
    let hwnd = fake_hwnd(1);
    assert!(!foreground_really_changed_away(hwnd, hwnd));
}

#[test]
fn a_different_foreground_hwnd_is_a_real_loss() {
    let hwnd = fake_hwnd(1);
    let other = fake_hwnd(2);
    assert!(foreground_really_changed_away(hwnd, other));
}

#[test]
fn null_current_foreground_counts_as_a_real_loss() {
    let hwnd = fake_hwnd(1);
    assert!(foreground_really_changed_away(hwnd, HWND::default()));
}
