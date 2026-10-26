//! The card's context menu (spec §4.5): built from the card's last update,
//! popped up under the ⋯ button or the pointer, and routed back by item id.
//! What ends in the page (rename, crop, fit, compare, preview, close) goes to
//! the stack as a `card:action` event; the rest runs here.

use ferry_core::audio::{BAR, CoverPolicy, Loudness};
use ferry_core::i18n::{t, tf};
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::sniff::Format;
use ferry_core::video;
use serde::Serialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewWindow, Wry};
use tracing::warn;

use crate::jobs;
use crate::state::AppState;
use crate::windows::STACK;

const PREFIX: &str = "card:";

/// What a card menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Reveal,
    Copy,
    CopyPath,
    Rename,
    Compare,
    Preview,
    Rerun,
    Aggressive,
    Fit,
    /// Downscale to this percent.
    Scale(u8),
    Crop,
    // sE pdf-formats
    Pages,
    /// Resample a PDF's images to this DPI.
    Dpi(u16),
    Convert(Format),
    KeepBoth(Format),
    Restore,
    Close,
    // stream A: video
    Speed,
    Mute,
    // ---- stream B: audio ----
    Loudness(Loudness),
    Cover(CoverPolicy),
    /// Downscale the cover to a square this many pixels wide.
    CoverSquare(u32),
    ExtractCover,
    /// Stream H: integrations (share, open with, edit with, upload, shelf).
    Integration(crate::integrations::menu::Item),
    Pipe(crate::pipe_menu::PipeAction), // stream F: pipelines
}

const FORMATS: [Format; 18] = [
    Format::Png,
    Format::Jpeg,
    Format::Gif,
    Format::WebP,
    // sE pdf-formats
    Format::Tiff,
    Format::Heic,
    Format::Avif,
    Format::Pdf,
    // stream A: video
    Format::Mp4,
    Format::Hevc,
    Format::Av1,
    Format::WebM,
    Format::Mov,
    Format::Mkv,
    // ---- stream B: audio ----
    Format::Mp3,
    Format::M4a,
    Format::Opus,
    Format::Wav,
];

/// Square cover sizes in "Cover art ▸ Downscale ▸".
const COVER_SQUARES: [u32; 4] = [1400, 1000, 600, 300];
const LOUDNESS: [Loudness; 5] = [
    Loudness::Off,
    Loudness::Streaming,
    Loudness::AppleMusic,
    Loudness::Podcast,
    Loudness::Broadcast,
];
const COVERS: [CoverPolicy; 3] = [
    CoverPolicy::Keep,
    CoverPolicy::Optimise,
    CoverPolicy::Remove,
];

/// A value's part of an item id: its serde name.
fn serde_key<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

/// The actions without a value, by their id part.
const NAMED: [(&str, Action); 17] = [
    ("open", Action::Open),
    ("reveal", Action::Reveal),
    ("copy", Action::Copy),
    ("copy_path", Action::CopyPath),
    ("rename", Action::Rename),
    ("compare", Action::Compare),
    ("preview", Action::Preview),
    ("rerun", Action::Rerun),
    ("aggressive", Action::Aggressive),
    ("fit", Action::Fit),
    ("crop", Action::Crop),
    // sE pdf-formats
    ("pages", Action::Pages),
    ("restore", Action::Restore),
    ("close", Action::Close),
    // stream A: video
    ("speed", Action::Speed),
    ("mute", Action::Mute),
    // stream B: audio
    ("extract_cover", Action::ExtractCover),
];

