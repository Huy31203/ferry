# Ferry Cycle 3 · Stream D (drop-batch) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Files enter Ferry by drag and drop: a drop zone that appears while files are dragged (or on a modifier tap; on Windows, by default, only on a modifier tap), a batch window that runs hundreds or thousands of files with progress, pause, cancel, re-run and roll back, and a compact results list with multi-select and a selection bar, all over the existing `Job` / `JobUpdate` card model.

**Architecture:** Pure logic goes in three new `ferry-core` modules: `intake` (classify, folder scan, drop summary, routing), `drag` (drag and modifier-tap detection from polled samples, zone placement) and `batch` (rows, bounded work queue, tally, pace). The app gets one module tree, `src-tauri/src/drop_batch/`, plus `platform/{macos,windows}_drop.rs`. It owns a hidden non-activating **drop zone** window, a polling **drag watcher** thread, a normal **batch** window fed by a dispatcher thread, and compact-list commands. Batch rows are ordinary jobs started through `jobs::start`. A three-line hook in `jobs.rs` sends their updates to the batch window instead of the stack. The compact list is a second layout of the same `JobUpdate[]` inside the stack page.

**Tech Stack:** Tauri 2.12 (`WindowEvent::DragDrop` / `DragDropEvent`, `tauri::menu`, `tauri-plugin-global-shortcut` 2.4 / `global-hotkey` 0.8, `tauri-plugin-dialog` 2.8, `tauri-plugin-drag` 2.1), `tauri-nspanel` 2.1, `objc2-app-kit` 0.3.2, `windows-sys` 0.61; Svelte 5 + TypeScript, vitest 5, svelte-check 4; Rust 2024. **No new dependencies** (two feature flags on existing crates, see Dependencies).

**Spec:** [Cycle 3 stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding: stream D row, "Cross-stream contract", "Shared files and merge rules", "Plan requirements"). Behaviour: [requirements](../../requirements/ferry-optimize-requirements.md) R§2.1 stories 3 and 5, FR-T6, FR-T8, R§2.10 (Option/Alt tap, Cmd/Ctrl+A, Esc), NFR-P4, NFR-P6, NFR-P7, NFR-R1, R§3.3, R§3.6, R§4.2, R§4.4, R§4.5, R§4.8, R§4.10, R§5 ("Show drop zone when any drag starts", "Modifier tap while dragging"). Architecture it extends: [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md) §4.3, §4.4, §5.4, §7, and [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) §4.3 to §4.5.

## Decisions and why

