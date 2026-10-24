//! Audio file jobs end to end. Audio is never read into memory: it moves
//! between the backup, ffmpeg and the destination as files, and the result is
//! placed by the same `put` / `put_back` rules as an image's. Covers go through
//! Ferry's own image engine.

use std::{
    ffi::OsString,
    fs,
    io::Read,
    path::{Path, PathBuf},
    sync::Arc,
};

use super::{Content, Finished, Job, RunOptions, SourceState, bytes_label, describe, shown};
use crate::{
    audio::{
        AudioCard, cover, is_lossless_format, loudness,
        plan::{self, CoverInput, CoverPlan, DropReason, EncodeArgs, Grade, Inputs},
        probe::{self, AudioInfo},
        settings::Audio,
        tool::{self, AacEncoder, AudioTool, ToolError},
    },
    backup::Backups,
    cancel::CancelToken,
    edit::{self, raster},
    i18n::{t, tf},
    job::{JobId, JobState, JobUpdate, Source},
    optimise::{Level, OptimiseError, OptimiseOptions},
    output,
    sniff::{self, Format},
    trigger::{MB, Skip, is_ignored_extension, normalise_ext},
};

/// What an audio run reads besides the shared options.
#[derive(Debug, Clone)]
pub struct AudioRunOptions {
    pub settings: Audio,
    /// `None`: no ffmpeg next to Ferry; audio jobs end in an Error card.
    pub tool: Option<Arc<dyn AudioTool>>,
    pub aac: AacEncoder,
}

impl AudioRunOptions {
    pub fn new(settings: &crate::settings::Settings) -> Self {
        Self {
            settings: settings.audio.clone(),
            tool: tool::installed(),
            aac: tool::aac_encoder(),
        }
    }
}

/// What an audio job knows besides what every job knows.
#[derive(Debug, Clone, Default)]
pub(crate) struct AudioJob {
    pub(crate) info: Option<AudioInfo>,
    /// The original's cover, stored beside the backup.
    pub(crate) cover_file: Option<PathBuf>,
    pub(crate) bitrate_after: Option<u16>,
    pub(crate) cover_removed: bool,
    pub(crate) lufs_after: Option<f32>,
}

impl AudioJob {
    /// Back to "the original is the result".
    pub(crate) fn reset_result(&mut self) {
        self.bitrate_after = None;
        self.cover_removed = false;
        self.lufs_after = None;
    }
}

/// The job's temporary folder, removed however the run ends.
struct TempDir(PathBuf);

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// The audio format of a file and its length, from its first `sniff::HEAD` bytes.
pub(super) fn audio_container(path: &Path) -> Option<(Format, u64)> {
    let mut head = Vec::new();
    fs::File::open(path)
        .ok()?
        .take(sniff::HEAD as u64)
        .read_to_end(&mut head)
        .ok()?;
    let format = sniff::sniff(&head).filter(|f| f.is_audio())?;
    Some((format, fs::metadata(path).ok()?.len()))
}

/// Run one ffmpeg step, reporting its progress over `range` of the whole bar.
fn step(
    tool: &dyn AudioTool,
    args: &[OsString],
    cancel: &CancelToken,
    base: &JobUpdate,
    label: &str,
    range: (f32, f32),
    emit: &mut dyn FnMut(JobUpdate),
) -> Result<String, ToolError> {
    let optimising = |progress: f32, step: String| JobUpdate {
        state: JobState::Optimising { progress, step },
        ..base.clone()
    };
    emit(optimising(range.0, label.to_owned()));
    let mut shown_at = range.0;
    tool.run(args, cancel, &mut |p| {
        let overall = range.0 + (range.1 - range.0) * p;
        if overall - shown_at >= 0.01 || (p >= 1.0 && overall > shown_at) {
            shown_at = overall;
            let percent = (overall * 100.0).round().to_string();
            let text = tf("audio.step.progress", &[("step", label), ("n", &percent)]);
            emit(optimising(overall, text));
        }
    })
}

/// "first note wins".
fn note(slot: &mut Option<String>, text: String) {
    slot.get_or_insert(text);
}

impl Job {
    pub fn is_audio(&self) -> bool {
        self.audio.is_some()
    }

    /// A copied audio file, to be optimised in place. `Err` is the card text
    /// for a file the settings exclude.
    pub(super) fn audio_file(
        path: &Path,
        settings: &crate::settings::Settings,
        format: Format,
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
        if !ext.is_empty() && is_ignored_extension(&settings.audio.ignored_extensions, &ext) {
            return Err(Skip::IgnoredExtension(normalise_ext(&ext)).reason());
        }
        let limit_mb = settings.audio.max_size_mb;
        if len > u64::from(limit_mb) * MB {
            return Err(Skip::TooLarge { limit_mb }.reason());
        }
        Ok(Job {
            id: JobId::generate(),
            source: Source::File,
            name: stem,
            source_ext: ext.clone(),
            ext,
            format,
            before: len,
            dims: None,
            from_bitmap: false,
            output: path.to_owned(),
            level: Level::Normal,
            edits: edit::Edits::default(),
            prior: None,
            source_format: format,
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
            video: None,
            audio: Some(AudioJob::default()),
        })
    }

