use std::cell::Cell;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CaptureOwner {
    #[default]
    None,
    SettingsSection,
    StandaloneDialog,
}

thread_local! {
    static ARMED: Cell<bool> = const { Cell::new(false) };
    static OWNER: Cell<CaptureOwner> = const { Cell::new(CaptureOwner::None) };
}

pub fn arm(owner: CaptureOwner) {
    OWNER.with(|o| o.set(owner));
    ARMED.with(|a| a.set(true));
}

pub fn disarm() {
    ARMED.with(|a| a.set(false));
}

pub fn release(owner: CaptureOwner) {
    OWNER.with(|o| {
        if o.get() == owner {
            o.set(CaptureOwner::None);
            ARMED.with(|a| a.set(false));
        }
    });
}

pub fn is_armed() -> bool {
    ARMED.with(Cell::get)
}

pub fn owner() -> CaptureOwner {
    OWNER.with(Cell::get)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn armed_and_owner_start_at_rest() {
        assert!(!is_armed());
        assert_eq!(owner(), CaptureOwner::None);
    }

    #[test]
    fn arming_sets_both_armed_and_owner() {
        arm(CaptureOwner::SettingsSection);
        assert!(is_armed());
        assert_eq!(owner(), CaptureOwner::SettingsSection);
    }

    #[test]
    fn disarm_clears_armed_but_keeps_the_owner() {
        arm(CaptureOwner::SettingsSection);
        disarm();
        assert!(!is_armed());
        assert_eq!(
            owner(),
            CaptureOwner::SettingsSection,
            "Esc desarma sem soltar a detenção (contracts §3)"
        );
    }

    #[test]
    fn rearming_after_a_disarm_works_without_releasing_first() {
        arm(CaptureOwner::SettingsSection);
        disarm();
        arm(CaptureOwner::SettingsSection);
        assert!(is_armed(), "clicar \"Alterar\" de novo deve rearmar");
    }

    #[test]
    fn release_by_the_current_owner_clears_everything() {
        arm(CaptureOwner::StandaloneDialog);
        release(CaptureOwner::StandaloneDialog);
        assert!(!is_armed());
        assert_eq!(owner(), CaptureOwner::None);
    }

    #[test]
    fn release_by_a_different_owner_is_a_no_op() {
        arm(CaptureOwner::SettingsSection);
        release(CaptureOwner::StandaloneDialog);
        assert_eq!(
            owner(),
            CaptureOwner::SettingsSection,
            "release só tem efeito quando vem de quem detém a captura agora"
        );
        assert!(is_armed(), "e não deve desarmar por engano junto");
    }
}
