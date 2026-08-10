use serde::Serialize;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Operation cancelled.")]
    Cancelled,
    #[error("{0}")]
    Validation(String),
    #[error("{0}")]
    NotFound(String),
    #[error("{0}")]
    Conflict(String),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Database(#[from] sqlx::Error),
    #[error("{0}")]
    Json(#[from] serde_json::Error),
    #[error("{0}")]
    Network(#[from] reqwest::Error),
    #[error("{0}")]
    Archive(String),
    #[error("{0}")]
    Process(String),
    #[error("{0}")]
    Unsupported(String),
    #[error("{0}")]
    Internal(String),
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AppError {
    pub code: &'static str,
    pub message: String,
    pub detail: Option<String>,
    pub field: Option<String>,
    pub recoverable: bool,
}

impl From<Error> for AppError {
    fn from(value: Error) -> Self {
        let (code, recoverable) = match &value {
            Error::Cancelled => ("cancelled", true),
            Error::Validation(_) => ("validation", true),
            Error::NotFound(_) => ("not_found", true),
            Error::Conflict(_) => ("conflict", true),
            Error::Io(_) => ("io", true),
            Error::Database(_) => ("database", false),
            Error::Json(_) => ("invalid_data", true),
            Error::Network(_) => ("network", true),
            Error::Archive(_) => ("archive", true),
            Error::Process(_) => ("process", true),
            Error::Unsupported(_) => ("unsupported", true),
            Error::Internal(_) => ("internal", false),
        };
        Self {
            code,
            message: value.to_string(),
            detail: None,
            field: None,
            recoverable,
        }
    }
}

impl From<zip::result::ZipError> for Error {
    fn from(value: zip::result::ZipError) -> Self {
        Self::Archive(value.to_string())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
pub type CommandResult<T> = std::result::Result<T, AppError>;
