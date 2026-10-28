//! Windows: the stack is a tool window (no taskbar button, not in Alt+Tab)
//! that never activates. tao shows a window with SW_SHOW once it has been
//! created and rewrites GWL_EXSTYLE from its own flags whenever one changes,
//! which drops WS_EX_TOOLWINDOW. So the stack is shown and hidden here with
//! Win32 calls and the style is re-applied on every show. tao never learns the
//! window is visible, which is why `hide` also lives here.

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, GetWindowThreadProcessId, HWND_TOPMOST,
    SW_HIDE, SW_SHOWNOACTIVATE, SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
    SetForegroundWindow, SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_NOACTIVATE,
    WS_EX_TOOLWINDOW,
};

/// Whether the stack may take the keyboard: only while a text field or a
/// slider drag is open (spec §2 "Text input in the stack").
static KEY_WANTED: AtomicBool = AtomicBool::new(false);
/// The window that had the keyboard before the stack took it.
static PREVIOUS: AtomicIsize = AtomicIsize::new(0);

fn apply_tool_window_style(hwnd: HWND) {
    let no_activate = if KEY_WANTED.load(Ordering::Relaxed) {
        0
    } else {
        WS_EX_NOACTIVATE
    };
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & !(WS_EX_NOACTIVATE as isize);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (no_activate | WS_EX_TOOLWINDOW) as isize,
        );
    }
}

pub fn make_non_activating(window: &WebviewWindow) -> tauri::Result<()> {
    apply_tool_window_style(window.hwnd()?.0);
    Ok(())
}

/// Not needed here: the webview gets real pointer events, and Chromium repeats
/// the last one after a layout change.
pub fn pointer_in(_window: &WebviewWindow) -> Result<Option<(f64, f64)>, String> {
    Err("the webview tracks the pointer itself".into())
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

/// Let the stack take the keyboard (dropping WS_EX_NOACTIVATE) and remember
/// the window that had it; give it back when `on` is false.
pub fn set_stack_key(window: &WebviewWindow, on: bool) -> tauri::Result<()> {
    KEY_WANTED.store(on, Ordering::Relaxed);
    // tao tracks focusability itself and would put WS_EX_NOACTIVATE back.
    window.set_focusable(on)?;
    let hwnd = window.hwnd()?.0;
    apply_tool_window_style(hwnd);
    // SAFETY: both handles are top-level windows; a stale one only makes the call fail.
    unsafe {
        if on {
            PREVIOUS.store(GetForegroundWindow() as isize, Ordering::Relaxed);
            SetForegroundWindow(hwnd);
        } else {
            let previous = PREVIOUS.swap(0, Ordering::Relaxed) as HWND;
            // Only while the stack still has the foreground: a click on another
            // window (which closed the field) already put the keyboard there.
            if !previous.is_null() && GetForegroundWindow() == hwnd {
                SetForegroundWindow(previous);
            }
        }
    }
    Ok(())
}

/// Whether one of Ferry's own windows is the foreground window.
pub fn app_is_active() -> bool {
    let mut pid = 0u32;
    // SAFETY: plain queries; a null foreground window leaves `pid` at 0.
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == GetCurrentProcessId()
    }
}
