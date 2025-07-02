use clap::ValueEnum;
use clap::builder::PossibleValue;
use handlebars::Handlebars;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::collections::HashMap;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::Weak;
use toml_edit::DocumentMut;

use crate::detector::detect_builtin_tags;
use crate::file_ops::check_copied;
use crate::hermitgrab_error::ApplyError;
use crate::hermitgrab_error::ConfigError;

pub const CONF_FILE_NAME: &str = "hermit.toml";
pub const DEFAULT_PROFILE: &str = "default";

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Source {
    Unknown,
    CommandLine,
    Detector(String),
    BuiltInDetector,
    Config,
}
impl Display for Source {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Source::Unknown => write!(f, "unknown"),
            Source::CommandLine => write!(f, "command line"),
            Source::Detector(name) => write!(f, "detector: {}", name),
            Source::BuiltInDetector => write!(f, "built-in detector"),
            Source::Config => write!(f, "config"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tag(String, Source);
impl Tag {
    pub fn new(tag: &str, source: Source) -> Self {
        Tag(tag.to_lowercase(), source)
    }

    pub fn name(&self) -> &str {
        &self.0
    }
    pub fn source(&self) -> &Source {
        &self.1
    }

    pub fn is_detected(&self) -> bool {
        matches!(self.1, Source::Detector(_) | Source::BuiltInDetector)
    }
}
impl Hash for Tag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
    }
}
impl PartialEq for Tag {
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}
impl Eq for Tag {}
impl PartialOrd for Tag {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}
impl Ord for Tag {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.0.cmp(&other.0)
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl FromStr for Tag {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if s.is_empty() {
            return Err("Tag cannot be empty".to_string());
        }
        Ok(Tag::new(s, Source::Unknown))
    }
}

impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Tag::new(&s, Source::Config))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
pub enum RequireTag {
    Positive(String),
    Negative(String),
}

impl RequireTag {
    pub fn matches(&self, tags: &BTreeSet<Tag>) -> bool {
        match self {
            RequireTag::Positive(tag) => tags.contains(&Tag::new(tag, Source::Unknown)),
            RequireTag::Negative(tag) => !tags.contains(&Tag::new(tag, Source::Unknown)),
        }
    }
}

impl FromStr for RequireTag {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if let Some(rest) = trimmed.strip_prefix('+') {
            Ok(RequireTag::Positive(rest.to_string()))
        } else if let Some(rest) = trimmed.strip_prefix('-') {
            Ok(RequireTag::Negative(rest.to_string()))
        } else if let Some(rest) = trimmed.strip_prefix('~') {
            Ok(RequireTag::Negative(rest.to_string()))
        } else {
            Ok(RequireTag::Positive(trimmed.to_string()))
        }
    }
}
impl Display for RequireTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequireTag::Positive(tag) => write!(f, "+{}", tag),
            RequireTag::Negative(tag) => write!(f, "-{}", tag),
        }
    }
}

impl Serialize for RequireTag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            RequireTag::Positive(tag) => format!("+{}", tag),
            RequireTag::Negative(tag) => format!("-{}", tag),
        };
        serializer.serialize_str(&s)
    }
}

impl<'de> Deserialize<'de> for RequireTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        RequireTag::from_str(&s).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct HermitConfig {
    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    global_cfg: Weak<GlobalConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub provides: BTreeSet<Tag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file: Vec<LinkConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patch: Vec<PatchConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<InstallConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub sources: BTreeMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, BTreeSet<Tag>>,
}

