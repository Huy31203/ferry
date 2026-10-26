//! Errors from running ffmpeg and from video jobs. `reason` is the one line a
//! card shows; `Display` is the technical text for the log.

use std::{fmt, io};

use crate::i18n::t;

/// What ffmpeg prints when the disk fills.
const DISK_FULL: &str = "No space left on device";

#[derive(Debug)]
pub enum FerryError {
    /// No ffmpeg next to Ferry, and FERRY_FFMPEG unset or not a file.
    FfmpegMissing,
    /// It could not be started (permissions, a broken binary).
    Spawn(io::Error),
    Cancelled,
    /// No output on stdout or stderr for the stall limit.
    Stalled,
    /// It ran and exited non-zero (or was killed by a signal: code None).
    Failed {
        code: Option<i32>,
        command: String,
        stderr: String,
    },
    Io(io::Error),
    /// Ferry cannot handle this input; the text is the card's reason.
    Unsupported(String),
}

impl FerryError {
    /// One-line card text.
    pub fn reason(&self) -> String {
        match self {
            Self::FfmpegMissing => t("video.error.ffmpeg_missing"),
            Self::Spawn(_) => t("video.error.spawn"),
            Self::Cancelled => t("error.cancelled"),
            Self::Stalled => t("video.error.stalled"),
            Self::Failed { stderr, .. } if stderr.contains(DISK_FULL) => t("video.error.disk_full"),
            Self::Failed { .. } => t("video.error.encode"),
            Self::Io(e) if e.kind() == io::ErrorKind::StorageFull => t("video.error.disk_full"),
            Self::Io(e) => e.to_string(),
            Self::Unsupported(message) => message.clone(),
        }
    }
}

impl FerryError {
    /// Whether the encoder itself failed, so another encoder may do better. A
    /// full disk or a stalled run would fail the same way again.
    pub fn is_encoder_failure(&self) -> bool {
        matches!(self, Self::Failed { stderr, .. } if !stderr.contains(DISK_FULL))
    }
}

impl fmt::Display for FerryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FfmpegMissing => write!(f, "ffmpeg was not found"),
            Self::Spawn(e) => write!(f, "could not start ffmpeg: {e}"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Stalled => write!(f, "ffmpeg stopped responding"),
            Self::Failed {
                code,
                command,
                stderr,
            } => {
                match code {
                    Some(code) => write!(f, "ffmpeg exited with {code}")?,
                    None => write!(f, "ffmpeg was stopped")?,
                }
                write!(f, ": {command}\n{}", stderr.trim())
            }
            Self::Io(e) => write!(f, "I/O failed: {e}"),
            Self::Unsupported(message) => write!(f, "{message}"),
        }
    }
}

impl std::error::Error for FerryError {}

impl From<io::Error> for FerryError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_genuine_encoder_failure_is_worth_a_retry() {
        let failed = |stderr: &str| FerryError::Failed {
            code: Some(1),
            command: "ffmpeg".into(),
            stderr: stderr.into(),
        };
        assert!(failed("Error while opening encoder").is_encoder_failure());
        assert!(
            !failed("av_interleaved_write_frame(): No space left on device").is_encoder_failure()
        );
        assert!(!FerryError::Stalled.is_encoder_failure());
        assert!(!FerryError::Io(io::ErrorKind::StorageFull.into()).is_encoder_failure());
        assert!(!FerryError::Spawn(io::Error::other("x")).is_encoder_failure());
    }

    #[test]
    fn every_error_has_a_one_line_reason() {
        let all = [
            FerryError::FfmpegMissing,
            FerryError::Spawn(io::Error::other("x")),
            FerryError::Cancelled,
            FerryError::Stalled,
            FerryError::Failed {
                code: Some(1),
                command: "ffmpeg".into(),
                stderr: "boom\nmore".into(),
            },
            FerryError::Failed {
                code: None,
                command: "ffmpeg".into(),
                stderr: "No space left on device".into(),
            },
            FerryError::Io(io::Error::other("x")),
            FerryError::Io(io::ErrorKind::StorageFull.into()),
            FerryError::Unsupported("nope".into()),
        ];
        for error in all {
            let reason = error.reason();
            assert!(!reason.is_empty() && !reason.contains('\n'), "{error:?}");
        }
        assert_eq!(
            FerryError::FfmpegMissing.reason(),
            "ffmpeg missing — reinstall Ferry"
        );
    }
}
