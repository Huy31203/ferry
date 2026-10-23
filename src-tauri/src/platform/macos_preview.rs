//! macOS focus hand-back: Ferry is an accessory app that is never frontmost,
//! so when the compare window closes it hands focus back to the app the user
//! was in.

use std::sync::Mutex;

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

/// The app that was frontmost before Ferry took focus.
static FRONT: Mutex<Option<Retained<NSRunningApplication>>> = Mutex::new(None);

/// Note the frontmost app before Ferry takes focus; nothing while Ferry
/// itself is active (Settings in front).
pub fn remember_front() {
    let front = if NSRunningApplication::currentApplication().isActive() {
        None
    } else {
        NSWorkspace::sharedWorkspace().frontmostApplication()
    };
    *FRONT.lock().unwrap() = front;
}

/// Activate the app `remember_front` noted, once.
pub fn give_back_focus() {
    if let Some(app) = FRONT.lock().unwrap().take() {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}
