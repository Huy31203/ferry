# Stream I: clipboard fixes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Results can be pasted together (accumulate), pasted as files and paths instead of image data, and copied automatically after a drop or a watched-folder job. Eight deferred Cycle 2 minors are fixed and one is dropped.

**Architecture:** A new pure module `ferry_core::clip` decides *what* goes on the clipboard: an image, files, or files with their paths as text. It also keeps the accumulated collection, keyed by job so it follows renames and conversions. The app's `jobs.rs` routes every result through one helper, `put_result`. That helper holds the existing `ClipboardLog` lock from the platform write until the new sequence number is recorded, which is the existing loop prevention. The platform layer gains `write_paths` on macOS (one `NSPasteboardItem` per file) and Windows (`CF_HDROP` + `CF_UNICODETEXT`). Auto-copy is a new entry point, `jobs::start_auto_copied`, for streams C and D. It captures the clipboard's sequence number when the job is queued, so the user's own later copy always wins.

**Tech Stack:** Rust 2024 (stable), Tauri 2.12, objc2 0.6 / objc2-app-kit 0.3.2 (macOS), clipboard-win 5.4.1 (Windows), Svelte 5 + TypeScript, vitest.

**Spec:** [Cycle 3 stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding; it wins over the requirements), [requirements](../../requirements/ferry-optimize-requirements.md) R§4 "Clipboard output", R§3 "No self-loops", [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md) §4.4, §5.4, §5.5, [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) §4.2, §4.3, §4.5.

## Global Constraints

- **Out of scope:** secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP automation.
- **No new dependencies:** no crate, npm package or binary. Everything used is already in `Cargo.lock` / `pnpm-lock.yaml`: `clipboard-win 5.4.1` (BSL-1.0) for `raw::set_file_list`, `raw::set_string_with`, `options::NoClear` and `get_clipboard_string`, and `objc2-app-kit 0.3.2` (Zlib OR Apache-2.0 OR MIT, feature `NSPasteboardItem` already enabled) for `NSPasteboardItem`. `Cargo.lock` must not change.
- **Shared files** (brief "Shared files and merge rules"): additions go at the named anchors listed below, each marked with a `sI` comment where the language allows one. Never reorder or reformat existing lines.
- **Settings:** one new section `[clip]` (`struct Clip`, `#[serde(default)]`). `settings::VERSION` stays `2`.
- **New settings key:** `clip.auto_copy: bool`, default `false`.
- **i18n:** new strings go under `clip.*` in `src/lib/i18n/en.json`, in one block inserted directly after the line `"clipboard.pick_app": "Choose an app to ignore",`. The first task that adds a `clip.*` key creates the block; later tasks append to its end. Keys: `clip.auto_copy`, `clip.auto_copy.help`, `clip.error.cmyk`. Two existing *values* change in place: `clipboard.accumulate.help` and `clipboard.copy_paths.help`. These are not new strings.
- **Loop prevention (R§3 "No self-loops"):** every clipboard write holds `AppState.clipboard` (the `ClipboardLog` mutex) from before the platform write until its sequence number is recorded. The watcher (`clipboard.rs` `on_change`) drops any sequence number the log holds.
- **Never overwrite a newer user copy:** a result tied to a copy or a queue time (`source: Some(seq)`) is written only when `ClipboardLog::may_replace(seq, live)` allows it.
- **Error text:** one line that says what to do (C1§6).
- **Windows code:** everything in `src-tauri/src/platform/windows_clipboard.rs` is compiled only on Windows (`#[cfg(windows)]` in `platform/mod.rs`). It **cannot be compiled or run on the Mac**. CI's `bundle (windows)` job checks it with `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p ferry`. Write it exactly as given, and say in the commit message that it is CI-verified only.
- **JavaScript/TypeScript:** never use `var`.
- **Live app:** ask the user before running `pnpm tauri dev`, because it takes over the real clipboard. Use a TextEdit window as the focus target and don't click into other apps. Every "manual check" step is run by, or with the permission of, the user.
- **Git:** never push, push tags, merge, or delete branches or worktrees without asking. End each commit message with the trailer your session's instructions give (the handover rule is `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).
- **Environment**, run from the worktree root before any command:
  ```bash
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  ```
  A new worktree needs the git-ignored `src-tauri/binaries/` copied from the main checkout. `cargo test` takes one name filter per command. The app crate's tests need a built frontend: run `pnpm build` once before `cargo test -p ferry` or `cargo clippy --workspace`.

### Shared-file edits (all of them)

| File | Anchor | Edit |
| --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | after `pub mod cancel;` | `pub mod clip;` (Task 1) |
| `crates/ferry-core/src/settings.rs` | after `pub hotkeys: Hotkeys,` in `struct Settings`; after `hotkeys: Hotkeys::default(),` in `impl Default for Settings`; new `struct Clip` immediately before `/// How \`Settings::load\` found the file.`; one test appended at the end of `mod tests` | Task 1 |
| `src-tauri/src/state.rs` | `ClipboardLog`: field `accumulated` replaced by `collected`, plus `last_write`. `Entry`: new last field `auto_copy`. New items after `fn output_busy`. Tests appended at the end of `mod tests` | Tasks 3, 4, 11 |
| `src-tauri/src/commands.rs` | body of `fn clear_all` (`.accumulated` → `.collected`) | Task 3 |
| `src-tauri/src/lib.rs` | one line immediately before `.invoke_handler(tauri::generate_handler![` | Task 10 |
| `src-tauri/src/platform/mod.rs` | module doc comment, lines 1–3 (names `write_paths`) | Task 2 |
| `src/lib/i18n/en.json` | `clip.*` block after `"clipboard.pick_app"`; values of `clipboard.accumulate.help`, `clipboard.copy_paths.help` | Tasks 5, 7 |
| `src/stack/Stack.svelte` | comment lines 86–87 only | Task 13 |

Files that are not on the brief's list but that other streams may also touch are edited at named anchors too: `src/lib/schema.ts` (after the `copy_paths` row), `src/lib/settings.ts` and `src/lib/settings.fixture.ts` (after the `hotkeys` line), and `src-tauri/src/jobs.rs` (new functions grouped under a `// ---- sI clipboard-fixes ----` comment at the end of the file, before `fn entry_name`).

### Cross-stream contract (produced by this stream)

- **Auto-copy entry point, for C (watchers) and D (drop zone):** `crate::jobs::start_auto_copied(app: &AppHandle, job: Job, level: Level) -> bool`. It behaves exactly like `jobs::start(app, job, level, None)`, and when `clip.auto_copy` is on it also puts the finished result on the clipboard (Task 4). C and D call it in place of `jobs::start` for jobs that come from a drop or a watched folder. Until they merge it carries `#[allow(dead_code, reason = "called by streams C and D")]`. Remove the allow once they call it.

## Review Focus

