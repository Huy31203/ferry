//! Ferry's windows: Settings (normal, on demand).

use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use crate::i18n::t;

pub const SETTINGS: &str = "settings";

/// Show the Settings window, creating it the first time.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title(t("settings.window_title"))
        .inner_size(820.0, 600.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
    if let Err(e) = built {
        eprintln!("ferry: could not open settings: {e}");
    }
}
