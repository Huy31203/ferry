//! Clipboard triggers (spec §5.2, §5.3): the watcher's automatic jobs and the
//! optimise hotkeys. The rules are `ferry_core::trigger`; this file reads the
//! clipboard and starts jobs.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::Local;
use ferry_core::i18n::{t, tf};
use ferry_core::optimise::Level;
use ferry_core::pipeline::Job;
use ferry_core::trigger::{self, HotkeyPlan, Skip, Snapshot};
use tauri::{AppHandle, Manager};
use tracing::{debug, info, warn};

use crate::state::{self, AppState};
use crate::{jobs, platform, tray};

const RETRY_AFTER: Duration = Duration::from_millis(100);

/// Watch the clipboard and optimise copied images while Running.
pub fn start(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    platform::clipboard::watch(tx);
    let app = app.clone();
    thread::Builder::new()
        .name("ferry-trigger".into())
        .spawn(move || {
            for seq in rx {
                on_change(&app, seq);
            }
        })
        .expect("spawn the trigger thread");
}

fn on_change(app: &AppHandle, seq: u64) {
    let state = app.state::<AppState>();
    if state.clipboard.lock().unwrap().is_own(seq) {
        return;
    }
    let settings = state.settings.read().unwrap().clone();
    let run_state = *state.run_state.lock().unwrap();
    if !trigger::wants_read(&settings, run_state) {
        return;
    }
    let Some(snap) = read() else {
        return; // automatic triggers never show a card for a failed read (§6)
    };
    let was_paused = *state.run_state.lock().unwrap() == ferry_core::run_state::RunState::Paused;
    let decision = trigger::automatic(
        &snap,
        &settings,
        state.gifsicle.is_some(),
        &mut state.run_state.lock().unwrap(),
    );
    match decision {
        Ok(_) => start_clipboard_job(app, snap, Level::Normal),
        Err(Skip::NotRunning) => {
            info!("skipped one copy while paused or stopped");
            if was_paused {
                tray::rebuild(app); // Paused has been spent: back to Running
            }
        }
        Err(skip) => debug!(?skip, "clipboard change left alone"),
    }
}

/// The optimise hotkeys and tray items: whatever the run state (§5.3).
pub fn optimise_now(app: &AppHandle, level: Level) {
    let Some(snap) = read() else {
        jobs::notice(app, t("error.clipboard_read"));
        return;
    };
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();
    // Ferry's own file paths on the clipboard are its cards: re-run them.
    if !snap.concealed && state.clipboard.lock().unwrap().is_own(snap.seq) {
        let own = state::jobs_with_outputs(&state.jobs.lock().unwrap(), &snap.files);
        if let Some(ids) = own {
            for id in ids {
                if let Err(reason) = jobs::rerun(app, &id, level == Level::Aggressive) {
                    jobs::notice(app, reason);
                }
            }
            return;
        }
    }
    match trigger::hotkey(&snap, &settings, state.gifsicle.is_some()) {
        HotkeyPlan::Image(_) => start_clipboard_job(app, snap, level),
        HotkeyPlan::Files(paths) => {
            for path in paths {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                match Job::file(&path, &settings, state.gifsicle.is_some()) {
                    Ok(job) => {
                        if !jobs::start(app, job, level, None) {
                            jobs::notice(app, tf("error.busy", &[("name", &name)]));
                        }
                    }
                    Err(reason) => jobs::notice(app, reason),
                }
            }
        }
        HotkeyPlan::Nothing(skip) => jobs::notice(app, skip.reason()),
    }
}

fn start_clipboard_job(app: &AppHandle, snap: Snapshot, level: Level) {
    let seq = snap.seq;
    let Some((format, bytes)) = snap.image else {
        return;
    };
    let state = app.state::<AppState>();
    let template = state
        .settings
        .read()
        .unwrap()
        .clipboard
        .file_template
        .clone();
    let now = Local::now().naive_local();
    let job = Job::clipboard(
        format,
        bytes,
        snap.from_bitmap,
        &template,
        &state.work_dir,
        now,
    );
    crate::pipe_jobs::start_assigned(
        app,
        job,
        crate::pipe_jobs::Trigger::Clipboard,
        level,
        Some(seq),
    ); // stream F: pipelines
}

/// Read the clipboard, retrying once after 100 ms (§6).
fn read() -> Option<Snapshot> {
    platform::clipboard::read()
        .or_else(|first| {
            debug!("clipboard read failed, retrying: {first}");
            thread::sleep(RETRY_AFTER);
            platform::clipboard::read()
        })
        .map_err(|e| warn!("could not read the clipboard: {e}"))
        .ok()
}
