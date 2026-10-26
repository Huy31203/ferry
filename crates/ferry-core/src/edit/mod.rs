//! What a card asks for after the fact (spec §4.2): a recipe that every run
//! applies to the backed-up original, so edits never compound or lose quality.

pub mod fit;
pub mod geometry;
pub mod raster;
pub mod smart;

use serde::{Deserialize, Deserializer, Serialize};

use crate::audio::{CoverPolicy, Loudness};
use crate::i18n::tf;
use crate::optimise::{self, Encoder, Level, OptimiseError, OptimiseOptions, gif};
use crate::sniff::{self, Format};
use geometry::Rect;
use raster::Raster;

/// The largest width, height or long edge a card may ask for.
pub const MAX_SIDE: u32 = 30_000;

// sE pdf-formats
/// The range of a recipe's `dpi`.
pub const MIN_DPI: u16 = 36;
pub const MAX_DPI: u16 = 1200;

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
    /// Width/Height/LongEdge and `scale` replace each other; an Exact size keeps a later scale.
    pub size: Option<Size>,
    /// 10..=90 percent of the cropped original, or of an Exact size.
    pub scale: Option<u8>,
    /// `None` keeps the source format.
    pub format: Option<Format>,
    /// 5..=100; `None` uses the settings. Mutually exclusive with `fit_under`.
    pub compression: Option<u8>,
    /// Bytes.
    pub fit_under: Option<u64>,
    pub level: Level,
    // stream F: pipelines
    /// `None`: the settings' encoder.
    pub encoder: Option<Encoder>,
    // stream A: video
    pub speed: Option<Speed>,
    pub mute: bool,
    // ---- stream B: audio ----
    /// kbps, lossy audio results only. Mutually exclusive with `fit_under`.
    pub bitrate: Option<u16>,
    /// `None`: the settings' target.
    pub loudness: Option<Loudness>,
    /// `None`: the settings' policy. `size` and `scale` downscale the cover.
    pub cover: Option<CoverPolicy>,
}

// stream A: video
/// Playback speed: `percent` is 50..=400; 100 means no change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speed {
    pub percent: u16,
    pub keep_frames: bool,
}

