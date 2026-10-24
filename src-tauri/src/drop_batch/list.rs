//! The compact results list: its settings, and the commands of its selection
//! bar. No list key is handled here: the keys are DOM key events in the stack
//! page, which only has the keyboard after a click on the list.

use std::sync::Mutex;

use ferry_core::clip::ClipWrite;
use ferry_core::edit::EditsPatch;
use ferry_core::i18n::t;
use ferry_core::job::{JobId, JobUpdate};
use ferry_core::optimise::Level;
use ferry_core::sniff::Format;
use ferry_core::video;
use serde::Serialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem};
use tauri::{AppHandle, LogicalPosition, Manager, WebviewWindow, Wry};
use tracing::warn;

use crate::state::AppState;
use crate::{compare, jobs, menu, platform};

const PREFIX: &str = "list:";

#[derive(Debug, Serialize)]
pub struct ListConfig {
    pub after_cards: u32,
}

/// Where the stack switches from cards to the list.
#[tauri::command]
pub fn list_config(app: AppHandle) -> ListConfig {
    ListConfig {
        after_cards: app
            .state::<AppState>()
            .settings
            .read()
            .unwrap()
            .results_list
            .after_cards,
    }
}

/// What a selection-bar menu item does to the selected cards.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ListItem {
    /// Downscale to this percent.
    Scale(u8),
    /// Compression quality.
    Quality(u8),
    Extreme,
    Convert(Format),
}

const FORMATS: [Format; 10] = [
    Format::Png,
    Format::Jpeg,
    Format::Gif,
    Format::WebP,
    Format::Mp4,
    Format::Hevc,
    Format::Av1,
    Format::WebM,
    Format::Mov,
    Format::Mkv,
];

fn format_key(f: Format) -> &'static str {
    if f.is_video() {
        video::format_id(f)
    } else {
        f.extension()
    }
}

pub fn list_item_id(item: ListItem) -> String {
    match item {
        ListItem::Scale(p) => format!("{PREFIX}scale-{p}"),
        ListItem::Quality(q) => format!("{PREFIX}quality-{q}"),
        ListItem::Extreme => format!("{PREFIX}extreme"),
        ListItem::Convert(f) => format!("{PREFIX}convert-{}", format_key(f)),
    }
}

pub fn parse_list_item(id: &str) -> Option<ListItem> {
    let rest = id.strip_prefix(PREFIX)?;
    if rest == "extreme" {
        return Some(ListItem::Extreme);
    }
    let (kind, value) = rest.split_once('-')?;
    match kind {
        "scale" => value.parse().ok().map(ListItem::Scale),
        "quality" => value.parse().ok().map(ListItem::Quality),
        "convert" => FORMATS
            .into_iter()
            .find(|f| format_key(*f) == value)
            .map(ListItem::Convert),
        _ => None,
    }
}

/// The formats some editable selected card can take; a format is enabled when
/// some editable card is not already in it.
pub fn selection_formats(cards: &[JobUpdate]) -> Vec<(Format, bool)> {
    let mut out: Vec<(Format, bool)> = Vec::new();
    for card in cards.iter().filter(|c| c.can_edit) {
        for f in menu::convert_formats(card) {
            let takes = card.format != Some(f);
            match out.iter_mut().find(|(have, _)| *have == f) {
                Some((_, enabled)) => *enabled |= takes,
                None => out.push((f, takes)),
            }
        }
    }
    out
}

/// The edit a menu item asks for; conversions go through `jobs::convert` instead.
pub fn patch_for(item: ListItem) -> Option<EditsPatch> {
    Some(match item {
        ListItem::Scale(p) => EditsPatch {
            scale: Some(Some(p)),
            ..EditsPatch::default()
        },
        ListItem::Quality(q) => EditsPatch {
            compression: Some(Some(q)),
            level: Some(Level::Normal),
            ..EditsPatch::default()
        },
        ListItem::Extreme => EditsPatch {
            compression: Some(Some(5)),
            level: Some(Level::Aggressive),
            ..EditsPatch::default()
        },
        ListItem::Convert(_) => return None,
    })
}

/// The cards the last popped-up menu acts on.
static LAST: Mutex<Vec<JobId>> = Mutex::new(Vec::new());

