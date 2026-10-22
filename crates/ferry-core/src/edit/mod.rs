//! What a card asks for after the fact (spec §4.2): a recipe that every run
//! applies to the backed-up original, so edits never compound or lose quality.

pub mod geometry;
pub mod smart;

use serde::{Deserialize, Deserializer, Serialize};

use crate::optimise::Level;
use crate::sniff::Format;

/// The largest width, height or long edge a card may ask for.
pub const MAX_SIDE: u32 = 30_000;

/// Where an aspect crop or an exact size cuts from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    #[default]
    Centre,
    Smart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Size {
    /// Cover-crops to `width:height` using the framing, then scales.
    Exact {
        width: u32,
        height: u32,
    },
    /// Height follows the ratio ("0 keeps ratio").
    Width {
        width: u32,
    },
    Height {
        height: u32,
    },
    LongEdge {
        edge: u32,
    },
}

impl Size {
    /// An `Exact` size with one zero side keeps the ratio; a size with no
    /// usable side, or one over `MAX_SIDE`, is no size at all.
    fn normalised(self) -> Option<Size> {
        let size = match self {
            Size::Exact {
                width: 0,
                height: 0,
            } => return None,
            Size::Exact { width, height: 0 } => Size::Width { width },
            Size::Exact { width: 0, height } => Size::Height { height },
            other => other,
        };
        let sides = match size {
            Size::Exact { width, height } => [width, height],
            Size::Width { width: side }
            | Size::Height { height: side }
            | Size::LongEdge { edge: side } => [side, side],
        };
        sides
            .iter()
            .all(|side| (1..=MAX_SIDE).contains(side))
            .then_some(size)
    }
}

/// Everything a card asked for. Steps always run in the same order (aspect
/// crop, size or scale, convert, compress), whatever order they were asked in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edits {
    /// Crop to this ratio, such as `(16, 9)`; `None` is "Free".
    pub aspect: Option<(u32, u32)>,
    pub framing: Framing,
    /// Mutually exclusive with `scale`.
    pub size: Option<Size>,
    /// 10..=90 percent of the cropped original.
    pub scale: Option<u8>,
    /// `None` keeps the source format.
    pub format: Option<Format>,
    /// 5..=100; `None` uses the settings. Mutually exclusive with `fit_under`.
    pub compression: Option<u8>,
    /// Bytes.
    pub fit_under: Option<u64>,
    pub level: Level,
}

impl Edits {
    /// Nothing but a level: Cycle 1's optimise rules apply.
    pub fn is_plain(&self) -> bool {
        !self.reshapes()
            && self.format.is_none()
            && self.compression.is_none()
            && self.fit_under.is_none()
    }

    /// Whether the pixels change size: the raster path.
    pub fn reshapes(&self) -> bool {
        self.aspect.is_some() || self.size.is_some() || self.scale.is_some()
    }

    pub fn merge(&mut self, patch: EditsPatch) {
        if let Some(aspect) = patch.aspect {
            self.aspect = aspect.filter(|&(w, h)| w > 0 && h > 0);
        }
        if let Some(framing) = patch.framing {
            self.framing = framing;
        }
        if let Some(size) = patch.size {
            self.size = size.and_then(Size::normalised);
            if self.size.is_some() {
                self.scale = None;
            }
        }
        if let Some(scale) = patch.scale {
            self.scale = scale.map(|s| s.clamp(10, 90));
            if self.scale.is_some() {
                self.size = None;
            }
        }
        if let Some(format) = patch.format {
            self.format = format;
        }
        if let Some(compression) = patch.compression {
            self.compression = compression.map(|c| c.clamp(5, 100));
            if self.compression.is_some() {
                self.fit_under = None;
            }
        }
        if let Some(fit_under) = patch.fit_under {
            self.fit_under = fit_under.filter(|&bytes| bytes > 0);
            if self.fit_under.is_some() {
                self.compression = None;
            }
        }
        if let Some(level) = patch.level {
            self.level = level;
        }
    }
}