1. **Pasting files-plus-text into chat apps and browsers.** With "Copy file paths" on, the clipboard holds file references *and* text paths. A paste into Slack, Mail or a browser upload should attach the file, not type the path. Tested by: Task 3 manual check, step 1.
2. **A result renamed or converted while it is on the clipboard.** Pasting afterwards gives the renamed or converted file, never a missing one. Tested by: Task 1 tests `put_replaces_a_jobs_path_in_place` and `a_rename_follows_into_the_written_files`, Task 3 test `a_rewrite_keeps_which_copy_is_shown`, and Task 3 manual check, step 3.
3. **An optimise hotkey pressed while Ferry's own file paths are on the clipboard.** It re-runs those cards and does not start a second file job on Ferry's working file. Tested by: Task 3 test `own_paths_map_back_to_their_jobs`.
4. **An auto-copied result that finishes after the user copied something else.** The user's copy stays. Tested by: Task 4 test `an_auto_copy_never_replaces_a_later_copy`.
5. **An accumulated file deleted by Force clean or by hand.** It is left out, and an empty file list is never written. Tested by: Task 1 tests `paths_skip_files_that_are_gone` and `an_accumulated_plan_always_holds_the_new_file`.

## Deferred minors: verification at HEAD (`af75888`)

| # | Minor | Real? | Evidence | Task |
| --- | --- | --- | --- | --- |
| 1 | GRAY ICC kept on RGB JPEG output | Yes | `jpeg.rs:213` keeps the ICC profile and `:215-221` expands grey to RGBA. `raster.rs:24-38` keeps `icc`. `jpeg.rs:264` always encodes `JCS_RGB` and `:270-274` writes `r.icc`. A `GRAY` profile ends up on a 3-channel JPEG, and the same happens for the WebP and palette-PNG raster outputs. | 6 |
| 2 | CMYK JPEG fails with bare "CMYK JPEG" | Yes | `jpeg.rs:126` and `:223` return `Unsupported("CMYK JPEG")`. `pipeline.rs:905` shows an `Unsupported` message on the card verbatim. | 7 |
| 3 | Downscale after Exact crop drops the crop | Yes | `edit/mod.rs:126-131`: setting `scale` sets `size = None`, which also clears `Size::Exact`. `geometry.rs:28-33` takes the crop ratio from that Exact size, so the crop is lost. | 8 |
| 4 | Second Cmd+D in compare window needs hover to close | Yes | `Compare.svelte:71-76` closes only on Escape or Space. ⌘D reaches Rust only as a hover hotkey (`hover_keys.rs:30-33`), which is held only while an idle card is hovered (`plan`, `:62-65`). With the pointer in the compare window, ⌘D does nothing. | 9 |
| 5 | Space opens preview while typing in Settings | Yes | Hover keys are system-wide shortcuts. `hover_keys.rs:148` acts on Preview and Compare *before* the `app_is_active` guard at `:159`. A Space typed into a Settings field while the pointer rests on a card is taken and opens a preview. Digits are taken and dropped. | 10 |
| 6 | Stale comments `Stack.svelte:86-87`, `compare.rs:122` | Yes | `Stack.svelte:86-87` says "M3c's open_compare and quick_preview reject until M3c is merged", but M3c is merged (`lib.rs:40-44`). `compare.rs:121-122` says "`Entry::output` does not [follow a conversion or rename]", but `jobs.rs:107,137,389` call `entry.set_output` after rename, restore and run. | 13 |
| 7 | Queued job cancelled then re-edited can run twice (stale cancel token) | Yes | `jobs.rs:334` reads the token *after* waiting for a permit. `requeue` (`jobs.rs:121`) swaps in a new token, so a cancelled queued task reads the new token and runs beside the new request's own task. Capturing the token early is not enough on its own: the stale task's `cancel_recipe()` (`pipeline.rs:424-429`) would then undo the new edit's recipe (`prior` is set again at `:413`). | 11 |
| 8 | Twin of a renamed parent keeps card name only, not the rename stem | Yes | `pipeline.rs:464` (`fork`) sets `stem: None` but copies `name`. `output::resolve` uses `stem`, else the source's stem (`output.rs:47-50,67`). When the parent's result is not at the source path (Same folder or Specific folder), the twin writes `shot.webp` under the card name "hero". | 12 |
| 9 | Copy/Copy path on a twin before first run points at the placeholder | **No, dropped** | Not reachable from the UI. `JobUpdate.output` is `None` while the file does not exist (`pipeline.rs:352`). The rail shows no buttons without an output (`cards.ts:25`). The menu enables Copy and Copy path only with an output (`menu.rs:122-129`). `copy_path` already refuses a missing file (`jobs.rs:227-230`). Task 3's rewritten `jobs::copy` adds an existence guard anyway, which also covers a file deleted behind a stale Recent-results card. | — |

---

### Task 1: `ferry_core::clip` and the `[clip]` settings section

**Files:**
- Create: `crates/ferry-core/src/clip.rs`
- Modify: `crates/ferry-core/src/lib.rs` (anchor: after `pub mod cancel;`)
- Modify: `crates/ferry-core/src/settings.rs` (anchors in the table above)
- Test: `crates/ferry-core/src/clip.rs` (`mod tests`), `crates/ferry-core/src/settings.rs` (end of `mod tests`)

**Interfaces:**
- Consumes: `ferry_core::job::{JobId, JobState, Source}`.
- Produces:
  ```rust
  // clip.rs
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum Produced { Image, File }           // a clipboard job's image result, or a file job's file

  #[derive(Debug, Clone, PartialEq, Eq)]
  pub enum ClipWrite { Image, Files(Vec<PathBuf>), Paths(Vec<PathBuf>) }
  impl ClipWrite {
      /// The same write with `old` replaced by `new`; `None` when `old` is not in it.
      pub fn renamed(&self, old: &Path, new: &Path) -> Option<ClipWrite>;
  }

  #[derive(Debug, Clone, Default, PartialEq, Eq)]
  pub struct Collected(Vec<(JobId, PathBuf)>);   // "accumulate results", oldest first
  impl Collected {
      pub fn put(&mut self, id: &JobId, path: &Path);     // replace this job's path in place, else append
      pub fn follow(&mut self, id: &JobId, path: &Path);  // replace only if the job is in it
      pub fn retain(&mut self, keep: impl Fn(&JobId) -> bool);
      pub fn clear(&mut self);
      pub fn paths(&self) -> Vec<PathBuf>;                // in order, only files that exist
  }

  /// What to write for a result at `file`. `collected` is `Some` when accumulating
  /// (the caller has already `put` this job into it).
  pub fn plan(copy_paths: bool, produced: Produced, file: &Path, collected: Option<&Collected>) -> ClipWrite;

  /// Whether the app must still put a finished run's result on the clipboard
  /// (the pipeline did not): see Task 4.
  pub fn copy_after_run(source: Source, state: &JobState, first_run: bool, accumulate: bool, auto_copy: bool) -> bool;

  // settings.rs
  pub struct Clip { pub auto_copy: bool, #[serde(flatten)] pub extra: Map<String, Value> }
  // Settings gains `pub clip: Clip`
  ```

`plan` rules:
- With `collected` `Some(c)`: take `c.paths()`, and fall back to `vec![file]` if that is empty. The result is `Paths(..)` when `copy_paths` is on, otherwise `Files(..)`.
- With `collected` `None`: `copy_paths` gives `Paths(vec![file])`. Otherwise `Produced::Image` gives `Image` and `Produced::File` gives `Files(vec![file])`.

`copy_after_run` rules:
- `Source::Clipboard`: true only when `first_run && accumulate` and the state is `AlreadyOptimal` or `Larger`. On a first run the pipeline delivers nothing for those states, but the result must still join the collection.
- `Source::File`: true when `auto_copy` and the state is `Done`, `AlreadyOptimal` or `Larger`.
- Everything else is false.