/// Pop up the selection bar's Downscale, Compression or Convert menu at `(x, y)`.
#[tauri::command]
pub fn list_menu(
    app: AppHandle,
    window: WebviewWindow,
    ids: Vec<JobId>,
    kind: String,
    x: f64,
    y: f64,
) -> Result<(), String> {
    let cards: Vec<JobUpdate> = ids
        .iter()
        .filter_map(|id| compare::card(&app, id).ok())
        .collect();
    let item = |it: ListItem, label: String, enabled: bool| {
        MenuItem::with_id(&app, list_item_id(it), label, enabled, None::<&str>)
    };
    let mut items: Vec<MenuItem<Wry>> = Vec::new();
    let built: tauri::Result<()> = (|| {
        match kind.as_str() {
            "downscale" => {
                for n in (1..=9).rev() {
                    let p = n * 10;
                    let label = ferry_core::i18n::tf("menu.percent", &[("n", &p.to_string())]);
                    items.push(item(ListItem::Scale(p), label, true)?);
                }
            }
            "compression" => {
                for n in (1..=9).rev() {
                    let q = n * 10;
                    let label = ferry_core::i18n::tf("drop.list.quality", &[("n", &q.to_string())]);
                    items.push(item(ListItem::Quality(q), label, true)?);
                }
                items.push(item(ListItem::Extreme, t("slider.extreme"), true)?);
            }
            "convert" => {
                for (f, enabled) in selection_formats(&cards) {
                    items.push(item(ListItem::Convert(f), f.label().into(), enabled)?);
                }
            }
            _ => {}
        }
        Ok(())
    })();
    built.map_err(|e| e.to_string())?;
    *LAST.lock().unwrap() = ids;
    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    let menu = Menu::with_items(&app, &refs).map_err(|e| e.to_string())?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

/// The selection menu's items; the rest of the menus are routed elsewhere.
pub(super) fn on_menu(app: &AppHandle, id: &str) {
    let Some(item) = parse_list_item(id) else {
        return;
    };
    let ids = LAST.lock().unwrap().clone();
    let app = app.clone();
    // Menu events arrive on the main thread; edits do not belong there.
    tauri::async_runtime::spawn_blocking(move || {
        for id in ids {
            let done = match (item, patch_for(item)) {
                (ListItem::Convert(f), _) => {
                    let same = compare::card(&app, &id).is_ok_and(|c| c.format == Some(f));
                    if same {
                        continue;
                    }
                    jobs::convert(&app, &id, f, false)
                }
                (_, Some(patch)) => jobs::edit(&app, &id, patch),
                _ => continue,
            };
            if let Err(e) = done {
                warn!(?item, "list menu: {e}");
            }
        }
    });
}

/// Apply one edit to every selected card (the crop popover); the number that took it.
#[tauri::command]
pub async fn list_edit(app: AppHandle, ids: Vec<JobId>, edits: EditsPatch) -> Result<u32, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let mut done = 0;
        for id in ids {
            match jobs::edit(&app, &id, edits.clone()) {
                Ok(()) => done += 1,
                Err(e) => warn!("list edit: {e}"),
            }
        }
        done
    })
    .await
    .map_err(|e| e.to_string())
}

/// Put the selected cards' files on the clipboard, as Finder's Copy does.
#[tauri::command]
pub async fn list_copy(app: AppHandle, ids: Vec<JobId>) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || {
        let paths: Vec<_> = ids.iter().filter_map(|id| jobs::output(&app, id)).collect();
        if paths.is_empty() {
            return Err(t("compare.file_gone"));
        }
        let state = app.state::<AppState>();
        let mut log = state.clipboard.lock().unwrap();
        let seq = platform::clipboard::write_files(&paths)?;
        log.wrote(seq, None, ClipWrite::Files(paths));
        Ok(())
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Debug, Serialize)]
pub struct SaveReport {
    pub copied: u32,
    pub failed: u32,
}

/// Copy the selected files into a folder the user picks. Never moves or overwrites.
#[tauri::command]
pub async fn list_save_to(app: AppHandle, ids: Vec<JobId>) -> Result<Option<SaveReport>, String> {
    tauri::async_runtime::spawn_blocking(move || {
        use tauri_plugin_dialog::DialogExt;
        let picked = app
            .dialog()
            .file()
            .set_title(t("drop.list.save_to.title"))
            .blocking_pick_folder();
        let folder = picked.and_then(|p| p.into_path().ok())?;
        let mut report = SaveReport {
            copied: 0,
            failed: 0,
        };
        for from in ids.iter().filter_map(|id| jobs::output(&app, id)) {
            let to = ferry_core::output::unique_except(
                &folder.join(from.file_name().unwrap_or_default()),
                None,
                None,
            );
            match std::fs::copy(&from, &to) {
                Ok(_) => {
                    let _ = ferry_core::fsutil::copy_times(&from, &to);
                    report.copied += 1;
                }
                Err(e) => {
                    warn!(from = %from.display(), "could not save a copy: {e}");
                    report.failed += 1;
                }
            }
        }
        Some(report)
    })
    .await
    .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::pipeline;

    fn editable(format: Format, source: Format) -> JobUpdate {
        let mut card = pipeline::notice(String::new());
        card.can_edit = true;
        card.format = Some(format);
        card.source_format = Some(source);
        card
    }

    #[test]
    fn every_list_menu_id_reads_back() {
        for item in [
            ListItem::Scale(50),
            ListItem::Quality(60),
            ListItem::Extreme,
            ListItem::Convert(Format::WebP),
        ] {
            assert_eq!(parse_list_item(&list_item_id(item)), Some(item));
        }
        assert_eq!(parse_list_item("card:x:open"), None);
    }

    #[test]
    fn selection_formats_offer_what_some_editable_card_can_take() {
        let png = editable(Format::Png, Format::Png);
        let jpeg = editable(Format::Jpeg, Format::Jpeg);
        assert_eq!(
            selection_formats(&[png.clone(), jpeg]),
            [
                (Format::Png, true),
                (Format::Jpeg, true),
                (Format::WebP, true)
            ]
        );
        assert_eq!(
            selection_formats(&[png.clone(), png.clone()]),
            [
                (Format::Png, false),
                (Format::Jpeg, true),
                (Format::WebP, true)
            ]
        );
        let gif = editable(Format::Gif, Format::Gif);
        assert_eq!(selection_formats(&[gif])[0], (Format::Gif, false));
        let mut locked = png;
        locked.can_edit = false;
        assert!(selection_formats(&[locked]).is_empty());
    }

    #[test]
    fn extreme_is_aggressive_at_quality_five() {
        assert_eq!(
            patch_for(ListItem::Extreme),
            Some(EditsPatch {
                compression: Some(Some(5)),
                level: Some(Level::Aggressive),
                ..EditsPatch::default()
            })
        );
        assert_eq!(patch_for(ListItem::Convert(Format::Png)), None);
    }
}
