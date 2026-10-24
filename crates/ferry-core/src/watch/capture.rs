//! Where screenshots and screen recordings land, and which files in those
//! folders count as captures.

use std::path::{Path, PathBuf};

use serde::Serialize;

use super::media::Media;
use crate::settings::{Captures, expand_home};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureKind {
    Screenshot,
    Recording,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureRule {
    /// macOS: the folder also holds other files, so an image is a screenshot
    /// only with the capture marker, and a video is a recording with the marker
    /// or a name starting with one of the prefixes.
    Marked { recording_prefixes: Vec<String> },
    /// Windows: these folders hold only captures.
    Everything,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureDir {
    pub dir: PathBuf,
    pub rule: CaptureRule,
}

/// The attribute `screencaptureui` writes on every screenshot.
pub const SCREEN_CAPTURE_XATTR: &std::ffi::CStr = c"com.apple.metadata:kMDItemIsScreenCapture";
/// English only; whether recordings carry the marker is still to be checked.
pub const RECORDING_PREFIX: &str = "Screen Recording";

/// The screenshot location (a `~` is expanded), or the Desktop when it is
/// unset, blank or not a folder.
pub fn macos_dirs(
    location: Option<&str>,
    home: &Path,
    is_dir: &dyn Fn(&Path) -> bool,
) -> Vec<CaptureDir> {
    let dir = location
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| expand_home(l, home))
        .filter(|d| is_dir(d))
        .unwrap_or_else(|| home.join("Desktop"));
    vec![CaptureDir {
        dir,
        rule: CaptureRule::Marked {
            recording_prefixes: vec![RECORDING_PREFIX.into()],
        },
    }]
}

/// The Screenshots folder, the Game Bar `Videos\Captures` and the Snipping
/// Tool `Videos\Screen Recordings`.
pub fn windows_dirs(screenshots: Option<PathBuf>, videos: Option<PathBuf>) -> Vec<CaptureDir> {
    let everything = |dir: PathBuf| CaptureDir {
        dir,
        rule: CaptureRule::Everything,
    };
    let mut dirs: Vec<CaptureDir> = screenshots.into_iter().map(everything).collect();
    if let Some(videos) = videos {
        dirs.push(everything(videos.join("Captures")));
        dirs.push(everything(videos.join("Screen Recordings")));
    }
    dirs
}

/// What `name` is in a capture folder, if anything. `marked` is whether the file
/// carries the capture marker (only asked for on macOS).
pub fn capture_kind(
    rule: &CaptureRule,
    name: &str,
    media: Media,
    marked: bool,
) -> Option<CaptureKind> {
    match (rule, media) {
        (CaptureRule::Everything, Media::Image) => Some(CaptureKind::Screenshot),
        (CaptureRule::Everything, Media::Video) => Some(CaptureKind::Recording),
        (CaptureRule::Marked { .. }, Media::Image) => marked.then_some(CaptureKind::Screenshot),
        (CaptureRule::Marked { recording_prefixes }, Media::Video) => {
            let lower = name.to_lowercase();
            let named = recording_prefixes
                .iter()
                .any(|prefix| lower.starts_with(&prefix.to_lowercase()));
            (marked || named).then_some(CaptureKind::Recording)
        }
        _ => None,
    }
}

pub fn wanted(kind: CaptureKind, captures: &Captures) -> bool {
    match kind {
        CaptureKind::Screenshot => captures.screenshots,
        CaptureKind::Recording => captures.recordings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_uses_the_screenshot_location_or_the_desktop() {
        let home = Path::new("/Users/me");
        let exists =
            |p: &Path| p == Path::new("/Users/me/Pictures/Shots") || p == Path::new("/Volumes/X");
        assert_eq!(
            macos_dirs(None, home, &exists)[0].dir,
            Path::new("/Users/me/Desktop")
        );
        assert_eq!(
            macos_dirs(Some("  "), home, &exists)[0].dir,
            Path::new("/Users/me/Desktop")
        );
        assert_eq!(
            macos_dirs(Some("~/Pictures/Shots"), home, &exists)[0].dir,
            Path::new("/Users/me/Pictures/Shots")
        );
        assert_eq!(
            macos_dirs(Some("/Volumes/X"), home, &exists)[0].dir,
            Path::new("/Volumes/X")
        );
        assert_eq!(
            macos_dirs(Some("/gone"), home, &exists)[0].dir,
            Path::new("/Users/me/Desktop")
        );
    }

    #[test]
    fn windows_watches_screenshots_captures_and_screen_recordings() {
        let dirs = windows_dirs(
            Some("C:\\U\\Pictures\\Screenshots".into()),
            Some("C:\\U\\Videos".into()),
        );
        assert_eq!(dirs.len(), 3);
        assert_eq!(dirs[1].dir, Path::new("C:\\U\\Videos").join("Captures"));
        assert_eq!(
            dirs[2].dir,
            Path::new("C:\\U\\Videos").join("Screen Recordings")
        );
        assert!(dirs.iter().all(|d| d.rule == CaptureRule::Everything));
        assert_eq!(windows_dirs(None, Some("V".into())).len(), 2);
    }

    #[test]
    fn capture_kind_needs_the_marker_or_the_recording_name_on_macos() {
        let mac = CaptureRule::Marked {
            recording_prefixes: vec![RECORDING_PREFIX.into()],
        };
        assert_eq!(
            capture_kind(&mac, "x.png", Media::Image, true),
            Some(CaptureKind::Screenshot)
        );
        assert_eq!(capture_kind(&mac, "x.png", Media::Image, false), None);
        assert_eq!(
            capture_kind(&mac, "screen recording 2026.mov", Media::Video, false),
            Some(CaptureKind::Recording)
        );
        assert_eq!(capture_kind(&mac, "holiday.mov", Media::Video, false), None);
        assert_eq!(
            capture_kind(&mac, "holiday.mov", Media::Video, true),
            Some(CaptureKind::Recording)
        );
        assert_eq!(
            capture_kind(&CaptureRule::Everything, "a.png", Media::Image, false),
            Some(CaptureKind::Screenshot)
        );
        assert_eq!(
            capture_kind(&CaptureRule::Everything, "a.pdf", Media::Pdf, false),
            None
        );
    }

    #[test]
    fn capture_toggles_gate_each_kind() {
        let off = Captures::default();
        assert!(!wanted(CaptureKind::Screenshot, &off) && !wanted(CaptureKind::Recording, &off));
        let rec = Captures {
            recordings: true,
            ..Captures::default()
        };
        assert!(!wanted(CaptureKind::Screenshot, &rec) && wanted(CaptureKind::Recording, &rec));
        let shot = Captures {
            screenshots: true,
            ..Captures::default()
        };
        assert!(wanted(CaptureKind::Screenshot, &shot) && !wanted(CaptureKind::Recording, &shot));
    }
}
