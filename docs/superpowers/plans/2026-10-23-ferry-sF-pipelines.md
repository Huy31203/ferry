# Ferry Cycle 3, stream F: Pipelines Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let a user build ordered pipelines of image steps, run them from the clipboard, preset zones and result cards, and get three new card tools: adaptive compression, "Strip metadata" and "Re-optimise with encoder". Each pipeline run encodes the file once from its backed-up original.

**Architecture:** A pipeline is stored as JSON in a new `[pipelines]` settings section, as an ordered list of `{kind, params}` steps. A `StepKind` registry, keyed by (media, id), turns the steps into the Cycle 2 recipe: `compile` folds every transform step into one `Edits`, plus output-location overrides and after-steps (copy). The existing `Job::run` then applies that recipe to the backed-up original, as every card edit does today. `fork` splits off a second branch, which runs as a "keep both" twin job (`Job::fork`). Adaptive compression is a SSIMULACRA2-guided quality search inside the existing encoders. The app adds four one-line hooks to `jobs.rs` and one to `clipboard.rs`, plus two new windows: the pipeline editor and the preset-zones overlay.

**Tech Stack:** Rust 2024 (`ferry-core`, Tauri 2.12 app), Svelte 5 + TypeScript, vitest. New crate: `ssimulacra2` 0.5.1.

**Spec:** `docs/superpowers/specs/2026-10-23-ferry-cycle3-streams.md` (binding; it wins over the requirements), `docs/requirements/ferry-optimize-requirements.md` (R§2.4, R§2.5, R§2.7, R§4.6, R§4.7, R§4.9, R§4.12 Pipelines, R§4.13), `docs/superpowers/specs/2026-10-22-ferry-cycle2-card-tools-design.md` (the recipe model: C2§4.2, §4.3), `docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md` (C1§4.2 encoders, §5.7 settings, §5.8 metadata).

---

## Global Constraints

- Licence GPL-3.0-or-later (workspace). Every new dependency is listed in "New dependencies" below. Add no others. In particular, do not use `dssim`/`dssim-core`, which is AGPL-3.0.
- Rust edition 2024, stable toolchain (`rust-toolchain.toml`). `cargo fmt` clean, `cargo clippy --workspace --all-targets -- -D warnings` clean.
- Never use `var` in JavaScript or TypeScript. Use `const`, or `let` where the value changes.
- Every user-visible string goes in `src/lib/i18n/en.json` under the `pipe.*` prefix. The file is a flat map of strings that Rust also parses: Rust reads it with `ferry_core::i18n::{t, tf}`, TS with `t()` from `src/lib/i18n.ts`. The only exceptions are the existing values of `images.adaptive`, `images.adaptive.help` and `images.switch_png_jpeg.help`, which Task 14 rewrites in place.
- Shared files (brief §"Shared files and merge rules"): edit them only at the anchors listed in "Shared-file edits" below. Mark every added block with a `// stream F: pipelines` comment (`<!-- stream F -->` in HTML). Never reorder or reformat existing lines.
- Settings: one new top-level section `pipelines`, with `#[serde(default)]` everywhere. `settings::VERSION` stays `2`.
- The original is sacred (R§3.3). Every pipeline run goes through `ferry_core::pipeline::Job::run`, which backs up first and writes atomically. The copy step never overwrites: on a collision it picks a unique name with `output::unique`.
- Out of scope: secure send (so no `copyLinkForSending` step), onboarding, Vietnamese, signing/notarisation, Windows ARM64, CLI/SDK/MCP. Also deferred (see Open questions): the `runScript` and OS-automation steps.
- This plan ships IMAGE steps only. The common steps (`copy`, `fork`, `if_name_contains`) are registered for every media type. No video, audio or PDF transform step is registered, so the editor offers only Images.
- No Windows-only code is needed. All new native work reuses `platform::make_non_activating`, `platform::show_without_focus` and `platform::hide`, which already exist for both OSes. Do not add `cfg(windows)` code. Windows is built and smoke-tested only in CI.
- Before you run any command, set up the environment from the worktree root. If this is a fresh worktree, first copy the git-ignored `src-tauri/binaries/` from the main checkout:
  ```
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  ```
- `cargo test` takes one name filter per command. Run filters one at a time.
- Never run `pnpm tauri dev` without asking the user first: it takes over the real clipboard. Manual-check tasks begin by asking.
- Commit trailer on every commit: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`. Do not push.

## Review Focus

1. **A settings file from a newer Ferry.** It may hold a pipeline with an unknown step kind, an unknown media type or unknown fields. It must load without resetting the user's settings, survive a save byte for byte (except an unknown media name, which reads back as `"other"`), and refuse to run while saying why. Tests: Task 7 `a_pipeline_from_a_newer_ferry_survives_a_load_and_save` and `a_broken_pipeline_is_dropped_not_the_whole_file`; Task 8 `an_unregistered_step_stops_the_run_with_its_name`.
2. **A cancel during a pipeline run.** Neither the copy steps nor the forks may run then or later. A plan left pending must not fire on the user's next unrelated edit. Test: Task 11 `a_pending_plan_only_applies_to_the_recipe_it_set`.
3. **The copy step meets an existing or unwritable target.** An existing file is never overwritten (the copy gets ` (2)`). An unwritable folder keeps the result and shows an amber note, not an Error card. Tests: Task 10 `a_copy_never_overwrites_and_lands_beside_with_a_number` and `an_unwritable_target_is_an_error_naming_the_folder`; Task 11 `a_failed_copy_becomes_a_note`.
4. **Adaptive compression on odd images.** On images smaller than 8×8, or when the metric fails, it falls back to the slider quality and never shows an Error. Animated GIF ignores it. Tests: Task 3 `a_metric_error_falls_back_to_the_settings_quality` and `a_tiny_image_falls_back_instead_of_failing`.
5. **Deleting a pipeline that is still assigned.** The clipboard, drop-zone and zone assignments must clear, so nothing points at a missing id, and the zone tile shows "No pipeline for images". Tests: Task 7 `sanitise_gives_duplicate_ids_new_ones_and_clears_dangling_assignments`; Task 12 vitest `removing a pipeline clears every assignment`.

---

## Design decisions (read before the tasks)

### D1. The existing `pipeline.rs` and the new pipeline engine coexist, with no rename

`crates/ferry-core/src/pipeline.rs` (2322 lines) is the **job runner**: Cycle 1 named it for one job's flow (back up, optimise, write, report). It owns `Job`, `RunOptions`, `Job::run` and the Cycle 2 recipe re-application. Renaming it would rewrite every `use ferry_core::pipeline::…` in `jobs.rs`, `state.rs`, `clipboard.rs` and `menu.rs`. Streams A, B and E are extending `Job` in parallel for new media, so a rename would conflict across streams for no behavioural gain.

The user-facing pipeline engine therefore lives in new modules **beside** it, and **composes** it:

| Module | Role |
| --- | --- |
| `pipeline_steps.rs` | The `StepKind` registry (contract for streams A, B, E), plus the common steps `copy`, `fork` and `if_name_contains` |
| `image_steps.rs` | The image steps `optimise`, `convert`, `downscale`, `crop` and `strip_exif`, registered into the registry |
| `pipeline_def.rs` | The stored model (`Pipeline`, `Step`, `Zone`, `Pipelines`): the settings section, lenient loading, sanitising and starter templates |
| `pipeline_plan.rs` | `compile`, which turns a pipeline plus a file's facts into recipe branches; `check`, which collects all issues for the editor; captions |
| `pipeline_text.rs` | The editor's text mode: `parse` and `print` |
| `pipeline_after.rs` | Runs the copy step on a finished result |
| `pipeline_test.rs` | The editor's sandboxed test run |

`compile` yields an `Edits` (the Cycle 2 recipe). `jobs::start` or `jobs::edit` hands it to a `Job`, and `Job::run` applies it to the backed-up original exactly as a card edit does. Repeating a pipeline never compounds. Restore clears the recipe as before. After the run, the card stays fully editable: its recipe is the pipeline's recipe, and later card edits merge into it. Add one doc line at the top of `pipeline.rs` (Task 1) that names this relationship.

### D2. Encode once: what folds and what runs after

| Phase | Steps | Effect |
| --- | --- | --- |
| Transform | `optimise`, `convert`, `downscale`, `crop`, `strip_exif` | Fold into the branch's `Edits` (later steps override earlier ones, using `Edits::merge` rules) and set output-location overrides |
| Output | `copy` | Recorded as an `After` step. It runs on the branch's finished result file |
| Control | `fork` | Closes the current branch as a second result. The recipe so far carries on into the main branch, and after-steps reset |
| Control | `if_name_contains` | When it fails for this file, compilation stops there. Only the steps before it apply |

A transform after an output step in the same branch is an issue ("Steps that change the file must come before Copy, or after Fork"). The run is refused and the editor marks the step. Branches: `forks` (in order) and `main` (last). Every branch is encoded exactly once.

### D3. Pipelines are typed by media

`Pipeline.media` is one of `image | video | audio | pdf` (requirements: "one pipeline per file type"). The registry is keyed by `(media, id)`, and the editor offers only the steps registered for the pipeline's media. Assignments are per trigger and per media.

### D4. Adaptive compression, defined

- **Metric:** SSIMULACRA2, from the `ssimulacra2` crate 0.5.1 (BSD-2-Clause). It runs on 8-bit sRGB pixels, which are treated as `TransferCharacteristic::SRGB` and `ColorPrimaries::BT709`. Alpha is composited over mid-grey `#808080` on both images before scoring.
- **Sample regions:** if `width × height ≤ 1_048_576`, the whole image. Otherwise, four tiles of `min(512, w) × min(512, h)`, centred on the quadrant centres `(w/4, h/4)`, `(3w/4, h/4)`, `(w/4, 3h/4)` and `(3w/4, 3h/4)` and clamped inside the image. The score is the **minimum** tile score.
- **Target:** 80.0 for `Level::Normal` ("impossible to distinguish side by side at 1:1"). 70.0 for `Level::Aggressive`.
- **Search space:** quality candidates `30, 35, …, 95` (14 values). Binary-search for the lowest candidate whose score is at or above the target, assuming score rises with quality. Cap: `MAX_ENCODES = 5`. The bytes of the chosen candidate are kept, never re-encoded. If even 95 misses, keep 95's bytes (`reached: false`, logged).
- **Encoder mapping:** each candidate `q` is passed as `OptimiseOptions.compression = q`, with `level = Normal` and `adaptive = false`. The encoder keeps its own effort: if the original level was Aggressive and no encoder was chosen, the search uses `Encoder::Small`. So the knob is imagequant's max quality (PNG), mozjpeg quality (JPEG) or libwebp quality (WebP). **GIF never runs adaptive**: it uses the slider or settings quality, because gifsicle reshapes frames and a first-frame score would mislead. **`Encoder::VisuallyLossless` skips adaptive**, because it already fixes the quality.
- **Cancellation:** `o.cancel` is checked before every encode and before every score, and returns `OptimiseError::Cancelled`. One SSIMULACRA2 call is not interruptible, but the tiles keep each call to at most 1 MP.
- **Fallback:** a metric error (an image under 8 px, a size mismatch, a decode failure) means one plain encode at `quality(o)`, logged at `debug`. It never becomes an Error.
- **Where it applies:**
  - The setting `images.adaptive` drives plain runs.
  - A card's Adaptive detent sets `Edits.adaptive`.
  - A pipeline `optimise` step with `adaptive: true` does the same.
  - An explicit card quality (`Edits.compression`) or a fit-under-size target overrides the setting.
  - It is never used inside fit-under-size.

### D5. "Re-optimise with encoder" for images (R§4.6 Fast · Small · Visually lossless)

