# Ferry Cycle 3, Stream C: Watchers Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ferry optimises new screenshots, new screen recordings and new files in folders the user watches. Screenshot and recording watching are off by default; the user turns them on in Settings. Each file is handled once it has finished writing. Ferry's own writes never trigger a watcher again. A "Watched folders" pane in Settings controls all of it.

**Architecture:** `ferry-core` gains a `watch` module. It contains a raw `notify` watcher thread, Ferry's own settle state machine (its own debouncer), a router from file to source and a process-wide *self-write ledger* fed by `fsutil`. Everything except the thread is pure and unit-tested with a fake clock and a fake file system. Live tests use real temp folders and the real pipeline. When a file is ready, the Tauri app turns it into an ordinary `Job::file` and starts it through the existing `jobs::start`. Video, audio and PDF go through named hooks that return "unsupported" until streams A, B and E land.

**Tech Stack:** Rust 2024 (stable), `notify` 8.2.0, Tauri 2.12, objc2 0.6 / objc2-foundation 0.3.2 (macOS), windows-sys 0.61 (Windows), Svelte 5 + TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-ferry-cycle3-streams.md` (binding, stream C), with `docs/requirements/ferry-optimize-requirements.md` (FR-T4, FR-T5, R§2.4 "Settle before work", R§3.3 "No self-loops", "Watcher resilience", R§4.12 "Watched folders", R§5 table rows "Screenshot and recording detection" and "Folder watching and settle detection"), `docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md` (job lifecycle §5.4, run state §5.1) and `docs/superpowers/specs/2026-10-22-ferry-cycle2-card-tools-design.md` (output locations §4.4).

---

## Global Constraints

- The workspace builds with Rust stable and edition 2024. These must all pass on this Mac before each commit: `cargo fmt --all --check`, `cargo clippy -p ferry-core --all-targets -- -D warnings`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ferry-core`, `cargo test -p ferry`, `pnpm check`, `pnpm test`.
- Licence: the project is GPL-3.0-or-later. A new dependency must be GPL-compatible. This plan adds one crate, `notify` 8.2.0 (CC0-1.0), and no npm packages.
- `ferry-core` takes no Tauri or OS UI dependencies (crate doc, `crates/ferry-core/src/lib.rs`).
- Targets are macOS 13+ (this dev Mac is Intel, macOS 14.6) and Windows 10 22H2 / 11 on x64. Windows ARM64 is out of scope.
- **Out of scope:** secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP, pipelines (stream F), video/audio/PDF encoding (streams A, B, E).
- Windows-only code sits behind `#[cfg(windows)]`. It is built and tested only by CI (`windows-latest`). Status reports must say: "Windows watcher behaviour built and CI-tested, not verified on a real desktop."
- JavaScript/TypeScript never uses `var`. Use `const`, or `let` when a value is reassigned.
- Every new UI or card string goes in `src/lib/i18n/en.json` under the `watch.*` prefix. Rust reads them with `ferry_core::i18n::{t, tf}`; Svelte reads them with `t` from `src/lib/i18n.ts`.
- Settings: one new `[watch]` section. Every struct has `#[serde(default)]` and `#[serde(flatten)] extra: Map<String, Value>`. `settings::VERSION` stays `2`.
- Shared files: make additions at the named anchors listed under "Shared-file edits". Never reorder or reformat existing lines.
- Original is sacred. A watched file goes through the existing flow (`Job::file` → `jobs::start` → backup → optimise → `fsutil::atomic_replace`). No watcher code writes to user files.
- Run state (C1 §5.1): Stopped means no watcher work. Paused skips exactly one automatic trigger and then returns to Running. The pause is shared with the clipboard watcher (`RunState::take_automatic`).
- NFR-R1: idle CPU stays under 1%. With nothing pending, the watcher thread wakes only for the 30 s rescan.
- NFR-P4: a file-system probe that may touch a slow, sleeping or network drive gives up after 5 s (`fsutil::XATTR_TIMEOUT` value, `Duration::from_secs(5)`).
- R§3.3 "Fail visibly": watcher skips (unsupported, ignored, too large) are logged and show no card. A job that starts and then fails always shows its Error card, even when the folder's card setting is off.
- Logs never contain file contents (C1 §6).
- Commit messages follow the repo style, e.g. `feat(core): …`, `feat(app): …`, `feat(ui): …`, `test(core): …`.

## Review Focus

These five inputs are the ones most likely to bite a real user. Each has a test in the task named after it.

1. **macOS screenshot names with spaces and U+202F** (macOS 13+ writes `Screenshot 2026-10-23 at 10.21.33 AM.png` with a narrow no-break space before "AM"). Ferry must handle such a file exactly once and match it in the ledger. Tests: Task 2 `keys_survive_spaces_and_narrow_nbsp`, Task 5 `a_screenshot_name_with_narrow_nbsp_is_ready`.
2. **A folder listed twice under different spellings** (`~/Downloads` and `/Users/me/Downloads`). The file gets one job, not two. Test: Task 5 `a_folder_listed_twice_reports_a_file_once`.
3. **A watched folder removed or paused in Settings while a file in it is still settling.** That file must not start a job. Test: Task 5 `a_file_pending_when_its_folder_is_removed_is_dropped`.
4. **Something touches a Ferry output's dates or tags after Ferry wrote it** (Spotlight, Finder tags, a sync client setting the mtime). Ferry must not process it again. Test: Task 6 `a_touched_or_tagged_ferry_output_is_not_ready`.
5. **Several screenshots in a quick burst** (the user takes 5 within 3 s). Every one is optimised. The "more than N files at once" rule applies only to watched folders, never to captures. Test: Task 5 `captures_have_no_burst_limit`.

---

## Decisions (with justification)

| Topic | Decision | Why |
| --- | --- | --- |
| Watch crate | `notify = "8.2.0"` (CC0-1.0, released 2025-08-03, MSRV 1.77), default features (`macos_fsevent`). Uses FSEvents on macOS, ReadDirectoryChangesW on Windows and inotify on Linux. | R§5 names `notify`. 8.2.0 is the newest stable; 9.0.0 is only at `-rc.6`. Transitive crates are all permissive: `notify-types` 2.x (MIT OR Apache-2.0), `fsevent-sys` 4.x (MIT, macOS), `windows-sys` 0.60.x (MIT OR Apache-2.0, Windows), `walkdir` 2.5 (Unlicense OR MIT, already in Cargo.lock), `inotify`/`inotify-sys` (ISC, Linux only), `mio` 1.x (MIT, Linux), `libc`, `log`, `bitflags` (already present). None is GPL/LGPL/AGPL or patent-encumbered. |
| Debouncing | **Own settle state machine** (`watch::settle`), not `notify-debouncer-full` 0.7.0 or `-mini` 0.7.0 | (1) A debouncer only merges events in a time window. It does not prove a write finished. Ferry needs a size+mtime-stable check plus the Windows exclusive-open check (R§5). (2) `debouncer-full` walks every watched tree with `walkdir` at start to build its file-id cache. That costs CPU and I/O on big folders such as Downloads (NFR-R1). (3) A pure state machine with an injected clock and probe gives exact, fast, deterministic tests. (4) One dependency fewer. |
| Capture defaults | `watch.captures.screenshots` and `watch.captures.recordings` default to **false** (off). `show_results` defaults to true. A settings file without a `[watch]` section (every 0.2.0 install) loads with both off. | User decision (2026-10-23): on upgrade Ferry must not start optimising Desktop screenshots in place, and macOS must not show a Desktop-access prompt, until the user asks for it. With both off, `plan::build` marks the capture folders `Paused` and never lists them, so no Desktop prompt appears. Onboarding (out of scope) may turn them on later. |
| Recursion | Non-recursive: capture folders and watched folders only see files directly inside them | R§4.12 lists no recursion option. Subfolders hold Ferry's `.trash` test bins, app bundles, and so on. User decision: no recursion option this cycle. |
| What starts a wait | Only `Create(_)` and rename-into (`Modify(Name(To \| Both \| Any))`) start the settle wait. Data-modify events only extend a wait already running. `Modify(Metadata(_))` and `Access(_)` are ignored. Remove events never cancel a wait: the probe drops a file that is gone. | Tagging an old file, cloud hydration or dehydration (an attribute change) and Spotlight never look like "a file arrived". Rename-into catches browser downloads (`.crdownload` → final name) and macOS screenshot temp files (`.Screenshot …` → final name). |
| Settle (partial writes) | A file is *settled* when all of these hold: no event for it for `quiet` = 1 s; two probes at least `stable` = 1 s apart saw the same `(len, mtime)`; `len > 0`; not a cloud placeholder; and (Windows only) it opens with `share_mode(0)`. Probes run every `poll` = 500 ms. A wait with no event for `max_wait` = 30 min is dropped. At most 1,000 files wait at once. | R§2.4 "wait until a file stops changing". R§5 Windows "open without a sharing violation". Typical delay is about 2 s after the last write. |
| Names never processed | Ignored by name: anything starting with `.` (macOS screenshot temp `.Screenshot…`, AppleDouble `._x`, `.DS_Store`, iCloud `.x.icloud`) or `~$`; anything ending, case-insensitively, in `.ferry-tmp`, `.part`, `.partial`, `.crdownload`, `.download`, `.tmp`, `.temp`; and `thumbs.db`, `desktop.ini`. | Temp and placeholder files; `.ferry-tmp` is `fsutil::atomic_replace`'s own temp. |
| Cloud placeholders | Skipped if macOS `st_flags & SF_DATALESS (0x4000_0000)` is set, or Windows `file_attributes & (OFFLINE 0x1000 \| RECALL_ON_OPEN 0x4_0000 \| RECALL_ON_DATA_ACCESS 0x40_0000)` is set. On macOS, an `Arrived` for `name` within 10 s of any event on `.name.icloud` in the same folder is a re-download and is ignored. | R§2.4 / R§3.3 "ignore cloud-offload and re-download events". Probing a placeholder never reads it, so no download is forced. |
| No feedback loops | Four layers. (1) **Self-write ledger** `ferry_core::selfwrite`: `fsutil::atomic_replace` records `(canonical path → len, mtime)` after its rename. `fsutil::set_times` refreshes an entry that already exists. `Job::rename` records the new name. A settled file whose current stamp equals its ledger stamp is Ferry's own and is dropped. Entries live 10 min; the cap is 4,096. (2) **Excluded roots**: backups, working dir, log dir and config dir are never watched and their events are dropped. (3) Names ending `.ferry-tmp` are ignored. (4) **Seen map**: a path already settled with the same stamp is never handled twice (TTL 24 h, cap 4,096). | Covers in-place overwrite, In-place convert (new `x.webp`), Same-folder and Specific-folder outputs landing in a watched folder, Restore, card Rename, and settings saves. The ledger is consulted only at *settle* time, at least 2 s after the write, so the write always records before the check. A ledger keyed by stamp (not by path alone) still lets the user's own later edit of the same file through. |
| Path identity | `selfwrite::key(path)`: macOS/Linux use `fs::canonicalize`, falling back to canonical parent + file name, then to the path as given. Windows uses `std::path::absolute` and then the lowercased string, with no `\\?\` prefix. Every comparison (ledger, seen, routing, excluded roots) goes through `key`. | FSEvents reports `/private/var/…` for `/var/…` temp dirs and resolves symlinks. Windows paths are case-insensitive. A `\\?\` path would leak into card output paths and Explorer. |
| macOS capture folder | Read `location` from `NSUserDefaults` initialised with suite `com.apple.screencapture` (objc2-foundation feature `NSUserDefaults`, no shelling out). Expand `~`. If the value is unset, blank or not a directory, use `~/Desktop`. Re-read on every 30 s rescan, because the user can change it in ⇧⌘5 › Options. | R§5: "Read the screenshot location setting (default Desktop)". Ferry is not sandboxed, so another domain's defaults can be read. Apple docs: `NSUserDefaults` picks up other processes' changes. |
| macOS capture identity | The Desktop also holds other files. An image is a screenshot only if it has the xattr `com.apple.metadata:kMDItemIsScreenCapture`. A video is a recording if it has that xattr **or** its name starts with `Screen Recording` (ASCII case-insensitive). A file that fails these checks falls through to a watched-folder rule for the same folder, if there is one. | screencaptureui writes the marker (C1 §4.2 relies on it). Whether recordings carry it is unverified. User decision: keep the name fallback for now. Task 11 item 4 records whether recordings carry the marker; this stream does not change the rule based on the answer (a later cycle decides whether to drop the fallback). |
| Windows capture folders | `FOLDERID_Screenshots` (`SHGetKnownFolderPath`, `KF_FLAG_DONT_VERIFY`), `FOLDERID_Videos`\\`Captures` (Game Bar) and `FOLDERID_Videos`\\`Screen Recordings` (Snipping Tool). Every image or video in them counts as a capture. Code is `cfg(windows)`. | R§5 table. These folders hold only captures. `DONT_VERIFY` returns the path even before the first capture creates the folder; it shows as "Appears after your first capture" until then. User decision: use the default `Videos\Captures` only; a Game Bar folder the user moved elsewhere (registry setting) is not followed this cycle. |
| Burst rule (FR-T5) | Per watched folder, ready files are held for a 2 s window that starts at the first ready file. At window end, if `count > max_files_at_once` (default 10, range 1–1000), **all** of them are skipped and one notice card appears (if the folder's card is on). Otherwise all start. Captures have no burst rule. | "Skip when more than N files arrive at once" needs a window. 2 s adds little latency. |
| Quiet folders | When a folder's (or captures') "Show a card" is off, its jobs show no Queued/Optimising/Done card and stay out of tray Recent results. If one fails, its Error card shows; from then on it is a normal card. | FR-T5 "per-folder result visibility" + R§3.3 "fail visibly". |
| Hand-off to the job flow | The watch thread emits `Notice::Ready`. The app runs `dispatch_ready` on `spawn_blocking`: image → `Job::file(path, &settings, has_gifsicle)`; video/audio/PDF → `watch::hooks::{video_job, audio_job, pdf_job}`. Then `RunState::take_automatic`, then `jobs::start(app, job, Level::Normal, None)`. `trigger.rs`, `job.rs`, `sniff.rs`, `output.rs` and `commands.rs` are not changed. | Reuses C1 §5.4 and C2 §4.4 unchanged: backups, In place / Same folder / Specific / Temporary, never-worse, `copy_xattrs` (keeps the screenshot marker, FR-T4). `Job::file` already applies `images.max_size_mb` and `images.ignored_extensions`. |
| Video hook | `src-tauri/src/watch/hooks.rs`: `pub const VIDEO: bool = false;` and `pub fn video_job(app: &AppHandle, path: &Path, settings: &Settings) -> Option<Result<Job, String>>`, which returns `None` ("unsupported"). Same for `AUDIO`/`audio_job` and `PDF`/`pdf_job`. The Settings pane disables the matching type chips and the recordings switch while a flag is `false`. | Stream A runs in parallel. A merge only replaces one body and flips one const (see "Cross-stream notes"). |
| Settings UI | New pane `watch` ("Watched folders"): group "Screenshots and recordings" (3 switches + a read-only capture-folder status row) and group "Folders" (one table control). Add a folder with a picker or by dropping it on the pane (Tauri `onDragDropEvent`). | R§4.12 "Watched folders" and FR-T4. |

---

## File structure

**ferry-core (new):**
- `crates/ferry-core/src/selfwrite.rs`: `Stamp`, `key`, `Ledger`, and the process-wide `record` / `refresh` / `is_own`.
- `crates/ferry-core/src/watch/mod.rs`: module root and re-exports.
- `crates/ferry-core/src/watch/filter.rs`: ignored names, glob patterns, `is_inside`.
- `crates/ferry-core/src/watch/media.rs`: `Media`, `classify`.
- `crates/ferry-core/src/watch/capture.rs`: `CaptureKind`, `CaptureRule`, `CaptureDir`, macOS/Windows folder rules.
- `crates/ferry-core/src/watch/settle.rs`: `SettleConfig`, `Probe`, `Settler`.
- `crates/ferry-core/src/watch/probe.rs`: the real `probe`, `head`, placeholder bit tests.
- `crates/ferry-core/src/watch/events.rs`: `FsEvent`, `from_notify`.
- `crates/ferry-core/src/watch/plan.rs`: `Plan`, `Role`, `DirState`, `WatchStatus`, `resolve_dir`, `check_folder`.
- `crates/ferry-core/src/watch/engine.rs`: `Engine`, `Fs`, `RealFs`, `Ready`, `Origin`, `Notice`.
- `crates/ferry-core/src/watch/service.rs`: the thread that owns the `notify` watcher; `spawn`, `ServiceHandle`.
- `crates/ferry-core/src/watch/live_tests.rs`: `#[cfg(test)]` real-folder tests, including the end-to-end no-loop tests.

