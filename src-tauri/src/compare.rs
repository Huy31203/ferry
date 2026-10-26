//! The compare window (spec §4.5): a card's original beside its current
//! result, in one window that is reused and sized to the image. On Windows it
//! is also the quick preview, showing the result alone.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use ferry_core::geometry::{Rect, center, fit_image};
use ferry_core::i18n::{t, tf};
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::poster;
use ferry_core::sniff::{self, Format};
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tracing::warn;

use crate::platform;
use crate::state::AppState;
use crate::windows::STACK;

pub const COMPARE: &str = "compare";

/// The bars above and below the image in compare.html, in logical pixels.
const CHROME: (f64, f64) = (0.0, 84.0);
/// The scrubber and buttons under a video, on top of `CHROME` (stream A: video).
const VIDEO_BAR: f64 = 64.0;
/// Room for the toolbar however small the image.
const MIN_SIZE: (f64, f64) = (520.0, 360.0);
/// The window takes at most this share of its screen each way.
const SCREEN_SHARE: f64 = 0.8;

/// What the window shows; the page collects it when it loads (`compare_ready`).
static SHOWN: Mutex<Option<CompareView>> = Mutex::new(None);

/// One image in the window and what its bar says about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shot {
    pub path: PathBuf,
    pub bytes: u64,
    /// As shown (JPEG orientation applied); `None` when unknown.
    pub dims: Option<(u32, u32)>,
    /// "PNG", "JPEG", "GIF" or "WebP"; empty for a file Ferry can't read.
    pub format: &'static str,
    /// Changes when the file is rewritten, so the page never shows a cached older result.
    pub version: u64,
    // stream A: video
    /// "image" or "video".
    pub kind: &'static str,
    pub duration_ms: Option<u64>,
    pub fps: Option<f32>,
    // sE pdf-formats
    /// What the page draws: `path`, or a PNG poster of it (TIFF, HEIC, a PDF page, a pages-folder page).
    pub src: PathBuf,
    /// How many pages `src` can step through; `None` for a plain image.
    pub pages: Option<u32>,
}

/// Everything the page needs. `original` is `None` in single-image mode.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompareView {
    pub id: JobId,
    /// "shot.png".
    pub name: String,
    pub original: Option<Shot>,
    pub now: Shot,
    /// The 1-based page shown (sE pdf-formats).
    pub page: u32,
}

/// What the window shows for `card`, or why it can't. `single` leaves the
/// original out, so an expired backup does not matter.
pub fn view(card: &JobUpdate, single: bool) -> Result<CompareView, String> {
    // stream B: audio (Quick Look on macOS still previews it)
    if card.audio.is_some() && (!single || !cfg!(target_os = "macos")) {
        return Err(t("audio.compare_unavailable"));
    }
    if matches!(card.state, JobState::Queued | JobState::Optimising { .. }) {
        return Err("the job is still running".into());
    }
    let output = existing(card.output.as_deref()).ok_or_else(|| t("compare.file_gone"))?;
    let original = if single {
        None
    } else {
        // stream A: video (the card's thumb is a picture; the backup is the original itself)
        let kept = match &card.video {
            Some(video) => video.original.as_deref(),
            None => card.thumb_path.as_deref(),
        };
        let backup = existing(kept).ok_or_else(|| t("compare.original_gone"))?;
        let mut original = shot(backup, card.dims_before);
        if let Some(video) = &card.video {
            original.duration_ms = Some(video.original_duration_ms);
        }
        Some(original)
    };
    let mut now = shot(output, card.dims_after.or(card.dims_before));
    if let Some(video) = &card.video {
        now.duration_ms = Some(video.duration_ms);
        now.fps = video.fps;
    }
    Ok(CompareView {
        id: card.id.clone(),
        name: if card.ext.is_empty() {
            card.name.clone()
        } else {
            format!("{}.{}", card.name, card.ext)
        },
        original,
        now,
        page: 1,
    })
}

/// Whether `shown` is card `id` in the same mode, so a second ⌘D or Space
/// closes the window rather than reloading it.
fn same_view(shown: Option<&CompareView>, id: &JobId, single: bool) -> bool {
    shown.is_some_and(|v| v.id == *id && v.original.is_none() == single)
}

fn existing(path: Option<&Path>) -> Option<&Path> {
    path.filter(|p| p.is_file() || p.is_dir()) // sE pdf-formats: a pages folder is a result too
}

