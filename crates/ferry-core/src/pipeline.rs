//! One job end to end (spec §5.4): back up the original, optimise it, write the
//! result and report each step as a `JobUpdate`. The app runs this on a worker
//! thread and owns the clipboard, which it reaches through a `deliver` callback.
//! The user's pipelines (pipeline_def, pipeline_plan) compile into the `Edits` recipe that `Job::run` applies here; this module is the job runner, not the pipeline engine.

use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

use chrono::{Local, NaiveDateTime};
use serde::Serialize;

use crate::backup::Backups;
use crate::cancel::CancelToken;
use crate::edit::{self, Edits, EditsPatch};
use crate::fsutil::{self, Times};
use crate::i18n::{t, tf};
use crate::job::{JobId, JobState, JobUpdate, Source};
use crate::optimise::{self, Level, OptimiseError, OptimiseOptions, Outcome};
use crate::output::{self, OutputKind};
use crate::settings::{self as settings_model, Location, Settings, contract_home};
use crate::sniff::{self, Format};
use crate::template;
use crate::trigger::{MB, Skip, is_ignored_extension, normalise_ext};
// stream A: video
use crate::video::{self, hardware::Hardware};
use video_job::VideoState;

mod video_job;

// ---- stream B: audio ----
mod audio;
use audio::AudioJob;
pub use audio::AudioRunOptions;

/// Puts an image on the clipboard: format, bytes and the working file holding them.
pub type Deliver<'a> = &'a mut dyn FnMut(Format, &[u8], &Path) -> Result<(), String>;

/// A result or an original: bytes in memory, or a file (a video is never read
/// into memory).
#[derive(Clone, Copy)]
enum Content<'a> {
    Bytes(&'a [u8]),
    File(&'a Path),
}

/// What a clipboard branch says when it is handed a file.
fn in_memory<'a>(content: Content<'a>) -> Result<&'a [u8], (String, String)> {
    match content {
        Content::Bytes(bytes) => Ok(bytes),
        Content::File(_) => Err((
            t("error.unexpected"),
            "a clipboard result must be in memory".into(),
        )),
    }
}

/// Settings a run reads, taken when the run starts.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub compression: u8,
    pub strip_metadata: bool,
    pub preserve_dates: bool,
    pub gifsicle: Option<PathBuf>,
    pub cancel: CancelToken,
    pub output: settings_model::Output,
    /// The working folder; Temporary results go in `files/<job id>/`.
    pub work_dir: PathBuf,
    pub home: PathBuf,
    /// Moves a file Ferry made, or a converted-away source, to the Trash.
    /// The app logs failures; here a failure only means the file stays.
    pub trash: fn(&Path) -> io::Result<()>,
    // stream A: video
    pub video: settings_model::Video,
    /// `None`: `run_video` asks `video::hardware::probe_hardware`, which waits
    /// for the start-up probe. Tests set `Some(Hardware::default())` for
    /// deterministic software encodes.
    pub hardware: Option<Hardware>,
    // stream B: audio
    pub audio: AudioRunOptions,
    // stream F: pipelines
    /// Adaptive compression for plain runs (`images.adaptive`).
    pub adaptive: bool,
    /// A plain run may switch PNG and JPEG (`images.switch_png_jpeg`).
    pub switch_png_jpeg: bool,
}

impl RunOptions {
    pub fn new(
        settings: &Settings,
        gifsicle: Option<PathBuf>,
        work_dir: PathBuf,
        trash: fn(&Path) -> io::Result<()>,
        cancel: CancelToken,
    ) -> Self {
        Self {
            compression: settings.images.compression,
            strip_metadata: settings.images.strip_metadata,
            preserve_dates: settings.images.preserve_dates,
            gifsicle,
            cancel,
            output: settings.images.output.clone(),
            work_dir,
            home: std::env::home_dir().unwrap_or_default(),
            trash,
            // stream A: video
            video: settings.video.clone(),
            hardware: None,
            // stream B: audio
            audio: AudioRunOptions::new(settings),
            adaptive: settings.images.adaptive, // stream F: pipelines
            switch_png_jpeg: settings.images.switch_png_jpeg, // stream F: pipelines
        }
    }
}

/// How a run ended. `cause` is the underlying error, for the log only.
#[derive(Debug)]
pub struct Finished {
    pub update: JobUpdate,
    pub cause: Option<String>,
}

#[derive(Debug)]
pub struct Job {
    pub id: JobId,
    pub source: Source,
    /// Shown on the card: the rendered template, or the file's own stem.
    pub name: String,
    /// The current result's extension: the source's own until a conversion.
    pub ext: String,
    /// The current result's format.
    pub format: Format,
    pub before: u64,
    /// The original's size as shown (JPEG orientation applied).
    pub dims: Option<(u32, u32)>,
    pub from_bitmap: bool,
    /// The working file of a clipboard job, or the file job's current result.
    pub output: PathBuf,
    /// The level of the latest run; retry and re-optimise use it again.
    pub level: Level,
    /// What the card asked for; every run applies all of it to the original.
    pub edits: Edits,
    /// The recipe and level before the pending run's request, for a cancel.
    prior: Option<(Edits, Level)>,
    source_format: Format,
    source_ext: String,
    animated: bool,
    /// A file job's own location: where an in-place result goes.
    source_path: Option<PathBuf>,
    /// A file job's current result, so the next run can take it back.
    last: Option<Written>,
    /// What became of the source file on disk.
    source_state: SourceState,
    /// The name the user gave the result by renaming it; later results keep it.
    stem: Option<String>,
    dims_after: Option<(u32, u32)>,
    note: Option<String>,
    /// The original, once stored. Every run starts from it (R§2.4).
    backup: Option<PathBuf>,
    /// A clipboard job's original until it is backed up.
    pending: Option<Vec<u8>>,
    /// A file job's times, read before its first write.
    times: Option<Times>,
    /// `output` (and the clipboard) hold a result, not the original.
    changed: bool,
    /// A "Convert and keep both" twin: the source belongs to the parent's
    /// card, so the twin never overwrites, trashes or restores it.
    twin: bool,
    // stream A: video
    video: Option<VideoState>,
    // stream B: audio
    audio: Option<AudioJob>,
}

/// A file job's result on disk (spec §4.4: one result per job).
#[derive(Debug, Clone)]
struct Written {
    path: PathBuf,
    /// Ferry made this file; it is not the source.
    created: bool,
}

/// A file as Ferry left it: if either differs later, someone else changed it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Stamp {
    len: u64,
    modified: SystemTime,
}

impl Stamp {
    fn of(path: &Path) -> Option<Self> {
        let meta = fs::metadata(path).ok()?;
        Some(Self {
            len: meta.len(),
            modified: meta.modified().ok()?,
        })
    }
}

/// What became of a file job's source (the original, where it came from).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SourceState {
    /// Ferry has not touched it. The stamp is the file as Ferry last saw it
    /// (backed up, or put back by Restore); `None`: it could not be read.
    Untouched(Option<Stamp>),
    /// Ferry wrote its result over it. `None`: the file could not be read
    /// back, so nothing can tell if it changed.
    Overwritten(Option<Stamp>),
    /// Ferry moved it to the Trash for a conversion.
    Trashed,
}

/// Why a rename was refused; the card shows `message()` inline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenameError {
    Empty,
    /// Separators, characters Windows forbids, a trailing dot or space, or a
    /// reserved device name. Refused on every OS so names travel.
    Invalid,
    Taken,
    Gone,
    Failed {
        reason: String,
    },
}

impl RenameError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => t("rename.empty"),
            Self::Invalid => t("rename.invalid"),
            Self::Taken => t("rename.taken"),
            Self::Gone => t("rename.gone"),
            Self::Failed { reason } => reason.clone(),
        }
    }
}

/// A file stem every OS accepts.
fn check_stem(stem: &str) -> Result<(), RenameError> {
    if stem.trim().is_empty() {
        return Err(RenameError::Empty);
    }
    let forbidden = |c: char| {
        matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*') || c.is_control()
    };
    let device = stem
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((device.starts_with("COM") || device.starts_with("LPT"))
            && device.len() == 4
            && device.as_bytes()[3].is_ascii_digit()
            && device.as_bytes()[3] != b'0');
    if stem.contains(forbidden) || stem.ends_with(['.', ' ']) || reserved {
        return Err(RenameError::Invalid);
    }
    Ok(())
}

impl Job {
    /// A clipboard image. Its result goes to
    /// `<work_dir>/clipboard/<job id>/<rendered template>.<ext>`, so two jobs
    /// with the same name never share a file.
    pub fn clipboard(
        format: Format,
        bytes: Vec<u8>,
        from_bitmap: bool,
        file_template: &str,
        work_dir: &Path,
        now: NaiveDateTime,
    ) -> Job {
        let id = JobId::generate();
        let ext = format.extension();
        let dims = edit::raster::display_dims(&bytes, format);
        let animated = sniff::is_animated(&bytes, format);
        let name = template::clipboard_name(file_template, ext, dims.unwrap_or_default(), now);
        let output = work_dir
            .join("clipboard")
            .join(id.as_str())
            .join(format!("{name}.{ext}"));
        Job {
            id,
            source: Source::Clipboard,
            name,
            ext: ext.into(),
            format,
            before: bytes.len() as u64,
            dims,
            from_bitmap,
            output,
            level: Level::Normal,
            edits: Edits::default(),
            prior: None,
            source_format: format,
            source_ext: ext.into(),
            animated,
            source_path: None,
            last: None,
            source_state: SourceState::Untouched(None),
            stem: None,
            dims_after: None,
            note: None,
            backup: None,
            pending: Some(bytes),
            times: None,
            changed: false,
            twin: false,
            video: None,
            audio: None,
        }
    }

