//! Drops and batches: the batch window, its dispatcher, and the state the job
//! hooks in `jobs.rs` consult. Pure logic is in `ferry_core::{intake, drag, batch}`.

pub mod batch;
pub mod intake;
pub mod list;
pub mod watch;
pub mod zone;

use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::mpsc;

use ferry_core::batch::{BatchRun, pump};
use ferry_core::intake::keep_source_rules;
use ferry_core::job::JobId;
use ferry_core::pipeline::RunOptions;
use tauri::{AppHandle, Manager};
use tracing::warn;

pub use zone::ZONE;

pub struct DropState {
    /// Never held across a `jobs::*` call, a `tray::*` call or an emit:
    /// `jobs::show` calls back into `batch::claim`, which takes this lock.
    pub batch: Mutex<BatchRun>,
    /// Jobs whose runs and restores keep the original (the keep modifier was held).
    keep: Mutex<HashSet<JobId>>,
    /// Jobs that came from a drop, for auto-copy and the zone.
    dropped: Mutex<HashSet<JobId>>,
    /// The zone window's state; same lock rule as `batch`.
    pub zone: Mutex<zone::ZoneCtl>,
    wake: mpsc::Sender<()>,
}

impl DropState {
    fn new(limit: usize) -> (Self, mpsc::Receiver<()>) {
        let (wake, rx) = mpsc::channel();
        let state = Self {
            batch: Mutex::new(BatchRun::new(limit)),
            keep: Mutex::new(HashSet::new()),
            dropped: Mutex::new(HashSet::new()),
            zone: Mutex::new(zone::ZoneCtl::default()),
            wake,
        };
        (state, rx)
    }
}

/// Manage the state, start the dispatcher and flush threads, and route the
/// batch row menu's items.
pub fn init(app: &AppHandle) -> tauri::Result<()> {
    let limit = std::thread::available_parallelism().map_or(2, |n| n.get());
    let (state, rx) = DropState::new(limit);
    app.manage(state);
    let dispatcher = app.clone();
    spawn_thread("ferry-batch", move || {
        for () in rx {
            let state = dispatcher.state::<DropState>();
            pump(&state.batch, &mut |work| batch::exec(&dispatcher, work));
        }
    });
    let flusher = app.clone();
    spawn_thread("ferry-batch-flush", move || batch::flush_loop(&flusher));
    app.on_menu_event(|app, event| {
        batch::on_menu(app, event.id().as_ref());
        list::on_menu(app, event.id().as_ref());
    });
    zone::create(app)?;
    watch::start(app);
    Ok(())
}

fn spawn_thread(name: &str, work: impl FnOnce() + Send + 'static) {
    if let Err(e) = std::thread::Builder::new().name(name.into()).spawn(work) {
        warn!("could not start {name}: {e}");
    }
}

/// Remember that `id` came from a drop; with `keep`, its runs and restores
/// write beside the original.
pub fn mark_dropped(app: &AppHandle, id: &JobId, keep: bool) {
    let state = app.state::<DropState>();
    state.dropped.lock().unwrap().insert(id.clone());
    if keep {
        state.keep.lock().unwrap().insert(id.clone());
    }
}

/// Jobs that are gone.
pub fn forget(app: &AppHandle, ids: &[JobId]) {
    let state = app.state::<DropState>();
    let mut dropped = state.dropped.lock().unwrap();
    for id in ids {
        dropped.remove(id);
    }
    drop(dropped);
    let mut keep = state.keep.lock().unwrap();
    for id in ids {
        keep.remove(id);
    }
}

/// Whether `id` came from a drop.
#[allow(dead_code, reason = "called by stream I's auto-copy")]
pub fn came_from_drop(app: &AppHandle, id: &JobId) -> bool {
    app.state::<DropState>()
        .dropped
        .lock()
        .unwrap()
        .contains(id)
}

/// `options` for a run or restore of `id`: the keep rules when it was dropped with the modifier.
/// Apply it after `pipe_jobs::options`, which would otherwise put In place back.
pub fn with_keep(app: &AppHandle, id: &JobId, options: RunOptions) -> RunOptions {
    let keep = app.state::<DropState>().keep.lock().unwrap().contains(id);
    keep_options(keep, options)
}

fn keep_options(keep: bool, mut options: RunOptions) -> RunOptions {
    if keep {
        options.output = keep_source_rules(&options.output);
        options.video.output = keep_source_rules(&options.video.output);
        options.audio.settings.output = keep_source_rules(&options.audio.settings.output);
        options.pdf_output = keep_source_rules(&options.pdf_output);
    }
    options
}

/// A `JobId` from its text (the type has no public constructor).
pub(crate) fn job_id(text: &str) -> Option<JobId> {
    serde_json::from_value(serde_json::Value::String(text.into())).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::cancel::CancelToken;
    use ferry_core::settings::{Location, Settings};

    fn options(dir: &std::path::Path) -> RunOptions {
        RunOptions::new(
            &Settings::default(),
            None,
            dir.into(),
            |_| Ok(()),
            CancelToken::new(),
        )
    }

    /// Every rule set a job kind reads: images, video, audio and PDF.
    fn rule_sets(o: &RunOptions) -> [&ferry_core::settings::Output; 4] {
        [
            &o.output,
            &o.video.output,
            &o.audio.settings.output,
            &o.pdf_output,
        ]
    }

    #[test]
    fn keeping_the_original_writes_beside_it_and_otherwise_changes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let kept = keep_options(true, options(dir.path()));
        for out in rule_sets(&kept) {
            assert_eq!(out.optimised.location, Location::SameFolder);
            assert_eq!(out.converted.location, Location::SameFolder);
        }
        let plain = keep_options(false, options(dir.path()));
        let fresh = options(dir.path());
        assert_eq!(rule_sets(&plain), rule_sets(&fresh));
    }

    #[test]
    fn keeping_wins_over_a_pipeline_that_names_in_place() {
        // `pipe_jobs::options` runs first and may put In place back; keeping is applied after it.
        let dir = tempfile::tempdir().unwrap();
        let mut o = options(dir.path());
        let in_place = ferry_core::settings::OutputRule {
            location: Location::InPlace,
            ..Default::default()
        };
        for out in [
            &mut o.output,
            &mut o.video.output,
            &mut o.audio.settings.output,
            &mut o.pdf_output,
        ] {
            out.optimised = in_place.clone();
            out.converted = in_place.clone();
        }
        let kept = keep_options(true, o);
        for out in rule_sets(&kept) {
            assert_eq!(out.optimised.location, Location::SameFolder);
            assert_eq!(out.converted.location, Location::SameFolder);
        }
    }
}
