//! GIF: run the bundled gifsicle (stdin → stdout). gifsicle keeps every frame,
//! so animated GIFs stay animated with the same frame count.

use super::{OptimiseError, OptimiseOptions, quality};
use crate::edit::geometry::Rect;
use crate::sidecar::{self, DEFAULT_TIMEOUT, SidecarError};
use crate::sniff::gif_is_complete;

/// gifsicle `--lossy` strength for a quality: 0 at q100, 30 at q80, 90 at q40.
fn lossy(q: u8) -> u32 {
    (f32::from(100 - q.min(100)) * 1.5).round() as u32
}

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    edit(input, None, None, o)
}

/// Crop, then resize, every frame, then optimise as usual. gifsicle applies
/// `--crop` before `--resize`, and keeps the frame count and timing.
pub fn edit(
    input: &[u8],
    crop: Option<Rect>,
    size: Option<(u32, u32)>,
    o: &OptimiseOptions,
) -> Result<Vec<u8>, OptimiseError> {
    if !gif_is_complete(input) {
        return Err(OptimiseError::Decode("the GIF is incomplete".into()));
    }
    let Some(gifsicle) = &o.gifsicle else {
        return Err(OptimiseError::Unsupported(
            "gifsicle missing — reinstall Ferry".into(),
        ));
    };
    let mut args = edit_args(crop, size);
    args.extend([
        "-O3".into(),
        format!("--lossy={}", lossy(quality(o))),
        "--colors".into(),
        "256".into(),
    ]);
    if o.strip_metadata {
        // gifsicle keeps comments and unknown extensions by default; the loop
        // count survives both flags.
        args.extend(["--no-comments".into(), "--no-extensions".into()]);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    sidecar::run(gifsicle, &args, input, DEFAULT_TIMEOUT, &o.cancel).map_err(|e| match e {
        SidecarError::Cancelled => OptimiseError::Cancelled,
        SidecarError::Spawn(e) => {
            OptimiseError::Unsupported(format!("gifsicle could not start: {e}"))
        }
        other => OptimiseError::Encode(other.to_string()),
    })
}

fn edit_args(crop: Option<Rect>, size: Option<(u32, u32)>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(c) = crop {
        args.push(format!("--crop={},{}+{}x{}", c.x, c.y, c.width, c.height));
    }
    if let Some((w, h)) = size {
        args.push(format!("--resize={w}x{h}"));
    }
    args
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

    #[test]
    fn crop_comes_before_resize_in_the_arguments() {
        let crop = Rect {
            x: 8,
            y: 4,
            width: 32,
            height: 24,
        };
        assert_eq!(
            edit_args(Some(crop), Some((16, 12))),
            ["--crop=8,4+32x24", "--resize=16x12"]
        );
        assert!(edit_args(None, None).is_empty());
    }

    #[test]
    fn an_animated_gif_is_cropped_and_resized_frame_by_frame() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let input = testutil::gif(3);
        let crop = Rect {
            x: 8,
            y: 8,
            width: 32,
            height: 24,
        };
        let out = edit(&input, Some(crop), Some((16, 12)), &o).unwrap();
        assert_eq!(gif_frame_count(&out), 3);
        assert_eq!(
            crate::sniff::dimensions(&out, crate::sniff::Format::Gif),
            Some((16, 12))
        );
    }
}
