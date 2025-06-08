use std::collections::BTreeSet;
use crate::config::Tag;

pub fn detect_builtin_tags() -> BTreeSet<Tag> {
    let mut tags = BTreeSet::new();
    // Hostname
    if let Ok(hostname) = get_hostname() {
        tags.insert(format!("hostname={hostname}").into());
    }
    let info = os_info::get();

    tags.insert(format!("os_family={}", std::env::consts::FAMILY).into());
    tags.insert(format!("os={}", std::env::consts::OS).into());
    tags.insert(format!("arch={}", std::env::consts::ARCH).into());
    tags.insert(format!("os_version={}", info.version()).into());
    if let Some(edition) = info.edition() {
        tags.insert(format!("os_edition={}", edition).into());
    }
    if let Some(codename) = info.codename() {
        tags.insert(format!("os_codename={}", codename).into());
    }
    match info.bitness() {
        os_info::Bitness::X32 => tags.insert("os_bitness=32".into()),
        os_info::Bitness::X64 => tags.insert("os_bitness=64".into()),
        _ => false,
    };
    tags
}

fn get_hostname() -> Result<String, std::io::Error> {
    hostname::get().map(|h| h.to_string_lossy().to_string())
}
