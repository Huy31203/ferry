//! What Ferry does with the clipboard (spec §5.2, §5.3). Pure decisions over a
//! snapshot; the app reads the clipboard and starts the jobs.

use std::path::{Path, PathBuf};

use crate::i18n::{t, tf};
use crate::run_state::RunState;
use crate::settings::Settings;
use crate::sniff::Format;

/// The clipboard at one moment. Image bytes are not read for concealed content.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// The clipboard's change count (macOS) or sequence number (Windows).
    pub seq: u64,
    pub image: Option<(Format, Vec<u8>)>,
    /// Windows: the image was only a bitmap and `image` is a lossless PNG of it.
    pub from_bitmap: bool,
    pub files: Vec<PathBuf>,
    pub concealed: bool,
    /// macOS bundle id or Windows executable name of the app that copied.
    pub source_app: Option<String>,
}

/// Why the clipboard was left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    NotRunning,
    Concealed,
    IgnoredApp,
    NoImage,
    MultipleFiles,
    TooLarge {
        limit_mb: u32,
    },
    IgnoredExtension(String),
    NeedsGifsicle,
    /// A folder where a file was expected; folders go to the drop zone.
    Folder(String),
}

impl Skip {
    /// The Error-card text when the user asked with a hotkey.
    pub fn reason(&self) -> String {
        match self {
            Skip::Concealed => t("error.concealed"),
            Skip::MultipleFiles => t("error.multiple_files"),
            Skip::TooLarge { limit_mb } => {
                tf("error.too_large", &[("limit", &limit_mb.to_string())])
            }
            Skip::IgnoredExtension(ext) => tf("error.ignored_extension", &[("ext", ext)]),
            Skip::NeedsGifsicle => t("error.gifsicle_missing"),
            Skip::Folder(name) => tf("error.folder", &[("name", name)]),
            Skip::Disabled | Skip::NotRunning | Skip::IgnoredApp | Skip::NoImage => {
                t("error.nothing_to_optimise")
            }
        }
    }
}

/// What the optimise hotkeys act on.
#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyPlan {
    Image(Format),
    Files(Vec<PathBuf>),
    Nothing(Skip),
}

/// Bytes in a settings megabyte; sizes are shown in decimal units too.
pub const MB: u64 = 1_000_000;

/// Lower-case, no leading dot, `jpeg` / `jpe` count as `jpg` and `aif` / `aifc` as `aiff`.
pub fn normalise_ext(ext: &str) -> String {
    let ext = ext.trim().trim_start_matches('.').to_lowercase();
    match ext.as_str() {
        "jpeg" | "jpe" => "jpg".into(),
        // stream B: audio
        "aif" | "aifc" => "aiff".into(),
        // sE pdf-formats
        "tif" => "tiff".into(),
        "heif" => "heic".into(),
        _ => ext,
    }
}

pub fn is_ignored_extension(ignored: &[String], ext: &str) -> bool {
    let ext = normalise_ext(ext);
    ignored.iter().any(|i| normalise_ext(i) == ext)
}

/// Concealed content and ignored apps are never touched, whatever is on the clipboard.
fn check_source(snap: &Snapshot, settings: &Settings) -> Result<(), Skip> {
    if snap.concealed {
        return Err(Skip::Concealed);
    }
    if let Some(app) = &snap.source_app
        && settings
            .clipboard
            .ignored_apps
            .iter()
            .any(|i| i.eq_ignore_ascii_case(app))
    {
        return Err(Skip::IgnoredApp);
    }
    Ok(())
}

/// The content rules every clipboard image must pass, whoever asked. Copied
/// files win over image data: Finder and Explorer put the file's icon beside them.
pub fn check_image(
    snap: &Snapshot,
    settings: &Settings,
    has_gifsicle: bool,
) -> Result<Format, Skip> {
    check_source(snap, settings)?;
    let Some((format, bytes)) = &snap.image else {
        return Err(Skip::NoImage);
    };
    if !snap.files.is_empty() {
        return Err(Skip::NoImage);
    }
    let limit_mb = settings.clipboard.max_size_mb;
    if bytes.len() as u64 > u64::from(limit_mb) * MB {
        return Err(Skip::TooLarge { limit_mb });
    }
    if is_ignored_extension(&settings.images.ignored_extensions, format.extension()) {
        return Err(Skip::IgnoredExtension(format.extension().into()));
    }
    if *format == Format::Gif && !has_gifsicle {
        return Err(Skip::NeedsGifsicle);
    }
    Ok(*format)
}