fn shot(path: &Path, dims: Option<(u32, u32)>) -> Shot {
    let meta = std::fs::metadata(path).ok();
    let format = head_format(path);
    Shot {
        path: path.to_owned(),
        src: path.to_owned(), // sE pdf-formats
        pages: None,
        bytes: if path.is_dir() {
            folder_bytes(path)
        } else {
            meta.as_ref().map_or(0, |m| m.len())
        },
        dims,
        format: format.map_or("", Format::label),
        kind: if format.is_some_and(Format::is_video) {
            "video"
        } else {
            "image"
        },
        duration_ms: None,
        fps: None,
        version: meta
            .and_then(|m| m.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64),
    }
}

/// A file's format from its first bytes: the card only knows the result's
/// format, and the backup may differ after a conversion.
fn head_format(path: &Path) -> Option<Format> {
    // 64 bytes: a Matroska or WebM doc type sits after the EBML header.
    let mut head = Vec::with_capacity(64);
    std::fs::File::open(path)
        .ok()?
        .take(64)
        .read_to_end(&mut head)
        .ok()?;
    sniff::sniff(&head)
}

/// The card's latest state (`Entry::last`): the window needs its sizes and
/// backup as well as its current file.
pub fn card(app: &AppHandle, id: &JobId) -> Result<JobUpdate, String> {
    let state = app.state::<AppState>();
    let entry = state
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("this card's job is gone")?;
    let card = entry.last.lock().unwrap().clone();
    Ok(card)
}

/// Whether the window is on screen and has focus, so Space and ⌘D close it.
pub fn is_active(app: &AppHandle) -> bool {
    app.get_webview_window(COMPARE)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// Show card `id` (`single`: its result alone) and give the window focus.
/// Asking again for what is already on show closes the window instead.
pub fn show(app: &AppHandle, id: &JobId, single: bool) -> Result<(), String> {
    let mut view = view(&card(app, id)?, single)?;
    if same_view(SHOWN.lock().unwrap().as_ref(), id, single) && is_active(app) {
        close(app);
        return Ok(());
    }
    let work_dir = app.state::<AppState>().work_dir.clone();
    with_posters(&mut view, &work_dir); // sE pdf-formats
    // The configured scope only covers backups; results can be anywhere.
    let scope = app.asset_protocol_scope();
    for shot in view.original.iter().chain([&view.now]) {
        scope.allow_file(&shot.path).map_err(|e| e.to_string())?;
        scope.allow_file(&shot.src).map_err(|e| e.to_string())?; // sE pdf-formats
    }
    let shown = view.original.as_ref().unwrap_or(&view.now);
    let mut image = shown.dims;
    // stream A: video (two players side by side, and a bar for the scrubber)
    let mut chrome = CHROME;
    if view.now.kind == "video" {
        chrome.1 += VIDEO_BAR;
        if view.original.is_some() {
            image = image.map(|(w, h)| (2 * w + 1, h));
        }
    }
    let title = if single {
        view.name.clone()
    } else {
        tf("compare.window_title", &[("name", &view.name)])
    };
    if !is_active(app) {
        platform::preview::remember_front();
    }
    *SHOWN.lock().unwrap() = Some(view.clone());
    let window = match app.get_webview_window(COMPARE) {
        Some(window) => {
            if let Err(e) = app.emit_to(COMPARE, "compare:show", &view) {
                warn!("could not update the compare window: {e}");
            }
            window
        }
        None => build(app).map_err(|e| e.to_string())?,
    };
    let _ = window.set_title(&title);
    place(app, &window, image.unwrap_or((0, 0)), chrome);
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

/// Hide the window (it is reused), let the page drop its images, and hand
/// focus back to the app that had it.
pub fn close(app: &AppHandle) {
    *SHOWN.lock().unwrap() = None;
    let Some(window) = app.get_webview_window(COMPARE) else {
        return;
    };
    let focused = window.is_focused().unwrap_or(false);
    let _ = window.hide();
    let _ = app.emit_to(COMPARE, "compare:hidden", ());
    if focused {
        platform::preview::give_back_focus();
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, COMPARE, WebviewUrl::App("compare.html".into()))
        .title(t("app.name"))
        .content_protected(!show_in_screenshots)
        .inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .visible(false)
        .build()?;
    // The close button hides the window like Esc, so the page stays loaded.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            close(&handle);
        }
    });
    Ok(window)
}

