use serde::Deserialize;
use serde::Deserializer;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::fmt::Display;
use std::hash::Hash;
use std::path::{Path, PathBuf};

use crate::detector::detect_builtin_tags;
use crate::hermitgrab_error::ApplyError;
use crate::hermitgrab_error::ConfigLoadError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
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
}
impl Hash for Tag {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.0.hash(state);
        self.1.hash(state);
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

impl<'de> Deserialize<'de> for RequireTag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
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

#[derive(Debug, Deserialize)]
pub struct HermitConfig {
    #[serde(skip)]
    path: PathBuf,
    #[serde(default)]
    pub provides: Vec<Tag>,
    #[serde(default)]
    pub files: Vec<DotfileEntry>,
    #[serde(default)]
    pub install: Vec<InstallEntry>,
    #[serde(default)]
    pub sources: HashMap<String, String>,
    #[serde(default)]
    pub requires: Vec<RequireTag>,
    #[serde(default)]
    pub profiles: HashMap<String, BTreeSet<Tag>>,
}

impl HermitConfig {
    pub fn path(&self) -> &Path {
        self.path.as_path()
    }
}

#[derive(Debug, Deserialize, Default, Clone, Copy)]
#[serde(rename_all = "lowercase")]
pub enum LinkType {
    #[default]
    Soft,
    Hard,
    Copy,
}

#[derive(Debug, Deserialize)]
pub struct DotfileEntry {
    pub source: String,
    pub target: String,
    #[serde(default)]
    pub link: LinkType,
    #[serde(default)]
    pub requires: BTreeSet<RequireTag>,
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
}

#[derive(Debug, Deserialize, Clone)]
pub struct InstallEntry {
    pub name: String,
    pub check_cmd: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    pub variables: std::collections::HashMap<String, String>,
}

impl InstallEntry {
    pub fn to_handlebars_map(&self) -> std::collections::HashMap<String, String> {
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

#[derive(Debug)]
pub struct GlobalConfig {
    pub root_dir: PathBuf,
    pub subconfigs: Vec<HermitConfig>,
    pub all_profiles: BTreeMap<String, BTreeSet<Tag>>,
    pub all_provided_tags: BTreeSet<Tag>,
    pub all_detected_tags: BTreeSet<Tag>,
    pub all_sources: BTreeMap<String, String>,
}

impl GlobalConfig {
    pub fn from_paths(root_dir: PathBuf, paths: &[PathBuf]) -> Result<Self, ConfigLoadError> {
        let mut subconfigs = Vec::new();
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
            subconfigs.push(config);
        }
        let all_detected_tags = detect_builtin_tags();
        Ok(GlobalConfig {
            root_dir,
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
                    active_tags.insert(Tag::new(t, Source::CommandLine));
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
        } else if self.all_profiles.contains_key("default") {
            profiles
                .iter()
                .enumerate()
                .find(|(_, p)| p.eq_ignore_ascii_case("default"))
                .map(|(i, p)| (i, p.clone()))
        } else {
            None
        };
        Ok(profile_to_use)
    }
}

pub fn load_hermit_config<P: AsRef<Path>>(path: P) -> Result<HermitConfig, ConfigLoadError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigLoadError::IoError(e, path.as_ref().to_path_buf()))?;
    let mut config: HermitConfig = serde_yml::from_str(&content)
        .map_err(|e| ConfigLoadError::SerdeYmlError(e, path.as_ref().to_path_buf()))?;
    config.path = path.as_ref().to_path_buf();
    Ok(config)
}

pub fn find_hermit_yaml_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.is_file() && root.file_name().is_some_and(|f| f == "hermit.yaml") {
        result.push(root.to_path_buf());
    } else if root.is_dir() {
        if let Ok(entries) = std::fs::read_dir(root) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    result.extend(find_hermit_yaml_files(&path));
                } else if path.file_name().is_some_and(|f| f == "hermit.yaml") {
                    result.push(path);
                }
            }
        }
    }
    result
}
