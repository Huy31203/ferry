# Ferry M2b — App Core Loop Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make Ferry do its job: optimise copied images automatically and on a hotkey, show each job as a result card with working actions (copy, drag out, show in folder, restore, try aggressive, retry, re-optimise, cancel, close, Drag all, Clear all), and drive the tray icon and menu from the run state and the jobs.

**Architecture:**
- **Decisions live in `ferry-core`** and are tested on three OSes, like `geometry.rs` in M1:
  - the clipboard trigger rules (`trigger`) and the run state (`run_state`);
  - one job end to end (`pipeline`): back up, optimise, write and report;
  - Windows bitmaps (`bitmap`), the tray glyphs (`glyph`) and header dimensions (`sniff::dimensions`).
- **The Tauri app is thin glue:**
  - `platform::clipboard` reads, writes and watches the OS clipboard;
  - `jobs` runs pipeline jobs on `spawn_blocking` behind a semaphore and sends `job:update` events;
  - `clipboard` and `hotkeys` start jobs, `tray` mirrors the state, and `commands` serves the card buttons.
- **The Svelte stack renders `JobUpdate`s as cards.**

This is milestone **M2b** of three. M2a (the engine) is merged; M2c (Settings and housekeeping) gets its own plan.

**Tech Stack:**
- Tauri 2.12 with the `tray-icon`, `image-png`, `macos-private-api` and `protocol-asset` features.
- Plugins:
  - `tauri-plugin-global-shortcut` 2.4.1;
  - `tauri-plugin-drag` 2.1.1, with npm `@crabnebula/tauri-plugin-drag` 2.1.0;
  - `tauri-plugin-opener` 2.7.2.
- Clipboard: `objc2-app-kit` 0.3.2 (`NSPasteboard`) on macOS; `clipboard-win` 5.4.1 (with its `monitor` feature) on Windows.
- Runtime and logs: `tokio` 1 (`sync`), `tracing` + `tracing-appender` + `tracing-subscriber`.
- Frontend: Svelte 5 and vitest.

**Spec:** `docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md` (approved). Read §4.3, §4.4, §5.1 to §5.6, §6, §7 and §8 before starting. Requirements: `docs/requirements/ferry-optimize-requirements.md`.

**Prototype:** every line of code below was compiled, linted and tested on this Mac first. It lives on the local branch `spike/drag`, last commit 6763a39; the worktree is under the session scratchpad. If a step's code seems wrong, compare it with `git show spike/drag:<path>` before changing it. The Windows clipboard file is the exception: it was never compiled here (see Task 9).

## Global Constraints

Every task's requirements include these. Values are copied from the spec.

**Run state (§5.1)**
- Running does automatic work. "Paused: skips exactly one automatic trigger, then returns to Running." Stopped does no automatic work.
- "Hotkeys and card actions always work, whatever the state." The start state comes from settings.

**Automatic clipboard trigger (§5.2)**
- Ignore a change whose `seq` "is one our own write recorded".
- Skip when any of these holds:
  - the run state is not Running;
  - `clipboard.enabled` is off;
  - the content is concealed;
  - the source app is on the ignore list;
  - there is no supported image ("copied files are ignored in automatic mode");
  - the image is larger than `clipboard.max_size_mb`;
  - its extension is in `images.ignored_extensions`;
  - it is a GIF and gifsicle is missing.

**Hotkeys (§5.3)**
- Hotkeys run whatever the run state.
- Copied files become `File` jobs optimised in place.
- With `skip_multiple_files` on and more than one file, show a single Error card "More than one file copied".
- With nothing usable, show "Nothing to optimise on the clipboard".

**Job lifecycle (§5.4)**
- Show the Queued card right away (NFR-P3).
- Back up first; "If the backup fails, the job goes to `Error`."
- "Every re-run (retry, try aggressive, re-optimise) starts from the backed-up original (R§2.4)."
- Recent results keep "the last 10".

**Card actions and auto-hide (§5.5)**
- Every card action listed in §5.5 works.
- Auto-hide after `floating.auto_hide_secs` (default 6; 0 means never) applies to Done, AlreadyOptimal, Larger and Restored only.
- "Hover or keyboard focus pauses the timer."

**Errors (§6)**
- "A panic inside `spawn_blocking` shows up as a `JoinError` and becomes Error ('Unexpected error — see Details')."
- "The original is never modified before its backup is confirmed."
- Error reasons are one line and say what to do.
- Clipboard read failures are "retried once after 100 ms, then logged; no card is shown for automatic triggers".
- A hotkey that can't register is logged and left out of its tray item.

**Logs (§6)**
- Logs hold "every job's start, end, sizes, timings".
- "No file contents are logged."

**Concurrency (§4.3)**
- "encoding runs on `tokio::task::spawn_blocking`, behind a semaphore sized to `available_parallelism()`."
- "The UI thread never blocks."

**Paths (§4.3)**
- Working directory: `app_cache_dir()/work`.
- Backups: `app_data_dir()/backups`.
- Settings: `app_config_dir()/settings.json`.

**UI (§7)**
- Each card is an `<article>` with an accessible name "name.ext, state".
- Every icon button has an `aria-label`.
- Status pills pair colour with an icon and words.
- "Motion: cards slide in over 180 ms and fade out over 150 ms; none at all under `prefers-reduced-motion`."

**Testing (§8)**
- `ferry-core` tests pass on Ubuntu, macOS and Windows.
- The app crate passes `cargo clippy -D warnings` on macOS and Windows.

**Windows**
- Windows desktop behaviour is "built but not verified on a real desktop". Every status report says so.

**Project conventions**
- No `AGENTS.md` or `CONSTRAINTS.md` exists at the repo root (checked 2026-10-22).
- Reuse the existing patterns:
  - each Rust module opens with a doc comment and ends with `#[cfg(test)] mod tests`;
  - `cargo fmt`, and clippy with `-D warnings`;
  - UI text comes from `src/lib/i18n/en.json`;
  - no `var` in JavaScript or TypeScript.

## Decisions made while planning

These are mine, not the spec's. Veto any of them before execution; each says what it costs to change.

1. **The rules and the job pipeline live in `ferry-core`.** The rules are `trigger` and `run_state`; the pipeline is `pipeline`, plus `bitmap`, `glyph` and `sniff::dimensions`. They are the parts with branching worth testing, and CI tests them on three OSes; the app crate is tested on macOS and Windows only. *If wrong:* moving files.
2. **i18n moves from the app to `ferry-core`** (`ferry_core::i18n::{t, tf}`), so pipeline errors and trigger reasons are worded from the same `en.json` as the cards. *If wrong:* job errors become codes the app translates.
3. **The logs land in M2b, not M2c.** The Error card's "Details" needs a log, and manual check 2 (NFR-P2) reads timings from it. The files are `ferry.<yyyy-mm-dd>.log`, because `tracing-appender` names daily files that way, not the spec's `ferry.log`. Details reveals the newest one. *If wrong:* a custom appender.
4. **Paused is spent only on a copy Ferry would otherwise optimise.** Copying text while Paused does not use up the skip. The spec says "skips exactly one automatic trigger"; a trigger on text was never going to do anything. *If wrong:* one `if` in `trigger::automatic`.
5. **Clipboard names.** `%f` is "Clipboard yyyy-mm-dd at HH.MM.SS" (macOS screenshot style, with dots since `:` is not allowed in Windows file names). `%P` (parent folder) is "Clipboard". *If wrong:* one string in `en.json`.
6. **A clipboard working file lives at `work/clipboard/<job-id>/<name>.<ext>`**, not `work/clipboard/<name>.<ext>`. Two copies in the same second render the same name, and the second would overwrite the first card's file. *If wrong:* a counter suffix instead.
7. **Thumbnails are the backed-up original, served by Tauri's asset protocol** with the scope `$APPDATA/backups/**`. The drag icon is that thumbnail drawn into a 120×80 canvas, because `startDrag` shows a file icon at full size. *If wrong:* a generated thumbnail file per job.
8. **A re-run that does not come out smaller puts the original back.** For example, "Try aggressive" after a Done that ends Larger. The output (file or clipboard) always matches what the card says. *If wrong:* the previous result stays while the card says "Original kept".
9. **Windows clipboard choices:**
   - `clipboard-win`'s own `Monitor` replaces `clipboard-master` (§4.4). It is the same `AddClipboardFormatListener` underneath, from a crate we already need.
   - A GIF gets no `CF_DIBV5` copy, since Ferry has no GIF decoder.
   - "Clipboard Viewer Ignore" also counts as concealed.
   - Any GIF needs gifsicle, not only an animated one, since gifsicle is the GIF encoder.
   - *If wrong:* a dependency swap, or one condition.
10. **Tray states are drawn into the logo's dot**, as one template image that macOS tints:
    - Paused: two bars.
    - Working: a hollow square (the spec says "filled hull"; the "F." logo has no hull).
    - Error: "!".
    - Stopped: the whole glyph dimmed to 40%, with no slash.
    - Windows keeps its colour icon, and the state is in the tooltip.
    - The three `tray_style` variants are M2c.
    - *If wrong:* redraw in `glyph.rs`.
11. **"Notices" are Error cards without a Retry button.** Examples: "Nothing to optimise", "Settings were reset". `JobUpdate` gains `can_retry` and `output`. *If wrong:* a separate notice event.
12. **Hotkeys ignore the run state and `clipboard.enabled`.** Every other reason a hotkey does nothing becomes a card that says why, including concealed content: the user asked, so silence would look broken. *If wrong:* one match in `clipboard::optimise_now`.
13. **File jobs apply `images.max_size_mb` and `images.ignored_extensions`**, using the file's real extension and its sniffed format. *If wrong:* `Job::file`.
14. **Floating settings are honoured now:** `enabled`, `corner`, `card_size`, `auto_hide_secs` and `follow_cursor`. Their Settings UI is M2c. *If wrong:* hard-coded defaults until M2c.
15. **The M1 "Show test card" tray item is removed.** Real cards replace it.
16. **Copied files win over clipboard image data.** Finder puts a file's icon on the clipboard as an image, which is not what the user copied. *If wrong:* one branch in `trigger::check_image`.
17. **A result never replaces a newer copy.** If the user copies something else while a job runs, the result is left off the clipboard; the card still shows Done and Copy puts it on. `Snapshot` gains `seq` (as in §4.4's `ClipboardSnapshot`), and `ClipboardLog::may_replace` decides. The check applies to the first run of automatic and hotkey jobs only; Copy, Restore, Retry, Try aggressive and Re-optimise are explicit and always write. The spec does not cover this. Without it, a slow JPEG (Decision 19) would overwrite a URL the user copied a second later. *If wrong:* one `if` in `jobs::deliver`.
18. **The platform surface differs in shape from §4.4:**
    - It is `platform::clipboard::{read, seq, write_image, write_files, watch}`.
    - `Snapshot` lives in `ferry_core::trigger`, so the rules are tested without an OS.
    - `watch` returns no handle; it runs for the app's lifetime.
    - "Show in folder" uses `tauri_plugin_opener::reveal_item_in_dir`. The opener functions are called from Rust without registering the plugin, so no JavaScript permission is granted.
    - *If wrong:* renames.
19. **Large JPEG and WebP clipboard jobs stay slow.** There is no quick mode, as you decided on 2026-10-22. Cards show progress, and manual check 2 records the timings from the log.

## Review Focus

Failure modes the spec implies but no spec test names, most likely first. Each has a test in the task that owns the code.

1. **The user copies something else while a slow job runs** and expects their new copy to stay on the clipboard.
   - Task 7: `a_result_never_replaces_what_the_user_copied_while_it_ran`, `an_older_result_never_replaces_a_newer_one`, `a_result_never_replaces_an_explicit_copy`.
   - Task 7: `a_hotkey_on_ferrys_own_result_may_replace_it` checks that the guard still lets ⌃⇧A replace Ferry's own automatic result.
   - Task 8: `an_image_round_trips_and_the_change_count_moves` checks that the snapshot's `seq` is the live change count.
2. **Ferry's own clipboard write must not start another job** (an endless loop).
   - Task 7: `our_own_clipboard_writes_are_recognised`.
   - Task 8: `an_image_round_trips_and_the_change_count_moves` pins that the returned sequence number is the one the watcher sees.
3. **Re-runs start from the original and never leave a stale result.**
   - Task 4: `every_re_run_starts_from_the_original_and_reuses_its_backup`, `a_re_run_that_does_not_shrink_puts_the_original_back`, `a_clipboard_re_run_that_does_not_shrink_puts_the_original_back_on_the_clipboard`.
4. **A file Ferry cannot write, cannot back up, or that the user cancels stays exactly as it was.**
   - Task 4: `a_read_only_file_is_an_error_naming_its_folder_and_stays_untouched`, `a_failed_backup_is_an_error_and_the_file_is_untouched`, `a_cancelled_job_changes_nothing`.
5. **Password-manager content is never read.**
   - Task 3: `concealed_content_is_never_touched`.
   - Task 8: `concealed_content_is_flagged_and_not_read`.
   - Task 9: `content_marked_for_exclusion_is_concealed`.

## File Map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/i18n.rs` (moved from `src-tauri/src/i18n.rs`), `src/lib/i18n/en.json` | `t`, `tf`; every new UI string | 1 |
| `crates/ferry-core/src/sniff.rs` (modify) | `dimensions` from the header | 2 |
| `crates/ferry-core/src/run_state.rs` | `RunState` | 3 |
| `crates/ferry-core/src/trigger.rs` | `Snapshot`, `Skip`, `HotkeyPlan`, `automatic`, `hotkey` | 3 |
| `crates/ferry-core/src/fsutil.rs`, `job.rs` (modify) | `Times`; `JobUpdate.can_retry`, `.output` | 4 |
| `crates/ferry-core/src/pipeline.rs` | `Job` (clipboard and file), `run`, `restore`, `notice` | 4 |
| `crates/ferry-core/src/bitmap.rs`, `optimise/{mod,png}.rs` (modify) | DIB ↔ PNG/RGBA for the Windows clipboard | 5 |
| `crates/ferry-core/src/glyph.rs` | Tray state glyphs | 6 |
| `crates/ferry-core/src/lib.rs` | Module list | 1 to 6 |
| `src-tauri/Cargo.toml`, `tauri.conf.json`, `capabilities/default.json`, `package.json` | Dependencies, asset scope, drag permission | 7, 11 |
| `src-tauri/src/logging.rs`, `src-tauri/src/state.rs` | Log files; `AppState`, `Entry`, `ClipboardLog`, `StackInbox` | 7 |
| `src-tauri/src/platform/mod.rs`, `macos_clipboard.rs` | macOS pasteboard | 8 |
| `src-tauri/src/platform/windows_clipboard.rs` | Windows clipboard | 9 |
| `src-tauri/src/{jobs,clipboard,hotkeys,commands,tray,windows,lib}.rs` | The running app | 10 |
| `src/lib/{ipc,cards}.ts`, `src/lib/cards.test.ts`, `src/stack/{Card,Stack,DragAllBar}.svelte` | Cards and their actions | 11 |

## How to run things

Every Bash call needs the Rust environment and Node 22:

```bash
source "$HOME/.cargo/env"
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
```

GIF tests skip silently without `FERRY_GIFSICLE`.

The installed `/Applications/Ferry.app` is usually running, and single-instance hands a second Ferry over to it. Run development builds under their own identifier:

```bash
pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'
```

That build keeps its settings in `~/Library/Application Support/io.ferry.desktop.dev/` and its logs in `~/Library/Logs/io.ferry.desktop.dev/`.

Tasks 7 to 9 add app modules that nothing calls until Task 10, so `cargo test -p ferry` prints dead-code warnings until then. Clippy with `-D warnings` runs in Task 10.

Steps that say "add this test module" create the file with only that module; the next step adds the code **above** it.

---

### Task 1: UI strings move to `ferry-core`

**Files:**
- Move: `src-tauri/src/i18n.rs` → `crates/ferry-core/src/i18n.rs`
- Modify: `crates/ferry-core/src/lib.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/windows.rs`
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Produces:
  - `ferry_core::i18n::t(key: &str) -> String`.
  - `ferry_core::i18n::tf(key: &str, vars: &[(&str, &str)]) -> String`, which fills `{name}` placeholders and leaves unknown ones as they are.
  - Every M2b string key in `en.json`.

- [ ] **Step 1: Start the branch**

```bash
git switch main && git pull && git switch -c m2b-app-core-loop
```

Work in place on this branch (no worktree), as for M1 and M2a.

- [ ] **Step 2: Move the file and point it at the shared strings**

```bash
git mv src-tauri/src/i18n.rs crates/ferry-core/src/i18n.rs
```

Replace `crates/ferry-core/src/i18n.rs` with the module below. Its test module is new: `tf` does not exist yet.

```rust
//! UI strings, shared with the Svelte frontend through src/lib/i18n/en.json.
//! Core owns them so job errors are worded from the same file as the cards.

use std::{collections::HashMap, sync::LazyLock};

static EN: LazyLock<HashMap<String, String>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../../../src/lib/i18n/en.json"))
        .expect("src/lib/i18n/en.json must be a flat map of strings")
});

/// The English string for `key`; a missing key returns the key itself so the
/// gap is visible in the UI instead of crashing.
pub fn t(key: &str) -> String {
    EN.get(key).cloned().unwrap_or_else(|| key.to_string())
}

/// `t` with `{name}` placeholders filled from `vars`, as `t` does in src/lib/i18n.ts.
pub fn tf(key: &str, vars: &[(&str, &str)]) -> String {
    vars.iter().fold(t(key), |text, (name, value)| {
        text.replace(&format!("{{{name}}}"), value)
    })
}

#[cfg(test)]
mod tests {
    use super::{t, tf};

    #[test]
    fn known_key() {
        assert_eq!(t("tray.quit"), "Quit Ferry");
    }

    #[test]
    fn missing_key_falls_back_to_the_key() {
        assert_eq!(t("nope.missing"), "nope.missing");
    }

    #[test]
    fn placeholders_are_filled_and_unknown_ones_kept() {
        assert_eq!(
            tf("card.aria", &[("name", "a.png"), ("state", "optimised")]),
            "a.png, optimised"
        );
        assert_eq!(tf("card.aria", &[("name", "a.png")]), "a.png, {state}");
    }
}
```

Insert `pub mod i18n;` into `crates/ferry-core/src/lib.rs` after `pub mod geometry;`, keeping the list sorted.

- [ ] **Step 3: Run the core tests**

Run: `cargo test -p ferry-core i18n`
Expected: PASS, 3 tests. The move and `tf` land together, because `tf` is a one-line fold that the third test pins.

- [ ] **Step 4: Point the app at the core module**

In `src-tauri/src/lib.rs`, delete the line `mod i18n;`.

In `src-tauri/src/tray.rs`, replace `use crate::{i18n::t, windows};` with:

```rust
use ferry_core::i18n::t;

use crate::windows;
```

In `src-tauri/src/windows.rs`, replace `use crate::{i18n::t, platform};` with:

```rust
use ferry_core::i18n::t;

use crate::platform;
```

- [ ] **Step 5: Add the M2b strings**

Replace `src/lib/i18n/en.json` with:

```json
{
  "app.name": "Ferry",
  "tray.tooltip": "Ferry",
  "tray.settings": "Settings…",
  "tray.about": "About Ferry",
  "tray.quit": "Quit Ferry",
  "settings.window_title": "Ferry Settings",
  "settings.placeholder_title": "Settings",
  "settings.placeholder_body": "Settings panes arrive in the next milestone.",
  "card.aria": "{name}, {state}",
  "tray.test_card": "Show test card",
  "stack.window_title": "Ferry results",
  "card.state.done": "optimised",
  "card.copy": "Copy optimised file",
  "card.reveal.mac": "Show in Finder",
  "card.reveal.windows": "Show in Explorer",
  "card.aggressive": "Optimise aggressively",
  "card.close": "Close",
  "error.nothing_to_optimise": "Nothing to optimise on the clipboard",
  "error.multiple_files": "More than one file copied",
  "error.concealed": "Private clipboard content — left alone",
  "error.too_large": "Larger than {limit} MB — change the limit in Settings",
  "error.ignored_extension": ".{ext} files are ignored in Settings",
  "error.gifsicle_missing": "gifsicle missing — reinstall Ferry",
  "clipboard.name": "Clipboard {date} at {time}",
  "clipboard.folder": "Clipboard",
  "card.step.png": "Quantising colours",
  "card.step.jpeg": "Encoding JPEG",
  "card.step.webp": "Encoding WebP",
  "card.step.gif": "Running gifsicle",
  "card.detail.aggressive": "aggressive",
  "card.detail.bitmap": "from bitmap",
  "error.backup": "Backup failed: {why}",
  "error.backup_missing": "The original is gone from the backups folder",
  "error.write": "Can't write to {dir}",
  "error.read": "Can't read {name}: {why}",
  "error.not_an_image": "{name} is not a PNG, JPEG, GIF or WebP image",
  "error.decode": "Can't read this {format} — it may be damaged",
  "error.encode": "Encoder failed ({format})",
  "error.cancelled": "Cancelled",
  "error.clipboard_write": "Couldn't put the image on the clipboard",
  "error.why.disk_full": "disk full",
  "error.why.permission": "permission denied",
  "tray.tooltip.state": "Ferry — {state}",
  "tray.state.running": "Running",
  "tray.state.paused": "Paused",
  "tray.state.stopped": "Stopped",
  "tray.optimise": "Optimise clipboard",
  "tray.aggressive": "Optimise clipboard aggressively",
  "tray.recent": "Recent results",
  "tray.open_backups": "Open backups folder",
  "tray.open_work": "Open working directory",
  "tray.screenshots": "Show app UI in screenshots",
  "card.state.queued": "waiting",
  "card.state.optimising": "optimising",
  "card.state.already_optimal": "Already optimal",
  "card.state.larger": "Original kept (larger)",
  "card.state.restored": "Restored",
  "card.state.error": "Error",
  "card.waiting": "Waiting…",
  "card.restore": "Restore original",
  "card.more": "More tools (coming soon)",
  "card.cancel": "Cancel",
  "card.retry": "Retry",
  "card.reoptimise": "Re-optimise",
  "card.details": "Details",
  "card.notice": "Ferry",
  "stack.drag_all": "Drag all",
  "stack.clear_all": "Clear all",
  "error.clipboard_read": "Couldn't read the clipboard",
  "error.busy": "{name} is already being optimised",
  "error.unexpected": "Unexpected error — see Details",
  "error.settings_reset": "Settings were reset; old file kept",
  "tray.status.running": "Ferry is running",
  "tray.status.paused": "Ferry will skip the next copy",
  "tray.status.stopped": "Ferry is stopped"
}
```

Task 10 deletes `tray.test_card` together with the menu item that uses it.

- [ ] **Step 6: Verify**

Run: `cargo test -p ferry-core && cargo test -p ferry && pnpm check && pnpm test`
Expected: all pass; ferry-core reports 3 more tests than before.

- [ ] **Step 7: Commit**

```bash
git add -A crates/ferry-core/src src-tauri/src src/lib/i18n/en.json
git commit -m "refactor: move UI strings to ferry-core and add tf placeholders"
```

---

### Task 2: Image dimensions from the header

**Files:**
- Modify: `crates/ferry-core/src/sniff.rs`

**Interfaces:**
- Produces: `ferry_core::sniff::dimensions(bytes: &[u8], format: Format) -> Option<(u32, u32)>`. It reads the header only. The result is `None` when the header is cut short.
- Consumes: `testutil::{png, gif, jpeg, JpegExtras, exif_with_orientation, webp, webp_with_transparent_corner, screenshot_rgba}` and `optimise::webp::Extras` (all from M2a).

- [ ] **Step 1: Write the failing tests**

Add these two tests at the top of `mod tests` in `crates/ferry-core/src/sniff.rs`, right after `use super::*;`:

```rust
    #[test]
    fn dimensions_come_from_the_header_of_every_format() {
        use crate::{optimise::webp::Extras, testutil};
        assert_eq!(
            dimensions(&testutil::png(37, 21), Format::Png),
            Some((37, 21))
        );
        assert_eq!(dimensions(&testutil::gif(1), Format::Gif), Some((64, 48)));
        let jpeg = testutil::jpeg(
            50,
            30,
            80,
            &testutil::JpegExtras {
                exif: Some(testutil::exif_with_orientation(6, 100)),
                ..Default::default()
            },
        );
        assert_eq!(dimensions(&jpeg, Format::Jpeg), Some((50, 30)));
        let lossy = testutil::webp(41, 19, 80, &Extras::default());
        assert_eq!(dimensions(&lossy, Format::WebP), Some((41, 19)));
        let alpha = testutil::webp_with_transparent_corner(40, 22);
        assert_eq!(dimensions(&alpha, Format::WebP), Some((40, 22)));
        let rgba = testutil::screenshot_rgba(33, 17);
        let lossless = webp::Encoder::from_rgba(&rgba, 33, 17)
            .encode_lossless()
            .to_vec();
        assert_eq!(dimensions(&lossless, Format::WebP), Some((33, 17)));
    }

    #[test]
    fn dimensions_of_a_cut_header_are_unknown() {
        let png = crate::testutil::png(8, 8);
        assert_eq!(dimensions(&png[..20], Format::Png), None);
        assert_eq!(dimensions(b"\xFF\xD8\xFF\xE0\x00", Format::Jpeg), None);
        assert_eq!(dimensions(b"RIFF\0\0\0\0WEBPVP8 ", Format::WebP), None);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core sniff::tests::dimensions`
Expected: compile error, "cannot find function `dimensions`".

- [ ] **Step 3: Implement**

Add this between `skip_sub_blocks` and `#[cfg(test)]`:

```rust
/// Width and height from the header alone, without decoding; `None` when the
/// header is cut short or not where the format puts it.
pub fn dimensions(bytes: &[u8], format: Format) -> Option<(u32, u32)> {
    match format {
        Format::Png => Some((be32(bytes, 16)?, be32(bytes, 20)?)),
        Format::Gif => Some((le16(bytes, 6)?, le16(bytes, 8)?)),
        Format::Jpeg => jpeg_dimensions(bytes),
        Format::WebP => webp_dimensions(bytes),
    }
}

fn be32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_be_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn be16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_be_bytes(bytes.get(at..at + 2)?.try_into().ok()?).into())
}

fn le16(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?).into())
}

fn le24(bytes: &[u8], at: usize) -> Option<u32> {
    let b = bytes.get(at..at + 3)?;
    Some(u32::from(b[0]) | u32::from(b[1]) << 8 | u32::from(b[2]) << 16)
}

/// The size in the first start-of-frame segment (SOF0 to SOF15, except the
/// DHT, JPG and DAC markers that share that range).
fn jpeg_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    let mut pos = 2;
    loop {
        if *bytes.get(pos)? != 0xFF {
            return None;
        }
        let marker = *bytes.get(pos + 1)?;
        match marker {
            0xFF => pos += 1,               // fill byte
            0x01 | 0xD0..=0xD8 => pos += 2, // no length
            0xC0..=0xCF if !matches!(marker, 0xC4 | 0xC8 | 0xCC) => {
                return Some((be16(bytes, pos + 7)?, be16(bytes, pos + 5)?));
            }
            _ => pos += 2 + be16(bytes, pos + 2)? as usize,
        }
    }
}

fn webp_dimensions(bytes: &[u8]) -> Option<(u32, u32)> {
    match bytes.get(12..16)? {
        b"VP8X" => Some((le24(bytes, 24)? + 1, le24(bytes, 27)? + 1)),
        // Lossless: a 0x2F signature, then 14-bit width − 1 and height − 1.
        b"VP8L" if *bytes.get(20)? == 0x2F => {
            let bits = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
            Some(((bits & 0x3FFF) + 1, ((bits >> 14) & 0x3FFF) + 1))
        }
        // Lossy: a 3-byte frame tag and the 9D 01 2A start code, then 14-bit sizes.
        b"VP8 " if bytes.get(23..26)? == [0x9D, 0x01, 0x2A] => {
            Some((le16(bytes, 26)? & 0x3FFF, le16(bytes, 28)? & 0x3FFF))
        }
        _ => None,
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core sniff`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/sniff.rs
git commit -m "feat(core): read image dimensions from the header"
```

---

### Task 3: Run state and the clipboard trigger rules

**Files:**
- Create: `crates/ferry-core/src/run_state.rs`, `crates/ferry-core/src/trigger.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes:
  - `settings::{Settings, StartState}`;
  - `sniff::Format`;
  - `i18n::{t, tf}` (Task 1).
- Produces `RunState` (`ferry_core::run_state`): `Running | Paused | Stopped`, serde `snake_case`, `Copy`.
  - `RunState::from_start(StartState) -> RunState`.
  - `.next()` cycles Running → Paused → Stopped → Running.
  - `.take_automatic(&mut self) -> bool`: Paused returns `false` and becomes Running.
- Produces from `ferry_core::trigger`:
  - `Snapshot { seq: u64, image: Option<(Format, Vec<u8>)>, from_bitmap: bool, files: Vec<PathBuf>, concealed: bool, source_app: Option<String> }`, which is `Default + Clone + PartialEq`.
  - `Skip { Disabled, NotRunning, Concealed, IgnoredApp, NoImage, MultipleFiles, TooLarge { limit_mb: u32 }, IgnoredExtension(String), NeedsGifsicle }` with `.reason() -> String`.
  - `HotkeyPlan { Image(Format), Files(Vec<PathBuf>), Nothing(Skip) }`.
  - `MB: u64 = 1_000_000`.
  - `normalise_ext(&str) -> String`: lowercase, no dot, `jpeg`/`jpe` → `jpg`.
  - `is_ignored_extension(&[String], &str) -> bool`.
  - `check_image(&Snapshot, &Settings, has_gifsicle: bool) -> Result<Format, Skip>`.
  - `automatic(&Snapshot, &Settings, has_gifsicle: bool, &mut RunState) -> Result<Format, Skip>`.
  - `hotkey(&Snapshot, &Settings, has_gifsicle: bool) -> HotkeyPlan`.

- [ ] **Step 1: Write the failing run-state tests**

Create `crates/ferry-core/src/run_state.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hotkey_cycles_running_paused_stopped() {
        let mut state = RunState::Running;
        let mut seen = Vec::new();
        for _ in 0..4 {
            state = state.next();
            seen.push(state);
        }
        use RunState::*;
        assert_eq!(seen, [Paused, Stopped, Running, Paused]);
    }

    #[test]
    fn paused_skips_exactly_one_trigger_then_runs() {
        let mut state = RunState::Paused;
        assert!(!state.take_automatic());
        assert_eq!(state, RunState::Running);
        assert!(state.take_automatic());
        assert!(state.take_automatic());
    }

    #[test]
    fn stopped_skips_every_trigger() {
        let mut state = RunState::Stopped;
        for _ in 0..3 {
            assert!(!state.take_automatic());
        }
        assert_eq!(state, RunState::Stopped);
    }

    #[test]
    fn the_start_state_comes_from_settings() {
        assert_eq!(RunState::from_start(StartState::Paused), RunState::Paused);
        assert_eq!(RunState::from_start(StartState::Stopped), RunState::Stopped);
        assert_eq!(RunState::from_start(StartState::Running), RunState::Running);
    }
}
```

Add `pub mod run_state;` to `crates/ferry-core/src/lib.rs` after `pub mod optimise;`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core run_state`
Expected: compile error, "cannot find type `RunState`".

- [ ] **Step 3: Implement the run state**

Add above the test module:

```rust
//! Running, Paused or Stopped (spec §5.1). Hotkeys and card actions ignore it;
//! only automatic triggers ask it.

use serde::{Deserialize, Serialize};

use crate::settings::StartState;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunState {
    Running,
    Paused,
    Stopped,
}

impl RunState {
    pub fn from_start(start: StartState) -> Self {
        match start {
            StartState::Running => Self::Running,
            StartState::Paused => Self::Paused,
            StartState::Stopped => Self::Stopped,
        }
    }

    /// The cycle-state hotkey's order.
    pub fn next(self) -> Self {
        match self {
            Self::Running => Self::Paused,
            Self::Paused => Self::Stopped,
            Self::Stopped => Self::Running,
        }
    }

    /// Ask once per automatic trigger that has work to do. Paused lets exactly
    /// one through unprocessed and goes back to Running.
    pub fn take_automatic(&mut self) -> bool {
        match self {
            Self::Running => true,
            Self::Paused => {
                *self = Self::Running;
                false
            }
            Self::Stopped => false,
        }
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core run_state`
Expected: PASS, 4 tests.

- [ ] **Step 5: Write the failing trigger tests**

Create `crates/ferry-core/src/trigger.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n";

    fn png_snapshot() -> Snapshot {
        Snapshot {
            image: Some((Format::Png, PNG.to_vec())),
            source_app: Some("com.apple.screencaptureui".into()),
            ..Snapshot::default()
        }
    }

    fn auto(snap: &Snapshot, settings: &Settings, state: &mut RunState) -> Result<Format, Skip> {
        automatic(snap, settings, true, state)
    }

    #[test]
    fn a_copied_image_is_optimised_while_running() {
        let mut state = RunState::Running;
        assert_eq!(
            auto(&png_snapshot(), &Settings::default(), &mut state),
            Ok(Format::Png)
        );
    }

    #[test]
    fn paused_is_spent_on_an_image_not_on_text() {
        let settings = Settings::default();
        let mut state = RunState::Paused;
        assert_eq!(
            auto(&Snapshot::default(), &settings, &mut state),
            Err(Skip::NoImage)
        );
        assert_eq!(
            state,
            RunState::Paused,
            "copied text must not use up the pause"
        );
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Err(Skip::NotRunning)
        );
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Ok(Format::Png)
        );
    }

    #[test]
    fn the_clipboard_switch_turns_automatic_work_off() {
        let mut settings = Settings::default();
        settings.clipboard.enabled = false;
        let mut state = RunState::Running;
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Err(Skip::Disabled)
        );
    }

    #[test]
    fn concealed_content_is_never_touched() {
        let snap = Snapshot {
            concealed: true,
            ..png_snapshot()
        };
        let mut state = RunState::Running;
        assert_eq!(
            auto(&snap, &Settings::default(), &mut state),
            Err(Skip::Concealed)
        );
        assert_eq!(
            hotkey(&snap, &Settings::default(), true),
            HotkeyPlan::Nothing(Skip::Concealed)
        );
    }

    #[test]
    fn an_ignored_app_matches_without_case() {
        let mut settings = Settings::default();
        settings.clipboard.ignored_apps = vec!["COM.Apple.ScreenCaptureUI".into()];
        let mut state = RunState::Running;
        assert_eq!(
            auto(&png_snapshot(), &settings, &mut state),
            Err(Skip::IgnoredApp)
        );
    }

    #[test]
    fn the_size_limit_is_in_decimal_megabytes() {
        let mut settings = Settings::default();
        settings.clipboard.max_size_mb = 1;
        let mut bytes = PNG.to_vec();
        bytes.resize(1_000_000, 0);
        let mut snap = Snapshot {
            image: Some((Format::Png, bytes.clone())),
            ..png_snapshot()
        };
        assert_eq!(check_image(&snap, &settings, true), Ok(Format::Png));
        bytes.push(0);
        snap.image = Some((Format::Png, bytes));
        assert_eq!(
            check_image(&snap, &settings, true),
            Err(Skip::TooLarge { limit_mb: 1 })
        );
    }

    #[test]
    fn ignored_extensions_match_jpg_and_jpeg_in_any_case_with_or_without_a_dot() {
        let mut settings = Settings::default();
        settings.images.ignored_extensions = vec![".JPEG".into()];
        let snap = Snapshot {
            image: Some((Format::Jpeg, vec![0xFF, 0xD8, 0xFF])),
            ..png_snapshot()
        };
        assert_eq!(
            check_image(&snap, &settings, true),
            Err(Skip::IgnoredExtension("jpg".into()))
        );
        assert!(is_ignored_extension(
            &["jpg".into()],
            "Photo.JPE".rsplit('.').next().unwrap()
        ));
        assert!(!is_ignored_extension(&["png".into()], "jpg"));
    }

    #[test]
    fn a_gif_needs_the_sidecar() {
        let snap = Snapshot {
            image: Some((Format::Gif, b"GIF89a".to_vec())),
            ..png_snapshot()
        };
        assert_eq!(
            check_image(&snap, &Settings::default(), false),
            Err(Skip::NeedsGifsicle)
        );
        assert_eq!(
            check_image(&snap, &Settings::default(), true),
            Ok(Format::Gif)
        );
    }

    #[test]
    fn copied_files_are_left_alone_automatically_but_taken_by_the_hotkey() {
        let files = vec![PathBuf::from("/tmp/a.png")];
        let snap = Snapshot {
            files: files.clone(),
            ..png_snapshot()
        };
        let mut state = RunState::Running;
        assert_eq!(
            auto(&snap, &Settings::default(), &mut state),
            Err(Skip::NoImage)
        );
        assert_eq!(
            hotkey(&snap, &Settings::default(), true),
            HotkeyPlan::Files(files)
        );
    }

    #[test]
    fn more_than_one_file_is_refused_unless_allowed() {
        let files = vec![PathBuf::from("/tmp/a.png"), PathBuf::from("/tmp/b.png")];
        let snap = Snapshot {
            files: files.clone(),
            ..Snapshot::default()
        };
        let mut settings = Settings::default();
        assert_eq!(
            hotkey(&snap, &settings, true),
            HotkeyPlan::Nothing(Skip::MultipleFiles)
        );
        settings.clipboard.skip_multiple_files = false;
        assert_eq!(hotkey(&snap, &settings, true), HotkeyPlan::Files(files));
    }

    #[test]
    fn the_hotkey_ignores_the_run_state_and_the_clipboard_switch() {
        let mut settings = Settings::default();
        settings.clipboard.enabled = false;
        assert_eq!(
            hotkey(&png_snapshot(), &settings, true),
            HotkeyPlan::Image(Format::Png)
        );
    }

    #[test]
    fn an_empty_clipboard_tells_the_hotkey_user_why() {
        let plan = hotkey(&Snapshot::default(), &Settings::default(), true);
        assert_eq!(plan, HotkeyPlan::Nothing(Skip::NoImage));
        assert_eq!(
            Skip::NoImage.reason(),
            "Nothing to optimise on the clipboard"
        );
        assert_eq!(Skip::MultipleFiles.reason(), "More than one file copied");
    }
}
```

Add `pub mod trigger;` to `crates/ferry-core/src/lib.rs` after `pub mod template;`.

- [ ] **Step 6: Run them to see them fail**

Run: `cargo test -p ferry-core trigger`
Expected: compile error, "cannot find type `Snapshot`".

- [ ] **Step 7: Implement the rules**

Add above the test module:

```rust
//! What Ferry does with the clipboard (spec §5.2, §5.3). Pure decisions over a
//! snapshot; the app reads the clipboard and starts the jobs.