- [ ] **Step 1: Write the failing tests** in `clip.rs` `mod tests`. Use `tempfile::tempdir()` and real files for `paths()`. Ids come from `JobId::generate()`.
  - `put_replaces_a_jobs_path_in_place`: put a→`a.png`, b→`b.png`, then a→`a.webp`. `paths()` is `[a.webp, b.png]`, with every file created.
  - `follow_never_adds_a_job`: follow on an empty `Collected` leaves `paths()` empty. Follow a→`hero.png` after putting a→`a.png` gives `[hero.png]`.
  - `paths_skip_files_that_are_gone`: put two files, delete one, and `paths()` returns only the other.
  - `plan_without_accumulate`: `plan(false, Image, f, None) == ClipWrite::Image`. `plan(false, File, f, None) == Files(vec![f])`. `plan(true, Image, f, None) == Paths(vec![f])`.
  - `an_accumulated_plan_always_holds_the_new_file`: a collection whose files are all deleted, plus the existing `f`, gives `plan(false, Image, f, Some(&c)) == Files(vec![f])`. With two existing files and `copy_paths` on, it gives `Paths([a, b])` in order.
  - `a_rename_follows_into_the_written_files`: `Files([a, b]).renamed(a, h) == Some(Files([h, b]))`, `Paths([a]).renamed(x, h) == None`, `Image.renamed(a, h) == None`.
  - `only_unwritten_results_are_copied_after_a_run`: table over `(Source, JobState, first_run, accumulate, auto_copy) -> bool`. It covers: Clipboard AlreadyOptimal/Larger true only with `first_run && accumulate`. Clipboard Done false. File Done/AlreadyOptimal/Larger true only with `auto_copy`. File Error false. File Restored false.
  - In `settings.rs`: `the_clip_section_defaults_off_and_keeps_unknown_fields`. It asserts `!Settings::default().clip.auto_copy` and `Settings::default().version == 2`. Loading `{"clip": {"auto_copy": true, "later": 1}}` gives `auto_copy == true`. Saving and re-reading the JSON keeps `clip.later == 1`. A file with no `clip` key loads `Clip::default()`.

- [ ] **Step 2: Run the tests and see them fail**
  Run: `cargo test -p ferry-core clip`
  Expected: compile error, because `clip` and `Clip` do not exist yet.

- [ ] **Step 3: Implement** `clip.rs` as specified above, with a module doc that names stream I and R§4 "Clipboard output". Add `Clip` (`#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)] #[serde(default)]`) and the `Settings.clip` field and default at the anchors, under a `// ---- sI clipboard-fixes: [clip] ----` comment. `sanitised()` needs no change.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry-core clip`, then `cargo test -p ferry-core settings`
  Expected: PASS. Also run `cargo clippy -p ferry-core --all-targets -- -D warnings` and expect it clean.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/ferry-core/src/clip.rs crates/ferry-core/src/lib.rs crates/ferry-core/src/settings.rs
  git commit -m "feat(core): clipboard output plan, accumulated results by job, [clip] settings"
  ```

---

### Task 2: `write_paths` on both platforms; `write_image` writes the image only

**Files:**
- Modify: `src-tauri/src/platform/macos_clipboard.rs`
- Modify: `src-tauri/src/platform/windows_clipboard.rs` (**Windows only, not compilable on the Mac**)
- Modify: `src-tauri/src/platform/mod.rs` (doc comment, lines 1–3)
- Modify: `src-tauri/src/jobs.rs` (the two `write_image` call sites, so the crate still builds)

**Interfaces:**
- Produces, on both OSes:
  - `pub fn write_paths(paths: &[PathBuf]) -> Result<u64, String>`: file references plus each file's full path as plain text. It returns the new sequence number.
  - `pub fn write_image(format: Format, bytes: &[u8]) -> Result<u64, String>`: the `file: Option<&Path>` parameter is removed, because file references now come only from `write_files` and `write_paths`.
  - `write_files` and `write_text` are unchanged.

- [ ] **Step 1: Write the failing macOS test** in `macos_clipboard.rs` `mod tests`:
  ```rust
  #[test]
  fn paths_go_on_as_files_with_their_text() {
      let pb = Private::new();
      let dir = std::env::temp_dir();
      let paths = vec![dir.join("ferry a.png"), dir.join("b.jpg")];
      let seq = write_paths_to(&pb.0, &paths).unwrap();
      assert_eq!(pb.0.changeCount() as u64, seq, "the watcher must recognise this write");
      let snap = read_from(&pb.0);
      assert_eq!(snap.files, paths);
      assert_eq!(snap.image, None);
      let texts: Vec<String> = pb.0.pasteboardItems().unwrap().iter()
          .filter_map(|item| item.stringForType(&NSString::from_str(TEXT)))
          .map(|s| s.to_string()).collect();
      let want: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
      assert_eq!(texts, want);
  }
  ```
  Delete the test `an_image_can_carry_its_working_file` and drop the `None` argument from the other `write_image_to` calls.

- [ ] **Step 2: Run it and see it fail**
  Run: `cargo test -p ferry platform::clipboard`
  Expected: compile error, because `write_paths_to` is not defined.

- [ ] **Step 3: Implement on macOS.** `write_paths(paths)` calls `write_paths_to(&NSPasteboard::generalPasteboard(), paths)`. `write_paths_to` builds one `objc2_app_kit::NSPasteboardItem::new()` per path, sets `FILE_URL` to `NSURL::from_file_path(p)?.absoluteString()` and `TEXT` to `p.to_string_lossy()` with `setString_forType`, and fails with `"not an absolute path"` if any URL is missing. Then `let seq = pb.clearContents();` and `pb.writeObjects(&NSArray::from_retained_slice(&items))` with each item wrapped by `ProtocolObject::from_retained`. A refused write returns `Err("the clipboard refused the files".into())`. Return `seq as u64`. In `write_image_to`, remove the `file` parameter and the `if let Some(file)` block.

