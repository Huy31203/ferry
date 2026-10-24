//! The drop zone: a small floating panel that appears while files are dragged
//! (or on a modifier tap) and is itself the drop target. It is created hidden
//! at startup like the stack, so its page is loaded before the first drag.

use std::time::Duration;

use ferry_core::drag::{tab_at_edge, zone_from_tab, zone_near_results, zone_under_cursor};
use ferry_core::geometry::{Corner, Rect};
use ferry_core::i18n::t;
use ferry_core::intake::{Summary, ZoneMode, ZoneState, ZoneView, summarise, zone_view};
use ferry_core::settings::{self, ZonePosition};
use tauri::menu::MenuItem;
use tauri::{
    AppHandle, DragDropEvent, Emitter, Manager, Monitor, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder, WindowEvent, Wry,
};
use tracing::warn;

use super::{DropState, intake};
use crate::platform;
use crate::state::AppState;
use crate::windows::STACK;

pub const ZONE: &str = "drop";
/// Logical size: the 220 x 140 content plus 12 px of padding for the hover scale and shadow.
pub const ZONE_SIZE: (f64, f64) = (244.0, 164.0);
pub const TAB_SIZE: (f64, f64) = (24.0, 140.0);
/// A zone opened from the tray menu hides itself after this long if nothing hovers it.
pub const MENU_SHOW_SECS: u64 = 10;
/// A drag that ends shortly before its drop event still gets the drop.
pub const DROP_GRACE_MS: u64 = 300;
/// The drop collapse animation, plus a little.
pub const COLLAPSE_MS: u64 = 220;
/// Matches the stack's margin: both pages pad their content by 12 px.
const MARGIN: f64 = 4.0;

pub struct ZoneCtl {
    pub visible: bool,
    pub mode: ZoneMode,
    pub summary: Option<Summary>,
    pub hovering: bool,
    pub keep: bool,
    pub dropped: bool,
    /// Bumped by every show and hide, so a delayed hide can tell it is stale.
    pub generation: u64,
}

impl Default for ZoneCtl {
    fn default() -> Self {
        Self {
            visible: false,
            mode: ZoneMode::Zone,
            summary: None,
            hovering: false,
            keep: false,
            dropped: false,
            generation: 0,
        }
    }
}

/// Create the hidden zone window.
pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, ZONE, WebviewUrl::App("drop.html".into()))
        .title(t("drop.zone.window_title"))
        .inner_size(ZONE_SIZE.0, ZONE_SIZE.1)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .content_protected(!show_in_screenshots)
        .visible(false)
        .build()?;
    platform::drop::make_zone_window(&window)?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::DragDrop(drag) = event {
            on_drag_drop(&handle, drag);
        }
    });
    Ok(())
}

pub fn tray_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>> {
    MenuItem::with_id(app, "drop:zone", t("drop.tray.zone"), true, None::<&str>)
}

/// Run `f` on the zone's state, holding its lock only for the call.
fn with_zone<R>(app: &AppHandle, f: impl FnOnce(&mut ZoneCtl) -> R) -> R {
    let state = app.state::<DropState>();
    let mut ctl = state.zone.lock().unwrap();
    f(&mut ctl)
}

fn threshold(app: &AppHandle) -> u32 {
    app.state::<AppState>()
        .settings
        .read()
        .unwrap()
        .drop_zone
        .batch_threshold
}

fn current_view(app: &AppHandle) -> ZoneView {
    let threshold = threshold(app);
    with_zone(app, |ctl| {
        let mut view = zone_view(
            ctl.summary.as_ref(),
            ctl.hovering,
            ctl.keep,
            threshold,
            ctl.mode,
        );
        if !ctl.visible {
            view.state = ZoneState::Hidden;
        } else if ctl.dropped {
            view.state = ZoneState::Dropped;
        }
        view
    })
}

/// Send the current view to the zone page. Never call it with the zone lock held.
fn emit_view(app: &AppHandle) {
    if let Err(e) = app.emit_to(ZONE, "drop:view", current_view(app)) {
        warn!("could not update the drop zone: {e}");
    }
}

#[tauri::command]
pub fn zone_ready(app: AppHandle) -> ZoneView {
    current_view(&app)
}

