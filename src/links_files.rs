use clap::ValueEnum;
use clap::builder::PossibleValue;
use serde::{Deserialize, Serialize};

use crate::hermitgrab_error::AtomicLinkError;
use crate::{LinkType, info};
use std::ffi::OsString;
use std::fs::{self, hard_link};
use std::path::Path;

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum FallbackOperation {
    #[default]
    Abort,
    Backup,
    Delete,
    DeleteDir,
    BackupOverwrite,
}

impl ValueEnum for FallbackOperation {
    fn value_variants<'a>() -> &'a [Self] {
        &[
            Self::Abort,
            Self::Backup,
            Self::BackupOverwrite,
            Self::Delete,
            Self::DeleteDir,
        ]
    }

    fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
        match self {
            FallbackOperation::Abort => Some(PossibleValue::new("abort")),
            FallbackOperation::Backup => Some(PossibleValue::new("backup")),
            FallbackOperation::Delete => Some(PossibleValue::new("delete")),
            FallbackOperation::DeleteDir => Some(PossibleValue::new("deletedir")),
            FallbackOperation::BackupOverwrite => Some(PossibleValue::new("backupoverwrite")),
        }
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
