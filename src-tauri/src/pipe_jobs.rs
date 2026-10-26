//! Pipelines in the app (stream F): which pipeline a trigger runs, what a plan
//! leaves for a job's next finished run (copies, forks, where results go), and
//! the hooks `jobs.rs` calls. The rules are `ferry_core::pipeline_plan`; the
//! job runner does the work.

use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use chrono::{Local, NaiveDateTime};
use ferry_core::edit::{Edits, EditsPatch};
use ferry_core::i18n::{t, tf};
use ferry_core::job::{JobId, JobState, JobUpdate, Source};
use ferry_core::optimise::Level;
use ferry_core::pipeline::{Finished, Job, RunOptions};
use ferry_core::pipeline_after::copy_to;
use ferry_core::pipeline_plan::{captions, compile};
use ferry_core::pipeline_steps::{After, Branch, Facts, Media, registry};
use ferry_core::settings::{Output, OutputRule};
use tauri::{AppHandle, Manager};
use tracing::warn;

use crate::jobs;
use crate::state::{AppState, Entry, relock};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Trigger {
    Clipboard,
    DropZone,
    Zone(u32),
    /// A watched folder's pipeline.
    Pipeline(u32),
}

/// A `source_seq` that never matches the clipboard, so a fork twin's first run leaves the clipboard
/// to the main result (`ClipboardLog::may_replace` is false for a copy that never happened).
pub const NEVER_ON_CLIPBOARD: u64 = u64::MAX;

/// What a pipeline left for a job's next finished run.
#[derive(Debug, Clone)]
pub struct Pending {
    /// The recipe this plan set (level ignored when comparing).
    pub recipe: Edits,
    pub after: Vec<After>,
    pub forks: Vec<Branch>,
    /// "Step 2 of 4 · Convert to WebP"
    pub step_text: Option<String>,
    pub hide: bool,
    pub pipeline: u32,
}

#[derive(Default)]
struct Inner {
    pending: HashMap<JobId, Pending>,
    overrides: HashMap<JobId, (Option<OutputRule>, Option<OutputRule>)>,
    hidden: HashSet<JobId>,
    produced_by: HashMap<JobId, u32>,
    last_run: HashMap<u32, NaiveDateTime>,
}

#[derive(Default)]
pub struct PipeState {
    inner: Mutex<Inner>,
}

fn same_recipe(a: &Edits, b: &Edits) -> bool {
    a == &Edits {
        level: a.level,
        ..b.clone()
    }
}

impl PipeState {
    fn inner(&self) -> std::sync::MutexGuard<'_, Inner> {
        relock(&self.inner)
    }

    pub fn register(
        &self,
        id: &JobId,
        pending: Pending,
        optimised: Option<OutputRule>,
        converted: Option<OutputRule>,
    ) {
        let mut s = self.inner();
        if optimised.is_some() || converted.is_some() {
            s.overrides.insert(id.clone(), (optimised, converted));
        }
        if pending.hide {
            s.hidden.insert(id.clone());
        }
        s.produced_by.insert(id.clone(), pending.pipeline);
        s.pending.insert(id.clone(), pending);
    }

    /// The pending plan for `id` if its recipe equals `edits` (level ignored); a stale one is dropped either way.
    pub fn take_for(&self, id: &JobId, edits: &Edits) -> Option<Pending> {
        self.inner()
            .pending
            .remove(id)
            .filter(|p| same_recipe(edits, &p.recipe))
    }

    pub fn drop_pending(&self, id: &JobId) {
        self.inner().pending.remove(id);
    }

    pub fn step_text(&self, id: &JobId) -> Option<String> {
        self.inner().pending.get(id)?.step_text.clone()
    }

    pub fn hidden(&self, id: &JobId) -> bool {
        self.inner().hidden.contains(id)
    }

    pub fn override_output(&self, id: &JobId, output: &mut Output) {
        let s = self.inner();
        let Some((optimised, converted)) = s.overrides.get(id) else {
            return;
        };
        if let Some(rule) = optimised {
            output.optimised = rule.clone();
        }
        if let Some(rule) = converted {
            output.converted = rule.clone();
        }
    }

    pub fn producer(&self, id: &JobId) -> Option<u32> {
        self.inner().produced_by.get(id).copied()
    }

    pub fn ran(&self, pipeline: u32, at: NaiveDateTime) {
        self.inner().last_run.insert(pipeline, at);
    }

    pub fn last_runs(&self) -> HashMap<u32, NaiveDateTime> {
        self.inner().last_run.clone()
    }

    /// Forget every job not in `live`.
    pub fn prune(&self, live: &HashSet<JobId>) {
        let mut s = self.inner();
        s.pending.retain(|id, _| live.contains(id));
        s.overrides.retain(|id, _| live.contains(id));
        s.hidden.retain(|id| live.contains(id));
        s.produced_by.retain(|id, _| live.contains(id));
    }
}

