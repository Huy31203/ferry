# Image Drags From Any App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** On macOS, an image dragged out of any app (Slack, Chrome, Safari) shows Ferry's drop zone and, dropped on it, is saved as a real file and optimised like a dropped file.

**Architecture:** The drag watcher learns to count "image items" (drag items with image bytes or an image file promise but no file path) next to file paths. When Tauri delivers `DragDropEvent::Drop` with no paths, the zone reads the image bytes from the drag pasteboard on the main thread, a pure `ferry_core::dragged` module writes them into the save folder with a clipboard-style name, and the resulting paths go into the existing `intake::accept`. A file-promise fallback covers drags with no image bytes.

**Tech Stack:** Rust (Tauri 2, objc2 0.6 / objc2-app-kit 0.3 / objc2-foundation 0.3, chrono), Svelte 5 + TypeScript (settings type only), vitest, cargo test.

**Spec:** `docs/superpowers/specs/2026-11-09-ferry-image-drags-design.md`

## Global Constraints

- Read `AGENTS.md` and `CONSTRAINTS.md` at the repo root if present; follow both.
- macOS only for behaviour. Windows compiles with `drag_contents()` returning files only (`images: 0`); no Windows receiving. Any Windows-only code stays behind `cfg(windows)`.
- New setting `drop_zone.save_folder: String`, default `"~/Downloads/Ferry"`, stored with `~`; `#[serde(default)]` on the struct already covers old files; no settings version bump. No Settings window row.
- File name: `<Source> image <YYYY-MM-DD> at <HH.MM.SS>.<ext>`; `<Source>` is the frontmost app's localized name at drop time, or `Dragged` when unknown; clashes resolved by `ferry_core::output::unique` (`name (2).ext`).
- Image bytes preference order: `public.jpeg`, `public.png`, `org.webmproject.webp`, `public.heic`, `com.compuserve.gif`, `public.tiff`. Bytes are written unchanged.
- Error strings: `drop.dragged.unreadable` = "Couldn't read the dragged image", `drop.dragged.cant_save` = "Can't save to {folder}". Shown with `jobs::notice`.
- Non-goals: preset zones keep using file paths; no text or link drags; no `public.url` downloads; no shelf.
- Toolchain in this shell: prefix commands with `export PATH=/usr/local/bin:$HOME/.nvm/versions/node/v22.23.4/bin:$HOME/.cargo/bin:$PATH` (Node 22 is required by pnpm; nasm lives in `/usr/local/bin`).
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. A Finder file drag (file URL plus image types on the same item, which Finder does for images) must still count as a **file**, not as an image item: never both. Pinned in Task 2 (`a_file_item_is_never_an_image_item`).
2. A drag of text or a link from Chrome (`public.url`, `public.utf8-plain-text`, no image type) must not show the zone. Pinned in Task 2 (`text_and_links_are_not_images`) and Task 4 (watch plan test).
3. Two drops in the same second from the same app must not overwrite each other. Pinned in Task 3 (`two_images_in_one_second_get_distinct_names`).
4. A source app name containing `/` or `:` (or empty) must still produce a valid file name. Pinned in Task 3 (`an_awkward_source_name_is_made_safe`).
5. A zone `Enter` for an image drag must not show "Can't optimise these files" (all-zero summary with `files > 0`). Pinned in Task 2 (`an_image_only_drag_is_summarised_as_images`) and Task 4 (zone Enter uses `summarise_drag`).

---

## File Structure

| File | Responsibility |
| --- | --- |
| `crates/ferry-core/src/drag.rs` | `DragContents`; dragged-image type table; `is_dragged_image`, `pick_image_type`; `should_auto_show` unchanged in signature |
| `crates/ferry-core/src/intake.rs` | `summarise_drag(paths, images, head_limit)` |
| `crates/ferry-core/src/dragged.rs` (new) | `DraggedImage`, `dragged_name`, `save_dragged` (pure fs, no Tauri) |
| `crates/ferry-core/src/settings.rs` | `DropZone.save_folder` |
| `src/lib/i18n/en.json` | `dragged.name`, `drop.dragged.unreadable`, `drop.dragged.cant_save` |
| `src/lib/settings.ts`, `src/lib/settings.fixture.ts` | `save_folder` in the `drop_zone` type and fixture |
| `src-tauri/src/platform/macos_drop.rs` | `drag_contents()`, `read_dragged_images()`, `frontmost_app_name()` |
| `src-tauri/src/platform/windows_drop.rs` | `drag_contents()`, `read_dragged_images()`, `frontmost_app_name()` stubs |
| `src-tauri/src/drop_batch/watch.rs` | use `drag_contents()` |
| `src-tauri/src/drop_batch/zone.rs` | `Enter` summary via `summarise_drag`; empty-paths `Drop` reads and saves |
| `src-tauri/src/preset_zones.rs` | `drag_contents().files` |

---

### Task 1: Probe whether drag bytes are full resolution (manual, throwaway)

Decides the order in Task 5: bytes first (as specced) or promise first. No product code.

