//! The video steps of a pipeline (stream F, registered after stream A): they
//! fold into the same `Edits` recipe a video card edits.

use serde_json::json;

use crate::{
    edit::{EditsPatch, Speed},
    i18n::{t, tf},
    image_steps::{crop_kind, downscale_kind},
    optimise::Encoder,
    pipeline_steps::{
        Branch, Control, Effect, Media, Param, Params, Phase, Registry, StepKind, choice, number,
        output_rule, text, with_location,
    },
    sniff::Format,
};

/// What `convert` can write.
pub const CONVERT_TARGETS: &[(&str, &str)] = &[
    ("mp4", "pipe.choice.mp4"),
    ("hevc", "pipe.choice.hevc"),
    ("av1", "pipe.choice.av1"),
    ("webm", "pipe.choice.webm"),
    ("gif", "pipe.choice.gif"),
];

const ENCODERS: &[(&str, &str)] = &[
    ("auto", "pipe.choice.auto"),
    ("fast", "pipe.choice.fast"),
    ("small", "pipe.choice.small"),
    ("visually_lossless", "pipe.choice.visually_lossless"),
];

const FRAMES: &[(&str, &str)] = &[
    ("keep", "video.speed.keep_frames"),
    ("drop", "video.speed.drop_frames"),
];

/// The folder a video step names without one (the Video settings' own default).
fn folder() -> &'static str {
    if cfg!(windows) {
        "~/Videos/Ferry"
    } else {
        "~/Movies/Ferry"
    }
}

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

const SPEED_PARAMS: &[Param] = &[
    Param {
        name: "percent",
        label: "pipe.param.speed",
        control: Control::Number {
            min: 50.0,
            max: 400.0,
            step: 5.0,
        },
        required: true,
        shown_when: None,
    },
    Param {
        name: "frames",
        label: "pipe.param.frames",
        control: Control::Choice { options: FRAMES },
        required: false,
        shown_when: None,
    },
];

fn obj(v: serde_json::Value) -> Params {
    v.as_object().cloned().unwrap_or_default()
}

fn optimise_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let mut patch = EditsPatch::default();
    if let Some(q) = number(p, "compression", "pipe.param.compression", 5.0, 100.0)? {
        patch.compression = Some(Some(q.round() as u8));
    }
    if let Some(encoder) = choice(p, "encoder", "pipe.param.encoder", ENCODERS)? {
        patch.encoder = Some(match encoder {
            "fast" => Some(Encoder::Fast),
            "small" => Some(Encoder::Small),
            "visually_lossless" => Some(Encoder::VisuallyLossless),
            _ => None,
        });
    }
    let rule = output_rule(p, folder(), "%f_optimised.%e")?;
    b.recipe.merge(patch);
    if rule.is_some() {
        b.optimised = rule;
    }
    Ok(())
}

fn optimise_summary(p: &Params) -> String {
    let mut caption = t("pipe.caption.optimise");
    if let Some(q) = p.get("compression").and_then(|v| v.as_f64()) {
        caption.push_str(" · ");
        caption.push_str(&tf(
            "pipe.caption.quality",
            &[("q", &(q.round() as u32).to_string())],
        ));
    }
    caption
}

fn target_format(to: &str) -> Format {
    match to {
        "hevc" => Format::Hevc,
        "av1" => Format::Av1,
        "webm" => Format::WebM,
        "gif" => Format::Gif,
        _ => Format::Mp4,
    }
}

fn set_format(b: &mut Branch, format: Format) {
    b.recipe.merge(EditsPatch {
        format: Some(Some(format)),
        ..EditsPatch::default()
    });
}

fn convert_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let to = choice(p, "to", "pipe.param.convert_to", CONVERT_TARGETS)?.ok_or_else(|| {
        tf(
            "pipe.error.required",
            &[("name", &t("pipe.param.convert_to"))],
        )
    })?;
    let rule = output_rule(p, folder(), "%f.%e")?;
    set_format(b, target_format(to));
    if rule.is_some() {
        b.converted = rule;
    }
    Ok(())
}

fn convert_summary(p: &Params) -> String {
    let label = text(p, "to").map_or("", |to| target_format(to).label());
    tf("pipe.caption.convert", &[("format", label)])
}

fn to_gif_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let rule = output_rule(p, folder(), "%f.%e")?;
    set_format(b, Format::Gif);
    if rule.is_some() {
        b.converted = rule;
    }
    Ok(())
}

fn speed_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let percent = number(p, "percent", "pipe.param.speed", 50.0, 400.0)?
        .ok_or_else(|| tf("pipe.error.required", &[("name", &t("pipe.param.speed"))]))?;
    let frames = choice(p, "frames", "pipe.param.frames", FRAMES)?.unwrap_or("keep");
    b.recipe.merge(EditsPatch {
        speed: Some(Some(Speed {
            percent: percent.round() as u16,
            keep_frames: frames == "keep",
        })),
        ..EditsPatch::default()
    });
    Ok(())
}

