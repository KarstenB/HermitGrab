use std::{ffi::OsString, path::Path};

use crate::{
    FileOpsError, LinkType,
    config::{FallbackOperation, FileStatus},
};

pub fn link_files<P: AsRef<Path>, Q: AsRef<Path>>(
    src: P,
    dst: Q,
    link_type: &LinkType,
    fall_back: &FallbackOperation,
) -> Result<(), FileOpsError> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    if !src.exists() {
        return Err(FileOpsError::SourceNotFound(src.display().to_string()));
    }
    if dst.exists() {
        // If destination is a symlink to src, do nothing
        if let Ok(target) = dst.read_link() {
            if target == src {
                return Ok(());
            }
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
                    std::fs::rename(dst, &backup_file)
                        .map_err(|e| FileOpsError::Io(backup_file, e))?;
                } else {
                    return Err(FileOpsError::BackupAlreadyExists(dst.display().to_string()));
                }
            }
            FallbackOperation::BackupOverwrite => {
                let mut base_file_name = dst.file_name().expect("file name").to_os_string();
                base_file_name.push(OsString::from(".bak"));
                let backup_file = dst.with_file_name(base_file_name);
                std::fs::rename(dst, &backup_file).map_err(|e| FileOpsError::Io(backup_file, e))?;
            }
            FallbackOperation::Delete => {
                if dst.is_dir() {
                    std::fs::remove_dir(dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
                } else {
                    std::fs::remove_file(dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
                }
            }
            FallbackOperation::DeleteDir => {
                if dst.is_dir() {
                    std::fs::remove_dir_all(dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
                } else {
                    std::fs::remove_file(dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
                }
            }
        }
    }
    let dst_parent = dst.parent();
    if let Some(dst_parent) = dst_parent {
        if !dst_parent.exists() {
            std::fs::create_dir_all(dst_parent)
                .map_err(|e| FileOpsError::Io(dst_parent.into(), e))?;
        }
    }
    match link_type {
        LinkType::Soft => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::symlink;
                symlink(src, dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
            }
            #[cfg(windows)]
            {
                use std::os::windows::fs::symlink_file;
                symlink_file(src, dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
            }
        }
        LinkType::Hard => {
            std::fs::hard_link(src, dst).map_err(|e| FileOpsError::Io(dst.into(), e))?;
        }
        LinkType::Copy => {
            copy(src, dst)?;
        }
    }
    Ok(())
}

pub fn copy(src: &Path, dst: &Path) -> Result<(), FileOpsError> {
    if src.is_file() {
        if let Some(parent) = dst.parent() {
            if !parent.exists() {
                std::fs::create_dir_all(parent).map_err(|e| FileOpsError::Io(parent.into(), e))?;
            }
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
        let Ok(dst_meta) = actual_dst.metadata() else {
            return FileStatus::FailedToGetMetadata(actual_dst.into());
        };
        let Ok(src_meta) = src_file.metadata() else {
            return FileStatus::FailedToGetMetadata(src_file.into());
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
