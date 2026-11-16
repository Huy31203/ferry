//! Time estimates for "Add to archive" from measured throughput (spec §5).

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// MB/s per worker for levels 0–9, learned from finished archives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    pub mb_per_sec: [f64; 10],
}

impl Default for Rates {
    /// Conservative single-thread Deflate figures.
    fn default() -> Self {
        Self {
            mb_per_sec: [
                800.0, 120.0, 110.0, 90.0, 70.0, 45.0, 35.0, 25.0, 18.0, 15.0,
            ],
        }
    }
}

const MIN_BYTES: u64 = 8_000_000;

impl Rates {
    pub fn estimate(&self, bytes: u64, level: u32, workers: usize) -> f64 {
        bytes as f64 / 1e6 / (self.rate(level) * workers.max(1) as f64)
    }

    pub fn record(&mut self, bytes: u64, level: u32, workers: usize, secs: f64) {
        if bytes < MIN_BYTES || secs <= 0.0 {
            return;
        }
        let seen = bytes as f64 / 1e6 / secs / workers.max(1) as f64;
        let i = level.min(9) as usize;
        self.mb_per_sec[i] = 0.7 * self.mb_per_sec[i] + 0.3 * seen;
    }

    fn rate(&self, level: u32) -> f64 {
        self.mb_per_sec[level.min(9) as usize].max(0.1)
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, serde_json::to_vec(self).map_err(io::Error::other)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_divides_by_rate_and_workers() {
        let r = Rates::default();
        assert!((r.estimate(120_000_000, 1, 1) - 1.0).abs() < 1e-9);
        assert!((r.estimate(120_000_000, 1, 4) - 0.25).abs() < 1e-9);
        assert!(
            (r.estimate(15_000_000, 9, 0) - 1.0).abs() < 1e-9,
            "0 workers counts as 1"
        );
    }

    #[test]
    fn record_moves_toward_what_was_seen() {
        let mut r = Rates::default();
        r.record(100_000_000, 5, 2, 1.0); // 50 MB/s per worker
        assert!((r.mb_per_sec[5] - (0.7 * 45.0 + 0.3 * 50.0)).abs() < 1e-9);
    }

    #[test]
    fn small_or_instant_runs_teach_nothing() {
        let mut r = Rates::default();
        r.record(1_000_000, 5, 1, 0.5);
        r.record(100_000_000, 5, 1, 0.0);
        assert_eq!(r, Rates::default());
    }

    #[test]
    fn a_missing_or_corrupt_file_means_the_seeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive_rates.json");
        assert_eq!(Rates::load(&path), Rates::default());
        std::fs::write(&path, b"{nope").unwrap();
        assert_eq!(Rates::load(&path), Rates::default());
        let mut r = Rates::default();
        r.mb_per_sec[3] = 77.0;
        r.save(&path).unwrap();
        assert_eq!(Rates::load(&path), r);
    }
}
