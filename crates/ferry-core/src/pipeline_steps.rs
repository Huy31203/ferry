//! The step registry: what a pipeline step is, and the steps every media type
//! shares (`copy`, `fork`, `if_name_contains`). Image steps live in
//! `image_steps`; streams for other media register theirs in `builtin()`.

use std::sync::LazyLock;

use serde::{Deserialize, Serialize, Serializer, ser::SerializeSeq, ser::SerializeStruct};

use crate::{
    edit::Edits,
    i18n::{t, tf},
    settings::OutputRule,
    template,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Media {
    #[default]
    Image,
    Video,
    Audio,
    Pdf,
    #[serde(other)]
    Other,
}

impl Media {
    pub const ALL: [Media; 4] = [Media::Image, Media::Video, Media::Audio, Media::Pdf];

    /// By extension, case-insensitive.
    pub fn guess(ext: &str) -> Option<Media> {
        match ext.to_ascii_lowercase().as_str() {
            "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "avif" | "tif" | "tiff"
            | "jxl" => Some(Media::Image),
            "mp4" | "mov" | "mkv" | "webm" | "avi" | "m4v" => Some(Media::Video),
            "mp3" | "m4a" | "aac" | "wav" | "flac" | "aiff" | "aif" | "ogg" | "opus" => {
                Some(Media::Audio)
            }
            "pdf" => Some(Media::Pdf),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    /// Folds into the branch's recipe.
    Transform,
    /// Runs on the branch's finished result.
    Output,
    /// Changes how the pipeline runs.
    Control,
}

/// How the editor draws a param.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Control {
    Number {
        min: f64,
        max: f64,
        step: f64,
    },
    /// `(value, i18n label key)`; serialised as `[{value, label}]`.
    Choice {
        #[serde(serialize_with = "ser_options")]
        options: &'static [(&'static str, &'static str)],
    },
    Switch,
    Text,
    Template,
    Folder,
}

fn ser_options<S: Serializer>(
    options: &&'static [(&'static str, &'static str)],
    s: S,
) -> Result<S::Ok, S::Error> {
    let mut seq = s.serialize_seq(Some(options.len()))?;
    for (value, label) in options.iter() {
        seq.serialize_element(&serde_json::json!({ "value": value, "label": label }))?;
    }
    seq.end()
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Param {
    pub name: &'static str,
    /// i18n key.
    pub label: &'static str,
    pub control: Control,
    pub required: bool,
    /// Shown in the inspector only when `param` has one of `values`;
    /// serialised as `{param, values}` or `null`.
    #[serde(serialize_with = "ser_shown_when")]
    pub shown_when: Option<(&'static str, &'static [&'static str])>,
}

fn ser_shown_when<S: Serializer>(
    when: &Option<(&'static str, &'static [&'static str])>,
    s: S,
) -> Result<S::Ok, S::Error> {
    match when {
        None => s.serialize_none(),
        Some((param, values)) => {
            let mut st = s.serialize_struct("ShownWhen", 2)?;
            st.serialize_field("param", param)?;
            st.serialize_field("values", values)?;
            st.end()
        }
    }
}

/// What a file looks like to a guard and to captions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts {
    pub stem: String,
    pub ext: String,
    pub media: Media,
}

/// A step that runs after the branch's single encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After {
    CopyTo { template: String, step: usize },
}

/// One result being built: the recipe for `Job::run`, output-location
/// overrides, after-steps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Branch {
    pub recipe: Edits,
    pub optimised: Option<OutputRule>,
    pub converted: Option<OutputRule>,
    pub after: Vec<After>,
    /// 1-based number and caption of the last step that changed `recipe` (the card's step text).
    pub last_change: Option<(usize, String)>,
    /// 0-based index of the step being folded; set by `compile` before each fold.
    pub step: usize,
}

pub type Params = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Copy)]
pub enum Effect {
    Fold(fn(&Params, &mut Branch) -> Result<(), String>),
    Fork,
    Guard(fn(&Params, &Facts) -> Result<bool, String>),
}

#[derive(Clone, Copy)]
pub struct StepKind {
    /// Stable forever; stored in settings.
    pub id: &'static str,
    pub media: Media,
    /// i18n key, such as `pipe.step.downscale`.
    pub label: &'static str,
    pub phase: Phase,
    pub params: &'static [Param],
    pub defaults: fn() -> Params,
    /// The caption: "Downscale to 50 %".
    pub summary: fn(&Params) -> String,
    pub effect: Effect,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepKindView {
    pub id: String,
    pub media: Media,
    pub label: String,
    pub phase: Phase,
    pub params: Vec<Param>,
    pub defaults: Params,
}

#[derive(Default)]
pub struct Registry {
    kinds: Vec<StepKind>,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// `Err` names the (media, id) pair already taken.
    pub fn register(&mut self, kind: StepKind) -> Result<(), String> {
        if self.get(kind.media, kind.id).is_some() {
            return Err(format!(
                "{:?} step \"{}\" is already registered",
                kind.media, kind.id
            ));
        }
        self.kinds.push(kind);
        Ok(())
    }

    pub fn get(&self, media: Media, id: &str) -> Option<&StepKind> {
        self.kinds.iter().find(|k| k.media == media && k.id == id)
    }

    pub fn for_media(&self, media: Media) -> Vec<&StepKind> {
        self.kinds.iter().filter(|k| k.media == media).collect()
    }

    /// Media with at least one Transform step, in `Media::ALL` order.
    pub fn media(&self) -> Vec<Media> {
        Media::ALL
            .into_iter()
            .filter(|m| {
                self.kinds
                    .iter()
                    .any(|k| k.media == *m && k.phase == Phase::Transform)
            })
            .collect()
    }

    pub fn views(&self) -> Vec<StepKindView> {
        self.kinds
            .iter()
            .map(|k| StepKindView {
                id: k.id.into(),
                media: k.media,
                label: k.label.into(),
                phase: k.phase,
                params: k.params.to_vec(),
                defaults: (k.defaults)(),
            })
            .collect()
    }
}

// ---- parameter helpers, shared with the other streams' steps ----

/// `{name}`'s text: the translated label of a param.
fn shown(label: &str) -> String {
    t(label)
}

/// Whole numbers print without a decimal point.
fn plain(n: f64) -> String {
    if n.fract() == 0.0 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// A number param in `min..=max`; `None` when absent.
pub fn number(
    p: &Params,
    name: &str,
    label: &str,
    min: f64,
    max: f64,
) -> Result<Option<f64>, String> {
    let Some(value) = p.get(name).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    match value.as_f64() {
        Some(n) if (min..=max).contains(&n) => Ok(Some(n)),
        _ => Err(tf(
            "pipe.error.range",
            &[
                ("name", &shown(label)),
                ("min", &plain(min)),
                ("max", &plain(max)),
            ],
        )),
    }
}

/// A choice param that is one of `options`' values; `None` when absent.
pub fn choice<'a>(
    p: &'a Params,
    name: &str,
    label: &str,
    options: &[(&str, &str)],
) -> Result<Option<&'a str>, String> {
    let Some(value) = p.get(name).filter(|v| !v.is_null()) else {
        return Ok(None);
    };
    match value.as_str() {
        Some(s) if options.iter().any(|(v, _)| *v == s) => Ok(Some(s)),
        _ => {
            let shown_value = value
                .as_str()
                .map_or_else(|| value.to_string(), str::to_owned);
            Err(tf(
                "pipe.error.choice",
                &[("name", &shown(label)), ("value", &shown_value)],
            ))
        }
    }
}

/// A text param, as typed.
pub fn text<'a>(p: &'a Params, name: &str) -> Option<&'a str> {
    p.get(name).and_then(|v| v.as_str())
}

