//! The real ffmpeg sidecar (Task 2's build). Each test returns early when
//! FERRY_FFMPEG is unset; CI always sets it.

use std::{
    ffi::OsString,
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};

use ferry_core::{FerryError, cancel::CancelToken, ffmpeg};

fn sidecar() -> Option<PathBuf> {
    std::env::var_os(ffmpeg::ENV)
        .map(PathBuf::from)
        .filter(|path| path.exists())
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

/// `-encoders` and `-buildconf` print to stdout, which `run` uses for progress,
/// so these two read it directly.
fn stdout_of(ffmpeg: &PathBuf, args: &[&str]) -> String {
    let out = Command::new(ffmpeg).args(args).output().unwrap();
    String::from_utf8_lossy(&out.stdout).into_owned()
}

#[test]
fn the_sidecar_is_a_gpl3_build_without_nonfree_parts() {
    let Some(ffmpeg) = sidecar() else { return };
    let conf = stdout_of(&ffmpeg, &["-hide_banner", "-buildconf"]);
    assert!(conf.contains("--enable-gpl") && conf.contains("--enable-version3"));
    assert!(!conf.contains("--enable-nonfree"));
    assert!(
        !conf.contains("--enable-libx265"),
        "x265 must not be in the build: HEVC is hardware only"
    );
}

#[test]
fn the_sidecar_has_every_encoder_ferry_uses() {
    let Some(ffmpeg) = sidecar() else { return };
    let encoders = stdout_of(&ffmpeg, &["-hide_banner", "-encoders"]);
    let has = |name: &str| {
        encoders
            .lines()
            .any(|line| line.split_whitespace().nth(1) == Some(name))
    };
    for e in [
        "libx264",
        "libsvtav1",
        "libvpx-vp9",
        "libopus",
        "libmp3lame",
        "aac",
        "gif",
        "mjpeg",
        "png",
    ] {
        assert!(has(e), "{e}");
    }
    assert!(!has("libx265"), "no software HEVC encoder may ship");
    if cfg!(target_os = "macos") {
        for e in ["h264_videotoolbox", "hevc_videotoolbox", "aac_at"] {
            assert!(
                has(e),
                "{e}: macOS configure needs --enable-videotoolbox --enable-audiotoolbox"
            );
        }
    }
    if cfg!(windows) {
        for e in [
            "h264_nvenc",
            "hevc_nvenc",
            "h264_qsv",
            "hevc_qsv",
            "h264_amf",
            "hevc_amf",
        ] {
            assert!(has(e), "{e}");
        }
    }
}

#[test]
fn a_real_encode_reports_progress_to_the_end() {
    if sidecar().is_none() {
        return;
    }
    let mut seen = Vec::new();
    // lavfi inputs print `Duration: N/A`, so the test says how long the output is.
    ffmpeg::run_expecting(
        &os(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30:duration=20",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-f",
            "null",
            "-",
        ]),
        Duration::from_secs(20),
        &CancelToken::new(),
        &mut |p| seen.push(p),
    )
    .unwrap();
    assert!(
        seen.len() > 1,
        "progress should climb, not only jump to the end"
    );
    assert_eq!(*seen.last().unwrap(), 1.0);
}

#[test]
fn cancelling_a_real_encode_stops_it() {
    if sidecar().is_none() {
        return;
    }
    let cancel = CancelToken::new();
    let started = Instant::now();
    let result = ffmpeg::run_expecting(
        &os(&[
            "-f",
            "lavfi",
            "-i",
            "testsrc2=size=1280x720:rate=30:duration=600",
            "-c:v",
            "libx264",
            "-preset",
            "ultrafast",
            "-f",
            "null",
            "-",
        ]),
        Duration::from_secs(600),
        &cancel,
        &mut |_| cancel.cancel(),
    );
    assert!(matches!(result, Err(FerryError::Cancelled)));
    assert!(started.elapsed() < Duration::from_secs(3));
}
