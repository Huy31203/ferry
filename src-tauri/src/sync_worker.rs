//! Settings sync, the part with IO (stream G). One worker thread keeps this
//! computer's settings in step with a folder the user already syncs: it writes
//! one file per device into `<folder>/Ferry Settings/` and merges the other
//! devices' files key by key (`ferry_core::settings_sync`). It polls, because
//! file-system events miss many cloud-provider and network-share updates.
//! The ledger is owned by the worker thread alone and is never held across
//! `prefs::update`.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::fs;
use std::hash::{BuildHasher, RandomState};
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::mpsc::{self, RecvTimeoutError, Sender};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use ferry_core::fsutil;
use ferry_core::settings::{self, Settings};
use ferry_core::settings_sync::{
    self as sync, DeviceFile, Entry, FolderInfo, JoinMode, Ledger, MAX_FILE_BYTES, ReadError,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::state::AppState;
use crate::updater::one_line;
use crate::{jobs, prefs, windows};

pub const POLL_EVERY: Duration = Duration::from_secs(15);
/// A burst of local changes is written once.
pub const SETTLE: Duration = Duration::from_secs(2);
const OS: &str = std::env::consts::OS;
const LEDGER_FILE: &str = "sync-state.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum SyncStatus {
    Off,
    Starting,
    Synced { at: i64, devices: usize },
    FolderMissing,
    NewerFormat { device: String },
    Failed { reason: String },
}

enum Msg {
    LocalChanged,
    Reconfigure,
    SyncNow,
}

struct SyncWorker {
    tx: Mutex<Sender<Msg>>,
    status: Mutex<SyncStatus>,
}

/// Manage the worker's handle and spawn its thread.
pub fn start(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    app.manage(SyncWorker {
        tx: Mutex::new(tx),
        status: Mutex::new(SyncStatus::Starting),
    });
    let app = app.clone();
    std::thread::spawn(move || run(&app, rx));
}

fn send(app: &AppHandle, msg: Msg) {
    if let Some(worker) = app.try_state::<SyncWorker>() {
        let _ = worker.tx.lock().unwrap().send(msg);
    }
}

/// A cheap signal that the settings changed here; a no-op before `start`.
pub fn local_changed(app: &AppHandle) {
    send(app, Msg::LocalChanged);
}

/// The sync settings changed: start, stop or move to another folder.
pub fn reconfigure(app: &AppHandle) {
    send(app, Msg::Reconfigure);
}

pub fn sync_now(app: &AppHandle) {
    send(app, Msg::SyncNow);
}

pub fn status(app: &AppHandle) -> SyncStatus {
    app.try_state::<SyncWorker>()
        .map_or(SyncStatus::Starting, |worker| {
            worker.status.lock().unwrap().clone()
        })
}

fn set_status(app: &AppHandle, next: SyncStatus) {
    if let Some(worker) = app.try_state::<SyncWorker>() {
        *worker.status.lock().unwrap() = next.clone();
    }
    let _ = app.emit_to(windows::SETTINGS, "svc:sync", next);
}

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as i64)
}

/// A name for this computer, shown on the others. Plain `cfg!` branches, so the
/// Windows one is type-checked on every OS.
fn device_name() -> String {
    let (name, fallback) = if cfg!(target_os = "macos") {
        let name = std::process::Command::new("scutil")
            .args(["--get", "ComputerName"])
            .output()
            .ok()
            .and_then(|out| String::from_utf8(out.stdout).ok());
        (name, "Mac")
    } else if cfg!(windows) {
        (std::env::var("COMPUTERNAME").ok(), "Windows PC")
    } else {
        (None, "Computer")
    };
    name.map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| fallback.into())
}

fn new_id() -> String {
    sync::new_device_id(RandomState::new().hash_one((SystemTime::now(), std::process::id())))
}

fn ledger_path(app: &AppHandle) -> PathBuf {
    app.state::<AppState>()
        .settings_path
        .with_file_name(LEDGER_FILE)
}

fn settings_of(app: &AppHandle) -> Settings {
    app.state::<AppState>().settings.read().unwrap().clone()
}

fn home(app: &AppHandle) -> PathBuf {
    app.path().home_dir().unwrap_or_default()
}

/// Write a fresh ledger for joining the folder: `UseFolder` lets every value in
/// the folder win, `UseThis` lets this computer's win. The stored device id is
/// kept while the stored host still matches this computer.
pub fn prepare_join(app: &AppHandle, mode: JoinMode) -> Result<(), String> {
    let host = device_name();
    let device = load_ledger(&ledger_path(app))
        .filter(|ledger| ledger.host == host)
        .map_or_else(new_id, |ledger| ledger.device);
    let ledger = Ledger::join(&settings_of(app), OS, mode, &device, &host, now_ms());
    save_ledger(&ledger_path(app), &ledger).map_err(|e| e.to_string())
}

