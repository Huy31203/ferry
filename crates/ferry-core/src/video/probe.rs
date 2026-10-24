//! What `ffmpeg -i file` prints about a video: duration, the video stream, the
//! audio streams. ffprobe is not shipped (NFR-R3), so stderr is parsed.

use std::{path::Path, time::Duration};

use crate::{
    cancel::CancelToken,
    error::FerryError,
    ffmpeg,
    i18n::{t, tf},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub duration: Duration,
    pub bitrate: Option<u64>,
    pub video: VideoStream,
    pub audio: Vec<AudioStream>,
    pub subtitles: usize,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    /// ffmpeg's name: "h264", "hevc", "av1", "vp9", "prores", …
    pub codec: String,
    /// As displayed: a ±90° rotation swaps them.
    pub width: u32,
    pub height: u32,
    pub fps: Option<f32>,
    /// Bits per second.
    pub bitrate: Option<u64>,
    /// The pixel format is 10 or 12 bit (yuv420p10le, p010le, …).
    pub ten_bit: bool,
    /// The colour description names PQ or HLG.
    pub hdr: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream {
    pub codec: String,
    pub bitrate: Option<u64>,
}

/// Split on `", "` outside parentheses and brackets.
pub(crate) fn split_top_level(text: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let (mut depth, mut start) = (0_i32, 0);
    let bytes = text.as_bytes();
    for (i, &b) in bytes.iter().enumerate() {
        match b {
            b'(' | b'[' => depth += 1,
            b')' | b']' => depth -= 1,
            b',' if depth == 0 && bytes.get(i + 1) == Some(&b' ') => {
                parts.push(&text[start..i]);
                start = i + 2;
            }
            _ => {}
        }
    }
    parts.push(&text[start..]);
    parts
}

pub(crate) fn first_word(item: &str) -> &str {
    item.split_whitespace().next().unwrap_or("")
}

/// `128 kb/s` as bits per second.
fn kbps(item: &str) -> Option<u64> {
    let n: f64 = item.trim().strip_suffix(" kb/s")?.parse().ok()?;
    Some((n * 1000.0).round() as u64)
}

fn video_stream(after: &str) -> VideoStream {
    let items = split_top_level(after);
    let pix_fmt = items
        .get(1)
        .map_or("", |item| item.split('(').next().unwrap_or("").trim());
    let colour = items.get(1).copied().unwrap_or("");
    let ten_bit = ["p10", "p12", "p010", "p012", "10le", "10be", "12le", "12be"]
        .iter()
        .any(|marker| pix_fmt.contains(marker));
    let mut stream = VideoStream {
        codec: first_word(items[0]).to_string(),
        width: 0,
        height: 0,
        fps: None,
        bitrate: None,
        ten_bit,
        hdr: colour.contains("smpte2084") || colour.contains("arib-std-b67"),
    };
    for item in items.iter().skip(2) {
        let size = first_word(item)
            .split_once('x')
            .and_then(|(w, h)| Some((w.parse::<u32>().ok()?, h.parse::<u32>().ok()?)));
        if let (Some((w, h)), 0) = (size, stream.width) {
            (stream.width, stream.height) = (w, h);
        } else if let Some(rate) = kbps(item) {
            stream.bitrate.get_or_insert(rate);
        } else if let Some(fps) = item.strip_suffix(" fps").and_then(|n| n.parse().ok()) {
            stream.fps.get_or_insert(fps);
        }
    }
    stream
}

/// `None` when there is no `Input #0` or no real video stream (an attached
/// cover picture is not one).
pub fn parse(stderr: &str) -> Option<Probe> {
    let text = &stderr[stderr.find("Input #0")?..];
    let mut duration = Duration::ZERO;
    let mut bitrate = None;
    let mut video: Option<VideoStream> = None;
    let mut audio = Vec::new();
    let mut subtitles = 0;
    // Between the first video stream's line and the next stream's.
    let mut in_video = false;
    for line in text.lines().skip(1).map(str::trim_start) {
        if line.starts_with("Output #") || line.starts_with("Stream mapping:") {
            break;
        }
        if line.starts_with("Duration:") {
            duration = ffmpeg::parse_duration(line).unwrap_or(Duration::ZERO);
            bitrate = line
                .split_once("bitrate: ")
                .and_then(|(_, rate)| kbps(rate.trim()));
        } else if line.starts_with("Stream #0:") {
            in_video = false;
            if let Some((_, after)) = line.split_once(": Video: ") {
                if video.is_none() && !line.contains("(attached pic)") {
                    video = Some(video_stream(after));
                    in_video = true;
                }
            } else if let Some((_, after)) = line.split_once(": Audio: ") {
                let items = split_top_level(after);
                audio.push(AudioStream {
                    codec: first_word(items[0]).to_string(),
                    bitrate: items.iter().skip(1).find_map(|item| kbps(item)),
                });
            } else if line.contains(": Subtitle: ") {
                subtitles += 1;
            }
        } else if in_video && let Some((_, degrees)) = line.split_once("rotation of ") {
            let turned = degrees
                .split_whitespace()
                .next()
                .and_then(|d| d.parse::<f64>().ok())
                .is_some_and(|d| d.abs().round() as i64 % 180 == 90);
            if let (true, Some(stream)) = (turned, video.as_mut()) {
                std::mem::swap(&mut stream.width, &mut stream.height);
            }
        }
    }
    video.map(|video| Probe {
        duration,
        bitrate,
        video,
        audio,
        subtitles,
    })
}

pub fn probe(path: &Path, cancel: &CancelToken) -> Result<Probe, FerryError> {
    // `-i` alone exits with 1 ("At least one output file must be specified"); the
    // stream list is what matters.
    let out = ffmpeg::capture(&["-i".into(), path.into()], cancel)?;
    if !out.stderr.contains("Input #0") {
        return Err(FerryError::Unsupported(t("video.error.read")));
    }
    parse(&out.stderr).ok_or_else(|| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        FerryError::Unsupported(tf("video.error.no_video", &[("name", &name)]))
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{self, VideoSpec};

    const HDR_PORTRAIT_MOV: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'IMG_0001.MOV':
  Metadata:
    major_brand     : qt  
  Duration: 00:00:01.00, start: 0.000000, bitrate: 688 kb/s
  Stream #0:0[0x1]: Video: hevc (Main 10) (hvc1 / 0x31637668), yuv420p10le(tv, bt2020nc/bt2020/arib-std-b67, progressive), 640x360 [SAR 1:1 DAR 16:9], 500 kb/s, 30 fps, 30 tbr, 15360 tbn (default)
    Side data:
      displaymatrix: rotation of 90.00 degrees
  Stream #0:1[0x2]: Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, mono, fltp, 70 kb/s (default)
  Stream #0:2[0x3]: Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, mono, fltp, 68 kb/s
At least one output file must be specified
";

    const COVER_FIRST_MKV: &str = "Input #0, matroska,webm, from 'talk.mkv':
  Duration: 00:01:02.50, start: 0.000000, bitrate: 379 kb/s
  Stream #0:0: Video: png, rgb24(pc, gbr/unknown/unknown), 64x64 [SAR 1:1 DAR 1:1], 90k tbr, 90k tbn (attached pic)
  Stream #0:1(eng): Video: h264 (High), yuv420p(progressive), 1920x1080 [SAR 1:1 DAR 16:9], 29.97 fps, 29.97 tbr, 1k tbn (default)
  Stream #0:2(eng): Audio: aac (LC), 48000 Hz, stereo, fltp (default)
  Stream #0:3(eng): Subtitle: subrip
";

    const WEBM: &str = "Input #0, matroska,webm, from 'a.webm':
  Duration: 00:00:01.01, start: -0.007000, bitrate: 395 kb/s
  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv, progressive), 320x240, SAR 1:1 DAR 4:3, 25 fps, 25 tbr, 1k tbn
  Stream #0:1: Audio: opus, 48000 Hz, mono, fltp
";

    #[test]
    fn a_rotated_stream_reports_display_dimensions() {
        let p = parse(HDR_PORTRAIT_MOV).unwrap();
        assert_eq!((p.video.width, p.video.height), (360, 640));
        assert_eq!(p.video.codec, "hevc");
        assert!(p.video.ten_bit && p.video.hdr);
        assert_eq!((p.video.fps, p.video.bitrate), (Some(30.0), Some(500_000)));
        assert_eq!(p.duration, Duration::from_secs(1));
        assert_eq!(p.bitrate, Some(688_000));
        assert_eq!(p.audio.len(), 2);
        assert_eq!(p.audio[1].bitrate, Some(68_000));
    }

    #[test]
    fn an_attached_picture_is_not_the_video() {
        let p = parse(COVER_FIRST_MKV).unwrap();
        assert_eq!(
            (p.video.codec.as_str(), p.video.width, p.video.height),
            ("h264", 1920, 1080)
        );
        assert_eq!(p.video.fps, Some(29.97));
        assert!(!p.video.ten_bit && !p.video.hdr);
        assert_eq!(
            (p.audio.len(), p.audio[0].bitrate, p.subtitles),
            (1, None, 1)
        );
        assert_eq!(p.duration, Duration::from_millis(62_500));
    }

    #[test]
    fn a_webm_without_stream_bitrates_still_parses() {
        let p = parse(WEBM).unwrap();
        assert_eq!(
            (p.video.codec.as_str(), p.video.width, p.video.bitrate),
            ("vp9", 320, None)
        );
    }

    #[test]
    fn audio_only_or_garbage_is_not_a_video() {
        assert_eq!(
            parse(
                "Input #0, mp3, from 'a.mp3':\n  Duration: 00:00:01.00, start: 0, bitrate: 128 kb/s\n  Stream #0:0: Audio: mp3, 44100 Hz, stereo, fltp, 128 kb/s\n"
            ),
            None
        );
        assert_eq!(
            parse("a.mp4: Invalid data found when processing input"),
            None
        );
    }

    #[test]
    fn a_real_clip_probes_like_its_recipe() {
        let Some(_) = testutil::ffmpeg() else { return };
        let dir = tempfile::tempdir().unwrap();
        let spec = VideoSpec {
            size: (320, 240),
            secs: 2,
            fps: 30,
            codec: "libx264",
            audio_tracks: 2,
        };
        let path = testutil::video(dir.path(), "a.mp4", &spec).unwrap();
        let p = probe(&path, &CancelToken::new()).unwrap();
        assert_eq!(p.video.codec, "h264");
        assert_eq!((p.video.width, p.video.height), (320, 240));
        assert!(p.duration.abs_diff(Duration::from_secs(2)) < Duration::from_millis(50));
        assert_eq!(p.audio.len(), 2);
    }
}
