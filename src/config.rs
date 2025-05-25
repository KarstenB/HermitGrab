use serde::Deserialize;
use serde::Deserializer;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::{Path, PathBuf};

use crate::hermitgrab_error::ConfigLoadError;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Tag(String);
impl Tag {
    pub fn new(tag: &str) -> Self {
        Tag(tag.to_lowercase())
    }
}
impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
impl From<String> for Tag {
    fn from(tag: String) -> Self {
        Tag(tag.to_lowercase())
    }
}
impl From<&str> for Tag {
    fn from(tag: &str) -> Self {
        Tag(tag.to_lowercase())
    }
}
impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(Tag::new(&s))
    }
}

#[derive(Debug, PartialEq, Eq, Clone)]
pub enum RequireTag {
    Positive(String),
    Negative(String),
}

impl RequireTag {
    pub fn matches(&self, tags: &BTreeSet<Tag>) -> bool {
        match self {
            RequireTag::Positive(tag) => tags.contains(&Tag::new(tag)),
            RequireTag::Negative(tag) => !tags.contains(&Tag::new(tag)),
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
    pub depends: Vec<String>,
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
    pub requires: Vec<RequireTag>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct InstallEntry {
    pub name: String,
    pub check_cmd: Option<String>,
    pub source: Option<String>,
    pub version: Option<String>,
    #[serde(default)]
    pub requires: Vec<RequireTag>,
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
}

#[derive(Debug)]
pub struct GlobalConfig {
    pub subconfigs: Vec<HermitConfig>,
    pub all_profiles: BTreeMap<String, BTreeSet<Tag>>, // lowercased, deduped, error on duplicate
    pub all_tags: BTreeSet<Tag>,                       // lowercased, deduped
    pub all_sources: BTreeMap<String, String>,         // last one wins
}

impl GlobalConfig {
    pub fn from_paths(paths: &[PathBuf]) -> Result<Self, ConfigLoadError> {
        let mut subconfigs = Vec::new();
        let mut all_profiles = BTreeMap::new();
        let mut all_tags = BTreeSet::new();
        let mut all_sources = BTreeMap::new();
        for path in paths {
            let config = load_hermit_config(path)?;
            for tag in &config.provides {
                all_tags.insert(tag.clone());
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
        Ok(GlobalConfig {
            subconfigs,
            all_profiles,
            all_tags,
            all_sources,
        })
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
