//! The `[svc]` settings section: updates, settings sync, the card rail and the
//! hover hotkeys. Every struct keeps unknown fields, so a newer Ferry's file
//! loses nothing here.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const DEFAULT_RAIL: [&str; 6] = [
    "downscale",
    "compression",
    "crop",
    "restore",
    "compare",
    "copy",
];
pub const MAX_RAIL_ENTRIES: usize = 16;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Svc {
    pub updates: UpdateSettings,
    pub sync: SyncSettings,
    pub rail: RailSettings,
    pub shortcuts: ShortcutSettings,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UpdateSettings {
    pub auto_check: bool,
    pub pill: bool,
    pub skipped_version: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SyncSettings {
    pub enabled: bool,
    /// Stored with `~` (see `settings::contract_home`).
    pub folder: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RailSettings {
    /// Tool ids in the order the rail shows them. Ids this build does not know
    /// are kept.
    pub tools: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ShortcutSettings {
    pub preview: String,
    pub compare: String,
    pub step_down: String,
    /// Hotkey ids the user turned off.
    pub disabled: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Svc {
    fn default() -> Self {
        Self {
            updates: UpdateSettings::default(),
            sync: SyncSettings::default(),
            rail: RailSettings::default(),
            shortcuts: ShortcutSettings::default(),
            extra: Map::new(),
        }
    }
}

impl Default for UpdateSettings {
    fn default() -> Self {
        Self {
            auto_check: true,
            pill: true,
            skipped_version: String::new(),
            extra: Map::new(),
        }
    }
}

impl Default for SyncSettings {
    fn default() -> Self {
        Self {
            enabled: false,
            folder: String::new(),
            extra: Map::new(),
        }
    }
}

impl Default for RailSettings {
    fn default() -> Self {
        Self {
            tools: DEFAULT_RAIL.iter().map(|id| id.to_string()).collect(),
            extra: Map::new(),
        }
    }
}

impl Default for ShortcutSettings {
    fn default() -> Self {
        Self {
            preview: "Space".into(),
            compare: "CmdOrCtrl+D".into(),
            step_down: "-".into(),
            disabled: Vec::new(),
            extra: Map::new(),
        }
    }
}

/// `^[a-z][a-z0-9_.:-]{0,63}$`: built-in tools, other streams' tools,
/// `pipeline:<id>`.
pub fn is_tool_id(id: &str) -> bool {
    let mut chars = id.chars();
    chars.next().is_some_and(|c| c.is_ascii_lowercase())
        && id.len() <= 64
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_.:-".contains(c))
}

/// Trim, then drop empties and repeats (first wins).
fn tidy(list: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    for item in list {
        let item = item.trim();
        if !item.is_empty() && !out.iter().any(|seen| seen == item) {
            out.push(item.to_string());
        }
    }
    out
}

impl Svc {
    pub fn sanitised(mut self) -> Self {
        self.rail.tools = tidy(&self.rail.tools)
            .into_iter()
            .filter(|id| is_tool_id(id))
            .take(MAX_RAIL_ENTRIES)
            .collect();
        let defaults = ShortcutSettings::default();
        for (key, default) in [
            (&mut self.shortcuts.preview, defaults.preview),
            (&mut self.shortcuts.compare, defaults.compare),
            (&mut self.shortcuts.step_down, defaults.step_down),
        ] {
            if key.trim().is_empty() {
                *key = default;
            }
        }
        self.shortcuts.disabled = tidy(&self.shortcuts.disabled);
        self.updates.skipped_version = self.updates.skipped_version.trim().to_string();
        self.sync.folder = self.sync.folder.trim().to_string();
        if self.sync.folder.is_empty() {
            self.sync.enabled = false;
        }
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::Settings;
    use serde_json::json;
    use std::fs;

    #[test]
    fn svc_defaults_match_the_plan() {
        assert_eq!(
            serde_json::to_value(Svc::default()).unwrap(),
            json!({
                "updates": {"auto_check": true, "pill": true, "skipped_version": ""},
                "sync": {"enabled": false, "folder": ""},
                "rail": {"tools": ["downscale", "compression", "crop", "restore", "compare", "copy"]},
                "shortcuts": {"preview": "Space", "compare": "CmdOrCtrl+D", "step_down": "-", "disabled": []}
            })
        );
    }

    #[test]
    fn a_version_2_file_without_svc_loads_the_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 2, "images": {"compression": 70}}"#).unwrap();
        let (s, _) = Settings::load(&path);
        assert_eq!(s.svc, Svc::default());
        assert_eq!(s.version, 2);
        assert_eq!(s.images.compression, 70);
    }

    #[test]
    fn unknown_rail_ids_are_kept_and_junk_dropped() {
        let mut svc = Svc::default();
        svc.rail.tools = [" crop ", "crop", "", "pipeline:abc", "Bad Id!", "trim"]
            .map(String::from)
            .to_vec();
        assert_eq!(svc.sanitised().rail.tools, ["crop", "pipeline:abc", "trim"]);

        let mut svc = Svc::default();
        svc.rail.tools = (0..20).map(|n| format!("tool{n}")).collect();
        assert_eq!(svc.sanitised().rail.tools.len(), MAX_RAIL_ENTRIES);
    }

    #[test]
    fn blank_hover_keys_fall_back_to_their_defaults() {
        let mut svc = Svc::default();
        svc.shortcuts.preview = "  ".into();
        svc.shortcuts.disabled = [" scale ", "scale", ""].map(String::from).to_vec();
        let svc = svc.sanitised();
        assert_eq!(svc.shortcuts.preview, "Space");
        assert_eq!(svc.shortcuts.disabled, ["scale"]);
    }

    #[test]
    fn sync_without_a_folder_is_off() {
        let mut svc = Svc::default();
        svc.sync.enabled = true;
        svc.sync.folder = " ".into();
        let svc = svc.sanitised();
        assert!(!svc.sync.enabled);
        assert_eq!(svc.sync.folder, "");
    }

    #[test]
    fn unknown_fields_inside_svc_survive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"svc": {"future": 1, "rail": {"tools": ["copy"], "x": true}}}"#,
        )
        .unwrap();
        let (s, _) = Settings::load(&path);
        s.save(&path).unwrap();
        let json: Value = serde_json::from_slice(&fs::read(&path).unwrap()).unwrap();
        assert_eq!(json["svc"]["future"], 1);
        assert_eq!(json["svc"]["rail"]["x"], true);
        assert_eq!(json["svc"]["rail"]["tools"], json!(["copy"]));
        assert_eq!(json["svc"]["updates"]["pill"], true);
    }
}
