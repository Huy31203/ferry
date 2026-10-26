//! The watcher's brain, with no threads and no file system of its own: events
//! and a clock go in, "this file is ready" notices come out. A file is handed
//! over once: after it settled, if it is not Ferry's own write, if it has not
//! been seen with the same size and time, and if a capture folder or a watched
//! folder wants it.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde::Serialize;

use super::capture::{CaptureKind, CaptureRule, SCREEN_CAPTURE_XATTR, capture_kind, wanted};
use super::events::FsEvent;
use super::filter::{glob_matches, is_ignored_name};
use super::media::{Media, classify};
use super::plan::{Plan, Role, WatchStatus};
use super::probe;
use super::settle::{Polled, Probe, SettleConfig, Settler};
use crate::selfwrite::{self, Stamp};

/// Folder files are held this long from the first ready file, to count a burst.
pub const BURST_WINDOW: Duration = Duration::from_secs(2);
/// A file that appears this soon after its `.name.icloud` stub is a re-download.
pub const CLOUD_WINDOW: Duration = Duration::from_secs(10);
pub const SEEN_TTL: Duration = Duration::from_secs(86_400);
pub const SEEN_CAP: usize = 4096;

/// What the engine asks of the world.
pub trait Fs {
    fn probe(&self, path: &Path) -> Probe;
    fn head(&self, path: &Path) -> Option<Vec<u8>>;
    fn is_marked(&self, path: &Path) -> bool;
    fn is_own(&self, path: &Path, stamp: &Stamp) -> bool;
}

pub struct RealFs;

