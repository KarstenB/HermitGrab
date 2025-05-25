use std::path::PathBuf;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HermitGrabError {
    #[error(transparent)]
    AtomicLinkError(#[from] AtomicLinkError),
    #[error(transparent)]
    ConfigLoadError(#[from] ConfigLoadError),
    #[error(transparent)]
    ApplyError(#[from] ApplyError),
}

#[derive(Debug, Error)]
pub enum AtomicLinkError {
    #[error("Source does not exist: {0}")]
    SourceNotFound(String),
    #[error("Destination is an existing file: {0}")]
    DestinationExists(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum ConfigLoadError {
    #[error("Redeclaration of source: {0} found in file {1}")]
    DuplicateSource(String, PathBuf),
    #[error("An error occurred while loading the configuration file {1}: {0}")]
    IoError(std::io::Error, PathBuf),
    #[error("An error occurred while parsing the configuration file {1}: {0}")]
    SerdeYmlError(serde_yml::Error, PathBuf),
    #[error("Duplicate profile found: {0} in file {1}")]
    DuplicateProfile(String, PathBuf),
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Install source not found: {0}")]
    InstallSourceNotFound(String),
}