| Topic | Decision | Why |
| --- | --- | --- |
| What the drop zone is | A floating, transparent, **non-activating** webview window (`drop`), created hidden at startup like the stack. It is 220 × 140 content inside a 244 × 164 window (12 px of padding leaves room for the 1.04 hover scale and the shadow). It sits near the results corner, or centred under the pointer when summoned by a tap. It is not a tray target and not a permanent screen-edge strip | R§4.8 says a panel near the results corner or under the cursor. The window is a real drop target, so Tauri's own `DragDropEvent` supplies the paths, the hover and the drop on both OSes with no native drop code. Dropping onto the menu-bar icon (R§4.1) is deferred: Tauri's tray has no drop API (user decision, see "Questions decided by the user") |
| Seeing a drag start (macOS) | A watcher thread polls `NSEvent.pressedMouseButtons`, `NSEvent.modifierFlags` (class properties) and the drag pasteboard's `changeCount`. It polls every 100 ms while the button is up and every 30 ms while it is down. A pasteboard change while the button is held means a drag started. The drag pasteboard's `public.file-url` items say whether it carries files and which ones | R§5 names exactly this ("poll the drag pasteboard plus a global mouse monitor"). The class properties need no Input Monitoring or Accessibility permission, unlike a global event monitor or `CGEventTap`. About 10 cheap reads a second while idle keeps NFR-R1 (under 1 % CPU) |
| Seeing a drag start (Windows) | No public drag-start signal exists (R§5, high risk). `GetAsyncKeyState` and `GetCursorPos` are polled the same way. **Default: modifier tap only** (user decision, 2026-10-23): `drop_zone.show_on` defaults to `"modifier"` on Windows (`"any_drag"` on macOS), so the zone appears only when the zone modifier is tapped while the button is held, centred under the pointer. **Opt-in edge tab:** with `show_on = "any_drag"` on Windows, a left-button press that moves 24 px and lasts 300 ms, and that did not start on a Ferry window, shows a **slim tab** (a 24 × 140 window) at the results edge; a drag that enters the tab turns it into the full zone. The tap works in both modes | Both are the requirements' own fallback ("modifier tap or a slim screen-edge tab"). A moving press also happens on window moves and text selection, so as a default the tab would flash all day; it stays available for users who want it. No hooks, no DLL injection. Unverified on a real desktop (C1 §1) |
| Modifier taps | A pure `DragWatch` state machine turns samples into `DragStarted`, `PressMoved`, `DragEnded` and `Tap(Modifier)`. A tap is one modifier alone, pressed and released within 350 ms while the mouse button stays down. Taps of the zone modifier (default Option/Alt, R§2.10) toggle the zone. Every other tap is re-emitted app-wide as `drag:tap` for stream F's preset zones | Testable without a desktop; F needs the same signal for "Tap Control" |
| Keep originals | Holding the keep modifier (default **Shift**) at the drop marks the jobs. Their runs and restores use the File handling rules with `In place` replaced by `Same folder` (`%f_optimised.%e` / `%f.%e`), so the source file is never written | FR-T6 "holding a modifier while dropping keeps the original untouched". Reuses Cycle 2's output rules instead of a new write path |
| Routing a drop | `route`: any folder, or more top-level files than `drop_zone.batch_threshold` (default 20), goes to the batch window. Otherwise each file becomes a file job with its own card, exactly like a hotkey file job (`Job::file` + `jobs::start`) | FR-T8. Small drops behave like today's copied files |
| Folder recursion | `intake::scan` walks folders itself with no new crate. It does not follow symlinked folders, and it skips hidden entries (leading `.`), macOS packages by extension (`.app`, `.photoslibrary`, …) and Windows system folders, plus Ferry's own backups and working folders. It reads only the first 64 bytes of each file to sniff its type (content, never extension), dedupes by canonical path and stops at 10 000 files | Recursing into a `.photoslibrary` and optimising in place would corrupt the Photos library (R§3.3 "original is sacred"). Reading heads, not whole files, keeps a 5 000-file scan to seconds (NFR-P7) |
| Supported types | Only what `intake::classify` accepts, which today is `sniff::sniff` (PNG, JPEG, GIF, WebP) → `MediaKind::Image`. `MediaKind` already has `Video`, `Audio` and `Pdf` for streams A, B and E to fill in | One place to extend; drop zone, batch and folder scan agree |
| Batch model | No new job model. Every batch row becomes an ordinary file `Job` registered in `AppState.jobs`. A dispatcher thread creates at most `available_parallelism()` jobs at a time (the same cap as the encoder semaphore, NFR-P6), and `Job::file` reads a file only when it is dispatched. Updates for batch jobs are diverted from the stack to the batch window, and they skip "Recent results" | Reuses backups, restore, edits, cancel and source-state safety as they are. A bounded dispatcher means 5 000 files never queue 5 000 tasks or 5 000 full file reads |
| "Backing up N of M" phase (R§4.10) | **Dropped.** Each file is backed up immediately before its own first write (existing `Job::run`). | Same guarantee (no original changes before its backup exists), the first results arrive at once, and disk I/O is not doubled up front. Confirmed by the user |
| Re-run and roll back | The Adjust panel holds compression (or Extreme), downscale % and convert-to. "Re-run with new settings" sends one `EditsPatch` built from it to each finished row (`jobs::edit`; `jobs::rerun` for rows without an editable original), through the same bounded queue. "Roll back all" calls `jobs::restore` for each Done row | R§2.1 story 5 on the Cycle 2 recipe model (brief: extend it, no parallel edit model) |
| Batch progress | Rust coalesces row changes and flushes at most every 100 ms (`batch:rows` carries only the changed rows, `batch:status` the tally, phase, files/s and ETA). The table is virtualised (fixed 32 px rows), and its order is recomputed only when the user sorts, filters or rows are added, never on a status change | NFR-P7 "smooth with 5,000 files (virtualised list, incremental progress)". 15 000 job events become at most 10 IPC messages a second |
| Virtualisation | A 40-line helper `src/lib/virtual.ts` (`visibleRange`), shared by the batch table and the compact list, with no npm package | Fixed row heights make windowing arithmetic trivial. Existing Svelte virtual-list packages target Svelte 3/4 and would add a dependency for less than a screen of code |
| Compact list | The stack switches to a list of 44 px rows when it holds more than `results_list.after_cards` (default 5, R§4.4) cards. The mode never flips while the pointer is on the stack, a selection exists, a tool is open, the list has the keyboard or a drag runs. Rows are the same `JobUpdate`s, upserted in place (never reordered), and the list shows at most 8 rows at a time and scrolls | R§4.5, including "never reorders under the cursor while the user is selecting" |
| Selection bar | Drag set, Save to…, Crop, Downscale ▸, Compression ▸, Convert ▸, Copy, Remove, Select all, Clear. Drag, Save to and Copy act on rows with a file. Crop, Downscale, Compression and Convert act on rows whose `can_edit` is true. Remove closes every selected row (cancelling busy ones). "Bitrate" (R§4.5) is the image Compression menu until streams A and B give video and audio cards | Only tools that exist on cards today; each one goes through the same `jobs::edit` / `jobs::convert` as a single card |
| List keyboard | The list acts on keys **only after the user clicks it** (user decision, 2026-10-23); pointing at it takes nothing. The click (a row's checkbox, a row while something is selected, or the list body outside the rows) calls the existing `stack_focus(true)`, the same path rename and crop use, so the stack becomes the key window without activating Ferry and the keys arrive as ordinary DOM key events: ⌘/Ctrl+A, Esc, ↑/↓, ⇧↑/⇧↓, plus Space, ⌘/Ctrl+D, `-` and `1`–`9` on the cursor row. No global shortcut is registered for the list, and `floating.hover_hotkeys` does not affect these keys. The keyboard goes back on a click elsewhere, on Esc with nothing selected, after Copy, before a preview or compare opens, and when the list closes. Without a click, Cycle 2's hover hotkeys still act on the row under the pointer, as on cards | R§2.10 "Select all / clear in list" and R§3.6 keyboard reach, after one click. ⌘A, Esc and the arrows are too common in other apps to take from them on hover |
| Run state | Drops and the drop zone ignore Running / Paused / Stopped | A drop is an explicit request, like a hotkey or a card action (C1 §5.1) |

## Global Constraints

- Out of scope (brief): secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP automation. Also not in this plan: preset zones (stream F), auto-copy after drop (stream I), video/audio/PDF types (A, B, E).
- Shared files: additions only, at the anchors in "Shared files and anchors" below. Never reorder or reformat existing lines. If git conflicts with another stream, keep both sides.
- Settings: two new sections, `drop_zone` and `results_list`, each `#[serde(default)]` with its own `extra` map. `settings::VERSION` stays 2.
- i18n: every new string is a `drop.*` key in `src/lib/i18n/en.json` (a flat map shared with Rust via `ferry_core::i18n::{t, tf}`). Reuse existing keys only where this plan says so. Never hard-code English in Rust or Svelte. `t()` in TS is typed on the keys, so a missing key fails `pnpm check`.
- New windows: the drop zone gets one added arm in `windows::follows_screenshot_setting` (`src-tauri/src/windows.rs`). The batch window is a normal window like Settings and follows no screenshot setting.
- Never use `var` in JavaScript or TypeScript.
- Models: the implementer runs on `sonnet`; reviews on `opus`; never `fable`.
- Commit trailer: the `Co-Authored-By:` line your session's system instructions give (at the time of writing `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).
- Each task ends with `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, the task's tests, `pnpm check`, `pnpm test`, then a commit.
- `cargo test` takes one name filter per command; run two commands for two filters.
- Every new Rust module opens with a `//!` doc comment; tests live in `#[cfg(test)] mod tests` at the end of the file. Match the surrounding comment density and naming (tests read as sentences, e.g. `a_folder_is_walked_in_name_order`).
- No test stack is added (no jsdom, no testing-library, Tauri `test` feature off). Component behaviour is tested through pure helpers in `src/lib/*.ts` and pure Rust, plus the manual checks in Task 11.
- Ask the user before running the live app (`pnpm tauri dev` takes over the real clipboard). Drive the UI with a TextEdit window as the focus target; don't click into other apps.
- Windows: code that only builds on Windows lives in `src-tauri/src/platform/windows_drop.rs` (compiled only under `cfg(windows)` through `platform/mod.rs`). No Windows target is installed on the Mac; CI's `bundle (windows)` job runs clippy and `cargo test -p ferry` on it. Windows behaviour is **built but not verified on a real desktop**; say so in every status report.
- Exact copy from R§4.8, R§4.10 and R§4.5: "Drop to optimise", "Originals will be kept", "Can't optimise .zip" (`Can't optimise .{ext}`), "42 files · opens Batch", "3 images, 1 video", "Drop files or a folder", "1,204 files · 2.1 GB → 640 MB · −70%", "23 files · 41.2 MB saved (−68%)", statuses "Waiting", "Working", "Done", "Already optimal", "Skipped", "Failed", "Restored". Tray items "Open batch window" and "Show drop zone" (R§4.2), placed right after "Optimise clipboard aggressively".
- Sizes (R§4.8, R§4.10, R§4.5): zone panel 220 × 140, hover scale 1.04; batch window 960 × 640, resizable, minimum 720 × 480; list row height 44; batch table row height 32; compact list shows at most 8 rows; cards before the list: 5.
- Motion: zone hover scale and drop collapse within 180 ms; no motion under `prefers-reduced-motion`. Colours only from `src/lib/theme.css` tokens. Every status pairs colour with an icon and words (C1 §7).
- Threads: no blocking work on the main thread (NFR-P4). File reads, scans, dialogs and job starts run on `tauri::async_runtime::spawn_blocking` or D's own threads. AppKit window and panel calls (show, hide, resize, move of the zone) go through `app.run_on_main_thread`. The NSEvent class properties and the pasteboard reads in `macos_drop.rs` may run on the watcher thread (the clipboard watcher already reads pasteboards off the main thread).
- Lock rule: never call `jobs::*`, `tray::*` or emit while holding `DropState.batch` or `DropState.zone`. `jobs::show` calls back into `drop_batch::batch::claim`, which takes `DropState.batch`; a held lock would deadlock (std `Mutex` is not re-entrant).
- Environment, from the worktree root:
  ```
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  ```
- Tauri command arguments are camelCase on the JS side. Every D command parameter is a single word (`ids`, `edits`, `adjust`, `over`, `selecting`, `kind`, `x`, `y`, `id`), so nothing needs converting. Payload fields stay snake_case (serde).

## Review Focus

1. **Ferry's own drag-out pops the zone or re-optimises its own output.** Dragging a card, "Drag all", a selection or batch rows into Finder must never show the zone. A result dropped back on a tap-summoned zone gets a notice and is left alone, never re-optimised in place. Pinned in Task 2 (`ferrys_own_results_are_not_taken_as_new_input`), Task 3 (`the_zone_auto_shows_only_for_other_apps_file_drags`, `a_press_on_a_ferry_window_never_shows_the_tab`) and Task 11 check 6.
2. **Folder drops that reach what they must not.** Packages (`Photos Library.photoslibrary`, `Foo.app`), hidden folders (`.git`, `.Trash`), `$RECYCLE.BIN`, symlink loops, Ferry's own backups folder, and a package dropped directly must never be walked, read past 64 bytes, or optimised. Expected: they are skipped and the rest of the tree is processed. Pinned in Task 2 (`hidden_files_and_folders_packages_and_system_folders_are_skipped`, `a_dropped_package_is_not_opened`, `symlinked_folders_are_not_followed`, `ferrys_own_folders_are_excluded`) and Task 11 check 4.
3. **A deadlock between the job hook and the batch dispatcher.** `jobs::start` → `show` → `claim` takes the batch lock, so the dispatcher must have released it. Expected: a 1 000-file batch never freezes the app, and Cancel all during dispatch never hangs. Pinned in Task 4 (`pump_releases_the_lock_before_running_work`), plus the Global Constraints lock rule, which the reviewer checks in `drop_batch/batch.rs`.
4. **The compact list moving under the pointer.** New cards arrive, cards auto-hide, or the count crosses the threshold while the user is selecting or pointing. Expected: no row moves or disappears, and the layout does not flip until the pointer leaves, the selection is empty and the list no longer has the keyboard. Pinned in Task 9 (vitest `nextListMode` "holds the current layout while held", `rowsHeld`) and Task 11 check 9.
5. **The same file twice.** A file dropped twice, a folder and a file inside it in one drop, a file added again to a running batch, or a path through a symlink. Expected: one row and one job, never two writers on one file. Pinned in Task 2 (`the_same_file_twice_is_one_candidate`) and Task 4 (`a_path_already_in_the_batch_is_not_added_twice`). `jobs::start`'s existing `output_busy` check remains the last guard.
6. **Keys taken from another app.** With the pointer resting on the compact list but no click, ⌘/Ctrl+A, Esc and the arrows must reach the frontmost app. After a click they act on the list until the keyboard is handed back. Expected: `src-tauri/src/drop_batch/list.rs` registers no global shortcut; the list's key handler runs only while `listFocused` is true; a click in another app, Esc with nothing selected, Copy, a preview or compare, and the end of list mode all hand the keyboard back. Pinned in Task 10 (vitest `listKey`, `escapeSel`) and Task 11 check 9.
7. **Windows edge tab on by default.** On Windows the zone must appear only on a modifier tap unless the user picks "Whenever files are dragged". Expected: `DropZone::default().show_on` is `Modifier` off macOS, and `plan(PressMoved, …)` is `Nothing` under it. Pinned in Task 1 (`drop_zone_defaults_match_the_plan`) and Task 8 (`windows_shows_the_tab_only_when_auto_show_is_turned_on`).

---

## Setup (once, before Task 1)

```bash
cd /Users/gos/code/rust/ferry
git worktree add .worktrees/sD -b feat/c3-sD-drop-batch main
cp -R src-tauri/binaries .worktrees/sD/src-tauri/
cd .worktrees/sD
source "$HOME/.cargo/env"
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
pnpm install --frozen-lockfile
grep -n "pub fn follows_screenshot_setting" src-tauri/src/windows.rs
cargo test --workspace 2>&1 | grep "test result"
pnpm test 2>&1 | tail -3
```

Expected: the `grep` prints one line; every `test result` line says `ok`; vitest passes. Keep a ledger at `.superpowers-ledger-sD.md` (uncommitted), one line per task, plus a `Ruling:` line for every deviation from this plan.

## Shared files and anchors

| File | Edit | Anchor |
| --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | `pub mod batch;`, `pub mod drag;`, `pub mod intake;` | rustfmt sorts `pub mod` lines; add the three lines and let `cargo fmt` place them |
| `crates/ferry-core/src/settings.rs` | fields `pub drop_zone: DropZone,` and `pub results_list: ResultsList,` in `Settings`; two lines in `impl Default for Settings`; one call `self.sanitise_drop();` in `sanitised`; a type block `// ---- Stream D: drop zone and results list ----` | fields after `pub hotkeys: Hotkeys,`; defaults after `hotkeys: Hotkeys::default(),`; the call right before `self.version = self.version.max(VERSION);`; the type block right before `/// How `Settings::load` found the file.`; tests appended at the end of `mod tests` |
| `src-tauri/src/lib.rs` | `mod drop_batch;`; D commands; `drop_batch::init(app)?;` | `mod` line sorted by rustfmt (between `mod compare;` and `mod hotkeys;`); commands appended after `commands::pick_folder,` under a `// Stream D: drop-batch` comment; `drop_batch::init(app)?;` right after `menu::init(app);` |
| `src-tauri/src/jobs.rs` | three hooks (Task 5) | first statement of `show`; the `state.remember(&update);` and `tray::rebuild(app);` lines of `finish`; the line after `let options = run_options(app, cancel.clone());` in `run`; the `let options = run_options(app, CancelToken::new());` line of `restore` |
| `src-tauri/src/windows.rs` | one match arm for the drop zone | body of `follows_screenshot_setting` (see Task 7). If another stream has already turned it into a `match`, add only the arm |
| `src-tauri/src/tray.rs` | two menu items and two `on_menu` arms | `let batch = crate::drop_batch::batch::tray_item(app)?;` (Task 5) and `let zone = crate::drop_batch::zone::tray_item(app)?;` (Task 7) after the `aggressive` item is built; `&batch,` then `&zone,` right after `&aggressive,`; the arms right before `"settings" => windows::open_settings(app),` |
| `src-tauri/src/platform/mod.rs` | `pub mod drop;` per OS | a `#[cfg(target_os = "macos")] #[path = "macos_drop.rs"] pub mod drop;` block after the macOS `pub mod preview;` block, and the `#[cfg(windows)] #[path = "windows_drop.rs"]` twin after the Windows `pub mod preview;` block |
| `src-tauri/Cargo.toml` | features `"NSEvent"` on `objc2-app-kit`; `"Win32_UI_Input_KeyboardAndMouse"` on `windows-sys` | append to the end of each existing `features = [...]` list |
| `src-tauri/capabilities/default.json` | `"drop"`, `"batch"` in `windows` | appended after `"compare"` |
| `vite.config.ts` | `drop: page('drop.html'),`, `batch: page('batch.html'),` | after `compare: page('compare.html'),` |
| `src/lib/i18n/en.json` | every `drop.*` key | one block right after the line `"error.busy": "{name} is already being optimised",`, in task order |
| `src/lib/settings.ts`, `src/lib/settings.fixture.ts` | `drop_zone` and `results_list` | after the `hotkeys` member of the interface / the fixture object |
| `src/lib/schema.ts` | `'drop'` pane id and pane; D's rows | `PaneId` union and `panes` array: appended after `floating`; rows: a `// Stream D` block at the end of `all` |
| `src/stack/Stack.svelte` | list mode (Task 9); selection crop and list focus (Task 10) | inside the existing `{#each}` area: wrap it in `{#if listMode}<CompactList …/>{:else}…existing markup…{/if}`; new state and effects after the `frame` declaration; list focus changes two existing lines only: `const on = tool !== null;` in the `stack_focus` effect becomes `const on = tool !== null \|\| listFocused;`, and `windowBlur` gains `listFocused = false;` as its first statement |
| `src/stack/DragAllBar.svelte` | `fan`/`load` move to `src/lib/dragicon.ts` (Task 10) | delete the two local functions, import `fanIcon` |
| `Cargo.lock` | none expected (features only) | — |

Not edited: `commands.rs`, `state.rs`, `menu.rs`, `Card.svelte`, `src/lib/ipc.ts`. The brief names `src/lib/types.ts`, which does not exist; the TS mirrors of Rust types live in `src/lib/ipc.ts`. D keeps its own in a new `src/lib/drop-ipc.ts` so that `ipc.ts` stays untouched.

## Cross-stream hooks this plan provides

- **A, B, E (new media):** to make drops, batches and folder scans accept a type, extend `ferry_core::intake::classify(head: &[u8]) -> Option<Classified>` (it reads the first `intake::HEAD_LEN` = 64 bytes) to return `MediaKind::Video | Audio | Pdf`, and make `pipeline::Job::file` accept the file. Nothing else in D changes; the zone's sub-label already pluralises every kind.
- **F (preset zones, pipelines on drop):** every modifier tap during a drag, other than the zone modifier, is emitted app-wide as Tauri event `drag:tap` with payload `{ "modifier": "alt" | "shift" | "control" | "command" }`. `ferry_core::drag::DragWatch` and `drop_batch::zone::cursor_monitor` are `pub`. To run a pipeline on dropped files, F wraps `drop_batch::intake::accept(app, paths, keep)`. F adds the zone's hint row ("Tap Ctrl for presets") itself.
- **I (auto-copy after drop):** `drop_batch::came_from_drop(app, &JobId) -> bool`.
- **G (settings sync), for the integrator:** `drop_zone.show_on` and `drop_zone.ignored_apps` have per-OS defaults (Windows: modifier tap only; Windows exe names). If G syncs settings between computers, these two keys belong with the keys that sync only between same-OS computers, so a Mac never turns on the Windows edge tab.

## New settings keys

| Key | Type | Default | Range / values |
| --- | --- | --- | --- |
| `drop_zone.enabled` | bool | `true` | |
| `drop_zone.show_on` | enum | macOS: `"any_drag"`. Windows: `"modifier"` | `"any_drag"` (on Windows this is the opt-in edge tab), `"modifier"` (tap only) |
| `drop_zone.position` | enum | `"near_results"` | `"near_results"`, `"under_cursor"` |
| `drop_zone.modifier` | enum | `"alt"` | `"alt"`, `"shift"` (tap to show/hide) |
| `drop_zone.keep_original_modifier` | enum | `"shift"` | `"shift"`, `"alt"`; never equal to `modifier` (sanitised to the other one) |
| `drop_zone.batch_threshold` | u32 | `20` | clamped 1..=10 000 |
| `drop_zone.ignored_apps` | string[] | macOS: `com.apple.FinalCut`, `com.apple.iMovieApp`, `com.blackmagic-design.DaVinciResolve`, `com.adobe.PremierePro`, `com.adobe.AfterEffects`, `com.adobe.Photoshop`, `com.adobe.illustrator`, `com.figma.Desktop`, `com.bohemiancoding.sketch3`, `com.seriflabs.affinitydesigner2`, `com.seriflabs.affinityphoto2`. Windows: `Adobe Premiere Pro.exe`, `AfterFX.exe`, `Photoshop.exe`, `Illustrator.exe`, `Resolve.exe`, `Figma.exe`, `Affinity Designer 2.exe`, `Affinity Photo 2.exe` | case-insensitive **prefix** match; blank entries dropped |
| `results_list.after_cards` | u32 | `5` | clamped 1..=50 |

## Dependencies

No new crates, npm packages or binaries. Feature flags only on crates already in the lock file (cargo unifies features, so `Cargo.lock` does not change):

| Crate | Version (locked) | Licence | Added feature | Why |
| --- | --- | --- | --- | --- |
| `objc2-app-kit` | 0.3.2 | MIT | `NSEvent` | `NSEvent::pressedMouseButtons()`, `NSEvent::modifierFlags_class()` |
| `windows-sys` | 0.61.2 | MIT OR Apache-2.0 | `Win32_UI_Input_KeyboardAndMouse` | `GetAsyncKeyState`, `VK_*` |

None is GPL/LGPL/AGPL or patent-encumbered.

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/settings.rs` (modify) | `DropZone`, `ResultsList` and their enums, defaults, sanitising | 1 |
| `crates/ferry-core/src/intake.rs` (new) | `classify`, `scan`, `summarise`, `route`, `ZoneView`, `keep_source_rules`, own-output filters | 2 |
| `crates/ferry-core/src/drag.rs` (new) | `Sample`, `DragWatch`, taps, `should_auto_show`, `is_ignored_app`, zone/tab placement | 3 |
| `crates/ferry-core/src/batch.rs` (new) | `BatchRow`, `RowStatus`, `Adjust`, `Work`, `BatchRun`, `Tally`, `BatchStatus`, `Phase`, `Snapshot`, `pump` | 4 |
| `src-tauri/src/drop_batch/mod.rs` (new) | `DropState`, `init`, `with_keep`, `came_from_drop`, menu-event routing | 5 |
| `src-tauri/src/drop_batch/batch.rs` (new) | batch window, dispatcher and flush threads, `claim`/`owns`, batch commands, row menu | 5 |
| `src-tauri/src/jobs.rs` (modify) | three hooks | 5 |
| `src/batch.html`, `src/batch/{main.ts,Batch.svelte,BatchTable.svelte,AdjustPanel.svelte}` (new) | batch window UI | 6 |
| `src/lib/virtual.ts`, `src/lib/batchview.ts` (+ tests) (new) | windowing; sort, filter, toolbar state, status text | 6 |
| `src/lib/drop-ipc.ts` (new) | TS mirrors of D's Rust types | 6 (extended in 7, 9, 10) |
| `src-tauri/src/drop_batch/{zone.rs,intake.rs}` (new) | zone window, view state, `DragDropEvent` handling; drop → cards or batch | 7 |
| `src-tauri/src/platform/{macos_drop.rs,windows_drop.rs}` (new) | zone panel / tool window (7); sampling, drag files, frontmost app (8) | 7, 8 |
| `src/drop.html`, `src/drop/{main.ts,DropZone.svelte}`, `src/lib/zone.ts` (+ test) (new) | zone page and its text | 7 |
| `src-tauri/src/drop_batch/watch.rs` (new) | watcher thread: samples → signals → zone | 8 |
| `src/stack/{CompactList.svelte,ListRow.svelte}`, `src/lib/selection.ts` (+ test) (new) | compact list, selection model | 9 |
| `src/stack/SelectionBar.svelte`, `src/lib/dragicon.ts` (new); `src-tauri/src/drop_batch/list.rs` (new) | selection bar; list commands, menus and keys | 10 |

---

### Task 1: Settings sections and the Drop zone pane

**Files:**
- Modify: `crates/ferry-core/src/settings.rs`, `src/lib/settings.ts`, `src/lib/settings.fixture.ts`, `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/lib/i18n/en.json`

**Interfaces:**
- Produces (Rust, `ferry_core::settings`; all `Debug, Clone, PartialEq, Serialize, Deserialize`; enums also `Copy, Eq` with `#[serde(rename_all = "snake_case")]`):
  ```rust
  pub enum ZoneShowOn { AnyDrag, Modifier }
  pub enum ZonePosition { NearResults, UnderCursor }
  pub enum DragKey { Alt, Shift }
  #[serde(default)] pub struct DropZone { pub enabled: bool, pub show_on: ZoneShowOn, pub position: ZonePosition,
      pub modifier: DragKey, pub keep_original_modifier: DragKey, pub batch_threshold: u32,
      pub ignored_apps: Vec<String>, #[serde(flatten)] pub extra: Map<String, Value> }
  #[serde(default)] pub struct ResultsList { pub after_cards: u32, #[serde(flatten)] pub extra: Map<String, Value> }
  impl Settings { fn sanitise_drop(&mut self) }   // private, called from `sanitised`
  ```
- Per-OS default: `show_on` is `ZoneShowOn::AnyDrag` on macOS and `ZoneShowOn::Modifier` everywhere else (Windows: modifier tap only, the edge tab is opt-in). Choose it with `cfg!(target_os = "macos")`, the same switch the `ignored_apps` default uses.
- Produces (TS, `src/lib/settings.ts`): `drop_zone: { enabled: boolean; show_on: 'any_drag' | 'modifier'; position: 'near_results' | 'under_cursor'; modifier: 'alt' | 'shift'; keep_original_modifier: 'alt' | 'shift'; batch_threshold: number; ignored_apps: string[] }` and `results_list: { after_cards: number }`.

- [ ] **Step 1: Write the failing Rust tests** (append to `mod tests` in `settings.rs`)

```rust
#[test]
fn drop_zone_defaults_match_the_plan() {
    let s = Settings::default();
    let z = &s.drop_zone;
    assert!(z.enabled);
    // Windows shows the zone only on a modifier tap unless the user opts in to the edge tab.
    let mac = cfg!(target_os = "macos");
    let show_on = if mac { ZoneShowOn::AnyDrag } else { ZoneShowOn::Modifier };
    assert_eq!((z.show_on, z.position), (show_on, ZonePosition::NearResults));
    assert_eq!((z.modifier, z.keep_original_modifier), (DragKey::Alt, DragKey::Shift));
    assert_eq!(z.batch_threshold, 20);
    let figma = if mac { "com.figma.Desktop" } else { "Figma.exe" };
    assert!(z.ignored_apps.iter().any(|a| a == figma));
    assert_eq!(s.results_list.after_cards, 5);
    let json = serde_json::to_value(z).unwrap();
    assert_eq!(json["show_on"], if mac { "any_drag" } else { "modifier" });
    assert_eq!(json["keep_original_modifier"], "shift");
}

#[test]
fn a_file_without_the_drop_sections_gets_their_defaults_and_keeps_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{"version": 2, "drop_zone": {"future": 1}}"#).unwrap();
    let (s, _) = Settings::load(&path);
    assert_eq!(s.drop_zone.batch_threshold, 20);
    assert_eq!(s.results_list, ResultsList::default());
    s.save(&path).unwrap();
    assert_eq!(read_json(&path)["drop_zone"]["future"], 1);
}

#[test]
fn drop_settings_are_clamped_and_the_two_modifiers_kept_apart() {
    let mut s = Settings::default();
    s.drop_zone.batch_threshold = 0;
    s.results_list.after_cards = 99;
    s.drop_zone.modifier = DragKey::Shift;
    s.drop_zone.keep_original_modifier = DragKey::Shift;
    s.drop_zone.ignored_apps = vec!["  ".into(), " com.x ".into()];
    let s = s.sanitised();
    assert_eq!(s.drop_zone.batch_threshold, 1);
    assert_eq!(s.results_list.after_cards, 50);
    assert_eq!(s.drop_zone.keep_original_modifier, DragKey::Alt);
    assert_eq!(s.drop_zone.ignored_apps, ["com.x"]);
    let mut big = Settings::default();
    big.drop_zone.batch_threshold = 50_000;
    big.results_list.after_cards = 0;
    let big = big.sanitised();
    assert_eq!((big.drop_zone.batch_threshold, big.results_list.after_cards), (10_000, 1));
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry-core drop_` (one filter covers all three)
Expected: compile errors (`DropZone`, `ZoneShowOn` not found).

- [ ] **Step 3: Implement the types in the Stream D block of `settings.rs`**

Defaults as in "New settings keys" (`show_on` and the ignored apps chosen with `cfg!(target_os = "macos")`). `sanitise_drop` clamps `batch_threshold` to 1..=10 000 and `after_cards` to 1..=50. When `keep_original_modifier == modifier`, it sets the keep modifier to the other `DragKey`. It trims `ignored_apps` and drops blanks, the same way `sanitised` treats `clipboard.ignored_apps`.

- [ ] **Step 4: Run the Rust tests**

Run: `cargo test -p ferry-core drop_` then `cargo test -p ferry-core settings`
Expected: PASS (the existing settings tests still pass).

- [ ] **Step 5: Write the failing vitest** (append inside the `describe` in `src/lib/schema.test.ts`)

```ts
it('has the Drop zone pane and the list threshold under Floating results', () => {
  expect(panes.map((p) => p.id)).toContain('drop');
  expect(all.filter((s) => s.pane === 'drop').map((s) => s.path)).toEqual([
    'drop_zone.enabled',
    'drop_zone.show_on',
    'drop_zone.position',
    'drop_zone.modifier',
    'drop_zone.keep_original_modifier',
    'drop_zone.batch_threshold',
    'drop_zone.ignored_apps',
  ]);
  const list = all.find((s) => s.path === 'results_list.after_cards');
  expect(list?.pane).toBe('floating');
  expect(list?.control).toEqual({ kind: 'number', min: 1, max: 50, unit: 'drop.unit.cards' });
});
```

Run: `pnpm test schema` → FAIL (`drop` pane missing; `pnpm check` also fails until the types exist).

- [ ] **Step 6: Implement the TS side**

Settings interface and fixture members are as in Interfaces; the fixture uses the macOS defaults (`show_on: 'any_drag'`). The pane needs no per-OS code: it shows whatever value Rust loaded, which on a new Windows install is "Only when the modifier is tapped". `PaneId` gains `'drop'` and `panes` gains `{ id: 'drop', label: 'drop.pane' }`. The rows in the Stream D block, ids `drop_enabled`, `drop_show_on`, `drop_position`, `drop_modifier`, `drop_keep_modifier`, `drop_batch_threshold`, `drop_ignored_apps`, `list_after`, are:
  - `drop_enabled`: switch.
  - `drop_show_on`, `drop_position`, `drop_modifier`, `drop_keep_modifier`: `select`, with options from the keys below.
  - `drop_batch_threshold`: `number`, 1..10 000, unit `drop.unit.files`.
  - `drop_ignored_apps`: `apps`.
  - `list_after`: pane `floating`, `number` 1..50, unit `drop.unit.cards`, `depends_on: 'floating.enabled'`.
  - Every `drop_*` row except `drop_enabled` has `depends_on: 'drop_zone.enabled'`.

  i18n keys (exact text):

```json
"drop.pane": "Drop zone",
"drop.settings.enabled": "Drop zone",
"drop.settings.enabled.help": "Optimise files you drop on a small panel near the results.",
"drop.settings.show_on": "Show the zone",
"drop.settings.show_on.help": "Windows can't tell a file drag from other mouse drags, so there the default is the modifier tap. Choosing \"Whenever files are dragged\" on Windows shows a slim tab at the screen edge after any mouse drag, including moving windows and selecting text; drag files onto the tab to open the zone.",
"drop.settings.show_on.any_drag": "Whenever files are dragged",
"drop.settings.show_on.modifier": "Only when the modifier is tapped",
"drop.settings.position": "Position",
"drop.settings.position.help": "A modifier tap always opens the zone under the pointer.",
"drop.settings.position.near_results": "Near the results",
"drop.settings.position.under_cursor": "Under the pointer",
"drop.settings.modifier": "Modifier to tap while dragging",
"drop.settings.modifier.help": "Tap it once to show the zone under the pointer, again to hide it.",
"drop.settings.keep_modifier": "Hold to keep originals",
"drop.settings.keep_modifier.help": "Hold it while dropping to write results beside the originals instead of replacing them.",
"drop.mod.alt": "Option / Alt",
"drop.mod.shift": "Shift",
"drop.settings.batch_threshold": "Open the batch window above",
"drop.settings.batch_threshold.help": "Dropping more files than this, or any folder, opens the batch window.",
"drop.unit.files": "files",
"drop.settings.ignored_apps": "Don't show for drags from",
"drop.settings.ignored_apps.help": "Apps whose drags are not files to optimise, such as video editors and design tools. A modifier tap still works.",
"drop.settings.list_after": "Switch to the compact list above",
"drop.settings.list_after.help": "More cards than this show as a compact list.",
"drop.unit.cards": "cards",
```

- [ ] **Step 7: Run the checks**

Run: `pnpm check` then `pnpm test`
Expected: 0 errors; all vitest files pass.

- [ ] **Step 8: Commit**

```bash
git add crates/ferry-core/src/settings.rs src/lib/settings.ts src/lib/settings.fixture.ts src/lib/schema.ts src/lib/schema.test.ts src/lib/i18n/en.json
git commit -m "feat(settings): drop zone and compact list settings with their pane"
```

---

### Task 2: `ferry_core::intake`: classify, scan, summary, route

**Files:**
- Create: `crates/ferry-core/src/intake.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `sniff::{sniff, Format}`, `settings::{Output, Location}`.
- Produces:
  ```rust
  pub const HEAD_LEN: usize = 64;
  pub const MAX_FILES: usize = 10_000;
  pub const MAX_DEPTH: usize = 64;
  pub const PACKAGE_EXTENSIONS: &[&str];   // app bundle framework plugin kext photoslibrary photolibrary aplibrary migratedphotolibrary imovielibrary fcpbundle logicx band lrdata pages numbers key xcodeproj xcworkspace
  pub const SYSTEM_FOLDERS: &[&str];       // "$RECYCLE.BIN", "System Volume Information" (matched without case)
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
  pub enum MediaKind { Image, Video, Audio, Pdf }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Classified { pub kind: MediaKind, pub format: Option<Format> }
  pub fn classify(head: &[u8]) -> Option<Classified>;
  pub fn classify_file(path: &Path) -> io::Result<Option<Classified>>;   // reads at most HEAD_LEN bytes
  #[derive(Debug, Clone, PartialEq)] pub struct Candidate { pub path: PathBuf, pub len: u64, pub kind: MediaKind, pub format: Option<Format> }
  #[derive(Debug, Clone, Default, PartialEq)] pub struct Scan { pub files: Vec<Candidate>, pub unsupported: Vec<PathBuf>, pub skipped: u32, pub unreadable: u32, pub truncated: bool }
  #[derive(Debug, Clone)] pub struct ScanOptions { pub max_files: usize, pub exclude: Vec<PathBuf> }   // Default: MAX_FILES, empty
  pub fn scan(paths: &[PathBuf], o: &ScanOptions) -> Scan;
  pub fn is_skipped_dir(name: &OsStr) -> bool;
  #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
  pub struct Summary { pub files: u32, pub folders: u32, pub images: u32, pub videos: u32, pub audio: u32, pub pdfs: u32, pub unsupported: u32, pub unsupported_ext: Option<String> }
  impl Summary { pub fn supported(&self) -> u32 }
  pub fn summarise(paths: &[PathBuf], head_limit: usize) -> Summary;
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Route { Cards, Batch }
  pub fn route(s: &Summary, threshold: u32) -> Route;
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum ZoneState { Hidden, Idle, Hover, Unsupported, Dropped }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum ZoneMode { Zone, Tab }
  #[derive(Debug, Clone, PartialEq, Serialize)] pub struct ZoneView { pub state: ZoneState, pub mode: ZoneMode, pub summary: Option<Summary>, pub keep_original: bool, pub opens_batch: bool }
  pub fn zone_view(summary: Option<&Summary>, hovering: bool, keep: bool, threshold: u32, mode: ZoneMode) -> ZoneView;
  pub fn keep_source_rules(rules: &Output) -> Output;
  pub fn without_own(paths: &[PathBuf], outputs: &[PathBuf]) -> Vec<PathBuf>;
  pub fn all_own(paths: &[PathBuf], outputs: &[PathBuf]) -> bool;   // false for an empty list
  ```

Rules the tests pin:
- `classify`: `sniff(head)` → `Classified { kind: Image, format: Some(f) }`; otherwise `None`.
- `scan`:
  - **Top-level paths:** a directory that is a package (`is_skipped_dir`) goes to `unsupported`, and a top-level unsupported or unreadable file goes to `unsupported` too. Top-level hidden files are still classified, because the user named them.
  - **Inside folders:** an explicit stack walks entries sorted by file name. Hidden entries and `Thumbs.db` / `desktop.ini` are skipped without being counted. `is_skipped_dir` folders, symlinked folders (`file_type().is_symlink()` whose target is a directory) and folders deeper than `MAX_DEPTH` are skipped. A symlink to a file is followed. A folder file that is unsupported adds 1 to `skipped`; one whose head can't be read adds 1 to `unreadable`.
  - **Exclusions and dedupe:** any path whose canonical form starts with an `exclude` entry is skipped. Candidates are deduplicated on `fs::canonicalize`, and the canonical path is stored.
  - **Limit:** the scan stops at `max_files` and sets `truncated`.
- `is_skipped_dir`: the name starts with `.`, or its extension (without case) is in `PACKAGE_EXTENSIONS`, or it is in `SYSTEM_FOLDERS`.
- `summarise`:
  - Top level only, never recursing. A directory is a folder unless it is a package, which counts as an unsupported file with its extension.
  - Every file adds 1 to `files`. Only the first `head_limit` files are classified: each adds to its kind or to `unsupported`, and the first unsupported extension is recorded (lower case, no dot; `None` without one).
- `route`: `Batch` when `folders > 0 || files > threshold`.
- `zone_view`:
  - `opens_batch = summary.is_some_and(|s| route(s, threshold) == Batch)`.
  - `state` is `Unsupported` when a summary has `files > 0`, `folders == 0`, `supported() == 0` and every file was classified (`unsupported == files`); otherwise `Hover` if hovering, else `Idle`.
- `keep_source_rules`: a copy with every `Location::InPlace` replaced by `Location::SameFolder`; templates and folders unchanged.

- [ ] **Step 1: Write the failing tests** (in `intake.rs`, using `tempfile::tempdir()` and `crate::testutil::png(8, 8)` / `jpeg(8, 8, 80, &Default::default())`; a helper `put(dir, rel, bytes)` creates parents)

```rust
#[test] fn content_decides_the_kind_not_the_name()
// a PNG written as "photo.jpg" → classify_file == Some(Classified { kind: Image, format: Some(Format::Png) });
// b"PK\x03\x04..." → None; b"" → None
#[test] fn a_folder_is_walked_in_name_order_including_subfolders()
// d/b.png, d/a.png, d/sub/c.jpg → files' file names == ["a.png", "b.png", "c.jpg"], skipped == 0
#[test] fn hidden_files_and_folders_packages_and_system_folders_are_skipped()
// d/.git/x.png, d/.DS_Store, d/Foo.app/Contents/x.png, d/Photos Library.photoslibrary/x.png,
// d/$RECYCLE.BIN/x.png, d/Thumbs.db, d/ok.png → files == [ok.png], skipped == 0, unsupported == []
#[test] fn a_dropped_package_is_not_opened()
// scan([d/Photos Library.photoslibrary]) → files empty, unsupported == [that path];
// summarise → Summary { files: 1, unsupported: 1, unsupported_ext: Some("photoslibrary".into()), .. }
#[test] fn unsupported_files_in_folders_are_counted_and_dropped_ones_listed()
// scan([d]) with d/notes.txt → skipped == 1, unsupported empty; scan([d/notes.txt]) → unsupported == [path]
#[cfg(unix)] #[test] fn symlinked_folders_are_not_followed()
// d/loop -> d (std::os::unix::fs::symlink) and d/a.png → files == [a.png]; returns (no infinite walk)
#[test] fn the_same_file_twice_is_one_candidate()
// scan([d/a.png, d, d/a.png]) where d holds a.png → files.len() == 1
#[test] fn the_scan_stops_at_max_files()
// 5 PNGs, max_files 3 → files.len() == 3 && truncated
#[test] fn ferrys_own_folders_are_excluded()
// d/backups/x.png, d/y.png, exclude [d/backups] → files == [y.png]
#[test] fn a_summary_counts_kinds_folders_and_unsupported()
// [a.png, b.png, c.jpg, dir, z.zip], head_limit 50 →
// Summary { files: 4, folders: 1, images: 3, unsupported: 1, unsupported_ext: Some("zip".into()), ..Default::default() }
#[test] fn only_the_first_files_are_read_for_a_summary()
// 3 PNGs, head_limit 2 → files 3, images 2
#[test] fn a_folder_or_more_files_than_the_threshold_opens_the_batch()
// route(folders 1) == Batch; route(files 21, thr 20) == Batch; route(files 20, thr 20) == Cards
#[test] fn the_zone_says_unsupported_only_when_nothing_can_be_optimised()
// {files 1, unsupported 1} → Unsupported; {files 2, images 1, unsupported 1} hovering → Hover;
// {files 30, images 2} (head_limit reached) → Idle, not Unsupported; None → Idle
#[test] fn the_zone_shows_the_batch_hint_and_the_keep_line()
// {files 42, images 42}, thr 20, keep true → opens_batch && keep_original; {folders 1} → opens_batch
#[test] fn keeping_the_original_moves_in_place_results_beside_it()
// Output::default() → both locations SameFolder, templates "%f_optimised.%e" / "%f.%e";
// a Folder rule stays Folder
#[test] fn ferrys_own_results_are_not_taken_as_new_input()
// without_own([/w/a.png, /x/b.png], [/w/a.png]) == [/x/b.png]; all_own([/w/a.png], [/w/a.png]);
// !all_own([], [/w/a.png]); !all_own([/w/a.png, /x/b.png], [/w/a.png])
```

Write each as a full test with these exact inputs and assertions.

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry-core intake`
Expected: compile error (module `intake` not found).

- [ ] **Step 3: Implement `intake.rs`** with the signatures and rules above, and add `pub mod intake;` to `lib.rs`. Read heads with `File::open(..)?.take(HEAD_LEN as u64).read_to_end(..)`, as `compare::head_format` does.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core intake`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/intake.rs crates/ferry-core/src/lib.rs
git commit -m "feat(core): classify, scan and route dropped files"
```

---

### Task 3: `ferry_core::drag`: drag and tap detection, zone placement

**Files:**
- Create: `crates/ferry-core/src/drag.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `geometry::{Rect, Corner, place}`, `settings::{DropZone, ZoneShowOn, DragKey}`.
- Produces:
  ```rust
  pub const TAP_MS: u64 = 350;
  pub const MOVE_PX: f64 = 24.0;
  pub const PRESS_MS: u64 = 300;
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)] pub struct Mods { pub alt: bool, pub shift: bool, pub control: bool, pub command: bool }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum Modifier { Alt, Shift, Control, Command }
  impl Mods { pub fn only(self) -> Option<Modifier>; pub fn has(self, m: Modifier) -> bool; pub fn none(self) -> bool }
  impl From<DragKey> for Modifier
  #[derive(Debug, Clone, Copy, Default, PartialEq)]
  pub struct Sample { pub at_ms: u64, pub button: bool, pub drag_seq: Option<i64>, pub mods: Mods, pub cursor: (f64, f64), pub over_own: bool }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Signal { DragStarted, PressMoved, DragEnded, Tap(Modifier) }
  #[derive(Debug, Default)] pub struct DragWatch { /* private */ }
  impl DragWatch { pub fn new() -> Self; pub fn feed(&mut self, s: &Sample) -> Vec<Signal>; pub fn active(&self) -> bool }
  pub fn is_ignored_app(ignored: &[String], app: &str) -> bool;
  pub fn should_auto_show(zone: &DropZone, has_files: bool, all_own: bool, frontmost: Option<&str>) -> bool;
  pub fn zone_near_results(area: Rect, corner: Corner, zone: (u32, u32), stack: Option<(u32, u32)>, margin: u32, gap: u32) -> (i32, i32);
  pub fn zone_under_cursor(area: Rect, zone: (u32, u32), cursor: (i32, i32)) -> (i32, i32);
  pub fn tab_at_edge(area: Rect, corner: Corner, tab: (u32, u32)) -> (i32, i32);
  pub fn zone_from_tab(area: Rect, corner: Corner, zone: (u32, u32), cursor_y: i32) -> (i32, i32);
  ```

`DragWatch` rules:
- **While the button is up:** remember the last `drag_seq` as the idle seq.
- **On a press:** record the press time, cursor, idle seq and `over_own`.
- **While the button is held:**
  - `DragStarted` once when `drag_seq` is `Some` and differs from the idle seq at press. This also catches a drag that began between samples, because the comparison is against the seq seen while the button was still up.
  - `PressMoved` once when `drag_seq` is `None`, the press did not start `over_own`, the cursor is at least `MOVE_PX` from the press point, and `at_ms - press_at >= PRESS_MS`.
- **On release:** `DragEnded` if `DragStarted` or `PressMoved` was sent, then reset.
- **Taps:** a tap starts when `mods.only()` becomes `Some(m)` from `none()`, with the button down. It is cancelled if any other modifier joins or the button is released. It fires `Tap(m)` when the modifiers return to `none()` within `TAP_MS` with the button still down.
- `active()` is true between `DragStarted`/`PressMoved` and the release.

Other rules:
- `is_ignored_app`: any entry that is a case-insensitive prefix of `app`, with blank entries ignored.
- `should_auto_show`: `zone.enabled && zone.show_on == AnyDrag && has_files && !all_own && !frontmost.is_some_and(|a| is_ignored_app(&zone.ignored_apps, a))`. It is the only gate for both the macOS auto-show and the Windows edge tab, so the Windows default (`show_on == Modifier`) turns the tab off with no other check.
- Placement is in physical pixels, with `right = area.x + area.width` and `bottom = area.y + area.height`:
  - `zone_near_results`:
    - With no stack, it is `place(area, zone, corner, margin)`.
    - With a stack that fits, the zone sits beyond it along the vertical axis. For bottom corners, `y = bottom - margin - sh - gap - zh`, kept only if `>= area.y + margin`; for top corners, `y = area.y + margin + sh + gap`, kept only if `y + zh <= bottom - margin`. `x` is the corner's edge: `right - margin - zw` or `area.x + margin`.
    - Otherwise the zone sits beside the stack horizontally: `x = right - margin - sw - gap - zw` (or `area.x + margin + sw + gap`), with `y` at the corner (`bottom - margin - zh` or `area.y + margin`).
  - `zone_under_cursor`: centred on the cursor, clamped inside the area.
  - `tab_at_edge`: flush to the corner's side edge, vertically centred.
  - `zone_from_tab`: flush to the same edge, centred on `cursor_y`, clamped.

- [ ] **Step 1: Write the failing tests** (`s(at, button, seq, mods)` builds a `Sample`; `ALT = Mods { alt: true, ..Default::default() }`)

```rust
#[test] fn a_drag_is_seen_when_the_drag_pasteboard_changes_while_the_button_is_down()
// feed s(0,up,Some(5)), s(50,down,Some(5)) → []; s(80,down,Some(6)) → [DragStarted]; s(110,down,Some(6)) → []
#[test] fn a_drag_that_starts_between_samples_is_seen()
// s(0,up,Some(5)) then s(30,down,Some(6)) → [DragStarted]
#[test] fn a_click_without_a_drag_signals_nothing()
// up 5, down 5, up 5 → no signals at all
#[test] fn the_drag_ends_on_release()
// …DragStarted… then s(400,up,Some(6)) → [DragEnded]; active() false
#[test] fn windows_sees_a_press_that_moves_and_lasts()
// drag_seq None: press at (100,100) t=0; t=100 at (140,100) → []; t=350 at (140,100) → [PressMoved];
// a second run moving only 10 px until t=1000 → []
#[test] fn a_press_on_a_ferry_window_never_shows_the_tab()
// as above but the press sample has over_own: true → never PressMoved
#[test] fn a_quick_modifier_tap_during_a_drag_is_a_tap()
// button down throughout: mods ALT at t=1000, none at t=1200 → [Tap(Modifier::Alt)]
#[test] fn a_held_modifier_or_a_chord_is_not_a_tap()
// ALT 1000→1500 → nothing; ALT at 1000, ALT+SHIFT at 1050, none at 1100 → nothing
#[test] fn a_tap_with_the_button_up_is_ignored()
// button up: ALT 0→100 → nothing
#[test] fn ignored_apps_match_by_prefix_without_case()
// ["com.adobe.PremierePro"] matches "com.adobe.premierepro.24"; ["Figma.exe"] matches "figma.exe";
// [""] matches nothing; ["com.figma"] does not match "org.figma"
#[test] fn the_zone_auto_shows_only_for_other_apps_file_drags()
// zone = DropZone { show_on: AnyDrag, ignored_apps: vec!["com.figma.Desktop".into()], ..DropZone::default() }
// (built explicitly: the defaults differ by OS and CI runs the tests on Windows too):
// (has_files, !own, Some("com.apple.finder")) → true; no files → false; all own → false;
// frontmost "com.figma.Desktop" → false; show_on Modifier → false; enabled false → false
#[test] fn the_zone_sits_above_a_bottom_right_stack()
// area {0,0,1440,900}, zone (244,164), stack Some((324,400)), margin 16, gap 8 → (1180, 312)
#[test] fn the_zone_goes_beside_a_stack_too_tall_to_clear()
// stack Some((324,800)) → (848, 720)
#[test] fn without_a_stack_the_zone_takes_the_corner()
// BottomRight None → (1180, 720); TopLeft with stack Some((324,400)) → (16, 424)
#[test] fn under_the_cursor_the_zone_is_centred_and_kept_on_screen()
// cursor (700,450) → (578, 368); cursor (10,890) → (0, 736)
#[test] fn the_tab_hugs_the_results_edge()
// tab (24,140): BottomRight → (1416, 380); BottomLeft → (0, 380);
// zone_from_tab(BottomRight, (244,164), cursor_y 450) → (1196, 368)
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry-core drag`
Expected: compile error (module `drag` not found).

- [ ] **Step 3: Implement `drag.rs`** with the rules above; add `pub mod drag;` to `lib.rs`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core drag`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/drag.rs crates/ferry-core/src/lib.rs
git commit -m "feat(core): detect drags and modifier taps from samples; place the drop zone"
```

---

### Task 4: `ferry_core::batch`: rows, queue, tally, pace

**Files:**
- Create: `crates/ferry-core/src/batch.rs`
- Modify: `crates/ferry-core/src/lib.rs`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `intake::{Scan, Candidate, MediaKind}`, `job::{JobId, JobState, JobUpdate}`, `edit::EditsPatch`, `optimise::Level`, `sniff::Format`, `i18n::t`.
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
  pub enum RowStatus { Waiting, Working, Done, AlreadyOptimal, Larger, Skipped, Failed, Cancelled, Restored }
  impl RowStatus { pub fn is_finished(self) -> bool }   // Done | AlreadyOptimal | Larger | Failed | Restored
  pub fn row_status(state: &JobState) -> RowStatus;     // Queued|Optimising → Working, Error → Failed, others by name
  #[derive(Debug, Clone, PartialEq, Serialize)]
  pub struct BatchRow { pub index: u32, pub path: PathBuf, pub name: String, pub ext: String, pub kind: Option<MediaKind>,
      pub before: u64, pub after: Option<u64>, pub dims_before: Option<(u32, u32)>, pub dims_after: Option<(u32, u32)>,
      pub status: RowStatus, pub reason: Option<String>, pub progress: f32, pub step: Option<String>,
      pub job: Option<JobId>, pub thumb: Option<PathBuf>, pub output: Option<PathBuf>,
      pub can_restore: bool, pub can_edit: bool, pub keep_original: bool }
  impl BatchRow { pub fn apply(&mut self, u: &JobUpdate) }
  #[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(default)]
  pub struct Adjust { pub compression: Option<u8>, pub scale: Option<u8>, pub format: Option<Format>, pub extreme: bool }
  impl Adjust { pub fn is_default(&self) -> bool; pub fn level(&self) -> Level; pub fn patch(&self) -> EditsPatch }
  #[derive(Debug, Clone, PartialEq)]
  pub enum Work { Start { row: u32, path: PathBuf, keep: bool, level: Level, patch: Option<EditsPatch> },
                  Edit { row: u32, id: JobId, patch: EditsPatch }, Rerun { row: u32, id: JobId } }
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
  pub struct Tally { pub total: u32, pub waiting: u32, pub working: u32, pub done: u32, pub already_optimal: u32, pub larger: u32,
      pub skipped: u32, pub failed: u32, pub cancelled: u32, pub restored: u32, pub before: u64, pub after: u64 }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
  pub enum Phase { Empty, Running, Paused, Done, RollingBack }
  #[derive(Debug, Clone, PartialEq, Serialize)]
  pub struct BatchStatus { pub phase: Phase, pub tally: Tally, pub rate: f64, pub eta_secs: Option<u64>,
      pub rolling: Option<(u32, u32)>, pub skipped_in_folders: u32, pub truncated: bool }
  #[derive(Debug, Clone, PartialEq, Serialize)]
  pub struct Snapshot { pub rows: Vec<BatchRow>, pub status: BatchStatus, pub adjust: Adjust, pub threshold: u32 }
  pub struct BatchRun { /* private */ }
  impl BatchRun {
      pub fn new(limit: usize) -> Self;                       // limit clamped to ≥ 1
      pub fn add(&mut self, scan: Scan, keep: bool) -> u32;   // rows added
      pub fn set_adjust(&mut self, a: Adjust);  pub fn adjust(&self) -> &Adjust;
      pub fn take_work(&mut self) -> Vec<Work>;
      pub fn bind(&mut self, row: u32, id: JobId) -> bool;
      pub fn skip(&mut self, row: u32, reason: String);
      pub fn row_of(&self, id: &JobId) -> Option<u32>;
      pub fn on_update(&mut self, u: &JobUpdate, now: Instant) -> Option<bool>;   // None: not ours; Some(true): reached a finished state
      pub fn pause(&mut self, now: Instant);  pub fn resume(&mut self, now: Instant);  pub fn is_paused(&self) -> bool;
      pub fn cancel_all(&mut self) -> Vec<JobId>;
      pub fn cancelled(&mut self, id: &JobId);
      pub fn rerun(&mut self, now: Instant) -> usize;
      pub fn rollback_targets(&self) -> Vec<JobId>;
      pub fn set_rolling(&mut self, rolling: Option<(u32, u32)>);
      pub fn status(&self, now: Instant) -> BatchStatus;
      pub fn snapshot(&self, now: Instant, threshold: u32) -> Snapshot;
      pub fn take_flush(&mut self, now: Instant) -> Option<(Vec<BatchRow>, BatchStatus)>;
      pub fn is_idle(&self) -> bool;          // no Waiting or Working row and not rolling back
      pub fn first_output(&self) -> Option<PathBuf>;
      pub fn clear(&mut self) -> Vec<JobId>;  // every bound job id; the run is empty afterwards
  }
  pub fn pump(run: &Mutex<BatchRun>, exec: &mut dyn FnMut(Work));
  ```

