use std::sync::{mpsc, Arc};
use std::thread;

use duplicata_core::backoff::BackoffPolicy;
use duplicata_core::canonical::{CF_DIB, CF_HDROP, CF_UNICODETEXT};
use duplicata_core::{
    capture_and_enqueue, run_worker, utf16le, ByteBudgetQueue, CaptureQueue, CapturedFormat,
    Config, FakeClipboardSource, FakeClock, RecentCaptureGuard, WorkItem, WorkerCounters,
};
use duplicata_store::{open, SqliteHistoryRepository};

fn f(id: u32, bytes: Vec<u8>) -> CapturedFormat {
    CapturedFormat {
        format_id: id,
        format_name: None,
        bytes,
    }
}

fn named(id: u32, name: &str, bytes: Vec<u8>) -> CapturedFormat {
    CapturedFormat {
        format_id: id,
        format_name: Some(name.into()),
        bytes,
    }
}

fn cfg() -> Config {
    Config::with_paths("db".into(), "logs".into())
}

fn synthetic_dib_24bpp(width: i32, height: i32) -> Vec<u8> {
    let row_unpadded = width as usize * 3;
    let row_bytes = row_unpadded.div_ceil(4) * 4;
    let pixel_data_size = row_bytes * height as usize;

    let mut dib = Vec::with_capacity(40 + pixel_data_size);
    dib.extend_from_slice(&40u32.to_le_bytes());
    dib.extend_from_slice(&width.to_le_bytes());
    dib.extend_from_slice(&height.to_le_bytes());
    dib.extend_from_slice(&1u16.to_le_bytes());
    dib.extend_from_slice(&24u16.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&(pixel_data_size as u32).to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0i32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend_from_slice(&0u32.to_le_bytes());
    dib.extend((0..pixel_data_size).map(|i| (i % 251) as u8));
    dib
}

fn dropfiles_wide(paths: &[&str]) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&20u32.to_le_bytes());
    out.extend_from_slice(&[0u8; 8]);
    out.extend_from_slice(&0i32.to_le_bytes());
    out.extend_from_slice(&1i32.to_le_bytes());
    for p in paths {
        for u in p.encode_utf16() {
            out.extend_from_slice(&u.to_le_bytes());
        }
        out.extend_from_slice(&[0, 0]);
    }
    out.extend_from_slice(&[0, 0]);
    out
}

#[test]
fn one_copy_of_each_type_lands_as_a_row_with_all_formats_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let conn = open(&db).unwrap();
        let mut repo = SqliteHistoryRepository::new(conn);
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(
            q.as_ref(),
            &mut repo,
            &Config::with_paths("x".into(), "y".into()),
            c.as_ref(),
            &heuristic_tx,
        );
        repo
    });

    let policy = BackoffPolicy::production();
    let clock = FakeClock::starting_at(1_000);

    let text = FakeClipboardSource::always(vec![f(CF_UNICODETEXT, utf16le("só texto"))]);
    capture_and_enqueue(
        &text,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    let web = FakeClipboardSource::always(vec![
        f(CF_UNICODETEXT, utf16le("trecho web")),
        named(0xC000, "HTML Format", b"<p>trecho web</p>".to_vec()),
    ]);
    capture_and_enqueue(
        &web,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    let img = FakeClipboardSource::always(vec![f(CF_DIB, vec![7u8; 4096])]);
    capture_and_enqueue(
        &img,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    let files = FakeClipboardSource::always(vec![f(
        CF_HDROP,
        dropfiles_wide(&["C:\\a.txt", "C:\\b.txt"]),
    )]);
    capture_and_enqueue(
        &files,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    queue.close();
    let repo = worker.join().unwrap();
    let conn = repo.connection();

    assert_eq!(counters.processed(), 4);
    let clips: i64 = conn
        .query_row("SELECT count(*) FROM clip", [], |r| r.get(0))
        .unwrap();
    assert_eq!(clips, 4);

    let web_id: i64 = conn
        .query_row(
            "SELECT id FROM clip WHERE canonical_kind = 'unicode_text' AND preview = 'trecho web'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let html: Vec<u8> = conn
        .query_row(
            "SELECT bytes FROM clip_format WHERE clip_id = ?1 AND format_id = 49152",
            [web_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(html, b"<p>trecho web</p>");
    let canon: Vec<u8> = conn
        .query_row(
            "SELECT bytes FROM clip_format WHERE clip_id = ?1 AND is_canonical = 1",
            [web_id],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(canon, utf16le("trecho web"));

    let (kind, preview_null, thumb_null): (String, i64, i64) = conn
        .query_row(
            "SELECT canonical_kind, preview IS NULL, thumbnail IS NULL FROM clip
             WHERE canonical_kind = 'dib'",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(kind, "dib");
    assert_eq!(preview_null, 1);
    assert_eq!(thumb_null, 1);

    let hdrop_preview: String = conn
        .query_row(
            "SELECT preview FROM clip WHERE canonical_kind = 'hdrop'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(hdrop_preview.contains("a.txt") && hdrop_preview.contains("b.txt"));
}

#[test]
fn real_image_capture_gets_a_decodable_thumbnail_persisted_end_to_end() {
    let dir = tempfile::tempdir().unwrap();
    let db = dir.path().join("duplicata.db");
    let queue: Arc<ByteBudgetQueue<WorkItem>> = Arc::new(ByteBudgetQueue::new());
    let counters = Arc::new(WorkerCounters::new());

    let q = Arc::clone(&queue);
    let c = Arc::clone(&counters);
    let worker = thread::spawn(move || {
        let conn = open(&db).unwrap();
        let mut repo = SqliteHistoryRepository::new(conn);
        let (heuristic_tx, _heuristic_rx) = mpsc::channel();
        run_worker(q.as_ref(), &mut repo, &cfg(), c.as_ref(), &heuristic_tx);
        repo
    });

    let policy = BackoffPolicy::production();
    let clock = FakeClock::starting_at(1_000);
    let dib = synthetic_dib_24bpp(300, 200);
    let src = FakeClipboardSource::always(vec![f(CF_DIB, dib)]);
    capture_and_enqueue(
        &src,
        &clock,
        &policy,
        &cfg(),
        queue.as_ref(),
        &RecentCaptureGuard::new(),
    );

    queue.close();
    let repo = worker.join().unwrap();
    let conn = repo.connection();

    assert_eq!(counters.processed(), 1);
    let thumbnail: Option<Vec<u8>> = conn
        .query_row(
            "SELECT thumbnail FROM clip WHERE canonical_kind = 'dib'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    let thumbnail = thumbnail.expect("DIB válido deve gerar e persistir uma miniatura");

    let decoded = image::load_from_memory_with_format(&thumbnail, image::ImageFormat::Png)
        .expect("thumbnail persistido deve ser um PNG válido e decodificável");
    assert!(decoded.width().max(decoded.height()) <= 128);
}
