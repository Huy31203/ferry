//! Windows: the drop zone is a tool window that never takes focus, like the stack.

use tauri::WebviewWindow;

pub fn make_zone_window(window: &WebviewWindow) -> tauri::Result<()> {
    super::make_non_activating(window)
}
