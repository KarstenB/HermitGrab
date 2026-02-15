// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::{BTreeMap, BTreeSet};
use std::convert::Infallible;
use std::fmt::Display;
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::{Arc, Weak};

use clap::ValueEnum;
use clap::builder::PossibleValue;
use handlebars::{
    Context, Handlebars, Helper, Output, RenderContext, RenderError, RenderErrorReason,
};
use serde::{Deserialize, Deserializer, Serialize};
use toml_edit::DocumentMut;

use crate::action::install::InstallAction;
use crate::action::link::LinkAction;
use crate::action::patch::PatchAction;
use crate::action::{Actions, ArcAction, SourceSpec};
use crate::config::handlebar_math::math_helper;
use crate::debug;
use crate::detector::{detect_builtin_tags, get_detected_tags};
use crate::file_ops::dirs::*;
use crate::hermitgrab_error::{ApplyError, ConfigError};

pub const CONF_FILE_NAME: &str = "hermit.toml";
pub const DEFAULT_PROFILE: &str = "default";

mod handlebar_math;

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
            Source::Detector(name) => write!(f, "detector: {name}"),
            Source::BuiltInDetector => write!(f, "built-in detector"),
            Source::Config => write!(f, "config"),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Tag(String, Option<String>, Source);
impl Tag {
    pub fn new(tag: &str, source: Source) -> Self {
        Tag(tag.to_lowercase(), None, source)
    }

    pub fn new_with_value(tag: &str, value: &str, source: Source) -> Self {
        Tag(tag.to_lowercase(), Some(value.to_string()), source)
    }

    pub fn name(&self) -> &str {
        &self.0
    }
    pub fn value(&self) -> &Option<String> {
        &self.1
    }
    pub fn source(&self) -> &Source {
        &self.2
    }

    pub fn is_detected(&self) -> bool {
        matches!(self.2, Source::Detector(_) | Source::BuiltInDetector)
    }

    pub fn from_str_with_src(s: &str, config: Source) -> Tag {
        let mut tag: Tag = s.parse().expect("Infaillable");
        tag.2 = config;
        tag
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
        self.0 == other.0 && self.1 == other.1
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
        self.0.cmp(&other.0).then(self.1.cmp(&other.1))
    }
}

impl std::fmt::Display for Tag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(value) = &self.1 {
            write!(f, "{}={value}", self.0)
        } else {
            write!(f, "{}", self.0)
        }
    }
}

impl FromStr for Tag {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        if let Some((key, value)) = s.split_once('=') {
            Ok(Tag::new_with_value(key, value, Source::Unknown))
        } else {
            Ok(Tag::new(s, Source::Unknown))
        }
    }
}

impl Serialize for Tag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Tag {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(s.parse().expect("Infallible"))
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
pub enum RequireTag {
    Positive(Tag),
    Negative(Tag),
}

impl RequireTag {
    pub fn matches(&self, tags: &BTreeSet<Tag>) -> bool {
        match self {
            RequireTag::Positive(tag) => tags.contains(tag),
            RequireTag::Negative(tag) => !tags.contains(tag),
        }
    }
    pub fn name(&self) -> &str {
        match self {
            RequireTag::Positive(tag) => tag.name(),
            RequireTag::Negative(tag) => tag.name(),
        }
    }
}

impl FromStr for RequireTag {
    type Err = Infallible;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let trimmed = s.trim();
        if let Some(rest) = trimmed.strip_prefix('+') {
            Ok(RequireTag::Positive(rest.parse()?))
        } else if let Some(rest) = trimmed.strip_prefix('-') {
            Ok(RequireTag::Negative(rest.parse()?))
        } else if let Some(rest) = trimmed.strip_prefix('~') {
            Ok(RequireTag::Negative(rest.parse()?))
        } else {
            Ok(RequireTag::Positive(trimmed.parse()?))
        }
    }
}
impl Display for RequireTag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RequireTag::Positive(tag) => write!(f, "+{tag}"),
            RequireTag::Negative(tag) => write!(f, "-{tag}"),
        }
    }
}

