//! The built-in shelf (shelf spec): a panel on the bottom edge that holds files
//! by reference. macOS only; `init` does nothing elsewhere.

pub mod commands;
pub mod menu;
pub mod window;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ferry_core::i18n::{t, tf};
use ferry_core::shelf::{LoadStatus, Shelf};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use ferry_core::job::{JobId, JobState, JobUpdate};

use crate::jobs;
use crate::state::AppState;

pub const SHELF: &str = "shelf";

#[derive(Debug, Default, Clone, Copy)]
pub struct Layout {
    /// The page's last reported height in points (0 = hidden).
    pub height_pt: f64,
}

pub struct ShelfState {
    pub list: Mutex<Shelf>,
    pub path: PathBuf,
    /// The pile is being dragged out (spec §3 "Shelf drag flag").
    pub dragging: AtomicBool,
    pub layout: Mutex<Layout>,
    /// "Couldn't save the shelf" shows once per session.
    warned: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub is_dir: bool,
    /// Bytes for a file, 0 for a folder.
    pub size: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShelfView {
    pub items: Vec<Entry>,
    pub size: u64,
    pub can_undo: bool,
    /// `floating.shelf_opacity`, percent.
    pub opacity: u32,
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let path = app.path().app_data_dir()?.join("shelf.json");
    let (mut list, status) = Shelf::load(&path);
    list.prune();
    app.manage(ShelfState {
        list: Mutex::new(list),
        path,
        dragging: AtomicBool::new(false),
        layout: Mutex::new(Layout::default()),
        warned: AtomicBool::new(false),
    });
    if let LoadStatus::Recovered { .. } = status {
        jobs::notice(app, t("shelf.error.reset"));
    }
    window::create(app)?;
    menu::init(app);
    Ok(())
}

pub fn state(app: &AppHandle) -> Option<tauri::State<'_, ShelfState>> {
    app.try_state::<ShelfState>()
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

pub fn view(app: &AppHandle) -> ShelfView {
    let Some(state) = state(app) else {
        return ShelfView {
            items: vec![],
            size: 0,
            can_undo: false,
            opacity: 95,
        };
    };
    let list = state.list.lock().unwrap();
    let items = list
        .items
        .iter()
        .map(|i| Entry {
            path: i.path.to_string_lossy().into_owned(),
            name: i
                .path
                .file_name()
                .map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            ext: i
                .path
                .extension()
                .map_or_else(String::new, |e| e.to_string_lossy().to_lowercase()),
            is_dir: i.path.is_dir(),
            size: std::fs::metadata(&i.path)
                .ok()
                .filter(|m| m.is_file())
                .map_or(0, |m| m.len()),
        })
        .collect();
    let size = list
        .items
        .iter()
        .filter_map(|i| std::fs::metadata(&i.path).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    let opacity = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .shelf_opacity;
    ShelfView {
        items,
        size,
        can_undo: list.can_undo(),
        opacity,
    }
}

/// Change the list, save it, tell the page.
pub fn mutate<R>(app: &AppHandle, f: impl FnOnce(&mut Shelf) -> R) -> Option<R> {
    let state = state(app)?;
    let out = {
        let mut list = state.list.lock().unwrap();
        let out = f(&mut list);
        if list.save(&state.path).is_err() && !state.warned.swap(true, Ordering::SeqCst) {
            jobs::notice(app, t("shelf.error.save"));
        }
        out
    };
    let _ = app.emit_to(SHELF, "shelf:changed", view(app));
    Some(out)
}

pub fn add(app: &AppHandle, paths: Vec<PathBuf>) {
    let Some(added) = mutate(app, |list| list.add(&paths, now())) else {
        return;
    };
    if added.failed > 0 {
        let count = added.failed.to_string();
        jobs::notice(app, tf("shelf.error.add", &[("count", &count)]));
    }
}

/// "Show shelf" in the tray: the page opens the shelf for a few seconds.
pub fn summon(app: &AppHandle) {
    let _ = app.emit_to(SHELF, "shelf:summon", ());
}

/// The tray row, only where the shelf exists.
pub fn tray_item(app: &AppHandle) -> Option<tauri::menu::MenuItem<tauri::Wry>> {
    state(app)?;
    tauri::menu::MenuItem::with_id(app, "shelf:show", t("tray.shelf"), true, None::<&str>).ok()
}

/// What the shelf takes from the bottom of the stack's area, in physical pixels.
/// Only the expanded shelf pushes the stack: the resting strip is a drag-time
/// overlay and must not make cards hop at every drag start.
pub fn inset_px(app: &AppHandle, scale: f64) -> u32 {
    let Some(state) = state(app) else { return 0 };
    let height = state.layout.lock().unwrap().height_pt;
    let px = |pt: f64| (pt * scale).round() as u32;
    ferry_core::shelf_place::inset(false, px(height), px(ferry_core::shelf_place::STRIP))
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct DragView {
    pub offer: bool,
    pub near: bool,
    pub own: bool,
}

static DRAG: Mutex<DragView> = Mutex::new(DragView {
    offer: false,
    near: false,
    own: false,
});

fn emit_drag(app: &AppHandle, next: DragView) {
    let changed = {
        let mut cur = DRAG.lock().unwrap();
        let changed = *cur != next;
        *cur = next;
        changed
    };
    if changed {
        let _ = app.emit_to(SHELF, "shelf:drag", next);
    }
}

pub fn on_drag(app: &AppHandle, offer: bool, own: bool) {
    if offer {
        mutate(app, |list| list.prune());
    }
    emit_drag(
        app,
        DragView {
            offer,
            near: false,
            own,
        },
    );
}

pub fn drag_ended(app: &AppHandle) {
    emit_drag(app, DragView::default());
}

/// Each watcher tick during a drag: is the pointer near the strip?
pub fn track(app: &AppHandle) {
    let cur = *DRAG.lock().unwrap();
    if !cur.offer {
        return;
    }
    let near = near_strip(app).unwrap_or(false);
    emit_drag(app, DragView { near, ..cur });
}

fn near_strip(app: &AppHandle) -> Option<bool> {
    use ferry_core::shelf_place::{REACH, SIDE, STRIP, WIDTH, drag_near, shelf_rect};
    let monitor = window::monitor(app)?;
    let scale = monitor.scale_factor();
    let px = |pt: f64| (pt * scale).round() as u32;
    let area = window::area_of(&monitor);
    let corner = window::corner(app);
    let strip = shelf_rect(area, corner, (px(WIDTH), px(STRIP)), px(SIDE));
    let zone = crate::drop_batch::zone::base_rect(app);
    let cursor = crate::drop_batch::zone::cursor_px(app)?;
    Some(drag_near(cursor, strip, px(REACH), zone))
}

/// Pure: is the point (points) inside any of the windows `(x, y, w, h)` (points)?
pub fn cursor_over_ferry(cursor: (f64, f64), windows: &[(f64, f64, f64, f64)]) -> bool {
    windows
        .iter()
        .any(|&(x, y, w, h)| cursor.0 >= x && cursor.0 < x + w && cursor.1 >= y && cursor.1 < y + h)
}

/// The shelf's height in points while it is expanded (open or a drop target), else 0.
pub fn expanded_pt(app: &AppHandle) -> f64 {
    let Some(state) = state(app) else { return 0.0 };
    let height = state.layout.lock().unwrap().height_pt;
    if height > ferry_core::shelf_place::STRIP {
        height
    } else {
        0.0
    }
}

pub fn format_size(bytes: u64) -> String {
    // Replace with the project's byte formatter if one exists (see Task 7 Step 3).
    format!("{:.1} MB", bytes as f64 / 1e6)
}

static CLEARS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

/// "Clear shelf": empties the list, Undo for 5 s (spec §6). A later Clear restarts the 5 s.
pub fn clear(app: &AppHandle) {
    mutate(app, |list| list.clear());
    let generation = CLEARS.fetch_add(1, Ordering::SeqCst) + 1;
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        if CLEARS.load(Ordering::SeqCst) == generation {
            mutate(&app, |list| list.forget_undo());
        }
    });
}

static PENDING: std::sync::LazyLock<Mutex<std::collections::HashMap<JobId, PathBuf>>> =
    std::sync::LazyLock::new(|| Mutex::new(std::collections::HashMap::new()));

/// Files the optimiser can take; folders and other types stay on the shelf.
pub fn optimisable(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.is_file() && matches!(ferry_core::intake::classify_file(p), Ok(Some(_))))
        .cloned()
        .collect()
}

