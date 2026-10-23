//! Commands the stack page calls (spec §5.5). File and clipboard work runs on
//! a blocking worker, never on the main thread.

use ferry_core::edit::EditsPatch;
use ferry_core::i18n::t;
use ferry_core::job::{JobId, JobUpdate};
use ferry_core::settings::{CardSize, Corner, Images, Settings, TrayStyle};
use ferry_core::sniff::Format;
use ferry_core::{color, glyph, template};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::{hover_keys, jobs, logging, menu, platform, prefs};

/// Run `work` on a blocking worker and return its result.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct StackConfig {
    corner: Corner,
    card_size: CardSize,
    auto_hide_secs: u32,
    /// Where the Compression slider starts when a card has no quality of its own.
    compression: u8,
    crop_sizes: Vec<(u32, u32)>,
    hover_hotkeys: bool,
}

/// The stack page is listening: hand it its settings and any cards sent before.
#[tauri::command]
pub fn stack_ready(app: AppHandle) -> (StackConfig, Vec<JobUpdate>) {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();
    let mut inbox = state.stack.lock().unwrap();
    inbox.ready = true;
    let config = StackConfig {
        corner: settings.floating.corner,
        card_size: settings.floating.card_size,
        auto_hide_secs: settings.floating.auto_hide_secs,
        compression: settings.images.compression,
        crop_sizes: settings.images.crop_sizes,
        hover_hotkeys: settings.floating.hover_hotkeys,
    };
    (config, std::mem::take(&mut inbox.pending))
}

#[tauri::command]
pub async fn card_copy(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |app| jobs::copy(app, &id)).await
}

#[tauri::command]
pub async fn card_restore(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |app| jobs::restore(app, &id)).await
}

/// Retry, re-optimise (`aggressive: false`) or "Try aggressive".
#[tauri::command]
pub async fn card_rerun(app: AppHandle, id: JobId, aggressive: bool) -> Result<(), String> {
    blocking(app, move |app| jobs::rerun(app, &id, aggressive)).await
}

/// Merge `edits` into the card's recipe and run it again (spec §4.5).
#[tauri::command]
pub async fn card_edit(app: AppHandle, id: JobId, edits: EditsPatch) -> Result<(), String> {
    blocking(app, move |app| jobs::edit(app, &id, edits)).await
}

/// The format bar: convert, or with `keep_both` (JS `keepBoth`) add a twin card.
#[tauri::command]
pub async fn card_convert(
    app: AppHandle,
    id: JobId,
    format: Format,
    keep_both: bool,
) -> Result<(), String> {
    blocking(app, move |app| jobs::convert(app, &id, format, keep_both)).await
}

/// `Err` is the inline message under the name field.
#[tauri::command]
pub async fn card_rename(app: AppHandle, id: JobId, stem: String) -> Result<(), String> {
    blocking(app, move |app| jobs::rename(app, &id, &stem)).await
}

#[tauri::command]
pub fn card_cancel(app: AppHandle, id: JobId) {
    jobs::cancel(&app, &id);
}

/// The card's ⋯ button or right-click: the native context menu at `(x, y)`
/// in the stack page. Sync, so it runs on the main thread, as menus must.
#[tauri::command]
pub fn card_menu(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: JobId,
    x: f64,
    y: f64,
) -> Result<(), String> {
    menu::popup(&app, &window, &id, x, y)
}

/// The idle card under the pointer, or none: the hover hotkeys follow it.
/// Sync, so it runs on the main thread, where shortcuts are registered.
#[tauri::command]
pub fn card_hover(app: AppHandle, id: Option<JobId>) {
    hover_keys::set(&app, id);
}

