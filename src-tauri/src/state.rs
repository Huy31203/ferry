//! Everything the app shares between the tray, hotkeys, clipboard watcher,
//! job workers and commands.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use ferry_core::backup::Backups;
use ferry_core::cancel::CancelToken;
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::pipeline::Job;
use ferry_core::run_state::RunState;
use ferry_core::settings::Settings;
use tokio::sync::Semaphore;

/// How many finished jobs the tray's "Recent results" lists.
pub const RECENT: usize = 10;

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub run_state: Mutex<RunState>,
    pub backups: Backups,
    pub backups_dir: PathBuf,
    pub work_dir: PathBuf,
    pub log_dir: PathBuf,
    pub gifsicle: Option<PathBuf>,
    pub jobs: Mutex<HashMap<JobId, Arc<Entry>>>,
    /// Newest first.
    pub recent: Mutex<VecDeque<JobUpdate>>,
    /// One permit per CPU, so a burst of copies cannot starve the machine.
    pub permits: Arc<Semaphore>,
    /// Held across every clipboard write, so the watcher never mistakes our own
    /// write for the user's.
    pub clipboard: Mutex<ClipboardLog>,
    /// Cards sent before the stack page was listening; it collects them on load.
    pub stack: Mutex<StackInbox>,
    /// The tray shows an error mark until its menu is opened.
    pub tray_error: AtomicBool,
}

impl AppState {
    pub fn new(
        settings: Settings,
        settings_path: PathBuf,
        backups_dir: PathBuf,
        work_dir: PathBuf,
        log_dir: PathBuf,
        gifsicle: Option<PathBuf>,
    ) -> Self {
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get());
        Self {
            run_state: Mutex::new(RunState::from_start(settings.general.start_state)),
            settings: RwLock::new(settings),
            settings_path,
            backups: Backups::new(&backups_dir),
            backups_dir,
            work_dir,
            log_dir,
            gifsicle,
            jobs: Mutex::new(HashMap::new()),
            recent: Mutex::new(VecDeque::new()),
            permits: Arc::new(Semaphore::new(workers)),
            clipboard: Mutex::new(ClipboardLog::default()),
            stack: Mutex::new(StackInbox::default()),
            tray_error: AtomicBool::new(false),
        }
    }

    /// Whether any job is queued or optimising (the tray's Working glyph).
    pub fn is_working(&self) -> bool {
        self.jobs.lock().unwrap().values().any(|e| e.is_busy())
    }

    /// Keep a finished job for the tray's "Recent results", newest first.
    pub fn remember(&self, update: &JobUpdate) {
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|u| u.id != update.id);
        recent.push_front(update.clone());
        recent.truncate(RECENT);
    }
}

/// A job, its cancel switch and the card it last showed. `job` stays locked
/// while the job runs, so `output` is kept outside it.
pub struct Entry {
    pub job: Mutex<Job>,
    pub output: PathBuf,
    /// The clipboard copy a clipboard job came from, until its first run.
    pub source_seq: Mutex<Option<u64>>,
    pub cancel: Mutex<CancelToken>,
    pub last: Mutex<JobUpdate>,
}

impl Entry {
    pub fn is_busy(&self) -> bool {
        matches!(
            self.last.lock().unwrap().state,
            JobState::Queued | JobState::Optimising { .. }
        )
    }
}

/// Clipboard changes Ferry made itself, and the files "accumulate results" has
/// collected since launch or the last "Clear all".
#[derive(Default)]
pub struct ClipboardLog {
    own: VecDeque<u64>,
    /// The copy whose result Ferry last put on the clipboard; `None` after an
    /// explicit write such as a card's Copy.
    shown: Option<u64>,
    pub accumulated: Vec<PathBuf>,
}

impl ClipboardLog {
    const KEEP: usize = 16;

    /// Remember a write Ferry made, and which copy (`from`) it was the result of.
    pub fn record(&mut self, seq: u64, from: Option<u64>) {
        self.own.push_back(seq);
        if self.own.len() > Self::KEEP {
            self.own.pop_front();
        }
        self.shown = from;
    }

    pub fn is_own(&self, seq: u64) -> bool {
        self.own.contains(&seq)
    }

    /// Whether the result of the copy `source` may go on the clipboard, which
    /// is now at `live`: only while it still holds that copy or Ferry's result
    /// of it. Anything copied since wins.
    pub fn may_replace(&self, source: u64, live: u64) -> bool {
        live == source || (self.is_own(live) && self.shown == Some(source))
    }
}

/// Cards held until the stack page says it is listening.
#[derive(Default)]
pub struct StackInbox {
    pub ready: bool,
    pub pending: Vec<JobUpdate>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_clipboard_writes_are_recognised() {
        let mut log = ClipboardLog::default();
        log.record(41, None);
        assert!(log.is_own(41));
        assert!(!log.is_own(42));
    }

    #[test]
    fn only_recent_writes_are_remembered() {
        let mut log = ClipboardLog::default();
        for seq in 0..20 {
            log.record(seq, None);
        }
        assert!(!log.is_own(0));
        assert!(log.is_own(19));
    }

    #[test]
    fn a_result_replaces_the_copy_it_came_from() {
        let log = ClipboardLog::default();
        assert!(log.may_replace(7, 7));
    }

    #[test]
    fn a_result_never_replaces_what_the_user_copied_while_it_ran() {
        let log = ClipboardLog::default();
        assert!(!log.may_replace(7, 8));
    }

    #[test]
    fn an_older_result_never_replaces_a_newer_one() {
        // Two quick screenshots (seq 7, then 8); the second finishes first.
        let mut log = ClipboardLog::default();
        assert!(log.may_replace(8, 8));
        log.record(9, Some(8));
        assert!(!log.may_replace(7, 9));
    }

    #[test]
    fn a_hotkey_on_ferrys_own_result_may_replace_it() {
        // A screenshot (seq 2) became Ferry's result (seq 3); ⌃⇧A then
        // optimises that result harder.
        let mut log = ClipboardLog::default();
        log.record(3, Some(2));
        assert!(log.may_replace(3, 3));
    }

    #[test]
    fn a_result_never_replaces_an_explicit_copy() {
        // The user pressed Copy on an older card while this job ran.
        let mut log = ClipboardLog::default();
        log.record(9, None);
        assert!(!log.may_replace(7, 9));
    }

    #[test]
    fn recent_results_keep_ten_newest_first_without_repeats() {
        let dir = std::env::temp_dir();
        let state = AppState::new(
            Settings::default(),
            dir.join("s.json"),
            dir.join("b"),
            dir.join("w"),
            dir.join("l"),
            None,
        );
        let updates: Vec<JobUpdate> = (0..12)
            .map(|i| ferry_core::pipeline::notice(format!("n{i}")))
            .collect();
        for u in &updates {
            state.remember(u);
        }
        state.remember(&updates[11]);
        let recent = state.recent.lock().unwrap();
        assert_eq!(recent.len(), RECENT);
        assert_eq!(recent[0].id, updates[11].id);
        assert_eq!(recent[1].id, updates[10].id);
    }
}