use std::path::PathBuf;

use crate::i18n::{t, tf};
use crate::run_state::RunState;
use crate::settings::Settings;
use crate::sniff::Format;

/// The clipboard at one moment. Image bytes are not read for concealed content.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Snapshot {
    /// The clipboard's change count (macOS) or sequence number (Windows).
    pub seq: u64,
    pub image: Option<(Format, Vec<u8>)>,
    /// Windows: the image was only a bitmap and `image` is a lossless PNG of it.
    pub from_bitmap: bool,
    pub files: Vec<PathBuf>,
    pub concealed: bool,
    /// macOS bundle id or Windows executable name of the app that copied.
    pub source_app: Option<String>,
}

/// Why the clipboard was left alone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Skip {
    Disabled,
    NotRunning,
    Concealed,
    IgnoredApp,
    NoImage,
    MultipleFiles,
    TooLarge { limit_mb: u32 },
    IgnoredExtension(String),
    NeedsGifsicle,
}

impl Skip {
    /// The Error-card text when the user asked with a hotkey.
    pub fn reason(&self) -> String {
        match self {
            Skip::Concealed => t("error.concealed"),
            Skip::MultipleFiles => t("error.multiple_files"),
            Skip::TooLarge { limit_mb } => {
                tf("error.too_large", &[("limit", &limit_mb.to_string())])
            }
            Skip::IgnoredExtension(ext) => tf("error.ignored_extension", &[("ext", ext)]),
            Skip::NeedsGifsicle => t("error.gifsicle_missing"),
            Skip::Disabled | Skip::NotRunning | Skip::IgnoredApp | Skip::NoImage => {
                t("error.nothing_to_optimise")
            }
        }
    }
}

/// What the optimise hotkeys act on.
#[derive(Debug, Clone, PartialEq)]
pub enum HotkeyPlan {
    Image(Format),
    Files(Vec<PathBuf>),
    Nothing(Skip),
}

/// Bytes in a settings megabyte; sizes are shown in decimal units too.
pub const MB: u64 = 1_000_000;

/// Lower-case, no leading dot, and `jpeg` / `jpe` count as `jpg`.
pub fn normalise_ext(ext: &str) -> String {
    let ext = ext.trim().trim_start_matches('.').to_lowercase();
    match ext.as_str() {
        "jpeg" | "jpe" => "jpg".into(),
        _ => ext,
    }
}

pub fn is_ignored_extension(ignored: &[String], ext: &str) -> bool {
    let ext = normalise_ext(ext);
    ignored.iter().any(|i| normalise_ext(i) == ext)
}

/// The content rules every clipboard image must pass, whoever asked. Copied
/// files win over image data: Finder and Explorer put the file's icon beside them.
pub fn check_image(
    snap: &Snapshot,
    settings: &Settings,
    has_gifsicle: bool,
) -> Result<Format, Skip> {
    if snap.concealed {
        return Err(Skip::Concealed);
    }
    if let Some(app) = &snap.source_app
        && settings
            .clipboard
            .ignored_apps
            .iter()
            .any(|i| i.eq_ignore_ascii_case(app))
    {
        return Err(Skip::IgnoredApp);
    }
    let Some((format, bytes)) = &snap.image else {
        return Err(Skip::NoImage);
    };
    if !snap.files.is_empty() {
        return Err(Skip::NoImage);
    }
    let limit_mb = settings.clipboard.max_size_mb;
    if bytes.len() as u64 > u64::from(limit_mb) * MB {
        return Err(Skip::TooLarge { limit_mb });
    }
    if is_ignored_extension(&settings.images.ignored_extensions, format.extension()) {
        return Err(Skip::IgnoredExtension(format.extension().into()));
    }
    if *format == Format::Gif && !has_gifsicle {
        return Err(Skip::NeedsGifsicle);
    }
    Ok(*format)
}

/// An automatic trigger (§5.2). The run state is asked last, so Paused is spent
/// on an image Ferry would really have optimised, not on copied text.
pub fn automatic(
    snap: &Snapshot,
    settings: &Settings,
    has_gifsicle: bool,
    state: &mut RunState,
) -> Result<Format, Skip> {
    if !settings.clipboard.enabled {
        return Err(Skip::Disabled);
    }
    let format = check_image(snap, settings, has_gifsicle)?;
    if !state.take_automatic() {
        return Err(Skip::NotRunning);
    }
    Ok(format)
}

/// The optimise hotkeys (§5.3): they run whatever the run state, and act on
/// copied files when there are any.
pub fn hotkey(snap: &Snapshot, settings: &Settings, has_gifsicle: bool) -> HotkeyPlan {
    if snap.concealed {
        return HotkeyPlan::Nothing(Skip::Concealed);
    }
    if !snap.files.is_empty() {
        if settings.clipboard.skip_multiple_files && snap.files.len() > 1 {
            return HotkeyPlan::Nothing(Skip::MultipleFiles);
        }
        return HotkeyPlan::Files(snap.files.clone());
    }
    match check_image(snap, settings, has_gifsicle) {
        Ok(format) => HotkeyPlan::Image(format),
        Err(skip) => HotkeyPlan::Nothing(skip),
    }
}
```

- [ ] **Step 8: Run them to see them pass**

Run: `cargo test -p ferry-core trigger`
Expected: PASS, 12 tests.

- [ ] **Step 9: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): run state and clipboard trigger rules"
```

---

### Task 4: The job pipeline

**Files:**
- Modify: `crates/ferry-core/src/fsutil.rs`, `crates/ferry-core/src/job.rs`, `crates/ferry-core/src/lib.rs`
- Create: `crates/ferry-core/src/pipeline.rs`

**Interfaces:**
- Consumes:
  - from M2a: `backup::Backups`, `cancel::CancelToken`, `optimise::{optimise, Level, OptimiseOptions, Outcome, OptimiseError}`, `template::{render, safe_file_name}`, `settings::{Settings, contract_home}`, `job::{JobId, JobState, JobUpdate, Source}`;
  - `sniff::dimensions` (Task 2);
  - `trigger::{MB, Skip, is_ignored_extension, normalise_ext}` (Task 3);
  - `i18n::{t, tf}` (Task 1).
- Produces from `fsutil`:
  - `Times { accessed, modified: SystemTime, created: Option<SystemTime> }`;
  - `read_times(&Path) -> io::Result<Times>`;
  - `set_times(&Path, &Times) -> io::Result<()>`.
- Produces from `job`: `JobUpdate` gains `can_retry: bool` and `output: Option<PathBuf>`.
- Produces from `ferry_core::pipeline`:
  - `type Deliver<'a> = &'a mut dyn FnMut(Format, &[u8], &Path) -> Result<(), String>`.
  - `RunOptions { compression: u8, strip_metadata, preserve_dates: bool, gifsicle: Option<PathBuf>, cancel: CancelToken }` and `RunOptions::new(&Settings, Option<PathBuf>, CancelToken)`.
  - `Finished { update: JobUpdate, cause: Option<String> }`.
  - `Job`, with public fields `id, source, name, ext, format, before, dims, from_bitmap, output, level`, and these methods:
    - `Job::clipboard(format, bytes: Vec<u8>, from_bitmap: bool, file_template: &str, work_dir: &Path, now: NaiveDateTime) -> Job`;
    - `Job::file(path: &Path, settings: &Settings, has_gifsicle: bool) -> Result<Job, String>`;
    - `.update(JobState, after: Option<u64>) -> JobUpdate`;
    - `.run(level, &RunOptions, &Backups, emit: &mut dyn FnMut(JobUpdate), deliver: Deliver) -> Option<Finished>`, where `None` means cancelled before any write;
    - `.restore(&RunOptions, deliver: Deliver) -> Finished`.
  - `notice(reason: String) -> JobUpdate`: an Error card with `can_retry: false`.

- [ ] **Step 1: Keep a file's times before the first write**

M2a's `copy_times(from, to)` needs the original file to still exist. A file job replaces it in place, so the pipeline reads the times first and sets them after. In `crates/ferry-core/src/fsutil.rs`, change `use std::time::Duration;` to `use std::time::{Duration, SystemTime};`, and replace the whole `copy_times` function with:

```rust
/// A file's times, captured before Ferry replaces it; std's `FileTimes` cannot
/// be read back.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Times {
    pub accessed: SystemTime,
    pub modified: SystemTime,
    pub created: Option<SystemTime>,
}

pub fn read_times(path: &Path) -> io::Result<Times> {
    let meta = fs::metadata(path)?;
    Ok(Times {
        accessed: meta.accessed()?,
        modified: meta.modified()?,
        created: meta.created().ok(),
    })
}

/// Give `path` these times; the created time only on macOS and Windows.
pub fn set_times(path: &Path, times: &Times) -> io::Result<()> {
    #[allow(unused_mut)]
    let mut file_times = FileTimes::new()
        .set_accessed(times.accessed)
        .set_modified(times.modified);
    #[cfg(target_os = "macos")]
    if let Some(created) = times.created {
        use std::os::macos::fs::FileTimesExt;
        file_times = file_times.set_created(created);
    }
    #[cfg(windows)]
    if let Some(created) = times.created {
        use std::os::windows::fs::FileTimesExt;
        file_times = file_times.set_created(created);
    }
    OpenOptions::new()
        .write(true)
        .open(path)?
        .set_times(file_times)
}

/// Give `to` the modified, accessed and (macOS, Windows) created times of `from`.
pub fn copy_times(from: &Path, to: &Path) -> io::Result<()> {
    set_times(to, &read_times(from)?)
}
```

Run: `cargo test -p ferry-core fsutil`
Expected: PASS. The existing `copy_times_carries_modified_and_created_times` test covers both halves.

- [ ] **Step 2: Give `JobUpdate` its two new fields**

In `crates/ferry-core/src/job.rs`, add the following after `pub can_restore: bool,` in `struct JobUpdate`:

```rust
    /// An Error card for a real job; notices such as "Nothing to optimise" have no retry.
    pub can_retry: bool,
    /// The file the card's copy, drag and reveal act on, once it exists.
    pub output: Option<PathBuf>,
```

Then add these after `can_restore: true,` in the test `an_update_round_trips_through_json`:

```rust
            can_retry: false,
            output: Some(PathBuf::from("/tmp/work/shot.png")),
```

Run: `cargo test -p ferry-core job && cargo check -p ferry`
Expected: PASS, and the app still compiles (it builds no `JobUpdate` yet).

- [ ] **Step 3: Write the failing pipeline tests**

