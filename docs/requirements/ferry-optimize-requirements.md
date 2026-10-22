# Ferry Optimize (Tauri 2) — Requirements & UI Spec

Oct 21, 2026

## 1. Overview

Ferry is a background file utility for macOS and Windows, and its first module, Ferry Optimize, is a media optimiser: it watches the clipboard, folders and drag-and-drop, shrinks images, videos, audio and PDFs on-device, and shows each result as an interactive floating card. It is built with Tauri 2 (Rust core, web UI) and wraps proven open-source encoders.

**Free, with every feature included.** There is no paywall, trial, licence key, account or usage limit; every capability in this spec is available to every user from first launch.

**Name and suite structure.** The product is called **Ferry** because it carries files between apps, shrinking, holding or converting them on the way. The name holds as the app grows into a suite of modules that share one tray icon, one settings window and one floating-card component.

| Module | What it does | Release |
| --- | --- | --- |
| Ferry Optimize | Shrinks and converts images, video, audio and PDFs (this spec) | v1 |
| Ferry Shelf | Drop shelf that holds files, images and text while the user moves between apps | Later |
| Ferry Clipboard | Clipboard history with search, pinning, and paste as optimised | Later |
| Ferry Send | Encrypted peer-to-peer links (secure send in this spec), can become its own module | Later |

To keep that expansion cheap, the tray menu, settings sidebar, result card and hotkey system are built module-agnostic from v1: the settings sidebar groups panes under a module heading, and each module registers its own tray items, cards and hotkeys. The name and wordmark need a trademark and domain check before launch.

**Toolchain.** Every optimisation is a call to a bundled command-line tool, shipped as Tauri sidecars with one build per OS and CPU.

| Media | Tool | Role | Licence (verify before shipping) |
| --- | --- | --- | --- |
| PNG | pngquant | Lossy palette compression | GPL-3 or commercial |
| JPEG | jpegoptim + jpegli | Re-encode with Google's perceptual encoder | jpegoptim GPL-3, jpegli BSD |
| GIF | gifsicle | Optimise frames, cap colours at 256 | GPL-2 |
| Video, audio | ffmpeg | H.264, HEVC, AV1, WebM; AAC, MP3, Opus; speed, mute | LGPL or GPL depending on build |
| Resize, crop, convert | libvips (`vipsthumbnail`), libheif, libwebp, libjxl | Downscale, smart crop, HEIC/AVIF/WebP/JXL I/O | LGPL / BSD |
| Video to GIF | gifski | High-quality GIF export | AGPL-3 or commercial |
| PDF | Ghostscript 10.x | Recompress images, downsample DPI | AGPL-3 or commercial |

**Distribution licence.** Bundling pngquant, gifski and Ghostscript requires the app itself to be distributed as GPL/AGPL-compatible open source. Since the app is free, releasing it under GPL-3 (or AGPL-3) is the simplest compliant path. The licences above are from general knowledge and must be verified against each project.

**Scope.** Everything in this spec ships on both OSes. Apple-only services (macOS Shortcuts, Services menu, AirDrop) get Windows counterparts as mapped in section 5.

## 2. Functional requirements

The app turns any media file that enters the user's workflow into a smaller, compatible file without the user leaving what they are doing. Requirements below are grouped as inputs (triggers), formats, the optimisation rules, actions on a result, output handling, pipelines, integrations and app control. Every feature is available to every user, with no limits.

### 2.1 Key user workflows

1. **Copy large, paste small:** user copies a screenshot or image, the app optimises it within about a second, replaces the clipboard content, and shows a small result card with the size saved; the user pastes as usual.
2. **Share a screen recording:** user stops an OS screen recording, the app detects the new file once writing settles, re-encodes it, and shows a floating thumbnail the user drags into a chat or email.
3. **Drop to optimise:** user starts dragging files; a drop zone appears (or is summoned under the cursor with a modifier tap); files dropped there are optimised and appear as result cards.
4. **One-drop preset:** user taps a second modifier while dragging; preset zones appear, each bound to a pipeline (for example "convert to WebP and copy link"); the drop runs that pipeline.
5. **Batch a folder:** user drops a folder or more files than the batch threshold; a batch window lists every file, backs up originals, runs them in parallel and allows re-running the set with other settings or rolling all back.
6. **Automate a folder:** user assigns a pipeline to a watched folder; every matching file that arrives is processed silently or with a result card.
7. **Adjust after the fact:** on any result card the user downscales, crops, converts, changes compression, speeds up, compares with the original, or restores it, always re-encoding from the original.

### 2.2 Input triggers

| ID | Trigger | Required behaviour |
| --- | --- | --- |
| FR-T1 | Image copied to clipboard | Detect image data (PNG, JPEG, GIF, TIFF, WebP, HEIC), optimise, write the smaller image back to the clipboard, show a result |
| FR-T2 | On-demand hotkey | Optimise whatever is on the clipboard: image data, file paths, URLs, base64 images; a second hotkey runs aggressive mode |
| FR-T3 | Video or PDF file copied | Optional setting to optimise copied video and PDF files, not only images |
| FR-T4 | Screenshot or screen recording saved | Watch the OS capture locations, wait for the write to finish, optimise, keep the file marked as a screenshot |
| FR-T5 | Watched folders | Per media type (image, video, audio, PDF): list of folders, ignore rules per type, skip when more than N files arrive at once, per-folder result visibility, skip missing or unmounted folders without deleting them |
| FR-T6 | Drop zone | Appears near a screen edge while files are dragged, or under the cursor on a modifier tap; holding a modifier while dropping keeps the original untouched |
| FR-T7 | Preset zones | Shown on a second modifier tap while dragging; each zone runs an assigned pipeline, with one pipeline per file type |
| FR-T8 | Batch window | Opens automatically above a file-count threshold or when a folder is dropped (recursing into subfolders); can also be opened from the menu |
| FR-T9 | OS file integration | "Open with" target for images, videos and PDFs; context-menu action "Optimise with Ferry" in Finder / Explorer |
| FR-T10 | URL input | Download an image or video from a copied link, then optimise it |
| FR-T13 | Photo library copy | macOS only: images copied from Photos are optimised and downscaled to a set size, then copied back |

