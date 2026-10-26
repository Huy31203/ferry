//! A ledger of the files Ferry itself wrote, so a watcher that sees one of them
//! change never starts a job on it (R§3.3 "No self-loops"). An entry is a path
//! plus the `(len, mtime)` it had when Ferry finished writing; a file whose
//! length differs (the user edited it since) is not ours. A changed date alone
//! is not an edit.

use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant, SystemTime};

/// How long Ferry remembers a write.
pub const TTL: Duration = Duration::from_secs(600);
/// The most paths remembered at once.
pub const CAP: usize = 4096;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Stamp {
    pub len: u64,
    pub modified: SystemTime,
}

impl Stamp {
    /// Follows symlinks.
    pub fn of(path: &Path) -> Option<Stamp> {
        let meta = fs::metadata(path).ok()?;
        Some(Stamp {
            len: meta.len(),
            modified: meta.modified().ok()?,
        })
    }
}

/// One identity for a path however it was spelled: FSEvents reports
/// `/private/var/…` for `/var/…`, and Windows paths ignore case.
#[cfg(not(windows))]
pub fn key(path: &Path) -> PathBuf {
    if let Ok(real) = fs::canonicalize(path) {
        return real;
    }
    match (path.parent().map(fs::canonicalize), path.file_name()) {
        (Some(Ok(parent)), Some(name)) => parent.join(name),
        _ => path.to_owned(),
    }
}

#[cfg(windows)]
pub fn key(path: &Path) -> PathBuf {
    let absolute = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
    let text = absolute.to_string_lossy().to_lowercase();
    PathBuf::from(text.strip_prefix("\\\\?\\").unwrap_or(&text))
}

pub struct Ledger {
    ttl: Duration,
    cap: usize,
    entries: HashMap<PathBuf, (Stamp, Instant)>,
}

impl Ledger {
    pub fn new(ttl: Duration, cap: usize) -> Self {
        Self {
            ttl,
            cap,
            entries: HashMap::new(),
        }
    }

    pub fn record(&mut self, key: PathBuf, stamp: Stamp, now: Instant) {
        if self.entries.len() >= self.cap && !self.entries.contains_key(&key) {
            let ttl = self.ttl;
            self.entries
                .retain(|_, (_, at)| now.duration_since(*at) < ttl);
            if self.entries.len() >= self.cap {
                let oldest = self
                    .entries
                    .iter()
                    .min_by_key(|(_, (_, at))| *at)
                    .map(|(k, _)| k.clone());
                if let Some(oldest) = oldest {
                    self.entries.remove(&oldest);
                }
            }
        }
        self.entries.insert(key, (stamp, now));
    }

    /// A known path got new times (Ferry restored the original dates).
    pub fn refresh(&mut self, key: &Path, stamp: Stamp, now: Instant) {
        if self.entries.contains_key(key) {
            self.entries.insert(key.to_owned(), (stamp, now));
        }
    }

    /// Compares the length only: Spotlight, tag edits and sync clients change
    /// the dates of a file Ferry wrote, and that is not the user's edit.
    pub fn is_own(&self, key: &Path, stamp: &Stamp, now: Instant) -> bool {
        self.entries
            .get(key)
            .is_some_and(|(own, at)| own.len == stamp.len && now.duration_since(*at) < self.ttl)
    }
}

static LEDGER: LazyLock<Mutex<Ledger>> = LazyLock::new(|| Mutex::new(Ledger::new(TTL, CAP)));

fn ledger() -> std::sync::MutexGuard<'static, Ledger> {
    LEDGER.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Ferry just finished writing `path`.
pub fn record(path: &Path) {
    if let Some(stamp) = Stamp::of(path) {
        ledger().record(key(path), stamp, Instant::now());
    }
}

/// Ferry changed the times of `path`; a no-op unless it was recorded.
pub fn refresh(path: &Path) {
    if let Some(stamp) = Stamp::of(path) {
        ledger().refresh(&key(path), stamp, Instant::now());
    }
}

