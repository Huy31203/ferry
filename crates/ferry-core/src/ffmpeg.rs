//! Run the bundled ffmpeg sidecar. Output is drained continuously on its own
//! threads, progress comes from `-progress pipe:1`, and the child is killed on
//! cancel, on a stall, on `kill_all` (app exit) and if the caller unwinds.

use std::{
    ffi::{OsStr, OsString},
    io::{BufRead, BufReader, Read},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        mpsc::{self, RecvTimeoutError},
    },
    thread,
    time::{Duration, Instant},
};

use crate::cancel::CancelToken;
pub use crate::error::FerryError;

pub const ENV: &str = "FERRY_FFMPEG";
/// How long ffmpeg may print nothing on either pipe before it counts as hung.
pub const STALL: Duration = Duration::from_secs(120);

/// The last of stderr kept for `Captured` (a ring: older bytes are dropped).
const KEEP: usize = 1024 * 1024;
/// The part of stderr an error carries.
const ERROR_TAIL: usize = 4096;
/// Options every run starts with; callers never pass them.
const GLOBAL: [&str; 8] = [
    "-hide_banner",
    "-nostdin",
    "-y",
    "-nostats",
    "-progress",
    "pipe:1",
    "-stats_period",
    "0.5",
];

/// Bumped by `kill_all`; every running `exec` kills its child when it changes.
static EPOCH: AtomicU64 = AtomicU64::new(0);
static RUNNING: AtomicUsize = AtomicUsize::new(0);

#[derive(Debug)]
pub struct Captured {
    pub code: Option<i32>,
    pub stderr: String,
}

pub struct Exec {
    /// The output's length, when the input's own duration is not the answer.
    pub expected: Option<Duration>,
    pub stall: Duration,
    /// A non-zero exit is an error.
    pub check_status: bool,
}

impl Default for Exec {
    fn default() -> Self {
        Self {
            expected: None,
            stall: STALL,
            check_status: true,
        }
    }
}

pub fn locate() -> Result<PathBuf, FerryError> {
    locate_in(std::env::var_os(ENV), std::env::current_exe().ok())
}

fn locate_in(env: Option<OsString>, exe: Option<PathBuf>) -> Result<PathBuf, FerryError> {
    if let Some(path) = env {
        let path = PathBuf::from(path);
        return if path.is_file() {
            Ok(path)
        } else {
            Err(FerryError::FfmpegMissing)
        };
    }
    exe.map(|exe| exe.with_file_name(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX)))
        .filter(|path| path.is_file())
        .ok_or(FerryError::FfmpegMissing)
}

pub fn run(
    args: &[OsString],
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(f32),
) -> Result<(), FerryError> {
    exec(&locate()?, args, &Exec::default(), cancel, on_progress).map(drop)
}

pub fn run_expecting(
    args: &[OsString],
    expected: Duration,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(f32),
) -> Result<(), FerryError> {
    let how = Exec {
        expected: Some(expected),
        ..Exec::default()
    };
    exec(&locate()?, args, &how, cancel, on_progress).map(drop)
}

/// Run to the end whatever the exit code and return all of stderr (probing,
/// loudness analysis).
pub fn capture(args: &[OsString], cancel: &CancelToken) -> Result<Captured, FerryError> {
    let how = Exec {
        check_status: false,
        ..Exec::default()
    };
    exec(&locate()?, args, &how, cancel, &mut |_| {})
}

/// Stop every running ffmpeg and wait (at most a second) for them to go.
pub fn kill_all() {
    EPOCH.fetch_add(1, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(1);
    while RUNNING.load(Ordering::SeqCst) > 0 && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(10));
    }
}

struct Running(Child);

impl Running {
    fn new(child: Child) -> Self {
        RUNNING.fetch_add(1, Ordering::SeqCst);
        Self(child)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        // Harmless when the child already exited.
        let _ = self.0.kill();
        let _ = self.0.wait();
        RUNNING.fetch_sub(1, Ordering::SeqCst);
    }
}

#[derive(Clone, Copy)]
enum Pipe {
    Out,
    Err,
}

fn pump(from: impl Read, pipe: Pipe, to: mpsc::Sender<(Pipe, String)>) {
    let mut reader = BufReader::new(from);
    let mut line = Vec::new();
    loop {
        line.clear();
        match reader.read_until(b'\n', &mut line) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let text = String::from_utf8_lossy(&line).trim_end().to_string();
        if to.send((pipe, text)).is_err() {
            break;
        }
    }
}

