use std::{collections::BTreeSet, path::PathBuf};

use crate::{
    HermitConfig, LinkType, RequireTag, choice,
    common_cli::prompt,
    config::{CONF_FILE_NAME, GlobalConfig, Tag},
    hermit_dir,
    hermitgrab_error::AddError,
    info,
};

pub(crate) fn add_target_dir(
    global_config: &GlobalConfig,
    target_dir: &Option<String>,
    tags: &[String],
    required_tags: &[RequireTag],
) -> Result<(), AddError> {
    todo!()
}

pub(crate) fn add_link(
    global_config: &GlobalConfig,
    target_dir: &Option<String>,
    source: &PathBuf,
    link_type: &LinkType,
    destination: &Option<String>,
    required_tags: &[RequireTag],
) -> Result<(), AddError> {
    let target_dir = if let Some(target_dir) = target_dir {
        PathBuf::from(target_dir)
    } else {
        let absolute_source = source.canonicalize().unwrap_or(source.clone());
        println!("Absolute source path: {}", absolute_source.display());
        let last_segment = if absolute_source.is_dir() {
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
        crate::common_cli::info("Which target directory do you want to use?");
        let config_file = hermit_dir().join(last_segment).join(CONF_FILE_NAME);
        let display_path = config_file
            .strip_prefix(hermit_dir())
            .unwrap_or(&config_file)
            .display();
        if config_file.exists() {
            choice!("1. Add to existing config file: '{}'", display_path);
        } else {
            choice!("1. Create new config file: '{}'", display_path);
        }
        crate::common_cli::choice("2. Use custom directory");
        crate::common_cli::choice("3. Use the root HermitGrab directory");
        let choice = prompt("What do you want to do? Enter either 1, 2 or 3: ")?;
        match choice.as_str() {
            "1" => hermit_dir().join(last_segment),
            "2" => {
                let custom_dir = prompt("Enter custom directory path: ")?;
                PathBuf::from(custom_dir)
            }
            "3" => hermit_dir(),
            _ => return Err(AddError::InvalidChoice),
        }
    };

    todo!()
}

pub(crate) fn add_profile(
    global_config: &GlobalConfig,
    name: &str,
    tags: &[Tag],
) -> Result<(), AddError> {
    let root_config = global_config.root_config();
    let mut config = if let Some(config) = root_config {
        config.clone()
    } else {
        HermitConfig::default()
    };
    config
        .profiles
        .insert(name.to_string(), BTreeSet::from_iter(tags.iter().cloned()));
    config.save_to_file(&hermit_dir().join(CONF_FILE_NAME))?;
    Ok(())
}
