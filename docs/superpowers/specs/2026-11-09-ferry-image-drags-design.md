# Ferry: image drags from any app (2026-11-09)

Status: design, awaiting review. First piece of the built-in shelf work; the shelf (later spec) reuses the intake built here.

## Problem

Dragging an image out of Slack (or Chrome, Discord, Teams, a browser tab) shows no drop zone, and dropping it would do nothing. The drag carries no file path: `drag_files()` in `src-tauri/src/platform/macos_drop.rs` reads only `public.file-url`, so the watcher never shows the zone and Tauri's `DragDropEvent::Drop` arrives with an empty `paths`.

## Evidence (spike, 2026-11-09, throwaway, reverted)

- Slack's drag pasteboard holds one item with: `public.url` (a `files.slack.com` link, needs auth), image bytes as `public.jpeg` or `public.png` depending on the image, `com.apple.pasteboard.promised-file-url` + `promised-file-content-type` (a file promise), and Chromium-private types.
- Tauri's webview drop handler returns `true` for every drag (`tauri-runtime-wry` `with_drag_drop_handler`), so the zone's web page never receives HTML5 drag events. Receiving in the page is not possible.
- With the watcher patched to show the zone, dropping the Slack image produced `DragDropEvent::Drop { paths: [] }`, and at that moment the drag pasteboard still held the image bytes (`public.png`, 42 112 bytes). The drop is accepted as a copy, so the image does not slide back to Slack.

## Goal

On macOS, an image dragged from any app shows the drop zone and, when dropped on it, is saved as a real file and optimised exactly as a dropped file would be.

Success: dragging an image from Slack, Chrome and Safari onto the zone each produces a result card; Finder file drags behave as before.

## Non-goals