    fn audio_state(&mut self) -> &mut AudioJob {
        self.audio.as_mut().expect("an audio job")
    }

    pub(super) fn audio_card(&self) -> AudioCard {
        let audio = self.audio.as_ref();
        let info = audio.and_then(|a| a.info.as_ref());
        AudioCard {
            bitrate_before: info.filter(|i| !i.lossless()).and_then(|i| i.kbps),
            bitrate_after: audio.and_then(|a| a.bitrate_after),
            lossless: match info {
                Some(info) if self.format == self.source_format => info.lossless(),
                _ => is_lossless_format(self.format),
            },
            has_cover: audio.is_some_and(|a| a.cover_file.is_some()),
        }
    }

    /// "320 → 192 kbps · Cover 3000×3000 → 600×600 · MP3 · −14 LUFS"
    pub(super) fn audio_detail(&self) -> String {
        let mut parts = Vec::new();
        let card = self.audio_card();
        if !card.lossless {
            match (card.bitrate_before, card.bitrate_after) {
                (Some(from), Some(to)) if from != to => parts.push(tf(
                    "audio.detail.kbps_change",
                    &[("from", &from.to_string()), ("to", &to.to_string())],
                )),
                (_, Some(n)) => parts.push(tf("audio.detail.kbps", &[("n", &n.to_string())])),
                _ => {}
            }
        }
        let removed = self.audio.as_ref().is_some_and(|a| a.cover_removed);
        match (self.dims, self.dims_after) {
            _ if removed => parts.push(t("audio.detail.cover_removed")),
            (Some((w, h)), Some((aw, ah))) => parts.push(tf(
                "audio.detail.cover_change",
                &[
                    ("w", &w.to_string()),
                    ("h", &h.to_string()),
                    ("aw", &aw.to_string()),
                    ("ah", &ah.to_string()),
                ],
            )),
            (Some((w, h)), None) => parts.push(tf(
                "audio.detail.cover",
                &[("w", &w.to_string()), ("h", &h.to_string())],
            )),
            _ => {}
        }
        parts.push(self.format.label().to_owned());
        if let Some(lufs) = self.audio.as_ref().and_then(|a| a.lufs_after) {
            let label = tf(
                "audio.detail.lufs",
                &[("n", &lufs.to_string().replace('-', "−"))],
            );
            parts.push(label);
        }
        if self.level == Level::Aggressive {
            parts.push(t("card.detail.aggressive"));
        }
        parts.join(" · ")
    }

