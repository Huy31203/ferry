//! Stream H: what Ferry registers with Finder / Explorer and the pure parts of
//! sharing and opening files with other apps.

use std::ffi::OsString;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::settings::AppRef;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MediaKind {
    Image,
    Video,
    Audio,
    Pdf,
}

/// The kind of media a file extension (no leading dot, any case) names.
pub fn media_kind(ext: &str) -> Option<MediaKind> {
    match ext.to_ascii_lowercase().as_str() {
        "png" | "jpg" | "jpeg" | "gif" | "webp" | "heic" | "heif" | "tif" | "tiff" | "avif"
        | "bmp" => Some(MediaKind::Image),
        "mp4" | "mov" | "m4v" | "webm" | "mkv" | "avi" => Some(MediaKind::Video),
        "mp3" | "m4a" | "aac" | "wav" | "aif" | "aiff" | "flac" | "ogg" | "opus" => {
            Some(MediaKind::Audio)
        }
        "pdf" => Some(MediaKind::Pdf),
        _ => None,
    }
}

pub struct FileType {
    pub uti: &'static str,
    pub exts: &'static [&'static str],
}

/// The types Ferry registers for Open With / Optimise with Ferry.
pub const FILE_TYPES: [FileType; 12] = [
    FileType {
        uti: "public.png",
        exts: &["png"],
    },
    FileType {
        uti: "public.jpeg",
        exts: &["jpg", "jpeg"],
    },
    FileType {
        uti: "com.compuserve.gif",
        exts: &["gif"],
    },
    FileType {
        uti: "org.webmproject.webp",
        exts: &["webp"],
    },
    FileType {
        uti: "public.heic",
        exts: &["heic"],
    },
    FileType {
        uti: "public.heif",
        exts: &["heif"],
    },
    FileType {
        uti: "public.tiff",
        exts: &["tif", "tiff"],
    },
    FileType {
        uti: "public.avif",
        exts: &["avif"],
    },
    FileType {
        uti: "public.mpeg-4",
        exts: &["mp4"],
    },
    FileType {
        uti: "com.apple.quicktime-movie",
        exts: &["mov"],
    },
    FileType {
        uti: "com.apple.m4v-video",
        exts: &["m4v"],
    },
    FileType {
        uti: "com.adobe.pdf",
        exts: &["pdf"],
    },
];

/// The files a launch asks for: drops the program (`args[0]`) and every flag,
/// resolves relative paths against `cwd`, keeps the first of duplicates.
pub fn launch_paths(args: &[String], cwd: &Path) -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = Vec::new();
    for arg in args.iter().skip(1).filter(|a| !a.starts_with('-')) {
        let path = cwd.join(arg);
        if !paths.contains(&path) {
            paths.push(path);
        }
    }
    paths
}

/// How many apps the Open With submenu lists.
pub const MENU_APPS: usize = 15;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ListedApp {
    pub app: AppRef,
    pub default: bool,
}

/// The apps for a menu: no blank ids, never Ferry itself, no twins (ids compare
/// without case, as Windows paths do), the default first, the rest by name.
pub fn order_apps(apps: Vec<ListedApp>, own_id: &str, limit: usize) -> Vec<ListedApp> {
    let mut kept: Vec<ListedApp> = Vec::new();
    for listed in apps {
        let id = &listed.app.id;
        if id.is_empty() || id.eq_ignore_ascii_case(own_id) {
            continue;
        }
        match kept.iter_mut().find(|k| k.app.id.eq_ignore_ascii_case(id)) {
            Some(twin) => twin.default |= listed.default,
            None => kept.push(listed),
        }
    }
    kept.sort_by_cached_key(|l| (!l.default, l.app.name.to_lowercase()));
    kept.truncate(limit);
    kept
}

pub struct KnownApp {
    pub name: &'static str,
    pub bundle_ids: &'static [&'static str],
    pub bundle_names: &'static [&'static str],
}

pub const DROPSHARE: KnownApp = KnownApp {
    name: "Dropshare",
    bundle_ids: &["net.mkswap.Dropshare5", "net.mkswap.Dropshare-setapp"],
    bundle_names: &["Dropshare 5.app", "Dropshare.app"],
};

/// The bundle ids are unverified, so the name search is the fallback.
pub const SHELVES: [KnownApp; 4] = [
    KnownApp {
        name: "Yoink",
        bundle_ids: &["at.EternalStorms.Yoink", "at.EternalStorms.Yoink-setapp"],
        bundle_names: &["Yoink.app"],
    },
    KnownApp {
        name: "Dropover",
        bundle_ids: &["me.damir.dropover-mac"],
        bundle_names: &["Dropover.app"],
    },
    KnownApp {
        name: "Dockside",
        bundle_ids: &[],
        bundle_names: &["Dockside.app"],
    },
    KnownApp {
        name: "Dropshelf",
        bundle_ids: &["com.pilotmoon.Dropshelf"],
        bundle_names: &["Dropshelf.app"],
    },
];

