//! Stream C's hand-off points for media other streams add. Each `*_job`
//! returns `None` ("unsupported") until its stream is merged.

use std::path::Path;

use ferry_core::pipeline::Job;
use ferry_core::settings::Settings;
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

pub const VIDEO: bool = true;
pub const AUDIO: bool = true;
/// Stream E (PDF) is not merged yet.
pub const PDF: bool = false;

/// Which media types the Settings pane may offer.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Support {
    pub image: bool,
    pub video: bool,
    pub audio: bool,
    pub pdf: bool,
}

pub fn support() -> Support {
    Support {
        image: true,
        video: VIDEO,
        audio: AUDIO,
        pdf: PDF,
    }
}

/// A job for a watched video, the way images and audio go through `Job::file`.
pub fn video_job(app: &AppHandle, path: &Path, settings: &Settings) -> Option<Result<Job, String>> {
    VIDEO.then(|| Job::file(path, settings, has_gifsicle(app)))
}

pub fn audio_job(app: &AppHandle, path: &Path, settings: &Settings) -> Option<Result<Job, String>> {
    AUDIO.then(|| Job::file(path, settings, has_gifsicle(app)))
}

/// Replaced when stream E merges.
pub fn pdf_job(
    _app: &AppHandle,
    _path: &Path,
    _settings: &Settings,
) -> Option<Result<Job, String>> {
    None
}

fn has_gifsicle(app: &AppHandle) -> bool {
    app.state::<AppState>().gifsicle.is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn support_matches_the_hook_flags() {
        let s = support();
        assert!(s.image);
        assert_eq!((s.video, s.audio, s.pdf), (VIDEO, AUDIO, PDF));
    }
}
