//! Video: formats, codecs and (in the child modules) probing, hardware encoders
//! and the encode plan. Pure logic; the ffmpeg runner is `crate::ffmpeg`.

use std::{fs::File, io::Read, path::Path};

use crate::sniff::{self, Format};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec {
    H264,
    Hevc,
    Av1,
    Vp9,
}

impl Codec {
    pub fn label(self) -> &'static str {
        match self {
            Codec::H264 => "H.264",
            Codec::Hevc => "HEVC",
            Codec::Av1 => "AV1",
            Codec::Vp9 => "VP9",
        }
    }
}

/// The format's serde name, as menu ids spell it.
pub fn format_id(f: Format) -> &'static str {
    match f {
        Format::Png => "png",
        Format::Jpeg => "jpeg",
        Format::Gif => "gif",
        Format::WebP => "webp",
        Format::Mp4 => "mp4",
        Format::Hevc => "hevc",
        Format::Av1 => "av1",
        Format::WebM => "webm",
        Format::Mov => "mov",
        Format::Mkv => "mkv",
    }
}

/// The formats the format bar offers on a video card.
pub fn is_target(f: Format) -> bool {
    matches!(
        f,
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Gif
    )
}

/// Whether the file's first 64 bytes are a video container.
pub fn is_video_file(path: &Path) -> bool {
    let mut head = Vec::new();
    File::open(path)
        .and_then(|file| file.take(64).read_to_end(&mut head))
        .is_ok()
        && sniff::sniff(&head).is_some_and(Format::is_video)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_are_the_format_bar_chips() {
        assert!(
            [
                Format::Mp4,
                Format::Hevc,
                Format::Av1,
                Format::WebM,
                Format::Gif
            ]
            .into_iter()
            .all(is_target)
        );
        assert!(!is_target(Format::Mov) && !is_target(Format::Mkv) && !is_target(Format::Png));
        assert_eq!(format_id(Format::Hevc), "hevc");
    }

    #[test]
    fn a_file_is_a_video_by_its_first_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let mp4 = dir.path().join("x.png");
        std::fs::write(&mp4, [&[0, 0, 0, 0x18][..], b"ftypisom", &[0; 12]].concat()).unwrap();
        assert!(is_video_file(&mp4));
        let png = dir.path().join("x.mp4");
        std::fs::write(&png, crate::testutil::png(4, 4)).unwrap();
        assert!(!is_video_file(&png));
        assert!(!is_video_file(&dir.path().join("missing.mp4")));
    }
}
