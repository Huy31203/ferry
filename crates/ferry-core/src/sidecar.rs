//! Run a bundled helper program (gifsicle, later ffmpeg) with bytes on stdin
//! and bytes back on stdout. The child can never block: stdin is written and
//! stdout/stderr are drained on their own threads. It is killed on timeout, on
//! cancel, and if this function unwinds.

use std::{
    fmt,
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::cancel::CancelToken;

/// How long a sidecar may run unless the caller says otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub enum SidecarError {
    /// The program could not be started (missing, not executable).
    Spawn(io::Error),
    Timeout,
    Cancelled,
    /// It ran and exited non-zero; `stderr` is what it printed.
    Failed {
        code: Option<i32>,
        stderr: String,
    },
    Io(io::Error),
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not start the helper: {e}"),
            Self::Timeout => write!(f, "the helper timed out"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Failed { code, stderr } => match code {
                Some(code) => write!(f, "the helper exited with {code}: {}", stderr.trim()),
                None => write!(f, "the helper was stopped: {}", stderr.trim()),
            },
            Self::Io(e) => write!(f, "helper I/O failed: {e}"),
        }
    }
}

impl std::error::Error for SidecarError {}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        // Harmless when the child already exited.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn run(
    path: &Path,
    args: &[&str],
    stdin: &[u8],
    timeout: Duration,
    cancel: &CancelToken,
) -> Result<Vec<u8>, SidecarError> {
    let mut command = Command::new(path);
    command
        .args(args)
        .stdin(Stdio::piped())
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
    let mut child = command.spawn().map_err(SidecarError::Spawn)?;
    let mut to_child = child.stdin.take().expect("stdin is piped");
    let mut from_child = child.stdout.take().expect("stdout is piped");
    let mut errors = child.stderr.take().expect("stderr is piped");
    let mut child = KillOnDrop(child);

    thread::scope(|scope| {
        scope.spawn(move || {
            // The child may exit before reading everything; that is its call.
            let _ = to_child.write_all(stdin);
        });
        let stdout = scope.spawn(move || {
            let mut bytes = Vec::new();
            let _ = from_child.read_to_end(&mut bytes);
            bytes
        });
        let stderr = scope.spawn(move || {
            let mut bytes = Vec::new();
            let _ = errors.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.0.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(e) => break Err(SidecarError::Io(e)),
            }
            if cancel.is_cancelled() {
                break Err(SidecarError::Cancelled);
            }
            if Instant::now() >= deadline {
                break Err(SidecarError::Timeout);
            }
            thread::sleep(Duration::from_millis(5));
        };
        if status.is_err() {
            // Closes the pipes, which lets the reader threads finish.
            let _ = child.0.kill();
            let _ = child.0.wait();
        }

        let stdout = stdout.join().expect("stdout reader does not panic");
        let stderr = stderr.join().expect("stderr reader does not panic");
        let status = status?;
        if status.success() {
            Ok(stdout)
        } else {
            Err(SidecarError::Failed {
                code: status.code(),
                stderr,
            })
        }
    })
}
