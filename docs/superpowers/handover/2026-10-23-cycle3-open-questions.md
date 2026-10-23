# Cycle 3 plan writers: open questions for the user

One section per stream, added as each plan lands. Answer before implementation starts.

## I clipboard-fixes (`plans/2026-10-23-ferry-sI-clipboard-fixes.md`, 14 tasks, no new dependencies)
1. Exact crop then downscale: plan applies the scale to the Exact size (1200x628 at 50% gives 600x314), changing the Cycle 2 rule that size and scale replace each other. Alternative keeps the rule but scales the cropped original (can come out larger, 1440x754).
2. "Copy file paths" becomes "instead of the image": apps that only paste image data get no pixels while it is on. OK?
3. Auto-copy: one toggle for drops and watched folders, default off, first run only.
4. Cross-stream: C and D must call `jobs::start_auto_copied` instead of `jobs::start`; tell them or swap at merge.
5. Accumulated collection lifetime unchanged: since launch or last Clear all; closing a card does not remove it.
- Dropped minor: "Copy/Copy path on a twin before first run" is unreachable from the UI (`pipeline.rs:352`, `cards.ts:25`, `menu.rs:122-129`, `jobs.rs:227-230`); Task 3 adds an existence check anyway.
- **Decided (user accepted recommendations):** 1 scale applies to the Exact size; 2 yes, paths instead of image; 3 as planned; 4 integrator swaps C/D's `jobs::start` calls to `jobs::start_auto_copied` when merging into `feat/c3-all` (I's function does not exist on `main`, so C/D cannot call it in their worktrees); 5 as planned.

## C watchers (`plans/2026-10-23-ferry-sC-watchers.md`, 11 tasks; new dep `notify` 8.2.0 CC0-1.0, transitive all permissive)
1. Screenshot/recording watching defaults on: on upgrade Ferry starts optimising Desktop screenshots in place and macOS asks for Desktop access. Keep, or default off until onboarding exists? **Recommend: default off.**
2. macOS `.mov` counts as a recording by screenshot marker or name "Screen Recording" (English only). Drop the name fallback if recordings carry the marker? **Recommend: keep the fallback until the manual check answers it.**
3. Watched folders are not recursive. Per-folder option? **Recommend: not this cycle.**
4. Windows uses default `Videos\Captures`; follow a moved folder via registry? **Recommend: not this cycle (cannot test on the Mac).**
5. A watched USB folder on Windows blocks "Safely remove" until paused. Acceptable? **Recommend: yes for now.**
- Cross-stream: `src-tauri/src/watch/hooks.rs` has video/audio/PDF hooks (`VIDEO: bool = false`, `video_job(...) -> None`) for A/B/E to fill at merge.
- **Decided (user accepted recommendations):** 1 default off; 2 keep name fallback until the manual check; 3 no; 4 no; 5 yes. The implementer must change the plan's on-by-default screenshot/recording watching to off.

## H integrations (`plans/2026-10-23-ferry-sH-integrations.md`, 13 tasks; `windows` 0.62.2 + `windows-collections` 0.3.2 MIT/Apache, already in Cargo.lock; objc2 feature additions only)
1. Which of Dropshare, Yoink, Dropover, Dockside are installed for the manual check? Bundle ids are guesses; detection falls back to the app name. **User to answer.**
2. "Optimise with Ferry" sits under "Show more options" on Windows 11 (top-level needs a signed package). **Recommend: accept.**
3. Register HEIC, TIFF, AVIF, MP4/MOV/M4V and PDF now? Until A and E merge they show the "isn't an image" card. **Recommend: register them; every stream merges into `feat/c3-all` before release, so the gap never ships.**
4. macOS manual checks need the built `Ferry.app` in `/Applications`, replacing installed 0.2.0. **Recommend: OK; reinstall from the v0.2.0 release if needed.**
5. With no editor chosen, Cmd+E opens the default app and the menu offers a one-click chooser. **Recommend: as planned.**
- Known risks to accept: Windows hides the verb for >15 selected files; a cold start opening several files from Explorer can lose some to the single-instance race; macOS Services may need enabling in System Settings or `pbs -update`. **Recommend: accept for this cycle.**

