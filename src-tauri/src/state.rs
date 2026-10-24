//! Everything the app shares between the tray, hotkeys, clipboard watcher,
//! job workers and commands.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, RwLock};

use ferry_core::backup::Backups;
use ferry_core::cancel::CancelToken;
use ferry_core::clip::{ClipWrite, Collected};
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
/// while the job runs, so its current file is kept outside it.
pub struct Entry {
    pub job: Mutex<Job>,
    /// The job's current file: follows every run, conversion and rename.
    /// Never held while taking another lock.
    output: Mutex<PathBuf>,
    /// The clipboard copy a clipboard job came from, until its first run.
    pub source_seq: Mutex<Option<u64>>,
    pub cancel: Mutex<CancelToken>,
    pub last: Mutex<JobUpdate>,
    /// The final card a re-run replaced, for when the re-run is cancelled.
    previous: Mutex<Option<JobUpdate>>,
    /// The clipboard's sequence number when an auto-copied job was queued,
    /// until its first run finishes.
    auto_copy: Mutex<Option<u64>>,
}

impl Entry {
    pub fn new(job: Job, source_seq: Option<u64>, last: JobUpdate) -> Self {
        Self {
            output: Mutex::new(job.output.clone()),
            source_seq: Mutex::new(source_seq),
            job: Mutex::new(job),
            cancel: Mutex::new(CancelToken::new()),
            last: Mutex::new(last),
            previous: Mutex::new(None),
            auto_copy: Mutex::new(None),
        }
    }

    pub fn output(&self) -> PathBuf {
        self.output.lock().unwrap().clone()
    }

    /// Call after anything that may move the job's file: a run, a restore, a rename.
    pub fn set_output(&self, path: PathBuf) {
        *self.output.lock().unwrap() = path;
    }

    pub fn set_auto_copy(&self, seq: u64) {
        *self.auto_copy.lock().unwrap() = Some(seq);
    }

    /// `Some` once, then `None`: only a job's first run auto-copies.
    pub fn take_auto_copy(&self) -> Option<u64> {
        self.auto_copy.lock().unwrap().take()
    }

    pub fn is_busy(&self) -> bool {
        matches!(
            self.last.lock().unwrap().state,
            JobState::Queued | JobState::Optimising { .. }
        )
    }

    /// Show `queued` for a re-run, remembering the card it replaces.
    pub fn begin_rerun(&self, queued: JobUpdate) {
        let mut last = self.last.lock().unwrap();
        *self.previous.lock().unwrap() = Some(std::mem::replace(&mut *last, queued));
    }

    /// Cancelling a re-run leaves the file as the earlier run made it, so the
    /// card goes back to that run's result. `None` for a first run, whose card
    /// simply goes away.
    pub fn cancel_back(&self) -> Option<JobUpdate> {
        let previous = self.previous.lock().unwrap().take()?;
        *self.last.lock().unwrap() = previous.clone();
        Some(previous)
    }
}

/// Lock a mutex even if a panic poisoned it: an encoder that panicked must not
/// take the app down when its card is retried (spec §6).
pub fn relock<T>(lock: &Mutex<T>) -> MutexGuard<'_, T> {
    lock.lock().unwrap_or_else(PoisonError::into_inner)
}

/// Whether another queued or running job writes the same file as `entry`; two
/// writers would share one temporary file.
pub fn output_busy(jobs: &HashMap<JobId, Arc<Entry>>, entry: &Arc<Entry>) -> bool {
    let output = entry.output();
    jobs.values()
        .any(|e| !Arc::ptr_eq(e, entry) && e.is_busy() && e.output() == output)
}

// ---- sI clipboard-fixes ----
/// The jobs whose current files are `paths`, in order; `None` unless every path has one.
pub fn jobs_with_outputs(
    jobs: &HashMap<JobId, Arc<Entry>>,
    paths: &[PathBuf],
) -> Option<Vec<JobId>> {
    if paths.is_empty() {
        return None;
    }
    paths
        .iter()
        .map(|p| {
            jobs.iter()
                .find(|(_, e)| e.output() == *p)
                .map(|(id, _)| id.clone())
        })
        .collect()
}

/// Clipboard changes Ferry made itself, and the results "accumulate results"
/// has collected, by job, since launch or the last "Clear all".
#[derive(Default)]
pub struct ClipboardLog {
    own: VecDeque<u64>,
    /// The copy whose result Ferry last put on the clipboard; `None` after an
    /// explicit write such as a card's Copy.
    shown: Option<u64>,
    pub collected: Collected,
    /// What Ferry's latest write put on the clipboard, until any plain `record`.
    last_write: Option<(u64, ClipWrite)>,
}

impl ClipboardLog {
    const KEEP: usize = 16;

    /// Remember a write Ferry made, and which copy (`from`) it was the result of.
    pub fn record(&mut self, seq: u64, from: Option<u64>) {
        self.remember(seq);
        self.shown = from;
        self.last_write = None;
    }

    /// `record`, and remember what was written (for a rename that follows it).
    pub fn wrote(&mut self, seq: u64, from: Option<u64>, write: ClipWrite) {
        self.record(seq, from);
        self.last_write = Some((seq, write));
    }

    /// Ferry rewrote its own write (a rename): the copy it shows stays the same.
    pub fn rewrote(&mut self, seq: u64, write: ClipWrite) {
        self.remember(seq);
        self.last_write = Some((seq, write));
    }

    pub fn last_write(&self) -> Option<&(u64, ClipWrite)> {
        self.last_write.as_ref()
    }

