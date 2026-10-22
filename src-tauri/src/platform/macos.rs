//! macOS: the results stack is a non-activating NSPanel, so showing, hovering
//! or clicking a card never takes focus from the user's app (R§5).
//!
//! WKWebView only updates `:hover` in the key window, which this panel never is.
//! A tracking area that is active in any window feeds the pointer to the page
//! instead (spec §12).
#![allow(clippy::unused_unit)] // the panel_event! macro expands to `-> ()`

use tauri::{Emitter, Manager, WebviewWindow};
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelLevel, TrackingAreaOptions, WebviewWindowExt,
    objc2_app_kit::NSWindowStyleMask, tauri_panel,
};

use crate::windows::STACK;

tauri_panel! {
    panel!(StackPanel {
        config: {
            can_become_key_window: false,
            can_become_main_window: false,
            is_floating_panel: true
        }
        with: {
            tracking_area: {
                options: TrackingAreaOptions::new()
                    .active_always()
                    .mouse_entered_and_exited()
                    .mouse_moved(),
                auto_resize: true
            }
        }
    })

    panel_event!(StackPointer {
        window_did_become_key(notification: &NSNotification) -> ()
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

    let handler = StackPointer::new();
    let app = window.app_handle().clone();
    let moved = app.clone();
    let left = app.clone();
    handler.on_mouse_entered(move |event| send_pointer(&app, event));
    handler.on_mouse_moved(move |event| send_pointer(&moved, event));
    handler.on_mouse_exited(move |_| {
        let _ = left.emit_to(STACK, "stack:pointer-leave", ());
    });
    panel.set_event_handler(Some(handler.as_ref()));
    Ok(())
}

/// Tell the page where the pointer is. AppKit's origin is the window's bottom-left,
/// so the page flips y with its own height.
fn send_pointer(app: &tauri::AppHandle, event: &NSEvent) {
    let at = event.locationInWindow();
    let _ = app.emit_to(
        STACK,
        "stack:pointer",
        serde_json::json!({ "x": at.x, "fromBottom": at.y }),
    );
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

pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
    window.hide()
}
