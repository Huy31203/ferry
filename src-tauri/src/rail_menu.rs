//! The "+" menu on a card's rail (stream G): add a tool to the rail, or open
//! the rail editor in Settings. The page lists the tools; this pops the menu
//! and routes the choice.

use ferry_core::i18n::t;
use serde::Deserialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, LogicalPosition, WebviewWindow, Wry};
use tracing::warn;

use crate::{prefs, svc_commands};

const ADD: &str = "svc:rail:add:";
const EDIT: &str = "svc:rail:edit";

#[derive(Debug, Clone, Deserialize)]
pub struct RailMenuItem {
    pub id: String,
    pub label: String,
    pub enabled: bool,
}

/// Pop the menu at `(x, y)`, in the stack page's CSS pixels.
pub fn popup(
    app: &AppHandle,
    window: &WebviewWindow,
    x: f64,
    y: f64,
    items: &[RailMenuItem],
) -> Result<(), String> {
    let menu = build(app, items).map_err(|e| e.to_string())?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn build(app: &AppHandle, items: &[RailMenuItem]) -> tauri::Result<Menu<Wry>> {
    let tools = items
        .iter()
        .map(|item| {
            MenuItem::with_id(
                app,
                format!("{ADD}{}", item.id),
                &item.label,
                item.enabled,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let separator = PredefinedMenuItem::separator(app)?;
    let edit = MenuItem::with_id(app, EDIT, t("svc.rail.customise"), true, None::<&str>)?;
    let mut all: Vec<&dyn IsMenuItem<Wry>> =
        tools.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
    all.push(&separator);
    all.push(&edit);
    Menu::with_items(app, &all)
}

/// Route the menu's items. Other menus' items are not ours.
pub fn init(app: &AppHandle) {
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if let Some(tool) = id.strip_prefix(ADD) {
            // Menu events arrive on the main thread; saving settings does not belong there.
            let (app, tool) = (app.clone(), tool.to_string());
            tauri::async_runtime::spawn_blocking(move || {
                let result = prefs::update(&app, |s| {
                    if !s.svc.rail.tools.contains(&tool) {
                        s.svc.rail.tools.push(tool);
                    }
                });
                if let Err(e) = result {
                    warn!("could not add the tool to the rail: {e}");
                }
            });
        } else if id == EDIT {
            svc_commands::open_settings_at(app, "floating", Some("svc_rail"));
        }
    });
}
