//! Changing settings while the app runs (spec §3, §5.7): sanitise, save,
//! then apply what the change touches. Everything else, such as compression or
//! the ignored apps, is read from `AppState` by the next job.

use ferry_core::settings::{Settings, TrayStyle};
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use crate::state::AppState;
use crate::{tray, windows};

/// What a change to the settings must do beyond being saved.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// Hide the stack from screenshots, or stop hiding it.
    pub screenshots: bool,
    /// Register or remove the login item.
    pub autostart: bool,
    /// Show or hide the Dock icon (macOS).
    pub dock: bool,
    /// Rebuild the tray menu and icon.
    pub tray: bool,
    /// Register the hotkeys again (svc, stream G).
    pub hotkeys: bool,
    /// Tell the updater its settings changed (svc).
    pub updates: bool,
    /// Tell the sync worker to start, stop or move (svc).
    pub sync: bool,
}

impl Effects {
    pub fn between(old: &Settings, new: &Settings) -> Self {
        let (a, b) = (&old.general, &new.general);
        let screenshots = a.show_in_screenshots != b.show_in_screenshots;
        let style: fn(&Settings) -> TrayStyle = |s| s.general.tray_style;
        Self {
            screenshots,
            autostart: a.launch_at_login != b.launch_at_login,
            dock: a.show_dock_icon != b.show_dock_icon,
            // The menu's "Show app UI in screenshots" tick follows the setting.
            tray: screenshots || style(old) != style(new),
            hotkeys: old.hotkeys != new.hotkeys
                || old.svc.shortcuts != new.svc.shortcuts
                || old.floating.hover_hotkeys != new.floating.hover_hotkeys,
            updates: old.svc.updates != new.svc.updates,
            sync: old.svc.sync != new.svc.sync,
        }
    }
}

/// Change the settings: `change` edits a copy, which is sanitised, saved and
/// applied. Every window learns of it through `settings:changed`. The write
/// lock is held across the save so two changes cannot interleave.
pub fn update(app: &AppHandle, change: impl FnOnce(&mut Settings)) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let (old, new) = {
        let mut current = state.settings.write().unwrap();
        let old = current.clone();
        let mut next = old.clone();
        change(&mut next);
        let next = next.sanitised();
        next.save(&state.settings_path).map_err(|e| e.to_string())?;
        *current = next.clone();
        (old, next)
    };
    apply(app, &old, &new);
    let _ = app.emit("settings:changed", &new);
    crate::sync_worker::local_changed(app); // svc
    Ok(new)
}

fn apply(app: &AppHandle, old: &Settings, new: &Settings) {
    let effects = Effects::between(old, new);
    if effects.screenshots {
        windows::set_content_protected(app, !new.general.show_in_screenshots);
    }
    if effects.autostart {
        sync_autostart(app, new.general.launch_at_login);
    }
    if effects.dock {
        set_dock(app, new.general.show_dock_icon);
    }
    if effects.tray {
        tray::rebuild(app);
    }
    if effects.hotkeys {
        // Global shortcuts are registered on the main thread only.
        let app = app.clone();
        let _ = app
            .clone()
            .run_on_main_thread(move || crate::hotkeys::reregister(&app));
    }
    if effects.updates {
        crate::updater::settings_changed(app);
    }
    if effects.sync {
        crate::sync_worker::reconfigure(app);
    }
}

/// What to do to the login item: `Some(true)` to (re)write it, `Some(false)`
/// to remove it, `None` for nothing. While wanted it is written at every start,
/// because an item registered when the app ran from a DMG or Downloads keeps
/// pointing there after the app is moved to /Applications.
fn autostart_step(want: bool, enabled: bool) -> Option<bool> {
    match (want, enabled) {
        (true, _) => Some(true),
        (false, true) => Some(false),
        (false, false) => None,
    }
}

/// Make the login item match `want`. Debug builds never touch it: the default
/// is on, and a development run must not register `target/debug/ferry`.
pub fn sync_autostart(app: &AppHandle, want: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let enabled = match launcher.is_enabled() {
        Ok(on) => on,
        Err(e) => {
            warn!("could not read the login item: {e}");
            return;
        }
    };
    let result = match autostart_step(want, enabled) {
        Some(true) => launcher.enable(),
        Some(false) => launcher.disable(),
        None => Ok(()),
    };
    if let Err(e) = result {
        warn!("could not update the login item: {e}");
    }
}

/// Show or hide the Dock icon. Ferry is a tray app, so it starts hidden.
pub fn set_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        if let Err(e) = app.set_activation_policy(policy) {
            warn!("could not change the Dock icon: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_changes_nothing_happens() {
        let s = Settings::default();
        assert_eq!(Effects::between(&s, &s), Effects::default());
    }

    #[test]
    fn each_change_touches_only_its_own_effects() {
        let base = Settings::default();

        let mut s = base.clone();
        s.general.show_in_screenshots = !s.general.show_in_screenshots;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                screenshots: true,
                tray: true,
                ..Effects::default()
            }
        );

        let mut s = base.clone();
        s.general.launch_at_login = !s.general.launch_at_login;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                autostart: true,
                ..Effects::default()
            }
        );

        let mut s = base.clone();
        s.general.show_dock_icon = !s.general.show_dock_icon;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                dock: true,
                ..Effects::default()
            }
        );

        let mut s = base.clone();
        s.general.tray_style = TrayStyle::Flat;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                tray: true,
                ..Effects::default()
            }
        );
    }

    #[test]
    fn a_hotkey_change_reregisters() {
        let base = Settings::default();
        let only_hotkeys = Effects {
            hotkeys: true,
            ..Effects::default()
        };

        let mut s = base.clone();
        s.hotkeys.optimise = "Ctrl+Alt+O".into();
        assert_eq!(Effects::between(&base, &s), only_hotkeys);

        let mut s = base.clone();
        s.svc.shortcuts.preview = "P".into();
        assert_eq!(Effects::between(&base, &s), only_hotkeys);

        let mut s = base.clone();
        s.svc.shortcuts.disabled.push("compare".into());
        assert_eq!(Effects::between(&base, &s), only_hotkeys);

        let mut s = base.clone();
        s.floating.hover_hotkeys = !s.floating.hover_hotkeys;
        assert_eq!(Effects::between(&base, &s), only_hotkeys);
    }

    #[test]
    fn an_update_setting_change_reaches_the_updater() {
        let base = Settings::default();
        let mut s = base.clone();
        s.svc.updates.pill = !s.svc.updates.pill;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                updates: true,
                ..Effects::default()
            }
        );
    }

    #[test]
    fn a_sync_setting_change_reconfigures_the_worker() {
        let base = Settings::default();
        let mut s = base.clone();
        s.svc.sync.folder = "~/Sync".into();
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                sync: true,
                ..Effects::default()
            }
        );
    }

    #[test]
    fn settings_read_on_every_job_need_no_effect() {
        let base = Settings::default();
        let mut s = base.clone();
        s.images.compression = 40;
        s.clipboard.ignored_apps.push("com.example.App".into());
        s.floating.corner = ferry_core::settings::Corner::TopLeft;
        s.backups.retention_days = 30;
        assert_eq!(Effects::between(&base, &s), Effects::default());
    }

    #[test]
    fn the_login_item_is_rewritten_at_every_start_while_wanted() {
        // An existing item may point at an old location of the app (a DMG, Downloads).
        assert_eq!(autostart_step(true, true), Some(true));
        assert_eq!(autostart_step(true, false), Some(true));
        assert_eq!(autostart_step(false, true), Some(false));
        assert_eq!(autostart_step(false, false), None);
    }
}
