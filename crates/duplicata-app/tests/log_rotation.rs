use std::io::Write;

use duplicata_app::logging::SizeRotatingWriter;

#[test]
fn rotates_by_size_and_keeps_at_most_three_files() {
    let dir = tempfile::tempdir().unwrap();
    let max = 1024u64;
    let keep = 3;
    let mut w = SizeRotatingWriter::new(dir.path(), "duplicata.log", max, keep).unwrap();

    let line = [b'x'; 100];
    for _ in 0..100 {
        w.write_all(&line).unwrap();
    }
    w.flush().unwrap();
    drop(w);

    let mut names: Vec<String> = std::fs::read_dir(dir.path())
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();

    assert_eq!(
        names,
        vec![
            "duplicata.log".to_string(),
            "duplicata.log.1".to_string(),
            "duplicata.log.2".to_string(),
        ],
        "no máximo 3 arquivos: base + .1 + .2"
    );

    for name in &names {
        let len = std::fs::metadata(dir.path().join(name)).unwrap().len();
        assert!(len <= max + line.len() as u64, "{name} = {len} bytes");
    }
}

#[test]
fn does_not_rotate_below_the_threshold() {
    let dir = tempfile::tempdir().unwrap();
    let mut w = SizeRotatingWriter::new(dir.path(), "duplicata.log", 10_000, 3).unwrap();
    w.write_all(&[b'a'; 500]).unwrap();
    w.flush().unwrap();
    drop(w);

    let count = std::fs::read_dir(dir.path()).unwrap().count();
    assert_eq!(count, 1);
}

#[test]
fn reopening_appends_to_the_existing_file() {
    let dir = tempfile::tempdir().unwrap();
    {
        let mut w = SizeRotatingWriter::new(dir.path(), "duplicata.log", 10_000, 3).unwrap();
        w.write_all(b"primeira\n").unwrap();
        w.flush().unwrap();
    }
    {
        let mut w = SizeRotatingWriter::new(dir.path(), "duplicata.log", 10_000, 3).unwrap();
        w.write_all(b"segunda\n").unwrap();
        w.flush().unwrap();
    }
    let content = std::fs::read_to_string(dir.path().join("duplicata.log")).unwrap();
    assert_eq!(content, "primeira\nsegunda\n");
}
