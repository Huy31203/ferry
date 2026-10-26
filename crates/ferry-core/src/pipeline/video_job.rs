//! Video file jobs end to end. A video is never read into memory: it moves
//! between the backup, ffmpeg and the destination as files. The result is placed
//! by the same `put` / `put_back` rules as an image's.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

use super::{Content, Finished, Job, RunOptions, SourceState, bytes_label, describe, shown};
use crate::{
    backup::Backups,
    edit::{
        self, Framing, fit as edit_fit,
        geometry::{self, Rect},
        raster, smart,
    },
    error::FerryError,
    ffmpeg,
    i18n::{t, tf},
    job::{JobId, JobState, JobUpdate, Source, VideoMeta},
    optimise::{self, Level, OptimiseError, OptimiseOptions},
    sniff::{self, Format},
    trigger::{MB, Skip, is_ignored_extension, normalise_ext},
    video::{
        self, fit,
        hardware::{self, Hardware},
        plan::{self, Framed, Plan, PlanInput, Route},
        probe::{self, Probe},
    },
};

/// What a video job knows besides what every job knows.
#[derive(Debug, Clone, Default)]
pub(crate) struct VideoState {
    pub probe: Option<Probe>,
    /// The first frame, kept in the job's backup folder.
    pub thumb: Option<PathBuf>,
    /// The current result's facts; `None` while the original is the result.
    pub result: Option<Made>,
    /// The hardware this job planned with.
    pub hardware: Option<Hardware>,
}

#[derive(Debug, Clone)]
pub(crate) struct Made {
    pub codec: &'static str,
    pub fps: Option<f32>,
    pub duration: Duration,
    pub audio: bool,
    /// Percent, when sped up.
    pub speed: Option<u16>,
}

/// The job's temporary folder, removed however the run ends.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Why an encode stopped without a result.
enum Stop {
    Cancelled,
    /// An HEVC video the machine cannot re-encode: leave the file as it is.
    Keep,
    Failed(FerryError),
}

/// What every attempt of one run shares.
struct Encoding<'a> {
    o: &'a RunOptions,
    backup: &'a Path,
    tmp: &'a Path,
    probe: &'a Probe,
    framed: &'a Framed,
    crop: Option<Rect>,
    hardware: Hardware,
    plain: bool,
    /// The target the recipe asked for, before `route`.
    asked: Format,
}

/// What changes between attempts.
struct Attempt {
    target: Format,
    software: bool,
    hevc_fallback: bool,
}

/// The container of a video file and its length, from its first 64 bytes.
pub(super) fn video_container(path: &Path) -> Option<(Format, u64)> {
    let mut head = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(64)
        .read_to_end(&mut head)
        .ok()?;
    let format = sniff::sniff(&head).filter(|f| f.is_video())?;
    Some((format, fs::metadata(path).ok()?.len()))
}

impl Job {
    pub fn is_video(&self) -> bool {
        self.video.is_some()
    }

