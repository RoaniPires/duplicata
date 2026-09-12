use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use duplicata_core::config::{LOG_FILE_KEEP, LOG_FILE_MAX_BYTES};
use duplicata_core::InitError;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::filter::LevelFilter;

pub struct SizeRotatingWriter {
    dir: PathBuf,
    base: String,
    max_bytes: u64,
    keep: usize,
    file: File,
    written: u64,
}

impl SizeRotatingWriter {
    pub fn production(dir: impl Into<PathBuf>) -> io::Result<Self> {
        Self::new(dir, "duplicata.log", LOG_FILE_MAX_BYTES, LOG_FILE_KEEP)
    }

    pub fn new(
        dir: impl Into<PathBuf>,
        base: impl Into<String>,
        max_bytes: u64,
        keep: usize,
    ) -> io::Result<Self> {
        let dir = dir.into();
        let base = base.into();
        fs::create_dir_all(&dir)?;
        let (file, written) = open_append(&dir.join(&base))?;
        Ok(SizeRotatingWriter {
            dir,
            base,
            max_bytes,
            keep: keep.max(1),
            file,
            written,
        })
    }

    fn path_for(&self, index: usize) -> PathBuf {
        if index == 0 {
            self.dir.join(&self.base)
        } else {
            self.dir.join(format!("{}.{index}", self.base))
        }
    }

    fn rotate(&mut self) -> io::Result<()> {
        let _ = fs::remove_file(self.path_for(self.keep - 1));
        for i in (1..self.keep - 1).rev() {
            let from = self.path_for(i);
            if from.exists() {
                fs::rename(&from, self.path_for(i + 1))?;
            }
        }
        let base_path = self.path_for(0);
        if base_path.exists() {
            fs::rename(&base_path, self.path_for(1))?;
        }
        let (file, _) = open_append(&base_path)?;
        self.file = file;
        self.written = 0;
        Ok(())
    }
}

fn open_append(path: &Path) -> io::Result<(File, u64)> {
    let file = OpenOptions::new().create(true).append(true).open(path)?;
    let len = file.metadata()?.len();
    Ok((file, len))
}

impl Write for SizeRotatingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.written > 0 && self.written + buf.len() as u64 > self.max_bytes {
            self.rotate()?;
        }
        let n = self.file.write(buf)?;
        self.written += n as u64;
        Ok(n)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

pub const fn compile_time_level() -> LevelFilter {
    if cfg!(debug_assertions) {
        LevelFilter::DEBUG
    } else {
        LevelFilter::WARN
    }
}

pub fn init(log_dir: &Path) -> Result<WorkerGuard, InitError> {
    let writer = SizeRotatingWriter::production(log_dir).map_err(|_| InitError::Logging)?;
    let (non_blocking, guard) = tracing_appender::non_blocking(writer);

    tracing_subscriber::fmt()
        .with_ansi(false)
        .with_target(true)
        .with_max_level(compile_time_level())
        .with_writer(non_blocking)
        .try_init()
        .map_err(|_| InitError::Logging)?;

    Ok(guard)
}
