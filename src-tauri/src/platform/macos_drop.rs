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

pub use sampling::{drag_files, frontmost_app, mods, sample};

/// Sampling the mouse, the modifiers and the drag pasteboard for `drag::DragWatch`.
/// The NSEvent class properties need no Input Monitoring permission, and all of
/// it is safe off the main thread. A module of its own keeps its imports apart
/// from the ones `tauri_panel!` brings in.
mod sampling {
    use std::path::PathBuf;

    use ferry_core::drag::{Mods, Sample};
    use objc2_app_kit::{
        NSEvent, NSEventModifierFlags, NSPasteboard, NSPasteboardNameDrag, NSWorkspace,
    };
    use objc2_foundation::{NSString, NSURL};

    fn drag_pasteboard() -> objc2::rc::Retained<NSPasteboard> {
        // SAFETY: a constant AppKit string, valid for the life of the process.
        NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag })
    }

    pub fn mods() -> Mods {
        let flags = NSEvent::modifierFlags_class();
        Mods {
            alt: flags.contains(NSEventModifierFlags::Option),
            shift: flags.contains(NSEventModifierFlags::Shift),
            control: flags.contains(NSEventModifierFlags::Control),
            command: flags.contains(NSEventModifierFlags::Command),
        }
    }

    pub fn sample(at_ms: u64) -> Sample {
        let at = NSEvent::mouseLocation();
        Sample {
            at_ms,
            button: NSEvent::pressedMouseButtons() & 1 == 1,
            drag_seq: Some(drag_pasteboard().changeCount() as i64),
            mods: mods(),
            cursor: (at.x, at.y),
            over_own: false,
        }
    }

    /// The files on the drag pasteboard, as the clipboard reader finds copied files.
    pub fn drag_files() -> Vec<PathBuf> {
        drag_pasteboard()
            .pasteboardItems()
            .map(|items| {
                items
                    .iter()
                    .filter_map(|item| item.stringForType(&NSString::from_str("public.file-url")))
                    .filter_map(|url| NSURL::URLWithString(&url)?.to_file_path())
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The bundle id of the frontmost app.
    pub fn frontmost_app() -> Option<String> {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .and_then(|app| app.bundleIdentifier())
            .map(|id| id.to_string())
    }
}
