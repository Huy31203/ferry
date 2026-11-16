//! What goes into the archive: each shelf item under its own name, folders walked.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub source: PathBuf,
    /// The path inside the zip, `/`-separated; a folder ends in `/`.
    pub name: String,
    /// Bytes for a file, 0 for a folder.
    pub size: u64,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// Every item as a top-level entry; clashing names get " 2", " 3" before the
/// extension. Folders are walked sorted; symlinks inside and `.DS_Store` are skipped.
/// Runs before anything is written, so the archive never contains itself.
pub fn collect(items: &[PathBuf]) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut taken_names: Vec<String> = Vec::new();
    for item in items {
        let name = unique(&file_name(item), &taken_names);
        taken_names.push(name.clone());
        let meta = fs::metadata(item)?;
        if meta.is_dir() {
            walk(item, &name, &mut out)?;
        } else if meta.is_file() {
            out.push(Entry {
                source: item.clone(),
                name,
                size: meta.len(),
            });
        }
    }
    Ok(out)
}

fn walk(dir: &Path, name: &str, out: &mut Vec<Entry>) -> io::Result<()> {
    out.push(Entry {
        source: dir.to_path_buf(),
        name: format!("{name}/"),
        size: 0,
    });
    let mut children = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|c| c.file_name());
    for child in children {
        let file = child.file_name().to_string_lossy().into_owned();
        if file == ".DS_Store" {
            continue;
        }
        let (path, inner) = (child.path(), format!("{name}/{file}"));
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            walk(&path, &inner, out)?;
        } else if meta.is_file() {
            out.push(Entry {
                source: path,
                name: inner,
                size: meta.len(),
            });
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn unique(base: &str, taken_names: &[String]) -> String {
    if !taken_names.iter().any(|t| t == base) {
        return base.to_string();
    }
    let (stem, ext) = match base.rfind('.') {
        Some(i) if i > 0 => base.split_at(i),
        _ => (base, ""),
    };
    (2..)
        .map(|n| format!("{stem} {n}{ext}"))
        .find(|c| !taken_names.contains(c))
        .expect("an unused number exists")
}

/// As Finder's Compress: one item is `<name>.zip`, more are `Archive.zip`.
pub fn default_name(items: &[PathBuf]) -> String {
    match items {
        [one] => format!("{}.zip", file_name(one)),
        _ => "Archive.zip".to_string(),
    }
}

/// A target counts as taken if it or its first volume exists.
pub fn taken(path: &Path) -> bool {
    path.exists() || PathBuf::from(format!("{}.001", path.display())).exists()
}

/// `name` in `folder`, or the first free `<stem> 2.zip`, `<stem> 3.zip`…
pub fn free_path(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !taken(&first) {
        return first;
    }
    let (stem, ext) = name
        .strip_suffix(".zip")
        .map_or((name, ""), |s| (s, ".zip"));
    (2..)
        .map(|n| folder.join(format!("{stem} {n}{ext}")))
        .find(|p| !taken(p))
        .expect("an unused number exists")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn clashing_item_names_get_a_number_before_the_extension() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("photo.png"), b"1").unwrap();
        fs::write(b.join("photo.png"), b"22").unwrap();
        let got = collect(&[a.join("photo.png"), b.join("photo.png")]).unwrap();
        let names: Vec<_> = got.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["photo.png", "photo 2.png"]);
        assert_eq!(got[1].size, 2);
    }

    #[test]
    fn folders_are_walked_sorted_without_ds_store_or_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().join("pics");
        fs::create_dir_all(top.join("sub")).unwrap();
        fs::write(top.join("b.txt"), b"b").unwrap();
        fs::write(top.join("a.txt"), b"a").unwrap();
        fs::write(top.join(".DS_Store"), b"x").unwrap();
        fs::write(top.join("sub/c.txt"), b"c").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(top.join("a.txt"), top.join("link")).unwrap();
        let names: Vec<_> = collect(&[top])
            .unwrap()
            .into_iter()
            .map(|e| e.name)
            .collect();
        assert_eq!(
            names,
            [
                "pics/",
                "pics/a.txt",
                "pics/b.txt",
                "pics/sub/",
                "pics/sub/c.txt"
            ]
        );
    }

    #[test]
    fn default_name_follows_finder() {
        assert_eq!(default_name(&["/x/a.png".into()]), "a.png.zip");
        assert_eq!(default_name(&["/x/folder".into()]), "folder.zip");
        assert_eq!(default_name(&["/x/a".into(), "/x/b".into()]), "Archive.zip");
    }

    #[test]
    fn free_path_skips_taken_names_and_volumes() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            free_path(dir.path(), "Archive.zip"),
            dir.path().join("Archive.zip")
        );
        fs::write(dir.path().join("Archive.zip"), b"").unwrap();
        fs::write(dir.path().join("Archive 2.zip.001"), b"").unwrap();
        assert_eq!(
            free_path(dir.path(), "Archive.zip"),
            dir.path().join("Archive 3.zip")
        );
    }
}
