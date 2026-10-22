//! Windows: the stack is a tool window (no taskbar button, not in Alt+Tab)
//! that never activates. tao shows a window with SW_SHOW once it has been
//! created and rewrites GWL_EXSTYLE from its own flags whenever one changes,
//! which drops WS_EX_TOOLWINDOW. So the stack is shown and hidden here with
//! Win32 calls and the style is re-applied on every show. tao never learns the
//! window is visible, which is why `hide` also lives here.

use std::path::Path;

use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetWindowLongPtrW, SetWindowPos, ShowWindow,
    WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

fn apply_tool_window_style(hwnd: HWND) {
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
}

pub fn make_non_activating(window: &WebviewWindow) -> tauri::Result<()> {
    apply_tool_window_style(window.hwnd()?.0);
    Ok(())
}

pub fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    apply_tool_window_style(hwnd);
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        SetWindowPos(
            hwnd,
            HWND_TOPMOST,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_FRAMECHANGED,
        );
    }
    Ok(())
}

pub fn hide(window: &WebviewWindow) -> tauri::Result<()> {
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        ShowWindow(window.hwnd()?.0, SW_HIDE);
    }
    Ok(())
}

/// Windows uses the default accent this cycle (spec §7).
pub fn accent_rgb() -> Option<(u8, u8, u8)> {
    None
}

/// The executable's file name, which is what the clipboard snapshot reports as
/// its source app.
pub fn app_identifier(path: &Path) -> Option<String> {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
}
