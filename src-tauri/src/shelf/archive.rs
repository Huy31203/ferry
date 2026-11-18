//! "Add to archive" (shelf archive spec): one archive at a time, progress to the
//! shelf page, the result swapped into the shelf per `floating.shelf_archive_result`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use ferry_core::archive::{self, Encryption, Options, Rates};
use ferry_core::cancel::CancelToken;
use ferry_core::i18n::{t, tf};
use ferry_core::settings::ArchiveResult;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{SHELF, mutate, state};
use crate::jobs;
use crate::state::AppState;

static RUNNING: Mutex<Option<CancelToken>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Fast,
    Balanced,
    Smallest,
}

impl Preset {
    pub fn level(self) -> u32 {
        match self {
            Preset::Fast => 1,
            Preset::Balanced => 5,
            Preset::Smallest => 9,
        }
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    progress: Option<f32>,
}

pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get())
}

pub fn busy() -> bool {
    RUNNING.lock().unwrap().is_some()
}

pub fn cancel() {
    if let Some(token) = RUNNING.lock().unwrap().as_ref() {
        token.cancel();
    }
}

pub fn eta_label(secs: f64) -> String {
    if secs < 1.0 {
        t("shelf.archive.eta_now")
    } else if secs < 90.0 {
        tf(
            "shelf.archive.eta_secs",
            &[("n", &(secs.round() as u64).to_string())],
        )
    } else {
        tf(
            "shelf.archive.eta_mins",
            &[("n", &((secs / 60.0).round() as u64).to_string())],
        )
    }
}

fn rates_path(app: &AppHandle) -> Option<PathBuf> {
    app.path()
        .app_data_dir()
        .ok()
        .map(|d| d.join("archive_rates.json"))
}

#[derive(Debug, Clone, Copy)]
pub struct Totals {
    pub bytes: u64,
    pub files: usize,
}

/// The last walk's totals, keyed by the items walked.
static TOTALS: Mutex<Option<(Vec<PathBuf>, Totals)>> = Mutex::new(None);
static WALKING: AtomicBool = AtomicBool::new(false);

/// Walk `items` now and remember the result. Seconds for a big project folder.
pub fn walk(items: &[PathBuf]) -> Totals {
    let entries = archive::collect(items).unwrap_or_default();
    let totals = Totals {
        bytes: entries.iter().map(|e| e.size).sum(),
        files: entries.iter().filter(|e| !e.is_dir()).count(),
    };
    *TOTALS.lock().unwrap() = Some((items.to_vec(), totals));
    totals
}

/// The last walk of `items`, never waiting: a fresh walk starts in the
/// background so the next ask sees changes inside the folders.
pub fn cached(items: &[PathBuf]) -> Option<Totals> {
    if !WALKING.swap(true, Ordering::SeqCst) {
        let owned = items.to_vec();
        std::thread::spawn(move || {
            walk(&owned);
            WALKING.store(false, Ordering::SeqCst);
        });
    }
    TOTALS
        .lock()
        .unwrap()
        .as_ref()
        .filter(|(key, _)| key == items)
        .map(|(_, t)| *t)
}

/// Seconds to archive `totals` at `level` on `threads` (spec §5).
pub fn estimate(app: &AppHandle, totals: Totals, level: u32, threads: usize) -> f64 {
    let rates = rates_path(app).map(|p| Rates::load(&p)).unwrap_or_default();
    rates.estimate(totals.bytes, level, threads.min(totals.files).max(1))
}

pub(super) fn items(app: &AppHandle) -> Vec<PathBuf> {
    state(app)
        .map(|s| s.list.lock().unwrap().paths())
        .unwrap_or_default()
}

/// The quick menu items: Finder-style name in the first item's folder (spec §2).
pub fn quick(app: &AppHandle, preset: Preset) {
    let items = items(app);
    let Some(folder) = items
        .first()
        .and_then(|p| p.parent())
        .map(Path::to_path_buf)
    else {
        return;
    };
    let out = archive::free_path(&folder, &archive::default_name(&items));
    let opts = Options {
        level: preset.level(),
        threads: cores(),
        password: None,
        encryption: Encryption::Aes256,
        split: None,
    };
    if let Err(e) = start(app, items, out, opts) {
        jobs::notice(app, e);
    }
}

