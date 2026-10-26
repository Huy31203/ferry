//! Turning a stored pipeline into what `Job::run` applies: transform steps fold
//! into one recipe per branch, a fork closes a branch, a failing guard stops
//! the walk. Also the editor's checks and the step captions.

use serde::Serialize;

use crate::{
    i18n::{t, tf},
    pipeline_def::Pipeline,
    pipeline_steps::{Branch, Effect, Facts, Phase, Registry},
};

#[derive(Debug, Clone, PartialEq)]
pub struct Compiled {
    /// Closed branches, in order; each is its own result.
    pub forks: Vec<Branch>,
    /// The last branch.
    pub main: Branch,
    pub hide_result: bool,
    pub total: usize,
    /// The 0-based step whose guard failed, when one did.
    pub stopped_at: Option<usize>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue {
    /// 0-based step index.
    pub step: usize,
    pub message: String,
}

/// Fold `p` for one file. `Err` is the first issue: the run is refused.
pub fn compile(p: &Pipeline, r: &Registry, facts: &Facts) -> Result<Compiled, Issue> {
    walk(p, r, facts, true).map_err(|mut issues| issues.remove(0))
}

/// Every issue, for the editor: guards count as passing, and compiling goes on
/// after an issue.
pub fn check(p: &Pipeline, r: &Registry) -> Vec<Issue> {
    let facts = Facts {
        stem: String::new(),
        ext: String::new(),
        media: p.media,
    };
    match walk(p, r, &facts, false) {
        Ok(_) => Vec::new(),
        Err(issues) => issues,
    }
}

/// Walk the steps. With `first_only` the first issue ends the walk; without it
/// a failing guard is treated as passing and the walk goes on.
fn walk(
    p: &Pipeline,
    r: &Registry,
    facts: &Facts,
    first_only: bool,
) -> Result<Compiled, Vec<Issue>> {
    let mut issues = Vec::new();
    let mut branch = Branch::default();
    let mut forks = Vec::new();
    let mut seen_output = false;
    let mut stopped_at = None;
    for (i, step) in p.steps.iter().enumerate() {
        let mut issue = |message: String| issues.push(Issue { step: i, message });
        let Some(kind) = r.get(p.media, &step.kind) else {
            issue(tf("pipe.error.unknown_step", &[("kind", &step.kind)]));
            if first_only {
                break;
            }
            continue;
        };
        branch.step = i;
        match kind.effect {
            Effect::Fold(fold) => {
                if kind.phase == Phase::Transform && seen_output {
                    issue(t("pipe.error.after_output"));
                } else if let Err(message) = fold(&step.params, &mut branch) {
                    issue(message);
                } else {
                    if kind.phase == Phase::Transform {
                        branch.last_change = Some((i + 1, (kind.summary)(&step.params)));
                    }
                    if kind.phase == Phase::Output {
                        seen_output = true;
                    }
                }
            }
            Effect::Fork => {
                if i + 1 == p.steps.len() {
                    issue(t("pipe.error.fork_last"));
                }
                forks.push(branch.clone());
                branch.after.clear();
                seen_output = false;
            }
            Effect::Guard(guard) => match guard(&step.params, facts) {
                Err(message) => issue(message),
                Ok(false) if first_only => {
                    stopped_at = Some(i);
                    break;
                }
                Ok(_) => {}
            },
        }
        if first_only && !issues.is_empty() {
            break;
        }
    }
    if !issues.is_empty() {
        return Err(issues);
    }
    Ok(Compiled {
        forks,
        main: branch,
        hide_result: p.hide_result,
        total: p.steps.len(),
        stopped_at,
    })
}

/// The caption of each step; an unregistered one says it is unavailable.
pub fn captions(p: &Pipeline, r: &Registry) -> Vec<String> {
    p.steps
        .iter()
        .map(|step| match r.get(p.media, &step.kind) {
            Some(kind) => (kind.summary)(&step.params),
            None => tf("pipe.caption.unknown", &[("kind", &step.kind)]),
        })
        .collect()
}

/// The first `max` captions joined with " · ", for zone tiles.
pub fn caption(p: &Pipeline, r: &Registry, max: usize) -> String {
    captions(p, r)
        .into_iter()
        .take(max)
        .collect::<Vec<_>>()
        .join(" · ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        edit::Size,
        pipeline_def::starters,
        pipeline_steps::{Media, builtin},
        sniff::Format,
    };
    use serde_json::json;

    fn pipe(steps: serde_json::Value) -> Pipeline {
        Pipeline {
            id: 1,
            name: "t".into(),
            media: Media::Image,
            steps: serde_json::from_value(steps).unwrap(),
            ..Default::default()
        }
    }

    fn facts(stem: &str) -> Facts {
        Facts {
            stem: stem.into(),
            ext: "png".into(),
            media: Media::Image,
        }
    }

    #[test]
    fn transforms_fold_into_one_recipe_and_later_steps_win() {
        let c = compile(
            &pipe(json!([
                { "kind": "downscale", "params": { "by": "percent", "value": 50 } },
                { "kind": "convert", "params": { "to": "webp" } },
                { "kind": "downscale", "params": { "by": "width", "value": 800 } }
            ])),
            &builtin(),
            &facts("a"),
        )
        .unwrap();
        assert_eq!(
            (
                c.main.recipe.scale,
                c.main.recipe.size,
                c.main.recipe.format
            ),
            (None, Some(Size::Width { width: 800 }), Some(Format::WebP))
        );
        assert_eq!(
            c.main.last_change,
            Some((3, "Downscale to 800 px wide".into()))
        );
        assert!(c.forks.is_empty());
    }

    #[test]
    fn a_change_after_copy_is_an_issue_but_after_fork_it_is_a_new_branch() {
        let bad = pipe(json!([
            { "kind": "copy", "params": { "to": "~/a/%f.%e" } },
            { "kind": "convert", "params": { "to": "png" } }
        ]));
        assert_eq!(compile(&bad, &builtin(), &facts("a")).unwrap_err().step, 1);
        let good = pipe(json!([
            { "kind": "convert", "params": { "to": "webp" } },
            { "kind": "copy", "params": { "to": "~/a/%f.%e" } },
            { "kind": "fork" },
            { "kind": "downscale", "params": { "by": "percent", "value": 50 } }
        ]));
        let c = compile(&good, &builtin(), &facts("a")).unwrap();
        assert_eq!(c.forks.len(), 1);
        assert_eq!(
            (
                c.forks[0].recipe.format,
                c.forks[0].recipe.scale,
                c.forks[0].after.len()
            ),
            (Some(Format::WebP), None, 1)
        );
        assert_eq!(
            (
                c.main.recipe.format,
                c.main.recipe.scale,
                c.main.after.len()
            ),
            (Some(Format::WebP), Some(50), 0)
        );
    }

    #[test]
    fn a_failing_guard_keeps_only_the_steps_before_it() {
        let p = pipe(json!([
            { "kind": "strip_exif" },
            { "kind": "if_name_contains", "params": { "text": "invoice" } },
            { "kind": "convert", "params": { "to": "jpeg" } }
        ]));
        let c = compile(&p, &builtin(), &facts("holiday")).unwrap();
        assert_eq!(
            (
                c.stopped_at,
                c.main.recipe.format,
                c.main.recipe.strip_metadata
            ),
            (Some(1), None, Some(true))
        );
        assert_eq!(
            compile(&p, &builtin(), &facts("Invoice 3"))
                .unwrap()
                .main
                .recipe
                .format,
            Some(Format::Jpeg)
        );
    }

    #[test]
    fn an_unregistered_step_stops_the_run_with_its_name() {
        let issue = compile(
            &pipe(json!([{ "kind": "teleport" }])),
            &builtin(),
            &facts("a"),
        )
        .unwrap_err();
        assert_eq!(issue.step, 0);
        assert!(issue.message.contains("teleport"));
    }

    #[test]
    fn check_collects_every_issue_and_treats_guards_as_passing() {
        let p = pipe(json!([
            { "kind": "teleport" },
            { "kind": "if_name_contains", "params": { "text": "zzz" } },
            { "kind": "downscale", "params": { "by": "percent", "value": 99 } },
            { "kind": "fork" }
        ]));
        assert_eq!(
            check(&p, &builtin())
                .iter()
                .map(|i| i.step)
                .collect::<Vec<_>>(),
            [0, 2, 3]
        );
    }

    #[test]
    fn every_starter_is_a_valid_image_pipeline() {
        for p in starters() {
            assert!(check(&p, &builtin()).is_empty(), "{}", p.name);
        }
    }

    #[test]
    fn a_zone_caption_shows_the_first_steps() {
        let p = pipe(json!([
            { "kind": "convert", "params": { "to": "webp" } },
            { "kind": "copy", "params": { "to": "~/Out/%f.%e" } },
            { "kind": "strip_exif" }
        ]));
        assert_eq!(
            caption(&p, &builtin(), 2),
            "Convert to WebP · Copy to ~/Out"
        );
        assert_eq!(
            captions(&pipe(json!([{ "kind": "teleport" }])), &builtin()),
            ["teleport (unavailable)"]
        );
    }
}
