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
    #[serde(flatten)]
    pub source: FileOrText,
    #[serde(default, skip_serializing_if = "PreprocessingType::is_default")]
    pub pre_processing: PreprocessingType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rendered_file: Option<PathBuf>,
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

    pub fn normalize(&self, cfg: &HermitConfig) -> Result<Self, PatchActionError> {
        if self.normalized {
            return Ok(self.clone());
        }
        let src = match &self.source {
            FileOrText::File { file } => {
                let src = match cfg.expand_directory(&file) {
                    Ok(path) => path,
                    Err(e) => return Err(PatchActionError::Io(std::io::Error::other(e))),
                };
                let src = if src.is_absolute() {
                    src.clone()
                } else {
                    cfg.directory().join(&src)
                };
                src.canonicalize()?
            }
            FileOrText::Text { text } => {
                let temp_dir = BASE_DIRS
                    .data_dir()
                    .join("hermitgrab")
                    .join("patch_sources");
                std::fs::create_dir_all(&temp_dir)?;
                let contents = match &self.pre_processing {
                    PreprocessingType::Handlebars => {
                        cfg.render_handlebars(text, &BTreeMap::new())?
                    }
                    PreprocessingType::None => text.clone(),
                };
                let temp_file_path = temp_dir.join(format!(
                    "source_{}.txt",
                    self.rendered_file.as_ref().map_or_else(
                        || blake3::hash(contents.as_bytes()).to_string(),
                        |f| f.display().to_string()
                    )
                ));

                std::fs::write(&temp_file_path, contents)?;
                temp_file_path.canonicalize()?
            }
        };

        let rel_path = src
            .strip_prefix(cfg.directory())
            .unwrap_or(&src)
            .to_string_lossy()
            .to_string();
        Ok(Self {
            source: FileOrText::File { file: src },
            rel_path,
            pre_processing: PreprocessingType::default(),
            rendered_file: None,
            content_type: ContentType::default(),
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
        let dst = match cfg.expand_directory(&patch.target) {
            Ok(path) => path,
            Err(e) => return Err(PatchActionError::Io(std::io::Error::other(e))),
        };
        let rel_dst = dst
            .strip_prefix(BASE_DIRS.home_dir())
            .unwrap_or(&dst)
            .to_string_lossy()
            .to_string();
        let requires = patch.get_all_requires(cfg);
        Ok(Self {
            src: patch.source.normalize(cfg)?,
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
            "{} {} with {}",
            self.patch_type, self.rel_dst, self.src.rel_path
        )
    }

    fn long_description(&self) -> String {
        match &self.src.source {
            FileOrText::File { file } => format!(
                "{} {} with file {}",
                self.patch_type,
                self.dst.display(),
                file.display()
            ),
            FileOrText::Text { text } => {
                if text.len() > 30 {
                    let snippet = &text[..30];
                    return format!(
                        "{} {} with '{}…'",
                        self.patch_type,
                        self.dst.display(),
                        snippet.replace('\n', "\\n")
                    );
                } else {
                    format!(
                        "{} {} with '{}'",
                        self.patch_type,
                        self.dst.display(),
                        text.replace("\n", "\\n")
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
                merge_json(&self.src.file(), &self.dst)?;
                observer.action_progress(&self.id(), 1, 1, "Merge completed");
                Ok(())
            }
            PatchType::JsonPatch => {
                patch_json(&self.src.file(), &self.dst)?;
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

pub fn merge_json(src: &Path, dst: &Path) -> Result<ActionOutput, PatchActionError> {
    let (merge_content, _) = content_and_extension(src)?;
    let (mut dst_content, lower_case_ext) = content_and_extension(dst)?;
    json_patch::merge(&mut dst_content, &merge_content);
    let updated_dst = to_content(dst_content, &lower_case_ext)?;
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

pub fn patch_json(src: &Path, dst: &Path) -> Result<ActionOutput, PatchActionError> {
    let (merge_content, _) = content_and_extension(src)?;
    let patch: json_patch::Patch = serde_json::from_value(merge_content)?;
    let (mut dst_json, lower_case_ext) = content_and_extension(dst)?;
    json_patch::patch(&mut dst_json, &patch)?;
    let updated_dst = to_content(dst_json, &lower_case_ext)?;
    write_contents(dst, updated_dst)?;
    Ok(ActionOutput::new_stdout(format!(
        "Merged the contents of {src:?} into {dst:?}"
    )))
}

fn content_and_extension(
    dst: &Path,
) -> Result<(serde_json::Value, Option<String>), PatchActionError> {
    let dst_content = if dst.exists() {
        std::fs::read_to_string(dst).map_err(PatchActionError::Io)?
    } else {
        "".to_string()
    };
    let lower_case_ext = dst
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|s| s.to_lowercase());
    Ok((parse_file(dst_content, &lower_case_ext)?, lower_case_ext))
}

fn to_content(
    dst_json: serde_json::Value,
    extension: &Option<String>,
) -> Result<String, PatchActionError> {
    match extension.as_deref() {
        Some("yaml") | Some("yml") => {
            let yaml = serde_yaml_ng::to_string(&dst_json)?;
            Ok(yaml)
        }
        Some("toml") => {
            let toml = toml::to_string_pretty(&dst_json)?;
            Ok(toml)
        }
        _ => {
            let json = serde_json::to_string_pretty(&dst_json)?;
            Ok(json)
        }
    }
}

fn parse_file(
    dst_content: String,
    extension: &Option<String>,
) -> Result<serde_json::Value, PatchActionError> {
    match extension.as_deref() {
        Some("yaml") | Some("yml") => {
            let yaml: serde_yaml_ng::Value = serde_yaml_ng::from_str(&dst_content)?;
            Ok(serde_json::to_value(yaml)?)
        }
        Some("toml") => {
            let toml: toml::Value = toml::from_str(&dst_content)?;
            Ok(serde_json::to_value(toml)?)
        }
        _ => {
            let value = jsonc_parser::parse_to_serde_value(&dst_content, &ParseOptions::default())?;
            if let Some(value) = value {
                return Ok(value);
            }
            Ok(serde_json::from_str(&dst_content)?)
        }
    }
}
