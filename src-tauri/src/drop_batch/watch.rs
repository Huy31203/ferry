//! The drag watcher: a thread that polls the mouse, the modifiers and the drag
//! pasteboard, turns the samples into signals (`ferry_core::drag`), and shows
//! or hides the drop zone. Every modifier tap other than the zone's own goes
//! out app-wide as `drag:tap` for stream F's preset zones.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use ferry_core::drag::{DragWatch, Modifier, Signal, should_auto_show};
use ferry_core::intake::{ZoneMode, all_own, summarise};
use ferry_core::settings::DropZone;
use serde_json::json;
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use super::zone;
use crate::platform;
use crate::state::AppState;

/// Polling while the button is up: about 10 cheap reads a second keeps idle CPU near zero.
pub const IDLE_MS: u64 = 100;
/// Polling while the button is down.
pub const ACTIVE_MS: u64 = 30;

struct Context<'a> {
    zone: &'a DropZone,
    files: &'a [PathBuf],
    all_own: bool,
    frontmost: Option<&'a str>,
    visible: bool,
    windows: bool,
}

#[derive(Debug, PartialEq)]
enum Plan {
    Show { mode: ZoneMode, under_cursor: bool },
    Hide,
    HideAfterGrace,
    EmitTap(Modifier),
    Nothing,
}

fn plan(signal: Signal, ctx: &Context) -> Plan {
    match signal {
        Signal::DragStarted => {
            if should_auto_show(ctx.zone, !ctx.files.is_empty(), ctx.all_own, ctx.frontmost) {
                Plan::Show {
                    mode: ZoneMode::Zone,
                    under_cursor: false,
                }
            } else {
                Plan::Nothing
            }
        }
        // Where there is no drag signal, a press that moves and lasts shows the edge tab,
        // but only when the user turned that on (`show_on` is "Whenever files are dragged").
        Signal::PressMoved => {
            if ctx.windows && should_auto_show(ctx.zone, true, false, ctx.frontmost) {
                Plan::Show {
                    mode: ZoneMode::Tab,
                    under_cursor: false,
                }
            } else {
                Plan::Nothing
            }
        }
        Signal::Tap(m) if m == Modifier::from(ctx.zone.modifier) => {
            if !ctx.zone.enabled {
                Plan::Nothing
            } else if ctx.visible {
                Plan::Hide
            } else {
                Plan::Show {
                    mode: ZoneMode::Zone,
                    under_cursor: true,
                }
            }
        }
        Signal::Tap(m) => Plan::EmitTap(m),
        Signal::DragEnded if ctx.visible => Plan::HideAfterGrace,
        Signal::DragEnded => Plan::Nothing,
    }
}

/// Start the watcher thread.
pub fn start(app: &AppHandle) {
    let app = app.clone();
    let started = std::thread::Builder::new()
        .name("ferry-drag".into())
        .spawn(move || run(&app));
    if let Err(e) = started {
        warn!("could not start the drag watcher: {e}");
    }
}

fn run(app: &AppHandle) {
    let t0 = Instant::now();
    let mut watch = DragWatch::new();
    loop {
        let sample = platform::drop::sample(t0.elapsed().as_millis() as u64);
        for signal in watch.feed(&sample) {
            handle(app, signal);
        }
        let ms = if watch.active() || sample.button {
            ACTIVE_MS
        } else {
            IDLE_MS
        };
        std::thread::sleep(Duration::from_millis(ms));
    }
}

