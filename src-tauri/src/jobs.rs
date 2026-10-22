//! Runs jobs off the UI thread and keeps their cards up to date (spec §4.3, §5.4).
//! The work itself is `ferry_core::pipeline`; this file schedules it, puts
//! results on the clipboard and sends `job:update` to the stack.

use std::path::Path;
use std::sync::Arc;
use std::time::Instant;

use ferry_core::cancel::CancelToken;
use ferry_core::i18n::{t, tf};
use ferry_core::job::{JobId, JobState, JobUpdate, Source};
use ferry_core::optimise::Level;
use ferry_core::pipeline::{self, Job, RunOptions};
use ferry_core::sniff::Format;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::state::{AppState, Entry, output_busy, relock};
use crate::{platform, tray, windows::STACK};

/// Show the job's Queued card now (NFR-P3) and run it when a worker is free.
/// `source_seq` is the clipboard copy a clipboard job came from; its first run
/// leaves the result off the clipboard if the user has copied something since.
/// Returns false, starting nothing, when a queued or running job already
/// writes the same file: two writers would share one temporary file.
pub fn start(app: &AppHandle, job: Job, level: Level, source_seq: Option<u64>) -> bool {
    let state = app.state::<AppState>();
    let queued = job.update(JobState::Queued, None);
    let entry = Arc::new(Entry::new(job, source_seq, queued.clone()));
    {
        let mut jobs = state.jobs.lock().unwrap();
        if output_busy(&jobs, &entry) {
            return false;
        }
        jobs.insert(queued.id.clone(), entry.clone());
    }
    show(app, queued);
    tray::refresh(app);
    spawn(app, entry, level);
    true
}

/// Show a card that is not a job, such as "Nothing to optimise on the clipboard".
pub fn notice(app: &AppHandle, reason: String) {
    info!(%reason, "notice");
    show(app, pipeline::notice(reason));
}

/// Run the job again from its original: retry and re-optimise keep its level,
/// "Try aggressive" asks for Aggressive.
pub fn rerun(app: &AppHandle, id: &JobId, aggressive: bool) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let (queued, level) = {
        let job = relock(&entry.job);
        let level = if aggressive {
            Level::Aggressive
        } else {
            job.level
        };
        (job.update(JobState::Queued, None), level)
    };
    *entry.cancel.lock().unwrap() = CancelToken::new();
    entry.begin_rerun(queued.clone());
    show(app, queued);
    tray::refresh(app);
    spawn(app, entry, level);
    Ok(())
}

/// Put the original back (§5.5).
pub fn restore(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let options = run_options(app, CancelToken::new());
    let finished = relock(&entry.job).restore(&options, &mut |format, bytes, file| {
        deliver(app, format, bytes, file, None)
    });
    finish(app, &entry, finished.update, finished.cause, None);
    Ok(())
}

/// Copy the card's result again: the image for a clipboard job, the file for a file job.
pub fn copy(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let job = relock(&entry.job);
    let state = app.state::<AppState>();
    let copy_paths = state.settings.read().unwrap().clipboard.copy_paths;
    let mut log = state.clipboard.lock().unwrap();
    let seq = match job.source {
        Source::Clipboard => {
            let bytes = std::fs::read(&job.output).map_err(|e| e.to_string())?;
            let file = copy_paths.then_some(job.output.as_path());
            platform::clipboard::write_image(job.format, &bytes, file)?
        }
        Source::File => platform::clipboard::write_files(std::slice::from_ref(&job.output))?,
    };
    log.record(seq, None);
    Ok(())
}

/// Stop a queued or running job and leave its file as it was. A first run's
/// card is already gone; a cancelled re-run goes back to the earlier result's
/// card, which keeps its Restore.
pub fn cancel(app: &AppHandle, id: &JobId) {
    let state = app.state::<AppState>();
    let back = {
        let mut jobs = state.jobs.lock().unwrap();
        let Some(entry) = jobs.get(id).cloned() else {
            return;
        };
        entry.cancel.lock().unwrap().cancel();
        let back = if entry.is_busy() {
            entry.cancel_back()
        } else {
            None
        };
        if back.is_none() {
            jobs.remove(id);
        }
        back
    };
    if let Some(card) = back {
        show(app, card);
    }
    tray::refresh(app);
}

/// The output file of a card, for "Show in folder".
pub fn output(app: &AppHandle, id: &JobId) -> Option<std::path::PathBuf> {
    let state = app.state::<AppState>();
    let path = state.jobs.lock().unwrap().get(id)?.output.clone();
    path.exists().then_some(path)
}

/// "Force clean working directory" (§5.6): delete the working files of every
/// finished job and forget those jobs (their cards and Recent results), keeping
/// what queued or running jobs still use. Backups are untouched.
pub fn force_clean(app: &AppHandle) -> Result<ferry_core::workdir::CleanReport, String> {
    let state = app.state::<AppState>();
    let (keep, busy) = {
        let mut jobs = state.jobs.lock().unwrap();
        jobs.retain(|_, entry| entry.is_busy());
        let keep: Vec<std::path::PathBuf> = jobs.values().map(|e| e.output.clone()).collect();
        let busy: Vec<JobUpdate> = jobs
            .values()
            .map(|e| e.last.lock().unwrap().clone())
            .collect();
        (keep, busy)
    };
    state.recent.lock().unwrap().clear();
    state
        .clipboard
        .lock()
        .unwrap()
        .accumulated
        .retain(|f| keep.contains(f));
    let report = ferry_core::workdir::clean(&state.work_dir, &keep).map_err(|e| e.to_string())?;
    info!(
        files = report.files,
        bytes = report.bytes,
        "working directory cleaned"
    );
    // Empty the stack, then put back the cards of the jobs that carry on.
    state.stack.lock().unwrap().pending.clear();
    if let Err(e) = app.emit_to(STACK, "stack:clear", ()) {
        warn!("could not clear the stack: {e}");
    }
    for card in busy {
        show(app, card);
    }
    tray::rebuild(app);
    Ok(report)
}

