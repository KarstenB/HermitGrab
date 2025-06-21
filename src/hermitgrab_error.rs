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
    DeserializeTomlError(toml::de::Error, PathBuf),
    #[error("An error occurred while serializing the configuration file {1}: {0}")]
    SerializeTomlError(toml::ser::Error, PathBuf),
    #[error("Duplicate profile found: {0} in file {1}")]
    DuplicateProfile(String, PathBuf),
}

#[derive(Debug, Error)]
pub enum AddError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    ConfigLoadError(#[from] ConfigLoadError),
    #[error("Failed to determine home directory")]
    NoHomeDir,
    #[error("Invalid choice")]
    InvalidChoice,
}

#[derive(Debug, Error)]
pub enum ApplyError {
    #[error("Profile not found: {0}")]
    ProfileNotFound(String),
    #[error("Install source not found: {0}")]
    InstallSourceNotFound(String),
    #[error("The user aborted the operation")]
    UserAborted,
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Failed to find tag: {0}")]
    TagNotFound(String),
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error(transparent)]
    LinkActionError(#[from] LinkActionError),
    #[error(transparent)]
    InstallActionError(#[from] InstallActionError),
}

#[derive(Debug, Error)]
pub enum LinkActionError {
    #[error("Failed to create parent directory for destination {1} due to IO error: {0}")]
    CreateParentDir(std::io::Error, PathBuf),
    #[error(transparent)]
    AtomicLinkError(#[from] AtomicLinkError),
}

#[derive(Debug, Error)]
pub enum InstallActionError {
    #[error(transparent)]
    RenderError(#[from] handlebars::RenderError),
    #[error("Install command failed: {0} with exit code {1}")]
    CommandFailed(String, i32),
    #[error("Failed to launch install command: {0} due to IO error: {1}")]
    CommandFailedLaunch(String, std::io::Error),
    #[error("Failed to launch pre-command: {0} due to IO error: {1}")]
    PreCommandFailedLaunch(String, std::io::Error),
    #[error("Failed to launch post-command: {0} due to IO error: {1}")]
    PostCommandFailedLaunch(String, std::io::Error),
}

#[derive(Debug, Error)]
pub enum DiscoverError {
    #[error(transparent)]
    GitError(#[from] git2::Error),
    #[error(transparent)]
    OctocrabError(#[from] octocrab::Error),
    #[error("No Git clone URL in Github response for repository: {0}")]
    NoGitCloneUrl(String),
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Repository already exists at path: {0}")]
    RepoAlreadyExists(std::path::PathBuf),
}
