//! Real folders, the real `notify` watcher and the real pipeline: the proof
//! that Ferry's own writes never come back as new files.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use super::engine::{Notice, Ready};
use super::media::Media;
use super::plan::{DirState, WatchStatus};
use super::service::{ServiceConfig, ServiceHandle, spawn};
use super::settle::SettleConfig;
use crate::backup::Backups;
use crate::cancel::CancelToken;
use crate::edit::EditsPatch;
use crate::job::JobState;
use crate::optimise::Level;
use crate::pipeline::{Finished, Job, RunOptions};
use crate::settings::{Location, Settings, Watch, WatchedFolder};
use crate::sniff::Format;
use crate::testutil;

const READY_WITHIN: Duration = Duration::from_secs(15);
const QUIET_FOR: Duration = Duration::from_secs(4);

struct Harness {
    tmp: tempfile::TempDir,
    handle: ServiceHandle,
    rx: Receiver<Notice>,
}

impl Harness {
    fn new() -> Self {
        Self::with(true, |_| {})
    }

    /// Watches `tmp/watched` (created first when `create`); backups in
    /// `tmp/backups` and work files in `tmp/work` are Ferry's own folders.
    fn with(create: bool, edit: impl FnOnce(&mut WatchedFolder)) -> Self {
        // FSEvents reports real paths; macOS temp is a /var -> /private/var symlink.
        let tmp = tempfile::tempdir_in(std::env::temp_dir().canonicalize().unwrap()).unwrap();
        for name in ["backups", "work"] {
            fs::create_dir(tmp.path().join(name)).unwrap();
        }
        if create {
            fs::create_dir(tmp.path().join("watched")).unwrap();
        }
        let mut folder = WatchedFolder {
            path: tmp.path().join("watched").to_string_lossy().into_owned(),
            ..WatchedFolder::default()
        };
        edit(&mut folder);
        let watch = Watch {
            folders: vec![folder],
            ..Watch::default()
        };
        let config = ServiceConfig {
            watch,
            home: tmp.path().to_owned(),
            excluded: vec![tmp.path().join("backups"), tmp.path().join("work")],
            captures: Box::new(Vec::new),
            settle: SettleConfig {
                quiet: Duration::from_millis(200),
                stable: Duration::from_millis(300),
                poll: Duration::from_millis(100),
                max_wait: Duration::from_secs(60),
                max_pending: 100,
            },
            rescan_every: Duration::from_secs(3600),
        };
        let (tx, rx) = mpsc::channel();
        let handle = spawn(
            config,
            Box::new(move |notice| {
                let _ = tx.send(notice);
            }),
        )
        .unwrap();
        Self { tmp, handle, rx }
    }

    fn dir(&self, name: &str) -> PathBuf {
        self.tmp.path().join(name)
    }

    fn watched(&self, name: &str) -> PathBuf {
        self.dir("watched").join(name)
    }

    fn watch_with(&self, edit: impl FnOnce(&mut WatchedFolder)) -> Watch {
        let mut folder = WatchedFolder {
            path: self.dir("watched").to_string_lossy().into_owned(),
            ..WatchedFolder::default()
        };
        edit(&mut folder);
        Watch {
            folders: vec![folder],
            ..Watch::default()
        }
    }

    fn expect_ready(&self) -> Ready {
        let end = Instant::now() + READY_WITHIN;
        let mut statuses = Vec::new();
        loop {
            match self
                .rx
                .recv_timeout(end.saturating_duration_since(Instant::now()))
            {
                Ok(Notice::Ready(ready)) => return ready,
                Ok(Notice::Status(status)) => statuses.push(status),
                Ok(other) => panic!("unexpected {other:?}"),
                Err(e) => {
                    let probes: Vec<_> = fs::read_dir(self.dir("watched"))
                        .into_iter()
                        .flatten()
                        .flatten()
                        .map(|f| (f.file_name(), super::probe::probe(&f.path())))
                        .collect();
                    panic!(
                        "no file became ready: {e:?}; statuses: {statuses:?}; probes: {probes:?}"
                    )
                }
            }
        }
    }