/// What is in `folder` (as the user chose it): the other devices' files.
pub fn inspect(folder: &Path, own_device: Option<&str>) -> FolderInfo {
    let read = read_folder(&sync::sync_dir(folder), &mut Cache::default());
    let files: Vec<DeviceFile> = read.files.into_iter().map(|(_, file)| file).collect();
    FolderInfo {
        exists: folder.is_dir(),
        devices: sync::summarise(&files, own_device.unwrap_or_default()),
        newer_format: read.newer.is_some(),
    }
}

/// The device the ledger on disk belongs to, if there is one.
pub fn own_device(app: &AppHandle) -> Option<String> {
    load_ledger(&ledger_path(app)).map(|ledger| ledger.device)
}

/// A file read once: its length, its modified time, what it parsed to.
#[derive(Default)]
pub(crate) struct Cache(HashMap<String, (u64, Option<SystemTime>, DeviceFile)>);

pub(crate) struct FolderRead {
    pub files: Vec<(PathBuf, DeviceFile)>,
    /// The device (or file) of a file written by a newer Ferry.
    pub newer: Option<String>,
    pub unreadable: usize,
}

/// Read every device file in `dir`. Hidden files, temp files, other types and
/// anything over 1 MiB are skipped. A file that does not parse (half
/// downloaded, a cloud placeholder, hand-edited into junk) keeps its last good
/// copy, or is skipped until a later poll reads it whole.
pub(crate) fn read_folder(dir: &Path, cache: &mut Cache) -> FolderRead {
    let mut out = FolderRead {
        files: Vec::new(),
        newer: None,
        unreadable: 0,
    };
    let Ok(entries) = fs::read_dir(dir) else {
        return out;
    };
    let mut seen = HashSet::new();
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with('.') || name.ends_with(".ferry-tmp") || !name.ends_with(".json") {
            continue;
        }
        let Ok(meta) = entry.metadata() else {
            continue;
        };
        if !meta.is_file() {
            continue;
        }
        if meta.len() > MAX_FILE_BYTES {
            out.unreadable += 1;
            continue;
        }
        seen.insert(name.clone());
        let stamp = (meta.len(), meta.modified().ok());
        if let Some((len, modified, file)) = cache.0.get(&name)
            && (*len, *modified) == stamp
        {
            out.files.push((entry.path(), file.clone()));
            continue;
        }
        let parsed = fs::read(entry.path())
            .map_err(|_| ReadError::Corrupt)
            .and_then(|bytes| sync::parse(&bytes));
        match parsed {
            Ok(file) => {
                cache.0.insert(name, (stamp.0, stamp.1, file.clone()));
                out.files.push((entry.path(), file));
            }
            Err(ReadError::NewerFormat { device_name, .. }) => {
                let stem = name.trim_end_matches(".json").to_string();
                out.newer = Some(device_name.unwrap_or(stem));
            }
            Err(ReadError::Corrupt | ReadError::TooLarge) => match cache.0.get(&name) {
                Some((_, _, last_good)) => out.files.push((entry.path(), last_good.clone())),
                None => out.unreadable += 1,
            },
            Err(ReadError::NotFerry) => out.unreadable += 1,
        }
    }
    cache.0.retain(|name, _| seen.contains(name));
    out
}

/// Replace this device's file atomically: a reader sees the old file or the new one.
pub(crate) fn write_own(dir: &Path, file: &DeviceFile) -> io::Result<()> {
    let bytes = serde_json::to_vec_pretty(file).map_err(io::Error::other)?;
    fsutil::atomic_replace(&dir.join(sync::file_name(&file.device)), &bytes)
}

/// Files that are cloud clients' copies of a device file (named something else than `<device>.json`).
pub(crate) fn conflict_copies(files: &[(PathBuf, DeviceFile)]) -> Vec<PathBuf> {
    files
        .iter()
        .filter(|(path, file)| {
            let name = path.file_name().unwrap_or_default().to_string_lossy();
            sync::is_conflict_copy(&name, file)
        })
        .map(|(path, _)| path.clone())
        .collect()
}

pub(crate) fn load_ledger(path: &Path) -> Option<Ledger> {
    serde_json::from_slice(&fs::read(path).ok()?).ok()
}

pub(crate) fn save_ledger(path: &Path, ledger: &Ledger) -> io::Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let bytes = serde_json::to_vec_pretty(ledger).map_err(io::Error::other)?;
    fsutil::atomic_replace(path, &bytes)
}

