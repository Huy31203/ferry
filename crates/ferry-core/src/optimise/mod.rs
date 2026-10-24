//! Optimise one image from memory (spec §4.2). Never worse than the input:
//! the caller keeps the original unless the result is `Smaller`.

pub(crate) mod gif;
pub(crate) mod jpeg;
pub(crate) mod png;
pub(crate) mod webp;

use std::{fmt, path::PathBuf};

use serde::{Deserialize, Serialize};

use crate::{cancel::CancelToken, sniff::Format};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    #[default]
    Normal,
    Aggressive,
}

// stream F: pipelines
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoder {
    Fast,
    Small,
    VisuallyLossless,
}

#[derive(Debug, Clone)]
pub struct OptimiseOptions {
    pub level: Level,
    /// 5..=100, from settings (default 80).
    pub compression: u8,
    pub strip_metadata: bool,
    /// Resolved sidecar path; `None` makes GIF return `Unsupported`.
    pub gifsicle: Option<PathBuf>,
    pub cancel: CancelToken,
}

impl Default for OptimiseOptions {
    fn default() -> Self {
        Self {
            level: Level::Normal,
            compression: 80,
            strip_metadata: true,
            gifsicle: None,
            cancel: CancelToken::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// At least 1% smaller than the input.
    Smaller { bytes: Vec<u8> },
    /// Within 1% of the input, or an animation Ferry keeps unchanged.
    AlreadyOptimal,
    /// The encoder's output is bigger than the input.
    Larger,
}

#[derive(Debug)]
pub enum OptimiseError {
    Decode(String),
    Encode(String),
    Unsupported(String),
    Cancelled,
}

impl fmt::Display for OptimiseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(m) => write!(f, "could not read the image: {m}"),
            Self::Encode(m) => write!(f, "encoder failed: {m}"),
            Self::Unsupported(m) => write!(f, "unsupported: {m}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for OptimiseError {}

/// The encoder quality for these options: `compression`, or 20 lower (never
/// below 5) when aggressive.
pub fn quality(o: &OptimiseOptions) -> u8 {
    let q = o.compression.clamp(5, 100);
    match o.level {
        Level::Normal => q,
        Level::Aggressive => q.saturating_sub(20).max(5),
    }
}

/// Grade an encoder's output against the input it came from.
pub fn classify(input_len: usize, output: Vec<u8>) -> Outcome {
    // `output <= 99% of input` in integers, so exactly 1% smaller counts as Smaller.
    if output.len() * 100 <= input_len * 99 {
        Outcome::Smaller { bytes: output }
    } else if output.len() > input_len {
        Outcome::Larger
    } else {
        Outcome::AlreadyOptimal
    }
}

pub fn optimise(
    input: &[u8],
    format: Format,
    o: &OptimiseOptions,
) -> Result<Outcome, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    if crate::sniff::is_animated(input, format) && format != Format::Gif {
        return Ok(Outcome::AlreadyOptimal);
    }
    let output = encode(input, format, o)?;
    Ok(classify(input.len(), output))
}

/// The encoder's output for `input`, whatever its size; `optimise` grades it
/// and an explicit edit keeps it.
pub fn encode(input: &[u8], format: Format, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    let output = match format {
        Format::Png => png::optimise(input, o)?,
        Format::Jpeg => jpeg::optimise(input, o)?,
        Format::WebP => webp::optimise(input, o)?,
        Format::Gif => gif::optimise(input, o)?,
        // stream A: video
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => {
            return Err(OptimiseError::Unsupported("video is not an image".into()));
        }
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn opts(level: Level, compression: u8) -> OptimiseOptions {
        OptimiseOptions {
            level,
            compression,
            ..OptimiseOptions::default()
        }
    }

    #[test]
    fn video_formats_are_not_image_encoders() {
        assert!(matches!(
            encode(b"x", Format::Mp4, &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    #[test]
    fn aggressive_is_twenty_lower_and_never_below_five() {
        assert_eq!(quality(&opts(Level::Normal, 80)), 80);
        assert_eq!(quality(&opts(Level::Aggressive, 80)), 60);
        assert_eq!(quality(&opts(Level::Aggressive, 20)), 5);
        assert_eq!(quality(&opts(Level::Aggressive, 5)), 5);
        assert_eq!(quality(&opts(Level::Normal, 0)), 5);
    }

    #[test]
    fn classify_boundaries() {
        // 1000-byte input: 990 is exactly 1% smaller.
        assert!(matches!(
            classify(1000, vec![0; 990]),
            Outcome::Smaller { .. }
        ));
        assert_eq!(classify(1000, vec![0; 991]), Outcome::AlreadyOptimal);
        assert_eq!(classify(1000, vec![0; 1000]), Outcome::AlreadyOptimal);
        assert_eq!(classify(1000, vec![0; 1001]), Outcome::Larger);
    }

    #[test]
    fn a_cancelled_token_stops_before_any_work() {
        let o = OptimiseOptions::default();
        o.cancel.cancel();
        assert!(matches!(
            optimise(b"not even an image", Format::Png, &o),
            Err(OptimiseError::Cancelled)
        ));
    }

    #[test]
    fn a_one_pixel_png_comes_back_larger_so_the_original_is_kept() {
        let tiny = testutil::png(1, 1);
        assert_eq!(
            optimise(&tiny, Format::Png, &OptimiseOptions::default()).unwrap(),
            Outcome::Larger
        );
    }

    #[test]
    fn re_optimising_a_webp_is_already_optimal() {
        let o = OptimiseOptions::default();
        let source = testutil::webp(400, 300, 95, &webp::Extras::default());
        let Outcome::Smaller { bytes: once } = optimise(&source, Format::WebP, &o).unwrap() else {
            panic!("the first pass should shrink a quality-95 WebP");
        };
        assert_eq!(
            optimise(&once, Format::WebP, &o).unwrap(),
            Outcome::AlreadyOptimal
        );
    }

    #[test]
    fn animated_webp_and_apng_are_left_alone() {
        let o = OptimiseOptions::default();
        assert_eq!(
            optimise(&testutil::animated_webp(), Format::WebP, &o).unwrap(),
            Outcome::AlreadyOptimal
        );
        // acTL (frames = 2, plays = 0) marks a PNG as animated.
        let apng =
            testutil::with_png_chunk(&testutil::png(64, 64), b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]);
        assert_eq!(
            optimise(&apng, Format::Png, &o).unwrap(),
            Outcome::AlreadyOptimal
        );
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic() {
        let o = OptimiseOptions::default();
        let png = testutil::png(200, 200);
        let jpeg = testutil::jpeg(200, 200, 90, &testutil::JpegExtras::default());
        let webp = testutil::webp(200, 200, 90, &webp::Extras::default());
        let gif = testutil::gif(2);
        for (format, valid) in [
            (Format::Png, png),
            (Format::Jpeg, jpeg),
            (Format::WebP, webp),
            (Format::Gif, gif),
        ] {
            for broken in [
                valid[..valid.len() / 2].to_vec(),
                b"definitely not an image".to_vec(),
                Vec::new(),
            ] {
                assert!(
                    matches!(optimise(&broken, format, &o), Err(OptimiseError::Decode(_))),
                    "{format:?} with {} bytes",
                    broken.len()
                );
            }
        }
    }

    /// NFR-P2: a 5 MB clipboard image in under 1.5 s. Timing depends on the
    /// machine, so run it on purpose: `cargo test --release -- --ignored five_megabyte`.
    #[test]
    #[ignore = "timing; run with --release --ignored"]
    fn five_megabyte_png_is_optimised_within_the_clipboard_budget() {
        let input = testutil::png(4096, 2560);
        assert!(
            input.len() >= 4_500_000,
            "fixture is only {} bytes",
            input.len()
        );
        let start = std::time::Instant::now();
        let outcome = optimise(&input, Format::Png, &OptimiseOptions::default()).unwrap();
        let took = start.elapsed();
        println!("5 MB PNG optimised in {took:?}");
        assert!(matches!(outcome, Outcome::Smaller { .. }));
        assert!(took.as_millis() < 1500, "took {took:?}");
    }

    #[test]
    fn encode_returns_the_output_even_when_it_is_larger() {
        let tiny = testutil::png(1, 1);
        let out = encode(&tiny, Format::Png, &OptimiseOptions::default()).unwrap();
        assert!(out.len() > tiny.len());
        assert_eq!(crate::sniff::sniff(&out), Some(Format::Png));
    }
}
