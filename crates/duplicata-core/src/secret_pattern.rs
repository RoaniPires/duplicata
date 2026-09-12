use crate::preview::decode_utf16le_nul_terminated;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretPatternKind {
    PrivateKeyPem,
    AccessToken,
    CardNumber,
}

impl SecretPatternKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            SecretPatternKind::PrivateKeyPem => "private_key_pem",
            SecretPatternKind::AccessToken => "access_token",
            SecretPatternKind::CardNumber => "card_number",
        }
    }
}

pub fn matches_secret_pattern(canonical_bytes: &[u8]) -> Option<SecretPatternKind> {
    let text = decode_utf16le_nul_terminated(canonical_bytes);
    if matches_pem_private_key(&text) {
        Some(SecretPatternKind::PrivateKeyPem)
    } else if matches_access_token(&text) {
        Some(SecretPatternKind::AccessToken)
    } else if matches_card_number(&text) {
        Some(SecretPatternKind::CardNumber)
    } else {
        None
    }
}

fn matches_pem_private_key(text: &str) -> bool {
    const WINDOW_CHARS: usize = 64;
    let Some(begin_idx) = text.find("-----BEGIN") else {
        return false;
    };
    let window: String = text[begin_idx..].chars().take(WINDOW_CHARS).collect();
    window.contains("PRIVATE KEY-----")
}

const TOKEN_PREFIXES: &[&str] = &["ghp_", "gho_", "github_pat_", "sk-proj-", "sk-"];
const MIN_TOKEN_SUFFIX_LEN: usize = 16;
const MIN_BEARER_TOKEN_LEN: usize = 20;

fn matches_access_token(text: &str) -> bool {
    for prefix in TOKEN_PREFIXES {
        if let Some(idx) = text.find(prefix) {
            let rest = &text[idx + prefix.len()..];
            let suffix_len = rest
                .chars()
                .take_while(|c| c.is_ascii_alphanumeric() || *c == '_' || *c == '-')
                .count();
            if suffix_len >= MIN_TOKEN_SUFFIX_LEN {
                return true;
            }
        }
    }
    matches_bearer_token(text)
}

fn matches_bearer_token(text: &str) -> bool {
    let Some(idx) = text.find("Bearer ") else {
        return false;
    };
    let rest = &text[idx + "Bearer ".len()..];
    let token_len = rest
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '/' | '=' | '-' | '_'))
        .count();
    token_len >= MIN_BEARER_TOKEN_LEN
}

fn matches_card_number(text: &str) -> bool {
    let mut digits: Vec<u32> = Vec::new();
    for c in text.chars() {
        if let Some(d) = c.to_digit(10) {
            digits.push(d);
        } else if c == ' ' || c == '-' {
            continue;
        } else if is_valid_card_digit_run(&digits) {
            return true;
        } else {
            digits.clear();
        }
    }
    is_valid_card_digit_run(&digits)
}

fn is_valid_card_digit_run(digits: &[u32]) -> bool {
    (13..=19).contains(&digits.len()) && luhn_checksum_valid(digits)
}

fn luhn_checksum_valid(digits: &[u32]) -> bool {
    let sum: u32 = digits
        .iter()
        .rev()
        .enumerate()
        .map(|(i, &d)| {
            if i % 2 == 1 {
                let doubled = d * 2;
                if doubled > 9 {
                    doubled - 9
                } else {
                    doubled
                }
            } else {
                d
            }
        })
        .sum();
    sum % 10 == 0
}
