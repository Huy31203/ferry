mod i18n;
mod platform;
mod tray;
mod windows;

pub fn run() {
    // Must be the first plugin: a second launch focuses Settings instead of adding a tray icon.
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::open_settings(app)
        }));
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            windows::create_stack(app.handle())?;
            tray::build(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![windows::stack_resize])
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
