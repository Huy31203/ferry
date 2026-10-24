//! Files that reach the running app from outside: Finder "Open With" and
//! Services (macOS), the Explorer verb and "Open with" (Windows), and a second
//! launch. They all start a normal file job, whatever the run state: an explicit
//! user action, like the optimise hotkey.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use ferry_core::i18n::tf;
use ferry_core::optimise::Level;
use ferry_core::pipeline::Job;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::{jobs, windows};

/// Paths that arrive before `setup` has managed the app state (AppKit can
/// deliver `application:openURLs:` that early) wait here.
static EARLY: Mutex<Early> = Mutex::new(Early::new());

struct Early {
    held: Option<Vec<PathBuf>>,
}

impl Early {
    const fn new() -> Self {
        Self {
            held: Some(Vec::new()),
        }
    }

    /// The paths to run now: none while holding.
    fn offer(&mut self, paths: Vec<PathBuf>) -> Vec<PathBuf> {
        match &mut self.held {
            Some(held) => {
                held.extend(paths);
                Vec::new()
            }
            None => paths,
        }
    }

    /// Stop holding and hand back what was held.
    fn open(&mut self) -> Vec<PathBuf> {
        self.held.take().unwrap_or_default()
    }
}

pub fn optimise_paths(app: &AppHandle, paths: Vec<PathBuf>) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let settings = state.settings.read().unwrap().clone();
        for path in paths {
            match Job::file(&path, &settings, state.gifsicle.is_some()) {
                Ok(job) => {
                    if !jobs::start(&app, job, Level::Normal, None) {
                        let name = path.file_name().unwrap_or_default().to_string_lossy();
                        jobs::notice(&app, tf("error.busy", &[("name", &name)]));
                    }
                }
                Err(reason) => jobs::notice(&app, reason),
            }
        }
    });
}

/// A second launch: its files, or Settings when it brought none.
pub fn second_instance(app: &AppHandle, args: Vec<String>, cwd: String) {
    let paths = ferry_core::integrations::launch_paths(&args, Path::new(&cwd));
    if paths.is_empty() {
        windows::open_settings(app);
    } else {
        offer(app, paths);
    }
}

/// Finder "Open With" and files dropped on the Dock icon.
#[cfg(target_os = "macos")]
pub fn opened(app: &AppHandle, urls: Vec<tauri::Url>) {
    let paths: Vec<PathBuf> = urls
        .iter()
        .filter(|url| url.scheme() == "file")
        .filter_map(|url| url.to_file_path().ok())
        .collect();
    if !paths.is_empty() {
        offer(app, paths);
    }
}

/// Setup is done: optimise the launch files, then whatever arrived before.
pub fn ready(app: &AppHandle, initial: Vec<PathBuf>) {
    let held = EARLY.lock().unwrap().open();
    let paths: Vec<PathBuf> = initial.into_iter().chain(held).collect();
    if !paths.is_empty() {
        optimise_paths(app, paths);
    }
}

fn offer(app: &AppHandle, paths: Vec<PathBuf>) {
    let now = EARLY.lock().unwrap().offer(paths);
    if !now.is_empty() {
        optimise_paths(app, now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_paths_wait_for_setup() {
        let mut early = Early::new();
        assert!(early.offer(vec!["/a.png".into()]).is_empty());
        assert!(early.offer(vec!["/b.png".into()]).is_empty());
        assert_eq!(
            early.open(),
            vec![PathBuf::from("/a.png"), PathBuf::from("/b.png")]
        );
        assert_eq!(
            early.offer(vec!["/c.png".into()]),
            vec![PathBuf::from("/c.png")]
        );
        assert!(early.open().is_empty());
    }
}
