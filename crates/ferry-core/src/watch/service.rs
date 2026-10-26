//! The thread that owns the `notify` watcher and drives the engine. It sleeps
//! until the next event, the next settle or burst deadline, or the next rescan
//! (NFR-R1: with nothing pending it wakes every 30 s only).

use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::sync::{Arc, Mutex, PoisonError};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use notify::{RecommendedWatcher, RecursiveMode, Watcher};

use super::capture::CaptureDir;
use super::engine::{Engine, Notice, RealFs};
use super::events::from_notify;
use super::plan::{self, Resolve, WatchStatus};
use super::settle::SettleConfig;
use crate::settings::Watch;

pub const RESCAN_EVERY: Duration = Duration::from_secs(30);

pub struct ServiceConfig {
    pub watch: Watch,
    pub home: PathBuf,
    /// Ferry's own folders: never watched, their events dropped.
    pub excluded: Vec<PathBuf>,
    /// The capture folders now; asked again on every replan, because the user
    /// can move the screenshot folder at any time.
    pub captures: Box<dyn Fn() -> Vec<CaptureDir> + Send>,
    pub settle: SettleConfig,
    pub rescan_every: Duration,
}

enum Msg {
    Fs(notify::Result<notify::Event>),
    Reload(Watch),
    Rescan,
    Stop,
}

pub struct ServiceHandle {
    tx: Sender<Msg>,
    status: Arc<Mutex<WatchStatus>>,
    thread: Option<JoinHandle<()>>,
}

impl ServiceHandle {
    /// The watched list in Settings changed.
    pub fn reload(&self, watch: Watch) {
        let _ = self.tx.send(Msg::Reload(watch));
    }

    /// Look at the folders again now (a missing one may have appeared).
    pub fn rescan(&self) {
        let _ = self.tx.send(Msg::Rescan);
    }

    pub fn status(&self) -> WatchStatus {
        self.status
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .clone()
    }
}

impl Drop for ServiceHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(Msg::Stop);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub fn spawn(
    config: ServiceConfig,
    on_notice: Box<dyn FnMut(Notice) + Send>,
) -> io::Result<ServiceHandle> {
    let (tx, rx) = mpsc::channel();
    let events = tx.clone();
    let watcher = notify::recommended_watcher(move |res| {
        let _ = events.send(Msg::Fs(res));
    })
    .map_err(io::Error::other)?;
    let status = Arc::new(Mutex::new(WatchStatus::default()));
    let mut service = Service {
        engine: Engine::new(
            plan::build(&Watch::default(), &[], &config.home, &[], &mut |_| {
                Resolve::Missing
            }),
            config.settle,
        ),
        watcher,
        watched: Vec::new(),
        watch: config.watch.clone(),
        config,
        status: Arc::clone(&status),
        on_notice,
    };
    let thread = std::thread::Builder::new()
        .name("ferry-watch".into())
        .spawn(move || service.run(&rx))?;
    Ok(ServiceHandle {
        tx,
        status,
        thread: Some(thread),
    })
}

struct Service {
    config: ServiceConfig,
    engine: Engine,
    watcher: RecommendedWatcher,
    /// The directories `watcher` was asked to watch.
    watched: Vec<PathBuf>,
    watch: Watch,
    status: Arc<Mutex<WatchStatus>>,
    on_notice: Box<dyn FnMut(Notice) + Send>,
}

impl Service {
    fn run(&mut self, rx: &mpsc::Receiver<Msg>) {
        self.replan();
        let mut next_rescan = Instant::now() + self.config.rescan_every;
        loop {
            let now = Instant::now();
            let wait = self
                .engine
                .next_wake(now)
                .unwrap_or(Duration::MAX)
                .min(next_rescan.saturating_duration_since(now));
            match rx.recv_timeout(wait) {
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Ok(Msg::Reload(watch)) => {
                    self.watch = watch;
                    self.replan();
                }
                Ok(Msg::Rescan) => self.replan(),
                Ok(Msg::Fs(Ok(ev))) if ev.need_rescan() => {
                    crate::wdiag!("raw RESCAN {:?}", ev);
                    self.replan()
                }
                Ok(Msg::Fs(Ok(ev))) => {
                    crate::wdiag!("raw {:?} {:?}", ev.kind, ev.paths);
                    let now = Instant::now();
                    for event in from_notify(&ev) {
                        self.engine.on_event(event, now);
                    }
                }
                // The next rescan repairs the watch.
                Ok(Msg::Fs(Err(e))) => crate::wdiag!("raw ERR {e:?}"),
                Err(RecvTimeoutError::Timeout) => {}
            }
            if Instant::now() >= next_rescan {
                self.replan();
                next_rescan = Instant::now() + self.config.rescan_every;
            }
            for notice in self.engine.tick(Instant::now(), &RealFs) {
                self.forward(notice);
            }
        }
    }

    fn forward(&mut self, notice: Notice) {
        if let Notice::Status(status) = &notice {
            *self.status.lock().unwrap_or_else(PoisonError::into_inner) = status.clone();
        }
        (self.on_notice)(notice);
    }

    /// Build the plan again, watch what is new, stop watching what is gone.
    fn replan(&mut self) {
        let captures = (self.config.captures)();
        let excluded: Vec<PathBuf> = self
            .config
            .excluded
            .iter()
            .map(|dir| match plan::resolve_dir(dir) {
                Resolve::Dir(normal) => normal,
                _ => dir.clone(),
            })
            .collect();
        let mut plan = plan::build(
            &self.watch,
            &captures,
            &self.config.home,
            &excluded,
            &mut |dir| plan::resolve_dir(dir),
        );
        let wanted = plan.watch_dirs();
        for gone in self.watched.iter().filter(|d| !wanted.contains(d)) {
            let _ = self.watcher.unwatch(gone);
        }
        let mut now_watched = Vec::new();
        for dir in wanted {
            if self.watched.contains(&dir) {
                now_watched.push(dir);
            } else {
                crate::wdiag!("watch {:?}", dir);
                match self.watcher.watch(&dir, RecursiveMode::NonRecursive) {
                    Ok(()) => now_watched.push(dir),
                    Err(e) => plan.mark_failed(&dir, &e.to_string()),
                }
            }
        }
        crate::wdiag!("watched {:?}", now_watched);
        self.watched = now_watched;
        let status = plan.status.clone();
        self.engine.set_plan(plan);
        let changed = *self.status.lock().unwrap_or_else(PoisonError::into_inner) != status;
        if changed {
            self.forward(Notice::Status(status));
        }
    }
}
