use duplicata_core::identity_of;

#[test]
fn is_blake3_of_the_raw_bytes() {
    let bytes = b"exemplo de conteudo";
    let expected = *blake3::hash(bytes).as_bytes();
    assert_eq!(identity_of(bytes).0, expected);
}

#[test]
fn crlf_and_lf_produce_different_keys() {
    let crlf = identity_of(b"linha1\r\nlinha2");
    let lf = identity_of(b"linha1\nlinha2");
    assert_ne!(crlf, lf, "CRLF != LF — importante para quem copia codigo");
}

#[test]
fn no_trimming_or_case_folding() {
    assert_ne!(identity_of(b"  x  "), identity_of(b"x"));
    assert_ne!(identity_of(b"ABC"), identity_of(b"abc"));
}

#[test]
fn empty_input_is_stable() {
    assert_eq!(identity_of(b""), identity_of(&[]));
    assert_eq!(identity_of(b"").0, *blake3::hash(b"").as_bytes());
}

#[test]
fn identical_bytes_produce_identical_keys() {
    let a = vec![1u8, 2, 3, 4, 5];
    let b = a.clone();
    assert_eq!(identity_of(&a), identity_of(&b));
}
