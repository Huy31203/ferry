//! One job end to end (spec §5.4): back up the original, optimise it, write the
//! result and report each step as a `JobUpdate`. The app runs this on a worker
//! thread and owns the clipboard, which it reaches through a `deliver` callback.

use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::backup::Backups;
use crate::cancel::CancelToken;
use crate::fsutil::{self, Times};
use crate::i18n::{t, tf};
use crate::job::{JobId, JobState, JobUpdate, Source};
use crate::optimise::{self, Level, OptimiseError, OptimiseOptions, Outcome};
use crate::settings::{Settings, contract_home};
use crate::sniff::{self, Format};
use crate::template;
use crate::trigger::{MB, Skip, is_ignored_extension, normalise_ext};

/// Puts an image on the clipboard: format, bytes and the working file holding them.
pub type Deliver<'a> = &'a mut dyn FnMut(Format, &[u8], &Path) -> Result<(), String>;

/// Settings a run reads, taken when the run starts.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub compression: u8,
    pub strip_metadata: bool,
    pub preserve_dates: bool,
    pub gifsicle: Option<PathBuf>,
    pub cancel: CancelToken,
}

impl RunOptions {
    pub fn new(settings: &Settings, gifsicle: Option<PathBuf>, cancel: CancelToken) -> Self {
        Self {
            compression: settings.images.compression,
            strip_metadata: settings.images.strip_metadata,
            preserve_dates: settings.images.preserve_dates,
            gifsicle,
            cancel,
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
    pub ext: String,
    pub format: Format,
    pub before: u64,
    pub dims: Option<(u32, u32)>,
    pub from_bitmap: bool,
    /// The working file of a clipboard job, or the file itself.
    pub output: PathBuf,
    /// The level of the latest run; retry and re-optimise use it again.
    pub level: Level,
    /// The original, once stored. Every run starts from it (R§2.4).
    backup: Option<PathBuf>,
    /// A clipboard job's original until it is backed up.
    pending: Option<Vec<u8>>,
    /// A file job's times, read before its first write.
    times: Option<Times>,
    /// `output` (and the clipboard) hold an optimised result, not the original.
    changed: bool,
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
        let dims = sniff::dimensions(&bytes, format);
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
            backup: None,
            pending: Some(bytes),
            times: None,
            changed: false,
        }
    }

