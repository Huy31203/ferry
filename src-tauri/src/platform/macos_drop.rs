//! macOS: the drop zone is a floating, non-activating panel that never becomes
//! key, so showing it or dropping on it leaves the user's app in front. It is a
//! real drop target: Tauri's `DragDropEvent` supplies the paths.
#![allow(clippy::unused_unit)] // the tauri_panel! macro expands to `-> ()`

use tauri::WebviewWindow;
use tauri_nspanel::{
    CollectionBehavior, PanelLevel, WebviewWindowExt, objc2_app_kit::NSWindowStyleMask, tauri_panel,
};

tauri_panel! {
    panel!(DropZonePanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

/// Turn the zone's window into a floating panel on every Space, above full-screen apps.
pub fn make_zone_window(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel::<DropZonePanel>()?;
    panel.set_level(PanelLevel::Floating.value());
    panel.set_collection_behavior(
        CollectionBehavior::new()
            .can_join_all_spaces()
            .full_screen_auxiliary()
            .stationary()
            .ignores_cycle()
            .value(),
    );
    panel.set_hides_on_deactivate(false);
    if let Err(e) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
        eprintln!("ferry: could not make the drop zone non-activating: {e:?}");
    }
    Ok(())
}
