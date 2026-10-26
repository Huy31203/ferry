//! Commands and set-up for stream G's app services: the hotkey editor now, and
//! the rail, updater and sync commands in later tasks.

use std::sync::Mutex;

use ferry_core::settings::{self, Settings};
use ferry_core::settings_sync::{FolderInfo, JoinMode};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::hotkeys::{self, Verdict};
use crate::state::AppState;
use crate::{prefs, rail_menu, sync_worker, updater, windows};

const MAC: bool = cfg!(target_os = "macos");

fn settings_of(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.read().unwrap().clone()
}

#[tauri::command]
pub fn svc_hotkeys_status(app: AppHandle) -> Vec<hotkeys::Status> {
    hotkeys::status(&app)
}

#[tauri::command]
pub fn svc_hotkey_check(app: AppHandle, id: String, accelerator: String) -> Verdict {
    hotkeys::check(&settings_of(&app), &id, &accelerator, MAC)
}

/// Saves `accelerator` for `id` only when the check finds no problem.
#[tauri::command(async)]
pub fn svc_hotkey_set(app: AppHandle, id: String, accelerator: String) -> Result<Verdict, String> {
    let accelerator = accelerator.trim().to_string();
    let verdict = hotkeys::check(&settings_of(&app), &id, &accelerator, MAC);
    if verdict.problem.is_none() {
        prefs::update(&app, |s| {
            hotkeys::set_accelerator(s, &id, &accelerator);
        })?;
    }
    Ok(verdict)
}

/// Turning a hotkey on re-checks its current accelerator.
#[tauri::command(async)]
pub fn svc_hotkey_enable(app: AppHandle, id: String, on: bool) -> Result<Verdict, String> {
    let settings = settings_of(&app);
    let current = hotkeys::accelerator(&settings, &id).unwrap_or_default();
    let verdict = if on {
        hotkeys::check(&settings, &id, &current, MAC)
    } else {
        Verdict {
            problem: None,
            warning: None,
        }
    };
    if verdict.problem.is_none() {
        prefs::update(&app, |s| {
            let disabled = &mut s.svc.shortcuts.disabled;
            disabled.retain(|d| *d != id);
            if !on {
                disabled.push(id.clone());
            }
        })?;
    }
    Ok(verdict)
}

/// The default accelerator, under the same rules as `svc_hotkey_set`.
#[tauri::command(async)]
pub fn svc_hotkey_reset(app: AppHandle, id: String) -> Result<Verdict, String> {
    match hotkeys::default_accelerator(&id, MAC) {
        Some(default) => svc_hotkey_set(app, id, default),
        None => Ok(Verdict {
            problem: None,
            warning: None,
        }),
    }
}

/// Main thread (a plain command): the Settings window starts or stops recording.
#[tauri::command]
pub fn svc_hotkeys_suspend(app: AppHandle, on: bool) {
    hotkeys::suspend(&app, on);
}

/// Where Settings should open: a pane, and a row to highlight.
#[derive(Debug, Clone, Serialize)]
pub struct PaneRequest {
    pub pane: String,
    pub row: Option<String>,
}

static PANE: Mutex<Option<PaneRequest>> = Mutex::new(None);

/// Open Settings at `pane`, highlighting `row` (a setting id). Main thread, as
/// `windows::open_settings` is. The page asks for the request when it loads, or
/// on `svc:open-pane` when it is already open.
pub fn open_settings_at(app: &AppHandle, pane: &str, row: Option<&str>) {
    *PANE.lock().unwrap() = Some(PaneRequest {
        pane: pane.to_string(),
        row: row.map(String::from),
    });
    windows::open_settings(app);
    let _ = app.emit_to(windows::SETTINGS, "svc:open-pane", ());
}

/// Takes (and clears) the stored request.
#[tauri::command]
pub fn svc_settings_pane() -> Option<PaneRequest> {
    PANE.lock().unwrap().take()
}

/// The "+" on a card's rail: a native menu at `(x, y)`. Sync, so it runs on the
/// main thread, as menus must.
#[tauri::command]
pub fn svc_rail_menu(
    app: AppHandle,
    window: tauri::WebviewWindow,
    x: f64,
    y: f64,
    items: Vec<rail_menu::RailMenuItem>,
) -> Result<(), String> {
    rail_menu::popup(&app, &window, x, y, &items)
}

#[tauri::command]
pub fn svc_update_state(app: AppHandle) -> updater::UpdateView {
    updater::view(&app)
}

/// A manual check: the page learns of a failure from `svc:update` and from this `Err`.
#[tauri::command]
pub async fn svc_update_check(app: AppHandle) -> Result<(), String> {
    updater::check(app, true).await
}

#[tauri::command]
pub async fn svc_update_install(app: AppHandle) -> Result<(), String> {
    updater::install(app).await
}

#[tauri::command]
pub fn svc_update_later(app: AppHandle) {
    updater::later(&app);
}

#[tauri::command(async)]
pub fn svc_update_skip(app: AppHandle) -> Result<(), String> {
    updater::skip(&app)
}

#[tauri::command]
pub fn svc_open_release(app: AppHandle) -> Result<(), String> {
    let url = updater::view(&app)
        .release_url
        .ok_or("no update is on offer")?;
    tauri_plugin_opener::open_url(url, None::<&str>).map_err(|e| e.to_string())
}

/// The stack's pill opens About and updates. Async: building the Settings
/// window inside a sync command deadlocks on Windows.
#[tauri::command(async)]
pub fn svc_open_about(app: AppHandle) {
    open_settings_at(&app, "about", None);
}

#[tauri::command]
pub fn svc_sync_status(app: AppHandle) -> sync_worker::SyncStatus {
    sync_worker::status(&app)
}

/// What the chosen folder already holds, read off the main thread.
#[tauri::command]
pub async fn svc_sync_inspect(app: AppHandle, folder: String) -> Result<FolderInfo, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = app.path().home_dir().map_err(|e| e.to_string())?;
        let chosen = settings::expand_home(&folder, &home);
        Ok(sync_worker::inspect(
            &chosen,
            sync_worker::own_device(&app).as_deref(),
        ))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Join the folder (`join` picks whose values win), then turn sync on.
#[tauri::command(async)]
pub fn svc_sync_enable(app: AppHandle, folder: String, join: JoinMode) -> Result<(), String> {
    sync_worker::prepare_join(&app, join)?;
    prefs::update(&app, |s| {
        s.svc.sync.enabled = true;
        s.svc.sync.folder = folder;
    })
    .map(|_| ())
}

#[tauri::command(async)]
pub fn svc_sync_disable(app: AppHandle) -> Result<(), String> {
    prefs::update(&app, |s| s.svc.sync.enabled = false).map(|_| ())
}

#[tauri::command]
pub fn svc_sync_now(app: AppHandle) {
    sync_worker::sync_now(&app);
}

/// Grows in later tasks.
pub fn setup(app: &AppHandle) {
    rail_menu::init(app);
    updater::init(app);
    sync_worker::start(app);
}