/// Whether an automatic trigger could do anything, so the clipboard need not
/// be read (and held open) for nothing. Paused still reads: it spends its skip
/// on an image.
pub fn wants_read(settings: &Settings, state: RunState) -> bool {
    settings.clipboard.enabled && state != RunState::Stopped
}

/// An automatic trigger (§5.2). The run state is asked last, so Paused is spent
/// on an image Ferry would really have optimised, not on copied text.
pub fn automatic(
    snap: &Snapshot,
    settings: &Settings,
    has_gifsicle: bool,
    state: &mut RunState,
) -> Result<Format, Skip> {
    if !settings.clipboard.enabled {
        return Err(Skip::Disabled);
    }
    let format = check_image(snap, settings, has_gifsicle)?;
    if !state.take_automatic() {
        return Err(Skip::NotRunning);
    }
    Ok(format)
}

/// An automatic trigger for copied files (FR-T3), only with "Optimise copied
/// video and PDF files" on. `wanted` says whether a file is a video or a PDF;
/// the others are left alone. As for images, the run state is asked last, so a
/// Paused skip is spent on files Ferry would really have optimised.
pub fn automatic_files(
    snap: &Snapshot,
    settings: &Settings,
    wanted: &dyn Fn(&Path) -> bool,
    state: &mut RunState,
) -> Result<Vec<PathBuf>, Skip> {
    if !settings.clipboard.enabled || !settings.clipboard.optimise_files {
        return Err(Skip::Disabled);
    }
    check_source(snap, settings)?;
    if settings.clipboard.skip_multiple_files && snap.files.len() > 1 {
        return Err(Skip::MultipleFiles);
    }
    let files: Vec<PathBuf> = snap.files.iter().filter(|f| wanted(f)).cloned().collect();
    if files.is_empty() {
        return Err(Skip::NoImage);
    }
    if !state.take_automatic() {
        return Err(Skip::NotRunning);
    }
    Ok(files)
}

