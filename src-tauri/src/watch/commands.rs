//! What the Watched folders pane asks of the app.

use std::path::Path;

use ferry_core::i18n::t;
use ferry_core::settings::{contract_home, expand_home};
use ferry_core::watch::plan::{FolderProblem, WatchStatus, check_folder};
use tauri::{AppHandle, Manager};

use super::hooks::{self, Support};

#[tauri::command]
pub fn watch_status(app: AppHandle) -> WatchStatus {
    super::status(&app)
}

#[tauri::command]
pub fn watch_support() -> Support {
    hooks::support()
}

/// May the user watch this folder? `Ok` is the path as settings store it
/// (`~` for the home folder).
#[tauri::command]
pub fn watch_check_folder(app: AppHandle, path: String) -> Result<String, String> {
    let home = app.path().home_dir().map_err(|e| e.to_string())?;
    check(&app, &expand_home(&path, &home), &home)
}

/// A folder picker, then the same check. `None` when the dialog is cancelled.
#[tauri::command]
pub async fn watch_pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let Some(picked) = app
            .dialog()
            .file()
            .set_title(t("watch.pick_folder"))
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let dir = picked.into_path().map_err(|e| e.to_string())?;
        let home = app.path().home_dir().map_err(|e| e.to_string())?;
        check(&app, &dir, &home).map(Some)
    })
    .await
    .map_err(|e| e.to_string())?
}

fn check(app: &AppHandle, dir: &Path, home: &Path) -> Result<String, String> {
    match check_folder(dir, &super::excluded(app)) {
        Ok(()) => Ok(contract_home(dir, home)),
        Err(FolderProblem::NotAFolder) => Err(t("watch.error.not_folder")),
        Err(FolderProblem::OwnFolder) => Err(t("watch.error.own_folder")),
    }
}
