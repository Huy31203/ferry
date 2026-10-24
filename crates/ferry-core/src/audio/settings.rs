//! The `[audio]` section of `settings.json`.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use super::{CoverPolicy, Loudness, TARGETS};
use crate::{
    edit::Size,
    settings::{Output, repair_output},
    sniff::Format,
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Audio {
    pub outputs: Outputs,
    /// MP3 and M4A, 32 to 320.
    pub bitrate_kbps: u16,
    /// 16 to 256.
    pub opus_bitrate_kbps: u16,
    pub loudness: Loudness,
    pub cover: CoverPolicy,
    pub cover_size: CoverSize,
    pub ignored_extensions: Vec<String>,
    pub max_size_mb: u32,
    pub output: Output,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Audio {
    fn default() -> Self {
        Self {
            outputs: Outputs::default(),
            bitrate_kbps: 192,
            opus_bitrate_kbps: 128,
            loudness: Loudness::Off,
            cover: CoverPolicy::Optimise,
            cover_size: CoverSize::default(),
            ignored_extensions: Vec::new(),
            max_size_mb: 2000,
            output: Output::with_folder("~/Music/Ferry"),
            extra: Map::new(),
        }
    }
}

/// What each kind of audio file becomes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Outputs {
    pub wav: Format,
    pub aiff: Format,
    pub flac: Format,
    pub mp3: Format,
    pub m4a: Format,
    pub ogg: Format,
    pub opus: Format,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Outputs {
    fn default() -> Self {
        Self {
            wav: Format::Mp3,
            aiff: Format::M4a,
            flac: Format::M4a,
            mp3: Format::Mp3,
            m4a: Format::M4a,
            ogg: Format::Opus,
            opus: Format::Opus,
            extra: Map::new(),
        }
    }
}

impl Outputs {
    /// An image format is its own output.
    pub fn for_source(&self, source: Format) -> Format {
        match source {
            Format::Wav => self.wav,
            Format::Aiff => self.aiff,
            Format::Flac => self.flac,
            Format::Mp3 => self.mp3,
            Format::M4a => self.m4a,
            Format::Ogg => self.ogg,
            Format::Opus => self.opus,
            other => other,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverShape {
    Square,
    LongEdge,
}

/// What Downscale makes of a cover.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CoverSize {
    pub shape: CoverShape,
    /// 100 to 3000.
    pub side: u32,
}

impl Default for CoverSize {
    fn default() -> Self {
        Self {
            shape: CoverShape::Square,
            side: 1000,
        }
    }
}

impl CoverSize {
    pub fn as_size(self) -> Size {
        match self.shape {
            CoverShape::Square => Size::Exact {
                width: self.side,
                height: self.side,
            },
            CoverShape::LongEdge => Size::LongEdge { edge: self.side },
        }
    }
}

impl Audio {
    /// A hand-edited file must not turn the encoders or limits into nonsense.
    pub fn sanitise(&mut self) {
        self.bitrate_kbps = self.bitrate_kbps.clamp(32, 320);
        self.opus_bitrate_kbps = self.opus_bitrate_kbps.clamp(16, 256);
        self.cover_size.side = self.cover_size.side.clamp(100, 3000);
        self.max_size_mb = self.max_size_mb.max(1);
        let defaults = Outputs::default();
        let o = &mut self.outputs;
        for (format, default) in [
            (&mut o.wav, defaults.wav),
            (&mut o.aiff, defaults.aiff),
            (&mut o.flac, defaults.flac),
            (&mut o.mp3, defaults.mp3),
            (&mut o.m4a, defaults.m4a),
            (&mut o.ogg, defaults.ogg),
            (&mut o.opus, defaults.opus),
        ] {
            if !TARGETS.contains(format) {
                *format = default;
            }
        }
        self.ignored_extensions = self
            .ignored_extensions
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
        repair_output(&mut self.output, &Audio::default().output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Location, Settings};

    #[test]
    fn defaults_match_the_plan() {
        let a = Audio::default();
        let o = &a.outputs;
        assert_eq!(
            [o.wav, o.aiff, o.flac, o.mp3, o.m4a, o.ogg, o.opus],
            [
                Format::Mp3,
                Format::M4a,
                Format::M4a,
                Format::Mp3,
                Format::M4a,
                Format::Opus,
                Format::Opus
            ]
        );
        assert_eq!(
            (a.bitrate_kbps, a.opus_bitrate_kbps, a.loudness, a.cover),
            (192, 128, Loudness::Off, CoverPolicy::Optimise)
        );
        assert_eq!(
            a.cover_size,
            CoverSize {
                shape: CoverShape::Square,
                side: 1000
            }
        );
        assert_eq!((a.max_size_mb, a.ignored_extensions.len()), (2000, 0));
        assert_eq!(
            (
                a.output.optimised.folder.as_str(),
                a.output.optimised.template.as_str()
            ),
            ("~/Music/Ferry", "%f_optimised.%e")
        );
        assert_eq!(
            (
                a.output.converted.location,
                a.output.converted.template.as_str()
            ),
            (Location::InPlace, "%f.%e")
        );
        assert_eq!(Settings::default().version, 2);
    }

    #[test]
    fn a_file_without_an_audio_section_gets_the_defaults_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        std::fs::write(
            &path,
            r#"{"images":{"compression":60},"audio":{"bitrate_kbps":160,"future":1}}"#,
        )
        .unwrap();
        let (s, _) = Settings::load(&path);
        assert_eq!(
            (s.audio.bitrate_kbps, s.audio.opus_bitrate_kbps),
            (160, 128)
        );
        s.save(&path).unwrap();
        let saved: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved["audio"]["future"], 1);
        assert_eq!(saved["audio"]["outputs"]["wav"], "mp3");
    }

    #[test]
    fn nonsense_is_repaired() {
        let mut a = Audio {
            bitrate_kbps: 5,
            opus_bitrate_kbps: 999,
            max_size_mb: 0,
            ..Audio::default()
        };
        a.outputs.wav = Format::Png;
        a.outputs.flac = Format::Aiff;
        a.cover_size.side = 9;
        a.ignored_extensions = vec![" aif ".into(), "".into()];
        a.output.optimised.template = "%q".into();
        a.sanitise();
        assert_eq!(
            (
                a.bitrate_kbps,
                a.opus_bitrate_kbps,
                a.max_size_mb,
                a.cover_size.side
            ),
            (32, 256, 1, 100)
        );
        assert_eq!((a.outputs.wav, a.outputs.flac), (Format::Mp3, Format::M4a));
        assert_eq!(a.ignored_extensions, ["aif"]);
        assert_eq!(a.output.optimised.template, "%f_optimised.%e");
    }

    #[test]
    fn the_cover_size_becomes_an_edit_size() {
        assert_eq!(
            CoverSize {
                shape: CoverShape::Square,
                side: 600
            }
            .as_size(),
            Size::Exact {
                width: 600,
                height: 600
            }
        );
        assert_eq!(
            CoverSize {
                shape: CoverShape::LongEdge,
                side: 800
            }
            .as_size(),
            Size::LongEdge { edge: 800 }
        );
        assert_eq!(Outputs::default().for_source(Format::Ogg), Format::Opus);
        assert_eq!(Outputs::default().for_source(Format::Png), Format::Png);
    }
}