#[tauri::command]
pub fn card_reveal(app: AppHandle, id: JobId) -> Result<(), String> {
    let path = jobs::output(&app, &id).ok_or("the file is gone")?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// An Error card's "Details": the newest log file in the file manager.
#[tauri::command]
pub fn open_log(app: AppHandle) -> Result<(), String> {
    let dir = app.state::<AppState>().log_dir.clone();
    match logging::latest(&dir) {
        Some(file) => tauri_plugin_opener::reveal_item_in_dir(file),
        None => tauri_plugin_opener::open_path(&dir, None::<&str>),
    }
    .map_err(|e| e.to_string())
}

/// "Clear all" also ends an "accumulate results" collection (§5.4).
#[tauri::command]
pub fn clear_all(app: AppHandle) {
    app.state::<AppState>()
        .clipboard
        .lock()
        .unwrap()
        .accumulated
        .clear();
}

#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    app.state::<AppState>().settings.read().unwrap().clone()
}

/// Apply a change from the Settings page. The answer is what was saved, which
/// may differ from what was sent (values are clamped).
#[tauri::command(async)]
pub fn settings_set(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    prefs::update(&app, |current| *current = settings)
}

/// The crop popover's "Restore defaults": put the saved crop sizes back.
#[tauri::command(async)]
pub fn crop_sizes_reset(app: AppHandle) -> Result<Settings, String> {
    prefs::update(&app, |s| s.images.crop_sizes = Images::default().crop_sizes)
}

/// The live example under the file-name template field, or its unknown tokens.
#[tauri::command]
pub fn template_preview(template: String) -> Result<String, Vec<String>> {
    template::preview(&template)
}

#[derive(Serialize)]
pub struct TrayPreview {
    style: TrayStyle,
    png: Vec<u8>,
}

/// The three tray icon styles as PNGs, for the picker. Black on transparent,
/// like the template image the menu bar tints; the page inverts it in dark mode.
#[tauri::command]
pub fn tray_style_previews() -> Vec<TrayPreview> {
    let Ok(base) = tauri::image::Image::from_bytes(crate::tray::BASE_ICON) else {
        return Vec::new();
    };
    [TrayStyle::Default, TrayStyle::Classic, TrayStyle::Flat]
        .into_iter()
        .map(|style| {
            let rgba =
                glyph::render_styled(base.rgba(), base.width(), glyph::TrayGlyph::Running, style);
            TrayPreview {
                style,
                png: glyph::to_png(&rgba, base.width()),
            }
        })
        .collect()
}

/// The macOS accent colour as `#rrggbb`, fit for white text; `None` elsewhere.
#[tauri::command]
pub fn accent_color() -> Option<String> {
    platform::accent_rgb().map(color::accent_hex)
}

/// Let the user pick an application and answer with the identifier the
/// clipboard reports for it (bundle id on macOS, file name on Windows);
/// `None` when the dialog is cancelled.
#[tauri::command]
pub async fn pick_app(app: AppHandle) -> Result<Option<String>, String> {
    blocking(app, |app| {
        use tauri_plugin_dialog::DialogExt;
        let dialog = app.dialog().file().set_title(t("clipboard.pick_app"));
        #[cfg(target_os = "macos")]
        let dialog = dialog
            .add_filter("Applications", &["app"])
            .set_directory("/Applications");
        #[cfg(windows)]
        let dialog = dialog.add_filter("Programs", &["exe"]);
        let Some(path) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        platform::app_identifier(&path)
            .map(Some)
            .ok_or_else(|| t("error.pick_app"))
    })
    .await
}

#[tauri::command]
pub fn open_backups(app: AppHandle) {
    crate::tray::open_folder(&app.state::<AppState>().backups_dir);
}

#[tauri::command]
pub fn open_work(app: AppHandle) {
    crate::tray::open_folder(&app.state::<AppState>().work_dir);
}

#[derive(Serialize)]
pub struct CleanDone {
    files: u32,
    bytes: u64,
    failed: u32,
}

#[tauri::command]
pub async fn force_clean(app: AppHandle) -> Result<CleanDone, String> {
    blocking(app, |app| {
        jobs::force_clean(app).map(|r| CleanDone {
            files: r.files,
            bytes: r.bytes,
            failed: r.failed,
        })
    })
    .await
}
