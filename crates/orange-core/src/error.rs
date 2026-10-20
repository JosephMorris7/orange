use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("{0}")]
    Msg(String),
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),
    #[error("missing dependency: {0}")]
    MissingDep(String),
    #[error("unsupported: {0}")]
    Unsupported(String),
    #[error("{tool} failed: {detail}")]
    ToolFailed { tool: String, detail: String },
    #[error("cancelled")]
    Cancelled,
    #[error("invalid parameters: {0}")]
    BadParams(String),
    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;

impl Error {
    pub fn msg(s: impl Into<String>) -> Self {
        Error::Msg(s.into())
    }
}
