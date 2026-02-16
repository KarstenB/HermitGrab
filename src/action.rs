// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::HashMap;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use enum_dispatch::enum_dispatch;
use handlebars::RenderError;
use serde::{Deserialize, Serialize};
use xxhash_rust::xxh3::Xxh3;

use crate::config::ArcHermitConfig;
use crate::file_ops::dirs::BASE_DIRS;
use crate::hermitgrab_error::ActionError;
use crate::{HermitConfig, RequireTag};
pub mod install;
pub mod link;
pub mod patch;

#[derive(Debug, Clone, Default, Serialize)]
pub struct ActionOutput {
    pub output_order: Vec<String>,
    standard_output: HashMap<String, String>,
    error_output: HashMap<String, String>,
}

impl ActionOutput {
    pub fn new_stdout(stdout: String) -> Self {
        let mut output = Self::default();
        output.standard_output.insert("stdout".to_string(), stdout);
        output.output_order.push("stdout".to_string());
        output
    }

    fn add(&mut self, name: &str, stdout: &str, stderr: &str) {
        if !stdout.is_empty() {
            self.standard_output
                .insert(name.to_string(), stdout.to_string());
            if !self.output_order.contains(&name.to_string()) {
                self.output_order.push(name.to_string());
            }
        }
        if !stderr.is_empty() {
            self.error_output
                .insert(name.to_string(), stderr.to_string());
            if !self.output_order.contains(&name.to_string()) {
                self.output_order.push(name.to_string());
            }
        }
    }

    pub fn is_empty(&self) -> bool {
        self.output_order.is_empty()
    }
}

impl IntoIterator for ActionOutput {
    type Item = (String, Option<String>, Option<String>);
    type IntoIter = Box<dyn Iterator<Item = Self::Item>>;

    fn into_iter(self) -> Self::IntoIter {
        Box::new(self.output_order.into_iter().map(move |key| {
            (
                key.clone(),
                self.standard_output.get(&key).cloned(),
                self.error_output.get(&key).cloned(),
            )
        }))
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Default, Clone, Copy)]
pub enum PreprocessingType {
    #[default]
    None,
    Handlebars,
}

impl PreprocessingType {
    pub fn is_default(&self) -> bool {
        *self == PreprocessingType::default()
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Clone)]
#[serde(untagged)]
pub enum FileOrText {
    File { file: PathBuf },
    Text { text: String },
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Default, Clone, Copy)]
pub enum ContentType {
    #[default]
    Auto,
    Json,
    Yaml,
    Toml,
    Unknown,
}
impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Auto => write!(f, "auto"),
            ContentType::Json => write!(f, "json"),
            ContentType::Yaml => write!(f, "yaml"),
            ContentType::Toml => write!(f, "toml"),
            ContentType::Unknown => write!(f, "unknown"),
        }
    }
}

impl ContentType {
    pub fn is_default(&self) -> bool {
        *self == ContentType::default()
    }
}

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Clone)]
pub struct SourceSpec {
    /// The source can either be a file path or a text string.
    /// If it's a file path, it will be read and used as the content.
    /// If it's a text string, it will be used directly as the content.
    #[serde(flatten)]
    pub source: FileOrText,
    /// The pre-processing type to apply to the source content before usage.
    #[serde(default, skip_serializing_if = "PreprocessingType::is_default")]
    pub pre_processing: PreprocessingType,
    /// This is the output file path after pot-processing the source content.
    /// If not set a temporary file will be created for text sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered_file: Option<PathBuf>,
    /// The content type of the source, which can be used to determine how to parse it.
    /// On auto content type will be determined based on the src or as fall back the destination file extension.
    ///
    /// NOTE: This is ignored in link actions.
    #[serde(default, skip_serializing_if = "ContentType::is_default")]
    pub content_type: ContentType,
    // internal fields for providing references
    #[serde(skip)]
    rel_path: String,
    #[serde(skip)]
    normalized: bool,
}

impl SourceSpec {
    pub fn raw_path(path: PathBuf) -> Self {
        let rel_path = path.display().to_string();
        Self {
            source: FileOrText::File { file: path },
            rel_path,
            pre_processing: PreprocessingType::default(),
            rendered_file: None,
            content_type: ContentType::default(),
            normalized: false,
        }
    }