impl Action {
    /// The item id's last part: "open", "scale-50", "convert-webp", "keep_both-jpg".
    fn key(self) -> String {
        match self {
            Action::Scale(percent) => format!("scale-{percent}"),
            Action::Dpi(n) => format!("dpi-{n}"), // sE pdf-formats
            Action::Convert(format) => format!("convert-{}", format_key(format)),
            Action::KeepBoth(format) => format!("keep_both-{}", format_key(format)),
            // stream B: audio
            Action::Loudness(l) => format!("loudness-{}", serde_key(&l)),
            Action::Cover(c) => format!("cover-{}", serde_key(&c)),
            Action::CoverSquare(n) => format!("cover_square-{n}"),
            Action::Integration(item) => format!("h-{}", item.key()),
            Action::Pipe(p) => format!("pipe-{}", p.key()), // stream F: pipelines
            named => NAMED
                .iter()
                .find(|(_, a)| *a == named)
                .map(|(key, _)| key.to_string())
                .expect("every action without a value is in NAMED"),
        }
    }

    fn parse(key: &str) -> Option<Self> {
        let format = |key: &str| FORMATS.into_iter().find(|f| format_key(*f) == key);
        match key.split_once('-') {
            Some(("scale", value)) => value.parse().ok().map(Action::Scale),
            Some(("dpi", value)) => value.parse().ok().map(Action::Dpi), // sE pdf-formats
            Some(("convert", value)) => format(value).map(Action::Convert),
            Some(("keep_both", value)) => format(value).map(Action::KeepBoth),
            // stream B: audio
            Some(("loudness", value)) => LOUDNESS
                .into_iter()
                .find(|l| serde_key(l) == value)
                .map(Action::Loudness),
            Some(("cover", value)) => COVERS
                .into_iter()
                .find(|c| serde_key(c) == value)
                .map(Action::Cover),
            Some(("cover_square", value)) => value.parse().ok().map(Action::CoverSquare),
            Some(("h", value)) => {
                crate::integrations::menu::Item::parse(value).map(Action::Integration)
            }
            Some(("pipe", value)) => crate::pipe_menu::PipeAction::parse(value).map(Action::Pipe), // stream F: pipelines
            Some(_) => None,
            None => NAMED.iter().find(|(k, _)| *k == key).map(|(_, a)| *a),
        }
    }

    /// The `card:action` the page handles, for actions that end in the page.
    pub fn page(self) -> Option<&'static str> {
        Some(match self {
            Action::Rename => "rename",
            Action::Crop => "crop",
            // sE pdf-formats
            Action::Pages => "pages",
            Action::Fit => "fit",
            Action::Compare => "compare",
            Action::Preview => "preview",
            Action::Close => "close",
            // stream A: video
            Action::Speed => "speed",
            // Stream H: integrations (the page measures the card, then calls card_share)
            Action::Integration(crate::integrations::menu::Item::Share) => "share",
            _ => return None,
        })
    }
}

// stream A: video
/// A format's part of an item id: the serde name for video (`mp4` and `hevc`
/// share an extension), the extension for images.
fn format_key(f: Format) -> &'static str {
    if f.is_video() {
        video::format_id(f)
    } else {
        f.extension()
    }
}

/// The menu item id: `card:<job id>:<action>`. Job ids have no `:`.
pub fn item_id(id: &JobId, action: Action) -> String {
    format!("{PREFIX}{}:{}", id.as_str(), action.key())
}

/// The card and action behind a menu item id; `None` for other menus' items.
pub fn parse_id(item: &str) -> Option<(JobId, Action)> {
    let (id, key) = item.strip_prefix(PREFIX)?.split_once(':')?;
    Some((job_id(id)?, Action::parse(key)?))
}

/// A `JobId` from its text (the type has no public constructor).
fn job_id(text: &str) -> Option<JobId> {
    serde_json::from_value(serde_json::Value::String(text.into())).ok()
}

