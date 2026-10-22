# Ferry — Cycle 2: Card tools and card extras

- **Date:** 2026-10-22
- **Status:** Approved design, awaiting spec review
- **Requirements:** [docs/requirements/ferry-optimize-requirements.md](../../requirements/ferry-optimize-requirements.md) ("R§" below)
- **Builds on:** [Cycle 1 spec](2026-10-21-ferry-cycle1-foundation-phase1-design.md) ("C1§" below), merged and released as 0.1.1
- **Mockups:** `.superpowers/brainstorm/78228-1792661273/content/` (card-layout, crop-popover, compare; git-ignored, chosen options A, A, confirmed)

## 1. Goal and success criteria

Make every finished result card adjustable after the fact (R§1 story 7): downscale, crop, convert, recompress, fit under a size, compare with the original, preview, rename, all re-encoded from the backed-up original, from the card rail, a native context menu, or hover hotkeys.

Cycle 2 is done when all of these hold on this Mac (Intel, macOS 14.6):

1. Copy a 2880×1800 screenshot, press-and-drag the card's Downscale button to 50%: the clipboard now holds a 1440×900 PNG, the card reads "2880×1800 → 1440×900".
2. Copy a PNG file in Finder (file job), click **WebP** in the format bar: with the default "In place" location the file is now `shot.webp` and `shot.png` is gone; **Restore** brings back `shot.png` and moves `shot.webp` to the Trash.
3. Crop to 16:9 with Smart framing, then downscale, then convert: the result equals applying all three to the original once (no generation loss: each run starts from the backup).
4. Fit under 1 MB on a 4 MB JPEG produces a file ≤ 1 MB, or an amber "Couldn't reach 1 MB" note when even quality 5 misses.
5. Hovering a card and pressing Space opens Quick Look; ⌘D opens the compare window with a working divider; `5` downscales to 50%. Moving the pointer off the card returns Space to the frontmost app.
6. Renaming via the filename chip renames the file on disk; a taken name shows an inline error.
7. All `ferry-core` tests pass on Ubuntu, macOS and Windows; vitest and svelte-check are clean.

Windows behaviour is **built but not verified on a real desktop**, as in Cycle 1.

## 2. Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Transform engine | In-process: decode with the codecs already linked (png, mozjpeg, libwebp), resample with `fast_image_resize` 5.x (MIT/Apache, pure Rust, SIMD), re-encode with the existing optimisers | No new sidecar or per-OS binary; testable on 3 OSes. libvips stays an option for the later media-types slice (HEIC/AVIF), behind the same edit step |
| Animated GIF | gifsicle's own `--crop` / `--resize` / `--lossy` | Already bundled; keeps frames and timing |
| Animated WebP / APNG | Edit tools disabled ("Animated WebP can't be edited yet") | No animation encoder yet (C1§2) |
| Convert targets | PNG, JPEG, WebP. A GIF can be converted to a still (first frame) | AVIF/HEIC/JXL belong to the media-types slice |
| Edit model | A per-job `Edits` recipe re-applied to the original on every run | R§1.7 "always re-encoding from the original"; no generation loss; Restore = clear the recipe |
| Explicit edits vs plain optimise | Plain optimise keeps C1 rules (Already optimal / Larger keep the original). Any explicit edit always writes its result, even if larger | The user asked for it; the card shows the real sizes |
| Output location | New File handling setting, Images row: Optimised and Converted each choose In place (default), Same folder, Specific folder or Temporary | User request; R§2.6. Video/PDF rows arrive with those types |
| Trash | `trash` crate 5.x for moving produced files to the Trash / Recycle Bin | Never hard-delete user-visible files (safety rules) |
| Text input in the stack | The stack panel becomes key only while a text field is open, then hands focus back | The panel is non-activating (C1); rename, crop and fit need typing |
| Hover hotkeys | Registered globally only while the pointer is over an idle card; setting on by default | The stack never has focus, so keys must be global; scoped to hover to limit stealing |
| Context menu | Native Tauri menu popped up from Rust | R§5 "use native context menus" |
| Quick preview | macOS: Quick Look (`QLPreviewPanel`, fallback `qlmanage -p`); Windows: the compare window in single-image mode | R§4.11 |
| Compression slider | Encoder quality 100 → 5, then an **Extreme** stop (aggressive at q 5). No Adaptive detent | Adaptive is still "Coming soon" (C1§5.7) |
| Automation | CLI, SDK and MCP removed from the requirements (2026-10-22). Pipelines stay later work | User decision |