## D drop-batch (`plans/2026-10-23-ferry-sD-drop-batch.md`, 11 tasks; no new deps, features only: objc2-app-kit `NSEvent`, windows-sys `Win32_UI_Input_KeyboardAndMouse`)
1. Windows has no drag-start signal; plan shows an edge tab after any left-press moving 24 px for 300 ms, which also flashes on window moves and text selection. **Recommend: default to "modifier tap only" on Windows; keep auto-show as an opt-in.**
2. Keep-originals modifier is Shift (Option/Alt is the zone tap). **Recommend: yes.**
3. No separate backup phase in the batch; each original is backed up before its own write, so "Backing up N of M" never shows. **Recommend: yes.**
4. Batch threshold 20 files; cards before compact list stays 5. **Recommend: yes.**
5. List keys (Cmd+A, Esc, arrows, Shift+arrows) taken from the frontmost app while hovering the list. **Recommend: require a click on the list first; those keys are too common in other apps to take on hover.**
6. Drop onto the menu-bar icon deferred (Tauri tray has no drop API). **Recommend: yes, defer.**
7. Batch Adjust panel only has compression/Extreme, downscale %, convert-to. **Recommend: yes for this cycle.**
- Brief mismatch: `src/lib/types.ts` does not exist; TS mirrors live in `src/lib/ipc.ts`; D adds `src/lib/drop-ipc.ts`.
- Hooks: A/B/E extend `intake::classify`; F receives `drag:tap` and may wrap `intake::accept`; I gets `came_from_drop`. Integrator swaps D's `jobs::start` to I's `jobs::start_auto_copied` at merge.

## G app-services (`plans/2026-10-23-ferry-sG-app-services.md`, 15 tasks; new dep `tauri-plugin-updater` 2.14.0 Apache-2.0/MIT, transitive all permissive)
**User actions (implementer cannot do):** (1) `pnpm tauri signer generate -w ~/.tauri/ferry-updater.key` with a strong password, back up key and password (losing either means installed copies never update again); (2) put `~/.tauri/ferry-updater.key.pub` content into `plugins.updater.pubkey` in `src-tauri/tauri.conf.json`; (3) add GitHub secrets `TAURI_SIGNING_PRIVATE_KEY` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` (releases stop at "Check updater signing" until done); (4) keep Releases public; (5) users on 0.2.0 download the first updater-enabled release by hand once.
1. Update prompt notes come from the fixed "Unsigned build..." `releaseBody`; keep or per-version notes file? **Recommend: keep this cycle.**
2. Check 1 min after start, then every 6 h. **Recommend: yes.**
3. Custom rail order is applied with CSS, so Tab order differs from what is shown. **Recommend: accept now; fix the rail markup after integration, once `Card.svelte` merges settle.**
4. No tray "update available" dot; menu item and pill show it. **Recommend: accept.**
5. Sync split: launch at login, Dock icon, sync settings and skipped version stay local; hotkeys, hover keys and ignored apps sync only between same-OS computers. **Recommend: yes, and the integrator adds C's watched folders and H's per-type "Edit with" apps to `settings_sync::LOCAL` (both hold machine-specific paths).**
- Windows update install built but not tested on a real desktop.

## A video (`plans/2026-10-23-ferry-sA-video.md`, 15 tasks; FFmpeg 9.0.2 sidecar built `--enable-gpl --enable-version3`, so GPL-3.0-or-later like Ferry; static x264, x265 (GPL-2.0+), LAME (LGPL-2.0+), libvpx, SVT-AV1, dav1d, Opus (BSD); Windows-only headers MIT; CI adds actions/cache@v6, msys2/setup-msys2@v2)
Patent-encumbered: H.264, HEVC, AAC; VP9/AV1 have third-party pool claims.
1. Patents on software encoders: (a) ship x264+x265, get legal advice before 1.0; (b) drop x265, HEVC by hardware only; (c) drop both, H.264/HEVC by hardware only. Plan assumes (a). **Recommend: (b). HEVC pools are the more aggressive, and VideoToolbox/NVENC/QSV/AMF cover HEVC on most current machines. Not legal advice.**
2. GPL source offer: attach a third-party sources archive to each release; also adds gifsicle's licence and source, missing in 0.1.x and 0.2.0. **Recommend: yes (fixes a GPL compliance gap).**
3. Video to GIF uses ffmpeg palette filters + gifsicle instead of gifski (AGPL-3.0). **Recommend: yes.**
4. FR-T3 auto-optimise of copied video files is owned by no stream. **Recommend: one integration task on `feat/c3-all` after A and I merge (needs both).**
5. First ffmpeg CI build 30-60 min per OS, cached after. **Recommend: accept; avoids trusting third-party prebuilt binaries.**
6. Stream C accepts `.avi`, which A does not support. **Recommend: drop AVI from C.**
- Mac setup: `brew install nasm pkgconf meson ninja cmake`; first local ffmpeg build 30-45 min. Intel Mac builds x86_64 only locally (universal in CI); VideoToolbox quality mode is Apple Silicon only, so Fast is bitrate-driven on the Intel Mac.
- Stream C's video hook becomes `Some(Job::file(...))` at merge.

## F pipelines (`plans/2026-10-23-ferry-sF-pipelines.md`, 18 tasks; new dep `ssimulacra2` 0.5.1 BSD-2-Clause, transitive MIT/BSD; avoided AGPL `dssim`)
1. Defer `runScript`, OS automation and `copyLinkForSending` steps. **Recommend: yes (security design needed; secure send out of scope).**
2. `images.adaptive` means adaptive compression; `switch_png_jpeg` becomes live (switch when the other format is >=10% smaller). **Recommend: yes.**
3. Adaptive targets SSIMULACRA2 80 Normal / 70 Aggressive, quality 30-95 step 5, max 5 encodes. **Recommend: accept, tune after the manual check.**
4. F defines `ferry_core::optimise::Encoder` and `Edits.encoder`; A also adds an encoder choice to `Edits`. **Recommend: align A's plan now to reuse F's definition, so the two do not collide at merge.**
5. Post-merge edits to D: emit `drag:ended`; swap D's `jobs::start` in `intake::accept` to `pipe_jobs::start_assigned(..., Trigger::DropZone, ...)`; draw `pipe.zones.hint_tap` in D's zone. **Recommend: yes, as integration tasks.**
6. C must ignore files written by F's copy step. **C records every write made through `fsutil::atomic_replace`; integrator checks F's copy step writes through it (or records the write).**
7. Text mode `step key=value` with a suggestion list as "autocomplete". **Recommend: yes.**
8. Clipboard pipeline that forks: clipboard keeps the main result, forks as cards. **Recommend: yes.**
9. Pipeline rail buttons are a post-merge edit on G's rail editor. **Recommend: yes.**
- Hook for A/B/E: `pub fn register(r: &mut crate::pipeline_steps::Registry)` in `{video,audio,pdf}_steps.rs`, one line each in `pipeline_steps::builtin()` under `// Later streams register their steps here, one line each:`.
- C calls `pipe_jobs::start_assigned(..., Trigger::Pipeline(id), ...)`; G rail ids `pipeline:<id>` call `pipe_run`.

