//! Hover hotkeys (spec §2, §4.5): while the pointer rests on an idle card,
//! Space previews it, ⌘/Ctrl+D compares it, `1`–`9` downscale it to 10–90 %
//! and `-` steps it down 10 points. The stack never has the keyboard, so the
//! keys are global, and they are held only while there is a hovered card, the
//! stack is not key and `floating.hover_hotkeys` is on.
//!
//! Every caller (`card_hover`, `stack_focus`, `stack_resize`, the key handler)
//! runs on the main thread, where the global-shortcut plugin registers keys.

use std::sync::Mutex;

use ferry_core::edit::EditsPatch;
use ferry_core::job::JobId;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{debug, warn};

use crate::state::AppState;
use crate::{jobs, menu, platform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Preview,
    Compare,
    StepDown,
    /// Downscale to this percent.
    Scale(u8),
}

const KEYS: [(&str, Key); 12] = [
    ("Space", Key::Preview),
    ("CmdOrCtrl+D", Key::Compare),
    ("-", Key::StepDown),
    ("1", Key::Scale(10)),
    ("2", Key::Scale(20)),
    ("3", Key::Scale(30)),
    ("4", Key::Scale(40)),
    ("5", Key::Scale(50)),
    ("6", Key::Scale(60)),
    ("7", Key::Scale(70)),
    ("8", Key::Scale(80)),
    ("9", Key::Scale(90)),
];

struct Keys {
    /// The idle card under the pointer, as the page reports it.
    card: Option<JobId>,
    /// The stack has the keyboard (a text field is open).
    keyed: bool,
    /// The accelerators this module registered, to unregister exactly those.
    held: Vec<&'static str>,
    /// A registration failure was logged once already (spec §6).
    warned: bool,
}

static STATE: Mutex<Keys> = Mutex::new(Keys {
    card: None,
    keyed: false,
    held: Vec::new(),
    warned: false,
});

/// Whether to register (`Some(true)`) or unregister (`Some(false)`) now, or
/// leave the keys as they are (`None`).
fn plan(held: bool, card: bool, enabled: bool, keyed: bool) -> Option<bool> {
    let want = card && enabled && !keyed;
    (want != held).then_some(want)
}

/// The scale `-` steps to: 10 points below the current one (none is 100 %),
/// never under 10 %; `None` when it is already 10 %.
fn step_down(scale: Option<u8>) -> Option<u8> {
    let next = scale.unwrap_or(100).saturating_sub(10).max(10);
    (Some(next) != scale).then_some(next)
}

/// The page's hovered idle card, or `None` when the pointer left it.
pub fn set(app: &AppHandle, card: Option<JobId>) {
    STATE.lock().unwrap().card = card;
    apply(app);
}

/// The stack took or gave back the keyboard: typing must reach the field.
pub fn set_keyed(app: &AppHandle, keyed: bool) {
    STATE.lock().unwrap().keyed = keyed;
    apply(app);
}

fn apply(app: &AppHandle) {
    let enabled = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .hover_hotkeys;
    // Not held across (un)registering: the key handler takes the same lock.
    let (change, held) = {
        let state = STATE.lock().unwrap();
        let change = plan(
            !state.held.is_empty(),
            state.card.is_some(),
            enabled,
            state.keyed,
        );
        (change, state.held.clone())
    };
    let shortcuts = app.global_shortcut();
    match change {
        Some(true) => {
            let mut now = Vec::new();
            let mut failed = Vec::new();
            for (accelerator, key) in KEYS {
                let result = shortcuts.on_shortcut(accelerator, move |app, _, event| {
                    if event.state == ShortcutState::Pressed {
                        pressed(app, key);
                    }
                });
                match result {
                    Ok(()) => now.push(accelerator),
                    Err(e) => failed.push(format!("{accelerator}: {e}")),
                }
            }
            let mut state = STATE.lock().unwrap();
            if !failed.is_empty() && !state.warned {
                state.warned = true;
                warn!(?failed, "hover hotkeys not registered");
            }
            state.held = now;
        }
        Some(false) => {
            for accelerator in held {
                if let Err(e) = shortcuts.unregister(accelerator) {
                    debug!(accelerator, "hover hotkey not unregistered: {e}");
                }
            }
            STATE.lock().unwrap().held.clear();
        }
        None => {}
    }
}

fn pressed(app: &AppHandle, key: Key) {
    let Some(id) = STATE.lock().unwrap().card.clone() else {
        return;
    };
    let scale = match key {
        Key::Preview | Key::Compare => {
            let action = if key == Key::Preview {
                "preview"
            } else {
                "compare"
            };
            menu::send(app, id, action);
            return;
        }
        // Ferry has the keyboard (the compare window or Quick Look): digits
        // and `-` are not ours to take (spec §1).
        _ if platform::app_is_active() => return,
        Key::Scale(percent) => Some(percent),
        Key::StepDown => {
            let current = app
                .state::<AppState>()
                .jobs
                .lock()
                .unwrap()
                .get(&id)
                .map(|entry| entry.last.lock().unwrap().edits.scale);
            current.and_then(step_down)
        }
    };
    let Some(scale) = scale else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let patch = EditsPatch {
            scale: Some(Some(scale)),
            ..EditsPatch::default()
        };
        if let Err(e) = jobs::edit(&app, &id, patch) {
            debug!("hover downscale: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn keys_are_held_only_over_a_card_with_the_setting_on() {
        assert_eq!(plan(false, true, true, false), Some(true));
        assert_eq!(plan(true, false, true, false), Some(false));
        assert_eq!(plan(true, true, false, false), Some(false));
        assert_eq!(plan(false, true, false, false), None);
        assert_eq!(plan(false, false, true, false), None);
    }

    #[test]
    fn moving_between_cards_keeps_the_keys() {
        assert_eq!(plan(true, true, true, false), None);
    }

    #[test]
    fn a_key_window_never_keeps_the_hover_keys() {
        assert_eq!(plan(true, true, true, true), Some(false));
        assert_eq!(plan(false, true, true, true), None);
    }

    #[test]
    fn minus_steps_ten_points_down_to_ten() {
        assert_eq!(step_down(None), Some(90));
        assert_eq!(step_down(Some(55)), Some(45));
        assert_eq!(step_down(Some(15)), Some(10));
        assert_eq!(step_down(Some(10)), None);
    }

    #[test]
    fn every_accelerator_parses() {
        for (accelerator, _) in KEYS {
            assert!(accelerator.parse::<Shortcut>().is_ok(), "{accelerator}");
        }
    }
}
