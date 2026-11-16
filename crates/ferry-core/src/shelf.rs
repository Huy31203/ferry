//! The shelf: files and folders held by reference until they are dragged out
//! (shelf spec §2). Paths only; files on disk are never touched.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::fsutil;
pub use crate::settings::LoadStatus;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Item {
    pub path: PathBuf,
    /// Unix seconds.
    #[serde(default)]
    pub added_at: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Added {
    pub added: usize,
    /// Paths that could not be read (gone, or no permission).
    pub failed: usize,
}

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Shelf {
    pub items: Vec<Item>,
    /// What "Clear shelf" took, until Undo, an add, or the Undo window ends.
    #[serde(skip)]
    undo: Option<Vec<Item>>,
}

impl Shelf {
    pub fn add(&mut self, paths: &[PathBuf], now: u64) -> Added {
        self.undo = None;
        let mut out = Added {
            added: 0,
            failed: 0,
        };
        for path in paths {
            let Ok(path) = fs::canonicalize(path) else {
                out.failed += 1;
                continue;
            };
            if self.items.iter().any(|i| i.path == path) {
                continue;
            }
            self.items.push(Item {
                path,
                added_at: now,
            });
            out.added += 1;
        }
        out
    }

    pub fn remove(&mut self, paths: &[PathBuf]) {
        self.items.retain(|i| !paths.contains(&i.path));
    }

    /// "Optimise all": the result takes its source's place.
    pub fn replace(&mut self, from: &Path, to: &Path) {
        let to_listed = self.items.iter().any(|i| i.path == to);
        if to_listed {
            self.items.retain(|i| i.path != from);
        } else if let Some(item) = self.items.iter_mut().find(|i| i.path == from) {
            item.path = to.to_path_buf();
        }
    }

    pub fn clear(&mut self) {
        if !self.items.is_empty() {
            self.undo = Some(std::mem::take(&mut self.items));
        }
    }

    pub fn undo(&mut self) -> bool {
        match self.undo.take() {
            Some(items) => {
                self.items = items;
                true
            }
            None => false,
        }
    }

    pub fn can_undo(&self) -> bool {
        self.undo.is_some()
    }

    pub fn forget_undo(&mut self) {
        self.undo = None;
    }

    /// Drop items whose file is gone. Returns how many went.
    pub fn prune(&mut self) -> usize {
        let before = self.items.len();
        self.items.retain(|i| i.path.exists());
        before - self.items.len()
    }

    pub fn paths(&self) -> Vec<PathBuf> {
        self.items.iter().map(|i| i.path.clone()).collect()
    }

    /// As `Settings::load`: missing → empty; corrupt → empty plus a `.bak` copy.
    pub fn load(path: &Path) -> (Shelf, LoadStatus) {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Shelf::default(), LoadStatus::Missing);
            }
            Err(_) => return (Shelf::default(), crate::settings::recover(path)),
        };
        match serde_json::from_slice::<Shelf>(&bytes) {
            Ok(shelf) => (shelf, LoadStatus::Loaded),
            Err(_) => (Shelf::default(), crate::settings::recover(path)),
        }
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fsutil::atomic_replace(path, &json)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(dir: &Path, name: &str) -> PathBuf {
        let p = dir.join(name);
        fs::write(&p, b"x").unwrap();
        fs::canonicalize(p).unwrap()
    }

    #[test]
    fn add_skips_duplicates_and_counts_what_failed() {
        let dir = tempfile::tempdir().unwrap();
        let a = file(dir.path(), "a.png");
        let mut shelf = Shelf::default();
        let added = shelf.add(&[a.clone(), a.clone(), dir.path().join("gone.png")], 1);
        assert_eq!(
            added,
            Added {
                added: 1,
                failed: 1
            }
        );
        assert_eq!(shelf.paths(), vec![a]);
    }

    #[test]
    fn a_symlink_and_its_target_are_one_item() {
        let dir = tempfile::tempdir().unwrap();
        let a = file(dir.path(), "a.png");
        let link = dir.path().join("link.png");
        std::os::unix::fs::symlink(&a, &link).unwrap();
        let mut shelf = Shelf::default();
        shelf.add(&[a.clone(), link, dir.path().join("./a.png")], 1);
        assert_eq!(shelf.paths(), vec![a]);
    }

    #[test]
    fn folders_go_on_the_shelf_too() {
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("photos");
        fs::create_dir(&sub).unwrap();
        let mut shelf = Shelf::default();
        assert_eq!(shelf.add(&[sub], 1).added, 1);
    }

    #[test]
    fn remove_takes_only_the_named_items() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (file(dir.path(), "a.png"), file(dir.path(), "b.png"));
        let mut shelf = Shelf::default();
        shelf.add(&[a.clone(), b.clone()], 1);
        shelf.remove(&[a]);
        assert_eq!(shelf.paths(), vec![b]);
    }

    #[test]
    fn clear_then_undo_restores_the_list() {
        let dir = tempfile::tempdir().unwrap();
        let a = file(dir.path(), "a.png");
        let mut shelf = Shelf::default();
        shelf.add(std::slice::from_ref(&a), 1);
        shelf.clear();
        assert!(shelf.items.is_empty() && shelf.can_undo());
        assert!(shelf.undo());
        assert_eq!(shelf.paths(), vec![a]);
        assert!(!shelf.can_undo());
    }

    #[test]
    fn an_add_after_clear_drops_the_undo() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (file(dir.path(), "a.png"), file(dir.path(), "b.png"));
        let mut shelf = Shelf::default();
        shelf.add(&[a], 1);
        shelf.clear();
        shelf.add(std::slice::from_ref(&b), 2);
        assert!(!shelf.undo());
        assert_eq!(shelf.paths(), vec![b]);
    }

    #[test]
    fn clearing_an_empty_shelf_offers_no_undo() {
        let mut shelf = Shelf::default();
        shelf.clear();
        assert!(!shelf.can_undo());
    }

    #[test]
    fn prune_drops_missing_paths() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (file(dir.path(), "a.png"), file(dir.path(), "b.png"));
        let mut shelf = Shelf::default();
        shelf.add(&[a.clone(), b], 1);
        fs::remove_file(dir.path().join("b.png")).unwrap();
        assert_eq!(shelf.prune(), 1);
        assert_eq!(shelf.paths(), vec![a]);
    }

    #[test]
    fn replace_swaps_a_source_for_its_result_without_duplicating() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (file(dir.path(), "a.png"), file(dir.path(), "a.webp"));
        let mut shelf = Shelf::default();
        shelf.add(std::slice::from_ref(&a), 1);
        shelf.replace(&a, &b);
        assert_eq!(shelf.paths(), vec![b.clone()]);
        shelf.add(std::slice::from_ref(&a), 2);
        shelf.replace(&a, &b); // b is already there: a just goes
        assert_eq!(shelf.paths(), vec![b]);
    }

    #[test]
    fn the_list_round_trips_through_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let a = file(dir.path(), "a.png");
        let path = dir.path().join("data/shelf.json");
        let mut shelf = Shelf::default();
        shelf.add(&[a], 7);
        shelf.save(&path).unwrap();
        let (back, status) = Shelf::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(back.items, shelf.items);
    }

    #[test]
    fn a_missing_file_is_an_empty_shelf() {
        let dir = tempfile::tempdir().unwrap();
        let (shelf, status) = Shelf::load(&dir.path().join("shelf.json"));
        assert!(shelf.items.is_empty());
        assert_eq!(status, LoadStatus::Missing);
    }

    #[test]
    fn a_corrupt_file_starts_empty_and_keeps_a_bak_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shelf.json");
        fs::write(&path, b"{ not json").unwrap();
        let (shelf, status) = Shelf::load(&path);
        assert!(shelf.items.is_empty());
        let bak = dir.path().join("shelf.json.bak");
        assert_eq!(
            status,
            LoadStatus::Recovered {
                backup: Some(bak.clone())
            }
        );
        assert_eq!(fs::read(bak).unwrap(), b"{ not json");
    }

    #[test]
    fn an_item_without_added_at_loads() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("shelf.json");
        fs::write(&path, br#"{"items":[{"path":"/x/a.png"}]}"#).unwrap();
        let (shelf, status) = Shelf::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(shelf.items[0].added_at, 0);
    }
}
