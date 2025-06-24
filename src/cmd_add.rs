use itertools::Itertools;
use std::{
    collections::BTreeSet,
    iter::FromIterator,
    path::{Path, PathBuf},
};
use toml_edit::{Array, ArrayOfTables, Formatted, Item, Table, Value};

use crate::{
    DotfileEntry, HermitConfig, InstallEntry, LinkType, RequireTag, choice,
    common_cli::{hint, prompt},
    config::{CONF_FILE_NAME, Source::CommandLine, Tag, load_hermit_config_editable},
    error, hermit_dir,
    hermitgrab_error::AddError,
    info,
    links_files::{FallbackOperation, copy},
    success, user_home,
};

pub(crate) fn add_config(
    target_dir: &PathBuf,
    provided_tags: &[Tag],
    required_tags: &[RequireTag],
    files: &[DotfileEntry],
    installs: &[InstallEntry],
) -> Result<(), AddError> {
    let config_file = if target_dir.ends_with(CONF_FILE_NAME) {
        target_dir.clone()
    } else {
        target_dir.join(CONF_FILE_NAME)
    };
    let mut config = HermitConfig::default();
    info!("Creating a new configuration file at {config_file:?}");
    let provided_tags = if provided_tags.is_empty() {
        prompt_for_provides()?
    } else {
        provided_tags.to_vec()
    };
    config.provides.extend(provided_tags);
    config.requires.extend(required_tags.to_vec());
    config.file.extend(files.to_vec());
    config.install.extend(installs.to_vec());
    std::fs::create_dir_all(target_dir)?;
    config.save_to_file(&config_file)?;
    Ok(())
}

fn prompt_for_provides() -> Result<Vec<Tag>, AddError> {
    hint(
        "If you want to avoid manually entering provided tags, use the --provides command line argument",
    );
    let cs_tags = prompt(
        "Please enter the tags that the new configuration file will provide as comma separated list: ",
    )?;
    Ok(cs_tags
        .split(',')
        .map(|x| Tag::new(x, CommandLine))
        .collect())
}

pub(crate) fn add_link(
    target_dir: &Option<String>,
    source: &Path,
    link_type: &LinkType,
    destination: &Option<String>,
    required_tags: &[RequireTag],
    provided_tags: &[Tag],
    fallback: &FallbackOperation,
) -> Result<(), AddError> {
    let target_dir = if let Some(target_dir) = target_dir {
        let new_target = PathBuf::from(target_dir);
        if new_target.is_absolute() {
            new_target
        } else {
            hermit_dir().join(new_target)
        }
    } else {
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
            "To avoid being prompted about the directory, you can use the --target-dir command line option",
        );
        let relative_source = absolute_source
            .strip_prefix(user_home())
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
        let deep_config_file = hermit_dir()
            .join(relative_source.parent().unwrap_or(relative_source))
            .join(CONF_FILE_NAME);
        let deep_config_display_path = deep_config_file
            .strip_prefix(hermit_dir())
            .unwrap_or(&deep_config_file)
            .display();
        let simple_config_file = hermit_dir()
            .join(last_segment_from_absolute)
            .join(CONF_FILE_NAME);
        let simple_config_display_path = simple_config_file
            .strip_prefix(hermit_dir())
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
        match choice.as_str() {
            "1" => simple_config_file.parent().unwrap().to_path_buf(),
            "2" => deep_config_file.parent().unwrap().to_path_buf(),
            "3" => {
                let custom_dir = prompt("Enter custom directory path: ")?;
                PathBuf::from(custom_dir)
            }
            "4" => hermit_dir(),
            _ => return Err(AddError::InvalidChoice),
        }
    };
    let config_file = target_dir.join(CONF_FILE_NAME);
    let target = if let Some(destination) = destination {
        let path = PathBuf::from(destination);
        path.strip_prefix(user_home())
            .map(|x| x.to_path_buf())
            .unwrap_or(path)
    } else {
        source.strip_prefix(user_home())?.to_path_buf()
    };
    let source_filename = source
        .file_name()
        .ok_or(AddError::FileNameError)?
        .to_string_lossy()
        .to_string();
    let file_entry = DotfileEntry {
        source: source_filename.clone(),
        target: target.to_string_lossy().to_string(),
        link: *link_type,
        requires: BTreeSet::from_iter(required_tags.iter().cloned()),
        fallback: *fallback,
    };
    if config_file.exists() {
        let table = to_table(&file_entry)?;
        let mut config = load_hermit_config_editable(&config_file)?;
        let files = config["files"].or_insert(Item::ArrayOfTables(ArrayOfTables::new()));
        match files {
            Item::ArrayOfTables(arr) => {
                for entry in arr.iter() {
                    let Item::Value(Value::String(ref source)) = entry["source"] else {
                        continue;
                    };
                    let Item::Value(Value::String(ref target)) = entry["target"] else {
                        continue;
                    };
                    let source_str = source.value();
                    let target_str = target.value();
                    if source_str == &file_entry.source && target_str == &file_entry.target {
                        error!(
                            "The [[files]] table already contains an entry with the same source {source_str} and target {target_str}"
                        );
                        return Err(AddError::SourceAlreadyExists(file_entry.source.clone()));
                    }
                }
                arr.push(table);
            }
            i => {
                return Err(AddError::ExpectedTable(
                    "files".to_string(),
                    i.type_name().to_string(),
                ));
            }
        }
        let updated_config = config.to_string();
        std::fs::write(&config_file, &updated_config)?;
    } else {
        add_config(&target_dir, provided_tags, &[], &[file_entry], &[])?;
    }
    copy(source, target_dir.join(source_filename).as_path())?;
    crate::success!("Added new link to {config_file:?}");
    Ok(())
}

fn to_table(file_entry: &DotfileEntry) -> Result<toml_edit::Table, AddError> {
    let value =
        serde::Serialize::serialize(file_entry, toml_edit::ser::ValueSerializer::new()).unwrap();
    let item: Item = value.into();
    let table = match item {
        Item::Table(table) => table,
        Item::Value(Value::InlineTable(it)) => it.into_table(),
        i => {
            return Err(AddError::ExpectedTable(
                "file".to_string(),
                i.type_name().to_string(),
            ));
        }
    };
    Ok(table)
}

pub(crate) fn add_profile(name: &str, tags: &[Tag]) -> Result<(), AddError> {
    let config_file = hermit_dir().join(CONF_FILE_NAME);
    info!("Updating profiles in {config_file:?}");
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
    use crate::DotfileEntry;

    #[test]
    pub fn test_to_table() {
        let entry = DotfileEntry::default();
        to_table(&entry).unwrap();
    }
}
