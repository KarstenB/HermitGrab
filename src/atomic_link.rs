use crate::hermitgrab_error::AtomicLinkError;
use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::Path;

/// Atomically create a symlink, replacing the target if it exists.
pub fn atomic_symlink<P: AsRef<Path>, Q: AsRef<Path>>(
    src: P,
    dst: Q,
) -> Result<(), AtomicLinkError> {
    let src = src.as_ref();
    let dst = dst.as_ref();
    // Check if source exists
    if !src.exists() {
        return Err(AtomicLinkError::SourceNotFound(src.display().to_string()));
    }
    // If destination exists
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
        // Remove the destination (symlink or otherwise)
        fs::remove_file(dst)?;
    }
    unix_fs::symlink(src, dst)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::env;
    use std::fs;

    #[test]
    fn test_atomic_symlink_success() {
        let tmp_dir = env::temp_dir();
        let src = tmp_dir.join("hermitgrab_test_src");
        let dst = tmp_dir.join("hermitgrab_test_dst");
        fs::write(&src, b"test").unwrap();
        atomic_symlink(&src, &dst).unwrap();
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
        let result = atomic_symlink(&src, &dst);
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
        let result = atomic_symlink(&src, &dst);
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
        unix_fs::symlink(&src, &dst).unwrap();
        let result = atomic_symlink(&src, &dst);
        assert!(result.is_ok());
        fs::remove_file(&src).unwrap();
        fs::remove_file(&dst).unwrap();
    }
}
