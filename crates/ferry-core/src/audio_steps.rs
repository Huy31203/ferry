//! The audio steps of a pipeline (stream F, registered after stream B): they
//! fold into the same `Edits` recipe an audio card edits.

use serde_json::json;

use crate::{
    audio::{CoverPolicy, Loudness},
    edit::EditsPatch,
    i18n::{t, tf},
    pipeline_steps::{
        Branch, Control, Effect, Media, Param, Params, Phase, Registry, StepKind, choice, number,
        output_rule, text, with_location,
    },
    sniff::Format,
};

const FOLDER: &str = "~/Music/Ferry";

const TARGETS: &[(&str, &str)] = &[
    ("mp3", "audio.format.mp3"),
    ("m4a", "audio.format.m4a"),
    ("opus", "audio.format.opus"),
    ("wav", "audio.format.wav"),
    ("flac", "audio.format.flac"),
];

const LOUDNESS: &[(&str, &str)] = &[
    ("off", "audio.loudness.off"),
    ("streaming", "audio.loudness.streaming"),
    ("apple_music", "audio.loudness.apple_music"),
    ("podcast", "audio.loudness.podcast"),
    ("broadcast", "audio.loudness.broadcast"),
];

const COVERS: &[(&str, &str)] = &[
    ("keep", "audio.cover.keep"),
    ("optimise", "audio.cover.optimise"),
    ("downscale", "audio.cover.downscale"),
    ("remove", "audio.cover.remove"),
];

const OPTIMISE_PARAMS: &[Param] = with_location!(Param {
    name: "bitrate",
    label: "pipe.param.bitrate",
    control: Control::Number {
        min: 16.0,
        max: 320.0,
        step: 8.0
    },
    required: false,
    shown_when: None,
});

const CONVERT_PARAMS: &[Param] = with_location!(Param {
    name: "to",
    label: "pipe.param.convert_to",
    control: Control::Choice { options: TARGETS },
    required: true,
    shown_when: None,
});

const LOUDNESS_PARAMS: &[Param] = &[Param {
    name: "target",
    label: "pipe.param.loudness",
    control: Control::Choice { options: LOUDNESS },
    required: true,
    shown_when: None,
}];

const COVER_PARAMS: &[Param] = &[Param {
    name: "policy",
    label: "pipe.param.cover",
    control: Control::Choice { options: COVERS },
    required: true,
    shown_when: None,
}];

fn obj(v: serde_json::Value) -> Params {
    v.as_object().cloned().unwrap_or_default()
}

fn required(name: &str) -> String {
    tf("pipe.error.required", &[("name", &t(name))])
}

fn target_format(to: &str) -> Format {
    match to {
        "m4a" => Format::M4a,
        "opus" => Format::Opus,
        "wav" => Format::Wav,
        "flac" => Format::Flac,
        _ => Format::Mp3,
    }
}

fn optimise_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let bitrate = number(p, "bitrate", "pipe.param.bitrate", 16.0, 320.0)?;
    let rule = output_rule(p, FOLDER, "%f_optimised.%e")?;
    if let Some(kbps) = bitrate {
        b.recipe.merge(EditsPatch {
            bitrate: Some(Some(kbps.round() as u16)),
            ..EditsPatch::default()
        });
    }
    if rule.is_some() {
        b.optimised = rule;
    }
    Ok(())
}

fn optimise_summary(p: &Params) -> String {
    let mut caption = t("pipe.caption.optimise");
    if let Some(kbps) = p.get("bitrate").and_then(|v| v.as_f64()) {
        caption.push_str(" · ");
        caption.push_str(&tf(
            "pipe.caption.bitrate",
            &[("n", &(kbps.round() as u32).to_string())],
        ));
    }
    caption
}

fn convert_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let to = choice(p, "to", "pipe.param.convert_to", TARGETS)?
        .ok_or_else(|| required("pipe.param.convert_to"))?;
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

fn loudness_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let target = choice(p, "target", "pipe.param.loudness", LOUDNESS)?
        .ok_or_else(|| required("pipe.param.loudness"))?;
    b.recipe.merge(EditsPatch {
        loudness: Some(Some(match target {
            "streaming" => Loudness::Streaming,
            "apple_music" => Loudness::AppleMusic,
            "podcast" => Loudness::Podcast,
            "broadcast" => Loudness::Broadcast,
            _ => Loudness::Off,
        })),
        ..EditsPatch::default()
    });
    Ok(())
}

fn loudness_summary(p: &Params) -> String {
    let label = text(p, "target")
        .and_then(|v| LOUDNESS.iter().find(|(id, _)| *id == v))
        .map_or_else(String::new, |(_, key)| t(key));
    tf("pipe.caption.loudness", &[("target", &label)])
}

