# Ferry Cycle 3: stream brief (2026-10-23)

Nine parallel streams, one plan and one worktree each, merged into one integration branch `feat/c3-all`. Source of truth for behaviour: [requirements](../../requirements/ferry-optimize-requirements.md). Existing architecture: [Cycle 1 spec](2026-10-21-ferry-cycle1-foundation-phase1-design.md) and [Cycle 2 spec](2026-10-22-ferry-cycle2-card-tools-design.md). Where this brief and the requirements disagree, this brief wins.

**Out of scope for every stream:** secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP automation.

## Streams

| Id | Name | Scope |
| --- | --- | --- |
| A | video | ffmpeg sidecar + `ferry_core::ffmpeg`; video optimise (hardware encoders where present, software fallback); downscale, crop, convert and compression on video; playback speed, remove audio, video to GIF; compare/preview for video |
| B | audio | audio optimise; loudness targets (Streaming, Apple Music, Podcast, Broadcast); cover-art options. Consumes A's ffmpeg helper |
| C | watchers | screenshot and screen-recording watcher; folder watchers; their Settings pane |
| D | drop-batch | drop zone; batch window with progress; compact list with selection bar |
| E | pdf-formats | PDF optimise, DPI slider, crop, extract pages as images; TIFF, HEIC, AVIF; animated WebP |
| F | pipelines | pipeline engine + editor; preset zones; "strip metadata" and "re-optimise with encoder" menu items; adaptive compression. Step kinds are a registry: ship image steps now; video/audio/PDF steps register from A, B and E |
| G | app-services | updater; settings sync; hotkey editor; "+" rail editor choosing which card tools show |
| H | integrations | share sheet incl. AirDrop/Nearby Share; Open with / Edit with; Finder/Explorer context menus; Dropshare and shelf apps |
| I | clipboard-fixes | accumulate results to paste together; copy file paths instead of image data; auto-copy; the nine deferred minors listed below |

Deferred minors for I: GRAY ICC kept on RGB JPEG output; CMYK JPEG fails with bare "CMYK JPEG"; downscale after Exact crop drops the crop; second Cmd+D in compare window needs hover to close; Space opens preview while typing in Settings; stale comments at `Stack.svelte:86-87` and `compare.rs:122`; queued job cancelled then re-edited can run twice (stale cancel token); twin of a renamed parent keeps card name only, not the rename stem; Copy/Copy path on a twin before first run points at the placeholder.

## Cross-stream contract

- **ffmpeg (A produces, B and F consume).** `crates/ferry-core/src/ffmpeg.rs`: `pub fn locate() -> Result<PathBuf, FerryError>` and `pub fn run(args: &[OsString], cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<(), FerryError>` (progress 0.0..=1.0). Sidecar binary `src-tauri/binaries/ffmpeg-<triple>`, wired like the existing gifsicle sidecar.
- **Pipeline step registry (F produces, A/B/E consume later).** F defines a `StepKind` registry in `crates/ferry-core/src/pipeline_steps.rs`. A, B and E do not touch pipelines; F's plan lists the exact hook each of them must call to register a step, and F ships image steps only.
- **Card tools on new media (A, B, E).** Reuse the Cycle 2 recipe model (`Job` recipe re-applied to the backed-up original, `Target`, `source_state`). Extend it; do not add a parallel edit model.

## Shared files and merge rules

These files are touched by many streams. Make additions at a distinct, clearly named anchor (a new `mod`/`fn`/key group named for your stream), never reorder or reformat existing lines:
`src-tauri/src/lib.rs`, `src-tauri/src/commands.rs`, `src-tauri/src/state.rs`, `src-tauri/src/menu.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/windows.rs`, `src-tauri/src/platform/mod.rs`, `crates/ferry-core/src/settings.rs` (settings schema), `crates/ferry-core/src/lib.rs`, `src/lib/i18n/en.json`, `src/stack/Card.svelte`, `src/stack/Stack.svelte`, `src/lib/types.ts`, `src-tauri/tauri.conf.json`, `src-tauri/capabilities/*`, `Cargo.lock`.

- **Settings schema:** each stream adds its own `[section]` struct with `#[serde(default)]`, with no version bump unless it renames or removes a field. List every new key in the plan.
- **i18n:** every new string goes under a key prefix named for the stream (`video.*`, `audio.*`, `watch.*`, `drop.*`, `pdf.*`, `pipe.*`, `svc.*`, `share.*`, `clip.*`).
- **New windows:** register in `windows.rs` with one added match arm each.

## Plan requirements

Each plan follows `superpowers:writing-plans` and is self-contained for a Sonnet implementer who sees only its own plan. Include the Global Constraints and Review Focus sections. State every new dependency (crate, npm package, binary) with the exact version and its licence; flag any that is GPL/LGPL/AGPL or patent-encumbered. List every shared-file edit with its anchor. Mark each task that cannot be verified by automated tests ("manual check") and say exactly what to look at. Windows code that cannot be compiled on this Mac must be isolated behind `cfg(windows)` and called out.
