//! Audio on the app side: saving a card's cover art and the start-up check for
//! Apple's AAC encoder. The work itself is `ferry_core::audio`.

use std::path::PathBuf;

use ferry_core::job::JobId;
use tauri::AppHandle;
use tracing::info;

use crate::{jobs, state::relock};

/// Save the card's cover beside its result and show it in Finder/Explorer.
pub fn extract_cover(app: &AppHandle, id: &JobId) -> Result<PathBuf, String> {
    let entry = jobs::idle_entry(app, id)?;
    let path = relock(&entry.job).extract_cover()?;
    tauri_plugin_opener::reveal_item_in_dir(&path).map_err(|e| e.to_string())?;
    Ok(path)
}

#[tauri::command]
pub async fn card_extract_cover(app: AppHandle, id: JobId) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || extract_cover(&app, &id).map(drop))
        .await
        .map_err(|e| e.to_string())?
}

/// Ask once, off the main thread, whether the sidecar offers Apple's AAC
/// encoder. A job that starts first waits for the same answer.
pub fn startup() {
    let started = std::thread::Builder::new()
        .name("ferry-audio-probe".into())
        .spawn(|| {
            let aac = ferry_core::audio::tool::aac_encoder();
            info!(?aac, "audio AAC encoder");
        });
    if let Err(e) = started {
        tracing::warn!("could not start the audio probe: {e}");
    }
}
