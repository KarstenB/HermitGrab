use std::{collections::BTreeSet, path::PathBuf};

use crate::{
    LinkType, RequireTag,
    action::Action,
    config::{FallbackOperation, Tag, expand_directory},
    hermitgrab_error::{ActionError, LinkActionError},
};

use crate::hermitgrab_error::AtomicLinkError;
use crate::info;
use std::ffi::OsString;
use std::fs::{self, hard_link};
use std::path::Path;

pub struct LinkAction {
    id: String,
    rel_src: String,
    rel_dst: String,
    src: PathBuf,
    dst: String,
    link_type: LinkType,
    requires: Vec<RequireTag>,
    provides: Vec<Tag>,
    fallback: FallbackOperation,
}
impl LinkAction {
    pub(crate) fn new(
        id: String,
        config_dir: &PathBuf,
        src: PathBuf,
        dst: String,
        requires: BTreeSet<RequireTag>,
        provides: BTreeSet<Tag>,
        link_type: LinkType,
        fallback: FallbackOperation,
    ) -> Self {
        let rel_src = src
            .strip_prefix(config_dir)
            .unwrap_or(&src)
            .to_string_lossy()
            .to_string();
        let rel_dst = expand_directory(&dst);
        let rel_dst = rel_dst
            .strip_prefix(shellexpand::tilde("~/").as_ref())
            .unwrap_or(&rel_dst)
            .to_string_lossy()
            .to_string();

        Self {
            id,
            src,
            rel_src,
            dst,
            rel_dst,
            link_type,
            requires: requires.into_iter().collect(),
            provides: provides.into_iter().collect(),
            fallback,
        }
    }
}

impl Action for LinkAction {
    fn short_description(&self) -> String {
        let link_type_str = match self.link_type {
            LinkType::Soft => "Symlink",
            LinkType::Hard => "Hardlink",
            LinkType::Copy => "Copy",
        };
        format!("{link_type_str} {} -> {}", self.rel_src, self.rel_dst)
    }
    fn long_description(&self) -> String {
        format!(
            "Symlink from {} to {} (tags: {:?})",
            self.src.display(),
            self.dst,
            self.requires
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
        let dst_str = expand_directory(&self.dst);
        link_files(&self.src, dst_str, &self.link_type, &self.fallback)
            .map_err(LinkActionError::AtomicLinkError)?;
        Ok(())
    }
}

pub fn link_files<P: AsRef<Path>, Q: AsRef<Path>>(
    src: P,
    dst: Q,
    link_type: &LinkType,
    fall_back: &FallbackOperation,
) -> Result<(), AtomicLinkError> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    if !src.exists() {
        return Err(AtomicLinkError::SourceNotFound(src.display().to_string()));
    }
    if dst.exists() {
        // If destination is a symlink to src, do nothing
        if let Ok(target) = dst.read_link() {
            if target == src {
                return Ok(());
            }
        }
        if !dst.is_symlink() {
            match fall_back {
                FallbackOperation::Abort => {
                    return Err(AtomicLinkError::DestinationExists(
                        dst.display().to_string(),
                    ));
                }
                FallbackOperation::Backup => {
                    let mut base_file_name = dst.file_name().expect("file name").to_os_string();
                    base_file_name.push(OsString::from(".bak"));
                    let backup_file = dst.with_file_name(base_file_name);
                    if !backup_file.exists() {
                        std::fs::rename(dst, &backup_file)?;
                    } else {
                        return Err(AtomicLinkError::BackupAlreadyExists(
                            dst.display().to_string(),
                        ));
                    }
                }
                FallbackOperation::BackupOverwrite => {
                    let mut base_file_name = dst.file_name().expect("file name").to_os_string();
                    base_file_name.push(OsString::from(".bak"));
                    let backup_file = dst.with_file_name(base_file_name);
                    std::fs::rename(dst, &backup_file)?;
                }
                FallbackOperation::Delete => {
                    if dst.is_dir() {
                        std::fs::remove_dir(dst)?;
                    } else {
                        std::fs::remove_file(dst)?;
                    }
                }
                FallbackOperation::DeleteDir => {
                    if dst.is_dir() {
                        std::fs::remove_dir_all(dst)?;
                    } else {
                        std::fs::remove_file(dst)?;
                    }
                }
            }
        }
    }
    let dst_parent = dst.parent();
    if let Some(dst_parent) = dst_parent {
        if !dst_parent.exists() {
            fs::create_dir_all(dst_parent)?;
        }
    }
    match link_type {
        LinkType::Soft => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(src, dst)?;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::symlink_file;
                symlink_file(src, dst)?;
            }
        }
        LinkType::Hard => {
            hard_link(src, dst)?;
        }
        LinkType::Copy => {
            copy(src, dst)?;
        }
    }
    Ok(())
}