/// Whether the item applies to the card now (spec §4.5): items that need the
/// original follow `can_edit`, Restore needs a backup, and a busy card offers
/// only Close.
pub fn enabled(card: &JobUpdate, action: Action) -> bool {
    let idle = !matches!(card.state, JobState::Queued | JobState::Optimising { .. });
    let file = idle && card.output.is_some();
    match action {
        Action::Close => true,
        Action::Open
        | Action::Reveal
        | Action::Copy
        | Action::CopyPath
        | Action::Rename
        | Action::Preview => file,
        Action::Integration(_) => file,
        Action::Pipe(p) => crate::pipe_menu::enabled(card, p), // stream F: pipelines
        Action::Restore => idle && card.can_restore,
        // sE pdf-formats (an image or PDF card converts only to its own choices)
        Action::Convert(format) => {
            card.can_edit
                && card.format != Some(format)
                && (card.video.is_some()
                    || card.audio.is_some()
                    || card.choices.iter().any(|c| c.format == format))
        }
        Action::Scale(percent) => card.can_edit && card.edits.scale.unwrap_or(100) != percent,
        Action::Rerun | Action::Aggressive | Action::Fit | Action::Crop | Action::KeepBoth(_) => {
            card.can_edit
        }
        // stream B: audio (no compare for audio yet)
        Action::Compare => card.can_edit && card.audio.is_none(),
        Action::Loudness(l) => card.can_edit && card.edits.loudness != Some(l),
        Action::Cover(_) | Action::CoverSquare(_) => {
            card.can_edit && card.audio.as_ref().is_some_and(|a| a.has_cover)
        }
        Action::ExtractCover => idle && card.audio.as_ref().is_some_and(|a| a.has_cover),
        // stream A: video
        Action::Speed | Action::Mute => card.can_edit && card.video.is_some(),
        // sE pdf-formats
        Action::Pages => card.can_edit && card.source_format == Some(Format::Pdf),
        Action::Dpi(n) => {
            card.can_edit && card.source_format == Some(Format::Pdf) && card.edits.dpi != Some(n)
        }
    }
}

/// "Convert to ▸": the card's own choices (like the format bar).
pub fn convert_formats(card: &JobUpdate) -> Vec<Format> {
    // stream B: audio
    if card.audio.is_some() {
        return BAR.to_vec();
    }
    // stream A: video (HEVC is hardware only: offered where an encoder exists)
    if card.source_format.is_some_and(Format::is_video) {
        let hevc = card.video.as_ref().is_some_and(|v| v.hevc_encoder);
        return [
            Format::Mp4,
            Format::Hevc,
            Format::Av1,
            Format::WebM,
            Format::Gif,
        ]
        .into_iter()
        .filter(|&f| f != Format::Hevc || hevc)
        .collect();
    }
    // sE pdf-formats
    card.choices.iter().map(|c| c.format).collect()
}

