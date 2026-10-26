//! Screenshot, screen-recording and watched-folder jobs (stream C). The watcher
//! itself is `ferry_core::watch`; this module starts it, follows the settings
//! and turns every ready file into an ordinary job.

use std::path::PathBuf;
use std::sync::Mutex;

use ferry_core::i18n::tf;
use ferry_core::optimise::Level;
use ferry_core::pipeline::Job;
use ferry_core::run_state::RunState;
use ferry_core::settings::Watch;
use ferry_core::watch::engine::{Notice, Origin, Ready};
use ferry_core::watch::media::Media;
use ferry_core::watch::plan::WatchStatus;
use ferry_core::watch::service::{RESCAN_EVERY, ServiceConfig, ServiceHandle, spawn};
use ferry_core::watch::settle::SettleConfig;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{debug, info, warn};

use crate::state::{AppState, relock};
use crate::pipe_jobs::{self, Trigger};
use crate::{jobs, platform, tray};

pub mod commands;
pub mod hooks;

/// The running watcher thread, if it could start.
struct WatchService(Mutex<Option<ServiceHandle>>);

/// Ferry's own folders: never watched, and their events are dropped.
fn excluded(app: &AppHandle) -> Vec<PathBuf> {
    let state = app.state::<AppState>();
    let mut dirs = vec![
        state.backups_dir.clone(),
        state.work_dir.clone(),
        state.log_dir.clone(),
    ];
    dirs.extend(app.path().app_config_dir());
    dirs
}

pub fn start(app: &AppHandle) {
    let watch = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .watch
        .clone();
    let notices = app.clone();
    let config = ServiceConfig {
        watch,
        home: app.path().home_dir().unwrap_or_default(),
        excluded: excluded(app),
        captures: Box::new(platform::capture::capture_dirs),
        settle: SettleConfig::default(),
        rescan_every: RESCAN_EVERY,
    };
    let handle = match spawn(config, Box::new(move |notice| on_notice(&notices, notice))) {
        Ok(handle) => Some(handle),
        Err(e) => {
            warn!("could not start the watchers: {e}");
            None
        }
    };
    app.manage(WatchService(Mutex::new(handle)));
}

/// The watched folders in Settings changed.
pub fn reload(app: &AppHandle, watch: &Watch) {
    if let Some(service) = app.try_state::<WatchService>()
        && let Some(handle) = relock(&service.0).as_ref()
    {
        handle.reload(watch.clone());
    }
}

pub fn status(app: &AppHandle) -> WatchStatus {
    app.try_state::<WatchService>()
        .and_then(|service| relock(&service.0).as_ref().map(ServiceHandle::status))
        .unwrap_or_default()
}

fn on_notice(app: &AppHandle, notice: Notice) {
    match notice {
        Notice::Ready(ready) => {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || dispatch_ready(&app, ready));
        }
        Notice::BurstSkipped {
            folder,
            count,
            show_results,
        } => {
            info!(count, "skipped a burst of files in a watched folder");
            if show_results {
                let count = count.to_string();
                jobs::notice(
                    app,
                    tf(
                        "watch.burst_skipped",
                        &[("count", &count), ("folder", &folder)],
                    ),
                );
            }
        }
        Notice::Status(status) => {
            let _ = app.emit("watch:status", &status);
        }
    }
}

/// A watched file finished writing: make it a job, unless Ferry is stopped, or
/// paused (which skips exactly this one).
fn dispatch_ready(app: &AppHandle, ready: Ready) {
    let state = app.state::<AppState>();
    if *state.run_state.lock().unwrap() == RunState::Stopped {
        return;
    }
    let settings = state.settings.read().unwrap().clone();
    let built = match ready.media {
        Media::Image => Some(Job::file(&ready.path, &settings, state.gifsicle.is_some())),
        Media::Video => hooks::video_job(app, &ready.path, &settings),
        Media::Audio => hooks::audio_job(app, &ready.path, &settings),
        Media::Pdf => hooks::pdf_job(app, &ready.path, &settings),
    };
    let job = match built {
        None => {
            debug!(media = ?ready.media, "no support for this media yet");
            return;
        }
        Some(Err(reason)) => {
            info!(%reason, "watched file left alone");
            return;
        }
        Some(Ok(job)) => job,
    };
    let was_paused = *state.run_state.lock().unwrap() == RunState::Paused;
    if !state.run_state.lock().unwrap().take_automatic() {
        info!("skipped one watched file while paused or stopped");
        if was_paused {
            tray::rebuild(app); // Paused has been spent: back to Running
        }
        return;
    }
    let id = job.id.clone();
    if !ready.show_results {
        relock(&state.quiet).insert(id.clone());
    }
    let started = match ready.origin {
        // stream F: a folder's pipeline runs its files (and auto-copies them like a drop)
        Origin::Folder { .. } => match ready.pipeline {
            Some(id) => pipe_jobs::start_assigned(app, job, Trigger::Pipeline(id), Level::Normal, None),
            None => jobs::start_auto_copied(app, job, Level::Normal),
        },
        Origin::Capture { .. } => jobs::start(app, job, Level::Normal, None),
    };
    if !started {
        relock(&state.quiet).remove(&id);
        info!("already being optimised");
        return;
    }
    info!(origin = ?ready.origin, "watched file started");
}
