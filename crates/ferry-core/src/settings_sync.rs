//! Settings sync, pure part: one file per device in a shared folder, merged key
//! by key (last writer wins on `(modified ms, device id)`). No IO here; the app's
//! sync worker reads and writes the files.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::settings::Settings;

pub const FORMAT: &str = "ferry-settings-sync";
pub const FORMAT_VERSION: u32 = 1;
pub const SUBFOLDER: &str = "Ferry Settings";
pub const MAX_FILE_BYTES: u64 = 1_048_576;
/// Path prefixes that never leave this computer. Other streams add theirs here.
pub const LOCAL: &[&str] = &[
    "version",
    "general.launch_at_login",
    "general.show_dock_icon",
    "svc.sync",
    "svc.updates.skipped_version",
];
/// Path prefixes that sync only between computers with the same OS.
pub const PER_OS: &[&str] = &["hotkeys", "svc.shortcuts", "clipboard.ignored_apps"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    Shared,
    PerOs,
    Local,
}

fn under(path: &str, prefix: &str) -> bool {
    path == prefix
        || path
            .strip_prefix(prefix)
            .is_some_and(|rest| rest.starts_with('.'))
}

/// Prefix match on a dot boundary.
pub fn scope(path: &str) -> Scope {
    if LOCAL.iter().any(|p| under(path, p)) {
        Scope::Local
    } else if PER_OS.iter().any(|p| under(path, p)) {
        Scope::PerOs
    } else {
        Scope::Shared
    }
}

/// `None` for local keys; `@macos/hotkeys.optimise` for per-OS ones.
pub fn sync_key(path: &str, os: &str) -> Option<String> {
    match scope(path) {
        Scope::Local => None,
        Scope::PerOs => Some(format!("@{os}/{path}")),
        Scope::Shared => Some(path.to_string()),
    }
}

/// The inverse of `sync_key`; `None` for another OS's key and for local keys.
pub fn settings_path_of(key: &str, os: &str) -> Option<String> {
    match key.strip_prefix('@') {
        Some(rest) => {
            let (key_os, path) = rest.split_once('/')?;
            (key_os == os && scope(path) == Scope::PerOs).then(|| path.to_string())
        }
        None => (scope(key) == Scope::Shared).then(|| key.to_string()),
    }
}

/// Dotted leaf paths; arrays, scalars and empty objects are leaves.
pub fn flatten(value: &Value) -> BTreeMap<String, Value> {
    fn walk(prefix: &str, value: &Value, out: &mut BTreeMap<String, Value>) {
        match value {
            Value::Object(map) if !map.is_empty() => {
                for (key, child) in map {
                    let path = if prefix.is_empty() {
                        key.clone()
                    } else {
                        format!("{prefix}.{key}")
                    };
                    walk(&path, child, out);
                }
            }
            _ if prefix.is_empty() => {}
            _ => {
                out.insert(prefix.to_string(), value.clone());
            }
        }
    }
    let mut out = BTreeMap::new();
    walk("", value, &mut out);
    out
}

/// A value, when it was modified (ms since the epoch) and by which device.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Entry {
    pub v: Value,
    pub t: i64,
    pub d: String,
}

pub fn newer(a: &Entry, b: &Entry) -> bool {
    a.t > b.t || (a.t == b.t && a.d > b.d)
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeviceFile {
    pub format: String,
    pub format_version: u32,
    pub device: String,
    pub device_name: String,
    pub os: String,
    pub app_version: String,
    pub written_at: i64,
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, PartialEq)]
pub enum ReadError {
    TooLarge,
    Corrupt,
    NotFerry,
    NewerFormat {
        version: u32,
        device_name: Option<String>,
    },
}