pub fn exec(
    program: &Path,
    args: &[OsString],
    how: &Exec,
    cancel: &CancelToken,
    on_progress: &mut dyn FnMut(f32),
) -> Result<Captured, FerryError> {
    let epoch = EPOCH.load(Ordering::SeqCst);
    let mut command = Command::new(program);
    command
        .args(GLOBAL)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    // Ferry is a GUI app, so Windows would open (and focus) a console window for
    // every console child, even with all three handles piped.
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const CREATE_NO_WINDOW: u32 = 0x0800_0000;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let mut child = command.spawn().map_err(FerryError::Spawn)?;
    let stdout = child.stdout.take().expect("stdout is piped");
    let stderr = child.stderr.take().expect("stderr is piped");
    let mut child = Running::new(child);
    #[cfg(windows)]
    win::kill_with_ferry(&child.0);

    let (tx, rx) = mpsc::channel();
    thread::scope(|scope| {
        let out_tx = tx.clone();
        scope.spawn(move || pump(stdout, Pipe::Out, out_tx));
        scope.spawn(move || pump(stderr, Pipe::Err, tx));

        let mut progress = Progress::new(how.expected);
        let mut tail: Vec<u8> = Vec::new();
        let mut last_output = Instant::now();
        let status: Result<ExitStatus, FerryError> = loop {
            match rx.recv_timeout(Duration::from_millis(50)) {
                Ok((pipe, line)) => {
                    last_output = Instant::now();
                    let fraction = match pipe {
                        Pipe::Out => progress.feed(&line),
                        Pipe::Err => {
                            progress.learn_total(&line);
                            tail.extend_from_slice(line.as_bytes());
                            tail.push(b'\n');
                            if tail.len() > 2 * KEEP {
                                tail.drain(..tail.len() - KEEP);
                            }
                            None
                        }
                    };
                    if let Some(fraction) = fraction {
                        on_progress(fraction);
                    }
                }
                Err(RecvTimeoutError::Timeout) => {}
                // Both pipes closed: it has exited (or is about to).
                Err(RecvTimeoutError::Disconnected) => {
                    break child.0.wait().map_err(FerryError::Io);
                }
            }
            if cancel.is_cancelled() || EPOCH.load(Ordering::SeqCst) != epoch {
                break Err(FerryError::Cancelled);
            }
            if last_output.elapsed() > how.stall {
                break Err(FerryError::Stalled);
            }
        };
        if status.is_err() {
            // Closes the pipes, which lets the reader threads finish.
            let _ = child.0.kill();
            let _ = child.0.wait();
        }
        let status = status?;

        let stderr = tail_text(&tail, KEEP);
        if how.check_status && !status.success() {
            let command = std::iter::once(program.as_os_str())
                .chain(GLOBAL.iter().map(OsStr::new))
                .chain(args.iter().map(OsString::as_os_str))
                .map(|part| part.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" ");
            return Err(FerryError::Failed {
                code: status.code(),
                command,
                stderr: tail_text(&tail, ERROR_TAIL),
            });
        }
        Ok(Captured {
            code: status.code(),
            stderr,
        })
    })
}

fn tail_text(bytes: &[u8], keep: usize) -> String {
    String::from_utf8_lossy(&bytes[bytes.len().saturating_sub(keep)..]).into_owned()
}

/// `Duration: 00:01:02.50, start: …` as printed for each input; `N/A` is None.
fn parse_duration(line: &str) -> Option<Duration> {
    let rest = line.trim_start().strip_prefix("Duration: ")?;
    let stamp = rest.split(',').next()?;
    let mut parts = stamp.split(':');
    let hours: u64 = parts.next()?.parse().ok()?;
    let minutes: u64 = parts.next()?.parse().ok()?;
    let seconds: f64 = parts.next()?.parse().ok()?;
    Some(Duration::from_secs(hours * 3600 + minutes * 60) + Duration::from_secs_f64(seconds))
}

/// Turns ffmpeg's `-progress` lines into a fraction that only ever rises.
struct Progress {
    total: Option<Duration>,
    fixed: bool,
    last: f32,
}

impl Progress {
    fn new(expected: Option<Duration>) -> Self {
        Self {
            total: expected,
            fixed: expected.is_some(),
            last: 0.0,
        }
    }

    /// The input's duration, unless the caller said how long the output is.
    fn learn_total(&mut self, line: &str) {
        if !self.fixed && self.total.is_none() {
            self.total = parse_duration(line);
        }
    }