Create `crates/ferry-core/src/pipeline.rs` with this test module. It has 16 tests:
- clipboard and file round trips;
- re-runs from the original;
- restore and cancel;
- read-only folders and failed backups;
- damaged images and a refused clipboard;
- settings exclusions and notices.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;
    use chrono::NaiveDate;
    use std::cell::RefCell;

    struct Env {
        dir: tempfile::TempDir,
        backups: Backups,
    }

    impl Env {
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let backups = Backups::new(dir.path().join("backups"));
            Self { dir, backups }
        }
        fn work(&self) -> PathBuf {
            self.dir.path().join("work")
        }
        fn file(&self, name: &str, bytes: &[u8]) -> PathBuf {
            let path = self.dir.path().join(name);
            fs::write(&path, bytes).unwrap();
            path
        }
    }

    fn now() -> NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 22)
            .unwrap()
            .and_hms_opt(14, 23, 11)
            .unwrap()
    }

    fn options() -> RunOptions {
        RunOptions::new(
            &Settings::default(),
            testutil::gifsicle(),
            CancelToken::new(),
        )
    }

    /// What a run emitted and delivered.
    #[derive(Default)]
    struct Seen {
        updates: Vec<JobUpdate>,
        delivered: Vec<(Format, Vec<u8>, PathBuf)>,
    }

    fn run(job: &mut Job, level: Level, o: &RunOptions, env: &Env) -> (Option<Finished>, Seen) {
        let seen = RefCell::new(Seen::default());
        let finished = job.run(
            level,
            o,
            &env.backups,
            &mut |u| seen.borrow_mut().updates.push(u),
            &mut |f, b, p| {
                seen.borrow_mut()
                    .delivered
                    .push((f, b.to_vec(), p.to_owned()));
                Ok(())
            },
        );
        (finished, seen.into_inner())
    }

    fn clipboard_png(env: &Env, template: &str) -> (Job, Vec<u8>) {
        let input = testutil::png(300, 200);
        let job = Job::clipboard(
            Format::Png,
            input.clone(),
            false,
            template,
            &env.work(),
            now(),
        );
        (job, input)
    }

    #[test]
    fn a_clipboard_image_is_backed_up_optimised_and_put_back_on_the_clipboard() {
        let env = Env::new();
        let (mut job, input) = clipboard_png(&env, "%f");
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;

        assert_eq!(update.state, JobState::Done);
        let after = update.after.unwrap();
        assert!(after < input.len() as u64);
        assert_eq!(update.detail, "300×200 · PNG");
        assert!(update.can_restore);
        let backup = update.thumb_path.clone().unwrap();
        assert_eq!(
            fs::read(&backup).unwrap(),
            input,
            "the backup is the original"
        );

        let output = update.output.clone().unwrap();
        assert_eq!(fs::metadata(&output).unwrap().len(), after);
        assert_eq!(seen.delivered.len(), 1);
        let (format, bytes, file) = &seen.delivered[0];
        assert_eq!(
            (*format, bytes.len() as u64, file),
            (Format::Png, after, &output)
        );

        assert!(matches!(seen.updates[0].state, JobState::Optimising { .. }));
        assert_eq!(seen.updates[0].thumb_path, Some(backup));
    }

    #[test]
    fn the_working_file_is_named_by_the_template_inside_its_own_job_folder() {
        let env = Env::new();
        let (job, _) = clipboard_png(&env, "%f %z");
        assert_eq!(job.name, "Clipboard 2026-10-22 at 14.23.11 300x200");
        let expected = env
            .work()
            .join("clipboard")
            .join(job.id.as_str())
            .join("Clipboard 2026-10-22 at 14.23.11 300x200.png");
        assert_eq!(job.output, expected);

        let (unsafe_name, _) = clipboard_png(&env, "../%y/%m");
        assert_eq!(unsafe_name.name, ".._2026_10");
        let (twin, _) = clipboard_png(&env, "%f %z");
        assert_ne!(twin.output, job.output, "same name, separate folders");
    }

    #[test]
    fn an_image_that_does_not_shrink_stays_off_the_clipboard_but_can_still_be_dragged() {
        let env = Env::new();
        let input = testutil::png(1, 1);
        let mut job = Job::clipboard(Format::Png, input.clone(), false, "%f", &env.work(), now());
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;
        assert!(matches!(
            update.state,
            JobState::Larger | JobState::AlreadyOptimal
        ));
        assert!(seen.delivered.is_empty());
        assert!(!update.can_restore);
        assert_eq!(fs::read(update.output.unwrap()).unwrap(), input);
    }

    #[test]
    fn a_file_is_replaced_in_place_with_its_dates_kept() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let then = std::time::UNIX_EPOCH + std::time::Duration::from_secs(1_000_000_000);
        let times = Times {
            accessed: then,
            modified: then,
            created: Some(then),
        };
        fsutil::set_times(&path, &times).unwrap();

        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        assert_eq!((job.name.as_str(), job.ext.as_str()), ("shot", "png"));
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;

        assert_eq!(update.state, JobState::Done);
        assert_eq!(update.output, Some(path.clone()));
        assert!(fs::metadata(&path).unwrap().len() < input.len() as u64);
        assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), then);
        assert_eq!(fs::read(update.thumb_path.unwrap()).unwrap(), input);
        assert!(
            seen.delivered.is_empty(),
            "file jobs leave the clipboard alone"
        );
    }

    #[test]
    fn every_re_run_starts_from_the_original_and_reuses_its_backup() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        let first = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        let second = run(&mut job, Level::Aggressive, &o, &env).0.unwrap().update;

        assert_eq!(first.id, second.id);
        assert_eq!(first.thumb_path, second.thumb_path, "one backup per job");
        assert_eq!(fs::read(second.thumb_path.unwrap()).unwrap(), input);
        let expected = optimise::optimise(
            &input,
            Format::Png,
            &OptimiseOptions {
                level: Level::Aggressive,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        let Outcome::Smaller { bytes } = expected else {
            panic!("aggressive must shrink")
        };
        assert_eq!(
            fs::read(&path).unwrap(),
            bytes,
            "aggressive ran on the original"
        );
        assert!(second.detail.ends_with("· aggressive"));
    }

    #[test]
    fn a_re_run_that_does_not_shrink_puts_the_original_back() {
        let env = Env::new();
        let input = testutil::jpeg(400, 300, 50, &testutil::JpegExtras::default());
        let path = env.file("photo.jpg", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let low = RunOptions {
            compression: 30,
            ..options()
        };
        let high = RunOptions {
            compression: 100,
            ..options()
        };

        let first = run(&mut job, Level::Normal, &low, &env).0.unwrap().update;
        assert_eq!(first.state, JobState::Done);
        assert_ne!(fs::read(&path).unwrap(), input);

        let second = run(&mut job, Level::Normal, &high, &env).0.unwrap().update;
        assert_eq!(second.state, JobState::Larger);
        assert_eq!(
            fs::read(&path).unwrap(),
            input,
            "the file matches the card again"
        );
    }

    #[test]
    fn a_clipboard_re_run_that_does_not_shrink_puts_the_original_back_on_the_clipboard() {
        let env = Env::new();
        let input = testutil::jpeg(400, 300, 50, &testutil::JpegExtras::default());
        let mut job = Job::clipboard(Format::Jpeg, input.clone(), false, "%f", &env.work(), now());
        let low = RunOptions {
            compression: 30,
            ..options()
        };
        let high = RunOptions {
            compression: 100,
            ..options()
        };
        let (_, first) = run(&mut job, Level::Normal, &low, &env);
        let (finished, second) = run(&mut job, Level::Normal, &high, &env);
        assert_eq!(first.delivered.len(), 1);
        assert_eq!(finished.unwrap().update.state, JobState::Larger);
        assert_eq!(second.delivered.len(), 1);
        assert_eq!(second.delivered[0].1, input);
    }

    #[test]
    fn restore_puts_the_original_back_in_place_and_on_the_clipboard() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut file_job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        run(&mut file_job, Level::Normal, &o, &env);
        let restored = file_job.restore(&o, &mut |_, _, _| Ok(()));
        assert_eq!(restored.update.state, JobState::Restored);
        assert_eq!(fs::read(&path).unwrap(), input);

        let (mut clip, input) = clipboard_png(&env, "%f");
        run(&mut clip, Level::Normal, &o, &env);
        let mut delivered = Vec::new();
        let restored = clip.restore(&o, &mut |_, b, _| {
            delivered.push(b.to_vec());
            Ok(())
        });
        assert_eq!(restored.update.state, JobState::Restored);
        assert_eq!(delivered, vec![input.clone()]);
        assert_eq!(fs::read(&clip.output).unwrap(), input);
    }

    #[test]
    fn a_cancelled_job_changes_nothing() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        o.cancel.cancel();
        let (finished, seen) = run(&mut job, Level::Normal, &o, &env);
        assert!(finished.is_none());
        assert!(seen.updates.is_empty() && seen.delivered.is_empty());
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[cfg(unix)]
    #[test]
    fn a_read_only_file_is_an_error_naming_its_folder_and_stays_untouched() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut perms = fs::metadata(&path).unwrap().permissions();
        perms.set_readonly(true);
        fs::set_permissions(&path, perms).unwrap();

        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let finished = run(&mut job, Level::Normal, &options(), &env).0.unwrap();
        let JobState::Error { reason } = &finished.update.state else {
            panic!("{:?}", finished.update.state)
        };
        assert!(reason.starts_with("Can't write to "), "{reason}");
        assert!(finished.update.can_retry);
        assert!(finished.cause.is_some());
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[test]
    fn a_failed_backup_is_an_error_and_the_file_is_untouched() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let blocker = env.file("not-a-folder", b"x");
        let backups = Backups::new(&blocker);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let finished = job
            .run(
                Level::Normal,
                &options(),
                &backups,
                &mut |_| {},
                &mut |_, _, _| Ok(()),
            )
            .unwrap();
        let JobState::Error { reason } = finished.update.state else {
            panic!()
        };
        assert!(reason.starts_with("Backup failed: "), "{reason}");
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[test]
    fn a_damaged_image_is_an_error_with_a_retry() {
        let env = Env::new();
        let mut broken = testutil::png(300, 200);
        broken.truncate(broken.len() / 2);
        let mut job = Job::clipboard(Format::Png, broken, false, "%f", &env.work(), now());
        let finished = run(&mut job, Level::Normal, &options(), &env).0.unwrap();
        assert_eq!(
            finished.update.state,
            JobState::Error {
                reason: "Can't read this PNG — it may be damaged".into()
            }
        );
        assert!(finished.update.can_retry);
    }

    #[test]
    fn a_clipboard_that_refuses_the_write_is_an_error() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let finished = job
            .run(
                Level::Normal,
                &options(),
                &env.backups,
                &mut |_| {},
                &mut |_, _, _| Err("busy".into()),
            )
            .unwrap();
        assert_eq!(
            finished.update.state,
            JobState::Error {
                reason: "Couldn't put the image on the clipboard".into()
            }
        );
        assert_eq!(finished.cause.as_deref(), Some("busy"));
    }

    #[test]
    fn files_the_settings_exclude_are_refused_with_a_reason() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.ignored_extensions = vec!["png".into()];
        let png = env.file("a.PNG", &testutil::png(8, 8));
        assert_eq!(
            Job::file(&png, &settings, true).unwrap_err(),
            ".png files are ignored in Settings"
        );

        let text = env.file("notes.txt", b"hello");
        assert_eq!(
            Job::file(&text, &Settings::default(), true).unwrap_err(),
            "notes.txt is not a PNG, JPEG, GIF or WebP image"
        );

        let gif = env.file("anim.gif", &testutil::gif(2));
        assert_eq!(
            Job::file(&gif, &Settings::default(), false).unwrap_err(),
            "gifsicle missing — reinstall Ferry"
        );

        settings.images.ignored_extensions.clear();
        settings.images.max_size_mb = 1;
        let big = env.file("big.png", &vec![0; 1_000_001]);
        assert_eq!(
            Job::file(&big, &settings, true).unwrap_err(),
            "Larger than 1 MB — change the limit in Settings"
        );
    }

    #[test]
    fn a_file_sniffed_by_content_keeps_its_own_name() {
        let env = Env::new();
        let path = env.file(
            "really-a-jpeg.png",
            &testutil::jpeg(20, 20, 80, &Default::default()),
        );
        let job = Job::file(&path, &Settings::default(), true).unwrap();
        assert_eq!((job.format, job.ext.as_str()), (Format::Jpeg, "png"));
    }

    #[test]
    fn a_notice_is_an_error_card_without_retry() {
        let update = notice("Nothing to optimise on the clipboard".into());
        assert!(matches!(update.state, JobState::Error { .. }));
        assert!(!update.can_retry && !update.can_restore);
        assert!(update.output.is_none());
    }
}
```

Add `pub mod pipeline;` to `crates/ferry-core/src/lib.rs` after `pub mod optimise;`.

- [ ] **Step 4: Run them to see them fail**

Run: `cargo test -p ferry-core pipeline`
Expected: compile error, "cannot find type `Job`".

- [ ] **Step 5: Implement the pipeline**

Add above the test module:

```rust
//! One job end to end (spec §5.4): back up the original, optimise it, write the
//! result and report each step as a `JobUpdate`. The app runs this on a worker
//! thread and owns the clipboard, which it reaches through a `deliver` callback.

use std::fmt::Display;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::NaiveDateTime;

use crate::backup::Backups;
use crate::cancel::CancelToken;
use crate::fsutil::{self, Times};
use crate::i18n::{t, tf};
use crate::job::{JobId, JobState, JobUpdate, Source};
use crate::optimise::{self, Level, OptimiseError, OptimiseOptions, Outcome};
use crate::settings::{Settings, contract_home};
use crate::sniff::{self, Format};
use crate::template;
use crate::trigger::{MB, Skip, is_ignored_extension, normalise_ext};

/// Puts an image on the clipboard: format, bytes and the working file holding them.
pub type Deliver<'a> = &'a mut dyn FnMut(Format, &[u8], &Path) -> Result<(), String>;

/// Settings a run reads, taken when the run starts.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub compression: u8,
    pub strip_metadata: bool,
    pub preserve_dates: bool,
    pub gifsicle: Option<PathBuf>,
    pub cancel: CancelToken,
}

impl RunOptions {
    pub fn new(settings: &Settings, gifsicle: Option<PathBuf>, cancel: CancelToken) -> Self {
        Self {
            compression: settings.images.compression,
            strip_metadata: settings.images.strip_metadata,
            preserve_dates: settings.images.preserve_dates,
            gifsicle,
            cancel,
        }
    }
}

/// How a run ended. `cause` is the underlying error, for the log only.
#[derive(Debug)]
pub struct Finished {
    pub update: JobUpdate,
    pub cause: Option<String>,
}

#[derive(Debug)]
pub struct Job {
    pub id: JobId,
    pub source: Source,
    /// Shown on the card: the rendered template, or the file's own stem.
    pub name: String,
    pub ext: String,
    pub format: Format,
    pub before: u64,
    pub dims: Option<(u32, u32)>,
    pub from_bitmap: bool,
    /// The working file of a clipboard job, or the file itself.
    pub output: PathBuf,
    /// The level of the latest run; retry and re-optimise use it again.
    pub level: Level,
    /// The original, once stored. Every run starts from it (R§2.4).
    backup: Option<PathBuf>,
    /// A clipboard job's original until it is backed up.
    pending: Option<Vec<u8>>,
    /// A file job's times, read before its first write.
    times: Option<Times>,
    /// `output` (and the clipboard) hold an optimised result, not the original.
    changed: bool,
}

impl Job {
    /// A clipboard image. Its result goes to
    /// `<work_dir>/clipboard/<job id>/<rendered template>.<ext>`, so two jobs
    /// with the same name never share a file.
    pub fn clipboard(
        format: Format,
        bytes: Vec<u8>,
        from_bitmap: bool,
        file_template: &str,
        work_dir: &Path,
        now: NaiveDateTime,
    ) -> Job {
        let id = JobId::generate();
        let ext = format.extension();
        let dims = sniff::dimensions(&bytes, format);
        let date = now.format("%Y-%m-%d").to_string();
        let time = now.format("%H.%M.%S").to_string();
        let stem = tf("clipboard.name", &[("date", &date), ("time", &time)]);
        let (width, height) = dims.unwrap_or_default();
        let ctx = template::Ctx {
            stem: &stem,
            ext,
            width,
            height,
            parent: &t("clipboard.folder"),
            date: now.date(),
        };
        let name = template::safe_file_name(&template::render(file_template, &ctx));
        let output = work_dir
            .join("clipboard")
            .join(id.as_str())
            .join(format!("{name}.{ext}"));
        Job {
            id,
            source: Source::Clipboard,
            name,
            ext: ext.into(),
            format,
            before: bytes.len() as u64,
            dims,
            from_bitmap,
            output,
            level: Level::Normal,
            backup: None,
            pending: Some(bytes),
            times: None,
            changed: false,
        }
    }

    /// A copied file, to be optimised in place. `Err` is the card text for a
    /// file the settings exclude or that is not a supported image.
    pub fn file(path: &Path, settings: &Settings, has_gifsicle: bool) -> Result<Job, String> {
        let file_name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let (stem, ext) = match file_name.rsplit_once('.') {
            Some((stem, ext)) if !stem.is_empty() => (stem.to_owned(), ext.to_owned()),
            _ => (file_name.clone(), String::new()),
        };
        if !ext.is_empty() && is_ignored_extension(&settings.images.ignored_extensions, &ext) {
            return Err(Skip::IgnoredExtension(normalise_ext(&ext)).reason());
        }
        let cant_read = |e: io::Error| {
            tf(
                "error.read",
                &[("name", &file_name), ("why", &describe(&e))],
            )
        };
        let len = fs::metadata(path).map_err(cant_read)?.len();
        let limit_mb = settings.images.max_size_mb;
        if len > u64::from(limit_mb) * MB {
            return Err(Skip::TooLarge { limit_mb }.reason());
        }
        let bytes = fs::read(path).map_err(cant_read)?;
        let format = sniff::sniff(&bytes)
            .ok_or_else(|| tf("error.not_an_image", &[("name", &file_name)]))?;
        if format == Format::Gif && !has_gifsicle {
            return Err(Skip::NeedsGifsicle.reason());
        }
        Ok(Job {
            id: JobId::generate(),
            source: Source::File,
            name: stem,
            ext,
            format,
            before: len,
            dims: sniff::dimensions(&bytes, format),
            from_bitmap: false,
            output: path.to_owned(),
            level: Level::Normal,
            backup: None,
            pending: None,
            times: None,
            changed: false,
        })
    }

    /// This job's card in `state`.
    pub fn update(&self, state: JobState, after: Option<u64>) -> JobUpdate {
        let done = state == JobState::Done;
        JobUpdate {
            id: self.id.clone(),
            can_restore: done && self.backup.is_some(),
            can_retry: matches!(state, JobState::Error { .. }),
            detail: if done { self.detail() } else { String::new() },
            state,
            name: self.name.clone(),
            ext: self.ext.clone(),
            before: self.before,
            after,
            thumb_path: self.backup.clone(),
            source: self.source,
            output: self.output.exists().then(|| self.output.clone()),
        }
    }

    /// Optimise the original at `level`, write the result and report each step.
    /// `None` means the job was cancelled before anything was written. The
    /// output always matches the card: a re-run that does not come out smaller
    /// puts the original back.
    pub fn run(
        &mut self,
        level: Level,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
        deliver: Deliver,
    ) -> Option<Finished> {
        self.level = level;
        if o.cancel.is_cancelled() {
            return None;
        }
        let original = match self.original(backups) {
            Ok(bytes) => bytes,
            Err(e) => return Some(self.fail(tf("error.backup", &[("why", &describe(&e))]), e)),
        };
        let step = t(match self.format {
            Format::Png => "card.step.png",
            Format::Jpeg => "card.step.jpeg",
            Format::WebP => "card.step.webp",
            Format::Gif => "card.step.gif",
        });
        emit(self.update(
            JobState::Optimising {
                progress: 0.0,
                step,
            },
            None,
        ));
        let options = OptimiseOptions {
            level,
            compression: o.compression,
            strip_metadata: o.strip_metadata,
            gifsicle: o.gifsicle.clone(),
            cancel: o.cancel.clone(),
        };
        let outcome = match optimise::optimise(&original, self.format, &options) {
            Ok(outcome) => outcome,
            Err(OptimiseError::Cancelled) => return None,
            Err(e) => return Some(self.fail(self.reason(&e), e)),
        };
        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }
        let (state, bytes, after) = match &outcome {
            Outcome::Smaller { bytes } => (JobState::Done, bytes, Some(bytes.len() as u64)),
            Outcome::AlreadyOptimal => (JobState::AlreadyOptimal, &original, None),
            Outcome::Larger => (JobState::Larger, &original, None),
        };
        let optimised = state == JobState::Done;
        if let Err((reason, cause)) = self.write(bytes, optimised || self.changed, o, deliver) {
            return Some(self.fail(reason, cause));
        }
        self.changed = optimised;
        Some(Finished {
            update: self.update(state, after),
            cause: None,
        })
    }

    /// Put the original back (§5.5): in place for a file; in the working file
    /// and on the clipboard for a clipboard image.
    pub fn restore(&mut self, o: &RunOptions, deliver: Deliver) -> Finished {
        let read = self
            .backup
            .as_ref()
            .ok_or_else(|| io::Error::from(io::ErrorKind::NotFound))
            .and_then(fs::read);
        let original = match read {
            Ok(bytes) => bytes,
            Err(e) => return self.fail(t("error.backup_missing"), e),
        };
        if let Err((reason, cause)) = self.write(&original, true, o, deliver) {
            return self.fail(reason, cause);
        }
        self.changed = false;
        Finished {
            update: self.update(JobState::Restored, None),
            cause: None,
        }
    }

    /// The original bytes, backed up first on the first run.
    fn original(&mut self, backups: &Backups) -> io::Result<Vec<u8>> {
        if self.backup.is_none() {
            let stored = match self.source {
                Source::Clipboard => {
                    let name = format!("{}.{}", self.name, self.ext);
                    let bytes = self.pending.as_deref().unwrap_or_default();
                    backups.store_bytes(self.id.as_str(), &name, bytes)?
                }
                Source::File => {
                    self.times = fsutil::read_times(&self.output).ok();
                    backups.store_file(self.id.as_str(), &self.output)?
                }
            };
            self.backup = Some(stored);
            self.pending = None;
        }
        fs::read(self.backup.as_ref().expect("stored above"))
    }

    /// Write `bytes` to the output. A clipboard job always refreshes its working
    /// file (drag and reveal use it) and touches the clipboard only when
    /// `replace`; a file job is only rewritten when `replace`.
    fn write(
        &self,
        bytes: &[u8],
        replace: bool,
        o: &RunOptions,
        deliver: Deliver,
    ) -> Result<(), (String, String)> {
        let folder = self.output.parent().unwrap_or(Path::new(""));
        let cant_write =
            |e: io::Error| (tf("error.write", &[("dir", &shown(folder))]), e.to_string());
        match self.source {
            Source::Clipboard => {
                fs::create_dir_all(folder)
                    .and_then(|()| fs::write(&self.output, bytes))
                    .map_err(cant_write)?;
                if replace {
                    deliver(self.format, bytes, &self.output)
                        .map_err(|e| (t("error.clipboard_write"), e))?;
                }
            }
            Source::File if replace => {
                fsutil::atomic_replace(&self.output, bytes).map_err(cant_write)?;
                // Best effort: the content is safe; a lost tag or date is not worth an Error card.
                if let Some(backup) = &self.backup {
                    let _ = fsutil::copy_xattrs(backup, &self.output);
                }
                if o.preserve_dates
                    && let Some(times) = &self.times
                {
                    let _ = fsutil::set_times(&self.output, times);
                }
            }
            Source::File => {}
        }
        Ok(())
    }

    fn fail(&self, reason: String, cause: impl Display) -> Finished {
        Finished {
            update: self.update(JobState::Error { reason }, None),
            cause: Some(cause.to_string()),
        }
    }

    fn reason(&self, e: &OptimiseError) -> String {
        let format = label(self.format);
        match e {
            OptimiseError::Decode(_) => tf("error.decode", &[("format", format)]),
            OptimiseError::Encode(_) => tf("error.encode", &[("format", format)]),
            OptimiseError::Unsupported(message) => message.clone(),
            OptimiseError::Cancelled => t("error.cancelled"),
        }
    }

    /// "2880×1800 · PNG", plus "aggressive" and "from bitmap" when they apply.
    fn detail(&self) -> String {
        let mut parts = Vec::new();
        if let Some((w, h)) = self.dims {
            parts.push(format!("{w}×{h}"));
        }
        parts.push(label(self.format).to_owned());
        if self.level == Level::Aggressive {
            parts.push(t("card.detail.aggressive"));
        }
        if self.from_bitmap {
            parts.push(t("card.detail.bitmap"));
        }
        parts.join(" · ")
    }
}

/// An Error card that belongs to no job, such as "Nothing to optimise on the clipboard".
pub fn notice(reason: String) -> JobUpdate {
    JobUpdate {
        id: JobId::generate(),
        state: JobState::Error { reason },
        name: String::new(),
        ext: String::new(),
        before: 0,
        after: None,
        detail: String::new(),
        thumb_path: None,
        source: Source::Clipboard,
        can_restore: false,
        can_retry: false,
        output: None,
    }
}

fn label(format: Format) -> &'static str {
    match format {
        Format::Png => "PNG",
        Format::Jpeg => "JPEG",
        Format::Gif => "GIF",
        Format::WebP => "WebP",
    }
}

/// A short, plain reason: "disk full" rather than "No space left on device (os error 28)".
fn describe(e: &io::Error) -> String {
    match e.kind() {
        io::ErrorKind::StorageFull => t("error.why.disk_full"),
        io::ErrorKind::PermissionDenied => t("error.why.permission"),
        _ => e.to_string(),
    }
}

