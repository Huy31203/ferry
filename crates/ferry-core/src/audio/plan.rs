//! Resolving an audio run: the target, bitrate, sample rate, what to do with
//! the cover, and the ffmpeg arguments that make it.

use std::{ffi::OsString, path::Path};

use super::{
    CoverPolicy, Loudness, TARGETS, is_lossless_format, probe::AudioInfo, settings::Audio,
    supports_cover, tool::AacEncoder,
};
use crate::{
    edit::{Edits, Size},
    optimise::Level,
    sniff::Format,
};

/// The card's Compression stops, highest first.
pub const BITRATE_STOPS: [u16; 7] = [320, 256, 192, 160, 128, 96, 64];
/// The Layer III rates for 32 kHz and up, and the rates AudioToolbox offers.
pub const MP3_KBPS: [u16; 14] = [
    32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320,
];

pub struct Inputs<'a> {
    pub info: &'a AudioInfo,
    pub source: Format,
    pub edits: &'a Edits,
    pub level: Level,
    pub settings: &'a Audio,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AudioPlan {
    pub target: Format,
    /// `None` for a lossless target.
    pub kbps: Option<u16>,
    pub copy_audio: bool,
    pub sample_rate: u32,
    pub downmix: bool,
    pub pcm_bits: u8,
    pub loudness: Option<Loudness>,
    pub cover: CoverPlan,
    /// The recipe or the loudness target asks for a result; it always writes.
    pub explicit: bool,
    pub fit_under: Option<u64>,
    /// Which AAC encoder an M4A target uses; ignored for other targets.
    pub aac: AacEncoder,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropReason {
    Container,
    Unreadable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoverPlan {
    Absent,
    Copy,
    Optimise,
    Resize(Size),
    Scale(u8),
    Remove,
    Drop(DropReason),
}

impl AudioPlan {
    /// Nothing would change: copying the audio and leaving the cover be, or a
    /// WAV that stays a WAV.
    pub fn nothing_to_do(&self, source: Format) -> bool {
        (self.copy_audio && matches!(self.cover, CoverPlan::Absent | CoverPlan::Copy))
            || (source == Format::Wav
                && self.target == Format::Wav
                && self.loudness.is_none()
                && self.fit_under.is_none())
    }
}

/// The edit's format when it is a target, else the settings' rule.
pub fn target_for(source: Format, edits: &Edits, settings: &Audio) -> Format {
    match edits.format {
        Some(f) if TARGETS.contains(&f) => f,
        _ => settings.outputs.for_source(source),
    }
}

fn min_kbps(target: Format) -> u16 {
    if target == Format::Opus { 16 } else { 32 }
}

/// The largest `MP3_KBPS` value at or below `kbps`.
fn snap_down(kbps: u16) -> u16 {
    MP3_KBPS
        .iter()
        .rev()
        .find(|&&k| k <= kbps)
        .copied()
        .unwrap_or(MP3_KBPS[0])
}

/// requested, then Aggressive (two stops lower), then never above a lossy
/// source, then the codec's range, then a legal Layer III rate for MP3.
pub fn kbps(target: Format, requested: u16, level: Level, info: &AudioInfo, rate: u32) -> u16 {
    let mut k = requested;
    if level == Level::Aggressive {
        let at = BITRATE_STOPS
            .iter()
            .position(|&s| s <= requested)
            .unwrap_or(BITRATE_STOPS.len() - 1);
        let lower = BITRATE_STOPS[(at + 2).min(BITRATE_STOPS.len() - 1)];
        k = requested.min(lower);
    }
    if !info.lossless()
        && let Some(source) = info.kbps
    {
        k = k.min(source);
    }
    k = match target {
        Format::Opus => k.clamp(16, 256),
        _ => k.clamp(32, 320),
    };
    if target == Format::Mp3 && rate >= 32_000 {
        k = snap_down(k);
    }
    k
}

/// AudioToolbox swaps a bitrate it does not offer for the nearest one, which
/// can be higher, so M4A on it snaps down first.
pub fn encoder_kbps(target: Format, aac: AacEncoder, kbps: u16) -> u16 {
    match (target, aac) {
        (Format::M4a, AacEncoder::AudioToolbox) => snap_down(kbps),
        _ => kbps,
    }
}

/// The bitrate that fits `limit` bytes, leaving 2 % and a tag's worth spare.
pub fn fit_kbps(limit: u64, duration_secs: f64, cover_bytes: u64) -> u16 {
    let room = limit as f64 * 0.98 - cover_bytes as f64 - 4096.0;
    (room * 8.0 / duration_secs / 1000.0)
        .floor()
        .clamp(0.0, f64::from(u16::MAX)) as u16
}

/// The next try after a result of `size` bytes missed `limit`; `None` at the
/// codec's minimum.
pub fn retry_kbps(target: Format, current: u16, limit: u64, size: u64) -> Option<u16> {
    let min = min_kbps(target);
    if current <= min {
        return None;
    }
    let next = (f64::from(current) * limit as f64 * 0.97 / size.max(1) as f64).floor();
    Some(next.clamp(f64::from(min), f64::from(current - 1)) as u16)
}

/// The rate ffmpeg is asked for (`loudnorm` resamples to 192 kHz inside).
pub fn output_rate(target: Format, source_rate: u32) -> u32 {
    const STANDARD: [u32; 9] = [8000, 11025, 12000, 16000, 22050, 24000, 32000, 44100, 48000];
    match target {
        Format::Opus => 48_000,
        Format::Mp3 | Format::M4a => {
            if STANDARD.contains(&source_rate) {
                source_rate
            } else if source_rate > 48_000 && source_rate.is_multiple_of(44_100) {
                44_100
            } else if source_rate > 48_000 {
                48_000
            } else {
                44_100
            }
        }
        _ => source_rate,
    }
}

fn container_codec(target: Format) -> &'static str {
    match target {
        Format::Mp3 => "mp3",
        Format::M4a => "aac",
        _ => "opus",
    }
}