pub fn start(
    app: &AppHandle,
    items: Vec<PathBuf>,
    out: PathBuf,
    opts: Options,
) -> Result<(), String> {
    let token = {
        let mut running = RUNNING.lock().unwrap();
        if running.is_some() {
            return Err(t("shelf.archive.busy"));
        }
        let token = CancelToken::new();
        *running = Some(token.clone());
        token
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let began = Instant::now();
        let result = run(&app, &items, &out, &opts, &token);
        *RUNNING.lock().unwrap() = None;
        let _ = app.emit_to(SHELF, "shelf:archive", Progress { progress: None });
        match result {
            Ok((made, bytes, workers)) => {
                if let Some(path) = rates_path(&app) {
                    let mut rates = Rates::load(&path);
                    rates.record(bytes, opts.level, workers, began.elapsed().as_secs_f64());
                    let _ = rates.save(&path);
                }
                finish(&app, &items, made);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => jobs::notice(
                &app,
                tf("shelf.archive.failed", &[("reason", &e.to_string())]),
            ),
        }
    });
    Ok(())
}

fn run(
    app: &AppHandle,
    items: &[PathBuf],
    out: &Path,
    opts: &Options,
    token: &CancelToken,
) -> std::io::Result<(Vec<PathBuf>, u64, usize)> {
    let entries = archive::collect(items)?;
    let total: u64 = entries.iter().map(|e| e.size).sum::<u64>().max(1);
    let files = entries.iter().filter(|e| !e.is_dir()).count();
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let progress = |done: u64| {
        let mut last = last.lock().unwrap();
        if last.elapsed() >= Duration::from_millis(200) {
            *last = Instant::now();
            let p = (done as f64 / total as f64).min(1.0) as f32;
            let _ = app.emit_to(SHELF, "shelf:archive", Progress { progress: Some(p) });
        }
    };
    let made = archive::write(&entries, out, opts, token, &progress)?;
    Ok((made, total, opts.threads.min(files).max(1)))
}

/// Spec §6: replace takes the items off the shelf (never off the disk), add keeps them.
fn finish(app: &AppHandle, items: &[PathBuf], made: Vec<PathBuf>) {
    let mode = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .shelf_archive_result;
    if mode == ArchiveResult::Replace
        && let Some(s) = state(app)
    {
        let mut cards = s.cards.lock().unwrap();
        for item in items {
            cards.remove(item);
        }
    }
    mutate(app, |list| {
        if mode == ArchiveResult::Replace {
            list.remove(items);
        }
        list.add(&made, super::now());
    });
    super::summon(app);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_labels_read_like_the_spec() {
        assert_eq!(eta_label(0.4), "< 1 s");
        assert_eq!(eta_label(3.2), "~3 s");
        assert_eq!(eta_label(89.0), "~89 s");
        assert_eq!(eta_label(150.0), "~3 min");
    }

    #[test]
    fn presets_map_to_levels() {
        assert_eq!(
            [Preset::Fast, Preset::Balanced, Preset::Smallest].map(Preset::level),
            [1, 5, 9]
        );
    }

    #[test]
    fn the_menu_never_walks_a_folder_itself_and_answers_from_the_last_walk() {
        let dir = std::env::temp_dir().join(format!("ferry-totals-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.bin"), [0u8; 1000]).unwrap();
        let items = vec![dir.clone()];
        assert!(
            cached(&items).is_none(),
            "the first ask starts a walk, it doesn't wait for one"
        );
        let deadline = Instant::now() + Duration::from_secs(10);
        let got = loop {
            if let Some(t) = cached(&items) {
                break t;
            }
            assert!(
                Instant::now() < deadline,
                "the background walk never finished"
            );
            std::thread::sleep(Duration::from_millis(20));
        };
        assert_eq!((got.bytes, got.files), (1000, 1));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