**Files:**
- Create (scratch, not committed): `$SCRATCH/dragbytes.swift` where `$SCRATCH` is any temp dir outside the repo.

- [ ] **Step 1: Write the probe**

```swift
// Saves each new drag's image bytes for 30 s, then prints their pixel size.
import AppKit
let pb = NSPasteboard(name: .drag)
var last = pb.changeCount
let end = Date().addingTimeInterval(30)
print("drag an image from Slack now"); fflush(stdout)
while Date() < end {
  if pb.changeCount != last {
    last = pb.changeCount
    for item in pb.pasteboardItems ?? [] {
      for (uti, ext) in [("public.jpeg", "jpg"), ("public.png", "png")] {
        if let data = item.data(forType: NSPasteboard.PasteboardType(uti)) {
          let url = URL(fileURLWithPath: "dragged-\(last).\(ext)")
          try? data.write(to: url)
          let rep = NSBitmapImageRep(data: data)
          print("\(uti): \(data.count) bytes, \(rep?.pixelsWide ?? 0)x\(rep?.pixelsHigh ?? 0) -> \(url.path)")
        }
      }
    }
    fflush(stdout)
  }
  usleep(100_000)
}
```

- [ ] **Step 2: Run it and drag a large Slack image (a photo, not a small screenshot)**

Run: `cd $SCRATCH && swiftc -O dragbytes.swift -o dragbytes && ./dragbytes`
Then in Slack open the same image's menu → Download, and run `sips -g pixelWidth -g pixelHeight ~/Downloads/<that file>`.

- [ ] **Step 3: Record the outcome**

Expected outcomes and what to do:
- Pixel sizes equal → keep bytes-first (Task 5 as written).
- Dragged bytes smaller → in Task 6, call the promise path **before** the bytes path for items that have both (the step is spelled out there).
Write the outcome (sizes, decision) in the Task 5 commit message body.

---

### Task 2: Core types for dragged images and their summary

**Files:**
- Modify: `crates/ferry-core/src/drag.rs` (add after the `Mods` impl, before `should_auto_show`; tests in the existing `mod tests` at the end)
- Modify: `crates/ferry-core/src/intake.rs` (add `summarise_drag` right after `summarise`; tests in its `mod tests`)

**Interfaces:**
- Produces:
  - `ferry_core::drag::DragContents { pub files: Vec<PathBuf>, pub images: u32 }` (`Debug, Clone, Default, PartialEq, Eq`)
  - `ferry_core::drag::FILE_URL: &str = "public.file-url"`, `PROMISED_URL: &str = "com.apple.pasteboard.promised-file-url"`, `PROMISED_TYPE: &str = "com.apple.pasteboard.promised-file-content-type"`
  - `ferry_core::drag::IMAGE_TYPES: [(&str, &str); 6]` (uti, extension) in preference order
  - `ferry_core::drag::pick_image_type(types: &[String]) -> Option<(&'static str, &'static str)>`
  - `ferry_core::drag::is_dragged_image(types: &[String], promised_type: Option<&str>) -> bool`
  - `ferry_core::intake::summarise_drag(paths: &[PathBuf], images: u32, head_limit: usize) -> Summary`

- [ ] **Step 1: Write the failing tests in `drag.rs` `mod tests`**

```rust
    fn types(list: &[&str]) -> Vec<String> {
        list.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn slack_image_bytes_are_a_dragged_image() {
        let slack = types(&["org.chromium.chromium-initiated-drag", "public.url", "public.png", PROMISED_URL, PROMISED_TYPE]);
        assert!(is_dragged_image(&slack, Some("public.png")));
        assert_eq!(pick_image_type(&slack), Some(("public.png", "png")));
    }

    #[test]
    fn jpeg_is_preferred_over_tiff() {
        let both = types(&["public.tiff", "public.jpeg"]);
        assert_eq!(pick_image_type(&both), Some(("public.jpeg", "jpg")));
    }

    #[test]
    fn an_image_promise_without_bytes_is_a_dragged_image() {
        let promise = types(&[PROMISED_URL, PROMISED_TYPE]);
        assert!(is_dragged_image(&promise, Some("public.jpeg")));
        assert!(!is_dragged_image(&promise, Some("com.adobe.pdf")));
        assert!(!is_dragged_image(&promise, None));
        assert_eq!(pick_image_type(&promise), None);
    }

    #[test]
    fn text_and_links_are_not_images() {
        let link = types(&["public.url", "public.url-name", "public.utf8-plain-text"]);
        assert!(!is_dragged_image(&link, None));
    }

    #[test]
    fn a_file_item_is_never_an_image_item() {
        let finder = types(&[FILE_URL, "public.png", "public.tiff"]);
        assert!(!is_dragged_image(&finder, None));
    }
```

- [ ] **Step 2: Write the failing test in `intake.rs` `mod tests`**

```rust
    #[test]
    fn an_image_only_drag_is_summarised_as_images() {
        let s = summarise_drag(&[], 2, 10);
        assert_eq!((s.files, s.images, s.unsupported), (2, 2, 0));
        let view = zone_view(Some(&s), true, false, 20, ZoneMode::Zone);
        assert_eq!(view.state, ZoneState::Hover);
    }
```

