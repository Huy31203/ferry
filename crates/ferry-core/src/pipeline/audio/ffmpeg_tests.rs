//! Audio jobs against the real ffmpeg. Each test is skipped when there is none
//! (`FERRY_FFMPEG` unset and no sidecar next to the test binary); CI always has one.

use std::{io, path::Path};

use super::*;
use crate::{
    audio::{
        loudness,
        probe::{self, AudioInfo},
        tool::{self, AacEncoder, AudioTool},
    },
    edit::EditsPatch,
    settings::Settings,
    testutil,
};

struct Env {
    dir: tempfile::TempDir,
    backups: Backups,
}

impl Env {
    fn new() -> Self {
        let dir = tempfile::tempdir().unwrap();
        let backups = Backups::new(dir.path().join("backups"));
        Self { dir, backups }
    }
    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }
}

fn test_trash(path: &Path) -> io::Result<()> {
    let bin = path.parent().unwrap().join(".trash");
    fs::create_dir_all(&bin)?;
    fs::rename(path, bin.join(path.file_name().unwrap()))
}

fn ffmpeg(tool: &Arc<dyn AudioTool>, args: &[&str]) {
    let args: Vec<OsString> = args.iter().map(OsString::from).collect();
    tool.run(&args, &CancelToken::new(), &mut |_| {}).unwrap();
}

fn options(env: &Env, aac: AacEncoder) -> RunOptions {
    let mut o = RunOptions::new(
        &Settings::default(),
        None,
        env.path("work"),
        test_trash,
        CancelToken::new(),
    );
    o.audio.aac = aac;
    o
}

fn run(env: &Env, name: &str, patch: serde_json::Value, aac: AacEncoder) -> Finished {
    let mut job = Job::file(&env.path(name), &Settings::default(), false).unwrap();
    job.edit(serde_json::from_value::<EditsPatch>(patch).unwrap());
    job.run(
        Level::Normal,
        &options(env, aac),
        &env.backups,
        &mut |_| {},
        &mut |_, _, _| Ok(()),
    )
    .expect("not cancelled")
}

fn info_of(tool: &Arc<dyn AudioTool>, path: &Path) -> AudioInfo {
    let log = tool
        .run(&probe::args(path), &CancelToken::new(), &mut |_| {})
        .unwrap();
    probe::parse(&log).unwrap()
}

/// `tone.wav` of `secs` seconds, and optionally `cover.jpg`.
fn wav(env: &Env, segments: &[(f32, f32)]) {
    fs::write(env.path("tone.wav"), testutil::wav_tone(segments, 44100, 2)).unwrap();
}

fn cover(env: &Env) {
    fs::write(
        env.path("cover.jpg"),
        testutil::jpeg(1200, 1200, 90, &Default::default()),
    )
    .unwrap();
}

fn str_of(p: &Path) -> &str {
    p.to_str().unwrap()
}

#[test]
fn ci_always_has_ffmpeg() {
    if std::env::var_os("CI").is_some() {
        assert!(
            tool::installed().is_some(),
            "ffmpeg must be available to ferry-core tests in CI"
        );
    }
}

#[test]
fn a_real_wav_becomes_a_192_kbps_mp3() {
    let Some(ff) = tool::installed() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let env = Env::new();
    wav(&env, &[(3.0, 0.5)]);
    let f = run(&env, "tone.wav", serde_json::json!({}), AacEncoder::Native);
    assert_eq!(f.update.state, JobState::Done, "{:?}", f.cause);
    let out = f.update.output.unwrap();
    assert_eq!(sniff::sniff(&fs::read(&out).unwrap()), Some(Format::Mp3));
    let info = info_of(&ff, &out);
    assert_eq!(
        (info.codec.as_str(), info.sample_rate, info.kbps),
        ("mp3", 44100, Some(192))
    );
}

#[test]
fn a_real_flac_with_cover_becomes_an_m4a_that_keeps_a_downscaled_cover() {
    let Some(ff) = tool::installed() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let env = Env::new();
    wav(&env, &[(2.0, 0.5)]);
    cover(&env);
    ffmpeg(
        &ff,
        &[
            "-i",
            str_of(&env.path("tone.wav")),
            "-i",
            str_of(&env.path("cover.jpg")),
            "-map",
            "0:a",
            "-map",
            "1:v",
            "-c:a",
            "flac",
            "-c:v",
            "copy",
            "-disposition:v:0",
            "attached_pic",
            str_of(&env.path("album.flac")),
        ],
    );
    let patch = serde_json::json!({ "size": { "kind": "exact", "width": 600, "height": 600 } });
    let f = run(&env, "album.flac", patch, AacEncoder::Native);
    assert_eq!(f.update.state, JobState::Done, "{:?}", f.cause);
    let out = f.update.output.unwrap();
    assert!(out.to_string_lossy().ends_with(".m4a"));
    let info = info_of(&ff, &out);
    assert_eq!(info.codec, "aac");
    let cover = info.cover.expect("the cover survives");
    assert_eq!((cover.codec.as_str(), cover.dims), ("mjpeg", (600, 600)));
}

