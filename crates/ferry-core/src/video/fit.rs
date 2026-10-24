//! Bitrates: what "fit under N MB" aims at, and what a hardware encoder gets
//! (hardware encoders take a bitrate, not a quality).

use std::time::Duration;

use super::{Codec, probe::Probe};

pub const MIN_VIDEO_BPS: u64 = 50_000;

/// The video bitrate that should land under `limit_bytes`, with 4% left for the
/// container; and whether that is reachable at all.
pub fn budget(limit_bytes: u64, duration: Duration, audio_bps: u64) -> (u64, bool) {
    let secs = duration.as_secs_f64().max(0.001);
    let total = (limit_bytes as f64 * 8.0 * 0.96 / secs).round() as u64;
    let video = total.saturating_sub(audio_bps);
    (video.max(MIN_VIDEO_BPS), video >= MIN_VIDEO_BPS)
}

/// After a miss: scale the bitrate by how far off the result was, 5% lower.
pub fn next_bitrate(video_bps: u64, got_bytes: u64, limit_bytes: u64) -> u64 {
    let next = video_bps as f64 * limit_bytes as f64 / got_bytes.max(1) as f64 * 0.95;
    (next.round() as u64).max(MIN_VIDEO_BPS)
}

/// The bitrate a hardware encoder gets for quality `q` (5..=100).
pub fn hardware_bitrate(
    q: u8,
    probe: &Probe,
    out: (u32, u32),
    fps: Option<f32>,
    codec: Codec,
) -> u64 {
    let q = f64::from(q) / 100.0;
    let mut bpp = 0.04 + 0.12 * q * q;
    if codec == Codec::Hevc {
        bpp *= 0.7;
    }
    let out_pixels = f64::from(out.0) * f64::from(out.1);
    let cap = bpp * out_pixels * f64::from(fps.unwrap_or(30.0));
    let source_pixels = f64::from(probe.video.width) * f64::from(probe.video.height);
    let wanted = match probe.video.bitrate {
        Some(bps) if source_pixels > 0.0 => {
            bps as f64 * (0.25 + 0.5 * q * q) * (out_pixels / source_pixels)
        }
        _ => f64::INFINITY,
    };
    (cap.min(wanted).round() as u64).max(100_000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::video::plan::tests::probe_1080;

    #[test]
    fn the_budget_leaves_room_for_audio_and_the_container() {
        assert_eq!(
            budget(10_000_000, Duration::from_secs(10), 128_000),
            (7_552_000, true)
        );
        assert_eq!(
            budget(100_000, Duration::from_secs(60), 128_000),
            (MIN_VIDEO_BPS, false)
        );
    }

    #[test]
    fn a_miss_scales_the_next_bitrate() {
        assert_eq!(next_bitrate(1_000_000, 12_000_000, 10_000_000), 791_667);
    }

    #[test]
    fn hardware_bitrate_follows_quality_and_size() {
        let mut p = probe_1080();
        p.video.bitrate = None;
        assert_eq!(
            hardware_bitrate(80, &p, (1920, 1080), Some(30.0), Codec::H264),
            7_265_894
        );
        assert_eq!(
            hardware_bitrate(80, &p, (1920, 1080), Some(30.0), Codec::Hevc),
            5_086_126
        );
        assert!(hardware_bitrate(5, &p, (16, 16), None, Codec::H264) == 100_000);
    }
}
