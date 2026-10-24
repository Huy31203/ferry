//! Where macOS saves screenshots and screen recordings.

use ferry_core::watch::capture::{CaptureDir, macos_dirs};
use objc2::AllocAnyThread;
use objc2_foundation::{NSString, NSUserDefaults};

/// The folder from ⇧⌘5 › Options (the `location` default of
/// `com.apple.screencapture`), or the Desktop. Read on every call because the
/// user can change it while Ferry runs; Ferry is not sandboxed, so another
/// app's defaults domain is readable. Runs on the watcher thread; no AppKit UI.
pub fn capture_dirs() -> Vec<CaptureDir> {
    let defaults = NSUserDefaults::initWithSuiteName(
        NSUserDefaults::alloc(),
        Some(&NSString::from_str("com.apple.screencapture")),
    );
    let location = defaults
        .and_then(|d| d.stringForKey(&NSString::from_str("location")))
        .map(|s| s.to_string());
    let home = std::env::home_dir().unwrap_or_default();
    macos_dirs(location.as_deref(), &home, &|p| p.is_dir())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn there_is_always_one_capture_folder_on_macos() {
        let dirs = capture_dirs();
        assert_eq!(dirs.len(), 1);
        assert!(dirs[0].dir.is_absolute());
    }
}
