//! Per-OS native glue. Each OS module exposes the same functions, and each
//! `clipboard` module the same `read`, `write_image`, `write_files` and `watch`.

#[cfg(target_os = "macos")]
#[path = "macos_clipboard.rs"]
pub mod clipboard;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
#[allow(unused_imports)] // accent_rgb, app_identifier: used from Task 7
pub use macos::{accent_rgb, app_identifier, hide, make_non_activating, show_without_focus};

#[cfg(windows)]
#[path = "windows_clipboard.rs"]
pub mod clipboard;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{
    accent_rgb, app_identifier, hide, make_non_activating, show_without_focus,
};