`ferry_core::optimise::Encoder { Fast, Small, VisuallyLossless }`, carried as `Edits.encoder: Option<Encoder>` (`None` keeps today's behaviour). It is plain-compatible: a plain run with an encoder still follows the Cycle 1 never-worse rule.

| Encoder | PNG (imagequant speed, oxipng preset) | JPEG (mozjpeg) | WebP | GIF (gifsicle) |
| --- | --- | --- | --- | --- |
| `None`, Normal | (8, 1), as today | progressive, as today | method 4 | `-O3 --lossy=L` |
| `None`, Aggressive | (4, 4), as today | as today | method 4 | as today |
| `Fast` | (8, 1) | `set_fastest_defaults()` then quality (baseline JPEG) | method 2 | `-O2 --lossy=L` |
| `Small` | (4, 4) | as today (mozjpeg's max-compression profile) | method 6 | `-O3 --lossy=L` |
| `VisuallyLossless` | no quantisation: lossless RGBA through oxipng preset 2, same chunk rules | quality `max(q, 90)` | quality `max(q, 90)`, method 6 | `-O3`, no `--lossy` |

`optimise::quality(o)` floors at 90 when `o.encoder == Some(VisuallyLossless)` (after the aggressive −20).

**Stream A also needs an encoder choice** (hardware, software or visually lossless). This plan defines the enum and the `Edits.encoder` field, and A should reuse them. See Open questions.

### D6. "Strip metadata" (R§4.6)

`Edits.strip_metadata: Option<bool>` (`None` follows `images.strip_metadata`). Any `Some` makes the recipe non-plain, so the result is always written, even when it is not smaller: the user asked for it. The C1§5.8 rules (keep the ICC profile, DPI and orientation) are unchanged. The menu item is a check item showing the effective value, and a click flips it.

### D7. PNG ⇄ JPEG switching (`images.switch_png_jpeg`, R§2.3)

This applies on plain runs only, when the setting is on and the source is a still PNG or a JPEG. A PNG source also needs every alpha value at 255. The other format is encoded from the decoded raster through `edit::raster::encode` (adaptive applies there too, when on). It wins only when `other.len() * 100 <= own_len * 90` (at least 10 % smaller than the same-format result), where `own_len` is the same-format result's length, or the input's length when that result was not Smaller. It must also be ≥ 1 % smaller than the input. The result is a normal conversion (`OutputKind::Converted`, its location rule, the card's "converted" label), so Restore and the one-result-per-job rules from Cycle 2 apply unchanged.

### D8. Preset zones and stream D (R§2.2 FR-T7, R§4.9)

A zone is `{id, name, icon, pipelines: {image, video, audio, pdf}}` in `pipelines.zones`. F owns the zones overlay window (`zones`) and everything inside it: tiles filtered to the dragged media, hover, drop, and the right-click menu Edit / Go to pipeline / Remove.

**Seam with D, taken from D's plan (`2026-10-23-ferry-sD-drop-batch.md`, "Cross-stream hooks"); D's files are not edited.** D's `DragWatch` emits every modifier tap during a drag, other than its own zone modifier, as the app-wide Tauri event `drag:tap` with payload `{ "modifier": "alt" | "shift" | "control" | "command" }`. F listens in Rust with `app.listen_any("drag:tap", …)`:

| Event | Payload | F does |
| --- | --- | --- |
| `drag:tap` with `modifier == "control"` (the preset modifier on both OSes, R§2.10 "Tap Control" / "Tap Ctrl") | as above | Toggle: shows the zones window centred on the cursor in drag mode, or hides it when it is already showing |
| `drag:tap`, any other modifier | | Ignored |
| `drag:ended` (requested from D, see Open questions; not in D's plan yet) | `null` | Hides it unless a drop is in progress |

D emits no drag-end event today, so drag mode also hides itself:
- 1.5 s after the webview's drag `leave`, unless the drag re-enters;
- 5 s after showing, if no drag ever entered the window;
- 300 ms after a drop.

Other seams:

- D's Drop zone settings pane opens F's zone editor by invoking the command `pipe_open_editor` with `{ pipeline: null, tab: "zones" }`.
- The pipeline assigned to the drop zone is stored by F (`pipelines.drop_zone`). D's `drop_batch::intake::accept(app, paths, keep)` starts one job per accepted file. After both streams merge, whichever merges second changes the single `jobs::start(app, job, level, None)` call inside D's `accept` to `crate::pipe_jobs::start_assigned(app, job, crate::pipe_jobs::Trigger::DropZone, level, None)`. Until then, drop-zone drops run the plain optimise, and the editor's "Drop zone" assignment is stored but has no effect.
- D asks F to add the zone's hint row ("Tap Ctrl for presets"). F supplies the string as `pipe.zones.hint_tap`. Drawing it inside D's `drop.html` is the same post-merge integration edit, because F does not edit D's files.
- Preset-zone drops stay F's own (`pipe_zone_drop`), and they do not go through D's `accept`: a zone runs one pipeline per file, with no batch window.

Until D merges, the overlay opens from the editor's **Preview on screen** button, which has a Done tile. A Finder drag onto a tile works natively, because the window accepts OS file drops through Tauri's webview drag-drop events.

### D9. Other seams

| Consumer | Call |
| --- | --- |
| C (watched folders) | `crate::pipe_jobs::start_assigned(app, job, Trigger::Pipeline(id), Level::Normal, None)`, where `id: u32` is stored in C's own folder settings |
| G (rail editor "+" slot) | G's plan reserves rail tool ids `pipeline:<id>` (`svc_settings::is_tool_id` accepts them). A rail button with that id calls `invoke('pipe_run', { id: <job id>, pipeline: <u32> })`, and the picker lists `settings.pipelines.library`. Drawing these buttons needs G's `src/lib/rail.ts` and its `Card.svelte` block, so it is a post-merge integration edit, not a task here. Until then, pipelines run from the card menu's **Run pipeline ▸** |

### D10. Pipeline editor is its own window

The editor is the `pipelines` window, 1000×680 (min 860×560). Its three columns (library, step editor, assignments and test) do not fit the Settings content area. The Settings window gains a **Pipelines** pane holding the library list (R§4.12), whose buttons open the editor.

---

## File structure

**ferry-core (new):**

- `crates/ferry-core/src/metric.rs`: the SSIMULACRA2 score over sample regions.
- `crates/ferry-core/src/optimise/adaptive.rs`: the quality search (D4).
- `crates/ferry-core/src/optimise/switch.rs`: PNG ⇄ JPEG switching (D7).
- `crates/ferry-core/src/pipeline_steps.rs`: the registry types, `builtin()`, `registry()` and the common steps.
- `crates/ferry-core/src/image_steps.rs`: the five image steps.
- `crates/ferry-core/src/pipeline_def.rs`: the stored model and starters.
- `crates/ferry-core/src/pipeline_plan.rs`: `compile`, `check` and `caption`.
- `crates/ferry-core/src/pipeline_text.rs`: text mode `parse` and `print`.
- `crates/ferry-core/src/pipeline_after.rs`: the copy step.
- `crates/ferry-core/src/pipeline_test.rs`: the sandboxed test run.

**ferry-core (modified):**

- `optimise/{mod,png,jpeg,webp,gif}.rs`: `Encoder` and `adaptive`.
- `edit/mod.rs`: new recipe fields and `EditsPatch::from_recipe`.
- `pipeline.rs`: `RunOptions` fields, `run_recipe` option building, switching in the plain branch, a doc line.
- `settings.rs`: the `pipelines` field, its default and the sanitise call.
- `lib.rs`: the new `mod` lines.
- `testutil.rs`: photo fixtures.

**src-tauri (new):**

- `src-tauri/src/pipe_jobs.rs`: `PipeState`, the triggers, starting pipelines, the hooks called from `jobs.rs`.
- `src-tauri/src/pipe_menu.rs`: the card-menu items for stream F.
- `src-tauri/src/pipe_windows.rs`: the editor window.
- `src-tauri/src/preset_zones.rs`: the zones window, its D seam listener and its tile menu.
- `src-tauri/src/pipe_commands.rs`: every `pipe_*` Tauri command.

**src-tauri (modified, at anchors):** `lib.rs`, `jobs.rs`, `clipboard.rs`, `menu.rs`, `windows.rs`, `capabilities/default.json`.

**Frontend (new):**

- Editor: `src/pipelines.html`, `src/pipelines/{main.ts,Editor.svelte,Library.svelte,StepList.svelte,Inspector.svelte,TextMode.svelte,Assignments.svelte,TestWell.svelte,ZonesTab.svelte}`.
- Zones overlay: `src/zones.html`, `src/zones/{main.ts,Zones.svelte,ZoneGrid.svelte}`.
- Shared: `src/lib/pipelines.ts` plus `src/lib/pipelines.test.ts`, `src/lib/zone-icons.ts`.
- Settings: `src/settings/controls/PipelineLibrary.svelte`.

**Frontend (modified):** `src/lib/ipc.ts`, `src/lib/card.fixture.ts`, `src/lib/edits.ts` plus its test, `src/lib/settings.ts`, `src/lib/settings.fixture.ts`, `src/lib/schema.ts`, `src/settings/Row.svelte`, `src/lib/i18n/en.json`, `vite.config.ts`.

---

## New dependencies

| Item | Version | Licence | Where | Notes |
| --- | --- | --- | --- | --- |
| `ssimulacra2` (crate) | `=0.5.1`, default features | BSD-2-Clause | `crates/ferry-core/Cargo.toml` `[dependencies]` | Published 2024-12-29. Default feature `rayon` (already in `Cargo.lock` through oxipng) |
| transitive: `yuvxyb` | 0.4.x (resolves 0.4.2) | MIT | | |
| transitive: `yuvxyb-math` | 0.1.x | MIT | | |
| transitive: `av-data` | 0.4.4 | MIT | | Pulls `byte-slice-cast` (MIT), `num-derive` and `num-rational` (MIT/Apache-2.0); `bytes`, `num-traits` (already present) |
| transitive: `v_frame` | 0.3.x (0.3.9) | BSD-2-Clause | | Pulls `aligned-vec` (MIT) |
| transitive: `thiserror` 2, `log` | | MIT/Apache-2.0 | | Already in `Cargo.lock` |

There are no new npm packages, no new binaries or sidecars, and nothing GPL, LGPL, AGPL or patent-encumbered. After adding the crate, run `cargo tree -p ferry-core -e normal --prefix none | sort -u` and confirm that every new line is one of the crates above. If `ssimulacra2` fails to build, stop and report. Do not substitute another metric crate.

## Shared-file edits (every one, with its anchor)

| File | Anchor | Edit | Task |
| --- | --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | After the last `pub mod workdir;` | Comment `// stream F: pipelines`, then `pub mod image_steps; pub mod metric; pub mod pipeline_after; pub mod pipeline_def; pub mod pipeline_plan; pub mod pipeline_steps; pub mod pipeline_test; pub mod pipeline_text;` (one per line, added as each task creates its file) | 2, 5–10 |
| `crates/ferry-core/src/settings.rs` | `struct Settings`: after `pub hotkeys: Hotkeys,` | `// stream F: pipelines` + `pub pipelines: crate::pipeline_def::Pipelines,` | 7 |
| `crates/ferry-core/src/settings.rs` | `impl Default for Settings`: after `hotkeys: Hotkeys::default(),` | `pipelines: crate::pipeline_def::Pipelines::default(), // stream F: pipelines` | 7 |
| `crates/ferry-core/src/settings.rs` | `fn sanitised`: before `self.version = self.version.max(VERSION);` | `self.pipelines = self.pipelines.sanitised(); // stream F: pipelines` | 7 |
| `src-tauri/src/lib.rs` | After `mod windows;` | `// stream F: pipelines` + `mod pipe_commands; mod pipe_jobs; mod pipe_menu; mod pipe_windows; mod preset_zones;` | 11–13, 17 |
| `src-tauri/src/lib.rs` | `setup()`: after `menu::init(app);` | `app.manage(pipe_jobs::PipeState::default()); // stream F: pipelines` (Task 11), and `preset_zones::init(app);` (Task 17) | 11, 17 |
| `src-tauri/src/lib.rs` | `generate_handler![…]`: after `commands::pick_folder,` | `// stream F: pipelines` + the `pipe_commands::…` list (Task 12 and Task 17) | 12, 17 |
| `src-tauri/src/commands.rs` | none | Not edited: stream F's commands live in `pipe_commands.rs` | |
| `src-tauri/src/menu.rs` | See Task 13 (six insertions, each marked) | `Action::Pipe` variant and the item slots | 13 |
| `src-tauri/src/windows.rs` | After `pub const SETTINGS: &str = "settings";` | `// stream F: pipelines` + `pub const PIPELINES: &str = "pipelines";` + `pub const ZONES: &str = "zones";` | 12 |
| `src-tauri/capabilities/default.json` | the `"windows"` array | Append `"pipelines", "zones"` (the one existing line changes; this cannot be avoided) | 12 |
| `src/lib/i18n/en.json` | End of file | One contiguous block of `pipe.*` keys, plus in-place value edits for `images.adaptive`, `images.adaptive.help` and `images.switch_png_jpeg.help` (Task 14) | 6–17 |
| `src/lib/types.ts` | none | Not edited (it does not exist at HEAD). Stream F's types live in `src/lib/pipelines.ts` | |
| `src/stack/Card.svelte`, `src/stack/Stack.svelte` | none | Not edited | |
| `src-tauri/tauri.conf.json`, `state.rs`, `tray.rs`, `platform/mod.rs` | none | Not edited | |
| `Cargo.lock` | | Regenerated by cargo for `ssimulacra2` | 2 |

Files that are not on the brief's list but that other streams may touch: `jobs.rs` (four hook insertions, Task 11), `clipboard.rs` (one line, Task 11), `pipeline.rs`, `edit/mod.rs`, `optimise/*`, `src/lib/ipc.ts`, `src/lib/schema.ts`, `src/settings/Row.svelte`, `vite.config.ts`. Mark every edit to them with `// stream F: pipelines` too.

## New settings keys

```jsonc
{
  "images": {
    "adaptive": false,        // existing key, now live: adaptive compression (D4)
    "switch_png_jpeg": false  // existing key, now live: PNG ⇄ JPEG switching (D7)
  },
  "pipelines": {
    "library": [              // Pipeline[]
      { "id": 1, "name": "Convert to WebP", "media": "image", "hide_result": false,
        "steps": [ { "kind": "convert", "params": { "to": "webp" } } ] }
    ],
    "clipboard": { "image": null, "video": null, "audio": null, "pdf": null }, // pipeline id per media
    "drop_zone": { "image": null, "video": null, "audio": null, "pdf": null },
    "zones": [                // Zone[]
      { "id": 1, "name": "WebP", "icon": "image",
        "pipelines": { "image": 1, "video": null, "audio": null, "pdf": null } }
    ]
  }
}
```

Defaults: `library` and `zones` are empty, and every assignment is `null`. Ids are `u32` values starting at 1 and unique within `library` and within `zones`. Unknown fields survive at every level, through `#[serde(flatten)] extra`.

## Edits recipe additions (`Edits` / `EditsPatch`, also mirrored in `src/lib/ipc.ts`)

| Field | Rust type | TS type | Default | Merge rule |
| --- | --- | --- | --- | --- |
| `encoder` | `Option<Encoder>` | `'fast' \| 'small' \| 'visually_lossless' \| null` | `None` | Set or clear |
| `strip_metadata` | `Option<bool>` | `boolean \| null` | `None` | Set or clear |
| `adaptive` | `bool` | `boolean` | `false` | `true` clears `compression` and `fit_under`. Setting `compression` or `fit_under` to `Some` sets it `false` |

`Edits::is_plain()` also requires `strip_metadata.is_none() && !adaptive`. `encoder` does not affect `is_plain`.

---

### Task 1: Encoder choice and the strip-metadata override in the recipe

**Files:**
- Modify:
  - `crates/ferry-core/src/optimise/mod.rs`: `Encoder`, `OptimiseOptions.encoder` and `OptimiseOptions.adaptive`; `quality` floors at 90.
  - `crates/ferry-core/src/optimise/png.rs`, `jpeg.rs`, `webp.rs`, `gif.rs`: the mapping from D5.
  - `crates/ferry-core/src/edit/mod.rs`: the new recipe fields, `EditsPatch` and `from_recipe`.
  - `crates/ferry-core/src/pipeline.rs`: options built from the recipe, and the doc line.
  - `src/lib/ipc.ts`: `Edits` fields.
  - `src/lib/card.fixture.ts`: `edits` defaults.
- Test: the unit-test modules in each modified Rust file.

**Interfaces:**
- Produces:
  - `pub enum ferry_core::optimise::Encoder { Fast, Small, VisuallyLossless }`: `#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]`, `#[serde(rename_all = "snake_case")]`.
  - `OptimiseOptions { …existing…, pub encoder: Option<Encoder>, pub adaptive: bool }`, default `None` and `false`. Task 3 gives `adaptive` its effect. Here it is only carried.
  - `Edits.encoder`, `Edits.strip_metadata`, `Edits.adaptive`, with the matching `EditsPatch` fields (`encoder: Option<Option<Encoder>>` and `strip_metadata: Option<Option<bool>>`, both with `deserialize_with = "present"`; `adaptive: Option<bool>`).
  - `impl EditsPatch { pub fn from_recipe(e: &Edits) -> EditsPatch }`: every field `Some(…)`, so `merge` makes the target equal to `e`.

- [ ] **Step 1: Write the failing tests**

```rust
// optimise/mod.rs tests
#[test]
fn visually_lossless_floors_quality_at_90() {
    let vl = |level, q| OptimiseOptions { level, compression: q, encoder: Some(Encoder::VisuallyLossless), ..OptimiseOptions::default() };
    assert_eq!(quality(&vl(Level::Normal, 40)), 90);
    assert_eq!(quality(&vl(Level::Aggressive, 80)), 90);
    assert_eq!(quality(&vl(Level::Normal, 95)), 95);
}

// optimise/png.rs tests
#[test]
fn visually_lossless_png_keeps_every_pixel() {
    let input = testutil::png(64, 64);
    let o = OptimiseOptions { encoder: Some(Encoder::VisuallyLossless), ..OptimiseOptions::default() };
    let out = optimise(&input, &o).unwrap();
    assert_eq!(decode_raster(&out).unwrap().rgba, decode_raster(&input).unwrap().rgba);
}

// optimise/jpeg.rs tests (use the existing `markers` helper)
#[test]
fn fast_writes_a_baseline_jpeg_and_the_default_stays_progressive() {
    let input = testutil::jpeg(200, 200, 95, &testutil::JpegExtras::default());
    let fast = optimise(&input, &OptimiseOptions { encoder: Some(Encoder::Fast), ..OptimiseOptions::default() }).unwrap();
    let default = optimise(&input, &OptimiseOptions::default()).unwrap();
    assert!(markers(&fast).iter().any(|(m, _)| *m == 0xC0) && !markers(&fast).iter().any(|(m, _)| *m == 0xC2));
    assert!(markers(&default).iter().any(|(m, _)| *m == 0xC2));
}
#[test]
fn visually_lossless_jpeg_at_40_equals_quality_90() {
    let input = testutil::jpeg(200, 200, 98, &testutil::JpegExtras::default());
    let vl = optimise(&input, &OptimiseOptions { compression: 40, encoder: Some(Encoder::VisuallyLossless), ..OptimiseOptions::default() }).unwrap();
    let q90 = optimise(&input, &OptimiseOptions { compression: 90, ..OptimiseOptions::default() }).unwrap();
    assert_eq!(vl, q90);
}

// optimise/webp.rs tests
#[test]
fn the_encoder_picks_the_webp_method() {
    let method = |encoder| config(&OptimiseOptions { encoder, ..OptimiseOptions::default() }).unwrap().method;
    assert_eq!((method(None), method(Some(Encoder::Fast)), method(Some(Encoder::Small)), method(Some(Encoder::VisuallyLossless))), (4, 2, 6, 6));
}

// optimise/gif.rs tests
#[test]
fn the_encoder_picks_the_gifsicle_flags() {
    let args = |encoder| optimise_args(&OptimiseOptions { encoder, ..OptimiseOptions::default() });
    assert_eq!(args(None), ["-O3", "--lossy=30", "--colors", "256"]);
    assert_eq!(args(Some(Encoder::Fast)), ["-O2", "--lossy=30", "--colors", "256"]);
    assert_eq!(args(Some(Encoder::VisuallyLossless)), ["-O3", "--colors", "256"]);
}

// edit/mod.rs tests
#[test]
fn a_full_recipe_round_trips_through_from_recipe() {
    let recipe = Edits { aspect: Some((16, 9)), framing: Framing::Smart, scale: Some(50), format: Some(Format::WebP),
        compression: Some(60), level: Level::Aggressive, encoder: Some(Encoder::Small), strip_metadata: Some(false), ..Edits::default() };
    let mut other = Edits { size: Some(Size::Width { width: 10 }), fit_under: Some(5), adaptive: true, ..Edits::default() };
    other.merge(EditsPatch::from_recipe(&recipe));
    assert_eq!(other, recipe);
}
#[test]
fn strip_makes_a_recipe_explicit_but_an_encoder_does_not() {
    assert!(Edits { encoder: Some(Encoder::Fast), ..Edits::default() }.is_plain());
    assert!(!Edits { strip_metadata: Some(true), ..Edits::default() }.is_plain());
}
#[test]
fn encoder_and_strip_are_set_and_cleared_by_patches() {
    let mut e = Edits::default();
    e.merge(patch(json!({ "encoder": "small", "strip_metadata": true })));
    assert_eq!((e.encoder, e.strip_metadata), (Some(Encoder::Small), Some(true)));
    e.merge(patch(json!({ "encoder": null, "strip_metadata": null })));
    assert_eq!((e.encoder, e.strip_metadata), (None, None));
}

// pipeline.rs tests (append at the end of `mod tests`, under `// stream F: pipelines`)
#[test]
fn an_encoder_choice_keeps_the_plain_never_worse_rule() {
    let env = Env::new();
    let path = env.file("dot.png", &testutil::png(1, 1));
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    job.edit(EditsPatch { encoder: Some(Some(Encoder::Fast)), ..EditsPatch::default() });
    let (finished, _) = run(&mut job, Level::Normal, &options(), &env);
    assert_eq!(finished.unwrap().update.state, JobState::Larger);
}
#[test]
fn a_strip_override_writes_a_result_without_exif() {
    let env = Env::new();
    let mut settings = Settings::default();
    settings.images.strip_metadata = false;
    let exif = testutil::exif_with_orientation(1, 4000);
    let input = testutil::jpeg(300, 200, 90, &testutil::JpegExtras { exif: Some(exif), ..Default::default() });
    let path = env.file("photo.jpg", &input);
    let mut job = Job::file(&path, &settings, true).unwrap();
    job.edit(EditsPatch { strip_metadata: Some(Some(true)), ..EditsPatch::default() });
    let (finished, _) = run(&mut job, Level::Normal, &options_with(&settings), &env);
    assert_eq!(finished.unwrap().update.state, JobState::Done);
    let out = fs::read(&path).unwrap();
    // Orientation 1 needs no minimal EXIF (C1§5.8), so no APP1 Exif header is left at all.
    assert!(!out.windows(6).any(|w| w == b"Exif\0\0"));
}
```

- [ ] **Step 2: Run the tests and confirm that they fail.** Run each of these: `cargo test -p ferry-core visually_lossless`, `cargo test -p ferry-core encoder`, `cargo test -p ferry-core from_recipe`, `cargo test -p ferry-core strip_`. Expected: compile errors (no `Encoder`, no `optimise_args`, no `from_recipe`).

- [ ] **Step 3: Implement**
  - `optimise/mod.rs`: the enum and fields above. In `quality`, apply `.max(90)` for `VisuallyLossless`, after the aggressive step.
  - `png.rs`:
    - `effort(o)` follows the D5 table: `Some(Fast)` gives (8, 1) and `Some(Small)` gives (4, 4); `None` keeps the level-based values.
    - Give `pack` a lossless branch for `VisuallyLossless`: skip imagequant, build `RawImage::new(w, h, ColorType::RGBA, BitDepth::Eight, rgba bytes)` and add the same chunks and ICC. Then `create_optimized_png(&Options::from_preset(2))`.
  - `jpeg.rs`: in both encoder setups (`run`, around line 142, and `write_raster`, around line 266), `Some(Fast)` calls `encoder.set_fastest_defaults()` before `set_quality` and skips `set_progressive_mode()`.
  - `webp.rs` `config`: `method` follows the D5 table.
  - `gif.rs`: extract `pub(crate) fn optimise_args(o: &OptimiseOptions) -> Vec<String>`, holding the `-O…`, optional `--lossy=…` and `--colors 256` tail that `edit` appends today, and use it in `edit`.
  - `edit/mod.rs`:
    - Add the fields under a `// stream F: pipelines` comment at the end of `Edits`, `EditsPatch` and `merge`.
    - `from_recipe` takes care with `aspect`, `size`, `scale`, `format`, `compression` and `fit_under`, all `Some(e.field)`.
    - In `apply`, set `base.strip_metadata = edits.strip_metadata.unwrap_or(o.strip_metadata)` and `base.encoder = edits.encoder.or(o.encoder)`.
  - `pipeline.rs`:
    - Insert after line 3: `//! The user's pipelines (pipeline_def, pipeline_plan) compile into the `Edits` recipe that `Job::run` applies here; this module is the job runner, not the pipeline engine.`
    - In `run_recipe`'s `OptimiseOptions { … }` literal (line 545): `strip_metadata: self.edits.strip_metadata.unwrap_or(o.strip_metadata)`, `encoder: self.edits.encoder`, `adaptive: false`. Task 3 changes `adaptive` to `o.adaptive`.
  - `src/lib/ipc.ts`: add `encoder`, `strip_metadata` and `adaptive` to `interface Edits` (with a `// stream F: pipelines` comment).
  - `src/lib/card.fixture.ts`: add `encoder: null, strip_metadata: null, adaptive: false`.

- [ ] **Step 4: Run the tests and confirm that they pass.** Run the four filters above, then `cargo test -p ferry-core` (everything) and `pnpm check && pnpm test`. Expected: all pass.

- [ ] **Step 5: Commit.** Run `git add crates/ferry-core/src src/lib/ipc.ts src/lib/card.fixture.ts`, then `git commit -m "feat(core): encoder choice and strip-metadata override in the recipe"`.

---

### Task 2: The perceptual metric (`metric.rs`)

**Files:**
- Modify: `crates/ferry-core/Cargo.toml` (`ssimulacra2 = "=0.5.1"`), `crates/ferry-core/src/lib.rs` (`pub mod metric;`), `crates/ferry-core/src/testutil.rs` (photo fixtures).
- Create: `crates/ferry-core/src/metric.rs`.

**Interfaces:**
- Consumes: `edit::raster::Raster { width, height, rgba, icc }` and `edit::geometry::Rect`.
- Produces:
  - `pub fn score(reference: &Raster, candidate: &Raster) -> Result<f64, MetricError>`: the minimum SSIMULACRA2 score over `regions` (D4), with alpha composited over `#808080`.
  - `pub fn regions(width: u32, height: u32) -> Vec<Rect>`.
  - `#[derive(Debug)] pub enum MetricError { TooSmall, SizeMismatch, Failed(String) }`.
  - testutil: `pub fn photo_rgba(w: u32, h: u32) -> Vec<u8>` (deterministic: R = x·255/w, G = y·255/h, B = (x+y)·255/(w+h), each plus noise in −24..=24 from a fixed-seed LCG, clamped; alpha 255), plus `pub fn photo_png(w, h) -> Vec<u8>` and `pub fn photo_jpeg(w, h, quality) -> Vec<u8>`, encoded from it the way `png()` and `jpeg()` encode `screenshot_rgba`.

- [ ] **Step 1: Write the failing tests** (in `metric.rs`)

```rust
fn raster(rgba: Vec<u8>, w: u32, h: u32) -> Raster { Raster { width: w, height: h, rgba, icc: None } }
fn jpeg_round_trip(r: &Raster, q: u8) -> Raster {
    let bytes = crate::edit::raster::encode(r, Format::Jpeg, &OptimiseOptions { compression: q, ..Default::default() }).unwrap();
    crate::edit::raster::decode(&bytes, Format::Jpeg).unwrap()
}

#[test]
fn an_image_scores_about_100_against_itself() {
    let r = raster(testutil::photo_rgba(128, 96), 128, 96);
    assert!(score(&r, &r).unwrap() > 99.0);
}
#[test]
fn a_heavier_jpeg_scores_lower() {
    let r = raster(testutil::photo_rgba(256, 192), 256, 192);
    let (q30, q90) = (score(&r, &jpeg_round_trip(&r, 30)).unwrap(), score(&r, &jpeg_round_trip(&r, 90)).unwrap());
    assert!(q90 > q30 && q90 > 70.0, "q30 {q30}, q90 {q90}");
}
#[test]
fn small_images_are_scored_whole_and_large_ones_in_four_tiles() {
    assert_eq!(regions(800, 600), [Rect { x: 0, y: 0, width: 800, height: 600 }]);
    let tiles = regions(2000, 1500);
    assert_eq!(tiles.len(), 4);
    assert!(tiles.iter().all(|t| t.width == 512 && t.height == 512 && t.x + 512 <= 2000 && t.y + 512 <= 1500));
    assert_eq!(tiles[0], Rect { x: 244, y: 119, width: 512, height: 512 });
    assert!(regions(400, 3000).iter().all(|t| t.width == 400 && t.x == 0));
}
#[test]
fn colour_under_full_transparency_does_not_count() {
    let mut a = testutil::photo_rgba(64, 64);
    for px in a.chunks_mut(4).step_by(3) { px[3] = 0; }
    let mut b = a.clone();
    for px in b.chunks_mut(4).filter(|p| p[3] == 0) { px[0] = 255 - px[0]; }
    assert!(score(&raster(a, 64, 64), &raster(b, 64, 64)).unwrap() > 99.0);
}
#[test]
fn tiny_or_mismatched_images_are_errors() {
    let tiny = raster(vec![0; 4 * 4 * 4], 4, 4);
    assert!(matches!(score(&tiny, &tiny), Err(MetricError::TooSmall)));
    let a = raster(testutil::photo_rgba(16, 16), 16, 16);
    let b = raster(testutil::photo_rgba(16, 17), 16, 17);
    assert!(matches!(score(&a, &b), Err(MetricError::SizeMismatch)));
}
```

- [ ] **Step 2: Run `cargo test -p ferry-core metric` and confirm that it fails.** Expected: unresolved module `metric`.

- [ ] **Step 3: Implement `score` and `regions`.**
  - Convert each region into `ssimulacra2::Rgb::new(Vec<[f32; 3]>, w, h, TransferCharacteristic::SRGB, ColorPrimaries::BT709)`, with values in 0.0..=1.0 after compositing `c·a/255 + 128·(1 − a/255)`.
  - Call `ssimulacra2::compute_frame_ssimulacra2(reference_rgb, candidate_rgb)`.
  - Check sizes yourself before converting: a size mismatch gives `SizeMismatch`, and any side under 8 gives `TooSmall`.
  - Map every crate error to `Failed(e.to_string())`.
  - Tile centres and clamping follow D4. Use integer `x = clamp(cx − tw/2, 0, w − tw)`.

- [ ] **Step 4: Run `cargo test -p ferry-core metric` and confirm that it passes.** Then check licences with `cargo tree -p ferry-core -e normal --prefix none | sort -u` and compare against "New dependencies".

- [ ] **Step 5: Commit.** Run `git add crates/ferry-core Cargo.lock`, then `git commit -m "feat(core): SSIMULACRA2 score over sample regions"`.

---

### Task 3: Adaptive compression

**Files:**
- Create: `crates/ferry-core/src/optimise/adaptive.rs`.
- Modify:
  - `optimise/mod.rs`: `pub(crate) mod adaptive;`; `optimise()` uses it.
  - `edit/mod.rs`: `apply` uses it; merge exclusivity for `adaptive`.
  - `pipeline.rs`: `RunOptions.adaptive`; `run_recipe` passes `adaptive: o.adaptive`.

**Interfaces:**
- Consumes: `metric::score` (Task 2) and `OptimiseOptions.adaptive` / `encoder` (Task 1).
- Produces:
  - `pub const CANDIDATES: [u8; 14] = [30, 35, …, 95]` and `pub const MAX_ENCODES: usize = 5`.
  - `pub fn target(o: &OptimiseOptions) -> f64`: 70.0 when `o.level == Aggressive`, else 80.0.
  - `pub fn applies(format: Format, o: &OptimiseOptions) -> bool`: `o.adaptive && format != Gif && o.encoder != Some(VisuallyLossless)`.
  - `pub struct Adapted { pub bytes: Vec<u8>, pub quality: u8, pub score: Option<f64>, pub reached: bool }`.
  - `pub fn search(reference: &Raster, format: Format, o: &OptimiseOptions, encode: impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError>) -> Result<Adapted, OptimiseError>`: scores with `metric::score(reference, &raster::decode(candidate, format)?)`.
  - `pub fn search_with(o: &OptimiseOptions, encode: impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError>, score: impl FnMut(&[u8]) -> Result<f64, String>) -> Result<Adapted, OptimiseError>`: the algorithm, which `search` wraps.
  - `RunOptions { …, pub adaptive: bool }`, set from `settings.images.adaptive` in `RunOptions::new`.

Algorithm for `search_with`. Each candidate's options are `compression = q`, `level = Normal`, `adaptive = false`, and `encoder = o.encoder.or((o.level == Aggressive).then_some(Small))`.

```
lo = 0, hi = 13, best = None, encodes = 0
while lo <= hi and encodes < MAX_ENCODES:
    check cancel → Cancelled
    mid = (lo + hi) / 2; bytes = encode(opts(CANDIDATES[mid])); encodes += 1
    check cancel → Cancelled
    s = score(&bytes)
        Err → return fallback (one encode at quality(o), with the original level and encoder, score None, reached false)
    if s >= target: best = (mid, bytes, s); hi = mid - 1  (stop when mid == 0)
    else: lo = mid + 1; remember (mid, bytes, s) as `last_miss` if mid == 13
if best: return it with reached = true
else: return the bytes for CANDIDATES[13] (re-use `last_miss` when it is index 13; otherwise encode 95 once more), reached = false
```

- [ ] **Step 1: Write the failing tests**

```rust
// optimise/adaptive.rs tests
fn fake(calls: &mut Vec<u8>) -> impl FnMut(&OptimiseOptions) -> Result<Vec<u8>, OptimiseError> + '_ {
    move |o| { calls.push(o.compression); Ok(vec![o.compression; usize::from(o.compression)]) }
}
fn on() -> OptimiseOptions { OptimiseOptions { adaptive: true, ..OptimiseOptions::default() } }

#[test]
fn it_picks_the_lowest_quality_that_reaches_the_target() {
    let mut calls = Vec::new();
    let a = search_with(&on(), fake(&mut calls), |b| Ok(f64::from(b[0]))).unwrap();
    assert_eq!((a.quality, a.reached), (80, true));
    assert!(calls.len() <= MAX_ENCODES, "{calls:?}");
    assert!(calls.iter().all(|q| CANDIDATES.contains(q)));
}
#[test]
fn aggressive_aims_at_70() {
    let o = OptimiseOptions { level: Level::Aggressive, ..on() };
    let a = search_with(&o, fake(&mut Vec::new()), |b| Ok(f64::from(b[0]))).unwrap();
    assert_eq!(a.quality, 70);
}
#[test]
fn when_nothing_reaches_it_the_top_quality_is_kept() {
    let a = search_with(&on(), fake(&mut Vec::new()), |_| Ok(10.0)).unwrap();
    assert_eq!((a.quality, a.reached, a.bytes.len()), (95, false, 95));
}
#[test]
fn when_the_floor_already_reaches_it_the_floor_is_kept() {
    let a = search_with(&on(), fake(&mut Vec::new()), |_| Ok(100.0)).unwrap();
    assert_eq!(a.quality, 30);
}
#[test]
fn a_metric_error_falls_back_to_the_settings_quality() {
    let o = OptimiseOptions { compression: 72, ..on() };
    let a = search_with(&o, fake(&mut Vec::new()), |_| Err("no".into())).unwrap();
    assert_eq!((a.quality, a.score, a.reached), (72, None, false));
}
#[test]
fn a_cancel_stops_the_search() {
    let o = on();
    o.cancel.cancel();
    assert!(matches!(search_with(&o, fake(&mut Vec::new()), |_| Ok(90.0)), Err(OptimiseError::Cancelled)));
}

// optimise/mod.rs tests
#[test]
fn adaptive_jpeg_is_smaller_than_q95_and_still_scores_80() {
    let input = testutil::photo_jpeg(384, 256, 98);
    let o = OptimiseOptions { adaptive: true, ..OptimiseOptions::default() };
    let Outcome::Smaller { bytes } = optimise(&input, Format::Jpeg, &o).unwrap() else { panic!("should shrink") };
    let q95 = encode(&input, Format::Jpeg, &OptimiseOptions { compression: 95, ..OptimiseOptions::default() }).unwrap();
    assert!(bytes.len() < q95.len());
    let (reference, result) = (crate::edit::raster::decode(&input, Format::Jpeg).unwrap(), crate::edit::raster::decode(&bytes, Format::Jpeg).unwrap());
    assert!(crate::metric::score(&reference, &result).unwrap() >= 80.0);
}
#[test]
fn a_tiny_image_falls_back_instead_of_failing() {
    let tiny = testutil::png(4, 4);
    let o = OptimiseOptions { adaptive: true, ..OptimiseOptions::default() };
    assert!(optimise(&tiny, Format::Png, &o).is_ok());
}
#[test]
fn gif_ignores_adaptive() {
    let Some(gifsicle) = testutil::gifsicle() else { return };
    let gif = testutil::gif(2);
    let plain = OptimiseOptions { gifsicle: Some(gifsicle), ..OptimiseOptions::default() };
    let adaptive = OptimiseOptions { adaptive: true, ..plain.clone() };
    assert_eq!(optimise(&gif, Format::Gif, &plain).unwrap(), optimise(&gif, Format::Gif, &adaptive).unwrap());
}

// edit/mod.rs tests
#[test]
fn adaptive_and_an_explicit_quality_replace_each_other() {
    let mut e = Edits::default();
    e.merge(patch(json!({ "compression": 60 })));
    e.merge(patch(json!({ "adaptive": true })));
    assert_eq!((e.adaptive, e.compression, e.fit_under), (true, None, None));
    e.merge(patch(json!({ "fit_under": 1000 })));
    assert!(!e.adaptive);
    assert!(!Edits { adaptive: true, ..Edits::default() }.is_plain());
}
#[test]
fn an_explicit_card_quality_beats_the_adaptive_setting() {
    let input = testutil::photo_jpeg(256, 192, 98);
    let edits = Edits { compression: Some(40), ..Edits::default() };
    let with = apply(&input, Format::Jpeg, &edits, &OptimiseOptions { adaptive: true, ..Default::default() }).unwrap();
    let without = apply(&input, Format::Jpeg, &edits, &OptimiseOptions::default()).unwrap();
    assert_eq!(with.bytes, without.bytes);
}

// pipeline.rs tests
#[test]
fn run_options_carry_the_adaptive_setting() {
    let mut s = Settings::default();
    s.images.adaptive = true;
    assert!(options_with(&s).adaptive);
}
```

- [ ] **Step 2: Run each filter and confirm that it fails:** `cargo test -p ferry-core adaptive`, `cargo test -p ferry-core gif_ignores_adaptive`, `cargo test -p ferry-core an_explicit_card_quality`.

- [ ] **Step 3: Implement**
  - `adaptive.rs`: the algorithm above. Log `debug!` is not available in core, so put nothing about the chosen quality in core. Instead, `Adapted.quality` is returned for callers.
  - `optimise::optimise`: after the animated check, `if adaptive::applies(format, o)`. In that case, decode the reference with `edit::raster::decode(input, format)`; a decode error falls back to plain `encode`. Then `search(&reference, format, o, |opts| encode(input, format, opts))` and `classify(input.len(), adapted.bytes)`. `encode()` itself stays non-adaptive, because fit-under-size and adaptive both call it per candidate.
  - `edit::apply`:
    - Compute `base.adaptive = edits.adaptive || (o.adaptive && edits.compression.is_none() && edits.fit_under.is_none())`.
    - In the `None` arm of `match edits.fit_under`, when `adaptive::applies(target, &base)` and `work` is not `Work::Gifsicle`, take the reference from the work. For `Work::Pixels` it is the raster. For `Work::Encoder` it is `raster::decode(input, format)?`. Then run `search(&reference, target, &base, |opts| work.run(opts))`.
    - Otherwise call `work.run(&base)` as today.
  - `pipeline.rs`: add `RunOptions.adaptive`; `run_recipe` options get `adaptive: o.adaptive`.

- [ ] **Step 4: Confirm that everything passes.** Run the filters above, then `cargo test -p ferry-core`.

- [ ] **Step 5: Commit.** Run `git add crates/ferry-core/src`, then `git commit -m "feat(core): adaptive compression by SSIMULACRA2 quality search"`.

---

### Task 4: PNG ⇄ JPEG switching

**Files:**
- Create: `crates/ferry-core/src/optimise/switch.rs`.
- Modify:
  - `optimise/mod.rs`: `pub mod switch;`.
  - `pipeline.rs`: `RunOptions.switch_png_jpeg`; the plain branch of `run_recipe`.

**Interfaces:**
- Produces:
  - `pub fn optimise(input: &[u8], format: Format, o: &OptimiseOptions, enabled: bool) -> Result<(Outcome, Format), OptimiseError>`.
  - `RunOptions { …, pub switch_png_jpeg: bool }` from `settings.images.switch_png_jpeg`.

- [ ] **Step 1: Write the failing tests**

```rust
// optimise/switch.rs tests
#[test]
fn an_opaque_photo_png_becomes_jpeg() {
    let (outcome, format) = optimise(&testutil::photo_png(256, 256), Format::Png, &OptimiseOptions::default(), true).unwrap();
    assert!(matches!(outcome, Outcome::Smaller { .. }));
    assert_eq!(format, Format::Jpeg);
}
#[test]
fn a_png_with_any_transparency_stays_png() {
    let mut rgba = testutil::photo_rgba(128, 128);
    rgba[3] = 254;
    let png = crate::edit::raster::encode(&Raster { width: 128, height: 128, rgba, icc: None }, Format::Png, &OptimiseOptions { compression: 100, ..Default::default() }).unwrap();
    assert_eq!(optimise(&png, Format::Png, &OptimiseOptions::default(), true).unwrap().1, Format::Png);
}
#[test]
fn a_flat_four_colour_jpeg_becomes_png() {
    let flat: Vec<u8> = (0..256 * 256).flat_map(|i| { let c = [[255, 0, 0], [0, 128, 255], [255, 255, 255], [20, 20, 20]][(i % 256 / 64) as usize]; [c[0], c[1], c[2], 255] }).collect();
    let jpeg = crate::edit::raster::encode(&Raster { width: 256, height: 256, rgba: flat, icc: None }, Format::Jpeg, &OptimiseOptions { compression: 95, ..Default::default() }).unwrap();
    assert_eq!(optimise(&jpeg, Format::Jpeg, &OptimiseOptions::default(), true).unwrap().1, Format::Png);
}
#[test]
fn off_never_changes_the_format_and_gif_webp_never_switch() {
    let png = testutil::photo_png(256, 256);
    assert_eq!(optimise(&png, Format::Png, &OptimiseOptions::default(), false).unwrap().1, Format::Png);
    let webp = testutil::webp(200, 200, 95, &Default::default());
    assert_eq!(optimise(&webp, Format::WebP, &OptimiseOptions::default(), true).unwrap().1, Format::WebP);
}

// pipeline.rs tests
#[test]
fn a_switched_file_is_written_as_a_conversion_and_restores() {
    let env = Env::new();
    let mut s = Settings::default();
    s.images.switch_png_jpeg = true;
    let path = env.file("photo.png", &testutil::photo_png(256, 256));
    let mut job = Job::file(&path, &s, true).unwrap();
    let o = options_with(&s);
    let (finished, _) = run(&mut job, Level::Normal, &o, &env);
    let update = finished.unwrap().update;
    assert!(update.converted && update.format == Some(Format::Jpeg));
    assert!(path.with_extension(Format::Jpeg.extension()).exists());
    assert!(!path.exists(), "the in-place source went to the (test) Trash");
    job.restore(&o, &mut |_, _, _| Ok(()));
    assert!(path.exists());
}
```

- [ ] **Step 2: Run `cargo test -p ferry-core switch` and confirm that it fails.**

- [ ] **Step 3: Implement**
  - `switch::optimise` follows D7.
  - The opaque check reads the decoded raster's alpha bytes.
  - The other format is `edit::raster::encode(&raster, other, o)`, or `adaptive::search(&raster, other, o, |opts| raster::encode(&raster, other, opts))` when `adaptive::applies(other, o)`.
  - `pipeline.rs` `run_recipe`, plain branch: replace `optimise::optimise(&original, self.source_format, &options)` with `optimise::switch::optimise(&original, self.source_format, &options, o.switch_png_jpeg)`, matching on `(outcome, format)`. In the `Smaller` arm, pass `format` instead of `self.source_format`. `put()` already treats a different format as a conversion.

- [ ] **Step 4: Run `cargo test -p ferry-core switch`, then `cargo test -p ferry-core`, and confirm that both pass.**

- [ ] **Step 5: Commit.** `git commit -m "feat(core): switch PNG and JPEG when the other is 10% smaller"`.

---

### Task 5: The StepKind registry (`pipeline_steps.rs`) and the common steps

**Files:**
- Create: `crates/ferry-core/src/pipeline_steps.rs`.
- Modify: `lib.rs`, and `en.json` (the `pipe.step.copy`, `pipe.step.fork`, `pipe.step.if_name_contains`, `pipe.param.copy_to`, `pipe.param.text`, `pipe.caption.copy`, `pipe.caption.fork`, `pipe.caption.if`, `pipe.error.required`, `pipe.error.template` and `pipe.error.copy_no_name` keys).

**Interfaces:**
- Produces the contract that the next tasks and streams A, B and E use:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Media { #[default] Image, Video, Audio, Pdf, #[serde(other)] Other }
impl Media {
    pub const ALL: [Media; 4] = [Media::Image, Media::Video, Media::Audio, Media::Pdf];
    /// By extension, case-insensitive. Image: png jpg jpeg gif webp heic heif avif tif tiff jxl;
    /// Video: mp4 mov mkv webm avi m4v; Audio: mp3 m4a aac wav flac aiff aif ogg opus; Pdf: pdf.
    pub fn guess(ext: &str) -> Option<Media>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Phase { Transform, Output, Control }

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Control {
    Number { min: f64, max: f64, step: f64 },
    Choice { options: &'static [(&'static str, &'static str)] }, // (value, i18n label key); serialised as [{value, label}]
    Switch, Text, Template, Folder,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct Param {
    pub name: &'static str,
    pub label: &'static str,                                   // i18n key
    pub control: Control,
    pub required: bool,
    /// Shown in the inspector only when `param` has one of `values`.
    pub shown_when: Option<(&'static str, &'static [&'static str])>, // serialised as {param, values} or null
}

/// What a file looks like to a guard and to captions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Facts { pub stem: String, pub ext: String, pub media: Media }

/// A step that runs after the branch's single encode.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum After { CopyTo { template: String, step: usize } }

/// One result being built: the recipe for `Job::run`, output-location overrides, after-steps.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Branch {
    pub recipe: crate::edit::Edits,
    pub optimised: Option<crate::settings::OutputRule>,
    pub converted: Option<crate::settings::OutputRule>,
    pub after: Vec<After>,
    /// 1-based number and caption of the last step that changed `recipe` (the card's step text).
    pub last_change: Option<(usize, String)>,
    /// 0-based index of the step being folded; set by `compile` before each fold.
    pub step: usize,
}

pub type Params = serde_json::Map<String, serde_json::Value>;

#[derive(Clone, Copy)]
pub enum Effect {
    Fold(fn(&Params, &mut Branch) -> Result<(), String>),
    Fork,
    Guard(fn(&Params, &Facts) -> Result<bool, String>),
}

#[derive(Clone, Copy)]
pub struct StepKind {
    pub id: &'static str,             // stable forever; stored in settings
    pub media: Media,
    pub label: &'static str,          // i18n key, e.g. "pipe.step.downscale"
    pub phase: Phase,
    pub params: &'static [Param],
    pub defaults: fn() -> Params,
    pub summary: fn(&Params) -> String, // caption: "Downscale to 50 %"
    pub effect: Effect,
}

#[derive(Debug, Clone, Serialize)]
pub struct StepKindView { pub id: String, pub media: Media, pub label: String, pub phase: Phase, pub params: Vec<Param>, pub defaults: Params }

#[derive(Default)]
pub struct Registry { /* Vec<StepKind>, registration order */ }
impl Registry {
    pub fn new() -> Self;
    /// `Err` names the (media, id) pair already taken.
    pub fn register(&mut self, kind: StepKind) -> Result<(), String>;
    pub fn get(&self, media: Media, id: &str) -> Option<&StepKind>;
    pub fn for_media(&self, media: Media) -> Vec<&StepKind>;
    /// Media with at least one Transform step, in `Media::ALL` order.
    pub fn media(&self) -> Vec<Media>;
    pub fn views(&self) -> Vec<StepKindView>;
}

/// copy, fork, if_name_contains for `media`.
pub fn register_common(r: &mut Registry, media: Media);
/// Every step kind this build knows.
pub fn builtin() -> Registry;
/// `builtin()`, built once.
pub fn registry() -> &'static Registry;
```

`builtin()` body. This is THE hook for streams A, B and E:

```rust
pub fn builtin() -> Registry {
    let mut r = Registry::new();
    for media in Media::ALL {
        register_common(&mut r, media);
    }
    crate::image_steps::register(&mut r);
    // Later streams register their steps here, one line each:
    // crate::video_steps::register(&mut r);   // stream A: Media::Video
    // crate::audio_steps::register(&mut r);   // stream B: Media::Audio
    // crate::pdf_steps::register(&mut r);     // stream E: Media::Pdf
    r
}
```

In Task 5, `crate::image_steps::register` does not exist yet. Create `image_steps.rs` with an empty `pub fn register(_r: &mut Registry) {}` and its `lib.rs` line here. Task 6 fills it.

Common steps (ids, params, effects):

| id | phase | params | effect |
| --- | --- | --- | --- |
| `copy` | Output | `to`: Template, required (e.g. `~/Pictures/Ferry/%f.%e`) | Fold: validate with `template::validate`; the text after the last `/` or `\` must be non-empty (`pipe.error.copy_no_name`); push `After::CopyTo { template, step: branch.step }`. Summary: `Copy to <the template's text before its last separator, as typed>`, for example `Copy to ~/Out` |
| `fork` | Control | none | `Effect::Fork` |
| `if_name_contains` | Control | `text`: Text, required | `Guard`: `Ok(facts.stem.to_lowercase().contains(&text.to_lowercase()))`; an empty or missing `text` is `Err(tf("pipe.error.required", name))` |

Error text helpers, used by every fold in this task and the next. Write them once in `pipeline_steps.rs` as `pub fn` so that later streams reuse them:
- `pub fn number(p: &Params, name: &str, label: &str, min: f64, max: f64) -> Result<Option<f64>, String>`: `None` when absent.
- `pub fn choice<'a>(p: &'a Params, name: &str, label: &str, options: &[(&str, &str)]) -> Result<Option<&'a str>, String>`.
- `pub fn text<'a>(p: &'a Params, name: &str) -> Option<&'a str>`.
- `pub fn flag(p: &Params, name: &str) -> bool`.

Messages: `pipe.error.range` `{name} must be between {min} and {max}`, `pipe.error.choice` `{name} can't be “{value}”`, `pipe.error.required` `{name} is required`. `{name}` is the translated label. `{min}` and `{max}` print whole numbers without a decimal point (`5`, not `5.0`).

- [ ] **Step 1: Write the failing tests** (in `pipeline_steps.rs`)

```rust
fn noop(_: &Params, _: &mut Branch) -> Result<(), String> { Ok(()) }
fn kind(id: &'static str, media: Media, phase: Phase) -> StepKind {
    StepKind { id, media, label: "x", phase, params: &[], defaults: Params::new, summary: |_| String::new(), effect: Effect::Fold(noop) }
}
fn params(v: serde_json::Value) -> Params { v.as_object().unwrap().clone() }

#[test]
fn a_second_kind_with_the_same_id_and_media_is_refused() {
    let mut r = Registry::new();
    r.register(kind("a", Media::Image, Phase::Transform)).unwrap();
    assert!(r.register(kind("a", Media::Image, Phase::Transform)).is_err());
    assert!(r.register(kind("a", Media::Video, Phase::Transform)).is_ok());
}
#[test]
fn only_media_with_a_transform_step_are_offered() {
    assert_eq!(builtin().media(), [Media::Image]);
    assert!(builtin().get(Media::Video, "copy").is_some());
}
#[test]
fn media_is_guessed_from_the_extension() {
    assert_eq!(Media::guess("PNG"), Some(Media::Image));
    assert_eq!(Media::guess("mov"), Some(Media::Video));
    assert_eq!(Media::guess("m4a"), Some(Media::Audio));
    assert_eq!(Media::guess("pdf"), Some(Media::Pdf));
    assert_eq!(Media::guess("zip"), None);
}
#[test]
fn an_unknown_media_name_reads_as_other() {
    assert_eq!(serde_json::from_str::<Media>("\"hologram\"").unwrap(), Media::Other);
}
#[test]
fn copy_adds_an_after_step_and_needs_a_file_name() {
    let copy = builtin().get(Media::Image, "copy").copied().unwrap();
    let Effect::Fold(fold) = copy.effect else { panic!() };
    let mut b = Branch { step: 3, ..Branch::default() };
    fold(&params(json!({ "to": "~/Out/%f_%z.%e" })), &mut b).unwrap();
    assert_eq!(b.after, [After::CopyTo { template: "~/Out/%f_%z.%e".into(), step: 3 }]);
    assert!(fold(&params(json!({ "to": "~/Out/" })), &mut Branch::default()).is_err());
    assert!(fold(&params(json!({ "to": "~/Out/%q.png" })), &mut Branch::default()).unwrap_err().contains("%q"));
}
#[test]
fn if_name_contains_ignores_case_and_needs_text() {
    let guard = builtin().get(Media::Image, "if_name_contains").copied().unwrap();
    let Effect::Guard(g) = guard.effect else { panic!() };
    let facts = Facts { stem: "ACME Invoice 42".into(), ext: "png".into(), media: Media::Image };
    assert!(g(&params(json!({ "text": "invoice" })), &facts).unwrap());
    assert!(!g(&params(json!({ "text": "receipt" })), &facts).unwrap());
    assert!(g(&params(json!({})), &facts).is_err());
}
#[test]
fn views_carry_labels_params_and_defaults() {
    let views = builtin().views();
    let copy = views.iter().find(|v| v.id == "copy" && v.media == Media::Image).unwrap();
    assert_eq!(copy.label, "pipe.step.copy");
    assert_eq!(serde_json::to_value(&copy.params[0]).unwrap()["control"]["kind"], "template");
}
```

- [ ] **Step 2: Run `cargo test -p ferry-core pipeline_steps` and confirm that it fails.**
- [ ] **Step 3: Implement the types and functions above** (`registry()` uses `std::sync::LazyLock`). Add the i18n keys listed under Files, with their English text from the i18n table at the end of this plan.
- [ ] **Step 4: Run `cargo test -p ferry-core pipeline_steps` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(core): pipeline step registry with copy, fork and name guard"`.

---

### Task 6: The image steps (`image_steps.rs`)

**Files:**
- Modify: `crates/ferry-core/src/image_steps.rs` (created empty in Task 5), and `en.json` (the `pipe.step.*`, `pipe.param.*`, `pipe.choice.*` and `pipe.caption.*` keys for these steps, plus `pipe.error.crop_empty`).

**Interfaces:**
- Consumes: the Task 5 types and helpers; `Edits::merge` and `EditsPatch` (to keep Cycle 2's exclusivity rules); `settings::{OutputRule, Location}`.
- Produces: `pub fn register(r: &mut Registry)`, which registers five `Media::Image` kinds in this order: `optimise`, `convert`, `downscale`, `crop`, `strip_exif`. Also `pub const CONVERT_TARGETS: &[(&str, &str)] = &[("png", "pipe.choice.png"), ("jpeg", "pipe.choice.jpeg"), ("webp", "pipe.choice.webp")]`.

| id | params (name: control, default) | fold |
| --- | --- | --- |
| `optimise` | `compression`: Number 5–100 step 5 (absent means settings); `adaptive`: Switch, false; `level`: Choice `normal`/`aggressive`, normal; `encoder`: Choice `auto`/`fast`/`small`/`visually_lossless`, auto; `location`: Choice `keep`/`in_place`/`same_folder`/`folder`/`temporary`, keep; `folder`: Folder, `shown_when` location = folder; `template`: Template, `shown_when` location ∈ {same_folder, folder} | The patch sets `level`, `encoder` (auto → None) and either `adaptive: Some(true)` or `compression: Some(Some(q))`, when given. `adaptive` wins over `compression`. When location ≠ keep: `branch.optimised = Some(OutputRule { location, folder: param or "~/Pictures/Ferry", template: param or "%f_optimised.%e", ..Default })`. A template that fails `template::validate` is an `Err` |
| `convert` | `to`: Choice from `CONVERT_TARGETS`, required, default `webp`; `location`, `folder`, `template` as above (keep, default template `%f.%e`) | `format: Some(Some(Format))`; the converted-rule override |
| `downscale` | `by`: Choice `percent`/`width`/`height`/`long_edge`, percent; `value`: Number 1–30000 step 1, default 50 | percent → `scale` (10–90, else `pipe.error.range` with min 10 and max 90); width / height / long_edge → `size` (1–30000) |
| `crop` | `aspect`: Choice `free`/`16:9`/`4:3`/`1:1`/`1.91:1`/`9:16`, default `16:9`; `width`, `height`: Number 0–30000, default 0; `long_edge`: Switch, false; `smart`: Switch, true | `aspect` (`1.91:1` → (191, 100), free → None); `framing` Smart or Centre; with `long_edge` and `width > 0` → `Size::LongEdge`; else when width or height > 0 → `Size::Exact { width, height }` (merge normalises a zero side). Free with no size is `pipe.error.crop_empty` |
| `strip_exif` | none | `strip_metadata: Some(Some(true))` |

Every Transform fold applies its patch with `branch.recipe.merge(patch)`.

Captions (`summary`), with exact English:

| Step and params | Caption |
| --- | --- |
| optimise, no params | `Optimise` |
| optimise, compression 60 | `Optimise · q 60` |
| optimise, adaptive | `Optimise · adaptive` |
| optimise, level aggressive | append ` · aggressive` |
| convert | `Convert to WebP` (the format label from `Format::label`) |
| downscale by percent | `Downscale to 50 %` |
| downscale by width | `Downscale to 1200 px wide` |
| downscale by height | `Downscale to 800 px tall` |
| downscale by long edge | `Downscale to 1920 px long edge` |
| crop with an aspect | `Crop 16:9`, plus ` · smart` when smart |
| crop with a size | `Crop to 1080×1080`, plus ` · smart` when smart |
| strip_exif | `Strip metadata` |

- [ ] **Step 1: Write the failing tests** (in `image_steps.rs`)

```rust
fn fold(id: &str, v: serde_json::Value) -> Result<Branch, String> {
    let mut r = Registry::new(); register(&mut r);
    let Effect::Fold(f) = r.get(Media::Image, id).unwrap().effect else { panic!() };
    let mut b = Branch::default();
    f(v.as_object().unwrap(), &mut b).map(|()| b)
}
fn caption(id: &str, v: serde_json::Value) -> String {
    let mut r = Registry::new(); register(&mut r);
    (r.get(Media::Image, id).unwrap().summary)(v.as_object().unwrap())
}

#[test]
fn optimise_sets_quality_level_encoder_and_location() {
    let b = fold("optimise", json!({ "compression": 60, "level": "aggressive", "encoder": "small", "location": "folder", "folder": "~/Out" })).unwrap();
    assert_eq!((b.recipe.compression, b.recipe.level, b.recipe.encoder), (Some(60), Level::Aggressive, Some(Encoder::Small)));
    let rule = b.optimised.unwrap();
    assert_eq!((rule.location, rule.folder.as_str(), rule.template.as_str()), (Location::Folder, "~/Out", "%f_optimised.%e"));
    let adaptive = fold("optimise", json!({ "adaptive": true, "compression": 60 })).unwrap();
    assert_eq!((adaptive.recipe.adaptive, adaptive.recipe.compression), (true, None));
    assert!(fold("optimise", json!({})).unwrap().optimised.is_none());
    assert!(fold("optimise", json!({ "compression": 3 })).unwrap_err().contains("between 5 and 100"));
}
#[test]
fn convert_targets_png_jpeg_or_webp() {
    assert_eq!(fold("convert", json!({ "to": "webp" })).unwrap().recipe.format, Some(Format::WebP));
    assert!(fold("convert", json!({ "to": "gif" })).is_err());
    let b = fold("convert", json!({ "to": "jpeg", "location": "same_folder" })).unwrap();
    assert_eq!(b.converted.unwrap().template, "%f.%e");
}
#[test]
fn downscale_by_percent_or_side() {
    assert_eq!(fold("downscale", json!({ "by": "percent", "value": 50 })).unwrap().recipe.scale, Some(50));
    assert_eq!(fold("downscale", json!({ "by": "width", "value": 1200 })).unwrap().recipe.size, Some(Size::Width { width: 1200 }));
    assert_eq!(fold("downscale", json!({ "by": "long_edge", "value": 1920 })).unwrap().recipe.size, Some(Size::LongEdge { edge: 1920 }));
    assert!(fold("downscale", json!({ "by": "percent", "value": 95 })).is_err());
}
#[test]
fn crop_by_ratio_size_or_long_edge() {
    let b = fold("crop", json!({ "aspect": "16:9", "smart": true })).unwrap();
    assert_eq!((b.recipe.aspect, b.recipe.framing), (Some((16, 9)), Framing::Smart));
    assert_eq!(fold("crop", json!({ "aspect": "1.91:1" })).unwrap().recipe.aspect, Some((191, 100)));
    assert_eq!(fold("crop", json!({ "aspect": "free", "width": 1080, "height": 1080 })).unwrap().recipe.size, Some(Size::Exact { width: 1080, height: 1080 }));
    assert_eq!(fold("crop", json!({ "aspect": "free", "width": 1920, "long_edge": true })).unwrap().recipe.size, Some(Size::LongEdge { edge: 1920 }));
    assert!(fold("crop", json!({ "aspect": "free" })).is_err());
}
#[test]
fn strip_exif_overrides_the_setting() {
    assert_eq!(fold("strip_exif", json!({})).unwrap().recipe.strip_metadata, Some(true));
}
#[test]
fn captions_read_like_the_card() {
    assert_eq!(caption("convert", json!({ "to": "webp" })), "Convert to WebP");
    assert_eq!(caption("downscale", json!({ "by": "percent", "value": 50 })), "Downscale to 50 %");
    assert_eq!(caption("crop", json!({ "aspect": "16:9", "smart": true })), "Crop 16:9 · smart");
    assert_eq!(caption("optimise", json!({ "compression": 60, "level": "aggressive" })), "Optimise · q 60 · aggressive");
}
```

- [ ] **Step 2: Run `cargo test -p ferry-core image_steps` and confirm that it fails.**
- [ ] **Step 3: Implement `register` and the five kinds** (`defaults` return the defaults in the table), and add the i18n keys.
- [ ] **Step 4: Run `cargo test -p ferry-core image_steps` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(core): image pipeline steps"`.

---

### Task 7: The stored model and the `pipelines` settings section (`pipeline_def.rs`)

**Files:**
- Create: `crates/ferry-core/src/pipeline_def.rs`.
- Modify: `settings.rs` (three anchors), `lib.rs`, `en.json` (`pipe.untitled`, `pipe.starter.webp`, `pipe.starter.web`, `pipe.starter.invoices`, `pipe.zones.untitled`).

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Step { pub kind: String, pub params: Params, #[serde(flatten)] pub extra: Map<String, Value> }

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pipeline { pub id: u32, pub name: String, pub media: Media, pub steps: Vec<Step>, pub hide_result: bool, #[serde(flatten)] pub extra: Map<String, Value> }

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PerMedia { pub image: Option<u32>, pub video: Option<u32>, pub audio: Option<u32>, pub pdf: Option<u32>, #[serde(flatten)] pub extra: Map<String, Value> }
impl PerMedia { pub fn get(&self, m: Media) -> Option<u32>; pub fn set(&mut self, m: Media, id: Option<u32>); } // Other: get None, set ignored

pub const ZONE_ICONS: [&str; 6] = ["image", "copy", "folder", "zap", "globe", "star"];

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Zone { pub id: u32, pub name: String, pub icon: String, pub pipelines: PerMedia, #[serde(flatten)] pub extra: Map<String, Value> }

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Pipelines {
    #[serde(deserialize_with = "lenient")] pub library: Vec<Pipeline>,
    pub clipboard: PerMedia,
    pub drop_zone: PerMedia,
    #[serde(deserialize_with = "lenient")] pub zones: Vec<Zone>,
    #[serde(flatten)] pub extra: Map<String, Value>,
}
impl Pipelines {
    pub fn get(&self, id: u32) -> Option<&Pipeline>;
    pub fn zone(&self, id: u32) -> Option<&Zone>;
    pub fn next_id(&self) -> u32;      // 1 + max library id (1 when empty)
    pub fn next_zone_id(&self) -> u32;
    /// Ids 0 or repeated get the next free id; blank names become t("pipe.untitled") / t("pipe.zones.untitled");
    /// unknown zone icons become "image"; every assignment (clipboard, drop_zone, zone.pipelines) to a missing id,
    /// or to a pipeline of another media, becomes None. Steps are never touched.
    pub fn sanitised(self) -> Self;
}
/// A JSON array whose elements that fail to parse are dropped; anything that is not an array is empty.
fn lenient<'de, D: Deserializer<'de>, T: DeserializeOwned>(d: D) -> Result<Vec<T>, D::Error>;
/// Starter templates for the empty library (id 0: the caller assigns one).
pub fn starters() -> Vec<Pipeline>;
```

Starters, in this order:
1. **Convert to WebP:** `[convert {to: "webp"}]`.
2. **Downscale for the web:** `[downscale {by: "long_edge", value: 1920}, optimise {}]`.
3. **Invoices to folder:** `[if_name_contains {text: "invoice"}, optimise {}, copy {to: "~/Documents/Invoices/%f.%e"}]`.

All three have media `image`, and their names come from the `pipe.starter.*` keys.

- [ ] **Step 1: Write the failing tests** (in `pipeline_def.rs`, plus one in `settings.rs`)

```rust
#[test]
fn the_defaults_are_empty() {
    let p = Pipelines::default();
    assert!(p.library.is_empty() && p.zones.is_empty());
    assert_eq!((p.clipboard.image, p.drop_zone.image), (None, None));
}
#[test]
fn a_pipeline_from_a_newer_ferry_survives_a_load_and_save() {
    let json = json!({ "library": [{ "id": 4, "name": "Future", "media": "hologram", "colour": "teal",
        "steps": [{ "kind": "teleport", "params": { "to": "mars" }, "note": "x" }] }] });
    let p: Pipelines = serde_json::from_value(json).unwrap();
    assert_eq!(p.library[0].media, Media::Other);
    let back = serde_json::to_value(&p).unwrap();
    assert_eq!(back["library"][0]["colour"], "teal");
    assert_eq!(back["library"][0]["steps"][0], json!({ "kind": "teleport", "params": { "to": "mars" }, "note": "x" }));
}
#[test]
fn a_broken_pipeline_is_dropped_not_the_whole_file() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, json!({ "version": 2, "images": { "compression": 55 },
        "pipelines": { "library": [{ "id": "not a number" }, { "id": 2, "name": "Ok", "steps": [] }] } }).to_string()).unwrap();
    let (s, status) = Settings::load(&path);
    assert_eq!(status, LoadStatus::Loaded);
    assert_eq!(s.images.compression, 55);
    assert_eq!(s.pipelines.library.iter().map(|p| p.id).collect::<Vec<_>>(), [2]);
}
#[test]
fn sanitise_gives_duplicate_ids_new_ones_and_clears_dangling_assignments() {
    let mut p = Pipelines::default();
    p.library = vec![Pipeline { id: 1, ..Default::default() }, Pipeline { id: 1, ..Default::default() }, Pipeline { id: 0, ..Default::default() }];
    p.clipboard.image = Some(9);
    p.zones = vec![Zone { id: 1, pipelines: PerMedia { image: Some(9), ..Default::default() }, icon: "rocket".into(), ..Default::default() }];
    let p = p.sanitised();
    assert_eq!(p.library.iter().map(|p| p.id).collect::<Vec<_>>(), [1, 2, 3]);
    assert_eq!((p.clipboard.image, p.zones[0].pipelines.image, p.zones[0].icon.as_str()), (None, None, "image"));
    assert_eq!(p.library[0].name, t("pipe.untitled"));
}
#[test]
fn an_assignment_to_a_pipeline_of_another_media_is_cleared() {
    let mut p = Pipelines::default();
    p.library = vec![Pipeline { id: 1, name: "v".into(), media: Media::Video, ..Default::default() }];
    p.clipboard.image = Some(1);
    p.clipboard.video = Some(1);
    let p = p.sanitised();
    assert_eq!((p.clipboard.image, p.clipboard.video), (None, Some(1)));
}

// settings.rs tests
#[test]
fn a_version_2_file_without_pipelines_loads_them_empty_and_round_trips_them() {
    let s: Settings = serde_json::from_value(json!({ "version": 2 })).unwrap();
    assert_eq!(s.pipelines, Pipelines::default());
    let mut s = Settings::default();
    s.pipelines.library.push(Pipeline { id: 1, name: "A".into(), ..Default::default() });
    let back: Settings = serde_json::from_value(serde_json::to_value(&s).unwrap()).unwrap();
    assert_eq!(back.pipelines, s.pipelines);
}
```

- [ ] **Step 2: Run each filter and confirm that it fails:** `cargo test -p ferry-core pipeline_def`, then `cargo test -p ferry-core without_pipelines`.
- [ ] **Step 3: Implement**, including the three `settings.rs` anchors from "Shared-file edits". Check that the existing test `unknown_fields_survive_a_load_and_save_at_every_level` still passes.
- [ ] **Step 4: Run `cargo test -p ferry-core` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(core): pipelines settings section with lenient loading"`.

---

### Task 8: Compile, check and captions (`pipeline_plan.rs`)

**Files:**
- Create: `crates/ferry-core/src/pipeline_plan.rs`.
- Modify: `lib.rs`, `en.json` (`pipe.error.unknown_step`, `pipe.error.after_output`, `pipe.error.fork_last`, `pipe.caption.unknown`).

**Interfaces:**
- Consumes: the Task 5 registry, the Task 7 `Pipeline`.
- Produces:

```rust
#[derive(Debug, Clone, PartialEq)]
pub struct Compiled { pub forks: Vec<Branch>, pub main: Branch, pub hide_result: bool, pub total: usize, pub stopped_at: Option<usize> }
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Issue { pub step: usize, pub message: String }   // 0-based step index
pub fn compile(p: &Pipeline, r: &Registry, facts: &Facts) -> Result<Compiled, Issue>;
/// Every issue, for the editor: guards count as passing, and compiling goes on after an issue.
pub fn check(p: &Pipeline, r: &Registry) -> Vec<Issue>;
/// Captions of each step (unregistered: t("pipe.caption.unknown") with {kind}).
pub fn captions(p: &Pipeline, r: &Registry) -> Vec<String>;
/// The first `max` captions joined with " · ", for zone tiles.
pub fn caption(p: &Pipeline, r: &Registry, max: usize) -> String;
```

The algorithm (D2). `compile` stops at the first issue. `check` records it and carries on:

```
branch = Branch::default(); forks = []; seen_output = false; stopped_at = None
for (i, step) in p.steps:
    kind = r.get(p.media, &step.kind) or issue(i, tf("pipe.error.unknown_step", kind = step.kind))
    branch.step = i
    match kind.effect:
      Fold(f):
        if kind.phase == Transform && seen_output: issue(i, t("pipe.error.after_output"))
        f(&step.params, &mut branch) or issue(i, message)
        if Transform: branch.last_change = Some((i + 1, (kind.summary)(&step.params)))
        if Output: seen_output = true
      Fork:
        if i == p.steps.len() - 1: issue(i, t("pipe.error.fork_last"))
        forks.push(branch.clone()); branch.after.clear(); seen_output = false
      Guard(g):
        match g(&step.params, facts): Err(m) → issue(i, m); Ok(false) → stopped_at = Some(i), break; Ok(true) → continue
Compiled { forks, main: branch, hide_result: p.hide_result, total: p.steps.len(), stopped_at }
```

- [ ] **Step 1: Write the failing tests**

```rust
fn pipe(steps: serde_json::Value) -> Pipeline {
    Pipeline { id: 1, name: "t".into(), media: Media::Image, steps: serde_json::from_value(steps).unwrap(), ..Default::default() }
}
fn facts(stem: &str) -> Facts { Facts { stem: stem.into(), ext: "png".into(), media: Media::Image } }

#[test]
fn transforms_fold_into_one_recipe_and_later_steps_win() {
    let c = compile(&pipe(json!([{ "kind": "downscale", "params": { "by": "percent", "value": 50 } },
        { "kind": "convert", "params": { "to": "webp" } }, { "kind": "downscale", "params": { "by": "width", "value": 800 } }])), &builtin(), &facts("a")).unwrap();
    assert_eq!((c.main.recipe.scale, c.main.recipe.size, c.main.recipe.format), (None, Some(Size::Width { width: 800 }), Some(Format::WebP)));
    assert_eq!(c.main.last_change, Some((3, "Downscale to 800 px wide".into())));
    assert!(c.forks.is_empty());
}
#[test]
fn a_change_after_copy_is_an_issue_but_after_fork_it_is_a_new_branch() {
    let bad = pipe(json!([{ "kind": "copy", "params": { "to": "~/a/%f.%e" } }, { "kind": "convert", "params": { "to": "png" } }]));
    assert_eq!(compile(&bad, &builtin(), &facts("a")).unwrap_err().step, 1);
    let good = pipe(json!([{ "kind": "convert", "params": { "to": "webp" } }, { "kind": "copy", "params": { "to": "~/a/%f.%e" } },
        { "kind": "fork" }, { "kind": "downscale", "params": { "by": "percent", "value": 50 } }]));
    let c = compile(&good, &builtin(), &facts("a")).unwrap();
    assert_eq!(c.forks.len(), 1);
    assert_eq!((c.forks[0].recipe.format, c.forks[0].recipe.scale, c.forks[0].after.len()), (Some(Format::WebP), None, 1));
    assert_eq!((c.main.recipe.format, c.main.recipe.scale, c.main.after.len()), (Some(Format::WebP), Some(50), 0));
}
#[test]
fn a_failing_guard_keeps_only_the_steps_before_it() {
    let p = pipe(json!([{ "kind": "strip_exif" }, { "kind": "if_name_contains", "params": { "text": "invoice" } }, { "kind": "convert", "params": { "to": "jpeg" } }]));
    let c = compile(&p, &builtin(), &facts("holiday")).unwrap();
    assert_eq!((c.stopped_at, c.main.recipe.format, c.main.recipe.strip_metadata), (Some(1), None, Some(true)));
    assert_eq!(compile(&p, &builtin(), &facts("Invoice 3")).unwrap().main.recipe.format, Some(Format::Jpeg));
}
#[test]
fn an_unregistered_step_stops_the_run_with_its_name() {
    let issue = compile(&pipe(json!([{ "kind": "teleport" }])), &builtin(), &facts("a")).unwrap_err();
    assert_eq!(issue.step, 0);
    assert!(issue.message.contains("teleport"));
}
#[test]
fn check_collects_every_issue_and_treats_guards_as_passing() {
    let p = pipe(json!([{ "kind": "teleport" }, { "kind": "if_name_contains", "params": { "text": "zzz" } },
        { "kind": "downscale", "params": { "by": "percent", "value": 99 } }, { "kind": "fork" }]));
    assert_eq!(check(&p, &builtin()).iter().map(|i| i.step).collect::<Vec<_>>(), [0, 2, 3]);
}
#[test]
fn every_starter_is_a_valid_image_pipeline() {
    for p in starters() { assert!(check(&p, &builtin()).is_empty(), "{}", p.name); }
}
#[test]
fn a_zone_caption_shows_the_first_steps() {
    let p = pipe(json!([{ "kind": "convert", "params": { "to": "webp" } }, { "kind": "copy", "params": { "to": "~/Out/%f.%e" } }, { "kind": "strip_exif" }]));
    assert_eq!(caption(&p, &builtin(), 2), "Convert to WebP · Copy to ~/Out");
}
```

- [ ] **Step 2: Run `cargo test -p ferry-core pipeline_plan` and confirm that it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test -p ferry-core pipeline_plan` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(core): compile a pipeline into recipe branches"`.

---

### Task 9: Text mode (`pipeline_text.rs`)

**Files:**
- Create: `crates/ferry-core/src/pipeline_text.rs`.
- Modify: `lib.rs`, `en.json` (`pipe.error.syntax`, `pipe.error.quote`, `pipe.error.unknown_param`).

**Syntax:**
- One step per line: `<step id>` followed by zero or more `<param>=<value>` pairs, separated by spaces.
- Blank lines and lines starting with `#` are ignored.
- Values:
  - a JSON number (`50`, `1.5`);
  - `true` or `false`;
  - a bare word made of non-space characters, excluding `"`, read as a string;
  - or a `"double-quoted string"` with `\"` and `\\` escapes.
- Numbers are stored as JSON numbers, booleans as JSON booleans, everything else as strings.

**Interfaces:**
- Produces:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LineError { pub line: usize, pub message: String }   // 1-based
/// Every line's error, or the steps. Unknown step ids (for `media`) and unknown params are errors;
/// param values are not validated here (pipeline_plan::check does that).
pub fn parse(media: Media, text: &str, r: &Registry) -> Result<Vec<Step>, Vec<LineError>>;
/// One line per step, params in the kind's `params` order, then any others in key order;
/// strings quoted only when they contain a space, `"`, `\`, `=` or `#`, or are empty, or read as a number or boolean;
/// non-scalar values printed as compact JSON in quotes.
pub fn print(steps: &[Step], media: Media, r: &Registry) -> String;
```

- [ ] **Step 1: Write the failing tests**

```rust
#[test]
fn text_round_trips_through_steps() {
    let text = "downscale by=long_edge value=1920\nconvert to=webp\ncopy to=\"~/My Pictures/%f.%e\"\nif_name_contains text=invoice\nfork\n";
    let steps = parse(Media::Image, text, &builtin()).unwrap();
    assert_eq!(steps[0].params["value"], json!(1920));
    assert_eq!(steps[2].params["to"], json!("~/My Pictures/%f.%e"));
    assert_eq!(print(&steps, Media::Image, &builtin()), text);
}
#[test]
fn comments_blank_lines_and_booleans() {
    let steps = parse(Media::Image, "# web\n\ncrop aspect=1:1 smart=false\n", &builtin()).unwrap();
    assert_eq!((steps.len(), &steps[0].params["smart"]), (1, &json!(false)));
}
#[test]
fn errors_name_their_line() {
    let errors = parse(Media::Image, "convert to=webp\nteleport\ncrop sharpness=3\ncopy to=\"~/a\n", &builtin()).unwrap_err();
    assert_eq!(errors.iter().map(|e| e.line).collect::<Vec<_>>(), [2, 3, 4]);
    assert!(errors[0].message.contains("teleport") && errors[1].message.contains("sharpness"));
}
#[test]
fn a_string_that_looks_like_a_number_is_quoted() {
    let steps = vec![Step { kind: "if_name_contains".into(), params: json!({ "text": "2026" }).as_object().unwrap().clone(), ..Default::default() }];
    assert_eq!(print(&steps, Media::Image, &builtin()), "if_name_contains text=\"2026\"\n");
}
```

- [ ] **Step 2: Run `cargo test -p ferry-core pipeline_text` and confirm that it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `cargo test -p ferry-core pipeline_text` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(core): pipeline text mode"`.

---

### Task 10: The copy step and the sandboxed test run

**Files:**
- Create: `crates/ferry-core/src/pipeline_after.rs`, `crates/ferry-core/src/pipeline_test.rs`.
- Modify: `lib.rs`, `en.json` (`pipe.test.skipped`, `pipe.test.would_copy`, `pipe.test.not_image`).

**Interfaces:**
- Consumes: `template::{Ctx, render, safe_file_name, validate}`, `settings::{expand_home, contract_home}`, `output::unique`, `fsutil::{atomic_replace, copy_xattrs}`, `edit::apply`, `pipeline_plan::compile`.
- Produces:

```rust
// pipeline_after.rs
/// Where a copy goes: `template` rendered for the result (stem, ext, dims, its folder's name, today);
/// the last path part is made safe with `safe_file_name`; `~` expands with `home`; a relative folder is
/// relative to the result's folder.
pub fn copy_target(template: &str, result: &Path, dims: (u32, u32), home: &Path, today: NaiveDate) -> PathBuf;
/// Copy `result` to `unique(copy_target(...))`, creating folders; atomic; xattrs best effort.
/// `Err` is the card text: tf("error.write", dir) with the folder shown with `~`.
pub fn copy_to(result: &Path, template: &str, dims: (u32, u32), home: &Path, today: NaiveDate) -> Result<PathBuf, String>;

// pipeline_test.rs
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StepReport { pub step: usize, pub caption: String, pub bytes: Option<u64>, pub dims: Option<(u32, u32)>, pub millis: u64, pub error: Option<String>, pub note: Option<String> }
/// Run `p` on a copy of `sample` inside `sandbox` (emptied first). The sample is only read.
/// Transform steps: the recipe so far applied to the original (size, dims, time). Copy: the latest
/// result written into `sandbox` under the copy's file name, with note "Would copy to ~/…".
/// Fork: note with the fork caption. A failing guard: that step gets note "Skipped: …" and the run ends.
/// An issue: that step gets `error` and the run ends. `Err` only when the sample cannot be read or is not an image.
pub fn run(p: &Pipeline, r: &Registry, sample: &Path, sandbox: &Path, o: &OptimiseOptions, home: &Path, today: NaiveDate) -> Result<Vec<StepReport>, String>;
```

- [ ] **Step 1: Write the failing tests**

```rust
// pipeline_after.rs tests
fn today() -> NaiveDate { NaiveDate::from_ymd_opt(2026, 10, 23).unwrap() }
#[test]
fn the_target_renders_tokens_and_expands_home() {
    let home = Path::new("/home/me");
    assert_eq!(copy_target("~/Pictures/twitter/%f_%z.%e", Path::new("/tmp/Desk/shot.png"), (1920, 1080), home, today()),
        PathBuf::from("/home/me/Pictures/twitter/shot_1920x1080.png"));
    assert_eq!(copy_target("out/%P-%y.%e", Path::new("/tmp/Desk/shot.png"), (1, 1), home, today()), PathBuf::from("/tmp/Desk/out/Desk-2026.png"));
    assert_eq!(copy_target("~/x/a:b.%e", Path::new("/t/s.png"), (1, 1), home, today()), PathBuf::from("/home/me/x/a_b.png"));
}
#[test]
fn a_copy_never_overwrites_and_lands_beside_with_a_number() {
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("shot.png"); fs::write(&result, b"new").unwrap();
    let out = dir.path().join("out"); fs::create_dir(&out).unwrap(); fs::write(out.join("shot.png"), b"old").unwrap();
    let tpl = format!("{}/%f.%e", out.display());
    let copied = copy_to(&result, &tpl, (1, 1), dir.path(), today()).unwrap();
    assert_eq!(copied, out.join("shot (2).png"));
    assert_eq!((fs::read(out.join("shot.png")).unwrap(), fs::read(&copied).unwrap()), (b"old".to_vec(), b"new".to_vec()));
}
#[cfg(unix)]
#[test]
fn an_unwritable_target_is_an_error_naming_the_folder() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let result = dir.path().join("shot.png"); fs::write(&result, b"x").unwrap();
    let locked = dir.path().join("locked"); fs::create_dir(&locked).unwrap();
    fs::set_permissions(&locked, fs::Permissions::from_mode(0o500)).unwrap();
    let err = copy_to(&result, &format!("{}/%f.%e", locked.display()), (1, 1), dir.path(), today()).unwrap_err();
    assert!(err.contains("locked"), "{err}");
}

// pipeline_test.rs tests
#[test]
fn a_test_run_reports_each_step_and_never_touches_the_sample_or_the_target() {
    let dir = tempfile::tempdir().unwrap();
    let sample = dir.path().join("invoice.png"); let original = testutil::photo_png(200, 100); fs::write(&sample, &original).unwrap();
    let target = dir.path().join("target");
    let p = Pipeline { media: Media::Image, steps: serde_json::from_value(json!([
        { "kind": "downscale", "params": { "by": "percent", "value": 50 } },
        { "kind": "copy", "params": { "to": format!("{}/%f.%e", target.display()) } },
        { "kind": "if_name_contains", "params": { "text": "receipt" } },
        { "kind": "convert", "params": { "to": "webp" } }])).unwrap(), ..Default::default() };
    let sandbox = dir.path().join("sandbox");
    let reports = run(&p, &builtin(), &sample, &sandbox, &OptimiseOptions::default(), dir.path(), NaiveDate::from_ymd_opt(2026, 10, 23).unwrap()).unwrap();
    assert_eq!(reports.len(), 3);
    assert_eq!(reports[0].dims, Some((100, 50)));
    assert!(reports[1].note.as_deref().unwrap().starts_with("Would copy to"));
    assert!(sandbox.join("invoice.png").exists() && !target.exists());
    assert!(reports[2].note.as_deref().unwrap().contains("receipt"));
    assert_eq!(fs::read(&sample).unwrap(), original);
}
```

- [ ] **Step 2: Run each filter and confirm that it fails:** `cargo test -p ferry-core pipeline_after`, then `cargo test -p ferry-core pipeline_test`.
- [ ] **Step 3: Implement.** For `copy_target`, split on the last `/` or `\` of the *rendered* text; a template with no separator is just a file name, placed in the result's folder.
- [ ] **Step 4: Run both filters, then `cargo test -p ferry-core`, and confirm that everything passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(core): pipeline copy step and sandboxed test run"`.

---

### Task 11: Running pipelines in the app (`pipe_jobs.rs`, hooks in `jobs.rs` and `clipboard.rs`)

**Files:**
- Create: `src-tauri/src/pipe_jobs.rs`.
- Modify:
  - `crates/ferry-core/src/pipeline.rs`: one new method, `Job::recipe_for`, inserted after `pub fn edit`, plus its test.
  - `src-tauri/src/lib.rs`: the `mod pipe_jobs;` line and `app.manage(pipe_jobs::PipeState::default())`.
  - `src-tauri/src/jobs.rs`: four insertions.
  - `src-tauri/src/clipboard.rs`: one line.
  - `en.json`: `pipe.card.step`, `pipe.note.copy_failed`, `pipe.error.cannot_run`, `pipe.error.missing`, `pipe.zone.no_pipeline`, `pipe.media.one.*`.

**Interfaces:**
- Consumes: `jobs::{start, edit, show, notice}`, `state::{AppState, relock}`, `pipeline_plan::compile`, `pipeline_steps::registry`, `pipeline_after::copy_to`, `EditsPatch::from_recipe`, `Job::fork`, `Job::edit`.
- Produces in core: `impl Job { pub fn recipe_for(&self, patch: EditsPatch) -> Edits }`, which returns the recipe `edit(patch)` would leave, using the same merge and the same "the source format or GIF clears `format`" rule, without changing the job. A pending plan is always registered with the recipe the job will really hold: `job.edits.clone()` after `job.edit(…)`, or `recipe_for(…)` before `jobs::edit`. Never register the raw compiled recipe.
- Produces in the app (Task 12, Task 13 and streams C, D, G rely on these names):

```rust
pub enum Trigger { Clipboard, DropZone, Zone(u32), Pipeline(u32) }

/// A `source_seq` that never matches the clipboard, so a fork twin's first run leaves the clipboard
/// to the main result (`ClipboardLog::may_replace` is false for a copy that never happened).
pub const NEVER_ON_CLIPBOARD: u64 = u64::MAX;

/// What a pipeline left for a job's next finished run.
pub struct Pending {
    pub recipe: Edits,           // the recipe this plan set (level ignored when comparing)
    pub after: Vec<After>,
    pub forks: Vec<Branch>,
    pub step_text: Option<String>, // "Step 2 of 4 · Convert to WebP"
    pub hide: bool,
    pub pipeline: u32,
}

#[derive(Default)]
pub struct PipeState { /* Mutex<Inner>: pending, overrides, hidden, produced_by, last_run */ }
impl PipeState {
    pub fn register(&self, id: &JobId, pending: Pending, optimised: Option<OutputRule>, converted: Option<OutputRule>);
    /// The pending plan for `id` if its recipe equals `edits` (level ignored); a stale one is dropped either way.
    pub fn take_for(&self, id: &JobId, edits: &Edits) -> Option<Pending>;
    pub fn drop_pending(&self, id: &JobId);
    pub fn step_text(&self, id: &JobId) -> Option<String>;
    pub fn hidden(&self, id: &JobId) -> bool;
    pub fn override_output(&self, id: &JobId, output: &mut settings::Output);
    pub fn producer(&self, id: &JobId) -> Option<u32>;
    pub fn ran(&self, pipeline: u32, at: NaiveDateTime);
    pub fn last_runs(&self) -> HashMap<u32, NaiveDateTime>;
    /// Forget every job not in `live`.
    pub fn prune(&self, live: &HashSet<JobId>);
}

pub fn media_of(job: &Job) -> Media;     // Media::Image for every format today; streams A/B/E map theirs here
pub fn step_text(branch: &Branch, total: usize) -> Option<String>; // from branch.last_change
pub fn start_assigned(app: &AppHandle, job: Job, trigger: Trigger, level: Level, source_seq: Option<u64>) -> bool;
pub fn run_on_card(app: &AppHandle, id: &JobId, pipeline: u32) -> Result<(), String>;

// hooks called from jobs.rs
pub fn hidden(app: &AppHandle, update: &JobUpdate) -> bool;           // hidden and not an Error
pub fn options(app: &AppHandle, id: &JobId, o: RunOptions) -> RunOptions;
pub fn label(app: &AppHandle, update: JobUpdate) -> JobUpdate;        // Optimising step text while a plan is pending
pub fn after_run(app: &AppHandle, entry: &Arc<Entry>, finished: Option<Finished>) -> Option<Finished>;
```

Behaviour:

- **`start_assigned`** looks up the pipeline id:
  - `Clipboard` → `settings.pipelines.clipboard.get(media)`;
  - `DropZone` → `drop_zone.get(media)`;
  - `Zone(z)` → `zone(z)?.pipelines.get(media)`;
  - `Pipeline(id)` → `Some(id)`.

  Then:
  - **No id:** `Zone` shows a notice (`pipe.zone.no_pipeline`) and returns false. Every other trigger calls `jobs::start(app, job, level, source_seq)`.
  - **Missing or wrong-media pipeline:** `Pipeline(_)` logs `warn!` and falls back to `jobs::start`. Every other trigger takes the same "no id" path.
  - **Found:**
    1. Compile with `Facts { stem: job.name, ext: job.ext, media }`. An issue shows a notice (`tf("pipe.error.cannot_run", name, why)`) and returns false.
    2. Call `job.edit(EditsPatch::from_recipe(&compiled.main.recipe))`. The level is `max(level, job.level)`, where Aggressive > Normal.
    3. Prune, then register the pending plan (`recipe: job.edits.clone()`, `after` from main, `forks`, `step_text(&main, total)`, `hide_result`) with `main.optimised` and `main.converted`.
    4. Record `produced_by` and call `jobs::start`.
- **`run_on_card`:**
  1. Find the entry. It must exist and its `last.can_edit` must be true, else `Err(card hint)`.
  2. Compile with the job's facts.
  3. Register the plan with `recipe: relock(&entry.job).recipe_for(patch.clone())`, where `patch = EditsPatch::from_recipe(&main.recipe)`.
  4. Call `jobs::edit(app, id, patch)`. If that returns `Err`, call `drop_pending` and return the error.
- **`after_run`:**
  - `None` (cancelled): `drop_pending` for the entry's job id, and return `None`.
  - `Some(f)`: `take_for(id, &relock(&entry.job).edits)`. If there is no plan, return `Some(f)` unchanged. If the state is `Error`, return it unchanged; the plan is dropped. Otherwise:
    1. For each `After::CopyTo`: `jobs::show` an Optimising update with step `tf("pipe.card.step", n = step + 1, total, step = caption)`, then `copy_to(&entry.output(), template, dims_after.or(dims_before), home, today)`. On `Err(e)`, set `f.update.note = Some(tf("pipe.note.copy_failed", why = e))` and `warn!`.
    2. For each fork branch: `twin = relock(&entry.job).fork()`, then `twin.edit(EditsPatch::from_recipe(&branch.recipe))`. Register its own pending plan (`recipe: twin.edits.clone()`, its `after`, no forks, same `hide` and pipeline) and overrides. Then `jobs::start(app, twin, level, (job source == Clipboard).then_some(NEVER_ON_CLIPBOARD))`.
    3. `ran(pipeline, now)`.
    4. Return `Some(f)`.
- **`hidden(app, update)`:** `PipeState::hidden(&update.id) && !matches!(update.state, JobState::Error { .. })`.

`jobs.rs` insertions. Each is one new line ending in `// stream F: pipelines`. Do not change any existing line:
1. In `show()`, right after the `if !…floating.enabled { return; }` block: `if crate::pipe_jobs::hidden(app, &update) { return; }`.
2. In `run()`, right after `let mut job = relock(&entry.job);`: `let options = crate::pipe_jobs::options(app, &job.id, options);`.
3. In `run()`'s emit closure, right after the `if cancel.is_cancelled() { return; }` block: `let update = crate::pipe_jobs::label(app, update);`.
4. In `run()`, right after `drop(job);`: `let finished = crate::pipe_jobs::after_run(app, entry, finished);`.

`clipboard.rs`, the last line of `start_clipboard_job`: replace `jobs::start(app, job, level, Some(seq));` with `crate::pipe_jobs::start_assigned(app, job, crate::pipe_jobs::Trigger::Clipboard, level, Some(seq)); // stream F: pipelines`.

- [ ] **Step 1: Write the failing tests** (unit tests in `pipe_jobs.rs`, on `PipeState` and `step_text` only)

```rust
fn id(s: &str) -> JobId { serde_json::from_value(serde_json::Value::String(s.into())).unwrap() }
fn pending(recipe: Edits) -> Pending { Pending { recipe, after: vec![], forks: vec![], step_text: Some("Step 1 of 1 · x".into()), hide: true, pipeline: 7 } }

#[test]
fn a_pending_plan_only_applies_to_the_recipe_it_set() {
    let s = PipeState::default();
    let webp = Edits { format: Some(Format::WebP), ..Edits::default() };
    s.register(&id("1-a"), pending(webp.clone()), None, None);
    assert!(s.take_for(&id("1-a"), &Edits::default()).is_none(), "a cancel put the old recipe back");
    assert!(s.take_for(&id("1-a"), &webp).is_none(), "and the stale plan is gone");
    s.register(&id("1-a"), pending(webp.clone()), None, None);
    let aggressive = Edits { level: Level::Aggressive, ..webp.clone() };
    assert!(s.take_for(&id("1-a"), &aggressive).is_some(), "the level is the run's, not the plan's");
}
#[test]
fn overrides_hidden_and_producer_last_for_the_jobs_life() {
    let s = PipeState::default();
    let rule = OutputRule { location: Location::Folder, folder: "~/X".into(), ..OutputRule::default() };
    s.register(&id("1-b"), pending(Edits::default()), Some(rule.clone()), None);
    let _ = s.take_for(&id("1-b"), &Edits::default());
    let mut out = settings::Output::default();
    s.override_output(&id("1-b"), &mut out);
    assert_eq!((out.optimised, s.hidden(&id("1-b")), s.producer(&id("1-b"))), (rule, true, Some(7)));
    s.prune(&HashSet::new());
    assert!(!s.hidden(&id("1-b")) && s.producer(&id("1-b")).is_none());
}
#[test]
fn the_step_text_names_the_last_change() {
    let b = Branch { last_change: Some((2, "Convert to WebP".into())), ..Branch::default() };
    assert_eq!(step_text(&b, 4).as_deref(), Some("Step 2 of 4 · Convert to WebP"));
    assert_eq!(step_text(&Branch::default(), 4), None);
}
// crates/ferry-core/src/pipeline.rs tests (stream F block)
#[test]
fn recipe_for_matches_what_edit_makes_and_changes_nothing() {
    let env = Env::new();
    let path = env.file("a.png", &testutil::png(64, 64));
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    let patch = EditsPatch { format: Some(Some(Format::Png)), scale: Some(Some(50)), ..EditsPatch::default() };
    let predicted = job.recipe_for(patch.clone());
    assert_eq!(job.edits, Edits::default());
    job.edit(patch);
    assert_eq!((job.edits.clone(), job.edits.format), (predicted, None));
}

#[test]
fn a_failed_copy_becomes_a_note() {
    assert_eq!(tf("pipe.note.copy_failed", &[("why", "Can't write to ~/X")]), "Couldn't copy: Can't write to ~/X");
}
```

- [ ] **Step 2: Run each filter and confirm that it fails:** `cargo test -p ferry-core recipe_for`, then `cargo test -p ferry --lib pipe_jobs`.
- [ ] **Step 3: Implement `pipe_jobs.rs` and the five hook lines.** `PipeState` stays free of `AppHandle`. `start_assigned` and `run_on_card` call `prune` with the ids in `AppState.jobs`, copied out under that lock and released, before they call `register`. Lock order: take the `PipeState` mutex only inside its methods, and never while holding `AppState.jobs` or `entry.job`.
- [ ] **Step 4: Verify.** Run `cargo test -p ferry --lib pipe_jobs`, then `cargo clippy --workspace --all-targets -- -D warnings`, then `cargo test -p ferry`. Expected: all pass.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): run pipelines through the job runner"`.

---

### Task 12: Commands, the editor window shell and the Settings "Pipelines" pane

**Files:**
- Create:
  - `src-tauri/src/pipe_commands.rs`, `src-tauri/src/pipe_windows.rs`;
  - `src/pipelines.html`, `src/pipelines/main.ts`, `src/pipelines/Editor.svelte`, `src/pipelines/Library.svelte`;
  - `src/lib/pipelines.ts`, `src/lib/pipelines.test.ts`;
  - `src/settings/controls/PipelineLibrary.svelte`.
- Modify:
  - `src-tauri/src/lib.rs`: the mod and generate_handler anchors.
  - `src-tauri/src/windows.rs`: the constants.
  - `src-tauri/capabilities/default.json`.
  - `vite.config.ts`: inputs `pipelines: page('pipelines.html'), zones: page('zones.html'),`. Add `zones` now, with a placeholder `src/zones.html` and `src/zones/main.ts` that mount nothing; Task 17 fills them.
  - `src/lib/settings.ts`: `pipelines: Pipelines`. `src/lib/settings.fixture.ts`: default `pipelines`.
  - `src/lib/schema.ts`: `PaneId` adds `'pipelines'`; `panes` adds `{ id: 'pipelines', label: 'pipe.pane' }` after floating; `Control` adds `{ kind: 'pipelines' }`; `all` adds the row `{ id: 'pipelines_library', pane: 'pipelines', label: 'pipe.library', help: 'pipe.library.help', path: 'pipelines', control: { kind: 'pipelines' } }`.
  - `src/settings/Row.svelte`: one `{:else if control.kind === 'pipelines'}` branch rendering `PipelineLibrary`.
  - `en.json`: the editor and library keys.

**Interfaces:**
- Produces these Tauri commands, all in `pipe_commands.rs`. Work that touches files runs on `spawn_blocking`, through a local copy of the `blocking` helper in `commands.rs`.

| Command | Signature | Notes |
| --- | --- | --- |
| `pipe_registry` | `() -> Vec<StepKindView>` | `registry().views()` |
| `pipe_meta` | `(app) -> PipeMeta { media: Vec<Media>, last_run: HashMap<u32, String> }` | `last_run` values are formatted as `%H:%M` today, or `%Y-%m-%d` earlier |
| `pipe_check` | `(pipeline: Pipeline) -> Checked { issues: Vec<Issue>, captions: Vec<String> }` | |
| `pipe_parse` | `(media: Media, text: String) -> Result<Vec<Step>, Vec<LineError>>` | |
| `pipe_print` | `(media: Media, steps: Vec<Step>) -> String` | |
| `pipe_starters` | `() -> Vec<Pipeline>` | |
| `pipe_test` | `async (app, pipeline: Pipeline, path: String) -> Result<Vec<StepReport>, String>` | Sandbox `<work_dir>/pipeline-test`; options from settings (compression, strip, gifsicle, adaptive) |
| `pipe_reveal_test` | `(app) -> Result<(), String>` | Reveal the sandbox |
| `pipe_pick_file` | `async (app) -> Result<Option<String>, String>` | Dialog with an image filter: png jpg jpeg gif webp |
| `pipe_run` | `async (app, id: JobId, pipeline: u32) -> Result<(), String>` | `pipe_jobs::run_on_card` (the G seam) |
| `pipe_open_editor` | `(app, pipeline: Option<u32>, tab: Option<String>)` | `pipe_windows::open_editor` |

`pipe_windows.rs`:
- `pub fn open_editor(app: &AppHandle, pipeline: Option<u32>, tab: Option<&str>)`. If the window exists, it is unminimised, shown, focused and sent `pipe:focus` `{ pipeline, tab }`. Otherwise it is built with `WebviewUrl::App("pipelines.html?pipeline=<n>&tab=<t>")` (omitting absent parameters), titled `t("pipe.window_title")`, 1000×680, min 860×560, centred and focused (same pattern as `windows::open_settings`).

`src/lib/pipelines.ts` (exported, each one pure):
- **Types** mirroring Rust: `Media`, `Step`, `Pipeline`, `PerMedia`, `Zone`, `Pipelines`, `StepKindView`, `ParamView`, `ParamControl`, `Issue`, `LineError`, `StepReport`, `PipeMeta`, `Checked`.
- `newPipeline(lib: Pipelines, media: Media, name: string, steps?: Step[]): { lib: Pipelines; id: number }`: id is max + 1.
- `duplicate(lib, id, name: string): { lib; id }`.
- `removePipeline(lib, id): Pipelines`: also clears `clipboard`, `drop_zone` and every `zone.pipelines` value equal to `id`.
- `type Usage = { kind: 'clipboard' | 'drop_zone'; media: Media } | { kind: 'zone'; media: Media; zone: string }`.
- `usage(lib, id): Usage[]`, in the order clipboard, drop_zone, then zones in list order.
- `usageLabel(u: Usage[]): string`: `"Copied images · Zone “WebP”"`, with `t('pipe.assign.none')` when empty.

`Editor.svelte` (this task covers the shell only):
- Load the settings through `../settings/store.svelte` (`load`, `change`, `store`), plus `pipe_registry` and `pipe_meta` on mount.
- Read the `pipeline` and `tab` query parameters, and listen for `pipe:focus`.
- Tabs **Pipelines | Preset zones** (`pipe.tab.*`). The left column is `Library.svelte`: a list of name, media badge, `usageLabel` and last run, with **New**, **Duplicate** and **Delete**. Delete asks for an inline confirmation: the button turns into "Delete “X”?" for 4 s.
- The centre shows the pipeline name field (saved on blur or Enter) and a placeholder for Task 15. The right column is a placeholder for Task 16.

`PipelineLibrary.svelte` (the Settings pane): the same list read-only, with **New** (which invokes `pipe_open_editor` with no pipeline; the editor then creates one), **Edit** and **Open editor**, plus Duplicate and Delete that call `onchange(nextPipelines)`.

- [ ] **Step 1: Write the failing tests** (`src/lib/pipelines.test.ts`)

```ts
import { describe, expect, it } from 'vitest';
import { duplicate, newPipeline, removePipeline, usage, type Pipelines } from './pipelines';

const none = { image: null, video: null, audio: null, pdf: null };
const empty = (): Pipelines => ({ library: [], clipboard: { ...none }, drop_zone: { ...none }, zones: [] });

describe('pipelines', () => {
  it('gives a new pipeline the next id', () => {
    const a = newPipeline(empty(), 'image', 'A');
    const b = newPipeline(a.lib, 'image', 'B');
    expect([a.id, b.id]).toEqual([1, 2]);
  });
  it('duplicates steps under a new id and name', () => {
    const a = newPipeline(empty(), 'image', 'A', [{ kind: 'convert', params: { to: 'webp' } }]);
    const d = duplicate(a.lib, a.id, 'A copy');
    expect(d.lib.library[1]).toMatchObject({ id: 2, name: 'A copy', steps: [{ kind: 'convert', params: { to: 'webp' } }] });
  });
  it('removing a pipeline clears every assignment', () => {
    const a = newPipeline(empty(), 'image', 'A');
    const lib: Pipelines = { ...a.lib, clipboard: { ...none, image: 1 }, drop_zone: { ...none, image: 1 },
      zones: [{ id: 1, name: 'Z', icon: 'image', pipelines: { ...none, image: 1 } }] };
    const after = removePipeline(lib, 1);
    expect([after.library.length, after.clipboard.image, after.drop_zone.image, after.zones[0].pipelines.image]).toEqual([0, null, null, null]);
  });
  it('lists where a pipeline is used', () => {
    const lib: Pipelines = { ...newPipeline(empty(), 'image', 'A').lib, clipboard: { ...none, image: 1 },
      zones: [{ id: 3, name: 'WebP', icon: 'image', pipelines: { ...none, image: 1 } }] };
    expect(usage(lib, 1)).toEqual([{ kind: 'clipboard', media: 'image' }, { kind: 'zone', media: 'image', zone: 'WebP' }]);
  });
});
```

- [ ] **Step 2: Run `pnpm test -- pipelines` and confirm that it fails.**
- [ ] **Step 3: Implement the commands, the window, the pages, the pane and the helpers**, at the anchors in "Shared-file edits".
- [ ] **Step 4: Verify.** Run `pnpm check && pnpm test`, `cargo clippy --workspace --all-targets -- -D warnings` and `pnpm build`. Expected: all pass, and `dist/pipelines.html` exists.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): pipeline editor window shell and Settings pane"`.

---

### Task 13: Card menu items: Re-optimise with encoder ▸, Strip metadata, Run pipeline ▸, Show producing pipeline

**Files:**
- Create: `src-tauri/src/pipe_menu.rs`.
- Modify: `src-tauri/src/menu.rs` (six marked insertions), `src-tauri/src/lib.rs` (`mod pipe_menu;`), `en.json` (`pipe.menu.*`, `pipe.encoder.*`).

**Interfaces:**
- Consumes: `menu::{item_id, Action}`, `pipe_jobs::{run_on_card, PipeState}`, `pipe_windows::open_editor`, `jobs::edit`.
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipeAction { Encoder(Encoder), Strip, Run(u32), Producer }
impl PipeAction {
    pub fn key(self) -> String;               // "encoder-fast" | "encoder-small" | "encoder-visually_lossless" | "strip" | "run-3" | "producer"
    pub fn parse(key: &str) -> Option<Self>;
}
pub fn enabled(card: &JobUpdate, action: PipeAction) -> bool; // Encoder/Strip/Run: card.can_edit; Producer: idle
pub struct Items { pub encoder: Submenu<Wry>, pub strip: CheckMenuItem<Wry>, pub run: Submenu<Wry>, pub producer: MenuItem<Wry> }
impl Items { pub fn build(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Items>; }
pub fn run(app: &AppHandle, id: &JobId, action: PipeAction) -> Result<(), String>;
```

`Items::build`:
- **`encoder`:** submenu `pipe.menu.encoder` with three `CheckMenuItem`s (Fast, Small, Visually lossless). Each is checked when `card.edits.encoder == Some(e)` and enabled when `can_edit`.
- **`strip`:** `CheckMenuItem` `pipe.menu.strip`, checked when `card.edits.strip_metadata.unwrap_or(settings.images.strip_metadata)`.
- **`run`:** submenu `pipe.menu.run` listing the library pipelines whose `media == Image`, in list order. When the library is empty, it holds one disabled item `pipe.menu.run_none`. The submenu is disabled when `!can_edit`.
- **`producer`:** `pipe.menu.producer`, enabled when `PipeState::producer(id)` is `Some`.

`run`:
- `Encoder(e)` → `jobs::edit` with `encoder: Some(if current == Some(e) { None } else { Some(e) })`.
- `Strip` → `strip_metadata: Some(Some(!effective))`.
- `Run(p)` → `pipe_jobs::run_on_card`.
- `Producer` → `pipe_windows::open_editor(app, producer, None)`.

`menu.rs` insertions. Each is a new line marked `// stream F: pipelines`:
1. In `enum Action`, after `Close,`: `Pipe(crate::pipe_menu::PipeAction),`.
2. In `Action::key`, before the `named =>` arm: `Action::Pipe(p) => format!("pipe-{}", p.key()),`.
3. In `Action::parse`, before `Some(_) => None,`: `Some(("pipe", value)) => crate::pipe_menu::PipeAction::parse(value).map(Action::Pipe),`.
4. In `enabled`, add the arm `Action::Pipe(p) => crate::pipe_menu::enabled(card, p),`.
5. In `build`, before `Menu::with_items(`: `let pipe = crate::pipe_menu::Items::build(app, card)?;`. In the slice:
   - after the `Action::Fit` item: `&pipe.encoder,`;
   - after the `keep_both` submenu: `&pipe.strip,`;
   - right before the separator that precedes `Action::Restore`: `&PredefinedMenuItem::separator(app)?, &pipe.run, &pipe.producer,`.
6. In `run`'s `match action` inside `spawn_blocking`, before `_ => Ok(())`: `Action::Pipe(p) => crate::pipe_menu::run(&app, &id, p),`.

- [ ] **Step 1: Write the failing tests** (in `pipe_menu.rs`)

```rust
fn done() -> JobUpdate {
    let mut card = ferry_core::pipeline::notice("x".into());
    card.id = serde_json::from_value(serde_json::json!("18f-1")).unwrap();
    card.state = JobState::Done; card.can_edit = true; card.output = Some("/tmp/a.png".into());
    card
}
#[test]
fn every_pipe_item_id_reads_back() {
    let card = done();
    for p in [PipeAction::Encoder(Encoder::Fast), PipeAction::Encoder(Encoder::VisuallyLossless), PipeAction::Strip, PipeAction::Run(12), PipeAction::Producer] {
        assert_eq!(crate::menu::parse_id(&crate::menu::item_id(&card.id, crate::menu::Action::Pipe(p))), Some((card.id.clone(), crate::menu::Action::Pipe(p))));
    }
    assert_eq!(PipeAction::parse("run-x"), None);
    assert_eq!(PipeAction::parse("encoder-huge"), None);
}
#[test]
fn tools_follow_can_edit_and_producer_follows_idle() {
    let mut card = done();
    card.can_edit = false;
    assert!(!enabled(&card, PipeAction::Strip) && !enabled(&card, PipeAction::Run(1)) && enabled(&card, PipeAction::Producer));
    card.state = JobState::Queued;
    assert!(!enabled(&card, PipeAction::Producer));
}
```

- [ ] **Step 2: Run `cargo test -p ferry --lib pipe_menu` and confirm that it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify.** Run `cargo test -p ferry --lib pipe_menu`, then `cargo test -p ferry --lib menu`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: pass, and the existing menu tests are unchanged.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): card menu pipeline, encoder and strip-metadata items"`.

---

### Task 14: The Adaptive detent on the card's compression slider, and the live Images settings

**Files:**
- Modify:
  - `src/lib/edits.ts` and `src/lib/edits.test.ts`;
  - `en.json`: `pipe.slider.adaptive`, plus the in-place values `images.adaptive` → `"Adaptive compression"`, `images.adaptive.help` → `"Picks the lowest quality that still looks like the original. PNG, JPEG and WebP; slower."`, `images.switch_png_jpeg.help` → `"Saves as JPEG or PNG when the other is at least 10% smaller. Only images without transparency."`.

**Interfaces:**
- Produces: `type Quality = number | 'extreme' | 'adaptive'`; `QUALITY_STOPS = ['adaptive', 100, 95, …, 5, 'extreme']`; `axisValue('adaptive') = 110`; `stopY` uses `top = Math.max(...stops.map(axisValue))` in place of the literal `100`.

- [ ] **Step 1: Write the failing tests** (append to `src/lib/edits.test.ts`)

```ts
describe('adaptive detent', () => {
  it('sits at the top of the compression slider', () => {
    expect(QUALITY_STOPS[0]).toBe('adaptive');
    expect(stopY(QUALITY_STOPS, 0, 220)).toBe(0);
    expect(stopY(QUALITY_STOPS, 1, 220)).toBe(20);
    expect(stopY(QUALITY_STOPS, QUALITY_STOPS.length - 1, 220)).toBe(220);
  });
  it('reads, labels and patches as adaptive', () => {
    expect(currentQuality({ ...card({ kind: 'done' }).edits, adaptive: true }, 80)).toBe('adaptive');
    expect(qualityLabel('adaptive')).toBe('Adaptive');
    expect(qualityPatch('adaptive')).toEqual({ adaptive: true, compression: null, fit_under: null, level: 'normal' });
    expect(qualityPatch(60)).toEqual({ compression: 60, level: 'normal', adaptive: false });
    expect(nextQuality('adaptive')).toBe(90);
  });
  it('leaves the downscale slider where it was', () => {
    expect(stopY(SCALE_STOPS, 1, 180)).toBe(20); // 180 × (100 − 90) / (100 − 10)
  });
});
```

Import `card` from `./card.fixture`. Update any existing test that asserted compression-slider positions to the new axis (top 110).

- [ ] **Step 2: Run `pnpm test -- edits` and confirm that it fails.**
- [ ] **Step 3: Implement.** `qualityPatch('extreme')` also sets `adaptive: false`. `labelledStops` needs no change, because index 0 is always labelled.
- [ ] **Step 4: Run `pnpm check && pnpm test` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(ui): adaptive detent on the compression slider"`.

---

### Task 15: The editor centre: step list, inspector, text mode and issues

**Files:**
- Create: `src/pipelines/StepList.svelte`, `src/pipelines/Inspector.svelte`, `src/pipelines/TextMode.svelte`.
- Modify: `src/pipelines/Editor.svelte`, `src/lib/pipelines.ts` and its test, `en.json` (`pipe.mode.*`, `pipe.add_step`, `pipe.move_up`, `pipe.move_down`, `pipe.remove_step`, `pipe.drag_handle`, `pipe.inspector.empty`, `pipe.text.*`, `pipe.issues`, `pipe.param.*`, `pipe.choice.*`).

**Interfaces:**
- Consumes: the commands `pipe_check`, `pipe_parse`, `pipe_print` and `pick_folder` (existing), and the Task 12 types.
- Produces (`pipelines.ts`):
  - `moveStep(steps: Step[], from: number, to: number): Step[]`.
  - `addStep(steps: Step[], kind: StepKindView, at?: number): Step[]`: params are a copy of `kind.defaults`, and the step goes at the end when `at` is absent.
  - `removeStep(steps, i): Step[]`.
  - `setParam(steps, i, name, value: unknown | undefined): Step[]`: `undefined` deletes the key.
  - `suggest(line: string, col: number, kinds: StepKindView[]): string[]`:
    - When the cursor is in the first token, the step ids that start with it.
    - After a step id, the param names of that kind not yet on the line, each followed by `=`, that start with the current token.
    - After `name=` for a `choice` param, its option values that start with what is typed.
    - Otherwise `[]`.

UI behaviour:

- **Step list:**
  - Each card shows its 1-based number, its caption (from `pipe_check`), and a red issue line when an issue has that step.
  - Reorder by pointer drag on the handle (`pointerdown`/`pointermove`/`pointerup`, not HTML5 drag-and-drop, which Tauri's file-drop handler intercepts on Windows). Move up and Move down buttons do the same from the keyboard.
  - Clicking a card selects it.
  - **Add step** is a `<select>` of `kinds` for the pipeline's media. An unregistered step shows its caption ("teleport (unavailable)") with a remove button only.
- **Inspector:**
  - One control per `ParamView`, hidden when `shown_when` does not match:
    - number → `<input type=number>` with min, max and step;
    - choice → `<select>` with `t(label)`;
    - switch → the existing `src/settings/controls/Switch.svelte`;
    - text and template → `<input>`;
    - folder → `<input>` plus a "Choose…" button calling `pick_folder`.
  - Every control has a `<label for>`.
  - Changes call `change('pipelines', …)` with `setParam`.
- **Text mode:**
  - A `<textarea>` filled with `pipe_print` when entering the mode.
  - 300 ms after typing stops, call `pipe_parse`. On `Ok`, update the steps through `change`. On `Err`, list each `Line n: message` under the textarea and mark it with `aria-invalid`.
  - A suggestion list from `suggest` sits under the caret line: Tab or a click inserts the first or the clicked item.
- **Issues:** call `pipe_check` 150 ms after any change. The header shows `pipe.issues` with `{n}`.

- [ ] **Step 1: Write the failing tests** (append to `pipelines.test.ts`)

```ts
const kinds: StepKindView[] = [
  { id: 'convert', media: 'image', label: 'pipe.step.convert', phase: 'transform', defaults: { to: 'webp' },
    params: [{ name: 'to', label: 'pipe.param.convert_to', required: true, shown_when: null,
      control: { kind: 'choice', options: [{ value: 'png', label: 'pipe.choice.png' }, { value: 'webp', label: 'pipe.choice.webp' }] } }] },
  { id: 'copy', media: 'image', label: 'pipe.step.copy', phase: 'output', defaults: {}, params: [{ name: 'to', label: 'pipe.param.copy_to', required: true, shown_when: null, control: { kind: 'template' } }] },
  { id: 'crop', media: 'image', label: 'pipe.step.crop', phase: 'transform', defaults: { aspect: '16:9', smart: true }, params: [] },
];
describe('steps', () => {
  it('moves, adds with defaults and removes', () => {
    const s = addStep(addStep([], kinds[0]), kinds[1], 0);
    expect(s.map((x) => x.kind)).toEqual(['copy', 'convert']);
    expect(s[1].params).toEqual({ to: 'webp' });
    expect(moveStep(s, 0, 1).map((x) => x.kind)).toEqual(['convert', 'copy']);
    expect(removeStep(s, 0).map((x) => x.kind)).toEqual(['convert']);
  });
  it('deletes a param set to undefined', () => {
    expect(setParam([{ kind: 'convert', params: { to: 'png' } }], 0, 'to', undefined)[0].params).toEqual({});
  });
});
describe('suggest', () => {
  it('offers step ids, then params, then choice values', () => {
    expect(suggest('c', 1, kinds)).toEqual(['convert', 'copy', 'crop']);
    expect(suggest('convert ', 8, kinds)).toEqual(['to=']);
    expect(suggest('convert to=w', 12, kinds)).toEqual(['webp']);
    expect(suggest('convert to=webp ', 16, kinds)).toEqual([]);
  });
});
```

- [ ] **Step 2: Run `pnpm test -- pipelines` and confirm that it fails.**
- [ ] **Step 3: Implement the helpers and the three components**, and wire them into `Editor.svelte`.
- [ ] **Step 4: Run `pnpm check && pnpm test` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(ui): pipeline step list, inspector and text mode"`.

---

### Task 16: The editor right panel: assignments, hide result, test well and starters

**Files:**
- Create: `src/pipelines/Assignments.svelte`, `src/pipelines/TestWell.svelte`.
- Modify: `src/pipelines/Editor.svelte`, `src/pipelines/Library.svelte` (empty state), `src/lib/pipelines.ts` and its test, `en.json` (`pipe.assign.*`, `pipe.hide_result*`, `pipe.test.*`, `pipe.empty.*`).

**Interfaces:**
- Produces: `assign(lib: Pipelines, slot: { kind: 'clipboard' | 'drop_zone' } | { kind: 'zone'; zone: number }, media: Media, id: number | null): Pipelines`.

UI behaviour:

- **Assignments:**
  - Under the heading "Runs on", the switches are:
    - "Copied images" (`pipe.assign.clipboard` with the media plural; slot `clipboard`);
    - "Drop zone" (slot `drop_zone`);
    - one switch per zone, "Zone “name”".
  - Each switch is on when that slot holds this pipeline's id. Turning it on assigns, which replaces any other pipeline in that slot. Turning it off assigns `null`.
  - A static line reads `pipe.assign.folders`.
  - The amber `pipe.assign.none` warning shows when `usage()` is empty.
- **Hide result:** a switch bound to `pipeline.hide_result`, with help `pipe.hide_result.help`.
- **Test well:**
  - A drop target for OS file drops. Listen with `getCurrentWebview().onDragDropEvent` and use the first path when `type === 'drop'` and the pointer is over the well. A "Choose file…" button calls `pipe_pick_file`.
  - It is disabled with `pipe.test.disabled` while `pipe_check` has issues.
  - While a test runs it shows `pipe.test.running`. It then lists one row per `StepReport`: number, caption, size (`formatBytes`), dims, `{ms} ms`, note, and the error in red.
  - The summary reads `pipe.test.passed`, or `pipe.test.failed` with `{n}` of the first error.
  - A "Show test folder" button calls `pipe_reveal_test`.
- **Empty library:** the title `pipe.empty.title`, the body text, a **New pipeline** button, and one button per starter from `pipe_starters`. A starter button adds the starter through `newPipeline(lib, starter.media, starter.name, starter.steps)` and selects it.

- [ ] **Step 1: Write the failing test** (append to `pipelines.test.ts`)

```ts
describe('assign', () => {
  it('puts one pipeline per slot and media, and clears with null', () => {
    const lib: Pipelines = { ...empty(), zones: [{ id: 4, name: 'Z', icon: 'image', pipelines: { ...none } }] };
    const a = assign(lib, { kind: 'clipboard' }, 'image', 2);
    expect(assign(a, { kind: 'clipboard' }, 'image', 3).clipboard.image).toBe(3);
    expect(assign(a, { kind: 'zone', zone: 4 }, 'image', 2).zones[0].pipelines.image).toBe(2);
    expect(assign(a, { kind: 'clipboard' }, 'image', null).clipboard.image).toBeNull();
  });
});
```

- [ ] **Step 2: Run `pnpm test -- pipelines` and confirm that it fails.**
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run `pnpm check && pnpm test` and confirm that it passes.**
- [ ] **Step 5: Commit.** `git commit -m "feat(ui): pipeline assignments, test run and starters"`.

---

### Task 17: Preset zones: overlay window, zone editor and the D seam

**Files:**
- Create: `src-tauri/src/preset_zones.rs`, `src/zones/Zones.svelte`, `src/zones/ZoneGrid.svelte`, `src/pipelines/ZonesTab.svelte`, `src/lib/zone-icons.ts`.
- Modify:
  - `src/zones.html`, `src/zones/main.ts` (the Task 12 placeholders). Use a transparent body: the same markup as `stack.html`, with `html, body { background: transparent }`.
  - `src-tauri/src/lib.rs`: `preset_zones::init(app);` at the setup anchor, plus the commands below at the handler anchor.
  - `src-tauri/src/pipe_commands.rs`: the zone commands.
  - `src/pipelines/Editor.svelte`: the Preset zones tab.
  - `src/lib/pipelines.ts` and its test.
  - `en.json`: `pipe.zones.*`, `pipe.zone.none`, `pipe.media.*`.

**Interfaces:**
- Produces (Rust, `preset_zones.rs`):

```rust
pub enum Mode { Drag, Preview }
pub const TILE: (f64, f64) = (148.0, 100.0); pub const GAP: f64 = 8.0; pub const PAD: f64 = 12.0; pub const COLUMNS: usize = 4;
/// Logical size for `tiles` tiles: columns = min(4, tiles), rows = ceil(tiles / columns).
pub fn zones_size(tiles: usize) -> (f64, f64);
/// Top-left that centres `size` on `cursor`, kept 16 px inside `work` (all logical).
pub fn zones_origin(cursor: (f64, f64), size: (f64, f64), work: (f64, f64, f64, f64)) -> (f64, f64);
pub fn init(app: &AppHandle);   // listen_any for "drag:tap" (toggle on "control") and "drag:ended"; on_menu_event for "pipezone:" items
#[derive(Deserialize)] pub struct Tap { pub modifier: String }
/// Whether a `drag:tap` toggles the zones: only the preset modifier.
pub fn is_preset_tap(tap: &Tap) -> bool;   // modifier == "control"
pub fn show(app: &AppHandle, mode: Mode, paths: Vec<PathBuf>);
pub fn hide(app: &AppHandle);
#[derive(Serialize)] pub struct ZoneTile { pub id: u32, pub name: String, pub icon: String, pub pipelines: PerMedia, pub captions: HashMap<Media, String> }
#[derive(Serialize)] pub struct ZonesView { pub tiles: Vec<ZoneTile>, pub preview: bool, pub dragged: Vec<Media> }
```

- **Commands:**

| Command | Signature | Notes |
| --- | --- | --- |
| `pipe_zones_show` | `(app)` | `Preview` mode |
| `pipe_zones_hide` | `(app)` | |
| `pipe_zones_drag` | `(app, entered: bool)` | The page reports drag `enter` (true) and `leave` (false), for the drag-mode hide timers |
| `pipe_zones_ready` | `(app) -> ZonesView` | |
| `pipe_classify` | `(paths: Vec<String>) -> Vec<Option<Media>>` | `Media::guess` on each extension |
| `pipe_zone_drop` | `async (app, zone: u32, paths: Vec<String>)` | Per path: a folder gets the notice `pipe.zones.folder_skipped`; a file goes through `Job::file`, where `Err` is a notice and `Ok` calls `pipe_jobs::start_assigned(…, Trigger::Zone(zone), Level::Normal, None)`. Hide after 300 ms |
| `pipe_zone_menu` | `(app, window: WebviewWindow, zone: u32, x: f64, y: f64)` | Native menu with ids `pipezone:<zone>:edit`, `pipezone:<zone>:pipeline` and `pipezone:<zone>:remove`. Edit opens the editor zones tab. Go to pipeline opens the editor at the zone's image pipeline (disabled when there is none). Remove calls `prefs::update` to drop the zone |

- **Window:** label `windows::ZONES`, `zones.html`. Build it at first `show`. It is transparent, undecorated, without shadow, not resizable, always on top, skips the taskbar, visible on all workspaces, `focused(false)`, `focusable(false)`, and calls `platform::make_non_activating`. Drag-drop stays enabled (the default).
- **On each show:**
  1. `set_content_protected(!settings.general.show_in_screenshots)`.
  2. Size it with `zones_size(zones.len() + 1 + preview as usize)`.
  3. Position it with `zones_origin`, using the monitor under the cursor, found the same way as in `windows::stack_resize` (copy the lookup, including the macOS `main_scale` correction). Convert the cursor and the work area to logical units with that monitor's scale.
  4. Emit `pipe:zones` (`ZonesView`) to the window, then `platform::show_without_focus`.
  5. In `Mode::Drag` only, arm the D8 hide timers: 5 s with no drag `enter`, and 1.5 s after a `leave`. The page reports `enter` and `leave` through a command `pipe_zones_drag(entered: bool)`; add it to the command table. A `drag:tap` "control" while showing hides immediately, and so does `drag:ended`.
- **Zones page (`Zones.svelte` and `ZoneGrid.svelte`):**
  - A grid of tiles, each with an icon from `zone-icons.ts` (inline Lucide paths), its name, its caption and a type badge. The last tile is "Edit presets…". In preview mode, "Done" comes after it.
  - `onDragDropEvent`:
    - `enter`: `pipe_classify(paths)` sets the dragged media.
    - `over`: hit-test with `document.elementFromPoint(x / devicePixelRatio, y / devicePixelRatio)`. The hovered tile gets an accent border, and the others dim to 0.5 opacity.
    - `drop`: on an enabled tile, `pipe_zone_drop`.
    - `leave`: clear the hover.
  - Right-click on a tile → `pipe_zone_menu`.
  - Each tile has an `aria-label` reading "name, caption".
- **Zones tab (`ZonesTab.svelte`):** a list of zones, each with:
  - a name field;
  - an icon `<select>` over `ZONE_ICONS`;
  - one `<select>` per offered media (`pipe_meta.media`) listing that media's pipelines plus "None";
  - Move up, Move down and Remove buttons.

  Below the list: **Add zone**, a live `ZoneGrid` preview (no drag handling), and **Preview on screen** (`pipe_zones_show`).
- **`pipelines.ts`:**
  - `newZone(lib, name): { lib; id }`, `removeZone(lib, id)`, `moveZone(lib, from, to)`.
  - `tileState(tile: { pipelines: PerMedia; captions: Partial<Record<Media, string>> }, dragged: Media[]): { enabled: boolean; caption: string }`:
    - Nothing dragged: enabled when any pipeline is set; the caption is the first set caption.
    - Otherwise: enabled when some dragged media has a pipeline; the caption is that media's caption.
    - Disabled: `t('pipe.zone.none', { media: t('pipe.media.one.<first dragged>') })`.

- [ ] **Step 1: Write the failing tests**

```rust
// preset_zones.rs tests
#[test]
fn the_grid_grows_to_four_columns_then_wraps() {
    assert_eq!(zones_size(1), (12.0 * 2.0 + 148.0, 12.0 * 2.0 + 100.0));
    assert_eq!(zones_size(4), (24.0 + 4.0 * 148.0 + 3.0 * 8.0, 124.0));
    assert_eq!(zones_size(5), (24.0 + 4.0 * 148.0 + 3.0 * 8.0, 24.0 + 2.0 * 100.0 + 8.0));
}
#[test]
fn only_a_control_tap_toggles_the_zones() {
    let tap = |m: &str| serde_json::from_value::<Tap>(serde_json::json!({ "modifier": m })).unwrap();
    assert!(is_preset_tap(&tap("control")));
    assert!(!is_preset_tap(&tap("alt")) && !is_preset_tap(&tap("command")) && !is_preset_tap(&tap("shift")));
}
#[test]
fn the_grid_centres_on_the_cursor_and_stays_on_screen() {
    let work = (0.0, 0.0, 1440.0, 900.0);
    assert_eq!(zones_origin((720.0, 450.0), (200.0, 100.0), work), (620.0, 400.0));
    assert_eq!(zones_origin((5.0, 890.0), (200.0, 100.0), work), (16.0, 784.0));
}
```

```ts
// pipelines.test.ts
describe('zones', () => {
  const tile = { pipelines: { ...none, image: 1 }, captions: { image: 'Convert to WebP' } };
  it('enables a tile for a dragged media it has a pipeline for', () => {
    expect(tileState(tile, ['image', 'video'])).toEqual({ enabled: true, caption: 'Convert to WebP' });
  });
  it('disables it otherwise and says why', () => {
    expect(tileState(tile, ['video'])).toEqual({ enabled: false, caption: 'No pipeline for video' });
  });
  it('adds, moves and removes zones with their own ids', () => {
    const a = newZone(empty(), 'A'); const b = newZone(a.lib, 'B');
    expect(moveZone(b.lib, 0, 1).zones.map((z) => z.name)).toEqual(['B', 'A']);
    expect(removeZone(b.lib, a.id).zones.map((z) => z.id)).toEqual([b.id]);
  });
});
```

- [ ] **Step 2: Run each filter and confirm that it fails:** `cargo test -p ferry --lib preset_zones`, then `pnpm test -- pipelines`.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Verify.** Run the filters above, then `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check && pnpm test` and `pnpm build`. Expected: all pass.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): preset zones overlay and zone editor"`.

---

### Task 18: Manual check on this Mac (manual check)

**Files:** none. Record each result, with what you saw, in the commit message of an empty commit.

- [ ] **Step 1:** Ask the user before running `pnpm tauri dev`: it takes over the real clipboard. Open a TextEdit window as the focus target. Never click into other apps.
- [ ] **Step 2: Settings → Images.** Check:
  - "Adaptive compression" and "Switch between PNG and JPEG" no longer say "Coming soon".
  - Turn Adaptive on and copy a large photo (a JPEG from Photos or a browser). Look at the card: the after size, and in `~/Library/Logs/io.ferry.desktop/ferry.log`, a run that finished without Error.
- [ ] **Step 3: Settings → Pipelines → New** opens the Pipelines window. Check:
  - Add Convert → WebP, then Copy with `~/Desktop/ferry-test/%f.%e`. The step list shows "Convert to WebP" and "Copy to ~/Desktop/ferry-test".
  - Drag the Copy step above Convert: step 2 turns red with "Steps that change the file must come before Copy, or after Fork", and the test well is disabled. Move it back.
  - Switch to Text: it reads `convert to=webp` and `copy to=~/Desktop/ferry-test/%f.%e`. Type `teleport`: "Line 3: “teleport” isn't available…" appears. Delete it.
- [ ] **Step 4: Test well.** Drop a PNG from Finder onto the well. Expect step 1 with a size and ms, and step 2 with "Would copy to ~/Desktop/ferry-test/….webp". `~/Desktop/ferry-test` must NOT exist. "Show test folder" reveals the sandbox, which holds the files.
- [ ] **Step 5: Run on a card.** Turn on "Copied images" for the pipeline, then copy a screenshot to the clipboard. Look at:
  - the card text while it runs ("Step 1 of 2 · Convert to WebP");
  - the final card (WebP, "converted");
  - `~/Desktop/ferry-test/` holding the `.webp`;
  - Paste into TextEdit: the pasted image is the WebP result.
- [ ] **Step 6: Card context menu.** Check:
  - **Re-optimise with encoder ▸ Small** gets a tick and re-runs.
  - **Strip metadata** is ticked by default and toggles.
  - **Run pipeline ▸** lists the pipeline.
  - **Show producing pipeline** focuses the editor on it.
  - The compression slider shows an "Adaptive" stop above 100, and choosing it re-runs the card.
- [ ] **Step 7: Preset zones.** In Preset zones, add a zone "WebP" for the pipeline and click **Preview on screen**. Check:
  - The grid appears centred near the pointer, over TextEdit, and TextEdit keeps focus (its caret still blinks).
  - Drag a PNG from Finder over the tile: the accent border appears.
  - Drop: a card appears and the grid hides.
  - Drag a `.mov`: the tile is dimmed with "No pipeline for video".
  - Right-click a tile: the Edit, Go to pipeline and Remove menu appears.
- [ ] **Step 8: Fork and hide.** Make the pipeline `convert to=webp`, `fork`, `downscale by=percent value=50`, and turn on Hide result. Copy an image. No card appears, and the clipboard holds the 50 % WebP. Turn Hide result off and repeat: two cards appear (WebP, and a 50 % WebP), and the clipboard holds the 50 % one.
- [ ] **Step 9: Cancel.** Start a pipeline on a large image and cancel it from the card. The copy folder gains no new file, no fork card appears, and a later downscale on that card copies nothing.
- [ ] **Step 10: Windows.** Not verified on a real desktop. Report "built in CI, unverified": the zones overlay drop on a `WS_EX_NOACTIVATE` window is the main risk.
- [ ] **Step 11: Commit the notes.** `git commit --allow-empty -m "test: stream F manual check on macOS" -m "<results per step>"`.

---

## Later step (not tasks): video, audio and PDF steps

After streams A, B and E merge, a follow-up stream F plan adds three files, each with the same hook signature, and three lines in `pipeline_steps::builtin()` at the anchor comment `// Later streams register their steps here, one line each:`:

| File | Hook (exact) | Registers |
| --- | --- | --- |
| `crates/ferry-core/src/video_steps.rs` | `pub fn register(r: &mut crate::pipeline_steps::Registry)` | `Media::Video`: `optimise` (compression, encoder Fast/Small/VisuallyLossless mapped to A's hardware, software and visually lossless encoders, location), `convert` (mp4/hevc/av1/webm/gif), `downscale`, `crop`, `change_speed` (factor, frames keep/drop), `remove_audio`, `to_gif` |
| `crates/ferry-core/src/audio_steps.rs` | the same | `Media::Audio`: `optimise` (bitrate), `convert` (mp3/m4a/opus/wav), `normalise` (target), `cover_art` (optimise/remove/keep/downscale) |
| `crates/ferry-core/src/pdf_steps.rs` | the same | `Media::Pdf`: `optimise` (dpi stops 300…48 or adaptive), `crop`, `extract_pages_as_images` |

Each fold sets the `Edits` fields that A, B or E added to the Cycle 2 recipe (for example, A's speed and audio flags). The follow-up also:

- extends `EditsPatch::from_recipe` with those fields;
- maps their formats in `pipe_jobs::media_of`;
- adds `Media::Video` and the other new media to the Assignments UI automatically, because it follows `pipe_meta.media`;
- adds E's still-image formats (AVIF, HEIC, TIFF) to `image_steps::CONVERT_TARGETS`;
- adds video starters ("Compress screen recording"; the copy-link part stays out with secure send).

The common steps `copy`, `fork` and `if_name_contains` are already registered for every media. Existing pipelines, settings and the text-mode syntax are unchanged.

---

## i18n keys (`pipe.*`, English)

| Key | Text |
| --- | --- |
| `pipe.window_title` | Pipelines |
| `pipe.pane` | Pipelines |
| `pipe.library` | Pipelines |
| `pipe.library.help` | Pipelines run several steps on a file in one go. Choose where each runs in the editor. |
| `pipe.open_editor` | Open editor |
| `pipe.edit` | Edit |
| `pipe.new` | New pipeline |
| `pipe.duplicate` | Duplicate |
| `pipe.delete` | Delete |
| `pipe.delete_confirm` | Delete “{name}”? |
| `pipe.copy_name` | {name} copy |
| `pipe.untitled` | Untitled pipeline |
| `pipe.tab.pipelines` | Pipelines |
| `pipe.tab.zones` | Preset zones |
| `pipe.name` | Name |
| `pipe.media` | File type |
| `pipe.media.image` / `.video` / `.audio` / `.pdf` | Images / Videos / Audio / PDFs |
| `pipe.media.one.image` / `.video` / `.audio` / `.pdf` | image / video / audio / PDF |
| `pipe.empty.title` | No pipelines yet |
| `pipe.empty.body` | Start from a template or build your own. |
| `pipe.starter.webp` | Convert to WebP |
| `pipe.starter.web` | Downscale for the web |
| `pipe.starter.invoices` | Invoices to folder |
| `pipe.mode.visual` | Steps |
| `pipe.mode.text` | Text |
| `pipe.add_step` | Add step |
| `pipe.move_up` | Move up |
| `pipe.move_down` | Move down |
| `pipe.remove_step` | Remove step |
| `pipe.drag_handle` | Drag to reorder |
| `pipe.inspector.empty` | Select a step to change it. |
| `pipe.text.help` | One step per line: step setting=value. Put values with spaces in quotes. |
| `pipe.text.suggestions` | Suggestions |
| `pipe.issues` | {n} problems |
| `pipe.step.optimise` / `convert` / `downscale` / `crop` / `strip_exif` / `copy` / `fork` / `if_name_contains` | Optimise / Convert / Downscale / Crop / Strip metadata / Copy / Fork / If name contains |
| `pipe.param.compression` / `adaptive` / `level` / `encoder` / `location` / `folder` / `template` | Quality / Adaptive / Level / Encoder / Save to / Folder / File name |
| `pipe.param.convert_to` / `copy_to` / `by` / `value` / `aspect` / `width` / `height` / `long_edge` / `smart` / `text` | Format / Path / By / Value / Ratio / Width / Height / Long edge / Smart framing / Text |
| `pipe.choice.normal` / `aggressive` / `auto` / `fast` / `small` / `visually_lossless` | Normal / Aggressive / Automatic / Fast / Small / Visually lossless |
| `pipe.choice.keep` / `in_place` / `same_folder` / `folder` / `temporary` | As in File handling / In place / Same folder / Specific folder / Temporary |
| `pipe.choice.percent` / `width` / `height` / `long_edge` / `free` | Percent / Width / Height / Long edge / Free |
| `pipe.choice.r16_9` / `r4_3` / `r1_1` / `r1_91_1` / `r9_16` | 16:9 / 4:3 / 1:1 / 1.91:1 / 9:16 |
| `pipe.choice.png` / `jpeg` / `webp` | PNG / JPEG / WebP |
| `pipe.caption.optimise` | Optimise |
| `pipe.caption.quality` | q {q} |
| `pipe.caption.adaptive` | adaptive |
| `pipe.caption.aggressive` | aggressive |
| `pipe.caption.convert` | Convert to {format} |
| `pipe.caption.scale` | Downscale to {n} % |
| `pipe.caption.width` | Downscale to {n} px wide |
| `pipe.caption.height` | Downscale to {n} px tall |
| `pipe.caption.long_edge` | Downscale to {n} px long edge |
| `pipe.caption.crop` | Crop {ratio} |
| `pipe.caption.crop_size` | Crop to {w}×{h} |
| `pipe.caption.smart` | smart |
| `pipe.caption.strip` | Strip metadata |
| `pipe.caption.copy` | Copy to {dir} |
| `pipe.caption.fork` | Fork |
| `pipe.caption.if` | If name contains “{text}” |
| `pipe.caption.unknown` | {kind} (unavailable) |
| `pipe.error.unknown_step` | “{kind}” isn't available in this version of Ferry |
| `pipe.error.after_output` | Steps that change the file must come before Copy, or after Fork |
| `pipe.error.fork_last` | Add steps after Fork |
| `pipe.error.range` | {name} must be between {min} and {max} |
| `pipe.error.required` | {name} is required |
| `pipe.error.choice` | {name} can't be “{value}” |
| `pipe.error.template` | Unknown token {tokens} |
| `pipe.error.crop_empty` | Choose a ratio or a size |
| `pipe.error.copy_no_name` | End the path with a file name, such as %f.%e |
| `pipe.error.unknown_param` | “{name}” isn't a setting of {step} |
| `pipe.error.syntax` | Line {line}: {why} |
| `pipe.error.quote` | missing closing quote |
| `pipe.error.cannot_run` | Can't run “{name}”: {why} |
| `pipe.error.missing` | That pipeline no longer exists |
| `pipe.card.step` | Step {n} of {total} · {step} |
| `pipe.note.copy_failed` | Couldn't copy: {why} |
| `pipe.menu.encoder` | Re-optimise with encoder |
| `pipe.encoder.fast` / `small` / `visually_lossless` | Fast / Small / Visually lossless |
| `pipe.menu.strip` | Strip metadata |
| `pipe.menu.run` | Run pipeline |
| `pipe.menu.run_none` | No pipelines yet |
| `pipe.menu.producer` | Show producing pipeline |
| `pipe.slider.adaptive` | Adaptive |
| `pipe.assign.title` | Runs on |
| `pipe.assign.clipboard` | Copied {media} |
| `pipe.assign.drop_zone` | Drop zone |
| `pipe.assign.zone` | Zone “{name}” |
| `pipe.assign.folders` | Watched folders choose their pipeline in Watched folders. |
| `pipe.assign.none` | Not assigned to anything yet. |
| `pipe.hide_result` | Hide the result card |
| `pipe.hide_result.help` | Errors still show a card. |
| `pipe.test.title` | Test |
| `pipe.test.drop` | Drop a sample file here |
| `pipe.test.choose` | Choose file… |
| `pipe.test.running` | Testing… |
| `pipe.test.passed` | Test passed |
| `pipe.test.failed` | Failed at step {n} |
| `pipe.test.skipped` | Skipped: the name doesn't contain “{text}” |
| `pipe.test.would_copy` | Would copy to {path} |
| `pipe.test.not_image` | {name} isn't an image Ferry can read |
| `pipe.test.reveal` | Show test folder |
| `pipe.test.ms` | {ms} ms |
| `pipe.test.disabled` | Fix the problems above to test. |
| `pipe.last_run` | Last run {time} |
| `pipe.never_run` | Not run yet |
| `pipe.used_by` | Used by {list} |
| `pipe.zones.window_title` | Preset zones |
| `pipe.zones.add` | Add zone |
| `pipe.zones.name` | Name |
| `pipe.zones.icon` | Icon |
| `pipe.zones.pipeline_for` | Pipeline for {media} |
| `pipe.zones.none_choice` | None |
| `pipe.zones.preview` | Preview on screen |
| `pipe.zones.edit_presets` | Edit presets… |
| `pipe.zones.done` | Done |
| `pipe.zones.empty` | No preset zones yet. |
| `pipe.zones.remove` | Remove |
| `pipe.zones.menu.edit` | Edit zone… |
| `pipe.zones.menu.pipeline` | Go to pipeline |
| `pipe.zones.folder_skipped` | Drop files, not folders, on a preset zone |
| `pipe.zones.untitled` | Zone |
| `pipe.zones.hint_tap` | Tap Ctrl for presets |
| `pipe.zone.none` | No pipeline for {media} |
| `pipe.zone.no_pipeline` | “{zone}” has no pipeline for {media} |

Slash-separated rows are one key per item, in the same order as the texts. Write each as its own JSON entry.

## Open questions for the user

1. **Deferred steps.** `runScript` and the OS-automation steps (Shortcuts, PowerShell) need a security design (R§3.4 "only from pipelines the user created or approved") and a shell allow-list. `copyLinkForSending` depends on secure send, which is out of scope. This plan defers all three. Is that OK?
2. **`images.adaptive`.** Cycle 1 labelled it "Adaptive PNG/JPEG switching", while a separate `switch_png_jpeg` key also exists. This plan makes `adaptive` mean adaptive compression and `switch_png_jpeg` mean the format switch, and relabels the first. Confirm.
3. **Adaptive targets.** SSIMULACRA2 80 (Normal) and 70 (Aggressive), quality range 30–95, at most 5 encodes. Tune after the manual check?
4. **Encoder enum shared with stream A.** This plan defines `ferry_core::optimise::Encoder { Fast, Small, VisuallyLossless }` and `Edits.encoder`. Stream A's plan should reuse them for video rather than add a second enum. Please make sure A's plan agrees before both run in parallel.
5. **The seam with stream D.** F listens to D's `drag:tap` and toggles the zones on `"control"`. Before or after D merges, the overlay also opens from the editor's "Preview on screen". Three small asks of D:
   - **(a)** Also emit `drag:ended` (payload `null`) when a drag ends. Without it, F hides the overlay on timers.
   - **(b)** After both merge, change the single `jobs::start` call in D's `drop_batch::intake::accept` to `pipe_jobs::start_assigned(…, Trigger::DropZone, …)`, so that the drop-zone pipeline assignment takes effect.
   - **(c)** After both merge, draw F's `pipe.zones.hint_tap` in D's zone hint row.

   Are these post-merge edits OK to make in D's files?
6. **Watched folders.** The pipeline id for a watched folder lives in stream C's schema, and C calls `pipe_jobs::start_assigned(…, Trigger::Pipeline(id), …)`. Until then, the library's "Used by" does not list folders. C must also ignore files Ferry writes through the copy step, or a copy into a watched folder would loop. Confirm that C's plan covers this.
7. **Text mode.** The syntax is line-based (`step key=value`), with suggestions rather than a full autocomplete popup. Is that enough for "text mode with autocomplete" (R§4.13)?
8. **Fork on clipboard jobs.** The clipboard keeps the main (last) branch's result, and forks are cards only, through the `NEVER_ON_CLIPBOARD` sentinel. Is that the behaviour you want?