fn cover_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let policy = choice(p, "policy", "pipe.param.cover", COVERS)?
        .ok_or_else(|| required("pipe.param.cover"))?;
    b.recipe.merge(EditsPatch {
        cover: Some(Some(match policy {
            "keep" => CoverPolicy::Keep,
            "downscale" => CoverPolicy::Downscale,
            "remove" => CoverPolicy::Remove,
            _ => CoverPolicy::Optimise,
        })),
        ..EditsPatch::default()
    });
    Ok(())
}

fn cover_summary(p: &Params) -> String {
    let label = text(p, "policy")
        .and_then(|v| COVERS.iter().find(|(id, _)| *id == v))
        .map_or_else(String::new, |(_, key)| t(key));
    tf("pipe.caption.cover", &[("policy", &label)])
}

pub fn register(r: &mut Registry) {
    let kinds = [
        StepKind {
            id: "optimise",
            media: Media::Audio,
            label: "pipe.step.optimise",
            phase: Phase::Transform,
            params: OPTIMISE_PARAMS,
            defaults: || obj(json!({ "location": "keep" })),
            summary: optimise_summary,
            effect: Effect::Fold(optimise_fold),
        },
        StepKind {
            id: "convert",
            media: Media::Audio,
            label: "pipe.step.convert",
            phase: Phase::Transform,
            params: CONVERT_PARAMS,
            defaults: || obj(json!({ "to": "mp3", "location": "keep" })),
            summary: convert_summary,
            effect: Effect::Fold(convert_fold),
        },
        StepKind {
            id: "normalise_loudness",
            media: Media::Audio,
            label: "pipe.step.normalise_loudness",
            phase: Phase::Transform,
            params: LOUDNESS_PARAMS,
            defaults: || obj(json!({ "target": "streaming" })),
            summary: loudness_summary,
            effect: Effect::Fold(loudness_fold),
        },
        StepKind {
            id: "cover_art",
            media: Media::Audio,
            label: "pipe.step.cover_art",
            phase: Phase::Transform,
            params: COVER_PARAMS,
            defaults: || obj(json!({ "policy": "optimise" })),
            summary: cover_summary,
            effect: Effect::Fold(cover_fold),
        },
    ];
    for kind in kinds {
        r.register(kind).expect("audio steps are registered once");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kind(id: &str) -> StepKind {
        let mut r = Registry::new();
        register(&mut r);
        *r.get(Media::Audio, id).unwrap()
    }

    fn fold(id: &str, v: serde_json::Value) -> Result<Branch, String> {
        let Effect::Fold(f) = kind(id).effect else {
            panic!()
        };
        let mut b = Branch::default();
        f(v.as_object().unwrap(), &mut b).map(|()| b)
    }

    #[test]
    fn optimise_sets_the_bitrate_and_location() {
        let b = fold(
            "optimise",
            json!({ "bitrate": 128, "location": "same_folder" }),
        )
        .unwrap();
        assert_eq!(b.recipe.bitrate, Some(128));
        assert_eq!(b.optimised.unwrap().template, "%f_optimised.%e");
        assert!(fold("optimise", json!({ "bitrate": 1000 })).is_err());
    }

    #[test]
    fn convert_loudness_and_cover_fill_the_recipe() {
        assert_eq!(
            fold("convert", json!({ "to": "opus" }))
                .unwrap()
                .recipe
                .format,
            Some(Format::Opus)
        );
        assert!(fold("convert", json!({ "to": "ogg" })).is_err());
        assert_eq!(
            fold("normalise_loudness", json!({ "target": "podcast" }))
                .unwrap()
                .recipe
                .loudness,
            Some(Loudness::Podcast)
        );
        assert_eq!(
            fold("cover_art", json!({ "policy": "remove" }))
                .unwrap()
                .recipe
                .cover,
            Some(CoverPolicy::Remove)
        );
        assert!(fold("cover_art", json!({})).is_err());
    }

    #[test]
    fn captions_read_like_the_card() {
        let caption = |id: &str, v: serde_json::Value| (kind(id).summary)(v.as_object().unwrap());
        assert_eq!(
            caption("convert", json!({ "to": "flac" })),
            "Convert to FLAC"
        );
        assert_eq!(
            caption("optimise", json!({ "bitrate": 192 })),
            "Optimise · 192 kbps"
        );
        assert_eq!(
            caption("normalise_loudness", json!({ "target": "broadcast" })),
            "Loudness Broadcast (−23 LUFS)"
        );
        assert_eq!(
            caption("cover_art", json!({ "policy": "remove" })),
            "Cover art: Remove"
        );
    }
}