pub fn flag(p: &Params, name: &str) -> bool {
    p.get(name).and_then(|v| v.as_bool()).unwrap_or(false)
}

/// A text param that must be set.
fn required<'a>(p: &'a Params, name: &str, label: &str) -> Result<&'a str, String> {
    text(p, name)
        .filter(|s| !s.trim().is_empty())
        .ok_or_else(|| tf("pipe.error.required", &[("name", &shown(label))]))
}

// ---- the steps every media type shares ----

const COPY_PARAMS: &[Param] = &[Param {
    name: "to",
    label: "pipe.param.copy_to",
    control: Control::Template,
    required: true,
    shown_when: None,
}];

const IF_PARAMS: &[Param] = &[Param {
    name: "text",
    label: "pipe.param.text",
    control: Control::Text,
    required: true,
    shown_when: None,
}];

fn no_params() -> Params {
    Params::new()
}

/// The text before the last path separator, as typed (`~/Out/%f.%e` is `~/Out`).
fn copy_dir(template: &str) -> &str {
    match template.rfind(['/', '\\']) {
        Some(0) => &template[..1],
        Some(i) => &template[..i],
        None => template,
    }
}

fn copy_fold(p: &Params, b: &mut Branch) -> Result<(), String> {
    let tpl = required(p, "to", "pipe.param.copy_to")?;
    template::validate(tpl)
        .map_err(|tokens| tf("pipe.error.template", &[("tokens", &tokens.join(", "))]))?;
    let name = tpl.rsplit(['/', '\\']).next().unwrap_or_default();
    if name.is_empty() {
        return Err(t("pipe.error.copy_no_name"));
    }
    b.after.push(After::CopyTo {
        template: tpl.to_owned(),
        step: b.step,
    });
    Ok(())
}

