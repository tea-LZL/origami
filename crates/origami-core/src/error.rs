use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("backend error: {0}")]
    Backend(String),

    #[error("config error: {0}")]
    Config(String),

    #[error("secret error: {0}")]
    Secret(String),

    #[error("account not found: {0}")]
    AccountNotFound(String),

    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
