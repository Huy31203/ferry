//! "Wait until a file stops changing" (R§2.4): a state machine with an injected
//! clock and probe. A file is settled when it has had no event for `quiet`, two
//! probes at least `stable` apart saw the same size and time, it is not empty,
//! not a cloud placeholder and (Windows) not held open by its writer.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crate::selfwrite::Stamp;

#[derive(Debug, Clone, Copy)]
pub struct SettleConfig {
    pub quiet: Duration,
    pub stable: Duration,
    pub poll: Duration,
    pub max_wait: Duration,
    pub max_pending: usize,
}

impl Default for SettleConfig {
    fn default() -> Self {
        Self {
            quiet: Duration::from_secs(1),
            stable: Duration::from_secs(1),
            poll: Duration::from_millis(500),
            max_wait: Duration::from_secs(30 * 60),
            max_pending: 1000,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Probe {
    Gone,
    NotAFile,
    Placeholder,
    Busy,
    Denied,
    File(Stamp),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Polled {
    Settled { path: PathBuf, stamp: Stamp },
    Denied { path: PathBuf },
}

struct Entry {
    path: PathBuf,
    last_event: Instant,
    last_probe: Option<Instant>,
    seen: Option<(Stamp, Instant)>,
}

pub struct Settler {
    cfg: SettleConfig,
    /// In arrival order, so settled files come out in the order they arrived.
    pending: Vec<Entry>,
}

impl Settler {
    pub fn new(cfg: SettleConfig) -> Self {
        Self {
            cfg,
            pending: Vec::new(),
        }
    }

    /// A file arrived (created, or renamed into place).
    pub fn start(&mut self, path: PathBuf, now: Instant) {
        if let Some(entry) = self.pending.iter_mut().find(|e| e.path == path) {
            entry.last_event = now;
            entry.seen = None;
            return;
        }
        if self.pending.len() >= self.cfg.max_pending {
            let oldest = self
                .pending
                .iter()
                .enumerate()
                .min_by_key(|(_, e)| e.last_event)
                .map(|(i, _)| i);
            if let Some(oldest) = oldest {
                self.pending.remove(oldest);
            }
        }
        self.pending.push(Entry {
            path,
            last_event: now,
            last_probe: None,
            seen: None,
        });
    }

    /// The file changed. Only a file already waiting is extended: a change to an
    /// old file never starts a wait.
    pub fn extend(&mut self, path: &Path, now: Instant) {
        crate::wdiag!("extend {:?} pending={}", path, self.is_pending(path));
        if let Some(entry) = self.pending.iter_mut().find(|e| e.path == path) {
            entry.last_event = now;
            entry.seen = None;
        }
    }

    pub fn forget(&mut self, path: &Path) {
        self.pending.retain(|e| e.path != path);
    }

    pub fn is_pending(&self, path: &Path) -> bool {
        self.pending.iter().any(|e| e.path == path)
    }

    pub fn len(&self) -> usize {
        self.pending.len()
    }

    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// Keep only the waits `keep` says yes to.
    pub fn retain(&mut self, mut keep: impl FnMut(&Path) -> bool) {
        self.pending.retain(|e| keep(&e.path));
    }

    /// How long until `poll` has anything to do; `None` when nothing waits.
    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        self.pending
            .iter()
            .map(|e| {
                let due = e.last_event + self.cfg.quiet;
                let due = e.last_probe.map_or(due, |p| due.max(p + self.cfg.poll));
                due.saturating_duration_since(now)
            })
            .min()
    }

    pub fn poll(&mut self, now: Instant, probe: &mut dyn FnMut(&Path) -> Probe) -> Vec<Polled> {
        let cfg = self.cfg;
        let mut out = Vec::new();
        self.pending.retain_mut(|e| {
            if now.duration_since(e.last_event) >= cfg.max_wait {
                crate::wdiag!("poll {:?} max_wait drop", e.path);
                return false;
            }
            if now.duration_since(e.last_event) < cfg.quiet
                || e.last_probe
                    .is_some_and(|p| now.duration_since(p) < cfg.poll)
            {
                return true;
            }
            e.last_probe = Some(now);
            let pr = probe(&e.path);
            crate::wdiag!("poll {:?} probe={:?} seen={:?}", e.path, pr, e.seen.map(|(s, at)| (s, now.duration_since(at))));
            match pr {
                Probe::Gone | Probe::NotAFile | Probe::Placeholder => false,
                Probe::Denied => {
                    out.push(Polled::Denied {
                        path: e.path.clone(),
                    });
                    false
                }
                Probe::Busy => {
                    e.seen = None;
                    true
                }
                Probe::File(stamp) if stamp.len == 0 => {
                    e.seen = None;
                    true
                }
                Probe::File(stamp) => match e.seen {
                    Some((seen, at)) if seen == stamp => {
                        if now.duration_since(at) >= cfg.stable {
                            out.push(Polled::Settled {
                                path: e.path.clone(),
                                stamp,
                            });
                            false
                        } else {
                            true
                        }
                    }
                    _ => {
                        e.seen = Some((stamp, now));
                        true
                    }
                },
            }
        });
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use std::time::UNIX_EPOCH;

    fn stamp(len: u64) -> Stamp {
        Stamp {
            len,
            modified: UNIX_EPOCH + Duration::from_secs(len),
        }
    }

    struct Rig {
        t0: Instant,
        settler: Settler,
        probes: HashMap<PathBuf, Probe>,
    }

    impl Rig {
        fn new() -> Self {
            Self::with(SettleConfig::default())
        }
        fn with(cfg: SettleConfig) -> Self {
            Self {
                t0: Instant::now(),
                settler: Settler::new(cfg),
                probes: HashMap::new(),
            }
        }
        fn at(&self, ms: u64) -> Instant {
            self.t0 + Duration::from_millis(ms)
        }
        fn set(&mut self, name: &str, probe: Probe) {
            self.probes.insert(PathBuf::from(name), probe);
        }
        fn start(&mut self, name: &str, ms: u64) {
            let at = self.at(ms);
            self.settler.start(PathBuf::from(name), at);
        }
        fn poll(&mut self, ms: u64) -> Vec<Polled> {
            let now = self.at(ms);
            let probes = &self.probes;
            self.settler
                .poll(now, &mut |p| probes.get(p).copied().unwrap_or(Probe::Gone))
        }
    }

    fn settled(name: &str, len: u64) -> Polled {
        Polled::Settled {
            path: PathBuf::from(name),
            stamp: stamp(len),
        }
    }

    #[test]
    fn a_file_settles_after_quiet_and_stable_time() {
        let mut r = Rig::new();
        r.set("a", Probe::File(stamp(10)));
        r.start("a", 0);
        for ms in [999, 1000, 1500, 1999] {
            assert!(r.poll(ms).is_empty(), "{ms}");
        }
        assert_eq!(r.poll(2000), [settled("a", 10)]);
        assert_eq!(r.settler.len(), 0);
    }

    #[test]
    fn a_growing_file_waits_until_it_stops_growing() {
        let mut r = Rig::new();
        r.start("a", 0);
        r.set("a", Probe::File(stamp(10)));
        assert!(r.poll(1000).is_empty());
        r.set("a", Probe::File(stamp(20)));
        assert!(r.poll(1500).is_empty());
        assert!(r.poll(2000).is_empty());
        assert_eq!(r.poll(2500), [settled("a", 20)]);
    }

    #[test]
    fn new_events_restart_the_quiet_period() {
        let mut r = Rig::new();
        r.set("a", Probe::File(stamp(10)));
        r.start("a", 0);
        let now = r.at(1800);
        r.settler.extend(Path::new("a"), now);
        assert!(r.poll(2000).is_empty(), "still quiet");
        assert!(r.poll(2800).is_empty(), "first sighting");
        assert_eq!(r.poll(3800), [settled("a", 10)]);
    }

    #[test]
    fn an_empty_file_is_never_settled_while_empty() {
        let mut r = Rig::new();
        r.start("a", 0);
        r.set("a", Probe::File(stamp(0)));
        for ms in (1000..=10_000).step_by(500) {
            assert!(r.poll(ms).is_empty(), "{ms}");
        }
        r.set("a", Probe::File(stamp(5)));
        assert!(r.poll(10_500).is_empty());
        assert!(r.poll(11_000).is_empty());
        assert_eq!(r.poll(11_500), [settled("a", 5)]);
    }

    #[test]
    fn a_busy_file_waits_for_the_writer_to_let_go() {
        let mut r = Rig::new();
        r.start("a", 0);
        r.set("a", Probe::Busy);
        for ms in (1000..=5000).step_by(500) {
            assert!(r.poll(ms).is_empty(), "{ms}");
        }
        r.set("a", Probe::File(stamp(7)));
        assert!(r.poll(5500).is_empty());
        assert!(r.poll(6000).is_empty());
        assert_eq!(r.poll(6500), [settled("a", 7)]);
    }

    #[test]
    fn a_vanished_folder_or_placeholder_file_is_dropped() {
        for probe in [Probe::Gone, Probe::NotAFile, Probe::Placeholder] {
            let mut r = Rig::new();
            r.start("a", 0);
            r.set("a", probe);
            assert!(r.poll(1000).is_empty());
            assert_eq!(r.settler.len(), 0, "{probe:?}");
        }
    }

    #[test]
    fn a_change_event_never_starts_a_wait() {
        let mut r = Rig::new();
        let now = r.at(0);
        r.settler.extend(Path::new("old.png"), now);
        assert_eq!(r.settler.len(), 0);
        assert!(r.settler.next_wake(now).is_none());
    }

    #[test]
    fn the_wait_gives_up_after_max_wait_without_events() {
        let mut r = Rig::new();
        r.start("a", 0);
        r.set("a", Probe::Busy);
        assert!(r.poll(30 * 60 * 1000 - 1).is_empty());
        assert_eq!(r.settler.len(), 1);
        assert!(r.poll(30 * 60 * 1000).is_empty());
        assert_eq!(r.settler.len(), 0);
    }

    #[test]
    fn denied_is_reported_once_and_dropped() {
        let mut r = Rig::new();
        r.start("a", 0);
        r.set("a", Probe::Denied);
        assert_eq!(
            r.poll(1000),
            [Polled::Denied {
                path: PathBuf::from("a")
            }]
        );
        assert!(r.poll(2000).is_empty());
    }

    #[test]
    fn next_wake_is_none_when_idle_and_the_nearest_deadline_otherwise() {
        let mut r = Rig::new();
        assert!(r.settler.next_wake(r.at(0)).is_none());
        r.start("a", 0);
        assert_eq!(
            r.settler.next_wake(r.at(200)),
            Some(Duration::from_millis(800))
        );
        r.set("a", Probe::Busy);
        r.poll(1000);
        assert_eq!(
            r.settler.next_wake(r.at(1100)),
            Some(Duration::from_millis(400))
        );
    }

    #[test]
    fn pending_files_are_capped() {
        let mut r = Rig::with(SettleConfig {
            max_pending: 2,
            ..SettleConfig::default()
        });
        r.start("a", 0);
        r.start("b", 10);
        r.start("c", 20);
        assert!(!r.settler.is_pending(Path::new("a")));
        assert!(r.settler.is_pending(Path::new("b")) && r.settler.is_pending(Path::new("c")));
    }

    #[test]
    fn files_settle_in_the_order_they_arrived() {
        let mut r = Rig::new();
        for (i, name) in ["z", "a", "m"].into_iter().enumerate() {
            r.set(name, Probe::File(stamp(i as u64 + 1)));
            r.start(name, i as u64);
        }
        r.poll(1100);
        let done = r.poll(2100);
        let names: Vec<_> = done
            .iter()
            .map(|p| match p {
                Polled::Settled { path, .. } => path.to_str().unwrap().to_string(),
                Polled::Denied { .. } => unreachable!(),
            })
            .collect();
        assert_eq!(names, ["z", "a", "m"]);
    }
}
