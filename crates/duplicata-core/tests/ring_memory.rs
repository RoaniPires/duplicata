use duplicata_core::{ByteBudgetQueue, CaptureQueue, QUEUE_BYTE_BUDGET};

const MIB: usize = 1024 * 1024;

fn payload(mib: usize) -> Vec<u8> {
    vec![0u8; mib * MIB]
}

#[test]
fn production_queue_never_exceeds_its_declared_budget_under_a_flood_of_large_items() {
    let q: ByteBudgetQueue<Vec<u8>> = ByteBudgetQueue::new();

    for mib in [63usize, 40, 20, 63, 10, 63, 5, 63, 63, 1, 63] {
        q.push(payload(mib), mib * MIB);
        assert!(
            q.byte_len() <= QUEUE_BYTE_BUDGET,
            "orçamento agregado nunca pode passar de {QUEUE_BYTE_BUDGET} bytes; \
             ficou em {} depois de empurrar mais {mib} MiB",
            q.byte_len()
        );
    }
}

#[test]
fn a_single_item_larger_than_the_whole_budget_is_the_only_declared_exception() {
    let q: ByteBudgetQueue<Vec<u8>> = ByteBudgetQueue::new();
    let oversized = QUEUE_BYTE_BUDGET + 10 * MIB;
    q.push(vec![0u8; 1], oversized);
    assert_eq!(
        q.byte_len(),
        oversized,
        "o único item sempre entra, mesmo sozinho acima do teto"
    );
    assert_eq!(q.len(), 1);
}
