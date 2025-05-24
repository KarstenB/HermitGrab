use thiserror::Error;

#[derive(Debug, Error)]
pub enum AtomicLinkError {
    #[error("Source does not exist: {0}")]
    SourceNotFound(String),
    #[error("Destination is an existing file: {0}")]
    DestinationExists(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}
