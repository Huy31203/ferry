//! Ferry's platform-independent core. No Tauri or OS UI dependencies, so every
//! module is unit-tested on macOS, Windows and Linux.

pub mod audio; // stream B
pub mod backup;
pub mod batch;
pub mod bitmap;
pub mod cancel;
pub mod clip;
pub mod color;
pub mod drag;
pub mod edit;
pub mod fsutil;
pub mod geometry;
pub mod glyph;
pub mod i18n;
pub mod intake;
pub mod integrations; // stream H
pub mod job;
pub mod optimise;
pub mod output;
pub mod pipeline;
pub mod run_state;
pub mod settings;
pub mod settings_sync;
pub mod sidecar;
pub mod sniff;
pub mod svc_settings;
pub mod template;
pub mod trigger;
pub mod workdir;

// stream A: video
pub mod error;
pub mod ffmpeg;
pub use error::FerryError;
pub mod video;

#[cfg(test)]
mod testutil;
