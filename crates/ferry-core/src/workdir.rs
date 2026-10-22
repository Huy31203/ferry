//! "Force clean working directory" (spec §3, §5.6): finished jobs' working
//! files go; files that queued or running jobs still use stay. A file that
//! cannot be removed (another program holds it open) is counted, never fatal:
//! the rest is still cleaned.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What a clean removed, and what it could not.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanReport {
    pub files: u32,
    pub bytes: u64,
    /// Entries that could not be removed and were left in place.
    pub failed: u32,
}

/// Delete everything under `root` except the paths in `keep` and the folders
/// that lead to them. `root` itself stays, and a missing `root` is already
/// clean. A symlink is removed as a link; what it points to is never touched.
/// Only an unreadable `root` is an error.
pub fn clean(root: &Path, keep: &[PathBuf]) -> io::Result<CleanReport> {
    let mut report = CleanReport::default();
    match fs::read_dir(root) {
        Ok(_) => clean_dir(root, keep, &mut report),
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(e) => return Err(e),
    }
    Ok(report)
}

fn clean_dir(dir: &Path, keep: &[PathBuf], report: &mut CleanReport) {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return,
        Err(_) => {
            report.failed += 1;
            return;
        }
    };
    for entry in entries {
        let Ok(entry) = entry else {
            report.failed += 1;
            continue;
        };
        let path = entry.path();
        if keep.contains(&path) {
            continue;
        }
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => {
                if keep.iter().any(|k| k.starts_with(&path)) {
                    clean_dir(&path, keep, report);
                } else {
                    remove_tree(&path, report);
                }
            }
            Ok(kind) => remove_file(&entry, kind.is_symlink(), report),
            Err(_) => report.failed += 1,
        }
    }
}

/// Remove a folder and everything in it; the folder stays if anything in it did.
fn remove_tree(dir: &Path, report: &mut CleanReport) {
    let before = report.failed;
    clean_dir(dir, &[], report);
    if report.failed == before {
        match fs::remove_dir(dir) {
            Ok(()) => {}
            Err(e) if e.kind() == io::ErrorKind::NotFound => {}
            Err(_) => report.failed += 1,
        }
    }
}

fn remove_file(entry: &fs::DirEntry, symlink: bool, report: &mut CleanReport) {
    let path = entry.path();
    let len = entry.metadata().map_or(0, |m| m.len());
    let mut result = fs::remove_file(&path);
    // On Windows a link to a folder is removed like a folder.
    if result.is_err() && symlink {
        result = fs::remove_dir(&path);
    }
    match result {
        Ok(()) => {
            report.files += 1;
            report.bytes += len;
        }
        // Already gone, for example from a second clean running at once.
        Err(e) if e.kind() == io::ErrorKind::NotFound => {}
        Err(_) => report.failed += 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0; bytes]).unwrap();
    }

    #[test]
    fn finished_jobs_files_and_folders_go() {
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("clipboard/job-1/a.png"), 10);
        file(&dir.path().join("clipboard/job-2/b.png"), 5);
        file(&dir.path().join("loose.txt"), 1);
        let report = clean(dir.path(), &[]).unwrap();
        assert_eq!(
            report,
            CleanReport {
                files: 3,
                bytes: 16,
                ..CleanReport::default()
            }
        );
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(dir.path().exists(), "the working directory itself stays");
    }

    #[test]
    fn a_running_jobs_file_and_its_folder_stay() {
        let dir = tempfile::tempdir().unwrap();
        let busy = dir.path().join("clipboard/job-1/a.png");
        file(&busy, 10);
        file(&dir.path().join("clipboard/job-1/stale.tmp"), 2);
        file(&dir.path().join("clipboard/job-2/b.png"), 5);
        let report = clean(dir.path(), std::slice::from_ref(&busy)).unwrap();
        assert!(busy.exists());
        assert!(!dir.path().join("clipboard/job-1/stale.tmp").exists());
        assert!(!dir.path().join("clipboard/job-2").exists());
        assert_eq!(
            report,
            CleanReport {
                files: 2,
                bytes: 7,
                ..CleanReport::default()
            }
        );
    }

    #[test]
    fn a_missing_working_directory_is_already_clean() {
        let dir = tempfile::tempdir().unwrap();
        let report = clean(&dir.path().join("never-created"), &[]).unwrap();
        assert_eq!(report, CleanReport::default());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_removed_not_followed() {
        let outside = tempfile::tempdir().unwrap();
        file(&outside.path().join("precious.txt"), 3);
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        clean(dir.path(), &[]).unwrap();
        assert!(!dir.path().join("link").exists());
        assert!(outside.path().join("precious.txt").exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_file_that_cannot_be_removed_does_not_stop_the_rest() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("a-locked/x.png"), 4);
        file(&dir.path().join("z-open/y.png"), 6);
        let locked = dir.path().join("a-locked");
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
        let report = clean(dir.path(), &[]).unwrap();
        fs::set_permissions(&locked, fs::Permissions::from_mode(0o700)).unwrap();
        assert!(locked.join("x.png").exists(), "it could not be removed");
        assert!(
            !dir.path().join("z-open").exists(),
            "the rest is still cleaned"
        );
        assert_eq!(report.failed, 1);
    }

    #[test]
    fn something_already_gone_is_not_a_failure() {
        let dir = tempfile::tempdir().unwrap();
        let report = clean(dir.path(), &[]).unwrap();
        assert_eq!(report.failed, 0);
    }
}
