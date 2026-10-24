//! What a drop holds and where it goes: classify files by content, walk folders
//! safely, summarise a drop for the zone, and route it to cards or the batch.

use std::collections::HashSet;
use std::ffi::OsStr;
use std::fs::{self, File};
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::{Location, Output};
use crate::sniff::{self, Format};

/// Bytes read from a file to decide what it is.
pub const HEAD_LEN: usize = 64;
/// A folder scan stops here.
pub const MAX_FILES: usize = 10_000;
pub const MAX_DEPTH: usize = 64;
/// Folders that are one document to the user. Never opened, never optimised.
pub const PACKAGE_EXTENSIONS: &[&str] = &[
    "app",
    "bundle",
    "framework",
    "plugin",
    "kext",
    "photoslibrary",
    "photolibrary",
    "aplibrary",
    "migratedphotolibrary",
    "imovielibrary",
    "fcpbundle",
    "logicx",
    "band",
    "lrdata",
    "pages",
    "numbers",
    "key",
    "xcodeproj",
    "xcworkspace",
];
/// Windows system folders, matched without case.
pub const SYSTEM_FOLDERS: &[&str] = &["$RECYCLE.BIN", "System Volume Information"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Pdf,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classified {
    pub kind: MediaKind,
    pub format: Option<Format>,
}

/// What `head` is, by content. The one place to extend for new media.
pub fn classify(head: &[u8]) -> Option<Classified> {
    let format = sniff::sniff(head)?;
    let kind = if format.is_video() {
        MediaKind::Video
    } else {
        MediaKind::Image
    };
    Some(Classified {
        kind,
        format: Some(format),
    })
}

/// `classify` on the first `HEAD_LEN` bytes of the file.
pub fn classify_file(path: &Path) -> io::Result<Option<Classified>> {
    let mut head = Vec::with_capacity(HEAD_LEN);
    File::open(path)?
        .take(HEAD_LEN as u64)
        .read_to_end(&mut head)?;
    Ok(classify(&head))
}

#[derive(Debug, Clone, PartialEq)]
pub struct Candidate {
    pub path: PathBuf,
    pub len: u64,
    pub kind: MediaKind,
    pub format: Option<Format>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scan {
    pub files: Vec<Candidate>,
    /// Top-level paths that can't be optimised.
    pub unsupported: Vec<PathBuf>,
    /// Unsupported files found inside folders.
    pub skipped: u32,
    pub unreadable: u32,
    pub truncated: bool,
}

#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub max_files: usize,
    /// Folders (Ferry's own backups and working folders) the scan never enters.
    pub exclude: Vec<PathBuf>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            max_files: MAX_FILES,
            exclude: Vec::new(),
        }
    }
}

fn lower_ext(name: &OsStr) -> Option<String> {
    Path::new(name)
        .extension()
        .map(|e| e.to_string_lossy().to_lowercase())
}

/// Hidden, a package, or a system folder.
pub fn is_skipped_dir(name: &OsStr) -> bool {
    let text = name.to_string_lossy();
    text.starts_with('.')
        || lower_ext(name).is_some_and(|e| PACKAGE_EXTENSIONS.contains(&e.as_str()))
        || SYSTEM_FOLDERS.iter().any(|s| s.eq_ignore_ascii_case(&text))
}

fn is_junk_file(name: &OsStr) -> bool {
    let text = name.to_string_lossy();
    text.starts_with('.')
        || text.eq_ignore_ascii_case("Thumbs.db")
        || text.eq_ignore_ascii_case("desktop.ini")
}

struct Walk {
    scan: Scan,
    seen: HashSet<PathBuf>,
    exclude: Vec<PathBuf>,
    max_files: usize,
}

impl Walk {
    fn excluded(&self, canonical: &Path) -> bool {
        self.exclude.iter().any(|e| canonical.starts_with(e))
    }

