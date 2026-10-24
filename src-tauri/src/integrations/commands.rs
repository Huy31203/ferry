//! Commands the pages call for sharing.

use ferry_core::i18n::{t, tf};
use ferry_core::integrations::Anchor;
use ferry_core::job::JobId;
use ferry_core::settings::AppRef;
use tauri::{AppHandle, WebviewWindow};

use super::actions;
use crate::{jobs, platform};

/// Open the system share UI next to a card. The card's page measures `anchor`
/// (its box) and calls this from the menu's `card:action` `share`. Sync, so it
/// runs on the main thread, where AppKit and the Windows share UI need it.
#[tauri::command]
pub fn card_share(
    app: AppHandle,
    window: WebviewWindow,
    id: JobId,
    anchor: Anchor,
) -> Result<(), String> {
    let path = actions::output(&app, &id)?;
    platform::share::share(&window, vec![path.clone()], &anchor).inspect_err(|_| {
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        jobs::notice(&app, tf("share.error.share", &[("name", &name)]));
    })
}

/// Settings → Sharing: choose an app. `None` when the dialog is cancelled.
#[tauri::command]
pub async fn share_pick_app(app: AppHandle) -> Result<Option<AppRef>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let dialog = app.dialog().file().set_title(t("share.pick_app"));
        #[cfg(target_os = "macos")]
        let dialog = dialog
            .add_filter("Applications", &["app"])
            .set_directory("/Applications");
        #[cfg(windows)]
        let dialog = dialog.add_filter("Programs", &["exe"]);
        let Some(path) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        platform::apps::app_ref(&path)
            .map(Some)
            .ok_or_else(|| t("error.pick_app"))
    })
    .await
    .map_err(|e| e.to_string())?
}