    /// A copied video file, to be optimised in place. `Err` is the card text
    /// for a file the settings exclude.
    pub(super) fn video_file(
        path: &Path,
        settings: &crate::settings::Settings,
        container: Format,
        len: u64,
    ) -> Result<Job, String> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, ext) = match file_name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), ext.to_owned()),
            _ => (file_name.clone(), String::new()),
        };
        if !ext.is_empty() && is_ignored_extension(&settings.video.ignored_extensions, &ext) {
            return Err(Skip::IgnoredExtension(normalise_ext(&ext)).reason());
        }
        let limit_mb = settings.video.max_size_mb;
        if len > u64::from(limit_mb) * MB {
            return Err(Skip::TooLarge { limit_mb }.reason());
        }
        Ok(Job {
            id: JobId::generate(),
            source: Source::File,
            name: stem,
            source_ext: ext.clone(),
            ext,
            format: container,
            before: len,
            dims: None,
            from_bitmap: false,
            output: path.to_owned(),
            level: Level::Normal,
            edits: edit::Edits::default(),
            prior: None,
            source_format: container,
            animated: false,
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
            pdf_dpi: None,
            video: Some(VideoState::default()),
            audio: None,
            poster: None,
            pages: None,
        })
    }

    fn state(&mut self) -> &mut VideoState {
        self.video.as_mut().expect("a video job")
    }

    fn optimising(&self, progress: f32, step: String) -> JobUpdate {
        self.update(JobState::Optimising { progress, step }, None)
    }

    pub(super) fn run_video(
        &mut self,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
    ) -> Option<Finished> {
        // 1. The original is backed up first.
        if self.backup.is_none() {
            emit(self.optimising(0.0, t("video.step.backup")));
        }
        let backup = match self.backup_path(backups) {
            Ok(path) => path,
            Err(e) => return Some(self.fail(tf("error.backup", &[("why", &describe(&e))]), e)),
        };
        let tmp = TempDir(o.work_dir.join("video").join(self.id.as_str()));
        if let Err(e) = fs::create_dir_all(&tmp.0) {
            let reason = tf("error.write", &[("dir", &shown(&tmp.0))]);
            return Some(self.fail(reason, e));
        }

        // 2. What is in the file.
        if self.video.as_ref().is_some_and(|v| v.probe.is_none()) {
            emit(self.optimising(0.0, t("video.step.reading")));
            match probe::probe(&backup, &o.cancel) {
                Ok(probe) => {
                    self.source_format = video::source_format(self.source_format, &probe);
                    if !self.changed {
                        self.format = self.source_format;
                    }
                    self.dims = Some((probe.video.width, probe.video.height));
                    self.state().probe = Some(probe);
                }
                Err(FerryError::Cancelled) => return None,
                Err(e) => return Some(self.fail(e.reason(), e)),
            }
        }

        // 3. The card's picture. A failure leaves the placeholder.
        if self.video.as_ref().is_some_and(|v| v.thumb.is_none()) {
            let jpg = tmp.0.join("thumb.jpg");
            let made = ffmpeg::run(&plan::thumb_args(&backup, &jpg), &o.cancel, &mut |_| {});
            if made.is_ok()
                && let Ok(bytes) = fs::read(&jpg)
                && let Ok(stored) =
                    backups.store_bytes(self.id.as_str(), ".ferry-thumb.jpg", &bytes)
            {
                self.state().thumb = Some(stored);
            }
        }
        if o.cancel.is_cancelled() {
            return None;
        }

        // 4. What to write, and whether this machine can write it.
        let hardware = o.hardware.unwrap_or_else(hardware::probe_hardware);
        self.state().hardware = Some(hardware);
        let probe = self.video.as_ref()?.probe.clone()?;
        let plain = self.edits.is_plain() && !self.twin;
        let asked = self
            .edits
            .format
            .unwrap_or_else(|| plan::plain_target(self.source_format, &probe, &o.video));
        let (target, hevc_fallback) =
            match plan::route(asked, &probe, hardware.hevc.is_some(), plain) {
                Route::Keep => return Some(self.keep_video(&backup, o)),
                Route::Encode(target) => (target, target != asked),
            };

        // 5. The picture's window.
        let display = (probe.video.width, probe.video.height);
        let framed = plan::frame(display, &self.edits, target);
        let crop = (framed.window != display)
            .then(|| self.video_crop(&backup, &tmp.0, display, framed.window, o));

        // 7. Encode.
        let encoding = Encoding {
            o,
            backup: &backup,
            tmp: &tmp.0,
            probe: &probe,
            framed: &framed,
            crop,
            hardware,
            plain,
            asked,
        };
        let mut attempt = Attempt {
            target,
            software: false,
            hevc_fallback,
        };
        let mut missed = false;
        let encoded = match self.edits.fit_under {
            Some(limit) if target != Format::Gif => {
                // 9. Fit under size: aim a bitrate, then correct it, at most 3 encodes.
                let secs = match self.edits.speed {
                    Some(speed) => probe.duration.mul_f64(100.0 / f64::from(speed.percent)),
                    None => probe.duration,
                };
                let audio_bps: u64 = if self.edits.mute || o.video.remove_audio {
                    0
                } else {
                    probe
                        .audio
                        .iter()
                        .map(|a| a.bitrate.unwrap_or(128_000))
                        .sum()
                };
                let mut bps = fit::budget(limit, secs, audio_bps).0;
                let mut best: Option<(Plan, PathBuf, u64)> = None;
                for n in 0..3 {
                    let (plan, out) =
                        match self.encode(&encoding, &mut attempt, &n.to_string(), Some(bps), emit)
                        {
                            Ok(done) => done,
                            Err(stop) => return self.stopped(stop, &backup, o),
                        };
                    let size = fs::metadata(&out).map_or(u64::MAX, |m| m.len());
                    if best
                        .as_ref()
                        .is_none_or(|(_, _, smallest)| size < *smallest)
                    {
                        best = Some((plan, out, size));
                    }
                    if size <= limit {
                        break;
                    }
                    bps = fit::next_bitrate(bps, size, limit);
                }
                let (plan, out, size) = best?;
                missed = size > limit;
                (plan, out)
            }
            _ => match self.encode(&encoding, &mut attempt, "0", None, emit) {
                Ok(done) => done,
                Err(stop) => return self.stopped(stop, &backup, o),
            },
        };
        let (plan, out) = encoded;
        let target = attempt.target;

        // 8. A GIF goes through gifsicle.
        if target == Format::Gif
            && let Some(gifsicle) = &o.gifsicle
        {
            let made = self.polish_gif(&out, gifsicle, o);
            match made {
                Ok(gif_missed) => missed = gif_missed,
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        }

        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }
        let after = match fs::metadata(&out) {
            Ok(meta) => meta.len(),
            Err(e) => return Some(self.fail(t("error.unexpected"), e)),
        };

        // 10. Never worse, for a plain run.
        if plain {
            let state = if after.saturating_mul(100) <= self.before.saturating_mul(99) {
                None
            } else if after > self.before {
                Some(JobState::Larger)
            } else {
                Some(JobState::AlreadyOptimal)
            };
            if let Some(state) = state {
                return Some(self.keep_as(state, &backup, o, None));
            }
        }

        // 11. Place it.
        self.dims_after = Some(framed.out).filter(|d| Some(*d) != self.dims);
        self.note = if missed {
            let limit = bytes_label(self.edits.fit_under.unwrap_or_default());
            Some(tf("card.note.fit_missed", &[("size", &limit)]))
        } else if attempt.hevc_fallback || plan.hevc_as_h264 {
            Some(t("video.note.hevc_as_h264"))
        } else if plan.hdr_flattened {
            Some(t("video.note.hdr"))
        } else {
            None
        };
        let made = Made {
            codec: plan.codec_label,
            fps: plan.fps,
            duration: plan.expected,
            audio: plan.audio,
            speed: self.edits.speed.map(|s| s.percent),
        };
        let placed = self.put(
            Content::File(&out),
            target,
            Content::File(&backup),
            o,
            &mut |_, _, _| Ok(()),
        );
        Some(match placed {
            Ok(()) => {
                self.state().result = Some(made);
                Finished {
                    update: self.update(JobState::Done, Some(after)),
                    cause: None,
                }
            }
            Err((reason, cause)) => self.fail(reason, cause),
        })
    }

    /// One encode of the plan (and, for a failed hardware encode, the retry in
    /// software). Returns the plan that worked and the file it wrote.
    fn encode(
        &self,
        e: &Encoding,
        attempt: &mut Attempt,
        tag: &str,
        video_bitrate: Option<u64>,
        emit: &mut dyn FnMut(JobUpdate),
    ) -> Result<(Plan, PathBuf), Stop> {
        loop {
            let ext = plan::temp_extension(attempt.target, self.source_format, &self.source_ext);
            let out = e.tmp.join(format!("out{tag}.{ext}"));
            let planned = plan::plan(&PlanInput {
                input: e.backup,
                output: &out,
                probe: e.probe,
                source: self.source_format,
                target: attempt.target,
                edits: &self.edits,
                settings: &e.o.video,
                hardware: e.hardware,
                crop: e.crop,
                out: e.framed.out,
                software: attempt.software,
                video_bitrate,
                same_container: attempt.target == self.source_format,
            });
            let label = [("codec", planned.codec_label)];
            let step = if attempt.target == Format::Gif {
                t("video.step.gif")
            } else if planned.hardware {
                tf("video.step.encode", &label)
            } else {
                tf("video.step.encode_software", &label)
            };
            emit(self.optimising(0.0, step.clone()));
            let started = Instant::now();
            let mut shown_percent = 0;
            let ran =
                ffmpeg::run_expecting(&planned.args, planned.expected, &e.o.cancel, &mut |p| {
                    let percent = (p * 100.0) as u32;
                    if percent != shown_percent {
                        shown_percent = percent;
                        let text = video::step_text(&step, p, started.elapsed());
                        emit(self.optimising(p, text));
                    }
                });
            match ran {
                Ok(()) => return Ok((planned, out)),
                Err(FerryError::Cancelled) => return Err(Stop::Cancelled),
                // The hardware encoder failed: once more in software. A failed HEVC
                // attempt means no HEVC on this run. A full disk or a stall is not
                // the encoder's fault and takes the failure path below.
                Err(err) if planned.hardware && !attempt.software && err.is_encoder_failure() => {
                    match plan::route(e.asked, e.probe, false, e.plain) {
                        Route::Keep => return Err(Stop::Keep),
                        Route::Encode(target) => {
                            attempt.target = target;
                            attempt.hevc_fallback = target != e.asked;
                            attempt.software = true;
                        }
                    }
                }
                Err(err) => return Err(Stop::Failed(err)),
            }
        }
    }

    fn stopped(&mut self, stop: Stop, backup: &Path, o: &RunOptions) -> Option<Finished> {
        match stop {
            Stop::Cancelled => None,
            Stop::Keep => Some(self.keep_video(backup, o)),
            Stop::Failed(e) => Some(self.fail(e.reason(), e)),
        }
    }

    /// An HEVC video this machine cannot re-encode stays as it is.
    fn keep_video(&mut self, backup: &Path, o: &RunOptions) -> Finished {
        let cause = Some("HEVC kept: no hardware HEVC encoder".to_string());
        self.keep_as(JobState::AlreadyOptimal, backup, o, cause)
    }

    /// Leave the original as the result and say why.
    fn keep_as(
        &mut self,
        state: JobState,
        backup: &Path,
        o: &RunOptions,
        cause: Option<String>,
    ) -> Finished {
        let replace = self.changed;
        let put = self.put_back(Content::File(backup), replace, o, &mut |_, _, _| Ok(()));
        if let Err((reason, cause)) = put {
            return self.fail(reason, cause);
        }
        self.dims_after = None;
        self.note = None;
        self.state().result = None;
        Finished {
            update: self.update(state, None),
            cause,
        }
    }

    /// Where the crop window goes: the busiest part for smart framing, else the centre.
    fn video_crop(
        &self,
        backup: &Path,
        tmp: &Path,
        display: (u32, u32),
        window: (u32, u32),
        o: &RunOptions,
    ) -> Rect {
        if self.edits.framing == Framing::Smart {
            let png = tmp.join("frame.png");
            let made = ffmpeg::run(&plan::frame_args(backup, &png), &o.cancel, &mut |_| {});
            if made.is_ok()
                && let Ok(bytes) = fs::read(&png)
                && let Ok(frame) = raster::decode(&bytes, Format::Png)
                && (frame.width, frame.height) == display
            {
                return smart::frame(&frame.rgba, frame.width, frame.height, window);
            }
        }
        geometry::centre(display, window)
    }

    /// Run gifsicle over the palette GIF in place. `Ok(true)`: a fit-under-size
    /// target was out of reach.
    fn polish_gif(
        &self,
        out: &Path,
        gifsicle: &Path,
        o: &RunOptions,
    ) -> Result<bool, OptimiseError> {
        let base = OptimiseOptions {
            level: self.edits.level,
            compression: self.edits.compression.unwrap_or(o.video.compression),
            strip_metadata: o.strip_metadata,
            gifsicle: Some(gifsicle.to_owned()),
            cancel: o.cancel.clone(),
            ..OptimiseOptions::default() // stream F: pipelines
        };
        let bytes = fs::read(out).map_err(|e| OptimiseError::Decode(e.to_string()))?;
        let (polished, missed) = match self.edits.fit_under {
            Some(limit) => {
                let fitted = edit_fit::search(limit, |q| {
                    optimise::encode(
                        &bytes,
                        Format::Gif,
                        &OptimiseOptions {
                            compression: q,
                            level: Level::Normal,
                            ..base.clone()
                        },
                    )
                })?;
                (fitted.bytes, fitted.missed)
            }
            None => (optimise::encode(&bytes, Format::Gif, &base)?, false),
        };
        fs::write(out, polished).map_err(|e| OptimiseError::Encode(e.to_string()))?;
        Ok(missed)
    }

    /// Put the original back and clear the recipe, like `restore` for images.
    pub(super) fn restore_video(&mut self, o: &RunOptions) -> Finished {
        let Some(backup) = self.backup.clone().filter(|b| b.exists()) else {
            return self.fail(t("error.backup_missing"), "the backup is gone");
        };
        let put = self.put_back(Content::File(&backup), true, o, &mut |_, _, _| Ok(()));
        if let Err((reason, cause)) = put {
            return self.fail(reason, cause);
        }
        self.edits = edit::Edits::default();
        self.dims_after = None;
        self.note = None;
        self.state().result = None;
        Finished {
            update: self.update(JobState::Restored, None),
            cause: None,
        }
    }

    pub(super) fn video_meta(&self) -> Option<VideoMeta> {
        let video = self.video.as_ref()?;
        let probe = video.probe.as_ref();
        let original_ms = probe.map_or(0, |p| p.duration.as_millis() as u64);
        let (duration_ms, codec, fps, audio) = match (&video.result, probe) {
            (Some(made), _) => (
                made.duration.as_millis() as u64,
                made.codec.to_string(),
                made.fps,
                made.audio,
            ),
            (None, Some(p)) => (
                original_ms,
                video::codec_label(&p.video.codec),
                p.video.fps,
                !p.audio.is_empty(),
            ),
            (None, None) => (0, String::new(), None, false),
        };
        let hardware = video.hardware.unwrap_or_else(hardware::hardware);
        Some(VideoMeta {
            duration_ms,
            codec,
            fps,
            audio,
            original: self.backup.clone(),
            original_duration_ms: original_ms,
            hevc_encoder: hardware.hevc.is_some(),
        })
    }

    /// "1920×1080 → 960×540 · HEVC · 30 fps · 2× · no audio"
    pub(super) fn video_detail(&self) -> String {
        let mut parts = Vec::new();
        match (self.dims, self.dims_after) {
            (Some((w, h)), Some((aw, ah))) => parts.push(format!("{w}×{h} → {aw}×{ah}")),
            (Some((w, h)), None) => parts.push(format!("{w}×{h}")),
            _ => {}
        }
        if let Some(meta) = self.video_meta() {
            parts.push(meta.codec);
            if let Some(fps) = meta.fps {
                parts.push(tf("video.detail.fps", &[("n", &fps.to_string())]));
            }
            let sped = self
                .video
                .as_ref()
                .and_then(|v| v.result.as_ref())
                .and_then(|made| made.speed);
            if let Some(percent) = sped {
                let n = (f64::from(percent) / 100.0).to_string();
                parts.push(tf("video.detail.speed", &[("n", &n)]));
            }
            let had_audio = self
                .video
                .as_ref()
                .and_then(|v| v.probe.as_ref())
                .is_some_and(|p| !p.audio.is_empty());
            if had_audio && !meta.audio {
                parts.push(t("video.detail.no_audio"));
            }
        }
        if self.level == Level::Aggressive {
            parts.push(t("card.detail.aggressive"));
        }
        parts.join(" · ")
    }
}