impl Fs for RealFs {
    fn probe(&self, path: &Path) -> Probe {
        probe::probe(path)
    }
    fn head(&self, path: &Path) -> Option<Vec<u8>> {
        probe::head(path, 64)
    }
    fn is_marked(&self, path: &Path) -> bool {
        crate::fsutil::has_xattr(path, SCREEN_CAPTURE_XATTR)
    }
    fn is_own(&self, path: &Path, stamp: &Stamp) -> bool {
        selfwrite::is_own(path, stamp)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Origin {
    Capture { capture: CaptureKind },
    Folder { index: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ready {
    pub path: PathBuf,
    pub media: Media,
    pub origin: Origin,
    pub show_results: bool,
    /// The watched folder's pipeline at the time the plan was made.
    pub pipeline: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Notice {
    Ready(Ready),
    BurstSkipped {
        folder: String,
        count: usize,
        show_results: bool,
    },
    Status(WatchStatus),
}

pub struct Engine {
    plan: Plan,
    settler: Settler,
    /// Settled files already handled, by key: a path with the same stamp is
    /// never handled twice.
    seen: HashMap<PathBuf, (Stamp, Instant)>,
    /// `(parent key, name, when)` for each `.name.icloud` stub event.
    cloud: Vec<(PathBuf, String, Instant)>,
    /// Per folder index: when the first file was ready, and the files.
    bursts: BTreeMap<usize, (Instant, Vec<Ready>)>,
}

impl Engine {
    pub fn new(plan: Plan, cfg: SettleConfig) -> Self {
        Self {
            plan,
            settler: Settler::new(cfg),
            seen: HashMap::new(),
            cloud: Vec::new(),
            bursts: BTreeMap::new(),
        }
    }

    pub fn plan(&self) -> &Plan {
        &self.plan
    }

    /// Switch to a new plan. Waiting files whose folder is gone or paused are
    /// dropped, as are ready files held back for such a folder.
    pub fn set_plan(&mut self, plan: Plan) {
        let old = std::mem::replace(&mut self.plan, plan);
        let plan = &self.plan;
        self.settler.retain(|file| !plan.roles_for(file).is_empty());
        self.bursts.retain(|&index, _| {
            let now = plan.watch.folders.get(index);
            let before = old.watch.folders.get(index);
            matches!((now, before), (Some(now), Some(before))
                if !now.paused && now.path == before.path)
        });
    }

    pub fn on_event(&mut self, ev: FsEvent, now: Instant) {
        let (FsEvent::Arrived(path) | FsEvent::Changed(path) | FsEvent::Gone(path)) = &ev;
        let Some(name) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
            return;
        };
        if let Some(inner) = name
            .strip_prefix('.')
            .and_then(|n| n.strip_suffix(".icloud"))
        {
            let parent = path.parent().map(selfwrite::key).unwrap_or_default();
            self.cloud
                .retain(|(.., at)| now.duration_since(*at) < CLOUD_WINDOW);
            self.cloud.push((parent, inner.to_owned(), now));
            return;
        }
        if is_ignored_name(&name) || self.plan.roles_for(path).is_empty() {
            crate::wdiag!("event {:?} DROPPED ignored={} roles_empty={}", ev, is_ignored_name(&name), self.plan.roles_for(path).is_empty());
            return;
        }
        crate::wdiag!("event {:?} pending_before={}", ev, self.settler.is_pending(path));
        match ev {
            FsEvent::Arrived(path) => {
                let parent = path.parent().map(selfwrite::key).unwrap_or_default();
                let redownload = self.cloud.iter().any(|(p, n, at)| {
                    *p == parent && *n == name && now.duration_since(*at) < CLOUD_WINDOW
                });
                if !redownload {
                    self.settler.start(path, now);
                }
            }
            FsEvent::Changed(path) => self.settler.extend(&path, now),
            FsEvent::Gone(_) => {}
        }
    }

    pub fn tick(&mut self, now: Instant, fs: &dyn Fs) -> Vec<Notice> {
        let mut notices = Vec::new();
        for polled in self.settler.poll(now, &mut |p| fs.probe(p)) {
            match polled {
                Polled::Denied { path } => {
                    if self.plan.mark_denied(&path) {
                        notices.push(Notice::Status(self.plan.status.clone()));
                    }
                }
                Polled::Settled { path, stamp } => {
                    if let Some(ready) = self.settled(&path, stamp, now, fs) {
                        notices.extend(ready);
                    }
                }
            }
        }
        self.flush_bursts(now, &mut notices);
        notices
    }

    /// Notices a settled file produces right away (captures); a folder file
    /// joins its burst and produces none yet.
    fn settled(
        &mut self,
        path: &Path,
        stamp: Stamp,
        now: Instant,
        fs: &dyn Fs,
    ) -> Option<Vec<Notice>> {
        crate::wdiag!("settled {:?} {:?}", path, stamp);
        if fs.is_own(path, &stamp) {
            crate::wdiag!("settled {:?} -> is_own", path);
            return None;
        }
        let key = selfwrite::key(path);
        if self
            .seen
            .get(&key)
            .is_some_and(|(seen, at)| *seen == stamp && now.duration_since(*at) < SEEN_TTL)
        {
            crate::wdiag!("settled {:?} -> seen", path);
            return None;
        }
        self.remember(key, stamp, now);
        let name = path.file_name()?.to_string_lossy().into_owned();
        let head = fs.head(path);
        crate::wdiag!("settled {:?} head={:?}", path, head.as_ref().map(|h| h.iter().take(16).copied().collect::<Vec<u8>>()));
        let media = classify(&name, &head?);
        crate::wdiag!("settled {:?} media={:?} roles={}", path, media, self.plan.roles_for(path).len());
        let media = media?;
        let roles: Vec<Role> = self
            .plan
            .roles_for(path)
            .iter()
            .map(|d| d.role.clone())
            .collect();
        for role in roles {
            match role {
                Role::Capture { rule, .. } => {
                    let marked = matches!(rule, CaptureRule::Marked { .. }) && fs.is_marked(path);
                    let captures = &self.plan.watch.captures;
                    let Some(kind) = capture_kind(&rule, &name, media, marked)
                        .filter(|&kind| wanted(kind, captures))
                    else {
                        continue;
                    };
                    return Some(vec![Notice::Ready(Ready {
                        path: path.to_owned(),
                        media,
                        origin: Origin::Capture { capture: kind },
                        show_results: captures.show_results,
                        pipeline: None,
                    })]);
                }
                Role::Folder { index } => {
                    let Some(folder) = self.plan.watch.folders.get(index) else {
                        continue;
                    };
                    if folder.paused
                        || !media.allowed_by(&folder.types)
                        || folder.ignore.iter().any(|g| glob_matches(g, &name))
                    {
                        continue;
                    }
                    let ready = Ready {
                        path: path.to_owned(),
                        media,
                        origin: Origin::Folder { index },
                        show_results: folder.show_results,
                        pipeline: folder.pipeline,
                    };
                    crate::wdiag!("settled {:?} -> burst {}", path, index);
                    self.bursts
                        .entry(index)
                        .or_insert_with(|| (now, Vec::new()))
                        .1
                        .push(ready);
                    return None;
                }
            }
        }
        None
    }

    fn remember(&mut self, key: PathBuf, stamp: Stamp, now: Instant) {
        if self.seen.len() >= SEEN_CAP && !self.seen.contains_key(&key) {
            self.seen
                .retain(|_, (_, at)| now.duration_since(*at) < SEEN_TTL);
            if self.seen.len() >= SEEN_CAP {
                let oldest = self
                    .seen
                    .iter()
                    .min_by_key(|(_, (_, at))| *at)
                    .map(|(k, _)| k.clone());
                if let Some(oldest) = oldest {
                    self.seen.remove(&oldest);
                }
            }
        }
        self.seen.insert(key, (stamp, now));
    }

    fn flush_bursts(&mut self, now: Instant, notices: &mut Vec<Notice>) {
        let due: Vec<usize> = self
            .bursts
            .iter()
            .filter(|(_, (first, _))| now.duration_since(*first) >= BURST_WINDOW)
            .map(|(&index, _)| index)
            .collect();
        for index in due {
            let Some((_, files)) = self.bursts.remove(&index) else {
                continue;
            };
            let Some(folder) = self.plan.watch.folders.get(index) else {
                continue;
            };
            if files.len() > folder.max_files_at_once as usize {
                let name = Path::new(&folder.path)
                    .file_name()
                    .map_or_else(|| folder.path.clone(), |n| n.to_string_lossy().into_owned());
                notices.push(Notice::BurstSkipped {
                    folder: name,
                    count: files.len(),
                    show_results: folder.show_results,
                });
            } else {
                crate::wdiag!("flush {} files", files.len());
                notices.extend(files.into_iter().map(Notice::Ready));
            }
        }
    }

    pub fn next_wake(&self, now: Instant) -> Option<Duration> {
        let bursts = self
            .bursts
            .values()
            .map(|(first, _)| (*first + BURST_WINDOW).saturating_duration_since(now));
        self.settler.next_wake(now).into_iter().chain(bursts).min()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::selfwrite::Stamp;
    use crate::settings::{Watch, WatchedFolder};
    use crate::watch::capture::{CaptureDir, RECORDING_PREFIX};
    use crate::watch::plan::{self, DirState, Resolve};
    use std::cell::RefCell;
    use std::time::UNIX_EPOCH;

    const HOME: &str = "/Users/me";

    #[derive(Clone)]
    struct FakeFile {
        probe: Probe,
        head: Vec<u8>,
        marked: bool,
        own: bool,
    }

    #[derive(Default)]
    struct FakeFs {
        files: RefCell<HashMap<PathBuf, FakeFile>>,
    }

    impl FakeFs {
        fn put(&self, path: &str, len: u64, head: Vec<u8>) {
            self.files.borrow_mut().insert(
                PathBuf::from(path),
                FakeFile {
                    probe: Probe::File(stamp(len)),
                    head,
                    marked: false,
                    own: false,
                },
            );
        }
        fn edit(&self, path: &str, f: impl FnOnce(&mut FakeFile)) {
            f(self.files.borrow_mut().get_mut(Path::new(path)).unwrap());
        }
    }

    impl Fs for FakeFs {
        fn probe(&self, path: &Path) -> Probe {
            self.files
                .borrow()
                .get(path)
                .map_or(Probe::Gone, |f| f.probe)
        }
        fn head(&self, path: &Path) -> Option<Vec<u8>> {
            self.files.borrow().get(path).map(|f| f.head.clone())
        }
        fn is_marked(&self, path: &Path) -> bool {
            self.files.borrow().get(path).is_some_and(|f| f.marked)
        }
        fn is_own(&self, path: &Path, _: &Stamp) -> bool {
            self.files.borrow().get(path).is_some_and(|f| f.own)
        }
    }

    fn stamp(len: u64) -> Stamp {
        Stamp {
            len,
            modified: UNIX_EPOCH + Duration::from_secs(len),
        }
    }

    fn png_head() -> Vec<u8> {
        b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec()
    }

    fn mov_head() -> Vec<u8> {
        b"\0\0\0\x14ftypqt  ".to_vec()
    }

    fn folder(path: &str) -> WatchedFolder {
        WatchedFolder {
            path: path.into(),
            ..WatchedFolder::default()
        }
    }

    fn captures_on() -> Watch {
        let mut watch = Watch::default();
        watch.captures.screenshots = true;
        watch.captures.recordings = true;
        watch
    }

    struct Rig {
        t0: Instant,
        /// The last tick, in ms.
        clock: Option<u64>,
        engine: Engine,
        fs: FakeFs,
    }

    impl Rig {
        /// Every folder under `/Users/me` exists.
        fn new(watch: Watch, capture_dirs: Vec<CaptureDir>) -> Self {
            let plan = Self::plan(&watch, &capture_dirs);
            Self {
                t0: Instant::now(),
                clock: None,
                engine: Engine::new(plan, SettleConfig::default()),
                fs: FakeFs::default(),
            }
        }
        fn plan(watch: &Watch, capture_dirs: &[CaptureDir]) -> Plan {
            plan::build(
                watch,
                capture_dirs,
                Path::new(HOME),
                &[PathBuf::from("/Users/me/.ferry")],
                &mut |d| {
                    if d.starts_with("/Users/me") || d.starts_with("/home/me") {
                        Resolve::Dir(d.to_owned())
                    } else {
                        Resolve::Missing
                    }
                },
            )
        }
        fn at(&self, ms: u64) -> Instant {
            self.t0 + Duration::from_millis(ms)
        }
        fn arrive(&mut self, path: &str, ms: u64) {
            let at = self.at(ms);
            self.engine
                .on_event(FsEvent::Arrived(PathBuf::from(path)), at);
        }
        /// Ticks every 500 ms after the last tick, up to `to`; what came out.
        fn run_to(&mut self, to: u64) -> Vec<Notice> {
            let first = self.clock.map_or(0, |c| c + 500);
            let mut out = Vec::new();
            for ms in (first..=to).step_by(500) {
                let at = self.at(ms);
                out.extend(self.engine.tick(at, &self.fs));
                self.clock = Some(ms);
            }
            out
        }
        fn ready(&self, path: &str, media: Media, origin: Origin, show: bool) -> Notice {
            Notice::Ready(Ready {
                path: PathBuf::from(path),
                media,
                origin,
                show_results: show,
                pipeline: None,
            })
        }
    }

    fn mac_capture() -> Vec<CaptureDir> {
        vec![CaptureDir {
            dir: PathBuf::from("/Users/me/Desktop"),
            rule: CaptureRule::Marked {
                recording_prefixes: vec![RECORDING_PREFIX.into()],
            },
        }]
    }

    fn in_folder() -> Watch {
        Watch {
            folders: vec![folder("/Users/me/In")],
            ..Watch::default()
        }
    }

    #[test]
    fn a_new_image_in_a_watched_folder_is_ready_after_the_burst_window() {
        let mut r = Rig::new(in_folder(), vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        assert!(r.run_to(2000).is_empty(), "held for the burst window");
        assert_eq!(
            r.run_to(4000),
            [r.ready(
                "/Users/me/In/a.png",
                Media::Image,
                Origin::Folder { index: 0 },
                true
            )]
        );
        assert!(r.run_to(10_000).is_empty());
    }

    #[test]
    fn ferrys_own_write_is_never_ready() {
        let mut r = Rig::new(in_folder(), vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.fs.edit("/Users/me/In/a.png", |f| f.own = true);
        r.arrive("/Users/me/In/a.png", 0);
        assert!(r.run_to(10_000).is_empty());
    }

    #[test]
    fn a_file_seen_with_the_same_stamp_is_not_ready_twice() {
        let mut r = Rig::new(in_folder(), vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        assert_eq!(r.run_to(5000).len(), 1);
        r.arrive("/Users/me/In/a.png", 6000);
        assert!(r.run_to(20_000).is_empty());
    }

    #[test]
    fn a_file_that_arrives_again_with_new_content_is_ready_again() {
        let mut r = Rig::new(in_folder(), vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        assert_eq!(r.run_to(5000).len(), 1);
        r.fs.put("/Users/me/In/a.png", 20, png_head());
        r.arrive("/Users/me/In/a.png", 6000);
        assert_eq!(r.run_to(15_000).len(), 1);
    }

    #[test]
    fn hidden_temp_and_ferry_tmp_names_are_ignored() {
        let mut r = Rig::new(in_folder(), vec![]);
        for name in [".hidden.png", "a.png.ferry-tmp", "b.crdownload", "c.tmp"] {
            r.fs.put(&format!("/Users/me/In/{name}"), 10, png_head());
            r.arrive(&format!("/Users/me/In/{name}"), 0);
        }
        assert!(r.run_to(10_000).is_empty());
        assert_eq!(r.engine.settler.len(), 0);
    }

    #[test]
    fn files_in_subfolders_and_excluded_roots_are_ignored() {
        let watch = Watch {
            folders: vec![folder("/Users/me/In"), folder("/Users/me/.ferry")],
            ..Watch::default()
        };
        let mut r = Rig::new(watch, vec![]);
        for path in ["/Users/me/In/sub/a.png", "/Users/me/.ferry/b.png"] {
            r.fs.put(path, 10, png_head());
            r.arrive(path, 0);
        }
        assert!(r.run_to(10_000).is_empty());
    }

    #[test]
    fn a_ready_file_carries_its_folders_pipeline() {
        let mut watch = in_folder();
        watch.folders[0].pipeline = Some(4);
        let mut r = Rig::new(watch, vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        let out = r.run_to(4000);
        assert!(matches!(&out[..], [Notice::Ready(ready)] if ready.pipeline == Some(4)));
    }

    #[test]
    fn more_than_max_files_at_once_are_all_skipped() {
        let mut watch = in_folder();
        watch.folders[0].max_files_at_once = 3;
        let mut r = Rig::new(watch.clone(), vec![]);
        for i in 0..4 {
            r.fs.put(&format!("/Users/me/In/{i}.png"), 10 + i, png_head());
            r.arrive(&format!("/Users/me/In/{i}.png"), 0);
        }
        assert_eq!(
            r.run_to(6000),
            [Notice::BurstSkipped {
                folder: "In".into(),
                count: 4,
                show_results: true
            }]
        );
        let mut r = Rig::new(watch, vec![]);
        for i in 0..3 {
            r.fs.put(&format!("/Users/me/In/{i}.png"), 10 + i, png_head());
            r.arrive(&format!("/Users/me/In/{i}.png"), 0);
        }
        let out = r.run_to(6000);
        assert_eq!(out.len(), 3);
        assert!(out.iter().all(|n| matches!(n, Notice::Ready(_))));
    }

    #[test]
    fn types_and_ignore_patterns_filter_a_folder() {
        let mut watch = in_folder();
        watch.folders[0].types.video = false;
        watch.folders[0].ignore = vec!["draft-*".into()];
        let mut r = Rig::new(watch, vec![]);
        r.fs.put("/Users/me/In/clip.mov", 10, mov_head());
        r.fs.put("/Users/me/In/draft-1.png", 11, png_head());
        r.fs.put("/Users/me/In/keep.png", 12, png_head());
        for name in ["clip.mov", "draft-1.png", "keep.png"] {
            r.arrive(&format!("/Users/me/In/{name}"), 0);
        }
        assert_eq!(
            r.run_to(6000),
            [r.ready(
                "/Users/me/In/keep.png",
                Media::Image,
                Origin::Folder { index: 0 },
                true
            )]
        );
    }

    #[test]
    fn a_paused_folder_produces_nothing() {
        let mut watch = in_folder();
        watch.folders[0].paused = true;
        let mut r = Rig::new(watch, vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        assert!(r.run_to(10_000).is_empty());
    }

    #[test]
    fn a_macos_screenshot_needs_the_marker_and_falls_through_to_the_folder_otherwise() {
        let mut watch = captures_on();
        watch.folders = vec![folder("/Users/me/Desktop")];
        let mut r = Rig::new(watch, mac_capture());
        r.fs.put("/Users/me/Desktop/shot.png", 10, png_head());
        r.fs.edit("/Users/me/Desktop/shot.png", |f| f.marked = true);
        r.fs.put("/Users/me/Desktop/other.png", 11, png_head());
        r.arrive("/Users/me/Desktop/shot.png", 0);
        r.arrive("/Users/me/Desktop/other.png", 0);
        assert_eq!(
            r.run_to(2000),
            [r.ready(
                "/Users/me/Desktop/shot.png",
                Media::Image,
                Origin::Capture {
                    capture: CaptureKind::Screenshot
                },
                true
            )],
            "no burst wait for a capture"
        );
        assert_eq!(
            r.run_to(4000),
            [r.ready(
                "/Users/me/Desktop/other.png",
                Media::Image,
                Origin::Folder { index: 0 },
                true
            )]
        );
    }

    #[test]
    fn a_recording_is_known_by_marker_or_name() {
        let mut r = Rig::new(captures_on(), mac_capture());
        r.fs.put(
            "/Users/me/Desktop/Screen Recording 2026.mov",
            10,
            mov_head(),
        );
        r.fs.put("/Users/me/Desktop/holiday.mov", 11, mov_head());
        r.fs.put("/Users/me/Desktop/marked.mov", 12, mov_head());
        r.fs.edit("/Users/me/Desktop/marked.mov", |f| f.marked = true);
        for name in ["Screen Recording 2026.mov", "holiday.mov", "marked.mov"] {
            r.arrive(&format!("/Users/me/Desktop/{name}"), 0);
        }
        let paths: Vec<_> = r
            .run_to(4000)
            .into_iter()
            .map(|n| match n {
                Notice::Ready(r) => r.path.file_name().unwrap().to_string_lossy().into_owned(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(paths, ["Screen Recording 2026.mov", "marked.mov"]);
    }

    #[test]
    fn capture_toggles_gate_screenshots_and_recordings() {
        let both = |watch: Watch| {
            let mut r = Rig::new(watch, mac_capture());
            r.fs.put("/Users/me/Desktop/shot.png", 10, png_head());
            r.fs.edit("/Users/me/Desktop/shot.png", |f| f.marked = true);
            r.fs.put("/Users/me/Desktop/rec.mov", 11, mov_head());
            r.fs.edit("/Users/me/Desktop/rec.mov", |f| f.marked = true);
            r.arrive("/Users/me/Desktop/shot.png", 0);
            r.arrive("/Users/me/Desktop/rec.mov", 0);
            r.run_to(6000)
        };
        assert!(both(Watch::default()).is_empty(), "off by default");
        let mut shots = Watch::default();
        shots.captures.screenshots = true;
        let out = both(shots);
        assert!(matches!(&out[..], [Notice::Ready(r)] if r.media == Media::Image));
        let mut recs = Watch::default();
        recs.captures.recordings = true;
        let out = both(recs);
        assert!(matches!(&out[..], [Notice::Ready(r)] if r.media == Media::Video));
    }

    #[test]
    fn captures_have_no_burst_limit() {
        let mut r = Rig::new(captures_on(), mac_capture());
        for i in 0..5u64 {
            let path = format!("/Users/me/Desktop/s{i}.png");
            r.fs.put(&path, 10 + i, png_head());
            r.fs.edit(&path, |f| f.marked = true);
            r.arrive(&path, i * 600);
        }
        let out = r.run_to(10_000);
        assert_eq!(out.len(), 5);
        assert!(out.iter().all(|n| matches!(n, Notice::Ready(_))));
    }

    #[test]
    fn an_icloud_re_download_is_ignored() {
        let go = |arrive_at: u64| {
            let mut r = Rig::new(in_folder(), vec![]);
            r.fs.put("/Users/me/In/a.png", 10, png_head());
            let at = r.at(0);
            r.engine.on_event(
                FsEvent::Gone(PathBuf::from("/Users/me/In/.a.png.icloud")),
                at,
            );
            r.arrive("/Users/me/In/a.png", arrive_at);
            r.run_to(arrive_at + 6000)
        };
        assert!(go(500).is_empty());
        assert_eq!(go(11_000).len(), 1);
    }

    #[test]
    fn a_denied_file_marks_its_folder_denied() {
        let mut r = Rig::new(in_folder(), vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.fs.edit("/Users/me/In/a.png", |f| f.probe = Probe::Denied);
        r.arrive("/Users/me/In/a.png", 0);
        let out = r.run_to(2000);
        let [Notice::Status(status)] = &out[..] else {
            panic!("{out:?}");
        };
        assert_eq!(status.folders[0].state, DirState::Denied);
    }

    #[test]
    fn windows_capture_folders_take_every_image_and_video() {
        let dirs = vec![CaptureDir {
            dir: PathBuf::from("/Users/me/Shots"),
            rule: CaptureRule::Everything,
        }];
        let mut r = Rig::new(captures_on(), dirs);
        r.fs.put("/Users/me/Shots/a.png", 10, png_head());
        r.fs.put("/Users/me/Shots/b.mov", 11, mov_head());
        r.arrive("/Users/me/Shots/a.png", 0);
        r.arrive("/Users/me/Shots/b.mov", 0);
        assert_eq!(r.run_to(4000).len(), 2);
    }

    #[test]
    fn a_screenshot_name_with_narrow_nbsp_is_ready() {
        let path = "/Users/me/Desktop/Screenshot 2026-10-23 at 10.21.33\u{202F}AM.png";
        let mut r = Rig::new(captures_on(), mac_capture());
        r.fs.put(path, 10, png_head());
        r.fs.edit(path, |f| f.marked = true);
        r.arrive(path, 0);
        assert_eq!(
            r.run_to(3000),
            [r.ready(
                path,
                Media::Image,
                Origin::Capture {
                    capture: CaptureKind::Screenshot
                },
                true
            )]
        );
    }

    #[test]
    fn a_folder_listed_twice_reports_a_file_once() {
        let watch = Watch {
            folders: vec![folder("~/In"), folder("/Users/me/In")],
            ..Watch::default()
        };
        let mut r = Rig::new(watch, vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        let out = r.run_to(6000);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(r.engine.plan().watch_dirs().len(), 1);
    }

    #[test]
    fn a_file_pending_when_its_folder_is_removed_is_dropped() {
        for change in [
            (|w: &mut Watch| w.folders.clear()) as fn(&mut Watch),
            |w: &mut Watch| w.folders[0].paused = true,
        ] {
            let mut r = Rig::new(in_folder(), vec![]);
            r.fs.put("/Users/me/In/a.png", 10, png_head());
            r.arrive("/Users/me/In/a.png", 0);
            let mut watch = in_folder();
            change(&mut watch);
            let plan = Rig::plan(&watch, &[]);
            r.engine.set_plan(plan);
            assert!(r.run_to(6000).is_empty());
        }
    }

    #[test]
    fn a_ready_file_held_for_a_removed_folder_is_dropped() {
        let mut r = Rig::new(in_folder(), vec![]);
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        assert!(r.run_to(2000).is_empty());
        let mut watch = in_folder();
        watch.folders[0].paused = true;
        r.engine.set_plan(Rig::plan(&watch, &[]));
        assert!(r.run_to(8000).is_empty());
    }

    #[test]
    fn next_wake_covers_settling_and_bursts() {
        let mut r = Rig::new(in_folder(), vec![]);
        assert!(r.engine.next_wake(r.at(0)).is_none());
        r.fs.put("/Users/me/In/a.png", 10, png_head());
        r.arrive("/Users/me/In/a.png", 0);
        assert_eq!(
            r.engine.next_wake(r.at(200)),
            Some(Duration::from_millis(800))
        );
        r.run_to(2000);
        assert_eq!(
            r.engine.next_wake(r.at(2500)),
            Some(Duration::from_millis(1500))
        );
    }
}
