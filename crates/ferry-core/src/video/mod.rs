//! Video: formats, codecs and (in the child modules) probing, hardware encoders
//! and the encode plan. Pure logic; the ffmpeg runner is `crate::ffmpeg`.

use std::{fs::File, io::Read, path::Path};

use crate::sniff::{self, Format};

pub mod fit;
pub mod hardware;
pub mod plan;
pub mod probe;

/// A GIF made from a video: its frame rate, and the long edge it is capped at.
pub const GIF_FPS: u32 = 15;
pub const GIF_LONG_EDGE: u32 = 800;
/// The card thumbnail's width.
pub const THUMB_WIDTH: u32 = 600;

use probe::Probe;

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

/// A codec as people name it: h264 H.264, hevc HEVC, prores ProRes.
pub fn codec_label(ffmpeg_name: &str) -> String {
    match ffmpeg_name {
        "h264" => "H.264".into(),
        "hevc" => "HEVC".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "vp8" => "VP8".into(),
        "prores" => "ProRes".into(),
        "mpeg4" => "MPEG-4".into(),
        other => other.to_uppercase(),
    }
}

/// The container sniffed from the file, refined by what is inside: HEVC in MP4
/// is `Hevc`, AV1 in MP4 or MKV is `Av1`.
pub fn source_format(container: Format, probe: &Probe) -> Format {
    match (container, probe.video.codec.as_str()) {
        (Format::Mp4, "hevc") => Format::Hevc,
        (Format::Mp4 | Format::Mkv, "av1") => Format::Av1,
        _ => container,
    }
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

    #[test]
    fn the_source_format_follows_the_codec() {
        let probe = |codec: &str| Probe {
            duration: std::time::Duration::ZERO,
            bitrate: None,
            video: probe::VideoStream {
                codec: codec.into(),
                width: 2,
                height: 2,
                fps: None,
                bitrate: None,
                ten_bit: false,
                hdr: false,
            },
            audio: vec![],
            subtitles: 0,
        };
        assert_eq!(source_format(Format::Mp4, &probe("hevc")), Format::Hevc);
        assert_eq!(source_format(Format::Mkv, &probe("av1")), Format::Av1);
        assert_eq!(source_format(Format::Mov, &probe("hevc")), Format::Mov);
        assert_eq!(source_format(Format::WebM, &probe("av1")), Format::WebM);
        assert_eq!(source_format(Format::Mp4, &probe("h264")), Format::Mp4);
    }

    #[test]
    fn codecs_get_their_usual_names() {
        assert_eq!(codec_label("h264"), "H.264");
        assert_eq!(codec_label("prores"), "ProRes");
        assert_eq!(codec_label("dnxhd"), "DNXHD");
    }
}
