//! Commands and set-up for stream G's app services: the hotkey editor now, and
//! the rail, updater and sync commands in later tasks.

use std::sync::Mutex;

use ferry_core::settings::Settings;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::hotkeys::{self, Verdict};
use crate::state::AppState;
use crate::{prefs, windows};

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

/// Grows in later tasks.
pub fn setup(_app: &AppHandle) {}