/// A folder as the user would type it: `~/Shared`, not `/Users/me/Shared`.
fn shown(folder: &Path) -> String {
    match std::env::home_dir() {
        Some(home) => contract_home(folder, &home),
        None => folder.display().to_string(),
    }
}
```

- [ ] **Step 6: Run them to see them pass**

Run: `cargo test -p ferry-core pipeline`
Expected: PASS, 16 tests.

- [ ] **Step 7: Run the whole core suite and lint it**

Run: `cargo test -p ferry-core && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: all pass, and no warnings.

- [ ] **Step 8: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): job pipeline for clipboard and file jobs"
```

---

### Task 5: Windows clipboard bitmaps

**Files:**
- Create: `crates/ferry-core/src/bitmap.rs`
- Modify: `crates/ferry-core/src/optimise/mod.rs`, `crates/ferry-core/src/optimise/png.rs`, `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `optimise::png::decode` (made `pub(crate)` here), `mozjpeg::Decompress`, `webp::Decoder`, `png::Encoder`.
- Produces, in `ferry_core::bitmap`:
  - `type Rgba = (u32, u32, Vec<u8>)`;
  - `dib_to_png(&[u8]) -> Result<Vec<u8>, String>`;
  - `dib_to_rgba(&[u8]) -> Result<Rgba, String>`, which reads 40-byte, V4 and V5 headers, 24 and 32 bpp, `BI_RGB` and `BI_BITFIELDS`, and bottom-up or top-down rows (an all-zero alpha byte means opaque);
  - `rgba_to_dibv5(w, h, &[u8]) -> Vec<u8>`, which writes an sRGB V5 header with bottom-up BGRA rows;
  - `decode_rgba(&[u8], Format) -> Result<Rgba, String>`, which returns `Err` for GIF.

- [ ] **Step 1: Let the bitmap code decode PNGs**

In `crates/ferry-core/src/optimise/mod.rs`, change `mod png;` to `pub(crate) mod png;`. In `crates/ferry-core/src/optimise/png.rs`, change `fn decode(input: &[u8])` to `pub(crate) fn decode(input: &[u8])`.

- [ ] **Step 2: Write the failing tests**

Create `crates/ferry-core/src/bitmap.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{sniff, testutil};

    /// A 40-byte BITMAPINFOHEADER.
    fn info_header(width: i32, height: i32, bits: u16, compression: u32) -> Vec<u8> {
        let mut h = Vec::new();
        h.extend_from_slice(&40_u32.to_le_bytes());
        h.extend_from_slice(&width.to_le_bytes());
        h.extend_from_slice(&height.to_le_bytes());
        h.extend_from_slice(&1_u16.to_le_bytes());
        h.extend_from_slice(&bits.to_le_bytes());
        h.extend_from_slice(&compression.to_le_bytes());
        h.extend_from_slice(&[0; 20]); // image size, resolution, palette counts
        h
    }

    #[test]
    fn rgba_survives_a_round_trip_through_a_v5_bitmap() {
        let (w, h) = (5, 3);
        let rgba: Vec<u8> = (0..w * h * 4).map(|i| (i * 7) as u8).collect();
        let dib = rgba_to_dibv5(w, h, &rgba);
        assert_eq!(dib.len(), 124 + rgba.len());
        assert_eq!(dib_to_rgba(&dib).unwrap(), (w, h, rgba));
    }

    #[test]
    fn a_bottom_up_24_bit_bitmap_is_read_top_row_first_with_row_padding() {
        // 3×2 pixels; each 9-byte row is padded to 12. Bottom row first in the file.
        let mut dib = info_header(3, 2, 24, BI_RGB);
        dib.extend_from_slice(&[255, 0, 0, 255, 0, 0, 255, 0, 0, 0, 0, 0]); // bottom: blue
        dib.extend_from_slice(&[0, 0, 255, 0, 0, 255, 0, 0, 255, 0, 0, 0]); // top: red
        let (w, h, rgba) = dib_to_rgba(&dib).unwrap();
        assert_eq!((w, h), (3, 2));
        assert_eq!(&rgba[..4], &[255, 0, 0, 255], "top-left is red");
        assert_eq!(&rgba[12..16], &[0, 0, 255, 255], "bottom-left is blue");
    }

    #[test]
    fn a_32_bit_bitmap_with_an_unused_alpha_byte_is_opaque() {
        let mut dib = info_header(2, -1, 32, BI_RGB); // top-down
        dib.extend_from_slice(&[10, 20, 30, 0, 40, 50, 60, 0]);
        let (_, _, rgba) = dib_to_rgba(&dib).unwrap();
        assert_eq!(rgba, vec![30, 20, 10, 255, 60, 50, 40, 255]);
    }

    #[test]
    fn bitfield_masks_after_a_short_header_are_honoured() {
        let mut dib = info_header(1, 1, 32, BI_BITFIELDS);
        // Masks for an RGBX layout in memory: red in the low byte.
        for mask in [0x0000_00FF_u32, 0x0000_FF00, 0x00FF_0000] {
            dib.extend_from_slice(&mask.to_le_bytes());
        }
        dib.extend_from_slice(&[1, 2, 3, 0]);
        assert_eq!(dib_to_rgba(&dib).unwrap().2, vec![1, 2, 3, 255]);
    }

    #[test]
    fn a_bitmap_becomes_a_png_of_the_same_size() {
        let rgba = testutil::screenshot_rgba(40, 30);
        let png = dib_to_png(&rgba_to_dibv5(40, 30, &rgba)).unwrap();
        assert_eq!(sniff::sniff(&png), Some(Format::Png));
        assert_eq!(sniff::dimensions(&png, Format::Png), Some((40, 30)));
        assert_eq!(decode_rgba(&png, Format::Png).unwrap().2, rgba, "lossless");
    }

    #[test]
    fn broken_or_unsupported_bitmaps_are_errors() {
        assert!(dib_to_rgba(&[]).is_err());
        let mut short = info_header(100, 100, 24, BI_RGB);
        short.extend_from_slice(&[0; 10]);
        assert!(dib_to_rgba(&short).is_err());
        let mut eight_bit = info_header(1, 1, 8, BI_RGB);
        eight_bit.extend_from_slice(&[0; 8]);
        assert!(dib_to_rgba(&eight_bit).is_err());
        assert!(dib_to_rgba(&info_header(0, 1, 24, BI_RGB)).is_err());
    }

    #[test]
    fn every_format_but_gif_decodes_for_the_bitmap_copy() {
        let png = testutil::png(12, 7);
        assert_eq!(decode_rgba(&png, Format::Png).unwrap().0, 12);
        let jpeg = testutil::jpeg(12, 7, 80, &Default::default());
        let (w, h, rgba) = decode_rgba(&jpeg, Format::Jpeg).unwrap();
        assert_eq!((w, h, rgba.len()), (12, 7, 12 * 7 * 4));
        let webp = testutil::webp_with_transparent_corner(12, 8);
        let (_, _, rgba) = decode_rgba(&webp, Format::WebP).unwrap();
        assert_eq!(rgba[3], 0, "the transparent corner keeps its alpha");
        assert!(decode_rgba(&testutil::gif(1), Format::Gif).is_err());
    }
}
```

Add `pub mod bitmap;` to `crates/ferry-core/src/lib.rs` after `pub mod backup;`.

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p ferry-core bitmap`
Expected: compile error, "cannot find function `rgba_to_dibv5`".

- [ ] **Step 4: Implement**

Add above the test module:

```rust
//! Windows clipboard bitmaps (spec §4.4). A copy that holds only a DIB becomes
//! a lossless PNG that Ferry can optimise, and an optimised image also goes
//! back as a `CF_DIBV5` for apps that paste only bitmaps. Pure byte work, so it
//! is tested on every OS even though only Windows uses it.

use std::io::Cursor;

use crate::sniff::Format;

const BI_RGB: u32 = 0;
const BI_BITFIELDS: u32 = 3;
const V5_HEADER: usize = 124;
/// `LCS_sRGB`, the colour space tag of a V5 header.
const LCS_SRGB: u32 = 0x7352_4742;
/// `LCS_GM_IMAGES`, the rendering intent for photos and screenshots.
const LCS_GM_IMAGES: u32 = 4;

/// Width, height and straight (not premultiplied) RGBA pixels, top row first.
pub type Rgba = (u32, u32, Vec<u8>);

/// A packed DIB (`CF_DIB` or `CF_DIBV5`) as a lossless PNG.
pub fn dib_to_png(dib: &[u8]) -> Result<Vec<u8>, String> {
    let (width, height, rgba) = dib_to_rgba(dib)?;
    encode_png(width, height, &rgba)
}

/// Read a packed DIB: a BITMAPINFOHEADER, V4 or V5 header, then masks or a
/// palette, then 24- or 32-bit rows (bottom-up unless the height is negative).
pub fn dib_to_rgba(dib: &[u8]) -> Result<Rgba, String> {
    let bad = |what: &str| format!("unsupported bitmap: {what}");
    let header = le32(dib, 0).ok_or_else(|| bad("no header"))? as usize;
    let width = le32(dib, 4).ok_or_else(|| bad("no width"))? as i32;
    let height = le32(dib, 8).ok_or_else(|| bad("no height"))? as i32;
    let bits = le16(dib, 14).ok_or_else(|| bad("no bit count"))?;
    let compression = le32(dib, 16).ok_or_else(|| bad("no compression"))?;
    let colours_used = le32(dib, 32).ok_or_else(|| bad("no palette size"))? as usize;
    if header < 40 || width <= 0 || height == 0 {
        return Err(bad("size"));
    }
    if !matches!(bits, 24 | 32) || !matches!(compression, BI_RGB | BI_BITFIELDS) {
        return Err(bad(&format!("{bits}-bit, compression {compression}")));
    }
    let (w, h) = (width as u32, height.unsigned_abs());
    let top_down = height < 0;

    // BI_BITFIELDS puts three masks after a 40-byte header; V4 and V5 headers
    // hold all four masks themselves.
    let mut masks = [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0];
    let mut offset = header;
    if compression == BI_BITFIELDS {
        let count = if header == 40 { 3 } else { 4 };
        for (i, mask) in masks.iter_mut().enumerate().take(count) {
            *mask = le32(dib, 40 + i * 4).ok_or_else(|| bad("masks"))?;
        }
        if header == 40 {
            offset += 12;
        }
    }
    offset += colours_used * 4;

    let stride = (w as usize * bits as usize).div_ceil(32) * 4;
    let end = stride
        .checked_mul(h as usize)
        .and_then(|n| n.checked_add(offset))
        .ok_or_else(|| bad("size"))?;
    let pixels = dib.get(offset..end).ok_or_else(|| bad("cut short"))?;

    let mut rgba = Vec::with_capacity(w as usize * h as usize * 4);
    for y in 0..h as usize {
        let row = if top_down { y } else { h as usize - 1 - y };
        let row = &pixels[row * stride..][..stride];
        for x in 0..w as usize {
            if bits == 24 {
                let p = &row[x * 3..x * 3 + 3];
                rgba.extend_from_slice(&[p[2], p[1], p[0], 255]);
            } else {
                let v = u32::from_le_bytes(row[x * 4..x * 4 + 4].try_into().expect("4 bytes"));
                let alpha = if masks[3] == 0 {
                    255
                } else {
                    channel(v, masks[3])
                };
                rgba.extend_from_slice(&[
                    channel(v, masks[0]),
                    channel(v, masks[1]),
                    channel(v, masks[2]),
                    alpha,
                ]);
            }
        }
    }
    // Most apps leave the fourth byte of a 32-bit bitmap at zero; that is an
    // unused byte, not a fully transparent picture.
    if rgba.as_chunks::<4>().0.iter().all(|p| p[3] == 0) {
        rgba.as_chunks_mut::<4>()
            .0
            .iter_mut()
            .for_each(|p| p[3] = 255);
    }
    Ok((w, h, rgba))
}

/// A `CF_DIBV5` of `rgba`: a V5 header with BI_BITFIELDS masks and the sRGB
/// colour space, then 32-bit BGRA rows, bottom-up.
pub fn rgba_to_dibv5(width: u32, height: u32, rgba: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(V5_HEADER + rgba.len());
    let mut put = |v: u32| out.extend_from_slice(&v.to_le_bytes());
    put(V5_HEADER as u32);
    put(width);
    put(height); // positive: bottom-up
    put(1 | (32 << 16)); // planes, bit count
    put(BI_BITFIELDS);
    put(width * height * 4);
    put(0); // x pixels per metre
    put(0); // y pixels per metre
    put(0); // colours used
    put(0); // colours important
    for mask in [0x00FF_0000, 0x0000_FF00, 0x0000_00FF, 0xFF00_0000] {
        put(mask);
    }
    put(LCS_SRGB);
    for _ in 0..12 {
        put(0); // endpoints (9) and gamma (3), unused for sRGB
    }
    put(LCS_GM_IMAGES);
    put(0); // profile data
    put(0); // profile size
    put(0); // reserved
    for row in rgba.chunks_exact(width as usize * 4).rev() {
        for p in row.as_chunks::<4>().0 {
            out.extend_from_slice(&[p[2], p[1], p[0], p[3]]);
        }
    }
    out
}

/// Decode an optimised image for the bitmap copy. GIF is not decoded (Ferry has
/// no GIF decoder), so a GIF goes on the clipboard in its own format only.
pub fn decode_rgba(bytes: &[u8], format: Format) -> Result<Rgba, String> {
    match format {
        Format::Png => {
            let (w, h, pixels) = crate::optimise::png::decode(bytes).map_err(|e| e.to_string())?;
            Ok((
                w,
                h,
                pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect(),
            ))
        }
        Format::Jpeg => {
            let decoder = mozjpeg::Decompress::new_mem(bytes).map_err(|e| e.to_string())?;
            let (w, h) = decoder.size();
            let rgb = decoder
                .rgb()
                .and_then(|mut d| d.read_scanlines::<[u8; 3]>())
                .map_err(|e| e.to_string())?;
            let rgba = rgb.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect();
            Ok((w as u32, h as u32, rgba))
        }
        Format::WebP => {
            let image = webp::Decoder::new(bytes)
                .decode()
                .ok_or("not a still WebP")?;
            let (w, h) = (image.width(), image.height());
            let rgba = if image.is_alpha() {
                image.to_vec()
            } else {
                image
                    .as_chunks::<3>()
                    .0
                    .iter()
                    .flat_map(|p| [p[0], p[1], p[2], 255])
                    .collect()
            };
            Ok((w, h, rgba))
        }
        Format::Gif => Err("GIF has no bitmap copy".into()),
    }
}

fn encode_png(width: u32, height: u32, rgba: &[u8]) -> Result<Vec<u8>, String> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(Cursor::new(&mut out), width, height);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
    writer.write_image_data(rgba).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())?;
    Ok(out)
}

/// One channel of a packed pixel, scaled to 8 bits.
fn channel(pixel: u32, mask: u32) -> u8 {
    if mask == 0 {
        return 0;
    }
    let max = mask >> mask.trailing_zeros();
    let value = (pixel & mask) >> mask.trailing_zeros();
    (u64::from(value) * 255 / u64::from(max)) as u8
}

fn le32(bytes: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
}

fn le16(bytes: &[u8], at: usize) -> Option<u16> {
    Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `cargo test -p ferry-core bitmap && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: PASS, 7 tests, and no warnings.

- [ ] **Step 6: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): DIB to PNG and RGBA to DIBV5 for the Windows clipboard"
```

---

### Task 6: Tray state glyphs

**Files:**
- Create: `crates/ferry-core/src/glyph.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Produces, in `ferry_core::glyph`:
  - `TrayGlyph { Running, Paused, Stopped, Working, Error }`;
  - `render(base: &[u8], size: u32, glyph: TrayGlyph) -> Vec<u8>`. It draws into the logo's dot, which sits at x 32..48, y 44..60 on the 64-unit grid of `src-tauri/icons/tray.svg`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/glyph.rs` with this test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A stand-in for the logo: transparent, with the dot filled.
    fn logo(size: u32) -> Vec<u8> {
        let mut px = vec![0; (size * size * 4) as usize];
        let s = size as f32 / 64.0;
        for y in (44.0 * s) as u32..(60.0 * s) as u32 {
            for x in (32.0 * s) as u32..(48.0 * s) as u32 {
                px[((y * size + x) * 4 + 3) as usize] = 255;
            }
        }
        px
    }

    fn alpha(px: &[u8], size: u32, x: u32, y: u32) -> u8 {
        let s = size as f32 / 64.0;
        let (x, y) = ((x as f32 * s) as u32, (y as f32 * s) as u32);
        px[((y * size + x) * 4 + 3) as usize]
    }

    #[test]
    fn running_is_the_logo_unchanged() {
        assert_eq!(render(&logo(64), 64, TrayGlyph::Running), logo(64));
    }

    #[test]
    fn paused_turns_the_dot_into_two_bars() {
        let px = render(&logo(64), 64, TrayGlyph::Paused);
        assert_eq!(alpha(&px, 64, 34, 52), 255);
        assert_eq!(alpha(&px, 64, 40, 52), 0, "gap between the bars");
        assert_eq!(alpha(&px, 64, 46, 52), 255);
    }

    #[test]
    fn working_hollows_the_dot() {
        let px = render(&logo(64), 64, TrayGlyph::Working);
        assert_eq!(alpha(&px, 64, 33, 45), 255);
        assert_eq!(alpha(&px, 64, 40, 52), 0);
    }

    #[test]
    fn error_draws_an_exclamation_mark() {
        let px = render(&logo(64), 64, TrayGlyph::Error);
        assert_eq!(alpha(&px, 64, 40, 46), 255, "the stroke");
        assert_eq!(alpha(&px, 64, 40, 55), 0, "the gap");
        assert_eq!(alpha(&px, 64, 40, 58), 255, "the point");
        assert_eq!(alpha(&px, 64, 33, 46), 0, "narrower than the dot");
    }

    #[test]
    fn stopped_dims_everything() {
        let px = render(&logo(64), 64, TrayGlyph::Stopped);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] <= 102));
        assert_eq!(alpha(&px, 64, 40, 52), 102);
    }

    #[test]
    fn glyphs_scale_with_the_icon() {
        let px = render(&logo(32), 32, TrayGlyph::Paused);
        assert_eq!(alpha(&px, 32, 34, 52), 255);
        assert_eq!(alpha(&px, 32, 40, 52), 0);
    }
}
```

Add `pub mod glyph;` to `crates/ferry-core/src/lib.rs` after `pub mod geometry;`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core glyph`
Expected: compile error, "cannot find function `render`".

- [ ] **Step 3: Implement**

Add above the test module:

```rust
//! Tray icon states (spec §5.6), drawn into the dot of the "F." logo so the
//! icon stays a one-colour template image that macOS tints for light and dark
//! menu bars. Coordinates are in the 64-unit grid of `src-tauri/icons/tray.svg`.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayGlyph {
    /// The logo as drawn.
    Running,
    /// The dot becomes a pause sign.
    Paused,
    /// The whole glyph is dimmed, as macOS draws a disabled item.
    Stopped,
    /// The dot becomes a hollow square while any job runs.
    Working,
    /// The dot becomes an exclamation mark until the menu is opened.
    Error,
}

/// The logo's dot: x 32..48, y 44..60.
const DOT: (f32, f32, f32, f32) = (32.0, 44.0, 48.0, 60.0);
const STOPPED_ALPHA: f32 = 0.4;

/// Draw `glyph` over `base`, a square RGBA image `size` pixels wide.
pub fn render(base: &[u8], size: u32, glyph: TrayGlyph) -> Vec<u8> {
    let mut out = base.to_vec();
    let scale = size as f32 / 64.0;
    let mut fill = |x0: f32, y0: f32, x1: f32, y1: f32, alpha: u8| {
        let (x0, y0) = ((x0 * scale).round() as u32, (y0 * scale).round() as u32);
        let (x1, y1) = ((x1 * scale).round() as u32, (y1 * scale).round() as u32);
        for y in y0..y1.min(size) {
            for x in x0..x1.min(size) {
                let i = ((y * size + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&[0, 0, 0, alpha]);
            }
        }
    };
    let (x0, y0, x1, y1) = DOT;
    match glyph {
        TrayGlyph::Running => {}
        TrayGlyph::Paused => {
            fill(x0, y0, x1, y1, 0);
            fill(x0, y0, x0 + 6.0, y1, 255);
            fill(x1 - 6.0, y0, x1, y1, 255);
        }
        TrayGlyph::Working => {
            fill(x0, y0, x1, y1, 255);
            fill(x0 + 4.0, y0 + 4.0, x1 - 4.0, y1 - 4.0, 0);
        }
        TrayGlyph::Error => {
            fill(x0, y0, x1, y1, 0);
            fill(x0 + 5.0, y0, x1 - 5.0, y1 - 6.0, 255);
            fill(x0 + 5.0, y1 - 4.0, x1 - 5.0, y1, 255);
        }
        TrayGlyph::Stopped => {
            for pixel in out.as_chunks_mut::<4>().0 {
                pixel[3] = (f32::from(pixel[3]) * STOPPED_ALPHA).round() as u8;
            }
        }
    }
    out
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core glyph && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: PASS, 6 tests, and no warnings. Clippy asks for `as_chunks`, not `chunks_exact`; the code already uses it.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): tray glyphs for run state, work and errors"
```

---

### Task 7: App dependencies, logs and shared state

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/default.json`, `src-tauri/src/lib.rs`, `Cargo.lock`
- Create: `src-tauri/src/logging.rs`, `src-tauri/src/state.rs`

**Interfaces:**
- Consumes: `pipeline::{Job, notice}` (Task 4), `run_state::RunState` (Task 3), `backup::Backups`, `settings::Settings`, `job::*`.
- Produces from `logging`:
  - `init(dir: &Path) -> Option<WorkerGuard>`; keep the guard managed for the app's lifetime;
  - `latest(dir: &Path) -> Option<PathBuf>`.
- Produces from `state`:
  - `RECENT = 10`.
  - `AppState { settings: RwLock<Settings>, settings_path, run_state: Mutex<RunState>, backups: Backups, backups_dir, work_dir, log_dir: PathBuf, gifsicle: Option<PathBuf>, jobs: Mutex<HashMap<JobId, Arc<Entry>>>, recent: Mutex<VecDeque<JobUpdate>>, permits: Arc<Semaphore>, clipboard: Mutex<ClipboardLog>, stack: Mutex<StackInbox>, tray_error: AtomicBool }`, with these methods:
    - `AppState::new(settings, settings_path, backups_dir, work_dir, log_dir, gifsicle)`;
    - `.is_working()`;
    - `.remember(&JobUpdate)`.
  - `Entry { job: Mutex<Job>, output: PathBuf, source_seq: Mutex<Option<u64>>, cancel: Mutex<CancelToken>, last: Mutex<JobUpdate> }` with `.is_busy()`.
  - `ClipboardLog { pub accumulated: Vec<PathBuf>, .. }` with:
    - `.record(seq, from: Option<u64>)`;
    - `.is_own(seq)`;
    - `.may_replace(source, live) -> bool`.
  - `StackInbox { ready: bool, pending: Vec<JobUpdate> }`.

- [ ] **Step 1: Add the dependencies**

Apply this change to `src-tauri/Cargo.toml`:

```diff
diff --git a/src-tauri/Cargo.toml b/src-tauri/Cargo.toml
index 71d573a..d39df96 100644
--- a/src-tauri/Cargo.toml
+++ b/src-tauri/Cargo.toml
@@ -12,13 +12,29 @@ name = "ferry_lib"
 tauri-build = { version = "2", features = [] }
 
 [dependencies]
+chrono = { version = "0.4", default-features = false, features = ["clock"] }
 ferry-core = { path = "../crates/ferry-core" }
+serde = { version = "1", features = ["derive"] }
 serde_json = "1"
-tauri = { version = "2.12", features = ["tray-icon", "image-png", "macos-private-api"] }
+tauri = { version = "2.12", features = ["tray-icon", "image-png", "macos-private-api", "protocol-asset"] }
+tauri-plugin-drag = "2.1.1"
+tauri-plugin-global-shortcut = "2.4.1"
+tauri-plugin-opener = "2.7.2"
 tauri-plugin-single-instance = "2.5"
+tokio = { version = "1", features = ["sync"] }
+tracing = "0.1"
+tracing-appender = "0.2.5"
+tracing-subscriber = { version = "0.3.23", default-features = false, features = ["fmt", "std"] }
 
 [target.'cfg(target_os = "macos")'.dependencies]
+objc2 = "0.6.5"
+objc2-app-kit = { version = "0.3.2", default-features = false, features = ["std", "NSPasteboard", "NSPasteboardItem", "NSWorkspace", "NSRunningApplication"] }
+objc2-foundation = { version = "0.3.2", default-features = false, features = ["std", "NSArray", "NSData", "NSString", "NSURL"] }
 tauri-nspanel = "2.1"
 
 [target.'cfg(windows)'.dependencies]
-windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_UI_WindowsAndMessaging"] }
+clipboard-win = { version = "5.4.1", features = ["monitor", "std"] }
+windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_System_Threading", "Win32_UI_WindowsAndMessaging"] }
+
+[dev-dependencies]
+tempfile = "3"
```

Then run `cargo fetch` (this updates `Cargo.lock`). `cargo tree -p ferry -i objc2` must list a single `objc2` version (0.6.5); a second version would mean two Objective-C runtimes' worth of class definitions.

- [ ] **Step 2: Serve backups to the stack as thumbnails**

In `src-tauri/tauri.conf.json`, replace `"devCsp": null` with:

```json
      "devCsp": null,
      "assetProtocol": { "enable": true, "scope": ["$APPDATA/backups/**"] }
```

Enabling the `protocol-asset` feature without this block breaks Tauri's build script, so it lands in the same step. The CSP already allows `asset:` and `http://asset.localhost` images (M1).

In `src-tauri/capabilities/default.json`, change `"permissions": ["core:default"]` to `"permissions": ["core:default", "drag:default"]`.

- [ ] **Step 3: Write the failing tests**

Create `src-tauri/src/logging.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_latest_log_is_the_newest_dated_file() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["ferry.2026-10-20.log", "ferry.2026-10-22.log", "other.log"] {
            fs::write(dir.path().join(name), b"").unwrap();
        }
        assert_eq!(
            latest(dir.path()),
            Some(dir.path().join("ferry.2026-10-22.log"))
        );
        assert_eq!(latest(&dir.path().join("missing")), None);
    }
}
```

Create `src-tauri/src/state.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn our_own_clipboard_writes_are_recognised() {
        let mut log = ClipboardLog::default();
        log.record(41, None);
        assert!(log.is_own(41));
        assert!(!log.is_own(42));
    }

    #[test]
    fn only_recent_writes_are_remembered() {
        let mut log = ClipboardLog::default();
        for seq in 0..20 {
            log.record(seq, None);
        }
        assert!(!log.is_own(0));
        assert!(log.is_own(19));
    }

    #[test]
    fn a_result_replaces_the_copy_it_came_from() {
        let log = ClipboardLog::default();
        assert!(log.may_replace(7, 7));
    }

    #[test]
    fn a_result_never_replaces_what_the_user_copied_while_it_ran() {
        let log = ClipboardLog::default();
        assert!(!log.may_replace(7, 8));
    }

    #[test]
    fn an_older_result_never_replaces_a_newer_one() {
        // Two quick screenshots (seq 7, then 8); the second finishes first.
        let mut log = ClipboardLog::default();
        assert!(log.may_replace(8, 8));
        log.record(9, Some(8));
        assert!(!log.may_replace(7, 9));
    }

    #[test]
    fn a_hotkey_on_ferrys_own_result_may_replace_it() {
        // A screenshot (seq 2) became Ferry's result (seq 3); ⌃⇧A then
        // optimises that result harder.
        let mut log = ClipboardLog::default();
        log.record(3, Some(2));
        assert!(log.may_replace(3, 3));
    }

    #[test]
    fn a_result_never_replaces_an_explicit_copy() {
        // The user pressed Copy on an older card while this job ran.
        let mut log = ClipboardLog::default();
        log.record(9, None);
        assert!(!log.may_replace(7, 9));
    }

    #[test]
    fn recent_results_keep_ten_newest_first_without_repeats() {
        let dir = std::env::temp_dir();
        let state = AppState::new(
            Settings::default(),
            dir.join("s.json"),
            dir.join("b"),
            dir.join("w"),
            dir.join("l"),
            None,
        );
        let updates: Vec<JobUpdate> = (0..12)
            .map(|i| ferry_core::pipeline::notice(format!("n{i}")))
            .collect();
        for u in &updates {
            state.remember(u);
        }
        state.remember(&updates[11]);
        let recent = state.recent.lock().unwrap();
        assert_eq!(recent.len(), RECENT);
        assert_eq!(recent[0].id, updates[11].id);
        assert_eq!(recent[1].id, updates[10].id);
    }
}
```

Add `mod logging;` and `mod state;` to `src-tauri/src/lib.rs`, keeping the `mod` list sorted.

- [ ] **Step 4: Run them to see them fail**

Run: `cargo test -p ferry`
Expected: compile errors, "cannot find function `latest`" and "cannot find type `ClipboardLog`".

- [ ] **Step 5: Implement the logs**

Add above the test module in `logging.rs`:

```rust
//! Structured logs (spec §4.3, §6): `ferry.<date>.log` in the app's log folder,
//! a new file each day and the last 7 kept. No file contents are logged.

use std::fs;
use std::path::{Path, PathBuf};

use tracing_appender::non_blocking::WorkerGuard;
use tracing_appender::rolling::{RollingFileAppender, Rotation};

/// Start logging to `dir`. Keep the guard alive for the app's lifetime: dropping
/// it flushes and stops the writer thread. `None` if the folder is unusable.
pub fn init(dir: &Path) -> Option<WorkerGuard> {
    let appender = RollingFileAppender::builder()
        .rotation(Rotation::DAILY)
        .filename_prefix("ferry")
        .filename_suffix("log")
        .max_log_files(7)
        .build(dir)
        .map_err(|e| eprintln!("ferry: logging is off: {e}"))
        .ok()?;
    let (writer, guard) = tracing_appender::non_blocking(appender);
    tracing_subscriber::fmt()
        .with_writer(writer)
        .with_ansi(false)
        .with_max_level(tracing::Level::INFO)
        .try_init()
        .ok()?;
    Some(guard)
}

/// The newest log file, for an Error card's "Details".
pub fn latest(dir: &Path) -> Option<PathBuf> {
    fs::read_dir(dir)
        .ok()?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .is_some_and(|n| n.starts_with("ferry.") && n.ends_with(".log"))
        })
        .max()
}
```

- [ ] **Step 6: Implement the shared state**

Add above the test module in `state.rs`:

```rust
//! Everything the app shares between the tray, hotkeys, clipboard watcher,
//! job workers and commands.

use std::collections::{HashMap, VecDeque};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use ferry_core::backup::Backups;
use ferry_core::cancel::CancelToken;
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::pipeline::Job;
use ferry_core::run_state::RunState;
use ferry_core::settings::Settings;
use tokio::sync::Semaphore;

/// How many finished jobs the tray's "Recent results" lists.
pub const RECENT: usize = 10;

pub struct AppState {
    pub settings: RwLock<Settings>,
    pub settings_path: PathBuf,
    pub run_state: Mutex<RunState>,
    pub backups: Backups,
    pub backups_dir: PathBuf,
    pub work_dir: PathBuf,
    pub log_dir: PathBuf,
    pub gifsicle: Option<PathBuf>,
    pub jobs: Mutex<HashMap<JobId, Arc<Entry>>>,
    /// Newest first.
    pub recent: Mutex<VecDeque<JobUpdate>>,
    /// One permit per CPU, so a burst of copies cannot starve the machine.
    pub permits: Arc<Semaphore>,
    /// Held across every clipboard write, so the watcher never mistakes our own
    /// write for the user's.
    pub clipboard: Mutex<ClipboardLog>,
    /// Cards sent before the stack page was listening; it collects them on load.
    pub stack: Mutex<StackInbox>,
    /// The tray shows an error mark until its menu is opened.
    pub tray_error: AtomicBool,
}

impl AppState {
    pub fn new(
        settings: Settings,
        settings_path: PathBuf,
        backups_dir: PathBuf,
        work_dir: PathBuf,
        log_dir: PathBuf,
        gifsicle: Option<PathBuf>,
    ) -> Self {
        let workers = std::thread::available_parallelism().map_or(2, |n| n.get());
        Self {
            run_state: Mutex::new(RunState::from_start(settings.general.start_state)),
            settings: RwLock::new(settings),
            settings_path,
            backups: Backups::new(&backups_dir),
            backups_dir,
            work_dir,
            log_dir,
            gifsicle,
            jobs: Mutex::new(HashMap::new()),
            recent: Mutex::new(VecDeque::new()),
            permits: Arc::new(Semaphore::new(workers)),
            clipboard: Mutex::new(ClipboardLog::default()),
            stack: Mutex::new(StackInbox::default()),
            tray_error: AtomicBool::new(false),
        }
    }

    /// Whether any job is queued or optimising (the tray's Working glyph).
    pub fn is_working(&self) -> bool {
        self.jobs.lock().unwrap().values().any(|e| e.is_busy())
    }

    /// Keep a finished job for the tray's "Recent results", newest first.
    pub fn remember(&self, update: &JobUpdate) {
        let mut recent = self.recent.lock().unwrap();
        recent.retain(|u| u.id != update.id);
        recent.push_front(update.clone());
        recent.truncate(RECENT);
    }
}

/// A job, its cancel switch and the card it last showed. `job` stays locked
/// while the job runs, so `output` is kept outside it.
pub struct Entry {
    pub job: Mutex<Job>,
    pub output: PathBuf,
    /// The clipboard copy a clipboard job came from, until its first run.
    pub source_seq: Mutex<Option<u64>>,
    pub cancel: Mutex<CancelToken>,
    pub last: Mutex<JobUpdate>,
}

impl Entry {
    pub fn is_busy(&self) -> bool {
        matches!(
            self.last.lock().unwrap().state,
            JobState::Queued | JobState::Optimising { .. }
        )
    }
}

/// Clipboard changes Ferry made itself, and the files "accumulate results" has
/// collected since launch or the last "Clear all".
#[derive(Default)]
pub struct ClipboardLog {
    own: VecDeque<u64>,
    /// The copy whose result Ferry last put on the clipboard; `None` after an
    /// explicit write such as a card's Copy.
    shown: Option<u64>,
    pub accumulated: Vec<PathBuf>,
}

impl ClipboardLog {
    const KEEP: usize = 16;

    /// Remember a write Ferry made, and which copy (`from`) it was the result of.
    pub fn record(&mut self, seq: u64, from: Option<u64>) {
        self.own.push_back(seq);
        if self.own.len() > Self::KEEP {
            self.own.pop_front();
        }
        self.shown = from;
    }

    pub fn is_own(&self, seq: u64) -> bool {
        self.own.contains(&seq)
    }

    /// Whether the result of the copy `source` may go on the clipboard, which
    /// is now at `live`: only while it still holds that copy or Ferry's result
    /// of it. Anything copied since wins.
    pub fn may_replace(&self, source: u64, live: u64) -> bool {
        live == source || (self.is_own(live) && self.shown == Some(source))
    }
}

/// Cards held until the stack page says it is listening.
#[derive(Default)]
pub struct StackInbox {
    pub ready: bool,
    pub pending: Vec<JobUpdate>,
}
```

- [ ] **Step 7: Run them to see them pass**

Run: `cargo test -p ferry`
Expected: PASS, 9 tests (1 logging, 8 state). The app crate had none of its own after Task 1 moved the i18n tests to core. Dead-code warnings for `logging` and `state` are expected until Task 10.

- [ ] **Step 8: Commit**

```bash
git add Cargo.lock src-tauri
git commit -m "feat(app): dependencies, daily logs and shared app state"
```

---

### Task 8: macOS clipboard

**Files:**
- Create: `src-tauri/src/platform/macos_clipboard.rs`
- Modify: `src-tauri/src/platform/mod.rs`

**Interfaces:**
- Consumes: `trigger::Snapshot` (Task 3), `sniff::{sniff, Format}`.
- Produces, in `platform::clipboard` (the Windows file in Task 9 has the same functions):
  - `read() -> Result<Snapshot, String>`;
  - `seq() -> u64`;
  - `write_image(Format, &[u8], file: Option<&Path>) -> Result<u64, String>`, which returns the new change count;
  - `write_files(&[PathBuf]) -> Result<u64, String>`;
  - `watch(tx: Sender<u64>)`, which polls every 250 ms on its own thread.

- [ ] **Step 1: Expose the module**

Replace `src-tauri/src/platform/mod.rs` with:

```rust
//! Per-OS native glue. Each OS module exposes the same functions, and each
//! `clipboard` module the same `read`, `write_image`, `write_files` and `watch`.

#[cfg(target_os = "macos")]
#[path = "macos_clipboard.rs"]
pub mod clipboard;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
pub use macos::{hide, make_non_activating, show_without_focus};

#[cfg(windows)]
#[path = "windows_clipboard.rs"]
pub mod clipboard;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{hide, make_non_activating, show_without_focus};
```

Create `src-tauri/src/platform/windows_clipboard.rs` with only a doc comment for now:

```rust
//! Windows clipboard; filled in by Task 9.
```

Otherwise `cargo fmt` fails on the missing `#[path]` target.

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/platform/macos_clipboard.rs` with this test module. The tests use a private, uniquely named pasteboard and never touch the user's clipboard:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    /// A private pasteboard, so tests never touch the user's clipboard.
    struct Private(Retained<NSPasteboard>);
    impl Private {
        fn new() -> Self {
            Self(NSPasteboard::pasteboardWithUniqueName())
        }
    }
    impl Drop for Private {
        fn drop(&mut self) {
            // Not in objc2-app-kit's bindings; frees the named pasteboard server-side.
            let _: () = unsafe { objc2::msg_send![&*self.0, releaseGlobally] };
        }
    }

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn an_image_round_trips_and_the_change_count_moves() {
        let pb = Private::new();
        let before = pb.0.changeCount() as u64;
        let seq = write_image_to(&pb.0, Format::Png, PNG, None).unwrap();
        assert!(seq > before);
        assert_eq!(pb.0.changeCount() as u64, seq);
        let snap = read_from(&pb.0);
        assert_eq!(snap.seq, seq);
        assert_eq!(snap.image, Some((Format::Png, PNG.to_vec())));
        assert!(!snap.concealed);
        assert!(snap.files.is_empty());
    }

    #[test]
    fn a_label_that_lies_about_the_content_is_ignored() {
        let pb = Private::new();
        pb.0.clearContents();
        pb.0.setData_forType(
            Some(&NSData::with_bytes(b"not a png")),
            &NSString::from_str("public.png"),
        );
        assert_eq!(read_from(&pb.0).image, None);
    }

    #[test]
    fn concealed_content_is_flagged_and_not_read() {
        let pb = Private::new();
        write_image_to(&pb.0, Format::Png, PNG, None).unwrap();
        pb.0.setData_forType(
            Some(&NSData::with_bytes(b"")),
            &NSString::from_str("org.nspasteboard.ConcealedType"),
        );
        let snap = read_from(&pb.0);
        assert!(snap.concealed);
        assert_eq!(snap.image, None);
    }

    #[test]
    fn files_round_trip() {
        let pb = Private::new();
        let dir = std::env::temp_dir();
        let paths = vec![dir.join("ferry a.png"), dir.join("b.jpg")];
        write_files_to(&pb.0, &paths).unwrap();
        let snap = read_from(&pb.0);
        assert_eq!(snap.files, paths);
        assert_eq!(snap.image, None);
    }

    #[test]
    fn an_image_can_carry_its_working_file() {
        let pb = Private::new();
        let file = std::env::temp_dir().join("ferry work.png");
        write_image_to(&pb.0, Format::Png, PNG, Some(&file)).unwrap();
        let snap = read_from(&pb.0);
        assert_eq!(snap.image, Some((Format::Png, PNG.to_vec())));
        assert_eq!(snap.files, vec![file]);
    }
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p ferry platform`
Expected: compile error, "cannot find function `write_image_to`".

- [ ] **Step 4: Implement**

Add above the test module:

```rust
//! macOS clipboard: NSPasteboard read, write and change polling (spec §4.4).
//! NSPasteboard and NSWorkspace are thread-safe, so this runs off the main thread.

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::Duration;

use ferry_core::sniff::{Format, sniff};
use ferry_core::trigger::Snapshot;
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2_app_kit::{NSPasteboard, NSPasteboardWriting, NSWorkspace};
use objc2_foundation::{NSArray, NSData, NSString, NSURL};

/// Read order: the first type present wins.
const IMAGE_TYPES: [(&str, Format); 4] = [
    ("public.png", Format::Png),
    ("public.jpeg", Format::Jpeg),
    ("com.compuserve.gif", Format::Gif),
    ("org.webmproject.webp", Format::WebP),
];

/// Markers password managers and clipboard tools put on secret or throwaway content
/// (nspasteboard.org).
const CONCEALED_TYPES: [&str; 4] = [
    "org.nspasteboard.ConcealedType",
    "org.nspasteboard.TransientType",
    "org.nspasteboard.AutoGeneratedType",
    "com.agilebits.onepassword",
];

const FILE_URL: &str = "public.file-url";
const POLL: Duration = Duration::from_millis(250);

fn uti(format: Format) -> &'static str {
    IMAGE_TYPES
        .iter()
        .find(|(_, f)| *f == format)
        .map(|(t, _)| *t)
        .expect("every format has a pasteboard type")
}

pub fn read() -> Result<Snapshot, String> {
    Ok(read_from(&NSPasteboard::generalPasteboard()))
}

/// The change count now, to tell whether the user has copied since a snapshot.
pub fn seq() -> u64 {
    NSPasteboard::generalPasteboard().changeCount() as u64
}

fn read_from(pb: &NSPasteboard) -> Snapshot {
    // Read first: a copy made while reading then looks newer, never older.
    let seq = pb.changeCount() as u64;
    let types: Vec<String> = pb
        .types()
        .map(|types| types.iter().map(|t| t.to_string()).collect())
        .unwrap_or_default();
    let source_app = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|app| app.bundleIdentifier())
        .map(|id| id.to_string());
    let concealed = types.iter().any(|t| CONCEALED_TYPES.contains(&t.as_str()));
    if concealed {
        // Never read the data of a secret.
        return Snapshot {
            seq,
            concealed,
            source_app,
            ..Snapshot::default()
        };
    }
    let image = IMAGE_TYPES.iter().find_map(|(t, format)| {
        if !types.iter().any(|have| have == t) {
            return None;
        }
        let bytes = pb.dataForType(&NSString::from_str(t))?.to_vec();
        // Trust the content, not the label (R§2.4).
        (sniff(&bytes) == Some(*format)).then_some((*format, bytes))
    });
    let files = pb
        .pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.stringForType(&NSString::from_str(FILE_URL)))
                .filter_map(|url| NSURL::URLWithString(&url)?.to_file_path())
                .collect()
        })
        .unwrap_or_default();
    Snapshot {
        seq,
        image,
        from_bitmap: false,
        files,
        concealed,
        source_app,
    }
}

/// Put an image on the clipboard, plus the working file's URL when `file` is
/// given. Returns the new change count, so the watcher can skip our own write.
pub fn write_image(format: Format, bytes: &[u8], file: Option<&Path>) -> Result<u64, String> {
    write_image_to(&NSPasteboard::generalPasteboard(), format, bytes, file)
}

fn write_image_to(
    pb: &NSPasteboard,
    format: Format,
    bytes: &[u8],
    file: Option<&Path>,
) -> Result<u64, String> {
    let seq = pb.clearContents();
    if !pb.setData_forType(
        Some(&NSData::with_bytes(bytes)),
        &NSString::from_str(uti(format)),
    ) {
        return Err("the clipboard refused the image".into());
    }
    if let Some(file) = file {
        let url = NSURL::from_file_path(file).ok_or("not an absolute path")?;
        if let Some(text) = url.absoluteString() {
            pb.setString_forType(&text, &NSString::from_str(FILE_URL));
        }
    }
    Ok(seq as u64)
}

/// Put files on the clipboard, as Finder's Copy does.
pub fn write_files(paths: &[PathBuf]) -> Result<u64, String> {
    write_files_to(&NSPasteboard::generalPasteboard(), paths)
}

fn write_files_to(pb: &NSPasteboard, paths: &[PathBuf]) -> Result<u64, String> {
    let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
        .iter()
        .map(|p| NSURL::from_file_path(p).map(ProtocolObject::from_retained))
        .collect::<Option<_>>()
        .ok_or("not an absolute path")?;
    let seq = pb.clearContents();
    if !pb.writeObjects(&NSArray::from_retained_slice(&urls)) {
        return Err("the clipboard refused the files".into());
    }
    Ok(seq as u64)
}

/// Send the change count on `tx` every time it changes. Stops when the receiver is gone.
pub fn watch(tx: Sender<u64>) {
    thread::Builder::new()
        .name("ferry-clipboard".into())
        .spawn(move || {
            let pb = NSPasteboard::generalPasteboard();
            let mut last = pb.changeCount();
            loop {
                thread::sleep(POLL);
                let now = pb.changeCount();
                if now != last {
                    last = now;
                    if tx.send(now as u64).is_err() {
                        return;
                    }
                }
            }
        })
        .expect("spawn the clipboard thread");
}
```

`releaseGlobally` is missing from objc2-app-kit's bindings, so the test helper calls it with `msg_send!`. `NSPasteboard` and `NSWorkspace` are not main-thread-only, so polling on a background thread is allowed.

- [ ] **Step 5: Run them to see them pass**

Run: `cargo test -p ferry platform`
Expected: PASS, 5 tests.

- [ ] **Step 6: Commit**

```bash
git add src-tauri/src/platform
git commit -m "feat(app): macOS pasteboard read, write and change polling"
```

---

### Task 9: Windows clipboard

**Files:**
- Modify: `src-tauri/src/platform/windows_clipboard.rs` (the placeholder from Task 8)

**Interfaces:**
- Consumes: `trigger::Snapshot`, `bitmap::{dib_to_png, decode_rgba, rgba_to_dibv5}` (Task 5), `sniff`.
- Produces: the same five functions as Task 8.
  - The read order is the registered `PNG`, `JFIF`, `image/jpeg`, `GIF` and `image/webp` formats, then `CF_DIBV5`/`CF_DIB` turned into a PNG with `from_bitmap = true`. Copied files come from `CF_HDROP`.
  - Content is concealed when the clipboard holds `ExcludeClipboardContentFromMonitorProcessing` or `Clipboard Viewer Ignore`, or `CanIncludeInClipboardHistory` = 0.
  - The source app is the clipboard owner's executable name.

**This file has not been compiled.** This Mac has no Windows target, and Windows has no private clipboard to test on. CI's Windows bundle job compiles it, lints it and runs its tests when Task 10 pushes. Until then, treat it as a draft: if CI fails here, fix it in this file and record a ruling in the ledger.

- [ ] **Step 1: Write the tests**

Replace `src-tauri/src/platform/windows_clipboard.rs` with the test module below. It uses the real clipboard, so the tests take a lock and run one at a time:

```rust
#[cfg(test)]
mod tests {
    //! These use the real clipboard (Windows has no private one), so they run
    //! one at a time and only in CI's Windows bundle job.
    use super::*;
    use std::sync::Mutex;

    static CLIPBOARD: Mutex<()> = Mutex::new(());

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn an_image_round_trips_and_the_sequence_moves() {
        let _one = CLIPBOARD.lock().unwrap();
        let before = seq();
        let written = write_image(Format::Png, PNG, None).unwrap();
        assert!(written > before);
        let snap = read().unwrap();
        assert_eq!(snap.seq, written);
        assert_eq!(snap.image, Some((Format::Png, PNG.to_vec())));
        assert!(!snap.from_bitmap && !snap.concealed);
    }

    #[test]
    fn files_round_trip() {
        let _one = CLIPBOARD.lock().unwrap();
        let dir = std::env::temp_dir();
        let paths = vec![dir.join("ferry a.png"), dir.join("b.jpg")];
        write_files(&paths).unwrap();
        assert_eq!(read().unwrap().files, paths);
    }

    #[test]
    fn a_bitmap_only_copy_becomes_a_png_from_bitmap() {
        let _one = CLIPBOARD.lock().unwrap();
        let rgba = vec![200_u8; 4 * 4 * 4];
        let dib = bitmap::rgba_to_dibv5(4, 4, &rgba);
        {
            let _open = Clipboard::new_attempts(ATTEMPTS).unwrap();
            raw::empty().unwrap();
            raw::set_without_clear(formats::CF_DIBV5, &dib).unwrap();
        }
        let snap = read().unwrap();
        assert!(snap.from_bitmap);
        assert_eq!(snap.image.map(|(f, _)| f), Some(Format::Png));
    }

    #[test]
    fn content_marked_for_exclusion_is_concealed() {
        let _one = CLIPBOARD.lock().unwrap();
        {
            let _open = Clipboard::new_attempts(ATTEMPTS).unwrap();
            raw::empty().unwrap();
            raw::set_without_clear(registered("PNG").unwrap(), PNG).unwrap();
            raw::set_without_clear(registered(EXCLUDE).unwrap(), &[0]).unwrap();
        }
        let snap = read().unwrap();
        assert!(snap.concealed);
        assert_eq!(snap.image, None);
    }
}
```

- [ ] **Step 2: Implement**

Add above the test module:

