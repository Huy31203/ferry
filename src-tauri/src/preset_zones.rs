//! Preset zones (stream F): a grid of drop targets that appears on a Ctrl tap
//! (stream D's `drag:tap`) or from the zone editor's preview. Each tile runs
//! its pipeline for the media dropped on it. The window is built at the first
//! show, off the main thread, and hides itself after a drag ends.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Duration;

use ferry_core::i18n::t;
use ferry_core::pipeline_def::PerMedia;
use ferry_core::pipeline_plan::caption;
use ferry_core::pipeline_steps::{Media, registry};
use serde::{Deserialize, Serialize};
use tauri::menu::{Menu, MenuItem};
use tauri::{
    AppHandle, Emitter, Listener, LogicalPosition, Manager, WebviewUrl, WebviewWindow,
    WebviewWindowBuilder,
};
use tracing::warn;

use crate::state::AppState;
use crate::{pipe_windows, platform, prefs};

pub const ZONES: &str = "zones";
pub const TILE: (f64, f64) = (148.0, 100.0);
pub const GAP: f64 = 8.0;
pub const PAD: f64 = 12.0;
pub const COLUMNS: usize = 4;
/// Kept between the grid and the work area's edge, logical px.
const MARGIN: f64 = 16.0;
/// A drag-mode grid hides itself after this long if no drag enters it.
const NO_ENTER_MS: u64 = 5000;
/// And this long after a drag leaves it.
const AFTER_LEAVE_MS: u64 = 1500;
/// A zone drop hides the grid after this long.
pub const DROPPED_MS: u64 = 300;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    /// Shown for a drag or a Ctrl tap: hides itself.
    Drag,
    /// Shown from the zone editor: stays until Done.
    Preview,
}

/// Logical size for `tiles` tiles: columns = min(4, tiles), rows = ceil(tiles / columns).
pub fn zones_size(tiles: usize) -> (f64, f64) {
    let tiles = tiles.max(1);
    let columns = tiles.min(COLUMNS);
    let rows = tiles.div_ceil(columns);
    (
        2.0 * PAD + columns as f64 * TILE.0 + (columns - 1) as f64 * GAP,
        2.0 * PAD + rows as f64 * TILE.1 + (rows - 1) as f64 * GAP,
    )
}

/// Top-left that centres `size` on `cursor`, kept 16 px inside `work` (x, y, width, height; all logical).
pub fn zones_origin(
    cursor: (f64, f64),
    size: (f64, f64),
    work: (f64, f64, f64, f64),
) -> (f64, f64) {
    let place = |at: f64, extent: f64, start: f64, room: f64| {
        let low = start + MARGIN;
        let high = (start + room - extent - MARGIN).max(low);
        (at - extent / 2.0).clamp(low, high)
    };
    (
        place(cursor.0, size.0, work.0, work.2),
        place(cursor.1, size.1, work.1, work.3),
    )
}

#[derive(Deserialize)]
pub struct Tap {
    pub modifier: String,
}

/// Whether a `drag:tap` toggles the zones: only the preset modifier.
pub fn is_preset_tap(tap: &Tap) -> bool {
    tap.modifier == "control"
}

struct Ctl {
    visible: bool,
    mode: Mode,
    /// Bumped by every show, hide and drag report, so a delayed hide can tell it is stale.
    generation: u64,
    entered: bool,
    dragged: Vec<Media>,
}

pub struct ZonesState {
    ctl: Mutex<Ctl>,
    /// Held while the window is built, so two shows build it once.
    build: Mutex<()>,
}

impl Default for ZonesState {
    fn default() -> Self {
        Self {
            ctl: Mutex::new(Ctl {
                visible: false,
                mode: Mode::Drag,
                generation: 0,
                entered: false,
                dragged: Vec::new(),
            }),
            build: Mutex::new(()),
        }
    }
}

fn with_ctl<R>(app: &AppHandle, f: impl FnOnce(&mut Ctl) -> R) -> R {
    let state = app.state::<ZonesState>();
    let mut ctl = crate::state::relock(&state.ctl);
    f(&mut ctl)
}

#[derive(Serialize, Clone)]
pub struct ZoneTile {
    pub id: u32,
    pub name: String,
    pub icon: String,
    pub pipelines: PerMedia,
    pub captions: HashMap<Media, String>,
}