## 3. Scope

**In scope:** downscale (slider, steps, presets), crop (aspect presets, exact size, long edge, Smart/Centre framing, ×0.5/×2, saved sizes), convert (format bar, keep-both), compression slider, fit under size, compare window, quick preview, rename, native context menu, hover hotkeys (Space, ⌘/Ctrl+D, `-`, `1`–`9`), File handling settings pane (Images row), animated GIF downscale/crop.

**Out of scope (later):** the "+" rail slot and rail editor, pipeline buttons, Open with / Edit with, Share / Send securely / Upload, Strip metadata and Re-optimise-with-encoder menu items, Adaptive compression, the compact list and its selection bar, video/audio/PDF tools, Windows real-desktop verification, release hardening.

## 4. Architecture

### 4.1 New and changed files

```
crates/ferry-core/src/
├─ edit/mod.rs        # Edits, Crop, Size, Framing; apply(original, format, &Edits, &OptimiseOptions)
├─ edit/raster.rs     # decode to RGBA (+ICC, +EXIF orientation applied), encode RGBA to PNG/JPEG/WebP
├─ edit/geometry.rs   # crop window and target size maths (pure, heavily tested)
├─ edit/smart.rs      # edge-energy smart crop
├─ edit/fit.rs        # fit under size: quality search
├─ output.rs          # output location resolution (In place / Same folder / Specific / Temporary)
├─ pipeline.rs        # Job gains `edits`, `source_path`, `kind`; run applies Edits; rename; restore trashes
├─ job.rs             # JobUpdate gains the fields in §4.3
├─ settings.rs        # images.output, images.crop_sizes, floating.hover_hotkeys; version 2
└─ optimise/gif.rs    # crop/resize args
src-tauri/src/
├─ jobs.rs            # card_edit / card_convert / card_fit / card_rename entry points
├─ menu.rs            # card context menu build + event routing (new)
├─ hover_keys.rs      # register/unregister hover hotkeys from pointer state (new)
├─ preview.rs         # Quick Look (macOS) / single-image window (Windows) (new)
├─ windows.rs         # compare window; stack key-on-demand
└─ platform/{macos,windows}.rs  # make_stack_key / resign_stack_key, quick_look
src/
├─ compare.html, src/compare/{main.ts,Compare.svelte}   # new page
├─ stack/Card.svelte  # rail A, sliders, format bar, rename field
├─ stack/{Slider,CropPopover,FitPopover,FormatBar}.svelte
├─ settings/…         # File handling pane, hover hotkeys toggle, crop sizes
└─ lib/edits.ts       # pure helpers: slider value ↔ label, crop validation, menu enabled model
```

### 4.2 `ferry-core` edit interfaces

```rust
pub enum Framing { Centre, Smart }

pub struct Crop { pub aspect: (u32, u32), pub framing: Framing }   // e.g. (16, 9)

pub enum Size {
    Exact { width: u32, height: u32 },  // cover-crops to w:h (using framing), then scales
    Width(u32),                         // height follows the ratio ("0 keeps ratio")
    Height(u32),
    LongEdge(u32),
}

#[derive(Default)]
pub struct Edits {
    pub crop: Option<Crop>,
    pub size: Option<Size>,        // mutually exclusive with `scale`: setting one clears the other
    pub scale: Option<u8>,         // 10..=90 percent of the (cropped) original
    pub format: Option<Format>,    // Png | Jpeg | WebP; None keeps the source format
    pub compression: Option<u8>,   // 5..=100; None uses settings
    pub fit_under: Option<u64>,    // bytes; overrides compression
    pub level: Level,              // Normal | Aggressive (Extreme = Aggressive + compression 5)
}

impl Edits { pub fn is_plain(&self) -> bool; }  // only level set → C1 optimise rules

pub fn apply(original: &[u8], source: Format, edits: &Edits, o: &OptimiseOptions)
    -> Result<Applied, OptimiseError>;
pub struct Applied { pub bytes: Vec<u8>, pub format: Format, pub dims: (u32, u32), pub missed_target: bool }
```

