//! The shelf panel: a non-activating NSPanel the size of what the page shows,
//! on the bottom edge of the stack's monitor (spec §4).

use ferry_core::geometry::{Corner, Rect};
use ferry_core::i18n::t;
use ferry_core::settings;
use ferry_core::shelf_place::{self, STRIP, WIDTH};
use tauri::{
    AppHandle, DragDropEvent, Manager, Monitor, WebviewUrl, WebviewWindowBuilder, WindowEvent,
};

use super::{SHELF, state};
use crate::platform;
use crate::state::AppState;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, SHELF, WebviewUrl::App("shelf.html".into()))
        .title(t("shelf.window_title"))
        .inner_size(WIDTH, STRIP)
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
    platform::drop::make_shelf_window(&window)?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
            on_drop(&handle, paths.clone());
        }
    });
    Ok(())
}

fn on_drop(app: &AppHandle, paths: Vec<std::path::PathBuf>) {
    if state(app).is_some_and(|s| s.dragging.load(std::sync::atomic::Ordering::SeqCst)) {
        return; // our own pile, dropped back on ourselves (spec §3)
    }
    if paths.is_empty() {
        return; // an image-only drag: spec 3
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || super::add(&app, paths));
}

pub fn corner(app: &AppHandle) -> Corner {
    match app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .corner
    {
        settings::Corner::TopLeft => Corner::TopLeft,
        settings::Corner::TopRight => Corner::TopRight,
        settings::Corner::BottomLeft => Corner::BottomLeft,
        settings::Corner::BottomRight => Corner::BottomRight,
    }
}

/// The stack's monitor when the stack shows, else the one the stack would pick.
pub fn monitor(app: &AppHandle) -> Option<Monitor> {
    let stack = app
        .get_webview_window(crate::windows::STACK)
        .filter(|w| w.is_visible().unwrap_or(false))
        .and_then(|w| w.current_monitor().ok().flatten());
    stack.or_else(|| crate::windows::stack_monitor(app))
}

pub fn area_of(monitor: &Monitor) -> Rect {
    let work = monitor.work_area();
    Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    }
}

/// Size the panel to `height_pt` points (0 hides it) and pin it to the bottom edge.
pub fn layout(app: &AppHandle, height_pt: f64) {
    let Some(state) = state(app) else { return };
    let Some(window) = app.get_webview_window(SHELF) else {
        return;
    };
    let Some(monitor) = monitor(app) else { return };
    let scale = monitor.scale_factor();
    let before = super::inset_px(app, scale);
    if height_pt < 1.0 {
        *state.layout.lock().unwrap() = super::Layout { height_pt: 0.0 };
        let _ = platform::hide(&window);
        restack_if(app, before, scale);
        return;
    }
    let size = (
        (WIDTH * scale).round() as u32,
        (height_pt * scale).round() as u32,
    );
    let margin = (shelf_place::SIDE * scale).round() as u32;
    let rect = shelf_place::shelf_rect(area_of(&monitor), corner(app), size, margin);
    *state.layout.lock().unwrap() = super::Layout { height_pt };
    use tauri::{LogicalPosition, LogicalSize};
    let _ = window.set_size(LogicalSize::new(WIDTH, height_pt));
    let _ = window.set_position(LogicalPosition::new(
        f64::from(rect.x) / scale,
        f64::from(rect.y) / scale,
    ));
    let _ = platform::show_without_focus(&window);
    restack_if(app, before, scale);
}

/// The stack follows the shelf's height: move it when what the shelf takes changed.
fn restack_if(app: &AppHandle, before: u32, scale: f64) {
    if super::inset_px(app, scale) != before {
        crate::windows::restack(app);
    }
    crate::drop_batch::zone::follow_shelf(app);
}