fn cover_plan(i: &Inputs, target: Format) -> CoverPlan {
    let Some(cover) = &i.info.cover else {
        return CoverPlan::Absent;
    };
    let (edits, policy) = (i.edits, i.edits.cover.unwrap_or(i.settings.cover));
    if edits.cover == Some(CoverPolicy::Remove)
        || (edits.size.is_none() && edits.scale.is_none() && policy == CoverPolicy::Remove)
    {
        return CoverPlan::Remove;
    }
    if !supports_cover(target) {
        return CoverPlan::Drop(DropReason::Container);
    }
    if !matches!(cover.codec.as_str(), "mjpeg" | "png") {
        return CoverPlan::Drop(DropReason::Unreadable);
    }
    if let Some(size) = edits.size {
        CoverPlan::Resize(size)
    } else if let Some(scale) = edits.scale {
        CoverPlan::Scale(scale)
    } else {
        match policy {
            CoverPolicy::Keep => CoverPlan::Copy,
            CoverPolicy::Optimise => CoverPlan::Optimise,
            CoverPolicy::Downscale => CoverPlan::Resize(i.settings.cover_size.as_size()),
            CoverPolicy::Remove => CoverPlan::Remove,
        }
    }
}

pub fn resolve(i: &Inputs) -> AudioPlan {
    resolve_with(i, AacEncoder::Native)
}