- [ ] **Step 3: Run to see them fail**

Run: `cargo test -p ferry-core drag:: intake::tests::an_image_only`
Expected: compile errors, `cannot find function is_dragged_image` / `summarise_drag`.

- [ ] **Step 4: Implement in `drag.rs`**

Add `use std::path::PathBuf;` to the imports, then:

```rust
/// Pasteboard types the drag watcher and the zone read (macOS UTIs).
pub const FILE_URL: &str = "public.file-url";
pub const PROMISED_URL: &str = "com.apple.pasteboard.promised-file-url";
pub const PROMISED_TYPE: &str = "com.apple.pasteboard.promised-file-content-type";

/// Image bytes a drag item may carry, best first, with the extension they are saved under.
pub const IMAGE_TYPES: [(&str, &str); 6] = [
    ("public.jpeg", "jpg"),
    ("public.png", "png"),
    ("org.webmproject.webp", "webp"),
    ("public.heic", "heic"),
    ("com.compuserve.gif", "gif"),
    ("public.tiff", "tiff"),
];

/// What a drag holds: file paths, and items with only image bytes or an image promise.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DragContents {
    pub files: Vec<PathBuf>,
    pub images: u32,
}

/// The best image type among a drag item's `types`.
pub fn pick_image_type(types: &[String]) -> Option<(&'static str, &'static str)> {
    IMAGE_TYPES.into_iter().find(|(uti, _)| types.iter().any(|t| t == uti))
}

/// Whether a drag item with `types` is an image to save: no file URL, and image bytes or a
/// promise whose content type (`promised_type`) is an image.
pub fn is_dragged_image(types: &[String], promised_type: Option<&str>) -> bool {
    if types.iter().any(|t| t == FILE_URL) {
        return false;
    }
    let promised_image = types.iter().any(|t| t == PROMISED_URL)
        && promised_type.is_some_and(|p| IMAGE_TYPES.iter().any(|(uti, _)| *uti == p));
    pick_image_type(types).is_some() || promised_image
}
```

- [ ] **Step 5: Implement in `intake.rs`, after `summarise`**

```rust
/// `summarise` for a drag that also holds `images` items with no file (counted as image files).
pub fn summarise_drag(paths: &[PathBuf], images: u32, head_limit: usize) -> Summary {
    let mut s = summarise(paths, head_limit);
    s.files += images;
    s.images += images;
    s
}
```

- [ ] **Step 6: Run the tests**

Run: `cargo test -p ferry-core`
Expected: all pass.

- [ ] **Step 7: Commit**

```bash
git add crates/ferry-core/src/drag.rs crates/ferry-core/src/intake.rs
git commit -m "feat(core): tell image drags from file drags

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Saving dragged images (core) and the save-folder setting

**Files:**
- Create: `crates/ferry-core/src/dragged.rs`
- Modify: `crates/ferry-core/src/lib.rs` (add `pub mod dragged;` after `pub mod drag;`)
- Modify: `crates/ferry-core/src/settings.rs:685-741` (`DropZone` field + default)
- Modify: `src/lib/i18n/en.json` (after `"clipboard.folder"`, and after `"drop.zone.tab"`)
- Modify: `src/lib/settings.ts:97-105`, `src/lib/settings.fixture.ts:99-107`

**Interfaces:**
- Consumes: `ferry_core::output::unique(path: &Path, own: Option<&Path>) -> PathBuf`, `ferry_core::template::safe_file_name(&str) -> String`, `ferry_core::i18n::tf`.
- Produces:
  - `ferry_core::dragged::DraggedImage { pub ext: &'static str, pub bytes: Vec<u8> }`
  - `ferry_core::dragged::dragged_name(source: &str, now: NaiveDateTime) -> String` (stem, no extension)
  - `ferry_core::dragged::save_dragged(folder: &Path, source: &str, items: Vec<Option<DraggedImage>>, now: NaiveDateTime) -> Saved`
  - `ferry_core::dragged::Saved { pub paths: Vec<PathBuf>, pub unreadable: u32, pub cant_save: bool }`
  - `DropZone.save_folder: String`

- [ ] **Step 1: Add the strings to `en.json`**

After `"clipboard.folder": "Clipboard",` add:
```json
  "dragged.name": "{source} image {date} at {time}",
  "dragged.source_unknown": "Dragged",
```
After `"drop.zone.tab": "Drop files here",` add:
```json
  "drop.dragged.unreadable": "Couldn't read the dragged image",
  "drop.dragged.cant_save": "Can't save to {folder}",
```

- [ ] **Step 2: Write the failing tests (bottom of the new `dragged.rs`)**

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use chrono::NaiveDate;

    fn at() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 11, 9).unwrap().and_hms_opt(15, 42, 10).unwrap()
    }

    fn png() -> Option<DraggedImage> {
        Some(DraggedImage { ext: "png", bytes: b"\x89PNG fake".to_vec() })
    }

    #[test]
    fn a_dragged_image_is_named_after_its_app() {
        assert_eq!(dragged_name("Slack", at()), "Slack image 2026-11-09 at 15.42.10");
    }

    #[test]
    fn an_awkward_source_name_is_made_safe() {
        assert_eq!(dragged_name("A/B: C", at()), "A_B_ C image 2026-11-09 at 15.42.10");
        assert_eq!(dragged_name("", at()), "Dragged image 2026-11-09 at 15.42.10");
    }

    #[test]
    fn bytes_are_written_unchanged_into_a_new_folder() {
        let dir = tempfile::tempdir().unwrap();
        let folder = dir.path().join("Ferry");
        let saved = save_dragged(&folder, "Slack", vec![png()], at());
        assert_eq!(saved.paths, vec![folder.join("Slack image 2026-11-09 at 15.42.10.png")]);
        assert_eq!(std::fs::read(&saved.paths[0]).unwrap(), b"\x89PNG fake");
        assert_eq!((saved.unreadable, saved.cant_save), (0, false));
    }

    #[test]
    fn two_images_in_one_second_get_distinct_names() {
        let dir = tempfile::tempdir().unwrap();
        let saved = save_dragged(dir.path(), "Slack", vec![png(), png()], at());
        assert_eq!(saved.paths.len(), 2);
        assert_ne!(saved.paths[0], saved.paths[1]);
        assert!(saved.paths.iter().all(|p| p.exists()));
    }

    #[test]
    fn unreadable_items_are_counted_and_the_rest_saved() {
        let dir = tempfile::tempdir().unwrap();
        let saved = save_dragged(dir.path(), "Slack", vec![None, png()], at());
        assert_eq!((saved.paths.len(), saved.unreadable), (1, 1));
    }

    #[test]
    fn an_unwritable_folder_saves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("file");
        std::fs::write(&blocker, b"x").unwrap();
        let saved = save_dragged(&blocker.join("sub"), "Slack", vec![png()], at());
        assert!(saved.paths.is_empty());
        assert!(saved.cant_save);
    }
}
```

Add to `settings.rs` `mod tests`:
```rust
    #[test]
    fn dragged_images_go_to_downloads_ferry_by_default() {
        assert_eq!(DropZone::default().save_folder, "~/Downloads/Ferry");
        let old: DropZone = serde_json::from_value(json!({ "enabled": true })).unwrap();
        assert_eq!(old.save_folder, "~/Downloads/Ferry");
    }
