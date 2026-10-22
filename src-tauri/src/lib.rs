mod clipboard;
mod commands;
mod hotkeys;
mod jobs;
mod logging;
mod platform;
mod state;
mod tray;
mod windows;

use std::path::PathBuf;

use ferry_core::i18n::t;
use ferry_core::settings::{LoadStatus, Settings};
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::state::AppState;

pub fn run() {
    // Must be the first plugin: a second launch focuses Settings instead of adding a tray icon.
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::open_settings(app)
        }));
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            windows::stack_resize,
            commands::stack_ready,
            commands::card_copy,
            commands::card_restore,
            commands::card_rerun,
            commands::card_cancel,
            commands::card_reveal,
            commands::open_log,
            commands::clear_all,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Ferry")
        .run(|_app, event| match event {
            // Closing Settings must not quit a tray app; only "Quit Ferry" (an explicit exit code) does.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            // Finder, Spotlight and `open Ferry.app` re-activate the running app instead of
            // starting a second process, so single-instance never sees them.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => windows::open_settings(_app),
            _ => {}
        });
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let paths = app.path();
    let log_dir = paths.app_log_dir()?;
    if let Some(guard) = logging::init(&log_dir) {
        app.manage(guard);
    }
    let settings_path = paths.app_config_dir()?.join("settings.json");
    let (settings, status) = Settings::load(&settings_path);
    info!(
        version = env!("CARGO_PKG_VERSION"),
        ?status,
        "Ferry started"
    );
    app.manage(AppState::new(
        settings,
        settings_path,
        paths.app_data_dir()?.join("backups"),
        paths.app_cache_dir()?.join("work"),
        log_dir,
        gifsicle(),
    ));
    windows::create_stack(app)?;
    let registered = hotkeys::register(app);
    app.manage(registered);
    tray::build(app)?;
    clipboard::start(app);
    if let LoadStatus::Recovered { .. } = status {
        jobs::notice(app, t("error.settings_reset"));
    }
    Ok(())
}

/// The bundled gifsicle sits next to Ferry's own executable (Tauri `externalBin`).
fn gifsicle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.with_file_name(format!("gifsicle{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}