### 2.3 Supported formats

| Type | Accepted inputs | Default output | Notes |
| --- | --- | --- | --- |
| Image | PNG, JPEG, GIF (animated), WebP (animated), HEIC incl. HDR, AVIF, TIFF, JPEG XL, base64 | Same format optimised; HEIC and TIFF converted to JPEG; HDR HEIC to HDR JPEG | Optional adaptive mode switches PNG and JPEG when the other is smaller (off by default) |
| Video | MP4, MOV, MKV, WebM, AV1, HEVC, drone footage | MP4 (H.264 or HEVC); MOV converted to MP4 replacing the original | Keep every source frame and all audio tracks; AV1 output uses MKV |
| Audio | WAV, FLAC, AIFF, high-bitrate MP3, M4A, Ogg | WAV to MP3, AIFF and FLAC to M4A (AAC), Opus optional | Never raise the bitrate above the original; lossless targets show no bitrate |
| PDF | Any unencrypted PDF | Recompressed PDF | DPI stops 300, 250, 200, 150, 100, 72, 48 or adaptive; PDFs over 150 pages split into 100-page chunks processed in parallel; encrypted PDFs rejected with a message |

### 2.4 Optimisation rules

- **Levels:** normal, aggressive and extreme, plus a compression slider from 5 to 100 or "adaptive" for every format.
- **Video encoder choice:** fast hardware encoder (low CPU, larger file), slow software encoder (smaller file), or visually lossless.
- **Never worse:** if the result is not smaller, keep the original and label the card "already optimised".
- **Always from the original:** every later action re-encodes from the backed-up original so quality is never lost twice; changing speed 2x then 1.5x yields 1.5x.
- **Settle before work:** wait until a file stops changing before reading it; ignore cloud-offload events so synced folders do not loop.
- **Sniff real format:** detect format by content, not extension (a JPEG named `.png` is treated as JPEG).
- **Animation safety net:** an optimisation that would flatten an animated GIF or WebP fails and keeps the original.
- **Metadata:** optional EXIF stripping that still keeps colour profile, DPI and the screenshot marker; optionally preserve creation and modification dates.
- **Optional automatic transforms:** downscale Retina images to 1x, cap video FPS, remove audio from video, convert video audio to AAC, playback-speed frame policy (keep frames for smoothness or drop frames for size).
- **Skip rules:** ignored extensions per type, maximum file size, more than one file copied, clipboard from ignored apps, transient or concealed clipboard types (password managers), vector data from design tools, Universal Clipboard (macOS).

### 2.5 Actions on a result

| Action | Applies to | Detail |
| --- | --- | --- |
| Downscale | Image, video, cover art | Step down 90% to 10% with `-`; jump to presets with `1`–`9`; press-and-drag slider shows the target resolution |
| Crop | Image, video, PDF | Exact size, aspect ratio (16:9, 4:3, 1:1, 1.91:1), long edge, smart or centre framing, multiplier buttons, saved size list with reset; PDF crop to device screens and uncrop |
| Compression | All | Slider re-encodes in the current format; extreme stop at the top |
| Fit under size | All | Compress until the file is under a chosen limit |
| Convert | All | Format bar on the card; one click converts and replaces the last conversion, a modifier keeps both; codec badges such as "mp4 · HEVC" |
| Playback speed | Video, audio | Factor with keep/drop frames |
| Audio tools | Audio | Loudness normalisation to Streaming, Apple Music, Podcast or Broadcast targets; cover art optimise, remove, keep, extract or downscale; A/B compare |
| PDF tools | PDF | DPI slider, extract pages as JPEG or PNG with progress, single page to image |
| Video to GIF | Video | High-quality GIF export |
| Strip metadata, remove audio, re-optimise with encoder | As relevant | From the context menu |
| Restore original | All | Puts the backup back and clears the size comparison |
| Compare | Image, video, PDF, audio | Side-by-side or A/B playback |
| Quick preview | All | Space opens the OS previewer |
| Rename | All | Click the filename chip |
| Hand off | All | Copy, copy path, drag out (single or all), show in folder, open with, edit with a per-type external app, share menu, run an assigned pipeline button |

### 2.6 Output and file handling

- **Location per type and per kind (optimised vs converted):** in place with the original moved to backup, same folder with a name template, a specific folder with a template, or a temporary folder.
- **Templates:** tokens for filename, extension, size (WxH), date parts and parent folder, for example `~/Pictures/twitter/%f_%z.%e`; paths stored with `~` so synced settings work for other usernames.
- **Backups:** clone originals before work; configurable working directory with reset, "open" and "force clean" actions; cleanup by file age.
- **Clipboard output:** name template for clipboard images, optional accumulation of all results so several can be pasted at once, optional copy of file paths instead of image data, optional auto-copy after drop or watched-folder optimisation.

### 2.7 Pipelines and automation

