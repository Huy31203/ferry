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
