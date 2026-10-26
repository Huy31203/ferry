//! Commands the pipeline editor and the Settings pane call (stream F). The
//! rules are `ferry_core::pipeline_plan`, `pipeline_text` and `pipeline_test`;
//! this file reads settings and runs the work off the UI thread.

use std::collections::HashMap;
use std::path::PathBuf;

use chrono::{Local, NaiveDate, NaiveDateTime};
use ferry_core::i18n::t;
use ferry_core::job::JobId;
use ferry_core::optimise::{Level, OptimiseOptions};
use ferry_core::pipeline::Job;
use ferry_core::pipeline_def::{Pipeline, Step, starters};
use ferry_core::pipeline_plan::{Issue, captions, check};
use ferry_core::pipeline_steps::{Media, StepKindView, registry};
use ferry_core::pipeline_test::{StepReport, run};
use ferry_core::pipeline_text::{LineError, parse, print};
use serde::Serialize;
use tauri::{AppHandle, Manager, WebviewWindow};

use crate::state::AppState;
use crate::{jobs, pipe_jobs, pipe_windows, preset_zones};

/// Run `work` on a blocking worker and return its result.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
pub fn pipe_registry() -> Vec<StepKindView> {
    registry().views()
}

#[derive(Serialize)]
pub struct PipeMeta {
    media: Vec<Media>,
    last_run: HashMap<u32, String>,
}

/// `%H:%M` for today, `%Y-%m-%d` before.
fn shown_time(at: NaiveDateTime, today: NaiveDate) -> String {
    if at.date() == today {
        at.format("%H:%M").to_string()
    } else {
        at.format("%Y-%m-%d").to_string()
    }
}

#[tauri::command]
pub fn pipe_meta(app: AppHandle) -> PipeMeta {
    let today = Local::now().date_naive();
    let last_run = app
        .state::<pipe_jobs::PipeState>()
        .last_runs()
        .into_iter()
        .map(|(id, at)| (id, shown_time(at, today)))
        .collect();
    PipeMeta {
        media: registry().media(),
        last_run,
    }
}

#[derive(Serialize)]
pub struct Checked {
    issues: Vec<Issue>,
    captions: Vec<String>,
}

#[tauri::command]
pub fn pipe_check(pipeline: Pipeline) -> Checked {
    Checked {
        issues: check(&pipeline, registry()),
        captions: captions(&pipeline, registry()),
    }
}

#[tauri::command]
pub fn pipe_parse(media: Media, text: String) -> Result<Vec<Step>, Vec<LineError>> {
    parse(media, &text, registry())
}

#[tauri::command]
pub fn pipe_print(media: Media, steps: Vec<Step>) -> String {
    print(&steps, media, registry())
}

#[tauri::command]
pub fn pipe_starters() -> Vec<Pipeline> {
    starters()
}

fn sandbox(app: &AppHandle) -> PathBuf {
    app.state::<AppState>().work_dir.join("pipeline-test")
}

/// Run `pipeline` on the sample at `path`, inside the sandbox folder.
#[tauri::command]
pub async fn pipe_test(
    app: AppHandle,
    pipeline: Pipeline,
    path: String,
) -> Result<Vec<StepReport>, String> {
    blocking(app, move |app| {
        let state = app.state::<AppState>();
        let options = {
            let settings = state.settings.read().unwrap();
            OptimiseOptions {
                compression: settings.images.compression,
                strip_metadata: settings.images.strip_metadata,
                gifsicle: state.gifsicle.clone(),
                adaptive: settings.images.adaptive,
                ..OptimiseOptions::default()
            }
        };
        let home = app.path().home_dir().unwrap_or_default();
        run(
            &pipeline,
            registry(),
            &PathBuf::from(path),
            &sandbox(app),
            &options,
            &home,
            Local::now().date_naive(),
        )
    })
    .await
}