fn handle(app: &AppHandle, signal: Signal) {
    if signal == Signal::DragEnded {
        let _ = app.emit("drag:ended", ()); // stream F: pipelines (preset zones hide with the drag)
    }
    let state = app.state::<AppState>();
    let zone_settings = state.settings.read().unwrap().drop_zone.clone();
    // The drag pasteboard and the frontmost app are read only when a signal needs them.
    let files = if matches!(signal, Signal::DragStarted | Signal::Tap(_)) {
        platform::drop::drag_files()
    } else {
        Vec::new()
    };
    let frontmost = if matches!(signal, Signal::DragEnded) {
        None
    } else {
        platform::drop::frontmost_app()
    };
    let outputs: Vec<PathBuf> = state
        .jobs
        .lock()
        .unwrap()
        .values()
        .map(|entry| entry.output())
        .collect();
    let ctx = Context {
        zone: &zone_settings,
        files: &files,
        all_own: all_own(&files, &outputs),
        frontmost: frontmost.as_deref(),
        visible: zone::visible(app),
        windows: cfg!(windows),
    };
    match plan(signal, &ctx) {
        Plan::Show { mode, under_cursor } => {
            zone::show(app, mode, under_cursor, None);
            if !files.is_empty() {
                // A slow drive never delays the panel: the summary follows it.
                let app = app.clone();
                let limit = zone_settings.batch_threshold as usize + 1;
                tauri::async_runtime::spawn_blocking(move || {
                    zone::set_summary(&app, summarise(&files, limit));
                });
            }
        }
        Plan::Hide => zone::hide(app),
        Plan::HideAfterGrace => zone::hide_after_grace(app),
        Plan::EmitTap(modifier) => {
            let _ = app.emit("drag:tap", json!({ "modifier": modifier }));
        }
        Plan::Nothing => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::settings::ZoneShowOn;

    // Built from explicit zones: the defaults differ by OS and CI runs these on Windows too.
    fn any_drag() -> DropZone {
        DropZone {
            show_on: ZoneShowOn::AnyDrag,
            ignored_apps: vec!["com.figma.Desktop".into()],
            ..DropZone::default()
        }
    }

    fn tap_only() -> DropZone {
        DropZone {
            show_on: ZoneShowOn::Modifier,
            ..any_drag()
        }
    }

    fn ctx<'a>(zone: &'a DropZone, files: &'a [PathBuf]) -> Context<'a> {
        Context {
            zone,
            files,
            all_own: false,
            frontmost: Some("com.apple.finder"),
            visible: false,
            windows: false,
        }
    }

    const SHOW_ZONE: Plan = Plan::Show {
        mode: ZoneMode::Zone,
        under_cursor: false,
    };

    #[test]
    fn a_file_drag_from_another_app_shows_the_zone_near_the_results() {
        let (zone, files) = (any_drag(), [PathBuf::from("/a.png")]);
        assert_eq!(plan(Signal::DragStarted, &ctx(&zone, &files)), SHOW_ZONE);
    }

    #[test]
    fn ferrys_own_drag_and_ignored_apps_show_nothing() {
        let (zone, files) = (any_drag(), [PathBuf::from("/a.png")]);
        let own = Context {
            all_own: true,
            ..ctx(&zone, &files)
        };
        assert_eq!(plan(Signal::DragStarted, &own), Plan::Nothing);
        let figma = Context {
            frontmost: Some("com.figma.Desktop"),
            ..ctx(&zone, &files)
        };
        assert_eq!(plan(Signal::DragStarted, &figma), Plan::Nothing);
        assert_eq!(plan(Signal::DragStarted, &ctx(&zone, &[])), Plan::Nothing);
    }

    #[test]
    fn the_zone_modifier_tap_toggles_the_zone_under_the_cursor() {
        let alt = Signal::Tap(Modifier::Alt);
        for zone in [any_drag(), tap_only()] {
            let hidden = ctx(&zone, &[]);
            assert_eq!(
                plan(alt, &hidden),
                Plan::Show {
                    mode: ZoneMode::Zone,
                    under_cursor: true
                }
            );
            let shown = Context {
                visible: true,
                ..ctx(&zone, &[])
            };
            assert_eq!(plan(alt, &shown), Plan::Hide);
            let off = DropZone {
                enabled: false,
                ..zone.clone()
            };
            assert_eq!(plan(alt, &ctx(&off, &[])), Plan::Nothing);
        }
    }

    #[test]
    fn other_taps_go_to_preset_zones() {
        let zone = any_drag();
        assert_eq!(
            plan(Signal::Tap(Modifier::Control), &ctx(&zone, &[])),
            Plan::EmitTap(Modifier::Control)
        );
    }

    #[test]
    fn windows_shows_the_tab_only_when_auto_show_is_turned_on() {
        let (zone, files) = (any_drag(), [PathBuf::from("/a.png")]);
        let windows = Context {
            windows: true,
            ..ctx(&zone, &[])
        };
        assert_eq!(
            plan(Signal::PressMoved, &windows),
            Plan::Show {
                mode: ZoneMode::Tab,
                under_cursor: false
            }
        );
        assert_eq!(plan(Signal::PressMoved, &ctx(&zone, &[])), Plan::Nothing);
        let tap_only = tap_only();
        let default = Context {
            windows: true,
            ..ctx(&tap_only, &files)
        };
        assert_eq!(plan(Signal::PressMoved, &default), Plan::Nothing);
        assert_eq!(plan(Signal::DragStarted, &default), Plan::Nothing);
    }

    #[test]
    fn the_drag_ending_hides_the_zone_after_a_grace() {
        let zone = any_drag();
        let shown = Context {
            visible: true,
            ..ctx(&zone, &[])
        };
        assert_eq!(plan(Signal::DragEnded, &shown), Plan::HideAfterGrace);
        assert_eq!(plan(Signal::DragEnded, &ctx(&zone, &[])), Plan::Nothing);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn sampling_works_off_the_main_thread() {
        let joined =
            std::thread::spawn(|| (platform::drop::sample(0), platform::drop::drag_files())).join();
        assert!(joined.is_ok());
    }
}
