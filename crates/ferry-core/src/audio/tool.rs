//! The one door to ffmpeg for audio: a trait over stream A's `crate::ffmpeg`, so
//! audio logic is tested without a real ffmpeg, plus the once-per-process check
//! for Apple's AAC encoder.

use std::{
    ffi::OsString,
    sync::{Arc, OnceLock},
};

use crate::{
    cancel::CancelToken,
    ffmpeg::{self, Exec},
};

pub trait AudioTool: Send + Sync + std::fmt::Debug {
    /// Run ffmpeg with `args`; `Ok` is ffmpeg's log (its stderr text).
    fn run(
        &self,
        args: &[OsString],
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<String, ToolError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ToolError {
    Cancelled,
    Failed(String),
}

/// The bundled ffmpeg.
#[derive(Debug, Default)]
pub struct Ffmpeg;

impl AudioTool for Ffmpeg {
    fn run(
        &self,
        args: &[OsString],
        cancel: &CancelToken,
        on_progress: &mut dyn FnMut(f32),
    ) -> Result<String, ToolError> {
        if cancel.is_cancelled() {
            return Err(ToolError::Cancelled);
        }
        let program = ffmpeg::locate().map_err(|e| ToolError::Failed(e.to_string()))?;
        ffmpeg::exec(&program, args, &Exec::default(), cancel, on_progress)
            .map(|captured| captured.stderr)
            .map_err(|e| match e {
                ffmpeg::FerryError::Cancelled => ToolError::Cancelled,
                _ if cancel.is_cancelled() => ToolError::Cancelled,
                e => ToolError::Failed(e.to_string()),
            })
    }
}

/// The production tool, when the sidecar is there.
pub fn installed() -> Option<Arc<dyn AudioTool>> {
    ffmpeg::locate()
        .ok()
        .map(|_| Arc::new(Ffmpeg) as Arc<dyn AudioTool>)
}

/// Which AAC encoder an M4A result uses.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum AacEncoder {
    #[default]
    Native,
    /// Apple's, through FFmpeg's `aac_at` wrapper (macOS builds only).
    AudioToolbox,
}

impl AacEncoder {
    /// The ffmpeg encoder name.
    pub fn codec(self) -> &'static str {
        match self {
            AacEncoder::Native => "aac",
            AacEncoder::AudioToolbox => "aac_at",
        }
    }
}

/// 0.1 s of silence through `aac_at`: proves the encoder is there and opens.
/// (`-encoders` prints to stdout, which `ffmpeg::exec` reads as progress.)
pub fn aac_at_probe_args() -> Vec<OsString> {
    [
        "-f",
        "lavfi",
        "-i",
        "anullsrc=r=48000:cl=stereo",
        "-t",
        "0.1",
        "-c:a",
        "aac_at",
        "-b:a",
        "128k",
        "-f",
        "null",
        "-",
    ]
    .map(OsString::from)
    .into()
}

pub fn detect_aac(tool: &dyn AudioTool) -> AacEncoder {
    match tool.run(&aac_at_probe_args(), &CancelToken::new(), &mut |_| {}) {
        Ok(_) => AacEncoder::AudioToolbox,
        Err(_) => AacEncoder::Native,
    }
}

/// Only macOS asks; everywhere else, and on a Mac without ffmpeg, it is native.
pub fn choose_aac(macos: bool, tool: Option<&dyn AudioTool>) -> AacEncoder {
    if !macos {
        return AacEncoder::Native;
    }
    tool.map_or(AacEncoder::Native, detect_aac)
}