fn speed_summary(p: &Params) -> String {
    let n = p
        .get("percent")
        .and_then(|v| v.as_f64())
        .map_or_else(String::new, |v| (v.round() as u32).to_string());
    tf("pipe.caption.speed", &[("n", &n)])
}

fn mute_fold(_: &Params, b: &mut Branch) -> Result<(), String> {
    b.recipe.merge(EditsPatch {
        mute: Some(true),
        ..EditsPatch::default()
    });
    Ok(())
}

pub fn register(r: &mut Registry) {
    let kinds = [
        StepKind {
            id: "optimise",
            media: Media::Video,
            label: "pipe.step.optimise",
            phase: Phase::Transform,
            params: OPTIMISE_PARAMS,
            defaults: || obj(json!({ "encoder": "auto", "location": "keep" })),
            summary: optimise_summary,
            effect: Effect::Fold(optimise_fold),
        },
        StepKind {
            id: "convert",
            media: Media::Video,
            label: "pipe.step.convert",
            phase: Phase::Transform,
            params: CONVERT_PARAMS,
            defaults: || obj(json!({ "to": "mp4", "location": "keep" })),
            summary: convert_summary,
            effect: Effect::Fold(convert_fold),
        },
        downscale_kind(Media::Video),
        crop_kind(Media::Video),
        StepKind {
            id: "change_speed",
            media: Media::Video,
            label: "pipe.step.change_speed",
            phase: Phase::Transform,
            params: SPEED_PARAMS,
            defaults: || obj(json!({ "percent": 150, "frames": "keep" })),
            summary: speed_summary,
            effect: Effect::Fold(speed_fold),
        },
        StepKind {
            id: "remove_audio",
            media: Media::Video,
            label: "pipe.step.remove_audio",
            phase: Phase::Transform,
            params: &[],
            defaults: Params::new,
            summary: |_| t("pipe.caption.remove_audio"),
            effect: Effect::Fold(mute_fold),
        },
        StepKind {
            id: "to_gif",
            media: Media::Video,
            label: "pipe.step.to_gif",
            phase: Phase::Transform,
            params: with_location!(),
            defaults: || obj(json!({ "location": "keep" })),
            summary: |_| tf("pipe.caption.convert", &[("format", Format::Gif.label())]),
            effect: Effect::Fold(to_gif_fold),
        },
    ];
    for kind in kinds {
        r.register(kind).expect("video steps are registered once");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fold(id: &str, v: serde_json::Value) -> Result<Branch, String> {
        let mut r = Registry::new();
        register(&mut r);
        let Effect::Fold(f) = r.get(Media::Video, id).unwrap().effect else {
            panic!()
        };
        let mut b = Branch::default();
        f(v.as_object().unwrap(), &mut b).map(|()| b)
    }

    fn caption(id: &str, v: serde_json::Value) -> String {
        let mut r = Registry::new();
        register(&mut r);
        (r.get(Media::Video, id).unwrap().summary)(v.as_object().unwrap())
    }

    #[test]
    fn optimise_sets_quality_and_encoder() {
        let b = fold(
            "optimise",
            json!({ "compression": 60, "encoder": "small", "location": "same_folder" }),
        )
        .unwrap();
        assert_eq!(
            (b.recipe.compression, b.recipe.encoder),
            (Some(60), Some(Encoder::Small))
        );
        assert_eq!(b.optimised.unwrap().template, "%f_optimised.%e");
        assert!(fold("optimise", json!({ "compression": 2 })).is_err());
    }

    #[test]
    fn convert_and_to_gif_choose_the_video_format() {
        assert_eq!(
            fold("convert", json!({ "to": "av1" }))
                .unwrap()
                .recipe
                .format,
            Some(Format::Av1)
        );
        assert!(fold("convert", json!({ "to": "mov" })).is_err());
        assert_eq!(
            fold("to_gif", json!({})).unwrap().recipe.format,
            Some(Format::Gif)
        );
    }

    #[test]
    fn speed_and_mute_fill_the_recipe() {
        let b = fold("change_speed", json!({ "percent": 200, "frames": "drop" })).unwrap();
        assert_eq!(
            b.recipe.speed,
            Some(Speed {
                percent: 200,
                keep_frames: false
            })
        );
        assert!(fold("change_speed", json!({ "percent": 10 })).is_err());
        assert!(fold("remove_audio", json!({})).unwrap().recipe.mute);
    }

    #[test]
    fn captions_read_like_the_card() {
        assert_eq!(
            caption("change_speed", json!({ "percent": 150 })),
            "Speed 150 %"
        );
        assert_eq!(
            caption("convert", json!({ "to": "webm" })),
            "Convert to WebM"
        );
        assert_eq!(caption("to_gif", json!({})), "Convert to GIF");
        assert_eq!(caption("remove_audio", json!({})), "Remove audio");
        assert_eq!(
            caption("optimise", json!({ "compression": 70 })),
            "Optimise · q 70"
        );
    }
}