    fn feed(&mut self, line: &str) -> Option<f32> {
        let fraction = if line == "progress=end" {
            1.0
        } else {
            let micros = line
                .strip_prefix("out_time_us=")
                .or_else(|| line.strip_prefix("out_time_ms="))?
                .parse::<i64>()
                .ok()?;
            let total = self.total?.as_micros() as f64;
            if micros < 0 || total <= 0.0 {
                return None;
            }
            (micros as f64 / total).min(1.0) as f32
        };
        (fraction > self.last).then(|| {
            self.last = fraction;
            fraction
        })
    }
}

#[cfg(windows)]
mod win {
    use std::{os::windows::io::AsRawHandle, process::Child, sync::OnceLock};

    use windows_sys::Win32::System::JobObjects::{
        AssignProcessToJobObject, CreateJobObjectW, JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE,
        JOBOBJECT_EXTENDED_LIMIT_INFORMATION, JobObjectExtendedLimitInformation,
        SetInformationJobObject,
    };

    /// One Job Object for the whole process, with kill-on-close: when Ferry
    /// dies, Windows closes the handle and ends every ffmpeg in it.
    static JOB: OnceLock<usize> = OnceLock::new();

    /// Best effort: failures leave ffmpeg outside the job.
    pub fn kill_with_ferry(child: &Child) {
        let job = *JOB.get_or_init(|| unsafe {
            let job = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if job.is_null() {
                return 0;
            }
            let mut info = JOBOBJECT_EXTENDED_LIMIT_INFORMATION::default();
            info.BasicLimitInformation.LimitFlags = JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE;
            SetInformationJobObject(
                job,
                JobObjectExtendedLimitInformation,
                (&raw const info).cast(),
                size_of::<JOBOBJECT_EXTENDED_LIMIT_INFORMATION>() as u32,
            );
            job as usize
        });
        if job != 0 {
            unsafe { AssignProcessToJobObject(job as _, child.as_raw_handle()) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn ci_always_has_ffmpeg() {
        if std::env::var_os("CI").is_some() {
            assert!(
                testutil::ffmpeg().is_some(),
                "FERRY_FFMPEG must be set in CI"
            );
        }
    }

    #[test]
    fn the_inputs_duration_is_read_from_stderr() {
        assert_eq!(
            parse_duration("  Duration: 00:01:02.50, start: 0.000000, bitrate: 1 kb/s"),
            Some(Duration::from_millis(62_500))
        );
        assert_eq!(
            parse_duration("  Duration: N/A, start: 0.000000, bitrate: N/A"),
            None
        );
        assert_eq!(parse_duration("frame=1"), None);
    }

    #[test]
    fn progress_lines_become_fractions_of_the_total() {
        let mut p = Progress::new(Some(Duration::from_secs(10)));
        assert_eq!(p.feed("out_time_us=2500000"), Some(0.25));
        assert_eq!(p.feed("out_time_ms=5000000"), Some(0.5));
        assert_eq!(p.feed("out_time_us=N/A"), None);
        assert_eq!(p.feed("out_time_us=99000000"), Some(1.0)); // clamped
        assert_eq!(p.feed("out_time_us=1"), None); // never goes back
        assert_eq!(Progress::new(None).feed("progress=end"), Some(1.0));
    }

    #[test]
    fn ffmpeg_is_found_from_the_env_or_beside_ferry() {
        let dir = tempfile::tempdir().unwrap();
        let name = format!("ffmpeg{}", std::env::consts::EXE_SUFFIX);
        let beside = dir.path().join(&name);
        std::fs::write(&beside, b"").unwrap();
        let exe = Some(dir.path().join("ferry"));
        let elsewhere = dir.path().join("elsewhere");
        std::fs::write(&elsewhere, b"").unwrap();

        let found = locate_in(Some(elsewhere.clone().into()), exe.clone()).unwrap();
        assert_eq!(found, elsewhere);
        let missing = Some(dir.path().join("nope").into());
        assert!(matches!(
            locate_in(missing, exe.clone()),
            Err(FerryError::FfmpegMissing)
        ));
        assert_eq!(locate_in(None, exe).unwrap(), beside);

        let empty = tempfile::tempdir().unwrap();
        assert!(matches!(
            locate_in(None, Some(empty.path().join("ferry"))),
            Err(FerryError::FfmpegMissing)
        ));
        assert!(matches!(
            locate_in(None, None),
            Err(FerryError::FfmpegMissing)
        ));
    }
}