#[derive(Serialize, Clone)]
pub struct ZonesView {
    pub tiles: Vec<ZoneTile>,
    pub preview: bool,
    pub dragged: Vec<Media>,
}

/// The tiles from the zones in settings, each with the caption of its pipeline per media.
fn tiles(app: &AppHandle) -> Vec<ZoneTile> {
    let settings = app.state::<AppState>().settings.read().unwrap().clone();
    let library = &settings.pipelines;
    library
        .zones
        .iter()
        .map(|zone| ZoneTile {
            id: zone.id,
            name: zone.name.clone(),
            icon: zone.icon.clone(),
            pipelines: zone.pipelines.clone(),
            captions: Media::ALL
                .into_iter()
                .filter_map(|media| {
                    let pipeline = library.get(zone.pipelines.get(media)?)?;
                    (pipeline.media == media).then(|| (media, caption(pipeline, registry(), 2)))
                })
                .collect(),
        })
        .collect()
}

pub fn view(app: &AppHandle) -> ZonesView {
    let (preview, dragged) = with_ctl(app, |c| (c.mode == Mode::Preview, c.dragged.clone()));
    ZonesView {
        tiles: tiles(app),
        preview,
        dragged,
    }
}

/// The media of each path by extension; folders and unknown files have none.
pub fn classify(paths: &[PathBuf]) -> Vec<Option<Media>> {
    paths
        .iter()
        .map(|p| {
            p.extension()
                .and_then(|e| e.to_str())
                .and_then(Media::guess)
        })
        .collect()
}

fn dragged_media(paths: &[PathBuf]) -> Vec<Media> {
    let mut media: Vec<Media> = Vec::new();
    for m in classify(paths).into_iter().flatten() {
        if !media.contains(&m) {
            media.push(m);
        }
    }
    media
}

pub fn is_visible(app: &AppHandle) -> bool {
    with_ctl(app, |c| c.visible)
}

/// The window, built on first use. Call from a worker thread: building waits on the main one.
fn ensure_window(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let state = app.state::<ZonesState>();
    let _building = crate::state::relock(&state.build);
    if let Some(window) = app.get_webview_window(ZONES) {
        return Ok(window);
    }
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, ZONES, WebviewUrl::App("zones.html".into()))
        .title(t("pipe.zones.window_title"))
        .inner_size(1.0, 1.0)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .content_protected(!show_in_screenshots)
        .visible(false)
        .build()?;
    // AppKit panels can only be changed on the main thread; this runs on a worker.
    let (done, wait) = std::sync::mpsc::channel();
    let panel = window.clone();
    app.run_on_main_thread(move || {
        let _ = done.send(platform::make_non_activating(&panel));
    })?;
    wait.recv()
        .map_err(|_| tauri::Error::FailedToReceiveMessage)??;
    Ok(window)
}

/// Show the grid centred on the cursor. In `Drag` mode it hides itself if no drag enters it.
pub fn show(app: &AppHandle, mode: Mode, paths: Vec<PathBuf>) {
    let window = match ensure_window(app) {
        Ok(window) => window,
        Err(e) => {
            warn!("could not build the preset zones: {e}");
            return;
        }
    };
    let generation = with_ctl(app, |c| {
        c.generation += 1;
        c.visible = true;
        c.mode = mode;
        c.entered = false;
        c.dragged = dragged_media(&paths);
        c.generation
    });
    let handle = app.clone();
    let shown = app.run_on_main_thread(move || {
        let view = view(&handle);
        let protect = !handle
            .state::<AppState>()
            .settings
            .read()
            .unwrap()
            .general
            .show_in_screenshots;
        let _ = window.set_content_protected(protect);
        place(
            &handle,
            &window,
            view.tiles.len() + 1 + usize::from(view.preview),
        );
        let _ = platform::show_without_focus(&window);
        let _ = window.emit("pipe:zones", view);
    });
    if let Err(e) = shown {
        warn!("could not show the preset zones: {e}");
    }
    if mode == Mode::Drag {
        hide_later(app, generation, NO_ENTER_MS);
    }
}

pub fn hide(app: &AppHandle) {
    with_ctl(app, |c| {
        c.generation += 1;
        c.visible = false;
        c.entered = false;
    });
    let handle = app.clone();
    let hidden = app.run_on_main_thread(move || {
        if let Some(window) = handle.get_webview_window(ZONES) {
            let _ = platform::hide(&window);
        }
    });
    if let Err(e) = hidden {
        warn!("could not hide the preset zones: {e}");
    }
}