fn copy_summary(p: &Params) -> String {
    tf(
        "pipe.caption.copy",
        &[("dir", copy_dir(text(p, "to").unwrap_or_default()))],
    )
}

fn if_guard(p: &Params, facts: &Facts) -> Result<bool, String> {
    let needle = required(p, "text", "pipe.param.text")?;
    Ok(facts.stem.to_lowercase().contains(&needle.to_lowercase()))
}

fn if_summary(p: &Params) -> String {
    tf(
        "pipe.caption.if",
        &[("text", text(p, "text").unwrap_or_default())],
    )
}

/// `copy`, `fork` and `if_name_contains` for `media`.
pub fn register_common(r: &mut Registry, media: Media) {
    let kinds = [
        StepKind {
            id: "copy",
            media,
            label: "pipe.step.copy",
            phase: Phase::Output,
            params: COPY_PARAMS,
            defaults: no_params,
            summary: copy_summary,
            effect: Effect::Fold(copy_fold),
        },
        StepKind {
            id: "fork",
            media,
            label: "pipe.step.fork",
            phase: Phase::Control,
            params: &[],
            defaults: no_params,
            summary: |_| t("pipe.caption.fork"),
            effect: Effect::Fork,
        },
        StepKind {
            id: "if_name_contains",
            media,
            label: "pipe.step.if_name_contains",
            phase: Phase::Control,
            params: IF_PARAMS,
            defaults: no_params,
            summary: if_summary,
            effect: Effect::Guard(if_guard),
        },
    ];
    for kind in kinds {
        r.register(kind)
            .expect("common steps are registered once per media");
    }
}

/// Every step kind this build knows. This is THE hook for the other streams.
pub fn builtin() -> Registry {
    let mut r = Registry::new();
    for media in Media::ALL {
        register_common(&mut r, media);
    }
    crate::image_steps::register(&mut r);
    // Later streams register their steps here, one line each:
    // crate::video_steps::register(&mut r);   // stream A: Media::Video
    // crate::audio_steps::register(&mut r);   // stream B: Media::Audio
    // crate::pdf_steps::register(&mut r);     // stream E: Media::Pdf
    r
}

