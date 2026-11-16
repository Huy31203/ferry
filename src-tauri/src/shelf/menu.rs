//! The shelf's native menu (spec §6), routed like `rail_menu.rs`.

use std::path::PathBuf;

use ferry_core::i18n::{t, tf};
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Wry};

use super::{SHELF, mutate, state};
use crate::platform;
use crate::state::AppState;

const PREFIX: &str = "shelf:";
const REMOVE: &str = "shelf:remove:";

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Header(String),
    Reveal,
    Copy,
    Paste,
    OptimiseAll,
    Archive,
    StopArchive,
    ShowItems,
    Clear,
    Separator,
}

pub fn entries(count: usize, size: u64, busy: bool) -> Vec<Entry> {
    if count == 0 {
        return vec![Entry::Paste];
    }
    let size = super::format_size(size);
    let header = if count == 1 {
        tf("shelf.menu.header_one", &[("size", &size)])
    } else {
        tf(
            "shelf.menu.header",
            &[("count", &count.to_string()), ("size", &size)],
        )
    };
    vec![
        Entry::Header(header),
        Entry::Reveal,
        Entry::Separator,
        Entry::Copy,
        Entry::Paste,
        Entry::Separator,
        Entry::OptimiseAll,
        if busy {
            Entry::StopArchive
        } else {
            Entry::Archive
        },
        Entry::ShowItems,
        Entry::Separator,
        Entry::Clear,
    ]
}

fn id(entry: &Entry) -> &'static str {
    match entry {
        Entry::Header(_) => "shelf:header",
        Entry::Reveal => "shelf:reveal",
        Entry::Copy => "shelf:copy",
        Entry::Paste => "shelf:paste",
        Entry::OptimiseAll => "shelf:optimise_all",
        Entry::Archive => "shelf:archive",
        Entry::StopArchive => "shelf:archive:stop",
        Entry::ShowItems => "shelf:show_items",
        Entry::Clear => "shelf:clear",
        Entry::Separator => "",
    }
}

fn label(entry: &Entry) -> String {
    match entry {
        Entry::Header(h) => h.clone(),
        Entry::Reveal => t("shelf.menu.reveal"),
        Entry::Copy => t("shelf.menu.copy"),
        Entry::Paste => t("shelf.menu.paste"),
        Entry::OptimiseAll => t("shelf.menu.optimise_all"),
        Entry::Archive => t("shelf.menu.archive"),
        Entry::StopArchive => t("shelf.menu.archive_stop"),
        Entry::ShowItems => t("shelf.menu.show_items"),
        Entry::Clear => t("shelf.menu.clear"),
        Entry::Separator => String::new(),
    }
}

