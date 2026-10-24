//! Audio: formats, loudness targets, cover art and the encode plan. Pure logic;
//! ffmpeg runs behind `tool::AudioTool`.

use serde::{Deserialize, Serialize};

use crate::sniff::Format;

pub mod loudness;
pub mod probe;
pub mod tool;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Loudness {
    #[default]
    Off,
    Streaming,
    AppleMusic,
    Podcast,
    Broadcast,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverPolicy {
    Keep,
    #[default]
    Optimise,
    Downscale,
    Remove,
}

/// What an audio card shows besides what every card shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioCard {
    pub bitrate_before: Option<u16>,
    pub bitrate_after: Option<u16>,
    pub lossless: bool,
    pub has_cover: bool,
}

/// What an audio file can become (Settings).
pub const TARGETS: [Format; 5] = [
    Format::Mp3,
    Format::M4a,
    Format::Opus,
    Format::Wav,
    Format::Flac,
];
/// What the card's format bar offers.
pub const BAR: [Format; 4] = [Format::Mp3, Format::M4a, Format::Opus, Format::Wav];

/// MP3, M4A and FLAC hold a picture; Opus and WAV cannot.
pub fn supports_cover(f: Format) -> bool {
    matches!(f, Format::Mp3 | Format::M4a | Format::Flac)
}

pub fn is_lossless_format(f: Format) -> bool {
    matches!(f, Format::Wav | Format::Flac | Format::Aiff)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn targets_and_covers() {
        assert_eq!(
            serde_json::to_string(&Loudness::AppleMusic).unwrap(),
            "\"apple_music\""
        );
        assert_eq!(
            serde_json::to_string(&CoverPolicy::default()).unwrap(),
            "\"optimise\""
        );
        assert!(
            supports_cover(Format::Flac)
                && !supports_cover(Format::Opus)
                && !supports_cover(Format::Wav)
        );
        assert!(is_lossless_format(Format::Aiff) && !is_lossless_format(Format::M4a));
    }
}
