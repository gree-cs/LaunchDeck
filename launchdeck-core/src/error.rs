use thiserror::Error;

#[derive(Error, Debug)]
pub enum LaunchDeckError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Scan error: {0}")]
    Scan(String),

    #[error("Config error: {0}")]
    Config(String),

    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),

    #[error("Crypto error: {0}")]
    Crypto(String),

    #[error("Not found: {0}")]
    NotFound(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Unknown: {0}")]
    Unknown(String),
}

pub type Result<T> = std::result::Result<T, LaunchDeckError>;
