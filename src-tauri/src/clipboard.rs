//! Clipboard triggers (spec §5.2, §5.3): the watcher's automatic jobs and the
//! optimise hotkeys. The rules are `ferry_core::trigger`; this file reads the
//! clipboard and starts jobs.

use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::Local;
use ferry_core::i18n::{t, tf};
use ferry_core::intake::{self, MediaKind};
use ferry_core::optimise::Level;
use ferry_core::pipeline::Job;
use ferry_core::run_state::RunState;
use ferry_core::settings::Settings;
use ferry_core::trigger::{self, HotkeyPlan, Skip, Snapshot};
use tauri::{AppHandle, Manager};
use tracing::{debug, info, warn};

use crate::state::{self, AppState};
use crate::{jobs, platform, tray};

const RETRY_AFTER: Duration = Duration::from_millis(100);
/// How long a browser copy may take to add its image (a 4K JPEG re-encodes to PNG).
const IMAGE_WAIT: Duration = Duration::from_secs(5);

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
    let Some(snap) = read_settled() else {
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
        Err(Skip::NoImage) if !snap.files.is_empty() => {
            optimise_copied_files(app, &snap, &settings)
        }
        Err(skip) => debug!(?skip, "clipboard change left alone"),
    }
}

/// An image, video or PDF that is not one of Ferry's own results.
fn wanted_file(path: &Path, own: &[PathBuf]) -> bool {
    !own.iter().any(|o| o == path)
        && matches!(
            intake::classify_file(path),
            Ok(Some(c)) if matches!(c.kind, MediaKind::Image | MediaKind::Video | MediaKind::Pdf)
        )
}

/// FR-T3: copied image, video and PDF files, when "Optimise copied files"
/// is on. They start as the hotkey starts copied files, under the clipboard's
/// pipeline for their media.
fn optimise_copied_files(app: &AppHandle, snap: &Snapshot, settings: &Settings) {
    let state = app.state::<AppState>();
    // Copying a finished result to paste it elsewhere must not re-encode it.
    let own = state::own_outputs(&state.jobs.lock().unwrap());
    let wanted = |path: &Path| wanted_file(path, &own);
    let was_paused = *state.run_state.lock().unwrap() == RunState::Paused;
    let plan = trigger::automatic_files(
        snap,
        settings,
        &wanted,
        &mut state.run_state.lock().unwrap(),
    );
    match plan {
        Ok(files) => {
            for path in files {
                let name = path.file_name().unwrap_or_default().to_string_lossy();
                match Job::file(&path, settings, state.gifsicle.is_some()) {
                    Ok(job) => {
                        if !crate::pipe_jobs::start_assigned(
                            app,
                            job,
                            crate::pipe_jobs::Trigger::Clipboard,
                            Level::Normal,
                            None,
                        ) {
                            jobs::notice(app, tf("error.busy", &[("name", &name)]));
                        }
                    }
                    Err(reason) => jobs::notice(app, reason),
                }
            }
        }
        Err(Skip::NotRunning) => {
            info!("skipped copied files while paused or stopped");
            if was_paused {
                tray::rebuild(app); // Paused has been spent: back to Running
            }
        }
        Err(skip) => debug!(?skip, "copied files left alone"),
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
    )
    .with_default_conversion(&state.settings.read().unwrap().formats); // sE pdf-formats
    crate::pipe_jobs::start_assigned(
        app,
        job,
        crate::pipe_jobs::Trigger::Clipboard,
        level,
        Some(seq),
    ); // stream F: pipelines
}

/// Like `read`, but a browser copy still missing its image is read again until the
/// image arrives, the user copies something else, or `IMAGE_WAIT` is up.
fn read_settled() -> Option<Snapshot> {
    let mut snap = read()?;
    let start = std::time::Instant::now();
    while snap.image.is_none()
        && snap.files.is_empty()
        && !snap.concealed
        && platform::clipboard::may_gain_image()
        && platform::clipboard::seq() == snap.seq
        && start.elapsed() < IMAGE_WAIT
    {
        thread::sleep(RETRY_AFTER);
        snap = read()?;
    }
    Some(snap)
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copied_files_skip_ferrys_own_results() {
        let dir = tempfile::tempdir().unwrap();
        let (mine, theirs) = (dir.path().join("mine.pdf"), dir.path().join("theirs.pdf"));
        std::fs::write(&mine, b"%PDF-1.7\n").unwrap();
        std::fs::write(&theirs, b"%PDF-1.7\n").unwrap();
        let own = [mine.clone()];
        assert!(!wanted_file(&mine, &own));
        assert!(wanted_file(&theirs, &own));
        assert!(!wanted_file(&dir.path().join("gone.pdf"), &[]));
    }

    #[test]
    fn a_copied_image_file_is_wanted_like_a_video() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("shot.png");
        std::fs::write(&png, b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR").unwrap();
        assert!(wanted_file(&png, &[]));
        assert!(!wanted_file(&png, std::slice::from_ref(&png)));
    }

    #[test]
    fn a_copied_folder_is_never_wanted_whatever_it_is_called() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("clips.mp4");
        std::fs::create_dir(&folder).unwrap();
        assert!(!wanted_file(&folder, &[]));
    }
}