pub fn visible(app: &AppHandle) -> bool {
    with_zone(app, |ctl| ctl.visible)
}

/// "Show drop zone" in the tray: it hides again after a while if nothing is dragged to it.
pub fn show_from_menu(app: &AppHandle) {
    let generation = show(app, ZoneMode::Zone, false, None);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(MENU_SHOW_SECS));
        let idle = with_zone(&app, |ctl| ctl.generation == generation && !ctl.hovering);
        if idle {
            hide(&app);
        }
    });
}

/// Show the zone (or the edge tab) and return its generation.
pub fn show(app: &AppHandle, mode: ZoneMode, under_cursor: bool, summary: Option<Summary>) -> u64 {
    let generation = with_zone(app, |ctl| {
        ctl.generation += 1;
        ctl.visible = true;
        ctl.mode = mode;
        ctl.summary = summary;
        ctl.hovering = false;
        ctl.dropped = false;
        ctl.keep = false;
        ctl.generation
    });
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        let Some(window) = handle.get_webview_window(ZONE) else {
            return;
        };
        place(&handle, &window, mode, under_cursor, false);
        let _ = platform::show_without_focus(&window);
        emit_view(&handle);
    });
    if let Err(e) = result {
        warn!("could not show the drop zone: {e}");
    }
    generation
}

/// The files under a drag are known a moment after the zone appears.
pub fn set_summary(app: &AppHandle, summary: Summary) {
    let shown = with_zone(app, |ctl| {
        if ctl.visible {
            ctl.summary = Some(summary);
        }
        ctl.visible
    });
    if shown {
        emit_view(app);
    }
}

pub fn hide(app: &AppHandle) {
    with_zone(app, |ctl| {
        ctl.generation += 1;
        ctl.visible = false;
        ctl.hovering = false;
        ctl.dropped = false;
        ctl.summary = None;
    });
    let handle = app.clone();
    let result = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(ZONE) {
            let _ = platform::hide(&window);
        }
        emit_view(&handle);
    });
    if let Err(e) = result {
        warn!("could not hide the drop zone: {e}");
    }
}

/// The monitor under the pointer, as the stack finds it (macOS reports the
/// cursor in points times the main display's scale).
pub fn cursor_monitor(app: &AppHandle) -> Option<Monitor> {
    app.cursor_position()
        .ok()
        .and_then(|p| {
            #[cfg(target_os = "macos")]
            let main_scale = app
                .primary_monitor()
                .ok()
                .flatten()
                .map_or(1.0, |m| m.scale_factor());
            #[cfg(not(target_os = "macos"))]
            let main_scale = 1.0;
            app.monitor_from_point(p.x / main_scale, p.y / main_scale)
                .ok()
                .flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten())
}

/// Size and move the window; `from_tab` puts the full zone where the tab was.
fn place(
    app: &AppHandle,
    window: &WebviewWindow,
    mode: ZoneMode,
    under_cursor: bool,
    from_tab: bool,
) {
    let Some(monitor) = cursor_monitor(app) else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let (floating, drop_zone) = {
        let state = app.state::<AppState>();
        let settings = state.settings.read().unwrap();
        (settings.floating.clone(), settings.drop_zone.clone())
    };
    let corner = match floating.corner {
        settings::Corner::TopLeft => Corner::TopLeft,
        settings::Corner::TopRight => Corner::TopRight,
        settings::Corner::BottomLeft => Corner::BottomLeft,
        settings::Corner::BottomRight => Corner::BottomRight,
    };
    let cursor = app.cursor_position().map_or(
        (
            area.x + area.width as i32 / 2,
            area.y + area.height as i32 / 2,
        ),
        |p| (p.x as i32, p.y as i32),
    );
    let logical = if mode == ZoneMode::Tab {
        TAB_SIZE
    } else {
        ZONE_SIZE
    };
    let size = (
        (logical.0 * scale).round() as u32,
        (logical.1 * scale).round() as u32,
    );
    let (x, y) = if mode == ZoneMode::Tab {
        tab_at_edge(area, corner, size)
    } else if from_tab {
        zone_from_tab(area, corner, size, cursor.1)
    } else if under_cursor || drop_zone.position == ZonePosition::UnderCursor {
        zone_under_cursor(area, size, cursor)
    } else {
        let stack = app
            .get_webview_window(STACK)
            .filter(|w| w.is_visible().unwrap_or(false))
            .and_then(|w| w.outer_size().ok())
            .map(|s| (s.width, s.height));
        let margin = (MARGIN * scale).round() as u32;
        zone_near_results(area, corner, size, stack, margin, 0)
    };
    apply_geometry(window, scale, size, (x, y));
}

/// The stack's per-OS way of sizing and moving a window (`windows::stack_resize`).
fn apply_geometry(window: &WebviewWindow, scale: f64, size: (u32, u32), at: (i32, i32)) {
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_size(LogicalSize::new(
            f64::from(size.0) / scale,
            f64::from(size.1) / scale,
        ));
        let _ = window.set_position(LogicalPosition::new(
            f64::from(at.0) / scale,
            f64::from(at.1) / scale,
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let _ = scale;
        // Move first so the window is on the target monitor before it is sized.
        let _ = window.set_position(PhysicalPosition::new(at.0, at.1));
        let _ = window.set_size(PhysicalSize::new(size.0, size.1));
        let _ = window.set_position(PhysicalPosition::new(at.0, at.1));
    }
}

/// Hide shortly after a drag ends, unless a drop arrived or the zone was shown again.
pub fn hide_after_grace(app: &AppHandle) {
    let generation = with_zone(app, |ctl| ctl.generation);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(DROP_GRACE_MS));
        if with_zone(&app, |ctl| ctl.generation == generation && !ctl.dropped) {
            hide(&app);
        }
    });
}

