//! The integration items of the card menu: Open With ▸, Edit with, Add to
//! Shelf ▸ and Upload with. `menu.rs` builds the card menu; `extend` adds these
//! by item id so that file gains only a few named lines.

use std::collections::HashMap;
use std::sync::{LazyLock, Mutex};

use ferry_core::i18n::{t, tf};
use ferry_core::integrations::{ListedApp, MENU_APPS, media_kind, order_apps};
use ferry_core::job::{JobId, JobUpdate};
use ferry_core::settings::AppRef;
use tauri::image::Image;
use tauri::menu::{IconMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Manager, Wry};

use super::actions;
use crate::menu::{Action, enabled, item_id};
use crate::{jobs, platform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Item {
    Share,
    AirDrop,
    OpenWith(u8),
    EditWith,
    ChooseEditor(u8),
    Upload,
    Shelf(u8),
}

impl Item {
    /// The item id's part after `h-`.
    pub fn key(self) -> String {
        match self {
            Item::Share => "share".into(),
            Item::AirDrop => "airdrop".into(),
            Item::OpenWith(n) => format!("open_with.{n}"),
            Item::EditWith => "edit_with".into(),
            Item::ChooseEditor(n) => format!("choose_editor.{n}"),
            Item::Upload => "upload".into(),
            Item::Shelf(n) => format!("shelf.{n}"),
        }
    }

    pub fn parse(key: &str) -> Option<Self> {
        match key {
            "share" => return Some(Item::Share),
            "airdrop" => return Some(Item::AirDrop),
            "edit_with" => return Some(Item::EditWith),
            "upload" => return Some(Item::Upload),
            _ => {}
        }
        let (name, n) = key.split_once('.')?;
        let n: u8 = n.parse().ok().filter(|n| usize::from(*n) < MENU_APPS)?;
        match name {
            "open_with" => Some(Item::OpenWith(n)),
            "choose_editor" => Some(Item::ChooseEditor(n)),
            "shelf" => Some(Item::Shelf(n)),
            _ => None,
        }
    }
}

/// The apps the card's menu listed, so an item index means the same app when it
/// is clicked. Another card's index, or one past the end, opens nothing.
pub struct Listed {
    job: JobId,
    open_with: Vec<AppRef>,
    shelves: Vec<AppRef>,
}

impl Listed {
    pub fn open_with(&self, job: &JobId, n: u8) -> Option<&AppRef> {
        (self.job == *job).then(|| self.open_with.get(usize::from(n)))?
    }

    pub fn shelf(&self, job: &JobId, n: u8) -> Option<&AppRef> {
        (self.job == *job).then(|| self.shelves.get(usize::from(n)))?
    }
}

static LISTED: Mutex<Option<Listed>> = Mutex::new(None);
static ICONS: LazyLock<Mutex<HashMap<String, Option<Image<'static>>>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

fn icon(app: &AppRef) -> Option<Image<'static>> {
    ICONS
        .lock()
        .unwrap()
        .entry(app.id.clone())
        .or_insert_with(|| platform::apps::icon(app))
        .clone()
}

/// Where the integration groups go: just after Open, and just before Restore.
pub fn insert_points(ids: &[String], open: &str, restore: &str) -> Option<(usize, usize)> {
    let open = ids.iter().position(|id| id == open)?;
    let restore = ids.iter().position(|id| id == restore)?;
    Some((open + 1, restore))
}