/// Size the window to `image` (pixels) and centre it on the cards' screen.
fn place(app: &AppHandle, window: &WebviewWindow, image: (u32, u32), chrome: (f64, f64)) {
    let monitor = app
        .get_webview_window(STACK)
        .and_then(|stack| stack.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let px = |logical: f64| (logical * scale).round() as u32;
    let size = fit_image(
        area,
        image,
        (px(chrome.0), px(chrome.1)),
        SCREEN_SHARE,
        (px(MIN_SIZE.0), px(MIN_SIZE.1)),
    );
    let (x, y) = center(area, size);
    // As in `windows::stack_resize`: macOS positions windows in points.
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_size(LogicalSize::new(
            f64::from(size.0) / scale,
            f64::from(size.1) / scale,
        ));
        let _ = window.set_position(LogicalPosition::new(
            f64::from(x) / scale,
            f64::from(y) / scale,
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let _ = window.set_size(PhysicalSize::new(size.0, size.1));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// ⌘D, the rail's Compare button and "Compare with original" (M3b).
#[tauri::command]
pub async fn open_compare(app: AppHandle, id: JobId) -> Result<(), String> {
    show(&app, &id, false)
}

/// The page has loaded: what to show, if anything yet.
#[tauri::command]
pub fn compare_ready() -> Option<CompareView> {
    SHOWN.lock().unwrap().clone()
}

/// Esc or Space in the page.
#[tauri::command]
pub fn compare_close(app: AppHandle) {
    close(&app);
}

// ---- sE pdf-formats ----

/// The total size of the files in a pages folder.
fn folder_bytes(dir: &Path) -> u64 {
    page_files(dir)
        .iter()
        .filter_map(|file| std::fs::metadata(file).ok())
        .map(|meta| meta.len())
        .sum()
}

/// The files of a pages folder sorted by name.
fn page_files(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_file())
        .collect();
    files.sort();
    files
}

/// Page `index` (1-based) of a pages folder, by file name order.
fn page_file(dir: &Path, index: u32) -> Option<PathBuf> {
    let at = usize::try_from(index.checked_sub(1)?).ok()?;
    page_files(dir).into_iter().nth(at)
}

/// The number a page file's name ends in: `doc-03.png` is 3.
fn page_number(file: &Path) -> Option<u32> {
    let stem = file.file_stem()?.to_str()?;
    let digits = stem.len() - stem.trim_end_matches(|c: char| c.is_ascii_digit()).len();
    stem[stem.len() - digits..].parse().ok()
}

/// How many pages the window can step through: the result's, else the original's.
fn page_count(view: &CompareView) -> Option<u32> {
    view.now
        .pages
        .or_else(|| view.original.as_ref().and_then(|o| o.pages))
}

/// Fill in what the page draws: a poster for each TIFF, HEIC or PDF shot, and
/// the shown page of a pages folder. A shot that can't be drawn keeps
/// `src = path`, so the page shows its error state.
pub fn with_posters(view: &mut CompareView, work_dir: &Path) {
    let page = view.page;
    let mut original_page = page;
    if view.now.path.is_dir() {
        let dir = view.now.path.clone();
        view.now.pages = u32::try_from(page_files(&dir).len()).ok();
        if let Some(file) = page_file(&dir, page) {
            original_page = page_number(&file).unwrap_or(page);
            if let Ok(bytes) = std::fs::read(&file) {
                let format = sniff::sniff(&bytes);
                view.now.dims = format.and_then(|f| sniff::dimensions(&bytes, f));
                view.now.format = format.map_or("", Format::label);
            }
            view.now.src = file;
        }
    }
    let shots = view
        .original
        .iter_mut()
        .map(|shot| (shot, original_page))
        .chain([(&mut view.now, page)]);
    for (shot, page) in shots {
        if !shot.path.is_file() || !head_format(&shot.path).is_some_and(poster::needs_poster) {
            continue;
        }
        if let Ok(shown) = poster::viewable(&shot.path, page, work_dir) {
            shot.src = shown.src;
            shot.dims = shown.dims.or(shot.dims);
            shot.pages = shown.pages;
        }
    }
}

/// Page Up and Page Down, or the stepper, in the window: show page `page`.
#[tauri::command]
pub fn compare_page(app: AppHandle, page: u32) -> Result<(), String> {
    let mut view = SHOWN
        .lock()
        .unwrap()
        .clone()
        .ok_or("nothing is being compared")?;
    view.page = page.clamp(1, page_count(&view).unwrap_or(1));
    let work_dir = app.state::<AppState>().work_dir.clone();
    with_posters(&mut view, &work_dir);
    let scope = app.asset_protocol_scope();
    for shot in view.original.iter().chain([&view.now]) {
        scope.allow_file(&shot.src).map_err(|e| e.to_string())?;
    }
    *SHOWN.lock().unwrap() = Some(view.clone());
    app.emit_to(COMPARE, "compare:show", &view)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest of a png";
    const WEBP: &[u8] = b"RIFF\0\0\0\0WEBPVP8 rest";

    /// A finished card whose backup is a PNG and whose result is a WebP.
    fn converted(dir: &Path) -> JobUpdate {
        let backup = dir.join("backup.png");
        let output = dir.join("shot.webp");
        fs::write(&backup, PNG).unwrap();
        fs::write(&output, WEBP).unwrap();
        JobUpdate {
            state: JobState::Done,
            name: "shot".into(),
            ext: "webp".into(),
            before: PNG.len() as u64,
            after: Some(WEBP.len() as u64),
            thumb_path: Some(backup),
            output: Some(output),
            dims_before: Some((2880, 1800)),
            dims_after: Some((1440, 900)),
            format: Some(Format::WebP),
            converted: true,
            ..ferry_core::pipeline::notice(String::new())
        }
    }

    #[test]
    fn a_finished_card_shows_its_original_beside_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        let view = view(&card, false).unwrap();
        assert_eq!(view.id, card.id);
        assert_eq!(view.name, "shot.webp");
        let original = view.original.unwrap();
        assert_eq!(original.path, dir.path().join("backup.png"));
        assert_eq!(original.format, "PNG");
        assert_eq!(original.dims, Some((2880, 1800)));
        assert_eq!(original.bytes, PNG.len() as u64);
        assert_eq!(view.now.path, dir.path().join("shot.webp"));
        assert_eq!(view.now.format, "WebP");
        assert_eq!(view.now.dims, Some((1440, 900)));
        assert_eq!(view.now.bytes, WEBP.len() as u64);
        assert!(view.now.version > 0);
    }

    // stream A: video
    #[test]
    fn a_video_card_compares_its_backup_not_its_thumbnail() {
        let dir = tempfile::tempdir().unwrap();
        let mp4 = [&[0, 0, 0, 0x18][..], b"ftypisom", &[0; 12]].concat();
        let backup = dir.path().join("backup.mp4");
        let output = dir.path().join("out.mp4");
        let thumb = dir.path().join("thumb.jpg");
        fs::write(&backup, &mp4).unwrap();
        fs::write(&output, &mp4).unwrap();
        fs::write(&thumb, b"\xFF\xD8\xFF jpeg").unwrap();
        let card = JobUpdate {
            state: JobState::Done,
            name: "clip".into(),
            ext: "mp4".into(),
            thumb_path: Some(thumb),
            output: Some(output),
            format: Some(Format::Mp4),
            video: Some(ferry_core::job::VideoMeta {
                duration_ms: 5000,
                codec: "H.264".into(),
                fps: Some(30.0),
                audio: true,
                original: Some(backup.clone()),
                original_duration_ms: 10_000,
                hevc_encoder: false,
            }),
            ..ferry_core::pipeline::notice(String::new())
        };
        let view = view(&card, false).unwrap();
        let original = view.original.unwrap();
        assert_eq!(original.path, backup);
        assert_eq!(original.kind, "video");
        assert_eq!(original.duration_ms, Some(10_000));
        assert_eq!(view.now.duration_ms, Some(5000));
        assert_eq!(view.now.fps, Some(30.0));
        assert_eq!(view.now.format, "MP4");
    }

    #[test]
    fn a_video_without_its_backup_cannot_be_compared() {
        let dir = tempfile::tempdir().unwrap();
        let mp4 = [&[0, 0, 0, 0x18][..], b"ftypisom", &[0; 12]].concat();
        let output = dir.path().join("out.mp4");
        fs::write(&output, &mp4).unwrap();
        let card = JobUpdate {
            state: JobState::Done,
            name: "clip".into(),
            ext: "mp4".into(),
            output: Some(output),
            format: Some(Format::Mp4),
            video: Some(ferry_core::job::VideoMeta {
                duration_ms: 5000,
                codec: "H.264".into(),
                fps: None,
                audio: false,
                original: None,
                original_duration_ms: 5000,
                hevc_encoder: false,
            }),
            ..ferry_core::pipeline::notice(String::new())
        };
        assert_eq!(view(&card, false), Err("Original no longer kept".into()));
        assert!(view(&card, true).is_ok());
    }

    #[test]
    fn a_result_that_kept_its_size_shows_the_original_size() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            dims_after: None,
            ..converted(dir.path())
        };
        assert_eq!(view(&card, false).unwrap().now.dims, Some((2880, 1800)));
    }

    #[test]
    fn an_expired_backup_means_the_original_is_no_longer_kept() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        fs::remove_file(card.thumb_path.as_ref().unwrap()).unwrap();
        assert_eq!(view(&card, false), Err("Original no longer kept".into()));
        let card = JobUpdate {
            thumb_path: None,
            ..card
        };
        assert_eq!(view(&card, false), Err("Original no longer kept".into()));
    }

    #[test]
    fn single_image_mode_needs_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            thumb_path: None,
            ..converted(dir.path())
        };
        let view = view(&card, true).unwrap();
        assert_eq!(view.original, None);
        assert_eq!(view.now.format, "WebP");
    }

    #[test]
    fn a_result_that_is_gone_cannot_be_shown() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        fs::remove_file(card.output.as_ref().unwrap()).unwrap();
        assert_eq!(view(&card, true), Err("File no longer there".into()));
        let notice = ferry_core::pipeline::notice("Nothing to optimise".into());
        assert_eq!(view(&notice, true), Err("File no longer there".into()));
    }

    #[test]
    fn a_busy_card_waits_for_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            state: JobState::Queued,
            ..converted(dir.path())
        };
        assert!(view(&card, false).is_err());
    }

    #[test]
    fn asking_again_for_what_is_on_show_toggles_it() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        let both = view(&card, false).unwrap();
        let alone = view(&card, true).unwrap();
        assert!(same_view(Some(&both), &card.id, false));
        assert!(same_view(Some(&alone), &card.id, true));
        assert!(!same_view(Some(&both), &card.id, true));
        assert!(!same_view(Some(&both), &JobId::generate(), false));
        assert!(!same_view(None, &card.id, false));
    }

    #[test]
    fn the_page_gets_plain_json() {
        let dir = tempfile::tempdir().unwrap();
        let json = serde_json::to_value(view(&converted(dir.path()), false).unwrap()).unwrap();
        assert_eq!(json["now"]["dims"], serde_json::json!([1440, 900]));
        assert_eq!(json["original"]["format"], "PNG");
        assert!(json["id"].is_string());
    }

    // sE pdf-formats
    fn pages_folder(dir: &Path) -> PathBuf {
        let folder = dir.join("doc");
        fs::create_dir(&folder).unwrap();
        for n in ["doc-02.png", "doc-03.png"] {
            fs::write(folder.join(n), PNG).unwrap();
        }
        folder
    }

    #[test]
    fn a_pages_folder_compares_page_by_page() {
        let dir = tempfile::tempdir().unwrap();
        let folder = pages_folder(dir.path());
        assert_eq!(page_file(&folder, 2), Some(folder.join("doc-03.png")));
        assert_eq!(page_file(&folder, 3), None);
        assert_eq!(page_file(&folder, 0), None);
        assert_eq!(page_number(&folder.join("doc-03.png")), Some(3));
        assert_eq!(page_number(&folder.join("doc.png")), None);
    }

    #[test]
    fn a_folder_result_can_be_shown() {
        let dir = tempfile::tempdir().unwrap();
        let folder = pages_folder(dir.path());
        let card = JobUpdate {
            output: Some(folder.clone()),
            ..converted(dir.path())
        };
        let mut view = view(&card, true).unwrap();
        assert_eq!(view.now.bytes, 2 * PNG.len() as u64);
        with_posters(&mut view, dir.path());
        assert_eq!(view.now.src, folder.join("doc-02.png"));
        assert_eq!((view.now.pages, view.now.format), (Some(2), "PNG"));
        view.page = 2;
        with_posters(&mut view, dir.path());
        assert_eq!(view.now.src, folder.join("doc-03.png"));
        assert_eq!(page_count(&view), Some(2));
    }

    #[test]
    fn a_plain_image_is_drawn_as_itself() {
        let dir = tempfile::tempdir().unwrap();
        let mut view = view(&converted(dir.path()), false).unwrap();
        with_posters(&mut view, dir.path());
        assert_eq!(view.now.src, view.now.path);
        assert_eq!(view.original.unwrap().pages, None);
    }

    // stream B: audio
    #[test]
    fn an_audio_card_has_no_compare_window() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            audio: Some(ferry_core::audio::AudioCard {
                bitrate_before: None,
                bitrate_after: Some(192),
                lossless: false,
                has_cover: false,
            }),
            ..converted(dir.path())
        };
        assert_eq!(
            view(&card, false).unwrap_err(),
            "Audio compare isn't available yet"
        );
        assert_eq!(view(&card, true).is_ok(), cfg!(target_os = "macos"));
    }
}
