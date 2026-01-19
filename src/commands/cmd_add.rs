// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::collections::BTreeSet;
use std::iter::FromIterator;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use itertools::Itertools;
use serde::Serialize;
use toml_edit::{Array, ArrayOfTables, Formatted, Item, Table, Value};

use crate::action::patch::SourceSpec;
use crate::common_cli::{hint, prompt};
use crate::config::{
    CONF_FILE_NAME, FallbackOperation, FullSpecOrPath, GlobalConfig, PatchConfig, PatchType, Tag,
    load_hermit_config_editable,
};
use crate::file_ops::copy;
use crate::file_ops::dirs::BASE_DIRS;
use crate::hermitgrab_error::AddError;
use crate::{
    HermitConfig, InstallConfig, LinkConfig, LinkType, RequireTag, choice, error, info, success,
};

pub fn add_config(
    config_dir: &Path,
    required_tags: &[RequireTag],
    links: &[LinkConfig],
    patches: &[PatchConfig],
    installs: &[InstallConfig],
    global_config: &Arc<GlobalConfig>,
    order: &Option<u64>,
) -> Result<(), AddError> {
    let config_dir = if config_dir.ends_with(CONF_FILE_NAME) {
        config_dir
            .parent()
            .expect("Failed to get parent directory")
            .to_path_buf()
    } else {
        config_dir.to_path_buf()
    };
    let config_dir = if config_dir.is_absolute() {
        config_dir.clone()
    } else {
        global_config.hermit_dir().join(config_dir)
    };
    let config_file = config_dir.join(CONF_FILE_NAME);
    if config_file.exists() {
        error!(
            "The configuration file {config_file:?} already exists. Please use a different directory or remove the existing file."
        );
        return Err(AddError::ConfigFileAlreadyExists(config_file));
    }
    let mut config = HermitConfig::default();
    info!("Creating a new configuration file at {config_file:?}");
    config.requires.extend(required_tags.to_vec());
    config.link.extend(links.to_vec());
    config.patch.extend(patches.to_vec());
    config.install.extend(installs.to_vec());
    config.order = *order;
    std::fs::create_dir_all(config_dir)?;
    config.save_to_file(&config_file)?;
    Ok(())
}

