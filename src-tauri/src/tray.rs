//! Tray icon and menu (spec §5.6). The menu is rebuilt whenever what it shows
//! changes; the icon follows the run state and whether any job is working.

use std::sync::atomic::Ordering;

#[cfg(target_os = "macos")]
use ferry_core::glyph;
use ferry_core::glyph::TrayGlyph;
use ferry_core::i18n::{t, tf};
use ferry_core::job::JobState;
use ferry_core::optimise::Level;
use ferry_core::run_state::RunState;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Result, Wry};
use tracing::warn;

use crate::hotkeys::{self, Registered};
use crate::state::AppState;
use crate::{jobs, windows};

const TRAY: &str = "ferry";
pub(crate) const BASE_ICON: &[u8] = include_bytes!("../icons/tray/64x64.png");

pub fn build(app: &AppHandle) -> Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip(t("tray.tooltip"))
        .menu(&menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| match event {
            // Opening the menu acknowledges the error mark.
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } => {
                let app = tray.app_handle();
                app.state::<AppState>()
                    .tray_error
                    .store(false, Ordering::Relaxed);
                refresh(app);
            }
            #[cfg(windows)]
            TrayIconEvent::DoubleClick { .. } => windows::open_settings(tray.app_handle()),
            _ => {}
        });
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    #[cfg(target_os = "macos")]
    {
        tray = tray
            .icon(tauri::image::Image::from_bytes(BASE_ICON)?)
            .icon_as_template(true);
    }
    tray.build(app)?;
    refresh(app);
    Ok(())
}

/// Rebuild the menu (run state, recent results, toggles) and refresh the icon.
pub fn rebuild(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY) else {
        return;
    };
    match menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => warn!("could not rebuild the tray menu: {e}"),
    }
    refresh(app);
}

/// Update the icon and tooltip for the current state.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY) else {
        return;
    };
    let state = app.state::<AppState>();
    let run_state = *state.run_state.lock().unwrap();
    let glyph = if state.tray_error.load(Ordering::Relaxed) {
        TrayGlyph::Error
    } else if state.is_working() {
        TrayGlyph::Working
    } else {
        match run_state {
            RunState::Running => TrayGlyph::Running,
            RunState::Paused => TrayGlyph::Paused,
            RunState::Stopped => TrayGlyph::Stopped,
        }
    };
    let _ = tray.set_tooltip(Some(tf(
        "tray.tooltip.state",
        &[("state", &status(run_state))],
    )));
    // Windows keeps its full-colour icon; the tooltip carries the state.
    #[cfg(target_os = "macos")]
    if let Ok(base) = tauri::image::Image::from_bytes(BASE_ICON) {
        let style = state.settings.read().unwrap().general.tray_style;
        let rgba = glyph::render_styled(base.rgba(), base.width(), glyph, style);
        let _ = tray.set_icon(Some(tauri::image::Image::new_owned(
            rgba,
            base.width(),
            base.height(),
        )));
        let _ = tray.set_icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = glyph;
}

fn status(run_state: RunState) -> String {
    t(match run_state {
        RunState::Running => "tray.state.running",
        RunState::Paused => "tray.state.paused",
        RunState::Stopped => "tray.state.stopped",
    })
}