**ferry-core (modified):** `lib.rs`, `settings.rs`, `fsutil.rs`, `pipeline.rs` (one line), `Cargo.toml`.

**App (new):**
- `src-tauri/src/watch/mod.rs`: start, reload, dispatch.
- `src-tauri/src/watch/hooks.rs`: the video/audio/PDF hooks.
- `src-tauri/src/watch/commands.rs`: Tauri commands.
- `src-tauri/src/platform/macos_capture.rs`
- `src-tauri/src/platform/windows_capture.rs` (cfg(windows)).

**App (modified):** `lib.rs`, `state.rs`, `jobs.rs` (two lines in `show`), `prefs.rs`, `platform/mod.rs`, `Cargo.toml`.

**Frontend (new):**
- `src/lib/watch.ts` + `src/lib/watch.test.ts`: types and pure helpers.
- `src/settings/watchStore.svelte.ts`: status/support store shared by the two controls.
- `src/settings/controls/CaptureDirs.svelte`
- `src/settings/controls/WatchFolders.svelte`

**Frontend (modified):** `src/lib/i18n/en.json`, `src/lib/settings.ts`, `src/lib/settings.fixture.ts`, `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src/settings/Settings.svelte`.

## Shared-file edits (anchors)

| File | Anchor | Edit |
| --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | after `pub mod run_state;` | `pub mod selfwrite; // stream C` |
| `crates/ferry-core/src/lib.rs` | after `pub mod trigger;` | `pub mod watch; // stream C` |
| `crates/ferry-core/src/settings.rs` | `Settings` struct, after `pub hotkeys: Hotkeys,` | `/// Stream C: watchers.` + `pub watch: Watch,` |
| `crates/ferry-core/src/settings.rs` | `impl Default for Settings`, after `hotkeys: Hotkeys::default(),` | `watch: Watch::default(),` |
| `crates/ferry-core/src/settings.rs` | after `impl Default for Hotkeys { … }` | new block starting `// ---- Stream C: watchers ([watch]) ----` with the structs from Task 1 |
| `crates/ferry-core/src/settings.rs` | `sanitised()`, directly before `self.version = self.version.max(VERSION);` | `self.watch = self.watch.sanitised(); // stream C` |
| `crates/ferry-core/src/settings.rs` | end of `mod tests` | tests under a `// ---- Stream C: watch ----` comment |
| `src-tauri/src/lib.rs` | after `mod tray;` | `mod watch; // stream C` |
| `src-tauri/src/lib.rs` | `generate_handler![…]`, after `commands::pick_folder,` | `// stream C (watch)` then the four `watch::commands::…` entries |
| `src-tauri/src/lib.rs` | `setup()`, after `clipboard::start(app);` | `watch::start(app); // stream C` |
| `src-tauri/src/state.rs` | `AppState` fields, after `pub tray_error: AtomicBool,` | `/// Stream C: …` + `pub quiet: Mutex<HashSet<JobId>>,` |
| `src-tauri/src/state.rs` | `AppState::new`, after `tray_error: AtomicBool::new(false),` | `quiet: Mutex::new(HashSet::new()),` |
| `src-tauri/src/state.rs` | `impl AppState`, after `fn remember` | `quiet_gate`, `is_quiet` (Task 8) |
| `src-tauri/src/state.rs` | first line inside `fn remember` | `if self.is_quiet(&update.id) { return; } // stream C` |
| `src-tauri/src/platform/mod.rs` | end of file | `// ---- Stream C: capture folders ----` block (Task 7) |
| `src/lib/i18n/en.json` | end of the object (after `"error.pick_app"`) | all `watch.*` keys (Task 9) |
| `Cargo.lock` | n/a | regenerated by cargo only |

Not on the brief's list but likely touched by other streams too; same anchor discipline applies:

| File | Anchor | Edit |
| --- | --- | --- |
| `crates/ferry-core/Cargo.toml` | `[dependencies]`, after `mozjpeg-sys` | `notify = "8.2.0"` |
| `crates/ferry-core/src/fsutil.rs` | `atomic_replace`, `set_times`, `with_timeout`, new `has_xattr` at end before `mod tests` | Task 2 |
| `crates/ferry-core/src/pipeline.rs` | `Job::rename`, directly after the `fs::rename(…)?;` statement | `crate::selfwrite::record(&to); // stream C` |
| `src-tauri/Cargo.toml` | macOS `objc2-foundation` features; Windows `windows-sys` features | add `"NSUserDefaults"`; add `"Win32_UI_Shell"`, `"Win32_System_Com"` |
| `src-tauri/src/jobs.rs` | `pub fn show`, after `let state = app.state::<AppState>();` | quiet gate (Task 8) |
| `src-tauri/src/prefs.rs` | `Effects` struct end, `between`, `apply` | `watch` effect (Task 8) |
| `src/lib/settings.ts`, `src/lib/settings.fixture.ts` | after `hotkeys` | `watch` types / sample |
| `src/lib/schema.ts` | `PaneId`, `panes`, `Control`, `Setting`, end of `all` | `// stream C` additions (Task 9) |
| `src/settings/Row.svelte` | imports; before `{:else if control.kind === 'action'}`; `.row` class | Task 10 |
| `src/settings/Settings.svelte` | `blocker` neighbour; the two `Row` props `disabled` / `hint` | Task 10 |

`src-tauri/src/commands.rs`, `menu.rs`, `tray.rs`, `windows.rs`, `tauri.conf.json`, `capabilities/*`, `src/stack/*` and `src/lib/types.ts` are **not** touched. `core:default` already allows the webview drag-drop event.

## New settings keys (`[watch]`, no version bump)

```jsonc
"watch": {
  "captures": { "screenshots": false, "recordings": false, "show_results": true },  // capture watching is off by default
  "folders": [
    // one per watched folder; defaults shown
    { "path": "~/Downloads",            // stored with ~ (contract_home)
      "types": { "image": true, "video": true, "audio": false, "pdf": false },
      "show_results": true,
      "ignore": [],                     // glob on the file name: * and ?, case-insensitive
      "max_files_at_once": 10,          // 1..=1000
      "paused": false }
  ]
}
```

---

### Task 1: `[watch]` settings schema

**Files:**
- Modify: `crates/ferry-core/src/settings.rs` (anchors above)
- Modify: `src/lib/settings.ts`, `src/lib/settings.fixture.ts`
- Test: `crates/ferry-core/src/settings.rs` (`mod tests`)

**Interfaces:**
- Produces (Rust, `ferry_core::settings`). All structs derive `Debug, Clone, PartialEq, Serialize, Deserialize` and carry `#[serde(default)]` + `#[serde(flatten)] pub extra: Map<String, Value>`:
  - `pub struct Watch { pub captures: Captures, pub folders: Vec<WatchedFolder>, pub extra }`
  - `pub struct Captures { pub screenshots: bool, pub recordings: bool, pub show_results: bool, pub extra }`
  - `pub struct WatchedFolder { pub path: String, pub types: WatchTypes, pub show_results: bool, pub ignore: Vec<String>, pub max_files_at_once: u32, pub paused: bool, pub extra }`
  - `pub struct WatchTypes { pub image: bool, pub video: bool, pub audio: bool, pub pdf: bool, pub extra }`
  - `pub const MAX_FILES_AT_ONCE: std::ops::RangeInclusive<u32> = 1..=1000;`
  - `impl Watch { pub fn sanitised(self) -> Self }`
  - `Settings.watch: Watch`
- Produces (TS, `src/lib/settings.ts`): `export interface WatchTypes { image: boolean; video: boolean; audio: boolean; pdf: boolean }`, `export interface WatchedFolder { path: string; types: WatchTypes; show_results: boolean; ignore: string[]; max_files_at_once: number; paused: boolean }`, `export interface Watch { captures: { screenshots: boolean; recordings: boolean; show_results: boolean }; folders: WatchedFolder[] }`, and `Settings.watch: Watch`.

- [ ] **Step 1: Write the failing tests** (in `settings.rs` tests, under `// ---- Stream C: watch ----`)

```rust
#[test]
fn watch_defaults_match_the_plan() {
    let s = Settings::default();
    assert_eq!(serde_json::to_value(&s.watch).unwrap(), json!({
        "captures": { "screenshots": false, "recordings": false, "show_results": true },
        "folders": []
    }));
    assert_eq!(s.version, 2);
}

#[test]
fn an_upgraded_settings_file_leaves_capture_watching_off() {
    // A 0.2.0 settings file has no [watch] section: Ferry must not start watching the Desktop.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{"version": 2, "images": {"compression": 80}}"#).unwrap();
    let (s, _) = Settings::load(&path);
    assert!(!s.watch.captures.screenshots);
    assert!(!s.watch.captures.recordings);
    assert!(s.watch.captures.show_results);
    assert!(s.watch.folders.is_empty());
}

#[test]
fn a_folder_with_only_a_path_takes_every_other_default() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{"watch": {"folders": [{"path": "~/Downloads"}]}}"#).unwrap();
    let (s, status) = Settings::load(&path);
    assert_eq!(status, LoadStatus::Loaded);
    let f = &s.watch.folders[0];
    assert_eq!(f.path, "~/Downloads");
    assert!(f.types.image && f.types.video && !f.types.audio && !f.types.pdf);
    assert!(f.show_results && !f.paused && f.ignore.is_empty());
    assert_eq!(f.max_files_at_once, 10);
    assert_eq!(s.watch.captures, Captures::default());
}

#[test]
fn broken_watch_entries_are_repaired() {
    let mut s = Settings::default();
    let folder = |p: &str| WatchedFolder { path: p.into(), ..WatchedFolder::default() };
    s.watch.folders = vec![folder("  "), folder(" ~/A "), folder("~/A"), folder("~/B")];
    s.watch.folders[1].max_files_at_once = 0;
    s.watch.folders[3].max_files_at_once = 5000;
    s.watch.folders[3].ignore = vec!["  ".into(), " *.tmp ".into(), "*.tmp".into()];
    let s = s.sanitised();
    let paths: Vec<_> = s.watch.folders.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, ["~/A", "~/B"]);
    assert_eq!(s.watch.folders[0].max_files_at_once, 1);
    assert_eq!(s.watch.folders[1].max_files_at_once, 1000);
    assert_eq!(s.watch.folders[1].ignore, ["*.tmp"]);
}

#[test]
fn unknown_fields_in_a_watched_folder_survive() {
    // Stream F will add `pipeline` here; nothing may drop it before then.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{"watch": {"folders": [{"path": "~/In", "pipeline": "webp"}], "later": 1}}"#).unwrap();
    let (s, _) = Settings::load(&path);
    s.save(&path).unwrap();
    let saved = read_json(&path);
    assert_eq!(saved["watch"]["folders"][0]["pipeline"], json!("webp"));
    assert_eq!(saved["watch"]["later"], json!(1));
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry-core settings::tests::watch -- --nocapture`, `cargo test -p ferry-core settings::tests::broken_watch` and `cargo test -p ferry-core settings::tests::an_upgraded_settings_file`
Expected: compile errors (`Watch`, `Captures`, `WatchedFolder` not found).