```rust
//! Windows clipboard (spec §4.4): registered image formats first, then a
//! bitmap kept as a lossless PNG; copied files from `CF_HDROP`. Changes come
//! from a clipboard listener on its own thread. Only CI has run this so far;
//! no real Windows desktop has (spec §1).

use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use std::thread;

use clipboard_win::{Clipboard, Monitor, formats, raw};
use ferry_core::bitmap;
use ferry_core::sniff::{Format, sniff};
use ferry_core::trigger::Snapshot;
use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// Another app may hold the clipboard open for a moment.
const ATTEMPTS: usize = 10;

/// Registered formats in read order; Windows apps name JPEG "JFIF".
const IMAGE_FORMATS: [(&str, Format); 5] = [
    ("PNG", Format::Png),
    ("JFIF", Format::Jpeg),
    ("image/jpeg", Format::Jpeg),
    ("GIF", Format::Gif),
    ("image/webp", Format::WebP),
];

/// Formats password managers and clipboard tools set on secret content.
const EXCLUDE: &str = "ExcludeClipboardContentFromMonitorProcessing";
const HISTORY: &str = "CanIncludeInClipboardHistory";
const VIEWER_IGNORE: &str = "Clipboard Viewer Ignore";

fn registered(name: &str) -> Option<u32> {
    raw::register_format(name).map(|id| id.get())
}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Png => "PNG",
        Format::Jpeg => "JFIF",
        Format::Gif => "GIF",
        Format::WebP => "image/webp",
    }
}

pub fn read() -> Result<Snapshot, String> {
    let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
    let seq = seq();
    let source_app = owner_exe();
    if is_concealed() {
        return Ok(Snapshot {
            seq,
            concealed: true,
            source_app,
            ..Snapshot::default()
        });
    }
    let mut snap = Snapshot {
        seq,
        source_app,
        ..Snapshot::default()
    };
    if raw::is_format_avail(formats::CF_HDROP) {
        let _ = raw::get_file_list_path(&mut snap.files);
    }
    for (name, format) in IMAGE_FORMATS {
        let Some(id) = registered(name).filter(|&id| raw::is_format_avail(id)) else {
            continue;
        };
        let mut bytes = Vec::new();
        // Trust the content, not the format name (R§2.4).
        if raw::get_vec(id, &mut bytes).is_ok() && sniff(&bytes) == Some(format) {
            snap.image = Some((format, bytes));
            return Ok(snap);
        }
    }
    for id in [formats::CF_DIBV5, formats::CF_DIB] {
        let mut dib = Vec::new();
        if raw::is_format_avail(id)
            && raw::get_vec(id, &mut dib).is_ok()
            && let Ok(png) = bitmap::dib_to_png(&dib)
        {
            snap.image = Some((Format::Png, png));
            snap.from_bitmap = true;
            break;
        }
    }
    Ok(snap)
}

fn is_concealed() -> bool {
    let present = |name| registered(name).is_some_and(raw::is_format_avail);
    if present(EXCLUDE) || present(VIEWER_IGNORE) {
        return true;
    }
    // A DWORD of 0 asks clipboard history (and so Ferry) to leave the content alone.
    let mut value = Vec::new();
    registered(HISTORY).is_some_and(|id| {
        raw::is_format_avail(id)
            && raw::get_vec(id, &mut value).is_ok()
            && value.get(..4) == Some(&[0, 0, 0, 0])
    })
}

/// The executable name of the app that owns the clipboard, such as `KeePass.exe`.
fn owner_exe() -> Option<String> {
    let owner = raw::get_owner()?;
    let mut pid = 0_u32;
    // SAFETY: `owner` is a window handle from GetClipboardOwner; a stale handle
    // makes the call return 0, which is checked.
    unsafe { GetWindowThreadProcessId(owner.as_ptr() as HWND, &mut pid) };
    if pid == 0 {
        return None;
    }
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0_u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        Some(path.file_name()?.to_string_lossy().into_owned())
    }
}

/// The sequence number now, to tell whether the user has copied since a snapshot.
pub fn seq() -> u64 {
    raw::seq_num().map_or(0, |n| n.get().into())
}

/// Put an image on the clipboard in its own format, plus a `CF_DIBV5` for apps
/// that paste only bitmaps (not for GIF), plus the working file when `file` is
/// given. Returns the new sequence number.
pub fn write_image(format: Format, bytes: &[u8], file: Option<&Path>) -> Result<u64, String> {
    // Decode before opening the clipboard, so it is held as briefly as possible.
    let dib = bitmap::decode_rgba(bytes, format)
        .ok()
        .map(|(w, h, rgba)| bitmap::rgba_to_dibv5(w, h, &rgba));
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::empty().map_err(|e| e.to_string())?;
        let id = registered(format_name(format)).ok_or("could not register the format")?;
        raw::set_without_clear(id, bytes).map_err(|e| e.to_string())?;
        if let Some(dib) = dib {
            raw::set_without_clear(formats::CF_DIBV5, &dib).map_err(|e| e.to_string())?;
        }
        if let Some(file) = file {
            raw::set_file_list(&[file.to_string_lossy()]).map_err(|e| e.to_string())?;
        }
    } // closing the clipboard publishes the change
    Ok(seq())
}

/// Put files on the clipboard, as Explorer's Copy does.
pub fn write_files(paths: &[PathBuf]) -> Result<u64, String> {
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::empty().map_err(|e| e.to_string())?;
        raw::set_file_list(&names).map_err(|e| e.to_string())?;
    }
    Ok(seq())
}

/// Send the sequence number on `tx` after every clipboard change. The listener
/// window must live on the thread that waits on it.
pub fn watch(tx: Sender<u64>) {
    thread::Builder::new()
        .name("ferry-clipboard".into())
        .spawn(move || {
            let mut monitor = match Monitor::new() {
                Ok(monitor) => monitor,
                Err(e) => {
                    tracing::warn!("clipboard listener failed: {e}");
                    return;
                }
            };
            while let Ok(true) = monitor.recv() {
                if tx.send(seq()).is_err() {
                    return;
                }
            }
        })
        .expect("spawn the clipboard thread");
}
```

- [ ] **Step 3: Check that macOS is unaffected**

Run: `cargo fmt --all -- --check && cargo test -p ferry`
Expected: formatting is clean and the macOS tests pass. The Windows file is `cfg(windows)` and is not compiled here.

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/platform/windows_clipboard.rs
git commit -m "feat(app): Windows clipboard read, write and listener (CI-verified in Task 10)"
```

---

### Task 10: The running app — jobs, triggers, hotkeys, tray and commands

**Files:**
- Create: `src-tauri/src/jobs.rs`, `src-tauri/src/clipboard.rs`, `src-tauri/src/hotkeys.rs`, `src-tauri/src/commands.rs`
- Replace: `src-tauri/src/tray.rs`, `src-tauri/src/windows.rs`, `src-tauri/src/lib.rs`
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: everything from Tasks 1 to 9.
- Produces from `jobs`:
  - `start(app, Job, Level, source_seq: Option<u64>)`, `notice(app, String)`;
  - `rerun(app, &JobId, aggressive: bool) -> Result<(), String>`;
  - `restore(app, &JobId) -> Result<(), String>`, `copy(app, &JobId) -> Result<(), String>`;
  - `cancel(app, &JobId)`, `output(app, &JobId) -> Option<PathBuf>`;
  - `show(app, JobUpdate)`, `is_busy_path(app, &Path) -> bool`.
- Produces from `clipboard`: `start(app)`, `optimise_now(app, Level)`.
- Produces from `hotkeys`: `Registered { optimise, aggressive: Option<String> }` (managed state), `register(app) -> Registered`, `optimise_in_background(app, Level)`.
- Produces from `tray`: `build(app)`, `rebuild(app)`, `refresh(app)`.
- Produces from `windows`: `STACK`, `create_stack`, `stack_resize`, `open_settings`, `set_content_protected(app, protected: bool)`.
- Produces these commands for Task 11. Argument names follow Tauri's camelCase mapping in JavaScript:
  - `stack_ready() -> (StackConfig { corner, card_size, auto_hide_secs }, Vec<JobUpdate>)`;
  - `card_copy(id)`, `card_restore(id)`, `card_rerun(id, aggressive)`, `card_cancel(id)`, `card_reveal(id)`;
  - `open_log()`, `clear_all()`.
- Produces the event `job:update` with a `JobUpdate` payload, sent to the stack window.

- [ ] **Step 1: Jobs**

Create `src-tauri/src/jobs.rs`:

```rust
//! Runs jobs off the UI thread and keeps their cards up to date (spec §4.3, §5.4).
//! The work itself is `ferry_core::pipeline`; this file schedules it, puts
//! results on the clipboard and sends `job:update` to the stack.

use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use ferry_core::cancel::CancelToken;
use ferry_core::i18n::t;
use ferry_core::job::{JobId, JobState, JobUpdate, Source};
use ferry_core::optimise::Level;
use ferry_core::pipeline::{self, Job, RunOptions};
use ferry_core::sniff::Format;
use tauri::{AppHandle, Emitter, Manager};
use tracing::{info, warn};

use crate::state::{AppState, Entry};
use crate::{platform, tray, windows::STACK};

/// Show the job's Queued card now (NFR-P3) and run it when a worker is free.
/// `source_seq` is the clipboard copy a clipboard job came from; its first run
/// leaves the result off the clipboard if the user has copied something since.
pub fn start(app: &AppHandle, job: Job, level: Level, source_seq: Option<u64>) {
    let state = app.state::<AppState>();
    let queued = job.update(JobState::Queued, None);
    let entry = Arc::new(Entry {
        output: job.output.clone(),
        source_seq: Mutex::new(source_seq),
        job: Mutex::new(job),
        cancel: Mutex::new(CancelToken::new()),
        last: Mutex::new(queued.clone()),
    });
    state
        .jobs
        .lock()
        .unwrap()
        .insert(queued.id.clone(), entry.clone());
    show(app, queued);
    tray::refresh(app);
    spawn(app, entry, level);
}

/// Show a card that is not a job, such as "Nothing to optimise on the clipboard".
pub fn notice(app: &AppHandle, reason: String) {
    info!(%reason, "notice");
    show(app, pipeline::notice(reason));
}

/// Run the job again from its original: retry and re-optimise keep its level,
/// "Try aggressive" asks for Aggressive.
pub fn rerun(app: &AppHandle, id: &JobId, aggressive: bool) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let (queued, level) = {
        let job = entry.job.lock().unwrap();
        let level = if aggressive {
            Level::Aggressive
        } else {
            job.level
        };
        (job.update(JobState::Queued, None), level)
    };
    *entry.cancel.lock().unwrap() = CancelToken::new();
    *entry.last.lock().unwrap() = queued.clone();
    show(app, queued);
    tray::refresh(app);
    spawn(app, entry, level);
    Ok(())
}

/// Put the original back (§5.5).
pub fn restore(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let options = run_options(app, CancelToken::new());
    let finished = entry
        .job
        .lock()
        .unwrap()
        .restore(&options, &mut |format, bytes, file| {
            deliver(app, format, bytes, file, None)
        });
    finish(app, &entry, finished.update, finished.cause, None);
    Ok(())
}

/// Copy the card's result again: the image for a clipboard job, the file for a file job.
pub fn copy(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let job = entry.job.lock().unwrap();
    let state = app.state::<AppState>();
    let copy_paths = state.settings.read().unwrap().clipboard.copy_paths;
    let mut log = state.clipboard.lock().unwrap();
    let seq = match job.source {
        Source::Clipboard => {
            let bytes = std::fs::read(&job.output).map_err(|e| e.to_string())?;
            let file = copy_paths.then_some(job.output.as_path());
            platform::clipboard::write_image(job.format, &bytes, file)?
        }
        Source::File => platform::clipboard::write_files(std::slice::from_ref(&job.output))?,
    };
    log.record(seq, None);
    Ok(())
}

/// Stop a queued or running job; its card is already gone and nothing changes.
pub fn cancel(app: &AppHandle, id: &JobId) {
    let state = app.state::<AppState>();
    if let Some(entry) = state.jobs.lock().unwrap().remove(id) {
        entry.cancel.lock().unwrap().cancel();
    }
    tray::refresh(app);
}

/// The output file of a card, for "Show in folder".
pub fn output(app: &AppHandle, id: &JobId) -> Option<std::path::PathBuf> {
    let state = app.state::<AppState>();
    let path = state.jobs.lock().unwrap().get(id)?.output.clone();
    path.exists().then_some(path)
}

/// Send a card to the stack, or hold it until the stack page is listening.
pub fn show(app: &AppHandle, update: JobUpdate) {
    let state = app.state::<AppState>();
    if !state.settings.read().unwrap().floating.enabled {
        return;
    }
    let mut inbox = state.stack.lock().unwrap();
    if !inbox.ready {
        inbox.pending.retain(|u| u.id != update.id);
        inbox.pending.push(update);
        return;
    }
    if let Err(e) = app.emit_to(STACK, "job:update", &update) {
        warn!("could not send a card: {e}");
    }
}

/// Put an optimised image on the clipboard, or the collected files when
/// "accumulate results" is on. The clipboard log stays locked until the write's
/// sequence number is recorded, so the watcher cannot mistake it for the user's.
/// With a `source` copy, nothing is written if the user has copied since.
fn deliver(
    app: &AppHandle,
    format: Format,
    bytes: &[u8],
    file: &Path,
    source: Option<u64>,
) -> Result<(), String> {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clipboard.clone();
    let mut log = state.clipboard.lock().unwrap();
    if let Some(source) = source
        && !log.may_replace(source, platform::clipboard::seq())
    {
        info!("the clipboard changed while the job ran; result left off it");
        return Ok(());
    }
    let seq = if settings.accumulate {
        if !log.accumulated.iter().any(|f| f == file) {
            log.accumulated.push(file.to_owned());
        }
        platform::clipboard::write_files(&log.accumulated)?
    } else {
        platform::clipboard::write_image(format, bytes, settings.copy_paths.then_some(file))?
    };
    log.record(seq, source);
    Ok(())
}

fn spawn(app: &AppHandle, entry: Arc<Entry>, level: Level) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let permits = app.state::<AppState>().permits.clone();
        let Ok(_permit) = permits.acquire_owned().await else {
            return;
        };
        let cancel = entry.cancel.lock().unwrap().clone();
        if cancel.is_cancelled() {
            return;
        }
        let (worker_app, worker_entry) = (app.clone(), entry.clone());
        let joined = tauri::async_runtime::spawn_blocking(move || {
            run(&worker_app, &worker_entry, level, cancel);
        })
        .await;
        // A panic in an encoder must still end the job (spec §6).
        if let Err(e) = joined {
            let update = {
                let job = entry.job.lock().unwrap_or_else(|p| p.into_inner());
                job.update(
                    JobState::Error {
                        reason: t("error.unexpected"),
                    },
                    None,
                )
            };
            finish(&app, &entry, update, Some(e.to_string()), None);
        }
    });
}

fn run(app: &AppHandle, entry: &Arc<Entry>, level: Level, cancel: CancelToken) {
    let options = run_options(app, cancel);
    let started = Instant::now();
    let mut job = entry.job.lock().unwrap();
    info!(
        job = job.id.as_str(),
        source = ?job.source,
        format = ?job.format,
        ?level,
        before = job.before,
        "job started"
    );
    let state = app.state::<AppState>();
    // Only the first run checks; a re-run is the user's explicit request.
    let source = entry.source_seq.lock().unwrap().take();
    let finished = job.run(
        level,
        &options,
        &state.backups,
        &mut |update| {
            *entry.last.lock().unwrap() = update.clone();
            show(app, update);
        },
        &mut |format, bytes, file| deliver(app, format, bytes, file, source),
    );
    drop(job);
    match finished {
        Some(f) => finish(app, entry, f.update, f.cause, Some(started)),
        None => {
            info!(ms = started.elapsed().as_millis() as u64, "job cancelled");
            tray::refresh(app);
        }
    }
}

/// Record a job's final card: log it, show it, keep it in Recent results.
fn finish(
    app: &AppHandle,
    entry: &Entry,
    update: JobUpdate,
    cause: Option<String>,
    started: Option<Instant>,
) {
    let ms = started.map(|s| s.elapsed().as_millis() as u64);
    match &update.state {
        JobState::Error { reason } => {
            warn!(job = update.id.as_str(), ?ms, %reason, cause = cause.as_deref(), "job failed");
            app.state::<AppState>()
                .tray_error
                .store(true, std::sync::atomic::Ordering::Relaxed);
        }
        state => info!(
            job = update.id.as_str(),
            ?ms,
            ?state,
            before = update.before,
            after = update.after,
            "job finished"
        ),
    }
    *entry.last.lock().unwrap() = update.clone();
    let state = app.state::<AppState>();
    state.remember(&update);
    show(app, update);
    tray::rebuild(app);
}

fn idle_entry(app: &AppHandle, id: &JobId) -> Result<Arc<Entry>, String> {
    let state = app.state::<AppState>();
    let entry = state
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("this card's job is gone")?;
    if entry.is_busy() {
        return Err("the job is still running".into());
    }
    Ok(entry)
}

fn run_options(app: &AppHandle, cancel: CancelToken) -> RunOptions {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap();
    RunOptions::new(&settings, state.gifsicle.clone(), cancel)
}

/// True when a queued or running file job already targets `path`; two writers
/// would share one temporary file.
pub fn is_busy_path(app: &AppHandle, path: &Path) -> bool {
    let state = app.state::<AppState>();
    let jobs = state.jobs.lock().unwrap();
    jobs.values().any(|e| e.is_busy() && e.output == path)
}
```

The `job` mutex stays locked while a job runs. Anything that must answer during a run (`is_busy_path`, `output`) reads `Entry.output`, which lives outside it.

- [ ] **Step 2: Clipboard triggers**

Create `src-tauri/src/clipboard.rs`:

```rust
//! Clipboard triggers (spec §5.2, §5.3): the watcher's automatic jobs and the
//! optimise hotkeys. The rules are `ferry_core::trigger`; this file reads the
//! clipboard and starts jobs.

use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use chrono::Local;
use ferry_core::i18n::{t, tf};
use ferry_core::optimise::Level;
use ferry_core::pipeline::Job;
use ferry_core::trigger::{self, HotkeyPlan, Skip, Snapshot};
use tauri::{AppHandle, Manager};
use tracing::{debug, info, warn};

use crate::state::AppState;
use crate::{jobs, platform, tray};

const RETRY_AFTER: Duration = Duration::from_millis(100);

/// Watch the clipboard and optimise copied images while Running.
pub fn start(app: &AppHandle) {
    let (tx, rx) = mpsc::channel();
    platform::clipboard::watch(tx);
    let app = app.clone();
    thread::Builder::new()
        .name("ferry-trigger".into())
        .spawn(move || {
            for seq in rx {
                on_change(&app, seq);
            }
        })
        .expect("spawn the trigger thread");
}

fn on_change(app: &AppHandle, seq: u64) {
    let state = app.state::<AppState>();
    if state.clipboard.lock().unwrap().is_own(seq) {
        return;
    }
    let Some(snap) = read() else {
        return; // automatic triggers never show a card for a failed read (§6)
    };
    let settings = state.settings.read().unwrap().clone();
    let was_paused = *state.run_state.lock().unwrap() == ferry_core::run_state::RunState::Paused;
    let decision = trigger::automatic(
        &snap,
        &settings,
        state.gifsicle.is_some(),
        &mut state.run_state.lock().unwrap(),
    );
    match decision {
        Ok(_) => start_clipboard_job(app, snap, Level::Normal),
        Err(Skip::NotRunning) => {
            info!("skipped one copy while paused or stopped");
            if was_paused {
                tray::rebuild(app); // Paused has been spent: back to Running
            }
        }
        Err(skip) => debug!(?skip, "clipboard change left alone"),
    }
}

/// The optimise hotkeys and tray items: whatever the run state (§5.3).
pub fn optimise_now(app: &AppHandle, level: Level) {
    let Some(snap) = read() else {
        jobs::notice(app, t("error.clipboard_read"));
        return;
    };
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();
    match trigger::hotkey(&snap, &settings, state.gifsicle.is_some()) {
        HotkeyPlan::Image(_) => start_clipboard_job(app, snap, level),
        HotkeyPlan::Files(paths) => {
            for path in paths {
                if jobs::is_busy_path(app, &path) {
                    let name = path.file_name().unwrap_or_default().to_string_lossy();
                    jobs::notice(app, tf("error.busy", &[("name", &name)]));
                    continue;
                }
                match Job::file(&path, &settings, state.gifsicle.is_some()) {
                    Ok(job) => jobs::start(app, job, level, None),
                    Err(reason) => jobs::notice(app, reason),
                }
            }
        }
        HotkeyPlan::Nothing(skip) => jobs::notice(app, skip.reason()),
    }
}

fn start_clipboard_job(app: &AppHandle, snap: Snapshot, level: Level) {
    let seq = snap.seq;
    let Some((format, bytes)) = snap.image else {
        return;
    };
    let state = app.state::<AppState>();
    let template = state
        .settings
        .read()
        .unwrap()
        .clipboard
        .file_template
        .clone();
    let now = Local::now().naive_local();
    let job = Job::clipboard(
        format,
        bytes,
        snap.from_bitmap,
        &template,
        &state.work_dir,
        now,
    );
    jobs::start(app, job, level, Some(seq));
}

/// Read the clipboard, retrying once after 100 ms (§6).
fn read() -> Option<Snapshot> {
    platform::clipboard::read()
        .or_else(|first| {
            debug!("clipboard read failed, retrying: {first}");
            thread::sleep(RETRY_AFTER);
            platform::clipboard::read()
        })
        .map_err(|e| warn!("could not read the clipboard: {e}"))
        .ok()
}
```

- [ ] **Step 3: Hotkeys**

Create `src-tauri/src/hotkeys.rs`:

```rust
//! The three global hotkeys (spec §3, §5.1, §5.3). A hotkey another app already
//! owns is logged and left out of its tray item; the app keeps running (§6).

use ferry_core::optimise::Level;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{info, warn};

use crate::state::AppState;
use crate::{clipboard, tray};

#[derive(Debug, Clone, Copy)]
enum Action {
    Optimise,
    Aggressive,
    CycleState,
}

/// The accelerators that registered, for the tray's labels.
#[derive(Debug, Clone, Default)]
pub struct Registered {
    pub optimise: Option<String>,
    pub aggressive: Option<String>,
}

pub fn register(app: &AppHandle) -> Registered {
    let hotkeys = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .hotkeys
        .clone();
    let mut registered = Registered::default();
    for (accelerator, action) in [
        (&hotkeys.optimise, Action::Optimise),
        (&hotkeys.aggressive, Action::Aggressive),
        (&hotkeys.cycle_state, Action::CycleState),
    ] {
        let result =
            app.global_shortcut()
                .on_shortcut(accelerator.as_str(), move |app, _, event| {
                    if event.state == ShortcutState::Pressed {
                        act(app, action);
                    }
                });
        match result {
            Ok(()) => {
                info!(%accelerator, ?action, "hotkey registered");
                match action {
                    Action::Optimise => registered.optimise = Some(accelerator.clone()),
                    Action::Aggressive => registered.aggressive = Some(accelerator.clone()),
                    Action::CycleState => {}
                }
            }
            Err(e) => warn!(%accelerator, ?action, "hotkey not registered: {e}"),
        }
    }
    registered
}

fn act(app: &AppHandle, action: Action) {
    match action {
        Action::Optimise => optimise_in_background(app, Level::Normal),
        Action::Aggressive => optimise_in_background(app, Level::Aggressive),
        Action::CycleState => {
            let state = app.state::<AppState>();
            let mut run_state = state.run_state.lock().unwrap();
            *run_state = run_state.next();
            info!(state = ?*run_state, "run state changed");
            drop(run_state);
            tray::rebuild(app);
        }
    }
}

/// Reading a large clipboard image must not block the main thread.
pub fn optimise_in_background(app: &AppHandle, level: Level) {
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || clipboard::optimise_now(&app, level));
}
```

- [ ] **Step 4: Card commands**

Create `src-tauri/src/commands.rs`:

```rust
//! Commands the stack page calls (spec §5.5). File and clipboard work runs on
//! a blocking worker, never on the main thread.

use ferry_core::job::{JobId, JobUpdate};
use ferry_core::settings::{CardSize, Corner};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;
use crate::{jobs, logging};

/// Run `work` on a blocking worker and return its result.
async fn blocking<T: Send + 'static>(
    app: AppHandle,
    work: impl FnOnce(&AppHandle) -> Result<T, String> + Send + 'static,
) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(move || work(&app))
        .await
        .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
pub struct StackConfig {
    corner: Corner,
    card_size: CardSize,
    auto_hide_secs: u32,
}

/// The stack page is listening: hand it its settings and any cards sent before.
#[tauri::command]
pub fn stack_ready(app: AppHandle) -> (StackConfig, Vec<JobUpdate>) {
    let state = app.state::<AppState>();
    let floating = state.settings.read().unwrap().floating.clone();
    let mut inbox = state.stack.lock().unwrap();
    inbox.ready = true;
    let config = StackConfig {
        corner: floating.corner,
        card_size: floating.card_size,
        auto_hide_secs: floating.auto_hide_secs,
    };
    (config, std::mem::take(&mut inbox.pending))
}

#[tauri::command]
pub async fn card_copy(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |app| jobs::copy(app, &id)).await
}

#[tauri::command]
pub async fn card_restore(app: AppHandle, id: JobId) -> Result<(), String> {
    blocking(app, move |app| jobs::restore(app, &id)).await
}

