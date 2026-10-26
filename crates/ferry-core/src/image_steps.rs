//! The image steps of a pipeline: `optimise`, `convert`, `downscale`, `crop`
//! and `strip_exif`. Each folds into the branch's `Edits` recipe, so a whole
//! pipeline is one encode.

use serde_json::json;

use crate::{
    edit::{EditsPatch, Framing, Size},
    i18n::{t, tf},
    optimise::{Encoder, Level},
    pipeline_steps::{
        Branch, Control, Effect, Media, Param, Params, Phase, Registry, StepKind, choice, flag,
        number, output_rule, text, with_location,
    },
    sniff::Format,
};

/// What `convert` can write.
pub const CONVERT_TARGETS: &[(&str, &str)] = &[
    ("png", "pipe.choice.png"),
    ("jpeg", "pipe.choice.jpeg"),
    ("webp", "pipe.choice.webp"),
];

/// Where an image step sends a result when it names the folder location without a folder.
const FOLDER: &str = "~/Pictures/Ferry";

const LEVELS: &[(&str, &str)] = &[
    ("normal", "pipe.choice.normal"),
    ("aggressive", "pipe.choice.aggressive"),
];
const ENCODERS: &[(&str, &str)] = &[
    ("auto", "pipe.choice.auto"),
    ("fast", "pipe.choice.fast"),
    ("small", "pipe.choice.small"),
    ("visually_lossless", "pipe.choice.visually_lossless"),
];
const SIDES: &[(&str, &str)] = &[
    ("percent", "pipe.choice.percent"),
    ("width", "pipe.choice.width"),
    ("height", "pipe.choice.height"),
    ("long_edge", "pipe.choice.long_edge"),
];
pub(crate) const ASPECTS: &[(&str, &str)] = &[
    ("free", "pipe.choice.free"),
    ("16:9", "pipe.choice.r16_9"),
    ("4:3", "pipe.choice.r4_3"),
    ("1:1", "pipe.choice.r1_1"),
    ("1.91:1", "pipe.choice.r1_91_1"),
    ("9:16", "pipe.choice.r9_16"),
];

const OPTIMISE_PARAMS: &[Param] = with_location!(
    Param {
        name: "compression",
        label: "pipe.param.compression",
        control: Control::Number {
            min: 5.0,
            max: 100.0,
            step: 5.0
        },
        required: false,
        shown_when: None,
    },
    Param {
        name: "adaptive",
        label: "pipe.param.adaptive",
        control: Control::Switch,
        required: false,
        shown_when: None,
    },
    Param {
        name: "level",
        label: "pipe.param.level",
        control: Control::Choice { options: LEVELS },
        required: false,
        shown_when: None,
    },
    Param {
        name: "encoder",
        label: "pipe.param.encoder",
        control: Control::Choice { options: ENCODERS },
        required: false,
        shown_when: None,
    }
);

const CONVERT_PARAMS: &[Param] = with_location!(Param {
    name: "to",
    label: "pipe.param.convert_to",
    control: Control::Choice {
        options: CONVERT_TARGETS
    },
    required: true,
    shown_when: None,
});

const DOWNSCALE_PARAMS: &[Param] = &[
    Param {
        name: "by",
        label: "pipe.param.by",
        control: Control::Choice { options: SIDES },
        required: false,
        shown_when: None,
    },
    Param {
        name: "value",
        label: "pipe.param.value",
        control: Control::Number {
            min: 1.0,
            max: 30000.0,
            step: 1.0,
        },
        required: true,
        shown_when: None,
    },
];

const CROP_PARAMS: &[Param] = &[
    Param {
        name: "aspect",
        label: "pipe.param.aspect",
        control: Control::Choice { options: ASPECTS },
        required: false,
        shown_when: None,
    },
    Param {
        name: "width",
        label: "pipe.param.width",
        control: Control::Number {
            min: 0.0,
            max: 30000.0,
            step: 1.0,
        },
        required: false,
        shown_when: None,
    },
    Param {
        name: "height",
        label: "pipe.param.height",
        control: Control::Number {
            min: 0.0,
            max: 30000.0,
            step: 1.0,
        },
        required: false,
        shown_when: None,
    },
    Param {
        name: "long_edge",
        label: "pipe.param.long_edge",
        control: Control::Switch,
        required: false,
        shown_when: None,
    },
    Param {
        name: "smart",
        label: "pipe.param.smart",
        control: Control::Switch,
        required: false,
        shown_when: None,
    },
];

fn obj(v: serde_json::Value) -> Params {
    v.as_object().cloned().unwrap_or_default()
}

// ---- optimise ----

