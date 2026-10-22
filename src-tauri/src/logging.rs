//! Structured logs (spec §4.3, §6): `ferry.<date>.log` in the app's log folder,
//! a new file each day and the last 7 kept. No file contents are logged.

use std::fs;
use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

/// Start logging to `dir`. Keep the guard alive for the app's lifetime: dropping
/// it flushes and stops the writer thread. `None` if the folder is unusable.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ferry")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .map_err(|e| eprintln!("ferry: logging is off: {e}"))
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .ok()?;
    Some(guard)
}

/// The newest log file, for an Error card's "Details".
pub fn latest(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ferry.") && n.ends_with(".log"))
        })
        .max()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_log_is_the_newest_dated_file() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["ferry.2026-10-20.log", "ferry.2026-10-22.log", "other.log"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        assert_eq!(
            latest(dir.path()),
            Some(dir.path().join("ferry.2026-10-22.log"))
        );
        assert_eq!(latest(&dir.path().join("missing")), None);
    }
}
