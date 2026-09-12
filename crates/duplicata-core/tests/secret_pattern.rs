use duplicata_core::{matches_secret_pattern, utf16le, SecretPatternKind};

#[test]
fn pem_rsa_private_key_block_matches() {
    let text =
        "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA...\n-----END RSA PRIVATE KEY-----";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::PrivateKeyPem)
    );
}

#[test]
fn pem_plain_private_key_block_matches() {
    let text = "-----BEGIN PRIVATE KEY-----\nMIIEvQ...\n-----END PRIVATE KEY-----";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::PrivateKeyPem)
    );
}

#[test]
fn pem_openssh_private_key_block_matches() {
    let text = "-----BEGIN OPENSSH PRIVATE KEY-----\nb3BlbnNzaC1rZXk...\n-----END OPENSSH PRIVATE KEY-----";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::PrivateKeyPem)
    );
}

#[test]
fn pem_public_certificate_does_not_match() {
    let text = "-----BEGIN CERTIFICATE-----\nMIIDXTCCAkWgAwIBAgIJAK...\n-----END CERTIFICATE-----";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn mentioning_private_key_far_from_any_begin_marker_does_not_match() {
    let text = "Este documento fala sobre o conceito de chave privada em geral, \
                sem nenhum bloco -----BEGIN de verdade por perto.";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn github_personal_access_token_prefix_matches() {
    let text = "ghp_1234567890abcdefghijklmnopqrstuvwxyz";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::AccessToken)
    );
}

#[test]
fn github_fine_grained_pat_prefix_matches() {
    let text = "github_pat_11ABCDEFG0abcdefghijklmnopqrstuvwxyz0123456789";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::AccessToken)
    );
}

#[test]
fn sk_style_api_key_prefix_matches() {
    let text = "sk-proj-abcdefghijklmnopqrstuvwxyz0123456789";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::AccessToken)
    );
}

#[test]
fn bearer_token_with_a_long_enough_value_matches() {
    let text = "Authorization: Bearer eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.payload.signature";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::AccessToken)
    );
}

#[test]
fn bare_prefix_with_a_too_short_suffix_does_not_match() {
    let text = "sk-abc";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn arbitrary_text_does_not_match_as_a_token() {
    let text = "esta é uma frase comum, sem nenhum prefixo de token conhecido";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn a_valid_test_visa_number_matches() {
    let text = "4111111111111111";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::CardNumber)
    );
}

#[test]
fn the_same_number_with_spaces_and_dashes_still_matches() {
    assert_eq!(
        matches_secret_pattern(&utf16le("4111 1111 1111 1111")),
        Some(SecretPatternKind::CardNumber)
    );
    assert_eq!(
        matches_secret_pattern(&utf16le("4111-1111-1111-1111")),
        Some(SecretPatternKind::CardNumber)
    );
}

#[test]
fn a_digit_sequence_that_fails_the_luhn_checksum_does_not_match() {
    let text = "4111111111111112";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn a_digit_run_outside_13_to_19_digits_never_matches_by_substring() {
    let text = "1234567890123456789012345";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn ordinary_text_without_long_digit_runs_does_not_match() {
    let text = "reunião às 14h30 na sala 302, ramal 5521";
    assert_eq!(matches_secret_pattern(&utf16le(text)), None);
}

#[test]
fn when_multiple_patterns_could_match_pem_wins_by_being_checked_first() {
    let text = "-----BEGIN PRIVATE KEY-----\n1234567890123456789\n-----END PRIVATE KEY-----";
    assert_eq!(
        matches_secret_pattern(&utf16le(text)),
        Some(SecretPatternKind::PrivateKeyPem)
    );
}
