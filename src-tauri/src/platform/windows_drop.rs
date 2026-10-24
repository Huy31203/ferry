//! Windows: the drop zone is a tool window that never takes focus, like the
//! stack. There is no public drag-start signal, so the watcher polls the
//! mouse button, the modifiers and the cursor; built but not verified on a
//! real desktop.

use std::path::PathBuf;

use ferry_core::drag::{Mods, Sample};
use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::{CloseHandle, POINT};
use windows_sys::Win32::System::Threading::{
    GetCurrentProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    GetAsyncKeyState, VK_CONTROL, VK_LBUTTON, VK_LWIN, VK_MENU, VK_RBUTTON, VK_RWIN, VK_SHIFT,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GA_ROOT, GetAncestor, GetCursorPos, GetForegroundWindow, GetSystemMetrics,
    GetWindowThreadProcessId, SM_SWAPBUTTON, WindowFromPoint,
};

pub fn make_zone_window(window: &WebviewWindow) -> tauri::Result<()> {
    super::make_non_activating(window)
}

fn down(vkey: u16) -> bool {
    // SAFETY: a plain query of the async key state.
    unsafe { GetAsyncKeyState(i32::from(vkey)) < 0 }
}

pub fn mods() -> Mods {
    Mods {
        alt: down(VK_MENU),
        shift: down(VK_SHIFT),
        control: down(VK_CONTROL),
        command: down(VK_LWIN) || down(VK_RWIN),
    }
}

/// Whether the window at `at` belongs to this process. The root window is
/// needed: the WebView2 child under the pointer belongs to a browser process.
fn over_own(at: POINT) -> bool {
    // SAFETY: plain Win32 queries; a null window makes the calls return 0.
    unsafe {
        let root = GetAncestor(WindowFromPoint(at), GA_ROOT);
        let mut pid = 0_u32;
        GetWindowThreadProcessId(root, &mut pid);
        pid != 0 && pid == GetCurrentProcessId()
    }
}

pub fn sample(at_ms: u64) -> Sample {
    let mut at = POINT { x: 0, y: 0 };
    // SAFETY: `at` is a valid out pointer.
    unsafe { GetCursorPos(&mut at) };
    // SAFETY: a plain query.
    let swapped = unsafe { GetSystemMetrics(SM_SWAPBUTTON) } != 0;
    let button = down(if swapped { VK_RBUTTON } else { VK_LBUTTON });
    Sample {
        at_ms,
        button,
        drag_seq: None,
        mods: mods(),
        cursor: (f64::from(at.x), f64::from(at.y)),
        over_own: button && over_own(at),
    }
}

/// Windows has no drag pasteboard to read.
pub fn drag_files() -> Vec<PathBuf> {
    Vec::new()
}

/// The executable name of the foreground app, such as `Figma.exe`.
pub fn frontmost_app() -> Option<String> {
    let mut pid = 0_u32;
    // SAFETY: a null foreground window makes the call return 0, which is checked.
    unsafe { GetWindowThreadProcessId(GetForegroundWindow(), &mut pid) };
    if pid == 0 {
        return None;
    }
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0_u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        Some(path.file_name()?.to_string_lossy().into_owned())
    }
}