pub fn resolve_with(i: &Inputs, aac: AacEncoder) -> AudioPlan {
    let (info, edits) = (i.info, i.edits);
    let target = target_for(i.source, edits, i.settings);
    let sample_rate = output_rate(target, info.sample_rate);
    let loudness =
        Some(edits.loudness.unwrap_or(i.settings.loudness)).filter(|l| *l != Loudness::Off);
    let mut kbps_value = None;
    let mut copy_audio = false;
    if !is_lossless_format(target) {
        let fit = edits
            .fit_under
            .zip(info.duration_secs.filter(|d| *d > 0.0))
            .map(|(limit, secs)| fit_kbps(limit, secs, 0));
        let requested = fit.unwrap_or_else(|| {
            edits.bitrate.unwrap_or(if target == Format::Opus {
                i.settings.opus_bitrate_kbps
            } else {
                i.settings.bitrate_kbps
            })
        });
        let level = if fit.is_some() {
            Level::Normal
        } else {
            i.level
        };
        let planned = kbps(target, requested, level, info, sample_rate);
        copy_audio = edits.fit_under.is_none()
            && loudness.is_none()
            && target == i.source
            && !info.lossless()
            && info.codec == container_codec(target)
            && info
                .kbps
                .is_some_and(|source| u32::from(planned) * 100 >= u32::from(source) * 95);
        kbps_value = Some(match (copy_audio, info.kbps) {
            (true, Some(source)) => source,
            _ => encoder_kbps(target, aac, planned),
        });
    }
    AudioPlan {
        target,
        kbps: kbps_value,
        copy_audio,
        sample_rate,
        downmix: target == Format::Mp3 && info.channels > 2,
        pcm_bits: if info.bits.is_some_and(|b| b > 16) {
            24
        } else {
            16
        },
        loudness,
        cover: cover_plan(i, target),
        explicit: !edits.is_plain() || loudness.is_some(),
        fit_under: edits.fit_under,
        aac,
    }
}

pub enum CoverInput<'a> {
    None,
    /// The picture in the source file.
    Source,
    /// An already processed picture.
    File(&'a Path),
}

pub struct EncodeArgs<'a> {
    pub input: &'a Path,
    pub cover: CoverInput<'a>,
    pub plan: &'a AudioPlan,
    pub filter: Option<&'a str>,
    pub output: &'a Path,
}

pub fn muxer(target: Format) -> &'static str {
    match target {
        Format::Mp3 => "mp3",
        Format::M4a => "ipod",
        Format::Opus => "opus",
        Format::Wav => "wav",
        _ => "flac",
    }
}

