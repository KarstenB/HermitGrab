// Built-in detectors for HermitGrab
// Detects: hostname, architecture (docker style), OS, OS version (numeric), OS version nickname

use std::collections::BTreeSet;

use crate::config::Tag;

pub fn detect_builtin_tags() -> BTreeSet<Tag> {
    let mut tags = BTreeSet::new();
    // Hostname
    if let Ok(hostname) = get_hostname() {
        tags.insert(format!("hostname={hostname}").into());
    }
    let info = os_info::get();

    // Print full information:
    println!("OS information: {info}");

    tags.insert(format!("os_type={}", info.os_type().to_string().replace(' ', "")).into());
    tags.insert(format!("os_version={}", info.version()).into());
    if let Some(edition) = info.edition() {
        tags.insert(format!("os_edition={}", edition).into());
    }
    if let Some(codename) = info.codename() {
        tags.insert(format!("os_codename={}", codename).into());
    }
    tags.insert(format!("os_bitness={}", info.bitness()).into());
    if let Some(architecture) = info.architecture() {
        tags.insert(format!("arch={}", architecture).into());
    } 
    tags
}

fn get_hostname() -> Result<String, std::io::Error> {
    hostname::get().map(|h| h.to_string_lossy().to_string())
}

