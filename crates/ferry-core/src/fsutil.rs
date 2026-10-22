//! Writing results back over the user's files without ever leaving a
//! half-written one, and carrying file metadata across the replacement.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
#[cfg(any(target_os = "macos", test))]
use std::sync::mpsc;
#[cfg(any(target_os = "macos", test))]
use std::thread;
use std::time::Duration;

/// How long extended attributes may take to copy (NFR-P4).
pub const XATTR_TIMEOUT: Duration = Duration::from_secs(5);

/// Replace `target` with `bytes`: write `<target>.ferry-tmp` beside it, `fsync`
/// it, then rename it over the target. If anything fails the original is untouched
/// and the temporary file is removed. A symlink target is followed, so the link
/// survives and its destination is the file that changes.
pub fn atomic_replace(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = if fs::symlink_metadata(target).is_ok_and(|m| m.is_symlink()) {
        fs::canonicalize(target)?
    } else {
        target.to_owned()
    };
    let tmp = temp_path(&target)?;
    let result = write_synced(&tmp, bytes)
        .and_then(|()| match fs::metadata(&target) {
            // Keep the original's permissions (a 0600 file must not become 0644).
            Ok(meta) => fs::set_permissions(&tmp, meta.permissions()),
            Err(_) => Ok(()),
        })
        .and_then(|()| fs::rename(&tmp, &target));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn temp_path(target: &Path) -> io::Result<PathBuf> {
    let mut name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_owned();
    name.push(".ferry-tmp");
    Ok(target.with_file_name(name))
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Give `to` the modified, accessed and (macOS, Windows) created times of `from`.
pub fn copy_times(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::metadata(from)?;
    #[allow(unused_mut)]
    let mut times = FileTimes::new()
        .set_accessed(meta.accessed()?)
        .set_modified(meta.modified()?);
    #[cfg(target_os = "macos")]
    if let Ok(created) = meta.created() {
        use std::os::macos::fs::FileTimesExt;
        times = times.set_created(created);
    }
    #[cfg(windows)]
    if let Ok(created) = meta.created() {
        use std::os::windows::fs::FileTimesExt;
        times = times.set_created(created);
    }
    OpenOptions::new().write(true).open(to)?.set_times(times)
}

/// Copy every extended attribute from `from` to `to`, including
/// `com.apple.metadata:kMDItemIsScreenCapture`, giving up after [`XATTR_TIMEOUT`].
/// Replacing a file drops its attributes, so call this after `atomic_replace`
/// with the backed-up original as `from`.
#[cfg(target_os = "macos")]
pub fn copy_xattrs(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (from.to_owned(), to.to_owned());
    with_timeout(XATTR_TIMEOUT, move || xattr::copy(&from, &to))
}

/// Other platforms have no attribute to carry across; Windows alternate data
/// streams such as `Zone.Identifier` belong to the old file.
#[cfg(not(target_os = "macos"))]
pub fn copy_xattrs(_from: &Path, _to: &Path) -> io::Result<()> {
    Ok(())
}

/// Run `work` on its own thread and stop waiting after `limit`. A blocked
/// system call cannot be cancelled, so the thread is left to finish by itself.
/// Only the macOS `copy_xattrs` calls it; the tests run it on every OS.
#[cfg(any(target_os = "macos", test))]
fn with_timeout<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit)
        .unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "timed out")))
}

