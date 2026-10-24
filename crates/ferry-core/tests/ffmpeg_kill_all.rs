//! Its own test binary: `kill_all` touches process-wide state.

use std::{
    ffi::OsString,
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use ferry_core::{
    FerryError,
    cancel::CancelToken,
    ffmpeg::{self, Exec},
};

#[test]
fn kill_all_stops_every_running_ffmpeg() {
    let runner = thread::spawn(|| {
        ffmpeg::exec(
            &PathBuf::from(env!("CARGO_BIN_EXE_sidecar-fixture")),
            &[OsString::from("ffmpeg-hang")],
            &Exec::default(),
            &CancelToken::new(),
            &mut |_| {},
        )
    });
    thread::sleep(Duration::from_millis(300));
    let started = Instant::now();
    ffmpeg::kill_all();
    assert!(matches!(runner.join().unwrap(), Err(FerryError::Cancelled)));
    assert!(started.elapsed() < Duration::from_secs(2));
}
