//! Stream H: handing a card's file to other apps (share, Open With, Edit with,
//! Upload, shelves) and taking files in from Finder / Explorer.

pub mod launch;

use ferry_core::integrations::launch_paths;
use tauri::AppHandle;

/// Called once from `setup`: optimises the files this process was started with.
pub fn init(app: &AppHandle) {
    let args: Vec<String> = std::env::args_os()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    let cwd = std::env::current_dir().unwrap_or_default();
    launch::ready(app, launch_paths(&args, &cwd));
}
