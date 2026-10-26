//! The PDF steps of a pipeline (stream F, registered after stream E): they
//! fold into the same `Edits` recipe a PDF card edits (`dpi`, `aspect`,
//! `uncrop`, `pages`, `format`).

use serde_json::json;

use crate::{
    edit::{EditsPatch, MAX_DPI, MIN_DPI, PageRange},
    i18n::{t, tf},
    image_steps::{ASPECTS, ratio},
    pipeline_steps::{
        Branch, Control, Effect, Media, Param, Params, Phase, Registry, StepKind, choice, flag,
        number, output_rule, with_location,
    },
    sniff::Format,
};

const FOLDER: &str = "~/Documents/Ferry";
const LAST_PAGE: f64 = 100_000.0;

const PAGE_IMAGES: &[(&str, &str)] = &[("png", "pipe.choice.png"), ("jpeg", "pipe.choice.jpeg")];

const DPI: Param = Param {
    name: "dpi",
    label: "pipe.param.dpi",
    control: Control::Number {
        min: MIN_DPI as f64,
        max: MAX_DPI as f64,
        step: 1.0,
    },
    required: false,
    shown_when: None,
};

const OPTIMISE_PARAMS: &[Param] = with_location!(DPI);

const CROP_PARAMS: &[Param] = &[
    Param {
        name: "aspect",
        label: "pipe.param.aspect",
        control: Control::Choice { options: ASPECTS },
        required: false,
        shown_when: None,
    },
    Param {
        name: "uncrop",
        label: "pipe.param.uncrop",
        control: Control::Switch,
        required: false,
        shown_when: None,
    },
];

const EXTRACT_PARAMS: &[Param] = &[
    Param {
        name: "to",
        label: "pipe.param.convert_to",
        control: Control::Choice {
            options: PAGE_IMAGES,
        },
        required: true,
        shown_when: None,
    },
    Param {
        name: "first",
        label: "pipe.param.first_page",
        control: Control::Number {
            min: 1.0,
            max: LAST_PAGE,
            step: 1.0,
        },
        required: false,
        shown_when: None,
    },
    Param {
        name: "last",
        label: "pipe.param.last_page",
        control: Control::Number {
            min: 1.0,
            max: LAST_PAGE,
            step: 1.0,
        },
        required: false,
        shown_when: None,
    },
    DPI,
];

fn obj(v: serde_json::Value) -> Params {
    v.as_object().cloned().unwrap_or_default()
}

fn dpi_of(p: &Params) -> Result<Option<u16>, String> {
    Ok(
        number(p, "dpi", "pipe.param.dpi", MIN_DPI.into(), MAX_DPI.into())?
            .map(|d| d.round() as u16),
    )
}

fn dpi_caption(p: &Params, mut caption: String) -> String {
    if let Some(n) = p.get("dpi").and_then(|v| v.as_f64()) {
        caption.push_str(" · ");
        caption.push_str(&tf(
            "pipe.caption.dpi",
            &[("n", &(n.round() as u32).to_string())],
        ));
    }
    caption
}

fn optimise_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let dpi = dpi_of(p)?;
    let rule = output_rule(p, FOLDER, "%f_optimised.%e")?;
    if dpi.is_some() {
        b.recipe.merge(EditsPatch {
            dpi: Some(dpi),
            ..EditsPatch::default()
        });
    }
    if rule.is_some() {
        b.optimised = rule;
    }
    Ok(())
}

fn crop_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let aspect = choice(p, "aspect", "pipe.param.aspect", ASPECTS)?.unwrap_or("16:9");
    let uncrop = flag(p, "uncrop");
    if aspect == "free" && !uncrop {
        return Err(t("pipe.error.crop_empty"));
    }
    b.recipe.merge(EditsPatch {
        aspect: Some(ratio(aspect)),
        uncrop: Some(uncrop),
        ..EditsPatch::default()
    });
    Ok(())
}

fn crop_summary(p: &Params) -> String {
    let aspect = p.get("aspect").and_then(|v| v.as_str()).unwrap_or("16:9");
    if aspect == "free" {
        return t("pipe.caption.uncrop");
    }
    let label = ASPECTS
        .iter()
        .find(|(v, _)| *v == aspect)
        .map_or_else(|| aspect.to_owned(), |(_, label)| t(label));
    tf("pipe.caption.crop", &[("ratio", &label)])
}

