//! Ferry's platform-independent core. No Tauri or OS UI dependencies, so every
//! module is unit-tested on macOS, Windows and Linux.

pub mod backup;
pub mod cancel;
pub mod fsutil;
pub mod geometry;
pub mod i18n;
pub mod job;
pub mod optimise;
pub mod pipeline;
pub mod run_state;
pub mod settings;
pub mod sidecar;
pub mod sniff;
pub mod template;
pub mod trigger;

#[cfg(test)]
mod testutil;