/// Hold the keep modifier to write results beside the originals. Read while a
/// drag is over the zone and again at the drop; emits only when it changed.
fn refresh_keep(app: &AppHandle) -> bool {
    let key = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .drop_zone
        .keep_original_modifier;
    let keep = platform::drop::mods().has(key.into());
    if with_zone(app, |ctl| std::mem::replace(&mut ctl.keep, keep) != keep) {
        emit_view(app);
    }
    keep
}

/// The edge tab becomes the full zone once a drag enters it.
fn expand_tab(app: &AppHandle) {
    with_zone(app, |ctl| ctl.mode = ZoneMode::Zone);
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(ZONE) {
            place(&handle, &window, ZoneMode::Zone, false, true);
        }
    });
}

fn on_drag_drop(app: &AppHandle, event: &DragDropEvent) {
    match event {
        DragDropEvent::Enter { paths, .. } => {
            let (app, paths) = (app.clone(), paths.clone());
            tauri::async_runtime::spawn_blocking(move || {
                let summary = summarise(&paths, threshold(&app) as usize + 1);
                refresh_keep(&app);
                let tab = with_zone(&app, |ctl| {
                    ctl.summary = Some(summary);
                    ctl.hovering = true;
                    ctl.mode == ZoneMode::Tab
                });
                if tab {
                    expand_tab(&app);
                }
                emit_view(&app);
            });
        }
        DragDropEvent::Over { .. } => {
            refresh_keep(app);
        }
        DragDropEvent::Leave => {
            with_zone(app, |ctl| ctl.hovering = false);
            emit_view(app);
        }
        DragDropEvent::Drop { paths, .. } => {
            let keep = refresh_keep(app);
            let generation = with_zone(app, |ctl| {
                ctl.dropped = true;
                ctl.hovering = false;
                ctl.generation
            });
            emit_view(app);
            let (worker, paths) = (app.clone(), paths.clone());
            tauri::async_runtime::spawn_blocking(move || intake::accept(&worker, paths, keep));
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(COLLAPSE_MS));
                let same = with_zone(&app, |ctl| ctl.generation == generation);
                if same {
                    hide(&app);
                }
            });
        }
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_drop_zone_follows_the_screenshot_setting_and_the_batch_window_does_not() {
        assert!(crate::windows::follows_screenshot_setting(ZONE));
        assert!(!crate::windows::follows_screenshot_setting(
            crate::drop_batch::batch::BATCH
        ));
    }
}
