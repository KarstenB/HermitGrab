use crate::hermitgrab_error::AtomicLinkError;
use std::fs::{self, hard_link};
use std::path::Path;

pub fn link_files<P: AsRef<Path>, Q: AsRef<Path>>(
    src: P,
    dst: Q,
    link_type: crate::LinkType, // Currently unused, but can be extended for different link types
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
        // If destination is a file (not a symlink), abort
        if dst.is_file() && !dst.is_symlink() {
            return Err(AtomicLinkError::DestinationExists(
                dst.display().to_string(),
            ));
        }
        //TODO: Create a backup of the existing symlink/file to support rollback
        fs::remove_file(dst)?;
    }
    match link_type {
        crate::LinkType::Soft => {
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
        crate::LinkType::Hard => {
            hard_link(src, dst)?;
        }
        crate::LinkType::Copy => {
            fs::copy(src, dst)?;
        },
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
        link_files(&src, &dst, LinkType::Soft).unwrap();
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
        let result = link_files(&src, &dst, LinkType::Soft);
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
        let result = link_files(&src, &dst, LinkType::Soft);
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
        let result = link_files(&src, &dst, LinkType::Soft);
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
        link_files(&src, &dst, LinkType::Soft).unwrap();
        assert!(dst.exists());
        assert!(dst.is_symlink());
        assert!(dst.read_link().unwrap() == src);
        fs::remove_dir_all(&src).unwrap();
        fs::remove_dir_all(&dst).unwrap();
    }
}