    /// A copied file, to be optimised in place. `Err` is the card text for a
    /// file the settings exclude or that is not a supported image.
    pub fn file(path: &Path, settings: &Settings, has_gifsicle: bool) -> Result<Job, String> {
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
            ext,
            format,
            before: len,
            dims: sniff::dimensions(&bytes, format),
            from_bitmap: false,
            output: path.to_owned(),
            level: Level::Normal,
            backup: None,
            pending: None,
            times: None,
            changed: false,
        })
    }

    /// This job's card in `state`.
    pub fn update(&self, state: JobState, after: Option<u64>) -> JobUpdate {
        let done = state == JobState::Done;
        JobUpdate {
            id: self.id.clone(),
            can_restore: done && self.backup.is_some(),
            can_retry: matches!(state, JobState::Error { .. }),
            detail: if done { self.detail() } else { String::new() },
            state,
            name: self.name.clone(),
            ext: self.ext.clone(),
            before: self.before,
            after,
            thumb_path: self.backup.clone(),
            source: self.source,
            output: self.output.exists().then(|| self.output.clone()),
        }
    }

    /// Optimise the original at `level`, write the result and report each step.
    /// `None` means the job was cancelled before anything was written. The
    /// output always matches the card: a re-run that does not come out smaller
    /// puts the original back.
    pub fn run(
        &mut self,
        level: Level,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
        deliver: Deliver,
    ) -> Option<Finished> {
        self.level = level;
        if o.cancel.is_cancelled() {
            return None;
        }
        let original = match self.original(backups) {
            Ok(bytes) => bytes,
            Err(e) => return Some(self.fail(tf("error.backup", &[("why", &describe(&e))]), e)),
        };
        let step = t(match self.format {
            Format::Png => "card.step.png",
            Format::Jpeg => "card.step.jpeg",
            Format::WebP => "card.step.webp",
            Format::Gif => "card.step.gif",
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
            strip_metadata: o.strip_metadata,
            gifsicle: o.gifsicle.clone(),
            cancel: o.cancel.clone(),
        };
        let outcome = match optimise::optimise(&original, self.format, &options) {
            Ok(outcome) => outcome,
            Err(OptimiseError::Cancelled) => return None,
            Err(e) => return Some(self.fail(self.reason(&e), e)),
        };
        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }
        let (state, bytes, after) = match &outcome {
            Outcome::Smaller { bytes } => (JobState::Done, bytes, Some(bytes.len() as u64)),
            Outcome::AlreadyOptimal => (JobState::AlreadyOptimal, &original, None),
            Outcome::Larger => (JobState::Larger, &original, None),
        };
        let optimised = state == JobState::Done;
        if let Err((reason, cause)) = self.write(bytes, optimised || self.changed, o, deliver) {
            return Some(self.fail(reason, cause));
        }
        self.changed = optimised;
        Some(Finished {
            update: self.update(state, after),
            cause: None,
        })
    }

    /// Put the original back (§5.5): in place for a file; in the working file
    /// and on the clipboard for a clipboard image.
    pub fn restore(&mut self, o: &RunOptions, deliver: Deliver) -> Finished {
        let read = self
            .backup
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
            .and_then(fs::read);
        let original = match read {
            Ok(bytes) => bytes,
            Err(e) => return self.fail(t("error.backup_missing"), e),
        };
        if let Err((reason, cause)) = self.write(&original, true, o, deliver) {
            return self.fail(reason, cause);
        }
        self.changed = false;
        Finished {
            update: self.update(JobState::Restored, None),
            cause: None,
        }
    }

    /// The original bytes, backed up first on the first run.
    fn original(&mut self, backups: &Backups) -> io::Result<Vec<u8>> {
        if self.backup.is_none() {
            let stored = match self.source {
                Source::Clipboard => {
                    let name = format!("{}.{}", self.name, self.ext);
                    let bytes = self.pending.as_deref().unwrap_or_default();
                    backups.store_bytes(self.id.as_str(), &name, bytes)?
                }
                Source::File => {
                    self.times = fsutil::read_times(&self.output).ok();
                    backups.store_file(self.id.as_str(), &self.output)?
                }
            };
            self.backup = Some(stored);
            self.pending = None;
        }
        fs::read(self.backup.as_ref().expect("stored above"))
    }

    /// Write `bytes` to the output. A clipboard job always refreshes its working
    /// file (drag and reveal use it) and touches the clipboard only when
    /// `replace`; a file job is only rewritten when `replace`.
    fn write(
        &self,
        bytes: &[u8],
        replace: bool,
        o: &RunOptions,
        deliver: Deliver,
    ) -> Result<(), (String, String)> {
        let folder = self.output.parent().unwrap_or(Path::new(""));
        let cant_write =
            |e: io::Error| (tf("error.write", &[("dir", &shown(folder))]), e.to_string());
        match self.source {
            Source::Clipboard => {
                fs::create_dir_all(folder)
                    .and_then(|()| fs::write(&self.output, bytes))
                    .map_err(cant_write)?;
                if replace {
                    deliver(self.format, bytes, &self.output)
                        .map_err(|e| (t("error.clipboard_write"), e))?;
                }
            }
            Source::File if replace => {
                fsutil::atomic_replace(&self.output, bytes).map_err(cant_write)?;
                // Best effort: the content is safe; a lost tag or date is not worth an Error card.
                if let Some(backup) = &self.backup {
                    let _ = fsutil::copy_xattrs(backup, &self.output);
                }
                if o.preserve_dates
                    && let Some(times) = &self.times
                {
                    let _ = fsutil::set_times(&self.output, times);
                }
            }
            Source::File => {}
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
        let format = label(self.format);
        match e {
            OptimiseError::Decode(_) => tf("error.decode", &[("format", format)]),
            OptimiseError::Encode(_) => tf("error.encode", &[("format", format)]),
            OptimiseError::Unsupported(message) => message.clone(),
            OptimiseError::Cancelled => t("error.cancelled"),
        }
    }

    /// "2880×1800 · PNG", plus "aggressive" and "from bitmap" when they apply.
    fn detail(&self) -> String {
        let mut parts = Vec::new();
        if let Some((w, h)) = self.dims {
            parts.push(format!("{w}×{h}"));
        }
        parts.push(label(self.format).to_owned());
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
    }
}

fn label(format: Format) -> &'static str {
    match format {
        Format::Png => "PNG",
        Format::Jpeg => "JPEG",
        Format::Gif => "GIF",
        Format::WebP => "WebP",
    }
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

    fn options() -> RunOptions {
        RunOptions::new(
            &Settings::default(),
            testutil::gifsicle(),
            CancelToken::new(),
        )
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
}
