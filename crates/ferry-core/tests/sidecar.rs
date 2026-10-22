use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use ferry_core::{
    cancel::CancelToken,
    sidecar::{self, SidecarError},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sidecar-fixture"))
}

fn run(args: &[&str], stdin: &[u8], timeout: Duration) -> Result<Vec<u8>, SidecarError> {
    sidecar::run(&fixture(), args, stdin, timeout, &CancelToken::new())
}

#[test]
fn bytes_round_trip_through_the_child() {
    let out = run(&["echo"], b"hello sidecar", Duration::from_secs(10)).unwrap();
    assert_eq!(out, b"hello sidecar");
}

#[test]
fn a_large_input_and_output_do_not_deadlock() {
    let input = vec![7_u8; 8 * 1024 * 1024];
    let out = run(&["echo"], &input, Duration::from_secs(30)).unwrap();
    assert_eq!(out.len(), input.len());
}

#[test]
fn large_stderr_is_drained_while_stdin_is_still_unread() {
    let out = run(&["noisy"], &[1; 1024], Duration::from_secs(30)).unwrap();
    assert_eq!(out, b"done");
}

#[test]
fn a_failing_child_reports_its_code_and_stderr() {
    match run(&["fail"], b"", Duration::from_secs(10)) {
        Err(SidecarError::Failed { code, stderr }) => {
            assert_eq!(code, Some(3));
            assert!(stderr.contains("boom"), "{stderr:?}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_missing_program_is_a_spawn_error() {
    let missing = PathBuf::from("definitely-not-a-ferry-sidecar");
    let result = sidecar::run(
        &missing,
        &[],
        b"",
        Duration::from_secs(1),
        &CancelToken::new(),
    );
    assert!(matches!(result, Err(SidecarError::Spawn(_))));
}

#[test]
fn a_timeout_kills_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("alive.txt");
    let started = Instant::now();
    let result = run(
        &["sleep-then-touch", marker.to_str().unwrap()],
        b"",
        Duration::from_millis(300),
    );
    assert!(matches!(result, Err(SidecarError::Timeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_millis(1200));
    thread::sleep(Duration::from_millis(2000)); // longer than the child's own sleep
    assert!(!marker.exists(), "the child kept running after the timeout");
}

#[test]
fn cancelling_kills_the_child_promptly() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("alive.txt");
    let cancel = CancelToken::new();
    let canceller = cancel.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        canceller.cancel();
    });
    let started = Instant::now();
    let result = sidecar::run(
        &fixture(),
        &["sleep-then-touch", marker.to_str().unwrap()],
        b"",
        Duration::from_secs(30),
        &cancel,
    );
    assert!(matches!(result, Err(SidecarError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_millis(1200));
    thread::sleep(Duration::from_millis(2000));
    assert!(!marker.exists(), "the child kept running after the cancel");
}