/// The first `bundle_names` entry that is a folder, searching `dirs` in order.
pub fn find_in_dirs(known: &KnownApp, dirs: &[PathBuf]) -> Option<PathBuf> {
    dirs.iter()
        .flat_map(|dir| known.bundle_names.iter().map(move |name| dir.join(name)))
        .find(|path| path.is_dir())
}

/// Detected apps, then configured ones, without twins.
pub fn merge_apps(detected: Vec<AppRef>, configured: &[AppRef]) -> Vec<AppRef> {
    let mut merged: Vec<AppRef> = Vec::new();
    for app in detected.into_iter().chain(configured.iter().cloned()) {
        if !merged.iter().any(|m| m.id.eq_ignore_ascii_case(&app.id)) {
            merged.push(app);
        }
    }
    merged
}

/// Arguments for `/usr/bin/open`: `-a` for an app path, `-b` for a bundle id.
pub fn mac_open_args(app_id: &str, files: &[PathBuf]) -> Vec<OsString> {
    let flag = if app_id.starts_with('/') { "-a" } else { "-b" };
    [OsString::from(flag), OsString::from(app_id)]
        .into_iter()
        .chain(files.iter().map(OsString::from))
        .collect()
}

/// Which side of the card the share picker opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Toward {
    Left,
    Right,
}

/// The card's box in the stack page's CSS px (top-left origin).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct Anchor {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub toward: Toward,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ViewRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

