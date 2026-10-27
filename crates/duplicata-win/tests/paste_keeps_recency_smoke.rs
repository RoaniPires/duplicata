#![cfg(windows)]

//! Colar pelo histórico não conta como "cópia mais recente": a escrita do
//! próprio duplicata na área de transferência passa pelo `SelfWriteFilter`, o
//! ouvinte não a captura e a recência do item colado não muda (tarefa 01,
//! regra 9).

use std::rc::Rc;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use duplicata_core::testsupport::unicode_text_format;
use duplicata_core::{
    run_worker, ByteBudgetQueue, CaptureQueue, CaptureRecord, Config, FakeHistoryRepository,
    HistoryReader, HistoryRepository, SelfWriteFilter, SetPinnedOutcome, StoreError, UpsertOutcome,
    WorkItem, WorkerCounters,
};
use duplicata_win::clipboard_restore;
use duplicata_win::listener::ClipboardListener;
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, PeekMessageW, TranslateMessage, MSG, PM_REMOVE, WM_CLIPBOARDUPDATE,
};

/// Uma gravação que o worker fez: a prévia e o que o repositório respondeu.
type Upserts = Arc<Mutex<Vec<(Option<String>, UpsertOutcome)>>>;

struct RecordingRepo {
    inner: Arc<Mutex<FakeHistoryRepository>>,
    upserts: Upserts,
}

impl HistoryRepository for RecordingRepo {
    fn upsert(&mut self, record: &CaptureRecord) -> Result<UpsertOutcome, StoreError> {
        let outcome = self.inner.lock().unwrap().upsert(record)?;
        self.upserts
            .lock()
            .unwrap()
            .push((record.preview.clone(), outcome));
        Ok(outcome)
    }
    fn purge_older_than(&mut self, cutoff_ms: u64) -> Result<u64, StoreError> {
        self.inner.lock().unwrap().purge_older_than(cutoff_ms)
    }
    fn purge_over_count(&mut self, max_items: u32) -> Result<u64, StoreError> {
        self.inner.lock().unwrap().purge_over_count(max_items)
    }
    fn set_pinned(
        &mut self,
        clip_id: i64,
        pinned: bool,
        max_pinned: u32,
    ) -> Result<SetPinnedOutcome, StoreError> {
        self.inner
            .lock()
            .unwrap()
            .set_pinned(clip_id, pinned, max_pinned)
    }
    fn delete_all(&mut self, keep_pinned: bool) -> Result<u64, StoreError> {
        self.inner.lock().unwrap().delete_all(keep_pinned)
    }
    fn delete_one(&mut self, clip_id: i64) -> Result<bool, StoreError> {
        self.inner.lock().unwrap().delete_one(clip_id)
    }
    fn recreate(&mut self) -> Result<(), StoreError> {
        self.inner.lock().unwrap().recreate()
    }
    fn toggle_encryption(
        &mut self,
        enable: bool,
        progress: &dyn Fn(u64),
    ) -> Result<(), StoreError> {
        self.inner
            .lock()
            .unwrap()
            .toggle_encryption(enable, progress)
    }
}

fn formats(text: &str) -> Vec<(u32, Option<String>, Vec<u8>)> {
    let f = unicode_text_format(text);
    vec![(f.format_id, f.format_name, f.bytes)]
}

/// Entrega uma mensagem pendente desta thread; diz se era um aviso de
/// mudança da área de transferência.
fn pump_one() -> Option<bool> {
    let mut msg = MSG::default();
    // SAFETY: fila desta thread; `msg` é local.
    let got = unsafe { PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE) }.as_bool();
    if !got {
        return None;
    }
    // SAFETY: `msg` acabou de ser lida da fila desta thread.
    unsafe {
        let _ = TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    Some(msg.message == WM_CLIPBOARDUPDATE)
}

/// Entrega mensagens até `done()` valer, com prazo.
fn pump_until(what: &str, mut done: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !done() {
        assert!(Instant::now() < deadline, "prazo esgotado esperando {what}");
        if pump_one().is_none() {
            thread::yield_now();
        }
    }
}

#[test]
#[ignore = "precisa de sessão gráfica com clipboard; sobrescreve o clipboard atual"]
fn pasting_from_the_history_does_not_raise_the_recency_of_the_pasted_item() {
    let inner = Arc::new(Mutex::new(FakeHistoryRepository::new()));
    let upserts: Upserts = Arc::new(Mutex::new(Vec::new()));
    let queue: Arc<dyn CaptureQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());

    let worker_queue = Arc::clone(&queue);
    let mut worker_repo = RecordingRepo {
        inner: Arc::clone(&inner),
        upserts: Arc::clone(&upserts),
    };
    let worker = thread::spawn(move || {
        let (heuristic_tx, _heuristic_rx) = std::sync::mpsc::channel();
        run_worker(
            worker_queue.as_ref(),
            &mut worker_repo,
            &Config::with_paths("db".into(), "logs".into()),
            &WorkerCounters::default(),
            &heuristic_tx,
        );
    });

    let filter = Rc::new(SelfWriteFilter::new());
    let listener = ClipboardListener::new(
        Arc::clone(&queue),
        Config::with_paths("db".into(), "logs".into()),
        Rc::clone(&filter),
    )
    .expect("criar o ouvinte da área de transferência");
    // Cópia feita em outro programa: um filtro que o ouvinte não conhece.
    let elsewhere = SelfWriteFilter::new();
    let recorded = |n: usize| upserts.lock().unwrap().len() >= n;

    // 1. Copia A e depois B fora do app (B tira A da janela de dedupe).
    assert!(clipboard_restore::write_all(&elsewhere, &formats("item A")));
    pump_until("a captura de A", || recorded(1));
    assert!(clipboard_restore::write_all(&elsewhere, &formats("item B")));
    pump_until("a captura de B", || recorded(2));
    let activity_of = |preview: &str| {
        inner
            .lock()
            .unwrap()
            .list_for_display(10)
            .expect("ler a lista")
            .into_iter()
            .find(|c| c.preview.as_deref() == Some(preview))
            .expect("item gravado")
            .last_activity_ms
    };
    let a_before = activity_of("item A");

    // 2. Cola A pelo histórico: a mesma escrita que `confirm` faz.
    assert!(clipboard_restore::write_all(&filter, &formats("item A")));
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(
            Instant::now() < deadline,
            "prazo esgotado esperando o aviso da colagem"
        );
        match pump_one() {
            Some(true) => break,
            Some(false) => {}
            None => thread::yield_now(),
        }
    }

    // 3. Uma cópia de fora depois: quando ela chega ao repositório, o aviso
    //    da colagem já foi tratado (o worker grava na ordem da fila).
    assert!(clipboard_restore::write_all(&elsewhere, &formats("item X")));
    pump_until("a captura de X", || recorded(3));

    queue.close();
    worker.join().expect("o worker termina no close da fila");
    drop(listener);

    let previews: Vec<Option<String>> = upserts
        .lock()
        .unwrap()
        .iter()
        .map(|(p, _)| p.clone())
        .collect();
    assert_eq!(
        previews,
        vec![
            Some("item A".to_string()),
            Some("item B".to_string()),
            Some("item X".to_string())
        ],
        "a colagem de A não foi gravada como cópia"
    );
    assert_eq!(activity_of("item A"), a_before, "a recência de A não mudou");
}
