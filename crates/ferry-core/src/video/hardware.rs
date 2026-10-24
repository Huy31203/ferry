//! Which hardware encoders this machine can use. A 1-second test encode per
//! candidate, run once in the background at start-up (R§5).

use std::{ffi::OsString, sync::OnceLock};

use super::Codec;
use crate::{cancel::CancelToken, ffmpeg};

/// `hevc == None`: this machine cannot write HEVC at all (the build has no
/// x265), so HEVC is not offered and anything that needs it falls back to H.264.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hardware {
    pub h264: Option<&'static str>,
    pub hevc: Option<&'static str>,
}

pub fn candidates(codec: Codec) -> &'static [&'static str] {
    match codec {
        Codec::H264 if cfg!(target_os = "macos") => &["h264_videotoolbox"],
        Codec::Hevc if cfg!(target_os = "macos") => &["hevc_videotoolbox"],
        Codec::H264 if cfg!(windows) => &["h264_nvenc", "h264_qsv", "h264_amf"],
        Codec::Hevc if cfg!(windows) => &["hevc_nvenc", "hevc_qsv", "hevc_amf"],
        _ => &[],
    }
}

/// The first working candidate per codec, in order.
pub fn detect(works: impl Fn(&str) -> bool) -> Hardware {
    let first = |codec| candidates(codec).iter().copied().find(|e| works(e));
    Hardware {
        h264: first(Codec::H264),
        hevc: first(Codec::Hevc),
    }
}

static PROBED: OnceLock<Hardware> = OnceLock::new();

/// Probe once per process; a second caller waits for the first probe to finish.
pub fn probe_hardware() -> Hardware {
    *PROBED.get_or_init(|| detect(test_encode))
}

/// The probed value, or none before `probe_hardware` has finished.
pub fn hardware() -> Hardware {
    PROBED.get().copied().unwrap_or_default()
}

fn test_encode(encoder: &str) -> bool {
    let args: Vec<OsString> = [
        "-f",
        "lavfi",
        "-i",
        "color=c=black:s=256x256:r=30:d=1",
        "-pix_fmt",
        "nv12",
        "-c:v",
        encoder,
        "-f",
        "null",
        "-",
    ]
    .iter()
    .map(OsString::from)
    .collect();
    ffmpeg::run(&args, &CancelToken::new(), &mut |_| {}).is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_first_working_candidate_wins_per_codec() {
        let hw = detect(|e| e.ends_with("_qsv") || e == "hevc_amf");
        if cfg!(windows) {
            assert_eq!(
                hw,
                Hardware {
                    h264: Some("h264_qsv"),
                    hevc: Some("hevc_qsv")
                }
            );
        }
        let none = detect(|_| false);
        assert_eq!(none, Hardware::default());
    }

    #[test]
    fn macos_offers_videotoolbox() {
        if cfg!(target_os = "macos") {
            assert_eq!(candidates(Codec::Hevc), ["hevc_videotoolbox"]);
        }
        assert!(candidates(Codec::Av1).is_empty());
    }

    #[test]
    fn hevc_has_hardware_candidates_only_where_an_os_encoder_exists() {
        // HEVC has no software fallback, so its list is the whole story.
        assert_eq!(
            candidates(Codec::Hevc).is_empty(),
            !(cfg!(target_os = "macos") || cfg!(windows))
        );
        assert!(
            candidates(Codec::Hevc)
                .iter()
                .all(|e| e.starts_with("hevc_"))
        );
    }
}
