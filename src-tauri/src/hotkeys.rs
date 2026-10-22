//! The three global hotkeys (spec §3, §5.1, §5.3). A hotkey another app already
//! owns is logged and left out of its tray item; the app keeps running (§6).

use ferry_core::optimise::Level;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{info, warn};

use crate::state::AppState;
use crate::{clipboard, tray};

#[derive(Debug, Clone, Copy)]
enum Action {
    Optimise,
    Aggressive,
    CycleState,
}

/// The accelerators that registered, for the tray's labels.
#[derive(Debug, Clone, Default)]
pub struct Registered {
    pub optimise: Option<String>,
    pub aggressive: Option<String>,
}

pub fn register(app: &AppHandle) -> Registered {
    let hotkeys = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .hotkeys
        .clone();
    let mut registered = Registered::default();
    for (accelerator, action) in [
        (&hotkeys.optimise, Action::Optimise),
        (&hotkeys.aggressive, Action::Aggressive),
        (&hotkeys.cycle_state, Action::CycleState),
    ] {
        let result =
            app.global_shortcut()
                .on_shortcut(accelerator.as_str(), move |app, _, event| {
                    if event.state == ShortcutState::Pressed {
                        act(app, action);
                    }
                });
        match result {
            Ok(()) => {
                info!(%accelerator, ?action, "hotkey registered");
                match action {
                    Action::Optimise => registered.optimise = Some(accelerator.clone()),
                    Action::Aggressive => registered.aggressive = Some(accelerator.clone()),
                    Action::CycleState => {}
                }
            }
            Err(e) => warn!(%accelerator, ?action, "hotkey not registered: {e}"),
        }
    }
    registered
}

fn act(app: &AppHandle, action: Action) {
    match action {
        Action::Optimise => optimise_in_background(app, Level::Normal),
        Action::Aggressive => optimise_in_background(app, Level::Aggressive),
        Action::CycleState => {
            let state = app.state::<AppState>();
            let mut run_state = state.run_state.lock().unwrap();
            *run_state = run_state.next();
            info!(state = ?*run_state, "run state changed");
            drop(run_state);
            tray::rebuild(app);
        }
    }
}

/// Reading a large clipboard image must not block the main thread.
pub fn optimise_in_background(app: &AppHandle, level: Level) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || clipboard::optimise_now(&app, level));
}