fn optimise_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let mut patch = EditsPatch::default();
    if let Some(level) = choice(p, "level", "pipe.param.level", LEVELS)? {
        patch.level = Some(if level == "aggressive" {
            Level::Aggressive
        } else {
            Level::Normal
        });
    }
    if let Some(encoder) = choice(p, "encoder", "pipe.param.encoder", ENCODERS)? {
        patch.encoder = Some(match encoder {
            "fast" => Some(Encoder::Fast),
            "small" => Some(Encoder::Small),
            "visually_lossless" => Some(Encoder::VisuallyLossless),
            _ => None,
        });
    }
    let quality = number(p, "compression", "pipe.param.compression", 5.0, 100.0)?;
    if flag(p, "adaptive") {
        patch.adaptive = Some(true);
    } else if let Some(q) = quality {
        patch.compression = Some(Some(q.round() as u8));
    }
    let rule = output_rule(p, FOLDER, "%f_optimised.%e")?;
    b.recipe.merge(patch);
    if rule.is_some() {
        b.optimised = rule;
    }
    Ok(())
}

fn optimise_summary(p: &Params) -> String {
    let mut caption = t("pipe.caption.optimise");
    let mut add = |part: String| {
        caption.push_str(" · ");
        caption.push_str(&part);
    };
    if flag(p, "adaptive") {
        add(t("pipe.caption.adaptive"));
    } else if let Some(q) = p.get("compression").and_then(|v| v.as_f64()) {
        add(tf(
            "pipe.caption.quality",
            &[("q", &(q.round() as u32).to_string())],
        ));
    }
    if text(p, "level") == Some("aggressive") {
        add(t("pipe.caption.aggressive"));
    }
    caption
}

// ---- convert ----

fn target_format(to: &str) -> Format {
    match to {
        "png" => Format::Png,
        "jpeg" => Format::Jpeg,
        _ => Format::WebP,
    }
}

fn convert_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let to = choice(p, "to", "pipe.param.convert_to", CONVERT_TARGETS)?.ok_or_else(|| {
        tf(
            "pipe.error.required",
            &[("name", &t("pipe.param.convert_to"))],
        )
    })?;
    let rule = output_rule(p, FOLDER, "%f.%e")?;
    b.recipe.merge(EditsPatch {
        format: Some(Some(target_format(to))),
        ..EditsPatch::default()
    });
    if rule.is_some() {
        b.converted = rule;
    }
    Ok(())
}

fn convert_summary(p: &Params) -> String {
    let label = text(p, "to").map_or("", |to| target_format(to).label());
    tf("pipe.caption.convert", &[("format", label)])
}

// ---- downscale ----

fn downscale_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let by = choice(p, "by", "pipe.param.by", SIDES)?.unwrap_or("percent");
    let (min, max) = if by == "percent" {
        (10.0, 90.0)
    } else {
        (1.0, 30000.0)
    };
    let value = number(p, "value", "pipe.param.value", min, max)?
        .ok_or_else(|| tf("pipe.error.required", &[("name", &t("pipe.param.value"))]))?;
    let n = value.round() as u32;
    let patch = match by {
        "percent" => EditsPatch {
            scale: Some(Some(n as u8)),
            ..EditsPatch::default()
        },
        "width" => size_patch(Size::Width { width: n }),
        "height" => size_patch(Size::Height { height: n }),
        _ => size_patch(Size::LongEdge { edge: n }),
    };
    b.recipe.merge(patch);
    Ok(())
}

fn size_patch(size: Size) -> EditsPatch {
    EditsPatch {
        size: Some(Some(size)),
        ..EditsPatch::default()
    }
}

fn downscale_summary(p: &Params) -> String {
    let n = p
        .get("value")
        .and_then(|v| v.as_f64())
        .map_or_else(String::new, |v| (v.round() as u32).to_string());
    let key = match text(p, "by").unwrap_or("percent") {
        "width" => "pipe.caption.width",
        "height" => "pipe.caption.height",
        "long_edge" => "pipe.caption.long_edge",
        _ => "pipe.caption.scale",
    };
    tf(key, &[("n", &n)])
}

// ---- crop ----

pub(crate) fn ratio(aspect: &str) -> Option<(u32, u32)> {
    match aspect {
        "16:9" => Some((16, 9)),
        "4:3" => Some((4, 3)),
        "1:1" => Some((1, 1)),
        "1.91:1" => Some((191, 100)),
        "9:16" => Some((9, 16)),
        _ => None,
    }
}

/// The width and height asked for, 0 when absent.
fn sides(p: &Params) -> Result<(u32, u32), String> {
    let side = |name, label| {
        number(p, name, label, 0.0, 30000.0).map(|n| n.map_or(0, |n| n.round() as u32))
    };
    Ok((
        side("width", "pipe.param.width")?,
        side("height", "pipe.param.height")?,
    ))
}

