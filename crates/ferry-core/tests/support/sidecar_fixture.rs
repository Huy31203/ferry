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