Rules the tests pin:
- **`add`:**
  - Each candidate whose path is not already a row becomes a `Waiting` row: name from the file stem, ext from the extension, `before = len`, kind set. Each such row is queued for a `Start`.
  - Each `scan.unsupported` path becomes a `Skipped` row with reason `t("drop.batch.reason.unsupported")`.
  - `scan.skipped` adds to `skipped_in_folders`; `scan.truncated` sets `truncated`.
  - The pace starts at the first added row if it is not running yet.
- **`take_work`:**
  - Pops queued items while not paused and the number of `Working` rows is below the limit.
  - It marks each row `Working` (progress 0, reason cleared) and returns the items in queue order.
  - A `Start` carries `level = adjust.level()` and `patch = (!adjust.is_default()).then(|| adjust.patch())`.
- **`Adjust`:**
  - `level()` is Aggressive when `extreme`.
  - `patch()` sets exactly `compression: Some(if extreme { Some(5) } else { compression })`, `level: Some(level())`, `scale: Some(scale)` and `format: Some(format)`, leaving every other field `None`.
  - Job::edit already drops a `format` equal to the source format.
- **`bind`:** records `row → id`. It returns false, recording nothing, when the row is no longer `Working` (it was cancelled while the job was being built).
- **`on_update`:**
  - It finds the row by job id and calls `apply`.
  - When the row moves from `Working` to a finished status, it adds 1 to the pace's completed count and returns `Some(true)`.
  - It marks the row dirty.
