mod audio; // stream B
mod clipboard;
mod commands;
mod compare;
mod drop_batch;
mod hotkeys;
mod hover_keys;
mod integrations; // Stream H
mod jobs;
mod logging;
mod menu;
mod pdf_commands; // sE pdf-formats
mod pipe_commands; // stream F
mod pipe_jobs; // stream F
mod pipe_menu; // stream F
mod pipe_windows; // stream F
mod platform;
mod prefs;
mod preset_zones; // stream F
mod preview;
mod state;
mod tray;
mod watch; // stream C
mod windows;

// svc (stream G)
mod rail_menu;
mod svc_commands;
mod sync_worker;
mod updater;

use std::path::PathBuf;

use ferry_core::i18n::t;
use ferry_core::settings::{LoadStatus, Settings};
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::state::AppState;

pub fn run() {
    // Must be the first plugin: a second launch hands its files to this one (Explorer, Open with) or focuses Settings.
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, args, cwd| {
            integrations::launch::second_instance(app, args, cwd)
        }));
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_drag::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_updater::Builder::new().build()) // svc
        // svc: closing Settings mid-recording gives the hotkeys back
        .on_window_event(|window, event| {
            if window.label() == windows::SETTINGS && matches!(event, tauri::WindowEvent::Destroyed)
            {
                hotkeys::suspend(window.app_handle(), false);
            }
        })
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            setup(app.handle())?;
            Ok(())
        })
        // sI: hover keys stand down while a Ferry window takes typing
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::Focused(focused) = event {
                hover_keys::window_focus(window.app_handle(), window.label(), *focused)
            }
        })
        .invoke_handler(tauri::generate_handler![
            windows::stack_resize,
            compare::open_compare,
            compare::compare_ready,
            compare::compare_close,
            compare::compare_page, // sE pdf-formats
            preview::quick_preview,
            commands::stack_ready,
            commands::card_copy,
            commands::card_restore,
            commands::card_rerun,
            commands::card_cancel,
            commands::card_reveal,
            commands::open_log,
            commands::clear_all,
            commands::settings_get,
            commands::settings_set,
            commands::template_preview,
            commands::tray_style_previews,
            commands::accent_color,
            commands::pick_app,
            commands::open_backups,
            commands::open_work,
            commands::force_clean,
            commands::card_edit,
            commands::card_convert,
            commands::card_rename,
            windows::stack_focus,
            commands::crop_sizes_reset,
            commands::card_menu,
            commands::card_hover,
            commands::output_preview,
            commands::pick_folder,
            pdf_commands::pdf_config, // sE pdf-formats
            // stream C (watch)
            watch::commands::watch_status,
            watch::commands::watch_support,
            watch::commands::watch_check_folder,
            watch::commands::watch_pick_folder,
            // Stream H: integrations
            integrations::commands::card_share,
            integrations::commands::share_pick_app,
            // Stream D: drop-batch
            drop_batch::batch::batch_ready,
            drop_batch::batch::batch_pick_files,
            drop_batch::batch::batch_pick_folder,
            drop_batch::batch::batch_pause,
            drop_batch::batch::batch_resume,
            drop_batch::batch::batch_cancel_all,
            drop_batch::batch::batch_set_adjust,
            drop_batch::batch::batch_rerun,
            drop_batch::batch::batch_rollback,
            drop_batch::batch::batch_reveal,
            drop_batch::batch::batch_close,
            drop_batch::batch::batch_row_menu,
            drop_batch::zone::zone_ready,
            drop_batch::list::list_config,
            drop_batch::list::list_menu,
            drop_batch::list::list_edit,
            drop_batch::list::list_copy,
            drop_batch::list::list_save_to,
            // stream B: audio
            audio::card_extract_cover,
            // stream F: pipelines
            pipe_commands::pipe_registry,
            pipe_commands::pipe_meta,
            pipe_commands::pipe_check,
            pipe_commands::pipe_parse,
            pipe_commands::pipe_print,
            pipe_commands::pipe_starters,
            pipe_commands::pipe_test,
            pipe_commands::pipe_reveal_test,
            pipe_commands::pipe_pick_file,
            pipe_commands::pipe_run,
            pipe_commands::pipe_open_editor,
            pipe_commands::pipe_zones_show,
            pipe_commands::pipe_zones_hide,
            pipe_commands::pipe_zones_drag,
            pipe_commands::pipe_zones_ready,
            pipe_commands::pipe_classify,
            pipe_commands::pipe_zone_drop,
            pipe_commands::pipe_zone_menu,
            svc_commands::svc_hotkeys_status,
            svc_commands::svc_hotkey_check,
            svc_commands::svc_hotkey_set,
            svc_commands::svc_hotkey_enable,
            svc_commands::svc_hotkey_reset,
            svc_commands::svc_hotkeys_suspend,
            svc_commands::svc_settings_pane,
            svc_commands::svc_rail_menu,
            svc_commands::svc_update_state,
            svc_commands::svc_update_check,
            svc_commands::svc_update_install,
            svc_commands::svc_update_later,
            svc_commands::svc_update_skip,
            svc_commands::svc_open_release,
            svc_commands::svc_open_about,
            svc_commands::svc_sync_status,
            svc_commands::svc_sync_inspect,
            svc_commands::svc_sync_enable,
            svc_commands::svc_sync_disable,
            svc_commands::svc_sync_now,
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
            // stream A: video
            tauri::RunEvent::Exit => ferry_core::ffmpeg::kill_all(),
            // Stream H: Finder "Open With" and files dropped on the Dock icon.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Opened { urls } => integrations::launch::opened(_app, urls),
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
    app.manage(pipe_jobs::PipeState::default()); // stream F: pipelines
    windows::create_stack(app)?;
    if let Some(path) = pdfium() {
        ferry_core::pdf::render::init(path); // sE pdf-formats
    }
    hotkeys::reregister(app);
    tray::build(app)?;
    menu::init(app);
    drop_batch::init(app)?;
    preset_zones::init(app); // stream F: pipelines
    clipboard::start(app);
    watch::start(app); // stream C
    integrations::init(app); // Stream H
    housekeeping(app);
    svc_commands::setup(app);
    if let LoadStatus::Recovered { .. } = status {
        jobs::notice(app, t("error.settings_reset"));
    }
    video_startup(app);
    audio::startup(); // stream B: audio
    Ok(())
}

/// Work that follows the settings at every start: prune old backups off the
/// main path, and make the login item and the Dock icon match.
fn housekeeping(app: &AppHandle) {
    let (retention, login, dock) = {
        let settings = app.state::<AppState>().settings.read().unwrap().clone();
        (
            settings.backups.retention_days,
            settings.general.launch_at_login,
            settings.general.show_dock_icon,
        )
    };
    prefs::sync_autostart(app, login);
    if dock {
        prefs::set_dock(app, true);
    }
    let app = app.clone();
    std::thread::spawn(
        move || match app.state::<AppState>().backups.prune(retention) {
            Ok(report) => info!(
                removed = report.removed.len(),
                retention, "pruned old backups"
            ),
            Err(e) => tracing::warn!("could not prune old backups: {e}"),
        },
    );
}

/// The bundled gifsicle sits next to Ferry's own executable (Tauri `externalBin`).
fn gifsicle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.with_file_name(format!("gifsicle{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}

// sE pdf-formats
/// The bundled PDFium library: in the app's `Frameworks` on macOS, beside the
/// executable on Windows. A debug build also finds the fetched copy in
/// `src-tauri/binaries/`, so `pnpm tauri dev` renders PDFs.
fn pdfium() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let name = if cfg!(windows) {
        "pdfium.dll"
    } else {
        "libpdfium.dylib"
    };
    let bundled = if cfg!(windows) {
        exe.with_file_name(name)
    } else {
        exe.parent()?.join("../Frameworks").join(name)
    };
    let fetched = cfg!(debug_assertions)
        .then(|| PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/binaries")).join(name));
    [Some(bundled), fetched]
        .into_iter()
        .flatten()
        .find(|path| path.is_file())
}

// stream A: video
/// Find ffmpeg and probe the hardware encoders off the main path (R§5). A video
/// job waits for the probe, so a clip copied right after launch still finds HEVC.
fn video_startup(_app: &AppHandle) {
    let started = std::thread::Builder::new()
        .name("ferry-video-probe".into())
        .spawn(|| {
            match ferry_core::ffmpeg::locate() {
                Ok(path) => info!(path = %path.display(), "ffmpeg found"),
                Err(e) => tracing::warn!("ffmpeg missing: {e}"),
            }
            let hardware = ferry_core::video::hardware::probe_hardware();
            info!(?hardware, "video encoders");
            if hardware.hevc.is_none() {
                tracing::warn!("no hardware HEVC encoder: HEVC is not offered");
            }
        });
    if let Err(e) = started {
        tracing::warn!("could not start the video probe: {e}");
    }
}

#[cfg(test)]
mod asset_scope_tests {
    use glob::{MatchOptions, Pattern};

    /// The card and batch thumbnails (`.ferry-poster.png`, `.ferry-thumb.jpg`,
    /// `.ferry-cover.*`) are dotfiles in the backup folder; Tauri's asset scope
    /// refuses dotfiles on Unix unless the config says otherwise.
    #[test]
    fn asset_scope_serves_the_dot_named_thumbnails_in_backups() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let scope = &conf["app"]["security"]["assetProtocol"]["scope"];
        let (allow, literal_dot) = match scope.as_array() {
            Some(list) => (list.clone(), true), // Tauri's Unix default
            None => (
                scope["allow"].as_array().unwrap().clone(),
                scope["requireLiteralLeadingDot"].as_bool().unwrap_or(true),
            ),
        };
        let options = MatchOptions {
            require_literal_separator: true,
            require_literal_leading_dot: literal_dot,
            ..Default::default()
        };
        let backups = "/data/backups/2026-10-26/j1";
        let served = |file: &str| {
            let path = format!("{backups}/{file}");
            allow.iter().any(|p| {
                let pattern = p.as_str().unwrap().replace("$APPDATA", "/data");
                Pattern::new(&pattern).unwrap().matches_with(&path, options)
            })
        };
        for name in ["a.png", ".ferry-poster.png", ".ferry-thumb.jpg", ".ferry-cover.jpg"] {
            assert!(served(name), "{name} is outside the asset scope");
        }
    }
}
