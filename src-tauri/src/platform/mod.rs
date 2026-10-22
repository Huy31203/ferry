//! Per-OS native glue. Each OS module exposes the same functions.

#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{make_non_activating, show_without_focus};

#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{make_non_activating, show_without_focus};