A pipeline is an ordered list of steps that is the single model behind every action, preset and automation, so a file is encoded only once however many steps change it.

- **Steps:** `optimise(compression, dpi, encoder, location)`, `convert(to, location)`, `downscale`, `crop(width, height, aspectRatio, smartCrop, longEdge)`, `changeSpeed(factor, frames)`, `removeAudio`, `stripExif`, `copy(to: template)`, `copyLinkForSending`, `extractPagesAsImages`, `runScript(code)`, run OS automation (Shortcuts on macOS, PowerShell on Windows), `fork`, `if(nameContains: …)` and a `hideResult` flag.
- **Triggers:** copied to clipboard, arriving in a folder, dropped on the drop zone, a preset zone or a result-card button.
- **Library:** create, edit with live preview, duplicate, delete, assign to any trigger, see where each is assigned, jump from a zone or result to its pipeline.
- **Result behaviour:** one card per pipeline run updates in place step by step; `fork` adds a second draggable file while the pipeline continues.

### 2.8 Sharing and integrations

- **Send securely:** peer-to-peer encrypted transfer (WebRTC) from the user's machine; a link is copied, the receiver downloads in a browser, no server storage, unlimited size, several receivers, expiry from 1 minute to 3 days or never; active sends listed in the tray menu with copy and stop.
- **Upload and shelve:** Dropshare upload, shelf apps (Yoink, Dockside, Dropover on macOS), system share sheet, AirDrop on macOS and Nearby Share on Windows.
- **External editors:** per-type "edit with" app.

### 2.9 App control and updates

- **States:** Running, Paused (skips the next event then resumes), Stopped (no automatic work until resumed), cycled by a global hotkey and shown in the tray icon.
- **Updates:** background check, subtle "update available" button under the results, release notes.
- **Settings:** fuzzy search, import/export and optional sync, launch at login, reopen Settings when launched twice, toggle whether app UI appears in screenshots, logs viewer, onboarding on first run.

### 2.10 Hotkeys

| Action | macOS | Proposed on Windows |
| --- | --- | --- |
| Optimise clipboard | Ctrl+Shift+C | Ctrl+Alt+Shift+C |
| Optimise clipboard aggressively | Ctrl+Shift+A | Ctrl+Alt+Shift+A |
| Cycle running / paused / stopped | Ctrl+Shift+P | Ctrl+Alt+Shift+P |
| Drop zone under cursor | Tap Option | Tap Alt while dragging |
| Preset zones | Tap Control | Tap Ctrl while dragging |
| Quick preview (hovered card) | Space | Space |
| Edit with external app | Cmd+E | Ctrl+E |
| Compare | Cmd+D | Ctrl+D |
| Upload | Cmd+U | Ctrl+U |
| Lower DPI or bitrate | Cmd+minus | Ctrl+minus |
| Downscale step / preset | `-` / `1`–`9` | `-` / `1`–`9` |
| Select all / clear in list | Cmd+A / Esc | Ctrl+A / Esc |

Windows defaults add Alt to the global hotkeys because Ctrl+Shift+C is taken by browser dev tools; all hotkeys are user-configurable.

## 3. Non-functional requirements

The app must feel invisible: instant on small files, never freezing, never losing an original, and never sending a file off the machine unless the user asks. The numeric targets below are proposals to validate; the qualitative rules guard against known failure modes of tools like this (freezes on slow drives, flattened GIFs, dropped frames, optimisation loops in synced folders).

### 3.1 Performance

| ID | Requirement | Target |
| --- | --- | --- |
| NFR-P1 | Clipboard change detected | Within 300 ms of the copy |
| NFR-P2 | Clipboard image up to 5 MB optimised and written back | Within 1.5 s on a 2020-era laptop |
| NFR-P3 | Result card visible after any trigger | Within 200 ms, with a placeholder thumbnail until the real one is ready |
| NFR-P4 | UI responsiveness | No UI thread blocking; all I/O and encoding off the main thread; file-system calls on slow, network or sleeping drives use timeouts (for example 5 s for extended attributes) |
| NFR-P5 | Video encoding | Hardware encoder by default (VideoToolbox on macOS; NVENC, Quick Sync or AMF on Windows) with automatic software fallback |
| NFR-P6 | Parallelism | Concurrency capped at available cores; large PDFs split into chunks; the hardware video engine's real encoder count respected |
| NFR-P7 | Batch scale | Batch window stays smooth with 5,000 files (virtualised list, incremental progress) |

### 3.2 Resource usage

| ID | Requirement | Target |
| --- | --- | --- |
| NFR-R1 | Idle CPU while watching clipboard and folders | Under 1% average |
| NFR-R2 | Idle memory | Under 150 MB including the webview |
| NFR-R3 | Installer size per platform | Under 120 MB including encoders; encoders shipped without debug symbols |
| NFR-R4 | Disk | Working files deleted after each job; backups pruned by age (default 7 days, configurable); no temp folder left per job |
| NFR-R5 | Battery | Prefer hardware encoders on battery; pause heavy batch work under low-power mode (optional setting) |

### 3.3 Reliability and data safety

- **Original is sacred:** back up before any change, write outputs to a temp file then atomically rename, and keep "Restore original" available until the backup expires.
- **Fail visibly, fail safe:** any failure leaves the original untouched and shows the reason on the result card (for example which folder is read-only), never a card stuck on "Optimising".
- **No self-loops:** files the app writes are never re-triggered as new input, including clipboard writes and outputs landing in watched folders; cloud offload and re-download events are ignored.
- **Process hygiene:** child encoder processes are killed when the app quits or crashes; their output is drained continuously so a chatty tool cannot block the app; the app restarts itself after a crash or hang.
- **Watcher resilience:** a missing, unmounted or network folder is skipped and retried, never removed from settings, and never stops other watchers.
- **Fidelity guarantees:** never output a single frame from an animated source, never drop video frames unless the user chose "drop frames", keep all audio tracks, keep colour profile and DPI.

