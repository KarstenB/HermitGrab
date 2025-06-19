use crate::config::Tag;
use std::collections::BTreeSet;

pub fn detect_builtin_tags() -> BTreeSet<Tag> {
    let mut tags = BTreeSet::new();
    // get user name
    let user = whoami::username();
    tags.insert(Tag::new(
        &format!("user={user}"),
        crate::config::Source::BuiltInDetector,
    ));
    // Hostname
    if let Ok(hostname) = get_hostname() {
        tags.insert(Tag::new(
            &format!("hostname={hostname}"),
            crate::config::Source::BuiltInDetector,
        ));
    }
    let info = os_info::get();

    tags.insert(Tag::new(
        &format!("os_family={}", std::env::consts::FAMILY),
        crate::config::Source::BuiltInDetector,
    ));
    tags.insert(Tag::new(
        &format!("os={}", std::env::consts::OS),
        crate::config::Source::BuiltInDetector,
    ));
    tags.insert(Tag::new(
        &format!("arch={}", std::env::consts::ARCH),
        crate::config::Source::BuiltInDetector,
    ));
    tags.insert(Tag::new(
        &format!("os_version={}", info.version()),
        crate::config::Source::BuiltInDetector,
    ));
    if let Some(edition) = info.edition() {
        tags.insert(Tag::new(
            &format!("os_edition={}", edition),
            crate::config::Source::BuiltInDetector,
        ));
    }
    if let Some(codename) = info.codename() {
        tags.insert(Tag::new(
            &format!("os_codename={}", codename),
            crate::config::Source::BuiltInDetector,
        ));
    }
    match info.bitness() {
        os_info::Bitness::X32 => tags.insert(Tag::new(
            "os_bitness=32",
            crate::config::Source::BuiltInDetector,
        )),
        os_info::Bitness::X64 => tags.insert(Tag::new(
            "os_bitness=64",
            crate::config::Source::BuiltInDetector,
        )),
        _ => false,
    };
    tags
}

fn get_hostname() -> Result<String, std::io::Error> {
    hostname::get().map(|h| h.to_string_lossy().to_string())
}