/// The anchor in an AppKit view's coordinates (`flipped` views have a top-left origin).
pub fn view_rect(anchor: &Anchor, view_height: f64, flipped: bool) -> ViewRect {
    let y = if flipped {
        anchor.y
    } else {
        (view_height - anchor.y - anchor.height).max(0.0)
    };
    ViewRect {
        x: anchor.x,
        y,
        width: anchor.width,
        height: anchor.height,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn media_kind_follows_the_extension_in_any_case() {
        assert_eq!(media_kind("PNG"), Some(MediaKind::Image));
        assert_eq!(media_kind("jpeg"), Some(MediaKind::Image));
        assert_eq!(media_kind("mov"), Some(MediaKind::Video));
        assert_eq!(media_kind("m4a"), Some(MediaKind::Audio));
        assert_eq!(media_kind("pdf"), Some(MediaKind::Pdf));
        assert_eq!(media_kind("zip"), None);
        assert_eq!(media_kind(""), None);
    }

    #[test]
    fn every_registered_extension_has_a_kind_and_appears_once() {
        let exts: Vec<&str> = FILE_TYPES
            .iter()
            .flat_map(|t| t.exts.iter().copied())
            .collect();
        assert_eq!(exts.len(), 14);
        for ext in &exts {
            assert!(media_kind(ext).is_some(), "{ext}");
            assert_eq!(exts.iter().filter(|e| *e == ext).count(), 1, "{ext}");
            assert_eq!(*ext, ext.to_lowercase());
        }
    }

    #[test]
    fn launch_paths_skip_the_program_and_flags() {
        let cwd = std::env::temp_dir();
        let shot = cwd.join("shot a.png");
        let args = vec![
            "Ferry".to_string(),
            "--optimise".into(),
            shot.to_string_lossy().into_owned(),
            "-psn_0_1234".into(),
        ];
        assert_eq!(launch_paths(&args, &cwd), vec![shot]);
    }

    #[test]
    fn relative_launch_paths_resolve_against_the_working_folder() {
        let cwd = std::env::temp_dir();
        let args = vec!["Ferry".to_string(), "b.png".into()];
        assert_eq!(launch_paths(&args, &cwd), vec![cwd.join("b.png")]);
    }

    #[test]
    fn a_bare_launch_has_no_paths() {
        let cwd = std::env::temp_dir();
        assert!(launch_paths(&["Ferry".to_string()], &cwd).is_empty());
        assert!(launch_paths(&[], &cwd).is_empty());
    }

    #[test]
    fn the_same_file_twice_is_optimised_once() {
        let cwd = std::env::temp_dir();
        let a = cwd.join("a.png").to_string_lossy().into_owned();
        let args = vec!["Ferry".to_string(), a.clone(), "--optimise".into(), a];
        assert_eq!(launch_paths(&args, &cwd).len(), 1);
    }

    fn app(id: &str, name: &str) -> AppRef {
        AppRef {
            id: id.into(),
            name: name.into(),
        }
    }
    fn listed(id: &str, name: &str, default: bool) -> ListedApp {
        ListedApp {
            app: app(id, name),
            default,
        }
    }

    #[test]
    fn ferry_never_lists_itself_and_the_default_comes_first() {
        let apps = vec![
            listed("com.b", "beta", false),
            listed("io.ferry.desktop", "Ferry", false),
            listed("com.a", "Alpha", false),
            listed("com.p", "Preview", true),
            listed("com.a", "Alpha", false),
            listed("", "Nameless", false),
        ];
        let names: Vec<String> = order_apps(apps, "IO.FERRY.DESKTOP", MENU_APPS)
            .into_iter()
            .map(|l| l.app.name)
            .collect();
        assert_eq!(names, ["Preview", "Alpha", "beta"]);
    }

    #[test]
    fn windows_paths_compare_without_case() {
        let apps = vec![listed(
            r"c:\users\x\appdata\local\ferry\ferry.exe",
            "Ferry",
            false,
        )];
        assert!(
            order_apps(apps, r"C:\Users\x\AppData\Local\Ferry\Ferry.exe", MENU_APPS).is_empty()
        );
    }

    #[test]
    fn the_list_stops_at_the_limit() {
        let apps = (0..20)
            .map(|n| listed(&format!("com.{n}"), &format!("App {n:02}"), false))
            .collect();
        assert_eq!(order_apps(apps, "io.ferry.desktop", MENU_APPS).len(), 15);
    }

    #[test]
    fn a_known_app_is_found_by_bundle_name_in_the_first_folder_that_has_it() {
        let root = tempfile::tempdir().unwrap();
        let apps = root.path().join("Applications");
        let setapp = root.path().join("Setapp");
        fs::create_dir_all(&apps).unwrap();
        fs::create_dir_all(setapp.join("Dropover.app")).unwrap();
        fs::write(apps.join("Dropover.app"), b"").unwrap(); // a file, not a bundle
        let dropover = SHELVES.iter().find(|k| k.name == "Dropover").unwrap();
        let dirs = [apps, setapp.clone()];
        assert_eq!(
            find_in_dirs(dropover, &dirs),
            Some(setapp.join("Dropover.app"))
        );
        assert_eq!(find_in_dirs(&DROPSHARE, &dirs), None);
    }

    #[test]
    fn configured_apps_follow_the_detected_ones_without_twins() {
        let merged = merge_apps(
            vec![app("at.EternalStorms.Yoink", "Yoink")],
            &[
                app("AT.ETERNALSTORMS.YOINK", "Yoink"),
                app("com.x.shelf", "Shelf X"),
            ],
        );
        assert_eq!(
            merged,
            [
                app("at.EternalStorms.Yoink", "Yoink"),
                app("com.x.shelf", "Shelf X")
            ]
        );
    }

    #[test]
    fn open_args_pass_each_file_whole() {
        let os = |s: &str| OsString::from(s);
        let files = [
            PathBuf::from("/tmp/shot a.png"),
            PathBuf::from("/tmp/-b.png"),
        ];
        assert_eq!(
            mac_open_args("com.apple.Preview", &files),
            [
                os("-b"),
                os("com.apple.Preview"),
                os("/tmp/shot a.png"),
                os("/tmp/-b.png")
            ]
        );
        assert_eq!(
            mac_open_args("/Applications/Tool.app", &files[..1]),
            [
                os("-a"),
                os("/Applications/Tool.app"),
                os("/tmp/shot a.png")
            ]
        );
    }

    #[test]
    fn the_anchor_flips_into_an_unflipped_view() {
        let anchor = Anchor {
            x: 12.0,
            y: 40.0,
            width: 300.0,
            height: 200.0,
            toward: Toward::Left,
        };
        let rect = |y| ViewRect {
            x: 12.0,
            y,
            width: 300.0,
            height: 200.0,
        };
        assert_eq!(view_rect(&anchor, 500.0, false), rect(260.0));
        assert_eq!(view_rect(&anchor, 500.0, true), rect(40.0));
        assert_eq!(view_rect(&anchor, 100.0, false).y, 0.0);
    }

    #[test]
    fn the_page_sends_the_anchor_as_json() {
        let anchor: Anchor =
            serde_json::from_str(r#"{"x":1,"y":2,"width":3,"height":4,"toward":"right"}"#).unwrap();
        assert_eq!(anchor.toward, Toward::Right);
        assert_eq!(anchor.width, 3.0);
    }
}