    pub(super) fn run_audio(
        &mut self,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
    ) -> Option<Finished> {
        // 1. The original is backed up first.
        let backup = match self.backup_path(backups) {
            Ok(path) => path,
            Err(e) => return Some(self.fail(tf("error.backup", &[("why", &describe(&e))]), e)),
        };
        let Some(tool) = o.audio.tool.clone() else {
            return Some(self.fail(t("audio.error.ffmpeg_missing"), "ffmpeg not found"));
        };
        let tmp = TempDir(o.work_dir.join("audio").join(self.id.as_str()));
        if let Err(e) = fs::create_dir_all(&tmp.0) {
            let reason = tf("error.write", &[("dir", &shown(&tmp.0))]);
            return Some(self.fail(reason, e));
        }

        // 2. What is in the file, and its cover.
        if self.audio.as_ref().is_some_and(|a| a.info.is_none()) {
            let base = self.update(JobState::Queued, None);
            let label = t("audio.step.probe");
            let log = match step(
                &*tool,
                &probe::args(&backup),
                &o.cancel,
                &base,
                &label,
                (0.0, 0.0),
                emit,
            ) {
                Ok(log) => log,
                Err(ToolError::Cancelled) => return None,
                Err(ToolError::Failed(why)) => return Some(self.fail(self.decode_reason(), why)),
            };
            let info = match probe::parse(&log) {
                Ok(info) => info,
                Err(e) => return Some(self.fail(self.decode_reason(), format!("{e:?}"))),
            };
            if let Some(ext) = info.cover.as_ref().and_then(|c| cover::file_ext(&c.codec)) {
                let out = tmp.0.join(format!("cover-original.{ext}"));
                match tool.run(&cover::extract_args(&backup, &out), &o.cancel, &mut |_| {}) {
                    Err(ToolError::Cancelled) => return None,
                    Err(ToolError::Failed(_)) => {}
                    Ok(_) => {
                        if let Ok(bytes) = fs::read(&out)
                            && let Ok(stored) = backups.store_bytes(
                                self.id.as_str(),
                                &format!(".ferry-cover.{ext}"),
                                &bytes,
                            )
                        {
                            self.dims = sniff::sniff(&bytes)
                                .and_then(|f| raster::display_dims(&bytes, f))
                                .or(info.cover.as_ref().map(|c| c.dims));
                            self.audio_state().cover_file = Some(stored);
                        }
                    }
                }
            }
            self.audio_state().info = Some(info);
        }
        let info = self.audio.as_ref()?.info.clone()?;

        // 3. What to write.
        let mut plan = plan::resolve_with(
            &Inputs {
                info: &info,
                source: self.source_format,
                edits: &self.edits,
                level: self.level,
                settings: &o.audio.settings,
            },
            o.audio.aac,
        );
        plan.explicit |= self.twin;
        let audio_o = RunOptions {
            output: o.audio.settings.output.clone(),
            ..o.clone()
        };
        if plan.nothing_to_do(self.source_format) && !self.twin {
            return Some(self.keep_audio(&backup, &audio_o));
        }

        // 4. The cover.
        let mut note_text = None;
        let mut dims_after = None;
        let mut cover_removed = false;
        let mut cover_input = CoverInput::None;
        let cover_out;
        let stored_cover = self.audio.as_ref()?.cover_file.clone();
        match plan.cover {
            CoverPlan::Absent => {}
            CoverPlan::Copy => cover_input = CoverInput::Source,
            CoverPlan::Optimise | CoverPlan::Resize(_) | CoverPlan::Scale(_) => {
                let options = OptimiseOptions {
                    level: Level::Normal,
                    compression: o.compression,
                    strip_metadata: o.strip_metadata,
                    gifsicle: None,
                    cancel: o.cancel.clone(),
                };
                let processed = stored_cover
                    .as_ref()
                    .and_then(|file| fs::read(file).ok())
                    .map(|bytes| cover::process(&bytes, plan.cover, &options));
                match processed {
                    Some(Ok(p)) => {
                        cover_out = tmp.0.join(format!("cover.{}", p.ext));
                        if let Err(e) = fs::write(&cover_out, &p.bytes) {
                            let reason = tf("error.write", &[("dir", &shown(&tmp.0))]);
                            return Some(self.fail(reason, e));
                        }
                        dims_after = Some(p.dims).filter(|d| Some(*d) != self.dims);
                        cover_input = CoverInput::File(&cover_out);
                    }
                    Some(Err(OptimiseError::Cancelled)) => return None,
                    _ => {
                        note(&mut note_text, t("audio.note.cover_unreadable"));
                        cover_removed = true;
                    }
                }
            }
            CoverPlan::Remove => cover_removed = true,
            CoverPlan::Drop(reason) => {
                cover_removed = true;
                note(
                    &mut note_text,
                    match reason {
                        DropReason::Container => tf(
                            "audio.note.cover_dropped",
                            &[("format", plan.target.label())],
                        ),
                        DropReason::Unreadable => t("audio.note.cover_unreadable"),
                    },
                );
            }
        }

        // 5. Loudness: measure first.
        let target_label = plan.target.label();
        let mut filter = None;
        let mut spec = None;
        let encode_label;
        let encode_range;
        if let Some(l) = plan.loudness {
            let s = loudness::spec(l, info.channels)?;
            let base = self.update(JobState::Queued, None);
            let measure = (0.0, loudness::MEASURE_SHARE);
            let log = match step(
                &*tool,
                &loudness::measure_args(&backup, &s),
                &o.cancel,
                &base,
                &t("audio.step.measure_of"),
                measure,
                emit,
            ) {
                Ok(log) => log,
                Err(ToolError::Cancelled) => return None,
                Err(ToolError::Failed(why)) => {
                    return Some(self.fail(t("audio.error.measure"), why));
                }
            };
            match loudness::parse_measurement(&log) {
                Ok(m) if m.is_measurable() => filter = Some(loudness::filter(&s, &m)),
                Ok(_) => note(&mut note_text, t("audio.note.too_quiet")),
                Err(e) => return Some(self.fail(t("audio.error.measure"), format!("{e:?}"))),
            }
            spec = Some(s);
            encode_label = tf("audio.step.encode_of", &[("format", target_label)]);
            encode_range = (loudness::MEASURE_SHARE, 1.0);
        } else {
            encode_label = tf("audio.step.encode", &[("format", target_label)]);
            encode_range = (0.0, 1.0);
        }

        // 6. Encode, and for Fit under size encode again at a lower bitrate.
        let out = tmp.0.join(format!("out.{}", plan.target.extension()));
        let mut tries = 0;
        let limited = loop {
            let args = plan::encode_args(&EncodeArgs {
                input: &backup,
                cover: cover_input,
                plan: &plan,
                filter: filter.as_deref(),
                output: &out,
            });
            let base = self.update(JobState::Queued, None);
            let limited = match step(
                &*tool,
                &args,
                &o.cancel,
                &base,
                &encode_label,
                encode_range,
                emit,
            ) {
                Ok(log) => filter.is_some() && loudness::was_limited(&log),
                Err(ToolError::Cancelled) => return None,
                Err(ToolError::Failed(why)) => {
                    let reason = tf("error.encode", &[("format", target_label)]);
                    return Some(self.fail(reason, why));
                }
            };
            let (Some(limit), Some(kbps)) = (plan.fit_under, plan.kbps) else {
                break limited;
            };
            let size = fs::metadata(&out).map_or(0, |m| m.len());
            if size <= limit || tries == 2 {
                break limited;
            }
            match plan::retry_kbps(plan.target, kbps, limit, size) {
                Some(next) => plan.kbps = Some(plan::encoder_kbps(plan.target, plan.aac, next)),
                None => break limited,
            }
            tries += 1;
        };
        let size = match fs::metadata(&out) {
            Ok(meta) => meta.len(),
            Err(e) => return Some(self.fail(t("error.unexpected"), e)),
        };
        if limited && let Some(s) = &spec {
            note(
                &mut note_text,
                tf("audio.note.limited", &[("target", &loudness::label(s))]),
            );
        }
        if let Some(limit) = plan.fit_under
            && size > limit
        {
            note_text = Some(tf("card.note.fit_missed", &[("size", &bytes_label(limit))]));
        }

        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }

