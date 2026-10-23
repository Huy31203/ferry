//! Windows: the quick preview is the compare window (spec §4.5). This hands
//! focus back to the window the user was in when the compare window closes.

use std::sync::Mutex;

use tauri::AppHandle;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};

/// The foreground window before Ferry took focus, as an address (`HWND` is not
/// `Send`); 0 for none.
static FRONT: Mutex<usize> = Mutex::new(0);

/// Note the foreground window before Ferry takes focus; nothing when it is
/// one of Ferry's own (Settings).
pub fn remember_front() {
    // SAFETY: plain queries; no pointer is kept beyond its address.
    let front = unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if hwnd.is_null() || pid == GetCurrentProcessId() {
            0
        } else {
            hwnd as usize
        }
    };
    *FRONT.lock().unwrap() = front;
}

/// Bring back the window `remember_front` noted, once, if it still exists.
pub fn give_back_focus() {
    let front = std::mem::take(&mut *FRONT.lock().unwrap());
    if front == 0 {
        return;
    }
    let hwnd = front as HWND;
    // SAFETY: IsWindow rejects a window that closed in the meantime.
    unsafe {
        if IsWindow(hwnd) != 0 {
            SetForegroundWindow(hwnd);
        }
    }
}

/// Windows has no Quick Look; the compare window follows the setting itself.
pub fn set_protected(_app: &AppHandle, _protected: bool) {}
