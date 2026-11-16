# Ferry Shelf Core Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A gray shelf panel at the bottom edge on macOS holds files and folders by reference. It takes drops from any app and from Ferry's cards, gives the pile back in one drag, and can send its contents through the optimiser.

**Architecture:**
- **Pure logic in `ferry_core`:**
  - `shelf.rs`: the item list and its persistence.
  - `shelf_place.rs`: rects, stack inset, zone clearance and drag proximity.
- **App side in `src-tauri/src/shelf/`:** the state, commands, native menu, and a non-activating NSPanel window.
- **Page in `src/shelf/`:** it decides its state with a pure `strip.ts` and reports its height back. Rust then sizes the panel and pushes the stack.
- **Drag watcher:** gains a per-tick hook for proximity and a shelf decision per drag kind.

**Tech Stack:**
- Rust: `ferry-core`, plus Tauri 2 with `tauri_nspanel`.
- Svelte 5 runes, vitest.
- `@crabnebula/tauri-plugin-drag`, `tauri-plugin-opener`.

**Spec:** `docs/superpowers/specs/2026-11-16-ferry-shelf-core-design.md`

## Global Constraints

- macOS only. The shelf window is not created on Windows. Gate each new app-side call with `#[cfg(target_os = "macos")]`, or do nothing on other OSes.
- The shelf stores paths only. Files on disk are never moved, copied or deleted.
- Sizes in points:
  - width 150
  - expanded height 170
  - strip 27 (16 % of 170)
  - proximity reach 80
  - hover collapse delay 300 ms
  - Undo window 5 s
- Colours:
  - background `#3a3d44`
  - border 1 px `#555963`
  - top corners rounded 14 px
  - no ✕ anywhere
- Never use `var` in JS. Reuse existing patterns: `settings.rs` load and `recover`, `rail_menu.rs` menu routing, `zone.rs` window creation, and `drop_batch/list.rs` clipboard write.
- Shell prefix: `export PATH=/usr/local/bin:$HOME/.nvm/versions/node/v22.23.4/bin:$HOME/.cargo/bin:$PATH`.
- Full core tests also need `FERRY_GIFSICLE=$PWD/src-tauri/binaries/gifsicle-x86_64-apple-darwin FERRY_FFMPEG=$PWD/src-tauri/binaries/ffmpeg-x86_64-apple-darwin`.
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- User-visible strings go in `src/lib/i18n/en.json`. Rust reads them with `ferry_core::i18n::{t, tf}`; the page with `t()` from `src/lib/i18n.ts`.

## Review Focus

1. **A shelved file that is renamed or deleted while Ferry runs** must not break a pile drag. Prune runs before every drag start and every menu open (Task 6 and Task 7 tests).
2. **Dropping the same file twice, or the same file via a different path spelling** (a symlink, `..`), keeps one item. Add canonicalises (Task 1 test `a_symlink_and_its_target_are_one_item`).
3. **Optimise all where an item is already Ferry's own result** goes through intake's own-result notice, not a silent drop (Task 8, `accept_paths` keeps `without_own`).
4. **A drag that starts on the shelf and is dropped back on the stack or zone** keeps the items and starts no job (Task 6 `cursor_over_ferry` test plus manual check).
5. **A hand-edited `shelf.json` with a missing `added_at`** loads (serde default), with no `.bak` churn (Task 1 test `an_item_without_added_at_loads`).

---

### Task 1: `ferry_core::shelf` (the list and its file)

**Files:**
- Create: `crates/ferry-core/src/shelf.rs`
- Modify: `crates/ferry-core/src/lib.rs` (add `pub mod shelf;` in alphabetical position)
- Modify: `crates/ferry-core/src/settings.rs:1031` (`fn recover` → `pub(crate) fn recover`)

