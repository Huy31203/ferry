//! Tray icon and menu (R§4.1–4.2, M1 subset).

use tauri::{
    AppHandle, Result,
    menu::{Menu, MenuItem, PredefinedMenuItem},
    tray::TrayIconBuilder,
};

use crate::{i18n::t, windows};

pub fn build(app: &AppHandle) -> Result<()> {
    let settings = MenuItem::with_id(
        app,
        "settings",
        t("tray.settings"),
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let about = PredefinedMenuItem::about(app, Some(&t("tray.about")), None)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    let menu = Menu::with_items(
        app,
        &[
            &settings,
            &PredefinedMenuItem::separator(app)?,
            &about,
            &quit,
        ],
    )?;

    let mut tray = TrayIconBuilder::with_id("ferry")
        .tooltip(t("tray.tooltip"))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| match event.id().as_ref() {
            "settings" => windows::open_settings(app),
            "quit" => app.exit(0),
            _ => {}
        });

    // macOS tints a black template image for light and dark menu bars;
    // Windows shows the full-colour app icon in the notification area.
    #[cfg(target_os = "macos")]
    {
        tray = tray
            .icon(tauri::image::Image::from_bytes(include_bytes!(
                "../icons/tray/64x64.png"
            ))?)
            .icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }

    tray.build(app)?;
    Ok(())
}