#[cfg(target_os = "macos")]
mod xattr {
    use std::ffi::{CStr, CString, OsStr};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn copy(from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (c_path(from)?, c_path(to)?);
        for name in names(&from)? {
            let value = get(&from, &name)?;
            // SAFETY: both pointers are NUL-terminated and live for the call.
            let rc = unsafe {
                libc::setxattr(
                    to.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn c_path(path: &Path) -> io::Result<CString> {
        CString::new(OsStr::as_bytes(path.as_os_str()))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))
    }

    fn names(path: &CString) -> io::Result<Vec<CString>> {
        // SAFETY: a null buffer with size 0 asks only for the needed length.
        let len = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0, 0) };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0u8; len as usize];
        // SAFETY: `buf` is `len` bytes long.
        let len = unsafe { libc::listxattr(path.as_ptr(), buf.as_mut_ptr().cast(), buf.len(), 0) };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        buf.truncate(len as usize);
        Ok(buf
            .split(|&b| b == 0)
            .filter(|n| !n.is_empty())
            .map(|n| {
                CStr::from_bytes_with_nul(&[n, &[0]].concat())
                    .expect("one NUL")
                    .to_owned()
            })
            .collect())
    }

    fn get(path: &CString, name: &CString) -> io::Result<Vec<u8>> {
        // SAFETY: a null buffer with size 0 asks only for the needed length.
        let len =
            unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut value = vec![0u8; len as usize];
        // SAFETY: `value` is `len` bytes long.
        let len = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        value.truncate(len as usize);
        Ok(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::testutil::{get_xattr, set_xattr};
    use std::time::{Instant, UNIX_EPOCH};

    #[test]
    fn atomic_replace_swaps_the_content_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.png");
        fs::write(&target, b"old").unwrap();
        atomic_replace(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!dir.path().join("a.png.ferry-tmp").exists());
    }

    #[test]
    fn a_failed_rename_leaves_the_original_untouched_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        // Renaming a file over a non-empty directory fails on every OS.
        let target = dir.path().join("a.png");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inner.txt"), b"keep").unwrap();
        assert!(atomic_replace(&target, b"new").is_err());
        assert_eq!(fs::read(target.join("inner.txt")).unwrap(), b"keep");
        assert!(!dir.path().join("a.png.ferry-tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_keeps_the_original_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("private.png");
        fs::write(&target, b"old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        atomic_replace(&target, b"new").unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_through_a_symlink_keeps_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.png");
        let link = dir.path().join("link.png");
        fs::write(&real, b"old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        atomic_replace(&link, b"new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read(&real).unwrap(), b"new");
    }

    #[test]
    fn copy_times_carries_modified_and_created_times() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from.png");
        let to = dir.path().join("to.png");
        fs::write(&from, b"a").unwrap();
        fs::write(&to, b"b").unwrap();
        // 2001-09-09, long before either file existed.
        let then = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        #[allow(unused_mut)]
        let mut times = FileTimes::new().set_modified(then).set_accessed(then);
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::FileTimesExt;
            times = times.set_created(then);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTimesExt;
            times = times.set_created(then);
        }
        OpenOptions::new()
            .write(true)
            .open(&from)
            .unwrap()
            .set_times(times)
            .unwrap();

        copy_times(&from, &to).unwrap();

        let meta = fs::metadata(&to).unwrap();
        assert_eq!(meta.modified().unwrap(), then);
        #[cfg(any(target_os = "macos", windows))]
        assert_eq!(meta.created().unwrap(), then);
    }

    #[test]
    fn with_timeout_gives_up_on_slow_work() {
        let start = Instant::now();
        let err = with_timeout(Duration::from_millis(50), || {
            thread::sleep(Duration::from_secs(2));
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn with_timeout_returns_the_work_result() {
        assert_eq!(with_timeout(Duration::from_secs(1), || Ok(7)).unwrap(), 7);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_xattrs_carries_the_screenshot_marker_and_binary_values() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from.png");
        let to = dir.path().join("to.png");
        fs::write(&from, b"a").unwrap();
        fs::write(&to, b"b").unwrap();
        let marker = c"com.apple.metadata:kMDItemIsScreenCapture";
        let value = [0u8, 1, 2, 255, 254];
        set_xattr(&from, marker, &value);
        set_xattr(&from, c"user.note", b"hello");

        copy_xattrs(&from, &to).unwrap();

        assert_eq!(get_xattr(&to, marker), value);
        assert_eq!(get_xattr(&to, c"user.note"), b"hello");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_xattrs_from_a_missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("to.png");
        fs::write(&to, b"b").unwrap();
        assert!(copy_xattrs(&dir.path().join("missing.png"), &to).is_err());
    }
}