impl Serialize for RequireTag {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        let s = match self {
            RequireTag::Positive(tag) => format!("+{tag}"),
            RequireTag::Negative(tag) => format!("-{tag}"),
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

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum DetectorConfig {
    EnableIf { enable_if: String },
    EnableIfNot { enable_if_not: String },
    ValueOf { value_of: String },
}

#[derive(Debug, Default, Serialize, Deserialize, Clone, PartialEq)]
pub struct HermitSettings {
    /// If true, handlebars will error on missing variables instead of leaving them blank
    pub strict_mode: bool,
}

impl HermitSettings {
    pub fn is_default(&self) -> bool {
        self == &Default::default()
    }
}

#[derive(Debug, Default, Serialize, Deserialize, Clone)]
pub struct HermitConfig {
    #[serde(skip)]
    path: PathBuf,
    #[serde(skip)]
    canonicalize_dir: PathBuf,
    #[serde(skip)]
    global_cfg: Weak<GlobalConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub link: Vec<LinkConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub patch: Vec<PatchConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub install: Vec<InstallConfig>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub snippets: BTreeMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "HermitSettings::is_default")]
    pub settings: HermitSettings,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub profiles: BTreeMap<String, BTreeSet<Tag>>,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub detectors: BTreeMap<String, DetectorConfig>,
}

pub type ArcHermitConfig = Arc<HermitConfig>;

impl HermitConfig {
    pub fn create_new(path: &Path, global_cfg: Weak<GlobalConfig>) -> Self {
        HermitConfig {
            global_cfg,
            ..Default::default()
        }
        .update_path(path)
    }

    pub fn update_path<P: AsRef<Path>>(self, path: P) -> Self {
        let full_path = path
            .as_ref()
            .canonicalize()
            .unwrap_or_else(|_| path.as_ref().to_path_buf());
        let full_dir = full_path
            .parent()
            .unwrap_or_else(|| Path::new("."))
            .canonicalize()
            .unwrap_or_else(|_| {
                full_path
                    .parent()
                    .unwrap_or_else(|| Path::new("."))
                    .to_path_buf()
            });
        HermitConfig {
            path: full_path,
            canonicalize_dir: full_dir,
            ..self
        }
    }

    /// Path to hermit.toml file
    pub fn hermit_file(&self) -> &Path {
        &self.path
    }

    pub fn global_config(&self) -> Arc<GlobalConfig> {
        self.global_cfg
            .upgrade()
            .expect("Global config should be set before using HermitConfig")
    }

    pub fn save_to_file(&self, conf_file_name: &PathBuf) -> Result<(), ConfigError> {
        let content = toml::to_string(self)
            .map_err(|e| ConfigError::SerializeToml(e, conf_file_name.clone()))?;
        std::fs::write(conf_file_name, content)
            .map_err(|e| ConfigError::Io(e, conf_file_name.clone()))?;
        Ok(())
    }

    /// Directory containing the hermit.toml file
    pub fn directory(&self) -> &Path {
        &self.canonicalize_dir
    }

    pub fn config_items(&self) -> impl Iterator<Item = &dyn ConfigItem> {
        self.link
            .iter()
            .map(|c| c as &dyn ConfigItem)
            .chain(self.patch.iter().map(|c| c as &dyn ConfigItem))
            .chain(self.install.iter().map(|c| c as &dyn ConfigItem))
            .chain(std::iter::once(self as &dyn ConfigItem))
    }

    pub fn get_snippet(
        &self,
        lc_src: &str,
        variables: &BTreeMap<String, String>,
    ) -> Result<String, RenderError> {
        self.snippets
            .get(lc_src)
            .cloned()
            .or_else(|| self.global_config().get_snippet(lc_src).cloned())
            .map(|src| self.render_handlebars(&src, variables))
            .unwrap_or_else(|| {
                Err(RenderErrorReason::Other(format!("Snippet not found: {lc_src}")).into())
            })
    }

