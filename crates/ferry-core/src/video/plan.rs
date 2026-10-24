//! Turn the edit recipe into ffmpeg arguments. Pure: nothing runs here, so every
//! rule is pinned by a unit test.

use std::{ffi::OsString, path::Path, time::Duration};

use super::{
    Codec, GIF_FPS, GIF_LONG_EDGE, THUMB_WIDTH, fit::hardware_bitrate, hardware::Hardware,
    probe::Probe,
};
use crate::{
    edit::geometry::Rect,
    edit::{Edits, geometry},
    optimise::{self, Encoder, OptimiseOptions},
    settings,
    sniff::Format,
};

pub struct PlanInput<'a> {
    pub input: &'a Path,
    pub output: &'a Path,
    pub probe: &'a Probe,
    pub source: Format,
    pub target: Format,
    pub edits: &'a Edits,
    pub settings: &'a settings::Video,
    pub hardware: Hardware,
    pub crop: Option<Rect>,
    pub out: (u32, u32),
    /// Forced: the hardware attempt failed (H.264 goes to libx264; HEVC goes to H.264).
    pub software: bool,
    /// Fit under size: the bitrate to aim at.
    pub video_bitrate: Option<u64>,
    /// The result keeps the source's extension: subtitles are copied.
    pub same_container: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub args: Vec<OsString>,
    pub expected: Duration,
    pub encoder: &'static str,
    pub hardware: bool,
    pub codec_label: &'static str,
    pub fps: Option<f32>,
    pub audio: bool,
    pub hdr_flattened: bool,
    /// The target asked for HEVC and no hardware HEVC encoder may be used, so H.264 was planned.
    pub hevc_as_h264: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Encode(Format),
    Keep,
}

pub struct Framed {
    pub window: (u32, u32),
    pub out: (u32, u32),
}

pub fn crf_x264(q: u8) -> u8 {
    18 + scaled_down(q, 24)
}

pub fn crf_svt(q: u8) -> u8 {
    20 + scaled_down(q, 35)
}

pub fn crf_vp9(q: u8) -> u8 {
    15 + scaled_down(q, 35)
}

/// round((100 − q) × span / 95)
fn scaled_down(q: u8, span: u32) -> u8 {
    ((u32::from(100 - q.min(100)) * span + 47) / 95) as u8
}

/// The codec the target asks for. `Mov` and `Mkv` keep HEVC if the source is HEVC.
pub fn codec_for(target: Format, probe: &Probe) -> Option<Codec> {
    match target {
        Format::Mp4 => Some(Codec::H264),
        Format::Hevc => Some(Codec::Hevc),
        Format::Av1 => Some(Codec::Av1),
        Format::WebM => Some(Codec::Vp9),
        Format::Mov | Format::Mkv => Some(if probe.video.codec == "hevc" {
            Codec::Hevc
        } else {
            Codec::H264
        }),
        Format::Gif | Format::Png | Format::Jpeg | Format::WebP => None,
    }
}

pub fn muxer(ext: &str) -> &'static str {
    match ext.to_ascii_lowercase().as_str() {
        "mp4" | "m4v" => "mp4",
        "mov" => "mov",
        "mkv" => "matroska",
        "webm" => "webm",
        _ => "gif",
    }
}

/// A `Mov` becomes HEVC or MP4 when converting MOV is on; anything else stays.
pub fn plain_target(source: Format, probe: &Probe, settings: &settings::Video) -> Format {
    match source {
        Format::Mov if settings.convert_mov => {
            if probe.video.codec == "hevc" {
                Format::Hevc
            } else {
                Format::Mp4
            }
        }
        other => other,
    }
}

/// What to do when the target needs HEVC but no hardware HEVC encoder is
/// available (`hevc_ok` false). A plain optimise of HEVC never becomes H.264.
pub fn route(target: Format, probe: &Probe, hevc_ok: bool, plain: bool) -> Route {
    if hevc_ok || codec_for(target, probe) != Some(Codec::Hevc) {
        Route::Encode(target)
    } else if plain && probe.video.codec == "hevc" {
        Route::Keep
    } else if target == Format::Hevc {
        Route::Encode(Format::Mp4)
    } else {
        Route::Encode(target)
    }
}

fn even(side: u32) -> u32 {
    (side & !1).max(2)
}