- **Order is fixed:** decode → orient → crop → size or scale → encode in target format → optimise. Repeating "downscale 50%" sets `scale = 50`; it never compounds.
- **Never upscale:** sizes larger than the cropped original clamp to it; the crop popover shows "Larger than the original" as a hint.
- **Raster path:** used only when `crop`, `size`, `scale` or a format change is present. Plain compression changes and fit-under-size on the same format go straight to the existing optimiser with a new quality, so metadata rules (C1§5.8) stay identical.
- **Orientation and colour:** JPEG EXIF orientation is applied to pixels before cropping and the output carries orientation 1 (minimal EXIF as in C1§5.8). The ICC profile is carried into the output (PNG `iCCP`, JPEG APP2, WebP `ICCP`); pixels are not colour-converted.
- **Alpha to JPEG:** flattened onto white.
- **GIF:** animated GIFs use gifsicle `--crop x,y+wxh` then `--resize WxH` then the usual `-O3 --lossy`. Smart framing on a GIF uses the first frame's energy map. Converting a GIF to PNG/JPEG/WebP takes the first frame.
- **Smart crop (`smart.rs`):** downsample luminance to ≤ 256 px on the long edge, compute Sobel gradient magnitude, build an integral image, slide the target-ratio window and keep the highest-energy position (ties → nearest centre); map back to full resolution.
- **Fit under size (`fit.rs`):** binary search over quality 5–100 (JPEG/WebP quality, PNG imagequant max quality, GIF lossy level), at most 7 encodes, keep the largest result ≤ target. If quality 5 still misses, write that smallest result and set `missed_target`.

### 4.3 Job and card model changes