/// What only the worker thread keeps between cycles.
struct Worker {
    host: String,
    ledger: Option<Ledger>,
    cache: Cache,
    /// The entries of the file this device last wrote.
    written: Option<BTreeMap<String, Entry>>,
}

fn run(app: &AppHandle, rx: mpsc::Receiver<Msg>) {
    let mut worker = Worker {
        host: device_name(),
        ledger: None,
        cache: Cache::default(),
        written: None,
    };
    cycle(app, &mut worker);
    let mut settle: Option<Instant> = None;
    loop {
        let wait = settle.map_or(POLL_EVERY, |at| {
            at.saturating_duration_since(Instant::now())
        });
        match rx.recv_timeout(wait) {
            Ok(Msg::LocalChanged) => {
                settle = Some(Instant::now() + SETTLE);
                continue;
            }
            Ok(Msg::Reconfigure) => {
                // The ledger on disk may be a fresh one from `prepare_join`.
                worker.ledger = None;
                worker.written = None;
            }
            Ok(Msg::SyncNow) | Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        settle = None;
        cycle(app, &mut worker);
    }
}

fn cycle(app: &AppHandle, worker: &mut Worker) {
    match sync_once(app, worker) {
        Ok(status) => set_status(app, status),
        Err(reason) => {
            warn!(%reason, "settings sync failed");
            set_status(app, SyncStatus::Failed { reason });
        }
    }
}

fn sync_once(app: &AppHandle, worker: &mut Worker) -> Result<SyncStatus, String> {
    let settings = settings_of(app);
    let sync_settings = &settings.svc.sync;
    if !sync_settings.enabled {
        worker.ledger = None;
        worker.written = None;
        return Ok(SyncStatus::Off);
    }
    let chosen = settings::expand_home(&sync_settings.folder, &home(app));
    if !chosen.is_dir() {
        return Ok(SyncStatus::FolderMissing);
    }
    let dir = sync::sync_dir(&chosen);
    fs::create_dir_all(&dir).map_err(|e| one_line(&e.to_string()))?;

    let path = ledger_path(app);
    let mut ledger = match worker.ledger.take() {
        Some(ledger) => ledger,
        None => match load_ledger(&path).filter(|ledger| ledger.host == worker.host) {
            Some(ledger) => ledger,
            None => {
                info!("starting settings sync from the folder's values");
                Ledger::join(
                    &settings,
                    OS,
                    JoinMode::UseFolder,
                    &new_id(),
                    &worker.host,
                    now_ms(),
                )
            }
        },
    };
    ledger.record_local(&settings, OS, now_ms());

    let read = read_folder(&dir, &mut worker.cache);
    let own_file = sync::file_name(&ledger.device);
    let remotes: Vec<DeviceFile> = read
        .files
        .iter()
        .filter(|(path, _)| {
            path.file_name()
                .is_none_or(|name| name != own_file.as_str())
        })
        .map(|(_, file)| file.clone())
        .collect();
    let adopted = ledger.merge(&remotes, OS);
    if !adopted.is_empty() {
        info!(
            keys = adopted.len(),
            "adopting settings from other computers"
        );
        let mut kept = Vec::new();
        let saved = prefs::update(app, |s| {
            let (next, skipped) = sync::apply(s, &adopted, OS);
            *s = next;
            kept = skipped;
        });
        match saved {
            Ok(now) => {
                ledger.revert(&kept, &adopted);
                // Sanitising, or a change made meanwhile, may differ from what was adopted.
                ledger.record_local(&now, OS, now_ms());
            }
            Err(e) => {
                warn!("could not save the merged settings: {e}");
                let all: Vec<String> = adopted
                    .iter()
                    .filter_map(|a| sync::settings_path_of(&a.key, OS))
                    .collect();
                ledger.revert(&all, &adopted);
            }
        }
    }

    let file = ledger.device_file(&worker.host, OS, env!("CARGO_PKG_VERSION"), now_ms());
    if worker.written.as_ref() != Some(&file.entries) || !dir.join(&own_file).exists() {
        write_own(&dir, &file).map_err(|e| one_line(&e.to_string()))?;
        worker.written = Some(file.entries.clone());
    }
    for copy in conflict_copies(&read.files) {
        // Their keys were merged above, so the copy is no longer needed.
        if let Err(e) = jobs::move_to_trash(&copy) {
            warn!(path = %copy.display(), "could not trash a sync conflict copy: {e}");
        }
    }
    save_ledger(&path, &ledger).map_err(|e| one_line(&e.to_string()))?;
    let devices: HashSet<&str> = read
        .files
        .iter()
        .map(|(_, file)| file.device.as_str())
        .filter(|device| *device != ledger.device)
        .collect();
    let devices = devices.len();
    worker.ledger = Some(ledger);
    Ok(match read.newer {
        Some(device) => SyncStatus::NewerFormat { device },
        None => SyncStatus::Synced {
            at: now_ms(),
            devices,
        },
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::settings_sync::JoinMode;

    fn file(device: &str, compression: i64) -> DeviceFile {
        let mut settings = Settings::default();
        settings.images.compression = compression as u8;
        let mut file = Ledger::join(&settings, "macos", JoinMode::UseThis, device, "host", 100)
            .device_file(&format!("{device} name"), "macos", "0.3.0", 100);
        file.written_at = compression;
        file
    }

    fn put(dir: &Path, name: &str, file: &DeviceFile) {
        fs::write(dir.join(name), serde_json::to_vec_pretty(file).unwrap()).unwrap();
    }

    #[test]
    fn read_folder_skips_hidden_temp_and_oversized_files() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "d1.json", &file("d1", 70));
        put(dir.path(), ".d2.json", &file("d2", 70));
        put(dir.path(), "d3.json.ferry-tmp", &file("d3", 70));
        fs::write(dir.path().join("notes.txt"), "hi").unwrap();
        fs::write(dir.path().join("d4.json"), vec![b' '; 2 * 1024 * 1024]).unwrap();
        let read = read_folder(dir.path(), &mut Cache::default());
        assert_eq!(read.files.len(), 1);
        assert_eq!(read.files[0].1.device, "d1");
        assert_eq!(read.unreadable, 1);
    }

    #[test]
    fn a_half_written_file_keeps_the_last_good_copy() {
        let dir = tempfile::tempdir().unwrap();
        let good = file("d1", 70);
        put(dir.path(), "d1.json", &good);
        let mut cache = Cache::default();
        assert_eq!(read_folder(dir.path(), &mut cache).files.len(), 1);

        let bytes = fs::read(dir.path().join("d1.json")).unwrap();
        fs::write(dir.path().join("d1.json"), &bytes[..20]).unwrap();
        let again = read_folder(dir.path(), &mut cache);
        assert_eq!(again.files.len(), 1);
        assert_eq!(again.files[0].1.entries, good.entries);

        let fresh = read_folder(dir.path(), &mut Cache::default());
        assert!(fresh.files.is_empty());
        assert_eq!(fresh.unreadable, 1);
    }

    #[test]
    fn a_newer_format_is_named() {
        let dir = tempfile::tempdir().unwrap();
        let mut newer = file("d1", 70);
        newer.format_version = 2;
        newer.device_name = "PC".into();
        put(dir.path(), "d1.json", &newer);
        let read = read_folder(dir.path(), &mut Cache::default());
        assert_eq!(read.newer.as_deref(), Some("PC"));
        assert!(read.files.is_empty());
    }

    #[test]
    fn write_own_replaces_atomically() {
        let dir = tempfile::tempdir().unwrap();
        write_own(dir.path(), &file("d1", 70)).unwrap();
        let second = file("d1", 60);
        write_own(dir.path(), &second).unwrap();
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, ["d1.json"]);
        let back = sync::parse(&fs::read(dir.path().join("d1.json")).unwrap()).unwrap();
        assert_eq!(back, second);
    }

    #[test]
    fn conflict_copies_are_picked_out() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "d1.json", &file("d1", 70));
        put(dir.path(), "d1 (conflicted copy).json", &file("d1", 60));
        let read = read_folder(dir.path(), &mut Cache::default());
        assert_eq!(read.files.len(), 2);
        let copies = conflict_copies(&read.files);
        assert_eq!(copies.len(), 1);
        assert!(copies[0].to_string_lossy().contains("conflicted copy"));
    }

    #[test]
    fn the_ledger_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("sync-state.json");
        let ledger = Ledger::join(
            &Settings::default(),
            "macos",
            JoinMode::UseThis,
            "d1",
            "host",
            5,
        );
        save_ledger(&path, &ledger).unwrap();
        assert_eq!(load_ledger(&path), Some(ledger));
        fs::write(&path, "{ not json").unwrap();
        assert_eq!(load_ledger(&path), None);
    }

    #[test]
    fn inspect_lists_other_devices() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("Ferry Settings");
        fs::create_dir(&sub).unwrap();
        put(&sub, "d1.json", &file("d1", 70));
        put(&sub, "d2.json", &file("d2", 60));
        put(&sub, "dme.json", &file("dme", 50));
        let info = inspect(dir.path(), Some("dme"));
        assert!(info.exists);
        let mut names: Vec<String> = info.devices.iter().map(|d| d.device.clone()).collect();
        names.sort();
        assert_eq!(names, ["d1", "d2"]);

        let missing = inspect(&dir.path().join("nowhere"), None);
        assert!(!missing.exists);
        assert!(missing.devices.is_empty());
    }
}
