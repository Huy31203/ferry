# Ferry — Cycle 1: Foundation + Phase 1 core loop

- **Date:** 2026-10-21
- **Status:** Approved design, awaiting spec review
- **Requirements:** [docs/requirements/ferry-optimize-requirements.md](../../requirements/ferry-optimize-requirements.md) (section numbers below refer to it as "R§")
- **UI design (Claude Design canvas):** https://claude.ai/artifact/3uQvNVg5WdxoWFo8gtcyds

## 1. Goal and success criteria

Stand up the Ferry repository, toolchain and CI, then ship Phase 1 of Ferry Optimize (R§6, phase 1) on macOS and Windows.

Cycle 1 is done when all of these hold:

1. `git push` to `main` produces a downloadable, installable Windows NSIS `.exe` and a macOS universal `.dmg` as GitHub Actions artifacts. Pushing a `v*` tag publishes both on a GitHub Release.
2. On this Mac (Intel, macOS 14.6), copying a screenshot replaces the clipboard with a smaller PNG and shows a result card. A 5 MB clipboard image completes in under 1.5 s (NFR-P2), and the card never takes focus from the frontmost app.
3. On a `windows-latest` CI runner, the `.exe` installs silently, the app launches and is still running after 10 s.
4. All `ferry-core` tests pass on Ubuntu, macOS and Windows.

Windows desktop behaviour (focus, clipboard listener, tray, drag-out) is **built but not verified on a real desktop** this cycle; nobody has a Windows machine yet. Status reports must say so until someone tests it on one.

