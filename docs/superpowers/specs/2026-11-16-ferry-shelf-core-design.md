# Ferry: built-in shelf, core (2026-11-16)

Status: design, awaiting review. Spec 1 of 3 for the shelf. Spec 2 is the archive tool ("Add to archive ▸"); spec 3 is image intake (Slack image drops saved to `$TMPDIR/Ferry`). Mockups: `.superpowers/brainstorm/47478-1794796844/content/shelf-pile-v7.html`, `shelf-menu-v8.html`.

## Problem

Ferry has a drop zone that optimises, but nowhere to park files while you gather them. Users juggle Finder windows or a separate shelf app, and results Ferry made can't be collected, moved on, or sent back through Ferry in one go.

## Goal

On macOS, a small shelf at the bottom edge holds any files and folders by reference until they are dragged out. It takes drops from any app and from Ferry's own cards, gives the whole pile back in one drag, and can send its contents through Ferry's optimiser (round trip).

Success: drag three Finder files and a result card onto the strip, quit and relaunch Ferry, drag the pile into a Mail draft. All four files are attached and the shelf hides. The optimise zone behaves as before.

## Non-goals

- Archiving (spec 2). Menu row "Add to archive ▸" is not built here.
- Shelving images dragged from Slack and similar apps (spec 3). They show the optimise zone as today but don't expand the shelf.
- Windows. The shelf window is not created there.
- Copying files. The shelf stores paths only; files on disk are never moved, copied or deleted.
- Multiple shelves, naming, or reordering.

## Design

### 1. Look

- Gray panel (`#3a3d44`, 1 px `#555963` border, top corners rounded 14 px), on the bottom edge of the work area, same side as the stack (left or right corner).
- **Hidden** at rest, with or without items (user decision 2026-11-16: a strip that is always on screen is bad UX). It shows during a drag, from the tray's "Show shelf" (open for 4 s), and for the Undo note.
- **Resting** (during a drag only): a flat strip, 16 % of the expanded height, with a grip line "—" inside it (no raised tab) and the item count at the right.
- **Expanded** (hover, or a drag near the strip): file pile preview (up to three stacked thumbnails or file icons, slightly rotated) and a pill "N Items ⌄" at the bottom.
- **Drop target**: expanded with an accent outline and the label "Drop to shelve".
- **Transparency**: the panel's background is `floating.shelf_transparency` percent see-through (default 80, 0–90, Settings → Floating). Content stays solid.
- **Cleared**: the strip shows "Shelf cleared · Undo" for 5 s (§6).
- No ✕ anywhere.

### 2. Architecture

