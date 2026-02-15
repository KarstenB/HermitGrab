// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use jsonc_parser::ParseOptions;
use serde::Serialize;

use crate::action::{
    Action, ActionObserver, ActionOutput, ContentType, FileOrText, PreprocessingType, SourceSpec,
    Status,
};
use crate::config::{ArcHermitConfig, ConfigItem, PatchConfig, PatchType};
use crate::file_ops::dirs::BASE_DIRS;
use crate::hermitgrab_error::{ActionError, PatchActionError};
use crate::{HermitConfig, RequireTag};

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
            src: patch.source.normalize::<PatchActionError>(cfg, &dst)?,
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

    fn execute(
        &self,
        observer: &Arc<impl ActionObserver>,
        cfg: &ArcHermitConfig,
    ) -> Result<(), ActionError> {
        observer.action_progress(&self.id(), 0, 2, "Applying patch");
        if matches!(self.src.pre_processing, PreprocessingType::Handlebars) {
            observer.action_progress(&self.id(), 1, 2, "Rendering source with Handlebars");
            let content =
                std::fs::read_to_string(self.src.file()).map_err(|e| PatchActionError::Io(e))?;
            let rendered_content = cfg
                .render_handlebars(&content, &BTreeMap::new())
                .map_err(|e| PatchActionError::Render(e))?;
            std::fs::write(self.src.file(), rendered_content)
                .map_err(|e| PatchActionError::Io(e))?;
        } else {
            observer.action_progress(&self.id(), 1, 2, "No preprocessing required");
        }
        match self.patch_type {
            PatchType::JsonMerge => {
                merge_json(&self.src.file(), &self.dst, &self.src.content_type)?;
                observer.action_progress(&self.id(), 2, 2, "Merge completed");
                Ok(())
            }
            PatchType::JsonPatch => {
                patch_json(&self.src.file(), &self.dst, &self.src.content_type)?;
                observer.action_progress(&self.id(), 2, 2, "Patch completed");
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