## 2. Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Cycle scope | Foundation first, then Phase 1 core loop | A working `.exe` lands before any encoder work |
| Encoders | Hybrid: in-process Rust crates for PNG / JPEG / WebP; `gifsicle` as the one sidecar | Crates build with cargo on every target, so there are no per-OS binaries to find and bundle. In-memory encoding helps the 1.5 s clipboard target. gifsicle also sets up the sidecar runner that ffmpeg needs in Phase 2 |
| PNG | `imagequant` 4.x (pngquant's engine, GPL-3.0-or-later) then `oxipng` 10.x (MIT) | Same quality as pngquant |
| JPEG | `mozjpeg` 0.10.x with default `nasm_simd` | jpegli has no Rust binding we would rely on; re-evaluate in a later cycle |
| WebP | `webp` 0.3.x (libwebp) for still images; animated WebP is kept unchanged | Animation safety (R§2.4) without an animation encoder this cycle |
| GIF | `gifsicle` 1.98 built from pinned source, as a Tauri sidecar | No Rust equivalent; GPL-2 |
| Frontend | Svelte 5 + TypeScript + Vite, multi-page (`stack.html`, `settings.html`) | Least code for reactive card states and settings forms |
| Card windowing | One transparent, non-activating **stack** window, resized to fit its content | One webview and the least native code |
| macOS window | Turn the stack window into a non-activating `NSPanel` with `tauri-nspanel` 2.x | Floats over full-screen apps without activating Ferry |
| Windows window | Apply `WS_EX_NOACTIVATE \| WS_EX_TOOLWINDOW` to the stack window | Never takes focus and has no taskbar button |
| Installer | NSIS `.exe`, per-user install, WebView2 download bootstrapper | Matches the request; no admin prompt |
| Targets | Windows `x86_64-pc-windows-msvc`; macOS `universal-apple-darwin` | This Mac is Intel; CI runners are arm64 |
| Signing | Unsigned this cycle | Windows SmartScreen warns on install ("More info → Run anyway") |
| Release flow | CI on every push and PR; artifacts on `main`; GitHub Release on `v*` tags | Approved option |
| Licence | GPL-3.0-or-later | R§1; required by imagequant |
| Bundle identifier | `io.ferry.desktop` (placeholder) | Pending trademark and domain check (R§6) |
| Strings | English only, all externalised in one `en.json` shared by Rust and Svelte | R§3.7; Vietnamese in a later cycle |
| Design system | None; Ferry's own tokens from R§4.0 (see §7) | User choice |

## 3. Scope

**In scope (Phase 1):**

- **Tray icon and menu**, with Phase 1 items only (§5.6).
- **Global hotkeys:** optimise clipboard, optimise aggressively, cycle Running / Paused / Stopped.
- **Clipboard:**
  - Automatic optimisation of copied images (PNG, JPEG, GIF, WebP).
  - The optimise hotkey also handles image *files* copied in Finder or Explorer.
- **Result cards** in the states Queued, Optimising, Done, Already optimal, Original kept (larger), Error and Restored.
- **Card actions:** copy, drag out, show in folder, restore original, optimise aggressively ("Try aggressive"), retry, re-optimise, close and cancel.
- **Results stack** with the "Drag all" and "Clear all" bar.
- **Data safety:** backups, restore, pruning by age and force-clean of the working directory.
- **Settings window** with the General, Clipboard, Images and Floating results panes, plus search.
- **Housekeeping:** single instance (a second launch opens Settings), launch at login and structured logs.
- **CI/CD** as described in §9.

**Out of scope (later cycles):**
- **Watching and input:** folder and screenshot watchers, the drop zone, preset zones, the batch window and the compact list.
- **Media types:** video, audio, PDF, TIFF, HEIC, AVIF and animated-WebP optimisation.
- **Card tools:** downscale, crop, convert, the compression slider, compare and quick preview.
- **Card extras:** rename, the context menu and the format bar.
- **Automation:** pipelines, CLI, SDK and MCP.
- **App services:** the updater, secure send, sharing, onboarding, settings sync and the hotkey editor.
- **Release and polish:** Vietnamese, code signing, notarisation and Windows ARM64.

## 4. Architecture

### 4.1 Repository layout

```
ferry/
├─ Cargo.toml                    # workspace: crates/ferry-core, src-tauri
├─ LICENSE                       # GPL-3.0-or-later
├─ crates/ferry-core/            # pure Rust, no Tauri or OS UI deps
│  ├─ src/lib.rs
│  ├─ src/sniff.rs               # format detection by magic bytes
│  ├─ src/optimise/{mod,png,jpeg,webp,gif}.rs
│  ├─ src/sidecar.rs             # child-process runner (timeout, cancel, drained output)
│  ├─ src/backup.rs              # store / restore / prune originals
│  ├─ src/fsutil.rs              # atomic write, copy times, copy xattrs
│  ├─ src/template.rs            # %f %e %z %y %m %d %P render + validate
│  ├─ src/settings.rs            # Settings model, defaults, load/save, ~ paths
│  ├─ src/job.rs                 # JobId, JobState, JobUpdate (serde, sent to UI)
│  └─ tests/fixtures/            # tiny PNG/JPEG/GIF(static+animated)/WebP(still+animated)
├─ src-tauri/
│  ├─ Cargo.toml, tauri.conf.json, capabilities/*.json, icons/
│  ├─ binaries/                  # gifsicle-<target-triple>[.exe] (built, git-ignored)
│  └─ src/
│     ├─ main.rs, lib.rs         # builder, plugins, setup
│     ├─ state.rs                # AppState: settings, run state, job registry, recent results
│     ├─ jobs.rs                 # orchestrates a job end to end, emits job:update
│     ├─ clipboard.rs            # watcher loop + skip rules + self-write guard
│     ├─ tray.rs                 # tray icon (state glyphs) + menu
│     ├─ hotkeys.rs
│     ├─ windows.rs              # stack + settings windows, positioning
│     ├─ commands.rs             # #[tauri::command] handlers
│     ├─ i18n.rs                 # loads ../../src/lib/i18n/en.json via include_str!
│     └─ platform/{mod,macos,windows}.rs
├─ src/                          # Svelte 5 + TS
│  ├─ stack.html, settings.html
│  ├─ stack/{main.ts,Stack.svelte,Card.svelte,DragAllBar.svelte}
│  ├─ settings/{main.ts,Settings.svelte,Sidebar.svelte,panes/*.svelte,controls/*.svelte}
│  └─ lib/{ipc.ts,format.ts,fuzzy.ts,i18n.ts,i18n/en.json,theme.css}
├─ scripts/build-gifsicle.sh     # macOS (host or universal) + Windows (Git Bash + nmake)
├─ .github/workflows/{ci.yml,release.yml}
└─ docs/
```

### 4.2 `ferry-core` interfaces

```rust
pub enum Format { Png, Jpeg, Gif, WebP }
pub fn sniff(bytes: &[u8]) -> Option<Format>;          // content, never extension
pub fn is_animated(bytes: &[u8], f: Format) -> bool;   // GIF >1 frame, WebP ANIM chunk

pub enum Level { Normal, Aggressive }
pub struct OptimiseOptions {
    pub level: Level,
    pub compression: u8,          // 5..=100, from settings (default 80)
    pub strip_metadata: bool,
    pub gifsicle: Option<PathBuf>, // resolved sidecar path; None → GIF returns Unsupported
    pub cancel: CancelToken,
}
pub enum Outcome {
    Smaller { bytes: Vec<u8> },   // ≥1% smaller than input
    AlreadyOptimal,               // within 1%, or animated WebP
    Larger,                       // encoder output bigger than input
}
pub fn optimise(input: &[u8], f: Format, o: &OptimiseOptions) -> Result<Outcome, OptimiseError>;
```

How the level maps to each encoder (q = `compression`; aggressive = `max(5, q − 20)`):

| Format | Normal | Aggressive |
| --- | --- | --- |
| PNG | imagequant quality `0..=q`, speed 4; oxipng preset 2 | quality `0..=q−20`; oxipng preset 4 |
| JPEG | mozjpeg quality q, progressive, optimised Huffman | quality q−20 |
| WebP (still) | lossy quality q, method 4, alpha kept | quality q−20 |
| GIF | `gifsicle -O3 --lossy=L --colors 256`, where `L = round((100 − q) × 1.5)` | the same with q−20 |

Other modules:

- **`sidecar::run(path, args, stdin: &[u8], timeout, cancel) -> Result<Vec<u8>>`**
  - Writes stdin on its own thread and drains stdout and stderr continuously, so the child can never block.
  - Kills the child on timeout (60 s by default), on cancel, and when dropped.
  - Puts stderr into the error and the log.
- **`backup`**
  - `Backups::new(root)`.
  - `store_bytes(job, name, &[u8]) -> PathBuf` and `store_file(job, &Path) -> PathBuf`, both writing to `<root>/<yyyy-mm-dd>/<job-id>/<name>`.
  - `prune(retention: Duration, now) -> PruneReport`, which removes whole date folders older than the retention.
- **`fsutil`**
  - `atomic_replace(target, bytes)` writes `target.ferry-tmp` in the same directory, `fsync`s it, then renames it over the target.
  - `copy_times(from, to)` uses std `FileTimes`, including the created time on macOS and Windows.
  - `copy_xattrs(from, to)` is macOS only. It keeps `kMDItemIsScreenCapture` and the other xattrs, and times out after 5 s (NFR-P4).
- **`template::render(tpl, &Ctx) -> String` and `template::validate(tpl) -> Result<(), Vec<String>>`** (it returns the unknown tokens). The settings UI calls a Tauri command for its live example, so there is a single implementation.
- **`settings`**
  - `Settings::load(path)` returns defaults when the file is missing or corrupt, and keeps a `.bak` copy of a corrupt file.
  - `save` uses an atomic write.
  - Paths are stored with `~`.
- **`job`**
  - `JobState` is `Queued | Optimising{progress, step} | Done | AlreadyOptimal | Larger | Error{reason} | Restored`.
  - `JobUpdate` carries `{ id, state, name, ext, before, after, detail, thumb_path, source: Clipboard|File, can_restore }`.

### 4.3 Tauri app

- **Plugins:** `global-shortcut`, `single-instance`, `autostart`, `dialog` (for the ignored-apps picker), `opener` (to open folders), `drag` (`tauri-plugin-drag`) and `nspanel` (macOS only).
- **Windows:**
  - `stack` is transparent, has no decorations, stays always on top, skips the taskbar, can't take focus, has no shadow, is visible on all workspaces, is not resizable and starts hidden. Content protection follows `!show_in_screenshots`.
  - `settings` is decorated, 820×600, resizable with a minimum of 720×480, and created on demand.
- **Stack sizing:**
  - Svelte measures the stack with a `ResizeObserver` and calls `stack_resize(w, h)`.
  - Rust picks the monitor under the cursor (if "follow cursor" is on) and places the window in the chosen corner with a 16 px margin. On Windows it uses the monitor *work area* so the stack never covers the taskbar.
  - An empty stack hides the window.
- **Concurrency:** encoding runs on `tokio::task::spawn_blocking`, behind a semaphore sized to `available_parallelism()`. Clipboard polling and the Windows message loop run on dedicated threads. The UI thread never blocks.
- **Paths:**
  - Working directory: `app_cache_dir()/work`.
  - Backups: `app_data_dir()/backups`.
  - Settings: `app_config_dir()/settings.json`.
  - Logs: `app_log_dir()/ferry.log` (rolling daily, 7 files, via `tracing` + `tracing-appender`).

### 4.4 Platform module (`platform/{macos,windows}.rs`)

A single `pub` surface, with one implementation per OS:

```rust
pub struct ClipboardSnapshot { pub seq: u64, pub image: Option<(Format, Vec<u8>)>,
    pub files: Vec<PathBuf>, pub concealed: bool, pub source_app: Option<String> }
pub fn clipboard_watch(tx: Sender<u64>) -> WatchHandle;     // emits change sequence numbers
pub fn clipboard_read() -> Result<ClipboardSnapshot>;
pub fn clipboard_write_image(f: Format, bytes: &[u8], file: Option<&Path>) -> Result<u64>; // returns new seq
pub fn clipboard_write_files(paths: &[PathBuf]) -> Result<u64>;
pub fn make_non_activating(window: &WebviewWindow) -> Result<()>;
pub fn reveal_in_file_manager(path: &Path) -> Result<()>;
```

| Concern | macOS | Windows |
| --- | --- | --- |
| Watch | Poll `NSPasteboard.generalPasteboard.changeCount` every 250 ms on a thread | Hidden message-only window with `AddClipboardFormatListener` (via `clipboard-master`); seq = `GetClipboardSequenceNumber` |
| Read order | `public.png` → `public.jpeg` → `com.compuserve.gif` → `org.webmproject.webp`; files from `public.file-url` | Registered `PNG` → `JFIF`/`image/jpeg` → `GIF` → `image/webp`; if only `CF_DIBV5`/`CF_DIB` is present, encode it losslessly to PNG and treat that as the original; files from `CF_HDROP` |
| Skip as concealed | `org.nspasteboard.ConcealedType`, `org.nspasteboard.TransientType`, `org.nspasteboard.AutoGeneratedType`, `com.agilebits.onepassword` | `ExcludeClipboardContentFromMonitorProcessing` present, or `CanIncludeInClipboardHistory` = 0 |
| Source app | `NSWorkspace.frontmostApplication` bundle id | Clipboard owner window → process executable name |
| Write image | `clearContents`, then `setData` for the same UTI; plus `public.file-url` of the working file when "copy file paths" is on | `PNG` (or the original registered format) + `CF_DIBV5` decoded from the optimised image; `CF_HDROP` when "copy file paths" is on |
| Non-activating window | `tauri-nspanel`: `NSWindowStyleMaskNonactivatingPanel`, floating level, `canJoinAllSpaces \| fullScreenAuxiliary` | `SetWindowLongPtrW(GWL_EXSTYLE, …| WS_EX_NOACTIVATE \| WS_EX_TOOLWINDOW)` |

## 5. Behaviour

### 5.1 Run state

- **Running:** automatic work happens.
- **Paused:** skips exactly one automatic trigger, then returns to Running.
- **Stopped:** does no automatic work.
- Hotkeys and card actions always work, whatever the state.
- The cycle-state hotkey and the tray switch set the state; the tray glyph reflects it (§5.6).
- The start state comes from settings.

### 5.2 Clipboard trigger (automatic)

1. A change arrives with sequence number `seq`. Ignore it if `seq` is one our own write recorded.
2. Read the snapshot, then skip if any of these is true:
   - the run state is not Running (Paused is consumed here);
   - `clipboard.enabled` is off;
   - the content is concealed;
   - the source app is on the ignore list;
   - there is no supported image (copied files are ignored in automatic mode);
   - the image is larger than `clipboard.max_size_mb`;
   - the format's extension is in `images.ignored_extensions`;
   - the image is an animated GIF and the gifsicle sidecar is missing.
3. Start a job with source `Clipboard` and continue with §5.4.

### 5.3 Hotkey trigger

1. **Optimise clipboard** and **Optimise aggressively** run whatever the run state is.
2. **If the clipboard holds image data**, it is handled as in §5.2, with level Normal or Aggressive.
3. **If it holds image files** (Finder or Explorer copy):
   - when `clipboard.skip_multiple_files` is on and there is more than one file, nothing happens apart from a single Error card ("More than one file copied");
   - otherwise each supported file becomes a `File` job, optimised in place (§5.4).
4. **If nothing usable is on the clipboard**, an Error card says "Nothing to optimise on the clipboard".

### 5.4 Job lifecycle

```
Queued ─► Optimising ─┬─► Done ──(restore)──► Restored ──(re-optimise)──► Optimising
                      ├─► AlreadyOptimal
                      ├─► Larger ──(try aggressive)──► Optimising
                      └─► Error ──(retry)──► Optimising
any non-terminal ──(cancel)──► card removed, nothing changed
```

1. **Show the card right away (NFR-P3).** Emit `Queued` with the name and size.
2. **Back up first.** Clipboard jobs store the original bytes; file jobs copy the original file. If the backup fails, the job goes to `Error`.
3. **Optimise:** `sniff`, then `optimise`. Emit `Optimising` with a step label ("Quantising colours", "Encoding JPEG", "Running gifsicle").
4. **Write the output.**
   - *Clipboard:* write the result to `work/clipboard/<rendered template>.<ext>` and put it on the clipboard (§4.4). Record the returned `seq` as a self-write. If "accumulate results" is on, the clipboard instead gets the file list of every clipboard result since the app started or since the last "Clear all".
   - *File:* `atomic_replace` the original in place, then `copy_xattrs`. If `images.preserve_dates` is on, also `copy_times`.
5. **Finish.** Emit `Done` (before, after, saving, detail line), `AlreadyOptimal` or `Larger`, and add the job to the recent results (the last 10).
6. **Re-run from the original.** Every re-run (retry, try aggressive, re-optimise) starts from the backed-up original (R§2.4).

### 5.5 Card actions

| Action | Effect |
| --- | --- |
| Copy | Clipboard job: write the optimised image again. File job: put the file on the clipboard. Records the self-write |
| Drag out | `tauri-plugin-drag` `startDrag` with the output file path and the thumbnail as icon. Drag all: every Done / AlreadyOptimal output |
| Show in folder | Reveal the output file (working file for clipboard jobs) |
| Restore original | File job: `atomic_replace` with the backup bytes. Clipboard job: write the original bytes back to the clipboard and to the working file. Then → Restored |
| Try aggressive | Re-run at Level Aggressive from the original |
| Retry / Re-optimise | Re-run at the original level from the original |
| Cancel | Cancel the token; sidecar killed; card removed |
| Close / Clear all | Remove the card(s); jobs and backups are unaffected |
| Details (Error) | Reveal `ferry.log` in the file manager |

**Auto-hide:**
- After `floating.auto_hide_secs` (default 6; 0 means never), cards in Done, AlreadyOptimal, Larger or Restored fade out.
- Hover or keyboard focus pauses the timer.
- Error, Queued and Optimising cards never auto-hide.

### 5.6 Tray

- **Glyph:** a monochrome template image of the ferry hull on a wave line, in three styles (default, classic, flat).
- **State variants:** Running, Paused (pause badge), Stopped (dimmed with a slash), Working (filled hull while any job runs), Error (dot until the menu opens). "Update available" is not used this cycle.
- **Menu:**
  - Status header with Running / Paused / Stopped as radio items;
  - Optimise clipboard, Optimise clipboard aggressively;
  - Recent results ▸ (the last 10; clicking one re-shows its card);
  - Open backups folder, Open working directory, Force clean working directory;
  - ✓ Show app UI in screenshots;
  - Settings…, About Ferry, Quit Ferry.
- **Hotkey labels** use the OS's own symbols: ⌃⇧C on macOS, Ctrl+Alt+Shift+C on Windows.
- **Clicks on Windows:** a single click opens the menu; a double-click opens Settings.

### 5.7 Settings model (defaults)

```jsonc
{
  "version": 1,
  "general":   { "launch_at_login": true, "start_state": "running", "show_dock_icon": false,
                 "show_in_screenshots": false, "tray_style": "default", "language": "en" },
  "clipboard": { "enabled": true, "skip_multiple_files": true, "max_size_mb": 50,
                 "ignored_apps": [], "accumulate": false, "copy_paths": false,
                 "file_template": "%f" },
  "images":    { "compression": 80, "adaptive": false, "switch_png_jpeg": false,
                 "strip_metadata": true, "preserve_dates": true,
                 "ignored_extensions": [], "max_size_mb": 50 },
  "floating":  { "enabled": true, "corner": "bottom_right", "card_size": "default",
                 "auto_hide_secs": 6, "follow_cursor": true },
  "backups":   { "retention_days": 7 },
  "hotkeys":   { "optimise": "<os default>", "aggressive": "<os default>", "cycle_state": "<os default>" }
}
```

Default hotkeys are `Ctrl+Shift+C/A/P` on macOS and `Ctrl+Alt+Shift+C/A/P` on Windows. Unknown fields are kept when the file is saved; missing fields take their defaults. `adaptive` and `switch_png_jpeg` are stored and shown, but this cycle they make no difference to the output; their help text says "Coming soon".

### 5.8 Metadata rules (`strip_metadata`)

- **PNG:** strip non-rendering chunks but keep `iCCP`, `sRGB`, `gAMA`, `cHRM` and `pHYs` (DPI).
- **JPEG:** always keep the ICC profile (APP2) and the JFIF density (DPI).
  - When stripping, drop EXIF but write a minimal EXIF segment that holds only `Orientation`, so photos never rotate.
  - When not stripping, copy the original APP1 EXIF across.
- **WebP:** keep the `ICCP` chunk; drop `EXIF` and `XMP` when stripping.
- **GIF:** gifsicle default (comments and extensions dropped by `-O3`; application extensions for looping kept).
- **Screenshot marker:** on macOS this is a file xattr, kept by `copy_xattrs` (§4.2).

## 6. Error handling

- **Every job ends** in Done, AlreadyOptimal, Larger, Error or cancelled. A panic inside `spawn_blocking` shows up as a `JoinError` and becomes Error ("Unexpected error — see Details").
- **The original is never modified before its backup is confirmed.** In-place writes are always temp-then-rename, so a failed write leaves the original as it was.
- **Error reasons are one line and say what to do:**
  - "Can't write to ~/Shared" for a read-only folder;
  - "Backup failed: disk full";
  - "gifsicle missing — reinstall Ferry";
  - "Encoder failed (PNG)", with stderr in the log;
  - "Nothing to optimise on the clipboard".
- **Settings errors:** a corrupt settings file is kept as `settings.json.bak`, defaults load, and an Error card says "Settings were reset; old file kept".
- **Startup errors:** if a global hotkey can't be registered (a conflict), it is logged and that hotkey is left out of its tray menu item. The app keeps running.
- **Clipboard read failures:** retried once after 100 ms, then logged; no card is shown for automatic triggers.
- **Logs** hold every job's start, end, sizes, timings, sidecar command lines and stderr (R§3.7). No file contents are logged.

## 7. UI

The UI follows the approved Claude Design canvas (link above). Its boards are the card states, the card component, the stack on a desktop, the tray menu and the settings window, each with `dark` and `mac`/`windows` variants.

**Tokens** (`src/lib/theme.css`, CSS custom properties with light and dark sets under `prefers-color-scheme`):

| Token | Light | Dark |
| --- | --- | --- |
| `--bg-content` | `#F5F5F7` | `#1C1C1E` |
| `--bg-group` | `#FFFFFF` | `#2C2C2E` |
| `--bg-sidebar` | `#E9E9ED` | `#232325` |
| `--fg` / `--fg-2` | `#1D1D1F` / `#5E5E63` | `#F5F5F7` / `#A1A1A6` |
| `--accent` (fills, selection, white text on top) | `#0A64D8` | `#0A64D8` |
| `--accent-text` | `#0A64D8` | `#6CB4FF` |
| `--track-off` | `#8E8E93` | `#7C7C80` |
| `--error` | `#C42B1C` | `#FF8A80` |
| `--rail-bg` | `rgba(250,250,250,.92)` | `rgba(36,36,38,.9)` |

Card text always sits on a dark bottom scrim (`rgba(0,0,0,.8)` fading to 0 over 64% of the height), so the info strip uses fixed colours: white text; green pill `rgba(48,209,88,.26)` with `#8CF5AE` text; amber pill `rgba(255,159,10,.28)` with `#FFD27A`; grey pill `rgba(255,255,255,.2)` with `#F2F2F7`; error ring `#FF453A` with `#FFB3AE` text.

On macOS, Rust reads `NSColor.controlAccentColor` at startup and passes it to the UI as `--accent` (R§3.5). Windows uses `#0A64D8` this cycle, and accent changes while the app runs are picked up on the next launch.

- **Fonts:** `-apple-system, "SF Pro Text"` on macOS; `"Segoe UI Variable Text", "Segoe UI"` on Windows. Sizes are 11 / 12 / 13 / 15 / 20, and sizes and percentages use tabular figures.
- **Radius:** cards 12 on macOS and 8 on Windows; chips fully rounded.
- **Icons:** Lucide outline paths, inlined as SVG.
- **Motion:** cards slide in over 180 ms and fade out over 150 ms; none at all under `prefers-reduced-motion`.

**Card anatomy (300×200, compact 220×150):**
- **Thumbnail:** fills the card.
- **Dim layer:** shown while Optimising and on Error.
- **Progress ring:** 56 px, while Optimising.
- **Bottom scrim.**
- **Rail:** on the left, the screen-facing edge for a bottom-right stack. It holds Copy, Show in folder, Restore (Done only), Optimise aggressively and a disabled "+" slot (hidden in compact). It appears on hover or keyboard focus, not while Queued or Optimising.
- **Close button:** top-right, on hover or focus.
- **Info strip:**
  - the filename chip and extension chip, both read-only this cycle;
  - a line that depends on the state;
  - the detail line (Done only, hidden in compact).

**Accessibility (R§3.6):**
- **Keyboard and screen readers:** each card is an `<article>` with an accessible name ("name.ext, state"), and every icon button has an `aria-label`. Focusing a card shows the same controls as hovering it.
- **Not colour alone:** every status pill pairs its colour with an icon and words.
- **Settings controls are real elements:** `role="switch"` buttons, range inputs and labelled fields.
- **Search:** settings search matches each whitespace-separated word against the label, help text and pane name. Picking a result jumps to it and highlights the row for 2 s.
- **Template field:** shows its live example, or the red text "Unknown token %q" with `aria-invalid` set.
- **Dependent settings:** when the setting they depend on is off, they grey out and show "Turn on '…' first."

## 8. Testing

- **`ferry-core` unit and integration tests**, run in CI on Ubuntu, macOS and Windows:
  - `sniff` gets the format right regardless of the file extension;
  - never-worse: `classify(input_len, output_len)` covers the Smaller / AlreadyOptimal (within 1%) / Larger boundaries, and re-optimising an already-optimised fixture gives AlreadyOptimal;
  - each format optimises a realistic fixture to `Smaller`, and the output decodes;
  - an animated GIF stays animated with the same frame count (when the gifsicle sidecar is present; `FERRY_GIFSICLE` env); an animated WebP gives AlreadyOptimal;
  - metadata: stripping keeps the ICC profile and JPEG orientation;
  - backup store → restore round-trip, and prune by date;
  - `atomic_replace` leaves the original untouched when the rename fails;
  - template render and validate;
  - settings defaults, unknown-field round-trip, and recovery from a corrupt file;
  - `sidecar::run` kills the child on timeout and on cancel, and drains large stderr without deadlocking.
- **Frontend:** `svelte-check` for types; `vitest` for `format.ts` (byte sizes, percentages) and `fuzzy.ts`.
- **App crate:** `cargo clippy -D warnings` on macOS and Windows in the bundle jobs. There are no automated UI tests this cycle.
- **Manual checklist on this Mac** (run before claiming M2 done; each result recorded in the PR or commit message):
  1. Copying a screenshot replaces the clipboard with a smaller image, pasted into Slack or a browser.
  2. A 5 MB PNG round trip takes under 1.5 s (from the log timings).
  3. The frontmost app keeps focus while a card appears and while hovering it.
  4. Drag-out into Finder and into a browser upload works.
  5. Restore puts the original back.
  6. Paused skips exactly one copy.
  7. Copying from a password manager (concealed) is skipped.
  8. The stack follows the cursor between monitors, if two are available.
  9. Cards are hidden from screenshots unless the setting is on.
- **Windows:** the CI smoke test only (§9). Everything else is marked unverified.

## 9. CI/CD

**`.github/workflows/ci.yml`** runs on `push` (all branches) and `pull_request`:

| Job | Runner | Steps |
| --- | --- | --- |
| `core` | matrix ubuntu-latest, macos-latest, windows-latest | Rust stable with cache. Build the gifsicle sidecar for the host. `cargo fmt --check` (Ubuntu only). `cargo clippy -p ferry-core -D warnings`. `cargo test -p ferry-core` with `FERRY_GIFSICLE` set |
| `web` | ubuntu-latest | Node 22 and pnpm. `pnpm install --frozen-lockfile`, `pnpm check`, `pnpm test` |
| `bundle` (needs core, web) | matrix windows-latest (`x86_64-pc-windows-msvc`, `--bundles nsis`) and macos-latest (`universal-apple-darwin`, `--bundles dmg`) | Install NASM. Build the gifsicle sidecar for the target. `cargo clippy --workspace -D warnings`. `pnpm tauri build`. **Windows smoke test:** run the NSIS `.exe /S`, start `%LOCALAPPDATA%\Ferry\Ferry.exe`, wait 10 s, assert the process is alive, stop it. On `main`, `actions/upload-artifact` the `.exe` / `.dmg` (90-day retention) |

**`.github/workflows/release.yml`** runs on `push` of tags `v*` and has `permissions: contents: write`:
- matrix windows-latest and macos-latest (same targets);
- installs NASM and builds the gifsicle sidecar;
- runs `tauri-apps/tauri-action` (current major, checked when the plan is written) with `tagName: ${{ github.ref_name }}`, release name `Ferry ${{ github.ref_name }}`, not a draft, uploading the `.exe` and `.dmg`.

The app version comes from `src-tauri/tauri.conf.json` and is bumped by hand before tagging.

**gifsicle sidecar (`scripts/build-gifsicle.sh <target-triple>`):**
- Downloads the pinned `gifsicle-1.98` source tarball and checks its SHA-256 (recorded in the script).
- **macOS:** `./configure --disable-gifview && make`, with `-arch x86_64 -arch arm64` for `universal-apple-darwin`.
- **Windows:** `nmake -f Makefile.w32` from Git Bash after `ilammy/msvc-dev-cmd`.
- Writes `src-tauri/binaries/gifsicle-<triple>[.exe]`, stripped of symbols.
- If the Windows MSVC build doesn't work, fall back to MSYS2 `mingw-w64-x86_64-gcc` with `Makefile.mingw`, statically linked.

## 10. Local environment (this Mac)

- **Rust:** `rustup` stable with targets `x86_64-apple-darwin` and `aarch64-apple-darwin` (the latter for universal builds).
- **Node 22 LTS:** via the existing `nvm`, made the default. pnpm 11 needs it (it currently fails on Node 20).
- **NASM:** `brew install nasm`, needed by mozjpeg SIMD.
- **Xcode Command Line Tools:** already present (Xcode 16.2).
- **`gh` CLI:** optional. Creating the GitHub repository and the first push need the user's confirmation, because they are outward-facing.

## 11. Milestones

**M1 — Foundation:**
- toolchain installed;
- the workspace scaffold builds;
- a tray icon with a menu shell (Settings, About, Quit), with Settings opening an empty settings window;
- the empty stack window is made non-activating and shows a test card from a debug menu item;
- `ferry-core` skeleton with `sniff` and its tests;
- gifsicle script;
- CI green with both installers produced.

*Exit:* the CI-built `.exe` installs and launches on the runner, and the `.dmg` runs on this Mac.

**M2 — Phase 1 core loop:** everything else in §3 In scope.

*Exit:* §1 criteria 2–4 hold and the §8 manual checklist passes on this Mac.

## 12. Risks and open questions

- **Hover in non-key windows (macOS).** `WKWebView` may not update `:hover` in a non-activating panel that is not key. M1 includes a spike: if hover fails, Rust polls the cursor position while cards are visible and the frontend uses that hover state instead of CSS `:hover`.
- **Windows behaviour unverified.** No Windows machine is available; the CI smoke test only proves install and launch.
- **gifsicle on MSVC.** `Makefile.w32` may need small fixes; the mingw fallback is defined in §9.
- **Clipboard DIB-only sources on Windows.** The savings shown are measured against a lossless PNG of the pixels, not against the raw DIB. The card's detail line says "from bitmap".
- **SmartScreen.** The unsigned `.exe` shows a warning until signing is added in a later cycle.
- **Trademark.** The `io.ferry.desktop` identifier and the "Ferry" name are placeholders (R§6).
