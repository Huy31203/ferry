//! The batch window: its dispatcher work, its flush loop, and its commands.
//! A batch row is an ordinary file job; `jobs::show` hands its updates here.

use std::collections::HashSet;
use std::path::PathBuf;
use std::time::{Duration, Instant};

use ferry_core::batch::{Adjust, Snapshot, Work};
use ferry_core::i18n::{t, tf};
use ferry_core::intake::{ScanOptions, scan};
use ferry_core::job::{JobId, JobUpdate};
use ferry_core::pipeline::Job;
use serde::Serialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{
    AppHandle, DragDropEvent, Emitter, LogicalPosition, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent, Wry,
};
use tracing::{info, warn};

use super::{DropState, forget, job_id, mark_dropped};
use crate::menu::{self, Action};
use crate::state::AppState;
use crate::{compare, jobs};

pub const BATCH: &str = "batch";
/// Row changes reach the page at most this often.
pub const FLUSH_MS: u64 = 100;

const ROW_PREFIX: &str = "drop:row:";

pub fn tray_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, "drop:batch", t("drop.tray.batch"), true, None::<&str>)
}

/// Show the batch window, creating it the first time.
pub fn open(app: &AppHandle) {
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(BATCH) {
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            return;
        }
        match build(&handle) {
            // A tray app is never frontmost, so a new window opens behind other apps without this.
            Ok(window) => {
                let _ = window.set_focus();
            }
            Err(e) => warn!("could not open the batch window: {e}"),
        }
    });
    if let Err(e) = result {
        warn!("could not open the batch window: {e}");
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(app, BATCH, WebviewUrl::App("batch.html".into()))
        .title(t("drop.batch.window_title"))
        .inner_size(960.0, 640.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build()?;
    let handle = app.clone();
    window.on_window_event(move |event| match event {
        WindowEvent::CloseRequested { api, .. } => {
            api.prevent_close();
            close(&handle);
        }
        WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) => {
            add(&handle, paths.clone(), false);
        }
        _ => {}
    });
    Ok(window)
}

/// Add dropped or picked paths: open the window, scan off the main thread, queue the rows.
pub fn add(app: &AppHandle, paths: Vec<PathBuf>, keep: bool) {
    open(app);
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let options = ScanOptions {
            exclude: vec![state.backups_dir.clone(), state.work_dir.clone()],
            own: crate::state::own_outputs(&state.jobs.lock().unwrap()),
            ..ScanOptions::default()
        };
        let found = scan(&paths, &options);
        info!(
            files = found.files.len(),
            unsupported = found.unsupported.len(),
            "batch scan"
        );
        app.state::<DropState>()
            .batch
            .lock()
            .unwrap()
            .add(found, keep);
        wake(&app);
    });
}

/// A batch row's update: true when the batch took it (the stack must not show it).
pub fn claim(app: &AppHandle, update: &JobUpdate) -> bool {
    let Some(state) = app.try_state::<DropState>() else {
        return false;
    };
    let result = state
        .batch
        .lock()
        .unwrap()
        .on_update(update, Instant::now());
    if result == Some(true) {
        wake(app);
    }
    result.is_some()
}

pub fn owns(app: &AppHandle, id: &JobId) -> bool {
    app.try_state::<DropState>()
        .is_some_and(|state| state.batch.lock().unwrap().row_of(id).is_some())
}

/// Ask the dispatcher for more work.
pub fn wake(app: &AppHandle) {
    let _ = app.state::<DropState>().wake.send(());
}

/// Start one piece of work. No lock of the batch is held while a `jobs::` call runs.
pub(super) fn exec(app: &AppHandle, work: Work) {
    let state = app.state::<DropState>();
    let skip = |row: u32, reason: String| state.batch.lock().unwrap().skip(row, reason);
    match work {
        Work::Start {
            row,
            path,
            keep,
            level,
            patch,
        } => {
            let (settings, has_gifsicle) = {
                let app_state = app.state::<AppState>();
                (
                    app_state.settings.read().unwrap().clone(),
                    app_state.gifsicle.is_some(),
                )
            };
            let mut job = match Job::file(&path, &settings, has_gifsicle) {
                Ok(job) => job,
                Err(reason) => return skip(row, reason),
            };
            if let Some(patch) = patch {
                job.edit(patch);
            }
            let id = job.id.clone();
            if !state.batch.lock().unwrap().bind(row, id.clone()) {
                return;
            }
            mark_dropped(app, &id, keep);
            // stream I: a batch is a drop too, so "Copy results of drops" applies
            if !jobs::start_auto_copied(app, job, level) {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                skip(row, tf("error.busy", &[("name", &name)]));
                forget(app, &[id]);
            }
        }
        Work::Edit { row, id, patch } => {
            if let Err(e) = jobs::edit(app, &id, patch) {
                skip(row, e);
            }
        }
        Work::Rerun { row, id } => {
            if let Err(e) = jobs::rerun(app, &id, false) {
                skip(row, e);
            }
        }
    }
}

