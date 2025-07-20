// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use crate::{
    action::install::execute_script,
    config::{DetectorConfig, GlobalConfig, Tag},
};
use std::collections::{BTreeSet, HashMap};

pub fn detect_builtin_tags() -> BTreeSet<Tag> {
    let mut tags = BTreeSet::new();
    // get user name
    let user = whoami::username();
    tags.insert(Tag::new_with_value(
        "user",
        &user,
        crate::config::Source::BuiltInDetector,
    ));
    // Hostname
    if let Ok(hostname) = get_hostname() {
        tags.insert(Tag::new_with_value(
            "hostname",
            &hostname,
            crate::config::Source::BuiltInDetector,
        ));
    }
    tags.insert(Tag::new_with_value(
        "os_family",
        std::env::consts::FAMILY,
        crate::config::Source::BuiltInDetector,
    ));
    tags.insert(Tag::new_with_value(
        "os",
        std::env::consts::OS,
        crate::config::Source::BuiltInDetector,
    ));
    tags.insert(Tag::new_with_value(
        "arch",
        std::env::consts::ARCH,
        crate::config::Source::BuiltInDetector,
    ));
    let alias = get_arch_alias();
    tags.insert(Tag::new_with_value(
        "arch_alias",
        alias,
        crate::config::Source::BuiltInDetector,
    ));
    let info = os_info::get();
    tags.insert(Tag::new_with_value(
        "os_version",
        &info.version().to_string(),
        crate::config::Source::BuiltInDetector,
    ));
    if let Some(edition) = info.edition() {
        tags.insert(Tag::new_with_value(
            "os_edition",
            edition,
            crate::config::Source::BuiltInDetector,
        ));
    }
    if let Some(codename) = info.codename() {
        tags.insert(Tag::new_with_value(
            "os_codename",
            codename,
            crate::config::Source::BuiltInDetector,
        ));
    }
    match info.bitness() {
        os_info::Bitness::X32 => tags.insert(Tag::new_with_value(
            "os_bitness",
            "32",
            crate::config::Source::BuiltInDetector,
        )),
        os_info::Bitness::X64 => tags.insert(Tag::new_with_value(
            "os_bitness",
            "64",
            crate::config::Source::BuiltInDetector,
        )),
        _ => false,
    };
    tags
}

fn get_arch_alias() -> &'static str {
    let arch_map = HashMap::from([
        ("aarch64", "arm64"),
        ("x86_64", "amd64"),
        ("armv7", "armhf"),
    ]);
    arch_map
        .get(std::env::consts::ARCH)
        .unwrap_or(&std::env::consts::ARCH)
}

fn get_hostname() -> Result<String, std::io::Error> {
    hostname::get().map(|h| h.to_string_lossy().to_string())
}

fn create_detected_tag(
    (name, config): (&String, &DetectorConfig),
) -> Result<Option<Tag>, std::io::Error> {
    match config {
        DetectorConfig::EnableIf { enable_if } => {
            if execute_script(enable_if)?.status.success() {
                Ok(Some(Tag::new(
                    name,
                    crate::config::Source::Detector(name.clone()),
                )))
            } else {
                Ok(None)
            }
        }
        DetectorConfig::EnableIfNot { enable_if_not } => {
            let output = execute_script(enable_if_not)?;
            if let Some(exit_code) = output.status.code() {
                if exit_code != 0 {
                    Ok(Some(Tag::new(
                        name,
                        crate::config::Source::Detector(name.clone()),
                    )))
                } else {
                    Ok(None)
                }
            } else {
                Ok(None)
            }
        }
        DetectorConfig::ValueOf { value_of } => {
            let output = execute_script(value_of)?;
            if output.status.success() {
                let string = String::from_utf8(output.stdout)
                    .map_err(|_| std::io::Error::other("File not utf-8 encoded"))?;
                Ok(Some(Tag::new_with_value(
                    name,
                    string.trim(),
                    crate::config::Source::Detector(name.to_string()),
                )))
            } else {
                Ok(None)
            }
        }
    }
}

pub fn get_detected_tags(config: &GlobalConfig) -> Result<Vec<Tag>, std::io::Error> {
    let tags: Result<Vec<Option<Tag>>, std::io::Error> = config
        .all_detectors()
        .into_iter()
        .map(create_detected_tag)
        .collect();
    Ok(tags?.into_iter().flatten().collect::<Vec<Tag>>())
}