    /// A copied file, to be optimised in place. `Err` is the card text for a
    /// file the settings exclude or that is not a supported image.
    pub fn file(path: &Path, settings: &Settings, has_gifsicle: bool) -> Result<Job, String> {
        // stream A: video
        if let Some((container, len)) = video_job::video_container(path) {
            return Job::video_file(path, settings, container, len);
        }
        // stream B: audio
        if let Some((format, len)) = audio::audio_container(path) {
            return Job::audio_file(path, settings, format, len);
        }
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, ext) = match file_name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), ext.to_owned()),
            _ => (file_name.clone(), String::new()),
        };
        if !ext.is_empty() && is_ignored_extension(&settings.images.ignored_extensions, &ext) {
            return Err(Skip::IgnoredExtension(normalise_ext(&ext)).reason());
        }
        let cant_read = |e: io::Error| {
            tf(
                "error.read",
                &[("name", &file_name), ("why", &describe(&e))],
            )
        };
        let len = fs::metadata(path).map_err(cant_read)?.len();
        let limit_mb = settings.images.max_size_mb;
        if len > u64::from(limit_mb) * MB {
            return Err(Skip::TooLarge { limit_mb }.reason());
        }
        let bytes = fs::read(path).map_err(cant_read)?;
        let format = sniff::sniff(&bytes)
            .ok_or_else(|| tf("error.not_an_image", &[("name", &file_name)]))?;
        if format == Format::Gif && !has_gifsicle {
            return Err(Skip::NeedsGifsicle.reason());
        }
        Ok(Job {
            id: JobId::generate(),
            source: Source::File,
            name: stem,
            source_ext: ext.clone(),
            ext,
            format,
            before: len,
            dims: edit::raster::display_dims(&bytes, format),
            from_bitmap: false,
            output: path.to_owned(),
            level: Level::Normal,
            edits: Edits::default(),
            prior: None,
            source_format: format,
            animated: sniff::is_animated(&bytes, format),
            source_path: Some(path.to_owned()),
            last: None,
            source_state: SourceState::Untouched(None),
            stem: None,
            dims_after: None,
            note: None,
            backup: None,
            pending: None,
            times: None,
            changed: false,
            twin: false,
            video: None,
            audio: None,
        })
    }

    /// This job's card in `state`.
    pub fn update(&self, state: JobState, after: Option<u64>) -> JobUpdate {
        let done = state == JobState::Done;
        let busy = matches!(state, JobState::Queued | JobState::Optimising { .. });
        let editable = self.source_format == Format::Gif || !self.animated;
        let backed_up = self.backup.as_ref().is_some_and(|b| b.exists());
        let edit_hint = if busy || (editable && backed_up) {
            None
        } else if !editable {
            Some(tf(
                "error.animated_edit",
                &[("format", self.source_format.label())],
            ))
        } else {
            Some(t("card.no_original"))
        };
        JobUpdate {
            id: self.id.clone(),
            can_restore: done && self.backup.is_some(),
            can_retry: matches!(state, JobState::Error { .. }),
            detail: if done { self.detail() } else { String::new() },
            name: self.name.clone(),
            ext: self.ext.clone(),
            before: self.before,
            after,
            // stream A: video
            thumb_path: match (&self.video, &self.audio) {
                (Some(video), _) => video.thumb.clone(),
                // stream B: audio (the original's cover)
                (None, Some(audio)) => audio.cover_file.clone(),
                (None, None) => self.backup.clone(),
            },
            source: self.source,
            output: self.output.exists().then(|| self.output.clone()),
            dims_before: self.dims,
            dims_after: if done { self.dims_after } else { None },
            format: Some(self.format),
            converted: self.format != self.source_format,
            can_edit: !busy && editable && backed_up,
            edits: self.edits.clone(),
            note: if done { self.note.clone() } else { None },
            source_format: Some(self.source_format),
            edit_hint,
            state,
            // stream A: video
            video: self.video_meta(),
            // stream B: audio
            audio: self.audio.as_ref().map(|_| self.audio_card()),
        }
    }

    /// Rename the current result in its folder (a file job's real file, a
    /// clipboard job's working file). Later runs keep the new stem.
    pub fn rename(&mut self, stem: &str) -> Result<(), RenameError> {
        check_stem(stem)?;
        if !self.output.exists() {
            return Err(RenameError::Gone);
        }
        let file = if self.ext.is_empty() {
            stem.to_owned()
        } else {
            format!("{stem}.{}", self.ext)
        };
        let to = self.output.with_file_name(file);
        // On a case-insensitive disk "Shot.png" exists because it is "shot.png".
        let same_file = fs::canonicalize(&to).ok() == fs::canonicalize(&self.output).ok();
        if to.exists() && !same_file {
            return Err(RenameError::Taken);
        }
        fs::rename(&self.output, &to).map_err(|e| RenameError::Failed {
            reason: describe(&e),
        })?;
        crate::selfwrite::record(&to); // stream C
        if !self.twin {
            // The source follows its name: it is the very file that was renamed,
            // or it is in the Trash and Restore will bring it back under this name.
            if self.source_path.as_ref() == Some(&self.output) {
                self.source_path = Some(to.clone());
            } else if self.source_state == SourceState::Trashed {
                let file = if self.source_ext.is_empty() {
                    stem.to_owned()
                } else {
                    format!("{stem}.{}", self.source_ext)
                };
                self.source_path = Some(to.with_file_name(file));
            }
        }
        self.stem = Some(stem.to_owned());
        if let Some(last) = self.last.as_mut().filter(|w| w.path == self.output) {
            last.path = to.clone();
        }
        self.output = to;
        self.name = stem.to_owned();
        Ok(())
    }

    /// Merge what the card asked for. Asking for the source format, or for
    /// GIF, clears the conversion.
    pub fn edit(&mut self, patch: EditsPatch) {
        self.prior = Some((self.edits.clone(), self.level));
        self.edits.merge(patch);
        // stream A: video (a video can become a GIF; only its own targets are kept)
        let cleared = match (&self.video, &self.audio) {
            (Some(_), _) => matches!(
                self.edits.format,
                Some(f) if f == self.source_format || !video::is_target(f)
            ),
            // stream B: audio (the source format stays: "same format, ignore the rule")
            (None, Some(_)) => matches!(
                self.edits.format,
                Some(f) if !crate::audio::TARGETS.contains(&f)
            ),
            (None, None) => matches!(
                self.edits.format,
                Some(f) if f == self.source_format || f == Format::Gif || f.is_audio()
            ),
        };
        if cleared {
            self.edits.format = None;
        }
        self.level = self.edits.level;
    }

    /// A cancelled run leaves the file as the earlier run made it, so the
    /// request that started it is forgotten too. The app calls this for a job
    /// cancelled before its worker started.
    pub fn cancel_recipe(&mut self) {
        if let Some((edits, level)) = self.prior.take() {
            self.edits = edits;
            self.level = level;
        }
    }

    /// A second job on the same original, for "Convert and keep both". It has
    /// its own id, card and result; the backup is shared.
    pub fn fork(&self) -> Job {
        let id = JobId::generate();
        let output = match self.source {
            Source::Clipboard => self
                .output
                .parent()
                .and_then(Path::parent)
                .unwrap_or(Path::new(""))
                .join(id.as_str())
                .join(format!("{}.{}", self.name, self.source_ext)),
            Source::File => Self::twin_output(&self.source_path, &id),
        };
        Job {
            id,
            source: self.source,
            name: self.name.clone(),
            ext: self.source_ext.clone(),
            format: self.source_format,
            before: self.before,
            dims: self.dims,
            from_bitmap: self.from_bitmap,
            output,
            level: self.level,
            edits: self.edits.clone(),
            prior: None,
            source_format: self.source_format,
            source_ext: self.source_ext.clone(),
            animated: self.animated,
            source_path: self.source_path.clone(),
            last: None,
            source_state: SourceState::Untouched(None),
            stem: self.stem.clone(),
            dims_after: None,
            note: None,
            backup: self.backup.clone(),
            pending: self.pending.clone(),
            times: self.times,
            changed: false,
            twin: true,
            video: self.video.clone(),
            audio: self.audio.clone(),
        }
    }

    /// Where a file twin's card points until it has a result of its own: a
    /// name no file has, never its parent's.
    fn twin_output(source: &Option<PathBuf>, id: &JobId) -> PathBuf {
        let name = format!(".ferry-twin-{}", id.as_str());
        match source {
            Some(source) => source.with_file_name(name),
            None => PathBuf::from(name),
        }
    }

    /// A "Convert and keep both" copy: Restore closes its card.
    pub fn is_twin(&self) -> bool {
        self.twin
    }

    /// Apply the recipe at `level` to the original, write the result and
    /// report each step. `None` means the job was cancelled before anything
    /// was written. A plain run that does not come out smaller leaves the
    /// original as the result (Cycle 1); an explicit edit always writes.
    pub fn run(
        &mut self,
        level: Level,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
        deliver: Deliver,
    ) -> Option<Finished> {
        self.prior
            .get_or_insert_with(|| (self.edits.clone(), self.level));
        let finished = self.run_recipe(level, o, backups, emit, deliver);
        match finished {
            Some(_) => self.prior = None,
            None => self.cancel_recipe(),
        }
        finished
    }

    fn run_recipe(
        &mut self,
        level: Level,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
        deliver: Deliver,
    ) -> Option<Finished> {
        self.level = level;
        self.edits.level = level;
        if o.cancel.is_cancelled() {
            return None;
        }
        // stream A: video
        if self.video.is_some() {
            return self.run_video(o, backups, emit);
        }
        // stream B: audio
        if self.audio.is_some() {
            return self.run_audio(o, backups, emit);
        }
        let original = match self.original(backups) {
            Ok(bytes) => bytes,
            Err(e) => return Some(self.fail(tf("error.backup", &[("why", &describe(&e))]), e)),
        };
        // A twin is a copy the user asked for: it always writes, never "no gain".
        let plain = self.edits.is_plain() && !self.twin;
        let step = t(match (plain, self.source_format) {
            (false, _) => "card.step.edit",
            (true, Format::Png) => "card.step.png",
            (true, Format::Jpeg) => "card.step.jpeg",
            (true, Format::WebP) => "card.step.webp",
            (true, Format::Gif) => "card.step.gif",
            // stream A: video (never reached once videos are dispatched earlier)
            (
                true,
                Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv,
            ) => "video.step.reading",
            // stream B: audio never reaches the image engine
            (
                true,
                Format::Mp3
                | Format::M4a
                | Format::Flac
                | Format::Wav
                | Format::Aiff
                | Format::Ogg
                | Format::Opus,
            ) => "card.step.edit",
        });
        emit(self.update(
            JobState::Optimising {
                progress: 0.0,
                step,
            },
            None,
        ));
        let options = OptimiseOptions {
            level,
            compression: o.compression,
            strip_metadata: self.edits.strip_metadata.unwrap_or(o.strip_metadata), // stream F: pipelines
            gifsicle: o.gifsicle.clone(),
            cancel: o.cancel.clone(),
            encoder: self.edits.encoder, // stream F: pipelines
            adaptive: false,             // stream F: pipelines
        };
        // (state, Some((bytes, format, size after, fit target missed)))
        let (state, result) = if plain {
            // stream F: pipelines (switch_png_jpeg may pick the other format)
            match optimise::switch::optimise(
                &original,
                self.source_format,
                &options,
                o.switch_png_jpeg,
            ) {
                Ok((Outcome::Smaller { bytes }, format)) => {
                    (JobState::Done, Some((bytes, format, None, false)))
                }
                Ok((Outcome::AlreadyOptimal, _)) => (JobState::AlreadyOptimal, None),
                Ok((Outcome::Larger, _)) => (JobState::Larger, None),
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        } else {
            match edit::apply(&original, self.source_format, &self.edits, &options) {
                Ok(a) => (
                    JobState::Done,
                    Some((a.bytes, a.format, Some(a.dims), a.missed_target)),
                ),
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        };
        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }
        let written = match result {
            Some((bytes, format, dims_after, missed)) => {
                self.dims_after = dims_after.filter(|d| Some(*d) != self.dims);
                self.note = missed.then(|| {
                    let target = bytes_label(self.edits.fit_under.unwrap_or_default());
                    tf("card.note.fit_missed", &[("size", &target)])
                });
                let after = bytes.len() as u64;
                self.put(
                    Content::Bytes(&bytes),
                    format,
                    Content::Bytes(&original),
                    o,
                    deliver,
                )
                .map(|()| Some(after))
            }
            None => {
                self.dims_after = None;
                self.note = None;
                let replace = self.changed;
                self.put_back(Content::Bytes(&original), replace, o, deliver)
                    .map(|()| None)
            }
        };
        Some(match written {
            Ok(after) => Finished {
                update: self.update(state, after),
                cause: None,
            },
            Err((reason, cause)) => self.fail(reason, cause),
        })
    }

    /// Put the original back (§5.5) and clear the recipe: in place for a file
    /// (and the conversion to the Trash); in the working file and on the
    /// clipboard for a clipboard image.
    pub fn restore(&mut self, o: &RunOptions, deliver: Deliver) -> Finished {
        // stream A: video
        if self.video.is_some() {
            return self.restore_video(o);
        }
        // stream B: audio
        if self.audio.is_some() {
            return self.restore_audio(o);
        }
        let read = self
            .backup
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
            .and_then(fs::read);
        let original = match read {
            Ok(bytes) => bytes,
            Err(e) => return self.fail(t("error.backup_missing"), e),
        };
        if let Err((reason, cause)) = self.put_back(Content::Bytes(&original), true, o, deliver) {
            return self.fail(reason, cause);
        }
        self.edits = Edits::default();
        self.dims_after = None;
        self.note = None;
        Finished {
            update: self.update(JobState::Restored, None),
            cause: None,
        }
    }

    /// The original bytes, backed up first on the first run.
    fn original(&mut self, backups: &Backups) -> io::Result<Vec<u8>> {
        fs::read(self.backup_path(backups)?)
    }

    /// Where the original is stored, backing it up first on the first run.
    fn backup_path(&mut self, backups: &Backups) -> io::Result<PathBuf> {
        if self.backup.is_none() {
            let stored = match self.source {
                Source::Clipboard => {
                    let name = format!("{}.{}", self.name, self.ext);
                    let bytes = self.pending.as_deref().unwrap_or_default();
                    backups.store_bytes(self.id.as_str(), &name, bytes)?
                }
                Source::File => {
                    let source = self
                        .source_path
                        .clone()
                        .unwrap_or_else(|| self.output.clone());
                    self.times = fsutil::read_times(&source).ok();
                    self.source_state = SourceState::Untouched(Stamp::of(&source));
                    backups.store_file(self.id.as_str(), &source)?
                }
            };
            self.backup = Some(stored);
            self.pending = None;
        }
        Ok(self.backup.clone().expect("stored above"))
    }

    /// Make `bytes`, in `format`, the job's one result. A file job's result
    /// goes where the output rules say; a previous result elsewhere is taken
    /// back first (spec §4.4).
    fn put(
        &mut self,
        result: Content,
        format: Format,
        original: Content,
        o: &RunOptions,
        deliver: Deliver,
    ) -> Result<(), (String, String)> {
        let ext = if format == self.source_format {
            self.source_ext.clone()
        } else {
            format.extension().to_owned()
        };
        match self.source {
            Source::Clipboard => {
                let bytes = in_memory(result)?;
                let path = self.output.with_file_name(format!("{}.{ext}", self.name));
                self.write_work_file(&path, bytes)?;
                deliver(format, bytes, &self.output)
                    .map_err(|e| (t("error.clipboard_write"), e))?;
            }
            Source::File => {
                let source = self
                    .source_path
                    .clone()
                    .unwrap_or_else(|| self.output.clone());
                let kind = if format == self.source_format {
                    OutputKind::Optimised
                } else {
                    OutputKind::Converted
                };
                let dims = self.dims_after.or(self.dims).unwrap_or_default();
                let target = output::Target {
                    source: &source,
                    job: self.id.as_str(),
                    ext: &ext,
                    dims,
                    kind,
                    stem: self.stem.as_deref(),
                };
                // stream A: video
                let rules = if self.video.is_some() {
                    &o.video.output
                } else {
                    &o.output
                };
                let wanted = output::resolve(
                    &target,
                    rules,
                    &o.work_dir,
                    &o.home,
                    Local::now().date_naive(),
                );
                let own = self.last.as_ref().map(|w| w.path.clone());
                // Over the source, unless that is gone to the Trash and
                // something else has taken its place.
                let source_taken = self.source_state == SourceState::Trashed && source.exists();
                let path = if wanted == source && !self.twin && !source_taken {
                    wanted
                } else {
                    // A twin's source is its parent's: free or not, never its result's.
                    let reserved = self.twin.then_some(source.as_path());
                    output::unique_except(&wanted, own.as_deref(), reserved)
                };
                let trash_source = kind == OutputKind::Converted
                    && rules.converted.location == Location::InPlace
                    && path != source
                    && !self.twin;
                self.check_source(path == source || trash_source)?;
                if let Some(last) = self.last.clone().filter(|w| w.path != path) {
                    self.trash_result(&last, o);
                    self.last = None;
                    // The result leaves the source: it gets the original back.
                    if matches!(self.source_state, SourceState::Overwritten(_)) && path != source {
                        self.restore_source(original, o)?;
                    }
                }
                self.write_file(&path, result, &source, o)?;
                if trash_source && self.source_state != SourceState::Trashed {
                    // Only the original, which this job has not trashed yet.
                    let trashed = !source.exists() || (o.trash)(&source).is_ok();
                    if trashed {
                        self.source_state = SourceState::Trashed;
                    }
                } else if path == source {
                    // After write_file, which may set the dates back.
                    self.source_state = SourceState::Overwritten(Stamp::of(&path));
                }
                self.last = Some(Written {
                    path: path.clone(),
                    created: path != source,
                });
                self.output = path;
            }
        }
        self.format = format;
        self.ext = ext;
        self.changed = true;
        Ok(())
    }

    /// Leave the original as the result: a clipboard job's working file holds
    /// it (on the clipboard too when `replace`); a file job's previous result
    /// is taken back.
    fn put_back(
        &mut self,
        original: Content,
        replace: bool,
        o: &RunOptions,
        deliver: Deliver,
    ) -> Result<(), (String, String)> {
        match self.source {
            Source::Clipboard => {
                let original = in_memory(original)?;
                let path = self
                    .output
                    .with_file_name(format!("{}.{}", self.name, self.source_ext));
                self.write_work_file(&path, original)?;
                if replace {
                    deliver(self.source_format, original, &self.output)
                        .map_err(|e| (t("error.clipboard_write"), e))?;
                }
            }
            Source::File if self.twin => {
                // Only its own copy goes; the source is the parent's.
                if let Some(last) = self.last.take() {
                    self.trash_result(&last, o);
                }
                self.output = Self::twin_output(&self.source_path, &self.id);
            }
            Source::File => {
                self.check_source(false)?;
                if let Some(last) = self.last.take() {
                    self.trash_result(&last, o);
                }
                self.restore_source(original, o)?;
                if let Some(source) = &self.source_path {
                    self.output = source.clone();
                    // The original is back under the name it has now, and the
                    // card says so.
                    if let Some(stem) = source.file_stem() {
                        self.name = stem.to_string_lossy().into_owned();
                    }
                }
                self.stem = None;
            }
        }
        self.format = self.source_format;
        self.ext = self.source_ext.clone();
        self.changed = false;
        Ok(())
    }

    /// Refuse to go on when the source holds something Ferry did not leave
    /// there: the user (or another app) changed it since, and a write would
    /// lose that. A source Ferry overwrote is always in play; an untouched one
    /// only when `touching` it (overwriting or trashing it).
    fn check_source(&self, touching: bool) -> Result<(), (String, String)> {
        let left = match self.source_state {
            SourceState::Overwritten(stamp) => stamp,
            SourceState::Untouched(stamp) if touching => stamp,
            _ => None,
        };
        if let Some(left) = left
            && let Some(source) = &self.source_path
            && let Some(now) = Stamp::of(source)
            && now != left
        {
            return Err((
                t("error.source_changed"),
                format!("{} no longer matches what Ferry left", source.display()),
            ));
        }
        Ok(())
    }

    /// Send the previous result to the Trash when Ferry made it.
    fn trash_result(&self, last: &Written, o: &RunOptions) {
        if last.created && last.path.exists() {
            let _ = (o.trash)(&last.path);
        }
    }

    /// Put the original back where it came from: over the result Ferry wrote
    /// there, or, once it went to the Trash, at the same path if that is free.
    /// Something else may have taken the path; that file stays, and the
    /// original goes beside it (and the job follows it).
    fn restore_source(
        &mut self,
        original: Content,
        o: &RunOptions,
    ) -> Result<(), (String, String)> {
        let Some(mut source) = self.source_path.clone() else {
            return Ok(());
        };
        match self.source_state {
            SourceState::Untouched(_) => return Ok(()),
            SourceState::Overwritten(_) => {}
            SourceState::Trashed => source = output::unique(&source, None),
        }
        self.write_file(&source, original, &source, o)?;
        // After the write, which may set the dates back.
        self.source_state = SourceState::Untouched(Stamp::of(&source));
        self.source_path = Some(source);
        Ok(())
    }

    /// Write a file job's result atomically, with the original's extended
    /// attributes, and its dates when it replaces the source.
    fn write_file(
        &self,
        path: &Path,
        content: Content,
        source: &Path,
        o: &RunOptions,
    ) -> Result<(), (String, String)> {
        let folder = path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(folder)
            .and_then(|()| match content {
                Content::Bytes(bytes) => fsutil::atomic_replace(path, bytes),
                Content::File(from) => fsutil::atomic_replace_from(path, from),
            })
            .map_err(|e| (tf("error.write", &[("dir", &shown(folder))]), e.to_string()))?;
        // Best effort: the content is safe; a lost tag or date is not worth an Error card.
        if let Some(backup) = &self.backup {
            let _ = fsutil::copy_xattrs(backup, path);
        }
        // stream A: video
        let preserve = if self.video.is_some() {
            o.video.preserve_dates
        } else {
            o.preserve_dates
        };
        if preserve
            && path == source
            && let Some(times) = &self.times
        {
            let _ = fsutil::set_times(path, times);
        }
        Ok(())
    }

    /// A clipboard job's working file (drag and reveal use it), renamed when
    /// the result's extension changes.
    fn write_work_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), (String, String)> {
        let folder = path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(folder)
            .and_then(|()| fs::write(path, bytes))
            .map_err(|e| (tf("error.write", &[("dir", &shown(folder))]), e.to_string()))?;
        if path != self.output {
            let _ = fs::remove_file(&self.output);
            self.output = path.to_owned();
        }
        Ok(())
    }

    fn fail(&self, reason: String, cause: impl Display) -> Finished {
        Finished {
            update: self.update(JobState::Error { reason }, None),
            cause: Some(cause.to_string()),
        }
    }

    fn reason(&self, e: &OptimiseError) -> String {
        let format = self.source_format.label();
        match e {
            OptimiseError::Decode(_) => tf("error.decode", &[("format", format)]),
            OptimiseError::Encode(_) => tf("error.encode", &[("format", format)]),
            OptimiseError::Unsupported(message) => message.clone(),
            OptimiseError::Cancelled => t("error.cancelled"),
        }
    }

    /// "2880×1800 · PNG" or "2880×1800 → 1440×900 · WebP", plus "aggressive"
    /// and "from bitmap" when they apply.
    fn detail(&self) -> String {
        // stream A: video
        if self.video.is_some() {
            return self.video_detail();
        }
        // stream B: audio
        if self.audio.is_some() {
            return self.audio_detail();
        }
        let mut parts = Vec::new();
        match (self.dims, self.dims_after) {
            (Some((w, h)), Some((aw, ah))) => parts.push(format!("{w}×{h} → {aw}×{ah}")),
            (Some((w, h)), None) => parts.push(format!("{w}×{h}")),
            _ => {}
        }
        parts.push(self.format.label().to_owned());
        if self.level == Level::Aggressive {
            parts.push(t("card.detail.aggressive"));
        }
        if self.from_bitmap {
            parts.push(t("card.detail.bitmap"));
        }
        parts.join(" · ")
    }
}