#[test]
fn a_real_m4a_comes_out_of_each_aac_encoder_this_system_offers() {
    let Some(ff) = tool::installed() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let mut encoders = vec![AacEncoder::Native];
    if tool::aac_encoder() != AacEncoder::Native {
        encoders.push(tool::aac_encoder());
    }
    if cfg!(target_os = "macos") {
        assert_eq!(tool::aac_encoder(), AacEncoder::AudioToolbox);
    }
    for aac in encoders {
        let env = Env::new();
        wav(&env, &[(3.0, 0.5)]);
        let f = run(
            &env,
            "tone.wav",
            serde_json::json!({ "format": "m4a" }),
            aac,
        );
        assert_eq!(f.update.state, JobState::Done, "{aac:?}: {:?}", f.cause);
        let out = f.update.output.unwrap();
        assert_eq!(sniff::sniff(&fs::read(&out).unwrap()), Some(Format::M4a));
        let info = info_of(&ff, &out);
        assert_eq!(
            (info.codec.as_str(), info.sample_rate, info.channels),
            ("aac", 44100, 2),
            "{aac:?}"
        );
    }
}

#[test]
fn real_streaming_normalisation_lands_on_target() {
    let Some(ff) = tool::installed() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let env = Env::new();
    wav(&env, &[(6.0, 0.05), (6.0, 0.02)]);
    let patch = serde_json::json!({ "loudness": "streaming", "format": "wav" });
    let f = run(&env, "tone.wav", patch, AacEncoder::Native);
    assert_eq!(f.update.state, JobState::Done, "{:?}", f.cause);
    let out = f.update.output.unwrap();
    let spec = loudness::spec(crate::audio::Loudness::Streaming, 2).unwrap();
    let log = ff
        .run(
            &loudness::measure_args(&out, &spec),
            &CancelToken::new(),
            &mut |_| {},
        )
        .unwrap();
    let m = loudness::parse_measurement(&log).unwrap();
    assert!((m.i + 14.0).abs() <= 1.0, "{m:?}");
    assert!(m.tp <= -0.5, "{m:?}");
}

fn mp3_with_cover(tool: &Arc<dyn AudioTool>, env: &Env, kbps: &str) {
    wav(env, &[(3.0, 0.5)]);
    cover(env);
    ffmpeg(
        tool,
        &[
            "-i",
            str_of(&env.path("tone.wav")),
            "-i",
            str_of(&env.path("cover.jpg")),
            "-map",
            "0:a",
            "-map",
            "1:v",
            "-c:a",
            "libmp3lame",
            "-b:a",
            kbps,
            "-c:v",
            "copy",
            "-disposition:v:0",
            "attached_pic",
            "-id3v2_version",
            "3",
            str_of(&env.path("song.mp3")),
        ],
    );
}

#[test]
fn a_real_opus_result_has_no_cover_and_says_so() {
    let Some(ff) = tool::installed() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let env = Env::new();
    mp3_with_cover(&ff, &env, "320k");
    let f = run(
        &env,
        "song.mp3",
        serde_json::json!({ "format": "opus" }),
        AacEncoder::Native,
    );
    assert_eq!(f.update.state, JobState::Done, "{:?}", f.cause);
    let out = f.update.output.unwrap();
    assert_eq!(sniff::sniff(&fs::read(&out).unwrap()), Some(Format::Opus));
    assert!(info_of(&ff, &out).cover.is_none());
    assert_eq!(
        f.update.note.as_deref(),
        Some("Opus can't hold cover art — cover removed")
    );
}

#[test]
fn a_real_low_bitrate_mp3_is_left_alone() {
    let Some(ff) = tool::installed() else {
        eprintln!("skipped: no ffmpeg");
        return;
    };
    let env = Env::new();
    wav(&env, &[(3.0, 0.5)]);
    ffmpeg(
        &ff,
        &[
            "-i",
            str_of(&env.path("tone.wav")),
            "-c:a",
            "libmp3lame",
            "-b:a",
            "128k",
            str_of(&env.path("low.mp3")),
        ],
    );
    let before = fs::read(env.path("low.mp3")).unwrap();
    let f = run(&env, "low.mp3", serde_json::json!({}), AacEncoder::Native);
    assert_eq!(f.update.state, JobState::AlreadyOptimal, "{:?}", f.cause);
    assert_eq!(fs::read(env.path("low.mp3")).unwrap(), before);
}