/// Add the integration items to the card's built menu.
pub fn extend(app: &AppHandle, menu: &Menu<Wry>, card: &JobUpdate) -> tauri::Result<()> {
    let ids: Vec<String> = menu
        .items()?
        .iter()
        .map(|item| item.id().as_ref().to_string())
        .collect();
    let Some((open_at, restore_at)) = insert_points(
        &ids,
        &item_id(&card.id, Action::Open),
        &item_id(&card.id, Action::Restore),
    ) else {
        return Ok(());
    };
    let item = |item: Item, label: String, accelerator: Option<&str>| {
        let action = Action::Integration(item);
        MenuItem::with_id(
            app,
            item_id(&card.id, action),
            label,
            enabled(card, action),
            accelerator,
        )
    };
    let submenu = |first: Item, label: String, items: &[&dyn IsMenuItem<Wry>]| {
        Submenu::with_items(app, label, enabled(card, Action::Integration(first)), items)
    };
    let app_items = |apps: &[AppRef], make: &dyn Fn(u8) -> Item| {
        apps.iter()
            .zip(0u8..)
            .map(|(with, n)| {
                let action = Action::Integration(make(n));
                IconMenuItem::with_id(
                    app,
                    item_id(&card.id, action),
                    &with.name,
                    enabled(card, action),
                    icon(with),
                    None::<&str>,
                )
            })
            .collect::<tauri::Result<Vec<_>>>()
    };

    let output = card.output.clone().unwrap_or_default();
    let mut listed = order_apps(
        platform::apps::list(&output),
        &actions::own_id(app),
        MENU_APPS,
    );
    listed.truncate(MENU_APPS);
    let apps: Vec<AppRef> = listed.iter().map(|l: &ListedApp| l.app.clone()).collect();
    let shelves: Vec<AppRef> = actions::shelves(app).into_iter().take(MENU_APPS).collect();

    // Open group: Open With ▸, Edit with.
    let open_items = app_items(&apps, &Item::OpenWith)?;
    for (entry, listed) in open_items.iter().zip(&listed) {
        if listed.default {
            entry.set_text(tf("share.menu.default_app", &[("app", &listed.app.name)]))?;
        }
    }
    let none = |first: Item| {
        let none = item(first, t("share.menu.no_apps"), None)?;
        none.set_enabled(false)?;
        tauri::Result::Ok(none)
    };
    let no_open = none(Item::OpenWith(0))?;
    let open_refs: Vec<&dyn IsMenuItem<Wry>> = if open_items.is_empty() {
        vec![&no_open]
    } else {
        open_items
            .iter()
            .map(|i| i as &dyn IsMenuItem<Wry>)
            .collect()
    };
    let open_with = submenu(Item::OpenWith(0), t("share.menu.open_with"), &open_refs)?;

    let editor = card
        .output
        .as_ref()
        .and_then(|path| media_kind(&path.extension()?.to_string_lossy()))
        .and_then(|kind| {
            let state = app.state::<crate::state::AppState>();
            let settings = state.settings.read().unwrap();
            settings.share.edit_with.get(kind).cloned()
        });
    let choosers = app_items(&apps, &Item::ChooseEditor)?;
    let no_choice = none(Item::ChooseEditor(0))?;
    let chooser_refs: Vec<&dyn IsMenuItem<Wry>> = if choosers.is_empty() {
        vec![&no_choice]
    } else {
        choosers.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect()
    };
    let edit_with: Box<dyn IsMenuItem<Wry>> = match &editor {
        Some(editor) => Box::new(item(
            Item::EditWith,
            tf("share.menu.edit_with", &[("app", &editor.name)]),
            Some("CmdOrCtrl+E"),
        )?),
        None => Box::new(submenu(
            Item::ChooseEditor(0),
            t("share.menu.edit_with_choose"),
            &chooser_refs,
        )?),
    };

    // Share group: Share ▸ (macOS) or Share or Nearby Share… (Windows), Add to Shelf ▸, Upload with, then a rule.
    let share: Box<dyn IsMenuItem<Wry>> = if cfg!(target_os = "macos") {
        let airdrop = item(Item::AirDrop, t("share.menu.airdrop"), None)?;
        let more = item(Item::Share, t("share.menu.more"), None)?;
        Box::new(submenu(
            Item::Share,
            t("share.menu.share"),
            &[&airdrop, &more],
        )?)
    } else {
        Box::new(item(Item::Share, t("share.menu.share_windows"), None)?)
    };
    let shelf_items = app_items(&shelves, &Item::Shelf)?;
    let shelf_refs: Vec<&dyn IsMenuItem<Wry>> = shelf_items
        .iter()
        .map(|i| i as &dyn IsMenuItem<Wry>)
        .collect();
    let shelf = (!shelves.is_empty())
        .then(|| submenu(Item::Shelf(0), t("share.menu.shelf"), &shelf_refs))
        .transpose()?;
    let upload = actions::uploader(app)
        .map(|uploader| {
            item(
                Item::Upload,
                tf("share.menu.upload", &[("app", &uploader.name)]),
                Some("CmdOrCtrl+U"),
            )
        })
        .transpose()?;
    let rule = PredefinedMenuItem::separator(app)?;
    let mut share_group: Vec<&dyn IsMenuItem<Wry>> = vec![&*share];
    if let Some(shelf) = &shelf {
        share_group.push(shelf);
    }
    if let Some(upload) = &upload {
        share_group.push(upload);
    }
    share_group.push(&rule);
    menu.insert_items(&share_group, restore_at)?;
    menu.insert_items(&[&open_with, &*edit_with], open_at)?;

    *LISTED.lock().unwrap() = Some(Listed {
        job: card.id.clone(),
        open_with: apps,
        shelves,
    });
    Ok(())
}