/// An Error card that belongs to no job, such as "Nothing to optimise on the clipboard".
pub fn notice(reason: String) -> JobUpdate {
    JobUpdate {
        id: JobId::generate(),
        state: JobState::Error { reason },
        name: String::new(),
        ext: String::new(),
        before: 0,
        after: None,
        detail: String::new(),
        thumb_path: None,
        source: Source::Clipboard,
        can_restore: false,
        can_retry: false,
        output: None,
        dims_before: None,
        dims_after: None,
        format: None,
        converted: false,
        can_edit: false,
        edits: Edits::default(),
        note: None,
        source_format: None,
        edit_hint: None,
        // stream A: video
        video: None,
        // stream B: audio
        audio: None,
    }
}

/// "1 MB", "250 KB", "1.5 MB": decimal units, like formatBytes in src/lib/format.ts.
fn bytes_label(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut value, mut unit) = (bytes as f64, 0);
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let text = if unit > 0 && value < 100.0 {
        let one_decimal = format!("{value:.1}");
        one_decimal.trim_end_matches(".0").to_owned()
    } else {
        format!("{value:.0}")
    };
    format!("{text} {}", UNITS[unit])
}

/// A short, plain reason: "disk full" rather than "No space left on device (os error 28)".
fn describe(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::StorageFull => t("error.why.disk_full"),
        io::ErrorKind::PermissionDenied => t("error.why.permission"),
        _ => e.to_string(),
    }
}

