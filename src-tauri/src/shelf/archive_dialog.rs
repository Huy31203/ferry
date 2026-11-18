//! The "Advanced…" archive dialog (shelf archive spec §3): a normal, focusable
//! window, because the shelf panel is non-key and can't take a password.

use std::path::PathBuf;
use std::sync::Mutex;

use ferry_core::archive::{self as core, Encryption, Options};
use ferry_core::i18n::{t, tf};
use serde::{Deserialize, Serialize};
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};

use super::archive::{self, items};
use crate::state::AppState;

pub const ARCHIVE: &str = "archive";

/// Remembered for the session: the last level and threads (never the password).
static LAST: Mutex<Option<(u32, usize)>> = Mutex::new(None);

#[derive(Serialize)]
pub struct DialogView {
    pub folder: String,
    pub name: String,
    pub count: usize,
    pub cores: usize,
    pub level: u32,
    pub threads: usize,
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub folder: String,
    pub name: String,
    pub level: u32,
    pub threads: usize,
    pub password: Option<String>,
    /// "aes256" or "zipcrypto".
    pub encryption: String,
    pub split: Option<String>,
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, ARCHIVE, WebviewUrl::App("archive.html".into()))
        .title(t("archive.title"))
        .inner_size(560.0, 400.0)
        .resizable(false)
        .content_protected(!show_in_screenshots)
        .visible(false)
        .build()?;
    // The close button hides the window, so the page stays loaded.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            archive_close(handle.clone());
        }
    });
    Ok(window)
}

pub fn open(app: &AppHandle) {
    let window = match app.get_webview_window(ARCHIVE) {
        Some(w) => w,
        None => match build(app) {
            Ok(w) => w,
            Err(e) => {
                crate::jobs::notice(app, e.to_string());
                return;
            }
        },
    };
    let _ = app.emit_to(ARCHIVE, "archive:open", ());
    let _ = window.center();
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn archive_view(app: AppHandle) -> DialogView {
    let items = items(&app);
    let (level, threads) = LAST.lock().unwrap().unwrap_or((5, archive::cores()));
    DialogView {
        folder: items
            .first()
            .and_then(|p| p.parent())
            .map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
        name: core::default_name(&items),
        count: items.len(),
        cores: archive::cores(),
        level,
        threads,
    }
}

#[tauri::command]
pub async fn archive_estimate(app: AppHandle, level: u32, threads: usize) -> String {
    tauri::async_runtime::spawn_blocking(move || {
        let items = items(&app);
        let totals = archive::cached(&items).unwrap_or_else(|| archive::walk(&items));
        archive::eta_label(archive::estimate(&app, totals, level, threads))
    })
    .await
    .unwrap_or_default()
}

#[tauri::command]
pub async fn archive_pick(
    app: AppHandle,
    folder: String,
    name: String,
) -> Option<(String, String)> {
    use tauri_plugin_dialog::DialogExt;
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .set_directory(folder)
            .set_file_name(name)
            .add_filter("Zip", &["zip"])
            .blocking_save_file()?;
        let path = path.into_path().ok()?;
        Some((
            path.parent()?.to_string_lossy().into_owned(),
            path.file_name()?.to_string_lossy().into_owned(),
        ))
    })
    .await
    .ok()
    .flatten()
}

#[tauri::command]
pub fn archive_start(app: AppHandle, req: StartRequest) -> Result<(), String> {
    let name = req.name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(t("archive.error.name"));
    }
    let name = if name.to_lowercase().ends_with(".zip") {
        name.to_string()
    } else {
        format!("{name}.zip")
    };
    let split = match req
        .split
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
    {
        Some(s) => Some(core::parse_size(s).ok_or_else(|| t("archive.error.split"))?),
        None => None,
    };
    let out = PathBuf::from(&req.folder).join(&name);
    if core::taken(&out) {
        return Err(tf("archive.error.exists", &[("name", &name)]));
    }
    *LAST.lock().unwrap() = Some((req.level, req.threads));
    let opts = Options {
        level: req.level.min(9),
        threads: req.threads.clamp(1, archive::cores()),
        password: req.password.filter(|p| !p.is_empty()),
        encryption: if req.encryption == "zipcrypto" {
            Encryption::ZipCrypto
        } else {
            Encryption::Aes256
        },
        split,
    };
    archive::start(&app, items(&app), out, opts)?;
    archive_close(app);
    Ok(())
}

#[tauri::command]
pub fn archive_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window(ARCHIVE) {
        let _ = w.hide();
    }
}
