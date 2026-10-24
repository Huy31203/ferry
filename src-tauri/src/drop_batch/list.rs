//! The compact results list: its settings and, from the selection bar, its commands.

use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

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
