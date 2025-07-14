use std::path::{PathBuf, StripPrefixError};

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HermitGrabError {
    #[error(transparent)]
    FileOps(#[from] FileOpsError),
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Apply(#[from] ApplyError),
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
pub enum ConfigError {
    #[error("Redeclaration of source: {0} found in file {1}")]
    DuplicateSource(String, PathBuf),
    #[error("An error occurred while handling the configuration file {1}: {0}")]
    Io(std::io::Error, PathBuf),
    #[error("An error occurred while parsing the configuration file {1}: {0}")]
    DeserializeToml(toml::de::Error, PathBuf),
    #[error("An error occurred while serializing the configuration file {1}: {0}")]
    SerializeToml(toml::ser::Error, PathBuf),
    #[error("Duplicate profile found: {0} in file {1}")]
    DuplicateProfile(String, PathBuf),
    #[error("Failed to deserialize document in TOML format: {0} in file {1}")]
    DeserializeDocumentToml(toml_edit::TomlError, PathBuf),
    #[error(transparent)]
    Render(#[from] handlebars::RenderError),
    #[error("Failed to find source: {0}")]
    InstallSourceNotFound(String),
    #[error("Hermit configuration is not an action")]
    HermitConfigNotAction,
    #[error("The tag {0} was not found in the configuration")]
    TagNotFound(String),
}

#[derive(Debug, Error)]
pub enum StatusError {
    #[error(transparent)]
    Config(#[from] ConfigError),
    #[error(transparent)]
    Apply(#[from] ApplyError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    StdIo(#[from] std::io::Error),
}

#[derive(Debug, Error)]
pub enum PatchActionError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
    #[error(transparent)]
    Patch(#[from] json_patch::PatchError),
    #[error(transparent)]
    YamlParse(#[from] serde_yml::Error),
    #[error(transparent)]
    TomlDeserialize(#[from] toml::de::Error),
    #[error(transparent)]
    TomlSerialize(#[from] toml::ser::Error),
    #[error(transparent)]
    SerdecParse(#[from] jsonc_parser::errors::ParseError),
}

#[derive(Debug, Error)]
pub enum AddError {
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error(transparent)]
    FileOps(#[from] FileOpsError),
    #[error(transparent)]
    ConfigLoad(#[from] ConfigError),
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
    TomlSerialization(#[from] toml::ser::Error),
    #[error(transparent)]
    TomlDeserialization(#[from] toml::de::Error),
    #[error(transparent)]
    TomlEditSerialization(#[from] toml_edit::ser::Error),
    #[error(transparent)]
    TomlEditDeserialization(#[from] toml_edit::de::Error),
    #[error("Failed to get filename")]
    FileName,
    #[error("Failed to strip prefix")]
    StripPrefix(#[from] StripPrefixError),
    #[error("A source with the file {0} already exists")]
    SourceAlreadyExists(PathBuf),
    #[error("The configuration file {0} already exists")]
    ConfigFileAlreadyExists(PathBuf),
    #[error("The configuration file {0} does not exist")]
    ConfigFileNotFound(PathBuf),
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
    Io(#[from] std::io::Error),
    #[error("Failed to find tag: {0}")]
    TagNotFound(String),
    #[error(transparent)]
    ConfigLoad(#[from] ConfigError),
    #[error(transparent)]
    SerdeJson(#[from] serde_json::Error),
}

#[derive(Debug, Error)]
pub enum ActionError {
    #[error(transparent)]
    Link(#[from] LinkActionError),
    #[error(transparent)]
    Install(#[from] InstallActionError),
    #[error(transparent)]
    Patch(#[from] PatchActionError),
}

#[derive(Debug, Error)]
pub enum LinkActionError {
    #[error("Failed to create parent directory for destination {1} due to IO error: {0}")]
    CreateParentDir(std::io::Error, PathBuf),
    #[error(transparent)]
    FileOps(#[from] FileOpsError),
}

#[derive(Debug, Error)]
pub enum InstallActionError {
    #[error(transparent)]
    Render(#[from] handlebars::RenderError),
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
    Git(#[from] git2::Error),
    #[error(transparent)]
    Octocrab(#[from] octocrab::Error),
    #[error("No Git clone URL in Github response for repository: {0}")]
    NoGitCloneUrl(String),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("Invalid input: {0}")]
    InvalidInput(String),
    #[error("Repository already exists at path: {0}")]
    RepoAlreadyExists(std::path::PathBuf),
}
