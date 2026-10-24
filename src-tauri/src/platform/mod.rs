//! Per-OS native glue. Each OS module exposes the same functions, and each
//! `clipboard` module the same `read`, `write_image`, `write_files`,
//! `write_paths`, `write_text` and `watch`.

#[cfg(target_os = "macos")]
#[path = "macos_clipboard.rs"]
pub mod clipboard;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
#[path = "macos_preview.rs"]
pub mod preview;
#[cfg(target_os = "macos")]
#[path = "macos_drop.rs"]
pub mod drop; // Stream D
#[cfg(target_os = "macos")]
pub use macos::app_is_active;
#[cfg(target_os = "macos")]
pub use macos::set_stack_key;
#[cfg(target_os = "macos")]
pub use macos::{accent_rgb, app_identifier, hide, make_non_activating, show_without_focus};

#[cfg(windows)]
#[path = "windows_clipboard.rs"]
pub mod clipboard;
#[cfg(windows)]
#[path = "windows_preview.rs"]
pub mod preview;
#[cfg(windows)]
#[path = "windows_drop.rs"]
pub mod drop; // Stream D
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::app_is_active;
#[cfg(windows)]
pub use self::windows::set_stack_key;
#[cfg(windows)]
pub use self::windows::{
    accent_rgb, app_identifier, hide, make_non_activating, show_without_focus,
};