    pub fn render_handlebars(
        &self,
        content: &str,
        variables: &BTreeMap<String, String>,
    ) -> Result<String, RenderError> {
        let global_config = self.global_config();
        let dir_map = self.collect_dir_map(&global_config);
        let all_tags: BTreeMap<String, String> = global_config
            .all_detected_tags()
            .iter()
            .filter_map(|x| x.1.as_deref().map(|y| (x.0.to_string(), y.to_string())))
            .collect();
        let mut rendered_variables = BTreeMap::new();
        let empty_map = BTreeMap::new();
        let cfg = self;
        for (key, value) in variables.iter() {
            rendered_variables.insert(key, cfg.render_handlebars(value, &empty_map)?);
        }
        let env_vars: BTreeMap<String, String> = std::env::vars().collect();
        let sys_info: BTreeMap<String, String> = BTreeMap::from([
            (
                "cpu_num".to_string(),
                sys_info::cpu_num()
                    .map(|n| n.to_string())
                    .unwrap_or_default(),
            ),
            (
                "mem_total".to_string(),
                sys_info::mem_info()
                    .map(|m| m.total.to_string())
                    .unwrap_or_default(),
            ),
        ]);
        let mut hermit: BTreeMap<String, String> = BTreeMap::from([
            ("version".to_string(), env!("CARGO_PKG_VERSION").to_string()),
            (
                "exe".to_string(),
                HERMIT_EXE.to_str().unwrap_or_default().to_string(),
            ),
            (
                "ubi".to_string(),
                format!("\"{}\" ubi -- ", HERMIT_EXE.display()),
            ),
        ]);
        if let Some(sha) = option_env!("CARGO_MAKE_GIT_HEAD_LAST_COMMIT_HASH_PREFIX") {
            hermit.insert("git_sha".to_string(), sha.to_string());
        }
        let object = serde_json::json!({
            "dir": dir_map,
            "var": rendered_variables,
            "tag": all_tags,
            "env": env_vars,
            "sys": sys_info,
            "hermit": hermit,
        });
        let reg = create_handlebars(variables, cfg);
        debug!("Rendering content {content}");
        reg.render_template(content, &object)
    }

