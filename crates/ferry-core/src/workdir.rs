//! "Force clean working directory" (spec §3, §5.6): finished jobs' working
//! files go; files that queued or running jobs still use stay.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What a clean removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanReport {
    pub files: u32,
    pub bytes: u64,
}

/// Delete everything under `root` except the paths in `keep` and the folders
/// that lead to them. `root` itself stays, and a missing `root` is already
/// clean. A symlink is removed as a link; what it points to is never touched.
pub fn clean(root: &Path, keep: &[PathBuf]) -> io::Result<CleanReport> {
    let mut report = CleanReport::default();
    match clean_dir(root, keep, &mut report) {
        Ok(()) => Ok(report),
        Err(e) if e.kind() == io::ErrorKind::NotFound && !root.exists() => Ok(report),
        Err(e) => Err(e),
    }
}

fn clean_dir(dir: &Path, keep: &[PathBuf], report: &mut CleanReport) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if keep.contains(&path) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            if keep.iter().any(|k| k.starts_with(&path)) {
                clean_dir(&path, keep, report)?;
            } else {
                tally(&path, report);
                fs::remove_dir_all(&path)?;
            }
        } else {
            report.files += 1;
            report.bytes += entry.metadata().map_or(0, |m| m.len());
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Count what `remove_dir_all` is about to delete.
fn tally(dir: &Path, report: &mut CleanReport) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => tally(&entry.path(), report),
            _ => {
                report.files += 1;
                report.bytes += entry.metadata().map_or(0, |m| m.len());
            }
        }
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
                bytes: 16
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
        assert_eq!(report, CleanReport { files: 2, bytes: 7 });
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
}