/// Is this exactly what Ferry last wrote to `path`?
pub fn is_own(path: &Path, stamp: &Stamp) -> bool {
    ledger().is_own(&key(path), stamp, Instant::now())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::UNIX_EPOCH;

    fn stamp(len: u64) -> Stamp {
        Stamp {
            len,
            modified: UNIX_EPOCH + Duration::from_secs(len),
        }
    }

    #[test]
    fn a_recorded_file_is_own_until_it_changes() {
        let t0 = Instant::now();
        let mut l = Ledger::new(TTL, CAP);
        l.record("/w/a.png".into(), stamp(10), t0);
        assert!(l.is_own(Path::new("/w/a.png"), &stamp(10), t0));
        assert!(
            !l.is_own(Path::new("/w/a.png"), &stamp(11), t0),
            "the user's later edit is not ours"
        );
        assert!(!l.is_own(Path::new("/w/b.png"), &stamp(10), t0));
    }

    #[test]
    fn a_touched_file_is_still_own() {
        let t0 = Instant::now();
        let mut l = Ledger::new(TTL, CAP);
        l.record("/w/a.png".into(), stamp(10), t0);
        let touched = Stamp {
            len: 10,
            modified: UNIX_EPOCH + Duration::from_secs(99),
        };
        assert!(l.is_own(Path::new("/w/a.png"), &touched, t0));
    }

    #[test]
    fn a_record_expires_after_the_ttl() {
        let t0 = Instant::now();
        let mut l = Ledger::new(TTL, CAP);
        l.record("/w/a.png".into(), stamp(10), t0);
        let key = Path::new("/w/a.png");
        assert!(l.is_own(key, &stamp(10), t0 + TTL - Duration::from_millis(1)));
        assert!(!l.is_own(key, &stamp(10), t0 + TTL));
    }

    #[test]
    fn the_ledger_never_holds_more_than_its_cap() {
        let t0 = Instant::now();
        let mut l = Ledger::new(TTL, 3);
        for n in 0..5u64 {
            let at = t0 + Duration::from_secs(n);
            l.record(format!("/w/{n}.png").into(), stamp(n + 1), at);
        }
        let now = t0 + Duration::from_secs(5);
        let own = |n: u64| l.is_own(Path::new(&format!("/w/{n}.png")), &stamp(n + 1), now);
        assert!(!own(0) && !own(1));
        assert!(own(2) && own(3) && own(4));
    }

    #[test]
    fn refresh_only_updates_known_paths() {
        let t0 = Instant::now();
        let mut l = Ledger::new(TTL, CAP);
        l.refresh(Path::new("/w/a.png"), stamp(5), t0);
        assert!(!l.is_own(Path::new("/w/a.png"), &stamp(5), t0));
        l.record("/w/a.png".into(), stamp(10), t0);
        l.refresh(Path::new("/w/a.png"), stamp(20), t0);
        assert!(l.is_own(Path::new("/w/a.png"), &stamp(20), t0));
        assert!(!l.is_own(Path::new("/w/a.png"), &stamp(10), t0));
    }

    #[test]
    fn atomic_replace_marks_its_result_as_own() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        fs::write(&p, b"old").unwrap();
        crate::fsutil::atomic_replace(&p, b"new bytes").unwrap();
        assert!(is_own(&p, &Stamp::of(&p).unwrap()));
        fs::write(&p, b"user edit, longer").unwrap();
        assert!(!is_own(&p, &Stamp::of(&p).unwrap()));
    }

    #[test]
    fn atomic_replace_from_marks_its_result_as_own() {
        let dir = tempfile::tempdir().unwrap();
        let (p, src) = (dir.path().join("a.mp4"), dir.path().join("src.mp4"));
        fs::write(&p, b"old").unwrap();
        fs::write(&src, b"converted").unwrap();
        crate::fsutil::atomic_replace_from(&p, &src).unwrap();
        assert!(is_own(&p, &Stamp::of(&p).unwrap()));
    }

    #[test]
    fn set_times_keeps_the_mark_on_a_dated_result() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.png");
        fs::write(&p, b"old").unwrap();
        let old = crate::fsutil::read_times(&p).unwrap();
        let old = crate::fsutil::Times {
            modified: old.modified - Duration::from_secs(86_400),
            ..old
        };
        crate::fsutil::atomic_replace(&p, b"new bytes").unwrap();
        crate::fsutil::set_times(&p, &old).unwrap();
        let stamp = Stamp::of(&p).unwrap();
        assert_eq!(stamp.modified, old.modified);
        assert!(is_own(&p, &stamp));
    }

    #[cfg(unix)]
    #[test]
    fn keys_match_through_a_symlinked_folder() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real");
        let link = dir.path().join("link");
        fs::create_dir(&real).unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        fs::write(real.join("a.png"), b"x").unwrap();
        record(&link.join("a.png"));
        let seen = real.join("a.png");
        assert!(is_own(&seen, &Stamp::of(&seen).unwrap()));
    }

    #[test]
    fn keys_survive_spaces_and_narrow_nbsp() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir
            .path()
            .join("Screenshot 2026-10-23 at 10.21.33\u{202F}AM.png");
        fs::write(&p, b"old").unwrap();
        crate::fsutil::atomic_replace(&p, b"new bytes").unwrap();
        assert!(is_own(&p, &Stamp::of(&p).unwrap()));
    }

    #[cfg(windows)]
    #[test]
    fn keys_ignore_case_on_windows() {
        let a = key(Path::new("C:\\X\\A.PNG"));
        assert_eq!(a, key(Path::new("c:\\x\\a.png")));
        assert!(!a.to_string_lossy().starts_with("\\\\?\\"));
    }
}
