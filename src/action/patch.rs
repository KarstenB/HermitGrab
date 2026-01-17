// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use jsonc_parser::ParseOptions;
use serde::Serialize;

use crate::action::{Action, ActionObserver, ActionOutput, Status};
use crate::config::{ConfigItem, PatchConfig, PatchType};
use crate::file_ops::dirs::BASE_DIRS;
use crate::hermitgrab_error::{ActionError, PatchActionError};
use crate::{HermitConfig, RequireTag};

#[derive(Serialize, Debug, Hash, PartialEq)]
pub struct PatchAction {
    #[serde(skip)]
    rel_src: String,
    #[serde(skip)]
    rel_dst: String,
    src: PathBuf,
    dst: PathBuf,
    patch_type: PatchType,
    order: u64,
    requires: Vec<RequireTag>,
}

impl PatchAction {
    pub fn new(patch: &PatchConfig, cfg: &HermitConfig) -> Result<Self, std::io::Error> {
        let src = if patch.source.is_absolute() {
            patch.source.clone()
        } else {
            cfg.directory().join(&patch.source)
        };
        let src = src.canonicalize()?;
        let rel_src = src
            .strip_prefix(cfg.directory())
            .unwrap_or(&patch.source)
            .to_string_lossy()
            .to_string();
        let dst = cfg.expand_directory(&patch.target);
        let rel_dst = dst
            .strip_prefix(BASE_DIRS.home_dir())
            .unwrap_or(&dst)
            .to_string_lossy()
            .to_string();
        let requires = patch.get_all_requires(cfg);
        Ok(Self {
            src,
            rel_src,
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
        format!("{} {} with {}", self.patch_type, self.rel_dst, self.rel_src)
    }

    fn long_description(&self) -> String {
        format!(
            "{} {} with {}",
            self.patch_type,
            self.dst.display(),
            self.src.display()
        )
    }

    fn requires(&self) -> &[RequireTag] {
        &self.requires
    }

    fn execute(&self, observer: &Arc<impl ActionObserver>) -> Result<(), ActionError> {
        observer.action_progress(&self.id(), 0, 1, "Applying patch");
        match self.patch_type {
            PatchType::JsonMerge => {
                merge_json(&self.src, &self.dst)?;
                observer.action_progress(&self.id(), 1, 1, "Merge completed");
                Ok(())
            }
            PatchType::JsonPatch => {
                patch_json(&self.src, &self.dst)?;
                observer.action_progress(&self.id(), 1, 1, "Patch completed");
                Ok(())
            }
        }
    }

    fn id(&self) -> String {
        format!(
            "PatchAction:{}:{}:{}",
            self.rel_src,
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
