use std::sync::mpsc;
use std::sync::Arc;
use std::thread;

use duplicata_core::{
    run_worker, ByteBudgetQueue, CaptureQueue, Config, FailingHistoryRepository,
    FakeHistoryRepository, StoreError, WorkItem, WorkerCounters,
};

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

#[test]
fn recreate_database_via_run_worker_clears_the_repo_and_answers_done_without_counting_a_capture() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FakeHistoryRepository::new();
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
        repo
    });

    let (done_tx, done_rx) = mpsc::channel();
    queue.push(WorkItem::RecreateDatabase { done: done_tx }, 0);
    queue.close();

    let result = done_rx
        .recv()
        .expect("run_worker deve responder no canal done");
    assert_eq!(result, Ok(()));

    let repo = worker.join().unwrap();
    assert!(repo.is_empty(), "recreate() deve limpar o repositório");
    assert_eq!(
        counters.processed(),
        0,
        "RecreateDatabase não é uma captura — não deve incrementar processed"
    );
}

#[test]
fn recreate_database_via_run_worker_propagates_a_failing_recreate_through_done() {
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let mut repo = FailingHistoryRepository::new(StoreError::Io);
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
    });

    let (done_tx, done_rx) = mpsc::channel();
    queue.push(WorkItem::RecreateDatabase { done: done_tx }, 0);
    queue.close();

    let result = done_rx
        .recv()
        .expect("run_worker deve responder no canal done mesmo quando recreate() falha");
    assert_eq!(
        result,
        Err(StoreError::Io),
        "a falha de recreate() deve chegar a quem pediu, nunca engolida"
    );

    worker
        .join()
        .expect("worker não deve travar/entrar em pânico com uma recriação que falha");
}