/// The choice, made once per process.
pub fn aac_encoder() -> AacEncoder {
    static AAC: OnceLock<AacEncoder> = OnceLock::new();
    *AAC.get_or_init(|| {
        let macos = cfg!(target_os = "macos");
        let tool = if macos { installed() } else { None };
        choose_aac(macos, tool.as_deref())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    #[test]
    fn a_cancelled_token_never_starts_ffmpeg() {
        let c = CancelToken::new();
        c.cancel();
        assert_eq!(Ffmpeg.run(&[], &c, &mut |_| {}), Err(ToolError::Cancelled));
    }

    #[test]
    fn ffmpeg_hands_back_its_log() {
        let Some(tool) = installed() else { return };
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("tone.wav");
        std::fs::write(&wav, testutil::wav_tone(&[(0.5, 0.5)], 44100, 2)).unwrap();
        let args: Vec<OsString> = ["-i"]
            .map(OsString::from)
            .into_iter()
            .chain([wav.into_os_string()])
            .chain(["-f", "null", "-"].map(OsString::from))
            .collect();
        let log = tool.run(&args, &CancelToken::new(), &mut |_| {}).unwrap();
        assert!(log.contains("Input #0, wav"), "{log}");
        assert!(log.contains("44100 Hz"), "{log}");
    }

    #[test]
    fn a_failing_run_is_failed_with_ffmpegs_reason() {
        let Some(tool) = installed() else { return };
        let args = ["-i", "/no/such/file.wav", "-f", "null", "-"].map(OsString::from);
        assert!(matches!(
            tool.run(&args, &CancelToken::new(), &mut |_| {}),
            Err(ToolError::Failed(m)) if !m.is_empty()
        ));
    }

    #[test]
    fn the_aac_at_probe_encodes_a_tenth_of_a_second_of_silence() {
        let a: Vec<String> = aac_at_probe_args()
            .iter()
            .map(|s| s.to_string_lossy().into())
            .collect();
        assert_eq!(
            a,
            [
                "-f",
                "lavfi",
                "-i",
                "anullsrc=r=48000:cl=stereo",
                "-t",
                "0.1",
                "-c:a",
                "aac_at",
                "-b:a",
                "128k",
                "-f",
                "null",
                "-"
            ]
        );
    }

    #[test]
    fn audiotoolbox_aac_is_chosen_only_when_a_test_encode_succeeds() {
        let yes = testutil::FakeTool::new(vec![testutil::FakeStep::Log(String::new())]);
        assert_eq!(detect_aac(&*yes), AacEncoder::AudioToolbox);
        let probe: Vec<String> = aac_at_probe_args()
            .iter()
            .map(|s| s.to_string_lossy().into())
            .collect();
        assert_eq!(yes.calls(), vec![probe]);
        let no = testutil::FakeTool::new(vec![testutil::FakeStep::Fail(
            "Unknown encoder 'aac_at'".into(),
        )]);
        assert_eq!(detect_aac(&*no), AacEncoder::Native);
        assert_eq!(
            (
                AacEncoder::default(),
                AacEncoder::Native.codec(),
                AacEncoder::AudioToolbox.codec()
            ),
            (AacEncoder::Native, "aac", "aac_at")
        );
    }

    #[test]
    fn only_macos_runs_the_aac_at_test_encode() {
        let works = testutil::FakeTool::new(vec![testutil::FakeStep::Log(String::new())]);
        assert_eq!(choose_aac(false, Some(&*works)), AacEncoder::Native);
        assert!(works.calls().is_empty());
        assert_eq!(choose_aac(true, None), AacEncoder::Native);
        assert_eq!(choose_aac(true, Some(&*works)), AacEncoder::AudioToolbox);
        assert_eq!(works.calls().len(), 1);
        let broken = testutil::FakeTool::new(vec![testutil::FakeStep::Fail(
            "Error while opening encoder".into(),
        )]);
        assert_eq!(choose_aac(true, Some(&*broken)), AacEncoder::Native);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn the_macos_sidecar_offers_audiotoolbox_aac() {
        let Some(tool) = installed() else { return };
        assert_eq!(
            detect_aac(&*tool),
            AacEncoder::AudioToolbox,
            "A's macOS ffmpeg build needs --enable-audiotoolbox"
        );
        assert_eq!(aac_encoder(), AacEncoder::AudioToolbox);
    }

    #[cfg(not(target_os = "macos"))]
    #[test]
    fn other_systems_use_native_aac_without_asking_ffmpeg() {
        assert_eq!(aac_encoder(), AacEncoder::Native);
    }
}
