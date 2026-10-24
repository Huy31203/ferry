//! Which file names a watcher never processes, and the user's ignore globs.

use std::path::Path;

use crate::selfwrite;

/// Suffixes of temp and half-downloaded files (`.ferry-tmp` is
/// `fsutil::atomic_replace`'s own temp).
const TEMP_SUFFIXES: &[&str] = &[
    ".ferry-tmp",
    ".part",
    ".partial",
    ".crdownload",
    ".download",
    ".tmp",
    ".temp",
];

/// Hidden files (macOS screenshot temp `.Screenshot…`, AppleDouble `._x`,
/// `.DS_Store`, iCloud `.x.icloud`), Office lock files, temp files and the
/// folders' own system files.
pub fn is_ignored_name(name: &str) -> bool {
    let lower = name.to_lowercase();
    name.starts_with('.')
        || name.starts_with("~$")
        || TEMP_SUFFIXES.iter().any(|s| lower.ends_with(s))
        || lower == "thumbs.db"
        || lower == "desktop.ini"
}

/// `*` matches any run (including none), `?` exactly one character; the whole
/// name must match, ignoring case. A pattern with a path separator never matches.
pub fn glob_matches(pattern: &str, name: &str) -> bool {
    if pattern.contains(['/', '\\']) {
        return false;
    }
    let pattern: Vec<char> = pattern.to_lowercase().chars().collect();
    let name: Vec<char> = name.to_lowercase().chars().collect();
    let (mut p, mut n) = (0, 0);
    let mut backtrack: Option<(usize, usize)> = None;
    while n < name.len() {
        if p < pattern.len() && (pattern[p] == '?' || pattern[p] == name[n]) {
            p += 1;
            n += 1;
        } else if p < pattern.len() && pattern[p] == '*' {
            backtrack = Some((p, n));
            p += 1;
        } else if let Some((star, at)) = backtrack {
            p = star + 1;
            n = at + 1;
            backtrack = Some((star, at + 1));
        } else {
            return false;
        }
    }
    pattern[p..].iter().all(|&c| c == '*')
}

/// Is `path` equal to `root` or somewhere below it?
pub fn is_inside(path: &Path, root: &Path) -> bool {
    selfwrite::key(path).starts_with(selfwrite::key(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temp_hidden_and_system_names_are_ignored() {
        for n in [
            ".Screenshot 2026-10-23 at 10.21.33.png",
            "._a.png",
            ".DS_Store",
            ".a.png.icloud",
            "~$doc.docx",
            "a.png.ferry-tmp",
            "a.PART",
            "b.crdownload",
            "c.png.download",
            "d.tmp",
            "e.TEMP",
            "Thumbs.db",
            "desktop.ini",
            "f.partial",
        ] {
            assert!(is_ignored_name(n), "{n}");
        }
        for n in [
            "Screenshot 2026-10-23 at 10.21.33\u{202F}AM.png",
            "a.png",
            "tmp.png",
            "Screen Recording 1.mov",
        ] {
            assert!(!is_ignored_name(n), "{n}");
        }
    }

    #[test]
    fn globs_match_the_whole_name_without_case() {
        assert!(glob_matches("*.MOV", "clip.mov"));
        assert!(glob_matches("draft-??.png", "draft-01.png"));
        assert!(!glob_matches("draft-?.png", "draft-01.png"));
        assert!(glob_matches("*", "x"));
        assert!(!glob_matches("*.png", "a.png.bak"));
        assert!(!glob_matches("sub/*.png", "a.png"));
        assert!(glob_matches("a*b*c", "aXXbYYc"));
        assert!(!glob_matches("a*b*c", "aXXbYY"));
    }

    #[test]
    fn inside_compares_whole_components() {
        assert!(!is_inside(Path::new("/a/bc"), Path::new("/a/b")));
        assert!(is_inside(Path::new("/a/b/c"), Path::new("/a/b")));
        assert!(is_inside(Path::new("/a/b"), Path::new("/a/b")));
    }
}