```

- [ ] **Step 3: Run to see them fail**

Run: `cargo test -p ferry-core dragged settings::tests::dragged`
Expected: compile errors (`dragged` module / `save_folder` missing).

- [ ] **Step 4: Implement `dragged.rs` (above the tests)**

```rust
//! Images dragged from another app with no file behind them (image bytes on the
//! drag pasteboard): named like clipboard images and saved as real files.

use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::i18n::{t, tf};
use crate::output::unique;
use crate::template::safe_file_name;

/// One dragged image's bytes and the extension they are saved under.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraggedImage {
    pub ext: &'static str,
    pub bytes: Vec<u8>,
}

/// What `save_dragged` did: the files written, the items with no readable image, and
/// whether the folder could not be created or written.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Saved {
    pub paths: Vec<PathBuf>,
    pub unreadable: u32,
    pub cant_save: bool,
}

/// "Slack image 2026-11-09 at 15.42.10": the stem of a dragged image's file.
pub fn dragged_name(source: &str, now: NaiveDateTime) -> String {
    let source = if source.trim().is_empty() { t("dragged.source_unknown") } else { source.to_string() };
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H.%M.%S").to_string();
    safe_file_name(&tf("dragged.name", &[("source", &source), ("date", &date), ("time", &time)]))
}

/// Write each readable item into `folder` (created if missing). `None` items count as unreadable.
pub fn save_dragged(folder: &Path, source: &str, items: Vec<Option<DraggedImage>>, now: NaiveDateTime) -> Saved {
    let mut saved = Saved::default();
    if fs::create_dir_all(folder).is_err() {
        saved.cant_save = true;
        return saved;
    }
    let stem = dragged_name(source, now);
    for item in items {
        let Some(image) = item else {
            saved.unreadable += 1;
            continue;
        };
        let path = unique(&folder.join(format!("{stem}.{}", image.ext)), None);
        match fs::write(&path, &image.bytes) {
            Ok(()) => saved.paths.push(path),
            Err(_) => saved.cant_save = true,
        }
    }
    saved
}
```

If `t` is not exported from `crate::i18n` under that name, use the same function the `template.rs` code calls for `t("clipboard.folder")` (check its `use` line).

- [ ] **Step 5: Add the setting in `settings.rs`**

In `pub struct DropZone`, after `pub ignored_apps: Vec<String>,`:
```rust
    /// Where images dragged from other apps (no file behind them) are saved. Stored with `~`.
    pub save_folder: String,
```
In `impl Default for DropZone`, after `ignored_apps: …,`:
```rust
            save_folder: "~/Downloads/Ferry".into(),
```

- [ ] **Step 6: Mirror it in TypeScript**

`src/lib/settings.ts`, in `drop_zone: { … }` after `ignored_apps: string[];`:
```ts
    save_folder: string;
