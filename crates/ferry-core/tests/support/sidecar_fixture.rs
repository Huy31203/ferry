//! Test helper for tests/sidecar.rs: a child process with scripted behaviour,
//! so the runner is tested the same way on every OS.

use std::{
    env, fs,
    io::{self, Read, Write},
    thread,
    time::Duration,
};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    // ffmpeg runner tests: the runner puts its global options first, so the mode is last.
    if let Some(mode) = args.last().filter(|arg| arg.starts_with("ffmpeg-")) {
        ffmpeg(mode, &args);
        return;
    }
    match args.first().map(String::as_str) {
        // Copy stdin to stdout.
        Some("echo") => {
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input).unwrap();
            io::stdout().write_all(&input).unwrap();
        }
        // Write 8 MB to stderr while stdin is still unread, then answer "done".
        Some("noisy") => {
            let noise = [b'x'; 64 * 1024];
            let mut stderr = io::stderr();
            for _ in 0..128 {
                stderr.write_all(&noise).unwrap();
            }
            io::stdin().read_to_end(&mut Vec::new()).unwrap();
            print!("done");
        }
        // Print a complaint and exit 3.
        Some("fail") => {
            eprintln!("boom");
            std::process::exit(3);
        }
        // Sleep, then create the file in args[2]; being killed first means no file.
        Some("sleep-then-touch") => {
            thread::sleep(Duration::from_millis(1500));
            fs::write(&args[1], b"alive").unwrap();
        }
        other => panic!("unknown mode {other:?}"),
    }
}

fn ffmpeg(mode: &str, args: &[String]) {
    let mut stdout = io::stdout();
    let mut stderr = io::stderr();
    match mode {
        "ffmpeg-progress" => {
            stderr
                .write_all(b"  Duration: 00:00:02.00, start: 0.000000, bitrate: 1 kb/s\n")
                .unwrap();
            for i in 1..=4 {
                thread::sleep(Duration::from_millis(100));
                write!(stdout, "out_time_us={}\nprogress=continue\n", i * 500_000).unwrap();
                stdout.flush().unwrap();
            }
            writeln!(stdout, "progress=end").unwrap();
        }
        "ffmpeg-args" => {
            for arg in args {
                writeln!(stderr, "{arg}").unwrap();
            }
        }
        "ffmpeg-fail" => {
            writeln!(
                stderr,
                "Error opening output out.mp4: No space left on device"
            )
            .unwrap();
            std::process::exit(1);
        }
        "ffmpeg-hang" => thread::sleep(Duration::from_secs(30)),
        "ffmpeg-slow" => {
            for i in 1..=6 {
                thread::sleep(Duration::from_millis(300));
                write!(stdout, "out_time_us={}\nprogress=continue\n", i * 100_000).unwrap();
                stdout.flush().unwrap();
            }
            writeln!(stdout, "progress=end").unwrap();
        }
        "ffmpeg-flood" => {
            let mut line = vec![b'x'; 64 * 1024 - 1];
            line.push(b'\n');
            for _ in 0..128 {
                stderr.write_all(&line).unwrap();
            }
            writeln!(stderr, "flood-end").unwrap();
        }
        other => panic!("unknown ffmpeg mode {other:?}"),
    }
}