/// The app behind a menu index (an Open With app, or a shelf app).
fn listed_app(id: &JobId, n: u8, shelf: bool) -> Option<AppRef> {
    let guard = LISTED.lock().unwrap();
    let listed = guard.as_ref()?;
    if shelf {
        listed.shelf(id, n).cloned()
    } else {
        listed.open_with(id, n).cloned()
    }
}

/// Run an integration item. A failure becomes a notice card.
pub fn run(app: &AppHandle, id: &JobId, item: Item) -> Result<(), String> {
    let done = match item {
        Item::OpenWith(n) => match listed_app(id, n, false) {
            Some(with) => actions::open_with(app, id, &with),
            None => Ok(()),
        },
        Item::EditWith => actions::edit_with(app, id),
        Item::ChooseEditor(n) => match listed_app(id, n, false) {
            Some(with) => actions::choose_editor(app, id, with),
            None => Ok(()),
        },
        Item::Upload => actions::upload(app, id),
        Item::Shelf(n) => match listed_app(id, n, true) {
            Some(with) => actions::open_with(app, id, &with),
            None => Ok(()),
        },
        #[cfg(target_os = "macos")]
        Item::AirDrop => actions::airdrop(app, id),
        // Share ends in the page (card_share); AirDrop is a macOS item.
        #[cfg(not(target_os = "macos"))]
        Item::AirDrop => Ok(()),
        Item::Share => Ok(()),
    };
    if let Err(reason) = done {
        jobs::notice(app, reason);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::parse_id;
    use ferry_core::job::JobState;
    use ferry_core::pipeline;
    use ferry_core::sniff::Format;

    fn job(text: &str) -> JobId {
        serde_json::from_value(serde_json::Value::String(text.into())).unwrap()
    }

    fn done() -> JobUpdate {
        let mut card = pipeline::notice("x".into());
        card.id = job("18f-1");
        card.state = JobState::Done;
        card.output = Some("/tmp/shot.png".into());
        card.format = Some(Format::Png);
        card
    }

    const ITEMS: [Item; 8] = [
        Item::Share,
        Item::AirDrop,
        Item::OpenWith(0),
        Item::OpenWith(14),
        Item::EditWith,
        Item::ChooseEditor(3),
        Item::Upload,
        Item::Shelf(2),
    ];

    #[test]
    fn every_item_reads_back_through_the_card_menu_ids() {
        let id = job("18f-1");
        for item in ITEMS {
            let action = Action::Integration(item);
            assert_eq!(
                parse_id(&item_id(&id, action)),
                Some((id.clone(), action)),
                "{item:?}"
            );
        }
        assert_eq!(Item::parse("open_with.x"), None);
        assert_eq!(Item::parse("open_with.15"), None);
        assert_eq!(Item::parse("explode"), None);
    }

    #[test]
    fn a_busy_card_offers_no_integration_items() {
        let mut card = done();
        for item in ITEMS {
            assert!(enabled(&card, Action::Integration(item)), "{item:?}");
        }
        card.state = JobState::Queued;
        for item in ITEMS {
            assert!(!enabled(&card, Action::Integration(item)), "{item:?}");
        }
    }

    #[test]
    fn a_stale_index_opens_nothing() {
        let app = |id: &str| AppRef {
            id: id.into(),
            name: id.into(),
        };
        let listed = Listed {
            job: job("18f-1"),
            open_with: vec![app("com.a")],
            shelves: vec![],
        };
        assert_eq!(listed.open_with(&job("18f-1"), 0), Some(&app("com.a")));
        assert_eq!(listed.open_with(&job("18f-1"), 1), None);
        assert_eq!(listed.open_with(&job("18f-2"), 0), None);
        assert_eq!(listed.shelf(&job("18f-1"), 0), None);
    }

    #[test]
    fn integration_items_go_after_open_and_before_restore() {
        let ids: Vec<String> = [
            "card:1:open",
            "card:1:reveal",
            "sep",
            "card:1:copy",
            "sep2",
            "card:1:restore",
            "sep3",
            "card:1:close",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        assert_eq!(
            insert_points(&ids, "card:1:open", "card:1:restore"),
            Some((1, 5))
        );
        assert_eq!(insert_points(&ids, "card:1:open", "card:2:restore"), None);
    }
}