/// Every `FLUSH_MS`, send the rows that changed and the tally to the window.
pub(super) fn flush_loop(app: &AppHandle) {
    loop {
        std::thread::sleep(Duration::from_millis(FLUSH_MS));
        if app.get_webview_window(BATCH).is_none() {
            continue;
        }
        let flushed = app
            .state::<DropState>()
            .batch
            .lock()
            .unwrap()
            .take_flush(Instant::now());
        let Some((rows, status)) = flushed else {
            continue;
        };
        if !rows.is_empty() {
            let _ = app.emit_to(BATCH, "batch:rows", &rows);
        }
        let _ = app.emit_to(BATCH, "batch:status", &status);
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RowAction {
    Compare,
    Preview,
}

impl RowAction {
    fn key(self) -> &'static str {
        match self {
            RowAction::Compare => "compare",
            RowAction::Preview => "preview",
        }
    }
}

fn row_item_id(id: &JobId, action: RowAction) -> String {
    format!("{ROW_PREFIX}{}:{}", action.key(), id.as_str())
}

fn parse_row_item(item: &str) -> Option<(JobId, RowAction)> {
    let (action, id) = item.strip_prefix(ROW_PREFIX)?.split_once(':')?;
    let action = match action {
        "compare" => RowAction::Compare,
        "preview" => RowAction::Preview,
        _ => return None,
    };
    Some((job_id(id)?, action))
}

#[derive(Clone, Serialize)]
struct OpenRequest {
    id: JobId,
    action: &'static str,
}

/// The batch row menu's own items; the rest of its items are routed by `menu.rs`.
pub(super) fn on_menu(app: &AppHandle, item: &str) {
    if let Some((id, action)) = parse_row_item(item) {
        let _ = app.emit_to(
            BATCH,
            "batch:open",
            OpenRequest {
                id,
                action: action.key(),
            },
        );
    }
}

/// Close the window. An idle batch is cleared with its jobs; a running one carries on hidden.
fn close(app: &AppHandle) {
    let state = app.state::<DropState>();
    let cleared = {
        let mut run = state.batch.lock().unwrap();
        run.is_idle().then(|| {
            let ids = run.clear();
            (ids, run.status(Instant::now()))
        })
    };
    if let Some((ids, status)) = cleared {
        let gone: HashSet<&JobId> = ids.iter().collect();
        app.state::<AppState>()
            .jobs
            .lock()
            .unwrap()
            .retain(|id, entry| !gone.contains(id) || entry.is_busy());
        forget(app, &ids);
        let _ = app.emit_to(BATCH, "batch:status", &status);
    }
    if let Some(window) = app.get_webview_window(BATCH) {
        let _ = window.hide();
    }
}

fn threshold(app: &AppHandle) -> u32 {
    app.state::<AppState>()
        .settings
        .read()
        .unwrap()
        .drop_zone
        .batch_threshold
}

/// Run `work` on a blocking worker.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&AppHandle) -> T + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|e| e.to_string())
}

/// The page is listening: everything it needs to draw.
#[tauri::command]
pub fn batch_ready(app: AppHandle) -> Snapshot {
    let threshold = threshold(&app);
    app.state::<DropState>()
        .batch
        .lock()
        .unwrap()
        .snapshot(Instant::now(), threshold)
}

#[tauri::command]
pub async fn batch_pick_files(app: AppHandle) -> Result<(), String> {
    blocking(app, |app| {
        use tauri_plugin_dialog::DialogExt;
        let picked = app
            .dialog()
            .file()
            .set_title(t("drop.batch.add_files.title"))
            .blocking_pick_files();
        let paths: Vec<PathBuf> = picked
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| p.into_path().ok())
            .collect();
        if !paths.is_empty() {
            add(app, paths, false);
        }
    })
    .await
}

#[tauri::command]
pub async fn batch_pick_folder(app: AppHandle) -> Result<(), String> {
    blocking(app, |app| {
        use tauri_plugin_dialog::DialogExt;
        let picked = app
            .dialog()
            .file()
            .set_title(t("drop.batch.add_folder.title"))
            .blocking_pick_folder();
        if let Some(path) = picked.and_then(|p| p.into_path().ok()) {
            add(app, vec![path], false);
        }
    })
    .await
}

#[tauri::command]
pub fn batch_pause(app: AppHandle) {
    app.state::<DropState>()
        .batch
        .lock()
        .unwrap()
        .pause(Instant::now());
}

#[tauri::command]
pub fn batch_resume(app: AppHandle) {
    app.state::<DropState>()
        .batch
        .lock()
        .unwrap()
        .resume(Instant::now());
    wake(&app);
}