fn crop_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let aspect = choice(p, "aspect", "pipe.param.aspect", ASPECTS)?.unwrap_or("16:9");
    let (width, height) = sides(p)?;
    let mut patch = EditsPatch {
        aspect: Some(ratio(aspect)),
        framing: Some(if flag(p, "smart") {
            Framing::Smart
        } else {
            Framing::Centre
        }),
        ..EditsPatch::default()
    };
    if flag(p, "long_edge") && width > 0 {
        patch.size = Some(Some(Size::LongEdge { edge: width }));
    } else if width > 0 || height > 0 {
        patch.size = Some(Some(Size::Exact { width, height }));
    } else if aspect == "free" {
        return Err(t("pipe.error.crop_empty"));
    }
    b.recipe.merge(patch);
    Ok(())
}

fn crop_summary(p: &Params) -> String {
    let (width, height) = sides(p).unwrap_or_default();
    let mut caption = if flag(p, "long_edge") && width > 0 {
        tf("pipe.caption.long_edge_crop", &[("n", &width.to_string())])
    } else if width > 0 || height > 0 {
        let side = |n: u32| {
            if n == 0 {
                t("pipe.caption.auto")
            } else {
                n.to_string()
            }
        };
        tf(
            "pipe.caption.crop_size",
            &[("w", &side(width)), ("h", &side(height))],
        )
    } else {
        let label = ASPECTS
            .iter()
            .find(|(v, _)| Some(*v) == text(p, "aspect"))
            .map_or("16:9".to_owned(), |(_, label)| t(label));
        tf("pipe.caption.crop", &[("ratio", &label)])
    };
    if flag(p, "smart") {
        caption.push_str(" · ");
        caption.push_str(&t("pipe.caption.smart"));
    }
    caption
}

// ---- strip_exif ----

fn strip_fold(_: &Params, b: &mut Branch) -> Result<(), String> {
    b.recipe.merge(EditsPatch {
        strip_metadata: Some(Some(true)),
        ..EditsPatch::default()
    });
    Ok(())
}

/// Shared with the other media that have pixels to cut.
pub(crate) fn downscale_kind(media: Media) -> StepKind {
    StepKind {
        id: "downscale",
        media,
        label: "pipe.step.downscale",
        phase: Phase::Transform,
        params: DOWNSCALE_PARAMS,
        defaults: || obj(json!({ "by": "percent", "value": 50 })),
        summary: downscale_summary,
        effect: Effect::Fold(downscale_fold),
    }
}

/// Shared with the other media that have pixels to cut.
pub(crate) fn crop_kind(media: Media) -> StepKind {
    StepKind {
        id: "crop",
        media,
        label: "pipe.step.crop",
        phase: Phase::Transform,
        params: CROP_PARAMS,
        defaults: || {
            obj(
                json!({ "aspect": "16:9", "width": 0, "height": 0, "long_edge": false, "smart": true }),
            )
        },
        summary: crop_summary,
        effect: Effect::Fold(crop_fold),
    }
}

