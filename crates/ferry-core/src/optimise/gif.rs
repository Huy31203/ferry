//! GIF: run the bundled gifsicle (stdin → stdout). gifsicle keeps every frame,
//! so animated GIFs stay animated with the same frame count.

use super::{OptimiseError, OptimiseOptions, quality};
use crate::sidecar::{self, DEFAULT_TIMEOUT, SidecarError};
use crate::sniff::gif_is_complete;

/// gifsicle `--lossy` strength for a quality: 0 at q100, 30 at q80, 90 at q40.
fn lossy(q: u8) -> u32 {
    (f32::from(100 - q.min(100)) * 1.5).round() as u32
}

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    if !gif_is_complete(input) {
        return Err(OptimiseError::Decode("the GIF is incomplete".into()));
    }
    let Some(gifsicle) = &o.gifsicle else {
        return Err(OptimiseError::Unsupported(
            "gifsicle missing — reinstall Ferry".into(),
        ));
    };
    let lossy = format!("--lossy={}", lossy(quality(o)));
    let mut args = vec!["-O3", lossy.as_str(), "--colors", "256"];
    if o.strip_metadata {
        // gifsicle keeps comments and unknown extensions by default; the loop
        // count survives both flags.
        args.extend(["--no-comments", "--no-extensions"]);
    }
    sidecar::run(gifsicle, &args, input, DEFAULT_TIMEOUT, &o.cancel).map_err(|e| match e {
        SidecarError::Cancelled => OptimiseError::Cancelled,
        SidecarError::Spawn(e) => {
            OptimiseError::Unsupported(format!("gifsicle could not start: {e}"))
        }
        other => OptimiseError::Encode(other.to_string()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{optimise::Level, sniff::gif_frame_count, testutil};

    #[test]
    fn lossy_strength_follows_quality() {
        assert_eq!(lossy(100), 0);
        assert_eq!(lossy(80), 30);
        assert_eq!(lossy(60), 60);
        assert_eq!(lossy(5), 143);
    }

    #[test]
    fn without_gifsicle_it_is_unsupported() {
        assert!(matches!(
            optimise(&testutil::gif(1), &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    fn with_gifsicle(o: OptimiseOptions) -> Option<OptimiseOptions> {
        testutil::gifsicle().map(|path| OptimiseOptions {
            gifsicle: Some(path),
            ..o
        })
    }

    #[test]
    fn ci_always_has_gifsicle() {
        if std::env::var_os("CI").is_some() {
            assert!(
                testutil::gifsicle().is_some(),
                "FERRY_GIFSICLE must be set in CI"
            );
        }
    }

    #[test]
    fn a_static_gif_shrinks_and_stays_a_gif() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let input = testutil::gif(1);
        let out = optimise(&input, &o).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        assert_eq!(crate::sniff::sniff(&out), Some(crate::sniff::Format::Gif));
        assert_eq!(gif_frame_count(&out), 1);
    }

    #[test]
    fn an_animated_gif_keeps_every_frame_and_its_loop() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let out = optimise(&testutil::gif(3), &o).unwrap();
        assert_eq!(gif_frame_count(&out), 3);
        assert!(
            out.windows(11).any(|w| w == b"NETSCAPE2.0"),
            "loop extension was lost"
        );
    }

    #[test]
    fn comments_follow_the_metadata_setting() {
        let input = testutil::with_gif_comment(&testutil::gif(1), "hello");
        let contains_comment = |gif: &[u8]| gif.windows(5).any(|w| w == b"hello");
        let Some(strip) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        assert!(!contains_comment(&optimise(&input, &strip).unwrap()));
        let keep = OptimiseOptions {
            strip_metadata: false,
            ..strip
        };
        assert!(contains_comment(&optimise(&input, &keep).unwrap()));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let Some(normal) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let input = testutil::gif(2);
        let aggressive = OptimiseOptions {
            level: Level::Aggressive,
            ..normal.clone()
        };
        assert!(
            optimise(&input, &aggressive).unwrap().len()
                <= optimise(&input, &normal).unwrap().len()
        );
    }

    #[test]
    fn garbage_is_a_decode_error() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        assert!(matches!(
            optimise(b"GIF89a-but-not-really", &o),
            Err(OptimiseError::Decode(_))
        ));
    }

    #[test]
    fn a_gif_cut_short_is_refused_not_padded() {
        // gifsicle exits 0 on a truncated file and pads the missing frame data.
        let o = OptimiseOptions {
            gifsicle: testutil::gifsicle(),
            ..OptimiseOptions::default()
        };
        let full = testutil::gif(3);
        for cut in [full.len() / 2, full.len() * 7 / 10, full.len() * 9 / 10] {
            let err = optimise(&full[..cut], &o).unwrap_err();
            assert!(
                matches!(&err, OptimiseError::Decode(m) if m.contains("incomplete")),
                "cut at {cut}: {err}"
            );
        }
    }
}
