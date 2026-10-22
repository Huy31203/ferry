//! Originals are copied here before Ferry overwrites a file (R§2.4), under
//! `<root>/<yyyy-mm-dd>/<job-id>/<name>`, with the date in local time.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Days, Local, NaiveDate};

type Clock = Box<dyn Fn() -> NaiveDate + Send + Sync>;

const DATE_FORMAT: &str = "%Y-%m-%d";

pub struct Backups {
    root: PathBuf,
    today: Clock,
}

/// What `prune` removed, as date-folder names in ascending order.
#[derive(Debug, PartialEq, Eq)]
pub struct PruneReport {
    pub removed: Vec<String>,
}

impl Backups {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_clock(root, || Local::now().date_naive())
    }

    pub fn with_clock(
        root: impl Into<PathBuf>,
        today: impl Fn() -> NaiveDate + Send + Sync + 'static,
    ) -> Self {
        Self {
            root: root.into(),
            today: Box::new(today),
        }
    }

    /// Back up `bytes` as `name`. The first copy for a job wins: a second call
    /// returns the existing path, so an optimised file can never replace its original.
    pub fn store_bytes(&self, job: &str, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        let dest = self.destination(job, name)?;
        if !dest.exists() {
            fs::write(&dest, bytes)?;
        }
        Ok(dest)
    }

    /// Back up the file at `source` under its own file name; first copy wins.
    /// `fs::copy` keeps extended attributes on macOS, so the copy is a valid
    /// source for `fsutil::copy_xattrs`.
    pub fn store_file(&self, job: &str, source: &Path) -> io::Result<PathBuf> {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let dest = self.destination(job, name)?;
        if !dest.exists() {
            fs::copy(source, &dest)?;
        }
        Ok(dest)
    }

    /// Remove date folders more than `retention_days` days before today; today's
    /// folder always stays. Folders whose names are not dates are left alone.
    pub fn prune(&self, retention_days: u32) -> io::Result<PruneReport> {
        let mut removed = Vec::new();
        let Some(cutoff) = (self.today)().checked_sub_days(Days::new(retention_days.into())) else {
            return Ok(PruneReport { removed });
        };
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(PruneReport { removed }),
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if parse_date(&name).is_some_and(|day| day < cutoff) && entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
                removed.push(name);
            }
        }
        removed.sort();
        Ok(PruneReport { removed })
    }

    fn destination(&self, job: &str, name: &str) -> io::Result<PathBuf> {
        // Only the last component of each, so neither can climb out of the root.
        let name = Path::new(name)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty backup name"))?;
        let job = Path::new(job)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty job id"))?;
        let dir = self
            .root
            .join((self.today)().format(DATE_FORMAT).to_string())
            .join(job);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(name))
    }
}

/// Exactly `yyyy-mm-dd`; `2026-2-3` or `2026-02-31` are not Ferry's folders.
fn parse_date(name: &str) -> Option<NaiveDate> {
    let date = NaiveDate::parse_from_str(name, DATE_FORMAT).ok()?;
    (date.format(DATE_FORMAT).to_string() == name).then_some(date)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn backups(root: &Path) -> Backups {
        Backups::with_clock(root, || day(2026, 10, 22))
    }

    #[test]
    fn only_exact_iso_dates_are_date_folders() {
        assert_eq!(parse_date("2026-10-22"), Some(day(2026, 10, 22)));
        assert_eq!(parse_date("2026-02-31"), None);
        assert_eq!(parse_date("2026-2-3"), None);
        assert_eq!(parse_date("notes"), None);
    }

    #[test]
    fn bytes_are_stored_under_date_and_job_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = backups(dir.path())
            .store_bytes("job1", "shot.png", b"original")
            .unwrap();
        assert_eq!(path, dir.path().join("2026-10-22/job1/shot.png"));
        assert_eq!(fs::read(path).unwrap(), b"original");
    }

    #[test]
    fn a_file_is_copied_under_its_own_name() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.jpg");
        fs::write(&source, b"pixels").unwrap();
        let path = backups(&dir.path().join("backups"))
            .store_file("job2", &source)
            .unwrap();
        assert!(path.ends_with("2026-10-22/job2/photo.jpg"));
        assert_eq!(fs::read(path).unwrap(), b"pixels");
        assert_eq!(
            fs::read(&source).unwrap(),
            b"pixels",
            "source stays in place"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_stored_file_keeps_its_screenshot_marker() {
        use crate::testutil::{get_xattr, set_xattr};
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Screenshot.png");
        fs::write(&source, b"pixels").unwrap();
        let marker = c"com.apple.metadata:kMDItemIsScreenCapture";
        set_xattr(&source, marker, b"1");
        let path = backups(&dir.path().join("backups"))
            .store_file("job", &source)
            .unwrap();
        assert_eq!(get_xattr(&path, marker), b"1");
    }

    #[test]
    fn the_first_copy_for_a_job_is_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let b = backups(dir.path());
        b.store_bytes("job", "a.png", b"original").unwrap();
        let path = b.store_bytes("job", "a.png", b"already optimised").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"original");
    }

    #[test]
    fn a_name_cannot_climb_out_of_the_backup_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("backups");
        let path = backups(&root)
            .store_bytes("../job", "../../evil.png", b"x")
            .unwrap();
        assert!(path.starts_with(&root), "{path:?}");
        assert!(!dir.path().join("evil.png").exists());
    }

    #[test]
    fn prune_removes_whole_folders_older_than_retention_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "2026-10-10",
            "2026-10-14",
            "2026-10-15",
            "2026-10-22",
            "keep-me",
        ] {
            fs::create_dir_all(root.join(name).join("job")).unwrap();
            fs::write(root.join(name).join("job/f.png"), b"x").unwrap();
        }
        // 7 days before 2026-10-22 is 2026-10-15, which stays.
        let report = backups(root).prune(7).unwrap();
        assert_eq!(report.removed, ["2026-10-10", "2026-10-14"]);
        assert!(!root.join("2026-10-14").exists());
        for name in ["2026-10-15", "2026-10-22", "keep-me"] {
            assert!(root.join(name).exists(), "{name} was removed");
        }
    }

    #[test]
    fn prune_with_zero_days_keeps_today() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["2026-10-21", "2026-10-22"] {
            fs::create_dir_all(dir.path().join(name)).unwrap();
        }
        let report = backups(dir.path()).prune(0).unwrap();
        assert_eq!(report.removed, ["2026-10-21"]);
        assert!(dir.path().join("2026-10-22").exists());
    }

    #[test]
    fn prune_on_a_missing_root_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let report = backups(&dir.path().join("never-created")).prune(1).unwrap();
        assert!(report.removed.is_empty());
    }
}