/// Retry, re-optimise (`aggressive: false`) or "Try aggressive".
#[tauri::command]
pub fn card_rerun(app: AppHandle, id: JobId, aggressive: bool) -> Result<(), String> {
    jobs::rerun(&app, &id, aggressive)
}

#[tauri::command]
pub fn card_cancel(app: AppHandle, id: JobId) {
    jobs::cancel(&app, &id);
}

#[tauri::command]
pub fn card_reveal(app: AppHandle, id: JobId) -> Result<(), String> {
    let path = jobs::output(&app, &id).ok_or("the file is gone")?;
    tauri_plugin_opener::reveal_item_in_dir(path).map_err(|e| e.to_string())
}

/// An Error card's "Details": the newest log file in the file manager.
#[tauri::command]
pub fn open_log(app: AppHandle) -> Result<(), String> {
    let dir = app.state::<AppState>().log_dir.clone();
    match logging::latest(&dir) {
        Some(file) => tauri_plugin_opener::reveal_item_in_dir(file),
        None => tauri_plugin_opener::open_path(&dir, None::<&str>),
    }
    .map_err(|e| e.to_string())
}

/// "Clear all" also ends an "accumulate results" collection (§5.4).
#[tauri::command]
pub fn clear_all(app: AppHandle) {
    app.state::<AppState>()
        .clipboard
        .lock()
        .unwrap()
        .accumulated
        .clear();
}
```

- [ ] **Step 5: Tray**

Replace `src-tauri/src/tray.rs`:

```rust
//! Tray icon and menu (spec §5.6). The menu is rebuilt whenever what it shows
//! changes; the icon follows the run state and whether any job is working.

use std::sync::atomic::Ordering;

use ferry_core::glyph::{self, TrayGlyph};
use ferry_core::i18n::{t, tf};
use ferry_core::job::JobState;
use ferry_core::optimise::Level;
use ferry_core::run_state::RunState;
use tauri::menu::{CheckMenuItem, IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::{MouseButton, TrayIconBuilder, TrayIconEvent};
use tauri::{AppHandle, Manager, Result, Wry};
use tracing::warn;

use crate::hotkeys::{self, Registered};
use crate::state::AppState;
use crate::{jobs, windows};

const TRAY: &str = "ferry";
#[cfg(target_os = "macos")]
const BASE_ICON: &[u8] = include_bytes!("../icons/tray/64x64.png");

pub fn build(app: &AppHandle) -> Result<()> {
    let mut tray = TrayIconBuilder::with_id(TRAY)
        .tooltip(t("tray.tooltip"))
        .menu(&menu(app)?)
        .show_menu_on_left_click(true)
        .on_menu_event(|app, event| on_menu(app, event.id().as_ref()))
        .on_tray_icon_event(|tray, event| match event {
            // Opening the menu acknowledges the error mark.
            TrayIconEvent::Click {
                button: MouseButton::Left,
                ..
            } => {
                let app = tray.app_handle();
                app.state::<AppState>()
                    .tray_error
                    .store(false, Ordering::Relaxed);
                refresh(app);
            }
            #[cfg(windows)]
            TrayIconEvent::DoubleClick { .. } => windows::open_settings(tray.app_handle()),
            _ => {}
        });
    #[cfg(not(target_os = "macos"))]
    if let Some(icon) = app.default_window_icon() {
        tray = tray.icon(icon.clone());
    }
    #[cfg(target_os = "macos")]
    {
        tray = tray
            .icon(tauri::image::Image::from_bytes(BASE_ICON)?)
            .icon_as_template(true);
    }
    tray.build(app)?;
    refresh(app);
    Ok(())
}

/// Rebuild the menu (run state, recent results, toggles) and refresh the icon.
pub fn rebuild(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY) else {
        return;
    };
    match menu(app) {
        Ok(menu) => {
            let _ = tray.set_menu(Some(menu));
        }
        Err(e) => warn!("could not rebuild the tray menu: {e}"),
    }
    refresh(app);
}

/// Update the icon and tooltip for the current state.
pub fn refresh(app: &AppHandle) {
    let Some(tray) = app.tray_by_id(TRAY) else {
        return;
    };
    let state = app.state::<AppState>();
    let run_state = *state.run_state.lock().unwrap();
    let glyph = if state.tray_error.load(Ordering::Relaxed) {
        TrayGlyph::Error
    } else if state.is_working() {
        TrayGlyph::Working
    } else {
        match run_state {
            RunState::Running => TrayGlyph::Running,
            RunState::Paused => TrayGlyph::Paused,
            RunState::Stopped => TrayGlyph::Stopped,
        }
    };
    let _ = tray.set_tooltip(Some(tf(
        "tray.tooltip.state",
        &[("state", &status(run_state))],
    )));
    // Windows keeps its full-colour icon; the tooltip carries the state.
    #[cfg(target_os = "macos")]
    if let Ok(base) = tauri::image::Image::from_bytes(BASE_ICON) {
        let rgba = glyph::render(base.rgba(), base.width(), glyph);
        let _ = tray.set_icon(Some(tauri::image::Image::new_owned(
            rgba,
            base.width(),
            base.height(),
        )));
        let _ = tray.set_icon_as_template(true);
    }
    #[cfg(not(target_os = "macos"))]
    let _ = glyph;
}

fn status(run_state: RunState) -> String {
    t(match run_state {
        RunState::Running => "tray.state.running",
        RunState::Paused => "tray.state.paused",
        RunState::Stopped => "tray.state.stopped",
    })
}

fn menu(app: &AppHandle) -> Result<Menu<Wry>> {
    let state = app.state::<AppState>();
    let run_state = *state.run_state.lock().unwrap();
    let keys = app
        .try_state::<Registered>()
        .map(|r| r.inner().clone())
        .unwrap_or_default();
    let screenshots = state.settings.read().unwrap().general.show_in_screenshots;

    let header = MenuItem::with_id(
        app,
        "status",
        t(match run_state {
            RunState::Running => "tray.status.running",
            RunState::Paused => "tray.status.paused",
            RunState::Stopped => "tray.status.stopped",
        }),
        false,
        None::<&str>,
    )?;
    let radio = |id: &str, key: &str, value: RunState| {
        CheckMenuItem::with_id(app, id, t(key), true, run_state == value, None::<&str>)
    };
    let running = radio("state:running", "tray.state.running", RunState::Running)?;
    let paused = radio("state:paused", "tray.state.paused", RunState::Paused)?;
    let stopped = radio("state:stopped", "tray.state.stopped", RunState::Stopped)?;
    let optimise = MenuItem::with_id(
        app,
        "optimise",
        t("tray.optimise"),
        true,
        keys.optimise.as_deref(),
    )?;
    let aggressive = MenuItem::with_id(
        app,
        "aggressive",
        t("tray.aggressive"),
        true,
        keys.aggressive.as_deref(),
    )?;

    let recent_items: Vec<MenuItem<Wry>> = state
        .recent
        .lock()
        .unwrap()
        .iter()
        .map(|u| {
            MenuItem::with_id(
                app,
                format!("recent:{}", u.id.as_str()),
                recent_label(u),
                true,
                None::<&str>,
            )
        })
        .collect::<Result<_>>()?;
    let recent_refs: Vec<&dyn IsMenuItem<Wry>> = recent_items
        .iter()
        .map(|i| i as &dyn IsMenuItem<Wry>)
        .collect();
    let recent = Submenu::with_id_and_items(
        app,
        "recent",
        t("tray.recent"),
        !recent_items.is_empty(),
        &recent_refs,
    )?;

    let backups = MenuItem::with_id(
        app,
        "open_backups",
        t("tray.open_backups"),
        true,
        None::<&str>,
    )?;
    let work = MenuItem::with_id(app, "open_work", t("tray.open_work"), true, None::<&str>)?;
    let show = CheckMenuItem::with_id(
        app,
        "screenshots",
        t("tray.screenshots"),
        true,
        screenshots,
        None::<&str>,
    )?;
    let settings = MenuItem::with_id(
        app,
        "settings",
        t("tray.settings"),
        true,
        Some("CmdOrCtrl+,"),
    )?;
    let about = PredefinedMenuItem::about(app, Some(&t("tray.about")), None)?;
    let quit = MenuItem::with_id(app, "quit", t("tray.quit"), true, None::<&str>)?;
    Menu::with_items(
        app,
        &[
            &header,
            &running,
            &paused,
            &stopped,
            &PredefinedMenuItem::separator(app)?,
            &optimise,
            &aggressive,
            &recent,
            &PredefinedMenuItem::separator(app)?,
            &backups,
            &work,
            &PredefinedMenuItem::separator(app)?,
            &show,
            &PredefinedMenuItem::separator(app)?,
            &settings,
            &about,
            &quit,
        ],
    )
}

/// "Clipboard 2026-10-22 at 14.23.11.png — −84%" or "… — Already optimal".
fn recent_label(u: &ferry_core::job::JobUpdate) -> String {
    let name = if u.ext.is_empty() {
        u.name.clone()
    } else {
        format!("{}.{}", u.name, u.ext)
    };
    let outcome = match (&u.state, u.after) {
        // Rounded like savingPercent in src/lib/format.ts.
        (JobState::Done, Some(after)) if u.before > 0 => {
            let saved = (100.0 - after as f64 * 100.0 / u.before as f64).round();
            format!("−{saved}%")
        }
        (JobState::AlreadyOptimal, _) => t("card.state.already_optimal"),
        (JobState::Larger, _) => t("card.state.larger"),
        (JobState::Restored, _) => t("card.state.restored"),
        (JobState::Error { .. }, _) => t("card.state.error"),
        _ => return name,
    };
    if name.is_empty() {
        outcome
    } else {
        format!("{name} — {outcome}")
    }
}

fn on_menu(app: &AppHandle, id: &str) {
    let state = app.state::<AppState>();
    match id {
        "state:running" | "state:paused" | "state:stopped" => {
            *state.run_state.lock().unwrap() = match id {
                "state:running" => RunState::Running,
                "state:paused" => RunState::Paused,
                _ => RunState::Stopped,
            };
            rebuild(app);
        }
        "optimise" => hotkeys::optimise_in_background(app, Level::Normal),
        "aggressive" => hotkeys::optimise_in_background(app, Level::Aggressive),
        "open_backups" => open_folder(&state.backups_dir),
        "open_work" => open_folder(&state.work_dir),
        "screenshots" => {
            let show = {
                let mut settings = state.settings.write().unwrap();
                settings.general.show_in_screenshots = !settings.general.show_in_screenshots;
                if let Err(e) = settings.save(&state.settings_path) {
                    warn!("could not save settings: {e}");
                }
                settings.general.show_in_screenshots
            };
            windows::set_content_protected(app, !show);
            rebuild(app);
        }
        "settings" => windows::open_settings(app),
        "quit" => app.exit(0),
        other => {
            if let Some(id) = other.strip_prefix("recent:") {
                let update = state
                    .recent
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|u| u.id.as_str() == id)
                    .cloned();
                if let Some(update) = update {
                    jobs::show(app, update);
                }
            }
        }
    }
}

fn open_folder(path: &std::path::Path) {
    if let Err(e) = std::fs::create_dir_all(path)
        .map_err(|e| e.to_string())
        .and_then(|()| {
            tauri_plugin_opener::open_path(path, None::<&str>).map_err(|e| e.to_string())
        })
    {
        warn!("could not open {}: {e}", path.display());
    }
}
```

Delete the line `"tray.test_card": "Show test card",` from `src/lib/i18n/en.json`.

- [ ] **Step 6: Windows**

Replace `src-tauri/src/windows.rs`. Compared with M1, `show_test_card` is gone and four things change:
- content protection follows `show_in_screenshots`;
- `set_content_protected` is new;
- the stack sits in the `floating.corner` from settings;
- the stack follows the cursor only when `follow_cursor` is on.

```rust
//! Ferry's windows: the results stack (transparent, never focused) and
//! Settings (normal, on demand).

use ferry_core::geometry::{Corner, Rect, fit, place};
use ferry_core::settings;
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

use ferry_core::i18n::t;

use crate::platform;
use crate::state::AppState;

pub const STACK: &str = "stack";
pub const SETTINGS: &str = "settings";

/// stack.html pads the cards by 12 logical px (room for shadows); adding this
/// margin keeps cards 16 px from the work-area edge (R§4.4).
const STACK_MARGIN: f64 = 4.0;

/// Create the hidden results stack at startup so its page is loaded and
/// listening before the first card arrives.
pub fn create_stack(app: &AppHandle) -> tauri::Result<()> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, STACK, WebviewUrl::App("stack.html".into()))
        .title(t("stack.window_title"))
        .inner_size(1.0, 1.0)
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
    platform::make_non_activating(&window)
}

/// Hide the stack from screenshots and screen sharing, or show it (R§3.1).
pub fn set_content_protected(app: &AppHandle, protected: bool) {
    if let Some(stack) = app.get_webview_window(STACK) {
        let _ = stack.set_content_protected(protected);
    }
}

/// Fit the stack window to its content and pin it to the chosen corner of the
/// work area under the cursor (or of the main screen). A zero size hides it.
#[tauri::command]
pub fn stack_resize(app: AppHandle, width: f64, height: f64) -> Result<(), String> {
    let Some(stack) = app.get_webview_window(STACK) else {
        return Ok(());
    };
    if width < 1.0 || height < 1.0 {
        return platform::hide(&stack).map_err(|e| e.to_string());
    }
    let floating = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .clone();
    let corner = match floating.corner {
        settings::Corner::TopLeft => Corner::TopLeft,
        settings::Corner::TopRight => Corner::TopRight,
        settings::Corner::BottomLeft => Corner::BottomLeft,
        settings::Corner::BottomRight => Corner::BottomRight,
    };
    let monitor = app
        .cursor_position()
        .ok()
        .filter(|_| floating.follow_cursor)
        .and_then(|p| {
            // tao reports the macOS cursor as points × the main display's scale,
            // while monitor_from_point compares against points.
            #[cfg(target_os = "macos")]
            let main_scale = app
                .primary_monitor()
                .ok()
                .flatten()
                .map_or(1.0, |m| m.scale_factor());
            #[cfg(not(target_os = "macos"))]
            let main_scale = 1.0;
            app.monitor_from_point(p.x / main_scale, p.y / main_scale)
                .ok()
                .flatten()
        })
        .or_else(|| app.primary_monitor().ok().flatten())
        .ok_or("no monitor found")?;
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let margin = (STACK_MARGIN * scale).round() as u32;
    // A stack taller than the work area keeps its newest cards (the page anchors
    // them to the bottom) instead of running under the taskbar.
    let size = fit(
        area,
        (
            (width * scale).ceil() as u32,
            (height * scale).ceil() as u32,
        ),
        margin,
    );
    let (x, y) = place(area, size, corner, margin);

    // macOS positions windows in points; converting with the *target* monitor's
    // scale keeps mixed-DPI setups right. Windows uses physical pixels directly.
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, LogicalSize};
        stack
            .set_size(LogicalSize::new(
                f64::from(size.0) / scale,
                f64::from(size.1) / scale,
            ))
            .map_err(|e| e.to_string())?;
        stack
            .set_position(LogicalPosition::new(
                f64::from(x) / scale,
                f64::from(y) / scale,
            ))
            .map_err(|e| e.to_string())?;
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        // Move first so the window is on the target monitor before it is sized;
        // otherwise a DPI change rescales the new size a second time.
        stack
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
        stack
            .set_size(PhysicalSize::new(size.0, size.1))
            .map_err(|e| e.to_string())?;
        stack
            .set_position(PhysicalPosition::new(x, y))
            .map_err(|e| e.to_string())?;
    }
    platform::show_without_focus(&stack).map_err(|e| e.to_string())
}

/// Show the Settings window, creating it the first time.
pub fn open_settings(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(SETTINGS) {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        return;
    }
    let built = WebviewWindowBuilder::new(app, SETTINGS, WebviewUrl::App("settings.html".into()))
        .title(t("settings.window_title"))
        .inner_size(820.0, 600.0)
        .min_inner_size(720.0, 480.0)
        .center()
        .build();
    match built {
        // A tray app is never frontmost, so a new window opens behind other apps without this.
        Ok(window) => {
            let _ = window.set_focus();
        }
        Err(e) => eprintln!("ferry: could not open settings: {e}"),
    }
}
```

- [ ] **Step 7: Wire it together**

Replace `src-tauri/src/lib.rs`:

```rust
mod clipboard;
mod commands;
mod hotkeys;
mod jobs;
mod logging;
mod platform;
mod state;
mod tray;
mod windows;

use std::path::PathBuf;

use ferry_core::i18n::t;
use ferry_core::settings::{LoadStatus, Settings};
use tauri::{AppHandle, Manager};
use tracing::info;

use crate::state::AppState;

pub fn run() {
    // Must be the first plugin: a second launch focuses Settings instead of adding a tray icon.
    let builder =
        tauri::Builder::default().plugin(tauri_plugin_single_instance::init(|app, _args, _cwd| {
            windows::open_settings(app)
        }));
    #[cfg(target_os = "macos")]
    let builder = builder.plugin(tauri_nspanel::init());

    builder
        .plugin(tauri_plugin_global_shortcut::Builder::new().build())
        .plugin(tauri_plugin_drag::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);
            setup(app.handle())?;
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            windows::stack_resize,
            commands::stack_ready,
            commands::card_copy,
            commands::card_restore,
            commands::card_rerun,
            commands::card_cancel,
            commands::card_reveal,
            commands::open_log,
            commands::clear_all,
        ])
        .build(tauri::generate_context!())
        .expect("error while building Ferry")
        .run(|_app, event| match event {
            // Closing Settings must not quit a tray app; only "Quit Ferry" (an explicit exit code) does.
            tauri::RunEvent::ExitRequested {
                code: None, api, ..
            } => api.prevent_exit(),
            // Finder, Spotlight and `open Ferry.app` re-activate the running app instead of
            // starting a second process, so single-instance never sees them.
            #[cfg(target_os = "macos")]
            tauri::RunEvent::Reopen { .. } => windows::open_settings(_app),
            _ => {}
        });
}

fn setup(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    let paths = app.path();
    let log_dir = paths.app_log_dir()?;
    if let Some(guard) = logging::init(&log_dir) {
        app.manage(guard);
    }
    let settings_path = paths.app_config_dir()?.join("settings.json");
    let (settings, status) = Settings::load(&settings_path);
    info!(
        version = env!("CARGO_PKG_VERSION"),
        ?status,
        "Ferry started"
    );
    app.manage(AppState::new(
        settings,
        settings_path,
        paths.app_data_dir()?.join("backups"),
        paths.app_cache_dir()?.join("work"),
        log_dir,
        gifsicle(),
    ));
    windows::create_stack(app)?;
    let registered = hotkeys::register(app);
    app.manage(registered);
    tray::build(app)?;
    clipboard::start(app);
    if let LoadStatus::Recovered { .. } = status {
        jobs::notice(app, t("error.settings_reset"));
    }
    Ok(())
}

/// The bundled gifsicle sits next to Ferry's own executable (Tauri `externalBin`).
fn gifsicle() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let path = exe.with_file_name(format!("gifsicle{}", std::env::consts::EXE_SUFFIX));
    path.is_file().then_some(path)
}
```

- [ ] **Step 8: Lint and test**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferry`
Expected: no warnings, and 14 app tests pass (1 logging, 8 state, 5 macOS clipboard).

- [ ] **Step 9: Smoke-run without touching the clipboard**

Seed the dev settings so the app starts Stopped:

```bash
mkdir -p "$HOME/Library/Application Support/io.ferry.desktop.dev"
printf '{ "version": 1, "general": { "start_state": "stopped" } }\n' > "$HOME/Library/Application Support/io.ferry.desktop.dev/settings.json"
pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'
```

Expected:
- `~/Library/Logs/io.ferry.desktop.dev/ferry.<today>.log` contains `Ferry started` and three `hotkey registered` lines;
- neither the terminal nor the log shows a panic or a WARN line;
- the tray icon appears dimmed (Stopped).

Stop the app.

- [ ] **Step 10: Commit and let CI compile Windows**

```bash
git add src-tauri src/lib/i18n/en.json
git commit -m "feat(app): job runner, clipboard and hotkey triggers, tray menu and card commands"
git push -u origin m2b-app-core-loop
```

Pushing a feature branch is how M2a reached CI. Ask first if pushing this branch has not been approved in this session.

Watch the run with `~/.local/bin/gh run watch`. Wait for the macOS bundle job. Then check the Windows bundle job: its `cargo clippy` and `cargo test -p ferry` compile and test `windows_clipboard.rs` (Task 9). Fix any Windows failure in that file before Task 11, with a ruling in the ledger.

---

### Task 11: Cards, their actions and drag-out

**Files:**
- Modify: `package.json`, `pnpm-lock.yaml` (by pnpm)
- Replace: `src/lib/ipc.ts`, `src/stack/Card.svelte`, `src/stack/Stack.svelte`
- Create: `src/lib/cards.ts`, `src/lib/cards.test.ts`, `src/stack/DragAllBar.svelte`

**Interfaces:**
- Consumes: the commands and the `job:update` event from Task 10. Also `savingPercent` and `formatBytes` from `src/lib/format.ts`, `t` from `src/lib/i18n.ts`, and `startDrag` from `@crabnebula/tauri-plugin-drag`.
- Produces, in `ipc.ts`: `JobState`, `JobUpdate`, `Corner`, `StackConfig`, `isMac`, `applyOsClass`. `CardData` from M1 is removed.
- Produces, in `cards.ts`: `isBusy`, `autoHides`, `railActions`, `canDrag`, `upsert`, `dragAllFiles`.

- [ ] **Step 1: Add the drag plugin's JavaScript side**

```bash
pnpm add --save-exact @crabnebula/tauri-plugin-drag@2.1.0
```

Expected: `package.json` gains `"@crabnebula/tauri-plugin-drag": "2.1.0"` under `dependencies`.

- [ ] **Step 2: Replace the IPC types**

Replace `src/lib/ipc.ts`:

```ts
/** Mirrors ferry_core::job (Rust serialises with serde; sizes are bytes). */
export type JobState =
  | { kind: 'queued' }
  | { kind: 'optimising'; progress: number; step: string }
  | { kind: 'done' }
  | { kind: 'already_optimal' }
  | { kind: 'larger' }
  | { kind: 'error'; reason: string }
  | { kind: 'restored' };

export interface JobUpdate {
  id: string;
  state: JobState;
  name: string;
  ext: string;
  before: number;
  after: number | null;
  detail: string;
  thumb_path: string | null;
  source: 'clipboard' | 'file';
  can_restore: boolean;
  can_retry: boolean;
  output: string | null;
}

export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';

/** The floating-results settings the stack page needs (commands::stack_ready). */
export interface StackConfig {
  corner: Corner;
  card_size: 'default' | 'compact';
  auto_hide_secs: number;
}

export const isMac = navigator.userAgent.includes('Mac');

/** Tag <html> with os-mac / os-windows so theme.css can switch fonts and radii. */
export function applyOsClass(): void {
  document.documentElement.classList.add(isMac ? 'os-mac' : 'os-windows');
}
```

- [ ] **Step 3: Write the failing card-logic tests**

Create `src/lib/cards.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { autoHides, canDrag, dragAllFiles, isBusy, railActions, upsert } from './cards';
import type { JobState, JobUpdate } from './ipc';

function card(state: JobState, extra: Partial<JobUpdate> = {}): JobUpdate {
  return {
    id: 'a',
    state,
    name: 'shot',
    ext: 'png',
    before: 2_000_000,
    after: null,
    detail: '',
    thumb_path: '/b/shot.png',
    source: 'clipboard',
    can_restore: false,
    can_retry: false,
    output: '/w/shot.png',
    ...extra,
  };
}

describe('card states', () => {
  it('treats queued and optimising as busy', () => {
    expect(isBusy({ kind: 'queued' })).toBe(true);
    expect(isBusy({ kind: 'optimising', progress: 0, step: 'x' })).toBe(true);
    expect(isBusy({ kind: 'done' })).toBe(false);
  });

  it('auto-hides finished cards but never busy or error cards', () => {
    expect(autoHides({ kind: 'done' })).toBe(true);
    expect(autoHides({ kind: 'already_optimal' })).toBe(true);
    expect(autoHides({ kind: 'larger' })).toBe(true);
    expect(autoHides({ kind: 'restored' })).toBe(true);
    expect(autoHides({ kind: 'error', reason: 'x' })).toBe(false);
    expect(autoHides({ kind: 'queued' })).toBe(false);
  });
});

describe('railActions', () => {
  it('offers restore only on a restorable done card', () => {
    expect(railActions(card({ kind: 'done' }, { can_restore: true }))).toEqual([
      'copy',
      'reveal',
      'restore',
      'aggressive',
    ]);
    expect(railActions(card({ kind: 'larger' }))).toEqual(['copy', 'reveal', 'aggressive']);
  });

  it('leaves aggressive off a restored card, which offers re-optimise instead', () => {
    expect(railActions(card({ kind: 'restored' }))).toEqual(['copy', 'reveal']);
  });

  it('has no rail while busy, on errors, or without an output file', () => {
    expect(railActions(card({ kind: 'queued' }))).toEqual([]);
    expect(railActions(card({ kind: 'error', reason: 'x' }))).toEqual([]);
    expect(railActions(card({ kind: 'done' }, { output: null }))).toEqual([]);
  });
});

describe('the stack', () => {
  it('updates a card in place and adds new ones at the end', () => {
    const first = card({ kind: 'queued' });
    const second = card({ kind: 'queued' }, { id: 'b' });
    let cards = upsert(upsert([], first), second);
    cards = upsert(cards, card({ kind: 'done' }));
    expect(cards.map((c) => [c.id, c.state.kind])).toEqual([
      ['a', 'done'],
      ['b', 'queued'],
    ]);
  });

  it('drags every finished output and nothing else', () => {
    const cards = [
      card({ kind: 'done' }, { id: '1', output: '/w/1.png' }),
      card({ kind: 'optimising', progress: 0, step: '' }, { id: '2', output: '/w/2.png' }),
      card({ kind: 'error', reason: 'x' }, { id: '3', output: '/w/3.png' }),
      card({ kind: 'already_optimal' }, { id: '4', output: '/w/4.png' }),
    ];
    expect(dragAllFiles(cards)).toEqual(['/w/1.png', '/w/4.png']);
    expect(canDrag(cards[1])).toBe(false);
  });
});
```

