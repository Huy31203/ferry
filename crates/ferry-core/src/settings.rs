//! `settings.json`: the model with its defaults (spec §5.7), tolerant loading
//! and an atomic save. Unknown fields at any level survive a load and save, so
//! a newer or hand-edited file loses nothing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::integrations::MediaKind;
use crate::svc_settings::Svc;
use crate::{edit, fsutil, optimise::Encoder, template};

pub const VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub general: General,
    pub svc: Svc, // svc (stream G)
    pub clipboard: Clipboard,
    pub images: Images,
    pub floating: Floating,
    pub backups: Backups,
    pub hotkeys: Hotkeys,
    // Stream H: integrations
    pub share: Share,
    // ---- sI clipboard-fixes: [clip] ----
    pub clip: Clip,
    // stream A: video
    pub video: Video,
    // ---- Stream D: drop zone and results list ----
    pub drop_zone: DropZone,
    pub results_list: ResultsList,
    // stream B: audio
    pub audio: crate::audio::settings::Audio,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartState {
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayStyle {
    Default,
    Classic,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardSize {
    Default,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub launch_at_login: bool,
    pub start_state: StartState,
    pub show_dock_icon: bool,
    pub show_in_screenshots: bool,
    pub tray_style: TrayStyle,
    pub language: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clipboard {
    pub enabled: bool,
    pub skip_multiple_files: bool,
    pub max_size_mb: u32,
    pub ignored_apps: Vec<String>,
    pub accumulate: bool,
    pub copy_paths: bool,
    pub file_template: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Images {
    /// 5 to 100; the encoders' quality `q`.
    pub compression: u8,
    /// Stored and shown, but no effect this cycle ("Coming soon").
    pub adaptive: bool,
    /// Stored and shown, but no effect this cycle ("Coming soon").
    pub switch_png_jpeg: bool,
    pub strip_metadata: bool,
    pub preserve_dates: bool,
    pub ignored_extensions: Vec<String>,
    pub max_size_mb: u32,
    pub output: Output,
    /// Saved crop sizes, newest first.
    pub crop_sizes: Vec<(u32, u32)>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

/// Where a file job's result goes (spec §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    InPlace,
    SameFolder,
    Folder,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputRule {
    pub location: Location,
    /// Used by `Folder`; stored with `~` (see `contract_home`).
    pub folder: String,
    /// File name only: `%f %e %z %y %m %d %P`.
    pub template: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for OutputRule {
    fn default() -> Self {
        Self {
            location: Location::InPlace,
            folder: "~/Pictures/Ferry".into(),
            template: "%f.%e".into(),
            extra: Map::new(),
        }
    }
}

/// Images row of the File handling pane: optimised results and conversions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    pub optimised: OutputRule,
    pub converted: OutputRule,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            optimised: OutputRule {
                template: "%f_optimised.%e".into(),
                ..OutputRule::default()
            },
            converted: OutputRule::default(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Floating {
    pub enabled: bool,
    pub corner: Corner,
    pub card_size: CardSize,
    /// Seconds before a card hides; `0` keeps it until dismissed.
    pub auto_hide_secs: u32,
    pub follow_cursor: bool,
    /// Space, Cmd/Ctrl+D, - and 1–9 act on the hovered card.
    pub hover_hotkeys: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Backups {
    pub retention_days: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub optimise: String,
    pub aggressive: String,
    pub cycle_state: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Hotkeys {
    /// `Ctrl+Shift+C/A/P` on macOS; Windows adds Alt because Ctrl+Shift+C opens
    /// browser dev tools.
    pub fn for_platform(mac: bool) -> Self {
        let modifiers = if mac { "Ctrl+Shift" } else { "Ctrl+Alt+Shift" };
        Self {
            optimise: format!("{modifiers}+C"),
            aggressive: format!("{modifiers}+A"),
            cycle_state: format!("{modifiers}+P"),
            extra: Map::new(),
        }
    }
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self::for_platform(cfg!(target_os = "macos"))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            general: General::default(),
            svc: Svc::default(), // svc (stream G)
            clipboard: Clipboard::default(),
            images: Images::default(),
            floating: Floating::default(),
            backups: Backups::default(),
            hotkeys: Hotkeys::default(),
            share: Share::default(), // Stream H
            // ---- sI clipboard-fixes: [clip] ----
            clip: Clip::default(),
            // stream A: video
            video: Video::default(),
            drop_zone: DropZone::default(),
            results_list: ResultsList::default(),
            // stream B: audio
            audio: crate::audio::settings::Audio::default(),
            extra: Map::new(),
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            launch_at_login: true,
            start_state: StartState::Running,
            show_dock_icon: false,
            show_in_screenshots: false,
            tray_style: TrayStyle::Default,
            language: "en".into(),
            extra: Map::new(),
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            enabled: true,
            skip_multiple_files: true,
            max_size_mb: 50,
            ignored_apps: Vec::new(),
            accumulate: false,
            copy_paths: false,
            file_template: "%f".into(),
            extra: Map::new(),
        }
    }
}

impl Default for Images {
    fn default() -> Self {
        Self {
            compression: 80,
            adaptive: false,
            switch_png_jpeg: false,
            strip_metadata: true,
            preserve_dates: true,
            ignored_extensions: Vec::new(),
            max_size_mb: 50,
            output: Output::default(),
            crop_sizes: vec![(1200, 628), (1080, 1080), (1920, 1080), (1280, 720)],
            extra: Map::new(),
        }
    }
}

impl Default for Floating {
    fn default() -> Self {
        Self {
            enabled: true,
            corner: Corner::BottomRight,
            card_size: CardSize::Default,
            auto_hide_secs: 6,
            follow_cursor: true,
            hover_hotkeys: true,
            extra: Map::new(),
        }
    }
}

impl Default for Backups {
    fn default() -> Self {
        Self {
            retention_days: 7,
            extra: Map::new(),
        }
    }
}

// Stream H: integrations
/// An app the user chose or Ferry found. `id` is a macOS bundle id or `.app`
/// path, or on Windows an `.exe` path or handler name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AppRef {
    pub id: String,
    pub name: String,
}

impl AppRef {
    fn tidy(mut self) -> Option<Self> {
        self.id = self.id.trim().to_string();
        self.name = self.name.trim().to_string();
        if self.id.is_empty() {
            return None;
        }
        if self.name.is_empty() {
            self.name = Path::new(&self.id)
                .file_stem()
                .map(|stem| stem.to_string_lossy().into_owned())
                .unwrap_or_else(|| self.id.clone());
        }
        Some(self)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct EditWith {
    pub image: Option<AppRef>,
    pub video: Option<AppRef>,
    pub audio: Option<AppRef>,
    pub pdf: Option<AppRef>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl EditWith {
    pub fn get(&self, kind: MediaKind) -> Option<&AppRef> {
        match kind {
            MediaKind::Image => self.image.as_ref(),
            MediaKind::Video => self.video.as_ref(),
            MediaKind::Audio => self.audio.as_ref(),
            MediaKind::Pdf => self.pdf.as_ref(),
        }
    }

    pub fn set(&mut self, kind: MediaKind, app: Option<AppRef>) {
        match kind {
            MediaKind::Image => self.image = app,
            MediaKind::Video => self.video = app,
            MediaKind::Audio => self.audio = app,
            MediaKind::Pdf => self.pdf = app,
        }
    }
}

/// Share, Open With, Edit with, Upload and the shelf apps (`[share]`).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Share {
    pub edit_with: EditWith,
    pub uploader: Option<AppRef>,
    pub shelf_apps: Vec<AppRef>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Share {
    pub fn sanitise(&mut self) {
        for slot in [
            &mut self.edit_with.image,
            &mut self.edit_with.video,
            &mut self.edit_with.audio,
            &mut self.edit_with.pdf,
            &mut self.uploader,
        ] {
            *slot = slot.take().and_then(AppRef::tidy);
        }
        let mut seen: Vec<String> = Vec::new();
        self.shelf_apps = std::mem::take(&mut self.shelf_apps)
            .into_iter()
            .filter_map(AppRef::tidy)
            .filter(|app| {
                let id = app.id.to_lowercase();
                !seen.contains(&id) && {
                    seen.push(id);
                    true
                }
            })
            .collect();
    }
}

// ---- sI clipboard-fixes: [clip] ----
/// Clipboard output beyond the Clipboard section: copy the results of drops and
/// watched folders.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clip {
    pub auto_copy: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

// stream A: video settings
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpeedFrames {
    #[default]
    Keep,
    Drop,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FpsCap {
    #[default]
    #[serde(rename = "off")]
    Off,
    #[serde(rename = "24")]
    Fps24,
    #[serde(rename = "30")]
    Fps30,
    #[serde(rename = "60")]
    Fps60,
}

impl FpsCap {
    pub fn fps(self) -> Option<u32> {
        match self {
            FpsCap::Off => None,
            FpsCap::Fps24 => Some(24),
            FpsCap::Fps30 => Some(30),
            FpsCap::Fps60 => Some(60),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Video {
    pub encoder: Encoder,
    /// 5 to 100, like the image setting.
    pub compression: u8,
    pub convert_mov: bool,
    pub audio_to_aac: bool,
    pub remove_audio: bool,
    pub fps_cap: FpsCap,
    pub speed_frames: SpeedFrames,
    pub preserve_dates: bool,
    pub ignored_extensions: Vec<String>,
    pub max_size_mb: u32,
    pub output: Output,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Output {
    /// The default rules with both folders replaced.
    pub fn with_folder(folder: &str) -> Output {
        let defaults = Output::default();
        Output {
            optimised: OutputRule {
                folder: folder.into(),
                ..defaults.optimised
            },
            converted: OutputRule {
                folder: folder.into(),
                ..defaults.converted
            },
            extra: Map::new(),
        }
    }
}

impl Default for Video {
    fn default() -> Self {
        Self {
            encoder: Encoder::Fast,
            compression: 80,
            convert_mov: true,
            audio_to_aac: false,
            remove_audio: false,
            fps_cap: FpsCap::Off,
            speed_frames: SpeedFrames::Keep,
            preserve_dates: true,
            ignored_extensions: Vec::new(),
            max_size_mb: 4000,
            output: Output::with_folder(if cfg!(windows) {
                "~/Videos/Ferry"
            } else {
                "~/Movies/Ferry"
            }),
            extra: Map::new(),
        }
    }
}

// ---- Stream D: drop zone and results list ----

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneShowOn {
    AnyDrag,
    Modifier,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ZonePosition {
    NearResults,
    UnderCursor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DragKey {
    Alt,
    Shift,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DropZone {
    pub enabled: bool,
    pub show_on: ZoneShowOn,
    pub position: ZonePosition,
    pub modifier: DragKey,
    pub keep_original_modifier: DragKey,
    pub batch_threshold: u32,
    pub ignored_apps: Vec<String>,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for DropZone {
    fn default() -> Self {
        let mac = cfg!(target_os = "macos");
        let apps: &[&str] = if mac {
            &[
                "com.apple.FinalCut",
                "com.apple.iMovieApp",
                "com.blackmagic-design.DaVinciResolve",
                "com.adobe.PremierePro",
                "com.adobe.AfterEffects",
                "com.adobe.Photoshop",
                "com.adobe.illustrator",
                "com.figma.Desktop",
                "com.bohemiancoding.sketch3",
                "com.seriflabs.affinitydesigner2",
                "com.seriflabs.affinityphoto2",
            ]
        } else {
            &[
                "Adobe Premiere Pro.exe",
                "AfterFX.exe",
                "Photoshop.exe",
                "Illustrator.exe",
                "Resolve.exe",
                "Figma.exe",
                "Affinity Designer 2.exe",
                "Affinity Photo 2.exe",
            ]
        };
        Self {
            enabled: true,
            // Windows has no drag-start signal: modifier tap only unless the user opts in to the edge tab.
            show_on: if mac {
                ZoneShowOn::AnyDrag
            } else {
                ZoneShowOn::Modifier
            },
            position: ZonePosition::NearResults,
            modifier: DragKey::Alt,
            keep_original_modifier: DragKey::Shift,
            batch_threshold: 20,
            ignored_apps: apps.iter().map(|a| a.to_string()).collect(),
            extra: Map::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ResultsList {
    pub after_cards: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for ResultsList {
    fn default() -> Self {
        Self {
            after_cards: 5,
            extra: Map::new(),
        }
    }
}

/// How `Settings::load` found the file.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Loaded,
    Missing,
    /// The file could not be parsed. Defaults are in use; `backup` is where the
    /// unreadable file was copied, or `None` if that copy failed too.
    Recovered {
        backup: Option<PathBuf>,
    },
}

impl Settings {
    /// Read `path`. A missing file gives defaults; a corrupt one gives defaults
    /// and is copied to `<path>.bak` first. Out-of-range values are clamped.
    pub fn load(path: &Path) -> (Settings, LoadStatus) {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Settings::default(), LoadStatus::Missing);
            }
            Err(_) => return (Settings::default(), recover(path)),
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => (settings.sanitised(), LoadStatus::Loaded),
            Err(_) => (Settings::default(), recover(path)),
        }
    }

    /// Write the file atomically, creating its folder if needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fsutil::atomic_replace(path, &json)
    }

    /// A hand-edited file, or a typo in the Settings window, must not turn the
    /// encoders, the size limits or the housekeeping into nonsense.
    pub fn sanitised(mut self) -> Self {
        self.images.compression = self.images.compression.clamp(5, 100);
        self.images.max_size_mb = self.images.max_size_mb.max(1);
        self.clipboard.max_size_mb = self.clipboard.max_size_mb.max(1);
        self.floating.auto_hide_secs = self.floating.auto_hide_secs.min(3600);
        self.backups.retention_days = self.backups.retention_days.clamp(1, 3650);
        for list in [
            &mut self.images.ignored_extensions,
            &mut self.clipboard.ignored_apps,
        ] {
            *list = list
                .iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }
        repair_output(&mut self.images.output, &Output::default());
        let side = 1..=edit::MAX_SIDE;
        self.images
            .crop_sizes
            .retain(|(w, h)| side.contains(w) && side.contains(h));
        self.svc = self.svc.sanitised(); // svc (stream G)
        // stream A: video
        self.video.compression = self.video.compression.clamp(5, 100);
        self.video.max_size_mb = self.video.max_size_mb.max(1);
        self.video.ignored_extensions = self
            .video
            .ignored_extensions
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
        repair_output(&mut self.video.output, &Video::default().output);
        self.sanitise_drop();
        // stream B: audio
        self.audio.sanitise();
        self.share.sanitise(); // Stream H
        self.version = self.version.max(VERSION);
        self
    }

    // ---- Stream D: drop zone and results list ----
    fn sanitise_drop(&mut self) {
        let z = &mut self.drop_zone;
        z.batch_threshold = z.batch_threshold.clamp(1, 10_000);
        if z.keep_original_modifier == z.modifier {
            z.keep_original_modifier = match z.modifier {
                DragKey::Alt => DragKey::Shift,
                DragKey::Shift => DragKey::Alt,
            };
        }
        z.ignored_apps = z
            .ignored_apps
            .iter()
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
            .collect();
        self.results_list.after_cards = self.results_list.after_cards.clamp(1, 50);
    }
}

/// An invalid or empty template, or an empty folder, goes back to the default.
pub(crate) fn repair_output(out: &mut Output, defaults: &Output) {
    for (rule, default) in [
        (&mut out.optimised, &defaults.optimised),
        (&mut out.converted, &defaults.converted),
    ] {
        if rule.template.trim().is_empty() || template::validate(&rule.template).is_err() {
            rule.template = default.template.clone();
        }
        if rule.folder.trim().is_empty() {
            rule.folder = default.folder.clone();
        }
    }
}

fn recover(path: &Path) -> LoadStatus {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".bak");
    let backup = path.with_file_name(name);
    let backup = fs::copy(path, &backup).is_ok().then_some(backup);
    LoadStatus::Recovered { backup }
}

/// Expand a leading `~` using `home`. Anything else is returned unchanged.
pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => home.to_owned(),
        Some(rest) if rest.starts_with(['/', '\\']) => home.join(&rest[1..]),
        _ => PathBuf::from(path),
    }
}

/// Store `path` with a leading `~` when it is inside `home`, so a settings file
/// synced to another account still points at that account's folders.
pub fn contract_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(s.version, 2);
        assert_eq!(s.images.compression, 80);
        assert!(s.images.strip_metadata && s.images.preserve_dates);
        assert_eq!(s.clipboard.file_template, "%f");
        assert_eq!(s.clipboard.max_size_mb, 50);
        assert_eq!(s.floating.corner, Corner::BottomRight);
        assert_eq!(s.floating.auto_hide_secs, 6);
        assert_eq!(s.backups.retention_days, 7);
        assert_eq!(s.general.start_state, StartState::Running);
        assert!(!s.general.show_in_screenshots);
    }

    #[test]
    fn hotkeys_default_per_platform() {
        let mac = Hotkeys::for_platform(true);
        assert_eq!(
            (
                mac.optimise.as_str(),
                mac.aggressive.as_str(),
                mac.cycle_state.as_str()
            ),
            ("Ctrl+Shift+C", "Ctrl+Shift+A", "Ctrl+Shift+P")
        );
        assert_eq!(Hotkeys::for_platform(false).optimise, "Ctrl+Alt+Shift+C");
        assert_eq!(Hotkeys::for_platform(false).cycle_state, "Ctrl+Alt+Shift+P");
    }

    #[test]
    fn a_missing_file_gives_defaults_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (settings, status) = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        assert_eq!(status, LoadStatus::Missing);
        assert!(!path.exists());
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"images": {"compression": 60}, "floating": {"corner": "top_left"}}"#,
        )
        .unwrap();
        let (settings, status) = Settings::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(settings.images.compression, 60);
        assert!(
            settings.images.strip_metadata,
            "sibling field keeps its default"
        );
        assert_eq!(settings.floating.corner, Corner::TopLeft);
        assert_eq!(settings.clipboard, Clipboard::default());
    }

    #[test]
    fn unknown_fields_survive_a_load_and_save_at_every_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"future": {"a": 1}, "images": {"compression": 70, "sharpen": true},
                "hotkeys": {"paste_special": "Ctrl+Alt+V"}}"#,
        )
        .unwrap();
        let (settings, _) = Settings::load(&path);
        settings.save(&path).unwrap();
        let saved = read_json(&path);
        assert_eq!(saved["future"], json!({"a": 1}));
        assert_eq!(saved["images"]["sharpen"], json!(true));
        assert_eq!(saved["images"]["compression"], json!(70));
        assert_eq!(saved["hotkeys"]["paste_special"], json!("Ctrl+Alt+V"));
        assert_eq!(
            saved["images"]["strip_metadata"],
            json!(true),
            "defaults are written out"
        );
    }

    #[test]
    fn a_corrupt_file_gives_defaults_and_keeps_a_bak_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"{ not json").unwrap();
        let (settings, status) = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        let bak = dir.path().join("settings.json.bak");
        assert_eq!(
            status,
            LoadStatus::Recovered {
                backup: Some(bak.clone())
            }
        );
        assert_eq!(fs::read(bak).unwrap(), b"{ not json");
    }

    #[test]
    fn an_empty_file_and_a_non_object_count_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for content in ["", "[]", "42", r#"{"images": {"compression": "high"}}"#] {
            fs::write(&path, content).unwrap();
            let (settings, status) = Settings::load(&path);
            assert_eq!(settings, Settings::default(), "{content:?}");
            assert!(
                matches!(status, LoadStatus::Recovered { .. }),
                "{content:?}"
            );
        }
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"images": {"compression": 0, "max_size_mb": 0}, "clipboard": {"max_size_mb": 0}}"#,
        )
        .unwrap();
        let (settings, status) = Settings::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(settings.images.compression, 5);
        assert_eq!(settings.images.max_size_mb, 1);
        assert_eq!(settings.clipboard.max_size_mb, 1);
        fs::write(&path, r#"{"images": {"compression": 200}}"#).unwrap();
        assert_eq!(Settings::load(&path).0.images.compression, 100);
    }

    #[test]
    fn save_creates_the_folder_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/ferry/settings.json");
        Settings::default().save(&path).unwrap();
        assert_eq!(
            Settings::load(&path),
            (Settings::default(), LoadStatus::Loaded)
        );
        assert!(
            !dir.path()
                .join("config/ferry/settings.json.ferry-tmp")
                .exists()
        );
    }

    #[test]
    fn a_changed_setting_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.images.compression = 55;
        settings.clipboard.ignored_apps = vec!["com.1password.1password".into()];
        settings.floating.card_size = CardSize::Compact;
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).0, settings);
    }

    #[test]
    fn home_is_stored_as_a_tilde_and_expanded_back() {
        let home = Path::new("/Users/gos");
        assert_eq!(
            contract_home(Path::new("/Users/gos/Pictures/x"), home),
            "~/Pictures/x"
        );
        assert_eq!(contract_home(Path::new("/Users/gos"), home), "~");
        assert_eq!(
            contract_home(Path::new("/Volumes/Photos"), home),
            "/Volumes/Photos"
        );
        assert_eq!(
            contract_home(Path::new("/Users/gosling/x"), home),
            "/Users/gosling/x"
        );
        assert_eq!(
            expand_home("~/Pictures/x", home),
            Path::new("/Users/gos/Pictures/x")
        );
        assert_eq!(expand_home("~", home), Path::new("/Users/gos"));
        assert_eq!(
            expand_home("/Volumes/Photos", home),
            Path::new("/Volumes/Photos")
        );
        assert_eq!(expand_home("~other/x", home), Path::new("~other/x"));
    }

    #[test]
    fn out_of_range_values_are_clamped_and_blank_list_entries_dropped() {
        let mut s = Settings::default();
        s.images.compression = 0;
        s.images.max_size_mb = 0;
        s.clipboard.max_size_mb = 0;
        s.floating.auto_hide_secs = 99_999_999;
        s.backups.retention_days = 0;
        s.images.ignored_extensions = vec![" png ".into(), "".into(), "  ".into()];
        s.clipboard.ignored_apps = vec!["".into(), "com.example.App".into()];
        let s = s.sanitised();
        assert_eq!(s.images.compression, 5);
        assert_eq!((s.images.max_size_mb, s.clipboard.max_size_mb), (1, 1));
        assert_eq!(s.floating.auto_hide_secs, 3600);
        assert_eq!(s.backups.retention_days, 1);
        assert_eq!(s.images.ignored_extensions, vec!["png".to_string()]);
        assert_eq!(
            s.clipboard.ignored_apps,
            vec!["com.example.App".to_string()]
        );
    }

    #[test]
    fn cycle_2_defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(
            serde_json::to_value(&s.images.output).unwrap(),
            json!({
                "optimised": { "location": "in_place", "folder": "~/Pictures/Ferry", "template": "%f_optimised.%e" },
                "converted": { "location": "in_place", "folder": "~/Pictures/Ferry", "template": "%f.%e" }
            })
        );
        assert_eq!(
            s.images.crop_sizes,
            [(1200, 628), (1080, 1080), (1920, 1080), (1280, 720)]
        );
        assert!(s.floating.hover_hotkeys);
    }

    #[test]
    fn a_version_1_file_loads_as_version_2_with_the_new_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 1, "images": {"compression": 70}}"#).unwrap();
        let (settings, _) = Settings::load(&path);
        assert_eq!(settings.version, 2);
        assert_eq!(settings.images.compression, 70);
        assert_eq!(settings.images.output, Output::default());
        assert!(settings.floating.hover_hotkeys);
    }

    #[test]
    fn broken_output_rules_and_crop_sizes_are_repaired() {
        let mut s = Settings::default();
        s.images.output.optimised.template = "%q".into();
        s.images.output.converted.template = "  ".into();
        s.images.output.converted.folder = String::new();
        s.images.crop_sizes = vec![(0, 100), (1200, 628), (40_000, 10)];
        let s = s.sanitised();
        let defaults = Output::default();
        assert_eq!(
            s.images.output.optimised.template,
            defaults.optimised.template
        );
        assert_eq!(
            s.images.output.converted.template,
            defaults.converted.template
        );
        assert_eq!(s.images.output.converted.folder, defaults.converted.folder);
        assert_eq!(s.images.crop_sizes, [(1200, 628)]);
    }

    #[test]
    fn the_clip_section_defaults_off_and_keeps_unknown_fields() {
        assert!(!Settings::default().clip.auto_copy);
        assert_eq!(Settings::default().version, 2);
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"clip": {"auto_copy": true, "later": 1}}"#).unwrap();
        let (settings, _) = Settings::load(&path);
        assert!(settings.clip.auto_copy);
        settings.save(&path).unwrap();
        assert_eq!(read_json(&path)["clip"]["later"], json!(1));
        fs::write(&path, r#"{"images": {}}"#).unwrap();
        assert_eq!(Settings::load(&path).0.clip, Clip::default());
    }

    // stream A: video
    #[test]
    fn video_defaults_match_the_plan() {
        let v = Settings::default().video;
        assert_eq!(
            (
                v.encoder,
                v.compression,
                v.convert_mov,
                v.audio_to_aac,
                v.remove_audio
            ),
            (Encoder::Fast, 80, true, false, false)
        );
        assert_eq!(
            (v.fps_cap, v.speed_frames, v.preserve_dates, v.max_size_mb),
            (FpsCap::Off, SpeedFrames::Keep, true, 4000)
        );
        let folder = if cfg!(windows) {
            "~/Videos/Ferry"
        } else {
            "~/Movies/Ferry"
        };
        assert_eq!(v.output.optimised.folder, folder);
        assert_eq!(v.output.optimised.template, "%f_optimised.%e");
        assert_eq!(v.output.converted.template, "%f.%e");
    }

    #[test]
    fn a_file_without_a_video_section_loads_its_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, br#"{"version":2,"images":{"compression":60}}"#).unwrap();
        let (s, status) = Settings::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(s.images.compression, 60);
        assert_eq!(s.video, Video::default());
    }

    #[test]
    fn video_values_out_of_range_are_repaired() {
        let mut s = Settings::default();
        s.video.compression = 0;
        assert_eq!(s.clone().sanitised().video.compression, 5);
        s.video.compression = 250;
        s.video.max_size_mb = 0;
        s.video.output.optimised.template = "%q".into();
        s.video.output.converted.folder = " ".into();
        s.video.ignored_extensions = vec![" mov ".into(), "  ".into()];
        let s = s.sanitised();
        assert_eq!(s.video.compression, 100);
        assert_eq!(s.video.max_size_mb, 1);
        assert_eq!(s.video.output.optimised.template, "%f_optimised.%e");
        assert_eq!(
            s.video.output.converted.folder,
            Video::default().output.converted.folder
        );
        assert_eq!(s.video.ignored_extensions, ["mov"]);
    }

    #[test]
    fn the_fps_cap_reads_and_writes_as_text() {
        assert_eq!(serde_json::to_string(&FpsCap::Fps30).unwrap(), "\"30\"");
        assert_eq!(FpsCap::Fps60.fps(), Some(60));
    }

    #[test]
    fn the_video_encoder_setting_reads_snake_case() {
        let s: Settings =
            serde_json::from_str(r#"{"video":{"encoder":"visually_lossless"}}"#).unwrap();
        assert_eq!(s.video.encoder, Encoder::VisuallyLossless);
        let s: Settings = serde_json::from_str(r#"{"video":{"encoder":"small"}}"#).unwrap();
        assert_eq!(s.video.encoder, Encoder::Small);
    }

    #[test]
    fn drop_zone_defaults_match_the_plan() {
        let s = Settings::default();
        let z = &s.drop_zone;
        assert!(z.enabled);
        // Windows shows the zone only on a modifier tap unless the user opts in to the edge tab.
        let mac = cfg!(target_os = "macos");
        let show_on = if mac {
            ZoneShowOn::AnyDrag
        } else {
            ZoneShowOn::Modifier
        };
        assert_eq!(
            (z.show_on, z.position),
            (show_on, ZonePosition::NearResults)
        );
        assert_eq!(
            (z.modifier, z.keep_original_modifier),
            (DragKey::Alt, DragKey::Shift)
        );
        assert_eq!(z.batch_threshold, 20);
        let figma = if mac {
            "com.figma.Desktop"
        } else {
            "Figma.exe"
        };
        assert!(z.ignored_apps.iter().any(|a| a == figma));
        assert_eq!(s.results_list.after_cards, 5);
        let json = serde_json::to_value(z).unwrap();
        assert_eq!(json["show_on"], if mac { "any_drag" } else { "modifier" });
        assert_eq!(json["keep_original_modifier"], "shift");
    }

    #[test]
    fn a_file_without_the_drop_sections_gets_their_defaults_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 2, "drop_zone": {"future": 1}}"#).unwrap();
        let (s, _) = Settings::load(&path);
        assert_eq!(s.drop_zone.batch_threshold, 20);
        assert_eq!(s.results_list, ResultsList::default());
        s.save(&path).unwrap();
        assert_eq!(read_json(&path)["drop_zone"]["future"], 1);
    }

    #[test]
    fn drop_settings_are_clamped_and_the_two_modifiers_kept_apart() {
        let mut s = Settings::default();
        s.drop_zone.batch_threshold = 0;
        s.results_list.after_cards = 99;
        s.drop_zone.modifier = DragKey::Shift;
        s.drop_zone.keep_original_modifier = DragKey::Shift;
        s.drop_zone.ignored_apps = vec!["  ".into(), " com.x ".into()];
        let s = s.sanitised();
        assert_eq!(s.drop_zone.batch_threshold, 1);
        assert_eq!(s.results_list.after_cards, 50);
        assert_eq!(s.drop_zone.keep_original_modifier, DragKey::Alt);
        assert_eq!(s.drop_zone.ignored_apps, ["com.x"]);
        let mut big = Settings::default();
        big.drop_zone.batch_threshold = 50_000;
        big.results_list.after_cards = 0;
        let big = big.sanitised();
        assert_eq!(
            (big.drop_zone.batch_threshold, big.results_list.after_cards),
            (10_000, 1)
        );
    }

    // ---- Stream H: integrations ----
    fn app_ref(id: &str, name: &str) -> AppRef {
        AppRef {
            id: id.into(),
            name: name.into(),
        }
    }

    #[test]
    fn share_defaults_are_empty_and_a_version_2_file_loads_them() {
        let s = Settings::default();
        assert_eq!(s.share, Share::default());
        assert_eq!(s.share.edit_with.get(MediaKind::Image), None);
        assert!(s.share.uploader.is_none() && s.share.shelf_apps.is_empty());
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 2, "images": {"compression": 70}}"#).unwrap();
        let (loaded, _) = Settings::load(&path);
        assert_eq!(loaded.share, Share::default());
        assert_eq!(loaded.version, 2);
    }

    #[test]
    fn share_round_trips_and_keeps_unknown_fields() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"version": 2, "share": {"future": 1, "edit_with": {"image": {"id": "com.pixelmatorteam.pixelmator.x", "name": "Pixelmator Pro"}, "later": true}}}"#,
        )
        .unwrap();
        let (loaded, _) = Settings::load(&path);
        assert_eq!(
            loaded.share.edit_with.get(MediaKind::Image),
            Some(&app_ref(
                "com.pixelmatorteam.pixelmator.x",
                "Pixelmator Pro"
            ))
        );
        loaded.save(&path).unwrap();
        let json = read_json(&path);
        assert_eq!(json["share"]["future"], json!(1));
        assert_eq!(json["share"]["edit_with"]["later"], json!(true));
        assert_eq!(json["share"]["uploader"], json!(null));
    }

    #[test]
    fn sanitising_drops_blank_apps_and_twin_shelves() {
        let mut s = Settings::default();
        s.share.uploader = Some(app_ref("  ", "Nothing"));
        s.share.edit_with.set(
            MediaKind::Pdf,
            Some(app_ref(" /Applications/PDF Expert.app ", "")),
        );
        s.share.shelf_apps = vec![
            app_ref("com.x", "X"),
            app_ref("COM.X", "X again"),
            app_ref("", "Blank"),
        ];
        let s = s.sanitised();
        assert_eq!(s.share.uploader, None);
        assert_eq!(
            s.share.edit_with.get(MediaKind::Pdf),
            Some(&app_ref("/Applications/PDF Expert.app", "PDF Expert"))
        );
        assert_eq!(s.share.shelf_apps, [app_ref("com.x", "X")]);
    }
}