pub fn register(r: &mut Registry) {
    let kinds = [
        StepKind {
            id: "optimise",
            media: Media::Image,
            label: "pipe.step.optimise",
            phase: Phase::Transform,
            params: OPTIMISE_PARAMS,
            defaults: || {
                obj(
                    json!({ "adaptive": false, "level": "normal", "encoder": "auto", "location": "keep" }),
                )
            },
            summary: optimise_summary,
            effect: Effect::Fold(optimise_fold),
        },
        StepKind {
            id: "convert",
            media: Media::Image,
            label: "pipe.step.convert",
            phase: Phase::Transform,
            params: CONVERT_PARAMS,
            defaults: || obj(json!({ "to": "webp", "location": "keep" })),
            summary: convert_summary,
            effect: Effect::Fold(convert_fold),
        },
        downscale_kind(Media::Image),
        crop_kind(Media::Image),
        StepKind {
            id: "strip_exif",
            media: Media::Image,
            label: "pipe.step.strip_exif",
            phase: Phase::Transform,
            params: &[],
            defaults: Params::new,
            summary: |_| t("pipe.caption.strip"),
            effect: Effect::Fold(strip_fold),
        },
    ];
    for kind in kinds {
        r.register(kind).expect("image steps are registered once");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{edit::Size, pipeline_steps::Media, settings::Location};

    fn fold(id: &str, v: serde_json::Value) -> Result<Branch, String> {
        let mut r = Registry::new();
        register(&mut r);
        let Effect::Fold(f) = r.get(Media::Image, id).unwrap().effect else {
            panic!()
        };
        let mut b = Branch::default();
        f(v.as_object().unwrap(), &mut b).map(|()| b)
    }

    fn caption(id: &str, v: serde_json::Value) -> String {
        let mut r = Registry::new();
        register(&mut r);
        (r.get(Media::Image, id).unwrap().summary)(v.as_object().unwrap())
    }

    #[test]
    fn optimise_sets_quality_level_encoder_and_location() {
        let b = fold(
            "optimise",
            json!({ "compression": 60, "level": "aggressive", "encoder": "small", "location": "folder", "folder": "~/Out" }),
        )
        .unwrap();
        assert_eq!(
            (b.recipe.compression, b.recipe.level, b.recipe.encoder),
            (Some(60), Level::Aggressive, Some(Encoder::Small))
        );
        let rule = b.optimised.unwrap();
        assert_eq!(
            (rule.location, rule.folder.as_str(), rule.template.as_str()),
            (Location::Folder, "~/Out", "%f_optimised.%e")
        );
        let adaptive = fold("optimise", json!({ "adaptive": true, "compression": 60 })).unwrap();
        assert_eq!(
            (adaptive.recipe.adaptive, adaptive.recipe.compression),
            (true, None)
        );
        assert!(fold("optimise", json!({})).unwrap().optimised.is_none());
        assert!(
            fold("optimise", json!({ "compression": 3 }))
                .unwrap_err()
                .contains("between 5 and 100")
        );
    }

    #[test]
    fn convert_targets_png_jpeg_or_webp() {
        assert_eq!(
            fold("convert", json!({ "to": "webp" }))
                .unwrap()
                .recipe
                .format,
            Some(Format::WebP)
        );
        assert!(fold("convert", json!({ "to": "gif" })).is_err());
        let b = fold(
            "convert",
            json!({ "to": "jpeg", "location": "same_folder" }),
        )
        .unwrap();
        assert_eq!(b.converted.unwrap().template, "%f.%e");
    }

    #[test]
    fn downscale_by_percent_or_side() {
        assert_eq!(
            fold("downscale", json!({ "by": "percent", "value": 50 }))
                .unwrap()
                .recipe
                .scale,
            Some(50)
        );
        assert_eq!(
            fold("downscale", json!({ "by": "width", "value": 1200 }))
                .unwrap()
                .recipe
                .size,
            Some(Size::Width { width: 1200 })
        );
        assert_eq!(
            fold("downscale", json!({ "by": "long_edge", "value": 1920 }))
                .unwrap()
                .recipe
                .size,
            Some(Size::LongEdge { edge: 1920 })
        );
        assert!(fold("downscale", json!({ "by": "percent", "value": 95 })).is_err());
    }

    #[test]
    fn crop_by_ratio_size_or_long_edge() {
        let b = fold("crop", json!({ "aspect": "16:9", "smart": true })).unwrap();
        assert_eq!(
            (b.recipe.aspect, b.recipe.framing),
            (Some((16, 9)), Framing::Smart)
        );
        assert_eq!(
            fold("crop", json!({ "aspect": "1.91:1" }))
                .unwrap()
                .recipe
                .aspect,
            Some((191, 100))
        );
        assert_eq!(
            fold(
                "crop",
                json!({ "aspect": "free", "width": 1080, "height": 1080 })
            )
            .unwrap()
            .recipe
            .size,
            Some(Size::Exact {
                width: 1080,
                height: 1080
            })
        );
        assert_eq!(
            fold(
                "crop",
                json!({ "aspect": "free", "width": 1920, "long_edge": true })
            )
            .unwrap()
            .recipe
            .size,
            Some(Size::LongEdge { edge: 1920 })
        );
        assert!(fold("crop", json!({ "aspect": "free" })).is_err());
    }

    #[test]
    fn strip_exif_overrides_the_setting() {
        assert_eq!(
            fold("strip_exif", json!({})).unwrap().recipe.strip_metadata,
            Some(true)
        );
    }

    #[test]
    fn captions_read_like_the_card() {
        assert_eq!(
            caption("convert", json!({ "to": "webp" })),
            "Convert to WebP"
        );
        assert_eq!(
            caption("downscale", json!({ "by": "percent", "value": 50 })),
            "Downscale to 50 %"
        );
        assert_eq!(
            caption("crop", json!({ "aspect": "16:9", "smart": true })),
            "Crop 16:9 · smart"
        );
        assert_eq!(
            caption(
                "crop",
                json!({ "aspect": "free", "width": 1080, "height": 1080 })
            ),
            "Crop to 1080×1080"
        );
        assert_eq!(
            caption(
                "optimise",
                json!({ "compression": 60, "level": "aggressive" })
            ),
            "Optimise · q 60 · aggressive"
        );
        assert_eq!(caption("optimise", json!({})), "Optimise");
        assert_eq!(
            caption("optimise", json!({ "adaptive": true })),
            "Optimise · adaptive"
        );
        assert_eq!(caption("strip_exif", json!({})), "Strip metadata");
    }
}
