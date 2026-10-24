//! What `ffmpeg -i file` prints about an audio file: duration, the first audio
//! stream and an attached cover picture. ffprobe is not shipped, so the log is
//! parsed.

use std::{ffi::OsString, path::Path};

use crate::{
    ffmpeg,
    video::probe::{first_word, split_top_level},
};

#[derive(Debug, Clone, PartialEq)]
pub struct AudioInfo {
    /// ffmpeg's name: "mp3", "aac", "flac", "pcm_s16le", …
    pub codec: String,
    pub sample_rate: u32,
    pub channels: u16,
    pub bits: Option<u8>,
    pub duration_secs: Option<f64>,
    pub kbps: Option<u16>,
    pub cover: Option<CoverInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CoverInfo {
    pub codec: String,
    pub dims: (u32, u32),
}

impl AudioInfo {
    pub fn lossless(&self) -> bool {
        matches!(
            self.codec.as_str(),
            "flac" | "alac" | "wavpack" | "ape" | "tta"
        ) || self.codec.starts_with("pcm_")
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProbeError {
    NoAudio,
    Unreadable(String),
}

/// `exec` adds the global flags.
pub fn args(input: &Path) -> Vec<OsString> {
    let mut args = vec![OsString::from("-i"), input.as_os_str().to_owned()];
    args.extend(["-map", "0:a:0", "-frames:a", "1", "-f", "null", "-"].map(OsString::from));
    args
}

/// `320 kb/s` as 320.
fn kbps(item: &str) -> Option<u16> {
    let (n, _) = item.split_once(" kb/s")?;
    let n: f64 = n.trim().parse().ok()?;
    Some(n.round().min(f64::from(u16::MAX)) as u16)
}

fn channels(layout: &str) -> u16 {
    if let Some(n) = layout.strip_suffix(" channels") {
        return n.parse().unwrap_or(2);
    }
    let layout = layout.split('(').next().unwrap_or(layout);
    match layout {
        "mono" => 1,
        "stereo" => 2,
        "2.1" => 3,
        "quad" | "4.0" => 4,
        "5.0" => 5,
        "5.1" => 6,
        "6.1" => 7,
        "7.1" => 8,
        _ => 2,
    }
}

fn bits(codec: &str, items: &[&str]) -> Option<u8> {
    let marked = items.iter().find_map(|item| {
        let (_, rest) = item.split_once('(')?;
        rest.strip_suffix(" bit)")?.parse().ok()
    });
    let pcm = || {
        let digits = codec
            .strip_prefix("pcm_")?
            .get(1..)?
            .split(|c: char| !c.is_ascii_digit())
            .next()?;
        digits.parse().ok()
    };
    marked.or_else(pcm).or_else(|| {
        items
            .iter()
            .any(|item| matches!(*item, "s16" | "s16p"))
            .then_some(16)
    })
}

/// `3000x3000` from the first item that starts with one.
fn dims(items: &[&str]) -> Option<(u32, u32)> {
    items.iter().find_map(|item| {
        let (w, h) = first_word(item).split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    })
}

pub fn parse(log: &str) -> Result<AudioInfo, ProbeError> {
    let mut duration_secs = None;
    let mut container_kbps = None;
    let mut info = None;
    let mut cover = None;
    for line in log.lines().map(str::trim_start) {
        if line.starts_with("Output #") {
            break;
        }
        if line.starts_with("Duration:") {
            duration_secs = ffmpeg::parse_duration(line).map(|d| d.as_secs_f64());
            container_kbps = line
                .split_once("bitrate: ")
                .and_then(|(_, rate)| kbps(rate.trim()));
        } else if info.is_none()
            && let Some((_, after)) = line.split_once(": Audio: ")
        {
            let items = split_top_level(after);
            let codec = first_word(items[0]).to_string();
            let rate = items
                .iter()
                .position(|item| item.ends_with(" Hz"))
                .ok_or_else(|| ProbeError::Unreadable(line.to_string()))?;
            let sample_rate = items[rate]
                .trim_end_matches(" Hz")
                .parse()
                .map_err(|_| ProbeError::Unreadable(line.to_string()))?;
            info = Some(AudioInfo {
                sample_rate,
                channels: items.get(rate + 1).map_or(2, |layout| channels(layout)),
                bits: bits(&codec, &items),
                kbps: items.iter().find_map(|item| kbps(item)),
                codec,
                duration_secs: None,
                cover: None,
            });
        } else if cover.is_none()
            && line.contains("(attached pic)")
            && let Some((_, after)) = line.split_once(": Video: ")
        {
            let items = split_top_level(after);
            cover = Some(CoverInfo {
                codec: first_word(items[0]).to_string(),
                dims: dims(&items[1..]).unwrap_or((0, 0)),
            });
        }
    }
    let mut info = info.ok_or(ProbeError::NoAudio)?;
    info.duration_secs = duration_secs;
    info.kbps = info.kbps.or(container_kbps);
    info.cover = cover;
    Ok(info)
}

#[cfg(test)]
pub(crate) mod samples {
    pub const MP3_COVER: &str = "Input #0, mp3, from '/b/song.mp3':
  Metadata:
    title           : Song, with a comma
  Duration: 00:03:21.45, start: 0.025057, bitrate: 322 kb/s
  Stream #0:0: Audio: mp3 (mp3float), 44100 Hz, stereo, fltp, 320 kb/s
  Stream #0:1: Video: mjpeg (Baseline), yuvj420p(pc, bt470bg/unknown/unknown), 3000x3000 [SAR 1:1 DAR 1:1], 90k tbr, 90k tbn (attached pic)
    Metadata:
      comment         : Cover (front)
Stream mapping:
  Stream #0:0 -> #0:0 (mp3 (mp3float) -> pcm_s16le (native))
Output #0, null, to 'pipe:':
  Stream #0:0: Audio: pcm_s16le, 48000 Hz, mono, s16, 768 kb/s
";
    pub const M4A_PNG: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'in.m4a':
  Duration: 00:00:30.02, start: 0.000000, bitrate: 261 kb/s
  Stream #0:0[0x1](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 256 kb/s (default)
  Stream #0:1[0x0]: Video: png, rgb24(pc), 1400x1400, 90k tbr, 90k tbn (attached pic)
";
    pub const FLAC_MONO_24: &str = "Input #0, flac, from 'take.flac':
  Duration: 00:10:00.00, start: 0.000000, bitrate: 1650 kb/s
  Stream #0:0: Audio: flac, 96000 Hz, mono, s32 (24 bit)
";
    pub const WAV_51: &str = "Input #0, wav, from 'mix.wav':
  Duration: N/A, bitrate: 6912 kb/s
  Stream #0:0: Audio: pcm_s24le ([1][0][0][0] / 0x0001), 48000 Hz, 5.1, s32 (24 bit), 6912 kb/s
";
    pub const WAV_STEREO: &str = "Input #0, wav, from 'tone.wav':
  Duration: 00:00:01.00, bitrate: 1411 kb/s
  Stream #0:0: Audio: pcm_s16le ([1][0][0][0] / 0x0001), 44100 Hz, 2 channels, s16, 1411 kb/s
";
    pub const PNG_ONLY: &str = "Input #0, png_pipe, from 'x.png':
  Duration: N/A, bitrate: N/A
  Stream #0:0: Video: png, rgb24(pc), 64x48, 25 tbr, 25 tbn
";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_mp3_with_cover_reads_only_its_input_block() {
        let i = parse(samples::MP3_COVER).unwrap();
        assert_eq!(
            (i.codec.as_str(), i.sample_rate, i.channels, i.bits, i.kbps),
            ("mp3", 44100, 2, None, Some(320))
        );
        assert!((i.duration_secs.unwrap() - 201.45).abs() < 1e-6);
        assert_eq!(
            i.cover,
            Some(CoverInfo {
                codec: "mjpeg".into(),
                dims: (3000, 3000)
            })
        );
        assert!(!i.lossless());
    }

    #[test]
    fn an_m4a_stream_line_with_ids_and_default_parses() {
        let i = parse(samples::M4A_PNG).unwrap();
        assert_eq!(
            (i.codec.as_str(), i.sample_rate, i.kbps),
            ("aac", 48000, Some(256))
        );
        assert_eq!(i.cover.unwrap().dims, (1400, 1400));
    }

    #[test]
    fn lossless_sources_report_depth_layout_and_container_bitrate() {
        let f = parse(samples::FLAC_MONO_24).unwrap();
        assert_eq!(
            (f.channels, f.bits, f.kbps, f.lossless(), f.cover.is_none()),
            (1, Some(24), Some(1650), true, true)
        );
        let w = parse(samples::WAV_51).unwrap();
        assert_eq!(
            (w.codec.as_str(), w.channels, w.bits, w.duration_secs),
            ("pcm_s24le", 6, Some(24), None)
        );
        assert_eq!(parse(samples::WAV_STEREO).unwrap().channels, 2);
    }

    #[test]
    fn no_audio_stream_is_no_audio() {
        assert_eq!(parse(samples::PNG_ONLY), Err(ProbeError::NoAudio));
        assert_eq!(parse(""), Err(ProbeError::NoAudio));
    }

    #[test]
    fn the_probe_names_the_input_and_reads_one_frame() {
        let a: Vec<String> = args(Path::new("/b/my song.wav"))
            .iter()
            .map(|s| s.to_string_lossy().into())
            .collect();
        assert_eq!(
            a,
            [
                "-i",
                "/b/my song.wav",
                "-map",
                "0:a:0",
                "-frames:a",
                "1",
                "-f",
                "null",
                "-"
            ]
        );
    }
}