**Interfaces:**
- Produces:
  - `shelf::Item { pub path: PathBuf, pub added_at: u64 }`
  - `shelf::Shelf`, with:
    - `items: Vec<Item>` (pub)
    - `add(&mut self, paths: &[PathBuf], now: u64) -> Added`
    - `remove(&mut self, paths: &[PathBuf])`
    - `replace(&mut self, from: &Path, to: &Path)`
    - `clear(&mut self)`
    - `undo(&mut self) -> bool`
    - `can_undo(&self) -> bool`
    - `forget_undo(&mut self)`
    - `prune(&mut self) -> usize`
    - `paths(&self) -> Vec<PathBuf>`
    - `load(path: &Path) -> (Shelf, LoadStatus)`
    - `save(&self, path: &Path) -> io::Result<()>`
  - `shelf::Added { pub added: usize, pub failed: usize }`

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/shelf.rs` with only the test module, plus `pub mod shelf;` in `lib.rs`:

```rust
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
        assert_eq!(added, Added { added: 1, failed: 1 });
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
        shelf.add(&[a.clone()], 1);
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
        shelf.add(&[b.clone()], 2);
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
        shelf.add(&[a.clone()], 1);
        shelf.replace(&a, &b);
        assert_eq!(shelf.paths(), vec![b.clone()]);
        shelf.add(&[a.clone()], 2);
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
        assert_eq!(status, LoadStatus::Recovered { backup: Some(bak.clone()) });
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
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core shelf::`
Expected: compile errors (`Shelf`, `Added`, `LoadStatus` not found).

- [ ] **Step 3: Implement**

Put this above the test module in `shelf.rs`, and make `settings::recover` `pub(crate)`:

```rust
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
        let mut out = Added { added: 0, failed: 0 };
        for path in paths {
            let Ok(path) = fs::canonicalize(path) else {
                out.failed += 1;
                continue;
            };
            if self.items.iter().any(|i| i.path == path) {
                continue;
            }
            self.items.push(Item { path, added_at: now });
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
```

If `LoadStatus` doesn't derive `PartialEq`, the settings tests already compare it, so it does. Keep it as is.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core shelf::`
Expected: 13 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/shelf.rs crates/ferry-core/src/lib.rs crates/ferry-core/src/settings.rs
git commit -m "feat(core): the shelf's item list and its file

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: `ferry_core::shelf_place` (rects, stack inset, zone clearance, proximity)

**Files:**
- Create: `crates/ferry-core/src/shelf_place.rs`
- Modify: `crates/ferry-core/src/lib.rs` (`pub mod shelf_place;`)

**Interfaces:**
- Consumes: `geometry::{Corner, Rect}`.
- Produces (all in physical pixels):
  - `pub const WIDTH: f64 = 150.0; EXPANDED: f64 = 170.0; STRIP: f64 = 27.0; REACH: f64 = 80.0;` (points)
  - `shelf_rect(area: Rect, corner: Corner, size: (u32, u32), margin: u32) -> Rect`
  - `inset(has_items: bool, height: u32, strip: u32) -> u32`
  - `stack_area(area: Rect, corner: Corner, inset: u32) -> Rect`
  - `clear_of(zone: Rect, shelf: Rect, area: Rect) -> (i32, i32)`
  - `may_expand(zone: Option<Rect>, expanded: Rect) -> bool`
  - `drag_near(cursor: (i32, i32), strip: Rect, reach: u32, zone: Option<Rect>) -> bool`
  - `contains(r: Rect, p: (i32, i32)) -> bool`
  - `intersects(a: Rect, b: Rect) -> bool`

- [ ] **Step 1: Write the failing tests**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::drag::zone_near_results;

    const AREA: Rect = Rect { x: 0, y: 25, width: 1440, height: 875 };
    const SIZE: (u32, u32) = (150, 170);

    #[test]
    fn the_shelf_sits_on_the_bottom_edge_on_the_stacks_side() {
        let r = shelf_rect(AREA, Corner::BottomRight, SIZE, 4);
        assert_eq!(r, Rect { x: 1440 - 4 - 150, y: 900 - 170, width: 150, height: 170 });
        let l = shelf_rect(AREA, Corner::BottomLeft, (150, 27), 4);
        assert_eq!(l, Rect { x: 4, y: 900 - 27, width: 150, height: 27 });
    }

    #[test]
    fn top_corners_keep_the_shelf_at_the_bottom() {
        let r = shelf_rect(AREA, Corner::TopRight, SIZE, 4);
        assert_eq!(r.y, 900 - 170);
        assert_eq!(r.x, 1440 - 4 - 150);
        // and the stack keeps the whole area
        assert_eq!(stack_area(AREA, Corner::TopRight, 170), AREA);
    }

    #[test]
    fn the_stack_moves_up_by_the_strip_only_when_the_shelf_has_items() {
        assert_eq!(inset(true, 27, 27), 27);
        assert_eq!(inset(false, 27, 27), 0); // empty strip during a drag overlays
        assert_eq!(inset(false, 0, 27), 0);
        assert_eq!(inset(true, 0, 27), 0); // hidden window takes nothing
        assert_eq!(stack_area(AREA, Corner::BottomRight, 27).height, 875 - 27);
    }

    #[test]
    fn the_expanded_shelf_pushes_the_stack_up() {
        assert_eq!(inset(true, 170, 27), 170);
        assert_eq!(inset(false, 170, 27), 170); // drop target while empty
        assert_eq!(stack_area(AREA, Corner::BottomLeft, 170).height, 875 - 170);
    }

    #[test]
    fn a_zone_over_the_shelf_moves_up_clear_of_it() {
        let shelf = shelf_rect(AREA, Corner::BottomRight, SIZE, 4);
        let zone = Rect { x: 1200, y: 800, width: 220, height: 140 };
        let (x, y) = clear_of(zone, shelf, AREA);
        assert_eq!((x, y), (1200, shelf.y - 140));
        assert!(!intersects(Rect { x, y, ..zone }, shelf));
        let apart = Rect { x: 100, y: 100, ..zone };
        assert_eq!(clear_of(apart, shelf, AREA), (100, 100));
    }

    #[test]
    fn every_zone_placement_clears_the_expanded_shelf() {
        let shelf = shelf_rect(AREA, Corner::BottomRight, SIZE, 4);
        let zone = (220, 140);
        // near results, with and without a stack, in the area the pushed stack leaves
        let room = stack_area(AREA, Corner::BottomRight, 170);
        for stack in [None, Some((300, 200))] {
            let (x, y) = zone_near_results(room, Corner::BottomRight, zone, stack, 4, 0);
            assert!(!intersects(Rect { x, y, width: 220, height: 140 }, shelf), "{stack:?}");
        }
        // tab, from-tab and under-cursor placements land anywhere: clear_of fixes them
        for (x, y) in [(1440 - 220, 760), (1300, 880), (1250, 790)] {
            let (x, y) = clear_of(Rect { x, y, width: 220, height: 140 }, shelf, AREA);
            assert!(!intersects(Rect { x, y, width: 220, height: 140 }, shelf));
        }
    }

    #[test]
    fn the_zone_wins_when_both_cannot_fit() {
        let tiny = Rect { x: 0, y: 0, width: 400, height: 200 };
        let shelf = shelf_rect(tiny, Corner::BottomRight, SIZE, 4);
        let zone = Rect { x: 180, y: 40, width: 220, height: 140 };
        let (_, y) = clear_of(zone, shelf, tiny);
        assert_eq!(y, 0); // pinned to the top, still overlapping
        assert!(!may_expand(Some(Rect { y, ..zone }), shelf));
        assert!(may_expand(None, shelf));
    }

    #[test]
    fn a_drag_near_the_strip_expands_it_but_not_over_the_zone() {
        let strip = shelf_rect(AREA, Corner::BottomRight, (150, 27), 4);
        assert!(drag_near((strip.x + 10, strip.y + 5), strip, 80, None));
        assert!(drag_near((strip.x - 70, strip.y - 70), strip, 80, None));
        assert!(!drag_near((strip.x - 90, strip.y), strip, 80, None));
        assert!(!drag_near((strip.x + 10, strip.y - 81), strip, 80, None));
        let zone = Rect { x: strip.x - 50, y: strip.y - 60, width: 100, height: 50 };
        assert!(!drag_near((strip.x - 20, strip.y - 40), strip, 80, Some(zone)));
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core shelf_place::`
Expected: compile errors.

- [ ] **Step 3: Implement**

```rust
//! Where the shelf goes, and how the stack and the drop zone make room for it
//! (shelf spec §4). Rects are physical pixels; the constants are points.

use crate::geometry::{Corner, Rect};

pub const WIDTH: f64 = 150.0;
pub const EXPANDED: f64 = 170.0;
/// 16 % of the expanded height.
pub const STRIP: f64 = 27.0;
/// How near a drag must come to the strip to expand it.
pub const REACH: f64 = 80.0;

fn at_right(corner: Corner) -> bool {
    matches!(corner, Corner::TopRight | Corner::BottomRight)
}

fn at_bottom(corner: Corner) -> bool {
    matches!(corner, Corner::BottomLeft | Corner::BottomRight)
}

/// On the bottom edge of `area` whatever the corner, on the corner's side.
pub fn shelf_rect(area: Rect, corner: Corner, size: (u32, u32), margin: u32) -> Rect {
    let (w, h, m) = (size.0 as i32, size.1 as i32, margin as i32);
    let x = if at_right(corner) {
        area.x + area.width as i32 - m - w
    } else {
        area.x + m
    };
    let y = area.y + area.height as i32 - h;
    Rect { x, y, width: size.0, height: size.1 }
}

/// What the shelf takes from the stack: the expanded height when open (empty
/// or not), the strip when it rests with items, nothing otherwise.
pub fn inset(has_items: bool, height: u32, strip: u32) -> u32 {
    if height > strip {
        height
    } else if has_items && height > 0 {
        height
    } else {
        0
    }
}

/// The area the stack is placed in. Top-corner stacks keep all of it.
pub fn stack_area(area: Rect, corner: Corner, inset: u32) -> Rect {
    if at_bottom(corner) {
        Rect { height: area.height.saturating_sub(inset), ..area }
    } else {
        area
    }
}

pub fn contains(r: Rect, p: (i32, i32)) -> bool {
    p.0 >= r.x && p.0 < r.x + r.width as i32 && p.1 >= r.y && p.1 < r.y + r.height as i32
}

pub fn intersects(a: Rect, b: Rect) -> bool {
    a.x < b.x + b.width as i32
        && b.x < a.x + a.width as i32
        && a.y < b.y + b.height as i32
        && b.y < a.y + a.height as i32
}

/// The zone's top-left, moved up so it ends where the shelf starts, never above `area`.
pub fn clear_of(zone: Rect, shelf: Rect, area: Rect) -> (i32, i32) {
    if intersects(zone, shelf) {
        (zone.x, (shelf.y - zone.height as i32).max(area.y))
    } else {
        (zone.x, zone.y)
    }
}

/// The zone wins: the shelf stays resting when expanding would cover it.
pub fn may_expand(zone: Option<Rect>, expanded: Rect) -> bool {
    !zone.is_some_and(|z| intersects(z, expanded))
}

/// A drag within `reach` of the strip (above or beside it), not inside the zone.
pub fn drag_near(cursor: (i32, i32), strip: Rect, reach: u32, zone: Option<Rect>) -> bool {
    if zone.is_some_and(|z| contains(z, cursor)) {
        return false;
    }
    let r = reach as i32;
    let grown = Rect {
        x: strip.x - r,
        y: strip.y - r,
        width: strip.width + 2 * reach,
        height: strip.height + reach,
    };
    contains(grown, cursor)
}
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core shelf_place::`
Expected: 8 passed. If `every_zone_placement_clears_the_expanded_shelf` fails for the stack case, check `zone_near_results`'s beside-the-stack fallback (`drag.rs:269`). If it can land over the shelf, the app side (Task 5) must also run `clear_of` after `zone_near_results`. Make the test call `clear_of` on that result too, and keep the assertion.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/shelf_place.rs crates/ferry-core/src/lib.rs
git commit -m "feat(core): where the shelf goes and how the zone and stack make room

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: `src/shelf/strip.ts` (the page's state)

**Files:**
- Create: `src/shelf/strip.ts`, `src/shelf/strip.test.ts`
- Modify: `src/lib/i18n/en.json` (add `"shelf.items": "{count} Items"`, `"shelf.item": "1 Item"`, `"shelf.drop": "Drop to shelve"`, `"shelf.cleared": "Shelf cleared"`, `"shelf.undo": "Undo"`)

**Interfaces:**
- Produces:
  - `type StripState = 'hidden' | 'resting' | 'open' | 'target' | 'cleared'`
  - `interface DragView { offer: boolean; near: boolean; own: boolean }`
  - `interface StripInput { count: number; canUndo: boolean; hover: boolean; drag: DragView }`
  - `stripState(input: StripInput): StripState`
  - `stripHeight(state: StripState): number` (points)
  - `countLabel(count: number): string`
  - `const NO_DRAG: DragView`
  - `STRIP = 27`, `EXPANDED = 170`, `COLLAPSE_MS = 300`, `UNDO_MS = 5000`

- [ ] **Step 1: Write the failing test**

```ts
import { describe, expect, it } from 'vitest';
import { NO_DRAG, countLabel, stripHeight, stripState } from './strip';

const base = { count: 0, canUndo: false, hover: false, drag: NO_DRAG };

describe('stripState', () => {
  it('is hidden when empty and nothing is dragged', () => {
    expect(stripState(base)).toBe('hidden');
    expect(stripState({ ...base, hover: true })).toBe('hidden');
  });
  it('rests with items, opens on hover', () => {
    expect(stripState({ ...base, count: 3 })).toBe('resting');
    expect(stripState({ ...base, count: 3, hover: true })).toBe('open');
  });
  it('shows the strip for an offered drag, even when empty, and becomes the target near it', () => {
    const offer = { offer: true, near: false, own: false };
    expect(stripState({ ...base, drag: offer })).toBe('resting');
    expect(stripState({ ...base, drag: { ...offer, near: true } })).toBe('target');
  });
  it('stays open with no target while its own pile is dragged', () => {
    const own = { offer: false, near: true, own: true };
    expect(stripState({ ...base, count: 2, drag: own })).toBe('open');
  });
  it('shows the cleared note while Undo is possible, and an add dismisses it', () => {
    expect(stripState({ ...base, canUndo: true })).toBe('cleared');
    expect(stripState({ ...base, count: 1, canUndo: false })).toBe('resting');
  });
});

describe('stripHeight', () => {
  it('maps states to the panel height', () => {
    expect(stripHeight('hidden')).toBe(0);
    expect(stripHeight('resting')).toBe(27);
    expect(stripHeight('cleared')).toBe(27);
    expect(stripHeight('open')).toBe(170);
    expect(stripHeight('target')).toBe(170);
  });
});

describe('countLabel', () => {
  it('says Item or Items', () => {
    expect(countLabel(1)).toBe('1 Item');
    expect(countLabel(8)).toBe('8 Items');
  });
});
```

- [ ] **Step 2: Run it to see it fail**

Run: `node_modules/.bin/vitest run src/shelf/strip.test.ts`
Expected: FAIL, module not found.

- [ ] **Step 3: Implement**

```ts
import { t } from '../lib/i18n';

export type StripState = 'hidden' | 'resting' | 'open' | 'target' | 'cleared';

/** Mirrors the Rust `shelf:drag` event. */
export interface DragView {
  /** A drag the shelf can take (files, from any app or from Ferry's cards). */
  offer: boolean;
  /** The pointer is within reach of the strip, and not over the drop zone. */
  near: boolean;
  /** The shelf's own pile (or a Show items row) is being dragged out. */
  own: boolean;
}

export interface StripInput {
  count: number;
  canUndo: boolean;
  hover: boolean;
  drag: DragView;
}

export const NO_DRAG: DragView = { offer: false, near: false, own: false };
export const STRIP = 27;
export const EXPANDED = 170;
export const COLLAPSE_MS = 300;
export const UNDO_MS = 5000;

export function stripState({ count, canUndo, hover, drag }: StripInput): StripState {
  if (drag.own) return 'open';
  if (drag.offer && drag.near) return 'target';
  if (count === 0 && canUndo) return 'cleared';
  if (count > 0 && hover) return 'open';
  if (count > 0 || drag.offer) return 'resting';
  return 'hidden';
}

export function stripHeight(state: StripState): number {
  if (state === 'hidden') return 0;
  if (state === 'open' || state === 'target') return EXPANDED;
  return STRIP;
}

export function countLabel(count: number): string {
  return count === 1 ? t('shelf.item') : t('shelf.items', { count });
}
```

Add the five i18n keys to `en.json` next to `share.menu.shelf`. Keep the file's existing key order and style.

- [ ] **Step 4: Run the tests**

Run: `node_modules/.bin/vitest run src/shelf/strip.test.ts src/lib/i18n.test.ts`
Expected: all pass. `i18n.test.ts` may check that every key is used or typed. If it fails on the new keys, follow its message (`StringKey` is derived from `en.json`).

- [ ] **Step 5: Commit**

```bash
git add src/shelf/strip.ts src/shelf/strip.test.ts src/lib/i18n/en.json
git commit -m "feat(shelf): the strip's states

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The shelf window, its state, and drops onto it

The window, the page, persistence, hover open/close and Finder drops. No drag watcher yet: in this task the shelf expands on hover only.

**Files:**
- Create:
  - `src-tauri/src/shelf/mod.rs` (state + init)
  - `src-tauri/src/shelf/window.rs` (create, place, layout)
  - `src-tauri/src/shelf/commands.rs`
- Create: `src/shelf.html`, `src/shelf/main.ts`, `src/shelf/Shelf.svelte`
- Modify:
  - `src-tauri/src/lib.rs` (`mod shelf;`, register commands in `generate_handler!`, call `shelf::init(app)?` right after `drop_batch::init(app)?`)
  - `src-tauri/src/platform/macos_drop.rs` (`pub use` for a `make_shelf_window` that reuses the zone panel setup)
  - `vite.config.ts` (input `shelf: page('shelf.html')`)
  - `src-tauri/capabilities/default.json` (add `"shelf"` to `windows`)
  - `src/lib/i18n/en.json` (`"shelf.window_title": "Ferry shelf"`, `"shelf.error.add": "Couldn't add {count} item(s)"`, `"shelf.error.save": "Couldn't save the shelf"`)

**Interfaces:**
- Consumes: `ferry_core::shelf::{Shelf, Added}` (Task 1), `ferry_core::shelf_place::*` (Task 2), `strip.ts` (Task 3).
- Produces:
  - `shelf::SHELF: &str = "shelf"` (window label)
  - `shelf::ShelfState`:
    - `{ list: Mutex<Shelf>, path: PathBuf, dragging: AtomicBool, layout: Mutex<Layout>, warned: AtomicBool }`
    - `Layout { height_pt: f64, rect: Option<Rect> }` (the shelf window's last physical rect)
  - `shelf::view(app) -> ShelfView`, where `ShelfView { items: Vec<Entry>, size: u64, can_undo: bool }` and `Entry { path: String, name: String, ext: String, is_dir: bool }` (serde, `snake_case`)
  - `shelf::add(app, paths: Vec<PathBuf>)`: adds, saves, emits `shelf:changed`, and on failures shows the notice
  - `shelf::mutate(app, f: impl FnOnce(&mut Shelf))`: lock, run, save, emit
  - `shelf::inset_px(app, scale: f64) -> u32` (for `stack_resize`, Task 5)
  - Commands:
    - `shelf_view() -> ShelfView`
    - `shelf_add(paths: Vec<String>)`
    - `shelf_remove(paths: Vec<String>)`
    - `shelf_layout(height: f64)` (points; 0 hides)
  - Event `shelf:changed` with payload `ShelfView`

- [ ] **Step 1: State and persistence (`shelf/mod.rs`)**

```rust
//! The built-in shelf (shelf spec): a panel on the bottom edge that holds files
//! by reference. macOS only; `init` does nothing elsewhere.

pub mod commands;
pub mod window;

use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use ferry_core::geometry::Rect;
use ferry_core::i18n::{t, tf};
use ferry_core::shelf::{LoadStatus, Shelf};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use crate::jobs;

pub const SHELF: &str = "shelf";

#[derive(Debug, Default, Clone, Copy)]
pub struct Layout {
    /// The page's last reported height in points (0 = hidden).
    pub height_pt: f64,
    /// The window's physical rect while shown.
    pub rect: Option<Rect>,
}

pub struct ShelfState {
    pub list: Mutex<Shelf>,
    pub path: PathBuf,
    /// The pile is being dragged out (spec §3 "Shelf drag flag").
    pub dragging: AtomicBool,
    pub layout: Mutex<Layout>,
    /// "Couldn't save the shelf" shows once per session.
    warned: AtomicBool,
}

#[derive(Debug, Clone, Serialize)]
pub struct Entry {
    pub path: String,
    pub name: String,
    pub ext: String,
    pub is_dir: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct ShelfView {
    pub items: Vec<Entry>,
    pub size: u64,
    pub can_undo: bool,
}

pub fn init(app: &AppHandle) -> tauri::Result<()> {
    if !cfg!(target_os = "macos") {
        return Ok(());
    }
    let path = app.path().app_data_dir()?.join("shelf.json");
    let (mut list, status) = Shelf::load(&path);
    list.prune();
    app.manage(ShelfState {
        list: Mutex::new(list),
        path,
        dragging: AtomicBool::new(false),
        layout: Mutex::new(Layout::default()),
        warned: AtomicBool::new(false),
    });
    if let LoadStatus::Recovered { .. } = status {
        jobs::notice(app, t("shelf.error.reset"));
    }
    window::create(app)?;
    Ok(())
}

pub fn state(app: &AppHandle) -> Option<tauri::State<'_, ShelfState>> {
    app.try_state::<ShelfState>()
}

fn now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

pub fn view(app: &AppHandle) -> ShelfView {
    let Some(state) = state(app) else {
        return ShelfView { items: vec![], size: 0, can_undo: false };
    };
    let list = state.list.lock().unwrap();
    let items = list
        .items
        .iter()
        .map(|i| Entry {
            path: i.path.to_string_lossy().into_owned(),
            name: i.path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned()),
            ext: i.path.extension().map_or_else(String::new, |e| e.to_string_lossy().to_lowercase()),
            is_dir: i.path.is_dir(),
        })
        .collect();
    let size = list
        .items
        .iter()
        .filter_map(|i| std::fs::metadata(&i.path).ok())
        .filter(|m| m.is_file())
        .map(|m| m.len())
        .sum();
    ShelfView { items, size, can_undo: list.can_undo() }
}

/// Change the list, save it, tell the page.
pub fn mutate<R>(app: &AppHandle, f: impl FnOnce(&mut Shelf) -> R) -> Option<R> {
    let state = state(app)?;
    let out = {
        let mut list = state.list.lock().unwrap();
        let out = f(&mut list);
        if list.save(&state.path).is_err() && !state.warned.swap(true, Ordering::SeqCst) {
            jobs::notice(app, t("shelf.error.save"));
        }
        out
    };
    let _ = app.emit_to(SHELF, "shelf:changed", view(app));
    Some(out)
}

pub fn add(app: &AppHandle, paths: Vec<PathBuf>) {
    let Some(added) = mutate(app, |list| list.add(&paths, now())) else {
        return;
    };
    if added.failed > 0 {
        let count = added.failed.to_string();
        jobs::notice(app, tf("shelf.error.add", &[("count", &count)]));
    }
}
```

Add `"shelf.error.reset": "The shelf's file couldn't be read, so it starts empty"` to `en.json` as well.

- [ ] **Step 2: The window (`shelf/window.rs`)**

Model it on `drop_batch/zone.rs::create` and `apply_geometry`:

```rust
//! The shelf panel: a non-activating NSPanel the size of what the page shows,
//! on the bottom edge of the stack's monitor (spec §4).

use ferry_core::geometry::{Corner, Rect};
use ferry_core::i18n::t;
use ferry_core::settings;
use ferry_core::shelf_place::{self, STRIP, WIDTH};
use tauri::{AppHandle, DragDropEvent, Manager, Monitor, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use super::{SHELF, state};
use crate::platform;
use crate::state::AppState;

/// Matches the stack's side margin (`windows.rs` STACK_MARGIN).
const MARGIN: f64 = 4.0;

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    let show_in_screenshots = app.state::<AppState>().settings.read().unwrap().general.show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, SHELF, WebviewUrl::App("shelf.html".into()))
        .title(t("shelf.window_title"))
        .inner_size(WIDTH, STRIP)
        .transparent(true)
        .decorations(false)
        .shadow(false)
        .resizable(false)
        .always_on_top(true)
        .skip_taskbar(true)
        .visible_on_all_workspaces(true)
        .focused(false)
        .focusable(false)
        .content_protected(!show_in_screenshots)
        .visible(false)
        .build()?;
    platform::drop::make_shelf_window(&window)?;
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. }) = event {
            on_drop(&handle, paths.clone());
        }
    });
    Ok(())
}

fn on_drop(app: &AppHandle, paths: Vec<std::path::PathBuf>) {
    if state(app).is_some_and(|s| s.dragging.load(std::sync::atomic::Ordering::SeqCst)) {
        return; // our own pile, dropped back on ourselves (spec §3)
    }
    if paths.is_empty() {
        return; // an image-only drag: spec 3
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || super::add(&app, paths));
}

pub fn corner(app: &AppHandle) -> Corner {
    match app.state::<AppState>().settings.read().unwrap().floating.corner {
        settings::Corner::TopLeft => Corner::TopLeft,
        settings::Corner::TopRight => Corner::TopRight,
        settings::Corner::BottomLeft => Corner::BottomLeft,
        settings::Corner::BottomRight => Corner::BottomRight,
    }
}

/// The stack's monitor when the stack shows, else the one the stack would pick.
pub fn monitor(app: &AppHandle) -> Option<Monitor> {
    let stack = app
        .get_webview_window(crate::windows::STACK)
        .filter(|w| w.is_visible().unwrap_or(false))
        .and_then(|w| w.current_monitor().ok().flatten());
    stack.or_else(|| crate::windows::stack_monitor(app))
}

pub fn area_of(monitor: &Monitor) -> Rect {
    let work = monitor.work_area();
    Rect { x: work.position.x, y: work.position.y, width: work.size.width, height: work.size.height }
}

/// Size the panel to `height_pt` points (0 hides it) and pin it to the bottom edge.
pub fn layout(app: &AppHandle, height_pt: f64) {
    let Some(state) = state(app) else { return };
    let Some(window) = app.get_webview_window(SHELF) else { return };
    let Some(monitor) = monitor(app) else { return };
    let scale = monitor.scale_factor();
    if height_pt < 1.0 {
        *state.layout.lock().unwrap() = super::Layout { height_pt: 0.0, rect: None };
        let _ = platform::hide(&window);
        return;
    }
    let size = ((WIDTH * scale).round() as u32, (height_pt * scale).round() as u32);
    let margin = (MARGIN * scale).round() as u32;
    let rect = shelf_place::shelf_rect(area_of(&monitor), corner(app), size, margin);
    *state.layout.lock().unwrap() = super::Layout { height_pt, rect: Some(rect) };
    use tauri::{LogicalPosition, LogicalSize};
    let _ = window.set_size(LogicalSize::new(WIDTH, height_pt));
    let _ = window.set_position(LogicalPosition::new(f64::from(rect.x) / scale, f64::from(rect.y) / scale));
    let _ = platform::show_without_focus(&window);
}
```

In `platform/macos_drop.rs`, add next to `make_zone_window`:

```rust
/// The shelf is the same kind of panel as the zone.
pub fn make_shelf_window(window: &WebviewWindow) -> tauri::Result<()> {
    make_zone_window(window)
}
```

Add the same function to `platform/windows_drop.rs`, returning `Ok(())`, so the shared call site compiles. It is never called there, because `init` returns early.

In `windows.rs`:
- Make `STACK` `pub` if it isn't.
- Extract the monitor lookup in `stack_resize` (lines 102–122) into `pub fn stack_monitor(app: &AppHandle) -> Option<Monitor>`, and use it in `stack_resize` (`stack_monitor(&app).ok_or("no monitor found")?`).
- No behaviour change to the stack in this task.

- [ ] **Step 3: Commands (`shelf/commands.rs`)**

```rust
use std::path::PathBuf;

use tauri::AppHandle;

use super::{ShelfView, view, window};

#[tauri::command]
pub fn shelf_view(app: AppHandle) -> ShelfView {
    super::mutate(&app, |list| list.prune());
    view(&app)
}

#[tauri::command]
pub async fn shelf_add(app: AppHandle, paths: Vec<String>) {
    let paths = paths.into_iter().map(PathBuf::from).collect();
    let _ = tauri::async_runtime::spawn_blocking(move || super::add(&app, paths)).await;
}

#[tauri::command]
pub fn shelf_remove(app: AppHandle, paths: Vec<String>) {
    let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
    super::mutate(&app, |list| list.remove(&paths));
}

/// The page's height in points; 0 hides the panel.
#[tauri::command]
pub fn shelf_layout(app: AppHandle, height: f64) {
    let handle = app.clone();
    let _ = app.run_on_main_thread(move || window::layout(&handle, height));
}
```

Register the four in `lib.rs`'s `generate_handler!` as `shelf::commands::shelf_view` and so on.

- [ ] **Step 4: The page**

`src/shelf.html`: copy `src/drop.html`, with title `Ferry shelf` and script `/shelf/main.ts`.

`src/shelf/main.ts`: copy `src/drop/main.ts`, mounting `Shelf.svelte`.

`src/shelf/Shelf.svelte`:

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { t } from '../lib/i18n';
  import { COLLAPSE_MS, NO_DRAG, countLabel, stripHeight, stripState, type DragView } from './strip';

  interface Entry { path: string; name: string; ext: string; is_dir: boolean }
  interface ShelfView { items: Entry[]; size: number; can_undo: boolean }

  let view = $state<ShelfView>({ items: [], size: 0, can_undo: false });
  let hover = $state(false);
  let drag = $state<DragView>(NO_DRAG);
  let leaveTimer: ReturnType<typeof setTimeout> | undefined;

  const state = $derived(stripState({ count: view.items.length, canUndo: view.can_undo, hover, drag }));
  const height = $derived(stripHeight(state));
  const pile = $derived(view.items.slice(-3));

  $effect(() => {
    void invoke('shelf_layout', { height });
  });

  $effect(() => {
    const offs = [
      listen<ShelfView>('shelf:changed', (e) => (view = e.payload)),
      listen<DragView>('shelf:drag', (e) => (drag = e.payload)),
    ];
    void invoke<ShelfView>('shelf_view').then((v) => (view = v));
    return () => offs.forEach((off) => off.then((fn) => fn()));
  });

  function enter() {
    clearTimeout(leaveTimer);
    hover = true;
  }
  function leave() {
    clearTimeout(leaveTimer);
    leaveTimer = setTimeout(() => (hover = false), COLLAPSE_MS);
  }
</script>

<div
  class="shelf"
  class:open={state === 'open' || state === 'target'}
  class:target={state === 'target'}
  role="region"
  aria-label={t('shelf.window_title')}
  onmouseenter={enter}
  onmouseleave={leave}
>
  <div class="grip"></div>
  {#if state === 'resting' && view.items.length > 0}
    <span class="count">{view.items.length}</span>
  {/if}
  {#if state === 'cleared'}
    <span class="note">{t('shelf.cleared')}</span>
  {/if}
  {#if state === 'target'}
    <span class="drop">{t('shelf.drop')}</span>
  {:else if state === 'open'}
    <div class="pile">
      {#each pile as item, i (item.path)}
        <div class="doc" style:transform={`rotate(${(i - (pile.length - 1) / 2) * 7}deg)`}>
          <span>{item.is_dir ? '📁' : item.ext}</span>
        </div>
      {/each}
    </div>
    <span class="pill">{countLabel(view.items.length)} ⌄</span>
  {/if}
</div>

<style>
  :global(html), :global(body) { margin: 0; background: transparent; overflow: hidden; }
  .shelf {
    position: fixed; inset: 0;
    background: #3a3d44; border: 1px solid #555963; border-bottom: 0;
    border-radius: 14px 14px 0 0; box-sizing: border-box; color: #fff;
    font: 12px -apple-system, system-ui, sans-serif; user-select: none;
  }
  .shelf.target { outline: 2px solid var(--accent, #0a84ff); outline-offset: -3px; }
  .grip { position: absolute; left: 50%; top: 6px; transform: translateX(-50%); width: 22px; height: 3px; border-radius: 2px; background: rgba(255, 255, 255, 0.75); }
  .count { position: absolute; right: 10px; top: 4px; font-size: 10px; font-weight: 700; }
  .note { position: absolute; left: 10px; top: 6px; font-size: 11px; }
  .drop { position: absolute; inset: 0; display: flex; align-items: center; justify-content: center; font-weight: 600; }
  .pile { position: absolute; left: 50%; top: 26px; width: 62px; height: 78px; transform: translateX(-50%); }
  .doc { position: absolute; inset: 0; background: #fff; border-radius: 3px; box-shadow: 0 1px 3px rgba(0, 0, 0, 0.4); display: flex; align-items: flex-end; justify-content: center; color: #555; font-size: 10px; text-transform: uppercase; padding-bottom: 6px; box-sizing: border-box; }
  .pill { position: absolute; left: 50%; bottom: 12px; transform: translateX(-50%); background: rgba(0, 0, 0, 0.45); border-radius: 12px; padding: 3px 10px; white-space: nowrap; cursor: default; }
</style>
```

Add `shelf: page('shelf.html')` to `vite.config.ts` inputs, and add `"shelf"` to `capabilities/default.json` `windows`.

- [ ] **Step 5: Build and check**

```bash
cargo clippy -p ferry --all-targets -- -D warnings
node_modules/.bin/svelte-check --threshold error
node_modules/.bin/vitest run
```
Expected: clean.

- [ ] **Step 6: Manual check in the dev app**

1. Quit the installed Ferry.app.
2. Run `node_modules/.bin/tauri dev` in the background.
3. The shelf is empty, so no strip shows.
4. Run `osascript -e 'tell application "Finder" to activate'`, drag a Finder file to the screen's bottom-right corner, and drop it about 10 px above the screen edge. Nothing shows yet, so instead add an item by calling the command from the stack page's devtools: `__TAURI__.core.invoke('shelf_add', {paths: ['/path/to/file.png']})`. If devtools aren't available, write a `shelf.json` with one item to `~/Library/Application Support/io.ferry.desktop/` before launch.
5. Check that the strip appears with count "1" at the bottom-right: `screencapture -x -R1240,850,200,50 /tmp/claude-501/shelf1.png`.
6. Hover the strip with `mouse move`. It expands with the pile and "1 Item ⌄". Move away; after 300 ms it rests again.
7. With it expanded by hover, drag a Finder file onto it and drop. The count becomes 2. Relaunch the dev app: the count is still 2.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/shelf src-tauri/src/lib.rs src-tauri/src/windows.rs src-tauri/src/platform src/shelf.html src/shelf vite.config.ts src-tauri/capabilities/default.json src/lib/i18n/en.json
git commit -m "feat(shelf): the shelf panel, its file, and drops onto it

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: The stack and the zone make room

**Files:**
- Modify:
  - `src-tauri/src/windows.rs` (`stack_resize` uses `shelf_place::stack_area`; remembers the last size; new `pub fn restack(app)`)
  - `src-tauri/src/shelf/window.rs` (`layout` calls `restack` when the inset changed)
  - `src-tauri/src/shelf/mod.rs` (`inset_px`)
  - `src-tauri/src/drop_batch/zone.rs` (`place` keeps the zone clear of the expanded shelf)

**Interfaces:**
- Consumes:
  - `shelf_place::{inset, stack_area, shelf_rect, clear_of, EXPANDED, STRIP, WIDTH}`
  - `shelf::{state, Layout}`
  - `shelf::window::{corner, area_of}`
- Produces:
  - `windows::restack(app: &AppHandle)`
  - `shelf::inset_px(app: &AppHandle, scale: f64) -> u32`
  - `shelf::offering(app) -> bool` (a drag the shelf takes is in progress; set in Task 6, read here). In this task, add `pub offering: AtomicBool` to `ShelfState`, and `offering: AtomicBool::new(false)` to the struct literal in `shelf::init`.

- [ ] **Step 1: `inset_px` in `shelf/mod.rs`**

```rust
/// What the shelf takes from the bottom of the stack's area, in physical pixels.
pub fn inset_px(app: &AppHandle, scale: f64) -> u32 {
    let Some(state) = state(app) else { return 0 };
    let has_items = !state.list.lock().unwrap().items.is_empty();
    let height = state.layout.lock().unwrap().height_pt;
    let px = |pt: f64| (pt * scale).round() as u32;
    ferry_core::shelf_place::inset(has_items, px(height), px(ferry_core::shelf_place::STRIP))
}
```

- [ ] **Step 2: `stack_resize` and `restack` in `windows.rs`**

- At the top of `stack_resize`, after the zero-size early return, store `(width, height)` in `static LAST: Mutex<Option<(f64, f64)>> = Mutex::new(None);`. On the zero-size path, store `None`.
- After `let area = Rect { .. }`, add:

```rust
    let inset = crate::shelf::inset_px(&app, scale);
    let area = ferry_core::shelf_place::stack_area(area, corner, inset);
```

- Add:

```rust
/// Place the stack again at its last size, after the shelf grew or shrank.
pub fn restack(app: &AppHandle) {
    let last = *LAST.lock().unwrap();
    if let Some((width, height)) = last {
        let _ = stack_resize(app.clone(), width, height);
    }
}
```

`stack_resize` is a sync command, so calling it directly is fine. It sets window geometry, so only call `restack` from the main thread: `window::layout` already runs there.

- [ ] **Step 3: `layout` restacks on change**

In `shelf/window.rs::layout`, read `inset_px` before and after updating `state.layout`. If it changed, call `crate::windows::restack(app)`. Do this on both the hide path and the show path.

- [ ] **Step 4: The zone keeps clear (`zone.rs::place`)**

At the end of `place`, before `apply_geometry`, add:

```rust
    let (x, y) = clear_of_shelf(app, area, corner, scale, size, (x, y));
```

The cleared `y` from `zone_near_results` matters only when the stack is pushed. Pass it the area minus the expanded height for bottom corners when the shelf offers this drag:

```rust
        let room = if crate::shelf::offering(app) {
            let expanded = (ferry_core::shelf_place::EXPANDED * scale).round() as u32;
            ferry_core::shelf_place::stack_area(area, corner, expanded)
        } else {
            area
        };
        zone_near_results(room, corner, size, stack, margin, 0)
```

Then:

```rust
/// While the shelf offers this drag, no zone placement may overlap its expanded rect (spec §4).
fn clear_of_shelf(app: &AppHandle, area: Rect, corner: Corner, scale: f64, size: (u32, u32), at: (i32, i32)) -> (i32, i32) {
    use ferry_core::shelf_place::{EXPANDED, WIDTH, clear_of, shelf_rect};
    if !crate::shelf::offering(app) {
        return at;
    }
    let px = |pt: f64| (pt * scale).round() as u32;
    let shelf = shelf_rect(area, corner, (px(WIDTH), px(EXPANDED)), px(4.0));
    clear_of(Rect { x: at.0, y: at.1, width: size.0, height: size.1 }, shelf, area)
}
```

Read the real names of `area`, `corner` and `scale` in `zone.rs::place` (lines 246–305) and match them. `Rect` and `Corner` there are `ferry_core::geometry`'s.

Add `offering` to `shelf/mod.rs`:

```rust
pub fn offering(app: &AppHandle) -> bool {
    state(app).is_some_and(|s| s.offering.load(Ordering::SeqCst))
}
```

- [ ] **Step 5: Build, test, check**

```bash
cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry
```
Expected: clean, with existing tests still passing.

Manual check:
1. With one shelf item and a result card showing (copy an image in Finder to make a card), the card's bottom sits on top of the strip.
2. Hover the strip. The stack moves up by the expanded height, and moves back 300 ms after leaving.
3. Screenshot both states.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/windows.rs src-tauri/src/shelf src-tauri/src/drop_batch/zone.rs
git commit -m "feat(shelf): the stack sits on the shelf and the zone keeps clear of it

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: The drag watcher: show, expand, and the shelf's own drag

**Files:**
- Modify:
  - `src-tauri/src/drop_batch/watch.rs` (`ShelfPlan`, `shelf_plan`, a per-tick `track`, and hiding the zone during a shelf drag)
  - `src-tauri/src/shelf/mod.rs` (`on_drag`, `track`, `drag_start`, `drag_finished`, `cursor_over_ferry`)
  - `src-tauri/src/shelf/commands.rs` (`shelf_drag_start`, `shelf_drag_finished`)
  - `src-tauri/src/drop_batch/zone.rs` (`on_drag_drop` ignores the drop while `shelf.dragging`)
  - `src-tauri/src/lib.rs` (register the two commands)
  - `src/shelf/Shelf.svelte` (pile drag-out)

**Interfaces:**
- Consumes: `shelf_place::{drag_near, may_expand, contains, shelf_rect, REACH, STRIP, EXPANDED, WIDTH}`, `ShelfState.{dragging, offering, layout}`.
- Produces:
  - In `watch.rs`: `enum ShelfPlan { Offer, Hold, Nothing }` and `fn shelf_plan(signal: Signal, ctx: &Context) -> ShelfPlan`
  - `Context.shelf_dragging: bool`
  - `shelf::on_drag(app, ShelfPlan-equivalent: offer: bool, own: bool)`
  - `shelf::drag_ended(app)`
  - `shelf::track(app)`
  - `shelf::cursor_over_ferry(cursor_pt: (f64, f64), windows: &[Rect in points]) -> bool` (pure, tested)
  - Event `shelf:drag` with payload `{ offer: bool, near: bool, own: bool }`
  - Commands `shelf_drag_start()` and `shelf_drag_finished(paths: Vec<String>, dropped: bool, keep: bool)`

- [ ] **Step 1: Failing `watch.rs` tests, one per row of spec §3**

Add to `watch.rs` tests. Update `fn ctx` to set `shelf_dragging: false`.

```rust
    #[test]
    fn a_file_drag_from_another_app_offers_the_shelf() {
        let (zone, files) = (any_drag(), [PathBuf::from("/a.png")]);
        assert_eq!(shelf_plan(Signal::DragStarted, &ctx(&zone, &files)), ShelfPlan::Offer);
    }

    #[test]
    fn ferrys_own_cards_offer_the_shelf_though_the_zone_hides() {
        let (zone, files) = (any_drag(), [PathBuf::from("/out/a.png")]);
        let own = Context { all_own: true, ..ctx(&zone, &files) };
        assert_eq!(plan(Signal::DragStarted, &own), Plan::Nothing);
        assert_eq!(shelf_plan(Signal::DragStarted, &own), ShelfPlan::Offer);
    }

    #[test]
    fn the_shelfs_own_pile_hides_the_zone_and_offers_no_target() {
        let (zone, files) = (any_drag(), [PathBuf::from("/a.png")]);
        let pile = Context { shelf_dragging: true, ..ctx(&zone, &files) };
        assert_eq!(plan(Signal::DragStarted, &pile), Plan::Nothing);
        assert_eq!(shelf_plan(Signal::DragStarted, &pile), ShelfPlan::Hold);
    }

    #[test]
    fn an_image_only_drag_leaves_the_shelf_alone() {
        let zone = any_drag();
        let image = Context { images: 1, ..ctx(&zone, &[]) };
        assert_eq!(plan(Signal::DragStarted, &image), SHOW_ZONE);
        assert_eq!(shelf_plan(Signal::DragStarted, &image), ShelfPlan::Nothing);
    }

    #[test]
    fn an_ignored_app_leaves_the_shelf_alone() {
        let (zone, files) = (any_drag(), [PathBuf::from("/a.png")]);
        let figma = Context { frontmost: Some("com.figma.Desktop"), ..ctx(&zone, &files) };
        assert_eq!(shelf_plan(Signal::DragStarted, &figma), ShelfPlan::Nothing);
    }

    #[test]
    fn the_shelf_offers_even_when_the_zone_is_turned_off() {
        let zone = DropZone { enabled: false, ..any_drag() };
        let files = [PathBuf::from("/a.png")];
        assert_eq!(shelf_plan(Signal::DragStarted, &ctx(&zone, &files)), ShelfPlan::Offer);
    }
```

In `shelf/mod.rs` tests:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_drop_over_any_ferry_window_counts_as_cancelled() {
        let windows = [(100.0, 100.0, 300.0, 200.0), (1286.0, 873.0, 150.0, 27.0)];
        assert!(cursor_over_ferry((150.0, 150.0), &windows));
        assert!(cursor_over_ferry((1300.0, 880.0), &windows));
        assert!(!cursor_over_ferry((700.0, 500.0), &windows));
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry watch:: shelf::`
Expected: compile errors.

- [ ] **Step 3: Implement the watcher side**

In `watch.rs`:

```rust
#[derive(Debug, PartialEq)]
enum ShelfPlan {
    /// Show the strip; expand it as a drop target when the drag comes near.
    Offer,
    /// The shelf's own pile: stay open, no drop target.
    Hold,
    Nothing,
}

fn shelf_plan(signal: Signal, ctx: &Context) -> ShelfPlan {
    if signal != Signal::DragStarted {
        return ShelfPlan::Nothing;
    }
    if ctx.shelf_dragging {
        return ShelfPlan::Hold;
    }
    let ignored = ctx
        .frontmost
        .is_some_and(|a| ferry_core::drag::is_ignored_app(&ctx.zone.ignored_apps, a));
    if ctx.files.is_empty() || ignored {
        ShelfPlan::Nothing
    } else {
        ShelfPlan::Offer
    }
}
```

At the start of `plan()`:

```rust
    if ctx.shelf_dragging && signal == Signal::DragStarted {
        return Plan::Nothing;
    }
```

Add `shelf_dragging: bool` to `Context`. In `handle`, set it with `crate::shelf::state(app).is_some_and(|s| s.dragging.load(Ordering::SeqCst))`. After `match plan(...)`, add:

```rust
    match shelf_plan(signal, &ctx) {
        ShelfPlan::Offer => crate::shelf::on_drag(app, true, false),
        ShelfPlan::Hold => crate::shelf::on_drag(app, false, true),
        ShelfPlan::Nothing => {}
    }
    if signal == Signal::DragEnded {
        crate::shelf::drag_ended(app);
    }
```

`ctx` borrows `files`, so call `shelf_plan` before the match moves `files`. If the borrow checker complains, compute `let shelf = shelf_plan(signal, &ctx);` before `match plan(...)`.

In `run()`, after the `for signal` loop:

```rust
        if watch.active() {
            crate::shelf::track(app);
        }
```

- [ ] **Step 4: Implement the shelf side (`shelf/mod.rs`)**

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize)]
pub struct DragView {
    pub offer: bool,
    pub near: bool,
    pub own: bool,
}

static DRAG: Mutex<DragView> = Mutex::new(DragView { offer: false, near: false, own: false });

fn emit_drag(app: &AppHandle, next: DragView) {
    let changed = {
        let mut cur = DRAG.lock().unwrap();
        let changed = *cur != next;
        *cur = next;
        changed
    };
    if changed {
        let _ = app.emit_to(SHELF, "shelf:drag", next);
    }
}

pub fn on_drag(app: &AppHandle, offer: bool, own: bool) {
    let Some(state) = state(app) else { return };
    state.offering.store(offer, Ordering::SeqCst);
    if offer {
        mutate(app, |list| list.prune());
    }
    emit_drag(app, DragView { offer, near: false, own });
}

pub fn drag_ended(app: &AppHandle) {
    if let Some(state) = state(app) {
        state.offering.store(false, Ordering::SeqCst);
    }
    emit_drag(app, DragView::default());
}

/// Each watcher tick during a drag: is the pointer near the strip?
pub fn track(app: &AppHandle) {
    let cur = *DRAG.lock().unwrap();
    if !cur.offer {
        return;
    }
    let near = near_strip(app).unwrap_or(false);
    emit_drag(app, DragView { near, ..cur });
}

fn near_strip(app: &AppHandle) -> Option<bool> {
    use ferry_core::shelf_place::{EXPANDED, REACH, STRIP, WIDTH, drag_near, may_expand, shelf_rect};
    let monitor = window::monitor(app)?;
    let scale = monitor.scale_factor();
    let px = |pt: f64| (pt * scale).round() as u32;
    let area = window::area_of(&monitor);
    let corner = window::corner(app);
    let strip = shelf_rect(area, corner, (px(WIDTH), px(STRIP)), px(4.0));
    let expanded = shelf_rect(area, corner, (px(WIDTH), px(EXPANDED)), px(4.0));
    let zone = crate::drop_batch::zone::visible_rect(app);
    let cursor = crate::drop_batch::zone::cursor_px(app)?;
    Some(may_expand(zone, expanded) && drag_near(cursor, strip, px(REACH), zone))
}

/// Pure: is the point (points) inside any of the windows `(x, y, w, h)` (points)?
pub fn cursor_over_ferry(cursor: (f64, f64), windows: &[(f64, f64, f64, f64)]) -> bool {
    windows
        .iter()
        .any(|&(x, y, w, h)| cursor.0 >= x && cursor.0 < x + w && cursor.1 >= y && cursor.1 < y + h)
}
```

In `drop_batch/zone.rs`, add the two helpers `near_strip` needs. They reuse `cursor_monitor`'s main-scale conversion:
- `pub fn cursor_px(app) -> Option<(i32, i32)>` returns `app.cursor_position()` as physical `i32`. Tao's macOS cursor is points × the main scale. That matches the `work_area` physical coordinates on a single-scale setup; on mixed-DPI setups, divide by the main scale and multiply by the monitor's scale, as `cursor_monitor` hints.
- `pub fn visible_rect(app) -> Option<Rect>` returns the zone window's `outer_position` and `outer_size` when it is visible.

In `zone.rs::on_drag_drop`, at the top:

```rust
    if crate::shelf::state(app).is_some_and(|s| s.dragging.load(std::sync::atomic::Ordering::SeqCst)) {
        return; // the shelf's own pile: no intake (spec §3)
    }
```

- [ ] **Step 5: The drag commands (`shelf/commands.rs`)**

```rust
#[tauri::command]
pub fn shelf_drag_start(app: AppHandle) {
    if let Some(state) = super::state(&app) {
        state.dragging.store(true, std::sync::atomic::Ordering::SeqCst);
    }
    super::mutate(&app, |list| list.prune());
}

/// The pile's drag ended. `Dropped` removes the items unless ⌥ was held or the
/// drop landed on a Ferry window. The flag stays up for 1 s so the window's own
/// Drop event, whichever order it arrives in, is ignored.
#[tauri::command]
pub fn shelf_drag_finished(app: AppHandle, paths: Vec<String>, dropped: bool, keep: bool) {
    if dropped && !keep && !over_ferry(&app) {
        let paths: Vec<PathBuf> = paths.into_iter().map(PathBuf::from).collect();
        super::mutate(&app, |list| list.remove(&paths));
    }
    let handle = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(1));
        if let Some(state) = super::state(&handle) {
            state.dragging.store(false, std::sync::atomic::Ordering::SeqCst);
        }
    });
}

fn over_ferry(app: &AppHandle) -> bool {
    use tauri::Manager;
    let Ok(cursor) = app.cursor_position() else { return false };
    let main_scale = app.primary_monitor().ok().flatten().map_or(1.0, |m| m.scale_factor());
    let cursor = (cursor.x / main_scale, cursor.y / main_scale);
    let windows: Vec<(f64, f64, f64, f64)> = app
        .webview_windows()
        .values()
        .filter(|w| w.is_visible().unwrap_or(false))
        .filter_map(|w| {
            let scale = w.scale_factor().ok()?;
            let p = w.outer_position().ok()?.to_logical::<f64>(scale);
            let s = w.outer_size().ok()?.to_logical::<f64>(scale);
            Some((p.x, p.y, s.width, s.height))
        })
        .collect();
    super::cursor_over_ferry(cursor, &windows)
}
```

Register both commands in `lib.rs`.

- [ ] **Step 6: The pile drag in `Shelf.svelte`**

Add to the script:

```ts
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fanIcon } from '../lib/dragicon';

  const IMAGE = ['png', 'jpg', 'jpeg', 'webp', 'gif', 'heic', 'avif', 'tiff'];
  let icon = $state('');
  $effect(() => {
    const paths = view.items.filter((i) => IMAGE.includes(i.ext)).slice(-3).map((i) => i.path);
    let current = true;
    fanIcon(paths).then((url) => {
      if (current) icon = url;
    });
    return () => {
      current = false;
    };
  });

  async function dragPile(event: MouseEvent, paths = view.items.map((i) => i.path)) {
    if (event.button !== 0 || paths.length === 0) return;
    const keep = event.altKey;
    await invoke('shelf_drag_start');
    // A pile of folders or PDFs has no thumbnails: fall back to a 1×1 transparent PNG so the drag still starts.
    const blank = 'data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mNkYAAAAAYAAjCB0C8AAAAASUVORK5CYII=';
    startDrag({ item: paths, icon: icon || blank }, (result) => {
      void invoke('shelf_drag_finished', { paths, dropped: result.result === 'Dropped', keep });
    }).catch((e) => {
      console.warn('shelf drag', e);
      void invoke('shelf_drag_finished', { paths, dropped: false, keep: true });
    });
  }
```

Put `onmousedown={(e) => dragPile(e)}` on the `.pile` element. Before writing the callback, check the plugin's callback payload shape in `node_modules/@crabnebula/tauri-plugin-drag/dist-js/index.d.ts`. If it is `{ result: 'Dropped' | 'Cancelled', cursorPos }`, the code above is right; otherwise match it.

- [ ] **Step 7: Build, test, check**

```bash
cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry
node_modules/.bin/svelte-check --threshold error && node_modules/.bin/vitest run
```

Manual checks, driven by `mouse` and `screencapture`:
1. Drag a Finder file toward the bottom-right. The strip appears, even when the shelf is empty. Within 80 pt it expands to "Drop to shelve"; drop it and the count goes up.
2. Drag a result card from the stack onto the strip. It is added, and no optimise zone appears.
3. Drag the pile into a Finder window. The files appear there (as Finder copy/move semantics decide), and the shelf empties and hides.
4. Repeat with ⌥ held at mouse down. The shelf keeps the items.
5. Drag the pile and drop it on the stack. The items stay and no new card appears.
6. Drag the pile and press Esc. The items stay.
7. Delete a shelved file in Finder, then drag the pile. The drag carries only the remaining files.

- [ ] **Step 8: Commit**

```bash
git add src-tauri/src/drop_batch src-tauri/src/shelf src-tauri/src/lib.rs src/shelf
git commit -m "feat(shelf): show and expand during drags, drag the pile out

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: The menu: Show in Finder, Copy, Paste, Show items…, Clear shelf with Undo

**Files:**
- Create: `src-tauri/src/shelf/menu.rs`
- Modify:
  - `src-tauri/src/shelf/mod.rs` (`pub mod menu;`, call `menu::init(app)` in `init`)
  - `src-tauri/src/shelf/commands.rs` (`shelf_menu`, `shelf_undo`, `shelf_item_menu`)
  - `src-tauri/src/lib.rs` (register)
  - `src/shelf/Shelf.svelte` (pill click → menu, cleared note Undo, Show items list)
  - `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `shelf::{mutate, view, add}`, `platform::clipboard::{read, write_files}`, `AppState.clipboard` (`ClipboardLog::wrote`), `tauri_plugin_opener::reveal_items_in_dir`.
- Produces:
  - `menu::entries(count: usize, size: u64) -> Vec<Entry>` (pure, tested), where `enum Entry { Header(String), Reveal, Copy, Paste, OptimiseAll, ShowItems, Clear, Separator }`
  - Commands `shelf_menu(x: f64, y: f64)`, `shelf_undo()`, `shelf_item_menu(path: String, x: f64, y: f64)`
  - Event `shelf:show_items` (no payload)
- i18n:
  - `"shelf.menu.header": "{count} Items · {size}"`
  - `"shelf.menu.header_one": "1 Item · {size}"`
  - `"shelf.menu.reveal": "Show in Finder"`
  - `"shelf.menu.copy": "Copy"`
  - `"shelf.menu.paste": "Paste"`
  - `"shelf.menu.optimise_all": "Optimise all"`
  - `"shelf.menu.show_items": "Show items…"`
  - `"shelf.menu.clear": "Clear shelf"`
  - `"shelf.menu.remove": "Remove from shelf"`

- [ ] **Step 1: Failing test for the entries**

In `shelf/menu.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_full_shelf_lists_every_action_in_order() {
        let e = entries(8, 214_000_000);
        assert!(matches!(&e[0], Entry::Header(h) if h.starts_with("8 Items · ")));
        assert_eq!(
            &e[1..],
            &[
                Entry::Reveal,
                Entry::Separator,
                Entry::Copy,
                Entry::Paste,
                Entry::Separator,
                Entry::OptimiseAll,
                Entry::ShowItems,
                Entry::Separator,
                Entry::Clear,
            ]
        );
    }

    #[test]
    fn an_empty_shelf_offers_only_paste() {
        assert_eq!(entries(0, 0), vec![Entry::Paste]);
    }
}
```

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ferry shelf::menu`
Expected: compile error.

- [ ] **Step 3: Implement the menu**

Use `rail_menu.rs` as the pattern (prefixed ids, `init` with `on_menu_event`). For the size, use the project's existing byte formatter: grep for `fn format_bytes` or `fn human_size` in `crates/ferry-core/src` and use whichever exists. Only if none exists, use `format!("{:.1} MB", size as f64 / 1e6)`.

```rust
//! The shelf's native menu (spec §6), routed like `rail_menu.rs`.

use std::path::PathBuf;

use ferry_core::i18n::{t, tf};
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, Wry};

use super::{SHELF, mutate, state};
use crate::platform;
use crate::state::AppState;

const PREFIX: &str = "shelf:";
const REMOVE: &str = "shelf:remove:";

#[derive(Debug, Clone, PartialEq)]
pub enum Entry {
    Header(String),
    Reveal,
    Copy,
    Paste,
    OptimiseAll,
    ShowItems,
    Clear,
    Separator,
}

pub fn entries(count: usize, size: u64) -> Vec<Entry> {
    if count == 0 {
        return vec![Entry::Paste];
    }
    let size = super::format_size(size);
    let header = if count == 1 {
        tf("shelf.menu.header_one", &[("size", &size)])
    } else {
        tf("shelf.menu.header", &[("count", &count.to_string()), ("size", &size)])
    };
    vec![
        Entry::Header(header),
        Entry::Reveal,
        Entry::Separator,
        Entry::Copy,
        Entry::Paste,
        Entry::Separator,
        Entry::OptimiseAll,
        Entry::ShowItems,
        Entry::Separator,
        Entry::Clear,
    ]
}

fn id(entry: &Entry) -> &'static str {
    match entry {
        Entry::Header(_) => "shelf:header",
        Entry::Reveal => "shelf:reveal",
        Entry::Copy => "shelf:copy",
        Entry::Paste => "shelf:paste",
        Entry::OptimiseAll => "shelf:optimise_all",
        Entry::ShowItems => "shelf:show_items",
        Entry::Clear => "shelf:clear",
        Entry::Separator => "",
    }
}

fn label(entry: &Entry) -> String {
    match entry {
        Entry::Header(h) => h.clone(),
        Entry::Reveal => t("shelf.menu.reveal"),
        Entry::Copy => t("shelf.menu.copy"),
        Entry::Paste => t("shelf.menu.paste"),
        Entry::OptimiseAll => t("shelf.menu.optimise_all"),
        Entry::ShowItems => t("shelf.menu.show_items"),
        Entry::Clear => t("shelf.menu.clear"),
        Entry::Separator => String::new(),
    }
}

pub fn popup(app: &AppHandle, x: f64, y: f64) -> Result<(), String> {
    let view = super::view(app);
    let list = entries(view.items.len(), view.size);
    let mut owned: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    for entry in &list {
        let item: Box<dyn IsMenuItem<Wry>> = match entry {
            Entry::Separator => Box::new(PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?),
            other => Box::new(
                MenuItem::with_id(app, id(other), label(other), !matches!(other, Entry::Header(_)), None::<&str>)
                    .map_err(|e| e.to_string())?,
            ),
        };
        owned.push(item);
    }
    let refs: Vec<&dyn IsMenuItem<Wry>> = owned.iter().map(|b| &**b).collect();
    let menu = Menu::with_items(app, &refs).map_err(|e| e.to_string())?;
    let window = app.get_webview_window(SHELF).ok_or("no shelf window")?;
    window.popup_menu_at(&menu, LogicalPosition::new(x, y)).map_err(|e| e.to_string())
}

pub fn popup_item(app: &AppHandle, path: &str, x: f64, y: f64) -> Result<(), String> {
    let remove = MenuItem::with_id(app, format!("{REMOVE}{path}"), t("shelf.menu.remove"), true, None::<&str>)
        .map_err(|e| e.to_string())?;
    let menu = Menu::with_items(app, &[&remove]).map_err(|e| e.to_string())?;
    let window = app.get_webview_window(SHELF).ok_or("no shelf window")?;
    window.popup_menu_at(&menu, LogicalPosition::new(x, y)).map_err(|e| e.to_string())
}

pub fn init(app: &AppHandle) {
    app.on_menu_event(|app, event| {
        let id = event.id().as_ref();
        if !id.starts_with(PREFIX) {
            return;
        }
        let (app, id) = (app.clone(), id.to_string());
        // Menu events arrive on the main thread; file and clipboard work does not belong there.
        tauri::async_runtime::spawn_blocking(move || run(&app, &id));
    });
}

fn run(app: &AppHandle, id: &str) {
    if let Some(path) = id.strip_prefix(REMOVE) {
        mutate(app, |list| list.remove(&[PathBuf::from(path)]));
        return;
    }
    let paths = state(app).map(|s| s.list.lock().unwrap().paths()).unwrap_or_default();
    match id {
        "shelf:reveal" => {
            let _ = tauri_plugin_opener::reveal_items_in_dir(&paths);
        }
        "shelf:copy" => copy(app, paths),
        "shelf:paste" => {
            if let Ok(snap) = platform::clipboard::read() {
                super::add(app, snap.files);
            }
        }
        "shelf:optimise_all" => super::optimise_all(app),
        "shelf:show_items" => {
            let _ = app.emit_to(SHELF, "shelf:show_items", ());
        }
        "shelf:clear" => super::clear(app),
        _ => {}
    }
}

/// As `drop_batch/list.rs`: recorded as our own write so the watcher leaves it alone.
fn copy(app: &AppHandle, paths: Vec<PathBuf>) {
    let state = app.state::<AppState>();
    let mut log = state.clipboard.lock().unwrap();
    if let Ok(seq) = platform::clipboard::write_files(&paths) {
        log.wrote(seq, None, ferry_core::clipboard::ClipWrite::Files(paths));
    }
}
```

Fix the `ClipWrite` import path to match `drop_batch/list.rs`'s own `use` line. Make `Snapshot.files` reachable: if it is private, add `pub` to the field.

In `shelf/mod.rs`, add `format_size`, `clear` and a stub `optimise_all`. The stub must show the "Nothing here can be optimised" notice for now, and Task 8 replaces its body. Add `"shelf.error.nothing_to_optimise": "Nothing here can be optimised"` to `en.json`.

```rust
pub fn format_size(bytes: u64) -> String {
    // Replace with the project's byte formatter if one exists (see Task 7 Step 3).
    format!("{:.1} MB", bytes as f64 / 1e6)
}

/// "Clear shelf": empties the list, Undo for 5 s (spec §6).
pub fn clear(app: &AppHandle) {
    mutate(app, |list| list.clear());
    let app = app.clone();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(5));
        mutate(&app, |list| list.forget_undo());
    });
}

pub fn optimise_all(app: &AppHandle) {
    jobs::notice(app, t("shelf.error.nothing_to_optimise"));
}
```

A second Clear within 5 s would have its Undo cut short by the first timer. Guard against that with a generation counter: add `static CLEARS: AtomicU64`, increment it in `clear`, and in the thread call `forget_undo` only if the value is unchanged.

Commands:

```rust
#[tauri::command]
pub fn shelf_menu(app: AppHandle, x: f64, y: f64) -> Result<(), String> {
    super::mutate(&app, |list| list.prune());
    super::menu::popup(&app, x, y)
}

#[tauri::command]
pub fn shelf_item_menu(app: AppHandle, path: String, x: f64, y: f64) -> Result<(), String> {
    super::menu::popup_item(&app, &path, x, y)
}

#[tauri::command]
pub fn shelf_undo(app: AppHandle) {
    super::mutate(&app, |list| list.undo());
}
```

- [ ] **Step 4: The page**

In `Shelf.svelte`:
- Turn the pill into a `<button class="pill" onclick={openMenu}>`, where `openMenu = (e) => invoke('shelf_menu', { x: e.clientX, y: e.clientY })`.
- In the cleared note, add `<button class="undo" onclick={() => invoke('shelf_undo')}>{t('shelf.undo')}</button>`.
- Listen for `shelf:show_items` and set `listing = true`. While `listing` is set, the page shows a scrollable list in place of the pile. Each row shows the name, `onmousedown={(e) => dragPile(e, [item.path])}`, and `oncontextmenu={(e) => { e.preventDefault(); invoke('shelf_item_menu', { path: item.path, x: e.clientX, y: e.clientY }); }}`.
- `listing` counts as hover in `stripState`: pass `hover: hover || listing`. It resets when the pointer leaves (in `leave`'s timeout).

- [ ] **Step 5: Build, test, check**

```bash
cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry
node_modules/.bin/svelte-check --threshold error && node_modules/.bin/vitest run
```

Manual checks:
1. The menu shows the header, sizes and order.
2. Show in Finder selects every item.
3. Copy, then paste in a Finder window: the files are copied, and no job card appears.
4. Paste with files on the clipboard adds them.
5. Show items… lists the items. Drag one row out: only that item leaves. Right-click → Remove.
6. Clear shelf shows "Shelf cleared · Undo"; Undo restores the items. Clear again and wait 5 s: the strip hides.
7. Clear, then drop a file within 5 s: the note goes and only the new file is on the shelf.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/shelf src-tauri/src/lib.rs src/shelf src/lib/i18n/en.json src-tauri/src/platform
git commit -m "feat(shelf): menu with copy, paste, show items and clear with undo

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Optimise all (the round trip)

**Files:**
- Modify:
  - `src-tauri/src/drop_batch/intake.rs` (extract `accept_paths`, which returns the started jobs)
  - `src-tauri/src/shelf/mod.rs` (`optimise_all`, the pending map, `on_job_update`)
  - `src-tauri/src/jobs.rs` (`show` calls `crate::shelf::on_job_update`)

**Interfaces:**
- Consumes: `intake::accept` internals, `JobUpdate { id, state, output }`, `JobState::Done`, `ferry_core::intake::classify_file`.
- Produces:
  - `intake::accept_paths(app: &AppHandle, paths: Vec<PathBuf>, keep: bool) -> Vec<(JobId, PathBuf)>`, with no batch routing. The shelf sends any count as cards.
  - `shelf::optimise_all(app)`
  - `shelf::on_job_update(app, &JobUpdate)`
  - `fn optimisable(paths: &[PathBuf]) -> Vec<PathBuf>` (pure, tested)

- [ ] **Step 1: Failing test**

```rust
    #[test]
    fn only_supported_files_are_sent_to_optimise() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("a.png");
        std::fs::write(&png, ferry_core::testutil::png()).unwrap();
        let txt = dir.path().join("notes.txt");
        std::fs::write(&txt, b"hi").unwrap();
        let folder = dir.path().join("photos");
        std::fs::create_dir(&folder).unwrap();
        assert_eq!(optimisable(&[png.clone(), txt, folder]), vec![png]);
    }
```

If `ferry_core::testutil` is `#[cfg(test)]`-only inside ferry-core and not reachable from `src-tauri`, look for how `src-tauri` tests make image bytes (grep `fn png` under `src-tauri/src`) and reuse that. As a last resort, write a minimal valid PNG byte literal.

- [ ] **Step 2: Run it to see it fail**

Run: `cargo test -p ferry shelf::tests::only_supported`
Expected: compile error.

- [ ] **Step 3: Extract `accept_paths` in `intake.rs`**

Move the `for path in rest { ... }` loop of `accept` into:

```rust
/// Start a card for each path (no batch window). Returns the jobs started, with their source.
pub fn accept_paths(app: &AppHandle, paths: Vec<PathBuf>, keep: bool) -> Vec<(JobId, PathBuf)> {
    // own-result filtering and notices: exactly as `accept` does today, so moved here
    // the per-path loop from `accept`, pushing `(id.clone(), path.clone())` after a successful `start_assigned`
}
```

Move the `without_own` filtering and its notices into `accept_paths` too. `accept` becomes: filter own results (via `accept_paths`'s logic), check the batch route, then `accept_paths`. Keep `accept` behaving exactly as before: run `cargo test -p ferry intake` and expect the existing tests unchanged and passing. If the own-result filtering would run twice, keep it in `accept` and give `accept_paths` a private inner `start_cards(app, rest, keep)` that both call. The shelf calls `accept_paths`, which filters and then calls `start_cards`.

- [ ] **Step 4: Optimise all and the result swap (`shelf/mod.rs`)**

Replace the Task 7 stub:

```rust
use std::collections::HashMap;
use ferry_core::job::{JobId, JobState, JobUpdate};

static PENDING: LazyLock<Mutex<HashMap<JobId, PathBuf>>> = LazyLock::new(|| Mutex::new(HashMap::new()));

pub fn optimisable(paths: &[PathBuf]) -> Vec<PathBuf> {
    paths
        .iter()
        .filter(|p| p.is_file() && matches!(ferry_core::intake::classify_file(p), Ok(Some(_))))
        .cloned()
        .collect()
}

pub fn optimise_all(app: &AppHandle) {
    let paths = state(app).map(|s| s.list.lock().unwrap().paths()).unwrap_or_default();
    let wanted = optimisable(&paths);
    if wanted.is_empty() {
        jobs::notice(app, t("shelf.error.nothing_to_optimise"));
        return;
    }
    let started = crate::drop_batch::intake::accept_paths(app, wanted, false);
    PENDING.lock().unwrap().extend(started);
}

/// A finished card from "Optimise all" puts its result where its source was.
pub fn on_job_update(app: &AppHandle, update: &JobUpdate) {
    let finished = matches!(update.state, JobState::Done | JobState::AlreadyOptimal | JobState::Larger | JobState::Error { .. });
    if !finished {
        return;
    }
    let Some(source) = PENDING.lock().unwrap().remove(&update.id) else { return };
    if let (JobState::Done, Some(output)) = (&update.state, &update.output) {
        let (source, output) = (source.clone(), output.clone());
        mutate(app, |list| list.replace(&source, &output));
    }
}
```

`keep: false` means "the drop's normal behaviour". Check `mark_dropped(app, &id, keep)` in `intake.rs` to confirm that `false` is the default (not "keep the original") before using it. Pass whatever value a plain zone drop uses.

In `jobs.rs::show`, as the first line:

```rust
    crate::shelf::on_job_update(app, &update);
```

- [ ] **Step 5: Build, test, check**

```bash
cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry
```

Manual checks:
1. Shelve two PNGs and a `.txt`, then choose Optimise all. Two cards appear; the shelf ends with the two results and the `.txt`.
2. With only the `.txt` on the shelf, Optimise all shows "Nothing here can be optimised".

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/drop_batch/intake.rs src-tauri/src/shelf src-tauri/src/jobs.rs
git commit -m "feat(shelf): optimise all, with each result taking its source's place

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: Cards → shelf (Shelve all, and Ferry Shelf in Add to Shelf ▸)

**Files:**
- Modify:
  - `src/stack/DragAllBar.svelte` (a "Shelve all" button)
  - `src/stack/SelectionBar.svelte` (a "Shelve" button)
  - `src-tauri/src/integrations/menu.rs` (a "Ferry Shelf" row first in Add to Shelf ▸, and an `Item::FerryShelf` id)
  - `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: the `shelf_add(paths)` command (Task 4) and `shelf::add` (Task 4).
- Produces:
  - `Item::FerryShelf` with key `"ferry_shelf"`
  - i18n:
    - `"stack.shelve_all": "Shelve all"`
    - `"stack.shelve_all.tip": "Put every finished file on Ferry's shelf"`
    - `"drop.list.shelve": "Shelve"`
    - `"share.menu.ferry_shelf": "Ferry Shelf"`

- [ ] **Step 1: Failing test for the menu id**

In `integrations/menu.rs` tests, extend `every_item_reads_back_through_the_card_menu_ids` (line ~331) by adding `Item::FerryShelf` to its list. If it has no list, add:

```rust
    #[test]
    fn the_ferry_shelf_item_reads_back() {
        assert_eq!(Item::parse(&Item::FerryShelf.key()), Some(Item::FerryShelf));
    }
```

Run: `cargo test -p ferry integrations::menu`
Expected: compile error.

- [ ] **Step 2: Implement the menu row**

- Add `FerryShelf` to `Item`, with `key` `"ferry_shelf"`, and handle `"ferry_shelf" => return Some(Item::FerryShelf)` in `parse`.
- In `extend`, on macOS, build `item(Item::FerryShelf, t("share.menu.ferry_shelf"), None)?` and put it first in the Add to Shelf ▸ submenu, followed by a separator if there are outside shelf apps.
- The submenu now always exists on macOS: change `(!shelves.is_empty()).then(...)` so the submenu is built when `cfg!(target_os = "macos") || !shelves.is_empty()`.
- In `run`, add `Item::FerryShelf => { if let Some(path) = jobs::output(app, id) { crate::shelf::add(app, vec![path]); } Ok(()) }`. Use the existing `jobs::output` helper, as in `drop_batch/list.rs`.

- [ ] **Step 3: The buttons**

`DragAllBar.svelte`:
- Import `invoke`.
- Add `function shelveAll() { void invoke('shelf_add', { paths: files }); }`.
- Between the drag and clear buttons, add `<button type="button" data-action="shelve" disabled={files.length === 0} onclick={shelveAll}>{t('stack.shelve_all')}</button>`. Render it only on macOS: use the existing OS check from `src/lib/ipc.ts` (grep `isMac` or `applyOsClass`). If there is only a body class, hide it with `:global(.os-windows) [data-action='shelve'] { display: none; }` in the same style the codebase uses elsewhere (grep `os-windows`).
- Add `shelve: t('stack.shelve_all.tip')` to `tips`.

`SelectionBar.svelte`: next to `sel-copy`, add `<button type="button" data-action="sel-shelve" aria-label={t('drop.list.shelve')} title={t('drop.list.shelve')} disabled={targets.files.length === 0} onclick={() => invoke('shelf_add', { paths: targets.files })}>{t('drop.list.shelve')}</button>`. Check that `targets.files` is a `string[]` of paths (see `src/lib/selection.ts`); if it holds ids, map them to card outputs the way `drag` in that file does.

- [ ] **Step 4: Build, test, check**

```bash
cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry
node_modules/.bin/svelte-check --threshold error && node_modules/.bin/vitest run
```

Manual checks:
1. Shelve all with three cards: the count goes to 3.
2. Select two cards and click Shelve: they are added, with no duplicates.
3. A card's menu → Add to Shelf ▸ → Ferry Shelf: the item is added.

- [ ] **Step 5: Commit**

```bash
git add src/stack src-tauri/src/integrations/menu.rs src/lib/i18n/en.json
git commit -m "feat(shelf): shelve cards from the bars and the card menu

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: The whole-feature manual pass

**Files:** none, unless fixes are needed.

- [ ] **Step 1: Full checks**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
FERRY_GIFSICLE=$PWD/src-tauri/binaries/gifsicle-x86_64-apple-darwin FERRY_FFMPEG=$PWD/src-tauri/binaries/ffmpeg-x86_64-apple-darwin cargo test --workspace
node_modules/.bin/svelte-check --threshold error
node_modules/.bin/vitest run
```
Expected: all clean.

- [ ] **Step 2: Spec success scenario**

1. Drag three Finder files and one result card onto the strip.
2. Quit and relaunch the dev app.
3. Drag the pile into a Mail draft: all four are attached and the shelf hides.

Screenshot each step.

- [ ] **Step 3: The spec's remaining manual list**

- Set the corner to Top Right in Settings. The stack is at the top and the shelf stays at the bottom-right.
- Two monitors with `follow_cursor` on, if available. With a stack showing, the shelf is on the stack's monitor. Without one, it is on the monitor where the drag started.
- Turn Dock auto-hide on at the bottom (System Settings). The strip sits at the very bottom; note whether the Dock covers it while it is summoned.
- With the optimise zone on and a stack showing, drag a Finder file. The zone and the expanded shelf never overlap (screenshot).

- [ ] **Step 4: Record and commit any fixes**

Commit each fix on its own, with a `fix(shelf): …` message. Then hand back to the user with the screenshots.