- [ ] **Step 4: Run them to see them fail**

Run: `pnpm test`
Expected: FAIL, with "Failed to resolve import './cards'".

- [ ] **Step 5: Implement the card logic**

Create `src/lib/cards.ts`:

```ts
import type { JobState, JobUpdate } from './ipc';

export type RailAction = 'copy' | 'reveal' | 'restore' | 'aggressive';

/** Waiting for or running its encoder: no rail, and the close button cancels. */
export function isBusy(state: JobState): boolean {
  return state.kind === 'queued' || state.kind === 'optimising';
}

/** Finished cards fade out after `floating.auto_hide_secs`; busy and error cards stay (§5.5). */
export function autoHides(state: JobState): boolean {
  return ['done', 'already_optimal', 'larger', 'restored'].includes(state.kind);
}

/** The rail's buttons for a card, in order (§7 card anatomy). */
export function railActions(card: JobUpdate): RailAction[] {
  if (!autoHides(card.state) || card.output === null) return [];
  const actions: RailAction[] = ['copy', 'reveal'];
  if (card.state.kind === 'done' && card.can_restore) actions.push('restore');
  if (card.state.kind !== 'restored') actions.push('aggressive');
  return actions;
}

/** Whether the card's file can be dragged out. */
export function canDrag(card: JobUpdate): boolean {
  return autoHides(card.state) && card.output !== null;
}

/** Replace the card with the same id where it is, or add a new one at the end. */
export function upsert(cards: JobUpdate[], card: JobUpdate): JobUpdate[] {
  const at = cards.findIndex((c) => c.id === card.id);
  if (at === -1) return [...cards, card];
  return cards.map((c, i) => (i === at ? card : c));
}

/** The files "Drag all" carries: every finished card's output. */
export function dragAllFiles(cards: JobUpdate[]): string[] {
  return cards.filter(canDrag).map((c) => c.output as string);
}
```

Run: `pnpm test`
Expected: PASS, 7 new tests.

- [ ] **Step 6: The card**

Replace `src/stack/Card.svelte`:

```svelte
<script lang="ts">
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fade } from 'svelte/transition';
  import { autoHides, canDrag, isBusy, railActions } from '../lib/cards';
  import { formatBytes, savingPercent } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import { isMac, type JobUpdate } from '../lib/ipc';

  let {
    card,
    hovered = false,
    compact = false,
    railRight = false,
    autoHideSecs,
    onclose,
  }: {
    card: JobUpdate;
    hovered?: boolean;
    compact?: boolean;
    railRight?: boolean;
    autoHideSecs: number;
    onclose: () => void;
  } = $props();

  let focused = $state(false);
  let thumb = $state<HTMLImageElement>();

  const busy = $derived(isBusy(card.state));
  const notice = $derived(card.name === '' && card.ext === '');
  const fileName = $derived(card.ext ? `${card.name}.${card.ext}` : card.name);
  const rail = $derived(railActions(card));
  const stateLabel = $derived(t(`card.state.${card.state.kind}` as StringKey));
  const label = $derived(
    t('card.aria', { name: notice ? t('card.notice') : fileName, state: stateLabel }),
  );
  const motionOut = window.matchMedia('(prefers-reduced-motion: reduce)').matches ? 0 : 150;

  // Finished cards fade out after the configured time; hover or focus holds them (§5.5).
  $effect(() => {
    if (!autoHides(card.state) || autoHideSecs === 0 || hovered || focused) return;
    const timer = setTimeout(onclose, autoHideSecs * 1000);
    return () => clearTimeout(timer);
  });

  function act(command: string, args: Record<string, unknown> = {}) {
    invoke(command, { id: card.id, ...args }).catch((e) => console.warn(command, e));
  }

  function close() {
    if (busy) act('card_cancel');
    onclose();
  }

  // A drag preview the size of the card's thumbnail, not of the full image.
  function dragIcon(): string {
    const canvas = document.createElement('canvas');
    const width = 120;
    canvas.width = width;
    canvas.height = 80;
    const ctx = canvas.getContext('2d');
    if (ctx && thumb?.complete && thumb.naturalWidth > 0) {
      const scale = Math.max(width / thumb.naturalWidth, 80 / thumb.naturalHeight);
      const w = thumb.naturalWidth * scale;
      const h = thumb.naturalHeight * scale;
      ctx.drawImage(thumb, (width - w) / 2, (80 - h) / 2, w, h);
    }
    return canvas.toDataURL('image/png');
  }

  function drag(event: MouseEvent) {
    if (event.button !== 0 || !canDrag(card)) return;
    if ((event.target as HTMLElement).closest('button')) return;
    startDrag({ item: [card.output as string], icon: dragIcon() }).catch((e) =>
      console.warn('drag', e),
    );
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<article
  class="card"
  class:hovered
  class:compact
  class:busy
  class:error={card.state.kind === 'error'}
  class:rail-right={railRight}
  data-card-id={card.id}
  aria-label={label}
  onmousedown={drag}
  onfocusin={() => (focused = true)}
  onfocusout={() => (focused = false)}
  out:fade={{ duration: motionOut }}
>
  <div class="thumb" aria-hidden="true">
    {#if card.thumb_path}
      <img bind:this={thumb} src={convertFileSrc(card.thumb_path)} alt="" crossorigin="anonymous" />
    {:else}
      <svg width="40" height="40" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><circle cx="9" cy="9" r="2" /><path d="m21 15-3.1-3.1a2 2 0 0 0-2.8 0L6 21" /></svg>
    {/if}
  </div>
  {#if busy || card.state.kind === 'error'}
    <div class="dim" aria-hidden="true"></div>
  {/if}
  {#if busy}
    <svg class="ring" width="56" height="56" viewBox="0 0 56 56" aria-hidden="true">
      <circle cx="28" cy="28" r="24" fill="none" stroke="rgba(255,255,255,.25)" stroke-width="4" />
      <circle cx="28" cy="28" r="24" fill="none" stroke="#fff" stroke-width="4" stroke-linecap="round" stroke-dasharray="40 151" />
    </svg>
  {/if}
  <div class="scrim" aria-hidden="true"></div>

  {#if rail.length > 0}
    <div class="rail">
      {#if rail.includes('copy')}
        <button type="button" aria-label={t('card.copy')} onclick={() => act('card_copy')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
        </button>
      {/if}
      {#if rail.includes('reveal')}
        <button type="button" aria-label={t(isMac ? 'card.reveal.mac' : 'card.reveal.windows')} onclick={() => act('card_reveal')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z" /></svg>
        </button>
      {/if}
      {#if rail.includes('restore')}
        <button type="button" aria-label={t('card.restore')} onclick={() => act('card_restore')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5" /></svg>
        </button>
      {/if}
      {#if rail.includes('aggressive')}
        <button type="button" aria-label={t('card.aggressive')} onclick={() => act('card_rerun', { aggressive: true })}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M13 2 3 14h9l-1 8 10-12h-9l1-8z" /></svg>
        </button>
      {/if}
      {#if !compact}
        <button type="button" aria-label={t('card.more')} disabled>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round"><path d="M12 5v14M5 12h14" /></svg>
        </button>
      {/if}
    </div>
  {/if}

  <button class="close" type="button" aria-label={t(busy ? 'card.cancel' : 'card.close')} onclick={close}>
    <svg aria-hidden="true" width="12" height="12" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round"><path d="M18 6 6 18M6 6l12 12" /></svg>
  </button>

  <div class="info">
    {#if !notice}
      <div class="chips">
        <span class="name">{card.name}</span>
        {#if card.ext}<span class="ext">{card.ext}</span>{/if}
      </div>
    {/if}
    <div class="line">
      {#if card.state.kind === 'queued'}
        <span>{t('card.waiting')} {formatBytes(card.before)}</span>
      {:else if card.state.kind === 'optimising'}
        <span>{card.state.step}…</span>
      {:else if card.state.kind === 'done'}
        <span>{formatBytes(card.before)} → {formatBytes(card.after ?? card.before)}</span>
        <span class="pill ok">
          <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
          {savingPercent(card.before, card.after ?? card.before)}
        </span>
      {:else if card.state.kind === 'already_optimal' || card.state.kind === 'restored'}
        <span>{formatBytes(card.before)}</span>
        <span class="pill grey">
          <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M5 12h14" /></svg>
          {stateLabel}
        </span>
      {:else if card.state.kind === 'larger'}
        <span>{formatBytes(card.before)}</span>
        <span class="pill amber">
          <svg aria-hidden="true" width="11" height="11" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="3" stroke-linecap="round"><path d="M12 19V5M5 12l7-7 7 7" /></svg>
          {stateLabel}
        </span>
      {:else if card.state.kind === 'error'}
        <span class="reason" role="alert">{card.state.reason}</span>
      {/if}
    </div>
    {#if card.state.kind === 'error'}
      <div class="buttons">
        {#if card.can_retry}
          <button type="button" onclick={() => act('card_rerun', { aggressive: false })}>{t('card.retry')}</button>
        {/if}
        <button type="button" onclick={() => invoke('open_log')}>{t('card.details')}</button>
      </div>
    {:else if card.state.kind === 'restored'}
      <div class="buttons">
        <button type="button" onclick={() => act('card_rerun', { aggressive: false })}>{t('card.reoptimise')}</button>
      </div>
    {:else if card.state.kind === 'done' && card.detail && !compact}
      <span class="detail">{card.detail}</span>
    {/if}
  </div>
</article>

<style>
  .card {
    position: relative;
    width: 300px;
    height: 200px;
    border-radius: var(--card-radius);
    overflow: hidden;
    background: #2c2c2e;
    color: #fff;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22), 0 0 0 0.5px rgba(0, 0, 0, 0.18);
    font-variant-numeric: tabular-nums;
    animation: slide-in var(--motion-in) ease-out;
  }
  .card.compact {
    width: 220px;
    height: 150px;
  }
  .card.error {
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.22), inset 0 0 0 2px #ff453a;
  }
  .thumb {
    position: absolute;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    padding-bottom: 36px;
    box-sizing: border-box;
    color: rgba(255, 255, 255, 0.55);
  }
  .thumb img {
    position: absolute;
    inset: 0;
    width: 100%;
    height: 100%;
    object-fit: cover;
  }
  .dim {
    position: absolute;
    inset: 0;
    background: rgba(0, 0, 0, 0.45);
  }
  .ring {
    position: absolute;
    left: calc(50% - 28px);
    top: calc(50% - 46px);
    animation: spin 1s linear infinite;
  }
  .scrim {
    position: absolute;
    inset: auto 0 0 0;
    height: 64%;
    background: linear-gradient(to top, rgba(0, 0, 0, 0.8), rgba(0, 0, 0, 0.5) 45%, rgba(0, 0, 0, 0));
  }
  .rail {
    position: absolute;
    left: 8px;
    top: 8px;
    display: flex;
    flex-direction: column;
    gap: 2px;
    padding: 4px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.25), 0 0 0 0.5px var(--rail-line);
    opacity: 0;
    visibility: hidden;
    transition: opacity 120ms, visibility 120ms;
  }
  .rail-right .rail {
    left: auto;
    right: 8px;
    top: 40px;
  }
  .rail button {
    width: 28px;
    height: 28px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
  }
  .rail button:hover:not(:disabled) {
    background: var(--rail-line);
  }
  .rail button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .close {
    position: absolute;
    top: 8px;
    right: 8px;
    width: 24px;
    height: 24px;
    border-radius: 50%;
    border: 0.5px solid rgba(255, 255, 255, 0.35);
    background: rgba(0, 0, 0, 0.55);
    color: #fff;
    display: grid;
    place-items: center;
    padding: 0;
    cursor: pointer;
    opacity: 0;
    visibility: hidden;
    transition: opacity 120ms, visibility 120ms;
  }
  .card:hover .rail,
  .card.hovered .rail,
  .card:focus-within .rail,
  .card:hover .close,
  .card.hovered .close,
  .card:focus-within .close {
    opacity: 1;
    visibility: visible;
  }
  .info {
    position: absolute;
    left: 12px;
    right: 12px;
    bottom: 10px;
    display: flex;
    flex-direction: column;
    gap: 5px;
    transition: left 120ms, right 120ms;
  }
  .card:not(.rail-right):is(:hover, .hovered, :focus-within):has(.rail) .info {
    left: 52px;
  }
  .card.rail-right:is(:hover, .hovered, :focus-within):has(.rail) .info {
    right: 52px;
  }
  .chips {
    display: flex;
    gap: 6px;
    align-items: center;
    min-width: 0;
  }
  .name {
    min-width: 0;
    flex-shrink: 1;
    background: rgba(255, 255, 255, 0.16);
    font-size: 12px;
    padding: 2px 8px;
    border-radius: 999px;
    white-space: nowrap;
    overflow: hidden;
    text-overflow: ellipsis;
  }
  .ext {
    flex-shrink: 0;
    border: 1px solid rgba(255, 255, 255, 0.45);
    font-size: 11px;
    font-weight: 600;
    padding: 1px 7px;
    border-radius: 999px;
  }
  .line {
    display: flex;
    align-items: center;
    gap: 8px;
    font-size: 13px;
    font-weight: 600;
    min-width: 0;
  }
  .reason {
    color: #ffb3ae;
    font-weight: 500;
  }
  .pill {
    display: inline-flex;
    align-items: center;
    gap: 3px;
    font-size: 12px;
    font-weight: 600;
    padding: 1px 8px 1px 6px;
    border-radius: 999px;
    white-space: nowrap;
  }
  .pill.ok {
    background: rgba(48, 209, 88, 0.26);
    color: #8cf5ae;
  }
  .pill.amber {
    background: rgba(255, 159, 10, 0.28);
    color: #ffd27a;
  }
  .pill.grey {
    background: rgba(255, 255, 255, 0.2);
    color: #f2f2f7;
  }
  .buttons {
    display: flex;
    gap: 6px;
  }
  .buttons button {
    border: 0.5px solid rgba(255, 255, 255, 0.4);
    background: rgba(255, 255, 255, 0.16);
    color: #fff;
    font: inherit;
    font-size: 12px;
    padding: 2px 10px;
    border-radius: 999px;
    cursor: pointer;
  }
  .detail {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.82);
  }
  @keyframes slide-in {
    from {
      transform: translateX(24px);
      opacity: 0;
    }
    to {
      transform: none;
      opacity: 1;
    }
  }
  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }
  @media (prefers-reduced-motion: reduce) {
    .card,
    .ring {
      animation: none;
    }
  }
</style>
```

The rail and close button are hidden with `visibility`, so they cannot be clicked while invisible. The fade-out lasts 0 ms under `prefers-reduced-motion`. These fix the two M1 review minors about reduced motion and hidden-but-clickable controls.

- [ ] **Step 7: The Drag all / Clear all bar**

Create `src/stack/DragAllBar.svelte`:

```svelte
<script lang="ts">
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { t } from '../lib/i18n';

  let { files, onclear }: { files: string[]; onclear: () => void } = $props();

  // A plain accent square stands in for the stack in the drag preview.
  function icon(): string {
    const canvas = document.createElement('canvas');
    canvas.width = canvas.height = 48;
    const ctx = canvas.getContext('2d');
    if (ctx) {
      ctx.fillStyle = '#0a64d8';
      ctx.fillRect(0, 0, 48, 48);
    }
    return canvas.toDataURL('image/png');
  }

  function dragAll(event: MouseEvent) {
    if (event.button !== 0 || files.length === 0) return;
    startDrag({ item: files, icon: icon() }).catch((e) => console.warn('drag all', e));
  }
</script>

<div class="bar">
  <button type="button" class="drag" disabled={files.length === 0} onmousedown={dragAll}>
    <svg aria-hidden="true" width="14" height="14" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M5 9l-3 3 3 3M9 5l3-3 3 3M15 19l-3 3-3-3M19 9l3 3-3 3M2 12h20M12 2v20" /></svg>
    {t('stack.drag_all')}
  </button>
  <button type="button" onclick={onclear}>{t('stack.clear_all')}</button>
</div>

<style>
  .bar {
    display: flex;
    justify-content: space-between;
    gap: 8px;
    padding: 6px;
    border-radius: 10px;
    background: var(--rail-bg);
    color: var(--rail-fg);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.2), 0 0 0 0.5px var(--rail-line);
  }
  button {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    border: 0;
    border-radius: 7px;
    background: transparent;
    color: inherit;
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    padding: 4px 10px;
    cursor: pointer;
  }
  button:hover:not(:disabled) {
    background: var(--rail-line);
  }
  button.drag {
    cursor: grab;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
```

- [ ] **Step 8: The stack**

Replace `src/stack/Stack.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { dragAllFiles, upsert } from '../lib/cards';
  import type { JobUpdate, StackConfig } from '../lib/ipc';
  import Card from './Card.svelte';
  import DragAllBar from './DragAllBar.svelte';

  let cards = $state<JobUpdate[]>([]);
  let config = $state<StackConfig>({ corner: 'bottom_right', card_size: 'default', auto_hide_secs: 6 });
  let hovered = $state<string | null>(null);
  let root = $state<HTMLDivElement>();

  const top = $derived(config.corner.startsWith('top'));
  const railRight = $derived(config.corner.endsWith('left'));

  // Ask Rust to fit the window to the stack; 0×0 hides it. getBoundingClientRect
  // forces layout, so this works even while the window is still hidden.
  function report() {
    if (!root) return;
    const rect = root.getBoundingClientRect();
    const empty = cards.length === 0;
    invoke('stack_resize', {
      width: empty ? 0 : Math.ceil(rect.width),
      height: empty ? 0 : Math.ceil(rect.height),
    });
  }

  $effect(() => {
    void cards.length;
    report();
  });

  // Rust forwards the pointer because a never-key window gets no :hover. AppKit's
  // y runs up from the window's bottom edge.
  function pointAt(x: number, fromBottom: number) {
    const el = document.elementFromPoint(x, window.innerHeight - fromBottom);
    hovered = el?.closest<HTMLElement>('[data-card-id]')?.dataset.cardId ?? null;
  }

  onMount(() => {
    // Listen first, then tell Rust we are ready, so no card falls in between.
    const unlisten = Promise.all([
      listen<JobUpdate>('job:update', (event) => {
        cards = upsert(cards, event.payload);
      }),
      listen<{ x: number; fromBottom: number }>('stack:pointer', (event) =>
        pointAt(event.payload.x, event.payload.fromBottom),
      ),
      listen('stack:pointer-leave', () => {
        hovered = null;
      }),
    ]).then(async (stops) => {
      const [stackConfig, pending] = await invoke<[StackConfig, JobUpdate[]]>('stack_ready');
      config = stackConfig;
      for (const card of pending) cards = upsert(cards, card);
      return stops;
    });
    const observer = new ResizeObserver(report);
    if (root) observer.observe(root);
    return () => {
      observer.disconnect();
      unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  });

  function close(id: string) {
    cards = cards.filter((card) => card.id !== id);
  }

  function clearAll() {
    cards = [];
    invoke('clear_all');
  }
</script>

<div class="stack" class:top bind:this={root}>
  {#if cards.length > 1}
    <DragAllBar files={dragAllFiles(cards)} onclear={clearAll} />
  {/if}
  {#each cards as card (card.id)}
    <Card
      {card}
      hovered={card.id === hovered}
      compact={config.card_size === 'compact'}
      {railRight}
      autoHideSecs={config.auto_hide_secs}
      onclose={() => close(card.id)}
    />
  {/each}
</div>

<style>
  :global(html),
  :global(body) {
    background: transparent;
    height: 100%;
    overflow: hidden;
  }
  /* Bottom-anchored: when Rust clamps the window below the stack's height, the
     oldest cards are clipped at the top and the newest stay visible. */
  :global(#app) {
    display: flex;
    flex-direction: column;
    justify-content: flex-end;
    height: 100%;
  }
  :global(#app:has(.stack.top)) {
    justify-content: flex-start;
  }
  .stack {
    flex-shrink: 0;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    width: max-content;
  }
</style>
```

The page starts listening for `job:update` before it calls `stack_ready`, so a card sent in between is not lost: Rust keeps cards in `StackInbox` until `stack_ready` returns them.

- [ ] **Step 9: Verify**

Run: `pnpm check && pnpm test && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 svelte-check errors, every vitest passes, and no clippy warnings.

- [ ] **Step 10: Commit**

```bash
git add package.json pnpm-lock.yaml src
git commit -m "feat(ui): result cards with actions, drag out, Drag all and Clear all"
```

---

### Task 12: Manual checklist on this Mac, then CI

**Files:** none, unless a check fails. A failure goes through superpowers:systematic-debugging, and its fix gets a test where the code is testable.

Checks 1 and 6 overwrite the real clipboard, so ask the user before running them. Run against the dev build (`pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'`) with the seeded settings file deleted, so the defaults apply (start Running).

- [ ] **Step 1: Run spec §8's checklist and record each result**

| # | Check | How |
| --- | --- | --- |
| 1 | Copying a screenshot replaces the clipboard with a smaller image | ⌃⇧⌘4, then paste into a browser or Slack; the card says Done −N% |
| 2 | A 5 MB PNG round trip takes under 1.5 s | Copy a 5 MB PNG; read `ms=` on its `job finished` log line. Also record one large JPEG and one large WebP (Decision 19) |
| 3 | The frontmost app keeps focus while a card appears and is hovered | Type in TextEdit while a card appears; hover it |
| 4 | Drag-out works into Finder and into a browser upload | Drag a card; drag Drag all |
| 5 | Restore puts the original back | Restore a file job (compare checksums) and a clipboard job (paste) |
| 6 | Paused skips exactly one copy | ⌃⇧P to Paused; copy twice; only the second makes a card |
| 7 | Copying from a password manager is skipped | Copy a password from 1Password or Keychain Access; no card appears; a hotkey shows the "Private clipboard content" card |
| 8 | The stack follows the cursor between monitors | Only if two monitors are connected; otherwise record "not available" |
| 9 | Cards are hidden from screenshots unless the setting is on | Take a screenshot with a card showing; toggle the tray item; take it again |

Also check the M2b behaviours the spec names outside §8:
- Copy the same image twice quickly: two cards appear and neither overwrites the other's file.
- Start a large JPEG job, then copy text before it finishes: the text stays on the clipboard (Decision 17).
- Copy a screenshot, wait for Done, then press ⌃⇧A: the clipboard holds the aggressive result (the guard lets a hotkey replace Ferry's own result).
- Copy two image files in Finder and press ⌃⇧C: one "More than one file copied" card.
- Retry, Try aggressive and Re-optimise each show the Optimising ring, then a final state.
- Cancel while optimising removes the card, and the file is unchanged.
- The Error card's Details reveals the newest `ferry.<date>.log`.
- Recent results in the tray re-shows a card.

- [ ] **Step 2: Push and confirm CI**

```bash
git push
~/.local/bin/gh run watch
```

Expected: the core jobs (Ubuntu, macOS, Windows) and the macOS bundle job pass. Check the Windows bundle job too, without blocking on it. Note in the PR that Windows desktop behaviour is built but not verified on a real desktop (§1).

- [ ] **Step 3: Record the results**

Put the checklist table, with a result per row and the timings from check 2, into the PR description, or into the merge commit if merging locally.

---

## M2c — what is left

| Item | Spec |
| --- | --- |
| Settings window panes (General, Clipboard, Images, Floating results) with search, the template live preview, "Turn on '…' first" dependents | §3, §7 |
| Launch at login (`autostart` plugin) and `show_dock_icon` | §3, §4.3, §5.7 |
| Ignored-apps picker (`dialog` plugin) | §4.3 |
| Startup `Backups::prune(retention_days)` | §3, §5.7 |
| Force clean working directory, and its tray item | §3, §5.6 |
| Tray styles: default, classic, flat | §5.6 |
| macOS `NSColor.controlAccentColor` passed to the UI as `--accent` | §7 |
| Hotkey editor | Out of scope this cycle (§3) |