- [ ] **Step 3: Implement the structs, defaults, the `Settings.watch` field and `Watch::sanitised`**

Defaults: `Captures { screenshots: false, recordings: false, show_results: true }` (capture watching is off until the user turns it on; see Decisions › Capture defaults). Write `impl Default for Captures` by hand; do not derive it, because `show_results` must default to `true`. `WatchedFolder` defaults: `WatchedFolder { path: "", types: WatchTypes { image: true, video: true, audio: false, pdf: false }, show_results: true, ignore: [], max_files_at_once: 10, paused: false }`.

`sanitised` does the following:
- trims each `path`;
- drops entries whose path is empty;
- drops later entries whose trimmed path string equals an earlier one;
- clamps `max_files_at_once` to `MAX_FILES_AT_ONCE`;
- trims `ignore` items, drops empty ones and removes duplicates (first one wins).

Add the one-line call in `Settings::sanitised` at the anchor.

- [ ] **Step 4: Mirror in TS**

Add the interfaces above to `src/lib/settings.ts` after the `hotkeys` field, with the comment `/** Stream C: mirrors ferry_core::settings::Watch. */`. Add `watch: { captures: { screenshots: false, recordings: false, show_results: true }, folders: [] }` to `sample` in `settings.fixture.ts` (it mirrors the Rust defaults).

- [ ] **Step 5: Run the tests and check they pass**

Run: `cargo test -p ferry-core settings` and `pnpm check && pnpm test`
Expected: all PASS (the existing `a_version_1_file_loads_as_version_2_with_the_new_defaults` still passes).

- [ ] **Step 6: Commit**

```bash
git add crates/ferry-core/src/settings.rs src/lib/settings.ts src/lib/settings.fixture.ts
git commit -m "feat(core): [watch] settings for screenshot and folder watchers"
```

---

### Task 2: Self-write ledger and its hooks

**Files:**
- Create: `crates/ferry-core/src/selfwrite.rs`
- Modify: `crates/ferry-core/src/lib.rs` (anchor), `crates/ferry-core/src/fsutil.rs`, `crates/ferry-core/src/pipeline.rs` (one line in `Job::rename`)
- Test: in `selfwrite.rs` and `pipeline.rs` tests

**Interfaces:**
- Produces (`ferry_core::selfwrite`):
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub struct Stamp { pub len: u64, pub modified: std::time::SystemTime }`
  - `impl Stamp { pub fn of(path: &Path) -> Option<Stamp> }` (`fs::metadata`, follows symlinks)
  - `pub fn key(path: &Path) -> PathBuf` (rules in Decisions › Path identity)
  - `pub const TTL: Duration = Duration::from_secs(600); pub const CAP: usize = 4096;`
  - `pub struct Ledger`, with `impl Ledger { pub fn new(ttl: Duration, cap: usize) -> Self; pub fn record(&mut self, key: PathBuf, stamp: Stamp, now: Instant); pub fn refresh(&mut self, key: &Path, stamp: Stamp, now: Instant); pub fn is_own(&self, key: &Path, stamp: &Stamp, now: Instant) -> bool }`
  - Process-wide: `pub fn record(path: &Path)`, `pub fn refresh(path: &Path)`, `pub fn is_own(path: &Path, stamp: &Stamp) -> bool`. They use a `static LEDGER: LazyLock<Mutex<Ledger>>` (poison-tolerant: `lock().unwrap_or_else(PoisonError::into_inner)`).
- Produces (`ferry_core::fsutil`): `pub(crate) fn with_timeout<T: Send + 'static>(limit: Duration, work: impl FnOnce() -> io::Result<T> + Send + 'static) -> io::Result<T>` (now on every OS; remove the `cfg(any(target_os = "macos", test))` on it and on the `mpsc`/`thread` imports), and `pub fn has_xattr(path: &Path, name: &std::ffi::CStr) -> bool` (macOS: `libc::getxattr` size query `>= 0`, inside `with_timeout(XATTR_TIMEOUT, …)`, `false` on error or timeout; other OSes: always `false`).

- [ ] **Step 1: Write the failing tests** (`selfwrite.rs` `mod tests`; `t0 = Instant::now()`, `stamp(len)` builds `Stamp { len, modified: UNIX_EPOCH + Duration::from_secs(len) }`)

```rust
#[test]
fn a_recorded_file_is_own_until_it_changes() {
    let mut l = Ledger::new(TTL, CAP);
    l.record("/w/a.png".into(), stamp(10), t0);
    assert!(l.is_own(Path::new("/w/a.png"), &stamp(10), t0));
    assert!(!l.is_own(Path::new("/w/a.png"), &stamp(11), t0), "the user's later edit is not ours");
    assert!(!l.is_own(Path::new("/w/b.png"), &stamp(10), t0));
}
#[test]
fn a_record_expires_after_the_ttl() { /* record at t0; is_own at t0+TTL-1ms true, at t0+TTL false */ }
#[test]
fn the_ledger_never_holds_more_than_its_cap() { /* cap 3, record 5 keys at increasing times: the first two are gone, the last three are own */ }
#[test]
fn refresh_only_updates_known_paths() { /* refresh unknown key → is_own false; refresh known with new stamp → own with the new stamp, not the old */ }
#[test]
fn atomic_replace_marks_its_result_as_own() {
    let dir = tempfile::tempdir().unwrap();
    let p = dir.path().join("a.png");
    fs::write(&p, b"old").unwrap();
    crate::fsutil::atomic_replace(&p, b"new bytes").unwrap();
    assert!(is_own(&p, &Stamp::of(&p).unwrap()));
    fs::write(&p, b"user edit, longer").unwrap();
    assert!(!is_own(&p, &Stamp::of(&p).unwrap()));
}
#[test]
fn set_times_keeps_the_mark_on_a_dated_result() {
    // atomic_replace, then fsutil::set_times(&p, &old_times) as pipeline::write_file does with preserve_dates
    // → is_own(&p, &Stamp::of(&p)) is still true, and the mtime equals the old time.
}
#[cfg(unix)]
#[test]
fn keys_match_through_a_symlinked_folder() {
    // real dir R, symlink L → R; record(L/a.png); is_own(R/a.png, stamp) is true (FSEvents reports real paths).
}
#[test]
fn keys_survive_spaces_and_narrow_nbsp() {
    // file "Screenshot 2026-10-23 at 10.21.33\u{202F}AM.png"; atomic_replace then is_own via the same name → true
}
#[cfg(windows)]
#[test]
fn keys_ignore_case_on_windows() { /* key("C:\\X\\A.PNG") == key("c:\\x\\a.png"); neither starts with "\\\\?\\" */ }
```

In `pipeline.rs` tests, next to the existing rename tests:

```rust
#[test]
fn a_card_rename_is_ferrys_own_write() {
    // file job on env.file("shot.png", &testutil::png(300, 200)), run, then job.rename("renamed")
    let to = env.dir.path().join("renamed.png");
    assert!(crate::selfwrite::is_own(&to, &crate::selfwrite::Stamp::of(&to).unwrap()));
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry-core selfwrite` and `cargo test -p ferry-core a_card_rename_is_ferrys_own_write`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement `selfwrite.rs`**

Store entries as `HashMap<PathBuf, (Stamp, Instant)>`. When the map is full, `record` first removes expired entries, then the entry with the oldest `Instant`. `is_own` is true only when the key is present, not expired, and the stamp is equal. The global functions call `key(path)` and `Stamp::of(path)`. `record`/`refresh` are no-ops if the stamp cannot be read.

- [ ] **Step 4: Wire the hooks**

In `fsutil::atomic_replace`, after a successful `fs::rename(&tmp, &target)` (inside the `Ok` path, before returning), call `crate::selfwrite::record(&target);`. `target` is the resolved symlink destination already computed there. In `fsutil::set_times`, after `set_times(file_times)` succeeds, call `crate::selfwrite::refresh(path);`. Make `with_timeout` `pub(crate)` on every OS. Add `has_xattr`. In `Job::rename`, add the anchor line after the `fs::rename(...)` statement.

- [ ] **Step 5: Run the tests and check they pass**

Run: `cargo test -p ferry-core`
Expected: all PASS, including the existing `fsutil` and `pipeline` tests.

- [ ] **Step 6: Commit**

```bash
git add crates/ferry-core/src/selfwrite.rs crates/ferry-core/src/lib.rs crates/ferry-core/src/fsutil.rs crates/ferry-core/src/pipeline.rs
git commit -m "feat(core): remember the files Ferry writes so watchers never re-trigger on them"
```

---

### Task 3: Watch primitives: names, media, capture folders

**Files:**
- Create: `crates/ferry-core/src/watch/{mod.rs,filter.rs,media.rs,capture.rs}`
- Modify: `crates/ferry-core/src/lib.rs` (anchor `pub mod watch; // stream C`)

**Interfaces:**
- `watch/mod.rs`: `pub mod capture; pub mod engine; pub mod events; pub mod filter; pub mod media; pub mod plan; pub mod probe; pub mod service; pub mod settle; #[cfg(test)] mod live_tests;`. Declare each module in the task that creates it.
- `watch::filter`:
  - `pub fn is_ignored_name(name: &str) -> bool`: the exact list in Decisions › "Names never processed".
  - `pub fn glob_matches(pattern: &str, name: &str) -> bool`: `*` matches any run (including empty), `?` matches exactly one char. Case-insensitive (`to_lowercase` on both). The whole name must match. A pattern containing `/` or `\` never matches.
  - `pub fn is_inside(path: &Path, root: &Path) -> bool`: `selfwrite::key(path).starts_with(selfwrite::key(root))`, component-wise.
- `watch::media`:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum Media { Image, Video, Audio, Pdf }`
  - `pub const VIDEO_EXTENSIONS: &[&str] = &["mp4", "mov", "m4v", "mkv", "webm"];`. No `avi`: stream A (video) does not support AVI, so an `.avi` file is not a video for the watcher and is left alone.
  - `pub const AUDIO_EXTENSIONS: &[&str] = &["wav", "flac", "aif", "aiff", "mp3", "m4a", "ogg", "opus"];`
  - `pub fn classify(name: &str, head: &[u8]) -> Option<Media>`. In order: `sniff::sniff(head).is_some()` → `Image` (content wins over the extension); `head.starts_with(b"%PDF-")` → `Pdf`; extension (via `trigger::normalise_ext`) in `VIDEO_EXTENSIONS` → `Video`, in `AUDIO_EXTENSIONS` → `Audio`; otherwise `None`.
  - `impl Media { pub fn allowed_by(self, types: &WatchTypes) -> bool }`
- `watch::capture`:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum CaptureKind { Screenshot, Recording }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum CaptureRule { Marked { recording_prefixes: Vec<String> }, Everything }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct CaptureDir { pub dir: PathBuf, pub rule: CaptureRule }`
  - `pub const SCREEN_CAPTURE_XATTR: &std::ffi::CStr = c"com.apple.metadata:kMDItemIsScreenCapture";`
  - `pub const RECORDING_PREFIX: &str = "Screen Recording";`
  - `pub fn macos_dirs(location: Option<&str>, home: &Path, is_dir: &dyn Fn(&Path) -> bool) -> Vec<CaptureDir>`: one `CaptureDir` with rule `Marked { recording_prefixes: vec![RECORDING_PREFIX.into()] }`.
  - `pub fn windows_dirs(screenshots: Option<PathBuf>, videos: Option<PathBuf>) -> Vec<CaptureDir>`: in order `screenshots`, `videos/Captures`, `videos/Screen Recordings`, all `Everything`.
  - `pub fn capture_kind(rule: &CaptureRule, name: &str, media: Media, marked: bool) -> Option<CaptureKind>`
  - `pub fn wanted(kind: CaptureKind, captures: &Captures) -> bool`

- [ ] **Step 1: Write the failing tests**

```rust
// filter.rs
#[test] fn temp_hidden_and_system_names_are_ignored() {
    for n in [".Screenshot 2026-10-23 at 10.21.33.png", "._a.png", ".DS_Store", ".a.png.icloud",
              "~$doc.docx", "a.png.ferry-tmp", "a.PART", "b.crdownload", "c.png.download",
              "d.tmp", "e.TEMP", "Thumbs.db", "desktop.ini", "f.partial"] {
        assert!(is_ignored_name(n), "{n}");
    }
    for n in ["Screenshot 2026-10-23 at 10.21.33\u{202F}AM.png", "a.png", "tmp.png", "Screen Recording 1.mov"] {
        assert!(!is_ignored_name(n), "{n}");
    }
}
#[test] fn globs_match_the_whole_name_without_case() {
    assert!(glob_matches("*.MOV", "clip.mov"));
    assert!(glob_matches("draft-??.png", "draft-01.png"));
    assert!(!glob_matches("draft-?.png", "draft-01.png"));
    assert!(glob_matches("*", "x"));
    assert!(!glob_matches("*.png", "a.png.bak"));
    assert!(!glob_matches("sub/*.png", "a.png"));
}
#[test] fn inside_compares_whole_components() { /* /a/bc not inside /a/b; /a/b/c inside /a/b; /a/b inside /a/b */ }