/// Hide after `ms` unless anything else happened to the grid meanwhile.
fn hide_later(app: &AppHandle, generation: u64, ms: u64) {
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(ms));
        let stale = with_ctl(&app, |c| {
            c.generation != generation || c.entered || c.mode != Mode::Drag || !c.visible
        });
        if !stale {
            hide(&app);
        }
    });
}

/// The page reports a drag entering (true) or leaving (false) the grid.
pub fn dragging(app: &AppHandle, entered: bool) {
    let generation = with_ctl(app, |c| {
        c.entered = entered;
        c.generation += 1;
        c.generation
    });
    if !entered {
        hide_later(app, generation, AFTER_LEAVE_MS);
    }
}

/// Hide after a drop, once the jobs have started.
pub fn hide_after_drop(app: &AppHandle) {
    let generation = with_ctl(app, |c| c.generation);
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_millis(DROPPED_MS));
        if with_ctl(&app, |c| c.generation == generation && c.mode == Mode::Drag) {
            hide(&app);
        }
    });
}

/// Size and move the window to centre on the cursor's monitor position (the
/// stack's per-OS way, `windows::stack_resize`).
fn place(app: &AppHandle, window: &WebviewWindow, tiles: usize) {
    let Some(monitor) = crate::drop_batch::zone::cursor_monitor(app) else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    // tao reports the macOS cursor as points × the main display's scale.
    #[cfg(target_os = "macos")]
    let cursor_scale = app
        .primary_monitor()
        .ok()
        .flatten()
        .map_or(1.0, |m| m.scale_factor());
    #[cfg(not(target_os = "macos"))]
    let cursor_scale = scale;
    let cursor = app.cursor_position().map_or(
        (
            f64::from(work.position.x) / scale + f64::from(work.size.width) / scale / 2.0,
            f64::from(work.position.y) / scale + f64::from(work.size.height) / scale / 2.0,
        ),
        |p| (p.x / cursor_scale, p.y / cursor_scale),
    );
    let size = zones_size(tiles);
    let area = (
        f64::from(work.position.x) / scale,
        f64::from(work.position.y) / scale,
        f64::from(work.size.width) / scale,
        f64::from(work.size.height) / scale,
    );
    let at = zones_origin(cursor, size, area);
    #[cfg(target_os = "macos")]
    {
        use tauri::LogicalSize;
        let _ = window.set_size(LogicalSize::new(size.0, size.1));
        let _ = window.set_position(LogicalPosition::new(at.0, at.1));
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let to =
            PhysicalPosition::new((at.0 * scale).round() as i32, (at.1 * scale).round() as i32);
        // Move first so the window is on the target monitor before it is sized.
        let _ = window.set_position(to);
        let _ = window.set_size(PhysicalSize::new(
            (size.0 * scale).round() as u32,
            (size.1 * scale).round() as u32,
        ));
        let _ = window.set_position(to);
    }
}