/// A folder as the user would type it: `~/Shared`, not `/Users/me/Shared`.
fn shown(folder: &Path) -> String {
    match std::env::home_dir() {
        Some(home) => contract_home(folder, &home),
        None => folder.display().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::EditsPatch;
    use crate::settings::Location;
    use crate::testutil;
    use chrono::NaiveDate;
    use std::cell::RefCell;

    struct Env {
        dir: tempfile::TempDir,
        backups: Backups,
    }

    impl Env {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let backups = Backups::new(dir.path().join("backups"));
            Self { dir, backups }
        }
        fn work(&self) -> PathBuf {
            self.dir.path().join("work")
        }
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.dir.path().join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 22)
            .unwrap()
            .and_hms_opt(14, 23, 11)
            .unwrap()
    }

    /// Stands in for the system Trash: moves the file into `.trash` beside it.
    fn test_trash(path: &Path) -> io::Result<()> {
        let bin = path.parent().unwrap().join(".trash");
        fs::create_dir_all(&bin)?;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let dest = (0..)
            .map(|n| {
                bin.join(if n == 0 {
                    name.clone()
                } else {
                    format!("{n}-{name}")
                })
            })
            .find(|p| !p.exists())
            .unwrap();
        fs::rename(path, dest)
    }

    fn options_with(settings: &Settings) -> RunOptions {
        RunOptions::new(
            settings,
            testutil::gifsicle(),
            std::env::temp_dir().join("ferry-test-work"),
            test_trash,
            CancelToken::new(),
        )
    }

    fn options() -> RunOptions {
        options_with(&Settings::default())
    }

    fn scale(percent: u8) -> EditsPatch {
        EditsPatch {
            scale: Some(Some(percent)),
            ..EditsPatch::default()
        }
    }

    fn convert(format: Format) -> EditsPatch {
        EditsPatch {
            format: Some(Some(format)),
            ..EditsPatch::default()
        }
    }

    /// What a run emitted and delivered.
    #[derive(Default)]
    struct Seen {
        updates: Vec<JobUpdate>,
        delivered: Vec<(Format, Vec<u8>, PathBuf)>,
    }

    fn run(job: &mut Job, level: Level, o: &RunOptions, env: &Env) -> (Option<Finished>, Seen) {
        let seen = RefCell::new(Seen::default());
        let finished = job.run(
            level,
            o,
            &env.backups,
            &mut |u| seen.borrow_mut().updates.push(u),
            &mut |f, b, p| {
                seen.borrow_mut()
                    .delivered
                    .push((f, b.to_vec(), p.to_owned()));
                Ok(())
            },
        );
        (finished, seen.into_inner())
    }

    fn clipboard_png(env: &Env, template: &str) -> (Job, Vec<u8>) {
        let input = testutil::png(300, 200);
        let job = Job::clipboard(
            Format::Png,
            input.clone(),
            false,
            template,
            &env.work(),
            now(),
        );
        (job, input)
    }

    // stream B: audio
    #[test]
    fn a_file_content_is_copied_not_read_into_memory() {
        let env = Env::new();
        let src = env.file("big.bin", &vec![7u8; 3_000_000]);
        let target = env.file("t.bin", b"old");
        let job = Job::file(
            &env.file("a.png", &testutil::png(8, 8)),
            &Settings::default(),
            true,
        )
        .unwrap();
        job.write_file(&target, Content::File(&src), &target, &options())
            .unwrap();
        assert_eq!(fs::read(&target).unwrap().len(), 3_000_000);
        assert!(!target.with_file_name("t.bin.ferry-tmp").exists());
    }

    #[test]
    fn a_clipboard_image_is_backed_up_optimised_and_put_back_on_the_clipboard() {
        let env = Env::new();
        let (mut job, input) = clipboard_png(&env, "%f");
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;

        assert_eq!(update.state, JobState::Done);
        let after = update.after.unwrap();
        assert!(after < input.len() as u64);
        assert_eq!(update.detail, "300×200 · PNG");
        assert!(update.can_restore);
        let backup = update.thumb_path.clone().unwrap();
        assert_eq!(
            fs::read(&backup).unwrap(),
            input,
            "the backup is the original"
        );

        let output = update.output.clone().unwrap();
        assert_eq!(fs::metadata(&output).unwrap().len(), after);
        assert_eq!(seen.delivered.len(), 1);
        let (format, bytes, file) = &seen.delivered[0];
        assert_eq!(
            (*format, bytes.len() as u64, file),
            (Format::Png, after, &output)
        );

        assert!(matches!(seen.updates[0].state, JobState::Optimising { .. }));
        assert_eq!(seen.updates[0].thumb_path, Some(backup));
    }

    #[test]
    fn the_working_file_is_named_by_the_template_inside_its_own_job_folder() {
        let env = Env::new();
        let (job, _) = clipboard_png(&env, "%f %z");
        assert_eq!(job.name, "Clipboard 2026-10-22 at 14.23.11 300x200");
        let expected = env
            .work()
            .join("clipboard")
            .join(job.id.as_str())
            .join("Clipboard 2026-10-22 at 14.23.11 300x200.png");
        assert_eq!(job.output, expected);

        let (unsafe_name, _) = clipboard_png(&env, "../%y/%m");
        assert_eq!(unsafe_name.name, ".._2026_10");
        let (twin, _) = clipboard_png(&env, "%f %z");
        assert_ne!(twin.output, job.output, "same name, separate folders");
    }

    #[test]
    fn an_image_that_does_not_shrink_stays_off_the_clipboard_but_can_still_be_dragged() {
        let env = Env::new();
        let input = testutil::png(1, 1);
        let mut job = Job::clipboard(Format::Png, input.clone(), false, "%f", &env.work(), now());
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;
        assert!(matches!(
            update.state,
            JobState::Larger | JobState::AlreadyOptimal
        ));
        assert!(seen.delivered.is_empty());
        assert!(!update.can_restore);
        assert_eq!(fs::read(update.output.unwrap()).unwrap(), input);
    }

    #[test]
    fn a_file_is_replaced_in_place_with_its_dates_kept() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let then = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        let times = Times {
            accessed: then,
            modified: then,
            created: Some(then),
        };
        fsutil::set_times(&path, &times).unwrap();

        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        assert_eq!((job.name.as_str(), job.ext.as_str()), ("shot", "png"));
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;

        assert_eq!(update.state, JobState::Done);
        assert_eq!(update.output, Some(path.clone()));
        assert!(fs::metadata(&path).unwrap().len() < input.len() as u64);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), then);
        assert_eq!(fs::read(update.thumb_path.unwrap()).unwrap(), input);
        assert!(
            seen.delivered.is_empty(),
            "file jobs leave the clipboard alone"
        );
    }

    #[test]
    fn every_re_run_starts_from_the_original_and_reuses_its_backup() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        let first = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        let second = run(&mut job, Level::Aggressive, &o, &env).0.unwrap().update;

        assert_eq!(first.id, second.id);
        assert_eq!(first.thumb_path, second.thumb_path, "one backup per job");
        assert_eq!(fs::read(second.thumb_path.unwrap()).unwrap(), input);
        let expected = optimise::optimise(
            &input,
            Format::Png,
            &OptimiseOptions {
                level: Level::Aggressive,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        let Outcome::Smaller { bytes } = expected else {
            panic!("aggressive must shrink")
        };
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "aggressive ran on the original"
        );
        assert!(second.detail.ends_with("· aggressive"));
    }

    #[test]
    fn a_re_run_that_does_not_shrink_puts_the_original_back() {
        let env = Env::new();
        let input = testutil::jpeg(400, 300, 50, &testutil::JpegExtras::default());
        let path = env.file("photo.jpg", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let low = RunOptions {
            compression: 30,
            ..options()
        };
        let high = RunOptions {
            compression: 100,
            ..options()
        };

        let first = run(&mut job, Level::Normal, &low, &env).0.unwrap().update;
        assert_eq!(first.state, JobState::Done);
        assert_ne!(fs::read(&path).unwrap(), input);

        let second = run(&mut job, Level::Normal, &high, &env).0.unwrap().update;
        assert_eq!(second.state, JobState::Larger);
        assert_eq!(
            fs::read(&path).unwrap(),
            input,
            "the file matches the card again"
        );
    }

    #[test]
    fn a_clipboard_re_run_that_does_not_shrink_puts_the_original_back_on_the_clipboard() {
        let env = Env::new();
        let input = testutil::jpeg(400, 300, 50, &testutil::JpegExtras::default());
        let mut job = Job::clipboard(Format::Jpeg, input.clone(), false, "%f", &env.work(), now());
        let low = RunOptions {
            compression: 30,
            ..options()
        };
        let high = RunOptions {
            compression: 100,
            ..options()
        };
        let (_, first) = run(&mut job, Level::Normal, &low, &env);
        let (finished, second) = run(&mut job, Level::Normal, &high, &env);
        assert_eq!(first.delivered.len(), 1);
        assert_eq!(finished.unwrap().update.state, JobState::Larger);
        assert_eq!(second.delivered.len(), 1);
        assert_eq!(second.delivered[0].1, input);
    }

    #[test]
    fn restore_puts_the_original_back_in_place_and_on_the_clipboard() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut file_job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        run(&mut file_job, Level::Normal, &o, &env);
        let restored = file_job.restore(&o, &mut |_, _, _| Ok(()));
        assert_eq!(restored.update.state, JobState::Restored);
        assert_eq!(fs::read(&path).unwrap(), input);

        let (mut clip, input) = clipboard_png(&env, "%f");
        run(&mut clip, Level::Normal, &o, &env);
        let mut delivered = Vec::new();
        let restored = clip.restore(&o, &mut |_, b, _| {
            delivered.push(b.to_vec());
            Ok(())
        });
        assert_eq!(restored.update.state, JobState::Restored);
        assert_eq!(delivered, vec![input.clone()]);
        assert_eq!(fs::read(&clip.output).unwrap(), input);
    }

    #[test]
    fn a_cancelled_job_changes_nothing() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        o.cancel.cancel();
        let (finished, seen) = run(&mut job, Level::Normal, &o, &env);
        assert!(finished.is_none());
        assert!(seen.updates.is_empty() && seen.delivered.is_empty());
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_file_is_an_error_naming_its_folder_and_stays_untouched() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).unwrap();

        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let finished = run(&mut job, Level::Normal, &options(), &env).0.unwrap();
        let JobState::Error { reason } = &finished.update.state else {
            panic!("{:?}", finished.update.state)
        };
        assert!(reason.starts_with("Can't write to "), "{reason}");
        assert!(finished.update.can_retry);
        assert!(finished.cause.is_some());
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[test]
    fn a_failed_backup_is_an_error_and_the_file_is_untouched() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let blocker = env.file("not-a-folder", b"x");
        let backups = Backups::new(&blocker);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let finished = job
            .run(
                Level::Normal,
                &options(),
                &backups,
                &mut |_| {},
                &mut |_, _, _| Ok(()),
            )
            .unwrap();
        let JobState::Error { reason } = finished.update.state else {
            panic!()
        };
        assert!(reason.starts_with("Backup failed: "), "{reason}");
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[test]
    fn a_damaged_image_is_an_error_with_a_retry() {
        let env = Env::new();
        let mut broken = testutil::png(300, 200);
        broken.truncate(broken.len() / 2);
        let mut job = Job::clipboard(Format::Png, broken, false, "%f", &env.work(), now());
        let finished = run(&mut job, Level::Normal, &options(), &env).0.unwrap();
        assert_eq!(
            finished.update.state,
            JobState::Error {
                reason: "Can't read this PNG — it may be damaged".into()
            }
        );
        assert!(finished.update.can_retry);
    }

    #[test]
    fn a_clipboard_that_refuses_the_write_is_an_error() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let finished = job
            .run(
                Level::Normal,
                &options(),
                &env.backups,
                &mut |_| {},
                &mut |_, _, _| Err("busy".into()),
            )
            .unwrap();
        assert_eq!(
            finished.update.state,
            JobState::Error {
                reason: "Couldn't put the image on the clipboard".into()
            }
        );
        assert_eq!(finished.cause.as_deref(), Some("busy"));
    }

    #[test]
    fn files_the_settings_exclude_are_refused_with_a_reason() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.ignored_extensions = vec!["png".into()];
        let png = env.file("a.PNG", &testutil::png(8, 8));
        assert_eq!(
            Job::file(&png, &settings, true).unwrap_err(),
            ".png files are ignored in Settings"
        );

        let text = env.file("notes.txt", b"hello");
        assert_eq!(
            Job::file(&text, &Settings::default(), true).unwrap_err(),
            "notes.txt is not a PNG, JPEG, GIF or WebP image"
        );

        let gif = env.file("anim.gif", &testutil::gif(2));
        assert_eq!(
            Job::file(&gif, &Settings::default(), false).unwrap_err(),
            "gifsicle missing — reinstall Ferry"
        );

        settings.images.ignored_extensions.clear();
        settings.images.max_size_mb = 1;
        let big = env.file("big.png", &vec![0; 1_000_001]);
        assert_eq!(
            Job::file(&big, &settings, true).unwrap_err(),
            "Larger than 1 MB — change the limit in Settings"
        );
    }

    #[test]
    fn a_file_sniffed_by_content_keeps_its_own_name() {
        let env = Env::new();
        let path = env.file(
            "really-a-jpeg.png",
            &testutil::jpeg(20, 20, 80, &Default::default()),
        );
        let job = Job::file(&path, &Settings::default(), true).unwrap();
        assert_eq!((job.format, job.ext.as_str()), (Format::Jpeg, "png"));
    }

    #[test]
    fn a_notice_is_an_error_card_without_retry() {
        let update = notice("Nothing to optimise on the clipboard".into());
        assert!(matches!(update.state, JobState::Error { .. }));
        assert!(!update.can_retry && !update.can_restore);
        assert!(update.output.is_none());
    }

    #[test]
    fn a_downscale_re_encodes_from_the_original_and_never_compounds() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let o = options();
        run(&mut job, Level::Normal, &o, &env);
        job.edit(scale(50));
        let (first, seen) = run(&mut job, Level::Normal, &o, &env);
        let first = first.unwrap().update;
        assert_eq!(first.state, JobState::Done);
        assert_eq!(first.dims_after, Some((150, 100)));
        assert_eq!(first.detail, "300×200 → 150×100 · PNG");
        assert_eq!(first.edits.scale, Some(50));
        assert!(first.can_edit);
        let delivered = &seen.delivered.last().unwrap().1;
        assert_eq!(sniff::dimensions(delivered, Format::Png), Some((150, 100)));

        job.edit(scale(50));
        let (_, again) = run(&mut job, Level::Normal, &o, &env);
        let delivered = &again.delivered.last().unwrap().1;
        assert_eq!(
            sniff::dimensions(delivered, Format::Png),
            Some((150, 100)),
            "not 75×50"
        );
    }

    #[test]
    fn convert_in_place_trashes_the_source_and_restore_brings_it_back() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        run(&mut job, Level::Normal, &o, &env);
        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;

        let webp = env.dir.path().join("shot.webp");
        assert_eq!(update.output, Some(webp.clone()));
        assert_eq!(sniff::sniff(&fs::read(&webp).unwrap()), Some(Format::WebP));
        assert!(!path.exists(), "the source went to the Trash");
        assert_eq!(
            fs::read(env.dir.path().join(".trash/shot.png")).unwrap(),
            input,
            "the original, not the optimised file"
        );
        assert!(update.converted);
        assert_eq!(
            (update.ext.as_str(), update.format),
            ("webp", Some(Format::WebP))
        );

        let restored = job.restore(&o, &mut |_, _, _| Ok(())).update;
        assert_eq!(restored.state, JobState::Restored);
        assert_eq!(fs::read(&path).unwrap(), input);
        assert!(!webp.exists());
        assert_eq!(restored.output, Some(path));
        assert!(!restored.converted);
        assert_eq!(restored.edits, Edits::default());
    }

    #[test]
    fn switching_from_a_same_folder_result_to_an_in_place_conversion_leaves_one_file() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &settings, true).unwrap();

        let first = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        let optimised = env.dir.path().join("shot_optimised.png");
        assert_eq!(first.output, Some(optimised.clone()));
        assert_eq!(fs::read(&path).unwrap(), input, "the source is untouched");

        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &o, &env);
        assert!(env.dir.path().join("shot.webp").exists());
        assert!(!optimised.exists() && !path.exists());
    }

    #[test]
    fn a_twin_of_a_renamed_result_keeps_the_new_name() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let path = env.file("shot.png", &testutil::png(300, 200));
        let mut job = Job::file(&path, &settings, true).unwrap();
        job.edit(scale(50));
        run(&mut job, Level::Normal, &o, &env);
        job.rename("hero").unwrap();
        let mut twin = job.fork();
        twin.edit(convert(Format::WebP));
        let update = run(&mut twin, Level::Normal, &o, &env).0.unwrap().update;
        assert_eq!(update.output, Some(env.dir.path().join("hero.webp")));
        assert_eq!(update.name, "hero");
    }

    #[test]
    fn choosing_the_source_format_again_undoes_the_conversion() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &o, &env);
        job.edit(convert(Format::Png));
        assert_eq!(job.edits.format, None);
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert!(!env.dir.path().join("shot.webp").exists());
        assert!(fs::metadata(&path).unwrap().len() < input.len() as u64);
        assert!(!update.converted);
    }

    #[test]
    fn an_explicit_edit_writes_its_result_even_when_larger() {
        let env = Env::new();
        let mut job = Job::clipboard(
            Format::Png,
            testutil::png(1, 1),
            false,
            "%f",
            &env.work(),
            now(),
        );
        job.edit(EditsPatch {
            compression: Some(Some(100)),
            ..EditsPatch::default()
        });
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;
        assert_eq!(update.state, JobState::Done);
        assert!(update.after.is_some());
        assert_eq!(seen.delivered.len(), 1);
    }

    #[test]
    fn an_unreachable_fit_target_is_noted_on_the_card() {
        let env = Env::new();
        let jpeg = testutil::jpeg(400, 300, 90, &testutil::JpegExtras::default());
        let mut job = Job::clipboard(Format::Jpeg, jpeg, false, "%f", &env.work(), now());
        job.edit(EditsPatch {
            fit_under: Some(Some(100)),
            ..EditsPatch::default()
        });
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(
            update.note.as_deref(),
            Some("Couldn't reach 100 B — try downscaling")
        );
    }

    #[test]
    fn an_animated_webp_offers_no_edits() {
        let env = Env::new();
        let mut job = Job::clipboard(
            Format::WebP,
            testutil::animated_webp(),
            false,
            "%f",
            &env.work(),
            now(),
        );
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert!(!update.can_edit);
        let (mut png_job, _) = clipboard_png(&env, "%f");
        assert!(
            run(&mut png_job, Level::Normal, &options(), &env)
                .0
                .unwrap()
                .update
                .can_edit
        );
    }

    #[test]
    fn a_clipboard_conversion_renames_the_working_file_and_delivers_the_new_format() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let o = options();
        let first = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        job.edit(convert(Format::Jpeg));
        let (finished, seen) = run(&mut job, Level::Normal, &o, &env);
        let update = finished.unwrap().update;
        let output = update.output.unwrap();
        assert_eq!(output.extension().unwrap(), "jpg");
        assert!(
            !first.output.unwrap().exists(),
            "the old working file is gone"
        );
        assert_eq!(seen.delivered.last().unwrap().0, Format::Jpeg);
        assert_eq!(update.ext, "jpg");
    }

    #[test]
    fn an_edit_after_the_source_vanished_still_writes_a_result() {
        let env = Env::new();
        let path = env.file("shot.png", &testutil::png(300, 200));
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        run(&mut job, Level::Normal, &o, &env);
        fs::remove_file(&path).unwrap();
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(
            sniff::dimensions(&fs::read(&path).unwrap(), Format::Png),
            Some((150, 100))
        );
    }

    #[test]
    fn an_unwritable_output_folder_is_an_error_and_the_source_keeps_the_original() {
        let env = Env::new();
        let blocker = env.file("blocker", b"not a folder");
        let mut settings = Settings::default();
        settings.images.output.converted.location = Location::Folder;
        settings.images.output.converted.folder =
            blocker.join("sub").to_string_lossy().into_owned();
        let o = options_with(&settings);
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &settings, true).unwrap();
        run(&mut job, Level::Normal, &o, &env);
        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert!(
            matches!(&update.state, JobState::Error { reason } if reason.starts_with("Can't write to"))
        );
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[test]
    fn a_fork_shares_the_original_and_leaves_its_parent_alone() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let o = options();
        let parent = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        let mut twin = job.fork();
        assert_ne!(twin.id, job.id);
        twin.edit(convert(Format::WebP));
        let child = run(&mut twin, Level::Normal, &o, &env).0.unwrap().update;
        assert_eq!(child.thumb_path, parent.thumb_path, "same backup");
        assert_ne!(child.output, parent.output);
        assert!(parent.output.unwrap().exists());
        assert_eq!(child.format, Some(Format::WebP));
    }

    #[test]
    fn a_keep_both_twin_never_touches_its_parents_file() {
        let env = Env::new();
        let (job, path) = done_file(&env, "shot.png");
        let parent = fs::read(&path).unwrap();
        let o = options();
        let mut twin = job.fork();
        twin.edit(convert(Format::WebP));
        let update = run(&mut twin, Level::Normal, &o, &env).0.unwrap().update;

        let webp = env.dir.path().join("shot.webp");
        assert_eq!(update.output, Some(webp.clone()));
        assert_eq!(fs::read(&path).unwrap(), parent, "the parent's file stays");

        let restored = twin.restore(&o, &mut |_, _, _| Ok(())).update;
        assert_eq!(restored.state, JobState::Restored);
        assert!(!webp.exists(), "the twin's own file went to the Trash");
        assert_eq!(
            fs::read(&path).unwrap(),
            parent,
            "and the parent's still stays"
        );
    }

    #[test]
    fn a_twin_that_keeps_its_format_writes_beside_the_source() {
        let env = Env::new();
        let (job, path) = done_file(&env, "shot.png");
        let parent = fs::read(&path).unwrap();
        let mut twin = job.fork();
        twin.edit(scale(50));
        let update = run(&mut twin, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.output, Some(env.dir.path().join("shot (2).png")));
        assert_eq!(fs::read(&path).unwrap(), parent);
    }

    #[test]
    fn a_cancelled_edit_is_not_kept_in_the_recipe() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        job.edit(scale(50));
        let o = options();
        o.cancel.cancel();
        assert!(run(&mut job, Level::Normal, &o, &env).0.is_none());
        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.dims_after, None, "the cancelled 50 % came back");
    }

    #[test]
    fn an_edit_cancelled_before_it_started_is_not_kept_either() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        let before = (job.edits.clone(), job.level);
        job.edit(EditsPatch {
            level: Some(Level::Aggressive),
            ..scale(50)
        });
        job.cancel_recipe();
        assert_eq!((job.edits.clone(), job.level), before);
    }

    #[test]
    fn a_finished_run_keeps_its_recipe() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        job.edit(scale(50));
        run(&mut job, Level::Normal, &options(), &env);
        job.cancel_recipe();
        assert_eq!(job.edits.scale, Some(50));
    }

    const CHANGED: &str = "File changed since Ferry wrote it";

    #[test]
    fn an_edit_refuses_a_source_changed_outside_ferry() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        fs::write(&path, b"user's newer edit").unwrap();
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(
            update.state,
            JobState::Error {
                reason: CHANGED.into()
            }
        );
        assert_eq!(fs::read(&path).unwrap(), b"user's newer edit");
    }

    #[test]
    fn restore_refuses_a_source_changed_outside_ferry() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        fs::write(&path, b"user's newer edit").unwrap();
        let restored = job.restore(&options(), &mut |_, _, _| Ok(())).update;
        assert_eq!(
            restored.state,
            JobState::Error {
                reason: CHANGED.into()
            }
        );
        assert_eq!(fs::read(&path).unwrap(), b"user's newer edit");
    }

    #[test]
    fn a_source_with_the_same_size_but_another_date_counts_as_changed() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        let times = fsutil::read_times(&path).unwrap();
        let later = Times {
            modified: times.modified + std::time::Duration::from_secs(60),
            ..times
        };
        fsutil::set_times(&path, &later).unwrap();
        let restored = job.restore(&options(), &mut |_, _, _| Ok(())).update;
        assert!(matches!(restored.state, JobState::Error { .. }));
    }

    #[test]
    fn a_convert_refuses_to_trash_a_source_changed_outside_ferry() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        fs::write(&path, b"user's newer edit").unwrap();
        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert!(matches!(update.state, JobState::Error { .. }));
        assert_eq!(fs::read(&path).unwrap(), b"user's newer edit");
        assert!(
            !env.dir.path().join("shot.webp").exists(),
            "nothing was written"
        );
    }

    #[test]
    fn a_source_ferry_wrote_restores_even_after_several_edits() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        for percent in [80, 60] {
            job.edit(scale(percent));
            let state = run(&mut job, Level::Normal, &options(), &env)
                .0
                .unwrap()
                .update
                .state;
            assert_eq!(state, JobState::Done);
        }
        let restored = job.restore(&options(), &mut |_, _, _| Ok(())).update;
        assert_eq!(restored.state, JobState::Restored);
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    const FOREIGN: &[u8] = b"someone else's file";

    #[test]
    fn restore_never_overwrites_a_new_file_at_the_trashed_source() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        assert!(!path.exists());
        fs::write(&path, FOREIGN).unwrap();

        let restored = job.restore(&options(), &mut |_, _, _| Ok(())).update;

        let copy = env.dir.path().join("shot (2).png");
        assert_eq!(restored.state, JobState::Restored);
        assert_eq!(fs::read(&path).unwrap(), FOREIGN, "the new shot.png stays");
        assert_eq!(
            fs::read(&copy).unwrap(),
            input,
            "the original lands beside it"
        );
        assert_eq!(restored.output, Some(copy));
    }

    #[test]
    fn a_re_convert_never_overwrites_a_new_file_at_the_trashed_source() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        fs::write(&path, FOREIGN).unwrap();

        job.edit(convert(Format::Jpeg));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;

        assert_eq!(update.state, JobState::Done);
        assert_eq!(fs::read(&path).unwrap(), FOREIGN, "the new shot.png stays");
        assert_eq!(update.output, Some(env.dir.path().join("shot.jpg")));
        assert!(!env.dir.path().join("shot.webp").exists());
    }

    #[test]
    fn going_back_to_the_source_format_never_overwrites_a_new_file_at_the_trashed_source() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        fs::write(&path, FOREIGN).unwrap();

        job.edit(convert(Format::Png));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;

        assert_eq!(fs::read(&path).unwrap(), FOREIGN);
        assert_eq!(update.output, Some(env.dir.path().join("shot (2).png")));
    }

    #[test]
    fn a_parent_re_run_never_trashes_its_twins_file() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        assert!(!path.exists());
        let mut twin = job.fork();
        twin.edit(convert(Format::Png));
        let t = run(&mut twin, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        let twin_file = t.output.unwrap();
        job.edit(scale(50));
        run(&mut job, Level::Normal, &options(), &env);
        assert!(
            twin_file.exists(),
            "the twin's {twin_file:?} went to the Trash"
        );
    }

    #[test]
    fn a_twin_never_takes_its_parents_source_path_even_when_it_is_free() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        assert!(!path.exists(), "the parent trashed it");
        let mut twin = job.fork();
        twin.edit(convert(Format::Png));
        twin.edit(scale(50));
        let update = run(&mut twin, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.output, Some(env.dir.path().join("shot (2).png")));
        assert!(!path.exists());
    }

    #[test]
    fn a_twin_card_never_shows_its_parents_file() {
        let env = Env::new();
        let (job, path) = done_file(&env, "shot.png");
        let twin = job.fork();
        assert_ne!(twin.output, path);
        assert_eq!(twin.update(JobState::Queued, None).output, None);
        let other = job.fork();
        assert_ne!(
            twin.output, other.output,
            "queued twins are not busy on each other"
        );
    }

    #[test]
    fn a_twin_that_fails_does_not_point_at_its_parents_file() {
        let env = Env::new();
        let blocker = env.file("blocker", b"not a folder");
        let mut settings = Settings::default();
        settings.images.output.converted.location = Location::Folder;
        settings.images.output.converted.folder =
            blocker.join("sub").to_string_lossy().into_owned();
        let o = options_with(&settings);
        let (job, path) = done_file(&env, "shot.png");
        let mut twin = job.fork();
        twin.edit(convert(Format::WebP));
        let update = run(&mut twin, Level::Normal, &o, &env).0.unwrap().update;
        assert!(matches!(update.state, JobState::Error { .. }));
        assert_ne!(update.output, Some(path));
    }

    #[test]
    fn restoring_a_twin_trashes_its_copy_and_leaves_the_parent_alone() {
        let env = Env::new();
        let (job, path) = done_file(&env, "shot.png");
        let parent = fs::read(&path).unwrap();
        let mut twin = job.fork();
        twin.edit(convert(Format::WebP));
        run(&mut twin, Level::Normal, &options(), &env);
        let copy = env.dir.path().join("shot.webp");
        assert!(copy.exists());

        let restored = twin.restore(&options(), &mut |_, _, _| Ok(())).update;

        assert_eq!(restored.state, JobState::Restored);
        assert!(!copy.exists(), "the copy went to the Trash");
        assert_eq!(restored.output, None, "not the parent's file");
        assert_eq!(fs::read(&path).unwrap(), parent);
        assert_eq!(twin.rename("x"), Err(RenameError::Gone));
        assert!(path.exists(), "renaming the twin moved the parent's file");
    }

    #[test]
    fn a_twin_always_writes_its_copy_even_when_it_cannot_shrink() {
        let env = Env::new();
        let input = testutil::jpeg(400, 300, 50, &testutil::JpegExtras::default());
        let path = env.file("photo.jpg", &input);
        let job = Job::file(&path, &Settings::default(), true).unwrap();
        let high = RunOptions {
            compression: 100,
            ..options()
        };
        let mut twin = job.fork();
        twin.edit(convert(Format::Jpeg));
        assert!(twin.edits.is_plain(), "same format: nothing asked for");
        let update = run(&mut twin, Level::Normal, &high, &env).0.unwrap().update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(update.output, Some(env.dir.path().join("photo (2).jpg")));
        assert_eq!(fs::read(&path).unwrap(), input, "the parent's file stays");
    }

    #[test]
    fn a_rename_after_a_conversion_survives_the_next_edit() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        job.rename("hero").unwrap();
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.output, Some(env.dir.path().join("hero.webp")));
        assert!(!env.dir.path().join("shot.webp").exists());
    }

    #[test]
    fn a_rename_of_a_same_folder_result_survives_the_next_edit() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let path = env.file("shot.png", &testutil::png(300, 200));
        let mut job = Job::file(&path, &settings, true).unwrap();
        job.edit(scale(50));
        run(&mut job, Level::Normal, &o, &env);
        job.rename("hero").unwrap();
        job.edit(scale(40));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert_eq!(update.output, Some(env.dir.path().join("hero.png")));
        assert!(!env.dir.path().join("shot_optimised.png").exists());
    }

    #[test]
    fn restore_after_a_rename_brings_the_original_back_under_the_new_name() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &options(), &env);
        job.rename("hero").unwrap();

        let restored = job.restore(&options(), &mut |_, _, _| Ok(())).update;

        let hero = env.dir.path().join("hero.png");
        assert_eq!(restored.state, JobState::Restored);
        assert_eq!(fs::read(&hero).unwrap(), input);
        assert_eq!(restored.output, Some(hero));
        assert_eq!(restored.name, "hero", "the card and the disk agree");
        assert!(!path.exists());
        assert!(!env.dir.path().join("hero.webp").exists());
    }

    #[test]
    fn restore_after_renaming_a_beside_the_source_result_leaves_the_source_alone() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &settings, true).unwrap();
        run(&mut job, Level::Normal, &o, &env);
        job.rename("hero").unwrap();

        let restored = job.restore(&o, &mut |_, _, _| Ok(())).update;

        assert_eq!(
            fs::read(&path).unwrap(),
            input,
            "the user's file keeps its name"
        );
        assert!(
            !env.dir.path().join("hero.png").exists(),
            "the result went to the Trash"
        );
        assert_eq!(restored.output, Some(path));
        assert_eq!(
            restored.name, "shot",
            "the card shows the file that is there"
        );
    }

    #[test]
    fn a_restored_original_changed_outside_ferry_is_not_overwritten_by_the_next_edit() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        job.restore(&options(), &mut |_, _, _| Ok(()));
        fs::write(&path, b"annotated in Preview").unwrap();
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(
            update.state,
            JobState::Error {
                reason: CHANGED.into()
            }
        );
        assert_eq!(fs::read(&path).unwrap(), b"annotated in Preview");
    }

    #[test]
    fn a_convert_in_place_refuses_to_trash_an_untouched_source_changed_outside_ferry() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let path = env.file("shot.png", &testutil::png(300, 200));
        let mut job = Job::file(&path, &settings, true).unwrap();
        run(&mut job, Level::Normal, &o, &env);
        let beside = env.dir.path().join("shot_optimised.png");
        assert!(beside.exists());
        fs::write(&path, b"annotated in Preview").unwrap();

        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;

        assert_eq!(
            update.state,
            JobState::Error {
                reason: CHANGED.into()
            }
        );
        assert_eq!(fs::read(&path).unwrap(), b"annotated in Preview");
        assert!(beside.exists(), "nothing was taken back");
        assert!(!env.dir.path().join("shot.webp").exists());
    }

    #[test]
    fn a_result_beside_the_source_does_not_mind_the_source_changing() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let path = env.file("shot.png", &testutil::png(300, 200));
        let mut job = Job::file(&path, &settings, true).unwrap();
        run(&mut job, Level::Normal, &o, &env);
        fs::write(&path, b"annotated in Preview").unwrap();

        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;

        assert_eq!(update.state, JobState::Done);
        assert_eq!(fs::read(&path).unwrap(), b"annotated in Preview");
    }

    #[test]
    fn a_gif_twin_is_a_copy_too() {
        let env = Env::new();
        let path = env.file("loop.gif", &testutil::gif(3));
        let job = Job::file(&path, &Settings::default(), true).unwrap();
        let mut twin = job.fork();
        twin.edit(convert(Format::Gif));
        let update = run(&mut twin, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(update.output, Some(env.dir.path().join("loop (2).gif")));
        assert!(path.exists());
    }

    #[test]
    fn a_card_says_why_its_tools_are_off() {
        let env = Env::new();
        let mut animated = Job::clipboard(
            Format::WebP,
            testutil::animated_webp(),
            false,
            "%f",
            &env.work(),
            now(),
        );
        let update = run(&mut animated, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(
            update.edit_hint.as_deref(),
            Some("Animated WebP can't be edited yet")
        );
        assert_eq!(update.source_format, Some(Format::WebP));

        let (job, _) = done_file(&env, "shot.png");
        let done = job.update(JobState::Done, None);
        assert!(done.can_edit);
        assert_eq!(done.edit_hint, None);
        fs::remove_file(done.thumb_path.unwrap()).unwrap();
        let gone = job.update(JobState::Done, None);
        assert!(!gone.can_edit);
        assert_eq!(gone.edit_hint.as_deref(), Some("Original no longer kept"));
        assert_eq!(job.update(JobState::Queued, None).edit_hint, None);
    }

    #[test]
    fn sizes_read_like_the_cards() {
        assert_eq!(bytes_label(100), "100 B");
        assert_eq!(bytes_label(1_000_000), "1 MB");
        assert_eq!(bytes_label(1_500_000), "1.5 MB");
        assert_eq!(bytes_label(250_000), "250 KB");
    }

    fn done_file(env: &Env, name: &str) -> (Job, PathBuf) {
        let path = env.file(name, &testutil::png(300, 200));
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        run(&mut job, Level::Normal, &options(), env);
        (job, path)
    }

    #[test]
    fn rename_moves_the_file_and_later_runs_keep_the_name() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        job.rename("hero").unwrap();
        let hero = env.dir.path().join("hero.png");
        assert!(hero.exists() && !path.exists());
        assert_eq!(job.name, "hero");
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.output, Some(hero.clone()));
        assert_eq!(
            sniff::dimensions(&fs::read(&hero).unwrap(), Format::Png),
            Some((150, 100))
        );
    }

    #[test]
    fn a_card_rename_is_ferrys_own_write() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        job.rename("renamed").unwrap();
        let to = env.dir.path().join("renamed.png");
        assert!(crate::selfwrite::is_own(
            &to,
            &crate::selfwrite::Stamp::of(&to).unwrap()
        ));
    }

    #[test]
    fn rename_refuses_bad_and_taken_names() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        env.file("taken.png", b"x");
        assert_eq!(job.rename("  "), Err(RenameError::Empty));
        for bad in ["a/b", "a\\b", "a:b", "x.", "x ", "CON", "lpt1", "nul.txt"] {
            assert_eq!(job.rename(bad), Err(RenameError::Invalid), "{bad}");
        }
        assert_eq!(job.rename("taken"), Err(RenameError::Taken));
        assert_eq!(RenameError::Taken.message(), "Name taken");
    }

    #[test]
    fn a_case_only_rename_is_allowed() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        job.rename("Shot").unwrap();
        let names: Vec<String> = fs::read_dir(env.dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"Shot.png".to_owned()), "{names:?}");
    }

    #[test]
    fn renaming_a_vanished_file_says_so() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        fs::remove_file(path).unwrap();
        assert_eq!(job.rename("hero"), Err(RenameError::Gone));
        assert_eq!(job.name, "shot");
    }

    #[test]
    fn a_clipboard_rename_renames_the_working_file() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let first = run(&mut job, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        job.rename("diagram").unwrap();
        assert_eq!(job.output.file_name().unwrap(), "diagram.png");
        assert!(job.output.exists() && !first.output.unwrap().exists());
    }

    // stream F: pipelines
    #[test]
    fn an_encoder_choice_keeps_the_plain_never_worse_rule() {
        let env = Env::new();
        let path = env.file("dot.png", &testutil::png(1, 1));
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        job.edit(EditsPatch {
            encoder: Some(Some(crate::optimise::Encoder::Fast)),
            ..EditsPatch::default()
        });
        let (finished, _) = run(&mut job, Level::Normal, &options(), &env);
        assert_eq!(finished.unwrap().update.state, JobState::Larger);
    }

    #[test]
    fn a_strip_override_writes_a_result_without_exif() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.strip_metadata = false;
        let exif = testutil::exif_with_orientation(1, 4000);
        let input = testutil::jpeg(
            300,
            200,
            90,
            &testutil::JpegExtras {
                exif: Some(exif),
                ..Default::default()
            },
        );
        let path = env.file("photo.jpg", &input);
        let mut job = Job::file(&path, &settings, true).unwrap();
        job.edit(EditsPatch {
            strip_metadata: Some(Some(true)),
            ..EditsPatch::default()
        });
        let (finished, _) = run(&mut job, Level::Normal, &options_with(&settings), &env);
        assert_eq!(finished.unwrap().update.state, JobState::Done);
        let out = fs::read(&path).unwrap();
        // Orientation 1 needs no minimal EXIF (C1§5.8), so no APP1 Exif header is left at all.
        assert!(!out.windows(6).any(|w| w == b"Exif\0\0"));
    }

    // stream F: pipelines
    #[test]
    fn run_options_carry_the_adaptive_setting() {
        let mut s = Settings::default();
        s.images.adaptive = true;
        assert!(options_with(&s).adaptive);
    }

    #[test]
    fn a_switched_file_is_written_as_a_conversion_and_restores() {
        let env = Env::new();
        let mut s = Settings::default();
        s.images.switch_png_jpeg = true;
        let path = env.file("photo.png", &testutil::photo_png(256, 256));
        let mut job = Job::file(&path, &s, true).unwrap();
        let o = options_with(&s);
        let (finished, _) = run(&mut job, Level::Normal, &o, &env);
        let update = finished.unwrap().update;
        assert!(update.converted && update.format == Some(Format::Jpeg));
        assert!(path.with_extension(Format::Jpeg.extension()).exists());
        assert!(
            !path.exists(),
            "the in-place source went to the (test) Trash"
        );
        job.restore(&o, &mut |_, _, _| Ok(()));
        assert!(path.exists());
    }
}