- **`BatchRow::apply`:**
  - `status = row_status(&u.state)`; `after`, `dims_before`, `dims_after`, `thumb = thumb_path` and `output` are copied, as are `can_restore` and `can_edit`.
  - `name` and `ext` follow the update (rename, convert); `reason` is the error reason or `None`.
  - `progress`/`step` come from `Optimising`, and are `0.0`/`None` otherwise.
- **`cancel_all`:**
  - Every queued item is dropped and its row set to `Cancelled`.
  - Every `Working` row without a bound job becomes `Cancelled`.
  - It returns the ids of the `Working` rows with a job; the caller cancels those and then calls `cancelled(id)`.
- **`cancelled(id)`:** sets the row to `Cancelled` only if it is still `Working`. A cancelled re-run whose card went back to its earlier result keeps that result.
- **`rerun`:**
  - Each row whose status `is_finished()` and that has a job is set to `Waiting` and queued.
  - The queued item is `Edit { patch: adjust.patch() }` when `can_edit`, else `Rerun`.
  - The pace restarts at `now`. It returns the number queued.
- **`Tally`:**
  - Status counts.
  - `before` sums `before` over Done, AlreadyOptimal, Larger and Restored rows.
  - `after` sums `after.unwrap_or(before)` over Done rows and `before` over the other three.
- **`Phase`:** `RollingBack` if rolling; `Empty` with no rows; `Paused` if paused with Waiting or Working rows; `Running` with Waiting or Working rows; else `Done`.
- **Pace:**
  - Active time is wall time since start minus paused time.
  - `rate = completed / active_secs` once `active_secs >= 1.0`, else 0.
  - `eta_secs = ceil((waiting + working) / rate)` when `rate > 0` and something remains.
- **`take_flush`:** returns the dirty rows in index order plus the status if any row or the status changed since the last flush, else `None`. Every mutating method marks what it changes.
- **`pump`:** loop { take the work under the lock in its own block, drop the guard, `exec` each item }, until `take_work` returns nothing. It never runs `exec` while holding the guard.

- [ ] **Step 1: Write the failing tests** (`scan_of(&[("/d/a.png", 100), …])` builds a `Scan` of image candidates; `update(id, state, after)` builds a `JobUpdate` from `pipeline::notice` with the id, state and sizes set; `t0 = Instant::now()`)

```rust
#[test] fn job_states_map_to_row_statuses()
// Queued, Optimising → Working; Done → Done; AlreadyOptimal; Larger; Error → Failed; Restored
#[test] fn a_row_follows_its_jobs_updates()
// Done update with after Some(40), dims_after Some((10,10)), output "/d/a.webp", ext "webp", can_restore →
// row.after == Some(40), row.ext == "webp", row.output.unwrap() == "/d/a.webp", row.can_restore
#[test] fn adding_files_queues_them_and_skips_what_cannot_be_optimised()
// scan with 2 files and unsupported ["/d/x.zip"] → add == 3; statuses [Waiting, Waiting, Skipped];
// rows[2].reason == Some("Not a supported file".into())
#[test] fn a_path_already_in_the_batch_is_not_added_twice()
// add(scan_of(a, b)); add(scan_of(b, c)) == 1; rows().len() == 3
#[test] fn work_is_handed_out_up_to_the_limit()
// limit 2, 5 files → take_work().len() == 2, those rows Working; take_work() empty;
// bind(0, id0); on_update(update(id0, Done, Some(50)), t0) == Some(true); take_work().len() == 1
#[test] fn paused_hands_out_nothing_until_resumed()
#[test] fn a_start_carries_the_adjust_patch_only_when_adjusted()
// default adjust → Start { patch: None, level: Normal }; set_adjust(Adjust { scale: Some(50), ..Default::default() }) →
// Start { patch: Some(p) } with p.scale == Some(Some(50)) && p.compression == Some(None)
#[test] fn adjust_patches_set_and_clear_every_field()
// Adjust::default().patch() == EditsPatch { compression: Some(None), scale: Some(None), format: Some(None),
//   level: Some(Level::Normal), ..Default::default() };
// Adjust { extreme: true, compression: Some(60), .. }.patch().compression == Some(Some(5)) and level Aggressive
#[test] fn cancel_all_stops_waiting_rows_and_names_the_running_jobs()
// limit 2, 4 files; take_work (rows 0,1 Working); bind(0, id0) only →
// cancel_all() == [id0]; rows 1,2,3 Cancelled; bind(1, id1) == false; cancelled(&id0) → row 0 Cancelled
#[test] fn a_cancelled_rerun_keeps_its_earlier_result()
// row Done with id; rerun(t0) == 1; take_work; on_update(update(id, Done, Some(50))) (the card went back);
// cancelled(&id) → status stays Done
#[test] fn rerun_edits_finished_rows_and_retries_the_rest()
// row A Done can_edit → Edit; row B Failed !can_edit → Rerun; Waiting/Skipped/Cancelled rows untouched
#[test] fn the_tally_counts_statuses_and_sizes()
// Done before 100 after 40; AlreadyOptimal 50; Larger 30; Failed 70 → before 180, after 120, done 1, failed 1
#[test] fn the_phase_follows_the_rows()
// new → Empty; after add → Running; pause → Paused; all finished → Done; set_rolling(Some((1,3))) → RollingBack
#[test] fn rate_and_eta_leave_out_paused_time()
// add 20 at t0; pause at t0+5s, resume at t0+10s; 10 rows finished; status(t0+15s) → rate 1.0, eta_secs Some(10)
#[test] fn only_changed_rows_are_flushed()
// add 3; take_flush → 3 rows; take_flush → None; skip(1, "x") → take_flush rows == [row 1]
#[test] fn rollback_takes_done_rows_that_can_restore()
#[test] fn pump_releases_the_lock_before_running_work()
// Arc<Mutex<BatchRun>> with 3 files; thread runs pump with exec = |_| { run.lock().unwrap().skip(0, "x".into()); };
// main waits on a channel with recv_timeout(2 s) → Ok (a held lock would deadlock and time out)
#[test] fn clear_returns_every_job_and_empties_the_batch()
```

