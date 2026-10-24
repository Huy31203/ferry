//! What the card menu's integration items do. Every function returns a
//! user-facing sentence on failure; the caller shows it as a notice.

use std::path::{Path, PathBuf};

use ferry_core::i18n::{t, tf};
use ferry_core::integrations::{DROPSHARE, SHELVES, media_kind, merge_apps};
use ferry_core::job::JobId;
use ferry_core::settings::AppRef;
use tauri::{AppHandle, Manager};
use tracing::debug;

use crate::state::AppState;
use crate::{jobs, platform, prefs};

fn gone_reason() -> String {
    t("share.error.gone")
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

/// The card's result file.
pub fn output(app: &AppHandle, id: &JobId) -> Result<PathBuf, String> {
    jobs::output(app, id).ok_or_else(gone_reason)
}

pub fn open_with(app: &AppHandle, id: &JobId, with: &AppRef) -> Result<(), String> {
    let path = output(app, id)?;
    platform::apps::open_with(with, std::slice::from_ref(&path)).map_err(|reason| {
        debug!(%reason, app = %with.name, "open with failed");
        tf(
            "share.error.open",
            &[("name", &file_name(&path)), ("app", &with.name)],
        )
    })
}

/// Edit with the app chosen for this kind of file, else the default app.
pub fn edit_with(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let path = output(app, id)?;
    let editor = kind_of(&path).and_then(|kind| {
        let state = app.state::<AppState>();
        let settings = state.settings.read().unwrap();
        settings.share.edit_with.get(kind).cloned()
    });
    match editor {
        Some(editor) => open_with(app, id, &editor),
        None => tauri_plugin_opener::open_path(&path, None::<&str>).map_err(|reason| {
            debug!(%reason, "open in the default app failed");
            tf(
                "share.error.open",
                &[("name", &file_name(&path)), ("app", "")],
            )
        }),
    }
}

/// Remember `with` as this kind of file's editor, and edit with it now.
pub fn choose_editor(app: &AppHandle, id: &JobId, with: AppRef) -> Result<(), String> {
    let path = output(app, id)?;
    if let Some(kind) = kind_of(&path) {
        prefs::update(app, |settings| {
            settings.share.edit_with.set(kind, Some(with.clone()));
        })?;
    }
    open_with(app, id, &with)
}

fn kind_of(path: &Path) -> Option<ferry_core::integrations::MediaKind> {
    media_kind(&path.extension()?.to_string_lossy())
}

/// The chosen uploader, else Dropshare when it is installed.
pub fn uploader(app: &AppHandle) -> Option<AppRef> {
    let chosen = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .share
        .uploader
        .clone();
    chosen.or_else(|| platform::apps::find_known(&DROPSHARE))
}

/// Upload by opening the file with the uploader. Nothing happens without one.
pub fn upload(app: &AppHandle, id: &JobId) -> Result<(), String> {
    match uploader(app) {
        Some(uploader) => open_with(app, id, &uploader),
        None => {
            debug!("no uploader to upload with");
            Ok(())
        }
    }
}

/// The shelf apps found on this computer, then the ones added in Settings.
pub fn shelves(app: &AppHandle) -> Vec<AppRef> {
    let configured = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .share
        .shelf_apps
        .clone();
    let found = SHELVES
        .iter()
        .filter_map(platform::apps::find_known)
        .collect();
    merge_apps(found, &configured)
}

/// Send the card's file with AirDrop (macOS).
#[cfg(target_os = "macos")]
pub fn airdrop(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let path = output(app, id)?;
    let handle = app.clone();
    app.run_on_main_thread(move || {
        if let Err(reason) = platform::share::airdrop(&[path]) {
            jobs::notice(&handle, reason);
        }
    })
    .map_err(|e| e.to_string())
}

/// Ferry's own id in the Open With list, to leave it out.
pub fn own_id(app: &AppHandle) -> String {
    if cfg!(target_os = "macos") {
        app.config().identifier.clone()
    } else {
        std::env::current_exe()
            .map(|exe| exe.to_string_lossy().into_owned())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_output_is_a_notice() {
        assert_eq!(gone_reason(), ferry_core::i18n::t("share.error.gone"));
        assert_ne!(gone_reason(), "share.error.gone"); // the key exists in en.json
    }
}