impl HermitConfig {
    pub fn create_new(path: &Path, global_cfg: Weak<GlobalConfig>) -> Self {
        HermitConfig {
            path: path.to_path_buf(),
            global_cfg,
            ..Default::default()
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub fn global_config(&self) -> Arc<GlobalConfig> {
        self.global_cfg
            .upgrade()
            .expect("Global config should be set before using HermitConfig")
    }

    pub fn save_to_file(&self, conf_file_name: &PathBuf) -> Result<(), ConfigError> {
        let content = toml::to_string(self)
            .map_err(|e| ConfigError::SerializeTomlError(e, conf_file_name.clone()))?;
        std::fs::write(conf_file_name, content)
            .map_err(|e| ConfigError::IoError(e, conf_file_name.clone()))?;
        Ok(())
    }

    pub fn directory(&self) -> &Path {
        self.path.parent().expect("Expected to get parent")
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy, Hash, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    #[default]
    Soft,
    Hard,
    Copy,
}

impl ValueEnum for LinkType {
    fn value_variants<'a>() -> &'a [Self] {
        &[LinkType::Soft, LinkType::Hard, LinkType::Copy]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            LinkType::Soft => Some(clap::builder::PossibleValue::new("soft")),
            LinkType::Hard => Some(clap::builder::PossibleValue::new("hard")),
            LinkType::Copy => Some(clap::builder::PossibleValue::new("copy")),
        }
    }
}

impl FromStr for LinkType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "soft" | "symlink" => Ok(LinkType::Soft),
            "hard" | "hardlink" => Ok(LinkType::Hard),
            "copy" => Ok(LinkType::Copy),
            _ => Err(format!("Unknown link type: {}", s)),
        }
    }
}
impl Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LinkType::Soft => write!(f, "soft"),
            LinkType::Hard => write!(f, "hard"),
            LinkType::Copy => write!(f, "copy"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq)]
pub enum PatchType {
    JsonMerge,
    JsonPatch,
}

impl Display for PatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PatchType::JsonMerge => write!(f, "JsonMerge"),
            PatchType::JsonPatch => write!(f, "JsonPatch"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatchConfig {
    pub source: PathBuf,
    pub target: PathBuf,
    #[serde(rename = "type")]
    pub patch_type: PatchType,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
}
impl PatchConfig {
    pub fn get_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires.clone();
        for tag in cfg.provides.iter() {
            requires.insert(RequireTag::Positive(tag.0.clone()));
        }
        requires.extend(cfg.requires.iter().cloned());
        requires
    }

    pub fn get_provides(&self, cfg: &HermitConfig) -> BTreeSet<Tag> {
        let mut provides = BTreeSet::new();
        provides.extend(cfg.provides.iter().cloned());
        provides
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LinkConfig {
    pub source: PathBuf,
    pub target: PathBuf,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default_link")]
    pub link: LinkType,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub provides: BTreeSet<Tag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default_fallback")]
    pub fallback: FallbackOperation,
}
fn is_default_fallback(fallback: &FallbackOperation) -> bool {
    matches!(fallback, FallbackOperation::Abort)
}

fn is_default_link(link_type: &LinkType) -> bool {
    matches!(link_type, LinkType::Soft)
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub enum FallbackOperation {
    #[default]
    Abort,
    Backup,
    BackupOverwrite,
    Delete,
    DeleteDir,
}

impl ValueEnum for FallbackOperation {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Abort,
            Self::Backup,
            Self::BackupOverwrite,
            Self::Delete,
            Self::DeleteDir,
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            FallbackOperation::Abort => Some(PossibleValue::new("abort")),
            FallbackOperation::Backup => Some(PossibleValue::new("backup")),
            FallbackOperation::BackupOverwrite => Some(PossibleValue::new("backupoverwrite")),
            FallbackOperation::Delete => Some(PossibleValue::new("delete")),
            FallbackOperation::DeleteDir => Some(PossibleValue::new("deletedir")),
        }
    }
}

#[derive(Debug)]
pub enum FileStatus {
    Ok,
    DestinationNotSymLink(PathBuf),
    FailedToReadSymlink(PathBuf),
    SymlinkDestinationMismatch(PathBuf, PathBuf),
    DestinationDoesNotExist(PathBuf),
    FailedToGetMetadata(PathBuf),
    InodeMismatch(PathBuf),
    SizeDiffers(PathBuf, u64, u64),
    SrcIsFileButTargetIsDir(PathBuf),
    SrcIsDirButTargetIsFile(PathBuf),
    HashDiffers(PathBuf, blake3::Hash, blake3::Hash),
    FailedToAccessFile(PathBuf, std::io::Error),
    FailedToTraverseDir(PathBuf, std::io::Error),
    FailedToHashFile(PathBuf, std::io::Error),
}
impl FileStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
}
impl Display for FileStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FileStatus::Ok => f.write_str("OK"),
            FileStatus::DestinationNotSymLink(path_buf) => write!(
                f,
                "The destination {path_buf:?} is not a symlink to the expected source"
            ),
            FileStatus::FailedToReadSymlink(path_buf) => {
                write!(f, "Failed to read symlink destination of file {path_buf:?}")
            }
            FileStatus::SymlinkDestinationMismatch(path_buf, link) => {
                write!(f, "The destination {path_buf:?} links to {link:?}")
            }
            FileStatus::DestinationDoesNotExist(path_buf) => {
                write!(f, "The destination {path_buf:?} does not exist")
            }
            FileStatus::FailedToGetMetadata(path_buf) => {
                write!(f, "Failed to get metadata for the file {path_buf:?}")
            }
            FileStatus::InodeMismatch(path_buf) => {
                write!(f, "The file {path_buf:?} is not hardlinked to the source")
            }
            FileStatus::SizeDiffers(path_buf, src_size, dst_size) => write!(
                f,
                "The target file {path_buf:?} differ in size {dst_size} (dst) vs {src_size} (src)"
            ),
            FileStatus::FailedToAccessFile(path_buf, error) => write!(
                f,
                "Failed to access the file {path_buf:?}, error was {error}"
            ),
            FileStatus::FailedToTraverseDir(path_buf, error) => write!(
                f,
                "Failed to traverse the directory {path_buf:?}, error was {error}"
            ),
            FileStatus::SrcIsFileButTargetIsDir(path_buf) => write!(
                f,
                "Src is a file, but destination {path_buf:?} is a directory"
            ),
            FileStatus::SrcIsDirButTargetIsFile(path_buf) => write!(
                f,
                "Src is a directory, but destination {path_buf:?} is a file"
            ),
            FileStatus::FailedToHashFile(path_buf, error) => {
                write!(f, "Failed to hash the file {path_buf:?}, error was {error}")
            }
            FileStatus::HashDiffers(path_buf, src_hash, dst_hash) => write!(
                f,
                "The hash of the file {path_buf:?} differs: {src_hash} (src) vs {dst_hash} (dst)"
            ),
        }
    }
}