```
`src/lib/settings.fixture.ts`, in `drop_zone: { … }` after `ignored_apps: ['com.figma.Desktop'],`:
```ts
    save_folder: '~/Downloads/Ferry',
```

- [ ] **Step 7: Run everything touched**

Run: `cargo test -p ferry-core && ./node_modules/.bin/svelte-check --tsconfig ./tsconfig.json && ./node_modules/.bin/vitest run`
Expected: all pass, 0 errors.

- [ ] **Step 8: Commit**

```bash
git add crates/ferry-core/src/dragged.rs crates/ferry-core/src/lib.rs crates/ferry-core/src/settings.rs src/lib/i18n/en.json src/lib/settings.ts src/lib/settings.fixture.ts
git commit -m "feat(core): save dragged images under a clipboard-style name

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Show the zone for image drags

**Files:**
- Modify: `src-tauri/src/platform/macos_drop.rs:40` (re-export) and `:82-93` (`drag_files` → `drag_contents`)
- Modify: `src-tauri/src/platform/windows_drop.rs:70-74`
- Modify: `src-tauri/src/drop_batch/watch.rs` (imports, `Context`, `handle`, tests)
- Modify: `src-tauri/src/drop_batch/zone.rs:374-391` (`Enter`)
- Modify: `src-tauri/src/preset_zones.rs:476`

**Interfaces:**
- Consumes: Task 2's `DragContents`, `is_dragged_image`, `FILE_URL`, `PROMISED_TYPE`, `summarise_drag`.
- Produces: `platform::drop::drag_contents() -> ferry_core::drag::DragContents` on both platforms (replaces `drag_files`).

- [ ] **Step 1: Write the failing watch test (`watch.rs` `mod tests`)**

The existing helper `ctx(zone, files)` builds a `Context`. Change `Context.files: &[PathBuf]` to `Context.drag: &DragContents` in the test helper too (Step 3). The new test:

```rust
    #[test]
    fn an_image_dragged_from_slack_shows_the_zone() {
        let zone = any_drag();
        let drag = DragContents { files: vec![], images: 1 };
        let c = Context { drag: &drag, ..ctx(&zone, &DragContents::default()) };
        assert_eq!(plan(Signal::DragStarted, &c), SHOW_ZONE);
        // Text or a link: nothing to drop.
        assert_eq!(plan(Signal::DragStarted, &ctx(&zone, &DragContents::default())), Plan::Nothing);
    }
```

- [ ] **Step 2: Run to see it fail**

Run: `cargo test -p ferry --lib drop_batch::watch`
Expected: compile error, no field `drag` on `Context`.

- [ ] **Step 3: Change `watch.rs`**

- Imports: `use ferry_core::drag::{DragContents, DragWatch, Modifier, Signal, should_auto_show};` and `use ferry_core::intake::{ZoneMode, all_own, summarise_drag};` (drop `summarise` if no longer used).
- `struct Context`: replace `files: &'a [PathBuf],` with `drag: &'a DragContents,`.
- `plan`, `Signal::DragStarted` arm:
  ```rust
  let has_drop = !ctx.drag.files.is_empty() || ctx.drag.images > 0;
  if should_auto_show(ctx.zone, has_drop, ctx.all_own, ctx.frontmost) {
  ```
- `handle`:
  ```rust
  let drag = if matches!(signal, Signal::DragStarted | Signal::Tap(_)) {
      platform::drop::drag_contents()
  } else {
      DragContents::default()
  };
  ```
  and in `Context { … }`: `drag: &drag, all_own: all_own(&drag.files, &outputs),`.
  In `Plan::Show`: replace `if !files.is_empty() {` with `if !drag.files.is_empty() || drag.images > 0 {`, and the summary line with
  `zone::set_summary(&app, summarise_drag(&drag.files, drag.images, limit));` (move `drag` into the closure).
- Tests: update `fn ctx<'a>(zone: &'a DropZone, files: &'a [PathBuf]) -> Context<'a>` to take `drag: &'a DragContents` and set `drag`. In existing tests, replace `[PathBuf::from("/a.png")]` with `DragContents { files: vec![PathBuf::from("/a.png")], images: 0 }` and `&[]` with `&DragContents::default()`. In `sampling_works_off_the_main_thread`, call `platform::drop::drag_contents()`.

- [ ] **Step 4: macOS `drag_contents` (`macos_drop.rs`)**

Replace `drag_files` in `mod sampling`:
```rust
    /// The files on the drag pasteboard, and how many items are images with no file.
    pub fn drag_contents() -> DragContents {
        let mut out = DragContents::default();
        let items = drag_pasteboard().pasteboardItems().unwrap_or_default();
        for item in items.iter() {
            let file = item
                .stringForType(&NSString::from_str(FILE_URL))
                .and_then(|url| NSURL::URLWithString(&url)?.to_file_path());
            if let Some(path) = file {
                out.files.push(path);
                continue;
            }
            let types: Vec<String> = item.types().iter().map(|t| t.to_string()).collect();
            let promised = item.stringForType(&NSString::from_str(PROMISED_TYPE)).map(|s| s.to_string());
            if is_dragged_image(&types, promised.as_deref()) {
                out.images += 1;
            }
        }
        out
    }
```
Imports in `mod sampling`: `use ferry_core::drag::{DragContents, FILE_URL, Mods, PROMISED_TYPE, Sample, is_dragged_image};` (drop `PathBuf` if unused). Re-export line: `pub use sampling::{drag_contents, frontmost_app, mods, sample};`.

