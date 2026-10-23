//! Ferry's windows: the results stack (transparent, never focused) and
//! Settings (normal, on demand).

use ferry_core::geometry::{Corner, Rect, fit, place};
use ferry_core::settings;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use ferry_core::i18n::t;

use crate::platform;
use crate::state::AppState;

pub const STACK: &str = "stack";
pub const SETTINGS: &str = "settings";

/// stack.html pads the cards by 12 logical px (room for shadows); adding this
/// margin keeps cards 16 px from the work-area edge (R§4.4).
const STACK_MARGIN: f64 = 4.0;

/// Create the hidden results stack at startup so its page is loaded and
/// listening before the first card arrives.
pub fn create_stack(app: &AppHandle) -> tauri::Result<()> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, STACK, WebviewUrl::App("stack.html".into()))
        .title(t("stack.window_title"))
        .inner_size(1.0, 1.0)
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
    platform::make_non_activating(&window)
}

/// Hide the stack from screenshots and screen sharing, or show it (R§3.1).
pub fn set_content_protected(app: &AppHandle, protected: bool) {
    if let Some(stack) = app.get_webview_window(STACK) {
        let _ = stack.set_content_protected(protected);
    }
}

/// Fit the stack window to its content and pin it to the chosen corner of the
/// work area under the cursor (or of the main screen). A zero size hides it.
#[tauri::command]
pub fn stack_resize(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let Some(stack) = app.get_webview_window(STACK) else {
        return Ok(());
    };
    if width < 1.0 || height < 1.0 {
        return platform::hide(&stack).map_err(|e| e.to_string());
    }
    let floating = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .clone();
    let corner = match floating.corner {
        settings::Corner::TopLeft => Corner::TopLeft,
        settings::Corner::TopRight => Corner::TopRight,
        settings::Corner::BottomLeft => Corner::BottomLeft,
        settings::Corner::BottomRight => Corner::BottomRight,
    };
    let monitor = app
        .cursor_position()
        .ok()
        .filter(|_| floating.follow_cursor)
        .and_then(|p| {
            // tao reports the macOS cursor as points × the main display's scale,
            // while monitor_from_point compares against points.
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
        .ok_or("no monitor found")?;
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let margin = (STACK_MARGIN * scale).round() as u32;
    // A stack taller than the work area keeps its newest cards (the page anchors
    // them to the bottom) instead of running under the taskbar.
    let size = fit(
        area,
        (
            (width * scale).ceil() as u32,
            (height * scale).ceil() as u32,
        ),
        margin,
    );
    let (x, y) = place(area, size, corner, margin);

    // macOS positions windows in points; converting with the *target* monitor's
    // scale keeps mixed-DPI setups right. Windows uses physical pixels directly.
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, LogicalSize};
        stack
            .set_size(LogicalSize::new(
                f64::from(size.0) / scale,
                f64::from(size.1) / scale,
            ))
            .map_err(|e| e.to_string())?;
        stack
            .set_position(LogicalPosition::new(
                f64::from(x) / scale,
                f64::from(y) / scale,
            ))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        // Move first so the window is on the target monitor before it is sized;
        // otherwise a DPI change rescales the new size a second time.
        stack
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        stack
            .set_size(PhysicalSize::new(size.0, size.1))
            .map_err(|e| e.to_string())?;
        stack
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    platform::show_without_focus(&stack).map_err(|e| e.to_string())
}

/// Let the stack take the keyboard while a text field (rename, crop, fit) or
/// a slider drag is open, and hand it back after (spec §2, §5.3).
#[tauri::command]
pub fn stack_focus(app: AppHandle, on: bool) -> Result<(), String> {
    let Some(stack) = app.get_webview_window(STACK) else {
        return Ok(());
    };
    platform::set_stack_key(&stack, on).map_err(|e| e.to_string())
}

/// Show the Settings window, creating it the first time.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title(t("settings.window_title"))
        .inner_size(820.0, 600.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
    match built {
        // A tray app is never frontmost, so a new window opens behind other apps without this.
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("ferry: could not open settings: {e}"),
    }
}