/// The cut window and the result's size. Video sizes are even; a GIF is capped
/// at 800 px on its long edge unless the recipe sets a size or scale.
pub fn frame(display: (u32, u32), edits: &Edits, target: Format) -> Framed {
    let window = geometry::crop_size(display, edits);
    let out = geometry::output_size(window, edits);
    if target == Format::Gif {
        let long = out.0.max(out.1);
        let out = if edits.size.is_none() && edits.scale.is_none() && long > GIF_LONG_EDGE {
            let shrink = |side: u32| {
                ((u64::from(side) * u64::from(GIF_LONG_EDGE) + u64::from(long) / 2)
                    / u64::from(long))
                .max(1) as u32
            };
            (shrink(out.0), shrink(out.1))
        } else {
            out
        };
        return Framed { window, out };
    }
    Framed {
        window: (even(window.0), even(window.1)),
        out: (even(out.0), even(out.1)),
    }
}

/// The first frame as a small JPEG (the card's thumbnail).
pub fn thumb_args(input: &Path, output: &Path) -> Vec<OsString> {
    let scale = format!("scale='min({THUMB_WIDTH},iw)':-2");
    let mut args = vec![OsString::from("-i"), input.into()];
    args.extend(strs(&[
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-vf",
        &scale,
        "-c:v",
        "mjpeg",
        "-q:v",
        "3",
        "-f",
        "image2",
    ]));
    args.push(output.into());
    args
}

/// The first frame as a PNG (the compare window's still).
pub fn frame_args(input: &Path, output: &Path) -> Vec<OsString> {
    let mut args = vec![OsString::from("-i"), input.into()];
    args.extend(strs(&[
        "-map",
        "0:v:0",
        "-frames:v",
        "1",
        "-c:v",
        "png",
        "-f",
        "image2",
    ]));
    args.push(output.into());
    args
}

fn strs(items: &[&str]) -> Vec<OsString> {
    items.iter().map(OsString::from).collect()
}

/// A number without a trailing `.0`.
fn num(n: f64) -> String {
    n.to_string()
}

fn filters(p: &PlanInput, window: (u32, u32), speed: Option<u16>, fps: Option<f64>) -> Vec<String> {
    let mut chain = Vec::new();
    if let Some(r) = p.crop {
        chain.push(format!("crop={}:{}:{}:{}", r.width, r.height, r.x, r.y));
    }
    if p.out != window {
        chain.push(format!("scale={}:{}:flags=lanczos", p.out.0, p.out.1));
    }
    if let Some(percent) = speed {
        chain.push(format!("setpts=PTS*100/{percent}"));
    }
    if let Some(fps) = fps {
        chain.push(format!("fps={}", num(fps)));
    }
    chain
}

fn audio_fits(muxer: &str, codec: &str) -> bool {
    let common = ["aac", "mp3", "alac", "ac3", "eac3"];
    match muxer {
        "mp4" | "mov" => common.contains(&codec),
        "matroska" => {
            common.contains(&codec)
                || ["opus", "vorbis", "flac"].contains(&codec)
                || codec.starts_with("pcm_")
        }
        "webm" => ["opus", "vorbis"].contains(&codec),
        _ => false,
    }
}

