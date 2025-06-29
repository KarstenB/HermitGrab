use std::{
    collections::BTreeSet,
    path::{Path, PathBuf},
};

use jsonc_parser::ParseOptions;

use crate::{
    HermitConfig, RequireTag,
    action::{Action, ActionOutput},
    config::{PatchType, Tag},
    hermitgrab_error::{ActionError, PatchActionError},
    user_home,
};

pub struct PatchAction {
    id: String,
    rel_src: String,
    rel_dst: String,
    src: PathBuf,
    dst: PathBuf,
    patch_type: PatchType,
    requires: Vec<RequireTag>,
    provides: Vec<Tag>,
}

impl PatchAction {
    pub(crate) fn new(
        id: String,
        config_dir: &Path,
        src: PathBuf,
        dst: PathBuf,
        requires: BTreeSet<RequireTag>,
        provides: BTreeSet<Tag>,
        patch_type: PatchType,
        cfg: &HermitConfig,
    ) -> Self {
        let rel_src = src
            .strip_prefix(config_dir)
            .unwrap_or(&src)
            .to_string_lossy()
            .to_string();
        let dst = cfg.global_config().expand_directory(&dst);
        let rel_dst = dst
            .strip_prefix(user_home())
            .unwrap_or(&dst)
            .to_string_lossy()
            .to_string();

        Self {
            id,
            src,
            rel_src,
            dst,
            rel_dst,
            patch_type,
            requires: requires.into_iter().collect(),
            provides: provides.into_iter().collect(),
        }
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
    fn provides(&self) -> &[Tag] {
        &self.provides
    }
    fn id(&self) -> String {
        self.id.clone()
    }

    fn execute(&self) -> Result<(), ActionError> {
        match self.patch_type {
            PatchType::JsonMerge => {
                merge_json(&self.src, &self.dst)?;
                Ok(())
            }
            PatchType::JsonPatch => {
                patch_json(&self.src, &self.dst)?;
                Ok(())
            }
        }
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
        std::fs::read_to_string(dst).map_err(PatchActionError::IoError)?
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
            let yaml = serde_yml::to_string(&dst_json)?;
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
            let yaml: serde_yml::Value = serde_yml::from_str(&dst_content)?;
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
