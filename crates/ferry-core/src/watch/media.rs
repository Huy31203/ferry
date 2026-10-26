//! What kind of file a watched file is, by its content first.

use serde::Serialize;

use crate::settings::WatchTypes;
use crate::{sniff, trigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    Image,
    Video,
    Audio,
    Pdf,
}

/// Stream A does not support AVI, so an `.avi` file is left alone.
pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm"];
pub const AUDIO_EXTENSIONS: &[&str] = &["wav", "flac", "aif", "aiff", "mp3", "m4a", "ogg", "opus"];

/// The content wins over the extension; `head` is the first bytes of the file.
pub fn classify(name: &str, head: &[u8]) -> Option<Media> {
    if let Some(format) = sniff::sniff(head) {
        return Some(if format == sniff::Format::Pdf {
            Media::Pdf // sE pdf-formats: sniff knows PDFs now
        } else if format.is_audio() {
            Media::Audio
        } else if format.is_video() {
            Media::Video
        } else {
            Media::Image
        });
    }
    if head.starts_with(b"%PDF-") {
        return Some(Media::Pdf);
    }
    let ext = trigger::normalise_ext(name.rsplit_once('.')?.1);
    if VIDEO_EXTENSIONS.contains(&ext.as_str()) {
        Some(Media::Video)
    } else if AUDIO_EXTENSIONS.contains(&ext.as_str()) {
        Some(Media::Audio)
    } else {
        None
    }
}

impl Media {
    pub fn allowed_by(self, types: &WatchTypes) -> bool {
        match self {
            Media::Image => types.image,
            Media::Video => types.video,
            Media::Audio => types.audio,
            Media::Pdf => types.pdf,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn content_wins_over_the_extension() {
        let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
        assert_eq!(classify("photo.mov", png), Some(Media::Image));
        assert_eq!(classify("x.png", b"garbage"), None);
        assert_eq!(classify("doc.bin", b"%PDF-1.7"), Some(Media::Pdf));
        assert_eq!(
            classify("Screen Recording.MOV", b"\0\0\0\x14ftypqt  "),
            Some(Media::Video)
        );
        assert_eq!(classify("song.flac", b"fLaC"), Some(Media::Audio));
        assert_eq!(classify("notes.txt", b"hello"), None);
    }

    #[test]
    fn audio_is_never_counted_as_an_image() {
        assert_eq!(classify("a.png", b"fLaC"), Some(Media::Audio));
        assert_eq!(
            classify("a.mp3", b"ID3\x04\0\0\0\0\0\0"),
            Some(Media::Audio)
        );
        assert_eq!(
            classify("a.wav", b"RIFF\x24\0\0\0WAVEfmt "),
            Some(Media::Audio)
        );
    }

    #[test]
    fn avi_is_not_a_watched_video() {
        // Stream A does not support AVI, so the watcher must not hand it over.
        assert!(!VIDEO_EXTENSIONS.contains(&"avi"));
        assert_eq!(classify("clip.avi", b"RIFF\x24\0\0\0AVI LIST"), None);
        assert_eq!(classify("CLIP.AVI", b"RIFF\x24\0\0\0AVI LIST"), None);
    }

    #[test]
    fn types_allow_each_media() {
        let types = WatchTypes::default();
        assert!(Media::Image.allowed_by(&types) && Media::Video.allowed_by(&types));
        assert!(!Media::Audio.allowed_by(&types) && !Media::Pdf.allowed_by(&types));
    }
}
