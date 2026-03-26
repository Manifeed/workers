use thiserror::Error;

pub type Result<T> = std::result::Result<T, WorkerError>;

#[derive(Debug, Error)]
pub enum WorkerError {
    #[error("configuration error: {0}")]
    Config(String),
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    #[error("http error: {0}")]
    Http(#[from] reqwest::Error),
    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("api error ({status}): {message}")]
    Api { status: u16, message: String },
    #[error("auth error: {0}")]
    Auth(String),
    #[error("crypto error: {0}")]
    Crypto(String),
    #[error("process error: {0}")]
    Process(String),
    #[error("response decode error: {0}")]
    ResponseDecode(String),
    #[error("version error: {0}")]
    Version(String),
}