/// Send a card to the stack, or hold it until the stack page is listening.
pub fn show(app: &AppHandle, update: JobUpdate) {
    let state = app.state::<AppState>();
    if !state.settings.read().unwrap().floating.enabled {
        return;
    }
    let mut inbox = state.stack.lock().unwrap();
    if !inbox.ready {
        inbox.pending.retain(|u| u.id != update.id);
        inbox.pending.push(update);
        return;
    }
    if let Err(e) = app.emit_to(STACK, "job:update", &update) {
        warn!("could not send a card: {e}");
    }
}

/// Put an optimised image on the clipboard, or the collected files when
/// "accumulate results" is on. The clipboard log stays locked until the write's
/// sequence number is recorded, so the watcher cannot mistake it for the user's.
/// With a `source` copy, nothing is written if the user has copied since.
fn deliver(
    app: &AppHandle,
    format: Format,
    bytes: &[u8],
    file: &Path,
    source: Option<u64>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clipboard.clone();
    let mut log = state.clipboard.lock().unwrap();
    if let Some(source) = source
        && !log.may_replace(source, platform::clipboard::seq())
    {
        info!("the clipboard changed while the job ran; result left off it");
        return Ok(());
    }
    let seq = if settings.accumulate {
        if !log.accumulated.iter().any(|f| f == file) {
            log.accumulated.push(file.to_owned());
        }
        platform::clipboard::write_files(&log.accumulated)?
    } else {
        platform::clipboard::write_image(format, bytes, settings.copy_paths.then_some(file))?
    };
    log.record(seq, source);
    Ok(())
}

fn spawn(app: &AppHandle, entry: Arc<Entry>, level: Level) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let permits = app.state::<AppState>().permits.clone();
        let Ok(_permit) = permits.acquire_owned().await else {
            return;
        };
        let cancel = entry.cancel.lock().unwrap().clone();
        if cancel.is_cancelled() {
            return;
        }
        let (worker_app, worker_entry) = (app.clone(), entry.clone());
        let joined = tauri::async_runtime::spawn_blocking(move || {
            run(&worker_app, &worker_entry, level, cancel);
        })
        .await;
        // A panic in an encoder must still end the job (spec §6).
        if let Err(e) = joined {
            let update = {
                let job = relock(&entry.job);
                job.update(
                    JobState::Error {
                        reason: t("error.unexpected"),
                    },
                    None,
                )
            };
            finish(&app, &entry, update, Some(e.to_string()), None);
        }
    });
}

fn run(app: &AppHandle, entry: &Arc<Entry>, level: Level, cancel: CancelToken) {
    let options = run_options(app, cancel.clone());
    let started = Instant::now();
    let mut job = relock(&entry.job);
    info!(
        job = job.id.as_str(),
        source = ?job.source,
        format = ?job.format,
        ?level,
        before = job.before,
        "job started"
    );
    let state = app.state::<AppState>();
    // Only the first run checks; a re-run is the user's explicit request.
    let source = entry.source_seq.lock().unwrap().take();
    let finished = job.run(
        level,
        &options,
        &state.backups,
        &mut |update| {
            // A cancelled job's card has already been dealt with.
            if cancel.is_cancelled() {
                return;
            }
            *entry.last.lock().unwrap() = update.clone();
            show(app, update);
        },
        &mut |format, bytes, file| deliver(app, format, bytes, file, source),
    );
    drop(job);
    match finished {
        Some(f) => finish(app, entry, f.update, f.cause, Some(started)),
        None => {
            info!(ms = started.elapsed().as_millis() as u64, "job cancelled");
            tray::refresh(app);
        }
    }
}

/// Record a job's final card: log it, show it, keep it in Recent results.
fn finish(
    app: &AppHandle,
    entry: &Entry,
    update: JobUpdate,
    cause: Option<String>,
    started: Option<Instant>,
) {
    let ms = started.map(|s| s.elapsed().as_millis() as u64);
    match &update.state {
        JobState::Error { reason } => {
            warn!(job = update.id.as_str(), ?ms, %reason, cause = cause.as_deref(), "job failed");
            app.state::<AppState>()
                .tray_error
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        state => info!(
            job = update.id.as_str(),
            ?ms,
            ?state,
            before = update.before,
            after = update.after,
            "job finished"
        ),
    }
    *entry.last.lock().unwrap() = update.clone();
    let state = app.state::<AppState>();
    state.remember(&update);
    show(app, update);
    tray::rebuild(app);
}

fn idle_entry(app: &AppHandle, id: &JobId) -> Result<Arc<Entry>, String> {
    let state = app.state::<AppState>();
    let entry = state
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("this card's job is gone")?;
    if entry.is_busy() {
        return Err("the job is still running".into());
    }
    if output_busy(&state.jobs.lock().unwrap(), &entry) {
        return Err(tf("error.busy", &[("name", &entry_name(&entry))]));
    }
    Ok(entry)
}

fn run_options(app: &AppHandle, cancel: CancelToken) -> RunOptions {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap();
    RunOptions::new(&settings, state.gifsicle.clone(), cancel)
}

fn entry_name(entry: &Entry) -> String {
    entry
        .output
        .file_name()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}
