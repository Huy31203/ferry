//! Optimise one image from memory (spec §4.2). Never worse than the input:
//! the caller keeps the original unless the result is `Smaller`.

mod png;
mod jpeg;
pub(crate) mod webp;
mod gif;

use std::{fmt, path::PathBuf};

use crate::cancel::CancelToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    Aggressive,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn opts(level: Level, compression: u8) -> OptimiseOptions {
        OptimiseOptions {
            level,
            compression,
            ..OptimiseOptions::default()
        }
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
}
