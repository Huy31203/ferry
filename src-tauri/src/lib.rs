mod i18n;
mod tray;
mod windows;

pub fn run() {
    tauri::Builder::default()
        // Must be the first plugin: a second launch focuses Settings instead of adding a tray icon.
        .plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::open_settings(app);
        }))
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            tray::build(app.handle())?;
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("error while building Ferry")
        .run(|_app, event| {
            // Closing Settings must not quit a tray app; only "Quit Ferry" (an explicit exit code) does.
            if let tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } = event
            {
                api.prevent_exit();
            }
        });
}
