//! The card's context menu (spec §4.5): built from the card's last update,
//! popped up under the ⋯ button or the pointer, and routed back by item id.
//! What ends in the page (rename, crop, fit, compare, preview, close) goes to
//! the stack as a `card:action` event; the rest runs here.

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
    Convert(Format),
    KeepBoth(Format),
    Restore,
    Close,
    // stream A: video
    Speed,
    Mute,
}

const FORMATS: [Format; 10] = [
    Format::Png,
    Format::Jpeg,
    Format::Gif,
    Format::WebP,
    // stream A: video
    Format::Mp4,
    Format::Hevc,
    Format::Av1,
    Format::WebM,
    Format::Mov,
    Format::Mkv,
];

/// The actions without a value, by their id part.
const NAMED: [(&str, Action); 15] = [
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
    ("restore", Action::Restore),
    ("close", Action::Close),
    // stream A: video
    ("speed", Action::Speed),
    ("mute", Action::Mute),
];

impl Action {
    /// The item id's last part: "open", "scale-50", "convert-webp", "keep_both-jpg".
    fn key(self) -> String {
        match self {
            Action::Scale(percent) => format!("scale-{percent}"),
            Action::Convert(format) => format!("convert-{}", format_key(format)),
            Action::KeepBoth(format) => format!("keep_both-{}", format_key(format)),
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
            Some(("convert", value)) => format(value).map(Action::Convert),
            Some(("keep_both", value)) => format(value).map(Action::KeepBoth),
            Some(_) => None,
            None => NAMED.iter().find(|(k, _)| *k == key).map(|(_, a)| *a),
        }
    }

    /// The `card:action` the page handles, for actions that end in the page.
    pub fn page(self) -> Option<&'static str> {
        Some(match self {
            Action::Rename => "rename",
            Action::Crop => "crop",
            Action::Fit => "fit",
            Action::Compare => "compare",
            Action::Preview => "preview",
            Action::Close => "close",
            // stream A: video
            Action::Speed => "speed",
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
        Action::Restore => idle && card.can_restore,
        Action::Convert(format) => card.can_edit && card.format != Some(format),
        Action::Scale(percent) => card.can_edit && card.edits.scale.unwrap_or(100) != percent,
        Action::Compare
        | Action::Rerun
        | Action::Aggressive
        | Action::Fit
        | Action::Crop
        | Action::KeepBoth(_) => card.can_edit,
        // stream A: video
        Action::Speed | Action::Mute => card.can_edit && card.video.is_some(),
    }
}

/// "Convert to ▸": PNG, JPEG, WebP, and GIF first on a card that started as a
/// GIF (like the format bar).
pub fn convert_formats(card: &JobUpdate) -> Vec<Format> {
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
    let gif = (card.source_format == Some(Format::Gif)).then_some(Format::Gif);
    gif.into_iter()
        .chain([Format::Png, Format::Jpeg, Format::WebP])
        .collect()
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
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn build(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Menu<Wry>> {
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
    let formats = convert_formats(card);
    let converts = formats
        .iter()
        .map(|&f| item(Action::Convert(f), f.label().into(), None))
        .collect::<tauri::Result<Vec<_>>>()?;
    let twins = formats
        .iter()
        .map(|&f| item(Action::KeepBoth(f), f.label().into(), None))
        .collect::<tauri::Result<Vec<_>>>()?;

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
            &item(Action::Compare, t("card.compare"), Some("CmdOrCtrl+D"))?,
            &item(Action::Preview, t("menu.preview"), Some("Space"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Rerun, t("card.reoptimise"), None)?,
            &item(Action::Aggressive, t("card.aggressive"), None)?,
            &item(Action::Fit, t("menu.fit"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &submenu(t("menu.downscale"), &scales)?,
            &item(Action::Crop, t("card.crop"), None)?,
            &submenu(t("menu.convert"), &converts)?,
            &submenu(t("menu.keep_both"), &twins)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Restore, t("card.restore"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Close, t("card.close"), None)?,
        ],
    )?;
    // stream A: video (right after Crop)
    if card.video.is_some() {
        let mute = if card.edits.mute {
            "video.menu.unmute"
        } else {
            "video.menu.mute"
        };
        let speed = item(Action::Speed, t("video.menu.speed"), None)?;
        let mute = item(Action::Mute, t(mute), None)?;
        menu.insert_items(&[&speed, &mute], 16)?;
    }
    Ok(menu)
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
            Action::Convert(format) => jobs::convert(&app, &id, format, false),
            Action::KeepBoth(format) => jobs::convert(&app, &id, format, true),
            Action::Restore => jobs::restore(&app, &id),
            // stream A: video
            Action::Mute => jobs::toggle_mute(&app, &id),
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
        card
    }

    const ALL: [Action; 18] = [
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
            // Speed and Mute are for video cards only.
            let video_only = matches!(action, Action::Speed | Action::Mute);
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
    fn a_busy_card_offers_only_close() {
        let mut card = done();
        card.state = JobState::Queued;
        card.can_edit = false;
        let on: Vec<Action> = ALL.into_iter().filter(|&a| enabled(&card, a)).collect();
        assert_eq!(on, [Action::Close]);
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
        let mut card = done();
        assert_eq!(
            convert_formats(&card),
            [Format::Png, Format::Jpeg, Format::WebP]
        );
        card.source_format = Some(Format::Gif);
        card.format = Some(Format::WebP);
        assert_eq!(
            convert_formats(&card),
            [Format::Gif, Format::Png, Format::Jpeg, Format::WebP]
        );
        assert!(enabled(&card, Action::Convert(Format::Gif)));
    }
}