    /// Add one file. `Err(())` when the scan is full.
    fn add(&mut self, path: &Path, top: bool) -> Result<(), ()> {
        let Ok(canonical) = fs::canonicalize(path) else {
            self.unreadable(path, top);
            return Ok(());
        };
        if self.excluded(&canonical) || self.seen.contains(&canonical) {
            return Ok(());
        }
        let found = classify_file(&canonical)
            .ok()
            .zip(fs::metadata(&canonical).ok());
        match found {
            Some((Some(c), meta)) => {
                if self.scan.files.len() >= self.max_files {
                    self.scan.truncated = true;
                    return Err(());
                }
                self.seen.insert(canonical.clone());
                self.scan.files.push(Candidate {
                    path: canonical,
                    len: meta.len(),
                    kind: c.kind,
                    format: c.format,
                });
            }
            Some((None, _)) if top => self.scan.unsupported.push(path.to_path_buf()),
            Some((None, _)) => self.scan.skipped += 1,
            None => self.unreadable(path, top),
        }
        Ok(())
    }

    fn unreadable(&mut self, path: &Path, top: bool) {
        if top {
            self.scan.unsupported.push(path.to_path_buf());
        } else {
            self.scan.unreadable += 1;
        }
    }

    /// Walk a folder in name order. `Err(())` when the scan is full.
    fn folder(&mut self, root: &Path) -> Result<(), ()> {
        let mut stack = vec![(root.to_path_buf(), 0usize)];
        while let Some((dir, depth)) = stack.pop() {
            let Ok(canonical) = fs::canonicalize(&dir) else {
                self.scan.unreadable += 1;
                continue;
            };
            if self.excluded(&canonical) {
                continue;
            }
            let Ok(read) = fs::read_dir(&dir) else {
                self.scan.unreadable += 1;
                continue;
            };
            let mut entries: Vec<_> = read.filter_map(Result::ok).collect();
            entries.sort_by_key(|e| e.file_name());
            let mut subdirs = Vec::new();
            for entry in entries {
                let name = entry.file_name();
                let path = entry.path();
                let Ok(kind) = entry.file_type() else {
                    self.scan.unreadable += 1;
                    continue;
                };
                // A symlink to a file is followed; one to a folder never is.
                let (is_dir, is_symlink) = if kind.is_symlink() {
                    match fs::metadata(&path) {
                        Ok(m) => (m.is_dir(), true),
                        Err(_) => {
                            self.scan.unreadable += 1;
                            continue;
                        }
                    }
                } else {
                    (kind.is_dir(), false)
                };
                if is_dir {
                    if !is_symlink && !is_skipped_dir(&name) && depth < MAX_DEPTH {
                        subdirs.push((path, depth + 1));
                    }
                } else if !is_junk_file(&name) {
                    self.add(&path, false)?;
                }
            }
            stack.extend(subdirs.into_iter().rev());
        }
        Ok(())
    }
}

/// Classify the dropped paths and walk their folders. Reads only file heads.
pub fn scan(paths: &[PathBuf], o: &ScanOptions) -> Scan {
    let mut walk = Walk {
        scan: Scan::default(),
        seen: HashSet::new(),
        exclude: o
            .exclude
            .iter()
            .map(|e| fs::canonicalize(e).unwrap_or_else(|_| e.clone()))
            .collect(),
        max_files: o.max_files,
    };
    for path in paths {
        let name = path.file_name().unwrap_or_default();
        let result = match fs::metadata(path) {
            Ok(m) if m.is_dir() && is_skipped_dir(name) => {
                walk.scan.unsupported.push(path.clone());
                Ok(())
            }
            Ok(m) if m.is_dir() => walk.folder(path),
            Ok(_) => walk.add(path, true),
            Err(_) => {
                walk.scan.unsupported.push(path.clone());
                Ok(())
            }
        };
        if result.is_err() {
            break;
        }
    }
    walk.scan
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub files: u32,
    pub folders: u32,
    pub images: u32,
    pub videos: u32,
    pub audio: u32,
    pub pdfs: u32,
    pub unsupported: u32,
    pub unsupported_ext: Option<String>,
}

impl Summary {
    pub fn supported(&self) -> u32 {
        self.images + self.videos + self.audio + self.pdfs
    }
}