### 3.4 Security and privacy

- **On-device processing:** no file content leaves the machine for optimisation. Network use is limited to update checks, URL downloads the user requested, and signalling for secure send; this list is published in the app and privacy policy.
- **No telemetry by default;** crash reports opt-in.
- **Clipboard privacy:** skip content marked concealed or transient on macOS and content flagged as excluded from clipboard history or monitoring on Windows (password managers); per-app ignore list.
- **Secure send:** end-to-end encrypted, unguessable links, user-set expiry, stoppable at any time, no copies on any server.
- **Least privilege:** Tauri capabilities scoped per window; strict CSP; no remote web content loaded in app windows; sidecar binaries code-signed (macOS notarised, Windows Authenticode).
- **User scripts:** `runScript` steps run only from pipelines the user created or approved.

### 3.5 Compatibility

| Area | Requirement |
| --- | --- |
| macOS | 13 Ventura and later, Apple Silicon and Intel, signed and notarised DMG |
| Windows | Windows 10 22H2 and Windows 11, x64 and ARM64, WebView2 runtime installed by the setup if missing, signed MSI or NSIS installer |
| File systems | APFS, NTFS, exFAT, SMB shares, cloud-synced folders (iCloud Drive, OneDrive, Dropbox, Google Drive) |
| Displays | Multiple monitors with mixed scale factors; cards follow the cursor's screen |
| Appearance | Light and dark mode; follows system accent colour |
| Interop | Drag-out and clipboard output accepted by browsers, chat apps, mail clients, Office and design tools on both OSes |

### 3.6 Accessibility

- Every control, including hover-only card buttons, is reachable by keyboard and has an accessible name for VoiceOver, Narrator and NVDA.
- Hover reveals must have a focus equivalent: focusing a card shows its actions; a global hotkey focuses the newest card.
- Respect reduce motion, reduce transparency and increased contrast or high-contrast themes.
- Text and icons meet WCAG 2.2 AA contrast on both vibrancy and solid backgrounds; status never relies on colour alone.
- Auto-hide timers pause while a card is focused or hovered, and the delay is configurable.

### 3.7 Localisation and maintainability

- All strings externalised from day one; English first, Vietnamese as the first additional language; locale-aware number, size and date formats.
- Structured logs including encoder command lines and stderr, viewable and exportable from the app.
- Encoder versions pinned per release and updated together.

## 4. UI specification

The app has no main window: it lives in the tray, and its primary surface is a stack of floating result cards near a screen corner. Every other surface (drop zone, preset zones, batch, compare, settings, pipeline editor, onboarding) is opened from a card, a drag gesture, a hotkey or the tray menu. Sizes are in logical pixels (points on macOS, DIPs on Windows) and are starting values for design.

### 4.0 Design foundations

- **Personality:** a quiet utility. Dense, glanceable, native-feeling; it appears for seconds and gets out of the way.
- **Materials:** translucent vibrancy on macOS; Mica for windows and Acrylic for floating surfaces on Windows 11; solid tinted fallback on Windows 10 and when "reduce transparency" is on.
- **Type:** system UI font (SF Pro on macOS, Segoe UI Variable on Windows). Scale: 11 caption, 12 body small, 13 body, 15 title, 20 window title. Tabular figures for sizes and percentages.
- **Colour roles:** accent (system accent) for progress and selection; success green for savings; amber for "already optimised" or "result larger, original kept"; red for errors; neutral for metadata. Light and dark variants for each.
- **Shape:** card radius 12 on macOS, 8 on Windows 11; chips fully rounded; 8-pixel spacing grid.
- **Icons:** one cross-platform outline set (for example Lucide) at 16 and 20 pixels so both OSes match.
- **Motion:** cards slide in from the screen edge in 180 ms and fade out in 150 ms; sliders and chips animate under 120 ms; all motion off when "reduce motion" is on.

### 4.1 Tray icon

**Purpose:** always-visible status and entry point.

- **Elements:** monochrome template icon in three selectable styles (default, classic, flat geometric).
- **States:** Running (normal), Paused (icon with pause glyph), Stopped (dimmed with slash), Working (subtle animated fill while any job runs), Update available (small dot), Error (small red dot until the user opens the menu).
- **Interactions:** click opens the tray menu; on Windows, double-click opens Settings; dragging files onto the macOS menubar icon optimises them.

### 4.2 Tray menu

**Purpose:** quick commands and app state.

- **Elements, in order:** status header ("Running" with a segmented Running / Paused / Stopped control); Optimise clipboard; Optimise clipboard aggressively; Open batch window; Show drop zone; separator; Active secure sends submenu (each send shows file name, time left, Copy link, Stop); Recent results submenu (last 10, click to reopen the card); separator; Open backups folder; Open working directory; Force clean working directory; Show app UI in screenshots (toggle); separator; Settings…; Check for updates…; About; Quit.
- **Rules:** each command shows its hotkey on the right, hidden when that hotkey is disabled.

### 4.3 Floating result card

**Purpose:** the core component. It shows what was optimised, how much was saved, and offers every follow-up action right where the file is.

**Anatomy (default 300 × 200, compact size option 220 × 150):**