pub fn optimise_all(app: &AppHandle) {
    let paths = state(app)
        .map(|s| s.list.lock().unwrap().paths())
        .unwrap_or_default();
    let wanted = optimisable(&paths);
    if wanted.is_empty() {
        jobs::notice(app, t("shelf.error.nothing_to_optimise"));
        return;
    }
    let started = crate::drop_batch::intake::accept_paths(app, wanted, false);
    PENDING.lock().unwrap().extend(started);
}

/// A finished card from "Optimise all" puts its result where its source was.
pub fn on_job_update(app: &AppHandle, update: &JobUpdate) {
    let finished = matches!(
        update.state,
        JobState::Done | JobState::AlreadyOptimal | JobState::Larger | JobState::Error { .. }
    );
    if !finished {
        return;
    }
    let Some(source) = PENDING.lock().unwrap().remove(&update.id) else {
        return;
    };
    if let (JobState::Done, Some(output)) = (&update.state, &update.output) {
        let output = output.clone();
        mutate(app, |list| list.replace(&source, &output));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_over_any_ferry_window_counts_as_cancelled() {
        let windows = [(100.0, 100.0, 300.0, 200.0), (1286.0, 873.0, 150.0, 27.0)];
        assert!(cursor_over_ferry((150.0, 150.0), &windows));
        assert!(cursor_over_ferry((1300.0, 880.0), &windows));
        assert!(!cursor_over_ferry((700.0, 500.0), &windows));
    }

    /// A 1×1 PNG.
    const PNG: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44,
        0x52, 0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x00, 0x00, 0x1f,
        0x15, 0xc4, 0x89, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0x64,
        0x60, 0xf8, 0x5f, 0x0f, 0x00, 0x02, 0x87, 0x01, 0x80, 0xeb, 0x47, 0xba, 0x92, 0x00, 0x00,
        0x00, 0x00, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn only_supported_files_are_sent_to_optimise() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        std::fs::write(&png, PNG).unwrap();
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, b"hi").unwrap();
        let folder = dir.path().join("photos");
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(optimisable(&[png.clone(), txt, folder]), vec![png]);
    }
}
