// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use jsonc_parser::ParseOptions;
use serde::{Deserialize, Serialize};

use crate::action::{Action, ActionObserver, ActionOutput, Status};
use crate::config::{ConfigItem, PatchConfig, PatchType};
use crate::file_ops::dirs::BASE_DIRS;
use crate::hermitgrab_error::{ActionError, PatchActionError};
use crate::{HermitConfig, RequireTag};

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Default, Clone, Copy)]
pub enum ContentType {
    #[default]
    Auto,
    Json,
    Yaml,
    Toml,
}
impl std::fmt::Display for ContentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ContentType::Auto => write!(f, "auto"),
            ContentType::Json => write!(f, "json"),
            ContentType::Yaml => write!(f, "yaml"),
            ContentType::Toml => write!(f, "toml"),
        }
    }
}

impl ContentType {
    pub fn is_default(&self) -> bool {
        *self == ContentType::default()
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

#[derive(Serialize, Deserialize, Debug, Hash, PartialEq, Clone)]
pub struct SourceSpec {
    /// The source can either be a file path or a text string.
    /// If it's a file path, it will be read and used as the patch content.
    /// If it's a text string, it will be used directly as the patch content.
    #[serde(flatten)]
    pub source: FileOrText,
    /// The pre-processing type to apply to the source content before using it as a patch.
    #[serde(default, skip_serializing_if = "PreprocessingType::is_default")]
    pub pre_processing: PreprocessingType,
    /// This is the output file path after pot-processing the source content.
    /// If not set a temporary file will be created for text sources.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered_file: Option<PathBuf>,
    /// The content type of the source, which can be used to determine how to parse it.
    /// On auto content type will be determined based on the file extension.
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

    pub fn normalize(&self, cfg: &HermitConfig, dst: &Path) -> Result<Self, PatchActionError> {
        if self.normalized {
            return Ok(self.clone());
        }
        let src = match &self.source {
            FileOrText::File { file } => cfg.canonicalize_path::<PatchActionError>(file)?,
            FileOrText::Text { text } => {
                let contents = match &self.pre_processing {
                    PreprocessingType::Handlebars => {
                        cfg.render_handlebars(text, &BTreeMap::new())?
                    }
                    PreprocessingType::None => text.clone(),
                };
                let temp_file_path = if let Some(rendered_file) = &self.rendered_file {
                    rendered_file.to_owned()
                } else {
                    let temp_dir = BASE_DIRS
                        .data_dir()
                        .join("hermitgrab")
                        .join("patch_sources");
                    temp_dir.join(format!(
                        "source_{}.{}",
                        blake3::hash(contents.as_bytes()).to_string(),
                        dst.extension()
                            .and_then(|ext| ext.to_str())
                            .unwrap_or("rendered")
                    ))
                };
                let temp_file_path = cfg.canonicalize_path::<PatchActionError>(&temp_file_path)?;
                std::fs::create_dir_all(
                    temp_file_path.parent().unwrap_or_else(|| cfg.directory()),
                )?;
                std::fs::write(&temp_file_path, contents)?;
                temp_file_path
            }
        };
        let content_type = if matches!(self.content_type, ContentType::Auto) {
            src.extension()
                .and_then(|ext| ext.to_str())
                .map(|s| s.to_lowercase())
                .and_then(|ext| match ext.as_str() {
                    "json" | "jsonc" => Some(ContentType::Json),
                    "yaml" | "yml" => Some(ContentType::Yaml),
                    "toml" => Some(ContentType::Toml),
                    _ => None,
                })
                .unwrap_or(ContentType::Json)
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
            pre_processing: PreprocessingType::None,
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

#[derive(Serialize, Debug, Hash, PartialEq)]
pub struct PatchAction {
    #[serde(skip)]
    rel_dst: String,
    src: SourceSpec,
    dst: PathBuf,
    patch_type: PatchType,
    order: u64,
    requires: Vec<RequireTag>,
}

impl PatchAction {
    pub fn new(patch: &PatchConfig, cfg: &HermitConfig) -> Result<Self, PatchActionError> {
        let dst = cfg.expand_directory(&patch.target)?;
        let rel_dst = dst
            .strip_prefix(BASE_DIRS.home_dir())
            .unwrap_or(&dst)
            .to_string_lossy()
            .to_string();
        let requires = patch.get_all_requires(cfg);
        Ok(Self {
            src: patch.source.normalize(cfg, &dst)?,
            dst,
            rel_dst,
            order: patch.total_order(cfg),
            patch_type: patch.patch_type.clone(),
            requires: requires.into_iter().collect(),
        })
    }
}

impl Action for PatchAction {
    fn short_description(&self) -> String {
        format!(
            "{} {} with {} [{}]",
            self.patch_type, self.rel_dst, self.src.rel_path, self.src.content_type
        )
    }

    fn long_description(&self) -> String {
        match &self.src.source {
            FileOrText::File { file } => format!(
                "{} {} with file {} [{}]",
                self.patch_type,
                self.dst.display(),
                file.display(),
                self.src.content_type
            ),
            FileOrText::Text { text } => {
                if text.len() > 30 {
                    let snippet = &text[..30];
                    return format!(
                        "{} {} with '{}…' [{}]",
                        self.patch_type,
                        self.dst.display(),
                        snippet.replace('\n', "\\n"),
                        self.src.content_type
                    );
                } else {
                    format!(
                        "{} {} with '{}' [{}]",
                        self.patch_type,
                        self.dst.display(),
                        text.replace("\n", "\\n"),
                        self.src.content_type
                    )
                }
            }
        }
    }

    fn requires(&self) -> &[RequireTag] {
        &self.requires
    }

    fn execute(&self, observer: &Arc<impl ActionObserver>) -> Result<(), ActionError> {
        observer.action_progress(&self.id(), 0, 1, "Applying patch");
        match self.patch_type {
            PatchType::JsonMerge => {
                merge_json(&self.src.file(), &self.dst, &self.src.content_type)?;
                observer.action_progress(&self.id(), 1, 1, "Merge completed");
                Ok(())
            }
            PatchType::JsonPatch => {
                patch_json(&self.src.file(), &self.dst, &self.src.content_type)?;
                observer.action_progress(&self.id(), 1, 1, "Patch completed");
                Ok(())
            }
        }
    }

    fn id(&self) -> String {
        format!(
            "PatchAction:{}:{}:{}",
            self.src.rel_path,
            self.rel_dst,
            self.requires.iter().join(",")
        )
    }

    fn get_status(&self, _cfg: &HermitConfig, _quick: bool) -> Status {
        Status::NotSupported
    }

    fn get_order(&self) -> u64 {
        self.order
    }
}

pub fn merge_json(
    src: &Path,
    dst: &Path,
    content_type: &ContentType,
) -> Result<ActionOutput, PatchActionError> {
    let merge_content = content_and_extension(src, content_type)?;
    let mut dst_content = content_and_extension(dst, content_type)?;
    json_patch::merge(&mut dst_content, &merge_content);
    let updated_dst = to_content(dst_content, content_type)?;
    write_contents(dst, updated_dst)?;
    Ok(ActionOutput::new_stdout(format!(
        "Merged the contents of {src:?} into {dst:?}"
    )))
}

fn write_contents(dst: &Path, updated_dst: String) -> Result<(), PatchActionError> {
    let dst_dir = dst.parent().expect("Failed to get parent directory");
    if !dst_dir.exists() {
        std::fs::create_dir_all(dst_dir)?;
    }
    std::fs::write(dst, updated_dst)?;
    Ok(())
}

pub fn patch_json(
    src: &Path,
    dst: &Path,
    content_type: &ContentType,
) -> Result<ActionOutput, PatchActionError> {
    let merge_content = content_and_extension(src, content_type)?;
    let patch: json_patch::Patch = serde_json::from_value(merge_content)?;
    let mut dst_json = content_and_extension(dst, content_type)?;
    json_patch::patch(&mut dst_json, &patch)?;
    let updated_dst = to_content(dst_json, &content_type)?;
    write_contents(dst, updated_dst)?;
    Ok(ActionOutput::new_stdout(format!(
        "Merged the contents of {src:?} into {dst:?}"
    )))
}

fn content_and_extension(
    file: &Path,
    content_type: &ContentType,
) -> Result<serde_json::Value, PatchActionError> {
    let dst_content = if file.exists() {
        std::fs::read_to_string(file).map_err(PatchActionError::Io)?
    } else {
        "".to_string()
    };
    Ok(parse_file(dst_content, &content_type)?)
}

fn to_content(
    dst_json: serde_json::Value,
    content_type: &ContentType,
) -> Result<String, PatchActionError> {
    match content_type {
        ContentType::Yaml => Ok(serde_yaml_ng::to_string(&dst_json)?),
        ContentType::Toml => Ok(toml::to_string_pretty(&dst_json)?),
        ContentType::Json => Ok(serde_json::to_string_pretty(&dst_json)?),
        _ => {
            panic!(
                "Unsupported content type for serialization: {:?}",
                content_type
            )
        }
    }
}

fn parse_file(
    dst_content: String,
    content_type: &ContentType,
) -> Result<serde_json::Value, PatchActionError> {
    match content_type {
        ContentType::Yaml => {
            let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&dst_content)?;
            Ok(serde_json::to_value(yaml)?)
        }
        ContentType::Toml => {
            let toml: toml::Value = toml::from_str(&dst_content)?;
            Ok(serde_json::to_value(toml)?)
        }
        ContentType::Json => {
            let value = jsonc_parser::parse_to_serde_value(&dst_content, &ParseOptions::default())?;
            if let Some(value) = value {
                return Ok(value);
            }
            Ok(serde_json::from_str(&dst_content)?)
        }
        _ => {
            panic!("Unsupported content type for parsing: {:?}", content_type)
        }
    }
}