#[cfg(test)]
mod tests {
    use std::{io, time::UNIX_EPOCH};

    use super::*;
    use crate::{
        cancel::CancelToken,
        fsutil::{self, Times},
        optimise::Encoder,
        settings::Settings,
        testutil::{self, VideoSpec},
    };

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
    }

    /// Stands in for the system Trash: moves the file into `.trash` beside it.
    fn test_trash(path: &Path) -> io::Result<()> {
        let bin = path.parent().unwrap().join(".trash");
        fs::create_dir_all(&bin)?;
        fs::rename(path, bin.join(path.file_name().unwrap()))
    }

    fn spec() -> VideoSpec {
        VideoSpec {
            size: (320, 240),
            secs: 2,
            fps: 30,
            codec: "libx264",
            audio_tracks: 1,
        }
    }

    /// Software encodes only, so a Mac with VideoToolbox behaves like CI.
    fn options(env: &Env, cancel: CancelToken) -> RunOptions {
        let mut settings = Settings::default();
        settings.video.encoder = Encoder::Small;
        let mut o = RunOptions::new(
            &settings,
            testutil::gifsicle(),
            env.work(),
            test_trash,
            cancel,
        );
        o.hardware = Some(Hardware::default());
        o
    }

    fn run(job: &mut Job, o: &RunOptions, env: &Env) -> Option<Finished> {
        job.run(
            Level::Normal,
            o,
            &env.backups,
            &mut |_| {},
            &mut |_, _, _| Ok(()),
        )
    }

    fn make(env: &Env, file: &str, spec: &VideoSpec) -> Option<(Job, PathBuf)> {
        let path = testutil::video(env.dir.path(), file, spec)?;
        let job = Job::file(&path, &Settings::default(), true).unwrap();
        Some((job, path))
    }

    fn probed(path: &Path) -> Probe {
        probe::probe(path, &CancelToken::new()).unwrap()
    }

    fn edit(job: &mut Job, f: impl FnOnce(&mut edit::EditsPatch)) {
        let mut patch = edit::EditsPatch::default();
        f(&mut patch);
        job.edit(patch);
    }

    #[test]
    fn a_video_file_is_backed_up_optimised_in_place_and_keeps_its_dates() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        let original = fs::read(&path).unwrap();
        let then = UNIX_EPOCH + Duration::from_secs(1_577_836_800); // 2020-01-01
        let times = Times {
            accessed: then,
            modified: then,
            created: Some(then),
        };
        fsutil::set_times(&path, &times).unwrap();

        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;

        assert_eq!(update.state, JobState::Done);
        assert!(update.after.unwrap() < update.before);
        assert_eq!(update.output, Some(path.clone()));
        let p = probed(&path);
        assert_eq!(p.video.codec, "h264");
        assert_eq!((p.video.width, p.video.height), (320, 240));
        let video = update.video.unwrap();
        let backup = fs::read(video.original.unwrap()).unwrap();
        assert_eq!(backup.len(), original.len());
        assert_eq!(
            backup[..4096.min(backup.len())],
            original[..4096.min(original.len())]
        );
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), then);
        let thumb = update.thumb_path.unwrap();
        assert!(thumb.to_string_lossy().ends_with(".ferry-thumb.jpg") && thumb.exists());
        assert_eq!(video.codec, "H.264");
        assert!(!env.work().join("video").join(job.id.as_str()).exists());
    }

    #[test]
    fn a_mov_becomes_an_mp4_and_the_mov_goes_to_the_trash() {
        let env = Env::new();
        let Some((mut job, mov)) = make(&env, "clip.mov", &spec()) else {
            return;
        };
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        assert_eq!(update.state, JobState::Done);
        let mp4 = env.dir.path().join("clip.mp4");
        assert_eq!(update.output, Some(mp4.clone()));
        assert!(mp4.exists() && !mov.exists());
        assert!(env.dir.path().join(".trash/clip.mov").exists());
        assert!(update.converted);
        assert_eq!(update.format, Some(Format::Mp4));
    }

    #[test]
    fn restore_brings_the_mov_back_and_trashes_the_mp4() {
        let env = Env::new();
        let Some((mut job, mov)) = make(&env, "clip.mov", &spec()) else {
            return;
        };
        let original_len = fs::metadata(&mov).unwrap().len();
        let o = options(&env, CancelToken::new());
        run(&mut job, &o, &env).unwrap();
        let update = job.restore(&o, &mut |_, _, _| Ok(())).update;
        assert_eq!(update.state, JobState::Restored);
        assert_eq!(fs::metadata(&mov).unwrap().len(), original_len);
        assert!(env.dir.path().join(".trash/clip.mp4").exists());
    }

    #[test]
    fn downscaling_twice_never_compounds() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        let o = options(&env, CancelToken::new());
        for _ in 0..2 {
            edit(&mut job, |p| p.scale = Some(Some(50)));
            run(&mut job, &o, &env).unwrap();
            let p = probed(&path);
            assert_eq!((p.video.width, p.video.height), (160, 120));
        }
    }

    #[test]
    fn a_portrait_clip_is_cropped_as_it_is_seen() {
        let env = Env::new();
        let wide = VideoSpec {
            size: (640, 360),
            ..spec()
        };
        let Some(source) = testutil::video(env.dir.path(), "wide.mp4", &wide) else {
            return;
        };
        let portrait = env.dir.path().join("portrait.mp4");
        let args: Vec<std::ffi::OsString> = vec![
            "-display_rotation".into(),
            "90".into(),
            "-i".into(),
            source.into(),
            "-c".into(),
            "copy".into(),
            portrait.clone().into(),
        ];
        ffmpeg::run(&args, &CancelToken::new(), &mut |_| {}).unwrap();
        let mut job = Job::file(&portrait, &Settings::default(), true).unwrap();
        let o = options(&env, CancelToken::new());

        edit(&mut job, |p| p.aspect = Some(Some((9, 16))));
        let first = run(&mut job, &o, &env).unwrap().update;
        assert_eq!(first.dims_before, Some((360, 640)));
        let p = probed(&portrait);
        assert_eq!((p.video.width, p.video.height), (360, 640));

        edit(&mut job, |p| p.aspect = Some(Some((1, 1))));
        run(&mut job, &o, &env).unwrap();
        let p = probed(&portrait);
        assert_eq!((p.video.width, p.video.height), (360, 360));
    }

    #[test]
    fn twice_the_speed_halves_the_duration_and_keeps_audio() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        edit(&mut job, |p| {
            p.speed = Some(Some(edit::Speed {
                percent: 200,
                keep_frames: true,
            }));
        });
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        let p = probed(&path);
        assert!(p.duration.abs_diff(Duration::from_secs(1)) < Duration::from_millis(100));
        assert_eq!(p.audio.len(), 1);
        assert!(update.detail.contains("2×"), "{}", update.detail);
    }

    #[test]
    fn removing_audio_leaves_a_silent_video() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        edit(&mut job, |p| p.mute = Some(true));
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        assert!(probed(&path).audio.is_empty());
        assert!(!update.video.unwrap().audio);
        assert!(update.detail.contains("no audio"), "{}", update.detail);
    }

    #[test]
    fn two_audio_tracks_survive_an_optimise() {
        let env = Env::new();
        let two = VideoSpec {
            audio_tracks: 2,
            ..spec()
        };
        let Some((mut job, path)) = make(&env, "clip.mp4", &two) else {
            return;
        };
        run(&mut job, &options(&env, CancelToken::new()), &env).unwrap();
        assert_eq!(probed(&path).audio.len(), 2);
    }

    #[test]
    fn a_video_becomes_an_animated_gif() {
        let env = Env::new();
        let Some((mut job, _)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        edit(&mut job, |p| p.format = Some(Some(Format::Gif)));
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        assert_eq!(update.state, JobState::Done);
        let gif = env.dir.path().join("clip.gif");
        assert_eq!(update.output, Some(gif.clone()));
        let bytes = fs::read(&gif).unwrap();
        assert_eq!(sniff::sniff(&bytes), Some(Format::Gif));
        assert!(sniff::gif_frame_count(&bytes) > 1);
        let (w, h) = sniff::dimensions(&bytes, Format::Gif).unwrap();
        assert!(w.max(h) <= 800);
        assert_eq!(update.format, Some(Format::Gif));
    }

    #[test]
    fn fit_under_size_lands_under_the_limit() {
        let env = Env::new();
        let Some((mut job, _)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        let limit = job.before * 60 / 100;
        edit(&mut job, |p| p.fit_under = Some(Some(limit)));
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        assert_eq!(update.state, JobState::Done);
        assert!(
            update.after.unwrap() <= limit,
            "{:?} > {limit}",
            update.after
        );
        assert_eq!(update.note, None);
    }

    #[test]
    fn a_hevc_request_without_a_hevc_encoder_is_saved_as_h264() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        edit(&mut job, |p| p.format = Some(Some(Format::Hevc)));
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(probed(&path).video.codec, "h264");
        assert_eq!(update.format, Some(Format::Mp4));
        let video = update.video.unwrap();
        assert_eq!(video.codec, "H.264");
        assert!(!video.hevc_encoder);
        assert_eq!(
            update.note.as_deref(),
            Some("Saved as H.264 — this computer has no HEVC encoder")
        );
    }

    #[test]
    fn a_cancelled_encode_changes_nothing_and_leaves_no_temp_folder() {
        let env = Env::new();
        let big = VideoSpec {
            size: (1280, 720),
            secs: 10,
            ..spec()
        };
        let Some((mut job, path)) = make(&env, "clip.mp4", &big) else {
            return;
        };
        let original = fs::read(&path).unwrap();
        let cancel = CancelToken::new();
        let o = options(&env, cancel.clone());
        let finished = job.run(
            Level::Normal,
            &o,
            &env.backups,
            &mut |u| {
                if matches!(u.state, JobState::Optimising { progress, .. } if progress > 0.0) {
                    cancel.cancel();
                }
            },
            &mut |_, _, _| Ok(()),
        );
        assert!(finished.is_none());
        assert_eq!(fs::read(&path).unwrap(), original);
        assert!(!env.work().join("video").join(job.id.as_str()).exists());
    }

    #[test]
    fn a_broken_video_is_an_error_and_leaves_no_temp_folder() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        let broken = fs::read(&path).unwrap()[..2048].to_vec();
        fs::write(&path, &broken).unwrap();
        let update = run(&mut job, &options(&env, CancelToken::new()), &env)
            .unwrap()
            .update;
        match update.state {
            JobState::Error { reason } => assert!(
                reason == "Couldn't read this video"
                    || reason == "Couldn't encode the video — see Details",
                "{reason}"
            ),
            other => panic!("{other:?}"),
        }
        assert_eq!(fs::read(&path).unwrap(), broken);
        assert!(!env.work().join("video").join(job.id.as_str()).exists());
    }

    #[test]
    fn an_already_small_video_stays_as_it_was() {
        let env = Env::new();
        let Some((mut job, path)) = make(&env, "clip.mp4", &spec()) else {
            return;
        };
        let o = options(&env, CancelToken::new());
        // Aggressive first, so the plain run after it has nothing left to gain
        // (re-encoding a clean first generation at the same quality still shrinks it).
        job.run(
            Level::Aggressive,
            &o,
            &env.backups,
            &mut |_| {},
            &mut |_, _, _| Ok(()),
        )
        .unwrap();
        let once = fs::read(&path).unwrap();
        let mut again = Job::file(&path, &Settings::default(), true).unwrap();
        let update = run(&mut again, &o, &env).unwrap().update;
        assert!(
            matches!(update.state, JobState::AlreadyOptimal | JobState::Larger),
            "{:?}",
            update.state
        );
        assert_eq!(fs::read(&path).unwrap(), once);
    }

    #[test]
    fn the_settings_exclude_large_and_ignored_videos() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.mp4");
        let mut bytes = [&[0, 0, 0, 0x18][..], b"ftypisom", &[0; 12]].concat();
        bytes.resize(2_000_000, 0);
        fs::write(&path, &bytes).unwrap();

        let mut settings = Settings::default();
        settings.video.max_size_mb = 0;
        let settings = settings.sanitised(); // 0 becomes 1
        let err = Job::file(&path, &settings, true).unwrap_err();
        assert_eq!(err, Skip::TooLarge { limit_mb: 1 }.reason());

        let mut settings = Settings::default();
        settings.video.ignored_extensions = vec!["mp4".into()];
        let err = Job::file(&path, &settings, true).unwrap_err();
        assert_eq!(err, ".mp4 files are ignored in Settings");
    }
}
