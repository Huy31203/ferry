use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use super::{ShelfView, view, window};

#[tauri::command]
pub fn shelf_view(app: AppHandle) -> ShelfView {
    super::mutate(&app, |list| list.prune());
    view(&app)
}

#[tauri::command]
pub async fn shelf_add(app: AppHandle, paths: Vec<String>) {
    let paths = paths.into_iter().map(PathBuf::from).collect();
    let _ = tauri::async_runtime::spawn_blocking(move || super::add(&app, paths)).await;
}

#[tauri::command]
pub fn shelf_remove(app: AppHandle, paths: Vec<String>) {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    super::mutate(&app, |list| list.remove(&paths));
}

/// The page's height in points; 0 hides the panel.
#[tauri::command]
pub fn shelf_layout(app: AppHandle, height: f64) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || window::layout(&handle, height));
}

/// Whether the pointer is over the panel. The page gets no mouse-leave while it
/// isn't the key window, so it polls this while hovered. A sync command runs on
/// the main thread, as AppKit needs.
#[tauri::command]
pub fn shelf_pointer_inside(app: AppHandle) -> bool {
    app.get_webview_window(super::SHELF)
        .is_some_and(|w| crate::platform::pointer_in(&w).ok().flatten().is_some())
}

#[tauri::command]
pub fn shelf_drag_start(app: AppHandle) {
    if let Some(state) = super::state(&app) {
        state
            .dragging
            .store(true, std::sync::atomic::Ordering::SeqCst);
    }
    super::mutate(&app, |list| list.prune());
}

/// The pile's drag ended. `Dropped` removes the items unless ⌥ was held or the
/// drop landed on a Ferry window. The flag stays up for 1 s so the window's own
/// Drop event, whichever order it arrives in, is ignored.
#[tauri::command]
pub fn shelf_drag_finished(app: AppHandle, paths: Vec<String>, dropped: bool, keep: bool) {
    if dropped && !keep && !over_ferry(&app) {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        super::mutate(&app, |list| list.remove(&paths));
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Some(state) = super::state(&handle) {
            state
                .dragging
                .store(false, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

fn over_ferry(app: &AppHandle) -> bool {
    use tauri::Manager;
    let Ok(cursor) = app.cursor_position() else {
        return false;
    };
    let main_scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor());
    let cursor = (cursor.x / main_scale, cursor.y / main_scale);
    let windows: Vec<(f64, f64, f64, f64)> = app
        .webview_windows()
        .values()
        .filter(|w| w.is_visible().unwrap_or(false))
        .filter_map(|w| {
            let scale = w.scale_factor().ok()?;
            let p = w.outer_position().ok()?.to_logical::<f64>(scale);
            let s = w.outer_size().ok()?.to_logical::<f64>(scale);
            Some((p.x, p.y, s.width, s.height))
        })
        .collect();
    super::cursor_over_ferry(cursor, &windows)
}

#[tauri::command]
pub fn shelf_menu(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    super::mutate(&app, |list| list.prune());
    super::menu::popup(&app, x, y)
}

#[tauri::command]
pub fn shelf_item_menu(app: AppHandle, path: String, x: f64, y: f64) -> Result<(), String> {
    super::menu::popup_item(&app, &path, x, y)
}

#[tauri::command]
pub fn shelf_undo(app: AppHandle) {
    super::mutate(&app, |list| list.undo());
}