/// A file that is cut short, foreign or newer is an error, never "no settings".
pub fn parse(bytes: &[u8]) -> Result<DeviceFile, ReadError> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(ReadError::TooLarge);
    }
    let value: Value = serde_json::from_slice(bytes).map_err(|_| ReadError::Corrupt)?;
    let Some(map) = value.as_object() else {
        return Err(ReadError::NotFerry);
    };
    if map.get("format").and_then(Value::as_str) != Some(FORMAT) {
        return Err(ReadError::NotFerry);
    }
    if let Some(version) = map.get("format_version").and_then(Value::as_u64)
        && version > u64::from(FORMAT_VERSION)
    {
        return Err(ReadError::NewerFormat {
            version: version as u32,
            device_name: map
                .get("device_name")
                .and_then(Value::as_str)
                .map(String::from),
        });
    }
    serde_json::from_value(value).map_err(|_| ReadError::Corrupt)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum JoinMode {
    UseFolder,
    UseThis,
}

/// What this computer believes every synced key holds, and who set it when.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Ledger {
    pub device: String,
    pub host: String,
    pub entries: BTreeMap<String, Entry>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Adopted {
    pub key: String,
    pub previous: Option<Entry>,
    pub entry: Entry,
}

fn own_entries(settings: &Settings, os: &str) -> BTreeMap<String, Value> {
    let value = serde_json::to_value(settings).unwrap_or(Value::Null);
    flatten(&value)
        .into_iter()
        .filter_map(|(path, v)| sync_key(&path, os).map(|key| (key, v)))
        .collect()
}

/// The settings path a sync key stands for, whichever OS it is for.
fn path_of_key(key: &str) -> &str {
    match key.strip_prefix('@') {
        Some(rest) => rest.split_once('/').map_or(rest, |(_, path)| path),
        None => key,
    }
}

impl Ledger {
    /// `UseFolder`: every entry is stamped 0, so any value in the folder wins.
    /// `UseThis`: stamped `now`, so this computer's values win.
    pub fn join(
        settings: &Settings,
        os: &str,
        mode: JoinMode,
        device: &str,
        host: &str,
        now: i64,
    ) -> Self {
        let t = match mode {
            JoinMode::UseFolder => 0,
            JoinMode::UseThis => now,
        };
        let entries = own_entries(settings, os)
            .into_iter()
            .map(|(key, v)| {
                let entry = Entry {
                    v,
                    t,
                    d: device.to_string(),
                };
                (key, entry)
            })
            .collect();
        Self {
            device: device.to_string(),
            host: host.to_string(),
            entries,
        }
    }

    /// Stamp every key whose value differs from the ledger's. True if any did.
    pub fn record_local(&mut self, settings: &Settings, os: &str, now: i64) -> bool {
        let mut changed = false;
        for (key, v) in own_entries(settings, os) {
            let previous_t = match self.entries.get(&key) {
                Some(entry) if entry.v == v => continue,
                Some(entry) => entry.t,
                None => i64::MIN,
            };
            // A change made here always beats what it replaced, even if a clock is off.
            let t = now.max(previous_t.saturating_add(1));
            self.entries.insert(
                key,
                Entry {
                    v,
                    t,
                    d: self.device.clone(),
                },
            );
            changed = true;
        }
        changed
    }

    /// Adopt every newer entry whose value differs. Entries for another OS and
    /// local keys are ignored. Merging the same files again changes nothing.
    pub fn merge(&mut self, remotes: &[DeviceFile], os: &str) -> Vec<Adopted> {
        let mut adopted: Vec<Adopted> = Vec::new();
        for file in remotes {
            for (key, entry) in &file.entries {
                if settings_path_of(key, os).is_none() {
                    continue;
                }
                let current = self.entries.get(key);
                if let Some(current) = current
                    && (!newer(entry, current) || entry.v == current.v)
                {
                    continue;
                }
                let previous = current.cloned();
                self.entries.insert(key.clone(), entry.clone());
                match adopted.iter_mut().find(|a| &a.key == key) {
                    Some(earlier) => earlier.entry = entry.clone(),
                    None => adopted.push(Adopted {
                        key: key.clone(),
                        previous,
                        entry: entry.clone(),
                    }),
                }
            }
        }
        adopted
    }