- [ ] **Step 5: Windows stub (`windows_drop.rs`)**

```rust
/// Windows has no drag pasteboard to read.
pub fn drag_contents() -> ferry_core::drag::DragContents {
    ferry_core::drag::DragContents::default()
}
```
Remove `drag_files` and the now-unused `PathBuf` import if nothing else uses it.

- [ ] **Step 6: Zone `Enter` and preset zones**

`zone.rs` `DragDropEvent::Enter`: `paths` is empty for an image drag, so ask the pasteboard for images:
```rust
            let (app, paths) = (app.clone(), paths.clone());
            let images = if paths.is_empty() { crate::platform::drop::drag_contents().images } else { 0 };
            tauri::async_runtime::spawn_blocking(move || {
                let summary = summarise_drag(&paths, images, threshold(&app) as usize + 1);
```
(`drag_contents()` is read here on the event-loop thread, before the spawn.) Add `summarise_drag` to the `ferry_core::intake` import.

`preset_zones.rs:476`: `show(&app, Mode::Drag, platform::drop::drag_contents().files);`

- [ ] **Step 7: Build, test, lint**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: pass, no warnings.

- [ ] **Step 8: Manual check**

Quit the installed app (`osascript -e 'quit app "Ferry"'`), run `./node_modules/.bin/tauri dev`. Start a drag of an image in Slack (Finder must not be the source) and hover the zone: it appears and reads "1 image" under "Drop to optimise", not "Can't optimise". Drag selected text from a browser: no zone. Drag a PNG from Finder: zone as before. Dropping the Slack image does nothing yet (Task 5).

- [ ] **Step 9: Commit**

```bash
git add src-tauri/src/platform/macos_drop.rs src-tauri/src/platform/windows_drop.rs src-tauri/src/drop_batch/watch.rs src-tauri/src/drop_batch/zone.rs src-tauri/src/preset_zones.rs
git commit -m "feat(drop): show the zone for images dragged from other apps

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Receive the drop: read bytes, save, optimise

**Files:**
- Modify: `src-tauri/src/platform/macos_drop.rs` (`mod sampling`: `read_dragged_images`, `frontmost_app_name`; re-export)
- Modify: `src-tauri/src/platform/windows_drop.rs` (stubs)
- Modify: `src-tauri/src/drop_batch/zone.rs:399-417` (`Drop`)

**Interfaces:**
- Consumes: Task 2 `pick_image_type`, `is_dragged_image`; Task 3 `DraggedImage`, `save_dragged`, `Saved`, `DropZone.save_folder`; `settings::expand_home(&str, &Path) -> PathBuf`; `jobs::notice(&AppHandle, String)`; `intake::accept(&AppHandle, Vec<PathBuf>, bool)`.
- Produces:
  - `platform::drop::read_dragged_images() -> Vec<Option<DraggedImage>>` (one entry per image item; `None` when it has no bytes)
  - `platform::drop::frontmost_app_name() -> Option<String>`

- [ ] **Step 1: macOS readers (`macos_drop.rs`, `mod sampling`)**

```rust
    /// Each image item's best bytes, read now while the drop still holds them; `None` for an
    /// image item with no bytes (a promise only).
    pub fn read_dragged_images() -> Vec<Option<DraggedImage>> {
        let items = drag_pasteboard().pasteboardItems().unwrap_or_default();
        items
            .iter()
            .filter_map(|item| {
                let types: Vec<String> = item.types().iter().map(|t| t.to_string()).collect();
                let promised = item.stringForType(&NSString::from_str(PROMISED_TYPE)).map(|s| s.to_string());
                if !is_dragged_image(&types, promised.as_deref()) {
                    return None;
                }
                let image = pick_image_type(&types).and_then(|(uti, ext)| {
                    let data = item.dataForType(&NSString::from_str(uti))?;
                    Some(DraggedImage { ext, bytes: data.to_vec() })
                });
                Some(image)
            })
            .collect()
    }

    /// The frontmost app's name, such as "Slack".
    pub fn frontmost_app_name() -> Option<String> {
        NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .and_then(|app| app.localizedName())
            .map(|name| name.to_string())
    }
```
Add `DraggedImage` (`ferry_core::dragged::DraggedImage`) and `pick_image_type` to the imports and both functions to the re-export. If `NSData::to_vec` is missing, enable nothing new: use `data.to_vec()` from objc2-foundation's `NSData` (feature `NSData` is already on); if the compiler names a different method, use `unsafe { data.as_bytes_unchecked() }.to_vec()`.

- [ ] **Step 2: Windows stubs (`windows_drop.rs`)**

```rust
/// Windows: receiving dragged images is a later spike (spec §6).
pub fn read_dragged_images() -> Vec<Option<ferry_core::dragged::DraggedImage>> {
    Vec::new()
}