- **Thumbnail area:** fills the card; image as-is, video first frame with a duration badge, PDF first page with a page count badge, audio cover art or a waveform placeholder. A soft dark gradient at the bottom keeps text legible.
- **Close button:** top-right, visible on hover.
- **Info strip (bottom):** filename chip (click to rename inline); extension chip with codec badge such as "mp4 · HEVC" (click to open the format bar); size line "2.4 MB → 380 KB" with a savings pill "−84%"; secondary line with what changed: resolution "2880×1800 → 1440×900", video bitrate and FPS, audio bitrate and cover resolution, or PDF DPI.
- **Side action rail:** up to 6 icon buttons in a vertical rail on the screen-facing edge, visible on hover, plus an empty "+" slot to assign an action or a pipeline. Defaults: downscale, compression, crop, restore original, preview, copy. Downscale and compression are press-and-drag sliders; a plain click applies one step.
- **Format bar:** a row of target-format chips across the bottom on hover (for images: PNG, JPEG, WebP, AVIF, HEIC, JXL; for video: MP4, HEVC, AV1, WebM, GIF; for audio: MP3, M4A, Opus, WAV; for PDF: images). The current format is selected; formats identical to the current codec are hidden.
- **Pipeline buttons:** assigned pipelines appear as icons in the rail; one button may hold a different pipeline per file type.

**States:**

| State | What the card shows | Available actions |
| --- | --- | --- |
| Queued | Placeholder icon for the file type, filename, "Waiting…" | Cancel |
| Optimising | Thumbnail dimmed, progress ring or bar with percent and step text ("Step 2 of 4 · Convert to WebP"), original size | Cancel, drag original |
| Done | Full thumbnail, before → after size, savings pill in green | All actions |
| Already optimised | Amber pill "Already optimal", single size | All actions except restore |
| Larger, kept original | Amber pill "Original kept · result was larger" | Try aggressive, convert, close |
| Converted | Extension chip shows new format; small "converted" label | All actions; modifier-click a format keeps both |
| Restored | Grey pill "Original restored"; comparison lines cleared | Re-optimise, close |
| Error | Red border and icon, one-line reason (for example "Can't write to \~/Shared"), "Details" link to the log entry | Retry, show in folder, copy error, close |
| Pipeline running | Same as Optimising but step text follows each pipeline step; the card updates in place | Cancel pipeline |
| Forked result | Second card attached under the parent with a link glyph | All actions |
| Long-running job | Extra line with elapsed time and ETA | Cancel, keep running in background |

**Interactions:** drag the card (or any rail button) out to drop the optimised file into any app; swipe toward the screen edge to dismiss; hover pauses the auto-hide timer (default 6 s, configurable, or never); Space previews; right-click opens the context menu (4.6); hover hotkeys from section 2.10 act on the hovered card; keyboard focus shows the same controls as hover.

### 4.4 Results stack

**Purpose:** container that places cards on screen.

- **Elements:** cards stacked vertically from a chosen corner (default bottom-right, 16 px margins, 8 px gaps); a "drag all" handle above the stack showing a fanned pile of thumbnails and a count; a "Clear all" text button; a subtle "Update available" pill below the stack when relevant.
- **States:** empty (nothing rendered, click-through); 1–5 cards; more than 5 cards switches to the compact list (4.5) automatically.
- **Behaviour:** moves to the screen where the cursor is (setting); never takes keyboard focus from the user's app; click-through outside cards; excluded from screenshots and screen recordings unless the user enables "Show app UI in screenshots".

### 4.5 Compact results list

**Purpose:** a denser view for many results, and multi-file actions.

- **Row (height 44):** small thumbnail, filename, type icon, original → new size, percent saved, status icon; hover shows a checkbox and an inline action rail.
- **Selection mode:** clicking a hover checkbox enters selection; any row click adds to it; a selection bar appears with a drag handle for the whole set, Save to…, Crop, Downscale, Bitrate, Convert, Copy, Remove; Select all and Esc to clear.
- **States:** empty (hidden); processing rows with inline progress; mixed results; header shows totals "23 files · 41.2 MB saved (−68%)".
- **Rule:** the list never reorders under the cursor while the user is selecting.

### 4.6 Result context menu

Items, grouped: Open, Open with ▸, Edit with \<app> (⌘E / Ctrl+E), Show in Finder / Explorer; Copy, Copy path, Rename…; Compare with original (⌘D / Ctrl+D), Preview; Re-optimise, Re-optimise with encoder ▸ (Fast, Small, Visually lossless), Optimise aggressively, Fit under size…; Downscale ▸, Crop…, Convert to ▸, Change speed…, Remove audio, Strip metadata; for PDFs: Change DPI ▸, Extract pages as images…, Convert to image; for audio: Normalise loudness ▸, Cover art ▸ (Optimise, Remove, Extract, Downscale), Compare A/B; Share ▸ (system share, AirDrop or Nearby Share), Send securely…, Upload with Dropshare (⌘U), Add to shelf ▸; Run pipeline ▸, Show producing pipeline; Restore original; Close.

### 4.7 Inline controls and popovers