pub fn popup(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    let view = super::view(app);
    let list = entries(view.items.len(), view.size, super::archive::busy());
    let mut owned: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for entry in &list {
        let item: Box<dyn IsMenuItem<Wry>> = match entry {
            Entry::Separator => {
                Box::new(PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?)
            }
            Entry::Archive => Box::new(archive_submenu(app)?),
            other => Box::new(
                MenuItem::with_id(
                    app,
                    id(other),
                    label(other),
                    !matches!(other, Entry::Header(_)),
                    None::<&str>,
                )
                .map_err(|e| e.to_string())?,
            ),
        };
        owned.push(item);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(|b| &**b).collect();
    let menu = Menu::with_items(app, &refs).map_err(|e| e.to_string())?;
    let window = app.get_webview_window(SHELF).ok_or("no shelf window")?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn archive_submenu(app: &AppHandle) -> Result<Submenu<Wry>, String> {
    use super::archive::{Preset, cores, estimate, eta_label, items};
    let items = items(app);
    let quick = |preset: Preset, id: &str, key: &str| {
        let eta = eta_label(estimate(app, &items, preset.level(), cores()));
        MenuItem::with_id(app, id, tf(key, &[("eta", &eta)]), true, None::<&str>)
            .map_err(|e| e.to_string())
    };
    let fast = quick(Preset::Fast, "shelf:archive:fast", "shelf.archive.fast")?;
    let balanced = quick(
        Preset::Balanced,
        "shelf:archive:balanced",
        "shelf.archive.balanced",
    )?;
    let smallest = quick(
        Preset::Smallest,
        "shelf:archive:smallest",
        "shelf.archive.smallest",
    )?;
    let sep = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let advanced = MenuItem::with_id(
        app,
        "shelf:archive:advanced",
        t("shelf.archive.advanced"),
        true,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    Submenu::with_id_and_items(
        app,
        "shelf:archive",
        t("shelf.menu.archive"),
        true,
        &[&fast, &balanced, &smallest, &sep, &advanced],
    )
    .map_err(|e| e.to_string())
}

pub fn popup_item(app: &AppHandle, path: &str, x: f64, y: f64) -> Result<(), String> {
    let remove = MenuItem::with_id(
        app,
        format!("{REMOVE}{path}"),
        t("shelf.menu.remove"),
        true,
        None::<&str>,
    )
    .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&remove]).map_err(|e| e.to_string())?;
    let window = app.get_webview_window(SHELF).ok_or("no shelf window")?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

pub fn init(app: &AppHandle) {
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if !id.starts_with(PREFIX) {
            return;
        }
        let (app, id) = (app.clone(), id.to_string());
        // Menu events arrive on the main thread; file and clipboard work does not belong there.
        tauri::async_runtime::spawn_blocking(move || run(&app, &id));
    });
}

fn run(app: &AppHandle, id: &str) {
    if let Some(path) = id.strip_prefix(REMOVE) {
        mutate(app, |list| list.remove(&[PathBuf::from(path)]));
        return;
    }
    let paths = state(app)
        .map(|s| s.list.lock().unwrap().paths())
        .unwrap_or_default();
    match id {
        "shelf:reveal" => {
            let _ = tauri_plugin_opener::reveal_items_in_dir(&paths);
        }
        "shelf:copy" => copy(app, paths),
        "shelf:paste" => {
            if let Ok(snap) = platform::clipboard::read() {
                super::add(app, snap.files);
            }
        }
        "shelf:optimise_all" => super::optimise_all(app),
        "shelf:archive:fast" => super::archive::quick(app, super::archive::Preset::Fast),
        "shelf:archive:balanced" => super::archive::quick(app, super::archive::Preset::Balanced),
        "shelf:archive:smallest" => super::archive::quick(app, super::archive::Preset::Smallest),
        "shelf:archive:stop" => super::archive::cancel(),
        "shelf:archive:advanced" => {}
        "shelf:show_items" => {
            let _ = app.emit_to(SHELF, "shelf:show_items", ());
        }
        "shelf:clear" => super::clear(app),
        _ => {}
    }
}

/// As `drop_batch/list.rs`: recorded as our own write so the watcher leaves it alone.
fn copy(app: &AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<AppState>();
    let mut log = state.clipboard.lock().unwrap();
    if let Ok(seq) = platform::clipboard::write_files(&paths) {
        log.wrote(seq, None, ferry_core::clip::ClipWrite::Files(paths));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_shelf_lists_every_action_in_order() {
        let e = entries(8, 214_000_000, false);
        assert!(matches!(&e[0], Entry::Header(h) if h.starts_with("8 Items · ")));
        assert_eq!(
            &e[1..],
            &[
                Entry::Reveal,
                Entry::Separator,
                Entry::Copy,
                Entry::Paste,
                Entry::Separator,
                Entry::OptimiseAll,
                Entry::Archive,
                Entry::ShowItems,
                Entry::Separator,
                Entry::Clear,
            ]
        );
    }

    #[test]
    fn an_empty_shelf_offers_only_paste() {
        assert_eq!(entries(0, 0, false), vec![Entry::Paste]);
    }

    #[test]
    fn while_archiving_the_archive_entry_becomes_stop() {
        let e = entries(2, 10, true);
        assert!(e.contains(&Entry::StopArchive));
        assert!(!e.contains(&Entry::Archive));
    }
}
