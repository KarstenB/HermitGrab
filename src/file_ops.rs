// SPDX-FileCopyrightText: 2025 Karsten Becker
//
// SPDX-License-Identifier: GPL-3.0-only

use std::ffi::OsString;
use std::path::Path;

use crate::config::{FallbackOperation, FileStatus};
use crate::{FileOpsError, LinkType};

pub fn link_files<P: AsRef<Path>, Q: AsRef<Path>>(
    src: P,
    dst: Q,
    link_type: &LinkType,
    fall_back: &FallbackOperation,
) -> Result<(), FileOpsError> {
    let src = src
        .as_ref()
        .canonicalize()
        .unwrap_or(src.as_ref().to_path_buf());
    let dst = dst
        .as_ref()
        .canonicalize()
        .unwrap_or(dst.as_ref().to_path_buf());
    if !src.exists() {
        return Err(FileOpsError::SourceNotFound(src.display().to_string()));
    }
    let dst_clone = dst.clone();
    if dst.exists() || dst.is_symlink() {
        if src == dst {
            return Ok(());
        }
        match fall_back {
            FallbackOperation::Abort => {
                return Err(FileOpsError::DestinationExists(dst.display().to_string()));
            }
            FallbackOperation::Backup => {
                let mut base_file_name = dst.file_name().expect("file name").to_os_string();
                base_file_name.push(OsString::from(".bak"));
                let backup_file = dst.with_file_name(base_file_name);
                if !backup_file.exists() {
                    std::fs::rename(&dst, &backup_file)
                        .map_err(|e| FileOpsError::Io(backup_file, e))?;
                } else {
                    return Err(FileOpsError::BackupAlreadyExists(dst.display().to_string()));
                }
            }
            FallbackOperation::BackupOverwrite => {
                let mut base_file_name = dst.file_name().expect("file name").to_os_string();
                base_file_name.push(OsString::from(".bak"));
                let backup_file = dst.with_file_name(base_file_name);
                std::fs::rename(&dst, &backup_file)
                    .map_err(|e| FileOpsError::Io(backup_file, e))?;
            }
            FallbackOperation::Delete => {
                if dst.is_dir() {
                    std::fs::remove_dir(&dst).map_err(|e| FileOpsError::Io(dst, e))?;
                } else {
                    std::fs::remove_file(&dst).map_err(|e| FileOpsError::Io(dst, e))?;
                }
            }
            FallbackOperation::DeleteDir => {
                if dst.is_dir() {
                    std::fs::remove_dir_all(&dst).map_err(|e| FileOpsError::Io(dst, e))?;
                } else {
                    std::fs::remove_file(&dst).map_err(|e| FileOpsError::Io(dst, e))?;
                }
            }
            FallbackOperation::Ignore => {
                return Ok(());
            }
        }
    }
    let dst_parent = dst_clone.parent();
    if let Some(dst_parent) = dst_parent
        && !dst_parent.exists()
    {
        std::fs::create_dir_all(dst_parent).map_err(|e| FileOpsError::Io(dst_parent.into(), e))?;
    }
    match link_type {
        LinkType::Soft => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(src, &dst_clone).map_err(|e| FileOpsError::Io(dst_clone, e))?;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::symlink_file;
                symlink_file(src, &dst_clone).map_err(|e| FileOpsError::Io(dst_clone.into(), e))?;
            }
        }
        LinkType::Hard => {
            std::fs::hard_link(src, &dst_clone).map_err(|e| FileOpsError::Io(dst_clone, e))?;
        }
        LinkType::Copy => {
            copy(&src, &dst_clone)?;
        }
    }
    Ok(())
}

