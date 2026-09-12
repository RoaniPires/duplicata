#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Approval {
    Aprovado,
    Desaprovado,
    Indeterminado,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StartupState {
    Ligado,
    Desligado,
    DesligadoPeloGerenciador,
}

const DISAPPROVED_FIRST_BYTE: u8 = 0x03;

pub fn interpret_approval(value: Option<&[u8]>) -> Approval {
    match value {
        Some([first, ..]) if *first == DISAPPROVED_FIRST_BYTE => Approval::Desaprovado,
        _ => Approval::Indeterminado,
    }
}

pub fn startup_state(entry_present: bool, approval: Approval) -> StartupState {
    if !entry_present {
        return StartupState::Desligado;
    }
    match approval {
        Approval::Desaprovado => StartupState::DesligadoPeloGerenciador,
        Approval::Aprovado | Approval::Indeterminado => StartupState::Ligado,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn none_is_indeterminado() {
        assert_eq!(interpret_approval(None), Approval::Indeterminado);
    }

    #[test]
    fn empty_slice_is_indeterminado() {
        assert_eq!(interpret_approval(Some(&[])), Approval::Indeterminado);
    }

    #[test]
    fn unexpected_short_lengths_are_indeterminado() {
        for len in [1usize, 3, 7] {
            let buf = vec![0xFFu8; len];
            assert_eq!(
                interpret_approval(Some(&buf)),
                Approval::Indeterminado,
                "tamanho {len} não deveria ser reconhecido"
            );
        }
    }

    #[test]
    fn long_unrecognized_value_is_indeterminado() {
        let buf = vec![0xAAu8; 64];
        assert_eq!(interpret_approval(Some(&buf)), Approval::Indeterminado);
    }

    #[test]
    fn value_that_looks_like_the_approved_pattern_is_still_indeterminado() {
        let mut buf = vec![0u8; 12];
        buf[0] = 0x02;
        assert_eq!(interpret_approval(Some(&buf)), Approval::Indeterminado);
    }

    #[test]
    fn recognized_disapproved_pattern_is_desaprovado() {
        let mut buf = vec![0u8; 12];
        buf[0] = 0x03;
        assert_eq!(interpret_approval(Some(&buf)), Approval::Desaprovado);
    }

    #[test]
    fn only_the_disapproved_first_byte_ever_produces_desaprovado() {
        for first in 0u8..=255 {
            if first == DISAPPROVED_FIRST_BYTE {
                continue;
            }
            let buf = [first, 0, 0, 0];
            assert_ne!(
                interpret_approval(Some(&buf)),
                Approval::Desaprovado,
                "byte {first:#04x} não deveria produzir Desaprovado"
            );
        }
    }

    #[test]
    fn startup_state_absent_entry_is_desligado_regardless_of_approval() {
        assert_eq!(
            startup_state(false, Approval::Aprovado),
            StartupState::Desligado
        );
        assert_eq!(
            startup_state(false, Approval::Desaprovado),
            StartupState::Desligado
        );
        assert_eq!(
            startup_state(false, Approval::Indeterminado),
            StartupState::Desligado
        );
    }

    #[test]
    fn startup_state_present_and_approved_is_ligado() {
        assert_eq!(
            startup_state(true, Approval::Aprovado),
            StartupState::Ligado
        );
    }

    #[test]
    fn startup_state_present_and_indeterminado_is_ligado() {
        assert_eq!(
            startup_state(true, Approval::Indeterminado),
            StartupState::Ligado
        );
    }

    #[test]
    fn startup_state_present_and_disapproved_is_desligado_pelo_gerenciador() {
        assert_eq!(
            startup_state(true, Approval::Desaprovado),
            StartupState::DesligadoPeloGerenciador
        );
    }
}
