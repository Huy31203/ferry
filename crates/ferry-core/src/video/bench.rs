//! Time each video encoder on a generated 3 s 1080p clip, through the same plan
//! real videos use (onboarding spec §5).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cancel::CancelToken;
use crate::edit::Edits;
use crate::error::FerryError;
use crate::ffmpeg;
use crate::i18n::{t, tf};
use crate::optimise::Encoder;
use crate::settings;
use crate::sniff::Format;
use crate::video::{hardware, plan, probe};

pub const SAMPLE_SECS: u32 = 3;

pub fn make_sample(dir: &Path, cancel: &CancelToken) -> Result<PathBuf, FerryError> {
    let out = dir.join("sample.mp4");
    let source = format!("testsrc2=size=1920x1080:rate=30:duration={SAMPLE_SECS}");
    let args: Vec<OsString> = [
        "-f",
        "lavfi",
        "-i",
        &source,
        "-c:v",
        "libx264",
        "-preset",
        "ultrafast",
        "-crf",
        "10",
        "-pix_fmt",
        "yuv420p",
        "-an",
    ]
    .iter()
    .map(OsString::from)
    .chain([out.clone().into_os_string()])
    .collect();
    ffmpeg::run(&args, cancel, &mut |_| {})?;
    Ok(out)
}

/// Seconds per minute of video, or `None` when the encode failed or was cancelled.
pub fn time(
    sample: &Path,
    dir: &Path,
    encoder: Encoder,
    video: &settings::Video,
    cancel: &CancelToken,
) -> Option<f64> {
    if cancel.is_cancelled() {
        return None;
    }
    let probe = probe::probe(sample, cancel).ok()?;
    let settings = settings::Video {
        encoder,
        ..video.clone()
    };
    let edits = Edits {
        encoder: Some(encoder),
        ..Edits::default()
    };
    let target = plan::plain_target(Format::Mp4, &probe, &settings);
    let output = dir.join(format!("{encoder:?}.mp4"));
    let planned = plan::plan(&plan::PlanInput {
        input: sample,
        output: &output,
        probe: &probe,
        source: Format::Mp4,
        target,
        edits: &edits,
        settings: &settings,
        hardware: hardware::hardware(),
        crop: None,
        out: (probe.video.width, probe.video.height),
        software: false,
        video_bitrate: None,
        same_container: target == Format::Mp4,
    });
    let began = Instant::now();
    let ran = ffmpeg::run(&planned.args, cancel, &mut |_| {});
    let elapsed = began.elapsed();
    let _ = std::fs::remove_file(&output);
    ran.ok()?;
    Some(elapsed.as_secs_f64() * 60.0 / f64::from(SAMPLE_SECS))
}

/// Times Fast, Small and Visually lossless in that order, in a temporary
/// folder that is removed afterwards.
pub fn measure_all(
    video: &settings::Video,
    cancel: &CancelToken,
    each: &mut dyn FnMut(Encoder, Option<f64>),
) {
    let dir = std::env::temp_dir().join(format!("ferry-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sample = std::fs::create_dir_all(&dir)
        .ok()
        .and_then(|()| make_sample(&dir, cancel).ok());
    for encoder in [Encoder::Fast, Encoder::Small, Encoder::VisuallyLossless] {
        let secs = sample
            .as_deref()
            .and_then(|s| time(s, &dir, encoder, video, cancel));
        each(encoder, secs);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

pub fn label(secs_per_min: Option<f64>) -> String {
    match secs_per_min {
        None => t("onboarding.measure.failed"),
        Some(s) if s < 5.0 => t("onboarding.measure.quick"),
        Some(s) if s < 90.0 => tf(
            "onboarding.measure.secs",
            &[("n", &(s.round() as u64).to_string())],
        ),
        Some(s) => tf(
            "onboarding.measure.mins",
            &[("n", &((s / 60.0).round() as u64).to_string())],
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `measure_all` uses one folder per process, so its tests take turns.
    static MEASURE: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn labels_read_like_the_spec() {
        assert_eq!(label(None), "—");
        assert_eq!(label(Some(3.0)), "< 5 s per min of 1080p");
        assert_eq!(label(Some(20.4)), "~20 s per min of 1080p");
        assert_eq!(label(Some(150.0)), "~3 min per min of 1080p");
    }

    #[test]
    fn every_encoder_gets_a_time() {
        let _turn = MEASURE.lock().unwrap_or_else(|e| e.into_inner());
        if crate::testutil::ffmpeg().is_none() {
            return;
        }
        let mut got = Vec::new();
        measure_all(
            &settings::Video::default(),
            &CancelToken::new(),
            &mut |e, t| got.push((e, t)),
        );
        let encoders: Vec<_> = got.iter().map(|(e, _)| *e).collect();
        assert_eq!(
            encoders,
            [Encoder::Fast, Encoder::Small, Encoder::VisuallyLossless]
        );
        assert!(
            got.iter().all(|(_, t)| t.is_some_and(|s| s > 0.0)),
            "{got:?}"
        );
    }

    #[test]
    fn a_failed_encoder_is_none_and_the_rest_still_run() {
        if crate::testutil::ffmpeg().is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let sample = make_sample(dir.path(), &CancelToken::new()).unwrap();
        let cancelled = CancelToken::new();
        cancelled.cancel();
        let video = settings::Video::default();
        assert_eq!(
            time(&sample, dir.path(), Encoder::Small, &video, &cancelled),
            None
        );
        assert!(
            time(
                &sample,
                dir.path(),
                Encoder::Fast,
                &video,
                &CancelToken::new()
            )
            .is_some()
        );
    }

    #[test]
    fn the_sample_folder_is_removed() {
        let _turn = MEASURE.lock().unwrap_or_else(|e| e.into_inner());
        if crate::testutil::ffmpeg().is_none() {
            return;
        }
        let before = bench_dirs();
        measure_all(
            &settings::Video::default(),
            &CancelToken::new(),
            &mut |_, _| {},
        );
        assert_eq!(bench_dirs(), before);
    }

    fn bench_dirs() -> usize {
        std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| {
                e.file_name()
                    .to_string_lossy()
                    .starts_with(&format!("ferry-bench-{}", std::process::id()))
            })
            .count()
    }
}
