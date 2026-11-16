//! A drop becomes cards or a batch: Ferry's own results are left alone, a
//! folder or a big drop goes to the batch window, a few files get a card each.

use std::path::PathBuf;

use ferry_core::i18n::{t, tf};
use ferry_core::intake::{Route, classify_file, route, summarise, without_own};
use ferry_core::job::JobId;
use ferry_core::optimise::Level;
use ferry_core::pipeline::Job;
use tauri::{AppHandle, Manager};

use super::{batch, forget, mark_dropped};
use crate::jobs;
use crate::state::AppState;

fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

fn unsupported(path: &std::path::Path) -> String {
    match path.extension() {
        Some(ext) => tf(
            "drop.zone.unsupported",
            &[("ext", &ext.to_string_lossy().to_lowercase())],
        ),
        None => t("drop.zone.unsupported_any"),
    }
}

/// Drop own results (with a notice each); what is left is new work.
fn without_own_results(app: &AppHandle, paths: &[PathBuf]) -> Vec<PathBuf> {
    let state = app.state::<AppState>();
    let outputs: Vec<PathBuf> = state
        .jobs
        .lock()
        .unwrap()
        .values()
        .map(|entry| entry.output())
        .collect();
    let rest = without_own(paths, &outputs);
    for path in paths.iter().filter(|p| !rest.contains(p)) {
        let reason = tf("drop.error.own_result", &[("name", &file_name(path))]);
        jobs::notice(app, reason);
    }
    rest
}

/// Handle a drop. Reads files, so call it off the main thread.
pub fn accept(app: &AppHandle, paths: Vec<PathBuf>, keep: bool) {
    let rest = without_own_results(app, &paths);
    if rest.is_empty() {
        return;
    }
    let state = app.state::<AppState>();
    let threshold = state.settings.read().unwrap().drop_zone.batch_threshold;
    if route(&summarise(&rest, 0), threshold) == Route::Batch {
        batch::add(app, rest, keep);
        return;
    }
    start_cards(app, rest, keep);
}

/// Start a card for each path, with no batch window (the shelf's "Optimise all").
/// Returns the jobs started with their source.
pub fn accept_paths(app: &AppHandle, paths: Vec<PathBuf>, keep: bool) -> Vec<(JobId, PathBuf)> {
    let rest = without_own_results(app, &paths);
    start_cards(app, rest, keep)
}

fn start_cards(app: &AppHandle, rest: Vec<PathBuf>, keep: bool) -> Vec<(JobId, PathBuf)> {
    let state = app.state::<AppState>();
    let (settings, has_gifsicle) = (
        state.settings.read().unwrap().clone(),
        state.gifsicle.is_some(),
    );
    let mut started = Vec::new();
    for path in rest {
        if !matches!(classify_file(&path), Ok(Some(_))) {
            jobs::notice(app, unsupported(&path));
            continue;
        }
        match Job::file(&path, &settings, has_gifsicle) {
            Err(reason) => jobs::notice(app, reason),
            Ok(job) => {
                let id: JobId = job.id.clone();
                mark_dropped(app, &id, keep);
                if !crate::pipe_jobs::start_assigned(
                    app,
                    job,
                    crate::pipe_jobs::Trigger::DropZone, // stream F: pipelines
                    Level::Normal,
                    None,
                ) {
                    jobs::notice(app, tf("error.busy", &[("name", &file_name(&path))]));
                    forget(app, &[id]);
                } else {
                    started.push((id, path.clone()));
                }
            }
        }
    }
    started
}
