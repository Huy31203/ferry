//! Quick preview (Space, spec §4.5): Quick Look on macOS; on Windows the
//! compare window showing the result alone.

use ferry_core::job::JobId;
use tauri::AppHandle;

use crate::compare;

/// The card's hover Space and the menu's "Preview" (M3b). A second press closes the preview.
#[tauri::command]
pub async fn quick_preview(app: AppHandle, id: JobId) -> Result<(), String> {
    // While the pointer rests on a card, the hover hotkey takes Space before the
    // compare page sees it; Space still closes the window.
    if compare::is_active(&app) {
        compare::close(&app);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let view = compare::view(&compare::card(&app, &id)?, true)?;
        let protected = !app
            .state::<crate::state::AppState>()
            .settings
            .read()
            .unwrap()
            .general
            .show_in_screenshots;
        crate::platform::preview::quick_look(&app, view.now.path, protected)
    }
    #[cfg(not(target_os = "macos"))]
    {
        compare::show(&app, &id, true)
    }
}
