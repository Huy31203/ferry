//! A batch of dropped files: its rows, a bounded work queue, the tally and the pace.
//! Pure state. The app turns `Work` into ordinary file jobs and feeds their updates back.

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

use crate::edit::EditsPatch;
use crate::i18n::t;
use crate::intake::{MediaKind, Scan};
use crate::job::{JobId, JobState, JobUpdate};
use crate::optimise::Level;
use crate::sniff::Format;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum RowStatus {
    Waiting,
    Working,
    Done,
    AlreadyOptimal,
    Larger,
    Skipped,
    Failed,
    Cancelled,
    Restored,
}

const STATUSES: usize = 9;

impl RowStatus {
    pub fn is_finished(self) -> bool {
        matches!(
            self,
            Self::Done | Self::AlreadyOptimal | Self::Larger | Self::Failed | Self::Restored
        )
    }
}

pub fn row_status(state: &JobState) -> RowStatus {
    match state {
        JobState::Queued | JobState::Optimising { .. } => RowStatus::Working,
        JobState::Done => RowStatus::Done,
        JobState::AlreadyOptimal => RowStatus::AlreadyOptimal,
        JobState::Larger => RowStatus::Larger,
        JobState::Error { .. } => RowStatus::Failed,
        JobState::Restored => RowStatus::Restored,
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchRow {
    pub index: u32,
    pub path: PathBuf,
    pub name: String,
    pub ext: String,
    pub kind: Option<MediaKind>,
    pub before: u64,
    pub after: Option<u64>,
    pub dims_before: Option<(u32, u32)>,
    pub dims_after: Option<(u32, u32)>,
    pub status: RowStatus,
    pub reason: Option<String>,
    pub progress: f32,
    pub step: Option<String>,
    pub job: Option<JobId>,
    pub thumb: Option<PathBuf>,
    pub output: Option<PathBuf>,
    pub can_restore: bool,
    pub can_edit: bool,
    pub keep_original: bool,
}

impl BatchRow {
    /// Follow the row's job.
    pub fn apply(&mut self, u: &JobUpdate) {
        self.status = row_status(&u.state);
        self.after = u.after;
        self.dims_before = u.dims_before;
        self.dims_after = u.dims_after;
        self.thumb = u.thumb_path.clone();
        self.output = u.output.clone();
        self.can_restore = u.can_restore;
        self.can_edit = u.can_edit;
        self.name = u.name.clone();
        self.ext = u.ext.clone();
        (self.reason, self.progress, self.step) = match &u.state {
            JobState::Error { reason } => (Some(reason.clone()), 0.0, None),
            JobState::Optimising { progress, step } => (None, *progress, Some(step.clone())),
            _ => (None, 0.0, None),
        };
    }
}

/// What "Re-run with new settings" and the first run use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Adjust {
    pub compression: Option<u8>,
    pub scale: Option<u8>,
    pub format: Option<Format>,
    pub extreme: bool,
}

impl Adjust {
    pub fn is_default(&self) -> bool {
        *self == Adjust::default()
    }

    pub fn level(&self) -> Level {
        if self.extreme {
            Level::Aggressive
        } else {
            Level::Normal
        }
    }

    /// Sets (or clears) every field the panel owns; the rest stay as they are.
    pub fn patch(&self) -> EditsPatch {
        EditsPatch {
            compression: Some(if self.extreme {
                Some(5)
            } else {
                self.compression
            }),
            level: Some(self.level()),
            scale: Some(self.scale),
            format: Some(self.format),
            ..EditsPatch::default()
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Work {
    Start {
        row: u32,
        path: PathBuf,
        keep: bool,
        level: Level,
        patch: Option<EditsPatch>,
    },
    Edit {
        row: u32,
        id: JobId,
        patch: EditsPatch,
    },
    Rerun {
        row: u32,
        id: JobId,
    },
}

impl Work {
    fn row(&self) -> u32 {
        match self {
            Work::Start { row, .. } | Work::Edit { row, .. } | Work::Rerun { row, .. } => *row,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Tally {
    pub total: u32,
    pub waiting: u32,
    pub working: u32,
    pub done: u32,
    pub already_optimal: u32,
    pub larger: u32,
    pub skipped: u32,
    pub failed: u32,
    pub cancelled: u32,
    pub restored: u32,
    pub before: u64,
    pub after: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase {
    Empty,
    Running,
    Paused,
    Done,
    RollingBack,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BatchStatus {
    pub phase: Phase,
    pub tally: Tally,
    /// Files finished per second of active (unpaused) time.
    pub rate: f64,
    pub eta_secs: Option<u64>,
    pub rolling: Option<(u32, u32)>,
    pub skipped_in_folders: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Snapshot {
    pub rows: Vec<BatchRow>,
    pub status: BatchStatus,
    pub adjust: Adjust,
    pub threshold: u32,
}

pub struct BatchRun {
    limit: usize,
    rows: Vec<BatchRow>,
    by_job: HashMap<JobId, u32>,
    paths: HashSet<PathBuf>,
    queue: VecDeque<Work>,
    counts: [u32; STATUSES],
    adjust: Adjust,
    paused: bool,
    rolling: Option<(u32, u32)>,
    skipped_in_folders: u32,
    truncated: bool,
    started: Option<Instant>,
    paused_at: Option<Instant>,
    paused_for: Duration,
    completed: u32,
    dirty: BTreeSet<u32>,
    status_dirty: bool,
}

impl BatchRun {
    pub fn new(limit: usize) -> Self {
        Self {
            limit: limit.max(1),
            rows: Vec::new(),
            by_job: HashMap::new(),
            paths: HashSet::new(),
            queue: VecDeque::new(),
            counts: [0; STATUSES],
            adjust: Adjust::default(),
            paused: false,
            rolling: None,
            skipped_in_folders: 0,
            truncated: false,
            started: None,
            paused_at: None,
            paused_for: Duration::ZERO,
            completed: 0,
            dirty: BTreeSet::new(),
            status_dirty: false,
        }
    }

    pub fn rows(&self) -> &[BatchRow] {
        &self.rows
    }

    /// Rows added: files to run and unsupported paths shown as Skipped. A path
    /// already in the batch is not added again.
    pub fn add(&mut self, scan: Scan, keep: bool) -> u32 {
        self.add_at(scan, keep, Instant::now())
    }

    fn add_at(&mut self, scan: Scan, keep: bool, now: Instant) -> u32 {
        let before = self.rows.len();
        for c in scan.files {
            if !self.paths.insert(c.path.clone()) {
                continue;
            }
            let row = self.push_row(c.path.clone(), c.len, Some(c.kind), keep);
            self.queue.push_back(Work::Start {
                row,
                path: c.path,
                keep,
                level: Level::Normal,
                patch: None,
            });
        }
        for path in scan.unsupported {
            if !self.paths.insert(path.clone()) {
                continue;
            }
            let row = self.push_row(path, 0, None, keep);
            self.skip(row, t("drop.batch.reason.unsupported"));
        }
        self.skipped_in_folders += scan.skipped;
        self.truncated |= scan.truncated;
        self.status_dirty = true;
        if self.started.is_none() && self.rows.len() > before {
            self.started = Some(now);
        }
        (self.rows.len() - before) as u32
    }

    fn push_row(&mut self, path: PathBuf, len: u64, kind: Option<MediaKind>, keep: bool) -> u32 {
        let index = self.rows.len() as u32;
        let part = |s: Option<&std::ffi::OsStr>| s.map(|s| s.to_string_lossy().into_owned());
        self.rows.push(BatchRow {
            index,
            name: part(path.file_stem()).unwrap_or_default(),
            ext: part(path.extension()).unwrap_or_default(),
            path,
            kind,
            before: len,
            after: None,
            dims_before: None,
            dims_after: None,
            status: RowStatus::Waiting,
            reason: None,
            progress: 0.0,
            step: None,
            job: None,
            thumb: None,
            output: None,
            can_restore: false,
            can_edit: false,
            keep_original: keep,
        });
        self.counts[RowStatus::Waiting as usize] += 1;
        self.dirty.insert(index);
        index
    }

    fn set_status(&mut self, row: u32, new: RowStatus) {
        let Some(r) = self.rows.get_mut(row as usize) else {
            return;
        };
        self.counts[r.status as usize] -= 1;
        self.counts[new as usize] += 1;
        r.status = new;
        self.dirty.insert(row);
        self.status_dirty = true;
    }

    fn count(&self, s: RowStatus) -> u32 {
        self.counts[s as usize]
    }

    pub fn set_adjust(&mut self, a: Adjust) {
        self.adjust = a;
        self.status_dirty = true;
    }

    pub fn adjust(&self) -> &Adjust {
        &self.adjust
    }

    /// The next jobs to start: while not paused and under the limit of working rows.
    pub fn take_work(&mut self) -> Vec<Work> {
        let mut out = Vec::new();
        while !self.paused && (self.count(RowStatus::Working) as usize) < self.limit {
            let Some(mut work) = self.queue.pop_front() else {
                break;
            };
            let row = work.row();
            self.set_status(row, RowStatus::Working);
            let r = &mut self.rows[row as usize];
            r.progress = 0.0;
            r.reason = None;
            if let Work::Start { level, patch, .. } = &mut work {
                *level = self.adjust.level();
                *patch = (!self.adjust.is_default()).then(|| self.adjust.patch());
            }
            out.push(work);
        }
        out
    }

    /// Record the job built for `row`. False when the row was cancelled meanwhile.
    pub fn bind(&mut self, row: u32, id: JobId) -> bool {
        match self.rows.get_mut(row as usize) {
            Some(r) if r.status == RowStatus::Working => {
                r.job = Some(id.clone());
                self.by_job.insert(id, row);
                true
            }
            _ => false,
        }
    }

    pub fn skip(&mut self, row: u32, reason: String) {
        self.set_status(row, RowStatus::Skipped);
        if let Some(r) = self.rows.get_mut(row as usize) {
            r.reason = Some(reason);
        }
    }

    pub fn row_of(&self, id: &JobId) -> Option<u32> {
        self.by_job.get(id).copied()
    }

    /// `None`: not a batch job. `Some(true)`: its row reached a finished state.
    pub fn on_update(&mut self, u: &JobUpdate, _now: Instant) -> Option<bool> {
        let row = self.row_of(&u.id)?;
        let r = &mut self.rows[row as usize];
        let old = r.status;
        r.apply(u);
        let new = r.status;
        self.counts[old as usize] -= 1;
        self.counts[new as usize] += 1;
        self.dirty.insert(row);
        self.status_dirty = true;
        let finished = old == RowStatus::Working && new.is_finished();
        if finished {
            self.completed += 1;
        }
        Some(finished)
    }

    pub fn pause(&mut self, now: Instant) {
        if !self.paused {
            self.paused = true;
            self.paused_at = Some(now);
            self.status_dirty = true;
        }
    }

    pub fn resume(&mut self, now: Instant) {
        if let Some(at) = self.paused_at.take() {
            self.paused_for += now.saturating_duration_since(at);
        }
        if self.paused {
            self.paused = false;
            self.status_dirty = true;
        }
    }

    pub fn is_paused(&self) -> bool {
        self.paused
    }

    /// Drop everything queued and name the running jobs; the caller cancels
    /// those, then calls `cancelled` for each.
    pub fn cancel_all(&mut self) -> Vec<JobId> {
        self.queue.clear();
        let mut running = Vec::new();
        for i in 0..self.rows.len() {
            let r = &self.rows[i];
            match (r.status, &r.job) {
                (RowStatus::Waiting, _) | (RowStatus::Working, None) => {
                    self.set_status(i as u32, RowStatus::Cancelled);
                }
                (RowStatus::Working, Some(id)) => running.push(id.clone()),
                _ => {}
            }
        }
        running
    }

    /// A job stopped. A re-run whose card went back to its earlier result keeps that result.
    pub fn cancelled(&mut self, id: &JobId) {
        if let Some(row) = self.row_of(id)
            && self.rows[row as usize].status == RowStatus::Working
        {
            self.set_status(row, RowStatus::Cancelled);
        }
    }

    /// Queue every finished row again with the current adjust settings.
    pub fn rerun(&mut self, now: Instant) -> usize {
        let mut queued = 0;
        for i in 0..self.rows.len() {
            let r = &self.rows[i];
            let Some(id) = r.job.clone().filter(|_| r.status.is_finished()) else {
                continue;
            };
            let row = i as u32;
            self.queue.push_back(if r.can_edit {
                Work::Edit {
                    row,
                    id,
                    patch: self.adjust.patch(),
                }
            } else {
                Work::Rerun { row, id }
            });
            self.set_status(row, RowStatus::Waiting);
            queued += 1;
        }
        self.started = Some(now);
        self.paused_for = Duration::ZERO;
        self.paused_at = self.paused.then_some(now);
        self.completed = 0;
        queued
    }

    /// Done rows whose original is still there.
    pub fn rollback_targets(&self) -> Vec<JobId> {
        self.rows
            .iter()
            .filter(|r| r.status == RowStatus::Done && r.can_restore)
            .filter_map(|r| r.job.clone())
            .collect()
    }

    pub fn set_rolling(&mut self, rolling: Option<(u32, u32)>) {
        self.rolling = rolling;
        self.status_dirty = true;
    }

    fn tally(&self) -> Tally {
        let mut t = Tally {
            total: self.rows.len() as u32,
            waiting: self.count(RowStatus::Waiting),
            working: self.count(RowStatus::Working),
            done: self.count(RowStatus::Done),
            already_optimal: self.count(RowStatus::AlreadyOptimal),
            larger: self.count(RowStatus::Larger),
            skipped: self.count(RowStatus::Skipped),
            failed: self.count(RowStatus::Failed),
            cancelled: self.count(RowStatus::Cancelled),
            restored: self.count(RowStatus::Restored),
            ..Tally::default()
        };
        for r in &self.rows {
            match r.status {
                RowStatus::Done => {
                    t.before += r.before;
                    t.after += r.after.unwrap_or(r.before);
                }
                RowStatus::AlreadyOptimal | RowStatus::Larger | RowStatus::Restored => {
                    t.before += r.before;
                    t.after += r.before;
                }
                _ => {}
            }
        }
        t
    }

    pub fn status(&self, now: Instant) -> BatchStatus {
        let tally = self.tally();
        let remaining = tally.waiting + tally.working;
        let phase = if self.rolling.is_some() {
            Phase::RollingBack
        } else if self.rows.is_empty() {
            Phase::Empty
        } else if remaining == 0 {
            Phase::Done
        } else if self.paused {
            Phase::Paused
        } else {
            Phase::Running
        };
        let active = self.started.map_or(0.0, |start| {
            let end = self.paused_at.unwrap_or(now);
            end.saturating_duration_since(start)
                .saturating_sub(self.paused_for)
                .as_secs_f64()
        });
        let rate = if active >= 1.0 {
            f64::from(self.completed) / active
        } else {
            0.0
        };
        let eta_secs =
            (rate > 0.0 && remaining > 0).then(|| (f64::from(remaining) / rate).ceil() as u64);
        BatchStatus {
            phase,
            tally,
            rate,
            eta_secs,
            rolling: self.rolling,
            skipped_in_folders: self.skipped_in_folders,
            truncated: self.truncated,
        }
    }

    pub fn snapshot(&self, now: Instant, threshold: u32) -> Snapshot {
        Snapshot {
            rows: self.rows.clone(),
            status: self.status(now),
            adjust: self.adjust.clone(),
            threshold,
        }
    }

    /// The rows and status that changed since the last flush, if any.
    pub fn take_flush(&mut self, now: Instant) -> Option<(Vec<BatchRow>, BatchStatus)> {
        if self.dirty.is_empty() && !self.status_dirty {
            return None;
        }
        let rows = std::mem::take(&mut self.dirty)
            .into_iter()
            .map(|i| self.rows[i as usize].clone())
            .collect();
        self.status_dirty = false;
        Some((rows, self.status(now)))
    }

    /// Nothing waiting or working, and no roll back running.
    pub fn is_idle(&self) -> bool {
        self.count(RowStatus::Waiting) + self.count(RowStatus::Working) == 0
            && self.rolling.is_none()
    }

    pub fn first_output(&self) -> Option<PathBuf> {
        self.rows.iter().find_map(|r| r.output.clone())
    }

    /// Every job the batch started; the run is empty afterwards.
    pub fn clear(&mut self) -> Vec<JobId> {
        let ids = self.rows.iter().filter_map(|r| r.job.clone()).collect();
        *self = Self::new(self.limit);
        ids
    }
}

/// Start everything the limit allows, again and again until nothing is left to
/// hand out. `exec` runs with the lock released, because starting a job calls
/// back into the batch.
pub fn pump(run: &Mutex<BatchRun>, exec: &mut dyn FnMut(Work)) {
    loop {
        let work = run.lock().unwrap().take_work();
        if work.is_empty() {
            break;
        }
        for item in work {
            exec(item);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::intake::Candidate;
    use crate::pipeline::notice;
    use std::sync::Arc;
    use std::sync::mpsc;
    use std::thread;

    fn scan_of(files: &[(&str, u64)]) -> Scan {
        Scan {
            files: files
                .iter()
                .map(|(p, len)| Candidate {
                    path: PathBuf::from(p),
                    len: *len,
                    kind: MediaKind::Image,
                    format: Some(Format::Png),
                })
                .collect(),
            ..Scan::default()
        }
    }

    fn many(n: u32) -> Scan {
        let names: Vec<String> = (0..n).map(|i| format!("/d/{i}.png")).collect();
        scan_of(&names.iter().map(|p| (p.as_str(), 100)).collect::<Vec<_>>())
    }

    fn update(id: &JobId, state: JobState, after: Option<u64>) -> JobUpdate {
        JobUpdate {
            id: id.clone(),
            state,
            after,
            ..notice(String::new())
        }
    }

    /// A run with `n` files, all started and bound, row `i` bound to the returned id `i`.
    fn started(n: u32, limit: usize) -> (BatchRun, Vec<JobId>) {
        let mut run = BatchRun::new(limit);
        run.add(many(n), false);
        let ids: Vec<JobId> = run
            .take_work()
            .iter()
            .map(|w| {
                let id = JobId::generate();
                assert!(run.bind(w.row(), id.clone()));
                id
            })
            .collect();
        (run, ids)
    }

    #[test]
    fn job_states_map_to_row_statuses() {
        let working = [
            JobState::Queued,
            JobState::Optimising {
                progress: 0.5,
                step: "x".into(),
            },
        ];
        assert!(working.iter().all(|s| row_status(s) == RowStatus::Working));
        assert_eq!(row_status(&JobState::Done), RowStatus::Done);
        assert_eq!(
            row_status(&JobState::AlreadyOptimal),
            RowStatus::AlreadyOptimal
        );
        assert_eq!(row_status(&JobState::Larger), RowStatus::Larger);
        assert_eq!(
            row_status(&JobState::Error { reason: "x".into() }),
            RowStatus::Failed
        );
        assert_eq!(row_status(&JobState::Restored), RowStatus::Restored);
    }

    #[test]
    fn a_row_follows_its_jobs_updates() {
        let (mut run, ids) = started(1, 1);
        let mut u = update(&ids[0], JobState::Done, Some(40));
        u.dims_after = Some((10, 10));
        u.output = Some(PathBuf::from("/d/a.webp"));
        u.ext = "webp".into();
        u.can_restore = true;
        assert_eq!(run.on_update(&u, Instant::now()), Some(true));
        let row = &run.rows()[0];
        assert_eq!(row.after, Some(40));
        assert_eq!(row.dims_after, Some((10, 10)));
        assert_eq!(row.ext, "webp");
        assert_eq!(row.output.as_deref(), Some("/d/a.webp".as_ref()));
        assert!(row.can_restore);
        assert_eq!(
            run.on_update(
                &update(&JobId::generate(), JobState::Done, None),
                Instant::now()
            ),
            None
        );
    }

    #[test]
    fn adding_files_queues_them_and_skips_what_cannot_be_optimised() {
        let mut run = BatchRun::new(2);
        let mut scan = scan_of(&[("/d/a.png", 100), ("/d/b.png", 100)]);
        scan.unsupported.push(PathBuf::from("/d/x.zip"));
        assert_eq!(run.add(scan, false), 3);
        let statuses: Vec<_> = run.rows().iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            [RowStatus::Waiting, RowStatus::Waiting, RowStatus::Skipped]
        );
        assert_eq!(
            run.rows()[2].reason,
            Some("Not a supported file".to_string())
        );
    }

    #[test]
    fn a_path_already_in_the_batch_is_not_added_twice() {
        let mut run = BatchRun::new(2);
        run.add(scan_of(&[("/d/a.png", 1), ("/d/b.png", 1)]), false);
        assert_eq!(
            run.add(scan_of(&[("/d/b.png", 1), ("/d/c.png", 1)]), false),
            1
        );
        assert_eq!(run.rows().len(), 3);
    }

    #[test]
    fn work_is_handed_out_up_to_the_limit() {
        let (mut run, ids) = started(5, 2);
        assert_eq!(ids.len(), 2);
        assert!(run.take_work().is_empty());
        let done = update(&ids[0], JobState::Done, Some(50));
        assert_eq!(run.on_update(&done, Instant::now()), Some(true));
        assert_eq!(run.take_work().len(), 1);
    }

    #[test]
    fn paused_hands_out_nothing_until_resumed() {
        let mut run = BatchRun::new(5);
        run.add(many(3), false);
        let t0 = Instant::now();
        run.pause(t0);
        assert!(run.is_paused());
        assert!(run.take_work().is_empty());
        run.resume(t0);
        assert!(!run.is_paused());
        assert_eq!(run.take_work().len(), 3);
    }

    #[test]
    fn a_start_carries_the_adjust_patch_only_when_adjusted() {
        let mut run = BatchRun::new(1);
        run.add(many(2), false);
        let Work::Start { level, patch, .. } = run.take_work().remove(0) else {
            panic!("not a start");
        };
        assert_eq!((level, patch), (Level::Normal, None));
        let mut run = BatchRun::new(1);
        run.set_adjust(Adjust {
            scale: Some(50),
            ..Adjust::default()
        });
        run.add(many(1), false);
        let Work::Start { patch, .. } = run.take_work().remove(0) else {
            panic!("not a start");
        };
        let p = patch.unwrap();
        assert_eq!((p.scale, p.compression), (Some(Some(50)), Some(None)));
    }

    #[test]
    fn adjust_patches_set_and_clear_every_field() {
        assert_eq!(
            Adjust::default().patch(),
            EditsPatch {
                compression: Some(None),
                scale: Some(None),
                format: Some(None),
                level: Some(Level::Normal),
                ..EditsPatch::default()
            }
        );
        let p = Adjust {
            extreme: true,
            compression: Some(60),
            ..Adjust::default()
        }
        .patch();
        assert_eq!(p.compression, Some(Some(5)));
        assert_eq!(p.level, Some(Level::Aggressive));
    }

    #[test]
    fn cancel_all_stops_waiting_rows_and_names_the_running_jobs() {
        let mut run = BatchRun::new(2);
        run.add(many(4), false);
        assert_eq!(run.take_work().len(), 2);
        let id0 = JobId::generate();
        assert!(run.bind(0, id0.clone()));
        assert_eq!(run.cancel_all().as_slice(), std::slice::from_ref(&id0));
        let statuses: Vec<_> = run.rows().iter().map(|r| r.status).collect();
        assert_eq!(
            statuses,
            [
                RowStatus::Working,
                RowStatus::Cancelled,
                RowStatus::Cancelled,
                RowStatus::Cancelled
            ]
        );
        assert!(!run.bind(1, JobId::generate()));
        run.cancelled(&id0);
        assert_eq!(run.rows()[0].status, RowStatus::Cancelled);
    }

    #[test]
    fn a_cancelled_rerun_keeps_its_earlier_result() {
        let (mut run, ids) = started(1, 1);
        let t0 = Instant::now();
        let mut done = update(&ids[0], JobState::Done, Some(50));
        done.can_edit = true;
        run.on_update(&done, t0);
        assert_eq!(run.rerun(t0), 1);
        assert_eq!(run.take_work().len(), 1);
        run.on_update(&done, t0);
        run.cancelled(&ids[0]);
        assert_eq!(run.rows()[0].status, RowStatus::Done);
    }

    #[test]
    fn rerun_edits_finished_rows_and_retries_the_rest() {
        let (mut run, ids) = started(4, 4);
        let t0 = Instant::now();
        let mut a = update(&ids[0], JobState::Done, Some(50));
        a.can_edit = true;
        run.on_update(&a, t0);
        run.on_update(
            &update(&ids[1], JobState::Error { reason: "x".into() }, None),
            t0,
        );
        run.skip(2, "x".into());
        run.cancelled(&ids[3]);
        assert_eq!(run.rerun(t0), 2);
        let work = run.take_work();
        assert!(matches!(&work[0], Work::Edit { row: 0, .. }));
        assert!(matches!(&work[1], Work::Rerun { row: 1, .. }));
        assert_eq!(run.rows()[2].status, RowStatus::Skipped);
        assert_eq!(run.rows()[3].status, RowStatus::Cancelled);
    }

    #[test]
    fn the_tally_counts_statuses_and_sizes() {
        let mut run = BatchRun::new(4);
        run.add(
            scan_of(&[
                ("/d/a.png", 100),
                ("/d/b.png", 50),
                ("/d/c.png", 30),
                ("/d/d.png", 70),
            ]),
            false,
        );
        let ids: Vec<JobId> = run
            .take_work()
            .iter()
            .map(|w| {
                let id = JobId::generate();
                run.bind(w.row(), id.clone());
                id
            })
            .collect();
        let t0 = Instant::now();
        for (id, state, after) in [
            (&ids[0], JobState::Done, Some(40)),
            (&ids[1], JobState::AlreadyOptimal, None),
            (&ids[2], JobState::Larger, None),
            (&ids[3], JobState::Error { reason: "x".into() }, None),
        ] {
            run.on_update(&update(id, state, after), t0);
        }
        let t = run.status(t0).tally;
        assert_eq!((t.before, t.after, t.done, t.failed), (180, 120, 1, 1));
    }

    #[test]
    fn the_phase_follows_the_rows() {
        let t0 = Instant::now();
        let mut run = BatchRun::new(5);
        assert_eq!(run.status(t0).phase, Phase::Empty);
        run.add(many(2), false);
        assert_eq!(run.status(t0).phase, Phase::Running);
        run.pause(t0);
        assert_eq!(run.status(t0).phase, Phase::Paused);
        run.resume(t0);
        let ids: Vec<JobId> = run
            .take_work()
            .iter()
            .map(|w| {
                let id = JobId::generate();
                run.bind(w.row(), id.clone());
                id
            })
            .collect();
        for id in &ids {
            run.on_update(&update(id, JobState::Done, Some(1)), t0);
        }
        assert_eq!(run.status(t0).phase, Phase::Done);
        run.set_rolling(Some((1, 3)));
        assert_eq!(run.status(t0).phase, Phase::RollingBack);
    }

    #[test]
    fn rate_and_eta_leave_out_paused_time() {
        let t0 = Instant::now();
        let mut run = BatchRun::new(20);
        run.add_at(many(20), false, t0);
        let ids: Vec<JobId> = run
            .take_work()
            .iter()
            .map(|w| {
                let id = JobId::generate();
                run.bind(w.row(), id.clone());
                id
            })
            .collect();
        run.pause(t0 + Duration::from_secs(5));
        run.resume(t0 + Duration::from_secs(10));
        for id in &ids[..10] {
            run.on_update(&update(id, JobState::Done, Some(1)), t0);
        }
        let s = run.status(t0 + Duration::from_secs(15));
        assert_eq!((s.rate, s.eta_secs), (1.0, Some(10)));
    }

    #[test]
    fn only_changed_rows_are_flushed() {
        let t0 = Instant::now();
        let mut run = BatchRun::new(5);
        run.add(many(3), false);
        assert_eq!(run.take_flush(t0).unwrap().0.len(), 3);
        assert!(run.take_flush(t0).is_none());
        run.skip(1, "x".into());
        let rows = run.take_flush(t0).unwrap().0;
        assert_eq!(rows.iter().map(|r| r.index).collect::<Vec<_>>(), [1]);
    }

    #[test]
    fn rollback_takes_done_rows_that_can_restore() {
        let (mut run, ids) = started(3, 3);
        let t0 = Instant::now();
        let mut a = update(&ids[0], JobState::Done, Some(1));
        a.can_restore = true;
        run.on_update(&a, t0);
        run.on_update(&update(&ids[1], JobState::Done, Some(1)), t0);
        run.on_update(&update(&ids[2], JobState::Larger, None), t0);
        assert_eq!(run.rollback_targets(), [ids[0].clone()]);
    }

    #[test]
    fn pump_releases_the_lock_before_running_work() {
        let run = Arc::new(Mutex::new(BatchRun::new(3)));
        run.lock().unwrap().add(many(3), false);
        let (tx, rx) = mpsc::channel();
        let worker = Arc::clone(&run);
        thread::spawn(move || {
            pump(&worker, &mut |_| {
                worker.lock().unwrap().skip(0, "x".into());
            });
            tx.send(()).unwrap();
        });
        assert!(rx.recv_timeout(Duration::from_secs(2)).is_ok());
    }

    #[test]
    fn clear_returns_every_job_and_empties_the_batch() {
        let (mut run, ids) = started(2, 2);
        let mut cleared = run.clear();
        cleared.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        let mut want = ids;
        want.sort_by(|a, b| a.as_str().cmp(b.as_str()));
        assert_eq!(cleared, want);
        assert!(run.rows().is_empty());
        assert_eq!(run.status(Instant::now()).phase, Phase::Empty);
    }
}
