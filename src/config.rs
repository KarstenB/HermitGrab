use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Display;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use toml_edit::DocumentMut;

use crate::action::expand_directory;
use crate::detector::detect_builtin_tags;
use crate::hermitgrab_error::ApplyError;
use crate::hermitgrab_error::ConfigLoadError;
use crate::links_files::FallbackOperation;

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

    pub(crate) fn is_detected(&self) -> bool {
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
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub provides: BTreeSet<Tag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub file: Vec<DotfileEntry>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patch: Vec<PatchfileEntry>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<InstallEntry>,
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
    pub fn create_new(path: &Path) -> Self {
        HermitConfig {
            path: path.to_path_buf(),
            ..Default::default()
        }
    }

    pub fn path(&self) -> &Path {
        self.path.as_path()
    }

    pub(crate) fn save_to_file(&self, conf_file_name: &PathBuf) -> Result<(), ConfigLoadError> {
        let content = toml::to_string(self)
            .map_err(|e| ConfigLoadError::SerializeTomlError(e, conf_file_name.clone()))?;
        std::fs::write(conf_file_name, content)
            .map_err(|e| ConfigLoadError::IoError(e, conf_file_name.clone()))?;
        Ok(())
    }

    pub fn directory(&self) -> &Path {
        self.path.parent().expect("Expected to get parent")
    }
}

#[derive(Debug, Serialize, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    #[default]
    Soft,
    Hard,
    Copy,
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

#[derive(Debug, Serialize, Deserialize, Clone)]
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
pub struct PatchfileEntry {
    pub source: String,
    pub target: String,
    #[serde(rename = "type")]
    pub patch_type: PatchType,
    pub requires: BTreeSet<RequireTag>,
}
impl PatchfileEntry {
    pub(crate) fn get_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires.clone();
        for tag in cfg.provides.iter() {
            requires.insert(RequireTag::Positive(tag.0.clone()));
        }
        for tag in &cfg.requires {
            requires.insert(tag.clone());
        }
        requires
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct DotfileEntry {
    pub source: String,
    pub target: String,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default_link")]
    pub link: LinkType,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
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
    FailedToAccessFile(PathBuf, std::io::Error),
    FailedToTraverseDir(PathBuf, std::io::Error),
    SrcIsFileButTargetIsDir(PathBuf),
    SrcIsDirButTargetIsFile(PathBuf),
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
        }
    }
}

impl DotfileEntry {
    pub(crate) fn get_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires.clone();
        for tag in cfg.provides.iter() {
            requires.insert(RequireTag::Positive(tag.0.clone()));
        }
        for tag in &cfg.requires {
            requires.insert(tag.clone());
        }
        requires
    }

    pub(crate) fn check(&self, cfg_dir: &Path, quick: bool) -> FileStatus {
        let src_file = cfg_dir.join(&self.source);
        let src_file = src_file.canonicalize().unwrap_or(src_file);
        let actual_dst = PathBuf::from(expand_directory(&self.target));
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
            LinkType::Copy => check_copied(quick, src_file, actual_dst),
        }
    }
}

