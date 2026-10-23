//! macOS: the results stack is a non-activating NSPanel, so showing, hovering
//! or clicking a card never takes focus from the user's app (R§5).
//!
//! WKWebView only updates `:hover` in the key window, which this panel never is.
//! A tracking area that is active in any window feeds the pointer to the page
//! instead (spec §12).
#![allow(clippy::unused_unit)] // the panel_event! macro expands to `-> ()`

use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use tauri::{Emitter, Manager, WebviewWindow};
use tauri_nspanel::{
    CollectionBehavior, ManagerExt, PanelLevel, TrackingAreaOptions, WebviewWindowExt,
    objc2_app_kit::NSWindowStyleMask, tauri_panel,
};

use crate::windows::STACK;

tauri_panel! {
    panel!(StackPanel {
        config: {
            can_become_key_window: key_wanted(),
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

/// The user's accent colour (System Settings → Appearance), in sRGB.
pub fn accent_rgb() -> Option<(u8, u8, u8)> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    let color = NSColor::controlAccentColor();
    let rgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some((
        byte(rgb.redComponent()),
        byte(rgb.greenComponent()),
        byte(rgb.blueComponent()),
    ))
}

/// The bundle identifier of the `.app` at `path`, which is what the clipboard
/// snapshot reports as its source app.
pub fn app_identifier(path: &Path) -> Option<String> {
    use objc2_foundation::{NSBundle, NSString};
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    bundle.bundleIdentifier().map(|id| id.to_string())
}

/// Whether the stack may take the keyboard: only while a text field or a
/// slider drag is open (spec §2 "Text input in the stack").
static KEY_WANTED: AtomicBool = AtomicBool::new(false);

/// The app that had the keyboard before the stack took it.
// `tauri_panel!` already imports `Retained` into this module.
static FRONT: Mutex<Option<objc2::rc::Retained<NSRunningApplication>>> = Mutex::new(None);

/// Fallback switch (Task 11): `true` activates Ferry while a field is open,
/// for when a non-activating key panel does not get typing.
const ACTIVATE_FOR_KEYS: bool = false;

/// The panel's `canBecomeKeyWindow`, asked by AppKit on every call.
fn key_wanted() -> bool {
    KEY_WANTED.load(Ordering::Relaxed)
}

/// Make the stack the key window without activating Ferry (the panel is
/// non-activating), or give the keyboard back to the app that had it.
pub fn set_stack_key(window: &WebviewWindow, on: bool) -> tauri::Result<()> {
    KEY_WANTED.store(on, Ordering::Relaxed);
    let Ok(panel) = window.app_handle().get_webview_panel(window.label()) else {
        return Ok(());
    };
    if on {
        let me = NSRunningApplication::currentApplication();
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .filter(|app| app.processIdentifier() != me.processIdentifier());
        *FRONT.lock().unwrap() = front;
        if ACTIVATE_FOR_KEYS {
            #[allow(deprecated)]
            me.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
        panel.make_key_window();
    } else {
        panel.resign_key_window();
        // That app stayed active while the panel was key (unless the fallback
        // is on); activating it again gives its own window the keyboard back.
        // Not when the user has since clicked another app (that click is what
        // closed the field): the keyboard stays where they put it.
        if let Some(front) = FRONT.lock().unwrap().take() {
            let me = NSRunningApplication::currentApplication().processIdentifier();
            let still_ours = NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .is_none_or(|now| {
                    let pid = now.processIdentifier();
                    pid == front.processIdentifier() || pid == me
                });
            if still_ours {
                front.activateWithOptions(NSApplicationActivationOptions::empty());
            }
        }
    }
    Ok(())
}

/// Whether Ferry is the active app, i.e. one of its own windows (compare,
/// Quick Look, Settings) has the keyboard. The non-activating stack never makes it so.
pub fn app_is_active() -> bool {
    NSRunningApplication::currentApplication().isActive()
}