pub fn frontmost_app_name() -> Option<String> {
    None
}
```

- [ ] **Step 3: Zone `Drop` (`zone.rs`)**

Replace the line `let (worker, paths) = (app.clone(), paths.clone());` and the `spawn_blocking(… intake::accept …)` line with:
```rust
            // An image dragged from another app has no file: read its bytes now, while the drop
            // still holds them, and save it before intake. `paths` is empty only for such drags.
            let dragged = if paths.is_empty() {
                Some((crate::platform::drop::read_dragged_images(), crate::platform::drop::frontmost_app_name()))
            } else {
                None
            };
            let (worker, paths) = (app.clone(), paths.clone());
            tauri::async_runtime::spawn_blocking(move || {
                let paths = match dragged {
                    Some((items, source)) => save_dragged_images(&worker, items, source),
                    None => paths,
                };
                if !paths.is_empty() {
                    intake::accept(&worker, paths, keep);
                }
            });
```
And add, above `#[cfg(test)]`:
```rust
/// Save dragged images into the configured folder; error cards for what could not be saved.
fn save_dragged_images(app: &AppHandle, items: Vec<Option<DraggedImage>>, source: Option<String>) -> Vec<PathBuf> {
    if items.is_empty() {
        jobs::notice(app, t("drop.dragged.unreadable"));
        return Vec::new();
    }
    let setting = app.state::<AppState>().settings.read().unwrap().drop_zone.save_folder.clone();
    let home = app.path().home_dir().unwrap_or_default(); // as sync_worker::home does
    let folder = settings::expand_home(&setting, &home);
    let now = chrono::Local::now().naive_local();
    let saved = save_dragged(&folder, source.as_deref().unwrap_or(""), items, now);
    if saved.unreadable > 0 {
        jobs::notice(app, t("drop.dragged.unreadable"));
    }
    if saved.cant_save {
        jobs::notice(app, tf("drop.dragged.cant_save", &[("folder", &setting)]));
    }
    saved.paths
}
```
`app.path()` needs `tauri::Manager` in scope (zone.rs already imports it). Imports: `ferry_core::dragged::{DraggedImage, save_dragged}`, `ferry_core::i18n::tf`, `crate::jobs`, `std::path::PathBuf` as needed (check which exist).

- [ ] **Step 4: Build, test, lint**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: pass.

- [ ] **Step 5: Manual check**

`tauri dev` as in Task 4.
1. Drag a Slack image onto the zone → a file `~/Downloads/Ferry/Slack image <date> at <time>.<ext>` exists and a result card appears and finishes.
2. Same from Chrome (an image on any web page) and Safari → same, named "Google Chrome image …" / "Safari image …".
3. Drop the same Slack image twice quickly → two files, the second `… (2).png`.
4. `chmod 500 ~/Downloads/Ferry` then drop → card "Can't save to ~/Downloads/Ferry"; `chmod 755 ~/Downloads/Ferry` after.
5. Finder PNG drop → unchanged behaviour, nothing new in `~/Downloads/Ferry`.