- [ ] **Step 4: Implement on Windows (CI-only).** Add `options` to the `clipboard_win` import (`use clipboard_win::{Clipboard, Monitor, formats, options, raw};`). In `write_image`, remove the `file` parameter and its `if let Some(file)` block. Add:
  ```rust
  /// Put files on the clipboard with their full paths as text, one per line
  /// ("Copy file paths"). Returns the new sequence number.
  pub fn write_paths(paths: &[PathBuf]) -> Result<u64, String> {
      let names: Vec<String> = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
      let text = names.join("\r\n");
      {
          let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
          raw::empty().map_err(|e| e.to_string())?;
          raw::set_file_list(&names).map_err(|e| e.to_string())?; // does not empty the clipboard
          raw::set_string_with(&text, options::NoClear).map_err(|e| e.to_string())?;
      } // closing the clipboard publishes the change
      Ok(seq())
  }
  ```
  Add this test to the Windows `mod tests` (it runs in CI's `bundle (windows)` job):
  ```rust
  #[test]
  fn paths_go_on_as_files_and_as_text() {
      let _one = CLIPBOARD.lock().unwrap();
      let dir = std::env::temp_dir();
      let paths = vec![dir.join("ferry a.png"), dir.join("b.jpg")];
      write_paths(&paths).unwrap();
      assert_eq!(read().unwrap().files, paths);
      let want = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect::<Vec<_>>().join("\r\n");
      assert_eq!(clipboard_win::get_clipboard_string().unwrap(), want);
  }
  ```

- [ ] **Step 5: Keep the app building.** In `platform/mod.rs`, change the doc to name `read`, `write_image`, `write_files`, `write_paths`, `write_text` and `watch`. In `jobs.rs`, change both `write_image(.., file)` calls to `write_image(format, bytes)` (in `copy` and `deliver`), and delete the locals that become unused (`copy_paths` and `file` in `copy`), or clippy's `-D warnings` fails. Task 3 rewires "copy file paths".

- [ ] **Step 6: Run the tests and see them pass (macOS)**
  Run: `pnpm build`, then `cargo test -p ferry platform::clipboard`, then `cargo clippy --workspace --all-targets -- -D warnings`
  Expected: PASS and clean. The Windows half is verified only by CI.

- [ ] **Step 7: Commit**
  ```bash
  git add src-tauri/src/platform/ src-tauri/src/jobs.rs
  git commit -m "feat(app): write file paths with their text to the clipboard on macOS and Windows" -m "Windows half verified by CI only."
  ```

---

### Task 3: Every result reaches the clipboard through one plan

**Files:**
- Modify: `src-tauri/src/state.rs` (`ClipboardLog`; new fn `jobs_with_outputs` after `fn output_busy`; tests)
- Modify: `src-tauri/src/jobs.rs` (`deliver` becomes `put_result`; `copy`, `rename`, `restore`, `run`, `force_clean`)
- Modify: `src-tauri/src/commands.rs` (`clear_all` body)
- Modify: `src-tauri/src/clipboard.rs` (`optimise_now`)

**Interfaces:**
- Consumes: Task 1 `clip::{Collected, ClipWrite, Produced, plan}` and Task 2 `platform::clipboard::{write_image, write_files, write_paths}`.
- Produces:
  ```rust
  // state.rs: ClipboardLog
  pub collected: Collected,                    // replaces `pub accumulated: Vec<PathBuf>`
  last_write: Option<(u64, ClipWrite)>,        // private
  pub fn record(&mut self, seq: u64, from: Option<u64>);       // unchanged, and now also clears last_write
  pub fn wrote(&mut self, seq: u64, from: Option<u64>, write: ClipWrite); // record + last_write
  pub fn rewrote(&mut self, seq: u64, write: ClipWrite);       // own seq + last_write; `shown` kept
  pub fn last_write(&self) -> Option<&(u64, ClipWrite)>;
  // state.rs: free fn
  /// The jobs whose current files are `paths`, in order; `None` unless every path has one.
  pub fn jobs_with_outputs(jobs: &HashMap<JobId, Arc<Entry>>, paths: &[PathBuf]) -> Option<Vec<JobId>>;
  // jobs.rs (private, used by Task 4)
  fn put_result(app: &AppHandle, id: &JobId, image: Option<(Format, &[u8])>, file: &Path, source: Option<u64>) -> Result<(), String>;
  fn write_clipboard(write: &ClipWrite, image: Option<(Format, &[u8])>) -> Result<u64, String>;
  ```

What `put_result` does:
1. Read `settings.clipboard` (`accumulate`, `copy_paths`), then lock `state.clipboard` and keep it locked to the end.
2. If `accumulate`, call `log.collected.put(id, file)`. A result joins the collection even when it is not written now.
3. If `source` is `Some(s)` and `!log.may_replace(s, platform::clipboard::seq())`, log at info level and return `Ok(())`.
4. Compute `write = clip::plan(copy_paths, if image.is_some() { Image } else { File }, file, accumulate.then_some(&log.collected))`, then `seq = write_clipboard(&write, image)?`, then `log.wrote(seq, source, write)`.

`write_clipboard` maps `Image` to `write_image` (`Err("no image to write")` if `image` is `None`), `Files` to `write_files` and `Paths` to `write_paths`.

- [ ] **Step 1: Write the failing tests** in `state.rs` `mod tests`:
  - `a_files_write_is_remembered_until_the_next_plain_write`: after `wrote(5, None, Files(vec![a]))`, `last_write() == Some(&(5, Files(vec![a])))` and `is_own(5)`. After `record(6, None)`, `last_write() == None`.
  - `a_rewrite_keeps_which_copy_is_shown`: `wrote(3, Some(2), Files([a]))`, then `rewrote(4, Files([h]))`. Then `is_own(4)`, `may_replace(2, 4)` is true (the result of copy 2 may still replace Ferry's rewrite of it), and `last_write().unwrap().0 == 4`.
  - `own_paths_map_back_to_their_jobs`: using the existing `entry()` helper, `/w/a.png` (Done) and `/w/b.png` (Done) in a map. `jobs_with_outputs(&jobs, &[a, b])` is `Some([id_a, id_b])`. `&[a, "/w/x.png"]` gives `None`. `&[]` gives `None`.

- [ ] **Step 2: Run the tests and see them fail**
  Run: `cargo test -p ferry state::tests`
  Expected: compile error, because `wrote`, `rewrote`, `last_write` and `jobs_with_outputs` are not defined.

- [ ] **Step 3: Implement `ClipboardLog`** as specified. Update its doc comment ("…and the results 'accumulate results' has collected, by job, since launch or the last 'Clear all'"). Replace `.accumulated.clear()` with `.collected.clear()` in `commands.rs` `clear_all`. In `jobs::force_clean`, after `jobs.retain(..)`, keep the busy ids (`jobs.keys().cloned().collect::<Vec<_>>()`) and use `.collected.retain(|id| ids.contains(id))`.

- [ ] **Step 4: Route results through `put_result`.** Replace `fn deliver` with `put_result` and `write_clipboard` under the `// ---- sI clipboard-fixes ----` comment. In `run`, capture `let id = job.id.clone();` before `job.run`, and pass `&mut |format, bytes, file| put_result(app, &id, Some((format, bytes)), file, source)`. In `restore`, do the same with `source` `None`.

- [ ] **Step 5: Rewrite `jobs::copy`** (a card's Copy, which never accumulates):
  - If `!job.output.exists()`, return `Err(t("compare.file_gone"))`. This is the guard noted for dropped minor 9.
  - Clipboard job: `plan(copy_paths, Produced::Image, &job.output, None)`. Read the bytes only when the plan is `Image`.
  - File job: `plan(copy_paths, Produced::File, &job.output, None)`.
  - Write with `write_clipboard` and finish with `log.wrote(seq, None, write)`.

- [ ] **Step 6: Make rename follow the clipboard.** In `jobs::rename`, keep `let old = job.output.clone();` before `job.rename(stem)`. After it succeeds, and while still inside the job-lock block, lock `state.clipboard` and call `log.collected.follow(id, &job.output)`. Then, if `log.last_write()` is `Some((seq, write))` with `*seq == platform::clipboard::seq()` and `write.renamed(&old, &job.output)` is `Some(next)`, write `next` with `write_clipboard(&next, None)` and call `log.rewrote(new_seq, next)`. A failed rewrite is only `warn!`ed; the rename still succeeds.

- [ ] **Step 7: Make the hotkey use Ferry's own results.** In `clipboard::optimise_now`, after reading `snap`, check two things before `trigger::hotkey`. If `!snap.concealed` and `state.clipboard.lock().unwrap().is_own(snap.seq)` and `state::jobs_with_outputs(&state.jobs.lock().unwrap(), &snap.files)` is `Some(ids)`, then call `jobs::rerun(app, &id, level == Level::Aggressive)` for each id, pass any `Err(reason)` to `jobs::notice`, and return. Take the two locks one after the other, never both at once.

- [ ] **Step 8: Run the tests and see them pass**
  Run: `cargo test -p ferry state::tests`, then `cargo clippy --workspace --all-targets -- -D warnings`
  Expected: PASS and clean.

- [ ] **Step 9: Manual check on this Mac** (ask the user first; `pnpm tauri dev`; TextEdit is the focus target):
  1. Turn on Settings ▸ Clipboard ▸ "Copy file paths" and leave "Accumulate results" off. Take a screenshot to the clipboard (⌃⇧⌘4) and wait for the card.
     - Paste in a Finder window: one file with the card's name appears.
     - Paste in TextEdit set to plain text (Format ▸ Make Plain Text): the full path of the working file appears.
     - Paste into a browser upload field or a chat app: the file is attached, not the path typed.
  2. Turn "Copy file paths" off and "Accumulate results" on. Take three screenshots and paste in Finder: three files appear. Click **Clear all**, take one more, and paste: one file appears.
  3. With "Accumulate results" on, click **WebP** on one card's format bar, then paste in Finder: the `.webp` appears with no "item can't be found" alert. Rename another card to `hero` and paste: `hero.png` appears.
  4. With "Copy file paths" on, take a screenshot and press ⌃⇧A: the *same* card re-runs with "aggressive" in its detail line, and no second card appears.
  5. In every case, no extra card appears after Ferry's own writes (no self-trigger).

  Record each result as pass or fail in the commit message.

- [ ] **Step 10: Commit**
  ```bash
  git add src-tauri/src/state.rs src-tauri/src/jobs.rs src-tauri/src/commands.rs src-tauri/src/clipboard.rs
  git commit -m "feat(app): accumulate and copy-paths results follow renames, conversions and the hotkey"
  ```

---

### Task 4: Auto-copy after drop or watched-folder optimisation

**Files:**
- Modify: `src-tauri/src/state.rs` (`Entry` gains field `auto_copy`; tests)
- Modify: `src-tauri/src/jobs.rs` (`start` delegates to `start_entry`; `start_auto_copied`; post-run hook in `run`)

**Interfaces:**
- Consumes: Task 1 `clip::copy_after_run`, Task 3 `put_result`, and `Settings.clip.auto_copy`.
- Produces:
  ```rust
  // state.rs: Entry (new last field, initialised to None in Entry::new)
  auto_copy: Mutex<Option<u64>>,
  pub fn set_auto_copy(&self, seq: u64);
  pub fn take_auto_copy(&self) -> Option<u64>;  // Some once, then None
  // jobs.rs
  #[allow(dead_code, reason = "called by streams C and D")]
  pub fn start_auto_copied(app: &AppHandle, job: Job, level: Level) -> bool;
  fn start_entry(app: &AppHandle, job: Job, level: Level, source_seq: Option<u64>, auto_copy: Option<u64>) -> bool;
  fn copy_after_run(app: &AppHandle, entry: &Entry, state: &JobState, source: Option<u64>, auto_copy: Option<u64>);
  ```

**When it fires:**
- Only on a job's **first** run.
- `start_auto_copied` reads `clip.auto_copy` when it queues the job, and stores `Some(platform::clipboard::seq())` on the entry.
- When the first run finishes, `run` takes `entry.take_auto_copy()` and asks `clip::copy_after_run(job.source, &state, source.is_some(), settings.clipboard.accumulate, auto.is_some())`.
- If the answer is yes, it calls `put_result(app, &id, None, &entry.output(), auto.or(source))`. With "accumulate results" on, the result joins the collection. With "copy file paths" on, the paths go on as text.
- Re-runs never auto-copy; a card's Copy button is still there.

**How it avoids self-triggering:**
- The write goes through `put_result`, which holds the `ClipboardLog` lock across write and record, so the watcher's `is_own` check drops it.
- A clipboard that holds only files never starts an automatic job anyway (`trigger::check_image` returns `NoImage` when `files` is non-empty).
- Auto-copy writes only the clipboard, never a folder, so no watched folder sees it.
- `may_replace(queued_seq, live)` means anything the user copied after the job was queued wins. Results from the same burst may replace each other, because each records `shown = Some(queued_seq)`.

- [ ] **Step 1: Write the failing tests** in `state.rs` `mod tests`:
  - `auto_copy_is_taken_once`: a new entry gives `take_auto_copy() == None`. After `set_auto_copy(9)`: `Some(9)`, then `None`.
  - `an_auto_copy_never_replaces_a_later_copy`: on a fresh log, a job queued at seq 4 with the clipboard still at 4 gives `may_replace(4, 4)`. After `wrote(5, Some(4), Files([a]))`, a second job queued at 4 gives `may_replace(4, 5)` true. With the user's copy at 6, `may_replace(4, 6)` is false.

- [ ] **Step 2: Run the tests and see them fail**
  Run: `cargo test -p ferry state::tests`
  Expected: compile error, because `set_auto_copy` is not defined.

- [ ] **Step 3: Implement** the `Entry` field and methods. Rename `start`'s body to `start_entry`, which also calls `entry.set_auto_copy(seq)` when `auto_copy` is `Some(seq)`. `start` keeps its signature and calls `start_entry(app, job, level, source_seq, None)`. Add `start_auto_copied` and `copy_after_run` under the `sI` comment. In `run`, take `let auto = entry.take_auto_copy();` next to `source`. In the `Some(f)` arm, clone `f.update.state` before `finish(..)` and call `copy_after_run(app, entry, &state, source, auto)` after it. A failed write is only `warn!`ed.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry state::tests`, then `cargo clippy --workspace --all-targets -- -D warnings`
  Expected: PASS and clean.

- [ ] **Step 5: Manual check, deferred to `feat/c3-all` after streams C and D merge.** Write this list into the commit body so the integrator runs it.
  1. Turn on Settings ▸ Clipboard ▸ "Copy results of drops and watched folders". Drop a PNG on the drop zone. When the card is Done, paste in Finder: the result file appears.
  2. Drop a large file and copy some text in TextEdit while it optimises. After it finishes, paste in TextEdit: your text is still there.
  3. Save an image into a watched folder and paste in Finder: the result appears, and no second card appears for Ferry's own write.

  Also check that streams C and D call `jobs::start_auto_copied`, and then remove its `#[allow(dead_code)]`.

- [ ] **Step 6: Commit**
  ```bash
  git add src-tauri/src/state.rs src-tauri/src/jobs.rs
  git commit -m "feat(app): auto-copy the results of drops and watched folders (jobs::start_auto_copied)"
  ```

---

### Task 5: Clipboard pane rows and strings

**Files:**
- Modify: `src/lib/schema.ts` (new row after the `copy_paths` row; remove `depends_on` from `accumulate` and `copy_paths`)
- Modify: `src/lib/settings.ts` (anchor: after the `hotkeys:` line of `interface Settings`)
- Modify: `src/lib/settings.fixture.ts` (anchor: after the `hotkeys:` line)
- Modify: `src/lib/i18n/en.json`
- Test: `src/lib/schema.test.ts`

**Interfaces:**
- Consumes: Task 1 `Settings.clip.auto_copy`.
- Produces: TS `Settings.clip: { auto_copy: boolean }`.

"Accumulate results" and "Copy file paths" now also shape hotkey and auto-copy results, which run with automatic clipboard work off. So they no longer depend on `clipboard.enabled`. "Copy results of drops and watched folders" does not depend on it either.

- [ ] **Step 1: Write the failing test** in `schema.test.ts`:
  ```ts
  it('has the clipboard output rows, none tied to automatic clipboard work', () => {
    const rows = ['accumulate', 'copy_paths', 'auto_copy'].map((id) => all.find((s) => s.id === id));
    expect(rows.map((s) => s?.path)).toEqual(['clipboard.accumulate', 'clipboard.copy_paths', 'clip.auto_copy']);
    expect(rows.map((s) => s?.depends_on)).toEqual([undefined, undefined, undefined]);
    expect(t('clip.auto_copy')).toBe('Copy results of drops and watched folders');
  });
  ```

- [ ] **Step 2: Run it and see it fail**
  Run: `pnpm test -- schema`
  Expected: FAIL (no `auto_copy` row; `pnpm check` would also flag the unknown key).

- [ ] **Step 3: Implement.** Add the row, preceded by the comment `// sI clipboard-fixes`:
  ```ts
  { id: 'auto_copy', pane: 'clipboard', label: 'clip.auto_copy', help: 'clip.auto_copy.help', path: 'clip.auto_copy', control: { kind: 'switch' } }
  ```
  Delete the two `depends_on: 'clipboard.enabled',` lines from the `accumulate` and `copy_paths` rows. Add `clip: { auto_copy: boolean };` to `Settings` and `clip: { auto_copy: false },` to `sample`. In `en.json`, create the `clip.*` block after `"clipboard.pick_app": "Choose an app to ignore",`:
  ```json
  "clip.auto_copy": "Copy results of drops and watched folders",
  "clip.auto_copy.help": "Put each result on the clipboard when it is ready, unless you have copied something since.",
  ```
  Change two existing values in place:
  - `clipboard.accumulate.help` → `"Put every result since launch or the last Clear all on the clipboard as files, to paste them together."`
  - `clipboard.copy_paths.help` → `"Put the result's file and its full path on the clipboard instead of the image."`

- [ ] **Step 4: Run the tests and see them pass**
  Run: `pnpm check`, then `pnpm test`, then `cargo test -p ferry-core i18n` (the Rust side `include_str!`s the same JSON)
  Expected: PASS.

- [ ] **Step 5: Commit**
  ```bash
  git add src/lib/schema.ts src/lib/schema.test.ts src/lib/settings.ts src/lib/settings.fixture.ts src/lib/i18n/en.json
  git commit -m "feat(settings): auto-copy toggle; accumulate and copy paths no longer wait on clipboard watching"
  ```

---

### Task 6: A grey ICC profile never rides on RGB output (minor 1)

**Files:**
- Modify: `crates/ferry-core/src/edit/raster.rs`
- Test: `crates/ferry-core/src/edit/raster.rs` `mod tests`

**Interfaces:**
- Produces: `fn rgb_profile(icc: Option<Vec<u8>>) -> Option<Vec<u8>>` (private). It drops a profile whose colour-space signature (bytes 16..20) is `b"GRAY"` and keeps everything else, including short or unknown profiles. `decode` applies it to every raster it returns, so `Raster.icc` always fits RGBA pixels. This covers the JPEG, WebP and palette-PNG raster outputs alike.

- [ ] **Step 1: Write the failing tests**
  - `a_grey_profile_is_dropped_once_the_pixels_are_rgb`:
    - Build a grey JPEG in the test with `mozjpeg::Compress::new(ColorSpace::JCS_GRAYSCALE)` (like `half_dark_jpeg`), writing one APP2 marker: `b"ICC_PROFILE\0"`, then `[1, 1]`, then a 128-byte profile with `profile[16..20] == *b"GRAY"`.
    - Assert `decode(&jpeg, Format::Jpeg).unwrap().icc == None`.
    - For each of PNG, JPEG and WebP, `decode(&encode(&r, target, &o).unwrap(), target).unwrap().icc == None`.
  - `an_rgb_profile_survives_decoding`: the same profile with `b"RGB "` at 16..20, put on `testutil::jpeg(32, 32, 90, &JpegExtras { icc: Some(<APP2 payload>), ..Default::default() })`. `decode(..).icc == Some(profile)`.

- [ ] **Step 2: Run them and see the first fail**
  Run: `cargo test -p ferry-core edit::raster`
  Expected: `a_grey_profile_is_dropped_once_the_pixels_are_rgb` FAILS (`icc` is `Some`).

- [ ] **Step 3: Implement** `rgb_profile` and apply it at the end of `decode`, after the `match`.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry-core edit::raster`
  Expected: PASS, including the existing `the_icc_profile_survives_every_target`.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/ferry-core/src/edit/raster.rs
  git commit -m "fix(core): a grey ICC profile is not carried onto RGB edit output"
  ```

---

### Task 7: CMYK JPEGs say what to do (minor 2)

**Files:**
- Modify: `crates/ferry-core/src/optimise/jpeg.rs` (lines 126 and 223)
- Modify: `src/lib/i18n/en.json` (append to the `clip.*` block)
- Test: `crates/ferry-core/src/optimise/jpeg.rs` `mod tests`

**Interfaces:**
- Produces: `OptimiseError::Unsupported(t("clip.error.cmyk"))` from both `optimise` and `decode_raster` for `JCS_CMYK` and `JCS_YCCK` input.
- New string: `"clip.error.cmyk": "CMYK JPEGs aren't supported yet — save as RGB first"`.

- [ ] **Step 1: Write the failing test** `a_cmyk_jpeg_says_what_to_do`:
  - Build the input with `Compress::new(ColorSpace::JCS_CMYK)`, size 16×16, quality 90, and `write_scanlines(&vec![0x40; 16 * 16 * 4])`.
  - With `want = crate::i18n::t("clip.error.cmyk")`, assert that `optimise(&input, &OptimiseOptions::default())` and `decode_raster(&input)` both give `Err(OptimiseError::Unsupported(m))` with `m == want`.
  - Assert `want == "CMYK JPEGs aren't supported yet — save as RGB first"`.

- [ ] **Step 2: Run it and see it fail**
  Run: `cargo test -p ferry-core optimise::jpeg`
  Expected: FAIL (the message is `"CMYK JPEG"`, and the key is missing).

- [ ] **Step 3: Implement.** Add the key to `en.json` at the end of the `clip.*` block, and use `crate::i18n::t("clip.error.cmyk")` at both sites.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry-core optimise::jpeg`, then `pnpm check`
  Expected: PASS.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/ferry-core/src/optimise/jpeg.rs src/lib/i18n/en.json
  git commit -m "fix(core): a CMYK JPEG's error card says to save it as RGB"
  ```

---

### Task 8: Downscale keeps an Exact crop (minor 3)

**Files:**
- Modify: `crates/ferry-core/src/edit/mod.rs` (`Edits::merge`, and the doc comments on `size` and `scale`)
- Modify: `crates/ferry-core/src/edit/geometry.rs` (`output_size`)
- Modify: `src/lib/edits.ts` (`scaleLabel`)
- Test: `edit/mod.rs`, `edit/geometry.rs`, `src/lib/edits.test.ts`

**Rule** (a deliberate change to C2§4.2's "setting one clears the other", required by the brief's minor):
- A scale applies on top of whatever the crop step produces. An `Exact { w, h }` size *is* a crop, so a later scale keeps it and scales `w × h`, clamped to the crop window as today, by the percent.
- Setting a scale still clears `Width`, `Height` and `LongEdge`.
- Setting any size still clears the scale.

Example: 2880×1800, Exact 1200×628, then 50 % gives 600×314.

- [ ] **Step 1: Write the failing tests**
  - `edit/mod.rs` `a_downscale_keeps_an_exact_crop`: merge `{"size": {"kind": "exact", "width": 1200, "height": 628}}`, then `{"scale": 50}`. `(edits.size, edits.scale) == (Some(Size::Exact { width: 1200, height: 628 }), Some(50))`. Then merging `{"size": {"kind": "width", "width": 800}}` gives `(Some(Size::Width { width: 800 }), None)`.
  - `edit/mod.rs` `an_exact_crop_then_downscale_comes_out_smaller`: `applied(&testutil::png(300, 200), Format::Png, |e| { e.size = Some(Size::Exact { width: 120, height: 60 }); e.scale = Some(50) })` has `a.dims == (60, 30)` and `dimensions(&a.bytes, Format::Png) == Some((60, 30))`.
  - `geometry.rs` `a_downscale_scales_an_exact_size`:
    - window `(2880, 1507)` with Exact 1200×628 and scale 50 gives `(600, 314)`;
    - window `(1000, 523)` with Exact 1200×628 and scale 50 gives `(500, 262)` (clamped to the window first).
  - `edits.test.ts`:
    - `scaleLabel(50, [2880, 1800], { ...edits, size: { kind: 'exact', width: 1200, height: 628 } }) === '50 % · 600×314'`;
    - `scaleLabel(50, [2880, 1800], { ...edits, size: { kind: 'exact', width: 5000, height: 2000 } }) === '50 % · 1440×576'`.

- [ ] **Step 2: Run them and see them fail**
  Run: `cargo test -p ferry-core edit::`, then `pnpm test -- edits`
  Expected: the four new tests FAIL.

- [ ] **Step 3: Implement**
  - `merge`: in the `scale` branch, clear `size` only when it is not `Some(Size::Exact { .. })`.
  - `output_size`: in the `Exact` arm, compute today's clamped size as `base`, then return `edits.scale.map_or(base, |p| (scaled(base.0, p.into(), 100), scaled(base.1, p.into(), 100)))`.
  - `scaleLabel`: when `edits.size?.kind === 'exact'` and `dims` is known, the base is `[w, h]` if `w < cw && h < ch`, else `[cw, ch]`, where `[cw, ch] = fitRatio(dims, [w, h])`. Label the scaled base. Every other case is unchanged.
  - Update the field docs: `size` reads "Width/Height/LongEdge and `scale` replace each other; an Exact size keeps a later scale", and `scale` reads "percent of the cropped original, or of an Exact size".

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry-core edit::`, then `pnpm test`, then `pnpm check`
  Expected: PASS, including the existing `size_and_scale_replace_each_other`.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/ferry-core/src/edit/mod.rs crates/ferry-core/src/edit/geometry.rs src/lib/edits.ts src/lib/edits.test.ts
  git commit -m "fix(core): downscaling after an exact crop keeps the crop and scales its size"
  ```

---

### Task 9: ⌘D / Ctrl+D closes the compare window from inside it (minor 4)

**Files:**
- Modify: `src/compare/view.ts`, `src/compare/Compare.svelte` (`onkeydown`, around line 71)
- Test: `src/compare/view.test.ts`

**Interfaces:**
- Produces: `export function closesCompare(event: Pick<KeyboardEvent, 'key' | 'metaKey' | 'ctrlKey'>, mac: boolean): boolean`. It is true for `Escape`, for `' '`, and for `d`/`D` with `metaKey` on macOS or `ctrlKey` on Windows.

- [ ] **Step 1: Write the failing test** `describe('closesCompare')`, with assertions:
  - true: Escape and `' '` on both OSes; `{ key: 'd', metaKey: true }` with `mac = true`; `{ key: 'D', ctrlKey: true }` with `mac = false`.
  - false: `{ key: 'd', ctrlKey: true }` with `mac = true`; `{ key: 'd', metaKey: true }` with `mac = false`; a plain `{ key: 'd' }`.

- [ ] **Step 2: Run it and see it fail**
  Run: `pnpm test -- view`
  Expected: FAIL (`closesCompare` is not exported).

- [ ] **Step 3: Implement** `closesCompare` and use it in `Compare.svelte`: `if (closesCompare(event, isMac)) { event.preventDefault(); close(); }`, importing `isMac` from `../lib/ipc`.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `pnpm test`, then `pnpm check`
  Expected: PASS.

- [ ] **Step 5: Manual check on this Mac** (with the user's OK): hover an idle card and press ⌘D, and the compare window opens. Move the pointer into the compare window, away from every card, and press ⌘D again. Look for: the window closes at once, without hovering a card, and focus returns to TextEdit. Esc and Space still close it.

- [ ] **Step 6: Commit**
  ```bash
  git add src/compare/view.ts src/compare/view.test.ts src/compare/Compare.svelte
  git commit -m "fix(ui): a second Cmd/Ctrl+D closes the compare window without hovering a card"
  ```

---

### Task 10: Hover keys stand down while a Ferry window takes typing (minor 5)

**Files:**
- Modify: `src-tauri/src/hover_keys.rs`
- Modify: `src-tauri/src/lib.rs` (shared; one line immediately before `.invoke_handler(tauri::generate_handler![`)
- Test: `src-tauri/src/hover_keys.rs` `mod tests`

**Interfaces:**
- Consumes: `crate::windows::STACK`, `crate::compare::COMPARE`.
- Produces:
  ```rust
  fn plan(held: bool, card: bool, enabled: bool, keyed: bool, typing: bool) -> Option<bool>; // want = card && enabled && !keyed && !typing
  fn typing_after(current: Option<String>, label: &str, focused: bool) -> Option<String>;
  pub fn window_focus(app: &AppHandle, label: &str, focused: bool); // updates Keys.typing, then apply()
  ```
  `Keys` gains `typing: Option<String>`, the label of the focused Ferry window that takes typing. The stack and the compare window never count. Settings counts, and so do later windows such as the batch window or the pipeline editor, with no extra code. `typing_after` ignores `STACK` and `COMPARE`. Otherwise focus gives `Some(label)`, and a blur of that same label gives `None`. A blur of any other label leaves `current` alone.

  lib.rs line: `.on_window_event(|window, event| if let tauri::WindowEvent::Focused(focused) = event { hover_keys::window_focus(window.app_handle(), window.label(), *focused) }) // sI: hover keys stand down while a Ferry window takes typing`. Window events arrive on the main thread, where shortcuts must be registered.

- [ ] **Step 1: Write the failing tests**
  - Update the existing `plan` tests to pass `typing: false`.
  - `a_typing_window_never_keeps_the_hover_keys`: `plan(true, true, true, false, true) == Some(false)` and `plan(false, true, true, false, true) == None`.
  - `focus_follows_the_typing_window`: `typing_after(None, "settings", true) == Some("settings")`. `typing_after(Some("settings"), STACK, true)` and `typing_after(Some("settings"), COMPARE, true)` both stay `Some("settings")`. `typing_after(Some("settings"), "settings", false) == None`. `typing_after(Some("batch"), "settings", false) == Some("batch")`. `typing_after(Some("settings"), "batch", true) == Some("batch")`.

- [ ] **Step 2: Run them and see them fail**
  Run: `cargo test -p ferry hover_keys`
  Expected: compile errors (arity, missing `typing_after`).

- [ ] **Step 3: Implement** as specified, and add the lib.rs line.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry hover_keys`, then `cargo clippy --workspace --all-targets -- -D warnings`
  Expected: PASS and clean.

- [ ] **Step 5: Manual check on this Mac** (with the user's OK):
  1. Open Settings ▸ Clipboard and click into "File name template". Rest the pointer on an idle card and type `a b 5`. Look for: the field shows `a b 5`, Quick Look does not open, and the card is not downscaled.
  2. Click TextEdit, rest the pointer on the card and press Space. Look for: Quick Look opens.

- [ ] **Step 6: Commit**
  ```bash
  git add src-tauri/src/hover_keys.rs src-tauri/src/lib.rs
  git commit -m "fix(app): Space and digits typed into Settings never reach a hovered card"
  ```

---

### Task 11: A cancelled queued request never runs and never undoes a newer one (minor 7)

**Files:**
- Modify: `crates/ferry-core/src/cancel.rs`
- Modify: `src-tauri/src/state.rs` (`Entry::start_check`, `enum Start`, after `fn output_busy`)
- Modify: `src-tauri/src/jobs.rs` (`spawn`, around line 327)
- Test: `cancel.rs`, `state.rs`

**Interfaces:**
- Produces:
  ```rust
  impl CancelToken { pub fn same(&self, other: &CancelToken) -> bool } // Arc::ptr_eq
  #[derive(Debug, PartialEq, Eq)]
  pub enum Start { Run, Drop { forget_recipe: bool } }
  impl Entry { pub fn start_check(&self, token: &CancelToken) -> Start }
  ```
  `start_check` gives:
  - `Drop { forget_recipe: false }` when `token` is not the entry's current token: a newer request replaced it, and that request owns the recipe.
  - `Drop { forget_recipe: true }` when the token is current but cancelled.
  - `Run` otherwise.

- [ ] **Step 1: Write the failing tests**
  - `cancel.rs` `same_is_identity_not_state`: a token and its clone are `same`. Two `CancelToken::new()` are not.
  - `state.rs` `a_replaced_request_neither_runs_nor_undoes_its_replacement`, using `entry("/w/a.png", JobState::Done)`:
    - take `first = e.cancel.lock().unwrap().clone()`; `first.cancel()`; then `e.start_check(&first) == Start::Drop { forget_recipe: true }`;
    - set `*e.cancel.lock().unwrap() = CancelToken::new()`, as `requeue` does; then `e.start_check(&first) == Start::Drop { forget_recipe: false }`;
    - the new token gives `Start::Run`.

- [ ] **Step 2: Run them and see them fail**
  Run: `cargo test -p ferry-core cancel`, then `cargo test -p ferry state::tests`
  Expected: compile errors.

- [ ] **Step 3: Implement.** In `jobs::spawn`, read `let cancel = entry.cancel.lock().unwrap().clone();` **before** `tauri::async_runtime::spawn` and move it into the task. After the permit, `match entry.start_check(&cancel)`: `Run` proceeds as today, and `Drop { forget_recipe }` calls `relock(&entry.job).cancel_recipe()` only when `forget_recipe` is true, then returns.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry-core cancel`, then `cargo test -p ferry state::tests`, then `cargo clippy --workspace --all-targets -- -D warnings`
  Expected: PASS and clean.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/ferry-core/src/cancel.rs src-tauri/src/state.rs src-tauri/src/jobs.rs
  git commit -m "fix(app): a cancelled queued edit never runs beside, or undoes, the edit that replaced it"
  ```

---

### Task 12: A twin keeps its parent's rename (minor 8)

**Files:**
- Modify: `crates/ferry-core/src/pipeline.rs` (`fork`, line 464)
- Test: `crates/ferry-core/src/pipeline.rs` `mod tests`

- [ ] **Step 1: Write the failing test** `a_twin_of_a_renamed_result_keeps_the_new_name`:
  - Settings: `images.output.optimised.location = Location::SameFolder`.
  - `path = env.file("shot.png", &testutil::png(300, 200))` and `job = Job::file(&path, &settings, true)`.
  - `job.edit(scale(50))`, then run with `options_with(&settings)`. The explicit edit writes `shot_optimised.png`.
  - `job.rename("hero")` produces `hero.png`.
  - `twin = job.fork()`, `twin.edit(convert(Format::WebP))`, then run with the same options.
  - Assert `update.output == Some(env.dir.path().join("hero.webp"))` and `update.name == "hero"`.

- [ ] **Step 2: Run it and see it fail**
  Run: `cargo test -p ferry-core a_twin_of_a_renamed_result_keeps_the_new_name`
  Expected: FAIL (the output is `shot.webp`).

- [ ] **Step 3: Implement:** `stem: self.stem.clone(),` in `fork`.

- [ ] **Step 4: Run the tests and see them pass**
  Run: `cargo test -p ferry-core pipeline`
  Expected: PASS, including every existing twin test.

- [ ] **Step 5: Commit**
  ```bash
  git add crates/ferry-core/src/pipeline.rs
  git commit -m "fix(core): a keep-both twin of a renamed result takes the new name"
  ```

---

### Task 13: Stale comments (minor 6)

**Files:**
- Modify: `src/stack/Stack.svelte:86-87` (shared; these two comment lines only)
- Modify: `src-tauri/src/compare.rs:121-122`

- [ ] **Step 1: Replace the comments**
  - `Stack.svelte` 86–87 become: `// A menu item or hover key that ends here. open_compare and quick_preview` / `// reject when the card can't be shown (file gone, still running); that is only logged.`
  - `compare.rs` 121–122 become: `/// The card's latest state (\`Entry::last\`): the window needs its sizes and` / `/// backup as well as its current file.`

- [ ] **Step 2: Verify nothing else changed**
  Run: `git diff --stat`, then `pnpm check`, then `cargo clippy --workspace --all-targets -- -D warnings`
  Expected: 2 files, 4 lines changed; clean.

- [ ] **Step 3: Commit**
  ```bash
  git add src/stack/Stack.svelte src-tauri/src/compare.rs
  git commit -m "docs: fix two stale comments about M3c and Entry::output"
  ```

---

### Task 14: Stream verification

- [ ] **Step 1: Run everything**
  ```bash
  cargo fmt --all --check
  pnpm build
  cargo clippy --workspace --all-targets -- -D warnings
  cargo test -p ferry-core
  cargo test -p ferry
  pnpm check
  pnpm test
  git diff main --stat -- Cargo.lock pnpm-lock.yaml   # expect no output: no new dependencies
  ```
  Expected: every command passes, and the last prints nothing.

- [ ] **Step 2: Confirm the manual checks.** Tasks 3, 9 and 10 have recorded results. Task 4's list is in its commit body, waiting for `feat/c3-all`. The Windows half of Task 2 waits on CI's `bundle (windows)` job, which needs a push, so ask the user.