/// A cheap look at the top level of a drop, for the zone while it is hovered.
/// Reads at most `head_limit` file heads and never recurses.
pub fn summarise(paths: &[PathBuf], head_limit: usize) -> Summary {
    let mut s = Summary::default();
    let mut read = 0;
    let mut first_unsupported = true;
    for path in paths {
        let name = path.file_name().unwrap_or_default();
        let is_dir = fs::metadata(path).is_ok_and(|m| m.is_dir());
        if is_dir && !is_skipped_dir(name) {
            s.folders += 1;
            continue;
        }
        s.files += 1;
        // A package is unsupported without a read; files past the limit are not read.
        let class = if is_dir {
            None
        } else if read >= head_limit {
            continue;
        } else {
            read += 1;
            classify_file(path).ok().flatten()
        };
        match class.map(|c| c.kind) {
            Some(MediaKind::Image) => s.images += 1,
            Some(MediaKind::Video) => s.videos += 1,
            Some(MediaKind::Audio) => s.audio += 1,
            Some(MediaKind::Pdf) => s.pdfs += 1,
            None => {
                s.unsupported += 1;
                if first_unsupported {
                    first_unsupported = false;
                    s.unsupported_ext = lower_ext(name);
                }
            }
        }
    }
    s
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route {
    Cards,
    Batch,
}

/// Any folder, or more files than `threshold`, opens the batch window.
pub fn route(s: &Summary, threshold: u32) -> Route {
    if s.folders > 0 || s.files > threshold {
        Route::Batch
    } else {
        Route::Cards
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneState {
    Hidden,
    Idle,
    Hover,
    Unsupported,
    Dropped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ZoneMode {
    Zone,
    Tab,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ZoneView {
    pub state: ZoneState,
    pub mode: ZoneMode,
    pub summary: Option<Summary>,
    pub keep_original: bool,
    pub opens_batch: bool,
}

/// What the zone page shows for a hover over `summary`.
pub fn zone_view(
    summary: Option<&Summary>,
    hovering: bool,
    keep: bool,
    threshold: u32,
    mode: ZoneMode,
) -> ZoneView {
    let opens_batch = summary.is_some_and(|s| route(s, threshold) == Route::Batch);
    // Unsupported only when every file was looked at and none can be optimised.
    let unsupported = summary.is_some_and(|s| {
        s.files > 0 && s.folders == 0 && s.supported() == 0 && s.unsupported == s.files
    });
    let state = if unsupported {
        ZoneState::Unsupported
    } else if hovering {
        ZoneState::Hover
    } else {
        ZoneState::Idle
    };
    ZoneView {
        state,
        mode,
        summary: summary.cloned(),
        keep_original: keep,
        opens_batch,
    }
}

/// The File handling rules with `In place` replaced by `Same folder`, so the
/// source file is never written (hold the keep modifier while dropping).
pub fn keep_source_rules(rules: &Output) -> Output {
    let mut out = rules.clone();
    for rule in [&mut out.optimised, &mut out.converted] {
        if rule.location == Location::InPlace {
            rule.location = Location::SameFolder;
        }
    }
    out
}

/// `paths` without Ferry's own results.
pub fn without_own(paths: &[PathBuf], outputs: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| !outputs.contains(p))
        .cloned()
        .collect()
}

/// Every dropped path is one of Ferry's own results. False for an empty drop.
pub fn all_own(paths: &[PathBuf], outputs: &[PathBuf]) -> bool {
    !paths.is_empty() && paths.iter().all(|p| outputs.contains(p))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{jpeg, png};

    fn put(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let path = dir.join(rel);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, bytes).unwrap();
        path
    }

    fn names(scan: &Scan) -> Vec<String> {
        scan.files
            .iter()
            .map(|c| c.path.file_name().unwrap().to_string_lossy().into_owned())
            .collect()
    }

    fn images(files: u32, images: u32) -> Summary {
        Summary {
            files,
            images,
            ..Summary::default()
        }
    }

    #[test]
    fn content_decides_the_kind_not_the_name() {
        let dir = tempfile::tempdir().unwrap();
        let photo = put(dir.path(), "photo.jpg", &png(8, 8));
        assert_eq!(
            classify_file(&photo).unwrap(),
            Some(Classified {
                kind: MediaKind::Image,
                format: Some(Format::Png)
            })
        );
        assert_eq!(classify(b"PK\x03\x04 and some more bytes"), None);
        assert_eq!(classify(b""), None);
    }

    #[test]
    fn a_folder_is_walked_in_name_order_including_subfolders() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "d/b.png", &png(8, 8));
        put(dir.path(), "d/a.png", &png(8, 8));
        put(
            dir.path(),
            "d/sub/c.jpg",
            &jpeg(8, 8, 80, &Default::default()),
        );
        let scan = scan(&[dir.path().join("d")], &ScanOptions::default());
        assert_eq!(names(&scan), ["a.png", "b.png", "c.jpg"]);
        assert_eq!(scan.skipped, 0);
    }

    #[test]
    fn hidden_files_and_folders_packages_and_system_folders_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let p = png(8, 8);
        for rel in [
            "d/.git/x.png",
            "d/.DS_Store",
            "d/Foo.app/Contents/x.png",
            "d/Photos Library.photoslibrary/x.png",
            "d/$RECYCLE.BIN/x.png",
            "d/Thumbs.db",
            "d/ok.png",
        ] {
            put(dir.path(), rel, &p);
        }
        let scan = scan(&[dir.path().join("d")], &ScanOptions::default());
        assert_eq!(names(&scan), ["ok.png"]);
        assert_eq!(scan.skipped, 0);
        assert!(scan.unsupported.is_empty());
    }

    #[test]
    fn a_dropped_package_is_not_opened() {
        let dir = tempfile::tempdir().unwrap();
        put(
            dir.path(),
            "d/Photos Library.photoslibrary/x.png",
            &png(8, 8),
        );
        let lib = dir.path().join("d/Photos Library.photoslibrary");
        let scan = scan(std::slice::from_ref(&lib), &ScanOptions::default());
        assert!(scan.files.is_empty());
        assert_eq!(scan.unsupported.as_slice(), std::slice::from_ref(&lib));
        assert_eq!(
            summarise(&[lib], 50),
            Summary {
                files: 1,
                unsupported: 1,
                unsupported_ext: Some("photoslibrary".into()),
                ..Summary::default()
            }
        );
    }

    #[test]
    fn unsupported_files_in_folders_are_counted_and_dropped_ones_listed() {
        let dir = tempfile::tempdir().unwrap();
        let notes = put(dir.path(), "d/notes.txt", b"hello");
        let in_folder = scan(&[dir.path().join("d")], &ScanOptions::default());
        assert_eq!(in_folder.skipped, 1);
        assert!(in_folder.unsupported.is_empty());
        let dropped = scan(std::slice::from_ref(&notes), &ScanOptions::default());
        assert_eq!(dropped.unsupported, [notes]);
    }

    #[cfg(unix)]
    #[test]
    fn symlinked_folders_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let d = dir.path().join("d");
        put(dir.path(), "d/a.png", &png(8, 8));
        std::os::unix::fs::symlink(&d, d.join("loop")).unwrap();
        let scan = scan(&[d], &ScanOptions::default());
        assert_eq!(names(&scan), ["a.png"]);
    }

    #[test]
    fn the_same_file_twice_is_one_candidate() {
        let dir = tempfile::tempdir().unwrap();
        let a = put(dir.path(), "d/a.png", &png(8, 8));
        let scan = scan(
            &[a.clone(), dir.path().join("d"), a],
            &ScanOptions::default(),
        );
        assert_eq!(scan.files.len(), 1);
    }

    #[test]
    fn the_scan_stops_at_max_files() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..5 {
            put(dir.path(), &format!("d/{i}.png"), &png(8, 8));
        }
        let o = ScanOptions {
            max_files: 3,
            ..ScanOptions::default()
        };
        let scan = scan(&[dir.path().join("d")], &o);
        assert_eq!(scan.files.len(), 3);
        assert!(scan.truncated);
    }

    #[test]
    fn ferrys_own_folders_are_excluded() {
        let dir = tempfile::tempdir().unwrap();
        put(dir.path(), "d/backups/x.png", &png(8, 8));
        put(dir.path(), "d/y.png", &png(8, 8));
        let o = ScanOptions {
            exclude: vec![dir.path().join("d/backups")],
            ..ScanOptions::default()
        };
        let scan = scan(&[dir.path().join("d")], &o);
        assert_eq!(names(&scan), ["y.png"]);
    }

    #[test]
    fn a_summary_counts_kinds_folders_and_unsupported() {
        let dir = tempfile::tempdir().unwrap();
        let mut paths = vec![
            put(dir.path(), "a.png", &png(8, 8)),
            put(dir.path(), "b.png", &png(8, 8)),
            put(dir.path(), "c.jpg", &jpeg(8, 8, 80, &Default::default())),
        ];
        fs::create_dir(dir.path().join("dir")).unwrap();
        paths.push(dir.path().join("dir"));
        paths.push(put(dir.path(), "z.zip", b"PK\x03\x04"));
        assert_eq!(
            summarise(&paths, 50),
            Summary {
                files: 4,
                folders: 1,
                images: 3,
                unsupported: 1,
                unsupported_ext: Some("zip".into()),
                ..Summary::default()
            }
        );
    }

    #[test]
    fn only_the_first_files_are_read_for_a_summary() {
        let dir = tempfile::tempdir().unwrap();
        let paths: Vec<_> = (0..3)
            .map(|i| put(dir.path(), &format!("{i}.png"), &png(8, 8)))
            .collect();
        let s = summarise(&paths, 2);
        assert_eq!((s.files, s.images), (3, 2));
    }

    #[test]
    fn a_folder_or_more_files_than_the_threshold_opens_the_batch() {
        let folder = Summary {
            folders: 1,
            ..Summary::default()
        };
        assert_eq!(route(&folder, 20), Route::Batch);
        assert_eq!(route(&images(21, 21), 20), Route::Batch);
        assert_eq!(route(&images(20, 20), 20), Route::Cards);
    }

    #[test]
    fn the_zone_says_unsupported_only_when_nothing_can_be_optimised() {
        let view =
            |s: Option<&Summary>, hovering| zone_view(s, hovering, false, 20, ZoneMode::Zone).state;
        let zip = Summary {
            files: 1,
            unsupported: 1,
            ..Summary::default()
        };
        assert_eq!(view(Some(&zip), true), ZoneState::Unsupported);
        let mixed = Summary {
            files: 2,
            images: 1,
            unsupported: 1,
            ..Summary::default()
        };
        assert_eq!(view(Some(&mixed), true), ZoneState::Hover);
        assert_eq!(view(Some(&images(30, 2)), true), ZoneState::Hover);
        let unread = Summary {
            files: 30,
            unsupported: 2,
            ..Summary::default()
        };
        assert_eq!(view(Some(&unread), false), ZoneState::Idle);
        assert_eq!(view(None, false), ZoneState::Idle);
    }

    #[test]
    fn the_zone_shows_the_batch_hint_and_the_keep_line() {
        let many = zone_view(Some(&images(42, 42)), true, true, 20, ZoneMode::Zone);
        assert!(many.opens_batch && many.keep_original);
        let folder = Summary {
            folders: 1,
            ..Summary::default()
        };
        assert!(zone_view(Some(&folder), true, false, 20, ZoneMode::Zone).opens_batch);
    }

    #[test]
    fn keeping_the_original_moves_in_place_results_beside_it() {
        let kept = keep_source_rules(&Output::default());
        assert_eq!(kept.optimised.location, Location::SameFolder);
        assert_eq!(kept.converted.location, Location::SameFolder);
        assert_eq!(kept.optimised.template, "%f_optimised.%e");
        assert_eq!(kept.converted.template, "%f.%e");
        let mut folder = Output::default();
        folder.optimised.location = Location::Folder;
        assert_eq!(
            keep_source_rules(&folder).optimised.location,
            Location::Folder
        );
    }

    #[test]
    fn ferrys_own_results_are_not_taken_as_new_input() {
        let p = |s: &str| PathBuf::from(s);
        let own = [p("/w/a.png")];
        assert_eq!(
            without_own(&[p("/w/a.png"), p("/x/b.png")], &own),
            [p("/x/b.png")]
        );
        assert!(all_own(&[p("/w/a.png")], &own));
        assert!(!all_own(&[], &own));
        assert!(!all_own(&[p("/w/a.png"), p("/x/b.png")], &own));
    }
}