    fn collect_dir_map(&self, global_config: &Arc<GlobalConfig>) -> BTreeMap<&'static str, String> {
        let mut paths = vec![
            (
                "here",
                self.directory()
                    .to_path_buf()
                    .to_str()
                    .expect("PathBuf is correct")
                    .to_string(),
            ),
            (
                "hermit",
                global_config
                    .hermit_dir()
                    .to_path_buf()
                    .to_str()
                    .expect("PathBuf is correct")
                    .to_string(),
            ),
            ("home", HOME_DIR.clone()),
            ("config", CONFIG_DIR.clone()),
            ("data", DATA_DIR.clone()),
            ("cache", CACHE_DIR.clone()),
            ("preference", PREFERENCE_DIR.clone()),
            // These are windows specific LocalAppData equivalents
            ("config_local", CONFIG_LOCAL_DIR.clone()),
            ("data_local", DATA_LOCAL_DIR.clone()),
            // XDG based dirs
            ("xdg_home", XDG_HOME.clone()),
            ("xdg_config", XDG_CONFIG_HOME.clone()),
            ("xdg_data", XDG_DATA_HOME.clone()),
            ("xdg_cache", XDG_CACHE_HOME.clone()),
            ("xdg_bin", XDG_BIN_HOME.clone()),
            ("xdg_state", XDG_STATE_HOME.clone()),
        ];
        if let Some(state_dir) = STATE_DIR.clone() {
            debug!("Using state dir: {}", state_dir);
            paths.push(("state", state_dir));
        }
        if let Some(executable_dir) = EXECUTABLE_DIR.clone() {
            debug!("Using executable dir: {}", executable_dir);
            paths.push(("executable", executable_dir));
        }
        if let Some(runtime_dir) = RUNTIME_DIR.clone() {
            debug!("Using runtime dir: {}", runtime_dir);
            paths.push(("runtime", runtime_dir));
        }
        if let Some(xdg_runtime_dir) = XDG_RUNTIME_DIR.clone() {
            debug!("Using xdg runtime dir: {}", xdg_runtime_dir);
            paths.push(("xdg_runtime", xdg_runtime_dir));
        }
        let dir_map: BTreeMap<&'static str, String> = BTreeMap::from_iter(paths);
        dir_map
    }

    pub fn expand_directory<P: Into<PathBuf>>(&self, dir: P) -> Result<PathBuf, RenderError> {
        let dir: PathBuf = dir.into();
        let dir_str = dir.to_string_lossy().to_string();
        let dir = self.render_handlebars(&dir_str, &BTreeMap::new())?;
        debug!("Expanding directory: {}", dir);
        if dir.starts_with("~/.config") {
            Ok(dir.replace("~/.config", &XDG_CONFIG_HOME).into())
        } else if dir.starts_with("~/.local/share") {
            Ok(dir.replace("~/.local/share", &XDG_DATA_HOME).into())
        } else if dir.starts_with("~/.local/state") {
            Ok(dir.replace("~/.local/state", &XDG_STATE_HOME).into())
        } else if dir.starts_with("~/.cache") {
            Ok(dir.replace("~/.cache", &XDG_CACHE_HOME).into())
        } else if dir.starts_with("~/.local/bin") {
            Ok(dir.replace("~/.local/bin", &XDG_BIN_HOME).into())
        } else {
            Ok(
                shellexpand::tilde_with_context(&dir, || Some(HOME_DIR.to_string()))
                    .into_owned()
                    .into(),
            )
        }
    }

    pub fn canonicalize_source_path<E>(
        &self,
        file: &PathBuf,
        file_should_exist: bool,
    ) -> Result<PathBuf, E>
    where
        E: From<std::io::Error>,
        E: From<RenderError>,
    {
        let src = self.expand_directory(&file)?;
        let src = if src.is_absolute() {
            src.clone()
        } else {
            self.directory().join(&src)
        };
        if file_should_exist & !src.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File does not exist: {}", src.display()),
            )
            .into());
        }
        if !file_should_exist {
            return Ok(src);
        }
        Ok(src.canonicalize()?)
    }
}

fn create_handlebars<'cfg, 'v>(
    variables: &'v BTreeMap<String, String>,
    cfg: &'cfg HermitConfig,
) -> Handlebars<'cfg>
where
    'v: 'cfg,
{
    let mut reg = Handlebars::new();
    reg.register_helper(
        "snippet",
        Box::new(
            |h: &Helper,
             _: &Handlebars,
             _: &Context,
             _: &mut RenderContext,
             out: &mut dyn Output|
             -> Result<(), RenderError> {
                let snippet = h
                    .param(0)
                    .and_then(|x| x.relative_path())
                    .ok_or(RenderErrorReason::ParamNotFoundForIndex("format", 0))?;
                let resolved = cfg.get_snippet(snippet, variables)?;
                out.write(&resolved)?;
                Ok(())
            },
        ),
    );
    reg.register_helper("math", Box::new(math_helper));
    debug!(
        "Setting handlebars strict mode to {}",
        cfg.settings.strict_mode
    );
    reg.set_strict_mode(cfg.settings.strict_mode);
    reg
}

impl ConfigItem for HermitConfig {
    fn requires(&self) -> &BTreeSet<RequireTag> {
        &self.requires
    }

    fn as_action(&self, _: &HermitConfig, _options: &CliOptions) -> Result<ArcAction, ConfigError> {
        Err(ConfigError::HermitConfigNotAction)
    }

    fn id(&self) -> String {
        format!("HermitConfig {:?}", self.directory())
    }

