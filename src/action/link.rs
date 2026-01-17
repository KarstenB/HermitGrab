// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::path::PathBuf;
use std::sync::Arc;

use itertools::Itertools;
use serde::Serialize;

use crate::action::{Action, ActionObserver, Status};
use crate::config::{ConfigItem, FallbackOperation, FileStatus};
use crate::file_ops::dirs::BASE_DIRS;
use crate::file_ops::{check_copied, link_files};
use crate::hermitgrab_error::{ActionError, LinkActionError};
use crate::{HermitConfig, LinkConfig, LinkType, RequireTag};

#[derive(Serialize, Debug, Hash, PartialEq)]
pub struct LinkAction {
    #[serde(skip)]
    rel_src: String,
    #[serde(skip)]
    rel_dst: String,
    src: PathBuf,
    dst: PathBuf,
    link_type: LinkType,
    requires: Vec<RequireTag>,
    fallback: FallbackOperation,
    order: u64,
}

impl LinkAction {
    pub fn new(
        link_config: &LinkConfig,
        cfg: &HermitConfig,
        fallback: &Option<FallbackOperation>,
    ) -> Result<Self, std::io::Error> {
        let src = match cfg.expand_directory(&link_config.source) {
            Ok(path) => path,
            Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
        };
        let src = if src.is_absolute() {
            link_config.source.clone()
        } else {
            cfg.directory().join(&link_config.source)
        };
        if !src.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("Source file does not exist: {}", src.display()),
            ));
        }
        let src = src.canonicalize()?;
        let rel_src = src
            .strip_prefix(cfg.directory())
            .unwrap_or(&link_config.source)
            .to_string_lossy()
            .to_string();
        let dst = match cfg.expand_directory(&link_config.target) {
            Ok(path) => path,
            Err(e) => return Err(std::io::Error::new(std::io::ErrorKind::Other, e)),
        };
        let rel_dst = dst
            .strip_prefix(BASE_DIRS.home_dir())
            .unwrap_or(&dst)
            .to_string_lossy()
            .to_string();
        let requires = link_config.get_all_requires(cfg);
        let fallback = (*fallback).unwrap_or(link_config.fallback);
        Ok(Self {
            src,
            rel_src,
            dst,
            rel_dst,
            link_type: link_config.link,
            order: link_config.total_order(cfg),
            requires: requires.into_iter().collect(),
            fallback,
        })
    }

    pub fn check(&self, quick: bool) -> FileStatus {
        let actual_dst = self.dst.clone();
        match actual_dst.try_exists() {
            Ok(exists) => {
                if !exists {
                    return FileStatus::DestinationDoesNotExist(actual_dst);
                }
            }
            Err(e) => return FileStatus::FailedToAccessFile(actual_dst, e),
        }
        match self.link_type {
            LinkType::Soft => {
                if !actual_dst.is_symlink() {
                    return FileStatus::DestinationNotSymLink(actual_dst);
                }
                let read_link = actual_dst.canonicalize();
                let Ok(read_link) = read_link else {
                    return FileStatus::FailedToReadSymlink(actual_dst);
                };
                if read_link != self.src {
                    return FileStatus::SymlinkDestinationMismatch(actual_dst, read_link);
                }
                FileStatus::Ok
            }
            LinkType::Hard => {
                #[cfg(target_family = "unix")]
                {
                    let dst_meta = match actual_dst.metadata() {
                        Ok(meta) => meta,
                        Err(e) => return FileStatus::FailedToGetMetadata(actual_dst, e),
                    };
                    let src_meta = match self.src.metadata() {
                        Ok(src_meta) => src_meta,
                        Err(e) => return FileStatus::FailedToGetMetadata(self.src.clone(), e),
                    };
                    use std::os::unix::fs::MetadataExt;
                    let dst_ino = dst_meta.ino();
                    let src_ino = src_meta.ino();
                    if src_ino != dst_ino {
                        return FileStatus::InodeMismatch(actual_dst);
                    }
                    FileStatus::Ok
                }
                #[cfg(not(target_family = "unix"))]
                {
                    crate::common_cli::warn(
                        "Hardlink check not supported on non unix systems, checking file similarity",
                    );
                    return check_copied(quick, &self.src, &actual_dst);
                }
            }
            LinkType::Copy => check_copied(quick, &self.src, &actual_dst),
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
            self.dst.display(),
            self.requires
        )
    }

    fn requires(&self) -> &[RequireTag] {
        &self.requires
    }

    fn execute(&self, observer: &Arc<impl ActionObserver>) -> Result<(), ActionError> {
        observer.action_progress(&self.id(), 0, 1, "Linking files");
        link_files(&self.src, &self.dst, &self.link_type, &self.fallback)
            .map_err(LinkActionError::FileOps)?;
        observer.action_progress(&self.id(), 1, 1, "Linking completed");
        Ok(())
    }

    fn id(&self) -> String {
        format!(
            "LinkAction:{}:{}:{}:{}:{}",
            self.rel_src,
            self.rel_dst,
            self.link_type,
            self.fallback,
            self.requires.iter().join(",")
        )
    }

    fn get_status(&self, _cfg: &HermitConfig, quick: bool) -> Status {
        let status = self.check(quick);
        if status.is_ok() {
            return Status::Ok(format!("{} is linked", self.rel_dst));
        }
        if status.is_error() {
            return Status::Error(format!(
                "File check failed for {}: {}",
                self.rel_dst, status
            ));
        }
        Status::NotOk(format!("{} has issues: {}", self.rel_dst, status))
    }

    fn get_order(&self) -> u64 {
        self.order
    }
}

#[cfg(test)]
mod tests {
    use std::{env, fs};

    use super::*;
    use crate::LinkType;

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
            Err(crate::FileOpsError::SourceNotFound(_))
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
            Err(crate::FileOpsError::DestinationExists(_))
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
        fs::write(src.join("file1.txt"), b"file 1").unwrap();
        link_files(&src, &dst, &LinkType::Soft, &FallbackOperation::Abort).unwrap();
        assert!(dst.exists());
        assert!(dst.is_symlink());
        assert_eq!(dst.read_link().unwrap(), src.canonicalize().unwrap());
        fs::remove_dir_all(&src).unwrap();
        fs::remove_dir_all(&dst).unwrap();
    }
}