| Control | Opened from | Elements | States |
| --- | --- | --- | --- |
| Downscale slider | Press-and-drag the downscale button | Vertical slider 90%–10% with live target resolution label | Dragging, applying (card shows progress), done |
| Compression slider | Press-and-drag the compression button | Slider 5–100 with an "Extreme" stop at the top and "Adaptive" detent; label shows estimated size or bitrate | Dragging, applying, done |
| PDF DPI slider | Minus button on a PDF card | Stops at 300, 250, 200, 150, 100, 72, 48 and Adaptive | Same |
| Crop popover | Crop button or clicking the resolution | Saved sizes list with "Restore defaults", aspect presets (16:9, 4:3, 1:1, 1.91:1, 9:16), width × height fields (0 keeps ratio), long-edge toggle, Smart / Centre framing toggle, ×0.5 / ×2 multiplier buttons, device list for PDFs | Default, invalid size (field turns red with hint), applying |
| Fit under size | Action button or menu | Size field with unit picker (KB, MB) and quick chips (1 MB, 5 MB, 10 MB, 25 MB) | Default, impossible target (message suggests lower resolution), applying |
| Speed popover | Menu | Factor chips 1.25×, 1.5×, 2×, 3×, custom; Keep / Drop frames toggle | Default, applying |
| Secure send | Send securely | Expiry slider 1 min to 3 days plus "Never", Copy link button, receiver count | Preparing link, live (pulsing dot, time left), receiver connected (progress per receiver), expired, stopped |
| Rename field | Clicking the filename chip | Inline text field, Enter to confirm, Esc to cancel | Editing, name taken (inline error) |

### 4.8 Drop zone

**Purpose:** a target for drag-and-drop optimisation that appears only when files are being dragged.

- **Elements:** a rounded translucent panel (220 × 140) near the results corner, or centred under the cursor when summoned with a modifier tap; app glyph, label "Drop to optimise", sub-label showing the count and types being dragged ("3 images, 1 video"); a mode line "Originals will be kept" when the keep-originals modifier is held; a hint row at the bottom ("Tap Ctrl for presets").
- **States:** hidden; visible idle (dragging anywhere on screen); hover target (border in accent, panel scales to 1.04); unsupported files (dimmed, label "Can't optimise .zip"); batch hint ("42 files · opens Batch") when the count exceeds the threshold; dropped (panel collapses into a new result card).
- **Interactions:** appears on drag start if enabled, or only on modifier tap; tapping the modifier again dismisses; drops of folders recurse into subfolders; ignores drags from editors that are known false positives (video editors, design tools).

### 4.9 Preset zones

**Purpose:** one-drop actions that run a saved pipeline.

- **Elements:** a grid overlay of zone tiles near the drop zone, filtered to the type being dragged; each tile has an icon, a short name ("WebP + copy link"), the pipeline's first steps in caption text, and a type badge; an "Edit presets…" tile at the end.
- **States:** hidden; shown (on modifier tap); tile hover (accent border, other tiles dim); tile with no pipeline for this type (disabled, caption "No pipeline for video").
- **Interactions:** drop on a tile runs its pipeline and creates one result card per file; right-click a tile for Edit, Go to pipeline, Remove.

### 4.10 Batch window

**Purpose:** process hundreds or thousands of files with backups, without a card per file.

- **Layout (default 960 × 640, resizable):** toolbar with Add files, Add folder, Start / Pause, Re-run with new settings, Roll back all, Reveal output folder; a collapsible right-side Adjust panel; a file table; a footer summary.
- **File table columns:** checkbox, thumbnail, name, type, original size, new size, saved %, resolution change, status (Waiting, Working with inline progress, Done, Already optimal, Skipped with reason, Failed with reason). Sortable, filterable by type and status, virtualised.
- **Adjust panel:** compression slider, downscale size, crop or aspect ratio, target format per type, video encoder, audio bitrate, cover art size (Square, Long edge), output location and template, Keep metadata toggle.
- **States:** empty (large drop target "Drop files or a folder" with the threshold explained); backing up (progress "Backing up 312 of 1,204 originals"); running (overall progress bar, files per second, ETA); paused; done (summary "1,204 files · 2.1 GB → 640 MB · −70%", Reveal and Close buttons); done with failures (red count chip that filters the table); rolling back (progress, then all rows show "Restored").
- **Interactions:** drag rows out; double-click previews; right-click a row for the same context menu as a card.

### 4.11 Compare and preview

**Purpose:** let users judge quality before sharing.

- **Image and PDF compare:** window sized to the content (max 80% of the screen), original and optimised in one frame with a draggable vertical divider, or side by side via a toggle; zoom 100%, fit, and 2× with synced panning; header shows both sizes and resolutions; for PDFs a page stepper.
- **Video compare:** two synced players side by side, shared transport controls, frame step, toggle to swap.
- **Audio A/B:** one transport with an A / B switch that keeps the playhead, waveform for each, loudness and bitrate labels.
- **Preview (Windows):** a lightweight preview window for Space, since Windows has no Quick Look; on macOS Space uses Quick Look.
- **States:** loading (skeleton frame), ready, original missing (backup expired: compare disabled with a tooltip), error decoding.

### 4.12 Settings window

**Purpose:** all configuration in one place, searchable.