    fn order(&self) -> Option<u64> {
        self.order
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
        &[Self::Soft, Self::Hard, Self::Copy]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            Self::Soft => Some(clap::builder::PossibleValue::new("soft")),
            Self::Hard => Some(clap::builder::PossibleValue::new("hard")),
            Self::Copy => Some(clap::builder::PossibleValue::new("copy")),
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
            _ => Err(format!("Unknown link type: {s}")),
        }
    }
}

impl Display for LinkType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Soft => write!(f, "soft"),
            Self::Hard => write!(f, "hard"),
            Self::Copy => write!(f, "copy"),
        }
    }
}

impl ValueEnum for PatchType {
    fn value_variants<'a>() -> &'a [Self] {
        &[PatchType::JsonMerge, PatchType::JsonPatch]
    }

    fn to_possible_value(&self) -> Option<PossibleValue> {
        match self {
            Self::JsonMerge => {
                Some(PossibleValue::new("JsonMerge").aliases(["jsonmerge", "merge"]))
            }
            Self::JsonPatch => {
                Some(PossibleValue::new("JsonPatch").aliases(["jsonpatch", "patch"]))
            }
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Hash, PartialEq, Eq, Default)]
pub enum PatchType {
    #[default]
    JsonMerge,
    JsonPatch,
}

impl Display for PatchType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::JsonMerge => write!(f, "JsonMerge"),
            Self::JsonPatch => write!(f, "JsonPatch"),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(untagged)]
pub enum SourceSpecOrPath {
    Path(PathBuf),
    SourceSpec(SourceSpec),
}

impl SourceSpecOrPath {
    pub fn path(&self) -> &Path {
        match self {
            Self::Path(p) => p,
            Self::SourceSpec(s) => s.file(),
        }
    }

    pub fn normalize<E>(&self, cfg: &HermitConfig, dst: &Path) -> Result<SourceSpec, E>
    where
        E: From<std::io::Error>,
        E: From<RenderError>,
    {
        match self {
            Self::Path(p) => SourceSpec::raw_path(p.clone()).normalize(cfg, dst),
            Self::SourceSpec(s) => s.normalize(cfg, dst),
        }
    }
}