- Windows receiving (see "Windows" below): a follow-up spike, not this spec.
- Preset zones (`preset_zones.rs`) keep using file paths only.
- Text, links and non-image data. Videos and PDFs arrive only through the file-promise fallback, if at all.
- Downloading `public.url` links (needs the source app's login).
- The shelf itself.

## Design

### 1. What the drag holds (`platform::drop`)

Replace `drag_files() -> Vec<PathBuf>` with `drag_contents() -> DragContents`:

```rust
// ferry_core::drag
pub struct DragContents {
    pub files: Vec<PathBuf>,
    /// Drag items with no file path but image bytes or a file promise.
    pub images: u32,
}
```

macOS counts an item as an image when it has no `public.file-url` and has one of: `public.png`, `public.jpeg`, `public.tiff`, `public.heic`, `org.webmproject.webp`, `com.compuserve.gif`, or `com.apple.pasteboard.promised-file-url` whose `promised-file-content-type` conforms to `public.image`. Windows returns `files: [], images: 0` (unchanged behaviour).

Callers: `drop_batch/watch.rs` (two call sites) switches to `drag_contents()`; `preset_zones.rs` uses `drag_contents().files`.

### 2. Showing the zone (`drop_batch/watch.rs`)

`should_auto_show` gets "has something to drop" = `!files.is_empty() || images > 0`. The `all_own` check (Ferry's own outputs) still looks at `files` only. The zone summary adds `images` to `Summary.images`, so the sub-label reads "1 image" with the existing strings.

### 3. Receiving the drop (`drop_batch/zone.rs` + new `drop_batch/dragged.rs`)

In `on_drag_drop`, `DragDropEvent::Drop { paths, .. }`:

- `paths` non-empty: unchanged.
- `paths` empty: on the blocking worker, call `dragged::save(app)`, which returns `Vec<PathBuf>`, then hand those to the existing `intake::accept(app, paths, keep)`. Nothing else in intake changes.

`dragged::save`, for each drag item without a file URL, picks the first that works:

1. **Image bytes**, preferring the original encoding: `public.jpeg`, `public.png`, `org.webmproject.webp`, `public.heic`, `com.compuserve.gif`, then `public.tiff`. Bytes are written unchanged with the matching extension.
2. **File promise**, when there are no image bytes: `NSFilePromiseReceiver` read from the drag pasteboard, received into the save folder, with a 10 s timeout.

If Task 1 of the plan shows the image bytes are a reduced-size copy, the order flips to promise first, bytes second. The spec's behaviour is otherwise the same.

The read must happen while the drag pasteboard still holds the data. The drop handler runs on the main thread, so the pasteboard read happens there synchronously, and only file writing moves to the worker.

### 4. Where it is saved

New setting `drop_zone.save_folder: String`, default `"~/Downloads/Ferry"`, stored with `~` (via `contract_home` / `expand_home`). Created on first use.

File name: `<Source> image <YYYY-MM-DD> at <HH.MM.SS>.<ext>`, matching the clipboard's "Clipboard 2026-11-09 at 15.39.38" style. `<Source>` is the localized name of the app that was frontmost when the drag started (for example "Slack"), or "Dragged" when unknown. Clashes get "name (2).ext", "name (3).ext", … via `ferry_core::output::unique`.

The saved file is Ferry's own copy, so the usual output rules then apply to it unchanged.

No Settings row in this piece: the schema has no plain folder control, and adding one is not needed to fix drags. The key exists, so a later change (or the shelf spec) can expose it; until then it is changed only by editing or importing settings.

### 5. Errors

- No readable image and no promise, or the promise times out: no file, and one error card "Couldn't read the dragged image" (`drop.dragged.unreadable`).
- The save folder can't be created or written: error card "Can't save to {folder}" (`drop.dragged.cant_save`).
- Partial success with several items: the saved ones go to intake, plus one error card for the rest.

Error cards use `jobs::notice`, as intake does for unsupported files.

### 6. Windows

Windows already shows the zone (as the edge tab) on any long press-and-drag, so detection works. Receiving does not: wry's Windows drop target reads only `CF_HDROP`, and Tauri does not expose the `IDataObject`. Chromium-based apps drag virtual files (`FileGroupDescriptorW` + `FileContents`) and images. Supporting them needs Ferry's own `IDropTarget` on the zone window. That is a separate spike on a Windows machine, followed by its own short design. This spec changes nothing on Windows beyond the shared types.

## Files

| File | Change |
| --- | --- |
| `crates/ferry-core/src/drag.rs` | `DragContents`; image-type table and pure helpers (type → extension, preference order, file name) |
| `crates/ferry-core/src/settings.rs` | `DropZone.save_folder` with `#[serde(default)]`; no version bump |
| `src-tauri/src/platform/macos_drop.rs` | `drag_contents()`, `read_dragged()` (bytes or promise per item) |
| `src-tauri/src/platform/windows_drop.rs` | `drag_contents()` returning files only, as today |
| `src-tauri/src/drop_batch/watch.rs` | use `drag_contents()`; images count toward showing and the summary |
| `src-tauri/src/drop_batch/dragged.rs` (new) | read on the main thread, write files, return paths and errors |
| `src-tauri/src/drop_batch/zone.rs` | empty-paths `Drop` goes through `dragged` |
| `src-tauri/src/preset_zones.rs` | `drag_contents().files` |
| `src/lib/settings.ts`, `src/lib/settings.fixture.ts` | `save_folder` in the `drop_zone` type and fixture |
| `src/lib/i18n/en.json` | `drop.dragged.unreadable`, `drop.dragged.cant_save` |

## Testing

Automated (TDD, `cargo test --workspace`, `pnpm test`):
- type → extension mapping and preference order;
- file-name format, the unknown-source fallback, and clash suffixes;
- `should_auto_show` with images-only drags; summary counts;
- settings default and round-trip of `save_folder`.

Manual checks (dev app, macOS):
1. **Plan Task 1:** drop a Slack image and compare the saved file's pixel size with the original downloaded from Slack. This decides bytes-first or promise-first.
2. Slack, Chrome and Safari image drags: the zone shows "1 image", and dropping produces a saved file in `~/Downloads/Ferry` and a result card.
3. Finder file drags: unchanged.
4. Dragging text or a link from Chrome: no zone.
5. Unwritable save folder: error card.