pub fn copy(src: &Path, dst: &Path) -> Result<(), FileOpsError> {
    if src.is_file() {
        if let Some(parent) = dst.parent()
            && !parent.exists()
        {
            std::fs::create_dir_all(parent).map_err(|e| FileOpsError::Io(parent.into(), e))?;
        }
        std::fs::copy(src, dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
    } else {
        for file in src
            .read_dir()
            .map_err(|e| FileOpsError::Io(src.into(), e))?
        {
            let entry = file.map_err(|e| FileOpsError::Io(src.into(), e))?;
            copy(&entry.path(), dst.join(entry.file_name()).as_path())?;
        }
    }
    Ok(())
}

pub fn check_copied(quick: bool, src_file: &Path, actual_dst: &Path) -> FileStatus {
    match actual_dst.try_exists() {
        Ok(exists) => {
            if !exists {
                return FileStatus::DestinationDoesNotExist(actual_dst.into());
            }
        }
        Err(e) => return FileStatus::FailedToAccessFile(actual_dst.into(), e),
    }
    if actual_dst.is_file() {
        if !src_file.is_file() {
            return FileStatus::SrcIsDirButTargetIsFile(actual_dst.into());
        }
        let dst_meta = match actual_dst.metadata() {
            Ok(dst_meta) => dst_meta,
            Err(e) => return FileStatus::FailedToGetMetadata(actual_dst.into(), e),
        };
        let src_meta = match src_file.metadata() {
            Ok(src_meta) => src_meta,
            Err(e) => return FileStatus::FailedToGetMetadata(src_file.into(), e),
        };
        if src_meta.len() != dst_meta.len() {
            return FileStatus::SizeDiffers(actual_dst.into(), src_meta.len(), dst_meta.len());
        }
        if !quick {
            let src_hash = match hash_file(src_file) {
                Ok(hash) => hash,
                Err(e) => return FileStatus::FailedToHashFile(src_file.into(), e),
            };
            let dst_hash = match hash_file(actual_dst) {
                Ok(hash) => hash,
                Err(e) => return FileStatus::FailedToHashFile(actual_dst.into(), e),
            };
            if src_hash != dst_hash {
                return FileStatus::HashDiffers(actual_dst.into(), src_hash, dst_hash);
            }
        }
        FileStatus::Ok
    } else {
        if !src_file.is_dir() {
            return FileStatus::SrcIsFileButTargetIsDir(actual_dst.into());
        }
        match src_file.read_dir() {
            Ok(e) => {
                for f in e {
                    let fs = match f {
                        Ok(file) => {
                            check_copied(quick, &file.path(), &actual_dst.join(file.file_name()))
                        }
                        Err(e) => return FileStatus::FailedToTraverseDir(src_file.into(), e),
                    };
                    if !fs.is_ok() {
                        return fs;
                    }
                }
            }
            Err(e) => {
                return FileStatus::FailedToTraverseDir(src_file.into(), e);
            }
        }
        FileStatus::Ok
    }
}

pub fn hash_file(path: &Path) -> Result<blake3::Hash, std::io::Error> {
    let mut hasher = blake3::Hasher::new();
    hasher.update_mmap(path)?;
    Ok(hasher.finalize())
}

#[cfg(not(debug_assertions))]
pub use std::sync::LazyLock;

#[cfg(debug_assertions)]
pub struct LazyLock<T: 'static, F: 'static = fn() -> T>
where
    F: Fn() -> T,
{
    init: F,
}

#[cfg(debug_assertions)]
impl<T: 'static, F: 'static> LazyLock<T, F>
where
    F: Fn() -> T,
{
    pub const fn new(init: F) -> Self {
        Self { init }
    }
}

#[cfg(debug_assertions)]
impl<T: 'static, F: 'static> std::ops::Deref for LazyLock<T, F>
where
    F: Fn() -> T,
{
    type Target = T;

    fn deref(&self) -> &Self::Target {
        // In tests, we use a leaked Box to create a stable reference
        // This will recompute on each access and leak memory, but that's OK for tests
        let value = Box::new((self.init)());
        Box::leak(value)
    }
}

pub mod dirs {
    use super::LazyLock;

    pub static BASE_DIRS: LazyLock<directories::BaseDirs, fn() -> directories::BaseDirs> =
        LazyLock::new(|| directories::BaseDirs::new().expect("BaseDirs should init"));

    pub static HOME_DIR: LazyLock<String> =
        LazyLock::new(|| BASE_DIRS.home_dir().to_path_buf().display().to_string());
    /// XDG_HOME env or $HOME
    pub static XDG_HOME: LazyLock<String> =
        LazyLock::new(|| std::env::var("XDG_HOME").unwrap_or_else(|_| HOME_DIR.clone()));

    pub static STATE_DIR: LazyLock<Option<String>> =
        LazyLock::new(|| BASE_DIRS.state_dir().map(|x| x.display().to_string()));
    /// XDG_STATE_HOME env or ~/.local/state
    pub static XDG_STATE_HOME: LazyLock<String> = LazyLock::new(|| {
        std::env::var("XDG_STATE_HOME").unwrap_or_else(|_| {
            BASE_DIRS
                .home_dir()
                .join(".local/state")
                .display()
                .to_string()
        })
    });
    pub static CONFIG_DIR: LazyLock<String> =
        LazyLock::new(|| BASE_DIRS.config_dir().display().to_string());
    /// XDG_CONFIG_HOME env or ~/.config
    pub static XDG_CONFIG_HOME: LazyLock<String> = LazyLock::new(|| {
        std::env::var("XDG_CONFIG_HOME")
            .unwrap_or_else(|_| BASE_DIRS.home_dir().join(".config").display().to_string())
    });
    pub static CONFIG_LOCAL_DIR: LazyLock<String> = LazyLock::new(|| {
        BASE_DIRS
            .config_local_dir()
            .to_path_buf()
            .display()
            .to_string()
    });
    pub static CACHE_DIR: LazyLock<String> =
        LazyLock::new(|| BASE_DIRS.cache_dir().to_path_buf().display().to_string());
    /// XDG_CACHE_HOME env or ~/.cache
    pub static XDG_CACHE_HOME: LazyLock<String> = LazyLock::new(|| {
        std::env::var("XDG_CACHE_HOME")
            .unwrap_or_else(|_| BASE_DIRS.home_dir().join(".cache").display().to_string())
    });
    pub static DATA_DIR: LazyLock<String> =
        LazyLock::new(|| BASE_DIRS.data_dir().to_path_buf().display().to_string());
    /// XDG_DATA_HOME env or ~/.local/share
    pub static XDG_DATA_HOME: LazyLock<String> = LazyLock::new(|| {
        std::env::var("XDG_DATA_HOME").unwrap_or_else(|_| {
            BASE_DIRS
                .home_dir()
                .join(".local/share")
                .display()
                .to_string()
        })
    });
    pub static DATA_LOCAL_DIR: LazyLock<String> = LazyLock::new(|| {
        BASE_DIRS
            .data_local_dir()
            .to_path_buf()
            .display()
            .to_string()
    });
    pub static EXECUTABLE_DIR: LazyLock<Option<String>> = LazyLock::new(|| {
        BASE_DIRS
            .executable_dir()
            .map(|x| x.to_path_buf().display().to_string())
    });
    /// XDG_BIN_HOME env or ~/.local/bin
    pub static XDG_BIN_HOME: LazyLock<String> = LazyLock::new(|| {
        std::env::var("XDG_BIN_HOME").unwrap_or_else(|_| {
            BASE_DIRS
                .home_dir()
                .join(".local/bin")
                .display()
                .to_string()
        })
    });
    pub static PREFERENCE_DIR: LazyLock<String> = LazyLock::new(|| {
        BASE_DIRS
            .preference_dir()
            .to_path_buf()
            .display()
            .to_string()
    });
    pub static RUNTIME_DIR: LazyLock<Option<String>> =
        LazyLock::new(|| BASE_DIRS.runtime_dir().map(|x| x.display().to_string()));
    /// XDG_RUNTIME_DIR env or None
    pub static XDG_RUNTIME_DIR: LazyLock<Option<String>> =
        LazyLock::new(|| std::env::var("XDG_RUNTIME_DIR").ok());
}