impl From<SourceSpecOrPath> for SourceSpec {
    fn from(value: SourceSpecOrPath) -> Self {
        match value {
            SourceSpecOrPath::Path(p) => SourceSpec::raw_path(p),
            SourceSpecOrPath::SourceSpec(s) => s,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PatchConfig {
    pub source: SourceSpecOrPath,
    pub target: PathBuf,
    #[serde(rename = "type", default)]
    pub patch_type: PatchType,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
}

impl ConfigItem for PatchConfig {
    fn requires(&self) -> &BTreeSet<RequireTag> {
        &self.requires
    }

    fn as_action(
        &self,
        cfg: &HermitConfig,
        _options: &CliOptions,
    ) -> Result<ArcAction, ConfigError> {
        Ok(Arc::new(Actions::Patch(PatchAction::new(self, cfg)?)))
    }

    fn id(&self) -> String {
        format!("Patch {:?} with {:?}", self.target, self.source)
    }

    fn order(&self) -> Option<u64> {
        self.order
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LinkConfig {
    pub source: SourceSpecOrPath,
    pub target: PathBuf,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default_link")]
    pub link: LinkType,
    #[serde(default)]
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub requires: BTreeSet<RequireTag>,
    #[serde(default)]
    #[serde(skip_serializing_if = "is_default_fallback")]
    pub fallback: FallbackOperation,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
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
    Ignore,
}

impl Display for FallbackOperation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FallbackOperation::Abort => f.write_str("abort"),
            FallbackOperation::Backup => f.write_str("backup"),
            FallbackOperation::BackupOverwrite => f.write_str("backupoverwrite"),
            FallbackOperation::Delete => f.write_str("delete"),
            FallbackOperation::DeleteDir => f.write_str("deletedir"),
            FallbackOperation::Ignore => f.write_str("ignore"),
        }
    }
}

impl ValueEnum for FallbackOperation {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Abort,
            Self::Backup,
            Self::BackupOverwrite,
            Self::Delete,
            Self::DeleteDir,
            Self::Ignore,
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            FallbackOperation::Abort => Some(PossibleValue::new("abort")),
            FallbackOperation::Backup => Some(PossibleValue::new("backup")),
            FallbackOperation::BackupOverwrite => Some(PossibleValue::new("backupoverwrite")),
            FallbackOperation::Delete => Some(PossibleValue::new("delete")),
            FallbackOperation::DeleteDir => Some(PossibleValue::new("deletedir")),
            FallbackOperation::Ignore => Some(PossibleValue::new("ignore")),
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
    InodeMismatch(PathBuf),
    SizeDiffers(PathBuf, u64, u64),
    SrcIsFileButTargetIsDir(PathBuf),
    SrcIsDirButTargetIsFile(PathBuf),
    HashDiffers(PathBuf, blake3::Hash, blake3::Hash),
    FailedToGetMetadata(PathBuf, std::io::Error),
    FailedToAccessFile(PathBuf, std::io::Error),
    FailedToTraverseDir(PathBuf, std::io::Error),
    FailedToHashFile(PathBuf, std::io::Error),
}
impl FileStatus {
    pub fn is_ok(&self) -> bool {
        matches!(self, Self::Ok)
    }
    pub fn is_error(&self) -> bool {
        matches!(
            self,
            Self::FailedToAccessFile(_, _)
                | Self::FailedToGetMetadata(_, _)
                | Self::FailedToHashFile(_, _)
                | Self::FailedToTraverseDir(_, _)
        )
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
            FileStatus::FailedToGetMetadata(path_buf, error) => {
                write!(
                    f,
                    "Failed to get metadata for the file {path_buf:?}, error was {error}"
                )
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

impl LinkConfig {}

impl ConfigItem for LinkConfig {
    fn requires(&self) -> &BTreeSet<RequireTag> {
        &self.requires
    }
    fn as_action(
        &self,
        cfg: &HermitConfig,
        options: &CliOptions,
    ) -> Result<ArcAction, ConfigError> {
        Ok(Arc::new(Actions::Link(LinkAction::new(
            self,
            cfg,
            &options.fallback,
        )?)))
    }

    fn id(&self) -> String {
        format!("Link {:?}->{:?}", self.source, self.target)
    }

    fn order(&self) -> Option<u64> {
        self.order
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct InstallConfig {
    pub name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub check: Option<String>,
    pub install: String,
    #[serde(skip_serializing_if = "BTreeSet::is_empty", default)]
    pub requires: BTreeSet<RequireTag>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty", default)]
    pub variables: BTreeMap<String, String>,
    #[serde(default)]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub order: Option<u64>,
}

impl ConfigItem for InstallConfig {
    fn requires(&self) -> &BTreeSet<RequireTag> {
        &self.requires
    }

    fn as_action(
        &self,
        cfg: &HermitConfig,
        _options: &CliOptions,
    ) -> Result<ArcAction, ConfigError> {
        Ok(Arc::new(Actions::Install(InstallAction::new(self, cfg)?)))
    }

    fn id(&self) -> String {
        format!("Install {}", self.name)
    }

    fn order(&self) -> Option<u64> {
        self.order
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct CliOptions {
    pub fallback: Option<FallbackOperation>,
    pub confirm: bool,
    pub verbose: bool,
    pub tags: Vec<String>,
    pub profile: Option<String>,
    pub json: Option<PathBuf>,
}

pub trait ConfigItem {
    fn id(&self) -> String;
    fn requires(&self) -> &BTreeSet<RequireTag>;
    fn order(&self) -> Option<u64>;
    fn total_order(&self, cfg: &HermitConfig) -> u64 {
        self.order().or(cfg.order).unwrap_or(0)
    }
    fn get_all_requires(&self, cfg: &HermitConfig) -> BTreeSet<RequireTag> {
        let mut requires = self.requires().clone();
        requires.extend(cfg.requires.iter().cloned());
        requires
    }
    fn as_action(&self, cfg: &HermitConfig, options: &CliOptions)
    -> Result<ArcAction, ConfigError>;
}

#[derive(Debug, Default)]
pub struct GlobalConfig {
    hermit_dir: PathBuf,
    subconfigs: BTreeMap<String, ArcHermitConfig>,
    all_profiles: BTreeMap<String, BTreeSet<Tag>>,
    all_required_tags: BTreeSet<RequireTag>,
    all_detected_tags: BTreeSet<Tag>,
    all_snippets: BTreeMap<String, String>,
    all_detectors: BTreeMap<String, DetectorConfig>,
}

impl GlobalConfig {
    pub fn from_paths(hermit_dir: &Path, paths: &[PathBuf]) -> Result<Arc<Self>, ConfigError> {
        let mut errors = Vec::new();
        Ok(Arc::new_cyclic(|global_config: &Weak<GlobalConfig>| {
            let mut result = GlobalConfig {
                hermit_dir: hermit_dir.to_path_buf(),
                all_detected_tags: detect_builtin_tags(),
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
                for tag in config.config_items().flat_map(|c| c.requires().iter()) {
                    log::debug!("Adding required tag: {tag}");
                    result.all_required_tags.insert(tag.clone());
                }
                for (k, v) in &config.snippets {
                    if result.all_snippets.contains_key(&k.to_lowercase()) {
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
                    log::debug!("Adding source {k}: {v}");
                    result.all_snippets.insert(k.to_lowercase(), v.clone());
                }
                for (k, v) in &config.detectors {
                    if result.all_detectors.contains_key(&k.to_lowercase()) {
                        crate::error!(
                            "Duplicate detector key '{}' in config file: {}",
                            k,
                            config.path.display()
                        );
                        errors.push(ConfigError::DuplicateSource(
                            k.to_string(),
                            config.path.clone(),
                        ));
                        continue;
                    }
                    log::debug!("Adding detector {k}: {v:?}");
                    result.all_detectors.insert(k.to_lowercase(), v.clone());
                }
                // Collect profiles (error on duplicate, lower-case, dedup tags)
                for (profile, tags) in &config.profiles {
                    let profile_lc = profile.to_lowercase();
                    log::debug!("Adding profile {profile_lc}: {tags:?}");
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
                let relative_path = path.strip_prefix(hermit_dir).unwrap_or(path);
                let relative_path_str = relative_path.to_string_lossy().to_string();
                result.subconfigs.insert(relative_path_str, config);
            }
            match get_detected_tags(&result) {
                Ok(custom_detected) => result.all_detected_tags.extend(custom_detected),
                Err(e) => {
                    crate::error!("Custom detector caused error: {e}");
                }
            }
            log::debug!("Detected tags: {:?}", result.all_detected_tags);
            result
        }))
    }

    pub fn hermit_dir(&self) -> &Path {
        &self.hermit_dir
    }

    pub fn all_required_tags(&self) -> &BTreeSet<RequireTag> {
        &self.all_required_tags
    }

    pub fn all_detected_tags(&self) -> &BTreeSet<Tag> {
        &self.all_detected_tags
    }

    pub fn all_profiles(&self) -> impl IntoIterator<Item = (&String, &BTreeSet<Tag>)> {
        self.all_profiles.iter()
    }

    pub fn all_detectors(&self) -> impl IntoIterator<Item = (&String, &DetectorConfig)> {
        self.all_detectors.iter()
    }

    pub fn subconfigs(&self) -> impl IntoIterator<Item = (&String, &ArcHermitConfig)> {
        self.subconfigs.iter()
    }

    pub fn get_snippet(&self, key: &str) -> Option<&String> {
        self.all_snippets.get(key)
    }

    pub fn get_tags_for_profile(&self, profile: &str) -> Result<BTreeSet<Tag>, ApplyError> {
        let profile = profile.to_lowercase();
        if let Some(tags) = self.all_profiles.get(&profile) {
            Ok(tags.clone())
        } else {
            Err(ApplyError::ProfileNotFound(profile))
        }
    }

    pub fn get_active_tags(
        &self,
        cli_tags: &[String],
        cli_profile: &Option<String>,
    ) -> Result<BTreeSet<Tag>, ConfigError> {
        let mut active_tags = self.all_detected_tags.clone();
        for tag in cli_tags {
            let tag = tag.split(',');
            for t in tag {
                let t = t.trim();
                if !t.is_empty() {
                    let cli_tag = Tag::new(t, Source::CommandLine);
                    if self
                        .all_required_tags
                        .iter()
                        .any(|r| r.name() == cli_tag.name())
                    {
                        active_tags.insert(cli_tag);
                    } else {
                        return Err(ConfigError::TagNotFound(t.to_string()));
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

    pub fn root_config(&self) -> Option<&ArcHermitConfig> {
        let root_path = self.hermit_dir.join(CONF_FILE_NAME);
        self.subconfigs
            .get(&root_path.to_string_lossy().to_string())
    }
}

pub fn load_hermit_config<P: AsRef<Path>>(
    path: P,
    global_config: Weak<GlobalConfig>,
) -> Result<Arc<HermitConfig>, ConfigError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigError::Io(e, path.as_ref().to_path_buf()))?;
    let config: HermitConfig = toml::from_str(&content)
        .map_err(|e| ConfigError::DeserializeToml(e, path.as_ref().to_path_buf()))?;
    let mut config = config.update_path(path);
    config.global_cfg = global_config;
    Ok(Arc::new(config))
}

pub fn load_hermit_config_editable<P: AsRef<Path>>(path: P) -> Result<DocumentMut, ConfigError> {
    let content = std::fs::read_to_string(path.as_ref())
        .map_err(|e| ConfigError::Io(e, path.as_ref().to_path_buf()))?;
    content
        .parse::<DocumentMut>()
        .map_err(|e| ConfigError::DeserializeDocumentToml(e, path.as_ref().to_path_buf()))
}

pub fn find_hermit_files(root: &Path) -> Vec<PathBuf> {
    let mut result = Vec::new();
    if root.is_file() && root.file_name().is_some_and(|f| f == CONF_FILE_NAME) {
        result.push(root.to_path_buf());
    } else if root.is_dir()
        && let Ok(entries) = std::fs::read_dir(root)
    {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                result.extend(find_hermit_files(&path));
            } else if path.file_name().is_some_and(|f| f == CONF_FILE_NAME) {
                result.push(path);
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_handlebar_snippets() {
        let mut hermit_cfg = None;
        let _global = Arc::new_cyclic(|weak| {
            let mut hermit = HermitConfig::create_new(Path::new("bla/hermit.toml"), weak.clone());
            hermit
                .snippets
                .insert("echo1".to_string(), "echo 1".to_string());
            hermit.snippets.insert(
                "echo2".to_string(),
                "echo 2;{{ snippet echo1 }}".to_string(),
            );
            hermit.snippets.insert(
                "echo3".to_string(),
                "echo 3;{{ snippet echo2 }}".to_string(),
            );
            let hermit = Arc::new(hermit);
            hermit_cfg = Some(hermit.clone());
            let mut global = GlobalConfig::default();
            global
                .subconfigs
                .insert("bla/hermit.toml".to_string(), hermit);
            global
        });
        let Some(hermit_cfg) = hermit_cfg else {
            panic!("Failed to get cfg");
        };
        let snippet = hermit_cfg.get_snippet("echo1", &BTreeMap::new()).unwrap();
        assert_eq!(snippet, "echo 1");
        let snippet = hermit_cfg.get_snippet("echo2", &BTreeMap::new()).unwrap();
        assert_eq!(snippet, "echo 2;echo 1");
        let snippet = hermit_cfg.get_snippet("echo3", &BTreeMap::new()).unwrap();
        assert_eq!(snippet, "echo 3;echo 2;echo 1");
    }
}