fn extract_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let to = choice(p, "to", "pipe.param.convert_to", PAGE_IMAGES)?.ok_or_else(|| {
        tf(
            "pipe.error.required",
            &[("name", &t("pipe.param.convert_to"))],
        )
    })?;
    let page = |name, label| number(p, name, label, 1.0, LAST_PAGE).map(|n| n.map(|n| n as u32));
    let first = page("first", "pipe.param.first_page")?;
    let last = page("last", "pipe.param.last_page")?;
    let range = (first.is_some() || last.is_some()).then(|| PageRange {
        first: first.unwrap_or(1),
        last: last.unwrap_or(LAST_PAGE as u32),
    });
    if range.is_some_and(|r| r.last < r.first) {
        return Err(tf(
            "pipe.error.range",
            &[
                ("name", &t("pipe.param.last_page")),
                ("min", &first.unwrap_or(1).to_string()),
                ("max", &(LAST_PAGE as u32).to_string()),
            ],
        ));
    }
    b.recipe.merge(EditsPatch {
        format: Some(Some(if to == "jpeg" {
            Format::Jpeg
        } else {
            Format::Png
        })),
        pages: Some(range),
        dpi: Some(dpi_of(p)?),
        ..EditsPatch::default()
    });
    Ok(())
}

fn extract_summary(p: &Params) -> String {
    let format = if p.get("to").and_then(|v| v.as_str()) == Some("jpeg") {
        Format::Jpeg
    } else {
        Format::Png
    };
    dpi_caption(p, tf("pipe.caption.extract", &[("format", format.label())]))
}

pub fn register(r: &mut Registry) {
    let kinds = [
        StepKind {
            id: "optimise",
            media: Media::Pdf,
            label: "pipe.step.optimise",
            phase: Phase::Transform,
            params: OPTIMISE_PARAMS,
            defaults: || obj(json!({ "location": "keep" })),
            summary: |p| dpi_caption(p, t("pipe.caption.optimise")),
            effect: Effect::Fold(optimise_fold),
        },
        StepKind {
            id: "crop",
            media: Media::Pdf,
            label: "pipe.step.crop",
            phase: Phase::Transform,
            params: CROP_PARAMS,
            defaults: || obj(json!({ "aspect": "16:9", "uncrop": false })),
            summary: crop_summary,
            effect: Effect::Fold(crop_fold),
        },
        StepKind {
            id: "extract_pages",
            media: Media::Pdf,
            label: "pipe.step.extract_pages",
            phase: Phase::Transform,
            params: EXTRACT_PARAMS,
            defaults: || obj(json!({ "to": "png" })),
            summary: extract_summary,
            effect: Effect::Fold(extract_fold),
        },
    ];
    for kind in kinds {
        r.register(kind).expect("PDF steps are registered once");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{edit::Edits, pipeline_steps::builtin};

    fn fold(id: &str, params: serde_json::Value) -> Result<Branch, String> {
        let kind = builtin().get(Media::Pdf, id).copied().unwrap();
        let Effect::Fold(f) = kind.effect else {
            panic!()
        };
        let mut b = Branch::default();
        f(&obj(params), &mut b).map(|()| b)
    }

    #[test]
    fn optimise_sets_the_dpi_and_a_location() {
        let b = fold("optimise", json!({ "dpi": 110, "location": "same_folder" })).unwrap();
        assert_eq!(b.recipe.dpi, Some(110));
        assert!(b.optimised.is_some());
        assert!(fold("optimise", json!({ "dpi": 5 })).is_err());
        assert_eq!(
            fold("optimise", json!({})).unwrap().recipe,
            Edits::default()
        );
    }

    #[test]
    fn crop_sets_a_ratio_or_uncrops_and_touches_no_pixel_size() {
        let b = fold("crop", json!({ "aspect": "4:3" })).unwrap();
        assert_eq!((b.recipe.aspect, b.recipe.uncrop), (Some((4, 3)), false));
        assert_eq!(b.recipe.size, None);
        let b = fold("crop", json!({ "aspect": "free", "uncrop": true })).unwrap();
        assert_eq!((b.recipe.aspect, b.recipe.uncrop), (None, true));
        assert!(fold("crop", json!({ "aspect": "free" })).is_err());
    }

    #[test]
    fn extract_pages_sets_format_range_and_dpi() {
        let b = fold(
            "extract_pages",
            json!({ "to": "jpeg", "first": 2, "last": 5, "dpi": 96 }),
        )
        .unwrap();
        assert_eq!(b.recipe.format, Some(Format::Jpeg));
        assert_eq!(b.recipe.pages, Some(PageRange { first: 2, last: 5 }));
        assert_eq!(b.recipe.dpi, Some(96));
        let all = fold("extract_pages", json!({ "to": "png" })).unwrap();
        assert_eq!(
            (all.recipe.pages, all.recipe.format),
            (None, Some(Format::Png))
        );
        let from = fold("extract_pages", json!({ "to": "png", "first": 3 })).unwrap();
        assert_eq!(from.recipe.pages.map(|r| r.first), Some(3));
        assert!(
            fold(
                "extract_pages",
                json!({ "to": "png", "first": 4, "last": 2 })
            )
            .is_err()
        );
    }

    #[test]
    fn the_pdf_steps_are_registered() {
        let ids: Vec<_> = builtin()
            .for_media(Media::Pdf)
            .iter()
            .map(|k| k.id)
            .collect();
        for id in ["optimise", "crop", "extract_pages"] {
            assert!(ids.contains(&id), "{id}");
        }
    }
}