impl Edits {
    /// Nothing but a level: Cycle 1's optimise rules apply. An encoder choice
    /// does not count: a plain run with an encoder still follows never-worse.
    pub fn is_plain(&self) -> bool {
        !self.reshapes()
            && self.format.is_none()
            && self.compression.is_none()
            && self.fit_under.is_none()
            && self.speed.is_none()
            && !self.mute
            // stream B: audio
            && self.bitrate.is_none()
            && self.loudness.is_none()
            && self.cover.is_none()
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
                // stream B: audio (downscaling a cover brings it back)
                if self.cover == Some(CoverPolicy::Remove) {
                    self.cover = None;
                }
            }
        }
        if let Some(scale) = patch.scale {
            self.scale = scale.map(|s| s.clamp(10, 90));
            // An Exact size is a crop, which a scale applies on top of.
            if self.scale.is_some() && !matches!(self.size, Some(Size::Exact { .. })) {
                self.size = None;
            }
            // stream B: audio
            if self.scale.is_some() && self.cover == Some(CoverPolicy::Remove) {
                self.cover = None;
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
                self.bitrate = None; // stream B: audio
            }
        }
        if let Some(level) = patch.level {
            self.level = level;
        }
        // stream F: pipelines
        if let Some(encoder) = patch.encoder {
            self.encoder = encoder;
        }
        // stream A: video
        if let Some(speed) = patch.speed {
            self.speed = speed
                .map(|s| Speed {
                    percent: s.percent.clamp(50, 400),
                    ..s
                })
                .filter(|s| s.percent != 100);
        }
        if let Some(mute) = patch.mute {
            self.mute = mute;
        }
        // ---- stream B: audio ----
        if let Some(bitrate) = patch.bitrate {
            self.bitrate = bitrate.map(|b| b.clamp(16, 320));
            if self.bitrate.is_some() {
                self.fit_under = None;
            }
        }
        if let Some(loudness) = patch.loudness {
            self.loudness = loudness;
        }
        if let Some(cover) = patch.cover {
            self.cover = cover;
            if self.cover.is_some() {
                self.size = None;
                self.scale = None;
            }
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
    // stream F: pipelines
    #[serde(deserialize_with = "present")]
    pub encoder: Option<Option<Encoder>>,
    // stream A: video
    #[serde(deserialize_with = "present")]
    pub speed: Option<Option<Speed>>,
    pub mute: Option<bool>,
    // ---- stream B: audio ----
    #[serde(deserialize_with = "present")]
    pub bitrate: Option<Option<u16>>,
    #[serde(deserialize_with = "present")]
    pub loudness: Option<Option<Loudness>>,
    #[serde(deserialize_with = "present")]
    pub cover: Option<Option<CoverPolicy>>,
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

/// What `apply` made: the bytes, their format and size, and whether a
/// fit-under-size target was out of reach.
#[derive(Debug)]
pub struct Applied {
    pub bytes: Vec<u8>,
    pub format: Format,
    pub dims: (u32, u32),
    pub missed_target: bool,
}

/// Run the whole recipe on the original (spec §4.2). `o` carries the
/// settings; the recipe's level and compression override them.
pub fn apply(
    original: &[u8],
    source: Format,
    edits: &Edits,
    o: &OptimiseOptions,
) -> Result<Applied, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    if source != Format::Gif && sniff::is_animated(original, source) {
        return Err(OptimiseError::Unsupported(tf(
            "error.animated_edit",
            &[("format", source.label())],
        )));
    }
    let target = match edits.format {
        Some(format) if format != Format::Gif => format,
        _ => source,
    };
    let (work, dims) = prepare(original, source, target, edits)?;
    let mut base = o.clone();
    base.level = edits.level;
    if let Some(compression) = edits.compression {
        base.compression = compression;
    }
    let (bytes, missed_target) = match edits.fit_under {
        Some(limit) => {
            let fitted = fit::search(limit, |q| {
                work.run(&OptimiseOptions {
                    compression: q,
                    level: Level::Normal,
                    ..base.clone()
                })
            })?;
            (fitted.bytes, fitted.missed)
        }
        None => (work.run(&base)?, false),
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    Ok(Applied {
        bytes,
        format: target,
        dims,
        missed_target,
    })
}

/// The encoder a recipe needs, prepared once so fit-under-size can run it
/// several times.
enum Work<'a> {
    /// GIF to GIF, animated or not: gifsicle crops and resizes every frame.
    Gifsicle {
        input: &'a [u8],
        crop: Option<Rect>,
        size: Option<(u32, u32)>,
    },
    /// Same format, same pixels: the format's optimiser at a new quality, so
    /// Cycle 1's metadata rules stay exactly as they were.
    Encoder { input: &'a [u8], format: Format },
    /// Everything else: decoded, cut and re-encoded.
    Pixels { raster: Raster, format: Format },
}

impl Work<'_> {
    fn run(&self, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
        match self {
            Work::Gifsicle { input, crop, size } => gif::edit(input, *crop, *size, o),
            Work::Encoder { input, format } => optimise::encode(input, *format, o),
            Work::Pixels { raster, format } => raster::encode(raster, *format, o),
        }
    }
}

fn prepare<'a>(
    original: &'a [u8],
    source: Format,
    target: Format,
    edits: &Edits,
) -> Result<(Work<'a>, (u32, u32)), OptimiseError> {
    if source == Format::Gif && target == Format::Gif {
        let full = sniff::dimensions(original, Format::Gif)
            .ok_or_else(|| OptimiseError::Decode("the GIF is incomplete".into()))?;
        let window = geometry::crop_size(full, edits);
        // Smart framing judges the first frame.
        let first = (edits.framing == Framing::Smart && window != full)
            .then(|| raster::decode(original, Format::Gif))
            .transpose()?;
        let crop = place(full, window, edits.framing, first.as_ref());
        let out = geometry::output_size(window, edits);
        let work = Work::Gifsicle {
            input: original,
            crop: (window != full).then_some(crop),
            size: (out != window).then_some(out),
        };
        return Ok((work, out));
    }
    if !edits.reshapes() && target == source {
        let dims = raster::display_dims(original, source).unwrap_or_default();
        return Ok((
            Work::Encoder {
                input: original,
                format: source,
            },
            dims,
        ));
    }
    let decoded = raster::decode(original, source)?;
    let full = (decoded.width, decoded.height);
    let window = geometry::crop_size(full, edits);
    let crop = place(full, window, edits.framing, Some(&decoded));
    let out = geometry::output_size(window, edits);
    let shaped = if window == full && out == full {
        decoded
    } else {
        raster::cut(&decoded, crop, out)?
    };
    Ok((
        Work::Pixels {
            raster: shaped,
            format: target,
        },
        out,
    ))
}