    /// Undo the adoption of the settings paths in `keys` (the ones `apply` left alone).
    pub fn revert(&mut self, keys: &[String], adopted: &[Adopted]) {
        for a in adopted.iter().rev() {
            if !keys.iter().any(|k| k == path_of_key(&a.key)) {
                continue;
            }
            match &a.previous {
                Some(previous) => {
                    self.entries.insert(a.key.clone(), previous.clone());
                }
                None => {
                    self.entries.remove(&a.key);
                }
            }
        }
    }

    /// Shared keys and this OS's `@os/` keys only.
    pub fn device_file(
        &self,
        device_name: &str,
        os: &str,
        app_version: &str,
        now: i64,
    ) -> DeviceFile {
        let own_prefix = format!("@{os}/");
        let entries = self
            .entries
            .iter()
            .filter(|(key, _)| !key.starts_with('@') || key.starts_with(&own_prefix))
            .map(|(key, entry)| (key.clone(), entry.clone()))
            .collect();
        DeviceFile {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            device: self.device.clone(),
            device_name: device_name.into(),
            os: os.into(),
            app_version: app_version.into(),
            written_at: now,
            entries,
        }
    }
}

fn set_path(root: &mut Value, path: &str, value: Value) {
    let mut node = root;
    for key in path.split('.') {
        if !node.is_object() {
            *node = Value::Object(Map::new());
        }
        let Value::Object(map) = node else {
            return;
        };
        node = map.entry(key).or_insert(Value::Null);
    }
    *node = value;
}

fn get_path<'a>(root: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(root, |node, key| node.as_object()?.get(key))
}

/// Apply adopted values to `settings`. A key is applied only when the current
/// value at its path still equals `previous.v` (or `previous` is None); one that
/// fails to deserialize is undone. Returns the new settings (not yet sanitised)
/// and the settings paths not applied.
pub fn apply(settings: &Settings, adopted: &[Adopted], os: &str) -> (Settings, Vec<String>) {
    let mut current = serde_json::to_value(settings).unwrap_or(Value::Null);
    let mut skipped = Vec::new();
    for a in adopted {
        let Some(path) = settings_path_of(&a.key, os) else {
            continue;
        };
        let unchanged = match &a.previous {
            Some(previous) => get_path(&current, &path) == Some(&previous.v),
            None => true,
        };
        if !unchanged {
            skipped.push(path);
            continue;
        }
        let before = current.clone();
        set_path(&mut current, &path, a.entry.v.clone());
        if serde_json::from_value::<Settings>(current.clone()).is_err() {
            current = before;
            skipped.push(path);
        }
    }
    let next = serde_json::from_value(current).unwrap_or_else(|_| settings.clone());
    (next, skipped)
}

/// `chosen/"Ferry Settings"`, or `chosen` itself when it is already named so.
pub fn sync_dir(chosen: &Path) -> PathBuf {
    if chosen.file_name().is_some_and(|name| name == SUBFOLDER) {
        chosen.to_path_buf()
    } else {
        chosen.join(SUBFOLDER)
    }
}

pub fn file_name(device: &str) -> String {
    format!("{device}.json")
}

/// A file whose name is not `<its own device>.json`: a cloud client's copy of it.
pub fn is_conflict_copy(file_name: &str, file: &DeviceFile) -> bool {
    file_name.strip_suffix(".json").unwrap_or(file_name) != file.device
}

