//! Ferry's platform-independent core. No Tauri or OS UI dependencies, so every
//! module is unit-tested on macOS, Windows and Linux.

pub mod cancel;
pub mod geometry;
pub mod optimise;
pub mod sidecar;
pub mod sniff;

#[cfg(test)]
mod testutil;