/// Where the window sits: the busiest spot for Smart (when pixels are at
/// hand), else the centre.
fn place(full: (u32, u32), window: (u32, u32), framing: Framing, pixels: Option<&Raster>) -> Rect {
    match (framing, pixels) {
        (Framing::Smart, Some(r)) if window != full => {
            smart::frame(&r.rgba, r.width, r.height, window)
        }
        _ => geometry::centre(full, window),
    }
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

    // ---- stream B: audio ----
    #[test]
    fn audio_fields_merge_and_exclude_each_other() {
        let mut e = Edits::default();
        e.merge(patch(json!({ "bitrate": 500 })));
        assert_eq!(e.bitrate, Some(320));
        e.merge(patch(json!({ "fit_under": 1_000_000 })));
        assert_eq!((e.bitrate, e.fit_under), (None, Some(1_000_000)));
        e.merge(patch(json!({ "bitrate": 128 })));
        assert_eq!((e.bitrate, e.fit_under), (Some(128), None));
        e.merge(patch(json!({ "scale": 50 })));
        e.merge(patch(json!({ "cover": "remove" })));
        assert_eq!((e.cover, e.scale), (Some(CoverPolicy::Remove), None));
        e.merge(patch(
            json!({ "size": { "kind": "exact", "width": 600, "height": 600 } }),
        ));
        assert_eq!(e.cover, None);
        e.merge(patch(json!({ "loudness": "podcast" })));
        assert_eq!(e.loudness, Some(Loudness::Podcast));
        assert!(!e.is_plain());
        assert!(
            !Edits {
                loudness: Some(Loudness::Off),
                ..Edits::default()
            }
            .is_plain()
        );
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
    fn a_downscale_keeps_an_exact_crop() {
        let mut edits = Edits::default();
        edits.merge(patch(
            json!({ "size": { "kind": "exact", "width": 1200, "height": 628 } }),
        ));
        edits.merge(patch(json!({ "scale": 50 })));
        assert_eq!(
            (edits.size, edits.scale),
            (
                Some(Size::Exact {
                    width: 1200,
                    height: 628
                }),
                Some(50)
            )
        );
        edits.merge(patch(json!({ "size": { "kind": "width", "width": 800 } })));
        assert_eq!(
            (edits.size, edits.scale),
            (Some(Size::Width { width: 800 }), None)
        );
    }

    #[test]
    fn an_exact_crop_then_downscale_comes_out_smaller() {
        let a = applied(&testutil::png(300, 200), Format::Png, |e| {
            e.size = Some(Size::Exact {
                width: 120,
                height: 60,
            });
            e.scale = Some(50);
        });
        assert_eq!(a.dims, (60, 30));
        assert_eq!(dimensions(&a.bytes, Format::Png), Some((60, 30)));
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
                "level": "normal",
                "encoder": null,
                "speed": null,
                "mute": false,
                "bitrate": null,
                "loudness": null,
                "cover": null
            })
        );
    }

    use crate::optimise::OptimiseOptions;
    use crate::sniff::{dimensions, gif_frame_count, sniff};
    use crate::testutil;

    fn applied(bytes: &[u8], source: Format, f: impl FnOnce(&mut Edits)) -> Applied {
        let mut edits = Edits::default();
        f(&mut edits);
        apply(bytes, source, &edits, &OptimiseOptions::default()).unwrap()
    }

    #[test]
    fn a_downscale_halves_the_picture() {
        let a = applied(&testutil::png(300, 200), Format::Png, |e| {
            e.scale = Some(50)
        });
        assert_eq!((a.format, a.dims), (Format::Png, (150, 100)));
        assert_eq!(dimensions(&a.bytes, Format::Png), Some((150, 100)));
    }

    #[test]
    fn crop_scale_and_convert_run_in_one_pass() {
        let a = applied(&testutil::png(300, 200), Format::Png, |e| {
            e.aspect = Some((1, 1));
            e.scale = Some(50);
            e.format = Some(Format::WebP);
        });
        assert_eq!((a.format, a.dims), (Format::WebP, (100, 100)));
        assert_eq!(sniff(&a.bytes), Some(Format::WebP));
        assert_eq!(dimensions(&a.bytes, Format::WebP), Some((100, 100)));
    }

    #[test]
    fn a_lower_compression_keeps_the_format_and_size_and_shrinks_the_file() {
        let jpeg = testutil::jpeg(300, 200, 95, &testutil::JpegExtras::default());
        let high = applied(&jpeg, Format::Jpeg, |e| e.compression = Some(95));
        let low = applied(&jpeg, Format::Jpeg, |e| e.compression = Some(30));
        assert_eq!((low.format, low.dims), (Format::Jpeg, (300, 200)));
        assert!(low.bytes.len() < high.bytes.len());
    }

    #[test]
    fn fit_under_lands_under_the_target_or_says_it_missed() {
        let jpeg = testutil::jpeg(400, 300, 95, &testutil::JpegExtras::default());
        let full = applied(&jpeg, Format::Jpeg, |e| e.compression = Some(100));
        let target = full.bytes.len() as u64 / 2;
        let fitted = applied(&jpeg, Format::Jpeg, |e| e.fit_under = Some(target));
        assert!(fitted.bytes.len() as u64 <= target && !fitted.missed_target);
        let missed = applied(&jpeg, Format::Jpeg, |e| e.fit_under = Some(100));
        assert!(missed.missed_target);
    }

    #[test]
    fn a_gif_converts_to_a_still_from_its_first_frame() {
        let a = applied(&testutil::gif(3), Format::Gif, |e| {
            e.format = Some(Format::Png)
        });
        assert_eq!((a.format, a.dims), (Format::Png, (64, 48)));
        assert_eq!(sniff(&a.bytes), Some(Format::Png));
    }

    #[test]
    fn asking_for_gif_output_keeps_the_source_format() {
        let a = applied(&testutil::png(60, 40), Format::Png, |e| {
            e.format = Some(Format::Gif)
        });
        assert_eq!(a.format, Format::Png);
    }

    #[test]
    fn an_animated_webp_cannot_be_edited() {
        let edits = Edits {
            scale: Some(50),
            ..Edits::default()
        };
        let result = apply(
            &testutil::animated_webp(),
            Format::WebP,
            &edits,
            &OptimiseOptions::default(),
        );
        assert!(
            matches!(result, Err(OptimiseError::Unsupported(m)) if m == "Animated WebP can't be edited yet")
        );
    }

    #[test]
    fn an_animated_gif_is_downscaled_by_gifsicle_and_stays_animated() {
        let Some(gifsicle) = testutil::gifsicle() else {
            return;
        };
        let o = OptimiseOptions {
            gifsicle: Some(gifsicle),
            ..OptimiseOptions::default()
        };
        let edits = Edits {
            scale: Some(50),
            ..Edits::default()
        };
        let a = apply(&testutil::gif(3), Format::Gif, &edits, &o).unwrap();
        assert_eq!((a.format, a.dims), (Format::Gif, (32, 24)));
        assert_eq!(gif_frame_count(&a.bytes), 3);
        assert_eq!(dimensions(&a.bytes, Format::Gif), Some((32, 24)));
    }

    #[test]
    fn a_cancelled_token_stops_an_edit() {
        let o = OptimiseOptions::default();
        o.cancel.cancel();
        let edits = Edits {
            scale: Some(50),
            ..Edits::default()
        };
        assert!(matches!(
            apply(&testutil::png(60, 40), Format::Png, &edits, &o),
            Err(OptimiseError::Cancelled)
        ));
    }

    // stream A: video
    #[test]
    fn speed_is_clamped_and_normal_speed_is_none() {
        let mut e = Edits::default();
        let speed = |percent, keep_frames| EditsPatch {
            speed: Some(Some(Speed {
                percent,
                keep_frames,
            })),
            ..Default::default()
        };
        e.merge(speed(1000, true));
        assert_eq!(
            e.speed,
            Some(Speed {
                percent: 400,
                keep_frames: true
            })
        );
        e.merge(speed(100, false));
        assert_eq!(e.speed, None);
        e.merge(speed(10, false));
        assert_eq!(e.speed.map(|s| s.percent), Some(50));
    }

    #[test]
    fn speed_and_mute_are_explicit_edits_but_an_encoder_is_not() {
        for patch in [
            EditsPatch {
                speed: Some(Some(Speed {
                    percent: 200,
                    keep_frames: true,
                })),
                ..Default::default()
            },
            EditsPatch {
                mute: Some(true),
                ..Default::default()
            },
        ] {
            let mut e = Edits::default();
            e.merge(patch);
            assert!(!e.is_plain());
        }
        let mut e = Edits::default();
        e.merge(EditsPatch {
            encoder: Some(Some(Encoder::Small)),
            ..Default::default()
        });
        assert_eq!(e.encoder, Some(Encoder::Small));
        assert!(e.is_plain());
    }

    #[test]
    fn the_new_fields_read_from_card_json() {
        let p: EditsPatch = serde_json::from_str(
            r#"{"speed":{"percent":150,"keep_frames":false},"mute":true,"encoder":null}"#,
        )
        .unwrap();
        assert_eq!(
            p.speed,
            Some(Some(Speed {
                percent: 150,
                keep_frames: false
            }))
        );
        assert_eq!((p.mute, p.encoder), (Some(true), Some(None)));
        let old: Edits = serde_json::from_str(r#"{"scale":50}"#).unwrap(); // a Cycle 2 recipe
        assert_eq!((old.speed, old.mute, old.encoder), (None, false, None));
    }
}