    pub fn normalize<E>(&self, cfg: &HermitConfig, dst: &Path) -> Result<Self, E>
    where
        E: From<std::io::Error>,
        E: From<RenderError>,
    {
        if self.normalized {
            return Ok(self.clone());
        }
        let src = match &self.source {
            FileOrText::File { file } => {
                let src = cfg.canonicalize_source_path::<E>(file, true)?;
                if let Some(render_file) = self.rendered_file.as_ref() {
                    let pre_render = cfg.canonicalize_source_path::<E>(render_file, false)?;
                    if src != pre_render {
                        std::fs::create_dir_all(
                            pre_render.parent().unwrap_or_else(|| cfg.directory()),
                        )?;
                        std::fs::copy(&src, &pre_render)?;
                    }
                    pre_render
                } else {
                    src
                }
            }
            FileOrText::Text { text } => {
                let temp_file_path = if let Some(rendered_file) = &self.rendered_file {
                    cfg.canonicalize_source_path::<E>(rendered_file, false)?
                } else {
                    let temp_dir = BASE_DIRS.data_dir().join("hermitgrab").join("sources");
                    temp_dir.join(format!(
                        "text_{}.{}",
                        blake3::hash(text.as_bytes()).to_string(),
                        dst.extension()
                            .and_then(|ext| ext.to_str())
                            .unwrap_or("rendered")
                    ))
                };
                std::fs::create_dir_all(
                    temp_file_path.parent().unwrap_or_else(|| cfg.directory()),
                )?;
                std::fs::write(&temp_file_path, text)?;
                cfg.canonicalize_source_path::<E>(&temp_file_path, true)?
            }
        };
        let content_type = if matches!(self.content_type, ContentType::Auto) {
            src.extension()
                .or_else(|| dst.extension())
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_lowercase())
                .map(|ext| match ext.as_str() {
                    "json" | "jsonc" => ContentType::Json,
                    "yaml" | "yml" => ContentType::Yaml,
                    "toml" => ContentType::Toml,
                    _ => ContentType::Unknown,
                })
                .unwrap_or(ContentType::Unknown)
        } else {
            self.content_type
        };

        let rel_path = src
            .strip_prefix(cfg.directory())
            .unwrap_or(&src)
            .display()
            .to_string();
        Ok(Self {
            source: FileOrText::File { file: src },
            rel_path,
            pre_processing: self.pre_processing,
            rendered_file: None,
            content_type: content_type,
            normalized: true,
        })
    }

    pub fn file(&self) -> &PathBuf {
        match &self.source {
            FileOrText::File { file } => file,
            FileOrText::Text { text: _ } => {
                panic!("SourceSpec with text source does not have a file path")
            }
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub enum Status {
    Ok(String),
    NotOk(String),
    Error(String),
    NotSupported,
}

pub trait ActionObserver {
    fn action_started(&self, action: &ArcAction);
    fn action_output(&self, action_id: &str, output: &ActionOutput);
    fn action_progress(&self, action_id: &str, current: u64, total: u64, msg: &str);
    fn action_finished(&self, action: &ArcAction, result: &Result<(), ActionError>);
}

#[enum_dispatch]
pub trait Action: Send + Sync {
    fn short_description(&self) -> String;
    fn long_description(&self) -> String;
    fn get_output(&self) -> Option<ActionOutput> {
        None
    }
    fn requires(&self) -> &[RequireTag];
    fn id(&self) -> String;
    fn execute(
        &self,
        observer: &Arc<impl ActionObserver>,
        cfg: &ArcHermitConfig,
    ) -> Result<(), ActionError>;
    fn get_status(&self, cfg: &HermitConfig, quick: bool) -> Status;
    fn get_order(&self) -> u64;
}

pub fn id_from_hash<T: Hash>(item: &T) -> String {
    let mut hash = Xxh3::new();
    item.hash(&mut hash);
    format!("{}:{:016x}", std::any::type_name::<T>(), hash.finish())
}

#[enum_dispatch(Action)]
#[derive(Debug, Hash, PartialEq, Serialize)]
pub enum Actions {
    Install(install::InstallAction),
    Link(link::LinkAction),
    Patch(patch::PatchAction),
}
pub type ArcAction = std::sync::Arc<Actions>;