pub fn plan(p: &PlanInput) -> Plan {
    let probe = p.probe;
    let display = (probe.video.width, probe.video.height);
    let window = p.crop.map_or(display, |r| (r.width, r.height));
    let speed = p.edits.speed;
    let speed_percent = speed.map(|s| s.percent);
    let duration = probe.duration;
    let expected = match speed_percent {
        Some(percent) => duration.mul_f64(100.0 / f64::from(percent)),
        None => duration,
    };
    let source_fps = probe.video.fps;

    if p.target == Format::Gif {
        return plan_gif(p, window, expected);
    }

    // The fps filter: drop frames when asked, and cap high frame rates.
    let mut fps_filter: Option<f64> = None;
    if let (Some(s), Some(fps)) = (speed, source_fps)
        && !s.keep_frames
    {
        fps_filter = Some((f64::from(fps) * 1000.0).round() / 1000.0);
    }
    if let (Some(cap), Some(fps)) = (p.settings.fps_cap.fps(), source_fps)
        && f64::from(fps) > f64::from(cap) + 0.5
    {
        fps_filter = Some(fps_filter.map_or(f64::from(cap), |f| f.min(f64::from(cap))));
    }
    let out_fps = match (fps_filter, speed, source_fps) {
        (Some(f), _, _) => Some(f as f32),
        (None, Some(s), Some(fps)) if s.keep_frames => Some(fps * f32::from(s.percent) / 100.0),
        (None, _, fps) => fps,
    };

    // The encoder.
    let mode = p.edits.encoder.unwrap_or(p.settings.encoder);
    let lossless = mode == Encoder::VisuallyLossless
        && p.edits.compression.is_none()
        && p.edits.fit_under.is_none();
    let mode = if mode == Encoder::VisuallyLossless && !lossless {
        Encoder::Small
    } else {
        mode
    };
    let q = optimise::quality(&OptimiseOptions {
        level: p.edits.level,
        compression: p.edits.compression.unwrap_or(p.settings.compression),
        ..Default::default()
    });
    let hevc_ok = p.hardware.hevc.is_some() && !p.software;
    let asked = codec_for(p.target, probe).unwrap_or(Codec::H264);
    let (codec, hevc_as_h264) = match asked {
        Codec::Hevc if !hevc_ok => (Codec::H264, true),
        other => (other, false),
    };
    let hardware_name = match codec {
        Codec::Hevc => p.hardware.hevc,
        Codec::H264 if mode == Encoder::Fast && !p.software => p.hardware.h264,
        _ => None,
    };
    let ten_bit = probe.video.ten_bit && matches!(codec, Codec::Hevc | Codec::Av1);

    let mut args = vec![OsString::from("-i"), p.input.into()];
    args.extend(strs(&["-map", "0:v:0"]));
    let audio_kept = !(p.edits.mute || p.settings.remove_audio);
    if audio_kept {
        args.extend(strs(&["-map", "0:a?"]));
    } else {
        args.push("-an".into());
    }
    if p.same_container {
        args.extend(strs(&["-map", "0:s?", "-c:s", "copy"]));
    } else {
        args.push("-sn".into());
    }
    args.extend(strs(&["-map_metadata", "0"]));

    let chain = filters(p, window, speed_percent, fps_filter);
    if !chain.is_empty() {
        args.extend(strs(&["-vf", &chain.join(",")]));
    }
    args.extend(strs(&[
        "-fps_mode",
        if fps_filter.is_some() {
            "cfr"
        } else {
            "passthrough"
        },
    ]));

    let encoder: &'static str;
    if let Some(name) = hardware_name {
        encoder = name;
        let wanted_q = if lossless && codec == Codec::Hevc {
            95
        } else {
            q
        };
        let rate = p
            .video_bitrate
            .unwrap_or_else(|| hardware_bitrate(wanted_q, probe, p.out, out_fps, codec));
        args.extend(strs(&["-c:v", name]));
        let capped = |args: &mut Vec<OsString>| {
            args.extend(strs(&[
                "-b:v",
                &rate.to_string(),
                "-maxrate",
                &(rate * 3 / 2).to_string(),
                "-bufsize",
                &(rate * 2).to_string(),
            ]));
        };
        if name.ends_with("_videotoolbox") {
            args.extend(strs(&["-b:v", &rate.to_string()]));
        } else if name.ends_with("_nvenc") {
            args.extend(strs(&["-preset", "p5", "-tune", "hq", "-rc", "vbr"]));
            capped(&mut args);
        } else if name.ends_with("_qsv") {
            args.extend(strs(&["-preset", "slow"]));
            capped(&mut args);
        } else {
            args.extend(strs(&["-quality", "quality", "-rc", "vbr_peak"]));
            capped(&mut args);
        }
        if codec == Codec::Hevc {
            let profile = if ten_bit { "main10" } else { "main" };
            if name.ends_with("_videotoolbox") {
                args.extend(strs(&["-profile:v", profile]));
            } else if ten_bit {
                args.extend(strs(&["-profile:v", "main10"]));
            }
            args.extend(strs(&["-tag:v", "hvc1"]));
        } else if name.ends_with("_videotoolbox") {
            args.extend(strs(&["-profile:v", "high"]));
        }
        args.extend(strs(&["-pix_fmt", if ten_bit { "p010le" } else { "nv12" }]));
    } else {
        match codec {
            Codec::H264 => {
                encoder = "libx264";
                let preset = if mode == Encoder::Fast {
                    "veryfast"
                } else {
                    "slow"
                };
                args.extend(strs(&["-c:v", "libx264", "-preset", preset]));
                match p.video_bitrate {
                    Some(b) => args.extend(strs(&[
                        "-b:v",
                        &b.to_string(),
                        "-maxrate",
                        &(b * 3 / 2).to_string(),
                        "-bufsize",
                        &(b * 2).to_string(),
                    ])),
                    None => {
                        let crf = if lossless { 16 } else { crf_x264(q) };
                        args.extend(strs(&["-crf", &crf.to_string()]));
                    }
                }
                args.extend(strs(&["-profile:v", "high", "-pix_fmt", "yuv420p"]));
            }
            Codec::Av1 => {
                encoder = "libsvtav1";
                let (preset, crf) = match (lossless, mode) {
                    (true, _) => ("4", 18),
                    (false, Encoder::Small) => ("6", crf_svt(q)),
                    (false, _) => ("10", crf_svt(q)),
                };
                args.extend(strs(&["-c:v", "libsvtav1", "-preset", preset]));
                match p.video_bitrate {
                    Some(b) => args.extend(strs(&["-b:v", &b.to_string()])),
                    None => args.extend(strs(&["-crf", &crf.to_string()])),
                }
                args.extend(strs(&[
                    "-pix_fmt",
                    if ten_bit { "yuv420p10le" } else { "yuv420p" },
                ]));
            }
            Codec::Vp9 => {
                encoder = "libvpx-vp9";
                let (cpu, crf) = match (lossless, mode) {
                    (true, _) => ("1", 12),
                    (false, Encoder::Small) => ("2", crf_vp9(q)),
                    (false, _) => ("5", crf_vp9(q)),
                };
                args.extend(strs(&[
                    "-c:v",
                    "libvpx-vp9",
                    "-deadline",
                    "good",
                    "-cpu-used",
                    cpu,
                    "-row-mt",
                    "1",
                ]));
                match p.video_bitrate {
                    Some(b) => args.extend(strs(&["-b:v", &b.to_string()])),
                    None => args.extend(strs(&["-b:v", "0", "-crf", &crf.to_string()])),
                }
                args.extend(strs(&["-pix_fmt", "yuv420p"]));
            }
            // HEVC has no software encoder: it was planned as H.264 above.
            Codec::Hevc => unreachable!("HEVC is hardware only"),
        }
    }

    // Audio.
    let ext = p.output.extension().and_then(|e| e.to_str()).unwrap_or("");
    let mux = muxer(ext);
    let has_audio = !probe.audio.is_empty();
    if audio_kept && has_audio {
        let copy = speed.is_none()
            && probe.audio.iter().all(|a| {
                audio_fits(mux, &a.codec) && !(p.settings.audio_to_aac && a.codec != "aac")
            });
        if copy {
            args.extend(strs(&["-c:a", "copy"]));
        } else {
            let known = probe.audio.iter().filter_map(|a| a.bitrate).max();
            let fit = p.video_bitrate.is_some();
            let (name, rate) = if mux == "webm" {
                let cap = if fit { 96_000 } else { 128_000 };
                ("libopus", known.unwrap_or(128_000).min(cap).max(48_000))
            } else {
                let cap = if fit { 128_000 } else { 192_000 };
                ("aac", known.unwrap_or(160_000).min(cap).max(64_000))
            };
            args.extend(strs(&["-c:a", name, "-b:a", &rate.to_string()]));
            if let Some(percent) = speed_percent {
                args.extend(strs(&[
                    "-af",
                    &format!("atempo={}", num(f64::from(percent) / 100.0)),
                ]));
            }
        }
    }
    if matches!(mux, "mp4" | "mov") {
        args.extend(strs(&["-movflags", "+faststart+use_metadata_tags"]));
    }
    args.extend(strs(&["-f", mux]));
    args.push(p.output.into());

    Plan {
        args,
        expected,
        encoder,
        hardware: hardware_name.is_some(),
        codec_label: codec.label(),
        fps: out_fps,
        audio: audio_kept && has_audio,
        hdr_flattened: probe.video.hdr && matches!(codec, Codec::H264 | Codec::Vp9),
        hevc_as_h264,
    }
}

