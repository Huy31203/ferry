//! Commands the pages call for sharing.

use ferry_core::i18n::tf;
use ferry_core::integrations::Anchor;
use ferry_core::job::JobId;
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
