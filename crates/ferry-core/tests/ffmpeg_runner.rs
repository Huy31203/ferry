use std::{
    ffi::OsString,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use ferry_core::{
    FerryError,
    cancel::CancelToken,
    ffmpeg::{self, Captured, Exec},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sidecar-fixture"))
}

fn os(args: &[&str]) -> Vec<OsString> {
    args.iter().map(OsString::from).collect()
}

fn exec(mode: &str, how: &Exec) -> (Result<Captured, FerryError>, Vec<f32>) {
    let mut seen = Vec::new();
    let result = ffmpeg::exec(
        &fixture(),
        &os(&[mode]),
        how,
        &CancelToken::new(),
        &mut |p| seen.push(p),
    );
    (result, seen)
}

#[test]
fn progress_climbs_to_one_from_the_duration_on_stderr() {
    let (result, seen) = exec("ffmpeg-progress", &Exec::default());
    result.unwrap();
    assert!(seen.windows(2).all(|w| w[0] < w[1]));
    assert!((seen[0] - 0.25).abs() < 1e-6);
    assert_eq!(*seen.last().unwrap(), 1.0);
}

#[test]
fn an_expected_duration_overrides_the_inputs() {
    let how = Exec {
        expected: Some(Duration::from_secs(4)),
        ..Default::default()
    };
    let (result, seen) = exec("ffmpeg-progress", &how);
    result.unwrap();
    assert!(seen.contains(&0.5)); // 2 s of 4 s before the end
    assert_eq!(*seen.last().unwrap(), 1.0);
}

#[test]
fn the_runner_adds_its_global_options() {
    let how = Exec {
        check_status: false,
        ..Default::default()
    };
    let (result, _) = exec("ffmpeg-args", &how);
    let stderr = result.unwrap().stderr;
    let lines: Vec<&str> = stderr.lines().collect();
    for want in [
        "-hide_banner",
        "-nostdin",
        "-y",
        "-nostats",
        "-progress",
        "pipe:1",
    ] {
        assert!(lines.contains(&want), "{want}");
    }
    assert_eq!(lines.last(), Some(&"ffmpeg-args"));
}

#[test]
fn a_failure_carries_the_code_and_the_end_of_stderr() {
    let (result, _) = exec("ffmpeg-fail", &Exec::default());
    let err = result.unwrap_err();
    assert!(matches!(
        &err,
        FerryError::Failed { code: Some(1), stderr, .. } if stderr.contains("No space left on device")
    ));
    assert_eq!(err.reason(), "Not enough disk space to encode");
}

#[test]
fn capture_returns_any_exit_code() {
    let how = Exec {
        check_status: false,
        ..Default::default()
    };
    let (result, _) = exec("ffmpeg-fail", &how);
    assert!(matches!(result, Ok(Captured { code: Some(1), .. })));
}

#[test]
fn cancel_kills_the_child_quickly() {
    let cancel = CancelToken::new();
    let later = cancel.clone();
    let canceller = thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        later.cancel();
    });
    let started = Instant::now();
    let result = ffmpeg::exec(
        &fixture(),
        &os(&["ffmpeg-hang"]),
        &Exec::default(),
        &cancel,
        &mut |_| {},
    );
    canceller.join().unwrap();
    assert!(matches!(result, Err(FerryError::Cancelled)));
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[test]
fn silence_longer_than_the_stall_limit_is_stalled() {
    let how = Exec {
        stall: Duration::from_millis(500),
        ..Default::default()
    };
    let started = Instant::now();
    let (result, _) = exec("ffmpeg-hang", &how);
    assert!(matches!(result, Err(FerryError::Stalled)));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn steady_progress_never_counts_as_a_stall() {
    let how = Exec {
        stall: Duration::from_secs(1),
        expected: Some(Duration::from_millis(600)),
        ..Default::default()
    };
    let (result, seen) = exec("ffmpeg-slow", &how);
    result.unwrap();
    assert_eq!(*seen.last().unwrap(), 1.0);
}

#[test]
fn a_flood_of_stderr_does_not_block_and_keeps_its_tail() {
    let how = Exec {
        check_status: false,
        ..Default::default()
    };
    let (result, _) = exec("ffmpeg-flood", &how);
    let captured = result.unwrap();
    assert!(captured.stderr.len() <= 1_048_576);
    assert!(captured.stderr.trim_end().ends_with("flood-end"));
}

#[test]
fn a_program_that_is_not_there_cannot_spawn() {
    let result = ffmpeg::exec(
        std::path::Path::new("/no/such/ffmpeg"),
        &[],
        &Exec::default(),
        &CancelToken::new(),
        &mut |_| {},
    );
    assert!(matches!(result, Err(FerryError::Spawn(_))));
}
