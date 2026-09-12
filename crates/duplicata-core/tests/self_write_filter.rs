use duplicata_core::self_write_filter::SelfWriteFilter;

#[test]
fn consume_without_prior_mark_is_false() {
    let f = SelfWriteFilter::new();
    assert!(!f.consume_if_pending());
}

#[test]
fn mark_then_one_consume_is_true() {
    let f = SelfWriteFilter::new();
    f.mark_pending();
    assert!(f.consume_if_pending());
}

#[test]
fn a_second_consume_right_after_without_a_new_mark_is_false() {
    let f = SelfWriteFilter::new();
    f.mark_pending();
    assert!(
        f.consume_if_pending(),
        "primeira consulta consome o pendente"
    );
    assert!(
        !f.consume_if_pending(),
        "segunda consulta, sem novo mark_pending(), não deve suprimir de novo"
    );
}