/// Image for every format that has no pipeline media of its own.
pub fn media_of(job: &Job) -> Media {
    if job.is_video() {
        Media::Video
    } else if job.is_audio() {
        Media::Audio
    } else if job.is_pdf() {
        Media::Pdf
    } else {
        Media::Image
    }
}

fn media_name(media: Media) -> String {
    let key = match media {
        Media::Video => "video",
        Media::Audio => "audio",
        Media::Pdf => "pdf",
        Media::Image | Media::Other => "image",
    };
    t(&format!("pipe.media.one.{key}"))
}

/// "Step 2 of 4 · Convert to WebP", from the branch's last changing step.
pub fn step_text(branch: &Branch, total: usize) -> Option<String> {
    let (n, caption) = branch.last_change.as_ref()?;
    Some(tf(
        "pipe.card.step",
        &[
            ("n", &n.to_string()),
            ("total", &total.to_string()),
            ("step", caption),
        ],
    ))
}

fn higher(a: Level, b: Level) -> Level {
    if a == Level::Aggressive || b == Level::Aggressive {
        Level::Aggressive
    } else {
        Level::Normal
    }
}

fn prune_to_live(app: &AppHandle) {
    let live: HashSet<JobId> = app
        .state::<AppState>()
        .jobs
        .lock()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    app.state::<PipeState>().prune(&live);
}

fn pending_of(recipe: Edits, branch: &Branch, total: usize, hide: bool, pipeline: u32) -> Pending {
    Pending {
        recipe,
        after: branch.after.clone(),
        forks: Vec::new(),
        step_text: step_text(branch, total),
        hide,
        pipeline,
    }
}

/// Start `job` the way `trigger` says: through its assigned pipeline, or as
/// the plain job it would have been.
pub fn start_assigned(
    app: &AppHandle,
    mut job: Job,
    trigger: Trigger,
    level: Level,
    source_seq: Option<u64>,
) -> bool {
    let media = media_of(&job);
    let (library, name) = {
        let settings = app.state::<AppState>().settings.read().unwrap().clone();
        let p = &settings.pipelines;
        let (id, name) = match trigger {
            Trigger::Clipboard => (p.clipboard.get(media), String::new()),
            Trigger::DropZone => (p.drop_zone.get(media), String::new()),
            Trigger::Zone(z) => {
                let zone = p.zone(z);
                (
                    zone.and_then(|z| z.pipelines.get(media)),
                    zone.map(|z| z.name.clone()).unwrap_or_default(),
                )
            }
            Trigger::Pipeline(id) => (Some(id), String::new()),
        };
        (
            id.and_then(|id| p.get(id))
                .filter(|p| p.media == media)
                .cloned(),
            name,
        )
    };
    let Some(pipeline) = library else {
        match trigger {
            Trigger::Zone(_) => {
                jobs::notice(
                    app,
                    tf(
                        "pipe.zone.no_pipeline",
                        &[("zone", &name), ("media", &media_name(media))],
                    ),
                );
                return false;
            }
            Trigger::Pipeline(id) => {
                warn!(id, "the pipeline to run is missing or of another media")
            }
            _ => {}
        }
        return launch(app, job, trigger, level, source_seq);
    };
    let facts = Facts {
        stem: job.name.clone(),
        ext: job.ext.clone(),
        media,
    };
    let compiled = match compile(&pipeline, registry(), &facts) {
        Ok(c) => c,
        Err(issue) => {
            jobs::notice(
                app,
                tf(
                    "pipe.error.cannot_run",
                    &[("name", &pipeline.name), ("why", &issue.message)],
                ),
            );
            return false;
        }
    };
    job.edit(EditsPatch::from_recipe(&compiled.main.recipe));
    let level = higher(level, job.level);
    prune_to_live(app);
    let mut pending = pending_of(
        job.edits.clone(),
        &compiled.main,
        compiled.total,
        compiled.hide_result,
        pipeline.id,
    );
    pending.forks = compiled.forks;
    app.state::<PipeState>().register(
        &job.id,
        pending,
        compiled.main.optimised,
        compiled.main.converted,
    );
    launch(app, job, trigger, level, source_seq)
}