fn plan_gif(p: &PlanInput, window: (u32, u32), expected: Duration) -> Plan {
    let mut chain = filters(p, window, p.edits.speed.map(|s| s.percent), None);
    chain.push(format!("fps={GIF_FPS}"));
    chain.push("split[a][b]".into());
    let graph = format!(
        "[0:v:0]{};[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle[o]",
        chain.join(",")
    );
    let mut args = vec![OsString::from("-i"), p.input.into()];
    args.extend(strs(&[
        "-an",
        "-sn",
        "-filter_complex",
        &graph,
        "-map",
        "[o]",
        "-loop",
        "0",
        "-f",
        "gif",
    ]));
    args.push(p.output.into());
    Plan {
        args,
        expected,
        encoder: "gif",
        hardware: false,
        codec_label: "GIF",
        fps: Some(GIF_FPS as f32),
        audio: false,
        hdr_flattened: p.probe.video.hdr,
        hevc_as_h264: false,
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::{
        edit::{Size, Speed},
        settings::{FpsCap, Settings},
        video::probe::{AudioStream, VideoStream},
    };
    use std::path::PathBuf;

    /// 1920x1080 H.264 30 fps 6 Mb/s, one AAC 128k track, 10 s long.
    pub(crate) fn probe_1080() -> Probe {
        Probe {
            duration: Duration::from_secs(10),
            bitrate: Some(6_128_000),
            video: VideoStream {
                codec: "h264".into(),
                width: 1920,
                height: 1080,
                fps: Some(30.0),
                bitrate: Some(6_000_000),
                ten_bit: false,
                hdr: false,
            },
            audio: vec![AudioStream {
                codec: "aac".into(),
                bitrate: Some(128_000),
            }],
            subtitles: 0,
        }
    }

    fn hdr_hevc() -> Probe {
        let mut p = probe_1080();
        p.video.codec = "hevc".into();
        p.video.ten_bit = true;
        p.video.hdr = true;
        p
    }

    struct Case {
        probe: Probe,
        edits: Edits,
        settings: settings::Video,
        target: Format,
        hardware: Hardware,
        out_file: &'static str,
        crop: Option<Rect>,
        out: (u32, u32),
        software: bool,
        video_bitrate: Option<u64>,
        same_container: bool,
    }

    fn case() -> Case {
        Case {
            probe: probe_1080(),
            edits: Edits::default(),
            settings: Settings::default().video,
            target: Format::Mp4,
            hardware: Hardware::default(),
            out_file: "/w/out.mp4",
            crop: None,
            out: (1920, 1080),
            software: false,
            video_bitrate: None,
            same_container: false,
        }
    }

    fn run(c: &Case) -> Plan {
        let input = PathBuf::from("/b/in.mp4");
        let output = PathBuf::from(c.out_file);
        plan(&PlanInput {
            input: &input,
            output: &output,
            probe: &c.probe,
            source: Format::Mp4,
            target: c.target,
            edits: &c.edits,
            settings: &c.settings,
            hardware: c.hardware,
            crop: c.crop,
            out: c.out,
            software: c.software,
            video_bitrate: c.video_bitrate,
            same_container: c.same_container,
        })
    }

    fn has(args: &[OsString], pair: &[&str]) -> bool {
        args.windows(2).any(|w| w[0] == pair[0] && w[1] == pair[1])
    }

    fn lacks(args: &[OsString], flag: &str) -> bool {
        !args.iter().any(|a| a == flag)
    }

    fn small() -> Case {
        let mut c = case();
        c.settings.encoder = Encoder::Small;
        c
    }

    #[test]
    fn quality_maps_to_each_encoders_scale() {
        assert_eq!(
            (crf_x264(80), crf_x264(100), crf_x264(5), crf_x264(60)),
            (23, 18, 42, 28)
        );
        assert_eq!(
            (crf_svt(80), crf_svt(5), crf_vp9(80), crf_vp9(5)),
            (27, 55, 22, 50)
        );
    }

    #[test]
    fn a_plain_h264_optimise_keeps_every_frame_and_every_track() {
        let plan = run(&small());
        assert_eq!(plan.encoder, "libx264");
        assert!(has(&plan.args, &["-crf", "23"]));
        assert!(has(&plan.args, &["-preset", "slow"]));
        assert!(has(&plan.args, &["-pix_fmt", "yuv420p"]));
        assert!(has(&plan.args, &["-fps_mode", "passthrough"]));
        assert!(has(&plan.args, &["-map", "0:a?"]));
        assert!(has(&plan.args, &["-c:a", "copy"]));
        assert!(has(
            &plan.args,
            &["-movflags", "+faststart+use_metadata_tags"]
        ));
        assert!(has(&plan.args, &["-f", "mp4"]));
        assert!(lacks(&plan.args, "-vf"));
    }

    #[test]
    fn fast_uses_the_hardware_encoder_with_a_bitrate() {
        let mut c = case();
        c.hardware = Hardware {
            h264: Some("h264_videotoolbox"),
            hevc: None,
        };
        let plan = run(&c);
        assert_eq!((plan.encoder, plan.hardware), ("h264_videotoolbox", true));
        // 6 Mb/s x (0.25 + 0.5 x 0.64) = 3 420 000, under the cap of 7 265 894
        assert!(has(&plan.args, &["-b:v", "3420000"]));
        assert!(
            lacks(&plan.args, "-crf") && lacks(&plan.args, "-q:v") && lacks(&plan.args, "-maxrate")
        );
    }

    #[test]
    fn a_failed_hardware_encode_is_planned_again_in_software() {
        let mut c = case();
        c.hardware = Hardware {
            h264: Some("h264_videotoolbox"),
            hevc: None,
        };
        c.software = true;
        let plan = run(&c);
        assert_eq!((plan.encoder, plan.hardware), ("libx264", false));
    }

    #[test]
    fn ten_bit_hdr_stays_main10_on_plain_hevc() {
        let hevc = Hardware {
            h264: None,
            hevc: Some("hevc_videotoolbox"),
        };
        for mode in [Encoder::Fast, Encoder::Small, Encoder::VisuallyLossless] {
            let mut c = case();
            c.probe = hdr_hevc();
            c.target = Format::Hevc;
            c.hardware = hevc;
            c.settings.encoder = mode;
            let plan = run(&c);
            assert_eq!((plan.encoder, plan.hardware), ("hevc_videotoolbox", true));
            assert!(has(&plan.args, &["-pix_fmt", "p010le"]));
            assert!(has(&plan.args, &["-profile:v", "main10"]));
            assert!(has(&plan.args, &["-tag:v", "hvc1"]));
            assert!(lacks(&plan.args, "-crf"));
            assert!(!plan.hdr_flattened && !plan.hevc_as_h264);
            assert!(!plan.args.iter().any(|a| a == "libx265"));
            let q = if mode == Encoder::VisuallyLossless {
                95
            } else {
                80
            };
            let bps = hardware_bitrate(q, &c.probe, (1920, 1080), Some(30.0), Codec::Hevc);
            assert!(has(&plan.args, &["-b:v", &bps.to_string()]), "{mode:?}");
        }
    }

    #[test]
    fn hevc_without_a_hardware_encoder_is_written_as_h264() {
        let mut c = small();
        c.probe = hdr_hevc();
        c.target = Format::Hevc;
        let plan = run(&c);
        assert_eq!(plan.encoder, "libx264");
        assert!(has(&plan.args, &["-pix_fmt", "yuv420p"]));
        assert_eq!(plan.codec_label, "H.264");
        assert!(plan.hevc_as_h264 && plan.hdr_flattened);

        c.hardware = Hardware {
            h264: Some("h264_videotoolbox"),
            hevc: Some("hevc_videotoolbox"),
        };
        c.software = true;
        let plan = run(&c);
        assert_eq!(plan.encoder, "libx264");
        assert!(plan.hevc_as_h264);

        c.software = false;
        c.hardware = Hardware {
            h264: Some("h264_videotoolbox"),
            hevc: None,
        };
        c.settings.encoder = Encoder::Fast;
        let plan = run(&c);
        assert_eq!(plan.encoder, "h264_videotoolbox");
        assert!(has(&plan.args, &["-pix_fmt", "nv12"]));
        assert!(plan.hevc_as_h264);

        c.target = Format::Mp4;
        assert!(!run(&c).hevc_as_h264);
    }

    #[test]
    fn a_plain_hevc_optimise_never_becomes_h264() {
        let hp = hdr_hevc();
        let p = probe_1080();
        assert_eq!(route(Format::Hevc, &hp, false, true), Route::Keep);
        // a MOV holding HEVC, convert_mov off
        assert_eq!(route(Format::Mov, &hp, false, true), Route::Keep);
        assert_eq!(
            route(Format::Hevc, &hp, false, false),
            Route::Encode(Format::Mp4)
        );
        assert_eq!(
            route(Format::Mkv, &hp, false, false),
            Route::Encode(Format::Mkv)
        );
        // "Convert to HEVC" from a pipeline
        assert_eq!(
            route(Format::Hevc, &p, false, false),
            Route::Encode(Format::Mp4)
        );
        assert_eq!(
            route(Format::Hevc, &hp, true, true),
            Route::Encode(Format::Hevc)
        );
        assert_eq!(
            route(Format::Mp4, &p, false, true),
            Route::Encode(Format::Mp4)
        );
    }

    #[test]
    fn hdr_to_h264_is_flagged_flattened() {
        let mut c = small();
        c.probe = hdr_hevc();
        let plan = run(&c);
        assert_eq!(plan.encoder, "libx264");
        assert!(has(&plan.args, &["-pix_fmt", "yuv420p"]));
        assert!(plan.hdr_flattened);
        c.target = Format::Gif;
        c.out_file = "/w/out.gif";
        assert!(run(&c).hdr_flattened);
    }

    fn edits(f: &dyn Fn(&mut Edits)) -> Edits {
        let mut e = Edits::default();
        f(&mut e);
        e
    }

    #[test]
    fn frames_are_rounded_down_to_even_sizes() {
        assert_eq!(
            frame((1366, 767), &edits(&|e| e.scale = Some(50)), Format::Mp4).out,
            (682, 384)
        );
        let f = frame(
            (1920, 1080),
            &edits(&|e| e.aspect = Some((191, 100))),
            Format::Mp4,
        );
        // fit_ratio gives 1005, rounded down to even
        assert_eq!(f.window, (1920, 1004));
        assert_eq!(
            frame((3, 3), &edits(&|e| e.scale = Some(10)), Format::Mp4).out,
            (2, 2)
        );
    }

    #[test]
    fn a_gif_is_capped_at_800_px_unless_sized() {
        assert_eq!(
            frame((1920, 1080), &Edits::default(), Format::Gif).out,
            (800, 450)
        );
        assert_eq!(
            frame((640, 360), &Edits::default(), Format::Gif).out,
            (640, 360)
        );
        let e = edits(&|e| e.size = Some(Size::Width { width: 1200 }));
        assert_eq!(frame((1920, 1080), &e, Format::Gif).out, (1200, 675));
    }

    #[test]
    fn crop_scale_and_speed_share_one_filter_chain() {
        let mut c = small();
        c.crop = Some(Rect {
            x: 0,
            y: 28,
            width: 1920,
            height: 1024,
        });
        c.out = (960, 512);
        c.edits.speed = Some(Speed {
            percent: 200,
            keep_frames: true,
        });
        let plan = run(&c);
        assert!(has(
            &plan.args,
            &[
                "-vf",
                "crop=1920:1024:0:28,scale=960:512:flags=lanczos,setpts=PTS*100/200"
            ]
        ));
        assert!(has(&plan.args, &["-fps_mode", "passthrough"]));
        assert!(has(&plan.args, &["-af", "atempo=2"]));
        assert!(has(&plan.args, &["-c:a", "aac"]) && has(&plan.args, &["-b:a", "128000"]));
        assert_eq!(plan.expected, Duration::from_secs(5));
        assert_eq!(plan.fps, Some(60.0));
    }

    #[test]
    fn drop_frames_and_the_fps_cap_use_one_fps_filter() {
        let mut c = small();
        c.edits.speed = Some(Speed {
            percent: 150,
            keep_frames: false,
        });
        let plan = run(&c);
        assert!(has(&plan.args, &["-vf", "setpts=PTS*100/150,fps=30"]));
        assert!(has(&plan.args, &["-fps_mode", "cfr"]));
        assert_eq!(plan.fps, Some(30.0));

        let mut c = small();
        c.probe.video.fps = Some(60.0);
        c.settings.fps_cap = FpsCap::Fps24;
        assert!(has(&run(&c).args, &["-vf", "fps=24"]));
        c.probe.video.fps = Some(24.0);
        assert!(lacks(&run(&c).args, "-vf"));
    }

    #[test]
    fn every_audio_track_is_mapped_and_none_is_required() {
        let plan = run(&small());
        assert!(has(&plan.args, &["-map", "0:a?"]));
        let mut c = small();
        c.edits.mute = true;
        let plan = run(&c);
        assert!(lacks(&plan.args, "0:a?") && !lacks(&plan.args, "-an"));
        let mut c = small();
        c.settings.remove_audio = true;
        let plan = run(&c);
        assert!(lacks(&plan.args, "0:a?") && !lacks(&plan.args, "-an"));
        assert!(!plan.audio);
    }

    #[test]
    fn webm_gets_vp9_and_opus() {
        let mut c = small();
        c.target = Format::WebM;
        c.out_file = "/w/out.webm";
        c.settings.encoder = Encoder::Fast;
        let plan = run(&c);
        assert_eq!(plan.encoder, "libvpx-vp9");
        assert!(has(&plan.args, &["-b:v", "0"]) && has(&plan.args, &["-crf", "22"]));
        assert!(has(&plan.args, &["-row-mt", "1"]));
        assert!(has(&plan.args, &["-c:a", "libopus"]) && has(&plan.args, &["-b:a", "128000"]));
        assert!(has(&plan.args, &["-f", "webm"]));
        assert!(lacks(&plan.args, "-movflags"));
    }

    #[test]
    fn av1_goes_to_matroska_with_svt() {
        let mut c = case();
        c.target = Format::Av1;
        c.out_file = "/w/out.mkv";
        let plan = run(&c);
        assert_eq!(plan.encoder, "libsvtav1");
        assert!(has(&plan.args, &["-crf", "27"]) && has(&plan.args, &["-preset", "10"]));
        assert!(has(&plan.args, &["-f", "matroska"]));
    }

    #[test]
    fn visually_lossless_yields_to_an_explicit_compression() {
        let mut c = case();
        c.settings.encoder = Encoder::VisuallyLossless;
        assert!(has(&run(&c).args, &["-crf", "16"]));
        c.edits.compression = Some(60);
        let plan = run(&c);
        assert!(has(&plan.args, &["-crf", "28"]) && has(&plan.args, &["-preset", "slow"]));
    }

    #[test]
    fn mov_becomes_mp4_or_hevc_when_converting_mov_is_on() {
        let h264 = probe_1080();
        let hevc = hdr_hevc();
        let mut s = Settings::default().video;
        assert_eq!(plain_target(Format::Mov, &h264, &s), Format::Mp4);
        assert_eq!(plain_target(Format::Mov, &hevc, &s), Format::Hevc);
        assert_eq!(plain_target(Format::Hevc, &h264, &s), Format::Hevc);
        s.convert_mov = false;
        assert_eq!(plain_target(Format::Mov, &h264, &s), Format::Mov);
    }

    #[test]
    fn subtitles_ride_along_only_in_the_same_container() {
        let mut c = small();
        c.same_container = true;
        let plan = run(&c);
        assert!(has(&plan.args, &["-map", "0:s?"]) && has(&plan.args, &["-c:s", "copy"]));
        assert!(lacks(&plan.args, "-sn"));
        assert!(!lacks(&run(&small()).args, "-sn"));
    }

    #[test]
    fn a_video_to_gif_uses_a_palette() {
        let mut c = small();
        c.target = Format::Gif;
        c.out_file = "/w/out.gif";
        let plan = run(&c);
        let graph = plan
            .args
            .windows(2)
            .find(|w| w[0] == "-filter_complex")
            .map(|w| w[1].to_string_lossy().into_owned())
            .unwrap();
        assert!(graph.contains("fps=15"));
        assert!(graph.contains("palettegen=stats_mode=diff"));
        assert!(graph.contains("paletteuse=dither=sierra2_4a"));
        assert!(has(&plan.args, &["-loop", "0"]) && has(&plan.args, &["-f", "gif"]));
        assert!(!lacks(&plan.args, "-an"));
        assert_eq!(
            (plan.encoder, plan.codec_label, plan.fps),
            ("gif", "GIF", Some(15.0))
        );
    }

    #[test]
    fn fit_under_size_uses_a_bitrate_in_every_encoder() {
        let mut c = small();
        c.video_bitrate = Some(800_000);
        c.edits.fit_under = Some(1_000_000);
        let plan = run(&c);
        assert!(has(&plan.args, &["-b:v", "800000"]));
        assert!(has(&plan.args, &["-maxrate", "1200000"]));
        assert!(has(&plan.args, &["-bufsize", "1600000"]));
        assert!(lacks(&plan.args, "-crf"));
        // an AAC source at 128k is already at the 128k cap; speed forces a re-encode
        c.edits.speed = Some(Speed {
            percent: 200,
            keep_frames: true,
        });
        assert!(has(&run(&c).args, &["-b:a", "128000"]));
    }
}