fn menu(app: &AppHandle) -> Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let run_state = *state.run_state.lock().unwrap();
    let keys = app
        .try_state::<Registered>()
        .map(|r| r.inner().clone())
        .unwrap_or_default();
    let screenshots = state.settings.read().unwrap().general.show_in_screenshots;

    let header = MenuItem::with_id(
        app,
        "status",
        t(match run_state {
            RunState::Running => "tray.status.running",
            RunState::Paused => "tray.status.paused",
            RunState::Stopped => "tray.status.stopped",
        }),
        false,
        None::<&str>,
    )?;
    let radio = |id: &str, key: &str, value: RunState| {
        CheckMenuItem::with_id(app, id, t(key), true, run_state == value, None::<&str>)
    };
    let running = radio("state:running", "tray.state.running", RunState::Running)?;
    let paused = radio("state:paused", "tray.state.paused", RunState::Paused)?;
    let stopped = radio("state:stopped", "tray.state.stopped", RunState::Stopped)?;
    let optimise = MenuItem::with_id(
        app,
        "optimise",
        t("tray.optimise"),
        true,
        keys.optimise.as_deref(),
    )?;
    let aggressive = MenuItem::with_id(
        app,
        "aggressive",
        t("tray.aggressive"),
        true,
        keys.aggressive.as_deref(),
    )?;

    // Copy the list so the lock is not held while items are built: off the main
    // thread each `with_id` waits for the main thread, which may want this lock.
    let recent_updates: Vec<_> = state.recent.lock().unwrap().iter().cloned().collect();
    let recent_items: Vec<MenuItem<Wry>> = recent_updates
        .iter()
        .map(|u| {
            MenuItem::with_id(
                app,
                format!("recent:{}", u.id.as_str()),
                recent_label(u),
                true,
                None::<&str>,
            )
        })
        .collect::<Result<_>>()?;
    let recent_refs: Vec<&dyn IsMenuItem<Wry>> = recent_items
        .iter()
        .map(|i| i as &dyn IsMenuItem<Wry>)
        .collect();
    let recent = Submenu::with_id_and_items(
        app,
        "recent",
        t("tray.recent"),
        !recent_items.is_empty(),
        &recent_refs,
    )?;

    let backups = MenuItem::with_id(
        app,
        "open_backups",
        t("tray.open_backups"),
        true,
        None::<&str>,
    )?;
    let work = MenuItem::with_id(app, "open_work", t("tray.open_work"), true, None::<&str>)?;
    let force_clean = MenuItem::with_id(
        app,
        "force_clean",
        t("tray.force_clean"),
        true,
        None::<&str>,
    )?;
    let show = CheckMenuItem::with_id(
        app,
        "screenshots",
        t("tray.screenshots"),
        true,
        screenshots,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(
        app,
        "settings",
        t("tray.settings"),
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let about = PredefinedMenuItem::about(app, Some(&t("tray.about")), None)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &header,
            &running,
            &paused,
            &stopped,
            &PredefinedMenuItem::separator(app)?,
            &optimise,
            &aggressive,
            &recent,
            &PredefinedMenuItem::separator(app)?,
            &backups,
            &work,
            &force_clean,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &about,
            &quit,
        ],
    )
}

/// "Clipboard 2026-10-22 at 14.23.11.png — −84%" or "… — Already optimal".
fn recent_label(u: &ferry_core::job::JobUpdate) -> String {
    let name = if u.ext.is_empty() {
        u.name.clone()
    } else {
        format!("{}.{}", u.name, u.ext)
    };
    let outcome = match (&u.state, u.after) {
        // Rounded like savingPercent in src/lib/format.ts.
        (JobState::Done, Some(after)) if u.before > 0 => {
            let saved = (100.0 - after as f64 * 100.0 / u.before as f64).round();
            format!("−{saved}%")
        }
        (JobState::AlreadyOptimal, _) => t("card.state.already_optimal"),
        (JobState::Larger, _) => t("card.state.larger"),
        (JobState::Restored, _) => t("card.state.restored"),
        (JobState::Error { .. }, _) => t("card.state.error"),
        _ => return name,
    };
    if name.is_empty() {
        outcome
    } else {
        format!("{name} — {outcome}")
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "state:running" | "state:paused" | "state:stopped" => {
            *state.run_state.lock().unwrap() = match id {
                "state:running" => RunState::Running,
                "state:paused" => RunState::Paused,
                _ => RunState::Stopped,
            };
            rebuild(app);
        }
        "optimise" => hotkeys::optimise_in_background(app, Level::Normal),
        "aggressive" => hotkeys::optimise_in_background(app, Level::Aggressive),
        "open_backups" => open_folder(&state.backups_dir),
        "open_work" => open_folder(&state.work_dir),
        "screenshots" => {
            if let Err(e) = crate::prefs::update(app, |s| {
                s.general.show_in_screenshots = !s.general.show_in_screenshots;
            }) {
                warn!("could not save settings: {e}");
            }
        }
        "force_clean" => {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = jobs::force_clean(&app) {
                    warn!("could not clean the working directory: {e}");
                }
            });
        }
        "settings" => windows::open_settings(app),
        "quit" => app.exit(0),
        other => {
            if let Some(id) = other.strip_prefix("recent:") {
                let update = state
                    .recent
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|u| u.id.as_str() == id)
                    .cloned();
                if let Some(update) = update {
                    jobs::show(app, update);
                }
            }
        }
    }
}

pub(crate) fn open_folder(path: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(path)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
        })
    {
        warn!("could not open {}: {e}", path.display());
    }
}