- **Layout (820 × 600, resizable):** sidebar with a search field at the top and the pane list; content area scrolls; a live preview strip at the top of panes that affect visuals (floating results, drop zone).
- **Search:** fuzzy search across every setting label and description; results replace the pane list with matching settings grouped by pane; selecting one jumps and highlights it for 2 s; empty state "No settings match".
- **Panes and their contents:**
  - **General:** launch at login, tray icon style picker (3 thumbnails), dock icon on macOS, start state, show app UI in screenshots, language, import / export settings, optional sync.
  - **Clipboard:** enable, media types to watch (images, videos, PDFs), skip when more than one file is copied, maximum size, ignored apps list (add from running apps, remove), accumulate results, copy paths instead of images, clipboard filename template, Photos copy downscale size (macOS).
  - **Images:** default compression, adaptive PNG/JPEG switching, conversions (HEIC, TIFF, AVIF to …), downscale Retina to 1×, strip metadata (keeps colour profile), preserve dates, ignored extensions, maximum size, watched folders for images.
  - **Videos:** encoder picker (Fast · Small · Visually lossless) shown as three cards with a one-line trade-off each, compression, convert MOV to MP4, convert audio to AAC, remove audio, FPS cap, playback-speed frame policy, ignored extensions, maximum size, watched folders.
  - **Audio:** output format per input format (WAV → MP3, AIFF → M4A, FLAC → M4A, editable), bitrate, cover art policy, loudness target, watched folders.
  - **PDF:** default DPI for normal and aggressive, adaptive DPI, watched folders, PDF crop device list.
  - **Drop zone:** enable, show on any drag or only on modifier tap, position, modifier keys, keep-originals modifier, batch threshold, preset zones editor with a live preview of the zone grid.
  - **Floating results:** enable, corner, card size, auto-hide delay, follow cursor across screens, cards before compact list, format bar or hover picker, side-rail editor (drag to reorder, add, remove, assign pipelines), update-available pill.
  - **File handling:** a grid of media type × (optimised, converted) where each cell has a location picker (In place, Same folder, Specific folder, Temporary) and a template field with token chips (%f, %e, %z, %y, %m, %d, %P) and a live example path; backups folder and retention; working directory with Open, Reset and Force clean.
  - **Watched folders:** one table across types: path, types, assigned pipeline, show result toggle, ignore rules, max files at once, status (Watching, Missing, Paused). Add with a folder picker or by dropping a folder on the table.
  - **Pipelines:** library list (name, triggers it is assigned to, last run) with New, Duplicate, Delete; opens the editor (4.13).
  - **Sharing:** secure-send default expiry, Dropshare, shelf apps, edit-with app per type.
  - **Hotkeys:** table of actions with a recorder field, enable toggle and conflict warning.
  - **Advanced:** log viewer (filter by level, copy, export), reset all settings, debug options.
  - **About and updates:** version, auto-update toggle, Check now, release notes.
- **States per control:** default, changed (applies instantly, no Save button), invalid (red helper text, for example a template with an unknown token or a missing folder), dependent-disabled (greyed with a hint like "Enable clipboard optimisation first").

### 4.13 Pipeline editor

**Purpose:** build, test and assign pipelines without reading docs.

- **Layout:** left list of pipelines; centre editor with two synced modes, a visual step list (cards you add, reorder by drag, and configure in an inspector) and a text mode using the pipeline language with autocomplete and inline errors; right panel for triggers and assignments (clipboard by type, drop zone, preset zone, watched folders, card buttons) and the hide-result flag.
- **Test run:** drop a sample file on the test well to run the pipeline in a sandboxed temp folder; each step shows output size, time and errors.
- **States:** empty library (illustration and "New pipeline", plus templates such as "Convert to WebP", "Compress screen recording and copy link", "Invoices to folder"); editing; syntax error (line marked, run disabled); test running; test passed; test failed at step N; unassigned warning.

### 4.14 Onboarding

**Purpose:** get the user to their first saved megabyte in under a minute.

- **Steps (window 640 × 480):** 1 Welcome with the tagline and a short animation of copy → smaller paste; 2 Choose what to optimise automatically (clipboard images, screen recordings, a folder) as toggle cards; 3 Permissions where needed (macOS: notifications and any automation prompts; Windows: none expected) with a "Why?" line each; 4 Try it: "Copy any image now" with a live card appearing inline when they do; 5 Where to find the app (tray location illustrated per OS).
- **States:** each step has Back, Continue, and Skip; the try-it step shows waiting, success (saved size), and "Skip for now".

### 4.15 Update prompt

- **Elements:** version, release notes in a scrollable panel, Install and restart, Later, Skip this version.
- **States:** checking, up to date, available, downloading with progress, ready to install, failed with retry.

### 4.16 Secure-send receiver page (web)

**Purpose:** what the recipient sees in their browser; it is part of the product and must be designed.

- **Elements:** sender's file name, type icon and size, a large Download button, expiry countdown, note that the transfer is direct and encrypted.
- **States:** connecting to sender, ready, downloading with speed and percent, done, sender offline ("The sender's computer is not reachable; ask them to keep the app open"), link expired, link stopped by sender, browser unsupported.

## 5. macOS vs Windows mapping for Tauri 2

About 80% of the feature set maps cleanly to Tauri 2 plus small Rust modules per OS; the hard parts on Windows are detecting a drag anywhere on screen (to show the drop zone), modifier taps during a drag, and the Windows 11 Explorer context menu. Everything below sits in a Rust core with one `platform/macos.rs` and one `platform/windows.rs`, and a shared web UI.