        // 7. Never worse, for a plain run; otherwise place it.
        if !plan.explicit && plan::grade(self.before, size) != Grade::Smaller {
            let state = if plan::grade(self.before, size) == Grade::Larger {
                JobState::Larger
            } else {
                JobState::AlreadyOptimal
            };
            return Some(self.keep_audio_as(state, &backup, &audio_o));
        }
        self.dims_after = dims_after;
        self.note = note_text;
        let lossless = is_lossless_format(plan.target);
        let state = self.audio_state();
        state.bitrate_after = if lossless {
            None
        } else if plan.copy_audio {
            info.kbps
        } else {
            plan.kbps
        };
        state.cover_removed = cover_removed;
        state.lufs_after = spec.filter(|_| filter.is_some()).map(|s| s.i);
        let placed = self.put(
            Content::File(&out),
            plan.target,
            Content::File(&backup),
            &audio_o,
            &mut |_, _, _| Ok(()),
        );
        Some(match placed {
            Ok(()) => Finished {
                update: self.update(JobState::Done, Some(size)),
                cause: None,
            },
            Err((reason, cause)) => self.fail(reason, cause),
        })
    }

    fn decode_reason(&self) -> String {
        tf("error.decode", &[("format", self.source_format.label())])
    }

    fn keep_audio(&mut self, backup: &Path, o: &RunOptions) -> Finished {
        self.keep_audio_as(JobState::AlreadyOptimal, backup, o)
    }

    /// Leave the original as the result.
    fn keep_audio_as(&mut self, state: JobState, backup: &Path, o: &RunOptions) -> Finished {
        let replace = self.changed;
        if let Err((reason, cause)) =
            self.put_back(Content::File(backup), replace, o, &mut |_, _, _| Ok(()))
        {
            return self.fail(reason, cause);
        }
        self.dims_after = None;
        self.note = None;
        self.audio_state().reset_result();
        Finished {
            update: self.update(state, None),
            cause: None,
        }
    }

    pub(super) fn restore_audio(&mut self, o: &RunOptions) -> Finished {
        let Some(backup) = self.backup.clone().filter(|b| b.exists()) else {
            return self.fail(t("error.backup_missing"), "the backup is gone");
        };
        if let Err((reason, cause)) =
            self.put_back(Content::File(&backup), true, o, &mut |_, _, _| Ok(()))
        {
            return self.fail(reason, cause);
        }
        self.edits = edit::Edits::default();
        self.dims_after = None;
        self.note = None;
        self.audio_state().reset_result();
        Finished {
            update: self.update(JobState::Restored, None),
            cause: None,
        }
    }

    /// Save the original's cover beside the result, under a free name.
    pub fn extract_cover(&self) -> Result<PathBuf, String> {
        let file = self
            .audio
            .as_ref()
            .and_then(|a| a.cover_file.clone())
            .ok_or_else(|| t("audio.cover.none"))?;
        let ext = file
            .extension()
            .map_or("jpg".into(), |e| e.to_string_lossy());
        let wanted = self
            .output
            .with_file_name(format!("{} cover.{ext}", self.name));
        let dest = output::unique(&wanted, None);
        fs::copy(&file, &dest).map_err(|e| describe(&e))?;
        Ok(dest)
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, io};

    use super::*;
    use crate::{
        audio::{loudness::samples as loud, probe::samples},
        edit::EditsPatch,
        settings::Settings,
        testutil::{self, FakeStep, FakeTool},
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
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.dir.path().join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    fn test_trash(path: &Path) -> io::Result<()> {
        let bin = path.parent().unwrap().join(".trash");
        fs::create_dir_all(&bin)?;
        fs::rename(path, bin.join(path.file_name().unwrap()))
    }

    fn wav_bytes() -> Vec<u8> {
        testutil::wav_tone(&[(1.0, 0.5)], 44100, 2)
    }

    fn mp3_bytes() -> Vec<u8> {
        let mut b = vec![0xFF, 0xFB, 0x90, 0x64];
        b.resize(3000, 0);
        b
    }

    fn options(env: &Env, fake: &Arc<FakeTool>) -> RunOptions {
        let mut o = RunOptions::new(
            &Settings::default(),
            None,
            env.dir.path().join("work"),
            test_trash,
            CancelToken::new(),
        );
        o.audio.tool = Some(fake.clone());
        o.audio.aac = AacEncoder::Native;
        o
    }

    fn audio_job(name: &str, bytes: &[u8], fake: &Arc<FakeTool>) -> (Job, Env, RunOptions) {
        let env = Env::new();
        let job = Job::file(&env.file(name, bytes), &Settings::default(), false).unwrap();
        let o = options(&env, fake);
        (job, env, o)
    }

    fn run(job: &mut Job, o: &RunOptions, env: &Env) -> (Option<Finished>, Vec<JobUpdate>) {
        let seen = RefCell::new(Vec::new());
        let finished = job.run(
            Level::Normal,
            o,
            &env.backups,
            &mut |u| seen.borrow_mut().push(u),
            &mut |_, _, _| Ok(()),
        );
        (finished, seen.into_inner())
    }

    fn edit(job: &mut Job, patch: serde_json::Value) {
        job.edit(serde_json::from_value::<EditsPatch>(patch).unwrap());
    }

    fn write(len: usize) -> FakeStep {
        FakeStep::Write {
            log: String::new(),
            bytes: vec![1; len],
        }
    }

    fn probe_log() -> FakeStep {
        FakeStep::Log(samples::WAV_STEREO.into())
    }

    /// The `-b:a` value of every encode call.
    fn bitrates(fake: &FakeTool) -> Vec<String> {
        fake.calls()
            .iter()
            .filter_map(|c| c.iter().position(|a| a == "-b:a").map(|i| c[i + 1].clone()))
            .collect()
    }

    fn has_pair(call: &[String], a: &str, b: &str) -> bool {
        call.windows(2).any(|w| w[0] == a && w[1] == b)
    }

    #[test]
    fn an_audio_file_becomes_an_audio_job() {
        let env = Env::new();
        let path = env.file("my tone.wav", &wav_bytes());
        let job = Job::file(&path, &Settings::default(), false).unwrap();
        assert!(job.is_audio());
        assert_eq!(
            (job.format, job.ext.as_str(), job.name.as_str()),
            (Format::Wav, "wav", "my tone")
        );
        let card = job.update(JobState::Queued, None);
        assert_eq!(
            card.audio,
            Some(AudioCard {
                bitrate_before: None,
                bitrate_after: None,
                lossless: true,
                has_cover: false
            })
        );
        assert_eq!((card.thumb_path, card.can_edit), (None, false));
    }

    #[test]
    fn audio_files_follow_the_audio_skip_rules() {
        let env = Env::new();
        let mut s = Settings::default();
        s.audio.ignored_extensions = vec!["aif".into()];
        let aiff = env.file("x.aif", b"FORM\0\0\0\x10AIFFCOMM");
        assert_eq!(
            Job::file(&aiff, &s, true).unwrap_err(),
            ".aiff files are ignored in Settings"
        );
        s.images.ignored_extensions = vec!["wav".into()];
        s.audio.max_size_mb = 1;
        let big = env.file("big.wav", &testutil::wav_tone(&[(6.0, 0.5)], 44100, 2));
        assert_eq!(
            Job::file(&big, &s, true).unwrap_err(),
            "Larger than 1 MB — change the limit in Settings"
        );
    }

    #[test]
    fn an_mp4_video_is_not_taken_for_audio() {
        let env = Env::new();
        let mut mp4 = b"\0\0\0\x18ftypisom\0\0\x02\0isomiso2".to_vec();
        mp4.extend_from_slice(&[0; 64]);
        let path = env.file("clip.mp4", &mp4);
        assert!(Job::file(&path, &Settings::default(), true).map_or(true, |j| !j.is_audio()));
    }

    #[test]
    fn an_audio_edit_keeps_its_format_choice_and_refuses_image_formats() {
        let env = Env::new();
        let mut job =
            Job::file(&env.file("a.mp3", &mp3_bytes()), &Settings::default(), true).unwrap();
        edit(&mut job, serde_json::json!({ "format": "mp3" }));
        assert_eq!(job.edits.format, Some(Format::Mp3));
        edit(&mut job, serde_json::json!({ "format": "png" }));
        assert_eq!(job.edits.format, None);
        let mut image = Job::file(
            &env.file("s.png", &testutil::png(8, 8)),
            &Settings::default(),
            true,
        )
        .unwrap();
        edit(&mut image, serde_json::json!({ "format": "mp3" }));
        assert_eq!(image.edits.format, None);
        assert_eq!(image.update(JobState::Queued, None).audio, None);
    }

    fn wav_to_mp3() -> (
        Job,
        Env,
        RunOptions,
        Arc<FakeTool>,
        Finished,
        Vec<JobUpdate>,
    ) {
        let fake = FakeTool::new(vec![probe_log(), write(1000)]);
        let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
        let (f, seen) = run(&mut job, &o, &env);
        (job, env, o, fake, f.unwrap(), seen)
    }

    #[test]
    fn a_wav_becomes_an_mp3_beside_it_and_the_wav_goes_to_the_trash() {
        let (job, env, o, _, f, seen) = wav_to_mp3();
        let u = f.update;
        assert_eq!(
            (u.state, u.after, u.format, u.converted),
            (JobState::Done, Some(1000), Some(Format::Mp3), true)
        );
        assert_eq!(u.output.unwrap(), env.dir.path().join("tone.mp3"));
        assert!(
            !env.dir.path().join("tone.wav").exists()
                && env.dir.path().join(".trash/tone.wav").exists()
        );
        assert_eq!(u.detail, "192 kbps · MP3");
        assert_eq!(u.audio.unwrap().bitrate_after, Some(192));
        assert!(seen.iter().any(|s| matches!(&s.state,
            JobState::Optimising { progress, step } if *progress == 0.5 && step == "Encoding MP3 · 50 %")));
        assert!(!o.work_dir.join("audio").join(job.id.as_str()).exists());
    }

    #[test]
    fn loudness_runs_two_passes_and_reports_progress_across_both() {
        let fake = FakeTool::new(vec![
            probe_log(),
            FakeStep::Log(loud::MEASURED.into()),
            FakeStep::Write {
                log: loud::PASS2_DYNAMIC.into(),
                bytes: vec![1; 1000],
            },
        ]);
        let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
        edit(&mut job, serde_json::json!({ "loudness": "streaming" }));
        let (f, seen) = run(&mut job, &o, &env);
        let u = f.unwrap().update;
        let calls = fake.calls();
        assert_eq!(calls.len(), 3);
        assert!(
            calls[1].iter().any(|a| a.contains("print_format=json"))
                && has_pair(&calls[1], "-f", "null")
        );
        assert!(calls[2].iter().any(|a| a.contains("measured_I=-27.61")));
        let at = |wanted: f32, text: &str| {
            seen.iter().any(|s| matches!(&s.state,
                JobState::Optimising { progress, step } if (progress - wanted).abs() < 1e-4 && step == text))
        };
        assert!(at(0.15, "Step 1 of 2 · Measuring loudness · 15 %"));
        assert!(at(0.65, "Step 2 of 2 · Encoding MP3 · 65 %"));
        assert_eq!(u.note.as_deref(), Some("Peaks limited to reach −14 LUFS"));
        assert_eq!(u.detail, "192 kbps · MP3 · −14 LUFS");
    }

    #[test]
    fn an_unmeasurable_file_keeps_its_loudness_and_says_so() {
        let fake = FakeTool::new(vec![
            probe_log(),
            FakeStep::Log(loud::SILENT.into()),
            write(1000),
        ]);
        let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
        edit(&mut job, serde_json::json!({ "loudness": "streaming" }));
        let u = run(&mut job, &o, &env).0.unwrap().update;
        assert_eq!(u.state, JobState::Done);
        assert!(!fake.calls()[2].iter().any(|a| a.contains("loudnorm")));
        assert_eq!(
            u.note.as_deref(),
            Some("Too quiet to measure — loudness left as it was")
        );
        assert!(!u.detail.contains("LUFS"));
    }

    #[test]
    fn a_cancel_during_the_encode_changes_nothing_and_leaves_no_temp_file() {
        let fake = FakeTool::new(vec![probe_log(), write(1000)]);
        *fake.cancel_on_call.lock().unwrap() = Some(1);
        let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
        edit(&mut job, serde_json::json!({ "bitrate": 128 }));
        let before = fs::read(env.dir.path().join("tone.wav")).unwrap();
        assert!(run(&mut job, &o, &env).0.is_none());
        assert_eq!(fs::read(env.dir.path().join("tone.wav")).unwrap(), before);
        assert!(
            !env.dir.path().join("tone.mp3").exists() && !env.dir.path().join(".trash").exists()
        );
        let leftovers = fs::read_dir(env.dir.path())
            .unwrap()
            .filter_map(Result::ok)
            .any(|e| e.file_name().to_string_lossy().contains("ferry-tmp"));
        assert!(!leftovers);
        assert!(!o.work_dir.join("audio").join(job.id.as_str()).exists());
        assert_eq!(job.edits.bitrate, None);
    }

    #[test]
    fn an_encoder_failure_is_an_error_with_retry_and_the_source_is_untouched() {
        let fake = FakeTool::new(vec![
            probe_log(),
            FakeStep::Fail("Unknown encoder 'libmp3lame'".into()),
        ]);
        let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
        let f = run(&mut job, &o, &env).0.unwrap();
        assert_eq!(
            f.update.state,
            JobState::Error {
                reason: "Encoder failed (MP3)".into()
            }
        );
        assert!(f.update.can_retry && f.cause.unwrap().contains("libmp3lame"));
        assert_eq!(
            fs::read(env.dir.path().join("tone.wav")).unwrap(),
            wav_bytes()
        );
        assert!(!o.work_dir.join("audio").join(job.id.as_str()).exists());
    }

    #[test]
    fn a_low_bitrate_mp3_is_already_optimal_without_encoding() {
        let log: String = samples::MP3_COVER
            .replace("320 kb/s", "128 kb/s")
            .lines()
            .filter(|l| !l.contains(": Video:"))
            .collect::<Vec<_>>()
            .join("\n");
        let fake = FakeTool::new(vec![FakeStep::Log(log)]);
        let (mut job, env, o) = audio_job("low.mp3", &mp3_bytes(), &fake);
        let f = run(&mut job, &o, &env).0.unwrap();
        assert_eq!(f.update.state, JobState::AlreadyOptimal);
        assert_eq!(fake.calls().len(), 1);
        assert_eq!(
            fs::read(env.dir.path().join("low.mp3")).unwrap(),
            mp3_bytes()
        );
    }

    #[test]
    fn a_missing_ffmpeg_is_an_error_card() {
        let fake = FakeTool::new(vec![]);
        let (mut job, env, mut o) = audio_job("tone.wav", &wav_bytes(), &fake);
        o.audio.tool = None;
        let f = run(&mut job, &o, &env).0.unwrap();
        assert_eq!(
            f.update.state,
            JobState::Error {
                reason: "ffmpeg missing — reinstall Ferry".into()
            }
        );
        assert_eq!(
            fs::read(env.dir.path().join("tone.wav")).unwrap(),
            wav_bytes()
        );
    }

    #[test]
    fn restore_brings_the_wav_back_and_trashes_the_mp3() {
        let (mut job, env, o, _, _, _) = wav_to_mp3();
        let f = job.restore(&o, &mut |_, _, _| Ok(()));
        assert_eq!(f.update.state, JobState::Restored);
        assert_eq!(
            fs::read(env.dir.path().join("tone.wav")).unwrap(),
            wav_bytes()
        );
        assert!(env.dir.path().join(".trash/tone.mp3").exists());
        assert_eq!(f.update.audio.unwrap().bitrate_after, None);
    }

    fn one_minute() -> FakeStep {
        FakeStep::Log(samples::WAV_STEREO.replace("00:00:01.00", "00:01:00.00"))
    }

    #[test]
    fn fit_under_retries_with_a_lower_bitrate_then_notes_a_miss() {
        let fake = FakeTool::new(vec![
            one_minute(),
            write(1_200_000),
            write(1_100_000),
            write(1_050_000),
        ]);
        let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
        edit(&mut job, serde_json::json!({ "fit_under": 1_000_000 }));
        let u = run(&mut job, &o, &env).0.unwrap().update;
        assert_eq!(fake.calls().len(), 4);
        let rates: Vec<u32> = bitrates(&fake)
            .iter()
            .map(|b| b.trim_end_matches('k').parse().unwrap())
            .collect();
        assert_eq!(rates[0], 128);
        assert!(rates.windows(2).all(|w| w[1] < w[0]), "{rates:?}");
        assert_eq!(
            u.note.as_deref(),
            Some("Couldn't reach 1 MB — try downscaling")
        );
        assert_eq!(u.state, JobState::Done);
    }

    #[test]
    fn a_source_named_like_an_option_is_passed_safely() {
        let fake = FakeTool::new(vec![probe_log(), write(1000)]);
        let (mut job, env, o) = audio_job("-intro ü.wav", &wav_bytes(), &fake);
        run(&mut job, &o, &env);
        for call in fake.calls() {
            let at = call.iter().position(|a| a == "-i").unwrap() + 1;
            assert!(Path::new(&call[at]).is_absolute() && call[at].ends_with("-intro ü.wav"));
            let last = call.last().unwrap();
            assert!(last == "-" || Path::new(last).is_absolute());
        }
    }

    fn mp3_with_cover(extra: serde_json::Value) -> (Job, Env, RunOptions, Arc<FakeTool>) {
        let log = samples::MP3_COVER.replace("3000x3000", "300x300");
        let fake = FakeTool::new(vec![
            FakeStep::Log(log),
            FakeStep::Write {
                log: String::new(),
                bytes: testutil::jpeg(300, 300, 95, &Default::default()),
            },
            write(1000),
        ]);
        let (mut job, env, o) = audio_job("a.mp3", &mp3_bytes(), &fake);
        edit(&mut job, extra);
        (job, env, o, fake)
    }

    #[test]
    fn the_card_shows_the_cover_and_its_new_size() {
        let (mut job, env, o, fake) = mp3_with_cover(
            serde_json::json!({ "size": { "kind": "exact", "width": 150, "height": 150 } }),
        );
        let u = run(&mut job, &o, &env).0.unwrap().update;
        let thumb = u.thumb_path.unwrap();
        assert!(thumb.starts_with(env.dir.path().join("backups")));
        assert!(thumb.to_string_lossy().ends_with(".ferry-cover.jpg"));
        assert_eq!(
            (u.dims_before, u.dims_after),
            (Some((300, 300)), Some((150, 150)))
        );
        assert_eq!(u.detail, "320 → 192 kbps · Cover 300×300 → 150×150 · MP3");
        let encode = &fake.calls()[2];
        assert!(has_pair(encode, "-map", "1:v:0") && encode.iter().any(|a| a == "attached_pic"));
        assert!(u.audio.unwrap().has_cover);
    }

    #[test]
    fn an_opus_target_drops_the_cover_with_a_note() {
        let (mut job, env, o, fake) = mp3_with_cover(serde_json::json!({ "format": "opus" }));
        let u = run(&mut job, &o, &env).0.unwrap().update;
        assert_eq!(
            u.note.as_deref(),
            Some("Opus can't hold cover art — cover removed")
        );
        assert!(u.detail.contains("Cover removed"), "{}", u.detail);
        let encode = &fake.calls()[2];
        assert!(!has_pair(encode, "-map", "1:v:0") && !has_pair(encode, "-map", "0:v:0"));
        assert!(u.output.unwrap().to_string_lossy().ends_with(".opus"));
    }

    #[test]
    fn extract_cover_saves_beside_the_result() {
        let (mut job, env, o, _) = mp3_with_cover(
            serde_json::json!({ "size": { "kind": "exact", "width": 150, "height": 150 } }),
        );
        run(&mut job, &o, &env);
        assert_eq!(job.extract_cover(), Ok(env.dir.path().join("a cover.jpg")));
        assert_eq!(
            job.extract_cover(),
            Ok(env.dir.path().join("a cover (2).jpg"))
        );
    }

    #[test]
    fn an_m4a_on_audiotoolbox_reports_the_snapped_bitrate() {
        let fake = FakeTool::new(vec![probe_log(), write(1000)]);
        let (mut job, env, mut o) = audio_job("tone.wav", &wav_bytes(), &fake);
        o.audio.aac = AacEncoder::AudioToolbox;
        edit(
            &mut job,
            serde_json::json!({ "format": "m4a", "bitrate": 150 }),
        );
        let u = run(&mut job, &o, &env).0.unwrap().update;
        assert_eq!(u.state, JobState::Done);
        assert!(u.output.unwrap().to_string_lossy().ends_with(".m4a"));
        let encode = &fake.calls()[1];
        assert!(has_pair(encode, "-c:a", "aac_at") && has_pair(encode, "-b:a", "128k"));
        assert!(has_pair(encode, "-aac_at_mode", "cvbr"));
        assert_eq!(u.audio.unwrap().bitrate_after, Some(128));
        assert_eq!(u.detail, "128 kbps · M4A");
    }

    #[test]
    fn fit_under_on_audiotoolbox_uses_cbr_and_snapped_retries() {
        let fake = FakeTool::new(vec![
            one_minute(),
            write(1_200_000),
            write(1_100_000),
            write(1_050_000),
        ]);
        let (mut job, env, mut o) = audio_job("tone.wav", &wav_bytes(), &fake);
        o.audio.aac = AacEncoder::AudioToolbox;
        edit(
            &mut job,
            serde_json::json!({ "format": "m4a", "fit_under": 1_000_000 }),
        );
        let u = run(&mut job, &o, &env).0.unwrap().update;
        let calls = fake.calls();
        assert_eq!(calls.len(), 4);
        for call in &calls[1..] {
            assert!(has_pair(call, "-c:a", "aac_at") && has_pair(call, "-aac_at_mode", "cbr"));
        }
        assert_eq!(bitrates(&fake), ["128k", "96k", "80k"]);
        assert_eq!(
            u.note.as_deref(),
            Some("Couldn't reach 1 MB — try downscaling")
        );
        assert_eq!(u.audio.unwrap().bitrate_after, Some(80));
    }
}