/// The native menu of a tile: edit, go to its pipeline, remove.
pub fn popup_menu(
    app: &AppHandle,
    window: &WebviewWindow,
    zone: u32,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let has_pipeline = image_pipeline(app, zone).is_some();
    let item = |action: &str, label: String, enabled: bool| {
        MenuItem::with_id(
            app,
            format!("pipezone:{zone}:{action}"),
            label,
            enabled,
            None::<&str>,
        )
    };
    let menu = Menu::with_items(
        app,
        &[
            &item("edit", t("pipe.zones.menu.edit"), true).map_err(|e| e.to_string())?,
            &item("pipeline", t("pipe.zones.menu.pipeline"), has_pipeline)
                .map_err(|e| e.to_string())?,
            &item("remove", t("pipe.zones.remove"), true).map_err(|e| e.to_string())?,
        ],
    )
    .map_err(|e| e.to_string())?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

/// The zone's pipeline for images, the one "Go to pipeline" opens.
fn image_pipeline(app: &AppHandle, zone: u32) -> Option<u32> {
    app.state::<AppState>()
        .settings
        .read()
        .unwrap()
        .pipelines
        .zone(zone)?
        .pipelines
        .image
}

/// `pipezone:<zone>:<action>`.
fn parse_item(id: &str) -> Option<(u32, &str)> {
    let rest = id.strip_prefix("pipezone:")?;
    let (zone, action) = rest.split_once(':')?;
    Some((zone.parse().ok()?, action))
}

fn run_item(app: &AppHandle, zone: u32, action: &str) {
    match action {
        "edit" => pipe_windows::open_editor(app, None, Some("zones")),
        "pipeline" => {
            if let Some(pipeline) = image_pipeline(app, zone) {
                pipe_windows::open_editor(app, Some(pipeline), None);
            }
        }
        "remove" => {
            if let Err(e) = prefs::update(app, |s| s.pipelines.zones.retain(|z| z.id != zone)) {
                warn!("could not remove the zone: {e}");
            }
            // The grid on screen is stale now.
            let _ = app.emit_to(ZONES, "pipe:zones", view(app));
        }
        _ => {}
    }
}

/// Listen for the drag watcher's taps and the end of a drag, and route the tile menu.
pub fn init(app: &AppHandle) {
    app.manage(ZonesState::default());
    let tap = app.clone();
    app.listen_any("drag:tap", move |event| {
        let Ok(parsed) = serde_json::from_str::<Tap>(event.payload()) else {
            return;
        };
        if !is_preset_tap(&parsed) {
            return;
        }
        let app = tap.clone();
        // The listener runs on the watcher's thread; building a window must not hold it.
        tauri::async_runtime::spawn_blocking(move || {
            if is_visible(&app) {
                hide(&app);
            } else {
                show(&app, Mode::Drag, platform::drop::drag_files());
            }
        });
    });
    let ended = app.clone();
    app.listen_any("drag:ended", move |_| {
        if with_ctl(&ended, |c| c.visible && c.mode == Mode::Drag && !c.entered) {
            hide(&ended);
        }
    });
    app.on_menu_event(|app, event| {
        if let Some((zone, action)) = parse_item(event.id().as_ref()) {
            run_item(app, zone, action);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_grid_grows_to_four_columns_then_wraps() {
        assert_eq!(zones_size(1), (12.0 * 2.0 + 148.0, 12.0 * 2.0 + 100.0));
        assert_eq!(zones_size(4), (24.0 + 4.0 * 148.0 + 3.0 * 8.0, 124.0));
        assert_eq!(
            zones_size(5),
            (24.0 + 4.0 * 148.0 + 3.0 * 8.0, 24.0 + 2.0 * 100.0 + 8.0)
        );
    }

    #[test]
    fn only_a_control_tap_toggles_the_zones() {
        let tap =
            |m: &str| serde_json::from_value::<Tap>(serde_json::json!({ "modifier": m })).unwrap();
        assert!(is_preset_tap(&tap("control")));
        assert!(
            !is_preset_tap(&tap("alt"))
                && !is_preset_tap(&tap("command"))
                && !is_preset_tap(&tap("shift"))
        );
    }

    #[test]
    fn the_grid_centres_on_the_cursor_and_stays_on_screen() {
        let work = (0.0, 0.0, 1440.0, 900.0);
        assert_eq!(
            zones_origin((720.0, 450.0), (200.0, 100.0), work),
            (620.0, 400.0)
        );
        assert_eq!(
            zones_origin((5.0, 890.0), (200.0, 100.0), work),
            (16.0, 784.0)
        );
    }

    #[test]
    fn a_grid_larger_than_the_screen_keeps_its_corner_in_view() {
        assert_eq!(
            zones_origin((100.0, 100.0), (900.0, 700.0), (0.0, 0.0, 800.0, 600.0)),
            (16.0, 16.0)
        );
    }

    #[test]
    fn dropped_files_are_classified_by_extension() {
        let paths = [
            PathBuf::from("/a/shot.PNG"),
            PathBuf::from("/a/clip.mov"),
            PathBuf::from("/a/folder"),
            PathBuf::from("/a/two.png"),
        ];
        assert_eq!(
            classify(&paths),
            [
                Some(Media::Image),
                Some(Media::Video),
                None,
                Some(Media::Image)
            ]
        );
        assert_eq!(dragged_media(&paths), [Media::Image, Media::Video]);
    }

    #[test]
    fn a_menu_item_id_names_its_zone_and_action() {
        assert_eq!(parse_item("pipezone:3:edit"), Some((3, "edit")));
        assert_eq!(parse_item("pipezone:x:edit"), None);
        assert_eq!(parse_item("card:3:edit"), None);
    }
}