#[tauri::command]
pub async fn batch_cancel_all(app: AppHandle) -> Result<(), String> {
    blocking(app, |app| {
        let state = app.state::<DropState>();
        let ids = state.batch.lock().unwrap().cancel_all();
        for id in ids {
            jobs::cancel(app, &id);
            state.batch.lock().unwrap().cancelled(&id);
        }
    })
    .await
}

#[tauri::command]
pub fn batch_set_adjust(app: AppHandle, adjust: Adjust) {
    app.state::<DropState>()
        .batch
        .lock()
        .unwrap()
        .set_adjust(adjust);
}

#[tauri::command]
pub fn batch_rerun(app: AppHandle, adjust: Adjust) {
    {
        let state = app.state::<DropState>();
        let mut run = state.batch.lock().unwrap();
        run.set_adjust(adjust);
        run.rerun(Instant::now());
    }
    wake(&app);
}

#[tauri::command]
pub async fn batch_rollback(app: AppHandle) -> Result<(), String> {
    blocking(app, |app| {
        let state = app.state::<DropState>();
        let targets = {
            let mut run = state.batch.lock().unwrap();
            let targets = run.rollback_targets();
            run.set_rolling(Some((0, targets.len() as u32)));
            targets
        };
        let n = targets.len() as u32;
        for (i, id) in targets.iter().enumerate() {
            if let Err(e) = jobs::restore(app, id) {
                warn!("batch roll back: {e}");
            }
            state
                .batch
                .lock()
                .unwrap()
                .set_rolling(Some((i as u32 + 1, n)));
        }
        state.batch.lock().unwrap().set_rolling(None);
    })
    .await
}

#[tauri::command]
pub fn batch_reveal(app: AppHandle) -> Result<(), String> {
    let first = app
        .state::<DropState>()
        .batch
        .lock()
        .unwrap()
        .first_output();
    let path = first.ok_or_else(|| t("compare.file_gone"))?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn batch_close(app: AppHandle) {
    close(&app);
}

/// The row's context menu at `(x, y)`, in the batch page's CSS pixels.
#[tauri::command]
pub fn batch_row_menu(
    app: AppHandle,
    window: WebviewWindow,
    id: JobId,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let card = compare::card(&app, &id)?;
    let menu = row_menu(&app, &card).map_err(|e| e.to_string())?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn row_menu(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Menu<Wry>> {
    let item = |action: Action, label: String| {
        MenuItem::with_id(
            app,
            menu::item_id(&card.id, action),
            label,
            menu::enabled(card, action),
            None::<&str>,
        )
    };
    let row_item = |action: RowAction, label: String, enabled: bool| {
        MenuItem::with_id(
            app,
            row_item_id(&card.id, action),
            label,
            enabled,
            None::<&str>,
        )
    };
    let has_file = card.output.is_some();
    let reveal = if cfg!(target_os = "macos") {
        "card.reveal.mac"
    } else {
        "card.reveal.windows"
    };
    let scales = (1..=9)
        .rev()
        .map(|n| {
            let percent = n * 10;
            let label = tf("menu.percent", &[("n", &percent.to_string())]);
            item(Action::Scale(percent), label)
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let converts = menu::convert_formats(card)
        .into_iter()
        .map(|f| item(Action::Convert(f), f.label().into()))
        .collect::<tauri::Result<Vec<_>>>()?;
    let submenu = |label: String, items: &[MenuItem<Wry>]| {
        let refs: Vec<&dyn IsMenuItem<Wry>> =
            items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
        Submenu::with_items(app, label, card.can_edit, &refs)
    };
    let separator = || PredefinedMenuItem::separator(app);
    Menu::with_items(
        app,
        &[
            &item(Action::Open, t("menu.open"))?,
            &item(Action::Reveal, t(reveal))?,
            &separator()?,
            &item(Action::Copy, t("menu.copy"))?,
            &item(Action::CopyPath, t("menu.copy_path"))?,
            &separator()?,
            &row_item(RowAction::Compare, t("card.compare"), card.can_edit)?,
            &row_item(RowAction::Preview, t("menu.preview"), has_file)?,
            &separator()?,
            &item(Action::Rerun, t("card.reoptimise"))?,
            &item(Action::Aggressive, t("card.aggressive"))?,
            &submenu(t("menu.downscale"), &scales)?,
            &submenu(t("menu.convert"), &converts)?,
            &separator()?,
            &item(Action::Restore, t("card.restore"))?,
        ],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_row_menu_id_names_its_card_and_action() {
        let id = JobId::generate();
        for action in [RowAction::Compare, RowAction::Preview] {
            let item = row_item_id(&id, action);
            assert_eq!(item, format!("drop:row:{}:{}", action.key(), id.as_str()));
            assert_eq!(parse_row_item(&item), Some((id.clone(), action)));
        }
        assert_eq!(parse_row_item("card:x:open"), None);
        assert_eq!(parse_row_item("drop:row:other:x"), None);
    }
}