pub fn copy(src: &Path, dst: &Path) -> std::io::Result<()> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent)?;
            }
        }
        info!("Copying file {src:?} to {dst:?}");
        std::fs::copy(src, dst)?;
    } else {
        info!("Copying dir {src:?} to {dst:?}");
        for file in src.read_dir()? {
            let entry = file?;
            copy(&entry.path(), dst.join(entry.file_name()).as_path())?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::LinkType;

    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_atomic_symlink_success() {
        let tmp_dir = env::temp_dir();
        let src = tmp_dir.join("hermitgrab_test_src");
        let dst = tmp_dir.join("hermitgrab_test_dst");
        fs::write(&src, b"test").unwrap();
        link_files(&src, &dst, &LinkType::Soft, &FallbackOperation::Abort).unwrap();
        assert!(dst.exists());
        assert_eq!(fs::read_to_string(&dst).unwrap(), "test");
        fs::remove_file(&src).unwrap();
        fs::remove_file(&dst).unwrap();
    }

    #[test]
    fn test_atomic_symlink_source_missing() {
        let tmp_dir = env::temp_dir();
        let src = tmp_dir.join("hermitgrab_test_missing_src");
        let dst = tmp_dir.join("hermitgrab_test_dst2");
        if dst.exists() {
            fs::remove_file(&dst).unwrap();
        }
        let result = link_files(&src, &dst, &LinkType::Soft, &FallbackOperation::Abort);
        assert!(matches!(
            result,
            Err(crate::AtomicLinkError::SourceNotFound(_))
        ));
    }

    #[test]
    fn test_atomic_symlink_dst_is_file() {
        let tmp_dir = env::temp_dir();
        let src = tmp_dir.join("hermitgrab_test_src2");
        let dst = tmp_dir.join("hermitgrab_test_dst3");
        fs::write(&src, b"test").unwrap();
        fs::write(&dst, b"existing").unwrap();
        let result = link_files(&src, &dst, &LinkType::Soft, &FallbackOperation::Abort);
        assert!(matches!(
            result,
            Err(crate::AtomicLinkError::DestinationExists(_))
        ));
        fs::remove_file(&src).unwrap();
        fs::remove_file(&dst).unwrap();
    }

    #[test]
    fn test_atomic_symlink_dst_is_symlink_to_src() {
        let tmp_dir = env::temp_dir();
        let src = tmp_dir.join("hermitgrab_test_src3");
        let dst = tmp_dir.join("hermitgrab_test_dst4");
        fs::write(&src, b"test").unwrap();
        #[cfg(unix)]
        {
            use std::os::unix::fs::symlink;
            symlink(&src, &dst).unwrap();
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::symlink_file;
            symlink_file(&src, &dst).unwrap();
        }
        let result = link_files(&src, &dst, &LinkType::Soft, &FallbackOperation::Abort);
        assert!(result.is_ok());
        fs::remove_file(&src).unwrap();
        fs::remove_file(&dst).unwrap();
    }

    #[test]
    fn test_atomic_symlink_directory() {
        let tmp_dir = env::temp_dir();
        let src = tmp_dir.join("hermitgrab_test_src_dir");
        let dst = tmp_dir.join("hermitgrab_test_dst_dir");
        if dst.exists() {
            println!("Removing existing destination directory: {}", dst.display());
            fs::remove_dir_all(&dst).unwrap();
        }
        if src.exists() {
            println!("Removing existing source directory: {}", src.display());
            fs::remove_dir_all(&src).unwrap();
        }
        fs::create_dir(&src).unwrap();
        link_files(&src, &dst, &LinkType::Soft, &FallbackOperation::Abort).unwrap();
        assert!(dst.exists());
        assert!(dst.is_symlink());
        assert!(dst.read_link().unwrap() == src);
        fs::remove_dir_all(&src).unwrap();
        fs::remove_dir_all(&dst).unwrap();
    }
}
