/// Erro unico do core. O frontend so ve `kind` + `message`, nunca o erro
/// interno da biblioteca — isso mantem a UI desacoplada de qual crate esta
/// falando com o InnerTube.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    #[error("nao autenticado")]
    Unauthenticated,

    /// Cookies existem mas o YouTube os rejeitou: a UI mostra o banner de
    /// reconexao sem interromper a reproducao.
    #[error("sessao expirada")]
    SessionExpired,

    #[error("rede: {0}")]
    Network(String),

    /// A resposta chegou mas nao tinha o formato esperado. Quase sempre
    /// significa que o YouTube mudou alguma coisa.
    #[error("resposta inesperada do YouTube: {0}")]
    Parse(String),

    #[error("nao encontrado")]
    NotFound,

    #[error("{0}")]
    Other(String),
}

pub type Result<T> = std::result::Result<T, CoreError>;

/// Discriminante estavel para o frontend decidir o que mostrar.
/// Nao usar o texto de `Display` para isso — ele muda.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    Unauthenticated,
    SessionExpired,
    Network,
    Parse,
    NotFound,
    Other,
}

impl CoreError {
    pub fn kind(&self) -> ErrorKind {
        match self {
            Self::Unauthenticated => ErrorKind::Unauthenticated,
            Self::SessionExpired => ErrorKind::SessionExpired,
            Self::Network(_) => ErrorKind::Network,
            Self::Parse(_) => ErrorKind::Parse,
            Self::NotFound => ErrorKind::NotFound,
            Self::Other(_) => ErrorKind::Other,
        }
    }
}

/// Forma com que o erro cruza o IPC.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ErrorPayload {
    pub kind: ErrorKind,
    pub message: String,
}

impl From<CoreError> for ErrorPayload {
    fn from(e: CoreError) -> Self {
        Self { kind: e.kind(), message: e.to_string() }
    }
}