Add the i18n key: `"drop.batch.reason.unsupported": "Not a supported file",`

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry-core batch`
Expected: compile error (module `batch` not found).

- [ ] **Step 3: Implement `batch.rs`**; add `pub mod batch;` to `lib.rs`. Keep `by_job: HashMap<JobId, u32>`, `paths: HashSet<PathBuf>`, `queue: VecDeque<Work>`, a `working` counter updated in one private `set_status(row, status)`, and `dirty: BTreeSet<u32>` plus `status_dirty: bool`.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core batch`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/batch.rs crates/ferry-core/src/lib.rs src/lib/i18n/en.json
git commit -m "feat(core): batch rows, bounded work queue, tally and pace"
```

---

### Task 5: App backend: DropState, job hooks, batch window and dispatcher

**Files:**
- Create: `src-tauri/src/drop_batch/mod.rs`, `src-tauri/src/drop_batch/batch.rs`
- Modify: `src-tauri/src/lib.rs`, `src-tauri/src/jobs.rs`, `src-tauri/src/tray.rs`, `src-tauri/capabilities/default.json`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `ferry_core::batch::*`, `ferry_core::intake::{scan, ScanOptions, keep_source_rules}`, `jobs::{start, edit, rerun, restore, cancel}`, `pipeline::{Job, RunOptions}`, `menu::{Action, item_id, enabled, convert_formats}`, `compare::card`, `AppState`.
- Produces (`drop_batch/mod.rs`):
  ```rust
  pub use batch::BATCH;                  // "batch"
  pub struct DropState { pub batch: Mutex<BatchRun>, keep: Mutex<HashSet<JobId>>, dropped: Mutex<HashSet<JobId>>, wake: mpsc::Sender<()> }
  pub fn init(app: &AppHandle) -> tauri::Result<()>;   // manage DropState, start dispatcher + flush threads, register menu routing (zone/watch added in Tasks 7–8)
  pub fn mark_dropped(app: &AppHandle, id: &JobId, keep: bool);
  pub fn forget(app: &AppHandle, ids: &[JobId]);
  pub fn came_from_drop(app: &AppHandle, id: &JobId) -> bool;
  pub fn with_keep(app: &AppHandle, id: &JobId, options: RunOptions) -> RunOptions;
  fn keep_options(keep: bool, options: RunOptions) -> RunOptions;   // pure; tested
  ```
- Produces (`drop_batch/batch.rs`):
  ```rust
  pub const BATCH: &str = "batch";
  pub const FLUSH_MS: u64 = 100;
  pub fn tray_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>>;   // id "drop:batch", label drop.tray.batch
  pub fn open(app: &AppHandle);                                   // create (960×640, min 720×480, centred) or show; focus; on the main thread
  pub fn add(app: &AppHandle, paths: Vec<PathBuf>, keep: bool);   // open, then scan on spawn_blocking, then run.add, then wake
  pub fn claim(app: &AppHandle, update: &JobUpdate) -> bool;      // true: a batch row's update (consumed)
  pub fn owns(app: &AppHandle, id: &JobId) -> bool;
  pub fn wake(app: &AppHandle);
  fn exec(app: &AppHandle, work: Work);
  #[tauri::command] pub fn batch_ready(app: AppHandle) -> Snapshot;
  #[tauri::command] pub async fn batch_pick_files(app: AppHandle) -> Result<(), String>;
  #[tauri::command] pub async fn batch_pick_folder(app: AppHandle) -> Result<(), String>;
  #[tauri::command] pub fn batch_pause(app: AppHandle);
  #[tauri::command] pub fn batch_resume(app: AppHandle);
  #[tauri::command] pub async fn batch_cancel_all(app: AppHandle) -> Result<(), String>;
  #[tauri::command] pub fn batch_set_adjust(app: AppHandle, adjust: Adjust);
  #[tauri::command] pub fn batch_rerun(app: AppHandle, adjust: Adjust);
  #[tauri::command] pub async fn batch_rollback(app: AppHandle) -> Result<(), String>;
  #[tauri::command] pub fn batch_reveal(app: AppHandle) -> Result<(), String>;
  #[tauri::command] pub fn batch_close(app: AppHandle);
  #[tauri::command] pub fn batch_row_menu(app: AppHandle, window: WebviewWindow, id: JobId, x: f64, y: f64) -> Result<(), String>;
  ```
- Events to the batch window: `batch:rows` (`BatchRow[]`), `batch:status` (`BatchStatus`), `batch:open` (`{ id, action: "compare" | "preview" }`).

Behaviour:
- **`init`:**
  - Manage `DropState::new(available_parallelism)`.
  - Spawn thread `ferry-batch`: `for () in rx { pump(&state.batch, &mut |w| exec(&app, w)) }`.
  - Spawn thread `ferry-batch-flush`: every `FLUSH_MS`, if the batch window exists, `take_flush` under the lock, drop it, then emit both events.
  - Register `app.on_menu_event` for ids starting `drop:row:` (and `list:` in Task 10) only. The tray's `drop:batch` / `drop:zone` ids are handled by the arms in `tray.rs`; matching them here too would run them twice. This task handles `drop:row:compare:<id>` / `drop:row:preview:<id>` by emitting `batch:open`.
- **`exec(Start)`:**
  - Read settings and gifsicle from `AppState`, then `Job::file(&path, &settings, has_gifsicle)`.
  - `Err(reason)` → `skip(row, reason)`.
  - `Ok(mut job)`: apply the patch (`job.edit(p)` if `Some`) and take the id. If `!bind(row, id)`, return. Then call `mark_dropped(id, keep)`.
  - If `!jobs::start(app, job, level, None)`, `skip(row, tf("error.busy", name))` and `forget`.
  - Every lock scope ends before the `jobs::` call.
- **`exec(Edit)`** → `jobs::edit(app, &id, patch)`. **`exec(Rerun)`** → `jobs::rerun(app, &id, false)`. On `Err(e)`, `skip(row, e)`.
- **`claim`:** `try_state::<DropState>()`; lock; `on_update(update, Instant::now())`; drop the lock; on `Some(true)` call `wake`; return `is_some()`.
- **`batch_cancel_all`:** take the ids under the lock, then for each id call `jobs::cancel` followed by `cancelled(&id)` under a fresh lock.
- **`batch_rollback`:**
  - Take the targets, then `set_rolling(Some((0, n)))`.
  - For each target in order: `jobs::restore(app, &id)`, then `set_rolling(Some((i + 1, n)))`.
  - Finally `set_rolling(None)`. All of it runs on `spawn_blocking`.
- **`batch_close`** and the window's `CloseRequested` (with `api.prevent_close()`):
  - If `is_idle()`, the batch is cleared: `ids = clear()`, then `AppState.jobs.retain(|id, e| !ids.contains(id) || e.is_busy())`, then `forget(ids)`.
  - The window is hidden in every case.
- **Window drops:** `WindowEvent::DragDrop(DragDropEvent::Drop { paths, .. })` → `add(app, paths, false)`; other variants are ignored (`DragDropEvent` is `#[non_exhaustive]`).
- **Scan options:** `exclude: vec![state.backups_dir, state.work_dir]`.
- **`batch_reveal`:** `tauri_plugin_opener::reveal_item_in_dir(first_output)`.
- **`batch_row_menu`:**
  - A native menu built from `compare::card(app, &id)`.
  - Items, using `menu::item_id` and `menu::enabled`: Open, Reveal ─ Copy, Copy path ─ Compare (`drop:row:compare:<id>`, label `card.compare`, enabled when `card.can_edit`), Preview (`drop:row:preview:<id>`, label `menu.preview`, enabled when the card has an output) ─ Re-optimise, Optimise aggressively, Downscale ▸ (Scale 90..10, label `menu.percent`), Convert to ▸ (`menu::convert_formats`) ─ Restore original.
  - `card:` items are routed by `menu.rs` as they are today.
  - Popped with `window.popup_menu_at(&menu, LogicalPosition::new(x, y))`.
- **`with_keep`:** `keep_options(keep.contains(id), options)`.
- **`keep_options`:** sets `options.output = keep_source_rules(&options.output)` when `keep`.

Shared-file edits in this task:
- **`jobs.rs` `show`, new first statement:**
  ```rust
  // Stream D: a batch row's update goes to the batch window, not the stack.
  if crate::drop_batch::batch::claim(app, &update) { return; }
  ```
- **`jobs.rs` `finish`:** `let batch = crate::drop_batch::batch::owns(app, &update.id);` before `state.remember`; then `if !batch { state.remember(&update); }` and `if batch { tray::refresh(app) } else { tray::rebuild(app) }`.
- **`jobs.rs` `run`, after `let options = run_options(app, cancel.clone());`:** `let id = entry.last.lock().unwrap().id.clone(); let options = crate::drop_batch::with_keep(app, &id, options);`
- **`jobs.rs` `restore`:** `let options = crate::drop_batch::with_keep(app, id, run_options(app, CancelToken::new()));`
- **`tray.rs`:** `let batch = crate::drop_batch::batch::tray_item(app)?;` after the `aggressive` item; `&batch,` right after `&aggressive,`; the arm `"drop:batch" => crate::drop_batch::batch::open(app),`. Task 7 adds the zone item and arm the same way.
- **`lib.rs`:** `mod drop_batch;`; register the commands above; `drop_batch::init(app)?;` after `menu::init(app);`.
- **`capabilities/default.json`:** add `"batch"` (and `"drop"` now too, to touch the file once).

i18n keys: `"drop.batch.window_title": "Ferry Batch"`, `"drop.tray.batch": "Open batch window"`, `"drop.batch.add_files.title": "Add files to the batch"`, `"drop.batch.add_folder.title": "Add a folder to the batch"`.

- [ ] **Step 1: Write the failing app tests** (in `drop_batch/mod.rs`)

```rust
#[test]
fn keeping_the_original_writes_beside_it_and_otherwise_changes_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let o = || RunOptions::new(&Settings::default(), None, dir.path().into(), |_| Ok(()), CancelToken::new());
    let kept = keep_options(true, o());
    assert_eq!(kept.output.optimised.location, Location::SameFolder);
    assert_eq!(kept.output.converted.location, Location::SameFolder);
    assert_eq!(keep_options(false, o()).output, o().output);
}
```

and in `drop_batch/batch.rs`:

```rust
#[test]
fn every_row_menu_id_names_its_card_and_action() {
    // row_item_id(&id, "compare") == "drop:row:compare:<id>"; parse_row_item(that) == Some((id, RowAction::Compare));
    // parse_row_item("card:x:open") == None
}
```