- A separate non-activating `shelf` window (NSPanel, like the drop zone's `platform/macos_drop.rs::make_zone_window`), page `src/shelf.html` → `src/shelf/Shelf.svelte`, added to the vite inputs.
- The window is always exactly the visible panel: the strip height at rest and the expanded height when open. It grows upward and is never positioned offscreen.
- `ferry_core::shelf`: items are `{ path, added_at }`. It provides add (dedupe by canonical path, returns how many were added), remove, clear with a single-level undo, and prune of missing paths. The list persists to `shelf.json` in `app_data_dir`. A corrupt file starts empty and keeps a `.bak`, as settings do.
- `ferry_core::shelf_place` (pure): shelf, stack and zone rects from work area, corner, sizes and state (§4).
- `src-tauri/src/shelf/`: commands (`shelf_items`, `shelf_add`, `shelf_remove`, `shelf_clear`, `shelf_undo`, `shelf_menu`), window placement, and the native menu.
- `src/shelf/strip.ts` (pure): the strip state machine and labels (§8).

### 3. When the shelf shows and expands

The drag watcher (`drop_batch/watch.rs`) already samples the cursor every 30 ms while a drag is in progress. It gains the shelf decision next to the zone's.

| Drag | Optimise zone | Shelf strip | Expands to "Drop to shelve" |
|---|---|---|---|
| Files/folders from another app | as today | shown (even when empty) | pointer within 80 pt of the strip and not inside the zone rect |
| Ferry's own result cards / Drag all (`all_own`) | hidden, as today | shown | same rule |
| The shelf's own pile or a "Show items…" row | hidden | stays expanded, no drop target | never (no self-drop) |
| Image-only drag (Slack etc., `images > 0`, no files) | as today | no change | never (until spec 3) |
| Ignored frontmost app (`should_auto_show` false for that reason) | hidden | no change | never |

"No change" means the shelf stays as it was before the drag: resting if it has items, hidden if empty.

- **Shelf drag flag.** `all_own` matches only job outputs, so a pile drag would look like an outside drag. The app keeps `ShelfDragging: AtomicBool`, set by a command just before the page calls `startDrag`. While it is set, the watcher hides the zone, the shelf offers no drop target, and a Tauri `DragDropEvent::Drop` on any Ferry window (zone, stack, shelf) is ignored (no intake).
- **Drop back onto Ferry keeps the items.** Tauri accepts every drop, so the callback reports `Dropped` even over Ferry. In the callback, the shelf checks the cursor against the visible Ferry windows' rects. Inside one, it treats the drop as `Cancelled` and the items stay. The flag is cleared 1 s after the callback, so the window's Drop event is ignored whichever order the two arrive in.
- **Hover** (no drag): the pointer entering the strip expands it; leaving collapses it after 300 ms.
- **Drag ends** without a drop on the shelf: it returns to resting, or to hidden if empty.

### 4. Placement

- **Monitor and corner.**
  - The side (left/right) comes from `floating.corner`.
  - With a visible stack, the shelf uses the stack's monitor, so the two never split.
  - With no stack, it uses the monitor `windows::stack_resize` would pick: under the cursor when `follow_cursor` is on, otherwise the primary.
- **When it re-places.** The shelf is placed on drag start, at app launch, and on each `stack_resize`. It is never placed continuously, so the strip doesn't hop screens as the cursor moves.
- **Bottom corners.**
  - When the shelf has items, the stack sits directly above the resting strip: `stack_resize` subtracts the strip height.
  - An empty shelf's strip, shown only during a drag, overlays without moving the stack, so cards don't hop at each drag start and end.
  - When the shelf expands, the stack is pushed up so its bottom meets the expanded shelf's top, as in the mockups. It moves back on collapse.
  - Collapse waits 300 ms after the pointer leaves, so a brief exit doesn't bounce the stack.
  - While a drag shows the zone, the zone is placed as if the stack were already pushed, so a later push never runs the stack into it.
- **Top corners.** The stack stays at the top; the shelf stays at the bottom edge on the same side.
- **Zone and shelf never overlap.**
  - While a drag shows both, every zone placement in `zone.rs` is kept clear of the expanded shelf's rect: if the zone's rect would intersect it, the zone moves up until its bottom is at the shelf's top. That covers:
    - `zone_near_results`, including its `geometry::place` fallback with no stack
    - Tab mode (`tab_at_edge`)
    - `zone_from_tab`
    - under-cursor (`zone_under_cursor`)
  - The shelf doesn't expand while the pointer is inside the zone rect.
  - If both still can't fit, the zone wins and the shelf stays resting. The zone panel is above the shelf.
- **Dock auto-hide.** The work area reaches the screen edge, so the strip sits at the very bottom and the Dock may cover it while summoned. This is accepted, and is a manual check.

### 5. Getting files on and off

**On:**
- Drop files/folders from any app onto the expanded shelf.
- Drag a result card onto it.
- "Shelve all" button on `DragAllBar` / `SelectionBar`.
- "Ferry Shelf" as the first row in the card menu's "Add to Shelf ▸" (`integrations/menu.rs`).
- Paste in the shelf menu adds the clipboard's file URLs.
- Duplicates are skipped silently.

**Off:**
- Drag the pile (`@crabnebula/tauri-plugin-drag` `startDrag({ item: paths, icon }, onEvent)`).
  - `Dropped` removes the items, unless ⌥ was held at drag start.
  - `Cancelled` keeps them.
- Show items… opens a list in the expanded shelf. Each row can be dragged out singly (same rules), and right-click offers Remove.

### 6. Menu

Clicking "N Items ⌄" opens a native menu (like `menu.rs::popup`):

- Header "N Items · total size" (disabled)
- Show in Finder (reveals all items)
- ─
- Copy (file URLs to the clipboard). Recorded as Ferry's own write, as `drop_batch/list.rs` does (`write_files`, then `log.wrote`), so the clipboard watcher doesn't turn the copy into jobs.
- Paste
- ─
- Optimise all
- Show items…
- ─
- **Clear shelf** (red)

**Optimise all**:
- Sends supported items through normal intake. They appear as cards, as a drop would.
- Each finished result replaces its source item on the shelf. Unsupported items stay.
- If nothing is supported: "Nothing here can be optimised".

**Clear shelf**:
- Empties the list. The strip stays visible in the cleared state for 5 s with "Shelf cleared · Undo"; Undo restores the list.
- Adding anything during those 5 s dismisses Undo, and the cleared list is dropped.
- After 5 s the strip hides.

### 7. Persistence and errors

- Saved on every change. Missing paths are pruned on launch and each time the shelf expands.
- A partial add failure (path unreadable or vanished) shows "Couldn't add N item(s)" through `jobs::notice`; the rest are added.
- A save failure shows "Couldn't save the shelf" once per session. The in-memory list keeps working.

## Files

- New:
  - `crates/ferry-core/src/shelf.rs`
  - `crates/ferry-core/src/shelf_place.rs`
  - `src-tauri/src/shelf/{mod,commands,menu,window}.rs`
  - `src/shelf.html`
  - `src/shelf/Shelf.svelte`
  - `src/shelf/strip.ts` + `strip.test.ts`
- Changed:
  - `crates/ferry-core/src/lib.rs`
  - `crates/ferry-core/src/drag.rs` (zone bottom inset)
  - `src-tauri/src/drop_batch/watch.rs` (shelf decision, drag flag)
  - `src-tauri/src/drop_batch/zone.rs` (ignore drops while the flag is set, inset)
  - `src-tauri/src/windows.rs` (`stack_resize` strip offset)
  - `src-tauri/src/integrations/menu.rs` (Ferry Shelf row)
  - `src/stack/DragAllBar.svelte`, `SelectionBar.svelte` (Shelve all)
  - `src/lib/i18n/en.json`
  - `vite.config.ts`
  - Tauri capabilities for the new window

## Testing

- **Rust TDD, `ferry_core::shelf`:**
  - add dedupes
  - add reports the partial count
  - remove
  - clear then undo
  - an add after clear drops the undo
  - prune missing
  - round-trip to `shelf.json`
  - corrupt file → empty plus `.bak`
- **Rust TDD, `shelf_place`:**
  - bottom-right/left rects
  - top corners keep the shelf at the bottom
  - stack offset by the strip only when the shelf has items, and by the expanded height when expanded
  - the zone placed during a drag clears the pushed stack
  - shelf on the stack's monitor when a stack is visible
  - zone and expanded shelf never intersect: near-results with and without a stack, tab, from-tab, under-cursor
  - zone wins when space is short
- **Rust, `watch.rs` plan tests:** one per table row in §3, including the shelf drag flag hiding the zone.
- **vitest, `strip.ts`:**
  - states hidden / resting / hover / drop-target / cleared and their transitions
  - count label
  - menu entries for empty and non-empty lists
  - cleared state dismissed by an add
- **Manual, in the dev app**, driven by the mouse driver plus `screencapture`:
  - Finder drop
  - card drop
  - Shelve all
  - pile drag into Finder, then ⌥ drag, then a drag back onto the stack (items stay)
  - Copy doesn't create jobs
  - Esc-cancel drag
  - Optimise all
  - Clear and Undo
  - relaunch persistence
  - top-corner setting
  - two monitors with `follow_cursor`
  - Dock auto-hide

## Decisions (confirmed by the user 2026-11-16)

1. **The expanded shelf pushes the stack up** (bottom corners), as the mockups show.
2. **Top-corner stacks keep the shelf at the bottom edge**, same side.