#[tauri::command]
pub fn pipe_reveal_test(app: AppHandle) -> Result<(), String> {
    let dir = sandbox(&app);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    tauri_plugin_opener::open_path(&dir, None::<&str>).map_err(|e| e.to_string())
}

/// Let the user pick a sample image; `None` when the dialog is cancelled.
#[tauri::command]
pub async fn pipe_pick_file(app: AppHandle) -> Result<Option<String>, String> {
    blocking(app, |app| {
        use tauri_plugin_dialog::DialogExt;
        let Some(path) = app
            .dialog()
            .file()
            .set_title(t("pipe.test.pick"))
            .add_filter("Images", &["png", "jpg", "jpeg", "gif", "webp"])
            .blocking_pick_file()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        Ok(Some(path.to_string_lossy().into_owned()))
    })
    .await
}

/// Run a library pipeline on a card that is already showing.
#[tauri::command]
pub async fn pipe_run(app: AppHandle, id: JobId, pipeline: u32) -> Result<(), String> {
    blocking(app, move |app| pipe_jobs::run_on_card(app, &id, pipeline)).await
}

/// Async: building a window inside a sync command deadlocks on Windows.
#[tauri::command(async)]
pub fn pipe_open_editor(app: AppHandle, pipeline: Option<u32>, tab: Option<String>) {
    pipe_windows::open_editor(&app, pipeline, tab.as_deref());
}

// ---- preset zones ----

/// Show the grid from the zone editor; it stays until Done.
#[tauri::command]
pub async fn pipe_zones_show(app: AppHandle) -> Result<(), String> {
    blocking(app, |app| {
        preset_zones::show(app, preset_zones::Mode::Preview, Vec::new());
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn pipe_zones_hide(app: AppHandle) {
    preset_zones::hide(&app);
}

/// The page reports a drag entering (true) or leaving (false) the grid.
#[tauri::command]
pub fn pipe_zones_drag(app: AppHandle, entered: bool) {
    preset_zones::dragging(&app, entered);
}

#[tauri::command]
pub fn pipe_zones_ready(app: AppHandle) -> preset_zones::ZonesView {
    preset_zones::view(&app)
}

#[tauri::command]
pub fn pipe_classify(paths: Vec<String>) -> Vec<Option<Media>> {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    preset_zones::classify(&paths)
}

/// Files dropped on a zone's tile: each runs the zone's pipeline for its media.
#[tauri::command]
pub async fn pipe_zone_drop(app: AppHandle, zone: u32, paths: Vec<String>) -> Result<(), String> {
    blocking(app, move |app| {
        let (settings, has_gifsicle) = {
            let state = app.state::<AppState>();
            (
                state.settings.read().unwrap().clone(),
                state.gifsicle.is_some(),
            )
        };
        for path in paths.into_iter().map(PathBuf::from) {
            if path.is_dir() {
                jobs::notice(app, t("pipe.zones.folder_skipped"));
                continue;
            }
            match Job::file(&path, &settings, has_gifsicle) {
                Ok(job) => {
                    pipe_jobs::start_assigned(
                        app,
                        job,
                        pipe_jobs::Trigger::Zone(zone),
                        Level::Normal,
                        None,
                    );
                }
                Err(reason) => jobs::notice(app, reason),
            }
        }
        preset_zones::hide_after_drop(app);
        Ok(())
    })
    .await
}

#[tauri::command]
pub fn pipe_zone_menu(
    app: AppHandle,
    window: WebviewWindow,
    zone: u32,
    x: f64,
    y: f64,
) -> Result<(), String> {
    preset_zones::popup_menu(&app, &window, zone, x, y)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Datelike;

    #[test]
    fn a_last_run_shows_the_time_today_and_the_date_before() {
        let today = NaiveDate::from_ymd_opt(2026, 10, 23).unwrap();
        let at = |d: u32| today.with_day(d).unwrap().and_hms_opt(9, 5, 0).unwrap();
        assert_eq!(shown_time(at(23), today), "09:05");
        assert_eq!(shown_time(at(22), today), "2026-10-22");
    }
}
