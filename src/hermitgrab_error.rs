use std::path::{PathBuf, StripPrefixError};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HermitGrabError {
    #[error(transparent)]
    AtomicLinkError(#[from] FileOpsError),
    #[error(transparent)]
    ConfigLoadError(#[from] ConfigLoadError),
    #[error(transparent)]
    ApplyError(#[from] ApplyError),
}

#[derive(Debug, Error)]
pub enum FileOpsError {
    #[error("Source does not exist: {0}")]
    SourceNotFound(String),
    #[error("Destination is an existing file: {0}")]
    DestinationExists(String),
    #[error("{0}, IO error: {1}")]
    Io(PathBuf, std::io::Error),
    #[error("Failed to find a backup file name for {0}")]
    BackupAlreadyExists(String),
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
    #[error("Failed to deserialize document in TOML format: {0} in file {1}")]
    DeserializeDocumentTomlError(toml_edit::TomlError, PathBuf),
    #[error(transparent)]
    RenderError(#[from] handlebars::RenderError),
    #[error("Failed to find source: {0}")]
    InstallSourceNotFound(String),
}

#[derive(Debug, Error)]
pub enum StatusError {}

#[derive(Debug, Error)]
pub enum PatchActionError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    SerdeJsonError(#[from] serde_json::Error),
    #[error(transparent)]
    PatchError(#[from] json_patch::PatchError),
    #[error(transparent)]
    YamlParseError(#[from] serde_yml::Error),
    #[error(transparent)]
    TomlDeserializeError(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSerializeError(#[from] toml::ser::Error),
    #[error(transparent)]
    SerdecParseError(#[from] jsonc_parser::errors::ParseError),
}

#[derive(Debug, Error)]
pub enum AddError {
    #[error(transparent)]
    IoError(#[from] std::io::Error),
    #[error(transparent)]
    FileOpsError(#[from] FileOpsError),
    #[error(transparent)]
    ConfigLoadError(#[from] ConfigLoadError),
    #[error("Failed to determine home directory")]
    NoHomeDir,
    #[error("Invalid choice")]
    InvalidChoice,
    #[error("Failed to locate source: {0}")]
    SourceNotFound(PathBuf),
    #[error("Expected a table at key {0}, but found {1}")]
    ExpectedTable(String, String),
    #[error("Expected an array at key {0}, but found {1}")]
    ExpectedArray(String, String),
    #[error("Expected a string at key {0}, but found {1}")]
    ExpectedString(String, String),
    #[error(transparent)]
    TomlSerializationError(#[from] toml::ser::Error),
    #[error(transparent)]
    TomlDeserializationError(#[from] toml::de::Error),
    #[error(transparent)]
    TomlEditSerializationError(#[from] toml_edit::ser::Error),
    #[error(transparent)]
    TomlEditDeserializationError(#[from] toml_edit::de::Error),
    #[error("Internal conversion error")]
    TomlConversion,
    #[error("Failed to get filename")]
    FileNameError,
    #[error("Failed to strip prefix")]
    StripPrefixError(#[from] StripPrefixError),
    #[error("A source with the file {0} already exists")]
    SourceAlreadyExists(PathBuf),
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
    #[error(transparent)]
    ConfigLoadError(#[from] ConfigLoadError),
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error(transparent)]
    LinkActionError(#[from] LinkActionError),
    #[error(transparent)]
    InstallActionError(#[from] InstallActionError),
    #[error(transparent)]
    PatchActionError(#[from] PatchActionError),
}

#[derive(Debug, Error)]
pub enum LinkActionError {
    #[error("Failed to create parent directory for destination {1} due to IO error: {0}")]
    CreateParentDir(std::io::Error, PathBuf),
    #[error(transparent)]
    AtomicLinkError(#[from] FileOpsError),
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