- [ ] **Step 6: Commit (include Task 1's outcome in the body)**

```bash
git add src-tauri/src/platform/macos_drop.rs src-tauri/src/platform/windows_drop.rs src-tauri/src/drop_batch/zone.rs
git commit -m "feat(drop): save and optimise images dropped from other apps

Probe (plan Task 1): <dragged WxH vs original WxH; bytes-first kept | promise-first needed>

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: File-promise fallback

For image items with no bytes (and, if Task 1 found reduced-size bytes, for every item with a promise), receive the promised file into the save folder.

**Files:**
- Modify: `src-tauri/Cargo.toml:35-36` (features), add `block2 = "0.6.2"` (already in `Cargo.lock` as a transitive dependency; MIT)
- Modify: `src-tauri/src/platform/macos_drop.rs` (`receive_promises`)
- Modify: `src-tauri/src/platform/windows_drop.rs` (stub)
- Modify: `src-tauri/src/drop_batch/zone.rs` (`save_dragged_images`)

**Interfaces:**
- Consumes: Task 5's `save_dragged_images`, `Saved`.
- Produces: `platform::drop::receive_promises(folder: &Path, timeout: Duration) -> Vec<PathBuf>`: the files the drag's image promises wrote into `folder`, waiting at most `timeout`.

- [ ] **Step 1: Features**

`objc2-app-kit` features: add `"NSFilePromiseReceiver"`. `objc2-foundation` features: add `"NSOperation"`, `"block2"`. `[dependencies]`: add `block2 = "0.6.2"`. Run `cargo build -p ferry`; expected: builds.

- [ ] **Step 2: macOS `receive_promises` (`mod sampling`)**

```rust
    /// Receive the drag's file promises into `folder`; returns the files written within `timeout`.
    /// Must run on the main thread while the drop is current (called from the Drop handler).
    pub fn receive_promises(folder: &std::path::Path, timeout: std::time::Duration) -> Vec<PathBuf> {
        use std::sync::{Mutex, mpsc};
        use objc2::ClassType;
        use objc2_app_kit::NSFilePromiseReceiver;
        use objc2_foundation::{NSArray, NSDictionary, NSError, NSOperationQueue};

        let classes = NSArray::from_slice(&[NSFilePromiseReceiver::class()]);
        let Some(receivers) = (unsafe { drag_pasteboard().readObjectsForClasses_options(&classes, None) }) else {
            return Vec::new();
        };
        let dest = NSURL::from_directory_path(folder).expect("an absolute folder");
        let queue = NSOperationQueue::new();
        let (tx, rx) = mpsc::channel::<Option<PathBuf>>();
        let mut expected = 0;
        for object in receivers.iter() {
            let Ok(receiver) = object.downcast::<NSFilePromiseReceiver>() else { continue };
            expected += 1;
            let tx = Mutex::new(Some(tx.clone()));
            let block = block2::RcBlock::new(move |url: std::ptr::NonNull<NSURL>, error: *mut NSError| {
                let path = error.is_null().then(|| unsafe { url.as_ref() }.to_file_path()).flatten();
                if let Some(tx) = tx.lock().unwrap().take() {
                    let _ = tx.send(path);
                }
            });
            unsafe {
                receiver.receivePromisedFilesAtDestination_options_operationQueue_reader(
                    &dest, &NSDictionary::new(), &queue, &block,
                );
            }
        }
        drop(tx);
        let deadline = std::time::Instant::now() + timeout;
        let mut paths = Vec::new();
        for _ in 0..expected {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            match rx.recv_timeout(left) {
                Ok(Some(path)) => paths.push(path),
                Ok(None) => {}
                Err(_) => break,
            }
        }
        paths
    }
```
The objc2 method names above follow the AppKit selectors (`readObjectsForClasses:options:`, `receivePromisedFilesAtDestination:options:operationQueue:reader:`). If the generated binding differs (argument types, `Option` wrappers), follow the compiler, not this sketch; keep the behaviour: one result per receiver, waited for with a deadline. Remove any unused imports so clippy is clean.

**Main thread caveat:** `recv_timeout` blocks the main thread for up to the timeout while the source app writes the file. Use `Duration::from_secs(10)` as the spec says; if the manual check shows the UI freezing noticeably, move the wait (not the `receivePromisedFiles…` calls) to the blocking worker by returning the `Receiver` instead of the paths.

- [ ] **Step 3: Windows stub**

```rust
pub fn receive_promises(_folder: &std::path::Path, _timeout: std::time::Duration) -> Vec<std::path::PathBuf> {
    Vec::new()
}
```

- [ ] **Step 4: Use it in the `Drop` handler (`zone.rs`)**

The promise must be received during the drop on the main thread, so the folder is resolved there too. In the `Drop` arm, change the `dragged` computation to:
```rust
            let dragged = if paths.is_empty() {
                let items = crate::platform::drop::read_dragged_images();
                let missing = items.iter().any(Option::is_none);
                let promised = if missing {
                    let folder = save_folder(app);
                    let _ = std::fs::create_dir_all(&folder);
                    crate::platform::drop::receive_promises(&folder, Duration::from_secs(10))
                } else {
                    Vec::new()
                };
                Some((items, crate::platform::drop::frontmost_app_name(), promised))
            } else {
                None
            };
```
and in the worker: `Some((items, source, promised)) => { let mut p = save_dragged_images(&worker, items.into_iter().filter(Option::is_some).collect(), source); p.extend(promised); p }`. If both lists end up empty, `save_dragged_images` still shows "Couldn't read the dragged image" (its `items.is_empty()` branch). Factor the folder lookup from `save_dragged_images` into `fn save_folder(app: &AppHandle) -> PathBuf` and use it in both places.

**If Task 1 found reduced-size bytes:** set `missing` to `true` whenever any item has a promise. To make that possible, have `read_dragged_images` return `(Vec<Option<DraggedImage>>, bool /* any promise */)`, and then drop the byte items for which a promised file arrived. Keep it simple: if `promised` is non-empty, use only `promised` and ignore the bytes.

- [ ] **Step 5: Build, test, lint**

Run: `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings`
Expected: pass.

- [ ] **Step 6: Manual check**

Promise-only sources are uncommon. Test with Photos.app: drag a photo out of Photos onto the zone (Photos uses file promises without a file URL). Expected: the photo lands in `~/Downloads/Ferry` under Photos' own file name, and a result card appears. Then repeat Task 5's Slack check to confirm nothing regressed.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml Cargo.lock src-tauri/src/platform/macos_drop.rs src-tauri/src/platform/windows_drop.rs src-tauri/src/drop_batch/zone.rs
git commit -m "feat(drop): accept promised image files dropped from other apps

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Final verification

Run: `pnpm check && pnpm test && cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings` (with the PATH from Global Constraints). Expected: all pass. Relaunch the installed Ferry.app afterwards (`open -a Ferry`) if a task quit it.