## B audio (`plans/2026-10-23-ferry-sB-audio.md`, 16 tasks; no new deps; uses A's sidecar: FFmpeg GPL-3.0+, LAME LGPL-2.0+, Opus BSD-3, native aac; libfdk_aac ruled out (non-free); AAC may be patent-encumbered)
Aligned to A's plan: uses `ffmpeg::exec -> Captured`, A's refactor names (`Content`, `backup_path`, `atomic_replace_from`, `replace_with`), `restore_audio` like `restore_video`, progress in step text.
1. Merge order: wait for A's Tasks 1-4 and 9, or start after Task 1 and accept a `sniff.rs` conflict. **Recommend: wait for 1-4 and 9.**
2. Ogg Vorbis output defaults to Opus (`.ogg` becomes `.opus`). **Recommend: keep Opus; no libvorbis.**
3. Add ATSC A/85 (-24 LKFS, -2 dBTP) broadcast option? **Recommend: not this cycle.**
4. Audio A/B compare (Cmd+D disabled on audio cards). **Recommend: Cycle 4.**
5. Space on Windows audio cards does nothing; open default player instead (takes focus)? **Recommend: leave it doing nothing.**
6. Use Apple `aac_at` on macOS when present? **Recommend: yes (better quality at the same bitrate, one start-up check).**
7. Defaults: loudness Off; cover art Optimise (no resize); 192 kbps MP3/AAC, 128 kbps Opus; size limit 2000 MB. **Recommend: yes.**
- Merge notes: C sets `AUDIO = true`, routes `audio_job` through `Job::file`, stops counting audio as image, and puts `selfwrite::record` in the shared `replace_with` helper. A: B's M4A brand check runs before A's `ftyp` branch. D: nothing needed.
