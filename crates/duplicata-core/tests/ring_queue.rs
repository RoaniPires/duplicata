use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use duplicata_core::{ByteBudgetQueue, CaptureQueue};

const MIB: usize = 1024 * 1024;

fn payload(tag: u8, mib: usize) -> Vec<u8> {
    let mut v = vec![0u8; mib * MIB];
    v[0] = tag;
    v
}

#[test]
fn large_payloads_evict_front_until_the_new_item_fits() {
    let q: ByteBudgetQueue<Vec<u8>> = ByteBudgetQueue::with_budget(128 * MIB);

    for tag in 1..=3u8 {
        let p = payload(tag, 40);
        let n = p.len();
        assert!(q.push(p, n).is_clean(), "os 3 primeiros cabem em 128 MiB");
    }
    assert_eq!(q.len(), 3);
    assert_eq!(q.byte_len(), 120 * MIB);

    let p4 = payload(4, 40);
    let n4 = p4.len();
    let out = q.push(p4, n4);
    assert_eq!(
        out.evicted_bytes,
        vec![40 * MIB],
        "1 item descartado, 40 MiB"
    );
    assert_eq!(out.evicted_count(), 1);
    assert_eq!(q.len(), 3);
    assert_eq!(q.byte_len(), 120 * MIB);
    assert_eq!(q.evicted_total(), 1);

    assert_eq!(q.pop().unwrap()[0], 2);
    assert_eq!(q.pop().unwrap()[0], 3);
    assert_eq!(q.pop().unwrap()[0], 4);
}

#[test]
fn one_big_item_can_evict_several_small_ones() {
    let q: ByteBudgetQueue<Vec<u8>> = ByteBudgetQueue::with_budget(64 * MIB);
    for tag in 1..=5u8 {
        let p = payload(tag, 10);
        let n = p.len();
        q.push(p, n);
    }
    assert_eq!(q.len(), 5);

    let big = payload(9, 60);
    let n = big.len();
    let out = q.push(big, n);
    assert_eq!(out.evicted_count(), 5);
    assert_eq!(
        out.evicted_bytes,
        vec![10 * MIB; 5],
        "5 itens de 10 MiB cada"
    );
    assert_eq!(q.len(), 1);
    assert_eq!(q.pop().unwrap()[0], 9);
}

#[test]
fn push_does_not_block_when_full_and_completes_quickly() {
    let q: ByteBudgetQueue<Vec<u8>> = ByteBudgetQueue::with_budget(4 * MIB);
    for tag in 0..8u8 {
        let p = payload(tag, 1);
        let n = p.len();
        let start = Instant::now();
        q.push(p, n);
        assert!(
            start.elapsed() < Duration::from_millis(50),
            "push nunca bloqueia mesmo com a fila no teto"
        );
    }
    assert!(q.byte_len() <= 4 * MIB);
}

#[test]
fn pop_blocks_until_an_item_is_pushed() {
    let q: Arc<ByteBudgetQueue<u8>> = Arc::new(ByteBudgetQueue::with_budget(1024));
    let q2 = Arc::clone(&q);
    let handle = thread::spawn(move || q2.pop());

    thread::sleep(Duration::from_millis(50));
    assert!(
        !handle.is_finished(),
        "pop bloqueia enquanto a fila está vazia"
    );

    q.push(7, 1);
    assert_eq!(handle.join().unwrap(), Some(7));
}

#[test]
fn close_drains_remaining_then_returns_none() {
    let q: ByteBudgetQueue<u8> = ByteBudgetQueue::with_budget(1024);
    q.push(1, 1);
    q.push(2, 1);
    q.close();
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), None);
    assert_eq!(q.pop(), None);
}

#[test]
fn close_wakes_a_blocked_pop() {
    let q: Arc<ByteBudgetQueue<u8>> = Arc::new(ByteBudgetQueue::with_budget(1024));
    let q2 = Arc::clone(&q);
    let handle = thread::spawn(move || q2.pop());
    thread::sleep(Duration::from_millis(30));
    q.close();
    assert_eq!(handle.join().unwrap(), None);
}

#[test]
fn close_drains_pending_items_for_a_blocked_consumer_before_signaling_end() {
    let q: Arc<ByteBudgetQueue<u32>> = Arc::new(ByteBudgetQueue::with_budget(64 * 1024));
    let q2 = Arc::clone(&q);

    let consumer = thread::spawn(move || {
        let mut drained = Vec::new();
        while let Some(v) = q2.pop() {
            drained.push(v);
        }
        drained
    });

    thread::sleep(Duration::from_millis(20));
    for v in 1..=5u32 {
        q.push(v, 8);
    }
    q.close();

    assert_eq!(
        consumer.join().unwrap(),
        vec![1, 2, 3, 4, 5],
        "close() drena os pendentes antes de sinalizar fim"
    );
}

#[test]
fn blocked_pop_never_returns_spuriously_and_yields_exactly_what_was_pushed() {
    let q: Arc<ByteBudgetQueue<u32>> = Arc::new(ByteBudgetQueue::with_budget(64 * 1024));
    let q2 = Arc::clone(&q);

    let handle = thread::spawn(move || {
        let mut got = Vec::new();
        for _ in 0..3 {
            got.push(q2.pop().unwrap());
        }
        got
    });

    thread::sleep(Duration::from_millis(300));
    assert!(!handle.is_finished(), "pop não acorda por conta própria");

    q.push(10, 4);
    q.push(20, 4);
    q.push(30, 4);
    assert_eq!(handle.join().unwrap(), vec![10, 20, 30]);
    assert!(q.is_empty());
}

#[test]
fn push_after_close_is_still_drained() {
    let q: ByteBudgetQueue<u8> = ByteBudgetQueue::with_budget(1024);
    q.push(1, 1);
    q.close();
    q.push(2, 1);
    assert_eq!(q.pop(), Some(1));
    assert_eq!(q.pop(), Some(2));
    assert_eq!(q.pop(), None);
}

#[test]
fn is_empty_answers_without_blocking_on_an_open_empty_queue() {
    let q: ByteBudgetQueue<Vec<u8>> = ByteBudgetQueue::new();
    let t = Instant::now();
    assert!(q.is_empty(), "fila recém-criada está vazia");
    assert_eq!(q.len(), 0);
    assert!(
        t.elapsed() < Duration::from_millis(50),
        "is_empty() não pode bloquear — é a alternativa a `pop()` para \
         afirmar que nada foi enfileirado"
    );

    q.push(vec![1], 1);
    assert!(!q.is_empty());
    assert_eq!(q.len(), 1, "is_empty()/len() não retiram nada da fila");
}
