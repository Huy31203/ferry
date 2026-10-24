//! The three global hotkeys (spec §3, §5.1, §5.3). A hotkey another app already
//! owns is logged and left out of its tray item; the app keeps running (§6).

use std::sync::Mutex;

use ferry_core::optimise::Level;
use ferry_core::settings::{Hotkeys, Settings};
use ferry_core::svc_settings::ShortcutSettings;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Modifiers, Shortcut, ShortcutState};
use tracing::{info, warn};

use crate::state::AppState;
use crate::{clipboard, hover_keys, tray};

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

/// What this module holds right now (svc, stream G).
struct Runtime {
    /// Global hotkeys that registered: (id, accelerator).
    held: Vec<(&'static str, String)>,
    /// Global hotkeys whose registration failed.
    failed: Vec<&'static str>,
    /// The Settings window is recording a shortcut: hold nothing.
    suspended: bool,
    /// The statuses at the last `reregister` while not suspended.
    last: Vec<Status>,
}

static RUNTIME: Mutex<Runtime> = Mutex::new(Runtime {
    held: Vec::new(),
    failed: Vec::new(),
    suspended: false,
    last: Vec::new(),
});

fn action_for(id: &str) -> Option<Action> {
    match id {
        "optimise" => Some(Action::Optimise),
        "aggressive" => Some(Action::Aggressive),
        "cycle_state" => Some(Action::CycleState),
        _ => None,
    }
}

fn settings_of(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.read().unwrap().clone()
}

/// Main thread: unregister what this module holds, register the global hotkeys
/// the settings want, refresh the hover keys and the tray, and tell the page.
pub fn reregister(app: &AppHandle) {
    let settings = settings_of(app);
    let mac = cfg!(target_os = "macos");
    let shortcuts = app.global_shortcut();
    // Not held across (un)registering: the tray reads `current` while it rebuilds.
    let (held, suspended) = {
        let mut runtime = RUNTIME.lock().unwrap();
        (std::mem::take(&mut runtime.held), runtime.suspended)
    };
    for (id, accelerator) in held {
        if let Err(e) = shortcuts.unregister(accelerator.as_str()) {
            warn!(%accelerator, id, "hotkey not unregistered: {e}");
        }
    }
    let mut now = Vec::new();
    let mut failed = Vec::new();
    for (id, accelerator) in wanted(&settings, Group::Global, suspended, mac) {
        let Some(action) = action_for(id) else {
            continue;
        };
        let result = shortcuts.on_shortcut(accelerator.as_str(), move |app, _, event| {
            if event.state == ShortcutState::Pressed {
                act(app, action);
            }
        });
        match result {
            Ok(()) => {
                info!(%accelerator, ?action, "hotkey registered");
                now.push((id, accelerator));
            }
            Err(e) => {
                warn!(%accelerator, ?action, "hotkey not registered: {e}");
                failed.push(id);
            }
        }
    }
    {
        let mut runtime = RUNTIME.lock().unwrap();
        runtime.held = now;
        runtime.failed = failed;
    }
    hover_keys::refresh(app);
    if !suspended {
        let statuses = statuses(&settings);
        RUNTIME.lock().unwrap().last = statuses;
    }
    tray::rebuild(app);
    let _ = app.emit("svc:hotkeys", ());
}

/// Main thread: while the Settings window records a shortcut, Ferry holds none
/// of its own, so the chord reaches the field instead of triggering an action.
pub fn suspend(app: &AppHandle, on: bool) {
    {
        let mut runtime = RUNTIME.lock().unwrap();
        if runtime.suspended == on {
            return;
        }
        runtime.suspended = on;
    }
    hover_keys::set_suspended(app, on);
    reregister(app);
}

/// The registered global accelerators, for the tray's labels.
pub fn current() -> Registered {
    let runtime = RUNTIME.lock().unwrap();
    let find = |id: &str| {
        runtime
            .held
            .iter()
            .find(|(held, _)| *held == id)
            .map(|(_, accelerator)| accelerator.clone())
    };
    Registered {
        optimise: find("optimise"),
        aggressive: find("aggressive"),
    }
}

/// The accelerator to show beside a card menu item; `None` when the key is
/// off, blocked, or hover hotkeys are off.
pub fn hover_accelerator(app: &AppHandle, id: &str) -> Option<String> {
    let settings = settings_of(app);
    if !settings.floating.hover_hotkeys || !is_enabled(&settings, id) {
        return None;
    }
    let chord = accelerator(&settings, id)?;
    check(&settings, id, &chord, cfg!(target_os = "macos"))
        .problem
        .is_none()
        .then_some(chord)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum KeyState {
    Registered,
    /// A valid hover key: held only while the pointer rests on a card.
    Ready,
    Off,
    Failed,
    Blocked,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Status {
    pub id: &'static str,
    pub group: Group,
    pub accelerator: Option<String>,
    pub enabled: bool,
    pub state: KeyState,
    pub problem: Option<Problem>,
}

fn statuses(settings: &Settings) -> Vec<Status> {
    let mac = cfg!(target_os = "macos");
    let (held, failed) = {
        let runtime = RUNTIME.lock().unwrap();
        (runtime.held.clone(), runtime.failed.clone())
    };
    let hover_failed = hover_keys::failed();
    BINDINGS
        .iter()
        .map(|&(id, group)| {
            let chord = accelerator(settings, id);
            let enabled = is_enabled(settings, id);
            let problem = match &chord {
                Some(chord) => check(settings, id, chord, mac).problem,
                None => check(settings, id, "", mac).problem,
            };
            let state = if !enabled {
                KeyState::Off
            } else if problem.is_some() {
                KeyState::Blocked
            } else if group == Group::Global {
                if failed.contains(&id) {
                    KeyState::Failed
                } else if held.iter().any(|(held, _)| *held == id) {
                    KeyState::Registered
                } else {
                    KeyState::Ready
                }
            } else {
                let mine = |accelerator: &String| match &chord {
                    Some(chord) => accelerator == chord,
                    None => DIGITS.contains(&accelerator.as_str()),
                };
                if hover_failed.iter().any(mine) {
                    KeyState::Failed
                } else {
                    KeyState::Ready
                }
            };
            Status {
                id,
                group,
                accelerator: chord,
                enabled,
                state,
                problem,
            }
        })
        .collect()
}

/// Every binding's state for the Hotkeys pane. While suspended, the statuses
/// from before recording started.
pub fn status(app: &AppHandle) -> Vec<Status> {
    let (suspended, last) = {
        let runtime = RUNTIME.lock().unwrap();
        (runtime.suspended, runtime.last.clone())
    };
    if suspended && !last.is_empty() {
        last
    } else {
        statuses(&settings_of(app))
    }
}

// svc (stream G): hotkey bindings and their conflict check.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Group {
    Global,
    Hover,
}

/// Every hotkey Ferry owns, with its stable id. Another stream adds a hotkey by
/// adding a row here, its default to `default_accelerator` and its storage to
/// `accelerator` and `set_accelerator`.
pub const BINDINGS: [(&str, Group); 7] = [
    ("optimise", Group::Global),
    ("aggressive", Group::Global),
    ("cycle_state", Group::Global),
    ("preview", Group::Hover),
    ("compare", Group::Hover),
    ("step_down", Group::Hover),
    ("scale", Group::Hover),
];

/// The fixed `1`-`9` set `scale` stands for.
pub const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];

const RESERVED_MAC: &[&str] = &[
    "Cmd+Space",
    "Ctrl+Space",
    "Cmd+Tab",
    "Cmd+Shift+Tab",
    "Cmd+Q",
    "Cmd+W",
    "Cmd+H",
    "Cmd+M",
    "Cmd+Alt+H",
    "Cmd+Alt+Escape",
    "Ctrl+Cmd+Q",
    "Ctrl+Cmd+F",
    "Ctrl+Cmd+Space",
    "Cmd+Shift+3",
    "Cmd+Shift+4",
    "Cmd+Shift+5",
    "Cmd+C",
    "Cmd+V",
    "Cmd+X",
    "Cmd+Z",
    "Cmd+A",
    "Cmd+S",
];

const RESERVED_WINDOWS: &[&str] = &[
    "Alt+Tab",
    "Alt+F4",
    "Alt+Space",
    "Ctrl+Escape",
    "Ctrl+Shift+Escape",
    "Ctrl+Alt+Delete",
    "Super+L",
    "Super+D",
    "Super+E",
    "Super+R",
    "Super+Tab",
    "Ctrl+C",
    "Ctrl+V",
    "Ctrl+X",
    "Ctrl+Z",
    "Ctrl+Y",
    "Ctrl+A",
    "Ctrl+S",
];

/// Keys cards use themselves; a hover hotkey may not take them.
const CARD_KEYS: &[&str] = &["Escape", "Tab", "Enter", "Backspace"];

/// Browsers open developer tools with these; warned about on Windows only.
const DEV_TOOLS: &[&str] = &["Ctrl+Shift+C", "Ctrl+Shift+I", "Ctrl+Shift+J"];

fn group_of(id: &str) -> Option<Group> {
    BINDINGS
        .iter()
        .find(|(known, _)| *known == id)
        .map(|(_, group)| *group)
}

/// `None` for `scale` (a fixed set) and for unknown ids.
pub fn accelerator(settings: &Settings, id: &str) -> Option<String> {
    let hotkeys = &settings.hotkeys;
    let shortcuts = &settings.svc.shortcuts;
    Some(match id {
        "optimise" => hotkeys.optimise.clone(),
        "aggressive" => hotkeys.aggressive.clone(),
        "cycle_state" => hotkeys.cycle_state.clone(),
        "preview" => shortcuts.preview.clone(),
        "compare" => shortcuts.compare.clone(),
        "step_down" => shortcuts.step_down.clone(),
        _ => return None,
    })
}

/// False for `scale` and for unknown ids.
pub fn set_accelerator(settings: &mut Settings, id: &str, accelerator: &str) -> bool {
    let slot = match id {
        "optimise" => &mut settings.hotkeys.optimise,
        "aggressive" => &mut settings.hotkeys.aggressive,
        "cycle_state" => &mut settings.hotkeys.cycle_state,
        "preview" => &mut settings.svc.shortcuts.preview,
        "compare" => &mut settings.svc.shortcuts.compare,
        "step_down" => &mut settings.svc.shortcuts.step_down,
        _ => return false,
    };
    *slot = accelerator.to_string();
    true
}

pub fn default_accelerator(id: &str, mac: bool) -> Option<String> {
    let global = Hotkeys::for_platform(mac);
    let hover = ShortcutSettings::default();
    Some(match id {
        "optimise" => global.optimise,
        "aggressive" => global.aggressive,
        "cycle_state" => global.cycle_state,
        "preview" => hover.preview,
        "compare" => hover.compare,
        "step_down" => hover.step_down,
        _ => return None,
    })
}

pub fn is_enabled(settings: &Settings, id: &str) -> bool {
    !settings.svc.shortcuts.disabled.iter().any(|d| d == id)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Problem {
    Invalid,
    NeedsModifier,
    Reserved,
    CardKey,
    Duplicate { with: String },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Warning {
    DevTools,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Verdict {
    pub problem: Option<Problem>,
    pub warning: Option<Warning>,
}

fn blocked(problem: Problem) -> Verdict {
    Verdict {
        problem: Some(problem),
        warning: None,
    }
}

fn listed(list: &[&str], chord: &Shortcut) -> bool {
    list.iter()
        .filter_map(|item| item.parse::<Shortcut>().ok())
        .any(|item| item.id() == chord.id())
}

/// Would `proposed` be fine for binding `id`, given every *other* enabled
/// binding in `settings`? Two chords are equal when their ids are.
pub fn check(settings: &Settings, id: &str, proposed: &str, mac: bool) -> Verdict {
    if id == "scale" {
        // Turning the digits on: no other enabled hotkey may already hold one.
        let held = BINDINGS
            .iter()
            .map(|(other, _)| *other)
            .filter(|other| *other != "scale" && is_enabled(settings, other))
            .find(|other| {
                accelerator(settings, other)
                    .and_then(|a| a.parse::<Shortcut>().ok())
                    .is_some_and(|a| listed(&DIGITS, &a))
            });
        return Verdict {
            problem: held.map(|other| Problem::Duplicate {
                with: other.to_string(),
            }),
            warning: None,
        };
    }
    let Some(group) = group_of(id) else {
        return blocked(Problem::Invalid);
    };
    let Ok(chord) = proposed.trim().parse::<Shortcut>() else {
        return blocked(Problem::Invalid);
    };
    let main = Modifiers::CONTROL | Modifiers::ALT | Modifiers::SUPER;
    if group == Group::Global && !chord.mods.intersects(main) {
        return blocked(Problem::NeedsModifier);
    }
    if group == Group::Hover && listed(CARD_KEYS, &chord) {
        return blocked(Problem::CardKey);
    }
    if listed(if mac { RESERVED_MAC } else { RESERVED_WINDOWS }, &chord) {
        return blocked(Problem::Reserved);
    }
    for (other, _) in BINDINGS.iter().filter(|(other, _)| *other != id) {
        if !is_enabled(settings, other) {
            continue;
        }
        let clash = if *other == "scale" {
            listed(&DIGITS, &chord)
        } else {
            accelerator(settings, other)
                .and_then(|a| a.parse::<Shortcut>().ok())
                .is_some_and(|a| a.id() == chord.id())
        };
        if clash {
            return blocked(Problem::Duplicate {
                with: other.to_string(),
            });
        }
    }
    Verdict {
        problem: None,
        warning: (!mac && listed(DEV_TOOLS, &chord)).then_some(Warning::DevTools),
    }
}

/// The chords to register now: enabled bindings of `group` whose own accelerator
/// passes `check`; `scale` expands to the nine digits. Empty when `suspended`.
pub fn wanted(
    settings: &Settings,
    group: Group,
    suspended: bool,
    mac: bool,
) -> Vec<(&'static str, String)> {
    let mut out = Vec::new();
    if suspended {
        return out;
    }
    for (id, bound) in BINDINGS {
        if bound != group || !is_enabled(settings, id) {
            continue;
        }
        if id == "scale" {
            out.extend(DIGITS.iter().map(|digit| (id, digit.to_string())));
        } else if let Some(chord) = accelerator(settings, id)
            && check(settings, id, &chord, mac).problem.is_none()
        {
            out.push((id, chord));
        }
    }
    out
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

#[cfg(test)]
mod tests {
    use super::*;

    const MAC: bool = cfg!(target_os = "macos");

    fn is(verdict: &Verdict, problem: Option<Problem>) -> bool {
        verdict.problem == problem
    }

    #[test]
    fn every_binding_has_a_default_on_both_systems() {
        for (id, _) in BINDINGS.iter().filter(|(id, _)| *id != "scale") {
            for mac in [true, false] {
                let accelerator = default_accelerator(id, mac).unwrap();
                assert!(accelerator.parse::<Shortcut>().is_ok(), "{id} {mac}");
            }
        }
    }

    #[test]
    fn defaults_have_no_conflicts_on_either_system() {
        let s = Settings::default();
        for (id, _) in BINDINGS.iter().filter(|(id, _)| *id != "scale") {
            let chord = accelerator(&s, id).unwrap();
            assert_eq!(check(&s, id, &chord, MAC).problem, None, "{id}");
        }
    }

    #[test]
    fn a_global_hotkey_needs_ctrl_alt_or_cmd() {
        let s = Settings::default();
        for bare in ["Shift+C", "C"] {
            assert!(is(
                &check(&s, "optimise", bare, MAC),
                Some(Problem::NeedsModifier)
            ));
        }
        assert!(is(&check(&s, "optimise", "Alt+O", MAC), None));
    }

    #[test]
    fn a_hover_key_may_be_bare_but_not_a_card_key() {
        let s = Settings::default();
        assert!(is(&check(&s, "preview", "P", MAC), None));
        for key in ["Escape", "Tab"] {
            assert!(is(&check(&s, "preview", key, MAC), Some(Problem::CardKey)));
        }
    }

    #[test]
    fn a_chord_used_by_another_ferry_hotkey_is_a_duplicate() {
        let mut s = Settings::default();
        s.hotkeys.optimise = "Ctrl+Shift+C".into();
        assert!(is(
            &check(&s, "preview", "Shift+Ctrl+C", MAC),
            Some(Problem::Duplicate {
                with: "optimise".into()
            })
        ));
    }

    #[test]
    fn a_disabled_hotkey_does_not_block_its_chord() {
        let mut s = Settings::default();
        s.hotkeys.optimise = "Ctrl+Shift+C".into();
        s.svc.shortcuts.disabled.push("optimise".into());
        assert!(is(&check(&s, "preview", "Shift+Ctrl+C", MAC), None));
    }

    #[test]
    fn digits_conflict_only_while_scale_is_on() {
        let mut s = Settings::default();
        assert!(is(
            &check(&s, "step_down", "5", MAC),
            Some(Problem::Duplicate {
                with: "scale".into()
            })
        ));
        s.svc.shortcuts.disabled.push("scale".into());
        assert!(is(&check(&s, "step_down", "5", MAC), None));
    }

    #[test]
    fn the_digits_cannot_come_back_while_another_hotkey_holds_one() {
        let mut s = Settings::default();
        s.svc.shortcuts.disabled.push("scale".into());
        s.svc.shortcuts.step_down = "5".into();
        assert!(is(
            &check(&s, "scale", "", MAC),
            Some(Problem::Duplicate {
                with: "step_down".into()
            })
        ));
        s.svc.shortcuts.step_down = "-".into();
        assert!(is(&check(&s, "scale", "", MAC), None));
    }

    #[test]
    fn system_shortcuts_are_refused() {
        let s = Settings::default();
        for chord in ["Cmd+Space", "Cmd+C"] {
            assert!(is(
                &check(&s, "optimise", chord, true),
                Some(Problem::Reserved)
            ));
        }
        assert!(is(
            &check(&s, "optimise", "Ctrl+Alt+Delete", false),
            Some(Problem::Reserved)
        ));
        assert!(is(
            &check(&s, "compare", "Alt+Tab", false),
            Some(Problem::Reserved)
        ));
    }

    #[test]
    fn windows_warns_about_browser_dev_tools() {
        let s = Settings::default();
        let windows = check(&s, "optimise", "Ctrl+Shift+I", false);
        assert_eq!(windows.problem, None);
        assert_eq!(windows.warning, Some(Warning::DevTools));
        assert_eq!(check(&s, "optimise", "Ctrl+Shift+I", true).warning, None);
    }

    #[test]
    fn junk_is_invalid() {
        let s = Settings::default();
        for junk in ["", "Ctrl+", "Ctrl+Shift+Banana", "Ctrl+C+D"] {
            assert!(
                is(&check(&s, "optimise", junk, MAC), Some(Problem::Invalid)),
                "{junk:?}"
            );
        }
    }

    #[test]
    fn every_listed_shortcut_parses() {
        for list in [
            RESERVED_MAC,
            RESERVED_WINDOWS,
            CARD_KEYS,
            DEV_TOOLS,
            &DIGITS,
        ] {
            for item in list {
                assert!(item.parse::<Shortcut>().is_ok(), "{item}");
            }
        }
    }

    #[test]
    fn suspended_runtime_registers_nothing() {
        let mut s = Settings::default();
        assert!(wanted(&s, Group::Global, true, MAC).is_empty());
        assert_eq!(wanted(&s, Group::Global, false, MAC).len(), 3);
        assert_eq!(wanted(&s, Group::Hover, false, MAC).len(), 3 + 9);
        s.svc.shortcuts.disabled.push("scale".into());
        assert_eq!(wanted(&s, Group::Hover, false, MAC).len(), 3);
    }

    #[test]
    fn set_accelerator_writes_the_right_field() {
        let mut s = Settings::default();
        assert!(set_accelerator(&mut s, "cycle_state", "Ctrl+Alt+Z"));
        assert_eq!(s.hotkeys.cycle_state, "Ctrl+Alt+Z");
        assert!(set_accelerator(&mut s, "compare", "K"));
        assert_eq!(s.svc.shortcuts.compare, "K");
        assert!(!set_accelerator(&mut s, "scale", "K"));
        assert!(!set_accelerator(&mut s, "nope", "K"));
    }
}
