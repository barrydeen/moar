use thiserror::Error;

#[derive(Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Heed/LMDB error: {0}")]
    Heed(#[from] heed::Error),
    #[error("Serialization error: {0}")]
    Json(#[from] serde_json::Error),
    #[error("TOML serialization error: {0}")]
    Toml(#[from] toml::ser::Error),
    #[error("Nostr error: {0}")]
    Nostr(#[from] nostr::types::url::ParseError), // approximate placeholder
}

pub type Result<T> = std::result::Result<T, Error>;