/// `d` + 16 lowercase hex digits.
pub fn new_device_id(seed: u64) -> String {
    format!("d{seed:016x}")
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DeviceSummary {
    pub device: String,
    pub name: String,
    pub os: String,
    pub written_at: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FolderInfo {
    pub exists: bool,
    pub devices: Vec<DeviceSummary>,
    pub newer_format: bool,
}

/// One summary per other device (its newest file), newest first.
pub fn summarise(files: &[DeviceFile], own: &str) -> Vec<DeviceSummary> {
    let mut latest: BTreeMap<&str, &DeviceFile> = BTreeMap::new();
    for file in files.iter().filter(|f| f.device != own) {
        let slot = latest.entry(&file.device).or_insert(file);
        if file.written_at > slot.written_at {
            *slot = file;
        }
    }
    let mut out: Vec<DeviceSummary> = latest
        .into_values()
        .map(|f| DeviceSummary {
            device: f.device.clone(),
            name: f.device_name.clone(),
            os: f.os.clone(),
            written_at: f.written_at,
        })
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.written_at));
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const MAC: &str = "macos";

    fn entry(v: Value, t: i64, d: &str) -> Entry {
        Entry { v, t, d: d.into() }
    }

    fn remote(device: &str, os: &str, entries: &[(&str, Value, i64)]) -> DeviceFile {
        DeviceFile {
            format: FORMAT.into(),
            format_version: FORMAT_VERSION,
            device: device.into(),
            device_name: format!("{device} name"),
            os: os.into(),
            app_version: "0.3.0".into(),
            written_at: 1,
            entries: entries
                .iter()
                .map(|(k, v, t)| (k.to_string(), entry(v.clone(), *t, device)))
                .collect(),
        }
    }

    fn joined(mode: JoinMode, now: i64) -> (Settings, Ledger) {
        let settings = Settings::default();
        let ledger = Ledger::join(&settings, MAC, mode, "dme", "host", now);
        (settings, ledger)
    }

    #[test]
    fn scope_classifies_keys() {
        for path in [
            "version",
            "general.launch_at_login",
            "svc.sync.folder",
            "svc.updates.skipped_version",
        ] {
            assert_eq!(scope(path), Scope::Local, "{path}");
        }
        for path in [
            "hotkeys.optimise",
            "svc.shortcuts.preview",
            "clipboard.ignored_apps",
        ] {
            assert_eq!(scope(path), Scope::PerOs, "{path}");
        }
        for path in [
            "images.compression",
            "svc.rail.tools",
            "svc.updates.pill",
            "watchers.folders",
            "svc.sync_extra",
        ] {
            assert_eq!(scope(path), Scope::Shared, "{path}");
        }
    }

    #[test]
    fn sync_keys_round_trip() {
        assert_eq!(
            sync_key("hotkeys.optimise", "macos").as_deref(),
            Some("@macos/hotkeys.optimise")
        );
        assert_eq!(
            settings_path_of("@macos/hotkeys.optimise", "macos").as_deref(),
            Some("hotkeys.optimise")
        );
        assert_eq!(settings_path_of("@macos/hotkeys.optimise", "windows"), None);
        assert_eq!(sync_key("version", "macos"), None);
        assert_eq!(
            sync_key("images.compression", "windows").as_deref(),
            Some("images.compression")
        );
    }

    #[test]
    fn flatten_makes_leaves_of_objects_only() {
        let flat = flatten(&serde_json::to_value(Settings::default()).unwrap());
        assert!(flat.contains_key("images.output.optimised.template"));
        assert!(flat["images.crop_sizes"].is_array());
        assert_eq!(flatten(&json!({"a": {}}))["a"], json!({}));
    }

    #[test]
    fn a_local_change_is_stamped_and_others_keep_their_time() {
        let (mut settings, mut ledger) = joined(JoinMode::UseThis, 100);
        settings.images.compression = 60;
        assert!(ledger.record_local(&settings, MAC, 200));
        assert_eq!(ledger.entries["images.compression"].t, 200);
        assert_eq!(ledger.entries["images.strip_metadata"].t, 100);
        assert!(!ledger.record_local(&settings, MAC, 300));
    }

    #[test]
    fn the_newer_entry_wins_and_ties_go_to_the_larger_device_id() {
        let (_, mut ledger) = joined(JoinMode::UseThis, 100);
        let older = remote("dzz", MAC, &[("images.compression", json!(40), 99)]);
        assert!(ledger.merge(&[older], MAC).is_empty());
        let newer_file = remote("dzz", MAC, &[("images.compression", json!(40), 101)]);
        assert_eq!(ledger.merge(&[newer_file], MAC).len(), 1);

        let (_, mut ledger) = joined(JoinMode::UseThis, 100);
        let tie_small = remote("daa", MAC, &[("images.compression", json!(40), 100)]);
        assert!(ledger.merge(&[tie_small], MAC).is_empty());
        let tie_large = remote("dzz", MAC, &[("images.compression", json!(40), 100)]);
        assert_eq!(ledger.merge(&[tie_large], MAC).len(), 1);
        assert_eq!(ledger.entries["images.compression"].v, json!(40));
    }

    #[test]
    fn per_os_keys_never_cross_systems() {
        let (_, mut ledger) = joined(JoinMode::UseThis, 100);
        let win = remote(
            "dwin",
            "windows",
            &[
                ("@windows/hotkeys.optimise", json!("Ctrl+Alt+Shift+X"), 999),
                ("images.compression", json!(40), 999),
            ],
        );
        let adopted = ledger.merge(&[win], MAC);
        assert_eq!(adopted.len(), 1);
        assert_eq!(adopted[0].key, "images.compression");
        let file = ledger.device_file("Mac", MAC, "0.3.0", 5);
        assert!(file.entries.keys().all(|k| !k.starts_with("@windows/")));
        assert!(file.entries.contains_key("@macos/hotkeys.optimise"));
        assert!(!file.entries.contains_key("svc.sync.folder"));
        assert!(!file.entries.contains_key("general.launch_at_login"));
    }

    #[test]
    fn join_use_folder_lets_every_remote_key_win() {
        let (_, mut ledger) = joined(JoinMode::UseFolder, 100);
        let other = remote("dzz", MAC, &[("images.compression", json!(40), 50)]);
        assert_eq!(ledger.merge(&[other], MAC).len(), 1);
    }

    #[test]
    fn join_use_this_lets_local_win() {
        let (_, mut ledger) = joined(JoinMode::UseThis, 100);
        let other = remote("dzz", MAC, &[("images.compression", json!(40), 50)]);
        assert!(ledger.merge(&[other], MAC).is_empty());
    }

    #[test]
    fn a_truncated_or_foreign_file_is_rejected_not_read_as_empty() {
        assert_eq!(
            parse(br#"{"format":"ferry-settings-sync","form"#),
            Err(ReadError::Corrupt)
        );
        assert_eq!(parse(b"{}"), Err(ReadError::NotFerry));
        assert_eq!(parse(b"[]"), Err(ReadError::NotFerry));
        let big = vec![b' '; MAX_FILE_BYTES as usize + 1];
        assert_eq!(parse(&big), Err(ReadError::TooLarge));
    }

    #[test]
    fn a_newer_format_is_named_and_not_merged() {
        let mut file = remote("dzz", MAC, &[]);
        file.format_version = 2;
        file.device_name = "Huy's PC".into();
        let bytes = serde_json::to_vec(&file).unwrap();
        assert_eq!(
            parse(&bytes),
            Err(ReadError::NewerFormat {
                version: 2,
                device_name: Some("Huy's PC".into())
            })
        );
    }

    #[test]
    fn a_value_of_the_wrong_type_is_rejected_and_the_rest_applied() {
        let (settings, mut ledger) = joined(JoinMode::UseThis, 100);
        let file = remote(
            "dzz",
            MAC,
            &[
                ("images.compression", json!("high"), 200),
                ("images.strip_metadata", json!(false), 200),
            ],
        );
        let adopted = ledger.merge(&[file], MAC);
        let (next, rejected) = apply(&settings, &adopted, MAC);
        assert!(!next.images.strip_metadata);
        assert_eq!(next.images.compression, settings.images.compression);
        assert_eq!(rejected, ["images.compression"]);
    }

    #[test]
    fn a_local_change_made_during_a_merge_is_not_reverted() {
        let (mut settings, mut ledger) = joined(JoinMode::UseThis, 100);
        assert_eq!(ledger.entries["images.compression"].v, json!(80));
        let file = remote("dzz", MAC, &[("images.compression", json!(60), 200)]);
        let adopted = ledger.merge(&[file], MAC);
        assert_eq!(adopted[0].previous.as_ref().unwrap().v, json!(80));
        settings.images.compression = 70;
        let (next, kept) = apply(&settings, &adopted, MAC);
        assert_eq!(next.images.compression, 70);
        assert_eq!(kept, ["images.compression"]);
        ledger.revert(&kept, &adopted);
        assert!(ledger.record_local(&next, MAC, 300));
        assert_eq!(
            ledger.entries["images.compression"],
            entry(json!(70), 300, "dme")
        );
    }

    #[test]
    fn merging_the_same_files_twice_changes_nothing() {
        let (_, mut ledger) = joined(JoinMode::UseThis, 100);
        let file = remote("dzz", MAC, &[("images.compression", json!(60), 200)]);
        assert_eq!(ledger.merge(std::slice::from_ref(&file), MAC).len(), 1);
        assert!(ledger.merge(&[file], MAC).is_empty());
    }

    #[test]
    fn a_conflict_copy_of_our_own_file_is_merged() {
        let (_, mut ledger) = joined(JoinMode::UseThis, 100);
        let copy = remote("dme", MAC, &[("images.compression", json!(55), 300)]);
        assert_eq!(ledger.merge(&[copy], MAC).len(), 1);
        assert_eq!(ledger.entries["images.compression"].v, json!(55));
    }

    #[test]
    fn conflict_copies_are_recognised() {
        let file = remote("dabc", MAC, &[]);
        assert!(!is_conflict_copy("dabc.json", &file));
        assert!(is_conflict_copy(
            "dabc (Huy's conflicted copy 2026-10-23).json",
            &file
        ));
        assert!(is_conflict_copy("dabc 2.json", &file));
    }

    #[test]
    fn sync_dir_reuses_a_folder_already_named_ferry_settings() {
        assert_eq!(
            sync_dir(Path::new("/x/iCloud")),
            Path::new("/x/iCloud/Ferry Settings")
        );
        assert_eq!(
            sync_dir(Path::new("/x/Ferry Settings")),
            Path::new("/x/Ferry Settings")
        );
    }

    #[test]
    fn device_ids_look_alike() {
        let id = new_device_id(0);
        assert_eq!(id.len(), 17);
        assert!(id.starts_with('d'));
        assert!(
            id[1..]
                .chars()
                .all(|c| c.is_ascii_digit() || ('a'..='f').contains(&c))
        );
        assert_ne!(new_device_id(1), new_device_id(2));
    }

    #[test]
    fn a_device_file_round_trips() {
        let (_, ledger) = joined(JoinMode::UseThis, 100);
        let file = ledger.device_file("Mac", MAC, "0.3.0", 7);
        let bytes = serde_json::to_vec_pretty(&file).unwrap();
        assert_eq!(parse(&bytes), Ok(file));
    }

    #[test]
    fn summaries_list_other_devices_newest_first() {
        let mut a = remote("da", MAC, &[]);
        a.written_at = 10;
        let mut b = remote("db", "windows", &[]);
        b.written_at = 20;
        let mut a_old = remote("da", MAC, &[]);
        a_old.written_at = 5;
        let own = remote("dme", MAC, &[]);
        let list = summarise(&[a, b, a_old, own], "dme");
        let ids: Vec<&str> = list.iter().map(|s| s.device.as_str()).collect();
        assert_eq!(ids, ["db", "da"]);
        assert_eq!(list[1].written_at, 10);
    }
}