(`row_item_id`, `parse_row_item` and `enum RowAction { Compare, Preview }` are private helpers in `batch.rs`.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry drop_batch`
Expected: compile error (module `drop_batch` not found).

- [ ] **Step 3: Implement `drop_batch/mod.rs`, `drop_batch/batch.rs` and the shared-file edits above**

- [ ] **Step 4: Run the tests and the build**

Run: `cargo test -p ferry drop_batch`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test -p ferry`
Expected: PASS; no warnings; existing app tests unchanged.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/drop_batch src-tauri/src/lib.rs src-tauri/src/jobs.rs src-tauri/src/tray.rs src-tauri/capabilities/default.json src/lib/i18n/en.json
git commit -m "feat(app): batch runs over ordinary jobs with a bounded dispatcher"
```

---

### Task 6: Batch window UI

**Files:**
- Create: `src/batch.html`, `src/batch/main.ts`, `src/batch/Batch.svelte`, `src/batch/BatchTable.svelte`, `src/batch/AdjustPanel.svelte`, `src/lib/virtual.ts`, `src/lib/virtual.test.ts`, `src/lib/batchview.ts`, `src/lib/batchview.test.ts`, `src/lib/drop-ipc.ts`
- Modify: `vite.config.ts`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 5 commands and events; `formatBytes`, `savingPercent` (`src/lib/format.ts`); `startDrag` (`@crabnebula/tauri-plugin-drag`); `convertFileSrc`.
- Produces (`src/lib/drop-ipc.ts`): `MediaKind`, `RowStatus`, `BatchRow`, `Tally`, `Phase`, `BatchStatus`, `Adjust`, `Snapshot`, mirroring Task 4 field for field (snake_case; `Option` → `| null`; tuples as `[number, number]`).
- Produces (`src/lib/virtual.ts`):
  ```ts
  export interface Range { start: number; end: number; padTop: number; padBottom: number }
  export function visibleRange(scrollTop: number, viewport: number, rowHeight: number, count: number, overscan = 6): Range;
  ```
- Produces (`src/lib/batchview.ts`):
  ```ts
  export type SortKey = 'name' | 'kind' | 'before' | 'after' | 'saved' | 'status';
  export interface Filter { kind: MediaKind | null; status: RowStatus | null }
  export function orderRows(rows: BatchRow[], key: SortKey | null, descending: boolean, filter: Filter): number[];
  export function mergeRows(rows: BatchRow[], changed: BatchRow[]): { rows: BatchRow[]; added: boolean };
  export interface Toolbar { pause: 'pause' | 'resume' | null; cancel: boolean; rerun: boolean; rollback: boolean; reveal: boolean }
  export function toolbar(status: BatchStatus, rows: BatchRow[]): Toolbar;
  export function statusLine(status: BatchStatus, locale?: string): string;
  ```

Rules the tests pin:
- `visibleRange`: `start = max(0, floor(scrollTop / h) - overscan)`, `end = min(count, ceil((scrollTop + viewport) / h) + overscan)`, `padTop = start * h`, `padBottom = (count - end) * h`.
- `orderRows`:
  - Returns indices of rows passing the filter, sorted with a stable tiebreak on index.
  - `saved` sorts by `before - (after ?? before)`.
  - `status` sorts by `RowStatus` declaration order.
  - A `null` key keeps index order.
- `mergeRows`: replaces rows by `index` and appends new indices. `added` is true only when a new index appeared; the page recomputes the order only then or on a sort/filter change.
- `toolbar`:
  - `pause`: `'pause'` when Running, `'resume'` when Paused, else `null`.
  - `cancel`: Running or Paused.
  - `rerun`: Done or Paused, with any row whose status is finished.
  - `rollback`: Done or Paused, with `tally.done > 0`.
  - `reveal`: any row has an output.
- `statusLine`:
  - **Running:** `drop.batch.running` with `{done}` = total − waiting − working, `{total}`, `{rate}` with one decimal, and `{eta}` = `drop.batch.eta.seconds` below 120 s, else `drop.batch.eta.minutes` (rounded up). Without an ETA it uses `drop.batch.running_no_eta`.
  - **Paused:** `drop.batch.paused`.
  - **RollingBack:** `drop.batch.rolling_back`.
  - **Done:** `drop.batch.summary` with `{count}` = total via `Intl.NumberFormat`, `{before}`/`{after}` via `formatBytes`, and `{percent}` via `savingPercent`.
  - **Empty:** `''`.

- [ ] **Step 1: Write the failing vitest files**

```ts
// virtual.test.ts
it('shows the rows in view plus the overscan and pads the rest', () => {
  expect(visibleRange(0, 320, 32, 5000)).toEqual({ start: 0, end: 16, padTop: 0, padBottom: (5000 - 16) * 32 });
  expect(visibleRange(3200, 320, 32, 5000)).toEqual({ start: 94, end: 116, padTop: 94 * 32, padBottom: (5000 - 116) * 32 });
  expect(visibleRange(0, 320, 32, 3)).toEqual({ start: 0, end: 3, padTop: 0, padBottom: 0 });
});
// batchview.test.ts
it('sorts by saving and filters by status without moving ties', /* 3 rows → expected index arrays */);
it('merges changed rows in place and says when rows were added');
it('offers pause, resume, rerun and roll back only when they apply');
it('reads like the spec when done', () => {
  // tally { total: 1204, before: 2_100_000_000, after: 640_000_000, … }, phase 'done', locale 'en-US'
  expect(statusLine(done, 'en-US')).toBe('1,204 files · 2.1 GB → 640 MB · −70%');
});
it('shows done of total, rate and time left while running');
```

Run: `pnpm test virtual` then `pnpm test batchview` → FAIL (modules missing).

- [ ] **Step 2: Implement `virtual.ts`, `batchview.ts`, `drop-ipc.ts`, then run the tests** → PASS.

- [ ] **Step 3: Build the page**

`batch.html` follows `compare.html` (`<body class="window">`, script `/batch/main.ts`); `main.ts` follows `compare/main.ts`; add `batch: page('batch.html'),` to `vite.config.ts`.
- **`Batch.svelte`:**
  - Listen to `batch:rows`, `batch:status` and `batch:open` first, then `invoke<Snapshot>('batch_ready')`, as Stack does.
  - The toolbar has Add files, Add folder, Pause/Resume, Cancel all, Re-run with new settings, Roll back all, Reveal output folder and Adjust; each is enabled from `toolbar()`.
  - The empty state shows `drop.batch.empty` plus `drop.batch.empty.help` with `{count}` = threshold.
  - The footer has a progress bar, `statusLine`, a red `drop.batch.failures` chip when `tally.failed > 0` (clicking it sets the status filter to `failed`), and the `skipped_in_folders` / `truncated` notes.
  - A Close button is shown when Done.
  - `batch:open` → `invoke('open_compare' | 'quick_preview', { id })`.
- **`BatchTable.svelte`:**
  - Columns, in order: checkbox, thumbnail (28 px, `convertFileSrc(row.thumb)`, `loading="lazy"`, `decoding="async"`, file-type placeholder icon without one), name, type, original, new, saved, resolution (`dims_before → dims_after`), status (icon + word; `Working` shows an inline bar from `progress`; `Skipped`/`Failed` show `reason` as a tooltip).
  - Sortable headers; type and status filter selects.
  - Rows are 32 px, windowed with `visibleRange` inside a scrolling body.
  - Use `role="grid"` with `aria-rowcount` = filtered count, and `aria-rowindex` on each row.
  - **Pointer:** a click on the checkbox toggles; double-click → `quick_preview`; right-click → `batch_row_menu`; mousedown on name or thumb → `startDrag` of the checked rows' outputs, or the row's own output if it is unchecked.
  - **Keys:** ↑/↓ move the cursor row, Shift extends, ⌘/Ctrl+A selects all, Esc clears, Space → `quick_preview`, ⌘/Ctrl+D → `open_compare`.
- **`AdjustPanel.svelte`** (collapsible right side): Compression select (From Settings, 90 … 10 in steps of 10, Extreme), Downscale select (Original size, 90 % … 10 %), Convert to select (Keep format, PNG, JPEG, WebP). Each change calls `batch_set_adjust`; the toolbar's Re-run sends `batch_rerun` with the panel's `Adjust`. The help line is `drop.batch.adjust.help`.

i18n keys (exact):

```json
"drop.batch.add_files": "Add files",
"drop.batch.add_folder": "Add folder",
"drop.batch.pause": "Pause",
"drop.batch.resume": "Resume",
"drop.batch.cancel_all": "Cancel all",
"drop.batch.rerun": "Re-run with new settings",
"drop.batch.rollback": "Roll back all",
"drop.batch.reveal": "Reveal output folder",
"drop.batch.close": "Close",
"drop.batch.adjust": "Adjust",
"drop.batch.adjust.help": "Applies to files still waiting and to Re-run.",
"drop.batch.adjust.compression": "Compression",
"drop.batch.adjust.from_settings": "From Settings",
"drop.batch.adjust.downscale": "Downscale",
"drop.batch.adjust.original_size": "Original size",
"drop.batch.adjust.format": "Convert to",
"drop.batch.adjust.keep_format": "Keep format",
"drop.batch.empty": "Drop files or a folder",
"drop.batch.empty.help": "Drops of more than {count} files, or of any folder, open here.",
"drop.batch.col.name": "Name",
"drop.batch.col.kind": "Type",
"drop.batch.col.before": "Original",
"drop.batch.col.after": "New",
"drop.batch.col.saved": "Saved",
"drop.batch.col.resolution": "Resolution",
"drop.batch.col.status": "Status",
"drop.batch.status.waiting": "Waiting",
"drop.batch.status.working": "Working",
"drop.batch.status.done": "Done",
"drop.batch.status.already_optimal": "Already optimal",
"drop.batch.status.larger": "Original kept",
"drop.batch.status.skipped": "Skipped",
"drop.batch.status.failed": "Failed",
"drop.batch.status.cancelled": "Cancelled",
"drop.batch.status.restored": "Restored",
"drop.batch.filter.all_kinds": "All types",
"drop.batch.filter.all_statuses": "All statuses",
"drop.batch.running": "{done} of {total} · {rate} files/s · {eta} left",
"drop.batch.running_no_eta": "{done} of {total}",
"drop.batch.paused": "Paused · {done} of {total}",
"drop.batch.rolling_back": "Rolling back {done} of {total}",
"drop.batch.summary": "{count} files · {before} → {after} · {percent}",
"drop.batch.failures": "{count} failed",
"drop.batch.eta.seconds": "{n} s",
"drop.batch.eta.minutes": "{n} min",
"drop.batch.skipped_in_folders": "{count} files in folders aren't supported and were left out",
"drop.batch.truncated": "Only the first {count} files were added",
"drop.batch.select": "Select {name}",
"drop.batch.aria": "Files in the batch",
"drop.kind.image": "Image",
"drop.kind.video": "Video",
"drop.kind.audio": "Audio",
"drop.kind.pdf": "PDF",
```

The Compression select labels reuse `slider.extreme`; the percentages reuse `menu.percent`.

- [ ] **Step 4: Run the checks**

Run: `pnpm check`, `pnpm test`, `pnpm build`
Expected: 0 errors; tests pass; `dist/batch.html` exists.

- [ ] **Step 5: Manual check (ask the user first)**

Run `pnpm tauri dev`. Tray → "Open batch window" → empty state; Add folder (a folder of 50 PNGs). Look at:
- rows appear and turn Working, then Done, at most N (CPU count) at a time, with live progress, files/s and ETA;
- Pause stops new rows starting;
- Resume;
- Cancel all leaves the remaining rows Cancelled;
- Adjust Downscale 50 % + Re-run halves each resolution;
- Roll back all ends with every row Restored (`shasum` an original before and after);
- right-click a row: the menu works, and Compare opens the compare window;
- the stack shows **no** cards for batch rows, and Recent results stays unchanged.

Record the results in the ledger.

- [ ] **Step 6: Commit**

```bash
git add src/batch.html src/batch src/lib/virtual.ts src/lib/virtual.test.ts src/lib/batchview.ts src/lib/batchview.test.ts src/lib/drop-ipc.ts vite.config.ts src/lib/i18n/en.json
git commit -m "feat(ui): batch window with a virtualised table, progress and adjust panel"
```

---

### Task 7: Drop zone window and drop intake

**Files:**
- Create: `src-tauri/src/drop_batch/zone.rs`, `src-tauri/src/drop_batch/intake.rs`, `src-tauri/src/platform/macos_drop.rs`, `src-tauri/src/platform/windows_drop.rs`, `src/drop.html`, `src/drop/main.ts`, `src/drop/DropZone.svelte`, `src/lib/zone.ts`, `src/lib/zone.test.ts`
- Modify: `src-tauri/src/drop_batch/mod.rs`, `src-tauri/src/platform/mod.rs`, `src-tauri/src/windows.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/lib.rs`, `vite.config.ts`, `src/lib/drop-ipc.ts`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `intake::{summarise, route, zone_view, without_own, classify_file, Route, ZoneView, ZoneMode, ZoneState}`, `drag::{zone_near_results, zone_under_cursor, tab_at_edge, zone_from_tab}`, `batch::add`, `jobs::{start, notice}`, `platform::{show_without_focus, hide}`.
- Produces (`platform/{macos,windows}_drop.rs`, the same surface on both):
  ```rust
  pub fn make_zone_window(window: &WebviewWindow) -> tauri::Result<()>;
  // macOS: tauri_panel! { panel!(DropZonePanel { config: { can_become_key_window: false, can_become_main_window: false, is_floating_panel: true } }) }
  //        to_panel::<DropZonePanel>(), level Floating, collection behaviour as the stack (can_join_all_spaces, full_screen_auxiliary, stationary, ignores_cycle),
  //        hides_on_deactivate false, NonactivatingPanel style mask. Use the NSEvent/Retained names the macro imports; do not import them again.
  // Windows (cfg(windows)): `super::make_non_activating(window)`.
  ```
- Produces (`drop_batch/zone.rs`):
  ```rust
  pub const ZONE: &str = "drop";
  pub const ZONE_SIZE: (f64, f64) = (244.0, 164.0);   // logical; content 220×140 + 12 padding
  pub const TAB_SIZE: (f64, f64) = (24.0, 140.0);
  pub const MENU_SHOW_SECS: u64 = 10;
  pub const DROP_GRACE_MS: u64 = 300;
  pub const COLLAPSE_MS: u64 = 220;
  pub fn create(app: &AppHandle) -> tauri::Result<()>;   // hidden, transparent, undecorated, no shadow, always on top, skip taskbar, all workspaces, focusable(false), focused(false), content_protected(!show_in_screenshots)
  pub fn tray_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>>;   // id "drop:zone", label drop.tray.zone
  pub fn show_from_menu(app: &AppHandle);
  pub fn show(app: &AppHandle, mode: ZoneMode, under_cursor: bool, summary: Option<Summary>);   // Task 8 calls it too
  pub fn set_summary(app: &AppHandle, summary: Summary);   // re-emits the view; ignored when the zone is hidden
  pub fn hide(app: &AppHandle);
  pub fn visible(app: &AppHandle) -> bool;
  pub fn cursor_monitor(app: &AppHandle) -> Option<tauri::Monitor>;   // the stack's lookup, macOS scale quirk included
  #[tauri::command] pub fn zone_ready(app: AppHandle) -> ZoneView;
  ```
- Produces (`drop_batch/intake.rs`): `pub fn accept(app: &AppHandle, paths: Vec<PathBuf>, keep: bool);` (it runs on `spawn_blocking`).
- `DropState` gains `pub zone: Mutex<ZoneCtl>` with `struct ZoneCtl { visible: bool, mode: ZoneMode, summary: Option<Summary>, hovering: bool, keep: bool, dropped: bool, generation: u64 }`.
- Event to the zone page: `drop:view` (`ZoneView`).

Behaviour:
- **`show`:**
  - Bump the generation.
  - Compute the position on the cursor's monitor work area (physical px): `zone_under_cursor` when `under_cursor` or `drop_zone.position == UnderCursor`, else `zone_near_results`. The stack goes in as `Some(outer_size)` when visible. The margin is `round(4 × scale)` (both pages pad by 12) and the gap is 0.
  - Set the window size (`ZONE_SIZE`, or `TAB_SIZE` with `tab_at_edge` for `Tab`) and position, using the stack's per-OS branches from `windows::stack_resize`.
  - `platform::show_without_focus`, then emit the view. All window calls run inside `app.run_on_main_thread`.
- **`show_from_menu`:** `show(Zone, false, None)`, then after `MENU_SHOW_SECS` hide it if the generation is unchanged and it is not hovering.
- **`DragDropEvent::Enter { paths }`:**
  - `summary = summarise(&paths, threshold as usize + 1)` on `spawn_blocking`, then `hovering = true`, then emit.
  - In `Tab` mode, switch to `Zone` using `zone_from_tab` at the cursor's y.
- **`Over`:** no state change in this task (Task 8 samples the keep modifier here). **`Leave`:** `hovering = false`, then emit.
- **`Drop { paths }`:** `dropped = true`, emit `ZoneState::Dropped`, call `intake::accept(app, paths, keep)`, and hide after `COLLAPSE_MS`.
- **`intake::accept`:**
  - `outputs` = every `Entry::output()` in `AppState.jobs`.
  - Each path in `paths` but not in `without_own(paths, outputs)` gets a notice `tf("drop.error.own_result", name)`.
  - Then `summarise(rest, 0)` and `route`.
  - **`Batch`:** `batch::add(app, rest, keep)`.
  - **`Cards`:** per path, `classify_file`:
    - `Ok(None)` / `Err` → notice `tf("drop.zone.unsupported", ext)`, or `t("drop.zone.unsupported_any")` without an extension.
    - `Some` → `Job::file`: `Err(reason)` → notice; `Ok(job)` → `mark_dropped(id, keep)`, then `jobs::start(app, job, Level::Normal, None)`; if that returns false, a notice `tf("error.busy", name)` and `forget`.
- **`windows.rs`:** the body of `follows_screenshot_setting` becomes
  ```rust
  match label {
      STACK | crate::compare::COMPARE => true,
      crate::drop_batch::ZONE => true, // Stream D: the drop zone is floating UI
      _ => false,
  }
  ```
- **`drop_batch/mod.rs`:** `pub use zone::ZONE;`; `init` calls `zone::create(app)?`.
- **`tray.rs`:** `let zone = crate::drop_batch::zone::tray_item(app)?;`, then `&zone,` right after `&batch,`, then the arm `"drop:zone" => crate::drop_batch::zone::show_from_menu(app),`.
- **`lib.rs`:** register `zone_ready`.
- **`platform/mod.rs`:** the two `pub mod drop;` blocks.

- **Zone page:**
  - `drop.html` like `stack.html` (transparent, `/drop/main.ts`). `DropZone.svelte` renders `ZoneView` through `zoneText` and has the `aria-label` `drop.zone.aria` with a `role="status"` line.
  - States: idle, hover (accent border, `scale(1.04)` over 120 ms), unsupported (opacity 0.6), dropped (scale to 0.6 and fade toward the results corner over 180 ms), tab (a 6 px wide, 96 px tall rounded bar on the screen-facing side, accent coloured, label in the tooltip).
  - No motion under `prefers-reduced-motion`.
- **`src/lib/zone.ts`:**
  ```ts
  export interface ZoneText { label: string; sub: string | null; keep: string | null; dim: boolean }
  export function kindsText(s: Summary): string;   // "3 images, 1 video"; only non-zero kinds in order image, video, audio, pdf, folder; files not yet read appear as "{n} files"
  export function zoneText(view: ZoneView): ZoneText;
  ```
  - `label`: `drop.zone.label`, or for Unsupported `drop.zone.unsupported` with the extension (else `drop.zone.unsupported_any`).
  - `sub`: when `opens_batch`, `drop.zone.opens_batch` with `{what}`, where `what` is `{files} files` when files > 0 and then `{folders} folder(s)`, joined by ", "; otherwise `kindsText`; `null` without a summary.
  - `keep`: `drop.zone.keep` when `keep_original`.
  - `dim`: `state === 'unsupported'`.

i18n keys:

```json
"drop.zone.window_title": "Ferry drop zone",
"drop.zone.aria": "Ferry drop zone",
"drop.zone.label": "Drop to optimise",
"drop.zone.keep": "Originals will be kept",
"drop.zone.opens_batch": "{what} · opens Batch",
"drop.zone.unsupported": "Can't optimise .{ext}",
"drop.zone.unsupported_any": "Can't optimise these files",
"drop.zone.tab": "Drop files here",
"drop.count.image.one": "{n} image",
"drop.count.image.other": "{n} images",
"drop.count.video.one": "{n} video",
"drop.count.video.other": "{n} videos",
"drop.count.audio.one": "{n} audio file",
"drop.count.audio.other": "{n} audio files",
"drop.count.pdf.one": "{n} PDF",
"drop.count.pdf.other": "{n} PDFs",
"drop.count.folder.one": "{n} folder",
"drop.count.folder.other": "{n} folders",
"drop.count.file.one": "{n} file",
"drop.count.file.other": "{n} files",
"drop.tray.zone": "Show drop zone",
"drop.error.own_result": "{name} is already a Ferry result — left alone",
```

- [ ] **Step 1: Write the failing tests**

```ts
// zone.test.ts
it('names what is dragged like the spec', () => {
  expect(kindsText({ ...none, files: 4, images: 3, videos: 1 })).toBe('3 images, 1 video');
  expect(kindsText({ ...none, files: 1, images: 1 })).toBe('1 image');
});
it('hints at the batch for many files or a folder', () => {
  expect(zoneText(view({ files: 42, images: 42 }, { opens_batch: true })).sub).toBe('42 files · opens Batch');
  expect(zoneText(view({ folders: 1 }, { opens_batch: true })).sub).toBe('1 folder · opens Batch');
});
it('dims and names an unsupported file', () => {
  const text = zoneText(view({ files: 1, unsupported: 1, unsupported_ext: 'zip' }, { state: 'unsupported' }));
  expect([text.label, text.dim]).toEqual(["Can't optimise .zip", true]);
});
it('shows the keep line only while the modifier is held', /* keep_original true → 'Originals will be kept' */);
```

```rust
// drop_batch/zone.rs
#[test]
fn the_drop_zone_follows_the_screenshot_setting_and_the_batch_window_does_not() {
    assert!(crate::windows::follows_screenshot_setting(ZONE));
    assert!(!crate::windows::follows_screenshot_setting(crate::drop_batch::BATCH));
}
```

Run: `pnpm test zone` and `cargo test -p ferry drop_zone` → FAIL.

- [ ] **Step 2: Implement everything above, then run the tests** → PASS. Also run `cargo test -p ferry windows` (the existing screenshot test still passes) and `pnpm check`.

- [ ] **Step 3: Manual check (ask the user first)**

Run `pnpm tauri dev`. Tray → "Show drop zone": the panel appears near the results corner, above the stack if cards are showing, and TextEdit keeps the keyboard (keep typing). Check each of these:
- Drag 3 PNGs from Finder onto it: hover gives an accent border and a slight scale, and the sub-label reads "3 images". Drop: the panel collapses and three cards appear.
- Drop a `.zip`: "Can't optimise .zip", then a notice card.
- Drop a folder: the batch window opens with its images.
- Drop 25 PNGs: "25 files · opens Batch", then the batch window.
- With "Show app UI in screenshots" off, ⇧⌘5 captures no zone.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/drop_batch src-tauri/src/platform src-tauri/src/windows.rs src-tauri/src/tray.rs src-tauri/src/lib.rs src/drop.html src/drop src/lib/zone.ts src/lib/zone.test.ts src/lib/drop-ipc.ts vite.config.ts src/lib/i18n/en.json
git commit -m "feat(app): drop zone window that optimises or batches what is dropped"
```

---

### Task 8: Drag detection, modifier taps and keep-originals

**Files:**
- Create: `src-tauri/src/drop_batch/watch.rs`
- Modify: `src-tauri/src/platform/macos_drop.rs`, `src-tauri/src/platform/windows_drop.rs`, `src-tauri/src/drop_batch/{mod.rs,zone.rs}`, `src-tauri/Cargo.toml`

**Interfaces:**
- Consumes: `drag::{DragWatch, Sample, Signal, Modifier, Mods, should_auto_show}`, `intake::all_own`, `zone::{show, hide, visible}`, `settings::{DropZone, ZoneShowOn}`.
- Produces (platform, both OSes):
  ```rust
  pub fn sample(at_ms: u64) -> ferry_core::drag::Sample;
  pub fn mods() -> ferry_core::drag::Mods;
  pub fn drag_files() -> Vec<PathBuf>;        // macOS: file URLs on the drag pasteboard; Windows: always empty
  pub fn frontmost_app() -> Option<String>;   // macOS bundle id; Windows exe file name
  ```
  - **macOS:**
    - `button = NSEvent::pressedMouseButtons() & 1 == 1`.
    - `mods` from `NSEvent::modifierFlags_class()`, with `NSEventModifierFlags::{Option, Shift, Control, Command}`.
    - `drag_seq = Some(NSPasteboard::pasteboardWithName(unsafe { NSPasteboardNameDrag }).changeCount() as i64)`.
    - `cursor` from `NSEvent::mouseLocation()`; `over_own: false`.
    - `drag_files` reads `pasteboardItems` → `stringForType("public.file-url")` → `NSURL::URLWithString` → `to_file_path`, the same way `macos_clipboard.rs::read_from` reads copied files.
    - `frontmost_app` as `macos_clipboard.rs` does.
  - **Windows (`cfg(windows)`):**
    - `button = GetAsyncKeyState(if GetSystemMetrics(SM_SWAPBUTTON) != 0 { VK_RBUTTON } else { VK_LBUTTON } as i32) < 0`.
    - `mods` from `VK_MENU`, `VK_SHIFT`, `VK_CONTROL` and `VK_LWIN | VK_RWIN`.
    - `cursor` from `GetCursorPos`; `drag_seq: None`.
    - `over_own`: `GetAncestor(WindowFromPoint(cursor), GA_ROOT)` → `GetWindowThreadProcessId` equals `GetCurrentProcessId()`. The root is needed because the WebView2 child window under the pointer belongs to a browser process.
    - `frontmost_app`: `GetForegroundWindow` → pid → `QueryFullProcessImageNameW`, the way `windows_clipboard.rs::owner_exe` does.
    - Signatures (windows-sys 0.61): `GetAsyncKeyState(vkey: i32) -> i16`, `GetCursorPos(*mut POINT) -> BOOL`, `GetSystemMetrics(SM_SWAPBUTTON) -> i32`, `WindowFromPoint(POINT) -> HWND`.
- Produces (`watch.rs`): `pub fn start(app: &AppHandle)` (thread `ferry-drag`), `pub const IDLE_MS: u64 = 100`, `pub const ACTIVE_MS: u64 = 30`, `fn plan(signal: Signal, ctx: &Context) -> Plan` (pure), with
  ```rust
  struct Context<'a> { zone: &'a DropZone, files: &'a [PathBuf], all_own: bool, frontmost: Option<&'a str>, visible: bool, windows: bool }
  #[derive(Debug, PartialEq)] enum Plan { Show { mode: ZoneMode, under_cursor: bool }, Hide, HideAfterGrace, EmitTap(Modifier), Nothing }
  ```
- Emits `drag:tap` (`{ "modifier": "<snake_case>" }`) app-wide.

`plan` rules:
- `DragStarted` → `Show { Zone, under_cursor: false }` if `should_auto_show(zone, !files.is_empty(), all_own, frontmost)`.
- `PressMoved` → `Show { Tab, false }` if `windows && should_auto_show(zone, true, false, frontmost)`. With the Windows default (`show_on == Modifier`) this is always `Nothing`: the edge tab appears only after the user picks "Whenever files are dragged".
- `Tap(m)` where `m == zone.modifier.into() && zone.enabled` → `Hide` if visible, else `Show { Zone, under_cursor: true }`.
- Any other `Tap(m)` → `EmitTap(m)`.
- `DragEnded` → `HideAfterGrace` if visible.
- Otherwise `Nothing`.

So on Windows with the defaults, the only way the watcher shows the zone is a tap of the zone modifier (Alt) while the button is held. The watcher still samples on Windows in both modes, because taps need the samples.

The loop:
- `sample(ms)` → `watch.feed`.
- `files` and `frontmost` are read only on `DragStarted` or a tap. `all_own` compares against `AppState.jobs` outputs.
- `Show` → `zone::show(app, mode, under_cursor, None)`. When there are files, a `spawn_blocking` computes `summarise(&files, threshold + 1)` and calls `zone::set_summary`, so a slow drive never delays the panel.
- `HideAfterGrace` → `zone::hide_after_grace(app)` (new in this task): hide after `DROP_GRACE_MS` unless a drop arrived or the zone was shown again (generation check).
- Sleep `ACTIVE_MS` while `watch.active()` or the button is down, else `IDLE_MS`.

Zone additions:
- On `DragDropEvent::Over` and `Enter`, set `keep = platform::drop::mods().has(zone.keep_original_modifier.into())` and emit only when it changed.
- On `Drop`, read `keep` the same way, at that moment.

`Cargo.toml`: the two features from Dependencies.

- [ ] **Step 1: Write the failing tests** (in `watch.rs`)

Every test builds its `Context` from an explicit zone, never from bare `DropZone::default()`, because the defaults differ by OS and CI runs `cargo test -p ferry` on Windows: `fn any_drag() -> DropZone { DropZone { show_on: ZoneShowOn::AnyDrag, ignored_apps: vec!["com.figma.Desktop".into()], ..DropZone::default() } }` and `fn tap_only() -> DropZone { DropZone { show_on: ZoneShowOn::Modifier, ..any_drag() } }`.

```rust
#[test] fn a_file_drag_from_another_app_shows_the_zone_near_the_results()
// any_drag(): DragStarted, files [/a.png], !own, frontmost Some("com.apple.finder") → Show { Zone, under_cursor: false }
#[test] fn ferrys_own_drag_and_ignored_apps_show_nothing()
// any_drag(): all_own → Nothing; frontmost "com.figma.Desktop" → Nothing; files empty → Nothing
#[test] fn the_zone_modifier_tap_toggles_the_zone_under_the_cursor()
// any_drag() and tap_only() alike: Tap(Alt), not visible → Show { Zone, true }; visible → Hide; enabled false → Nothing
#[test] fn other_taps_go_to_preset_zones()
// Tap(Control) → EmitTap(Control)
#[test] fn windows_shows_the_tab_only_when_auto_show_is_turned_on()
// any_drag(), PressMoved, windows true → Show { Tab, false }; windows false → Nothing;
// tap_only() (the Windows default), PressMoved, windows true → Nothing, and DragStarted with files → Nothing
#[test] fn the_drag_ending_hides_the_zone_after_a_grace()
#[cfg(target_os = "macos")] #[test] fn sampling_works_off_the_main_thread()
// std::thread::spawn(|| (platform::drop::sample(0), platform::drop::drag_files())).join().is_ok()
```

Run: `cargo test -p ferry watch` → FAIL.

- [ ] **Step 2: Implement the platform sampling, `watch.rs` and the zone additions**; call `watch::start(app)` at the end of `drop_batch::init`.

- [ ] **Step 3: Run the tests and the checks**

Run: `cargo test -p ferry watch`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. The Windows file is compiled only in CI: push the branch only with the user's OK, or leave it for the integration branch; record "Windows: compiled in CI only" in the ledger.

- [ ] **Step 4: Manual check (ask the user first)**

`pnpm tauri dev`, then check each of these:
- Start dragging a PNG in Finder: the zone appears near the results corner within about 0.2 s, showing "1 image".
- Release elsewhere: it hides.
- With `drop_zone.show_on` = "Only when the modifier is tapped" (the Windows default; macOS defaults to "Whenever files are dragged"): a drag shows nothing; tapping Option during the drag shows it centred under the pointer; tapping again hides it.
- Hold Shift over the zone: "Originals will be kept"; drop: `shot_optimised.png` appears next to `shot.png`, and `shasum shot.png` is unchanged.
- Drag a result card out of the stack: no zone.
- Add `com.apple.finder` to "Don't show for drags from": a Finder drag shows no zone, and an Option tap still does.
- Activity Monitor: Ferry stays under 1 % CPU while idle for a minute.
- Windows (cannot be checked on the Mac; record as not verified): a fresh install shows "Only when the modifier is tapped" in the Drop zone pane; a drag shows nothing until Alt is tapped; picking "Whenever files are dragged" turns on the edge tab.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/drop_batch src-tauri/src/platform src-tauri/Cargo.toml
git commit -m "feat(app): show the drop zone on drag start or modifier tap; hold to keep originals"
```

---

### Task 9: Compact results list

**Files:**
- Create: `src/stack/CompactList.svelte`, `src/stack/ListRow.svelte`, `src/lib/selection.ts`, `src/lib/selection.test.ts`, `src-tauri/src/drop_batch/list.rs`
- Modify: `src/stack/Stack.svelte`, `src-tauri/src/drop_batch/mod.rs`, `src-tauri/src/lib.rs`, `src/lib/drop-ipc.ts`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `JobUpdate`, `autoHides`, `canDrag`, `isBusy`, `upsert`, `hoverTarget` (`src/lib/cards.ts`), `visibleRange`, `formatBytes`, `savingPercent`.
- Produces (`src/lib/selection.ts`):
  ```ts
  export const LIST_ROW = 44;
  export const LIST_ROWS_MAX = 8;
  export function nextListMode(count: number, after: number, current: boolean, held: boolean): boolean;
  export function rowsHeld(overList: boolean, selected: number, tool: boolean, dragging: boolean): boolean;
  export interface Totals { count: number; before: number; after: number; saved: number }
  export function listTotals(cards: JobUpdate[]): Totals;
  export function headerText(t: Totals, locale?: string): string;
  ```
- Produces (Rust, `drop_batch/list.rs`): `#[derive(Serialize)] pub struct ListConfig { pub after_cards: u32 }` and `#[tauri::command] pub fn list_config(app: AppHandle) -> ListConfig`.

Rules:
- `nextListMode`: `count === 0` → false; `held` → `current`; else `count > after`.
- `rowsHeld`: any of `overList`, `selected > 0`, `tool` or `dragging` is true.
- `listTotals`: `count = cards.length`; `before`/`after` summed over Done cards with `after !== null`; `saved = before - after`.
- `headerText`: `drop.list.header` when `saved > 0`, with `{count}` = `drop.list.count.one` / `other`, `{saved}` = `formatBytes(saved)` and `{percent}` = `savingPercent(before, after)`; else just the count text.

Stack changes (inside the existing markup, nothing reordered):
- New state: `listAfter = 5` (set from `invoke<ListConfig>('list_config')` in `onMount` and from `settings:changed` → `payload.results_list.after_cards`), `listMode = false`, `overList = false`.
- An `$effect` sets `listMode = nextListMode(cards.length, listAfter, listMode, held || overList || tool !== null)`.
- In list mode, the `{#each}` block and the two `DragAllBar`s are replaced by one `.row` holding `<CompactList …/>` and the open popover (crop or fit for `tool.id`, the same components and `apply`).
- `track()` sets `overList = el?.closest('[data-list]') != null` next to `overBar`, and `leave()` clears it; `CompactList`'s root carries `data-list`. This covers both macOS pointer forwarding and Windows native events.
- Hover hotkeys on rows need no change: the existing `hoverTarget(hovered, …)` call stays as it is. Each row carries `data-card-id`, so `track()` already reports the row under the pointer as `hovered`, and Space, ⌘/Ctrl+D, `-` and `1`–`9` act on it exactly as on a card, under `floating.hover_hotkeys`. The keyboard cursor of Task 10 is separate: it never follows the pointer.

`CompactList.svelte`:
- Width 320.
- Header: `headerText(listTotals(cards))` over the existing `DragAllBar` (Drag all + Clear all).
- Body: rows in `role="listbox"` with `aria-multiselectable="true"` and `aria-label` `drop.list.aria`, scrolling with `max-height: LIST_ROW * LIST_ROWS_MAX`, windowed by `visibleRange` once there are more than 50 cards.
- Props: `cards`, `hovered`, `held`, `overList`, plus the Card callbacks (`onclose`, `ondrag`, `ontool`). Selection state lives here (Task 10 adds the bar and the keyboard).

`ListRow.svelte`:
- A 44 px `role="option"` with `aria-selected`, `data-card-id`. It contains a 32 × 32 thumbnail (`convertFileSrc(card.thumb_path)`, a placeholder without one), the file name (ellipsis), the extension chip, `formatBytes(before) → formatBytes(after)` with a savings pill (green / amber "Already optimal" / "Original kept" / red error, each with an icon and words), and a status icon.
- Busy rows show a 2 px progress bar from `state.progress` and the step text.
- Hover or focus shows a checkbox (`aria-label` `drop.list.select`) and an inline rail: Copy (`card_copy`), Restore when `can_restore`, ⋯ (`card_menu`).
- Right-click → `card_menu`. Mousedown starts the drag like `Card.drag` when no selection exists.
- Rename from the menu opens an inline field with Enter/Esc. Copy `Card.svelte`'s `rename`, `renameKey`, `focusSelect` handlers; they call `card_rename` and `ontool`.
- Auto-hide works as in `Card.svelte`: `setTimeout(onclose, autoHideSecs * 1000)` only when `autoHides(state)`, `autoHideSecs > 0`, not hovered and not held. CompactList passes `held = rowsHeld(...) || held`.

i18n keys:

```json
"drop.list.aria": "Results",
"drop.list.header": "{count} · {saved} saved ({percent})",
"drop.list.count.one": "{n} file",
"drop.list.count.other": "{n} files",
"drop.list.select": "Select {name}",
```

- [ ] **Step 1: Write the failing vitest** (`selection.test.ts`, with `card()` from `card.fixture.ts`)

```ts
it('switches to the list above the threshold but never while held', () => {
  expect(nextListMode(6, 5, false, false)).toBe(true);
  expect(nextListMode(5, 5, true, false)).toBe(false);
  expect(nextListMode(9, 5, false, true)).toBe(false);   // holds the current layout while held
  expect(nextListMode(2, 5, true, true)).toBe(true);
  expect(nextListMode(0, 5, true, true)).toBe(false);
});
it('holds rows while pointed at, selected, edited or dragged', /* each flag alone → true; none → false */);
it('totals what was saved like the spec', () => {
  // 23 cards: 20 Done with before 3_030_000 / after 970_000 (60.6 MB → 19.4 MB) and 3 queued
  expect(headerText(listTotals(cards), 'en-US')).toBe('23 files · 41.2 MB saved (−68%)');
});
it('shows only the count when nothing was saved', /* 6 queued cards → '6 files' */);
```

Run: `pnpm test selection` → FAIL.

- [ ] **Step 2: Implement `selection.ts` (the functions above), `list.rs` (`list_config`), the Stack changes, `CompactList.svelte` and `ListRow.svelte`.** Add `pub mod list;` to `drop_batch/mod.rs` and register `list_config` in `lib.rs`.

- [ ] **Step 3: Run the checks**

Run: `pnpm test`, `pnpm check`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: pass.

- [ ] **Step 4: Manual check (ask the user first)**

`pnpm tauri dev`, then check each of these:
- Copy 7 screenshots quickly, with the pointer away from the stack: after the 6th card the stack becomes a list with the header "6 files · … saved (−…%)", and the newest row is at the bottom.
- Leave the pointer on the list for 10 s: no row fades.
- Move away: rows fade one by one; under 6 rows, the cards come back only once the pointer is off the stack.
- Scroll the list with the trackpad while TextEdit is frontmost.
- Right-click a row: the full card menu; Rename… edits inline.
- Hover a row and press `5`: the row downscales to 50 % (hover hotkeys work on rows).

- [ ] **Step 5: Commit**

```bash
git add src/stack/CompactList.svelte src/stack/ListRow.svelte src/stack/Stack.svelte src/lib/selection.ts src/lib/selection.test.ts src/lib/drop-ipc.ts src-tauri/src/drop_batch src-tauri/src/lib.rs src/lib/i18n/en.json
git commit -m "feat(ui): compact results list above five cards"
```

---

### Task 10: Selection mode, selection bar and list keys

**Files:**
- Create: `src/stack/SelectionBar.svelte`, `src/lib/dragicon.ts`
- Modify: `src/lib/selection.ts`, `src/lib/selection.test.ts`, `src/stack/CompactList.svelte`, `src/stack/ListRow.svelte`, `src/stack/Stack.svelte`, `src/stack/DragAllBar.svelte`, `src-tauri/src/drop_batch/{list.rs,mod.rs}`, `src-tauri/src/lib.rs`, `src/lib/drop-ipc.ts`, `src/lib/i18n/en.json`

**Interfaces:**
- Produces (`src/lib/selection.ts`, added):
  ```ts
  export interface Sel { ids: string[]; anchor: string | null; cursor: string | null }
  export const NO_SELECTION: Sel;
  export function toggle(sel: Sel, id: string): Sel;                       // anchor = cursor = id
  export function selectAll(sel: Sel, cards: JobUpdate[]): Sel;
  export function clearSel(sel: Sel): Sel;                                 // keeps cursor
  export function moveCursor(sel: Sel, cards: JobUpdate[], delta: 1 | -1, extend: boolean): Sel;
  export function prune(sel: Sel, cards: JobUpdate[]): Sel;
  export interface Targets { files: string[]; fileIds: string[]; edit: string[]; cantEdit: number }
  export function targets(cards: JobUpdate[], ids: string[]): Targets;
  export interface KeyPress { key: string; shiftKey: boolean; altKey: boolean; metaKey: boolean; ctrlKey: boolean }   // a KeyboardEvent fits
  export type ListKey =
    | { kind: 'select_all' }
    | { kind: 'escape' }
    | { kind: 'move'; delta: 1 | -1; extend: boolean }
    | { kind: 'preview' }
    | { kind: 'compare' }
    | { kind: 'step_down' }
    | { kind: 'scale'; percent: number };
  export function listKey(e: KeyPress, mac: boolean): ListKey | null;
  export function escapeSel(sel: Sel): { sel: Sel; release: boolean };
  export function stepDown(scale: number | null): number | null;
  ```
  Rules:
  - `moveCursor`: with no cursor it starts at the last card (down) or the first (up); it clamps at the ends. With `extend`, the selection becomes the range between `anchor` (or the old cursor) and the new cursor, in card order.
  - `prune`: drops ids that are gone, and clears a cursor or anchor that is gone.
  - `targets`: `files`/`fileIds` are the selected cards that pass `canDrag`; `edit` is the selected cards with `can_edit`; `cantEdit` is the selected cards without it.
  - `listKey`: the command key is `metaKey` on macOS and `ctrlKey` elsewhere. If `altKey` is down, or the other one of `metaKey`/`ctrlKey`, it returns `null`.
    - With the command key and no Shift: `a`/`A` → `select_all`, `d`/`D` → `compare`.
    - Without the command key: `Escape` → `escape`; `ArrowUp`/`ArrowDown` → `move` with `delta` −1/+1 and `extend = shiftKey`; and, without Shift, `' '` → `preview`, `-` → `step_down`, `1`–`9` → `scale` with `percent` 10–90.
    - Every other key → `null`: the page leaves it alone (no `preventDefault`).
  - `escapeSel`: with ids selected → `{ sel: clearSel(sel), release: false }`; with none → `{ sel, release: true }` (the list hands the keyboard back).
  - `stepDown`: the same rule as `hover_keys::step_down` in Rust: `next = max(10, (scale ?? 100) - 10)`; `null` when `next === scale`.
- Produces (`src/lib/dragicon.ts`): `export function fanIcon(paths: string[]): Promise<string>`, moved verbatim from `DragAllBar.svelte`'s `fan` + `load`; DragAllBar imports it.
- Produces (Rust, `drop_batch/list.rs`):
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum ListItem { Scale(u8), Quality(u8), Extreme, Convert(Format) }
  pub fn list_item_id(item: ListItem) -> String;          // "list:scale-50", "list:quality-60", "list:extreme", "list:convert-webp"
  pub fn parse_list_item(id: &str) -> Option<ListItem>;
  pub fn selection_formats(cards: &[JobUpdate]) -> Vec<(Format, bool)>;   // union of menu::convert_formats over editable cards; enabled when some editable card is not already that format
  pub fn patch_for(item: ListItem) -> Option<EditsPatch>;  // Scale → scale; Quality(q) → compression q + level Normal; Extreme → compression 5 + Aggressive; Convert → None (uses jobs::convert)
  #[tauri::command] pub fn list_menu(app: AppHandle, window: WebviewWindow, ids: Vec<JobId>, kind: String, x: f64, y: f64) -> Result<(), String>;  // kind: "downscale" | "compression" | "convert"
  #[tauri::command] pub async fn list_edit(app: AppHandle, ids: Vec<JobId>, edits: EditsPatch) -> Result<u32, String>;
  #[tauri::command] pub async fn list_copy(app: AppHandle, ids: Vec<JobId>) -> Result<(), String>;
  #[derive(Serialize)] pub struct SaveReport { pub copied: u32, pub failed: u32 }
  #[tauri::command] pub async fn list_save_to(app: AppHandle, ids: Vec<JobId>) -> Result<Option<SaveReport>, String>;   // None: dialog cancelled
  ```
  Behaviour:
  - **No list keys in Rust.** `list.rs` registers no global shortcut and does not read `floating.hover_hotkeys`. The list's keys are DOM key events in the stack page (see "List keyboard" below).
  - **`list_menu`:**
    - It stores `ids` in a private `static LAST: Mutex<Vec<JobId>>` and builds a native menu:
      - `downscale`: 90 % … 10 % (`menu.percent`).
      - `compression`: `drop.list.quality` for q 90 … 10, then `slider.extreme`.
      - `convert`: `selection_formats` of the cards `compare::card` returns.
    - The `drop_batch` menu handler routes `list:` ids. It applies `patch_for` through `jobs::edit` to each id in `LAST`, or `jobs::convert(app, id, f, false)` for cards whose format differs. This runs on `spawn_blocking` and logs errors.
  - **`list_edit`:** `jobs::edit` per id; returns the number that succeeded.
  - **`list_copy`:** collects `jobs::output` for each id, then `platform::clipboard::write_files`, with `state.clipboard` locked across the write and `log.record(seq, None)`, as `jobs::copy` does.
  - **`list_save_to`:**
    - Picks a folder with `tauri_plugin_dialog` (`blocking_pick_folder` on the blocking worker, title `drop.list.save_to.title`).
    - Copies each output to `output::unique_except(&folder.join(file_name), None, None)` with `std::fs::copy`, then `ferry_core::fsutil::copy_times`.
    - Never moves or overwrites a file.

SelectionBar (replaces the header while `sel.ids.length > 0`; buttons carry `data-action="sel-*"` so the existing tooltip forwarding works, and every icon button has an `aria-label`):
- **Count:** `drop.list.selected`.
- **Drag handle** (`sel-drag`): `startDrag({ item: targets.files, icon: await fanIcon(thumbs) })`.
- **Save to…** (`sel-save`): `list_save_to`; the tooltip then reads `drop.list.saved` for 1.4 s.
- **Crop** (`sel-crop`): opens `CropPopover` with the first `edit` card as `card`. Stack keeps `cropFor: string[] | null`, and its `apply` sends `list_edit({ ids: cropFor, edits })` instead of `card_edit`.
- **Downscale ▸ / Compression ▸ / Convert ▸** (`sel-downscale`, `sel-compression`, `sel-convert`): `list_menu` at the button's bottom-left.
- **Copy** (`sel-copy`): `list_copy`, then `drop.list.copied`; it also sets `focused = false` (Stack's `listFocused`, through the binding), so the user's next ⌘/Ctrl+V lands in the app they were in.
- **Remove** (`sel-remove`): closes each selected card (`card_cancel` first for busy ones).
- **Select all / Clear** (`sel-all` / `sel-clear`).
- The edit buttons are disabled when `targets.edit` is empty; with `cantEdit > 0` their tooltip adds `drop.list.cant_edit`.

Row selection:
- A click on the checkbox toggles the row.
- While anything is selected, a click anywhere on a row toggles it and mousedown does not start a drag.
- Shift-click extends from the anchor.

List keyboard (user decision: keys act on the list only after a click on it, never on hover):
- **State:** Stack owns `listFocused = $state(false)` and passes it to CompactList as `bind:focused`, together with a new `tool` prop (`OpenTool | null`).
- **Taking the keyboard:** only these clicks set `listFocused = true`: a click on a row's checkbox, a click on a row while something is selected, and a click on the list body outside the rows. The clicked row (if any) becomes `sel.cursor`. A press on a row body with nothing selected still starts a drag (Task 9) and takes nothing; clicks on rail buttons, the header and the selection bar take nothing either.
- **How:** Stack's existing `stack_focus` effect changes `const on = tool !== null;` to `const on = tool !== null || listFocused;`. That is the command rename and crop already use: `windows::stack_focus` → `platform::set_stack_key` makes the stack the key window without activating Ferry (Windows: drops `WS_EX_NOACTIVATE` and brings it to the foreground), remembers the app that had the keyboard, and gives the keyboard back to it when `on` turns false. It also calls `hover_keys::set_keyed`, which drops the global hover hotkeys while the stack has the keyboard. No Rust change is needed.
- **Handing it back:** `listFocused = false` when the stack window blurs (`windowBlur` gains `listFocused = false;` as its first statement: the user clicked another app or window), on Esc with nothing selected, after Copy, before a preview or compare (below), and in a Stack `$effect` when `!listMode || cards.length === 0`.
- **Keys:** CompactList has `<svelte:window onkeydown={…}>` that runs only while `focused && tool === null` and the event target is not an `input`, `textarea` or `select` (the rename field keeps its own Enter/Esc). It calls `listKey(event, isMac)` (`isMac` from `src/lib/ipc.ts`); on a non-null result it calls `event.preventDefault()` and then:
  - `select_all` → `selectAll`; `escape` → `escapeSel`, and on `release` sets `focused = false`; `move` → `moveCursor`.
  - `preview` / `compare` → for the cursor row, if it passes `canDrag`: first hand the keyboard back (`focused = false`, then `flushSync()` from `svelte` so the `stack_focus(false)` effect is sent first), then `invoke('quick_preview' | 'open_compare', { id })`. Quick Look and the compare window take the keyboard themselves, as they do from a hover key; releasing first stops the stack's blur from handing the keyboard to the other app while they open.
  - `scale` / `step_down` → `invoke('card_edit', { id, edits: { scale } })` for the cursor row if it has `can_edit` and is not busy; `step_down` takes `scale = stepDown(card.edits.scale)` and does nothing on `null`.
  - With no cursor row, the card keys do nothing.
  - These are the hover hotkeys' keys. The list handles them itself because the global hover hotkeys are dropped while the stack has the keyboard. None of them depends on `floating.hover_hotkeys`, since nothing is taken from another app.
- **While focused**, the cursor row gets a focus ring (only then) and is scrolled into view (`scrollIntoView({ block: 'nearest' })`), and rows are held like an open tool: Stack's list-mode effect passes `held || overList || tool !== null || listFocused`, and CompactList passes `tool !== null || focused` as `rowsHeld`'s `tool` flag.
- **Without a click** nothing changes from Task 9: the row under the pointer takes Cycle 2's hover hotkeys, under the setting, and ⌘/Ctrl+A, Esc and the arrows stay with the frontmost app.
- The listbox carries `aria-activedescendant` = the cursor row's element id while focused.

Prune effect: `sel = prune(sel, cards)` on every change to `cards`.

i18n keys:

```json
"drop.list.selected": "{n} selected",
"drop.list.drag": "Drag the selected files into another app",
"drop.list.save_to": "Save copies to…",
"drop.list.save_to.title": "Save copies of the selected files to",
"drop.list.saved": "Saved",
"drop.list.crop": "Crop",
"drop.list.downscale": "Downscale",
"drop.list.compression": "Compression",
"drop.list.quality": "q {n}",
"drop.list.convert": "Convert",
"drop.list.copy": "Copy",
"drop.list.copied": "Copied",
"drop.list.remove": "Remove",
"drop.list.select_all": "Select all",
"drop.list.clear": "Clear selection",
"drop.list.cant_edit": "{n} of these can't be edited",
```

- [ ] **Step 1: Write the failing tests**

```rust
// drop_batch/list.rs
#[test] fn every_list_menu_id_reads_back()
// Scale(50), Quality(60), Extreme, Convert(Format::WebP) round-trip; parse_list_item("card:x:open") == None
#[test] fn selection_formats_offer_what_some_editable_card_can_take()
// a PNG and a JPEG card (both can_edit): [(Png, true), (Jpeg, true), (WebP, true)];
// two PNG cards: Png disabled; a GIF-sourced card adds Gif first; non-editable cards are ignored
#[test] fn extreme_is_aggressive_at_quality_five()
// patch_for(Extreme) == Some(EditsPatch { compression: Some(Some(5)), level: Some(Level::Aggressive), ..Default::default() })
```

```ts
// selection.test.ts (added)
it('toggles, selects all and clears but keeps the cursor');
it('moves the cursor and extends from the anchor in card order', /* cards a b c d; toggle b; extend down twice → ids [b, c, d] */);
it('forgets cards that are gone');
it('splits a selection into what can be dragged and what can be edited', /* a busy card and a can_edit:false card → files excludes busy, cantEdit 1 */);
it('reads list keys with the platform command key and leaves every other key alone', () => {
  const k = (key: string, mods: Partial<KeyPress> = {}): KeyPress =>
    ({ key, shiftKey: false, altKey: false, metaKey: false, ctrlKey: false, ...mods });
  expect(listKey(k('a', { metaKey: true }), true)).toEqual({ kind: 'select_all' });
  expect(listKey(k('a', { ctrlKey: true }), true)).toBeNull();
  expect(listKey(k('a', { ctrlKey: true }), false)).toEqual({ kind: 'select_all' });
  expect(listKey(k('d', { metaKey: true }), true)).toEqual({ kind: 'compare' });
  expect(listKey(k('Escape'), true)).toEqual({ kind: 'escape' });
  expect(listKey(k('ArrowDown'), true)).toEqual({ kind: 'move', delta: 1, extend: false });
  expect(listKey(k('ArrowUp', { shiftKey: true }), false)).toEqual({ kind: 'move', delta: -1, extend: true });
  expect(listKey(k(' '), true)).toEqual({ kind: 'preview' });
  expect(listKey(k('-'), true)).toEqual({ kind: 'step_down' });
  expect(listKey(k('5'), true)).toEqual({ kind: 'scale', percent: 50 });
  for (const other of [k('0'), k('Enter'), k('a'), k('ArrowDown', { altKey: true }), k(' ', { shiftKey: true })]) {
    expect(listKey(other, true)).toBeNull();
  }
});
it('clears the selection on Esc and hands the keyboard back when nothing is selected', /* 2 selected → release false, ids []; none → release true */);
it('steps down like the hover key', () => {
  expect([stepDown(null), stepDown(50), stepDown(15), stepDown(10)]).toEqual([90, 40, 10, null]);
});
```

Run: `cargo test -p ferry list` and `pnpm test selection` → FAIL.

- [ ] **Step 2: Implement it all**, then register `list_menu`, `list_edit`, `list_copy`, `list_save_to` in `lib.rs` (there is no `list_keys` command).

- [ ] **Step 3: Run the checks**

Run: `cargo test -p ferry list`, `pnpm test`, `pnpm check`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: pass.

- [ ] **Step 4: Manual check (ask the user first)**

With 8 rows in the list and TextEdit frontmost (with some text in it), check each of these:
- Rest the pointer on the list without clicking and press ⌘A, then ↓ and Esc: TextEdit selects all its text and moves its caret; the list selects nothing and shows no ring.
- Click a row's checkbox: the selection bar appears with "1 selected" and the row has the ring.
- Click two more rows: "3 selected".
- ⌘A: all selected, even with the pointer off the list. Esc: cleared.
- ↓ ↓ moves the ring, ⇧↓ extends, `5` downscales the ring row to 50 %.
- Space previews the ring row: Quick Look opens and stays open with the keyboard (Space or Esc closes it). If it closes at once, that is a fix with a `Ruling:` line.
- Esc with nothing selected: the ring goes, and typing lands in TextEdit without a click.
- Click a checkbox again, then click into TextEdit and type: arrows, ⌘A and Esc work there again.
- Turn Settings → Floating results → "Hover hotkeys" off: the list keys still work after a click (they do not depend on it), and nothing is taken on hover.
- Downscale ▸ 50 %: every selected row re-runs.
- Convert ▸ WebP: rows show "webp".
- Copy, then paste in a Finder window: the files are pasted.
- Save to… a new folder: copies appear, and the originals are untouched.
- Drag handle into a Finder window: the files are copied there, and no drop zone appears.
- Remove: the rows are gone.

- [ ] **Step 5: Commit**

```bash
git add src/stack src/lib/selection.ts src/lib/selection.test.ts src/lib/dragicon.ts src/lib/drop-ipc.ts src-tauri/src/drop_batch src-tauri/src/lib.rs src/lib/i18n/en.json
git commit -m "feat(ui): select results in the list and act on the selection"
```

---

### Task 11: Manual check pass (macOS) and Windows status

**Manual check: ask the user before running the live app.** No code changes unless a check fails; a failure becomes a fix commit with a `Ruling:` line in the ledger.

Prepare: `mkdir -p /tmp/ferry-sD/tree/sub /tmp/ferry-sD/tree/.git /tmp/ferry-sD/tree/Foo.app/Contents && screencapture -x /tmp/ferry-sD/shot.png && for i in $(seq 1 1000); do cp /tmp/ferry-sD/shot.png /tmp/ferry-sD/tree/sub/s$i.png; done && cp /tmp/ferry-sD/shot.png /tmp/ferry-sD/tree/.git/ && cp /tmp/ferry-sD/shot.png /tmp/ferry-sD/tree/Foo.app/Contents/ && shasum /tmp/ferry-sD/tree/sub/s1.png`

1. **Drop to optimise:** drag 3 PNGs from Finder. The zone appears near the results corner, reads "3 images", gets an accent border on hover, collapses on drop, and 3 cards appear. TextEdit keeps the keyboard throughout.
2. **Keep originals:** hold Shift while dropping. The zone shows "Originals will be kept"; `*_optimised.png` files appear beside the originals and the originals' `shasum` is unchanged.
3. **Modifier tap:** with "Only when the modifier is tapped", tap Option during a drag: the zone shows under the pointer; tap again: it hides.
4. **Folder:** drag `/tmp/ferry-sD/tree`. The zone reads "1 folder · opens Batch"; on drop the batch window lists 1 000 rows and none from `.git` or `Foo.app`. While it runs:
   - scrolling stays smooth;
   - the status line shows files/s and ETA;
   - at most the CPU count are Working at once;
   - Activity Monitor shows the app responsive and memory under 400 MB.
5. **Batch controls:** Pause, Resume, Cancel all (remaining rows Cancelled), Re-run with Downscale 50 % (resolution column halves), Roll back all (all rows Restored, and `shasum` of `s1.png` matches the value printed in Prepare).
6. **No self-trigger:** dragging a card, "Drag all", a list selection or batch rows into Finder never shows the zone. Option-tap the zone during a card drag and drop it there: the notice "… is already a Ferry result — left alone" appears and the file is unchanged.
7. **Ignored apps:** after adding `com.apple.finder` to "Don't show for drags from", Finder drags show no zone and Option taps still do.
8. **Many files:** drag 25 PNGs → "25 files · opens Batch".
9. **Compact list:** the Task 9 and Task 10 checks, plus: while 3 rows are selected, new clipboard cards arrive at the bottom and no row moves or fades; and with the pointer resting on the list but no click, ⌘A, Esc and the arrows still reach TextEdit.
10. **Screenshots:** with "Show app UI in screenshots" off, ⇧⌘5 hides the zone and the stack; the batch window is visible, like Settings.
11. **Idle CPU:** under 1 % for 60 s with the app idle (Activity Monitor).
12. **Windows:** record "built, compiled in CI, not verified on a real desktop: Alt tap (the default trigger), the opt-in edge tab ("Whenever files are dragged"), keep modifier, batch window, list focus and list keys".

Commit only the ledger-driven fixes, if any.

---

## Out of scope (deferred, named so nobody builds them by accident)

- Preset zones and the zone's "Tap Ctrl for presets" hint (stream F, through `drag:tap`).
- Dropping onto the macOS menu-bar icon (R§4.1): Tauri's tray has no drop API, so it needs an `NSStatusItem` button subclass.
- The batch Adjust panel's crop/aspect, video encoder, audio bitrate, cover-art size, output location and template, and Keep-metadata controls (R§4.10), which need streams A, B and E or per-batch output overrides.
- The "Backing up N of M originals" phase (see Decisions).
- Pausing batches under low-power mode (R§3.2 NFR-R5, optional).
- Ignoring false-positive drags by anything other than the frontmost app's id.

## Questions decided by the user (2026-10-23)

These were open when the plan was written; the user accepted every recommendation. They are settled: do not reopen them, and record any forced deviation as a `Ruling:` line in the ledger.

1. **Windows auto-show:** the default on Windows is "modifier tap only" (`drop_zone.show_on = "modifier"`); the edge tab after a moving press is an opt-in ("Whenever files are dragged"). macOS is unchanged (`"any_drag"`). Applied in Decisions, New settings keys, Tasks 1, 3, 8 and 11.
2. **Keep-originals modifier:** Shift (Option/Alt is the zone tap). As planned.
3. **No separate backup phase in the batch:** each original is backed up just before its own write, so "Backing up N of M" never shows. As planned.
4. **Thresholds:** batch above 20 files; 5 cards before the compact list (R§4.4). As planned.
5. **List keys need a click on the list first** (⌘/Ctrl+A, Esc, ↑/↓, ⇧↑/⇧↓): never taken from the frontmost app on hover, and independent of the "Hover hotkeys" setting. Applied in Decisions, Tasks 9, 10 and 11.
6. **Menu-bar icon drop target** (R§4.1): deferred (Tauri's tray has no drop API). As planned.
7. **Batch Adjust panel:** compression (or Extreme), downscale % and convert-to only, this cycle. As planned.
