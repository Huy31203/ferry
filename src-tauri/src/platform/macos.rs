//! macOS: the results stack is a non-activating NSPanel, so showing, hovering
//! or clicking a card never takes focus from the user's app (R§5).

use tauri::{Manager, WebviewWindow};
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelLevel, WebviewWindowExt, objc2_app_kit::NSWindowStyleMask,
    tauri_panel,
};

tauri_panel! {
    panel!(StackPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
    })
}

pub fn make_non_activating(window: &WebviewWindow) -> tauri::Result<()> {
    let panel = window.to_panel::<StackPanel>()?;
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
    panel.set_accepts_mouse_moved_events(true);
    if let Err(e) = panel.add_style_mask(NSWindowStyleMask::NonactivatingPanel) {
        eprintln!("ferry: could not make the stack non-activating: {e:?}");
    }
    Ok(())
}

pub fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    match window.app_handle().get_webview_panel(window.label()) {
        Ok(panel) => {
            panel.show(); // orderFrontRegardless: visible, not key
            Ok(())
        }
        Err(_) => window.show(),
    }
}