/// Pop the card's menu at `(x, y)`, in the stack page's CSS pixels.
pub fn popup(
    app: &AppHandle,
    window: &WebviewWindow,
    id: &JobId,
    x: f64,
    y: f64,
) -> Result<(), String> {
    // Copy the card first: building items waits on nothing, but keep no lock across the popup.
    let card = app
        .state::<AppState>()
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .map(|entry| entry.last.lock().unwrap().clone())
        .ok_or("this card's job is gone")?;
    let menu = build(app, &card).map_err(|e| e.to_string())?;
    // Stream H: Open With, Edit with, Upload, Add to Shelf.
    crate::integrations::menu::extend(app, &menu, &card).map_err(|e| e.to_string())?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn build(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Menu<Wry>> {
    // stream B: audio
    if card.audio.is_some() {
        return build_audio(app, card);
    }
    let item = |action: Action, label: String, accelerator: Option<&str>| {
        MenuItem::with_id(
            app,
            item_id(&card.id, action),
            label,
            enabled(card, action),
            accelerator,
        )
    };
    let submenu = |label: String, items: &[MenuItem<Wry>]| {
        let refs: Vec<&dyn IsMenuItem<Wry>> =
            items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
        Submenu::with_items(app, label, card.can_edit, &refs)
    };
    let reveal = if cfg!(target_os = "macos") {
        "card.reveal.mac"
    } else {
        "card.reveal.windows"
    };
    let scales = (1..=9)
        .rev()
        .map(|n| {
            let percent = n * 10;
            let label = tf("menu.percent", &[("n", &percent.to_string())]);
            item(Action::Scale(percent), label, None)
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    // sE pdf-formats: a PDF has DPI stops where an image has scales
    let pdf = card.source_format == Some(Format::Pdf);
    let dpis = ferry_core::pdf::DPI_STOPS
        .iter()
        .map(|&n| {
            item(
                Action::Dpi(n),
                tf("pdf.dpi.label", &[("n", &n.to_string())]),
                None,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let downscale = if pdf {
        submenu(t("pdf.menu.dpi"), &dpis)?
    } else {
        submenu(t("menu.downscale"), &scales)?
    };
    let formats = convert_formats(card);
    let converts = formats
        .iter()
        .map(|&f| item(Action::Convert(f), f.label().into(), None))
        .collect::<tauri::Result<Vec<_>>>()?;
    let twins = formats
        .iter()
        .map(|&f| item(Action::KeepBoth(f), f.label().into(), None))
        .collect::<tauri::Result<Vec<_>>>()?;

    let pipe = crate::pipe_menu::Items::build(app, card)?; // stream F: pipelines
    let menu = Menu::with_items(
        app,
        &[
            &item(Action::Open, t("menu.open"), None)?,
            &item(Action::Reveal, t(reveal), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Copy, t("menu.copy"), None)?,
            &item(Action::CopyPath, t("menu.copy_path"), None)?,
            &item(Action::Rename, t("menu.rename"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(
                Action::Compare,
                t("card.compare"),
                crate::hotkeys::hover_accelerator(app, "compare").as_deref(),
            )?,
            &item(
                Action::Preview,
                t("menu.preview"),
                crate::hotkeys::hover_accelerator(app, "preview").as_deref(),
            )?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Rerun, t("card.reoptimise"), None)?,
            &item(Action::Aggressive, t("card.aggressive"), None)?,
            &item(Action::Fit, t("menu.fit"), None)?,
            &pipe.encoder, // stream F: pipelines
            &PredefinedMenuItem::separator(app)?,
            &downscale,
            &item(Action::Crop, t("card.crop"), None)?,
            &submenu(t("menu.convert"), &converts)?,
            &submenu(t("menu.keep_both"), &twins)?,
            &pipe.strip, // stream F: pipelines
            &PredefinedMenuItem::separator(app)?,
            &pipe.run,      // stream F: pipelines
            &pipe.producer, // stream F: pipelines
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Restore, t("card.restore"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Close, t("card.close"), None)?,
        ],
    )?;
    // sE pdf-formats (right after Crop)
    if card.source_format == Some(Format::Pdf) {
        let crop = item_id(&card.id, Action::Crop);
        if let Some(at) = menu.items()?.iter().position(|i| i.id().as_ref() == crop) {
            let extract = item(Action::Pages, t("pdf.menu.extract"), None)?;
            menu.insert(&extract, at + 1)?;
        }
    }
    // stream A: video (right after Crop)
    if card.video.is_some() {
        let mute = if card.edits.mute {
            "video.menu.unmute"
        } else {
            "video.menu.mute"
        };
        let speed = item(Action::Speed, t("video.menu.speed"), None)?;
        let mute = item(Action::Mute, t(mute), None)?;
        menu.insert_items(&[&speed, &mute], 17)?; // stream F: pipelines (the encoder item above shifts Crop by one)
    }
    Ok(menu)
}

// ---- stream B: audio ----
fn refs(items: &[MenuItem<Wry>]) -> Vec<&dyn IsMenuItem<Wry>> {
    items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect()
}

/// An audio card's menu: no Compare, Downscale or Crop; loudness and cover art instead.
fn build_audio(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Menu<Wry>> {
    let pipe = crate::pipe_menu::Items::build(app, card)?; // stream F: pipelines
    let item = |action: Action, label: String| {
        MenuItem::with_id(
            app,
            item_id(&card.id, action),
            label,
            enabled(card, action),
            None::<&str>,
        )
    };
    let sub = |label: String, items: &[&dyn IsMenuItem<Wry>]| {
        Submenu::with_items(app, label, card.can_edit, items)
    };
    let reveal = if cfg!(target_os = "macos") {
        "card.reveal.mac"
    } else {
        "card.reveal.windows"
    };
    let formats = convert_formats(card);
    let converts = formats
        .iter()
        .map(|&f| item(Action::Convert(f), f.label().into()))
        .collect::<tauri::Result<Vec<_>>>()?;
    let twins = formats
        .iter()
        .map(|&f| item(Action::KeepBoth(f), f.label().into()))
        .collect::<tauri::Result<Vec<_>>>()?;
    let loudness = LOUDNESS
        .iter()
        .map(|&l| {
            item(
                Action::Loudness(l),
                t(&format!("audio.loudness.{}", serde_key(&l))),
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let squares = COVER_SQUARES
        .iter()
        .map(|&n| {
            let label = tf("audio.cover.square", &[("n", &n.to_string())]);
            item(Action::CoverSquare(n), label)
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let keep = item(Action::Cover(CoverPolicy::Keep), t("audio.cover.keep"))?;
    let optimise = item(
        Action::Cover(CoverPolicy::Optimise),
        t("audio.cover.optimise"),
    )?;
    let remove = item(Action::Cover(CoverPolicy::Remove), t("audio.cover.remove"))?;
    let extract = item(Action::ExtractCover, t("audio.cover.extract"))?;
    let square_refs: Vec<&dyn IsMenuItem<Wry>> =
        squares.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let downscale = sub(t("audio.cover.downscale"), &square_refs)?;
    let rule = PredefinedMenuItem::separator(app)?;
    let cover = sub(
        t("audio.menu.cover"),
        &[&keep, &optimise, &downscale, &remove, &rule, &extract],
    )?;
    Menu::with_items(
        app,
        &[
            &item(Action::Open, t("menu.open"))?,
            &item(Action::Reveal, t(reveal))?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Copy, t("menu.copy"))?,
            &item(Action::CopyPath, t("menu.copy_path"))?,
            &item(Action::Rename, t("menu.rename"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Preview, t("menu.preview"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Rerun, t("card.reoptimise"))?,
            &item(Action::Aggressive, t("card.aggressive"))?,
            &item(Action::Fit, t("menu.fit"))?,
            &PredefinedMenuItem::separator(app)?,
            &sub(t("menu.convert"), &refs(&converts))?,
            &sub(t("menu.keep_both"), &refs(&twins))?,
            &PredefinedMenuItem::separator(app)?,
            &sub(t("audio.menu.loudness"), &refs(&loudness))?,
            &cover,
            &PredefinedMenuItem::separator(app)?,
            &pipe.run,      // stream F: pipelines
            &pipe.producer, // stream F: pipelines
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Restore, t("card.restore"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Close, t("card.close"))?,
        ],
    )
}

/// Route card menu items. Other menus' items (the tray's) are not ours.
pub fn init(app: &AppHandle) {
    app.on_menu_event(|app, event| {
        if let Some((id, action)) = parse_id(event.id().as_ref()) {
            run(app, id, action);
        }
    });
}

#[derive(Clone, Serialize)]
struct CardAction {
    id: JobId,
    action: &'static str,
}

/// Hand an action that ends in the page to the stack (`card:action`).
pub fn send(app: &AppHandle, id: JobId, action: &'static str) {
    let _ = app.emit_to(STACK, "card:action", CardAction { id, action });
}

fn run(app: &AppHandle, id: JobId, action: Action) {
    if let Some(page) = action.page() {
        send(app, id, page);
        return;
    }
    // Menu events arrive on the main thread; file and clipboard work does not belong there.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let done = match action {
            Action::Open | Action::Reveal => jobs::output(&app, &id)
                .ok_or_else(|| "the file is gone".to_string())
                .and_then(|path| {
                    if action == Action::Open {
                        tauri_plugin_opener::open_path(path, None::<&str>)
                    } else {
                        tauri_plugin_opener::reveal_item_in_dir(path)
                    }
                    .map_err(|e| e.to_string())
                }),
            Action::Copy => jobs::copy(&app, &id),
            Action::CopyPath => jobs::copy_path(&app, &id),
            Action::Rerun => jobs::rerun(&app, &id, false),
            Action::Aggressive => jobs::rerun(&app, &id, true),
            Action::Scale(percent) => jobs::edit(
                &app,
                &id,
                ferry_core::edit::EditsPatch {
                    scale: Some(Some(percent)),
                    ..Default::default()
                },
            ),
            Action::Dpi(n) => jobs::edit(
                &app,
                &id,
                ferry_core::edit::EditsPatch {
                    dpi: Some(Some(n)),
                    ..Default::default()
                },
            ), // sE pdf-formats
            Action::Convert(format) => jobs::convert(&app, &id, format, false),
            Action::KeepBoth(format) => jobs::convert(&app, &id, format, true),
            Action::Restore => jobs::restore(&app, &id),
            // stream A: video
            Action::Mute => jobs::toggle_mute(&app, &id),
            // stream B: audio
            Action::Loudness(l) => jobs::edit(
                &app,
                &id,
                ferry_core::edit::EditsPatch {
                    loudness: Some(Some(l)),
                    ..Default::default()
                },
            ),
            Action::Cover(c) => jobs::edit(
                &app,
                &id,
                ferry_core::edit::EditsPatch {
                    cover: Some(Some(c)),
                    ..Default::default()
                },
            ),
            Action::CoverSquare(n) => jobs::edit(
                &app,
                &id,
                ferry_core::edit::EditsPatch {
                    size: Some(Some(ferry_core::edit::Size::Exact {
                        width: n,
                        height: n,
                    })),
                    framing: Some(ferry_core::edit::Framing::Centre),
                    ..Default::default()
                },
            ),
            Action::ExtractCover => crate::audio::extract_cover(&app, &id).map(drop),
            // Stream H: integrations
            Action::Integration(item) => crate::integrations::menu::run(&app, &id, item),
            Action::Pipe(p) => crate::pipe_menu::run(&app, &id, p), // stream F: pipelines
            _ => Ok(()),
        };
        if let Err(e) = done {
            warn!(?action, "card menu: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::pipeline;

    /// A finished PNG result with its original backed up.
    fn done() -> JobUpdate {
        let mut card = pipeline::notice("x".into());
        card.id = job_id("18f-1").unwrap();
        card.state = JobState::Done;
        card.output = Some("/tmp/shot.png".into());
        card.format = Some(Format::Png);
        card.source_format = Some(Format::Png);
        card.can_edit = true;
        card.can_restore = true;
        card.choices = ferry_core::formats::choices(Format::Png, false, 1, false);
        card
    }

    const ALL: [Action; 20] = [
        Action::Open,
        Action::Reveal,
        Action::Copy,
        Action::CopyPath,
        Action::Rename,
        Action::Compare,
        Action::Preview,
        Action::Rerun,
        Action::Aggressive,
        Action::Fit,
        Action::Scale(50),
        Action::Crop,
        Action::Convert(Format::WebP),
        Action::KeepBoth(Format::Jpeg),
        Action::Restore,
        Action::Close,
        Action::Speed,
        Action::Mute,
        Action::Pages,
        Action::Dpi(150),
    ];

    #[test]
    fn every_item_id_reads_back_as_its_card_and_action() {
        let card = done();
        for action in ALL {
            assert_eq!(
                parse_id(&item_id(&card.id, action)),
                Some((card.id.clone(), action))
            );
        }
        assert_eq!(parse_id("recent:18f-1"), None);
        assert_eq!(parse_id("card:18f-1:explode"), None);
        assert_eq!(parse_id("card:18f-1:scale-x"), None);
    }

    #[test]
    fn a_finished_card_offers_everything_but_its_own_format_and_scale() {
        let card = done();
        for action in ALL {
            // Speed and Mute are for video cards only, Pages for PDFs.
            let video_only = matches!(
                action,
                Action::Speed | Action::Mute | Action::Pages | Action::Dpi(_)
            );
            assert_eq!(enabled(&card, action), !video_only, "{action:?}");
        }
        assert!(!enabled(&card, Action::Convert(Format::Png)));
        assert!(enabled(&card, Action::KeepBoth(Format::Png)));
        assert!(!enabled(&card, Action::Scale(100)));
    }

    #[test]
    fn without_the_original_only_file_actions_stay() {
        let mut card = done();
        card.can_edit = false;
        card.can_restore = false;
        let on: Vec<Action> = ALL.into_iter().filter(|&a| enabled(&card, a)).collect();
        assert_eq!(
            on,
            [
                Action::Open,
                Action::Reveal,
                Action::Copy,
                Action::CopyPath,
                Action::Rename,
                Action::Preview,
                Action::Close
            ]
        );
    }

    #[test]
    fn a_pdf_card_changes_dpi_but_not_to_its_current_one() {
        let mut card = done();
        card.source_format = Some(Format::Pdf);
        card.edits.dpi = Some(150);
        assert!(enabled(&card, Action::Dpi(72)));
        assert!(!enabled(&card, Action::Dpi(150)));
        assert!(!enabled(&done(), Action::Dpi(72)));
        assert_eq!(
            parse_id(&item_id(&card.id, Action::Dpi(72))),
            Some((card.id.clone(), Action::Dpi(72)))
        );
    }

    #[test]
    fn a_busy_card_offers_only_close() {
        let mut card = done();
        card.state = JobState::Queued;
        card.can_edit = false;
        let on: Vec<Action> = ALL.into_iter().filter(|&a| enabled(&card, a)).collect();
        assert_eq!(on, [Action::Close]);
    }

    // sE pdf-formats
    #[test]
    fn a_pdf_card_offers_extract_and_converts_only_to_images() {
        let mut card = done();
        card.source_format = Some(Format::Pdf);
        card.format = Some(Format::Pdf);
        card.choices = ferry_core::formats::choices(Format::Pdf, false, 3, false);
        assert!(enabled(&card, Action::Pages));
        assert_eq!(
            convert_formats(&card),
            [Format::Pdf, Format::Png, Format::Jpeg]
        );
        assert_eq!(
            parse_id(&item_id(&card.id, Action::Pages)),
            Some((card.id.clone(), Action::Pages))
        );
        assert_eq!(Action::Pages.page(), Some("pages"));
        assert!(!enabled(&done(), Action::Pages));
    }

    // stream A: video
    /// A finished MP4 result with its original backed up.
    fn video_card() -> JobUpdate {
        let mut card = done();
        card.format = Some(Format::Mp4);
        card.source_format = Some(Format::Mp4);
        card.output = Some("/tmp/clip.mp4".into());
        card.video = Some(ferry_core::job::VideoMeta {
            duration_ms: 5000,
            codec: "H.264".into(),
            fps: Some(30.0),
            audio: true,
            original: Some("/b/clip.mp4".into()),
            original_duration_ms: 5000,
            hevc_encoder: true,
        });
        card
    }

    #[test]
    fn video_formats_get_distinct_menu_ids() {
        let card = video_card();
        for action in [
            Action::Convert(Format::Mp4),
            Action::Convert(Format::Hevc),
            Action::KeepBoth(Format::Av1),
            Action::Convert(Format::Gif),
            Action::Speed,
            Action::Mute,
        ] {
            assert_eq!(
                parse_id(&item_id(&card.id, action)),
                Some((card.id.clone(), action))
            );
        }
        assert_ne!(
            item_id(&card.id, Action::Convert(Format::Mp4)),
            item_id(&card.id, Action::Convert(Format::Hevc))
        );
    }

    #[test]
    fn a_video_card_converts_to_video_targets_and_gif() {
        assert_eq!(
            convert_formats(&video_card()),
            [
                Format::Mp4,
                Format::Hevc,
                Format::Av1,
                Format::WebM,
                Format::Gif
            ]
        );
        assert!(!enabled(&video_card(), Action::Convert(Format::Mp4)));
        assert!(enabled(&video_card(), Action::Convert(Format::Gif)));
    }

    #[test]
    fn without_a_hevc_encoder_hevc_is_not_offered() {
        let mut card = video_card();
        card.video.as_mut().unwrap().hevc_encoder = false;
        assert_eq!(
            convert_formats(&card),
            [Format::Mp4, Format::Av1, Format::WebM, Format::Gif]
        );
    }

    #[test]
    fn speed_and_mute_are_for_video_cards_with_their_original() {
        assert!(enabled(&video_card(), Action::Speed) && enabled(&video_card(), Action::Mute));
        assert!(!enabled(&done(), Action::Speed) && !enabled(&done(), Action::Mute));
        let mut card = video_card();
        card.can_edit = false;
        assert!(!enabled(&card, Action::Speed));
        assert_eq!(Action::Speed.page(), Some("speed"));
    }

    #[test]
    fn a_gif_card_can_convert_back_to_gif() {
        use ferry_core::formats::choices;
        let mut card = done();
        assert_eq!(
            convert_formats(&card),
            [Format::Png, Format::Jpeg, Format::WebP]
        );
        card.source_format = Some(Format::Gif);
        card.format = Some(Format::WebP);
        card.choices = choices(Format::Gif, true, 1, false);
        assert_eq!(
            convert_formats(&card),
            [Format::Gif, Format::Png, Format::Jpeg, Format::WebP]
        );
        assert!(enabled(&card, Action::Convert(Format::Gif)));
    }

    // sE pdf-formats
    #[test]
    fn a_format_the_card_does_not_offer_is_disabled() {
        assert!(!enabled(&done(), Action::Convert(Format::Pdf)));
    }

    // ---- stream B: audio ----
    /// A finished MP3 result with a cover, converted from a WAV.
    fn done_audio() -> JobUpdate {
        let mut card = done();
        card.format = Some(Format::Mp3);
        card.source_format = Some(Format::Wav);
        card.output = Some("/tmp/song.mp3".into());
        card.audio = Some(ferry_core::audio::AudioCard {
            bitrate_before: None,
            bitrate_after: Some(192),
            lossless: false,
            has_cover: true,
        });
        card
    }

    #[test]
    fn audio_item_ids_read_back() {
        let card = done_audio();
        for a in [
            Action::Loudness(Loudness::AppleMusic),
            Action::Cover(CoverPolicy::Remove),
            Action::CoverSquare(600),
            Action::ExtractCover,
            Action::Convert(Format::Opus),
            Action::KeepBoth(Format::Wav),
        ] {
            assert_eq!(parse_id(&item_id(&card.id, a)), Some((card.id.clone(), a)));
        }
        assert_eq!(parse_id("card:18f-1:loudness-loud"), None);
    }

    #[test]
    fn an_audio_card_offers_audio_tools_and_no_compare() {
        let mut card = done_audio();
        assert!(!enabled(&card, Action::Compare));
        assert!(
            enabled(&card, Action::Loudness(Loudness::Streaming))
                && enabled(&card, Action::ExtractCover)
        );
        assert_eq!(
            convert_formats(&card),
            [Format::Mp3, Format::M4a, Format::Opus, Format::Wav]
        );
        assert!(!enabled(&card, Action::Convert(Format::Mp3)));
        card.edits.loudness = Some(Loudness::Streaming);
        assert!(!enabled(&card, Action::Loudness(Loudness::Streaming)));
        card.audio = Some(ferry_core::audio::AudioCard {
            has_cover: false,
            ..card.audio.clone().unwrap()
        });
        assert!(
            !enabled(&card, Action::Cover(CoverPolicy::Optimise))
                && !enabled(&card, Action::CoverSquare(600))
                && !enabled(&card, Action::ExtractCover)
        );
    }
}
