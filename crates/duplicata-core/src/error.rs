#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    Busy,
    Unavailable,
    Empty,
    TooLarge { byte_len: u64 },
}

impl core::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            CaptureError::Busy => write!(f, "área de transferência ocupada"),
            CaptureError::Unavailable => write!(f, "área de transferência indisponível"),
            CaptureError::Empty => write!(f, "área de transferência vazia"),
            CaptureError::TooLarge { byte_len } => {
                write!(f, "captura acima do limite ({byte_len} bytes)")
            }
        }
    }
}

impl std::error::Error for CaptureError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkerError {
    NoCanonicalFormat,
    Store(StoreError),
}

impl core::fmt::Display for WorkerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            WorkerError::NoCanonicalFormat => write!(f, "captura sem formato canônico"),
            WorkerError::Store(e) => write!(f, "falha de persistência: {e}"),
        }
    }
}

impl std::error::Error for WorkerError {}

impl From<StoreError> for WorkerError {
    fn from(e: StoreError) -> Self {
        WorkerError::Store(e)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StoreError {
    Corrupted,
    Io,
    Migration,
    Query,
}

impl core::fmt::Display for StoreError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            StoreError::Corrupted => write!(f, "banco corrompido"),
            StoreError::Io => write!(f, "falha de I/O no banco"),
            StoreError::Migration => write!(f, "falha ao migrar o banco"),
            StoreError::Query => write!(f, "falha de consulta no banco"),
        }
    }
}

impl std::error::Error for StoreError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InitError {
    Paths,
    CreateDir,
    Logging,
    OpenDb(StoreError),
    Listener,
    Tray,
    HistoryWindow,
}

impl core::fmt::Display for InitError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            InitError::Paths => write!(f, "não foi possível resolver %LocalAppData%"),
            InitError::CreateDir => write!(f, "não foi possível criar o diretório de dados"),
            InitError::Logging => write!(f, "não foi possível inicializar o log"),
            InitError::OpenDb(e) => write!(f, "não foi possível abrir o banco: {e}"),
            InitError::Listener => write!(f, "não foi possível registrar o listener de clipboard"),
            InitError::Tray => write!(f, "não foi possível criar o ícone de bandeja"),
            InitError::HistoryWindow => write!(f, "não foi possível criar a janela de histórico"),
        }
    }
}

impl std::error::Error for InitError {}

impl From<StoreError> for InitError {
    fn from(e: StoreError) -> Self {
        InitError::OpenDb(e)
    }
}