impl LinkConfig {
    pub fn get_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires.clone();
        for tag in cfg.provides.iter() {
            requires.insert(RequireTag::Positive(tag.0.clone()));
        }
        requires.extend(cfg.requires.iter().cloned());
        requires
    }

    pub fn get_provides(&self, cfg: &HermitConfig) -> BTreeSet<Tag> {
        let mut provides = self.provides.clone();
        provides.extend(cfg.provides.iter().cloned());
        provides
    }

    pub fn check(&self, cfg: &HermitConfig, quick: bool) -> FileStatus {
        let cfg_dir = cfg.directory();
        let src_file = cfg_dir.join(&self.source);
        let src_file = src_file.canonicalize().unwrap_or(src_file);
        let actual_dst = cfg.global_config().expand_directory(&self.target);
        match actual_dst.try_exists() {
            Ok(exists) => {
                if !exists {
                    return FileStatus::DestinationDoesNotExist(actual_dst);
                }
            }
            Err(e) => return FileStatus::FailedToAccessFile(actual_dst, e),
        }
        match self.link {
            LinkType::Soft => {
                if !actual_dst.is_symlink() {
                    return FileStatus::DestinationNotSymLink(actual_dst);
                }
                let read_link = std::fs::read_link(&actual_dst);
                let Ok(read_link) = read_link else {
                    return FileStatus::FailedToReadSymlink(actual_dst);
                };
                if read_link != src_file {
                    return FileStatus::SymlinkDestinationMismatch(actual_dst, read_link);
                }
                FileStatus::Ok
            }
            LinkType::Hard => {
                #[cfg(target_family = "unix")]
                {
                    let Ok(dst_meta) = actual_dst.metadata() else {
                        return FileStatus::FailedToGetMetadata(actual_dst);
                    };
                    let Ok(src_meta) = src_file.metadata() else {
                        return FileStatus::FailedToGetMetadata(src_file);
                    };
                    use std::os::unix::fs::MetadataExt;
                    let dst_ino = dst_meta.ino();
                    let src_ino = src_meta.ino();
                    if src_ino != dst_ino {
                        return FileStatus::InodeMismatch(actual_dst);
                    }
                    FileStatus::Ok
                }
                #[cfg(not(target_family = "unix"))]
                {
                    crate::common_cli::warn(
                        "Hardlink check not supported on non unix systems, checking file similarity",
                    );
                    return check_copied(quick, &src_file, &actual_dst);
                }
            }
            LinkType::Copy => check_copied(quick, &src_file, &actual_dst),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct InstallConfig {
    pub name: String,
    pub source: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_install_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_install_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub provides: BTreeSet<Tag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

impl InstallConfig {
    pub fn get_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires.clone();
        for tag in cfg.provides.iter() {
            requires.insert(RequireTag::Positive(tag.0.clone()));
        }
        requires.extend(cfg.requires.iter().cloned());
        requires
    }

    pub fn get_provides(&self, cfg: &HermitConfig) -> BTreeSet<Tag> {
        let mut provides = self.provides.clone();
        provides.extend(cfg.provides.iter().cloned());
        provides
    }
}

#[derive(Debug, Default)]
pub struct GlobalConfig {
    pub root_dir: PathBuf,
    pub subconfigs: BTreeMap<String, HermitConfig>,
    pub all_profiles: BTreeMap<String, BTreeSet<Tag>>,
    pub all_provided_tags: BTreeSet<Tag>,
    pub all_detected_tags: BTreeSet<Tag>,
    pub all_sources: BTreeMap<String, String>,
}

impl GlobalConfig {
    pub fn from_paths(root_dir: &Path, paths: &[PathBuf]) -> Result<Arc<Self>, ConfigError> {
        let mut errors = Vec::new();
        Ok(Arc::new_cyclic(|global_config: &Weak<GlobalConfig>| {
            let mut result = GlobalConfig {
                root_dir: root_dir.to_path_buf(),
                ..Default::default()
            };
            for path in paths {
                log::debug!("Loading config from path: {}", path.display());
                let config = load_hermit_config(path, global_config.clone());
                let config = match config {
                    Ok(cfg) => cfg,
                    Err(e) => {
                        crate::error!("Failed to load config from path: {}: {}", path.display(), e);
                        errors.push(e);
                        continue;
                    }
                };
                for tag in &config.provides {
                    log::debug!("Adding provided tag: {}", tag);
                    result.all_provided_tags.insert(tag.clone());
                }
                for (k, v) in &config.sources {
                    if result.all_sources.contains_key(&k.to_lowercase()) {
                        crate::error!(
                            "Duplicate source key '{}' in config file: {}",
                            k,
                            config.path.display()
                        );
                        errors.push(ConfigError::DuplicateSource(
                            k.to_string(),
                            config.path.clone(),
                        ));
                        continue;
                    }
                    log::debug!("Adding source {}: {}", k, v);
                    result.all_sources.insert(k.to_lowercase(), v.clone());
                }
                // Collect profiles (error on duplicate, lower-case, dedup tags)
                for (profile, tags) in &config.profiles {
                    let profile_lc = profile.to_lowercase();
                    log::debug!("Adding profile {}: {:?}", profile_lc, tags);
                    if result.all_profiles.contains_key(&profile_lc) {
                        crate::error!(
                            "Duplicate profile '{}' in config file: {}",
                            profile_lc,
                            config.path.display()
                        );
                        errors.push(ConfigError::DuplicateProfile(
                            profile_lc.clone(),
                            config.path.clone(),
                        ));
                    }
                    result.all_profiles.insert(profile_lc, tags.clone());
                }
                let relative_path = path.strip_prefix(root_dir).unwrap_or(path);
                let relative_path_str = relative_path.to_string_lossy().to_string();
                result.subconfigs.insert(relative_path_str, config);
            }
            result.all_detected_tags = detect_builtin_tags();
            log::debug!("Detected tags: {:?}", result.all_detected_tags);
            result
        }))
    }

    pub fn hermit_dir(&self) -> &Path {
        &self.root_dir
    }

    pub fn get_active_tags(
        &self,
        cli_tags: &[String],
        cli_profile: &Option<String>,
    ) -> Result<BTreeSet<Tag>, ApplyError> {
        let mut active_tags = self.all_detected_tags.clone();
        for tag in cli_tags {
            let tag = tag.split(',');
            for t in tag {
                let t = t.trim();
                if !t.is_empty() {
                    let cli_tag = Tag::new(t, Source::CommandLine);
                    if self.all_provided_tags.contains(&cli_tag) {
                        active_tags.insert(cli_tag);
                    } else {
                        return Err(ApplyError::TagNotFound(t.to_string()));
                    }
                }
            }
        }
        let profile_to_use = self.all_profiles.get(
            &cli_profile
                .as_deref()
                .map(|x| x.to_lowercase())
                .unwrap_or("default".to_string()),
        );
        if let Some(profile_tags) = profile_to_use {
            active_tags.extend(profile_tags.iter().cloned());
        }
        Ok(active_tags)
    }

    pub fn get_profile(
        &self,
        cli_profile: &Option<String>,
    ) -> Result<Option<(usize, String)>, ApplyError> {
        let profiles: Vec<String> = self.all_profiles.keys().cloned().collect();
        let profile_to_use = if let Some(profile) = &cli_profile {
            let res = profiles
                .iter()
                .enumerate()
                .find(|(_, p)| p.eq_ignore_ascii_case(profile))
                .map(|(i, p)| (i, p.clone()));
            if res.is_none() {
                return Err(ApplyError::ProfileNotFound(profile.to_string()));
            }
            res
        } else if self.all_profiles.contains_key(DEFAULT_PROFILE) {
            profiles
                .iter()
                .enumerate()
                .find(|(_, p)| p.eq_ignore_ascii_case(DEFAULT_PROFILE))
                .map(|(i, p)| (i, p.clone()))
        } else {
            None
        };
        Ok(profile_to_use)
    }

    pub fn root_config(&self) -> Option<&HermitConfig> {
        let root_path = self.root_dir.join(CONF_FILE_NAME);
        self.subconfigs
            .get(&root_path.to_string_lossy().to_string())
    }

    pub fn prepare_cmd(
        &self,
        cmd: &str,
        additional_variables: &BTreeMap<String, String>,
    ) -> Result<String, ConfigError> {
        let reg = Handlebars::new();
        let data = additional_variables.clone();
        let template = shellexpand::tilde(cmd).to_string();
        let cmd = reg
            .render_template(&template, &data)
            .map_err(ConfigError::RenderError)?;
        Ok(cmd)
    }

    pub fn expand_directory<P: Into<PathBuf>>(&self, dir: P) -> PathBuf {
        let handlebars = handlebars::Handlebars::new();
        let dir: PathBuf = dir.into();
        let dir_str = dir.to_string_lossy().to_string();
        let dir = handlebars
            .render_template(&dir_str, &HashMap::<String, String>::new())
            .unwrap_or_else(|_| dir_str.to_string());

        if dir.starts_with("~/.config") && std::env::var("XDG_CONFIG_HOME").is_ok() {
            dir.replace(
                "~/.config",
                &std::env::var("XDG_CONFIG_HOME").unwrap_or_default(),
            )
            .into()
        } else if dir.starts_with("~/.local/share") && std::env::var("XDG_DATA_HOME").is_ok() {
            dir.replace(
                "~/.local/share",
                &std::env::var("XDG_DATA_HOME").unwrap_or_default(),
            )
            .into()
        } else if dir.starts_with("~/.local/state") && std::env::var("XDG_STATE_HOME").is_ok() {
            dir.replace(
                "~/.local/state",
                &std::env::var("XDG_STATE_HOME").unwrap_or_default(),
            )
            .into()
        } else {
            shellexpand::tilde(&dir).into_owned().into()
        }
    }
}

pub fn load_hermit_config<P: AsRef<Path>>(
    path: P,
    global_config: Weak<GlobalConfig>,
) -> Result<HermitConfig, ConfigError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigError::IoError(e, path.as_ref().to_path_buf()))?;
    let mut config: HermitConfig = toml::from_str(&content)
        .map_err(|e| ConfigError::DeserializeTomlError(e, path.as_ref().to_path_buf()))?;
    config.path = path.as_ref().to_path_buf();
    config.global_cfg = global_config;
    Ok(config)
}

pub fn load_hermit_config_editable<P: AsRef<Path>>(path: P) -> Result<DocumentMut, ConfigError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigError::IoError(e, path.as_ref().to_path_buf()))?;
    content
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::DeserializeDocumentTomlError(e, path.as_ref().to_path_buf()))
}

pub fn find_hermit_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.is_file() && root.file_name().is_some_and(|f| f == CONF_FILE_NAME) {
        result.push(root.to_path_buf());
    } else if root.is_dir() {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(find_hermit_files(&path));
                } else if path.file_name().is_some_and(|f| f == CONF_FILE_NAME) {
                    result.push(path);
                }
            }
        }
    }
    result
}