// media.rs
#[test] fn content_wins_over_the_extension() {
    let png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";
    assert_eq!(classify("photo.mov", png), Some(Media::Image));
    assert_eq!(classify("x.png", b"garbage"), None);
    assert_eq!(classify("doc.bin", b"%PDF-1.7"), Some(Media::Pdf));
    assert_eq!(classify("Screen Recording.MOV", b"\0\0\0\x14ftypqt  "), Some(Media::Video));
    assert_eq!(classify("song.flac", b"fLaC"), Some(Media::Audio));
    assert_eq!(classify("notes.txt", b"hello"), None);
}
#[test] fn avi_is_not_a_watched_video() {
    // Stream A does not support AVI, so the watcher must not hand it over.
    assert!(!VIDEO_EXTENSIONS.contains(&"avi"));
    assert_eq!(classify("clip.avi", b"RIFF\x24\0\0\0AVI LIST"), None);
    assert_eq!(classify("CLIP.AVI", b"RIFF\x24\0\0\0AVI LIST"), None);
}
#[test] fn types_allow_each_media() { /* WatchTypes::default(): Image, Video allowed; Audio, Pdf not */ }

// capture.rs
#[test] fn macos_uses_the_screenshot_location_or_the_desktop() {
    let home = Path::new("/Users/me");
    let exists = |p: &Path| p == Path::new("/Users/me/Pictures/Shots") || p == Path::new("/Volumes/X");
    assert_eq!(macos_dirs(None, home, &exists)[0].dir, Path::new("/Users/me/Desktop"));
    assert_eq!(macos_dirs(Some("  "), home, &exists)[0].dir, Path::new("/Users/me/Desktop"));
    assert_eq!(macos_dirs(Some("~/Pictures/Shots"), home, &exists)[0].dir, Path::new("/Users/me/Pictures/Shots"));
    assert_eq!(macos_dirs(Some("/Volumes/X"), home, &exists)[0].dir, Path::new("/Volumes/X"));
    assert_eq!(macos_dirs(Some("/gone"), home, &exists)[0].dir, Path::new("/Users/me/Desktop"));
}
#[test] fn windows_watches_screenshots_captures_and_screen_recordings() {
    let dirs = windows_dirs(Some("C:\\U\\Pictures\\Screenshots".into()), Some("C:\\U\\Videos".into()));
    assert_eq!(dirs.len(), 3);
    assert_eq!(dirs[1].dir, Path::new("C:\\U\\Videos").join("Captures"));
    assert_eq!(dirs[2].dir, Path::new("C:\\U\\Videos").join("Screen Recordings"));
    assert!(dirs.iter().all(|d| d.rule == CaptureRule::Everything));
    assert_eq!(windows_dirs(None, Some("V".into())).len(), 2);
}
#[test] fn capture_kind_needs_the_marker_or_the_recording_name_on_macos() {
    let mac = CaptureRule::Marked { recording_prefixes: vec![RECORDING_PREFIX.into()] };
    assert_eq!(capture_kind(&mac, "x.png", Media::Image, true), Some(CaptureKind::Screenshot));
    assert_eq!(capture_kind(&mac, "x.png", Media::Image, false), None);
    assert_eq!(capture_kind(&mac, "screen recording 2026.mov", Media::Video, false), Some(CaptureKind::Recording));
    assert_eq!(capture_kind(&mac, "holiday.mov", Media::Video, false), None);
    assert_eq!(capture_kind(&mac, "holiday.mov", Media::Video, true), Some(CaptureKind::Recording));
    assert_eq!(capture_kind(&CaptureRule::Everything, "a.png", Media::Image, false), Some(CaptureKind::Screenshot));
    assert_eq!(capture_kind(&CaptureRule::Everything, "a.pdf", Media::Pdf, false), None);
}
#[test] fn capture_toggles_gate_each_kind() {
    // Captures::default() (both off) → wanted(Screenshot) false, wanted(Recording) false.
    // Captures { screenshots: false, recordings: true, ..Default::default() } → wanted(Screenshot) false, wanted(Recording) true.
    // Captures { screenshots: true, recordings: false, ..Default::default() } → wanted(Screenshot) true, wanted(Recording) false.
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry-core watch::`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement the three files and `watch/mod.rs`, adding the `lib.rs` anchor line**

`macos_dirs` expands with `settings::expand_home`. In `capture_kind`, compare the recording prefix with `name.to_lowercase().starts_with(&prefix.to_lowercase())`.

- [ ] **Step 4: Run the tests and check they pass**

Run: `cargo test -p ferry-core watch::`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/watch crates/ferry-core/src/lib.rs
git commit -m "feat(core): watcher name filters, media classes and capture-folder rules"
```

---

### Task 4: Settle state machine and real probe

**Files:**
- Create: `crates/ferry-core/src/watch/settle.rs`, `crates/ferry-core/src/watch/probe.rs`

**Interfaces:**
- `watch::settle`:
  - `#[derive(Debug, Clone, Copy)] pub struct SettleConfig { pub quiet: Duration, pub stable: Duration, pub poll: Duration, pub max_wait: Duration, pub max_pending: usize }`. `Default` is 1 s, 1 s, 500 ms, 30 min, 1000.
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Probe { Gone, NotAFile, Placeholder, Busy, Denied, File(Stamp) }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum Polled { Settled { path: PathBuf, stamp: Stamp }, Denied { path: PathBuf } }`
  - `pub struct Settler`, with `impl Settler { pub fn new(cfg: SettleConfig) -> Self; pub fn start(&mut self, path: PathBuf, now: Instant); pub fn extend(&mut self, path: &Path, now: Instant); pub fn forget(&mut self, path: &Path); pub fn is_pending(&self, path: &Path) -> bool; pub fn len(&self) -> usize; pub fn next_wake(&self, now: Instant) -> Option<Duration>; pub fn poll(&mut self, now: Instant, probe: &mut dyn FnMut(&Path) -> Probe) -> Vec<Polled> }`
- `watch::probe`:
  - `pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);`
  - `pub fn probe(path: &Path) -> Probe`
  - `pub fn head(path: &Path, n: usize) -> Option<Vec<u8>>` (reads at most `n` bytes, inside `with_timeout`)
  - `pub fn is_dataless(st_flags: u32) -> bool` (`& 0x4000_0000 != 0`)
  - `pub fn is_placeholder_attributes(attrs: u32) -> bool` (`& (0x1000 | 0x4_0000 | 0x40_0000) != 0`)

Settler algorithm (this exact behaviour is what the tests pin). Each entry holds `{ last_event, last_probe: Option<Instant>, seen: Option<(Stamp, Instant)> }`.

- `start(path, now)`: insert, or reset `last_event = now` and `seen = None`. If the map is at `max_pending`, first evict the entry with the oldest `last_event` (no logging: `ferry-core` has no logger).
- `extend(path, now)`: only when pending, set `last_event = now` and `seen = None`.
- `poll(now)`, for each entry:
  - drop it if `now - last_event >= max_wait`;
  - skip it if `now - last_event < quiet`, or if `last_probe` is set and `now - last_probe < poll`;
  - otherwise probe and set `last_probe = now`. Then:
    - `Gone | NotAFile | Placeholder` → drop.
    - `Denied` → drop and emit `Polled::Denied`.
    - `Busy`, or `File(s)` with `s.len == 0` → `seen = None`.
    - `File(s)` with `seen == Some((s, at))` and `now - at >= stable` → emit `Settled` and drop.
    - `File(s)` with `seen` holding the same stamp but not yet stable → keep.
    - Otherwise → `seen = Some((s, now))`.
- `next_wake`: `None` if empty; otherwise the minimum over entries of `max(last_event + quiet, last_probe + poll)` minus `now`, saturating at zero.

Real `probe` runs everything inside `fsutil::with_timeout(PROBE_TIMEOUT, …)`; a timeout counts as `Busy`. Steps:
- `fs::symlink_metadata`: `NotFound` → `Gone`; `PermissionDenied` → `Denied`.
- A symlink or a non-file → `NotAFile`.
- macOS `MetadataExt::st_flags()` → `is_dataless` → `Placeholder`. Windows `MetadataExt::file_attributes()` → `is_placeholder_attributes` → `Placeholder`.
- **Windows only:** `OpenOptions::new().read(true).share_mode(0).open(path)` failing with `raw_os_error()` 32 or 33 → `Busy`; `PermissionDenied` → `Denied`.
- Otherwise → `File(Stamp { len, modified })`.

- [ ] **Step 1: Write the failing tests**

In `settle.rs` (`cfg = SettleConfig::default()`; `at(ms) = t0 + Duration::from_millis(ms)`; the probe closure reads a `HashMap<PathBuf, Probe>` the test edits between polls):

```rust
#[test] fn a_file_settles_after_quiet_and_stable_time() {
    // start at 0; probe File(s) constant; poll at 999 → []; 1000 → [] (seen); 1500 → []; 1999 → []; 2000 → [Settled]
}
#[test] fn a_growing_file_waits_until_it_stops_growing() { /* len 10 at 1000, 20 at 1500, 20 at 2000 → [], 20 at 2500 → Settled */ }
#[test] fn new_events_restart_the_quiet_period() { /* start 0, extend 1800, poll 2000 → [] (quiet), 2800 seen, 3800 Settled */ }
#[test] fn an_empty_file_is_never_settled_while_empty() { /* len 0 for 10 s → []; then len 5 → settles 1 s later */ }
#[test] fn a_busy_file_waits_for_the_writer_to_let_go() { /* Busy for 5 s → []; then File(s) twice 1 s apart → Settled */ }
#[test] fn a_vanished_folder_or_placeholder_file_is_dropped() { /* each of Gone, NotAFile, Placeholder → len() == 0 after one poll */ }
#[test] fn a_change_event_never_starts_a_wait() { /* extend on an unknown path → len() == 0 */ }
#[test] fn the_wait_gives_up_after_max_wait_without_events() { /* Busy forever; poll at max_wait → dropped */ }
#[test] fn denied_is_reported_once_and_dropped() { /* Denied → [Polled::Denied{..}], next poll → [] */ }
#[test] fn next_wake_is_none_when_idle_and_the_nearest_deadline_otherwise() {
    // empty → None; start at 0 → next_wake(at(200)) == Some(800ms)
}
#[test] fn pending_files_are_capped() { /* max_pending 2, start a,b,c → a gone, b and c pending */ }
```

In `probe.rs`:

```rust
#[test] fn a_plain_file_probes_with_its_size() { /* write 5 bytes → File(Stamp{len:5,..}) */ }
#[test] fn a_missing_file_is_gone_and_a_folder_is_not_a_file() {}
#[cfg(unix)] #[test] fn a_symlink_is_not_followed() { /* link to a real file → NotAFile */ }
#[test] fn placeholder_bits_are_recognised() {
    assert!(is_dataless(0x4000_0000)); assert!(!is_dataless(0x20));
    for a in [0x1000, 0x4_0000, 0x40_0000] { assert!(is_placeholder_attributes(a | 0x20)); }
    assert!(!is_placeholder_attributes(0x20 | 0x80));
}
#[test] fn head_reads_at_most_n_bytes() {}
#[cfg(windows)] #[test] fn a_file_held_open_without_sharing_is_busy() {
    use std::os::windows::fs::OpenOptionsExt;
    // let held = OpenOptions::new().write(true).share_mode(0).open(&p).unwrap();
    // assert_eq!(probe(&p), Probe::Busy); drop(held); assert!(matches!(probe(&p), Probe::File(_)));
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry-core watch::settle watch::probe`
Expected: FAIL.

- [ ] **Step 3: Implement `settle.rs` and `probe.rs` as specified**

- [ ] **Step 4: Run the tests and check they pass**

Run: `cargo test -p ferry-core watch::`
Expected: PASS on this Mac. `a_file_held_open_without_sharing_is_busy` is `cfg(windows)` and runs only in CI (`core (windows-latest)`). Note that in the commit body.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/watch/settle.rs crates/ferry-core/src/watch/probe.rs crates/ferry-core/src/watch/mod.rs
git commit -m "feat(core): settle detection: quiet, size-stable, unlocked and not a cloud placeholder"
```

---

### Task 5: Events, plan and engine

**Files:**
- Modify: `crates/ferry-core/Cargo.toml` (add `notify = "8.2.0"` after `mozjpeg-sys`)
- Create: `crates/ferry-core/src/watch/events.rs`, `plan.rs`, `engine.rs`