/// The optimise hotkeys (§5.3): they run whatever the run state, and act on
/// copied files when there are any.
pub fn hotkey(snap: &Snapshot, settings: &Settings, has_gifsicle: bool) -> HotkeyPlan {
    if snap.concealed {
        return HotkeyPlan::Nothing(Skip::Concealed);
    }
    if !snap.files.is_empty() {
        if settings.clipboard.skip_multiple_files && snap.files.len() > 1 {
            return HotkeyPlan::Nothing(Skip::MultipleFiles);
        }
        return HotkeyPlan::Files(snap.files.clone());
    }
    match check_image(snap, settings, has_gifsicle) {
        Ok(format) => HotkeyPlan::Image(format),
        Err(skip) => HotkeyPlan::Nothing(skip),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn png_snapshot() -> Snapshot {
        Snapshot {
            image: Some((Format::Png, PNG.to_vec())),
            source_app: Some("com.apple.screencaptureui".into()),
            ..Snapshot::default()
        }
    }

    fn auto(snap: &Snapshot, settings: &Settings, state: &mut RunState) -> Result<Format, Skip> {
        automatic(snap, settings, true, state)
    }

    #[test]
    fn a_copied_image_is_optimised_while_running() {
        let mut state = RunState::Running;
        assert_eq!(
            auto(&png_snapshot(), &Settings::default(), &mut state),
            Ok(Format::Png)
        );
    }

    #[test]
    fn paused_is_spent_on_an_image_not_on_text() {
        let settings = Settings::default();
        let mut state = RunState::Paused;
        assert_eq!(
            auto(&Snapshot::default(), &settings, &mut state),
            Err(Skip::NoImage)
        );
        assert_eq!(
            state,
            RunState::Paused,
            "copied text must not use up the pause"
        );
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Err(Skip::NotRunning)
        );
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Ok(Format::Png)
        );
    }

    #[test]
    fn nothing_is_read_when_automatic_work_is_off() {
        let mut settings = Settings::default();
        assert!(wants_read(&settings, RunState::Running));
        assert!(wants_read(&settings, RunState::Paused));
        assert!(!wants_read(&settings, RunState::Stopped));
        settings.clipboard.enabled = false;
        assert!(!wants_read(&settings, RunState::Running));
    }

    #[test]
    fn the_clipboard_switch_turns_automatic_work_off() {
        let mut settings = Settings::default();
        settings.clipboard.enabled = false;
        let mut state = RunState::Running;
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Err(Skip::Disabled)
        );
    }

    #[test]
    fn concealed_content_is_never_touched() {
        let snap = Snapshot {
            concealed: true,
            ..png_snapshot()
        };
        let mut state = RunState::Running;
        assert_eq!(
            auto(&snap, &Settings::default(), &mut state),
            Err(Skip::Concealed)
        );
        assert_eq!(
            hotkey(&snap, &Settings::default(), true),
            HotkeyPlan::Nothing(Skip::Concealed)
        );
    }

    #[test]
    fn an_ignored_app_matches_without_case() {
        let mut settings = Settings::default();
        settings.clipboard.ignored_apps = vec!["COM.Apple.ScreenCaptureUI".into()];
        let mut state = RunState::Running;
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Err(Skip::IgnoredApp)
        );
    }

    #[test]
    fn the_size_limit_is_in_decimal_megabytes() {
        let mut settings = Settings::default();
        settings.clipboard.max_size_mb = 1;
        let mut bytes = PNG.to_vec();
        bytes.resize(1_000_000, 0);
        let mut snap = Snapshot {
            image: Some((Format::Png, bytes.clone())),
            ..png_snapshot()
        };
        assert_eq!(check_image(&snap, &settings, true), Ok(Format::Png));
        bytes.push(0);
        snap.image = Some((Format::Png, bytes));
        assert_eq!(
            check_image(&snap, &settings, true),
            Err(Skip::TooLarge { limit_mb: 1 })
        );
    }

    #[test]
    fn ignored_extensions_match_jpg_and_jpeg_in_any_case_with_or_without_a_dot() {
        let mut settings = Settings::default();
        settings.images.ignored_extensions = vec![".JPEG".into()];
        let snap = Snapshot {
            image: Some((Format::Jpeg, vec![0xFF, 0xD8, 0xFF])),
            ..png_snapshot()
        };
        assert_eq!(
            check_image(&snap, &settings, true),
            Err(Skip::IgnoredExtension("jpg".into()))
        );
        assert!(is_ignored_extension(
            &["jpg".into()],
            "Photo.JPE".rsplit('.').next().unwrap()
        ));
        assert!(!is_ignored_extension(&["png".into()], "jpg"));
        // stream B: audio
        assert_eq!(normalise_ext(".AIF"), "aiff");
        assert_eq!(normalise_ext("aifc"), "aiff");
    }

    // sE pdf-formats
    #[test]
    fn tif_and_heif_count_as_tiff_and_heic() {
        assert_eq!(normalise_ext(".TIF"), "tiff");
        assert_eq!(normalise_ext("heif"), "heic");
        assert!(is_ignored_extension(&["tif".into()], "tiff"));
    }

    #[test]
    fn a_gif_needs_the_sidecar() {
        let snap = Snapshot {
            image: Some((Format::Gif, b"GIF89a".to_vec())),
            ..png_snapshot()
        };
        assert_eq!(
            check_image(&snap, &Settings::default(), false),
            Err(Skip::NeedsGifsicle)
        );
        assert_eq!(
            check_image(&snap, &Settings::default(), true),
            Ok(Format::Gif)
        );
    }

    #[test]
    fn copied_files_are_left_alone_automatically_but_taken_by_the_hotkey() {
        let files = vec![PathBuf::from("/tmp/a.png")];
        let snap = Snapshot {
            files: files.clone(),
            ..png_snapshot()
        };
        let mut state = RunState::Running;
        assert_eq!(
            auto(&snap, &Settings::default(), &mut state),
            Err(Skip::NoImage)
        );
        assert_eq!(
            hotkey(&snap, &Settings::default(), true),
            HotkeyPlan::Files(files)
        );
    }

    #[test]
    fn more_than_one_file_is_refused_unless_allowed() {
        let files = vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")];
        let snap = Snapshot {
            files: files.clone(),
            ..Snapshot::default()
        };
        let mut settings = Settings::default();
        assert_eq!(
            hotkey(&snap, &settings, true),
            HotkeyPlan::Nothing(Skip::MultipleFiles)
        );
        settings.clipboard.skip_multiple_files = false;
        assert_eq!(hotkey(&snap, &settings, true), HotkeyPlan::Files(files));
    }

    #[test]
    fn the_hotkey_ignores_the_run_state_and_the_clipboard_switch() {
        let mut settings = Settings::default();
        settings.clipboard.enabled = false;
        assert_eq!(
            hotkey(&png_snapshot(), &settings, true),
            HotkeyPlan::Image(Format::Png)
        );
    }

    #[test]
    fn an_empty_clipboard_tells_the_hotkey_user_why() {
        let plan = hotkey(&Snapshot::default(), &Settings::default(), true);
        assert_eq!(plan, HotkeyPlan::Nothing(Skip::NoImage));
        assert_eq!(
            Skip::NoImage.reason(),
            "Nothing to optimise on the clipboard"
        );
        assert_eq!(Skip::MultipleFiles.reason(), "More than one file copied");
    }

    fn files_snapshot(names: &[&str]) -> Snapshot {
        Snapshot {
            files: names.iter().map(PathBuf::from).collect(),
            ..Snapshot::default()
        }
    }

    fn files_on() -> Settings {
        let mut settings = Settings::default();
        settings.clipboard.optimise_files = true;
        settings
    }

    fn videos_only(path: &Path) -> bool {
        path.extension().is_some_and(|e| e == "mp4")
    }

    #[test]
    fn copied_files_are_left_alone_unless_the_setting_is_on() {
        let snap = files_snapshot(&["a.mp4"]);
        let mut state = RunState::Running;
        assert_eq!(
            automatic_files(&snap, &Settings::default(), &videos_only, &mut state),
            Err(Skip::Disabled)
        );
        assert_eq!(
            automatic_files(&snap, &files_on(), &videos_only, &mut state),
            Ok(vec![PathBuf::from("a.mp4")])
        );
        let mut off = files_on();
        off.clipboard.enabled = false;
        assert_eq!(
            automatic_files(&snap, &off, &videos_only, &mut state),
            Err(Skip::Disabled)
        );
    }

    #[test]
    fn copied_files_follow_the_clipboard_rules() {
        let mut state = RunState::Running;
        let concealed = Snapshot {
            concealed: true,
            ..files_snapshot(&["a.mp4"])
        };
        assert_eq!(
            automatic_files(&concealed, &files_on(), &videos_only, &mut state),
            Err(Skip::Concealed)
        );
        let mut settings = files_on();
        settings.clipboard.ignored_apps = vec!["com.example.vault".into()];
        let ignored = Snapshot {
            source_app: Some("COM.example.Vault".into()),
            ..files_snapshot(&["a.mp4"])
        };
        assert_eq!(
            automatic_files(&ignored, &settings, &videos_only, &mut state),
            Err(Skip::IgnoredApp)
        );
        let two = files_snapshot(&["a.mp4", "b.mp4"]);
        assert_eq!(
            automatic_files(&two, &files_on(), &videos_only, &mut state),
            Err(Skip::MultipleFiles)
        );
        settings = files_on();
        settings.clipboard.skip_multiple_files = false;
        assert_eq!(
            automatic_files(&two, &settings, &videos_only, &mut state)
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn paused_is_spent_on_a_video_or_pdf_not_on_other_files() {
        let mut state = RunState::Paused;
        let other = files_snapshot(&["notes.txt"]);
        assert_eq!(
            automatic_files(&other, &files_on(), &videos_only, &mut state),
            Err(Skip::NoImage)
        );
        assert_eq!(state, RunState::Paused);
        let video = files_snapshot(&["a.mp4"]);
        assert_eq!(
            automatic_files(&video, &files_on(), &videos_only, &mut state),
            Err(Skip::NotRunning)
        );
        assert!(automatic_files(&video, &files_on(), &videos_only, &mut state).is_ok());
    }
}