    fn remember(&mut self, seq: u64) {
        self.own.push_back(seq);
        if self.own.len() > Self::KEEP {
            self.own.pop_front();
        }
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

    fn entry(output: &str, last: JobState) -> Arc<Entry> {
        let now = chrono::NaiveDate::from_ymd_opt(2026, 10, 22)
            .unwrap()
            .and_hms_opt(12, 0, 0)
            .unwrap();
        let mut job = Job::clipboard(
            ferry_core::sniff::Format::Png,
            vec![0],
            false,
            "%f",
            &std::env::temp_dir(),
            now,
        );
        job.output = PathBuf::from(output);
        let update = job.update(last, None);
        Arc::new(Entry::new(job, None, update))
    }

    #[test]
    fn a_files_write_is_remembered_until_the_next_plain_write() {
        let a = PathBuf::from("/w/a.png");
        let mut log = ClipboardLog::default();
        log.wrote(5, None, ClipWrite::Files(vec![a.clone()]));
        assert_eq!(log.last_write(), Some(&(5, ClipWrite::Files(vec![a]))));
        assert!(log.is_own(5));
        log.record(6, None);
        assert_eq!(log.last_write(), None);
    }

    #[test]
    fn a_rewrite_keeps_which_copy_is_shown() {
        let mut log = ClipboardLog::default();
        log.wrote(
            3,
            Some(2),
            ClipWrite::Files(vec![PathBuf::from("/w/a.png")]),
        );
        log.rewrote(4, ClipWrite::Files(vec![PathBuf::from("/w/hero.png")]));
        assert!(log.is_own(4));
        assert!(log.may_replace(2, 4));
        assert_eq!(log.last_write().unwrap().0, 4);
    }

    #[test]
    fn own_paths_map_back_to_their_jobs() {
        let (a, b) = (
            entry("/w/a.png", JobState::Done),
            entry("/w/b.png", JobState::Done),
        );
        let (id_a, id_b) = (
            a.job.lock().unwrap().id.clone(),
            b.job.lock().unwrap().id.clone(),
        );
        let jobs = HashMap::from([(id_a.clone(), a), (id_b.clone(), b)]);
        let (pa, pb) = (PathBuf::from("/w/a.png"), PathBuf::from("/w/b.png"));
        assert_eq!(
            jobs_with_outputs(&jobs, &[pa.clone(), pb]),
            Some(vec![id_a, id_b])
        );
        assert_eq!(
            jobs_with_outputs(&jobs, &[pa, PathBuf::from("/w/x.png")]),
            None
        );
        assert_eq!(jobs_with_outputs(&jobs, &[]), None);
    }

    #[test]
    fn auto_copy_is_taken_once() {
        let e = entry("/w/a.png", JobState::Queued);
        assert_eq!(e.take_auto_copy(), None);
        e.set_auto_copy(9);
        assert_eq!(e.take_auto_copy(), Some(9));
        assert_eq!(e.take_auto_copy(), None);
    }

    #[test]
    fn an_auto_copy_never_replaces_a_later_copy() {
        let mut log = ClipboardLog::default();
        assert!(log.may_replace(4, 4));
        log.wrote(
            5,
            Some(4),
            ClipWrite::Files(vec![PathBuf::from("/w/a.png")]),
        );
        assert!(log.may_replace(4, 5), "same burst");
        assert!(!log.may_replace(4, 6), "the user copied since");
    }

    #[test]
    fn a_poisoned_lock_is_still_usable() {
        let lock = Arc::new(Mutex::new(7));
        let held = lock.clone();
        let _ = std::thread::spawn(move || {
            let _guard = held.lock().unwrap();
            panic!("an encoder panicked");
        })
        .join();
        assert_eq!(*relock(&lock), 7);
    }

    #[test]
    fn a_cancelled_re_run_returns_to_the_card_it_came_from() {
        let e = entry("/w/a.png", JobState::Done);
        let done = e.last.lock().unwrap().clone();
        e.begin_rerun(e.job.lock().unwrap().update(JobState::Queued, None));
        assert!(e.is_busy());
        assert_eq!(e.cancel_back(), Some(done.clone()));
        assert!(!e.is_busy());
        assert_eq!(*e.last.lock().unwrap(), done);
    }

    #[test]
    fn a_cancelled_first_run_has_no_card_to_return_to() {
        let e = entry("/w/a.png", JobState::Queued);
        assert_eq!(e.cancel_back(), None);
    }

    #[test]
    fn a_moved_result_moves_the_busy_check_with_it() {
        let mut jobs = HashMap::new();
        let busy = entry("/w/b.webp", JobState::Queued);
        let idle = entry("/w/a.png", JobState::Done);
        for e in [&busy, &idle] {
            let id = e.job.lock().unwrap().id.clone();
            jobs.insert(id, e.clone());
        }
        assert!(!output_busy(&jobs, &idle));
        // The idle card was converted to b.webp, which the busy job also writes.
        idle.set_output(PathBuf::from("/w/b.webp"));
        assert_eq!(idle.output(), PathBuf::from("/w/b.webp"));
        assert!(output_busy(&jobs, &idle));
    }

    #[test]
    fn two_jobs_never_write_the_same_file() {
        let mut jobs = HashMap::new();
        let busy = entry("/w/a.png", JobState::Queued);
        let idle = entry("/w/a.png", JobState::Done);
        let other = entry("/w/b.png", JobState::Queued);
        for e in [&busy, &idle, &other] {
            let id = e.job.lock().unwrap().id.clone();
            jobs.insert(id, e.clone());
        }
        // The idle card's re-run would collide with the busy job on a.png.
        assert!(output_busy(&jobs, &idle));
        // A busy job does not collide with itself, nor with a different file.
        assert!(!output_busy(&jobs, &busy));
        assert!(!output_busy(&jobs, &other));
    }
}