fn check_copied(quick: bool, src_file: PathBuf, actual_dst: PathBuf) -> FileStatus {
    match actual_dst.try_exists() {
        Ok(exists) => {
            if !exists {
                return FileStatus::DestinationDoesNotExist(actual_dst);
            }
        }
        Err(e) => return FileStatus::FailedToAccessFile(actual_dst, e),
    }
    if actual_dst.is_file() {
        if !src_file.is_file() {
            return FileStatus::SrcIsDirButTargetIsFile(actual_dst);
        }
        let Ok(dst_meta) = actual_dst.metadata() else {
            return FileStatus::FailedToGetMetadata(actual_dst);
        };
        let Ok(src_meta) = src_file.metadata() else {
            return FileStatus::FailedToGetMetadata(src_file);
        };
        if src_meta.len() != dst_meta.len() {
            return FileStatus::SizeDiffers(actual_dst, src_meta.len(), dst_meta.len());
        }
        if !quick {
            todo!("blake3 the contents and determine mismatch")
        }
        FileStatus::Ok
    } else {
        if !src_file.is_dir() {
            return FileStatus::SrcIsFileButTargetIsDir(actual_dst);
        }
        match src_file.read_dir() {
            Ok(e) => {
                for f in e {
                    let fs = match f {
                        Ok(file) => {
                            check_copied(quick, file.path(), actual_dst.join(file.file_name()))
                        }
                        Err(e) => return FileStatus::FailedToTraverseDir(src_file, e),
                    };
                    if !fs.is_ok() {
                        return fs;
                    }
                }
            }
            Err(e) => {
                return FileStatus::FailedToTraverseDir(src_file, e);
            }
        }
        FileStatus::Ok
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct InstallEntry {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pre_install_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub post_install_cmd: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub variables: BTreeMap<String, String>,
}

impl InstallEntry {
    pub fn to_handlebars_map(&self) -> BTreeMap<String, String> {
        let mut map = self.variables.clone();
        map.insert("name".to_string(), self.name.clone());
        if let Some(version) = &self.version {
            map.insert("version".to_string(), version.clone());
        }
        map
    }

    pub(crate) fn get_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires.clone();
        for tag in cfg.provides.iter() {
            requires.insert(RequireTag::Positive(tag.0.clone()));
        }
        for tag in &cfg.requires {
            requires.insert(tag.clone());
        }
        requires
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
    pub fn from_paths(root_dir: &Path, paths: &[PathBuf]) -> Result<Self, ConfigLoadError> {
        let mut subconfigs = BTreeMap::new();
        let mut all_profiles = BTreeMap::new();
        let mut all_provided_tags = BTreeSet::new();
        let mut all_sources = BTreeMap::new();
        for path in paths {
            let config = load_hermit_config(path)?;
            for tag in &config.provides {
                all_provided_tags.insert(tag.clone());
            }
            for (k, v) in &config.sources {
                if all_sources.contains_key(&k.to_lowercase()) {
                    return Err(ConfigLoadError::DuplicateSource(k.to_string(), config.path));
                }
                all_sources.insert(k.to_lowercase(), v.clone());
            }
            // Collect profiles (error on duplicate, lower-case, dedup tags)
            for (profile, tags) in &config.profiles {
                let profile_lc = profile.to_lowercase();
                if all_profiles.contains_key(&profile_lc) {
                    return Err(ConfigLoadError::DuplicateProfile(
                        profile_lc.clone(),
                        config.path.clone(),
                    ));
                }
                all_profiles.insert(profile_lc, tags.clone());
            }
            let relative_path = path.strip_prefix(root_dir).unwrap_or(path);
            let relative_path_str = relative_path.to_string_lossy().to_string();
            subconfigs.insert(relative_path_str, config);
        }
        let all_detected_tags = detect_builtin_tags();
        Ok(GlobalConfig {
            root_dir: root_dir.to_path_buf(),
            subconfigs,
            all_profiles,
            all_provided_tags,
            all_detected_tags,
            all_sources,
        })
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
}

pub fn load_hermit_config<P: AsRef<Path>>(path: P) -> Result<HermitConfig, ConfigLoadError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigLoadError::IoError(e, path.as_ref().to_path_buf()))?;
    let mut config: HermitConfig = toml::from_str(&content)
        .map_err(|e| ConfigLoadError::DeserializeTomlError(e, path.as_ref().to_path_buf()))?;
    config.path = path.as_ref().to_path_buf();
    Ok(config)
}

pub fn load_hermit_config_editable<P: AsRef<Path>>(
    path: P,
) -> Result<DocumentMut, ConfigLoadError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigLoadError::IoError(e, path.as_ref().to_path_buf()))?;
    content
        .parse::<DocumentMut>()
        .map_err(|e| ConfigLoadError::DeserializeDocumentTomlError(e, path.as_ref().to_path_buf()))
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