/// `exec` adds the global flags. Every path is absolute, so none reads as an option.
pub fn encode_args(a: &EncodeArgs) -> Vec<OsString> {
    debug_assert!(cfg!(test) || (a.input.is_absolute() && a.output.is_absolute()));
    let plan = a.plan;
    let mut args: Vec<OsString> = vec!["-i".into(), a.input.as_os_str().to_owned()];
    if let CoverInput::File(file) = a.cover {
        args.extend(["-i".into(), file.as_os_str().to_owned()]);
    }
    let mut add = |items: &[&str]| args.extend(items.iter().map(OsString::from));
    add(&["-map", "0:a:0"]);
    match a.cover {
        CoverInput::None => {}
        CoverInput::Source => add(&["-map", "0:v:0"]),
        CoverInput::File(_) => add(&["-map", "1:v:0"]),
    }
    add(&["-map_metadata", "0"]);
    if let Some(filter) = a.filter {
        add(&["-af", filter]);
    }
    if plan.copy_audio {
        add(&["-c:a", "copy"]);
    } else {
        let rate = format!("{}k", plan.kbps.unwrap_or_default());
        match plan.target {
            Format::Mp3 => add(&["-c:a", "libmp3lame", "-b:a", &rate]),
            Format::M4a => {
                add(&["-c:a", plan.aac.codec(), "-b:a", &rate]);
                if plan.aac == AacEncoder::AudioToolbox {
                    // Constrained VBR; Fit under size needs CBR's predictable size.
                    let mode = if plan.fit_under.is_some() {
                        "cbr"
                    } else {
                        "cvbr"
                    };
                    add(&["-aac_at_mode", mode]);
                }
            }
            Format::Opus => add(&["-c:a", "libopus", "-b:a", &rate, "-vbr", "on"]),
            Format::Wav => add(&["-c:a", &format!("pcm_s{}le", plan.pcm_bits)]),
            _ => add(&["-c:a", "flac", "-compression_level", "8"]),
        }
        add(&["-ar", &plan.sample_rate.to_string()]);
        if plan.downmix {
            add(&["-ac", "2"]);
        }
    }
    if !matches!(a.cover, CoverInput::None) {
        add(&["-c:v", "copy", "-disposition:v:0", "attached_pic"]);
    }
    match plan.target {
        Format::Mp3 => add(&["-id3v2_version", "3"]),
        Format::Wav => add(&["-rf64", "auto"]),
        _ => {}
    }
    add(&["-f", muxer(plan.target)]);
    args.push(a.output.as_os_str().to_owned());
    args
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Grade {
    Smaller,
    Same,
    Larger,
}

/// Cycle 1's rule: at least 1 % smaller counts.
pub fn grade(before: u64, after: u64) -> Grade {
    if after * 100 <= before * 99 {
        Grade::Smaller
    } else if after > before {
        Grade::Larger
    } else {
        Grade::Same
    }
}

#[cfg(test)]
mod tests {
    use super::super::probe::{CoverInfo, parse, samples};
    use super::*;

    fn joined(v: Vec<OsString>) -> String {
        v.iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn inputs<'a>(
        info: &'a AudioInfo,
        source: Format,
        edits: &'a Edits,
        settings: &'a Audio,
    ) -> Inputs<'a> {
        Inputs {
            info,
            source,
            edits,
            level: Level::Normal,
            settings,
        }
    }

    #[test]
    fn the_bitrate_never_rises_above_a_lossy_source() {
        let mp3_128 = AudioInfo {
            kbps: Some(128),
            ..parse(samples::MP3_COVER).unwrap()
        };
        let lossless = parse(samples::WAV_STEREO).unwrap();
        assert_eq!(kbps(Format::Mp3, 192, Level::Normal, &lossless, 44100), 192);
        assert_eq!(
            kbps(Format::Mp3, 192, Level::Aggressive, &lossless, 44100),
            128
        );
        assert_eq!(
            kbps(Format::Mp3, 64, Level::Aggressive, &lossless, 44100),
            64
        );
        assert_eq!(kbps(Format::Mp3, 192, Level::Normal, &mp3_128, 44100), 128);
        assert_eq!(kbps(Format::M4a, 192, Level::Normal, &mp3_128, 44100), 128);
        assert_eq!(kbps(Format::Mp3, 150, Level::Normal, &lossless, 44100), 128);
        assert_eq!(kbps(Format::M4a, 150, Level::Normal, &lossless, 44100), 150);
        assert_eq!(
            kbps(Format::Opus, 300, Level::Normal, &lossless, 48000),
            256
        );
        assert_eq!(kbps(Format::Mp3, 20, Level::Normal, &lossless, 44100), 32);
    }

    #[test]
    fn sample_rates_suit_each_encoder() {
        assert_eq!(
            [
                output_rate(Format::Opus, 44100),
                output_rate(Format::Mp3, 44100),
                output_rate(Format::Mp3, 96000),
                output_rate(Format::Mp3, 88200),
                output_rate(Format::Mp3, 22050),
                output_rate(Format::Mp3, 37800),
                output_rate(Format::M4a, 96000),
                output_rate(Format::M4a, 8000),
                output_rate(Format::Wav, 96000),
                output_rate(Format::Flac, 192000)
            ],
            [
                48000, 44100, 48000, 44100, 22050, 44100, 48000, 8000, 96000, 192000
            ]
        );
    }

    #[test]
    fn a_plain_wav_becomes_a_192_kbps_mp3() {
        let info = parse(samples::WAV_STEREO).unwrap();
        let p = resolve(&inputs(
            &info,
            Format::Wav,
            &Edits::default(),
            &Audio::default(),
        ));
        assert_eq!(
            (
                p.target,
                p.kbps,
                p.copy_audio,
                p.sample_rate,
                p.cover,
                p.explicit
            ),
            (
                Format::Mp3,
                Some(192),
                false,
                44100,
                CoverPlan::Absent,
                false
            )
        );
    }

    #[test]
    fn a_low_bitrate_mp3_has_nothing_to_do() {
        let info = AudioInfo {
            kbps: Some(128),
            cover: None,
            ..parse(samples::MP3_COVER).unwrap()
        };
        let plain = Edits::default();
        let p = resolve(&inputs(&info, Format::Mp3, &plain, &Audio::default()));
        assert!(p.copy_audio && p.nothing_to_do(Format::Mp3));
        let keep_cover = Audio {
            cover: CoverPolicy::Keep,
            ..Audio::default()
        };
        let with_cover = AudioInfo {
            kbps: Some(128),
            ..parse(samples::MP3_COVER).unwrap()
        };
        let q = resolve(&inputs(&with_cover, Format::Mp3, &plain, &keep_cover));
        assert!(q.nothing_to_do(Format::Mp3));
        let r = resolve(&inputs(&with_cover, Format::Mp3, &plain, &Audio::default()));
        assert!(r.copy_audio && r.cover == CoverPlan::Optimise && !r.nothing_to_do(Format::Mp3));
    }

    #[test]
    fn cover_plans_follow_the_recipe_then_the_settings() {
        let info = parse(samples::MP3_COVER).unwrap();
        let settings = Audio::default();
        let plan = |edits: Edits, target| {
            let edits = Edits {
                format: Some(target),
                ..edits
            };
            resolve(&inputs(&info, Format::Mp3, &edits, &settings)).cover
        };
        assert_eq!(plan(Edits::default(), Format::Mp3), CoverPlan::Optimise);
        assert_eq!(
            plan(
                Edits {
                    scale: Some(50),
                    ..Edits::default()
                },
                Format::Mp3
            ),
            CoverPlan::Scale(50)
        );
        assert_eq!(
            plan(
                Edits {
                    cover: Some(CoverPolicy::Remove),
                    ..Edits::default()
                },
                Format::M4a
            ),
            CoverPlan::Remove
        );
        assert_eq!(
            plan(Edits::default(), Format::Opus),
            CoverPlan::Drop(DropReason::Container)
        );
        let bmp = AudioInfo {
            cover: Some(CoverInfo {
                codec: "bmp".into(),
                dims: (10, 10),
            }),
            ..info.clone()
        };
        assert_eq!(
            resolve(&inputs(&bmp, Format::Mp3, &Edits::default(), &settings)).cover,
            CoverPlan::Drop(DropReason::Unreadable)
        );
    }

    #[test]
    fn fit_under_size_picks_a_bitrate_from_the_duration() {
        assert_eq!(fit_kbps(1_000_000, 60.0, 0), 130);
        assert_eq!(
            retry_kbps(Format::Mp3, 128, 1_000_000, 1_100_000),
            Some(112)
        );
        assert_eq!(retry_kbps(Format::Mp3, 32, 1_000_000, 1_100_000), None);
    }

    #[test]
    fn encode_arguments_for_the_common_cases() {
        let settings = Audio::default();
        let plain = Edits::default();
        let wav_info = parse(samples::WAV_STEREO).unwrap();
        let wav = resolve(&inputs(&wav_info, Format::Wav, &plain, &settings));
        assert_eq!(
            joined(encode_args(&EncodeArgs {
                input: Path::new("/b/in.wav"),
                cover: CoverInput::None,
                plan: &wav,
                filter: None,
                output: Path::new("/w/out.mp3")
            })),
            "-i /b/in.wav -map 0:a:0 -map_metadata 0 -c:a libmp3lame -b:a 192k -ar 44100 -id3v2_version 3 -f mp3 /w/out.mp3"
        );
        let flac_info = AudioInfo {
            cover: Some(CoverInfo {
                codec: "mjpeg".into(),
                dims: (1200, 1200),
            }),
            ..parse(samples::FLAC_MONO_24).unwrap()
        };
        let flac = resolve(&inputs(&flac_info, Format::Flac, &plain, &settings));
        assert_eq!(
            joined(encode_args(&EncodeArgs {
                input: Path::new("/b/in.flac"),
                cover: CoverInput::File(Path::new("/w/cover.jpg")),
                plan: &flac,
                filter: None,
                output: Path::new("/w/out.m4a")
            })),
            "-i /b/in.flac -i /w/cover.jpg -map 0:a:0 -map 1:v:0 -map_metadata 0 -c:a aac -b:a 192k -ar 48000 -c:v copy -disposition:v:0 attached_pic -f ipod /w/out.m4a"
        );
        let loud = AudioPlan {
            target: Format::Wav,
            kbps: None,
            pcm_bits: 24,
            sample_rate: 48000,
            ..wav.clone()
        };
        assert_eq!(
            joined(encode_args(&EncodeArgs {
                input: Path::new("/b/in.wav"),
                cover: CoverInput::None,
                plan: &loud,
                filter: Some("loudnorm=I=-23.0"),
                output: Path::new("/w/out.wav")
            })),
            "-i /b/in.wav -map 0:a:0 -map_metadata 0 -af loudnorm=I=-23.0 -c:a pcm_s24le -ar 48000 -rf64 auto -f wav /w/out.wav"
        );
        let copy = AudioPlan {
            copy_audio: true,
            target: Format::Mp3,
            ..wav.clone()
        };
        assert_eq!(
            joined(encode_args(&EncodeArgs {
                input: Path::new("/b/a.mp3"),
                cover: CoverInput::Source,
                plan: &copy,
                filter: None,
                output: Path::new("/w/out.mp3")
            })),
            "-i /b/a.mp3 -map 0:a:0 -map 0:v:0 -map_metadata 0 -c:a copy -c:v copy -disposition:v:0 attached_pic -id3v2_version 3 -f mp3 /w/out.mp3"
        );
    }

    #[cfg(unix)]
    #[test]
    fn every_path_argument_is_absolute() {
        path_arguments_are_absolute("/b/-intro ü.wav", "/w/out.mp3");
    }

    #[cfg(windows)]
    #[test]
    fn every_path_argument_is_absolute() {
        path_arguments_are_absolute(r"C:\b\-intro ü.wav", r"C:\w\out.mp3");
    }

    fn path_arguments_are_absolute(input: &str, output: &str) {
        let info = parse(samples::WAV_STEREO).unwrap();
        let wav = resolve(&inputs(
            &info,
            Format::Wav,
            &Edits::default(),
            &Audio::default(),
        ));
        let a = encode_args(&EncodeArgs {
            input: Path::new(input),
            cover: CoverInput::None,
            plan: &wav,
            filter: None,
            output: Path::new(output),
        });
        let after_i = a.iter().position(|x| x == "-i").unwrap() + 1;
        assert_eq!(a[after_i], OsString::from(input));
        assert!(Path::new(a.last().unwrap()).is_absolute());
    }

    #[test]
    fn grading_uses_the_one_percent_rule() {
        assert_eq!(
            [grade(1000, 990), grade(1000, 991), grade(1000, 1001)],
            [Grade::Smaller, Grade::Same, Grade::Larger]
        );
    }

    #[test]
    fn m4a_uses_audiotoolbox_when_offered_and_native_aac_otherwise() {
        let flac = parse(samples::FLAC_MONO_24).unwrap();
        let (plain, settings) = (Edits::default(), Audio::default());
        let base = inputs(&flac, Format::Flac, &plain, &settings);
        let args = |p: &AudioPlan| {
            joined(encode_args(&EncodeArgs {
                input: Path::new("/b/in.flac"),
                cover: CoverInput::None,
                plan: p,
                filter: None,
                output: Path::new("/w/out.m4a"),
            }))
        };
        let native = resolve_with(&base, AacEncoder::Native);
        assert_eq!(native, resolve(&base));
        assert_eq!(
            args(&native),
            "-i /b/in.flac -map 0:a:0 -map_metadata 0 -c:a aac -b:a 192k -ar 48000 -f ipod /w/out.m4a"
        );
        let at = resolve_with(&base, AacEncoder::AudioToolbox);
        assert_eq!(at.aac, AacEncoder::AudioToolbox);
        assert_eq!(
            args(&at),
            "-i /b/in.flac -map 0:a:0 -map_metadata 0 -c:a aac_at -b:a 192k -aac_at_mode cvbr -ar 48000 -f ipod /w/out.m4a"
        );
        let fit = AudioPlan {
            fit_under: Some(1_000_000),
            ..at.clone()
        };
        assert!(args(&fit).contains(" -c:a aac_at -b:a 192k -aac_at_mode cbr -ar 48000 "));
        for p in [&at, &fit] {
            let a = args(p);
            assert!(
                !a.contains("-q:a")
                    && !a.contains("global_quality")
                    && !a.contains("aac_at_quality")
                    && !a.contains("mode vbr")
            );
        }
        let native_fit = AudioPlan {
            fit_under: Some(1_000_000),
            ..native.clone()
        };
        assert!(
            args(&native_fit).contains(" -c:a aac -b:a 192k -ar 48000 ")
                && !args(&native_fit).contains("aac_at")
        );
        let to_mp3 = Edits {
            format: Some(Format::Mp3),
            ..Edits::default()
        };
        let mp3 = resolve_with(
            &inputs(&flac, Format::Flac, &to_mp3, &settings),
            AacEncoder::AudioToolbox,
        );
        assert!(args(&mp3).contains("-c:a libmp3lame") && !args(&mp3).contains("aac"));
    }

    #[test]
    fn audiotoolbox_bitrates_snap_down_never_up() {
        let at = AacEncoder::AudioToolbox;
        assert_eq!(encoder_kbps(Format::M4a, at, 150), 128);
        assert_eq!(encoder_kbps(Format::M4a, at, 192), 192);
        assert_eq!(encoder_kbps(Format::M4a, at, 33), 32);
        assert_eq!(encoder_kbps(Format::M4a, AacEncoder::Native, 150), 150);
        assert_eq!(encoder_kbps(Format::Opus, at, 150), 150);
        let wav = parse(samples::WAV_STEREO).unwrap();
        let settings = Audio::default();
        let edits = Edits {
            format: Some(Format::M4a),
            bitrate: Some(150),
            ..Edits::default()
        };
        let i = inputs(&wav, Format::Wav, &edits, &settings);
        assert_eq!(resolve_with(&i, at).kbps, Some(128));
        assert_eq!(resolve_with(&i, AacEncoder::Native).kbps, Some(150));
        // The copy check sees the unsnapped bitrate.
        let aac_150 = AudioInfo {
            kbps: Some(150),
            cover: None,
            ..parse(samples::M4A_PNG).unwrap()
        };
        let plain = Edits::default();
        let copy = resolve_with(&inputs(&aac_150, Format::M4a, &plain, &settings), at);
        assert!(copy.copy_audio && copy.kbps == Some(150) && copy.nothing_to_do(Format::M4a));
    }

    #[test]
    fn fit_under_retries_snap_down_on_audiotoolbox_only() {
        let next = |aac, cur| {
            retry_kbps(Format::M4a, cur, 1_000_000, 1_200_000)
                .map(|k| encoder_kbps(Format::M4a, aac, k))
        };
        assert_eq!(next(AacEncoder::Native, 128), Some(103));
        assert_eq!(next(AacEncoder::AudioToolbox, 128), Some(96));
        assert_eq!(next(AacEncoder::AudioToolbox, 96), Some(64));
        assert_eq!(next(AacEncoder::AudioToolbox, 32), None);
        assert_eq!(
            encoder_kbps(
                Format::M4a,
                AacEncoder::AudioToolbox,
                fit_kbps(1_000_000, 60.0, 0)
            ),
            128
        );
    }
}
