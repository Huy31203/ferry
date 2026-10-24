//! Where Windows saves screenshots and screen recordings.

use std::ffi::OsString;
use std::os::windows::ffi::OsStringExt;
use std::path::PathBuf;

use ferry_core::watch::capture::{CaptureDir, windows_dirs};
use windows_sys::Win32::System::Com::CoTaskMemFree;
use windows_sys::Win32::UI::Shell::{
    FOLDERID_Screenshots, FOLDERID_Videos, KF_FLAG_DONT_VERIFY, SHGetKnownFolderPath,
};
use windows_sys::core::GUID;

/// A known folder's path. `KF_FLAG_DONT_VERIFY` returns it before the first
/// capture has created it.
fn known_folder(id: &GUID) -> Option<PathBuf> {
    let mut path: *mut u16 = std::ptr::null_mut();
    // SAFETY: `id` and `path` are valid for the call; the returned string is
    // freed with `CoTaskMemFree` below whether or not the call succeeded.
    unsafe {
        let hr = SHGetKnownFolderPath(
            id,
            KF_FLAG_DONT_VERIFY as u32,
            std::ptr::null_mut(),
            &mut path,
        );
        let found = if hr == 0 && !path.is_null() {
            let len = (0..).take_while(|&i| *path.add(i) != 0).count();
            Some(PathBuf::from(OsString::from_wide(
                std::slice::from_raw_parts(path, len),
            )))
        } else {
            None
        };
        CoTaskMemFree(path.cast());
        found
    }
}

/// The Screenshots folder, Game Bar's `Videos\Captures` and the Snipping Tool's
/// `Videos\Screen Recordings`. A Game Bar folder the user moved is not followed.
pub fn capture_dirs() -> Vec<CaptureDir> {
    windows_dirs(
        known_folder(&FOLDERID_Screenshots),
        known_folder(&FOLDERID_Videos),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_screenshots_folder_is_under_the_user_profile() {
        let dirs = capture_dirs();
        assert!(dirs.len() >= 2, "{dirs:?}");
        assert!(dirs.iter().all(|d| d.dir.is_absolute()));
    }
}
