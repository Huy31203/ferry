//! The built-in shelf (shelf spec): a panel on the bottom edge that holds files
//! by reference. macOS only; `init` does nothing elsewhere.

pub mod commands;
pub mod window;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ferry_core::geometry::Rect;
use ferry_core::i18n::{t, tf};
use ferry_core::shelf::{LoadStatus, Shelf};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::jobs;
use crate::state::AppState;

pub const SHELF: &str = "shelf";

#[derive(Debug, Default, Clone, Copy)]
pub struct Layout {
    /// The page's last reported height in points (0 = hidden).
    pub height_pt: f64,
    /// The window's physical rect while shown.
    // Read by the drag tracking in Task 6; remove the expect then.
    #[expect(dead_code)]
    pub rect: Option<Rect>,
}

pub struct ShelfState {
    pub list: Mutex<Shelf>,
    pub path: PathBuf,
    /// The pile is being dragged out (spec §3 "Shelf drag flag").
    pub dragging: AtomicBool,
    /// A drag the shelf takes is in progress (set by the watcher).
    pub offering: AtomicBool,
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
}

#[derive(Debug, Clone, Serialize)]
pub struct ShelfView {
    pub items: Vec<Entry>,
    pub size: u64,
    pub can_undo: bool,
    /// `floating.shelf_transparency`, percent.
    pub transparency: u32,
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
        offering: AtomicBool::new(false),
        layout: Mutex::new(Layout::default()),
        warned: AtomicBool::new(false),
    });
    if let LoadStatus::Recovered { .. } = status {
        jobs::notice(app, t("shelf.error.reset"));
    }
    window::create(app)?;
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
            transparency: 80,
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
        })
        .collect();
    let size = list
        .items
        .iter()
        .filter_map(|i| std::fs::metadata(&i.path).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    let transparency = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .shelf_transparency;
    ShelfView {
        items,
        size,
        can_undo: list.can_undo(),
        transparency,
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

pub fn offering(app: &AppHandle) -> bool {
    state(app).is_some_and(|s| s.offering.load(Ordering::SeqCst))
}