**Interfaces:**
- `watch::events`:
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum FsEvent { Arrived(PathBuf), Changed(PathBuf), Gone(PathBuf) }`
  - `pub fn from_notify(ev: &notify::Event) -> Vec<FsEvent>`. Mapping:
    - `Create(_)` → `Arrived` for each path.
    - `Modify(Name(To))` → `Arrived(paths[0])`.
    - `Modify(Name(From))` → `Gone(paths[0])`.
    - `Modify(Name(Both))` → `Gone(paths[0])`, `Arrived(paths[1])`.
    - `Modify(Name(Any | Other))` → `Arrived` for each path. The old path of a macOS rename no longer exists and is dropped by the probe.
    - `Modify(Data(_) | Any | Other)` → `Changed`.
    - `Modify(Metadata(_))` and `Access(_)` → nothing.
    - `Remove(_)` → `Gone`.
    - `Any | Other` → `Changed`.
- `watch::plan`:
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum DirState { Watching, Missing, Paused, Denied, Failed { reason: String } }`
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] pub struct DirStatus { pub path: String, pub state: DirState }`. For a folder, `path` is the `WatchedFolder.path` string exactly as stored. For a capture dir it is `contract_home(dir, home)`.
  - `#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)] pub struct WatchStatus { pub captures: Vec<DirStatus>, pub folders: Vec<DirStatus> }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum Resolve { Dir(PathBuf), Missing, Denied, Failed(String) }`
  - `#[derive(Debug, Clone)] pub enum Role { Capture { rule: CaptureRule }, Folder { index: usize } }`
  - `#[derive(Debug, Clone)] pub struct PlannedDir { pub dir: PathBuf, pub key: PathBuf, pub role: Role }`
  - `#[derive(Debug, Clone)] pub struct Plan { pub dirs: Vec<PlannedDir>, pub status: WatchStatus, pub watch: Watch, pub excluded: Vec<PathBuf> }`
  - `pub fn build(watch: &Watch, captures: &[CaptureDir], home: &Path, excluded: &[PathBuf], resolve: &mut dyn FnMut(&Path) -> Resolve) -> Plan`
  - `impl Plan { pub fn roles_for(&self, file: &Path) -> Vec<&PlannedDir>; pub fn watch_dirs(&self) -> Vec<PathBuf>; pub fn mark_denied(&mut self, file: &Path) -> bool }`
  - `pub fn resolve_dir(dir: &Path) -> Resolve`. Real file system, inside `with_timeout(PROBE_TIMEOUT)`. Normalise (macOS/Linux `fs::canonicalize`, Windows `std::path::absolute`), then `fs::read_dir(&normalised)`. `NotFound` / not a directory → `Missing`; `PermissionDenied` → `Denied`; `TimedOut` → `Failed(t("watch.status.timed_out"))`; any other error → `Failed(e.to_string())`.
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum FolderProblem { NotAFolder, OwnFolder }` and `pub fn check_folder(dir: &Path, excluded: &[PathBuf]) -> Result<(), FolderProblem>`
- `watch::engine`:
  - `pub trait Fs { fn probe(&self, path: &Path) -> Probe; fn head(&self, path: &Path) -> Option<Vec<u8>>; fn is_marked(&self, path: &Path) -> bool; fn is_own(&self, path: &Path, stamp: &Stamp) -> bool; }`
  - `pub struct RealFs;`. `impl Fs for RealFs` delegates to `probe::probe`, `probe::head(path, 64)`, `fsutil::has_xattr(path, SCREEN_CAPTURE_XATTR)` and `selfwrite::is_own`.
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")] pub enum Origin { Capture { capture: CaptureKind }, Folder { index: usize } }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct Ready { pub path: PathBuf, pub media: Media, pub origin: Origin, pub show_results: bool }`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub enum Notice { Ready(Ready), BurstSkipped { folder: String, count: usize, show_results: bool }, Status(WatchStatus) }`
  - `pub const BURST_WINDOW: Duration = Duration::from_secs(2); pub const CLOUD_WINDOW: Duration = Duration::from_secs(10); pub const SEEN_TTL: Duration = Duration::from_secs(86_400); pub const SEEN_CAP: usize = 4096;`
  - `pub struct Engine`, with `impl Engine { pub fn new(plan: Plan, cfg: SettleConfig) -> Self; pub fn set_plan(&mut self, plan: Plan); pub fn plan(&self) -> &Plan; pub fn on_event(&mut self, ev: FsEvent, now: Instant); pub fn tick(&mut self, now: Instant, fs: &dyn Fs) -> Vec<Notice>; pub fn next_wake(&self, now: Instant) -> Option<Duration> }`

`plan::build` rules:
- **Captures:** if both `screenshots` and `recordings` are off, each capture dir gets status `Paused` and no role. Otherwise call `resolve(dir)`. `Dir(n)` → `Watching` and a `PlannedDir { dir: n, key: selfwrite::key(&n), role: Capture }`. `Missing` / `Denied` / `Failed` → that state.
- **Folders,** by index: expand with `expand_home`. `paused` → `Paused`. Equal to or inside an excluded root → `Failed { reason: t("watch.error.own_folder") }`. Otherwise resolve as for captures.
- `roles_for(file)`: empty if `file` is inside any excluded root. Otherwise every planned dir whose `key` equals `selfwrite::key(file).parent()`, captures before folders, folders in settings order.
- `watch_dirs`: unique `dir`s.
- `mark_denied`: sets every matching dir's status to `Denied` and returns whether anything changed.

Engine rules:
- **`on_event`:**
  - If the file name starts with `.` and ends with `.icloud`, remember `(parent key, inner name without the leading '.' and trailing '.icloud', now)` and return.
  - If `is_ignored_name`, or `roles_for` is empty, return.
  - `Arrived`: if a cloud mark for the same parent and name is younger than `CLOUD_WINDOW`, return. Otherwise `settler.start`.
  - `Changed` → `settler.extend`.
  - `Gone` → nothing. The probe decides; a Remove event that comes before a re-create must not lose the file.
- **`tick`,** for each `Polled`:
  - `Denied` → `plan.mark_denied` and, if it changed, `Notice::Status`.
  - `Settled { path, stamp }`:
    1. If `fs.is_own(path, stamp)`, skip it.
    2. If the seen map has the same stamp for `key(path)`, skip it.
    3. Insert `key(path) → (stamp, now)` into the seen map. Evict entries older than `SEEN_TTL`, then the oldest, to stay within `SEEN_CAP`.
    4. Run `head` and `classify`; `None` → skip.
    5. For each role in `roles_for(path)`, take the first that accepts:
       - `Capture { rule }`: compute `kind = capture_kind(rule, name, media, marked)`, where `marked = fs.is_marked(path)` only for `Marked` rules. It accepts when `kind` is `Some` and `wanted(kind, &plan.watch.captures)`. It then emits `Ready { origin: Capture, show_results: captures.show_results }` immediately.
       - `Folder { index }`: it accepts when `!folder.paused`, `media.allowed_by(&folder.types)` and no `ignore` glob matches. The file then joins the burst for `index`.
- **Bursts:** per folder index, `(first: Instant, Vec<Ready>)`. In every `tick`, a burst with `now - first >= BURST_WINDOW` is flushed. More than `max_files_at_once` files → one `BurstSkipped { folder: <last path component of the folder's expanded path>, count, show_results }`. Otherwise one `Ready` per file, in arrival order.
- **`set_plan`:** keeps the settler, seen map and cloud marks. It drops burst entries whose folder index no longer exists, now points at a different path string, or is now paused.
- **`next_wake`:** the minimum of `settler.next_wake` and each burst's `first + BURST_WINDOW - now`.

- [ ] **Step 1: Write the failing tests**

`events.rs` builds events with `notify::Event::new(kind).add_path(p)`:

```rust
#[test] fn arrivals_are_creates_and_renames_into_a_folder() {
    use notify::event::{CreateKind, ModifyKind, RenameMode, DataChange, MetadataKind, RemoveKind};
    // Create(File) → [Arrived]; Name(To) → [Arrived]; Name(Both) [a,b] → [Gone(a), Arrived(b)];
    // Name(Any) → [Arrived]; Data(Content) → [Changed]; Metadata(Extended) → []; Access → []; Remove(File) → [Gone]
}
```

`plan.rs` uses a resolve closure over a `HashMap<PathBuf, Resolve>`, with no file-system access. Capture watching is off in `Watch::default()`, so a test that expects a capture `PlannedDir` or a `Watching` capture status turns `captures.screenshots` (or `recordings`) on first:

```rust
#[test] fn statuses_follow_each_folder_in_settings_order() { /* Watching, Missing, Paused, Denied; folders[i].path verbatim */ }
#[test] fn both_capture_toggles_off_pauses_the_capture_folders() {}
#[test] fn ferrys_own_folders_are_refused() {
    // a folder equal to or inside excluded "/app/backups" → Failed{reason == t("watch.error.own_folder")}, no PlannedDir
    assert_eq!(check_folder(Path::new("/app/backups/x"), &[PathBuf::from("/app/backups")]), Err(FolderProblem::OwnFolder));
}
#[test] fn roles_put_captures_first_and_skip_subfolders_and_excluded_roots() {}
```

`engine.rs` uses a `FakeFs { files: RefCell<HashMap<PathBuf, FakeFile { probe: Probe, head: Vec<u8>, marked: bool, own: bool }>> }`, fake time, and the `SettleConfig::default()` timeline: a file that arrives at 0 settles at 2000 ms, and a folder burst flushes at 4000 ms. Helper `png_head()` returns the 8 PNG signature bytes plus a fake IHDR. Capture watching is off in `Watch::default()`, so every test that expects a capture `Ready` builds its `Watch` with `captures.screenshots = true` and `captures.recordings = true` (helper `captures_on()`); `capture_toggles_gate_screenshots_and_recordings` also checks that `Watch::default()` produces no capture `Ready`:

```rust
#[test] fn a_new_image_in_a_watched_folder_is_ready_after_the_burst_window() {
    // Arrived(/w/In/a.png) at 0 → tick at 2000 → [] (held) → tick at 4000 → [Ready{origin: Folder{index:0}, media: Image}]
}
#[test] fn ferrys_own_write_is_never_ready() { /* own: true → no Ready ever */ }
#[test] fn a_file_seen_with_the_same_stamp_is_not_ready_twice() { /* Arrived again later with same stamp → nothing */ }
#[test] fn a_file_that_arrives_again_with_new_content_is_ready_again() {}
#[test] fn hidden_temp_and_ferry_tmp_names_are_ignored() {}
#[test] fn files_in_subfolders_and_excluded_roots_are_ignored() {}
#[test] fn more_than_max_files_at_once_are_all_skipped() {
    // max 3: 4 arrivals → [BurstSkipped{folder:"In", count:4, show_results:true}] and no Ready; 3 arrivals → 3 Ready
}
#[test] fn types_and_ignore_patterns_filter_a_folder() { /* video in image-only folder → none; "draft-*" ignores draft-1.png */ }
#[test] fn a_paused_folder_produces_nothing() {}
#[test] fn a_macos_screenshot_needs_the_marker_and_falls_through_to_the_folder_otherwise() {
    // /Users/me/Desktop is both a Marked capture dir and folder 0.
    // marked png → Ready{origin: Capture{capture: Screenshot}} at 2000 (no burst wait); unmarked png → Ready{origin: Folder{index:0}} at 4000
}
#[test] fn a_recording_is_known_by_marker_or_name() {}
#[test] fn capture_toggles_gate_screenshots_and_recordings() {
    // Watch::default() (both off): a marked screenshot and a marked recording → no Ready (capture dirs are Paused).
    // screenshots on, recordings off → only the screenshot is Ready; and the reverse.
}
#[test] fn captures_have_no_burst_limit() { /* 5 marked screenshots within 3 s → 5 Ready */ }
#[test] fn an_icloud_re_download_is_ignored() {
    // Gone(/w/In/.a.png.icloud) at 0, Arrived(/w/In/a.png) at 500 → nothing; same pair but Arrived at 11_000 → Ready
}
#[test] fn a_denied_file_marks_its_folder_denied() { /* Probe::Denied → Notice::Status with folders[0].state == Denied */ }
#[test] fn windows_capture_folders_take_every_image_and_video() {}
#[test] fn a_screenshot_name_with_narrow_nbsp_is_ready() { /* "Screenshot 2026-10-23 at 10.21.33\u{202F}AM.png", marked */ }
#[test] fn a_folder_listed_twice_reports_a_file_once() { /* folders "~/In" and "/home/me/In" resolve to the same dir → exactly one Ready */ }
#[test] fn a_file_pending_when_its_folder_is_removed_is_dropped() {
    // Arrived at 0; at 1000 set_plan(plan without that folder); ticks to 6000 → no Ready.
    // Same with folder.paused = true.
}
#[test] fn next_wake_covers_settling_and_bursts() {}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry-core watch::events watch::plan watch::engine`
Expected: FAIL.

- [ ] **Step 3: Add the dependency and implement the three files**

`cargo add -p ferry-core notify@8.2.0` (or edit `Cargo.toml` at the anchor). Then run `cargo tree -p ferry-core -e normal -i notify`. Check that the new transitive crates are only those listed in Decisions › Watch crate and that none is GPL/LGPL/AGPL (`cargo metadata --format-version 1 | jq` on the `license` fields, or read crates.io). Record the list in the commit body.

- [ ] **Step 4: Run the tests and check they pass**

Run: `cargo test -p ferry-core watch::` and `cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/Cargo.toml Cargo.lock crates/ferry-core/src/watch
git commit -m "feat(core): route settled files to captures or watched folders, once each"
```

---

### Task 6: Watch service thread and live no-loop tests

**Files:**
- Create: `crates/ferry-core/src/watch/service.rs`, `crates/ferry-core/src/watch/live_tests.rs`

**Interfaces:**
- Consumes: `Engine`, `plan::build`, `plan::resolve_dir`, `events::from_notify`, `RealFs` (Task 5).
- Produces (`watch::service`):
  - `pub const RESCAN_EVERY: Duration = Duration::from_secs(30);`
  - `pub struct ServiceConfig { pub watch: Watch, pub home: PathBuf, pub excluded: Vec<PathBuf>, pub captures: Box<dyn Fn() -> Vec<CaptureDir> + Send>, pub settle: SettleConfig, pub rescan_every: Duration }`
  - `pub fn spawn(config: ServiceConfig, on_notice: Box<dyn FnMut(Notice) + Send>) -> io::Result<ServiceHandle>`. The thread is named `ferry-watch`.
  - `pub struct ServiceHandle`, with `impl ServiceHandle { pub fn reload(&self, watch: Watch); pub fn rescan(&self); pub fn status(&self) -> WatchStatus }`. `impl Drop` sends Stop and joins.

The loop owns one `notify::RecommendedWatcher` whose callback sends `Msg::Fs(notify::Result<Event>)` into the same `std::sync::mpsc` channel as `Msg::{Reload(Watch), Rescan, Stop}`.

**Replan** (at start, on Reload, on Rescan, and every `rescan_every`):
- call `(config.captures)()`;
- build the plan with `resolve_dir`, giving excluded roots normalised with `resolve_dir` where they exist;
- diff `watch_dirs` against the currently watched set: `unwatch` the removed ones and `watch(dir, RecursiveMode::NonRecursive)` the added ones. A `watch` error sets that dir's state to `Failed { reason }`;
- `engine.set_plan`;
- if the status changed, store it in `Arc<Mutex<WatchStatus>>` and emit `Notice::Status`.

**Wait:** `recv_timeout(min(engine.next_wake(now), next_rescan - now))`.
- `Fs(Ok(ev))`: if `ev.need_rescan()` (FSEvents dropped events, or the Windows buffer overflowed), replan. Otherwise call `engine.on_event` for each `from_notify` result.
- `Fs(Err(_))`: ignore it. The next rescan repairs the watch.

After every wake, call `engine.tick(now, &RealFs)` and forward every notice. `Status` notices also update the shared status.

- [ ] **Step 1: Write the failing live tests** (`live_tests.rs`, `#[cfg(test)]`)

The `Harness` is built from a `WatchedFolder` list relative to a tempdir (`tmp/watched`; backups in `tmp/backups`; work in `tmp/work`; excluded = `[tmp/backups, tmp/work]`). It uses `SettleConfig { quiet: 200ms, stable: 300ms, poll: 100ms, max_wait: 60s, max_pending: 100 }`, `rescan_every: 3600s`, and a `captures` closure returning `vec![]`. Notices go into an mpsc channel.

Helpers:
- `expect_ready(&self, within: Duration) -> Ready`: panics on timeout, ignores `Status`.
- `expect_quiet(&self, for_: Duration)`: panics on any `Ready` or `BurstSkipped`.
- `wait_status(&self, f: impl Fn(&WatchStatus) -> bool, within)`.

Timeouts: `expect_ready` 15 s, `expect_quiet` 4 s (slow CI runners; FSEvents latency).

```rust
#[test] fn a_png_copied_into_a_watched_folder_is_ready_once() {
    // fs::write(watched/a.png, testutil::png(300,200)) → Ready{path ends "a.png", media: Image}; then expect_quiet
}
#[test] fn a_file_written_in_chunks_is_ready_only_after_the_last_chunk() {
    // 6 chunks, 150 ms apart; record Instant after the last write; Ready arrives after it; file len == full len
}
#[test] fn a_hidden_temp_file_renamed_into_place_is_ready_under_its_final_name() {
    // write ".Screenshot x.png-Ab12" then rename to "Screenshot x.png" → one Ready for the final name
}
#[test] fn ferry_overwriting_a_file_in_place_is_not_ready_again() {
    // Ready for a.png → build Job::file(&path, &Settings::default(), true), run it with Backups(tmp/backups)
    // and RunOptions::new(&settings, testutil::gifsicle(), tmp/work, test_trash, CancelToken::new())
    // assert state Done and the file is smaller (a real write happened) → expect_quiet
    // job.restore(..) → expect_quiet
}
#[test] fn ferrys_outputs_beside_the_source_and_conversions_are_not_ready() {
    // settings.images.output.optimised = SameFolder "%f_optimised.%e" → run → a_optimised.png exists → expect_quiet
    // job.edit(EditsPatch{ format: Some(Some(Format::WebP)), ..Default }) with converted = InPlace → run → a.webp exists → expect_quiet
}
#[test] fn a_card_rename_is_not_ready() { /* after a Done run: job.rename("renamed") → expect_quiet */ }
#[test] fn a_touched_or_tagged_ferry_output_is_not_ready() {
    // after Ferry's in-place write: set the mtime to now with File::set_modified, and on macOS set an xattr
    // (testutil::set_xattr) → expect_quiet
}
#[test] fn another_programs_save_over_a_ferry_output_is_ready() {
    // after Ferry's write: write "a.png.new" elsewhere then fs::rename over a.png with different bytes → Ready
}
#[test] fn a_missing_folder_starts_watching_when_it_appears() {
    // folder path not created → wait_status(folders[0] == Missing); create dir; handle.rescan()
    // → wait_status(Watching); write a png → Ready
}
#[test] fn a_folder_inside_ferrys_backups_is_refused() { /* folder tmp/backups/x → Failed{reason: own folder}; files there never Ready */ }
#[test] fn reload_starts_and_stops_watching() { /* reload with the folder paused → Paused, writes produce nothing; unpause → Ready */ }
```

`test_trash` copies the `pipeline.rs` test helper (moves into a hidden `.trash` folder beside the file, which is ignored by name and is a subfolder).

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry-core watch::live_tests`
Expected: FAIL (no `service`).

- [ ] **Step 3: Implement `service.rs`**

- [ ] **Step 4: Run the tests and check they pass, three times in a row** (flakiness check)

Run: `for i in 1 2 3; do cargo test -p ferry-core watch::live_tests || break; done`
Expected: PASS all three times on this Mac. Push the branch and confirm `core (windows-latest)` and `core (macos-latest)` are green before you continue (Windows ReadDirectoryChangesW path).

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/watch/service.rs crates/ferry-core/src/watch/live_tests.rs crates/ferry-core/src/watch/mod.rs
git commit -m "feat(core): watcher service on notify, with live tests that Ferry's writes never loop"
```

---

### Task 7: Platform capture folders (macOS, Windows)

**Files:**
- Create: `src-tauri/src/platform/macos_capture.rs`, `src-tauri/src/platform/windows_capture.rs`
- Modify: `src-tauri/src/platform/mod.rs` (anchor at end), `src-tauri/Cargo.toml` (features)

**Interfaces:**
- Consumes: `ferry_core::watch::capture::{macos_dirs, windows_dirs, CaptureDir}`.
- Produces: `crate::platform::capture::capture_dirs() -> Vec<ferry_core::watch::capture::CaptureDir>` on both OSes. It is called on the `ferry-watch` thread every 30 s and must not touch AppKit UI.

`platform/mod.rs` block:

```rust
// ---- Stream C: capture folders ----
#[cfg(target_os = "macos")]
#[path = "macos_capture.rs"]
pub mod capture;
#[cfg(windows)]
#[path = "windows_capture.rs"]
pub mod capture;
```

- macOS:
  - `NSUserDefaults::initWithSuiteName(NSUserDefaults::alloc(), Some(&NSString::from_str("com.apple.screencapture")))`, then `.stringForKey(&NSString::from_str("location"))` → `Option<String>`;
  - home = `std::env::home_dir()`;
  - return `macos_dirs(location.as_deref(), &home, &|p| p.is_dir())`.
  - Add `"NSUserDefaults"` to the `objc2-foundation` features list.
- Windows (all `cfg(windows)`):
  - `fn known_folder(id: &GUID) -> Option<PathBuf>`: `SHGetKnownFolderPath(id, KF_FLAG_DONT_VERIFY as u32, std::ptr::null_mut(), &mut pwstr)`. On `S_OK` (0), read the wide string up to NUL into an `OsString` (`OsStringExt::from_wide`). Always `CoTaskMemFree(pwstr.cast())`.
  - `capture_dirs()` = `windows_dirs(known_folder(&FOLDERID_Screenshots), known_folder(&FOLDERID_Videos))`.
  - Add `"Win32_UI_Shell"` and `"Win32_System_Com"` to `windows-sys` features.

- [ ] **Step 1: Write the failing test** (in `macos_capture.rs`, `#[cfg(test)]`; runs in `cargo test -p ferry` on macOS)

```rust
#[test]
fn there_is_always_one_capture_folder_on_macos() {
    let dirs = capture_dirs();
    assert_eq!(dirs.len(), 1);
    assert!(dirs[0].dir.is_absolute());
}
```

and in `windows_capture.rs`:

```rust
#[test]
fn the_screenshots_folder_is_under_the_user_profile() {
    let dirs = capture_dirs();
    assert!(dirs.len() >= 2, "{dirs:?}");
    assert!(dirs.iter().all(|d| d.dir.is_absolute()));
}
```

- [ ] **Step 2: Run the test and check it fails**

Run: `cargo test -p ferry platform::capture`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement both files and the `mod.rs` block**

- [ ] **Step 4: Run the test and check it passes**

Run: `cargo test -p ferry platform::capture` (macOS) and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS. The Windows test runs only in CI's `bundle (windows)` job (`cargo test -p ferry`). Push and confirm it is green. Call it out in the commit body as "cfg(windows), CI-built, not run on this Mac".

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(app): find the screenshot folder (macOS defaults, Windows known folders)"
```

---

### Task 8: App wiring: quiet jobs, hooks, dispatch, commands, settings effect

**Files:**
- Create: `src-tauri/src/watch/mod.rs`, `src-tauri/src/watch/hooks.rs`, `src-tauri/src/watch/commands.rs`
- Modify: `src-tauri/src/state.rs`, `src-tauri/src/jobs.rs`, `src-tauri/src/prefs.rs`, `src-tauri/src/lib.rs` (anchors)

**Interfaces:**
- Consumes: `ferry_core::watch::service::{spawn, ServiceConfig, ServiceHandle}`, `engine::{Notice, Ready}`, `plan::{check_folder, FolderProblem, WatchStatus}`, `media::Media`, `crate::platform::capture::capture_dirs`.
- Produces:
  - `state.rs`: `AppState.quiet: Mutex<HashSet<JobId>>`; `pub fn quiet_gate(&self, update: &JobUpdate) -> bool` (true means show it; a quiet job's `Error` removes it from the set and returns true; any other state of a quiet job returns false); `pub fn is_quiet(&self, id: &JobId) -> bool`.
  - `jobs::show`: right after `let state = app.state::<AppState>();`, add `// stream C (watch): quiet jobs show only their Error card.` and `if !state.quiet_gate(&update) { return; }`.
  - `watch/hooks.rs`:

    ```rust
    //! Stream C's hand-off points for media other streams add. Each `*_job`
    //! returns None ("unsupported") until its stream is merged; see the plan's
    //! "Cross-stream notes".
    pub const VIDEO: bool = false;
    pub const AUDIO: bool = false;
    pub const PDF: bool = false;
    #[derive(Debug, Clone, Copy, Serialize)]
    pub struct Support { pub image: bool, pub video: bool, pub audio: bool, pub pdf: bool }
    pub fn support() -> Support;                       // { image: true, video: VIDEO, audio: AUDIO, pdf: PDF }
    pub fn video_job(app: &AppHandle, path: &Path, settings: &Settings) -> Option<Result<Job, String>>; // None
    pub fn audio_job(app: &AppHandle, path: &Path, settings: &Settings) -> Option<Result<Job, String>>; // None
    pub fn pdf_job(app: &AppHandle, path: &Path, settings: &Settings) -> Option<Result<Job, String>>;   // None
    ```

  - `watch/mod.rs`:
    - `pub fn start(app: &AppHandle)`;
    - `fn excluded(app: &AppHandle) -> Vec<PathBuf>` (the one list `start` and `watch_check_folder` both use);
    - `pub fn reload(app: &AppHandle, watch: &Watch)`;
    - `pub fn status(app: &AppHandle) -> WatchStatus`;
    - `fn dispatch_ready(app: &AppHandle, ready: Ready)`;
    - managed `struct WatchService(Mutex<Option<ServiceHandle>>)`;
    - `pub mod commands; pub mod hooks;`.
  - `watch/commands.rs` (`#[tauri::command]`):
    - `watch_status(app) -> WatchStatus`
    - `watch_support() -> hooks::Support`
    - `watch_check_folder(app, path: String) -> Result<String, String>` (Ok is the `~`-contracted path)
    - `watch_pick_folder(app) -> Result<Option<String>, String>` (async, blocking dialog with title `t("watch.pick_folder")`, then the same check)
  - `prefs.rs`: `Effects.watch: bool` (`old.watch != new.watch`). `apply` adds `if effects.watch { crate::watch::reload(app, &new.watch); }` under `// stream C`.
  - Events: `watch:status` (payload `WatchStatus`), emitted with `app.emit` whenever the service reports a status.

`start(app)`:
- excluded = `excluded(app)` = `[state.backups_dir, state.work_dir, state.log_dir, app.path().app_config_dir()]`;
- home = `app.path().home_dir()`;
- `ServiceConfig { watch: settings.watch.clone(), captures: Box::new(platform::capture::capture_dirs), settle: SettleConfig::default(), rescan_every: RESCAN_EVERY, .. }`;
- `on_notice`:
  - `Ready(r)` → `tauri::async_runtime::spawn_blocking(move || dispatch_ready(&app, r))`;
  - `BurstSkipped { folder, count, show_results: true }` → `jobs::notice(app, tf("watch.burst_skipped", &[("count", …), ("folder", …)]))`; with `show_results: false` → `info!` only;
  - `Status(s)` → `app.emit("watch:status", &s)`.
- If `spawn` fails, `warn!` and continue. The app never fails to start because of watchers.

`dispatch_ready(app, ready)`, in this order:
1. `Stopped` → return (no read).
2. Build the job. `Media::Image` → `Some(Job::file(&ready.path, &settings, state.gifsicle.is_some()))`; `Video` / `Audio` / `Pdf` → the hook.
3. `None` → `debug!(media = ?ready.media, "no support for this media yet")`. `Some(Err(reason))` → `info!(%reason, "watched file left alone")`. No card in either case.
4. Note `was_paused`. If `!state.run_state.lock().unwrap().take_automatic()`, then `info!("skipped one watched file while paused or stopped")`, call `tray::rebuild` if it was paused, and return.
5. If `!ready.show_results`, insert `job.id` into `state.quiet`.
6. If `!jobs::start(app, job, Level::Normal, None)`, remove the id from `quiet` and `info!("already being optimised")`.
7. `info!(origin = ?ready.origin, "watched file started")`. **Stream I hook point:** auto-copy after watched-folder optimisation attaches here.

`watch_check_folder`:
- `expand_home(path)`;
- `check_folder(&dir, &excluded(app))`: `NotAFolder` → `Err(t("watch.error.not_folder"))`, `OwnFolder` → `Err(t("watch.error.own_folder"))`;
- `Ok(contract_home(&dir, &home))`.

- [ ] **Step 1: Write the failing tests**

`state.rs` tests:

```rust
#[test]
fn a_quiet_job_shows_no_card_until_it_fails() {
    let state = AppState::new(/* as in recent_results_keep_ten_newest_first_without_repeats */);
    let mut update = ferry_core::pipeline::notice("x".into());
    state.quiet.lock().unwrap().insert(update.id.clone());
    update.state = JobState::Queued;
    assert!(!state.quiet_gate(&update));
    update.state = JobState::Done;
    assert!(!state.quiet_gate(&update));
    update.state = JobState::Error { reason: "Can't write to ~/In".into() };
    assert!(state.quiet_gate(&update));
    update.state = JobState::Queued; // the user pressed Retry on the Error card
    assert!(state.quiet_gate(&update), "a failed quiet job becomes a normal card");
}
#[test]
fn a_quiet_job_stays_out_of_recent_results() {
    // insert id into quiet; remember(&update) → recent is empty; a normal update is remembered
}
```

`prefs.rs` test:

```rust
#[test]
fn a_watch_change_reloads_the_watchers() {
    let base = Settings::default();
    let mut s = base.clone();
    s.watch.captures.screenshots = true; // the default is off; turning it on is the change
    assert_eq!(Effects::between(&base, &s), Effects { watch: true, ..Effects::default() });
}
```

`hooks.rs` test:

```rust
#[test]
fn support_matches_the_hook_flags() {
    let s = support();
    assert!(s.image);
    assert_eq!((s.video, s.audio, s.pdf), (VIDEO, AUDIO, PDF));
}
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `cargo test -p ferry quiet watch_change support_matches`
Expected: FAIL.

- [ ] **Step 3: Implement, adding every anchor edit from "Shared-file edits"**

That covers `state.rs`, `jobs.rs`, `prefs.rs`, and `lib.rs` (`mod watch;`, the four commands `watch::commands::watch_status, watch::commands::watch_support, watch::commands::watch_check_folder, watch::commands::watch_pick_folder`, and `watch::start(app);`).

- [ ] **Step 4: Run the tests and check they pass**

Run: `cargo test -p ferry` and `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src
git commit -m "feat(app): start watched files as ordinary jobs, quietly when the folder says so"
```

---

### Task 9: Settings pane model: strings, schema rows, helpers

**Files:**
- Modify: `src/lib/i18n/en.json` (append at end), `src/lib/schema.ts`, `src/lib/schema.test.ts`
- Create: `src/lib/watch.ts`, `src/lib/watch.test.ts`

**Interfaces:**
- `en.json` keys (exact copy):

```json
"watch.pane": "Watched folders",
"watch.group.captures": "Screenshots and recordings",
"watch.group.folders": "Folders",
"watch.screenshots": "Optimise new screenshots",
"watch.screenshots.help": "Screenshots you save are optimised where they land. The original is backed up first.",
"watch.recordings": "Optimise new screen recordings",
"watch.recordings.help": "Recordings are re-encoded once they finish saving.",
"watch.capture_results": "Show a card for screenshots and recordings",
"watch.capture_results.help": "Off: they are optimised silently. Errors always show a card.",
"watch.capture_dirs": "Capture folders",
"watch.capture_dirs.help": "Where your screenshots and screen recordings are saved.",
"watch.folders": "Watched folders",
"watch.folders.help": "New files that arrive in these folders are optimised. Files already there are left alone.",
"watch.add": "Add folder…",
"watch.drop_hint": "or drop a folder here",
"watch.empty": "No watched folders yet",
"watch.col.folder": "Folder",
"watch.col.types": "Types",
"watch.col.card": "Card",
"watch.col.ignore": "Ignore",
"watch.col.at_once": "At once",
"watch.type.image": "Images",
"watch.type.video": "Videos",
"watch.type.audio": "Audio",
"watch.type.pdf": "PDFs",
"watch.unsupported": "Not available in this version yet",
"watch.ignore.placeholder": "*.tmp, draft-*",
"watch.ignore.aria": "Ignore rules for {folder}",
"watch.at_once.aria": "Skip when more than this many files arrive at once in {folder}",
"watch.card.aria": "Show a result card for {folder}",
"watch.pause": "Pause",
"watch.resume": "Resume",
"watch.remove": "Stop watching {folder}",
"watch.status.watching": "Watching",
"watch.status.missing": "Missing — will retry",
"watch.status.not_yet": "Appears after your first capture",
"watch.status.paused": "Paused",
"watch.status.denied": "No access — allow Ferry in your privacy settings",
"watch.status.failed": "Not watching: {reason}",
"watch.status.timed_out": "The folder did not answer in time",
"watch.error.not_folder": "That isn't a folder",
"watch.error.own_folder": "Ferry's own folders can't be watched",
"watch.error.duplicate": "Already watched",
"watch.pick_folder": "Choose a folder to watch",
"watch.burst_skipped": "Skipped {count} files that arrived in {folder} at once"
```

- `schema.ts` additions (each marked `// stream C`):
  - `PaneId` gains `'watch'`.
  - `panes` gets `{ id: 'watch', label: 'watch.pane' }` appended after `floating`.
  - `Control` gains `| { kind: 'capture_dirs' } | { kind: 'watch_folders' }`.
  - `Setting` gains `/** Media support this row needs (stream C hooks); off → disabled with watch.unsupported. */ requires?: 'video' | 'audio' | 'pdf';`.
  - Rows appended at the end of `all`, all with `pane: 'watch'`:
    - `watch_screenshots` (group `watch.group.captures`, `watch.captures.screenshots`, switch)
    - `watch_recordings` (same group, `watch.captures.recordings`, switch, `requires: 'video'`)
    - `watch_capture_results` (same group, `watch.captures.show_results`, switch)
    - `watch_capture_dirs` (same group, label `watch.capture_dirs`, help `watch.capture_dirs.help`, no path, `{ kind: 'capture_dirs' }`)
    - `watch_folders` (group `watch.group.folders`, label `watch.folders`, help `watch.folders.help`, path `watch.folders`, `{ kind: 'watch_folders' }`)
- `src/lib/watch.ts`:

```ts
export type DirState =
  | { kind: 'watching' } | { kind: 'missing' } | { kind: 'paused' } | { kind: 'denied' }
  | { kind: 'failed'; reason: string };
export interface DirStatus { path: string; state: DirState }
export interface WatchStatus { captures: DirStatus[]; folders: DirStatus[] }
export interface Support { image: boolean; video: boolean; audio: boolean; pdf: boolean }
export const MEDIA = ['image', 'video', 'audio', 'pdf'] as const;
export type Media = (typeof MEDIA)[number];
export function newFolder(path: string): WatchedFolder;            // Rust WatchedFolder::default() with path
export function addFolder(folders: WatchedFolder[], path: string): { folders: WatchedFolder[]; duplicate: boolean };
export function updateFolder(folders: WatchedFolder[], index: number, patch: Partial<WatchedFolder>): WatchedFolder[];
export function removeFolder(folders: WatchedFolder[], index: number): WatchedFolder[];
export function parseIgnore(text: string): string[];                // split on , and newlines; trim; drop empty; dedupe
export function formatIgnore(list: string[]): string;              // join with ', '
export function statusFor(status: WatchStatus | null, path: string): DirState | null;
export function statusText(state: DirState, capture: boolean): string; // capture && missing → watch.status.not_yet
export function statusTone(state: DirState): 'ok' | 'warn' | 'off' | 'error';
export function mediaLabel(media: Media): StringKey;               // watch.type.*
```

- [ ] **Step 1: Write the failing tests**

`watch.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { addFolder, formatIgnore, newFolder, parseIgnore, removeFolder, statusFor, statusText, statusTone, updateFolder } from './watch';

describe('watched folders', () => {
  it('adds a folder with the Rust defaults and refuses a duplicate', () => {
    const first = addFolder([], '~/Downloads');
    expect(first.folders).toEqual([newFolder('~/Downloads')]);
    expect(first.folders[0]).toEqual({
      path: '~/Downloads', types: { image: true, video: true, audio: false, pdf: false },
      show_results: true, ignore: [], max_files_at_once: 10, paused: false,
    });
    expect(addFolder(first.folders, '~/Downloads')).toEqual({ folders: first.folders, duplicate: true });
  });
  it('updates one folder and keeps unknown fields', () => {
    const withUnknown = [{ ...newFolder('~/A'), pipeline: 'webp' }] as never;
    const next = updateFolder(withUnknown, 0, { paused: true });
    expect(next[0]).toMatchObject({ paused: true, pipeline: 'webp' });
  });
  it('removes by index', () => { expect(removeFolder([newFolder('a'), newFolder('b')], 0).map((f) => f.path)).toEqual(['b']); });
  it('parses ignore rules', () => {
    expect(parseIgnore(' *.tmp, draft-*\n*.tmp ,, ')).toEqual(['*.tmp', 'draft-*']);
    expect(formatIgnore(['*.tmp', 'draft-*'])).toBe('*.tmp, draft-*');
  });
  it('finds a folder status by its stored path and words it', () => {
    const status = { captures: [], folders: [{ path: '~/A', state: { kind: 'missing' as const } }] };
    expect(statusFor(status, '~/A')).toEqual({ kind: 'missing' });
    expect(statusFor(status, '~/B')).toBeNull();
    expect(statusText({ kind: 'missing' }, false)).toBe('Missing — will retry');
    expect(statusText({ kind: 'missing' }, true)).toBe('Appears after your first capture');
    expect(statusText({ kind: 'failed', reason: 'x' }, false)).toBe('Not watching: x');
    expect(statusTone({ kind: 'watching' })).toBe('ok');
    expect(statusTone({ kind: 'denied' })).toBe('error');
    expect(statusTone({ kind: 'paused' })).toBe('off');
  });
});
```

In `schema.test.ts`, add:

```ts
it('has the Watched folders pane with captures and folders', () => {
  const rows = all.filter((s) => s.pane === 'watch');
  expect(rows.map((s) => s.id)).toEqual([
    'watch_screenshots', 'watch_recordings', 'watch_capture_results', 'watch_capture_dirs', 'watch_folders',
  ]);
  expect(rows.find((s) => s.id === 'watch_recordings')?.requires).toBe('video');
  expect(panes.map((p) => p.id)).toContain('watch');
});
```

- [ ] **Step 2: Run the tests and check they fail**

Run: `pnpm test`
Expected: FAIL (`./watch` not found; the new schema test fails).

- [ ] **Step 3: Implement the strings, schema additions and `watch.ts`**

The existing schema tests ("points every value setting at a field that exists", "real strings for every label") must also pass with the new rows.

- [ ] **Step 4: Run the tests and check they pass**

Run: `pnpm check && pnpm test && cargo test -p ferry-core i18n`
Expected: PASS. `en.json` is still a flat string map, so `ferry-core` `i18n` loads it.

- [ ] **Step 5: Commit**

```bash
git add src/lib/i18n/en.json src/lib/schema.ts src/lib/schema.test.ts src/lib/watch.ts src/lib/watch.test.ts
git commit -m "feat(ui): Watched folders pane model, strings and helpers"
```

---

### Task 10: Settings pane UI (manual check)

**Files:**
- Create: `src/settings/watchStore.svelte.ts`, `src/settings/controls/CaptureDirs.svelte`, `src/settings/controls/WatchFolders.svelte`
- Modify: `src/settings/Row.svelte`, `src/settings/Settings.svelte`

**Interfaces:**
- `watchStore.svelte.ts`: `export const watch = $state<{ status: WatchStatus | null; support: Support | null }>({ status: null, support: null });` and `export async function loadWatch(): Promise<void>`. `loadWatch` is idempotent: the first call does `invoke('watch_status')`, `invoke('watch_support')` and `listen<WatchStatus>('watch:status', …)`.
- `CaptureDirs.svelte` props `{ labelledby: string }`: a list with one line per `watch.status.captures` entry, showing the path plus a status chip (`statusText(state, true)`, tone class).
- `WatchFolders.svelte` props `{ folders: WatchedFolder[]; disabled: boolean; labelledby: string; onchange: (folders: WatchedFolder[]) => void }`.
  - `<table aria-labelledby={labelledby}>` with header cells `watch.col.folder | watch.col.types | watch.col.card | watch.col.ignore | watch.col.at_once`, plus an unlabelled actions column.
  - Each row:
    - **Folder:** the path, with CSS ellipsis and a `title` showing the full path, and below it the status chip from `statusFor(watch.status, folder.path)`.
    - **Types:** four `<button aria-pressed>` chips, one per `MEDIA`. A chip whose `watch.support[m]` is false is `disabled` with `title={t('watch.unsupported')}`.
    - **Card:** the existing `Switch` (`aria-label` = `watch.card.aria`).
    - **Ignore:** a text input showing `formatIgnore(folder.ignore)` that commits `parseIgnore(value)` on blur or Enter.
    - **At once:** the existing `NumberField`, min 1, max 1000.
    - **Actions:** a Pause/Resume text button toggling `paused`, and a × button (`aria-label` = `watch.remove`).
  - Below the table: `watch.empty` when there are no folders; an `Add folder…` button (`invoke<string | null>('watch_pick_folder')`) and `watch.drop_hint`; an inline `role="alert"` message for errors (`watch.error.duplicate` or the command's `Err`).
  - Drop: `onMount` registers `getCurrentWebview().onDragDropEvent((e) => …)` from `@tauri-apps/api/webview`. On `e.payload.type === 'drop'`, call `invoke('watch_check_folder', { path })` for each path; Ok → `addFolder`; Err → message. Unlisten in the returned cleanup. The component is mounted only while the pane is open, so drops elsewhere do nothing.
- `Row.svelte`:
  - Import both controls.
  - Before `{:else if control.kind === 'action'}`, add `{:else if control.kind === 'capture_dirs'}<CaptureDirs labelledby={labelId} />` and `{:else if control.kind === 'watch_folders'}<WatchFolders folders={value as WatchedFolder[]} {disabled} labelledby={labelId} {onchange} />`.
  - Add `class:wide={control.kind === 'watch_folders'}` on `.row`, with CSS `.row.wide { flex-direction: column; align-items: stretch; } .row.wide .control { width: 100%; }`.
- `Settings.svelte`:
  - Import `watch, loadWatch` and call `void loadWatch()` in `onMount`.
  - Add `function unsupported(setting: Setting): boolean { return !!setting.requires && watch.support !== null && !watch.support[setting.requires]; }` (comment `// stream C`).
  - Change the two `Row` props to `disabled={lock !== null || unsupported(setting)}` and `hint={lock ? t('settings.turn_on_first', { name: t(lock.label) }) : unsupported(setting) ? t('watch.unsupported') : null}`.

- [ ] **Step 1: Implement the store, both controls and the Row/Settings edits**

- [ ] **Step 2: Type-check and run unit tests**

Run: `pnpm check && pnpm test`
Expected: 0 errors, 0 warnings; tests PASS.

- [ ] **Step 3: Manual check on this Mac** (`pnpm tauri dev`, open Settings from the tray)

Look at each of these and record the result in the commit body:
1. The sidebar shows **Watched folders** last. The pane shows the group "Screenshots and recordings" with three switches. On a fresh settings file "Optimise new screenshots" is **off**, "Optimise new screen recordings" is **off** and greyed with "Not available in this version yet", and "Show a card for screenshots and recordings" is on. The "Capture folders" row shows `~/Desktop` (or your ⇧⌘5 location) with a grey **Paused** chip. Turn "Optimise new screenshots" on: the chip turns green **Watching** within 1 s (macOS may first ask for Desktop access; allow it). Turn it off again: back to **Paused**.
2. **Add folder…** opens a picker titled "Choose a folder to watch". Picking `~/Downloads` adds a row: path, Images and Videos pressed, Videos/Audio/PDFs chips disabled with that tooltip, Card on, Ignore empty, At once 10, status **Watching**.
3. Dragging a Finder folder onto the pane adds it. Dragging the same folder again shows "Already watched". Dragging a file shows "That isn't a folder". Picking `~/Library/Application Support/io.ferry.desktop/backups` shows "Ferry's own folders can't be watched".
4. At the minimum window width (720 px) the table fits with no horizontal page scroll. Long paths end in "…" and show in full on hover.
5. Keyboard: Tab reaches every chip, switch, field and button in order. VoiceOver reads the type chips as toggle buttons with their pressed state, and the × as "Stop watching Downloads".
6. Pause → the chip turns grey **Paused** within 1 s. Resume → **Watching**.
7. Search "screenshot" in the sidebar lists the watch rows. Choosing one jumps to it and highlights it for 2 s.
8. Dark mode: chips and table stay readable, and status is never shown by colour alone (each chip has text).

- [ ] **Step 4: Commit**

```bash
git add src/settings
git commit -m "feat(ui): Watched folders pane with capture status, folder table and drop to add"
```

---

### Task 11: End-to-end check on this Mac and CI (manual check)

**Files:** none. The results go into the PR description or the final commit message.

- [ ] **Step 1: Full automated suite**

Run: `cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferry-core && cargo test -p ferry && pnpm check && pnpm test`
Expected: all green. Push and confirm every CI job is green: `core (macos-latest)`, `core (windows-latest)`, `web`, `bundle (macos)`, `bundle (windows)` (including the 10 s smoke test with watchers running).

- [ ] **Step 2: Manual checklist** (release build: `pnpm tauri build --bundles app`, then run `Ferry.app`; watch `~/Library/Logs/io.ferry.desktop/ferry.log`)

Look at each of these and record the result:
1. **First launch (defaults off):** start from no settings file (move `~/Library/Application Support/io.ferry.desktop/settings.json` aside) and run `tccutil reset SystemPolicyDesktopFolder io.ferry.desktop`. Launch Ferry. macOS shows **no** Desktop-access prompt. Settings › Watched folders shows "Optimise new screenshots" and "Optimise new screen recordings" off, and Capture folders shows **Paused**. Take a ⇧⌘4 screenshot: no card, the file keeps its size, and the log has no `watched file started`. Quit Ferry, put your 0.2.0 settings file back (upgrade case), run the `tccutil reset` again and relaunch: same result (no prompt, **Paused**, no job for a new screenshot). Then turn "Optimise new screenshots" on. macOS asks once whether Ferry may access the Desktop folder. After **Allow**, Capture folders shows **Watching** within 30 s. If you choose **Don't Allow**, it shows "No access — allow Ferry in your privacy settings" and the app keeps running (to try again, run the `tccutil reset` above and toggle the switch off and on). Leave "Optimise new screenshots" on for the items below.
2. **⇧⌘4 screenshot:** once the file appears on the Desktop (after the floating thumbnail goes), a card appears within about 3 s. Finder › Get Info shows a smaller size. `xattr -l <file>` still lists `com.apple.metadata:kMDItemIsScreenCapture`. The log shows exactly one `job started` for that file and no second one in the next 30 s.
3. **Location change:** ⇧⌘5 › Options › Other Location… → choose `~/Pictures`. Within 30 s the Capture folders row shows `~/Pictures`. The next screenshot there gets a card.
4. **Screen recording (answers the recording-marker question):** ⇧⌘5 › Record Selected Portion, record 5 s, stop. No card appears and the log has no `watched file started` for the `.mov` (recordings are off and unsupported until stream A merges). Run `xattr -l "<the .mov file>"` and check whether `com.apple.metadata:kMDItemIsScreenCapture` is listed. **Record the answer** as the exact line `Recordings carry kMDItemIsScreenCapture: yes` or `Recordings carry kMDItemIsScreenCapture: no` (with the macOS version) in the PR description and in the final commit message, and include the `xattr -l` output. Do not change the name fallback in `capture_kind` either way; the user decides that from your answer.
5. **Watched Downloads:** saving an image from Safari into Downloads gives one card. Copying 15 PNGs into Downloads at once gives one card "Skipped 15 files that arrived in Downloads at once" and no other cards.
6. **No loops with outputs beside the source:** set File handling › Optimised to **Same folder** and copy a PNG into Downloads. `x_optimised.png` appears and the log shows exactly one job. Restore on the card → no new job. Rename via the card's name chip → no new job.
7. **Quiet folder:** turn the folder's Card off and copy a PNG in. No card appears, the file is smaller, and tray › Recent results does not list it. Then make a job fail: `mkdir ~/ro && chmod a-w ~/ro`, set File handling › Optimised to Specific folder `~/ro`, and copy another PNG in. The Error card "Can't write to ~/ro" appears. Put File handling back to In place afterwards.
8. **Removable folder:** watch a folder on a USB stick and eject it. The status becomes "Missing — will retry" and the row stays in Settings. Re-insert it → **Watching** within 30 s.
9. **Run state:** tray › Stopped → a screenshot is not optimised. Tray › Paused → the next screenshot is skipped and the one after is optimised.
10. **Idle CPU:** with captures plus two watched folders, Activity Monitor shows Ferry's average CPU under 1% over 1 minute with no file activity (NFR-R1).
11. **iCloud** (if Desktop & Documents sync is on): Finder › Remove Download on an optimised screenshot, then Download Now → no new job in the log.

- [ ] **Step 3: Record Windows status**

State in the PR: "Windows: capture folders, lock check and placeholder skip are built and covered by CI tests on windows-latest; not verified on a real Windows desktop."

---

## Cross-stream notes (for merges into `feat/c3-all`)

- **Stream A (video):**
  - In `src-tauri/src/watch/hooks.rs`, set `pub const VIDEO: bool = true;`.
  - Replace the body of `video_job` with A's constructor for a file job on a video, e.g. `Some(Job::file(path, settings, has_gifsicle))` if A extends `Job::file`, or `Some(Job::video(…))`. Nothing else in stream C changes: recordings route automatically and the Settings chip and switch enable themselves.
  - Any file A writes into a user-visible folder must go through `fsutil::atomic_replace`, or A must call `ferry_core::selfwrite::record(&path)` once the file is final. Otherwise a watched folder sees it as new. `fsutil::set_times` refreshes recorded paths by itself.
  - Extend `watch::media::VIDEO_EXTENSIONS` if A accepts more containers. AVI is left out on purpose (A does not support it); do not add it back unless A adds AVI support.
  - Recordings stay off by default after the merge (`watch.captures.recordings = false`); the merge only enables the switch.
- **Stream B (audio):** same pattern with `AUDIO` / `audio_job` and `AUDIO_EXTENSIONS`.
- **Stream E (PDF, TIFF/HEIC/AVIF):** `PDF` / `pdf_job`. New image formats need no watcher change, because `watch::media::classify` uses `sniff::sniff`.
- **Stream F (pipelines):** add `pub pipeline: Option<String>` to `WatchedFolder`. Unknown fields already survive, as tested in Task 1. Branch in `src-tauri/src/watch/mod.rs::dispatch_ready` before step 2. `Ready.origin` carries `Folder { index }`.
- **Stream I (clipboard):** "auto-copy after watched-folder optimisation" hooks at `dispatch_ready` step 7, the marked hook point.
- **Conflicts to expect:** `settings.rs` field and `Default` lists, `lib.rs` module and handler lists, `en.json` tail, and `schema.ts` `PaneId`/`panes`/`Control`. All are additive; keep every stream's lines.

## User decisions on the former open questions (2026-10-23)

These are settled; the tasks above already follow them.

1. **Defaults:** screenshot and recording watching default to **off** (`watch.captures.screenshots/recordings = false`). Upgrading users see no change and no Desktop-access prompt until they turn it on (Task 1, Task 10 item 1, Task 11 item 1).
2. **macOS recordings:** keep the "Screen Recording" name fallback for `.mov` (xattr *or* name). Task 11 item 4 records whether recordings carry the xattr; the user decides later whether to drop the fallback.
3. **Recursion:** watched folders stay non-recursive. No per-folder option this cycle.
4. **Game Bar location:** use the default `Videos\Captures`; a moved Game Bar folder is not followed this cycle.
5. **Windows eject:** accepted. Watching a folder on a USB drive keeps a directory handle open, so Windows may refuse "Safely remove" until the folder is paused.
6. **AVI:** `.avi` is not a watched video format, because stream A does not support AVI (Task 3).
