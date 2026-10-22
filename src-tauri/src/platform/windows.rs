//! Windows: the stack is a tool window (no taskbar button, not in Alt+Tab)
//! that never activates. tao already shows unfocusable windows with
//! SW_SHOWNOACTIVATE, so a plain show() leaves focus where it is.

use tauri::WebviewWindow;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetWindowLongPtrW, SetWindowLongPtrW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

pub fn make_non_activating(window: &WebviewWindow) -> tauri::Result<()> {
    let hwnd = window.hwnd()?.0;
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) as isize,
        );
    }
    Ok(())
}

pub fn show_without_focus(window: &WebviewWindow) -> tauri::Result<()> {
    window.show()
}