- `Job` gains `edits: Edits`, `source_path: Option<PathBuf>` (a file job's original location), `kind: OutputKind { Optimised, Converted }`, and outputs are resolved by `output::resolve` on first run per kind, then kept.
- `Job::run(edits)` stores the recipe and applies it; `Retry`, **Re-optimise** and **Optimise aggressively** re-run the current recipe with the level changed; **Restore** clears it.
- **Convert keep-both** (⌥/Alt-click a chip, or the menu with ⌥): a new `Job` with a new id that shares the same backup path; it gets its own card.
- **Rename:** `Job::rename(new_stem)` renames the current output in its folder (file job: the real file; clipboard job: the working file used by drag-out). Rejects empty names, path separators and names reserved on Windows; a taken name returns `RenameError::Taken`. Later runs keep the new stem.
- `JobUpdate` gains:

```rust
pub dims_before: Option<(u32, u32)>,
pub dims_after: Option<(u32, u32)>,
pub format: String,            // current output format, e.g. "webp"
pub converted: bool,
pub can_edit: bool,            // backup present, not animated WebP/APNG, not busy
pub edits: EditsView,          // scale, compression, crop aspect, size, format: what the sliders and popovers start from
pub note: Option<String>,      // amber note, e.g. "Couldn't reach 1 MB"
```

### 4.4 Output locations (`output.rs`)

```rust
pub enum Location { InPlace, SameFolder, Folder(PathBuf), Temporary }
pub fn resolve(source: &Path, kind: OutputKind, ext: &str, dims: (u32, u32), rule: &OutputRule, work_dir: &Path, today: NaiveDate) -> PathBuf;
```

- **Applies to file jobs only.** Clipboard jobs always use their working file (C1§5.4); a convert changes its extension.
- **In place:** Optimised overwrites the source atomically (C1 behaviour). Converted writes `<stem>.<new ext>` in the source folder, then moves the source file to the Trash (the backup already holds it). Re-converting replaces the last conversion's file.
- **Same folder / Specific folder:** the template (file name only; `%f %e %z %y %m %d %P`) is rendered in that folder. The source is never touched. Defaults: Optimised `%f_optimised.%e`, Converted `%f.%e`, folder `~/Pictures/Ferry`.
- **Temporary:** `<work_dir>/files/<job id>/<stem>.<ext>`.
- **Collisions:** if the resolved file exists and is not this job's own earlier output, append ` (2)`, ` (3)`, … before the extension. A resolved path equal to the source counts as In place.
- **Restore:** In place puts the original back at the source path (atomic) and, after a conversion, moves the converted file to the Trash. Other locations move the produced file to the Trash; the source was never changed.

### 4.5 Tauri app

- **Commands:**
  - `card_edit(id, edits: EditsPatch)`: merge and re-run.
  - `card_convert(id, format, keep_both)`.
  - `card_rename(id, stem) -> Result<(), RenameError>`.
  - `card_menu(id, x, y)`: pops the native menu at the cursor.
  - `card_compare(id)`, `card_preview(id)`.
  - `stack_focus(on: bool)`: key-on-demand for text fields.
- **Context menu (`menu.rs`):** Open · Show in Finder/Explorer ─ Copy · Copy path · Rename… ─ Compare with original ⌘D · Preview Space ─ Re-optimise · Optimise aggressively · Fit under size… ─ Downscale ▸ (90 % … 10 %) · Crop… · Convert to ▸ (PNG, JPEG, WebP; ⌥ shows "… (keep both)") ─ Restore original ─ Close. Items that need the original are disabled when `can_edit` is false; Restore is disabled without a backup. Rename…, Crop… and Fit under size… open the card's inline UI through a `card:open` event.
- **Hover hotkeys (`hover_keys.rs`):** the pointer forwarding that drives hover (C1 stack) also tells Rust which idle card is under the pointer. On enter, register Space, ⌘/Ctrl+D, `-`, `1`–`9` via `tauri-plugin-global-shortcut`; on leave, when the card becomes busy, while the stack is key, or when `floating.hover_hotkeys` is off, unregister. `1`–`9` set scale 10 %–90 %; `-` steps 10 percentage points down from the current scale (100 → 90 → … → 10).
- **Key-on-demand (platform):** macOS: the panel's `canBecomeKeyWindow` returns true only while a field is open; `makeKeyWindow` without activating the app (non-activating panel), `resignKeyWindow` afterwards so keys return to the frontmost app. Windows: drop `WS_EX_NOACTIVATE` and `SetForegroundWindow` while a field is open, remember the previous foreground window and restore it on close.
- **Compare window:** one reusable window (`compare.html`), sized to the image (max 80 % of the screen of the card), loads the backup and the current output through the asset protocol. Takes focus while open; Esc closes and focus returns.
- **Quick preview:** macOS `QLPreviewPanel` on the current output (fallback `qlmanage -p` if the panel cannot show from a non-activating app); Windows opens the compare window in single-image mode. Space toggles it.

## 5. Behaviour

### 5.1 Rail (layout A)

Downscale · Compression · Crop · Restore · Compare · Copy · ⋯ (opens the context menu). Busy cards keep today's Cancel. Compact cards hide Compression and Compare (still in the menu). Rail buttons keep today's tooltip and hover forwarding.

### 5.2 Sliders

- Press-and-drag on Downscale or Compression drops a vertical slider from the button over the thumbnail; a badge shows the live target ("50 % · 1440×900" / "q 60" / "Extreme"). Release applies; Esc or releasing back at the start value cancels.
- Plain click applies one step: Downscale 10 percentage points down (100 → 90 → … → 10); Compression 10 quality down (then Extreme).
- Downscale range 90–10 % in steps of 5; Compression 100–5 in steps of 5, then Extreme.

### 5.3 Crop and Fit popovers (layout A)

- Open beside the card toward the screen centre; the stack window grows to fit, and the card draws the live crop frame on its thumbnail.
- **Crop:** aspect chips Free, 16:9, 4:3, 1:1, 1.91:1, 9:16; W × H fields (0 keeps ratio); Size / Long edge toggle; Smart / Centre toggle; ×0.5 / ×2; saved sizes from settings with "Restore defaults"; **Crop** button (Enter). Invalid input (non-number, both 0, over 30 000) turns the field red with a hint and disables Crop.
- **Fit under size:** number field, KB/MB unit, chips 1 MB, 5 MB, 10 MB, 25 MB; **Fit** button (Enter).
- Both call `stack_focus(true)` on open and `stack_focus(false)` on close, Esc, Enter or click-away.

### 5.4 Format bar and convert

- On hover the bottom detail line is replaced by chips PNG · JPEG · WebP with the current format selected. GIF cards add a selected GIF chip and the others carry the tooltip "First frame only".
- Click converts (replacing the last conversion); ⌥/Alt-click keeps both (new card). The extension chip shows the new format and a small "converted" label.

### 5.5 Rename

Click the name chip → inline field prefilled with the stem, selected. Enter confirms, Esc cancels, blur cancels. Errors inline: "Name taken", "Can't use / or :" (per OS reserved set), "Name can't be empty".

### 5.6 Settings (version 2)

```jsonc
{
  "version": 2,
  "images": {
    // …C1 fields…
    "output": {
      "optimised": { "location": "in_place", "folder": "~/Pictures/Ferry", "template": "%f_optimised.%e" },
      "converted": { "location": "in_place", "folder": "~/Pictures/Ferry", "template": "%f.%e" }
    },
    "crop_sizes": [[1200, 628], [1080, 1080], [1920, 1080], [1280, 720]]
  },
  "floating": { /* …C1 fields… */ "hover_hotkeys": true }
}
```

Version 1 files load with these defaults (serde defaults; unknown fields kept, C1§5.7). The Settings window gains a **File handling** pane (Images row: Optimised and Converted columns, each a location picker, folder picker when Specific, template field with token chips and a live example path), a **Hover hotkeys** toggle under Floating results, and the crop sizes list under Images.

## 6. Error handling

- An edit that fails ends in Error with the reason (C1§6 wording rules); **Retry** re-applies the same recipe.
- Cancel during an edit returns the card to its previous result (existing `cancel_back`).
- Backup expired or missing: `can_edit` is false; tools, format bar and compare are disabled with the tooltip "Original no longer kept". Restore behaves as in C1.
- Fit target unreachable: the smallest result is written and the card shows the amber note "Couldn't reach 1 MB — try downscaling".
- Output folder missing or unwritable: Error "Can't write to ~/Pictures/Ferry", nothing changed at the source.
- Trash unavailable (e.g. network volume): log it and leave the file; the card still restores.
- Rename races (file vanished or renamed outside Ferry): inline error "File no longer there"; the card keeps its name.
- Hotkey registration failure (another app owns Space globally): log once, the card still works by click.

## 7. Testing

- **`ferry-core` (3 OSes):**
  - `geometry`: crop window for every aspect preset on landscape and portrait inputs; Exact/Width/Height/LongEdge targets; never-upscale clamp; scale on top of crop.
  - `smart`: a synthetic image with one textured block lands the window on the block; a flat image falls back to centre.
  - `raster`: every source × target format pair round-trips with the expected dimensions; alpha → JPEG is white; ICC survives; a rotated (orientation 6) JPEG is cropped in display orientation and comes out with orientation 1.
  - `apply`: crop + scale + convert from the original equals the same recipe applied in one go after other edits (no compounding).
  - `fit`: lands ≤ target on a fixture; unreachable target sets `missed_target`.
  - `gif`: animated crop and resize keep the frame count (skipped when gifsicle is absent, as today).
  - `output`: every location × kind, collisions, `%` tokens, path-equals-source, Windows reserved names.
  - `pipeline`: convert in place then restore puts the source back and trashes the conversion (trash stubbed behind a trait); rename taken/empty/invalid; keep-both shares the backup.
- **vitest:** slider value ↔ label (`edits.ts`), crop field validation, format-bar choices per source format, menu enabled model per card state.
- **Manual checklist (macOS now, Windows when a machine is available):** the §1 criteria; key-on-demand returns focus to the frontmost app; hover hotkeys stop at pointer leave; Quick Look over another app; compare window on a second monitor.

## 8. Milestones

| Milestone | Delivers | Done when |
| --- | --- | --- |
| **M3a** Edit engine | `edit/*`, `output.rs`, `Job` recipe, rename, restore-with-trash, settings v2 model, `JobUpdate` fields, gifsicle crop/resize | All core tests green on 3 OSes; existing app behaviour unchanged with default settings |
| **M3b** Card UI | Rail A, sliders, format bar, rename, crop and fit popovers, key-on-demand, context menu, hover hotkeys, File handling pane | §1 criteria 1–4 and 6 on this Mac |
| **M3c** Compare and preview | Compare window, Quick Look / Windows preview, ⌘D and Space wiring | §1 criterion 5 on this Mac |

## 9. Risks

- **Key-on-demand on a non-activating panel.** Text input in the stack is new. Mitigation: spike it first in M3b (a rename field) before the popovers; fallback is a small separate key window for the popover.
- **Quick Look from an inactive app.** `QLPreviewPanel` follows the key window's responder chain. Mitigation: fallback `qlmanage -p`, which always works but looks plainer.
- **Hover hotkeys stealing keys.** Space typed into another app while the pointer rests on a card goes to Ferry. Mitigation: register only on hover over an idle card, unregister on leave and on focus change; the setting turns it off.
- **Press-and-drag in the panel.** Drag events after mousedown should reach the webview even without key status (clicks already do); verify in the M3b spike alongside the rename field.
- **Large images.** A 50 MB-limit PNG decodes to a few hundred MB of RGBA. Mitigation: the existing per-CPU semaphore; resize in `fast_image_resize` is streaming-friendly; no extra copies beyond one RGBA buffer.