    fn expect_quiet(&self) {
        let end = Instant::now() + QUIET_FOR;
        loop {
            match self
                .rx
                .recv_timeout(end.saturating_duration_since(Instant::now()))
            {
                Ok(Notice::Status(_)) => {}
                Ok(other) => panic!("a file became ready that should not have: {other:?}"),
                Err(RecvTimeoutError::Timeout) => return,
                Err(e) => panic!("{e:?}"),
            }
        }
    }

    fn wait_folder(&self, ok: impl Fn(&DirState) -> bool) {
        self.wait_status(|s| s.folders.first().is_some_and(|f| ok(&f.state)));
    }

    fn wait_status(&self, ok: impl Fn(&WatchStatus) -> bool) {
        let end = Instant::now() + READY_WITHIN;
        while Instant::now() < end {
            if ok(&self.handle.status()) {
                return;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        panic!("status never matched: {:?}", self.handle.status());
    }

    fn options(&self, settings: &Settings) -> RunOptions {
        RunOptions::new(
            settings,
            testutil::gifsicle(),
            self.dir("work"),
            test_trash,
            CancelToken::new(),
        )
    }

    fn backups(&self) -> Backups {
        Backups::new(self.dir("backups"))
    }

    /// Run Ferry's own job over `path`, the way the app does for a ready file.
    fn optimise(&self, job: &mut Job, settings: &Settings) -> Finished {
        job.run(
            Level::Normal,
            &self.options(settings),
            &self.backups(),
            &mut |_| {},
            &mut |_, _, _| Ok(()),
        )
        .expect("the job ran")
    }
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

/// Ready, then optimised in place by Ferry: the common start of the no-loop tests.
fn optimised_in_place(h: &Harness, settings: &Settings) -> (Job, PathBuf, u64) {
    let path = h.watched("a.png");
    let original = testutil::png(600, 400);
    fs::write(&path, &original).unwrap();
    h.expect_ready();
    let mut job = Job::file(&path, settings, testutil::gifsicle().is_some()).unwrap();
    let done = h.optimise(&mut job, settings);
    assert_eq!(done.update.state, JobState::Done);
    let size = fs::metadata(&path).unwrap().len();
    assert!(
        size < original.len() as u64,
        "a real write happened ({size} < {})",
        original.len()
    );
    (job, path, size)
}

#[test]
fn a_png_copied_into_a_watched_folder_is_ready_once() {
    let h = Harness::new();
    fs::write(h.watched("a.png"), testutil::png(300, 200)).unwrap();
    let ready = h.expect_ready();
    assert!(ready.path.ends_with("a.png"));
    assert_eq!(ready.media, Media::Image);
    h.expect_quiet();
}

#[test]
fn a_file_written_in_chunks_is_ready_only_after_the_last_chunk() {
    use std::io::Write;
    let h = Harness::new();
    let png = testutil::png(300, 200);
    let chunk = png.len().div_ceil(6);
    let path = h.watched("slow.png");
    crate::wdiag!("TEST create {:?} total={}", path, png.len());
    let mut file = fs::File::create(&path).unwrap();
    for part in png.chunks(chunk) {
        file.write_all(part).unwrap();
        file.flush().unwrap();
        crate::wdiag!("TEST wrote {}", part.len());
        std::thread::sleep(Duration::from_millis(150));
    }
    let finished = Instant::now();
    drop(file);
    crate::wdiag!("TEST closed");
    let ready = h.expect_ready();
    assert!(
        finished.elapsed() >= Duration::from_millis(200),
        "not before the last chunk"
    );
    assert_eq!(fs::metadata(&ready.path).unwrap().len(), png.len() as u64);
}

#[test]
fn a_hidden_temp_file_renamed_into_place_is_ready_under_its_final_name() {
    let h = Harness::new();
    let temp = h.watched(".Screenshot x.png-Ab12");
    fs::write(&temp, testutil::png(300, 200)).unwrap();
    let last = h.watched("Screenshot x.png");
    fs::rename(&temp, &last).unwrap();
    let ready = h.expect_ready();
    assert_eq!(ready.path, last);
    h.expect_quiet();
}

#[test]
fn ferry_overwriting_a_file_in_place_is_not_ready_again() {
    let h = Harness::new();
    let settings = Settings::default();
    let (mut job, path, _) = optimised_in_place(&h, &settings);
    h.expect_quiet();
    let restored = job.restore(&h.options(&settings), &mut |_, _, _| Ok(()));
    assert_eq!(restored.update.state, JobState::Restored);
    assert!(path.exists());
    h.expect_quiet();
}

#[test]
fn ferrys_outputs_beside_the_source_and_conversions_are_not_ready() {
    let h = Harness::new();
    let mut settings = Settings::default();
    settings.images.output.optimised.location = Location::SameFolder;
    let path = h.watched("a.png");
    fs::write(&path, testutil::png(600, 400)).unwrap();
    h.expect_ready();
    let mut job = Job::file(&path, &settings, testutil::gifsicle().is_some()).unwrap();
    h.optimise(&mut job, &settings);
    assert!(h.watched("a_optimised.png").exists());
    h.expect_quiet();

    let h = Harness::new();
    let settings = Settings::default();
    let path = h.watched("b.png");
    fs::write(&path, testutil::png(600, 400)).unwrap();
    h.expect_ready();
    let mut job = Job::file(&path, &settings, testutil::gifsicle().is_some()).unwrap();
    job.edit(EditsPatch {
        format: Some(Some(Format::WebP)),
        ..EditsPatch::default()
    });
    h.optimise(&mut job, &settings);
    assert!(h.watched("b.webp").exists());
    h.expect_quiet();
}

#[test]
fn a_card_rename_is_not_ready() {
    let h = Harness::new();
    let (mut job, _, _) = optimised_in_place(&h, &Settings::default());
    h.expect_quiet();
    job.rename("renamed").unwrap();
    assert!(h.watched("renamed.png").exists());
    h.expect_quiet();
}

#[test]
fn a_touched_or_tagged_ferry_output_is_not_ready() {
    let h = Harness::new();
    let (_, path, _) = optimised_in_place(&h, &Settings::default());
    h.expect_quiet();
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    file.set_modified(std::time::SystemTime::now() + Duration::from_secs(60))
        .unwrap();
    #[cfg(target_os = "macos")]
    testutil::set_xattr(&path, c"com.apple.metadata:_kMDItemUserTags", b"tag");
    h.expect_quiet();
}

#[test]
fn another_programs_save_over_a_ferry_output_is_ready() {
    let h = Harness::new();
    let (_, path, _) = optimised_in_place(&h, &Settings::default());
    h.expect_quiet();
    let elsewhere = h.dir("a.png.new");
    fs::write(&elsewhere, testutil::png(310, 200)).unwrap();
    fs::rename(&elsewhere, &path).unwrap();
    assert_eq!(h.expect_ready().path, path);
}

#[test]
fn a_missing_folder_starts_watching_when_it_appears() {
    let h = Harness::with(false, |_| {});
    h.wait_folder(|s| *s == DirState::Missing);
    fs::create_dir(h.dir("watched")).unwrap();
    h.handle.rescan();
    h.wait_folder(|s| *s == DirState::Watching);
    fs::write(h.watched("a.png"), testutil::png(300, 200)).unwrap();
    assert!(h.expect_ready().path.ends_with("a.png"));
}

#[test]
fn a_folder_inside_ferrys_backups_is_refused() {
    let h = Harness::new();
    fs::create_dir(h.dir("backups").join("x")).unwrap();
    let mut watch = h.watch_with(|_| {});
    watch.folders[0].path = h.dir("backups").join("x").to_string_lossy().into_owned();
    h.handle.reload(watch);
    h.wait_folder(|s| matches!(s, DirState::Failed { .. }));
    fs::write(
        h.dir("backups").join("x").join("a.png"),
        testutil::png(300, 200),
    )
    .unwrap();
    h.expect_quiet();
}

#[test]
fn reload_starts_and_stops_watching() {
    let h = Harness::new();
    h.handle.reload(h.watch_with(|f| f.paused = true));
    h.wait_folder(|s| *s == DirState::Paused);
    fs::write(h.watched("a.png"), testutil::png(300, 200)).unwrap();
    h.expect_quiet();
    h.handle.reload(h.watch_with(|_| {}));
    h.wait_folder(|s| *s == DirState::Watching);
    fs::write(h.watched("b.png"), testutil::png(310, 200)).unwrap();
    assert!(h.expect_ready().path.ends_with("b.png"));
}