/// `builtin()`, built once.
pub fn registry() -> &'static Registry {
    static REGISTRY: LazyLock<Registry> = LazyLock::new(builtin);
    &REGISTRY
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn noop(_: &Params, _: &mut Branch) -> Result<(), String> {
        Ok(())
    }

    fn kind(id: &'static str, media: Media, phase: Phase) -> StepKind {
        StepKind {
            id,
            media,
            label: "x",
            phase,
            params: &[],
            defaults: Params::new,
            summary: |_| String::new(),
            effect: Effect::Fold(noop),
        }
    }

    fn params(v: serde_json::Value) -> Params {
        v.as_object().unwrap().clone()
    }

    #[test]
    fn a_second_kind_with_the_same_id_and_media_is_refused() {
        let mut r = Registry::new();
        r.register(kind("a", Media::Image, Phase::Transform))
            .unwrap();
        assert!(
            r.register(kind("a", Media::Image, Phase::Transform))
                .is_err()
        );
        assert!(
            r.register(kind("a", Media::Video, Phase::Transform))
                .is_ok()
        );
    }

    #[test]
    fn only_media_with_a_transform_step_are_offered() {
        assert_eq!(builtin().media(), [Media::Image]);
        assert!(builtin().get(Media::Video, "copy").is_some());
    }

    #[test]
    fn media_is_guessed_from_the_extension() {
        assert_eq!(Media::guess("PNG"), Some(Media::Image));
        assert_eq!(Media::guess("mov"), Some(Media::Video));
        assert_eq!(Media::guess("m4a"), Some(Media::Audio));
        assert_eq!(Media::guess("pdf"), Some(Media::Pdf));
        assert_eq!(Media::guess("zip"), None);
    }

    #[test]
    fn an_unknown_media_name_reads_as_other() {
        assert_eq!(
            serde_json::from_str::<Media>("\"hologram\"").unwrap(),
            Media::Other
        );
    }

    #[test]
    fn copy_adds_an_after_step_and_needs_a_file_name() {
        let copy = builtin().get(Media::Image, "copy").copied().unwrap();
        let Effect::Fold(fold) = copy.effect else {
            panic!()
        };
        let mut b = Branch {
            step: 3,
            ..Branch::default()
        };
        fold(&params(json!({ "to": "~/Out/%f_%z.%e" })), &mut b).unwrap();
        assert_eq!(
            b.after,
            [After::CopyTo {
                template: "~/Out/%f_%z.%e".into(),
                step: 3
            }]
        );
        assert!(fold(&params(json!({ "to": "~/Out/" })), &mut Branch::default()).is_err());
        assert!(
            fold(
                &params(json!({ "to": "~/Out/%q.png" })),
                &mut Branch::default()
            )
            .unwrap_err()
            .contains("%q")
        );
    }

    #[test]
    fn if_name_contains_ignores_case_and_needs_text() {
        let guard = builtin()
            .get(Media::Image, "if_name_contains")
            .copied()
            .unwrap();
        let Effect::Guard(g) = guard.effect else {
            panic!()
        };
        let facts = Facts {
            stem: "ACME Invoice 42".into(),
            ext: "png".into(),
            media: Media::Image,
        };
        assert!(g(&params(json!({ "text": "invoice" })), &facts).unwrap());
        assert!(!g(&params(json!({ "text": "receipt" })), &facts).unwrap());
        assert!(g(&params(json!({})), &facts).is_err());
    }

    #[test]
    fn views_carry_labels_params_and_defaults() {
        let views = builtin().views();
        let copy = views
            .iter()
            .find(|v| v.id == "copy" && v.media == Media::Image)
            .unwrap();
        assert_eq!(copy.label, "pipe.step.copy");
        assert_eq!(
            serde_json::to_value(&copy.params[0]).unwrap()["control"]["kind"],
            "template"
        );
    }

    #[test]
    fn a_choice_and_a_condition_serialise_for_the_editor() {
        let param = Param {
            name: "by",
            label: "pipe.param.by",
            control: Control::Choice {
                options: &[("a", "pipe.choice.png")],
            },
            required: false,
            shown_when: Some(("location", &["folder"])),
        };
        let v = serde_json::to_value(param).unwrap();
        assert_eq!(
            v["control"]["options"],
            json!([{ "value": "a", "label": "pipe.choice.png" }])
        );
        assert_eq!(
            v["shown_when"],
            json!({ "param": "location", "values": ["folder"] })
        );
    }

    #[test]
    fn helpers_check_ranges_choices_and_print_whole_numbers() {
        let p = params(json!({ "n": 3, "c": "x", "f": true }));
        assert_eq!(
            number(&p, "n", "pipe.param.value", 5.0, 100.0).unwrap_err(),
            "Value must be between 5 and 100"
        );
        assert_eq!(number(&p, "none", "pipe.param.value", 5.0, 100.0), Ok(None));
        assert!(choice(&p, "c", "pipe.param.by", &[("y", "")]).is_err());
        assert_eq!(
            choice(&p, "c", "pipe.param.by", &[("x", "")]),
            Ok(Some("x"))
        );
        assert!(flag(&p, "f") && !flag(&p, "g"));
    }
}