/// What a card sends: an absent field is left alone, `null` clears it, a
/// value sets it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct EditsPatch {
    #[serde(deserialize_with = "present")]
    pub aspect: Option<Option<(u32, u32)>>,
    pub framing: Option<Framing>,
    #[serde(deserialize_with = "present")]
    pub size: Option<Option<Size>>,
    #[serde(deserialize_with = "present")]
    pub scale: Option<Option<u8>>,
    #[serde(deserialize_with = "present")]
    pub format: Option<Option<Format>>,
    #[serde(deserialize_with = "present")]
    pub compression: Option<Option<u8>>,
    #[serde(deserialize_with = "present")]
    pub fit_under: Option<Option<u64>>,
    pub level: Option<Level>,
}

/// A field that is in the JSON: `null` is `Some(None)`. Absent fields never
/// get here; `#[serde(default)]` leaves them `None`.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn patch(value: serde_json::Value) -> EditsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_absent_field_is_left_alone_and_null_clears_it() {
        let mut edits = Edits {
            aspect: Some((16, 9)),
            scale: Some(50),
            ..Edits::default()
        };
        edits.merge(patch(json!({ "aspect": null })));
        assert_eq!(edits.aspect, None);
        assert_eq!(edits.scale, Some(50));
    }

    #[test]
    fn size_and_scale_replace_each_other() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "scale": 50 })));
        edits.merge(patch(json!({ "size": { "kind": "width", "width": 1200 } })));
        assert_eq!(
            (edits.size, edits.scale),
            (Some(Size::Width { width: 1200 }), None)
        );
        edits.merge(patch(json!({ "scale": 40 })));
        assert_eq!((edits.size, edits.scale), (None, Some(40)));
    }

    #[test]
    fn compression_and_fit_under_replace_each_other() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "compression": 60 })));
        edits.merge(patch(json!({ "fit_under": 1_000_000 })));
        assert_eq!(
            (edits.compression, edits.fit_under),
            (None, Some(1_000_000))
        );
        edits.merge(patch(json!({ "compression": 30 })));
        assert_eq!((edits.compression, edits.fit_under), (Some(30), None));
    }

    #[test]
    fn a_zero_side_keeps_the_ratio_and_nonsense_is_dropped() {
        let mut edits = Edits::default();
        edits.merge(patch(
            json!({ "size": { "kind": "exact", "width": 1200, "height": 0 } }),
        ));
        assert_eq!(edits.size, Some(Size::Width { width: 1200 }));
        edits.merge(patch(
            json!({ "size": { "kind": "exact", "width": 0, "height": 900 } }),
        ));
        assert_eq!(edits.size, Some(Size::Height { height: 900 }));
        for nonsense in [
            json!({ "kind": "exact", "width": 0, "height": 0 }),
            json!({ "kind": "long_edge", "edge": 0 }),
            json!({ "kind": "width", "width": 40_000 }),
        ] {
            edits.merge(patch(json!({ "size": nonsense })));
            assert_eq!(edits.size, None);
        }
        edits.merge(patch(json!({ "aspect": [0, 9] })));
        assert_eq!(edits.aspect, None);
    }

    #[test]
    fn scale_and_compression_are_clamped() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "scale": 5, "compression": 0 })));
        assert_eq!((edits.scale, edits.compression), (Some(10), Some(5)));
        edits.merge(patch(json!({ "scale": 95, "compression": 200 })));
        assert_eq!((edits.scale, edits.compression), (Some(90), Some(100)));
    }

    #[test]
    fn only_a_level_is_still_plain() {
        assert!(Edits::default().is_plain());
        let aggressive = Edits {
            level: Level::Aggressive,
            framing: Framing::Smart,
            ..Edits::default()
        };
        assert!(aggressive.is_plain());
        let recompressed = Edits {
            compression: Some(50),
            ..Edits::default()
        };
        assert!(!recompressed.is_plain());
        assert!(!recompressed.reshapes());
        assert!(
            Edits {
                scale: Some(50),
                ..Edits::default()
            }
            .reshapes()
        );
    }

    #[test]
    fn the_card_sees_the_recipe_as_plain_json() {
        let edits = Edits {
            aspect: Some((16, 9)),
            size: Some(Size::Exact {
                width: 1200,
                height: 628,
            }),
            format: Some(Format::WebP),
            ..Edits::default()
        };
        assert_eq!(
            serde_json::to_value(&edits).unwrap(),
            json!({
                "aspect": [16, 9],
                "framing": "centre",
                "size": { "kind": "exact", "width": 1200, "height": 628 },
                "scale": null,
                "format": "webp",
                "compression": null,
                "fit_under": null,
                "level": "normal"
            })
        );
    }
}