| Capability | macOS | Windows | Tauri 2 building block | Risk |
| --- | --- | --- | --- | --- |
| Tray icon and menu | Menubar template icon | Notification-area icon; often hidden in overflow, so onboarding shows how to pin it | Core tray and menu APIs | Low |
| Clipboard watch | Poll `NSPasteboard.changeCount` every 250 ms; skip concealed and transient types | `AddClipboardFormatListener` events; skip formats that exclude content from monitoring or history | Clipboard plugin for read/write; custom Rust for watching, file lists and type filters | Medium |
| Screenshot and recording detection | Read the screenshot location setting (default Desktop) | Pictures\\Screenshots, Videos\\Screen Recordings (Snipping Tool), Videos\\Captures (Game Bar) | `notify` crate watchers | Low |
| Folder watching and settle detection | FSEvents | ReadDirectoryChangesW; also wait until the file can be opened without a sharing violation | `notify` with debounce plus size-stable check | Low |
| Floating cards that never steal focus | Non-activating panel, floating level, visible on all Spaces and over full-screen apps | Tool window, no-activate and topmost styles; re-show on the current virtual desktop after a switch | Window builder: transparent, no decorations, always on top, skip taskbar, not focusable; native style tweaks in Rust | Medium |
| Blur material | NSVisualEffectView | Mica / Acrylic on Windows 11, solid on Windows 10 | `window-vibrancy` crate | Low |
| Click-through around cards | Ignore mouse events outside card bounds | Same | Toggle cursor-event passthrough from hit-testing | Low |
| Hide UI from screenshots | Window sharing type none | Display affinity exclude-from-capture (Windows 10 2004+) | Window content-protection option | Low |
| Drag files out of a card | Dragging session with file URLs | OLE drag with a file-drop list | Community drag plugin, or a small native module | Medium |
| Show drop zone when any drag starts | Poll the drag pasteboard plus a global mouse monitor | No public "drag started" event; needs a low-level mouse hook plus heuristics | Custom Rust | High: fall back to modifier tap or a slim screen-edge tab |
| Modifier tap while dragging | Global flags-changed monitor (may need Input Monitoring permission) | Low-level keyboard hook; Ctrl and Alt already change the drop effect in Explorer, so defaults may need to change | Custom Rust | High: user-test the defaults |
| Global hotkeys | Carbon/AppKit hotkeys | RegisterHotKey | Global-shortcut plugin | Low |
| Hardware video encode | VideoToolbox in ffmpeg | NVENC, Quick Sync or AMF in ffmpeg, probed with a 1-second test encode at startup | ffmpeg sidecar | Medium: GPU and driver variety |
| Quick preview | Quick Look panel | Built-in preview window (4.11) | Extra webview window | Low |
| OS automation step | Shortcuts via the `shortcuts` command | PowerShell scripts (Power Automate optional) | Shell plugin with an allow-list | Low |
| Open With and context menu | Document types in Info.plist; Services menu item | File associations in the installer; Windows 11 top-level menu needs a packaged app with an Explorer command handler, otherwise it sits under "Show more options" | Bundler file associations; custom installer step | Medium |
| Share and nearby send | Share picker, AirDrop | Share UI via WinRT (Nearby Share) | Small native module | Medium |
| Launch at login, single instance, updates | Login item | Run key or startup task | Autostart, single-instance and updater plugins (signed updates) | Low |
| Secure send | WebRTC data channel | Same | WebRTC in the webview or `webrtc-rs`; you must host a signalling server and the receiver page | Medium: ongoing server cost |
| Encoders | Universal (arm64 + x86\_64) builds, each binary signed and notarised | x64 and ARM64 builds, each binary Authenticode-signed; Ghostscript is `gswin64c` | `externalBin` sidecars with target-triple names | Medium: build pipeline |

**Design adaptations per OS:**

- Keep the card stack in the bottom-right on both, but on Windows place it inside the monitor work area so it never overlaps the taskbar; the tray menu opens upward on Windows and downward on macOS.
- Window chrome follows the OS: macOS traffic lights on the left with a unified toolbar; Windows caption buttons on the right with a Mica title bar. Use native context menus from Tauri's menu API rather than HTML menus.
- Swap Cmd for Ctrl in hover hotkeys and add Alt to global hotkeys on Windows (table 2.10); label hotkeys with the OS's own symbols (⌘⌥⌃⇧ on macOS, words on Windows).
- Card radius 12 on macOS, 8 on Windows; vibrancy vs Mica/Acrylic; system fonts per OS.
- Signing is mandatory on both: an Apple Developer ID (USD 99 per year) with notarisation, and a Windows code-signing certificate or Azure Trusted Signing to avoid SmartScreen warnings.

## 6. Open questions and MVP phasing

With every feature free, the simplest path is to release the app as GPL-3 open source, which allows bundling every encoder listed in section 1.

**Open questions:**

- [ ] Product name and icon: check trademark and domain availability for the chosen name before committing (see section 1).
- [ ] Secure send in v1? It needs a hosted signalling server and receiver page with ongoing cost.
- [ ] Windows drop-zone trigger: accept a modifier tap or a slim screen-edge tab if global drag detection proves unreliable?
- [ ] Windows default modifiers for drop zone and preset zones, since Ctrl and Alt already change drop effects in Explorer: validate with 5 users.
- [ ] Launch languages: English plus Vietnamese?

**Suggested phasing (each phase ships on both OSes before the next starts):**

1. **Core loop:** tray icon and menu, clipboard image optimisation (PNG, JPEG, GIF, WebP), floating result card with Done, Already optimal and Error states, drag out, restore original, backups, settings panes General, Clipboard, Images and Floating results. Gate: clipboard image under 1.5 s and cards never steal focus on either OS.
2. **Video and recordings:** ffmpeg sidecar with hardware encoders, screenshot and recording watcher, watched folders, downscale, crop, convert and compression sliders on cards, compare window.
3. **Drop and batch:** drop zone, compact list, batch window, PDF and audio optimisation, file-handling templates.
4. **Automation:** pipeline engine and editor, preset zones and updater.
5. **Integrations:** secure send, share sheets, Open With and Explorer / Finder context menus, external editors.
