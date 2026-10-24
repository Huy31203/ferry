//! Stream H: what Ferry registers with Finder / Explorer and the pure parts of
//! sharing and opening files with other apps.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

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
    FileType { uti: "public.png", exts: &["png"] },
    FileType { uti: "public.jpeg", exts: &["jpg", "jpeg"] },
    FileType { uti: "com.compuserve.gif", exts: &["gif"] },
    FileType { uti: "org.webmproject.webp", exts: &["webp"] },
    FileType { uti: "public.heic", exts: &["heic"] },
    FileType { uti: "public.heif", exts: &["heif"] },
    FileType { uti: "public.tiff", exts: &["tif", "tiff"] },
    FileType { uti: "public.avif", exts: &["avif"] },
    FileType { uti: "public.mpeg-4", exts: &["mp4"] },
    FileType { uti: "com.apple.quicktime-movie", exts: &["mov"] },
    FileType { uti: "com.apple.m4v-video", exts: &["m4v"] },
    FileType { uti: "com.adobe.pdf", exts: &["pdf"] },
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

#[cfg(test)]
mod tests {
    use super::*;

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
        let exts: Vec<&str> = FILE_TYPES.iter().flat_map(|t| t.exts.iter().copied()).collect();
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
}
