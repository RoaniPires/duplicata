//! Campos numéricos das Configurações: o que entra no campo e como o texto
//! vira o valor ajustado no Aplicar/Salvar.

use crate::config::{MAX_MAX_PINNED, MIN_MAX_ITEMS, MIN_RETENTION_DAYS};

/// Dígitos de `u32::MAX`: o campo não aceita mais do que isso.
pub const NUMERIC_MAX_DIGITS: usize = 10;

pub fn digits_only(text: &str) -> String {
    text.chars().filter(char::is_ascii_digit).collect()
}

/// Vazio volta a `previous`; acima de `u32::MAX` fica em `u32::MAX`.
fn parse_or(text: &str, previous: u32) -> u32 {
    let digits = digits_only(text);
    if digits.is_empty() {
        return previous;
    }
    // Só dígitos: o único erro possível é estouro.
    let n = digits.parse::<u64>().unwrap_or(u64::MAX);
    u32::try_from(n).unwrap_or(u32::MAX)
}

pub fn normalize_retention_days(text: &str, previous: u32) -> u32 {
    let min = u32::try_from(MIN_RETENTION_DAYS).unwrap_or(1);
    parse_or(text, previous).max(min)
}

pub fn normalize_max_items(text: &str, previous: u32) -> u32 {
    parse_or(text, previous).max(MIN_MAX_ITEMS)
}

pub fn normalize_max_pinned(text: &str, previous: u32) -> u32 {
    parse_or(text, previous).min(MAX_MAX_PINNED)
}