/// The drop zone and watched folders copy their results as a drop does; the rest start plainly.
fn launch(
    app: &AppHandle,
    job: Job,
    trigger: Trigger,
    level: Level,
    source_seq: Option<u64>,
) -> bool {
    if matches!(trigger, Trigger::DropZone | Trigger::Pipeline(_)) {
        jobs::start_auto_copied(app, job, level)
    } else {
        jobs::start(app, job, level, source_seq)
    }
}

/// Run `pipeline` on a card that is already showing, from its original.
pub fn run_on_card(app: &AppHandle, id: &JobId, pipeline: u32) -> Result<(), String> {
    let entry = jobs::idle_entry(app, id)?;
    let last = entry.last.lock().unwrap().clone();
    if !last.can_edit {
        return Err(last.edit_hint.unwrap_or_else(|| t("card.no_original")));
    }
    let found = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .pipelines
        .get(pipeline)
        .cloned();
    let (facts, current) = {
        let job = relock(&entry.job);
        let media = media_of(&job);
        (
            Facts {
                stem: job.name.clone(),
                ext: job.ext.clone(),
                media,
            },
            media,
        )
    };
    let pipeline = found
        .filter(|p| p.media == current)
        .ok_or_else(|| t("pipe.error.missing"))?;
    let compiled = compile(&pipeline, registry(), &facts).map_err(|issue| {
        tf(
            "pipe.error.cannot_run",
            &[("name", &pipeline.name), ("why", &issue.message)],
        )
    })?;
    let patch = EditsPatch::from_recipe(&compiled.main.recipe);
    let recipe = relock(&entry.job).recipe_for(patch.clone());
    prune_to_live(app);
    let mut pending = pending_of(
        recipe,
        &compiled.main,
        compiled.total,
        compiled.hide_result,
        pipeline.id,
    );
    pending.forks = compiled.forks;
    let pipe = app.state::<PipeState>();
    pipe.register(
        id,
        pending,
        compiled.main.optimised,
        compiled.main.converted,
    );
    if let Err(e) = jobs::edit(app, id, patch) {
        pipe.drop_pending(id);
        return Err(e);
    }
    Ok(())
}

// ---- hooks called from jobs.rs ----

/// A hidden pipeline result shows no card, unless it failed.
pub fn hidden(app: &AppHandle, update: &JobUpdate) -> bool {
    app.state::<PipeState>().hidden(&update.id) && !matches!(update.state, JobState::Error { .. })
}

/// The run's output rules, with the pipeline's where it named them. A job uses
/// one of the three, so each is overridden.
pub fn options(app: &AppHandle, id: &JobId, mut o: RunOptions) -> RunOptions {
    let pipe = app.state::<PipeState>();
    pipe.override_output(id, &mut o.output);
    pipe.override_output(id, &mut o.video.output);
    pipe.override_output(id, &mut o.audio.settings.output);
    pipe.override_output(id, &mut o.pdf_output); // stream E
    o
}

/// The card says which pipeline step is running while a plan is pending.
pub fn label(app: &AppHandle, mut update: JobUpdate) -> JobUpdate {
    if let JobState::Optimising { step, .. } = &mut update.state
        && let Some(text) = app.state::<PipeState>().step_text(&update.id)
    {
        *step = text;
    }
    update
}