pub fn add_patch(
    config_dir: &Option<PathBuf>,
    source: &Path,
    patch_type: &PatchType,
    target: &Option<PathBuf>,
    required_tags: &[RequireTag],
    global_config: &Arc<GlobalConfig>,
    order: Option<u64>,
) -> Result<(), AddError> {
    let config_dir = if let Some(target_dir) = config_dir {
        let new_target = PathBuf::from(target_dir);
        if new_target.is_absolute() {
            new_target
        } else {
            global_config.hermit_dir().join(new_target)
        }
    } else {
        get_config_dir_interactive(source, global_config)?
    };
    let config_file = config_dir.join(CONF_FILE_NAME);
    let target = normalize_target(source, target)?;
    let source_filename: PathBuf = source
        .file_name()
        .ok_or(AddError::FileName)?
        .to_string_lossy()
        .to_string()
        .into();
    let file_entry = PatchConfig {
        source: FullSpecOrPath::FullSpec(SourceSpec::raw_path(source_filename.clone())),
        target,
        patch_type: patch_type.clone(),
        requires: BTreeSet::from_iter(required_tags.iter().cloned()),
        order,
    };
    if config_file.exists() {
        insert_into_existing(&config_file, &file_entry)?;
    } else {
        add_config(
            &config_dir,
            required_tags,
            &[],
            &[file_entry],
            &[],
            global_config,
            &None,
        )?;
    }
    copy(source, config_dir.join(source_filename).as_path())?;
    crate::success!("Added new patch to {config_file:?}");
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub fn add_link(
    config_dir: &Option<PathBuf>,
    source: &Path,
    link_type: &LinkType,
    target: &Option<PathBuf>,
    required_tags: &[RequireTag],
    fallback: &FallbackOperation,
    global_config: &Arc<GlobalConfig>,
    order: Option<u64>,
) -> Result<(), AddError> {
    let config_dir = if let Some(target_dir) = config_dir {
        let new_target = PathBuf::from(target_dir);
        if new_target.is_absolute() {
            new_target
        } else {
            global_config.hermit_dir().join(new_target)
        }
    } else {
        get_config_dir_interactive(source, global_config)?
    };
    let config_file = config_dir.join(CONF_FILE_NAME);
    let target = normalize_target(source, target)?;
    let source_filename: PathBuf = source
        .file_name()
        .ok_or(AddError::FileName)?
        .to_string_lossy()
        .to_string()
        .into();
    let file_entry = LinkConfig {
        source: source_filename.clone(),
        target,
        link: *link_type,
        requires: BTreeSet::from_iter(required_tags.iter().cloned()),
        fallback: *fallback,
        order,
    };
    if config_file.exists() {
        insert_into_existing(&config_file, &file_entry)?;
    } else {
        add_config(
            &config_dir,
            &[],
            &[file_entry],
            &[],
            &[],
            global_config,
            &None,
        )?;
    }
    copy(source, config_dir.join(source_filename).as_path())?;
    crate::success!("Added new link to {config_file:?}");
    Ok(())
}

fn normalize_target(source: &Path, target: &Option<PathBuf>) -> Result<PathBuf, AddError> {
    let target = if let Some(target) = target {
        let path = PathBuf::from(target);
        path.strip_prefix(BASE_DIRS.home_dir())
            .map(|x| x.to_path_buf())
            .unwrap_or(path)
    } else {
        source.strip_prefix(BASE_DIRS.home_dir())?.to_path_buf()
    };
    let target = if target.is_absolute() {
        target
    } else {
        PathBuf::from("~").join(target)
    };
    Ok(target)
}

fn get_config_dir_interactive(
    source: &Path,
    global_config: &Arc<GlobalConfig>,
) -> Result<PathBuf, AddError> {
    let absolute_source = source.canonicalize().unwrap_or(source.to_path_buf());
    if absolute_source.is_file() {
        info!(
            "This will add a link for the file: {}",
            absolute_source.display()
        );
    } else if absolute_source.is_dir() {
        info!(
            "This will add a link for the directory: {}",
            absolute_source.display()
        );
    } else {
        return Err(AddError::SourceNotFound(absolute_source));
    }
    hint(
        "To avoid being prompted about the directory, you can use the --config-dir command line option",
    );
    let relative_source = absolute_source
        .strip_prefix(BASE_DIRS.home_dir())
        .unwrap_or(&absolute_source);
    let last_segment_from_absolute = if absolute_source.is_dir() {
        absolute_source
            .file_name()
            .and_then(|f| f.to_str())
            .unwrap_or("")
    } else {
        absolute_source
            .parent()
            .and_then(|p| p.file_name().and_then(|f| f.to_str()))
            .unwrap_or("")
    };
    let deep_config_file = global_config
        .hermit_dir()
        .join(relative_source.parent().unwrap_or(relative_source))
        .join(CONF_FILE_NAME);
    let deep_config_display_path = deep_config_file
        .strip_prefix(global_config.hermit_dir())
        .unwrap_or(&deep_config_file)
        .display();
    let simple_config_file = global_config
        .hermit_dir()
        .join(last_segment_from_absolute)
        .join(CONF_FILE_NAME);
    let simple_config_display_path = simple_config_file
        .strip_prefix(global_config.hermit_dir())
        .unwrap_or(&simple_config_file)
        .display();
    if simple_config_file.exists() {
        choice!(
            "1. Add to existing config file: '{}'",
            simple_config_display_path
        );
    } else {
        choice!(
            "1. Create new config file: '{}'",
            simple_config_display_path
        );
    }
    if deep_config_file.exists() {
        choice!(
            "2. Add to existing config file: '{}'",
            deep_config_display_path
        );
    } else {
        choice!("2. Create new config file: '{}'", deep_config_display_path);
    }
    choice!("3. Use custom directory");
    choice!("4. Use the root HermitGrab directory");
    let choice = prompt("What do you want to do? Enter either 1, 2, 3 or 4: ")?;
    Ok(match choice.as_str() {
        "1" => simple_config_file.parent().unwrap().to_path_buf(),
        "2" => deep_config_file.parent().unwrap().to_path_buf(),
        "3" => {
            let custom_dir = prompt("Enter custom directory path: ")?;
            PathBuf::from(custom_dir)
        }
        "4" => global_config.hermit_dir().into(),
        _ => return Err(AddError::InvalidChoice),
    })
}

trait GetSourceAndTarget<'a> {
    fn source(&'a self) -> &'a Path;
    fn target(&'a self) -> &'a Path;
    fn entry_name(&self) -> &'static str;
}

impl<'a> GetSourceAndTarget<'a> for LinkConfig {
    fn source(&'a self) -> &'a Path {
        &self.source
    }

    fn target(&'a self) -> &'a Path {
        &self.target
    }
    fn entry_name(&self) -> &'static str {
        "link"
    }
}
impl<'a> GetSourceAndTarget<'a> for PatchConfig {
    fn source(&'a self) -> &'a Path {
        &self.source.path()
    }

    fn target(&'a self) -> &'a Path {
        &self.target
    }

    fn entry_name(&self) -> &'static str {
        "patch"
    }
}

fn insert_into_existing<'a, T: Serialize + GetSourceAndTarget<'a>>(
    config_file: &PathBuf,
    file_entry: &'a T,
) -> Result<(), AddError> {
    let entry_name = file_entry.entry_name();
    let table = to_table(file_entry)?;
    let mut config = load_hermit_config_editable(config_file)?;
    let files = config[entry_name].or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
    match files {
        Item::ArrayOfTables(arr) => {
            for entry in arr.iter() {
                let Item::Value(Value::String(ref source)) = entry["source"] else {
                    continue;
                };
                let Item::Value(Value::String(ref target)) = entry["target"] else {
                    continue;
                };
                let source_str = PathBuf::from(source.value());
                let target_str = PathBuf::from(target.value());
                if source_str == file_entry.source() && target_str == file_entry.target() {
                    error!(
                        "The {entry_name} table already contains an entry with the same source {} and target {}",
                        source_str.display(),
                        target_str.display()
                    );
                    return Err(AddError::SourceAlreadyExists(
                        file_entry.source().to_path_buf(),
                    ));
                }
            }
            arr.push(table);
        }
        i => {
            return Err(AddError::ExpectedTable(
                entry_name.to_string(),
                i.type_name().to_string(),
            ));
        }
    }
    let updated_config = config.to_string();
    std::fs::write(config_file, &updated_config)?;
    Ok(())
}

fn to_table<T: Serialize>(file_entry: &T) -> Result<toml_edit::Table, AddError> {
    let value =
        serde::Serialize::serialize(file_entry, toml_edit::ser::ValueSerializer::new()).unwrap();
    let item: Item = value.into();
    let table = match item {
        Item::Table(table) => table,
        Item::Value(Value::InlineTable(it)) => it.into_table(),
        i => {
            return Err(AddError::ExpectedTable(
                "link".to_string(),
                i.type_name().to_string(),
            ));
        }
    };
    Ok(table)
}

pub fn add_profile(
    name: &str,
    tags: &[Tag],
    global_config: &Arc<GlobalConfig>,
) -> Result<(), AddError> {
    let config_file = global_config.hermit_dir().join(CONF_FILE_NAME);
    info!("Updating profiles in {config_file:?}");
    if !config_file.exists() {
        config_file.parent().map_or_else(
            || {
                error!("HermitGrab configuration file not found at {config_file:?}");
                Err(AddError::ConfigFileNotFound(config_file.clone()))
            },
            |parent| {
                std::fs::create_dir_all(parent)?;
                Ok(())
            },
        )?;
        std::fs::write(&config_file, "")?;
        info!("Created new configuration file at {config_file:?}");
    }
    let mut config = load_hermit_config_editable(&config_file)?;
    let profiles = config["profiles"].or_insert(Item::Table(Table::new()));
    match profiles {
        Item::Table(t) => {
            let entry = t.get_mut(name);
            match entry {
                None | Some(Item::None) => {
                    let new_tags = BTreeSet::from_iter(tags.iter().map(|t| t.name()));
                    let mut arr = Array::new();
                    for tag in &new_tags {
                        arr.push(Value::String(Formatted::new(tag.to_string())));
                    }
                    t.insert(name, Item::Value(Value::Array(arr)));
                    success!(
                        "Added new profile {name} with tags '{}'",
                        new_tags.iter().join(",")
                    );
                }
                Some(Item::Value(Value::Array(arr))) => {
                    let mut new_tags = BTreeSet::from_iter(tags.iter().map(|t| t.name()));
                    for (idx, item) in arr.iter().enumerate() {
                        match item {
                            Value::String(val) => {
                                new_tags.remove(val.value().as_str());
                            }
                            _ => {
                                return Err(AddError::ExpectedString(
                                    format!("profiles.{name}[{idx}]"),
                                    item.type_name().to_string(),
                                ));
                            }
                        }
                    }
                    for tag in &new_tags {
                        arr.push(Value::String(Formatted::new(tag.to_string())));
                    }
                    success!(
                        "Updated existing profile {name} with additional tags '{}'",
                        new_tags.iter().join(",")
                    );
                }
                _ => {
                    return Err(AddError::ExpectedArray(
                        format!("profiles.{name}"),
                        entry.expect("None is checked").type_name().to_string(),
                    ));
                }
            }
        }
        _ => {
            return Err(AddError::ExpectedTable(
                "profiles".to_string(),
                profiles.type_name().to_string(),
            ));
        }
    }
    let new_config = config.to_string();
    std::fs::write(config_file, new_config)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::LinkConfig;

    #[test]
    pub fn test_to_table() {
        let entry = LinkConfig::default();
        to_table(&entry).unwrap();
    }
}
