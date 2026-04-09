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

pub fn user_facing_error_message(error: &WorkerError) -> String {
    match error {
        WorkerError::Api { status, .. } => user_facing_status_message(*status),
        WorkerError::Http(error) => user_facing_reqwest_error_message(error),
        WorkerError::Json(_) | WorkerError::ResponseDecode(_) => "Invalid response".to_string(),
        WorkerError::Auth(_) => "Invalid API key".to_string(),
        WorkerError::Config(_) => "Invalid API URL".to_string(),
        _ => "Request failed".to_string(),
    }
}

pub fn user_facing_reqwest_error_message(error: &reqwest::Error) -> String {
    if let Some(status) = error.status() {
        return user_facing_status_message(status.as_u16());
    }
    if error.is_builder() {
        return "Invalid API URL".to_string();
    }
    if error.is_timeout() {
        return "Request timeout".to_string();
    }
    if error.is_connect() || error.is_request() {
        return "Backend offline".to_string();
    }
    if error.is_decode() {
        return "Invalid response".to_string();
    }
    "Request failed".to_string()
}

pub fn is_auth_error(error: &WorkerError) -> bool {
    matches!(
        error,
        WorkerError::Api {
            status: 401 | 403,
            ..
        } | WorkerError::Auth(_)
    )
}

fn user_facing_status_message(status: u16) -> String {
    match status {
        401 | 403 => "Invalid API key".to_string(),
        404 => "Invalid API URL".to_string(),
        408 | 504 => "Request timeout".to_string(),
        500..=599 => "Backend unavailable".to_string(),
        _ => "Request failed".to_string(),
    }
}