/// What follows a finished run: copies, then a twin job for each fork.
pub fn after_run(
    app: &AppHandle,
    entry: &Arc<Entry>,
    finished: Option<Finished>,
) -> Option<Finished> {
    let pipe = app.state::<PipeState>();
    let id = relock(&entry.job).id.clone();
    let Some(mut f) = finished else {
        pipe.drop_pending(&id);
        return None;
    };
    let edits = relock(&entry.job).edits.clone();
    let Some(plan) = pipe.take_for(&id, &edits) else {
        return Some(f);
    };
    if matches!(f.update.state, JobState::Error { .. }) {
        return Some(f);
    }
    let pipeline = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .pipelines
        .get(plan.pipeline)
        .cloned();
    let total = pipeline.as_ref().map_or(0, |p| p.steps.len());
    let names = pipeline
        .as_ref()
        .map(|p| captions(p, registry()))
        .unwrap_or_default();
    let home = std::env::home_dir().unwrap_or_default();
    let today = Local::now().date_naive();
    for After::CopyTo { template, step } in &plan.after {
        let text = tf(
            "pipe.card.step",
            &[
                ("n", &(step + 1).to_string()),
                ("total", &total.to_string()),
                ("step", names.get(*step).map_or("", String::as_str)),
            ],
        );
        jobs::show(
            app,
            JobUpdate {
                state: JobState::Optimising {
                    progress: 1.0,
                    step: text,
                },
                ..f.update.clone()
            },
        );
        let dims = f
            .update
            .dims_after
            .or(f.update.dims_before)
            .unwrap_or_default();
        if let Err(e) = copy_to(&entry.output(), template, dims, &home, today) {
            warn!(job = id.as_str(), "pipeline copy failed: {e}");
            f.update.note = Some(tf("pipe.note.copy_failed", &[("why", &e)]));
        }
    }
    let (level, source) = {
        let job = relock(&entry.job);
        (job.level, job.source)
    };
    for branch in &plan.forks {
        let mut twin = relock(&entry.job).fork();
        twin.edit(EditsPatch::from_recipe(&branch.recipe));
        let twin_level = higher(level, twin.level);
        let pending = pending_of(twin.edits.clone(), branch, total, plan.hide, plan.pipeline);
        pipe.register(
            &twin.id,
            pending,
            branch.optimised.clone(),
            branch.converted.clone(),
        );
        let seq = (source == Source::Clipboard).then_some(NEVER_ON_CLIPBOARD);
        let twin_id = twin.id.clone();
        if !jobs::start(app, twin, twin_level, seq) {
            pipe.drop_pending(&twin_id);
            warn!(job = id.as_str(), "a fork's result could not start");
        }
    }
    pipe.ran(plan.pipeline, Local::now().naive_local());
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::optimise::Level;
    use ferry_core::settings::Location;
    use ferry_core::sniff::Format;

    fn id(s: &str) -> JobId {
        serde_json::from_value(serde_json::Value::String(s.into())).unwrap()
    }

    fn pending(recipe: Edits) -> Pending {
        Pending {
            recipe,
            after: vec![],
            forks: vec![],
            step_text: Some("Step 1 of 1 · x".into()),
            hide: true,
            pipeline: 7,
        }
    }

    #[test]
    fn a_pending_plan_only_applies_to_the_recipe_it_set() {
        let s = PipeState::default();
        let webp = Edits {
            format: Some(Format::WebP),
            ..Edits::default()
        };
        s.register(&id("1-a"), pending(webp.clone()), None, None);
        assert!(
            s.take_for(&id("1-a"), &Edits::default()).is_none(),
            "a cancel put the old recipe back"
        );
        assert!(
            s.take_for(&id("1-a"), &webp).is_none(),
            "and the stale plan is gone"
        );
        s.register(&id("1-a"), pending(webp.clone()), None, None);
        let aggressive = Edits {
            level: Level::Aggressive,
            ..webp.clone()
        };
        assert!(
            s.take_for(&id("1-a"), &aggressive).is_some(),
            "the level is the run's, not the plan's"
        );
    }

    #[test]
    fn overrides_hidden_and_producer_last_for_the_jobs_life() {
        let s = PipeState::default();
        let rule = OutputRule {
            location: Location::Folder,
            folder: "~/X".into(),
            ..OutputRule::default()
        };
        s.register(
            &id("1-b"),
            pending(Edits::default()),
            Some(rule.clone()),
            None,
        );
        let _ = s.take_for(&id("1-b"), &Edits::default());
        let mut out = Output::default();
        s.override_output(&id("1-b"), &mut out);
        assert_eq!(
            (out.optimised, s.hidden(&id("1-b")), s.producer(&id("1-b"))),
            (rule, true, Some(7))
        );
        s.prune(&HashSet::new());
        assert!(!s.hidden(&id("1-b")) && s.producer(&id("1-b")).is_none());
    }

    #[test]
    fn the_step_text_names_the_last_change() {
        let b = Branch {
            last_change: Some((2, "Convert to WebP".into())),
            ..Branch::default()
        };
        assert_eq!(
            step_text(&b, 4).as_deref(),
            Some("Step 2 of 4 · Convert to WebP")
        );
        assert_eq!(step_text(&Branch::default(), 4), None);
    }

    #[test]
    fn a_failed_copy_becomes_a_note() {
        assert_eq!(
            tf("pipe.note.copy_failed", &[("why", "Can't write to ~/X")]),
            "Couldn't copy: Can't write to ~/X"
        );
    }
}
