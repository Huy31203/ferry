//! Commands the stack page calls (spec §5.5). File and clipboard work runs on
//! a blocking worker, never on the main thread.

use ferry_core::job::{JobId, JobUpdate};
use ferry_core::settings::{CardSize, Corner};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::{jobs, logging};

/// Run `work` on a blocking worker and return its result.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct StackConfig {
    corner: Corner,
    card_size: CardSize,
    auto_hide_secs: u32,
}

/// The stack page is listening: hand it its settings and any cards sent before.
#[tauri::command]
pub fn stack_ready(app: AppHandle) -> (StackConfig, Vec<JobUpdate>) {
    let state = app.state::<AppState>();
    let floating = state.settings.read().unwrap().floating.clone();
    let mut inbox = state.stack.lock().unwrap();
    inbox.ready = true;
    let config = StackConfig {
        corner: floating.corner,
        card_size: floating.card_size,
        auto_hide_secs: floating.auto_hide_secs,
    };
    (config, std::mem::take(&mut inbox.pending))
}

#[tauri::command]
pub async fn card_copy(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |app| jobs::copy(app, &id)).await
}

#[tauri::command]
pub async fn card_restore(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |app| jobs::restore(app, &id)).await
}

/// Retry, re-optimise (`aggressive: false`) or "Try aggressive".
#[tauri::command]
pub fn card_rerun(app: AppHandle, id: JobId, aggressive: bool) -> Result<(), String> {
    jobs::rerun(&app, &id, aggressive)
}

#[tauri::command]
pub fn card_cancel(app: AppHandle, id: JobId) {
    jobs::cancel(&app, &id);
}

#[tauri::command]
pub fn card_reveal(app: AppHandle, id: JobId) -> Result<(), String> {
    let path = jobs::output(&app, &id).ok_or("the file is gone")?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// An Error card's "Details": the newest log file in the file manager.
#[tauri::command]
pub fn open_log(app: AppHandle) -> Result<(), String> {
    let dir = app.state::<AppState>().log_dir.clone();
    match logging::latest(&dir) {
        Some(file) => tauri_plugin_opener::reveal_item_in_dir(file),
        None => tauri_plugin_opener::open_path(&dir, None::<&str>),
    }
    .map_err(|e| e.to_string())
}

/// "Clear all" also ends an "accumulate results" collection (§5.4).
#[tauri::command]
pub fn clear_all(app: AppHandle) {
    app.state::<AppState>()
        .clipboard
        .lock()
        .unwrap()
        .accumulated
        .clear();
}
