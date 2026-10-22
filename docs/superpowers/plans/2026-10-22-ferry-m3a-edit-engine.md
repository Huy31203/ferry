# M3a Edit Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `ferry-core` an edit recipe (downscale, crop, convert, recompress, fit under size) that every run re-applies to the backed-up original, plus output locations, rename, keep-both and settings v2, with the app's behaviour unchanged under default settings.

**Architecture:** A new `edit` module holds the recipe (`Edits`), pure geometry, smart framing, RGBA decode/encode (`raster`), a quality search (`fit`) and `apply`, which turns original bytes plus a recipe into result bytes. `output.rs` resolves where a file job's result goes. `pipeline::Job` keeps the recipe, owns exactly one result file at a time and undoes the previous one when the next run lands elsewhere. The app crate only passes two new things into `RunOptions` (working folder, Trash function).

**Tech Stack:** Rust 2024 workspace; existing png 0.18, mozjpeg 0.10, webp 0.3, imagequant 4.4, oxipng 10.2, gifsicle sidecar; new `fast_image_resize` 6.1 and `gif` 0.14 in `ferry-core`, `trash` 5.2 in `src-tauri`.

**Spec:** [docs/superpowers/specs/2026-10-22-ferry-cycle2-card-tools-design.md](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) (M3a row of §8; sections §2, §4.1–§4.4, §5.6, §6, §7)

## Global Constraints

- Every module opens with a `//!` doc comment and ends with `#[cfg(test)] mod tests`; match the surrounding comment density and naming.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` must be clean after every task.
- UI text lives in `src/lib/i18n/en.json` (flat map of strings, shared with Rust via `crate::i18n::{t, tf}`); never hard-code English card text in Rust.
- `ferry-core` stays free of Tauri and OS UI dependencies; the Trash call is injected from the app (`RunOptions::trash`).
- Each run starts from the backup, never from a previous result (R§1.7). Repeating "downscale 50 %" sets `scale = 50`; it never compounds.
- Plain optimise (recipe has only `level`) keeps Cycle 1 rules: Already optimal / Larger leave the original. Any explicit edit always writes its result.
- Never upscale: sizes larger than the cropped original clamp to it. Sizes and sides are 1..=30 000 (`edit::MAX_SIDE`).
- Scale 10..=90 %; compression 5..=100; Extreme = `Level::Aggressive` with compression 5.
- Convert targets PNG, JPEG, WebP only; a GIF converts to a still from its first frame; animated WebP/APNG are not editable.
- JPEG orientation is applied to pixels before cropping; the output carries orientation 1. ICC profiles are carried, never colour-converted. Alpha flattens onto white for JPEG.
- Output locations apply to file jobs only; defaults: Optimised and Converted both In place, folder `~/Pictures/Ferry`, templates `%f_optimised.%e` and `%f.%e`. Settings `version` becomes 2.
- User-visible files Ferry produced are moved to the Trash, never deleted. A Trash failure leaves the file and is logged by the app.
- Implementers run on model `sonnet`; plan writing and reviews run on `opus` (never `fable`).
- Commits end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Grayscale JPEG and 16-bit / palette PNG through the raster path** — a person crops a scanned grey JPEG or a 16-bit PNG and expects a normal result, not a decode error. Pinned in Task 4 (`every_still_format_round_trips_with_its_size` includes `jpeg_gray` and a 16-bit PNG).
2. **Very thin or tiny images** (1×N, 3×3) with scale 10 % or an aspect crop — expected: at least 1 px each side, no panic or zero-size encoder call. Pinned in Task 2 (`tiny_and_thin_pictures_never_reach_zero`) and Task 3 (`a_window_as_large_as_the_picture_is_the_whole_picture`).
3. **The source file was deleted or moved outside Ferry between runs** — expected: the next edit still finishes (the result is written where the source was) and nothing panics. Pinned in Task 9 (`an_edit_after_the_source_vanished_still_writes_a_result`).
4. **A Specific-folder output that cannot be created** — expected: an Error card naming the folder, and the source holds the original. Pinned in Task 9 (`an_unwritable_output_folder_is_an_error_and_the_source_keeps_the_original`).
5. **Case-only rename on a case-insensitive disk** (`shot` → `Shot` on macOS) — expected: allowed, not "Name taken". Pinned in Task 10 (`a_case_only_rename_is_allowed`).

---

## Setup (once, before Task 1)

Every shell needs the toolchain and, for GIF tests, the local gifsicle:

```bash
source "$HOME/.cargo/env"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
```

Work on a branch: `git checkout -b feat/m3a-edit-engine` (from `main`).

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/sniff.rs` (modify) | `Format` gets serde + `label()` | 1 |
| `crates/ferry-core/src/optimise/mod.rs` (modify) | `Level` gets serde + `Default`; `encode()` split out of `optimise()`; `jpeg`, `gif` become `pub(crate)` | 1, 5 |
| `crates/ferry-core/src/edit/mod.rs` (create) | `Edits`, `EditsPatch`, `Size`, `Framing`, `apply`, `Applied` | 1, 7 |
| `crates/ferry-core/src/edit/geometry.rs` (create) | crop window and output size maths | 2 |
| `crates/ferry-core/src/edit/smart.rs` (create) | edge-energy smart framing | 3 |
| `crates/ferry-core/src/edit/raster.rs` (create) | `Raster`, decode / orient / cut / encode, `display_dims` | 4 |
| `crates/ferry-core/src/optimise/{png,jpeg,webp}.rs` (modify) | raster decode/encode hooks | 4 |
| `crates/ferry-core/src/optimise/gif.rs` (modify) | gifsicle crop / resize | 5 |
| `crates/ferry-core/src/edit/fit.rs` (create) | fit-under-size quality search | 6 |
| `crates/ferry-core/src/settings.rs` (modify) | `Output`, `OutputRule`, `Location`, `crop_sizes`, `hover_hotkeys`, version 2 | 8 |
| `crates/ferry-core/src/output.rs` (create) | where a file job's result goes; collision names | 8 |
| `crates/ferry-core/src/job.rs` (modify) | new `JobUpdate` fields | 9 |
| `crates/ferry-core/src/pipeline.rs` (modify) | recipe, one result per job, undo, restore, `edit`, `fork`, `rename` | 9, 10 |
| `src-tauri/src/jobs.rs`, `src-tauri/Cargo.toml` (modify) | pass work dir and Trash function | 9 |
| `src/lib/i18n/en.json` (modify) | `card.step.edit`, `card.note.fit_missed`, `error.animated_edit`, `rename.*` | 7, 9, 10 |

**Handoff to M3b (not in this plan):** `src-tauri/src/state.rs` `Entry::output` is copied once at job creation and goes stale after a conversion or rename; M3b must refresh it (or read `job.output`) when it adds the convert/rename commands. The TS `JobUpdate` and `Settings` types and `settings.fixture.ts` (still `version: 1`) are updated in M3b.

---

### Task 1: The edit recipe

**Files:**
- Modify: `crates/ferry-core/src/sniff.rs` (derive serde on `Format`, add `label`)
- Modify: `crates/ferry-core/src/optimise/mod.rs:13-17` (`Level` derives)
- Create: `crates/ferry-core/src/edit/mod.rs`
- Modify: `crates/ferry-core/src/lib.rs` (`pub mod edit;`)

**Interfaces:**
- Consumes: `crate::sniff::Format`, `crate::optimise::Level`.
- Produces:
  - `Format: Serialize + Deserialize` (`"png" | "jpeg" | "gif" | "webp"`), `Format::label(self) -> &'static str` (`"PNG"`, `"JPEG"`, `"GIF"`, `"WebP"`).
  - `Level: Default (Normal) + Serialize + Deserialize` (`"normal" | "aggressive"`).
  - `edit::MAX_SIDE: u32 = 30_000`.
  - `edit::Framing { Centre (default), Smart }` (serde snake_case).
  - `edit::Size { Exact { width, height }, Width { width }, Height { height }, LongEdge { edge } }` (serde `tag = "kind"`, snake_case).
  - `edit::Edits { aspect: Option<(u32,u32)>, framing: Framing, size: Option<Size>, scale: Option<u8>, format: Option<Format>, compression: Option<u8>, fit_under: Option<u64>, level: Level }` with `is_plain()`, `reshapes()`, `merge(EditsPatch)`; `Clone + Default + PartialEq + Eq + Serialize + Deserialize`.
  - `edit::EditsPatch` (Deserialize; absent = leave, `null` = clear, value = set).

- [ ] **Step 1: Add serde to `Format` and `Level`, and `Format::label`**

In `crates/ferry-core/src/sniff.rs`, add `use serde::{Deserialize, Serialize};` and change the enum and impl:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Format {
    Png,
    Jpeg,
    Gif,
    WebP,
}

impl Format {
    /// File extension used when Ferry writes this format.
    pub fn extension(self) -> &'static str {
        match self {
            Format::Png => "png",
            Format::Jpeg => "jpg",
            Format::Gif => "gif",
            Format::WebP => "webp",
        }
    }

    /// The name shown on cards: "PNG", "JPEG", "GIF", "WebP".
    pub fn label(self) -> &'static str {
        match self {
            Format::Png => "PNG",
            Format::Jpeg => "JPEG",
            Format::Gif => "GIF",
            Format::WebP => "WebP",
        }
    }
}
```

In `crates/ferry-core/src/optimise/mod.rs`, add `use serde::{Deserialize, Serialize};` and:

```rust
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Level {
    #[default]
    Normal,
    Aggressive,
}
```

- [ ] **Step 2: Write the failing tests for the recipe**

Create `crates/ferry-core/src/edit/mod.rs` with only the tests module for now (plus the `use super::*` it needs to compile once the types exist):

```rust
//! What a card asks for after the fact (spec §4.2): a recipe that every run
//! applies to the backed-up original, so edits never compound or lose quality.

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn patch(value: serde_json::Value) -> EditsPatch {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn an_absent_field_is_left_alone_and_null_clears_it() {
        let mut edits = Edits {
            aspect: Some((16, 9)),
            scale: Some(50),
            ..Edits::default()
        };
        edits.merge(patch(json!({ "aspect": null })));
        assert_eq!(edits.aspect, None);
        assert_eq!(edits.scale, Some(50));
    }

    #[test]
    fn size_and_scale_replace_each_other() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "scale": 50 })));
        edits.merge(patch(json!({ "size": { "kind": "width", "width": 1200 } })));
        assert_eq!((edits.size, edits.scale), (Some(Size::Width { width: 1200 }), None));
        edits.merge(patch(json!({ "scale": 40 })));
        assert_eq!((edits.size, edits.scale), (None, Some(40)));
    }

    #[test]
    fn compression_and_fit_under_replace_each_other() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "compression": 60 })));
        edits.merge(patch(json!({ "fit_under": 1_000_000 })));
        assert_eq!((edits.compression, edits.fit_under), (None, Some(1_000_000)));
        edits.merge(patch(json!({ "compression": 30 })));
        assert_eq!((edits.compression, edits.fit_under), (Some(30), None));
    }

    #[test]
    fn a_zero_side_keeps_the_ratio_and_nonsense_is_dropped() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "size": { "kind": "exact", "width": 1200, "height": 0 } })));
        assert_eq!(edits.size, Some(Size::Width { width: 1200 }));
        edits.merge(patch(json!({ "size": { "kind": "exact", "width": 0, "height": 900 } })));
        assert_eq!(edits.size, Some(Size::Height { height: 900 }));
        for nonsense in [
            json!({ "kind": "exact", "width": 0, "height": 0 }),
            json!({ "kind": "long_edge", "edge": 0 }),
            json!({ "kind": "width", "width": 40_000 }),
        ] {
            edits.merge(patch(json!({ "size": nonsense })));
            assert_eq!(edits.size, None);
        }
        edits.merge(patch(json!({ "aspect": [0, 9] })));
        assert_eq!(edits.aspect, None);
    }

    #[test]
    fn scale_and_compression_are_clamped() {
        let mut edits = Edits::default();
        edits.merge(patch(json!({ "scale": 5, "compression": 0 })));
        assert_eq!((edits.scale, edits.compression), (Some(10), Some(5)));
        edits.merge(patch(json!({ "scale": 95, "compression": 200 })));
        assert_eq!((edits.scale, edits.compression), (Some(90), Some(100)));
    }

    #[test]
    fn only_a_level_is_still_plain() {
        assert!(Edits::default().is_plain());
        let aggressive = Edits {
            level: Level::Aggressive,
            framing: Framing::Smart,
            ..Edits::default()
        };
        assert!(aggressive.is_plain());
        let recompressed = Edits {
            compression: Some(50),
            ..Edits::default()
        };
        assert!(!recompressed.is_plain());
        assert!(!recompressed.reshapes());
        assert!(Edits { scale: Some(50), ..Edits::default() }.reshapes());
    }

    #[test]
    fn the_card_sees_the_recipe_as_plain_json() {
        let edits = Edits {
            aspect: Some((16, 9)),
            size: Some(Size::Exact { width: 1200, height: 628 }),
            format: Some(Format::WebP),
            ..Edits::default()
        };
        assert_eq!(
            serde_json::to_value(&edits).unwrap(),
            json!({
                "aspect": [16, 9],
                "framing": "centre",
                "size": { "kind": "exact", "width": 1200, "height": 628 },
                "scale": null,
                "format": "webp",
                "compression": null,
                "fit_under": null,
                "level": "normal"
            })
        );
    }
}
```

Add `pub mod edit;` to `crates/ferry-core/src/lib.rs` (alphabetical, after `pub mod color;`).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ferry-core edit::tests`
Expected: FAIL to compile — `cannot find type Edits`, `EditsPatch`, `Size`, `Framing`.

- [ ] **Step 4: Implement the recipe**

Insert above the tests module in `crates/ferry-core/src/edit/mod.rs`:

```rust
use serde::{Deserialize, Deserializer, Serialize};

use crate::optimise::Level;
use crate::sniff::Format;

/// The largest width, height or long edge a card may ask for.
pub const MAX_SIDE: u32 = 30_000;

/// Where an aspect crop or an exact size cuts from.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Framing {
    #[default]
    Centre,
    Smart,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Size {
    /// Cover-crops to `width:height` using the framing, then scales.
    Exact { width: u32, height: u32 },
    /// Height follows the ratio ("0 keeps ratio").
    Width { width: u32 },
    Height { height: u32 },
    LongEdge { edge: u32 },
}

impl Size {
    /// An `Exact` size with one zero side keeps the ratio; a size with no
    /// usable side, or one over `MAX_SIDE`, is no size at all.
    fn normalised(self) -> Option<Size> {
        let size = match self {
            Size::Exact { width: 0, height: 0 } => return None,
            Size::Exact { width, height: 0 } => Size::Width { width },
            Size::Exact { width: 0, height } => Size::Height { height },
            other => other,
        };
        let sides = match size {
            Size::Exact { width, height } => [width, height],
            Size::Width { width: side }
            | Size::Height { height: side }
            | Size::LongEdge { edge: side } => [side, side],
        };
        sides
            .iter()
            .all(|side| (1..=MAX_SIDE).contains(side))
            .then_some(size)
    }
}

/// Everything a card asked for. Steps always run in the same order (aspect
/// crop, size or scale, convert, compress), whatever order they were asked in.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Edits {
    /// Crop to this ratio, such as `(16, 9)`; `None` is "Free".
    pub aspect: Option<(u32, u32)>,
    pub framing: Framing,
    /// Mutually exclusive with `scale`.
    pub size: Option<Size>,
    /// 10..=90 percent of the cropped original.
    pub scale: Option<u8>,
    /// `None` keeps the source format.
    pub format: Option<Format>,
    /// 5..=100; `None` uses the settings. Mutually exclusive with `fit_under`.
    pub compression: Option<u8>,
    /// Bytes.
    pub fit_under: Option<u64>,
    pub level: Level,
}

impl Edits {
    /// Nothing but a level: Cycle 1's optimise rules apply.
    pub fn is_plain(&self) -> bool {
        !self.reshapes()
            && self.format.is_none()
            && self.compression.is_none()
            && self.fit_under.is_none()
    }

    /// Whether the pixels change size: the raster path.
    pub fn reshapes(&self) -> bool {
        self.aspect.is_some() || self.size.is_some() || self.scale.is_some()
    }

    pub fn merge(&mut self, patch: EditsPatch) {
        if let Some(aspect) = patch.aspect {
            self.aspect = aspect.filter(|&(w, h)| w > 0 && h > 0);
        }
        if let Some(framing) = patch.framing {
            self.framing = framing;
        }
        if let Some(size) = patch.size {
            self.size = size.and_then(Size::normalised);
            if self.size.is_some() {
                self.scale = None;
            }
        }
        if let Some(scale) = patch.scale {
            self.scale = scale.map(|s| s.clamp(10, 90));
            if self.scale.is_some() {
                self.size = None;
            }
        }
        if let Some(format) = patch.format {
            self.format = format;
        }
        if let Some(compression) = patch.compression {
            self.compression = compression.map(|c| c.clamp(5, 100));
            if self.compression.is_some() {
                self.fit_under = None;
            }
        }
        if let Some(fit_under) = patch.fit_under {
            self.fit_under = fit_under.filter(|&bytes| bytes > 0);
            if self.fit_under.is_some() {
                self.compression = None;
            }
        }
        if let Some(level) = patch.level {
            self.level = level;
        }
    }
}

/// What a card sends: an absent field is left alone, `null` clears it, a
/// value sets it.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct EditsPatch {
    #[serde(deserialize_with = "present")]
    pub aspect: Option<Option<(u32, u32)>>,
    pub framing: Option<Framing>,
    #[serde(deserialize_with = "present")]
    pub size: Option<Option<Size>>,
    #[serde(deserialize_with = "present")]
    pub scale: Option<Option<u8>>,
    #[serde(deserialize_with = "present")]
    pub format: Option<Option<Format>>,
    #[serde(deserialize_with = "present")]
    pub compression: Option<Option<u8>>,
    #[serde(deserialize_with = "present")]
    pub fit_under: Option<Option<u64>>,
    pub level: Option<Level>,
}

/// A field that is in the JSON: `null` is `Some(None)`. Absent fields never
/// get here; `#[serde(default)]` leaves them `None`.
fn present<'de, D, T>(d: D) -> Result<Option<Option<T>>, D::Error>
where
    D: Deserializer<'de>,
    T: Deserialize<'de>,
{
    Option::<T>::deserialize(d).map(Some)
}
```

`u8` fields deserialize values over 255 as an error; the card never sends them. `scale: 5` and `compression: 200` in the clamp test are in range for `u8`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ferry-core edit::tests && cargo test -p ferry-core`
Expected: PASS (all 7 new tests; the existing suite still green).

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/edit/mod.rs crates/ferry-core/src/lib.rs crates/ferry-core/src/sniff.rs crates/ferry-core/src/optimise/mod.rs
git commit -m "feat(core): edit recipe with patch merging

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Crop window and output size

**Files:**
- Create: `crates/ferry-core/src/edit/geometry.rs`
- Modify: `crates/ferry-core/src/edit/mod.rs` (add `pub mod geometry;` under the `//!` comment)

**Interfaces:**
- Consumes: `Edits`, `Size` (Task 1).
- Produces:
  - `geometry::Rect { x: u32, y: u32, width: u32, height: u32 }` (`Debug, Clone, Copy, PartialEq, Eq`).
  - `geometry::fit_ratio(dims: (u32,u32), ratio: (u32,u32)) -> (u32,u32)`.
  - `geometry::crop_size(dims: (u32,u32), edits: &Edits) -> (u32,u32)` — an `Exact` size's ratio wins over `aspect`.
  - `geometry::centre(dims: (u32,u32), window: (u32,u32)) -> Rect`.
  - `geometry::output_size(window: (u32,u32), edits: &Edits) -> (u32,u32)`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/edit/geometry.rs`:

```rust
//! Pixel maths for an edit: the window an aspect crop cuts and the size the
//! result ends up. Pure functions, so every rule is pinned here.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::{Edits, Size};

    fn with(f: impl FnOnce(&mut Edits)) -> Edits {
        let mut edits = Edits::default();
        f(&mut edits);
        edits
    }

    #[test]
    fn every_aspect_preset_fits_landscape_and_portrait() {
        let landscape = (2880, 1800);
        assert_eq!(fit_ratio(landscape, (16, 9)), (2880, 1620));
        assert_eq!(fit_ratio(landscape, (4, 3)), (2400, 1800));
        assert_eq!(fit_ratio(landscape, (1, 1)), (1800, 1800));
        assert_eq!(fit_ratio(landscape, (191, 100)), (2880, 1508));
        assert_eq!(fit_ratio(landscape, (9, 16)), (1013, 1800));
        let portrait = (1000, 2000);
        assert_eq!(fit_ratio(portrait, (16, 9)), (1000, 563));
        assert_eq!(fit_ratio(portrait, (9, 16)), (1000, 1778));
    }

    #[test]
    fn an_exact_size_cuts_its_own_ratio_and_wins_over_the_aspect() {
        let edits = with(|e| {
            e.aspect = Some((1, 1));
            e.size = Some(Size::Exact { width: 1200, height: 628 });
        });
        assert_eq!(crop_size((2880, 1800), &edits), fit_ratio((2880, 1800), (1200, 628)));
        assert_eq!(crop_size((2880, 1800), &Edits::default()), (2880, 1800));
    }

    #[test]
    fn the_centre_window_is_centred() {
        assert_eq!(
            centre((2880, 1800), (1800, 1800)),
            Rect { x: 540, y: 0, width: 1800, height: 1800 }
        );
    }

    #[test]
    fn sizes_and_scales_from_the_window() {
        let window = (2880, 1800);
        let out = |f: fn(&mut Edits)| output_size(window, &with(f));
        assert_eq!(out(|e| e.scale = Some(50)), (1440, 900));
        assert_eq!(out(|e| e.size = Some(Size::Width { width: 1440 })), (1440, 900));
        assert_eq!(out(|e| e.size = Some(Size::Height { height: 900 })), (1440, 900));
        assert_eq!(out(|e| e.size = Some(Size::LongEdge { edge: 1440 })), (1440, 900));
        assert_eq!(output_size((1800, 2880), &with(|e| e.size = Some(Size::LongEdge { edge: 1440 }))), (900, 1440));
        assert_eq!(out(|e| e.size = Some(Size::Exact { width: 1200, height: 750 })), (1200, 750));
        assert_eq!(out(|_| {}), window);
    }

    #[test]
    fn nothing_is_ever_upscaled() {
        let window = (2880, 1800);
        let out = |f: fn(&mut Edits)| output_size(window, &with(f));
        assert_eq!(out(|e| e.size = Some(Size::Width { width: 5000 })), window);
        assert_eq!(out(|e| e.size = Some(Size::LongEdge { edge: 9000 })), window);
        assert_eq!(out(|e| e.size = Some(Size::Exact { width: 5760, height: 3600 })), window);
    }

    #[test]
    fn tiny_and_thin_pictures_never_reach_zero() {
        assert_eq!(output_size((1, 400), &with(|e| e.scale = Some(10))), (1, 40));
        assert_eq!(output_size((3, 3), &with(|e| e.scale = Some(10))), (1, 1));
        assert_eq!(fit_ratio((1, 400), (16, 9)), (1, 1));
        assert_eq!(fit_ratio((400, 1), (9, 16)), (1, 1));
        assert_eq!(output_size((2000, 1), &with(|e| e.size = Some(Size::Width { width: 10 }))), (10, 1));
    }
}
```

Add `pub mod geometry;` to `crates/ferry-core/src/edit/mod.rs` right after the `//!` comment.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core edit::geometry`
Expected: FAIL to compile — `cannot find function fit_ratio`, `crop_size`, `centre`, `output_size`, `Rect`.

- [ ] **Step 3: Implement**

Insert above the tests module:

```rust
use super::{Edits, Size};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// The largest window of `ratio` that fits in `dims`, rounded, at least 1×1.
pub fn fit_ratio((w, h): (u32, u32), (rw, rh): (u32, u32)) -> (u32, u32) {
    let (w64, h64, rw, rh) = (u64::from(w), u64::from(h), u64::from(rw), u64::from(rh));
    if w64 * rh > h64 * rw {
        // Wider than the ratio: keep the height.
        (((h64 * rw + rh / 2) / rh).clamp(1, w64) as u32, h)
    } else {
        (w, ((w64 * rh + rw / 2) / rw).clamp(1, h64) as u32)
    }
}

/// The window cut from the (oriented) original: an `Exact` size's ratio, or
/// else the aspect, or else the whole picture.
pub fn crop_size(dims: (u32, u32), edits: &Edits) -> (u32, u32) {
    let ratio = match edits.size {
        Some(Size::Exact { width, height }) => Some((width, height)),
        _ => edits.aspect,
    };
    ratio.map_or(dims, |ratio| fit_ratio(dims, ratio))
}

pub fn centre((w, h): (u32, u32), (cw, ch): (u32, u32)) -> Rect {
    Rect {
        x: (w - cw) / 2,
        y: (h - ch) / 2,
        width: cw,
        height: ch,
    }
}

/// The result's size from the cut `window`; never larger than the window.
pub fn output_size((cw, ch): (u32, u32), edits: &Edits) -> (u32, u32) {
    let by_width = |w: u32| {
        let w = w.min(cw);
        (w, scaled(ch, w, cw))
    };
    let by_height = |h: u32| {
        let h = h.min(ch);
        (scaled(cw, h, ch), h)
    };
    match (edits.size, edits.scale) {
        (Some(Size::Exact { width, height }), _) => {
            if width >= cw || height >= ch {
                (cw, ch)
            } else {
                (width, height)
            }
        }
        (Some(Size::Width { width }), _) => by_width(width),
        (Some(Size::Height { height }), _) => by_height(height),
        (Some(Size::LongEdge { edge }), _) if cw >= ch => by_width(edge),
        (Some(Size::LongEdge { edge }), _) => by_height(edge),
        (None, Some(percent)) => (
            scaled(cw, u32::from(percent), 100),
            scaled(ch, u32::from(percent), 100),
        ),
        (None, None) => (cw, ch),
    }
}

/// `side * num / den`, rounded, at least 1.
fn scaled(side: u32, num: u32, den: u32) -> u32 {
    let (side, num, den) = (u64::from(side), u64::from(num), u64::from(den));
    ((side * num + den / 2) / den).max(1) as u32
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core edit::geometry`
Expected: PASS (6 tests). If an expected number in `every_aspect_preset_fits_landscape_and_portrait` is off by one, recompute it by hand with the rounding in `fit_ratio` (`(a*b + c/2) / c`) and fix the test only if the hand result matches the code; otherwise fix the code.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/edit/geometry.rs crates/ferry-core/src/edit/mod.rs
git commit -m "feat(core): crop window and output size maths

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: Smart framing

**Files:**
- Create: `crates/ferry-core/src/edit/smart.rs`
- Modify: `crates/ferry-core/src/edit/mod.rs` (`pub mod smart;`)

**Interfaces:**
- Consumes: `geometry::{Rect, centre}` (Task 2).
- Produces: `smart::frame(rgba: &[u8], width: u32, height: u32, window: (u32,u32)) -> Rect` — `rgba` is straight RGBA, `window` fits inside `width × height`; the result always fits.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/edit/smart.rs`:

```rust
//! Smart framing (spec §4.2): put the crop window over the busiest part of
//! the picture, judged by edge energy (Sobel) on a small grey copy. Ties go
//! to the window nearest the centre, so a flat picture is cut from the centre.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::edit::geometry::centre;

    /// A white `w`×`h` picture; `busy` paints a black-and-white checkerboard.
    fn picture(w: u32, h: u32, busy: Option<Rect>) -> Vec<u8> {
        let mut px = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h {
            for x in 0..w {
                let inside = busy.is_some_and(|b| {
                    (b.x..b.x + b.width).contains(&x) && (b.y..b.y + b.height).contains(&y)
                });
                let v = if inside && (x / 3 + y / 3) % 2 == 0 { 0 } else { 255 };
                px.extend_from_slice(&[v, v, v, 255]);
            }
        }
        px
    }

    #[test]
    fn a_busy_block_pulls_the_window_to_it() {
        let block = Rect { x: 300, y: 70, width: 60, height: 60 };
        let rect = frame(&picture(400, 200, Some(block)), 400, 200, (150, 150));
        assert!(rect.x <= 300 && rect.x + 150 >= 360, "{rect:?}");
        assert!(rect.y <= 70 && rect.y + 150 >= 130, "{rect:?}");
    }

    #[test]
    fn a_flat_picture_is_cut_from_the_centre() {
        assert_eq!(
            frame(&picture(400, 200, None), 400, 200, (150, 150)),
            centre((400, 200), (150, 150))
        );
    }

    #[test]
    fn the_window_always_fits_large_odd_pictures() {
        let (w, h) = (1333, 777);
        let block = Rect { x: 1200, y: 600, width: 120, height: 120 };
        let rect = frame(&picture(w, h, Some(block)), w, h, (437, 777));
        assert!(rect.x + rect.width <= w && rect.y + rect.height <= h, "{rect:?}");
        assert_eq!((rect.width, rect.height), (437, 777));
    }

    #[test]
    fn a_window_as_large_as_the_picture_is_the_whole_picture() {
        assert_eq!(
            frame(&picture(3, 3, None), 3, 3, (3, 3)),
            Rect { x: 0, y: 0, width: 3, height: 3 }
        );
        assert_eq!(frame(&picture(1, 50, None), 1, 50, (1, 1)).width, 1);
    }
}
```

Add `pub mod smart;` to `crates/ferry-core/src/edit/mod.rs` under `pub mod geometry;`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core edit::smart`
Expected: FAIL to compile — `cannot find function frame`.

- [ ] **Step 3: Implement**

Insert above the tests module:

```rust
use super::geometry::Rect;

/// Long edge of the grey copy the search runs on.
const PROBE: u32 = 256;

/// Where to cut `window` from the `width`×`height` RGBA picture.
pub fn frame(rgba: &[u8], width: u32, height: u32, window: (u32, u32)) -> Rect {
    let step = width.max(height).div_ceil(PROBE).max(1);
    let (sw, sh) = (width.div_ceil(step), height.div_ceil(step));
    let grey = shrink_grey(rgba, width, height, step, sw);
    let sums = integral(&sobel(&grey, sw, sh), sw, sh);
    let (ww, wh) = ((window.0 / step).clamp(1, sw), (window.1 / step).clamp(1, sh));
    // Twice the centred origin, so distances stay in integers.
    let (cx, cy) = (i64::from(sw - ww), i64::from(sh - wh));
    let mut best = (0_u64, u64::MAX, 0_u32, 0_u32); // energy, distance², x, y
    for y in 0..=sh - wh {
        for x in 0..=sw - ww {
            let energy = window_sum(&sums, sw, x, y, ww, wh);
            let dx = 2 * i64::from(x) - cx;
            let dy = 2 * i64::from(y) - cy;
            let distance = (dx * dx + dy * dy) as u64;
            if energy > best.0 || (energy == best.0 && distance < best.1) {
                best = (energy, distance, x, y);
            }
        }
    }
    Rect {
        x: (best.2 * step).min(width - window.0),
        y: (best.3 * step).min(height - window.1),
        width: window.0,
        height: window.1,
    }
}

/// Mean luma (alpha-weighted) of each `step`×`step` block.
fn shrink_grey(rgba: &[u8], width: u32, height: u32, step: u32, sw: u32) -> Vec<u32> {
    let sh = height.div_ceil(step);
    let mut sum = vec![0_u32; (sw * sh) as usize];
    let mut count = vec![0_u32; (sw * sh) as usize];
    for (i, p) in rgba.as_chunks::<4>().0.iter().enumerate() {
        let (x, y) = (i as u32 % width, i as u32 / width);
        let luma = (299 * u32::from(p[0]) + 587 * u32::from(p[1]) + 114 * u32::from(p[2])) / 1000;
        let cell = ((y / step) * sw + x / step) as usize;
        sum[cell] += luma * u32::from(p[3]) / 255;
        count[cell] += 1;
    }
    sum.iter().zip(count).map(|(s, c)| s / c.max(1)).collect()
}

/// |Gx| + |Gy| for inner cells; the border stays 0.
fn sobel(grey: &[u32], w: u32, h: u32) -> Vec<u64> {
    let mut out = vec![0_u64; (w * h) as usize];
    let at = |x: u32, y: u32| i64::from(grey[(y * w + x) as usize]);
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let gx = at(x + 1, y - 1) + 2 * at(x + 1, y) + at(x + 1, y + 1)
                - at(x - 1, y - 1) - 2 * at(x - 1, y) - at(x - 1, y + 1);
            let gy = at(x - 1, y + 1) + 2 * at(x, y + 1) + at(x + 1, y + 1)
                - at(x - 1, y - 1) - 2 * at(x, y - 1) - at(x + 1, y - 1);
            out[(y * w + x) as usize] = gx.unsigned_abs() + gy.unsigned_abs();
        }
    }
    out
}

/// Summed-area table, one row and column larger than the grid.
fn integral(values: &[u64], w: u32, h: u32) -> Vec<u64> {
    let stride = (w + 1) as usize;
    let mut sums = vec![0_u64; stride * (h + 1) as usize];
    for y in 0..h as usize {
        let mut row = 0;
        for x in 0..w as usize {
            row += values[y * w as usize + x];
            sums[(y + 1) * stride + x + 1] = sums[y * stride + x + 1] + row;
        }
    }
    sums
}

fn window_sum(sums: &[u64], w: u32, x: u32, y: u32, ww: u32, wh: u32) -> u64 {
    let stride = (w + 1) as usize;
    let (x0, y0, x1, y1) = (x as usize, y as usize, (x + ww) as usize, (y + wh) as usize);
    sums[y1 * stride + x1] + sums[y0 * stride + x0] - sums[y0 * stride + x1] - sums[y1 * stride + x0]
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core edit::smart`
Expected: PASS (4 tests).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/edit/smart.rs crates/ferry-core/src/edit/mod.rs
git commit -m "feat(core): smart framing by edge energy

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: Pixels in and out (`raster`)

**Files:**
- Modify: `crates/ferry-core/Cargo.toml` (`fast_image_resize = "6.1"`, `gif = "0.14"`)
- Modify: `crates/ferry-core/src/optimise/mod.rs` (`pub(crate) mod jpeg;`)
- Modify: `crates/ferry-core/src/optimise/png.rs` (split `pack`, add `decode_raster`, `encode_raster`, make `MAX_PIXELS` `pub(crate)`)
- Modify: `crates/ferry-core/src/optimise/jpeg.rs` (add `orientation`, `decode_raster`, `encode_raster`, `icc_segments`)
- Modify: `crates/ferry-core/src/optimise/webp.rs` (split `config`, add `decode_raster`, `encode_raster`)
- Create: `crates/ferry-core/src/edit/raster.rs`
- Modify: `crates/ferry-core/src/edit/mod.rs` (`pub mod raster;`)

**Interfaces:**
- Consumes: `geometry::Rect` (Task 2); `OptimiseOptions`, `OptimiseError`, `quality()`.
- Produces:
  - `raster::Raster { width: u32, height: u32, rgba: Vec<u8> /* straight RGBA, top row first */, icc: Option<Vec<u8>> }` (`Debug, Clone, PartialEq, Eq`).
  - `raster::decode(bytes: &[u8], format: Format) -> Result<Raster, OptimiseError>` — JPEG orientation applied; GIF = first frame.
  - `raster::orient(r: Raster, orientation: u16) -> Raster`.
  - `raster::cut(r: &Raster, crop: Rect, out: (u32,u32)) -> Result<Raster, OptimiseError>`.
  - `raster::encode(r: &Raster, format: Format, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>` — PNG, JPEG, WebP; GIF is `Unsupported`.
  - `raster::display_dims(bytes: &[u8], format: Format) -> Option<(u32,u32)>` — header size, swapped for JPEG orientations 5–8.
  - `optimise::jpeg::orientation(data: &[u8]) -> u16` (1 when absent).

- [ ] **Step 1: Add the dependencies**

In `crates/ferry-core/Cargo.toml` `[dependencies]`, keeping alphabetical order:

```toml
fast_image_resize = "6.1"
gif = "0.14"
```

Run: `cargo build -p ferry-core`
Expected: builds (both crates are pure Rust).

- [ ] **Step 2: Write the failing tests**

Create `crates/ferry-core/src/edit/raster.rs`:

```rust
//! Pixels in and out for edits (spec §4.2): decode any supported still image
//! to RGBA with its orientation applied and its ICC profile kept, cut and
//! resize it, and encode RGBA through the same encoders `optimise` uses.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::optimise::OptimiseOptions;
    use crate::optimise::webp::Extras;
    use crate::sniff::{dimensions, sniff};
    use crate::testutil;

    fn raster(w: u32, h: u32, px: impl Fn(u32, u32) -> [u8; 4]) -> Raster {
        let rgba = (0..h).flat_map(|y| (0..w).flat_map(move |x| px(x, y))).collect();
        Raster { width: w, height: h, rgba, icc: None }
    }

    /// A 16-bit RGB PNG, which the decoder must narrow to 8 bits.
    fn png16(w: u32, h: u32) -> Vec<u8> {
        let mut out = Vec::new();
        let mut enc = png::Encoder::new(&mut out, w, h);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Sixteen);
        let mut writer = enc.write_header().unwrap();
        writer.write_image_data(&vec![0x80; (w * h * 6) as usize]).unwrap();
        writer.finish().unwrap();
        out
    }

    /// A JPEG whose left half is black and right half white, tagged with `orientation`.
    fn half_dark_jpeg(w: u32, h: u32, orientation: u16) -> Vec<u8> {
        let rgb: Vec<u8> = (0..h)
            .flat_map(|_| (0..w).flat_map(move |x| if x < w / 2 { [0; 3] } else { [255; 3] }))
            .collect();
        let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
        enc.set_size(w as usize, h as usize);
        enc.set_quality(95.0);
        let mut enc = enc.start_compress(Vec::new()).unwrap();
        enc.write_marker(mozjpeg::Marker::APP(1), &testutil::exif_with_orientation(orientation, 0));
        enc.write_scanlines(&rgb).unwrap();
        enc.finish().unwrap()
    }

    fn pixel(r: &Raster, x: u32, y: u32) -> [u8; 4] {
        let i = ((y * r.width + x) * 4) as usize;
        r.rgba[i..i + 4].try_into().unwrap()
    }

    #[test]
    fn every_still_format_round_trips_with_its_size() {
        let o = OptimiseOptions::default();
        let sources = [
            (Format::Png, testutil::png(120, 80), (120, 80)),
            (Format::Png, png16(40, 30), (40, 30)),
            (Format::Jpeg, testutil::jpeg(120, 80, 90, &testutil::JpegExtras::default()), (120, 80)),
            (Format::Jpeg, testutil::jpeg_gray(120, 80, 90), (120, 80)),
            (Format::WebP, testutil::webp(120, 80, 90, &Extras::default()), (120, 80)),
            (Format::Gif, testutil::gif(3), (64, 48)),
        ];
        for (format, bytes, size) in sources {
            let r = decode(&bytes, format).unwrap();
            assert_eq!((r.width, r.height), size, "{format:?}");
            for target in [Format::Png, Format::Jpeg, Format::WebP] {
                let out = encode(&r, target, &o).unwrap();
                assert_eq!(sniff(&out), Some(target), "{format:?} → {target:?}");
                assert_eq!(dimensions(&out, target), Some(size), "{format:?} → {target:?}");
            }
        }
    }

    #[test]
    fn transparency_becomes_white_in_a_jpeg() {
        let clear = raster(16, 16, |_, _| [0, 0, 0, 0]);
        let jpeg = encode(&clear, Format::Jpeg, &OptimiseOptions::default()).unwrap();
        let back = decode(&jpeg, Format::Jpeg).unwrap();
        assert!(pixel(&back, 8, 8)[..3].iter().all(|&c| c >= 250), "{:?}", pixel(&back, 8, 8));
    }

    #[test]
    fn the_icc_profile_survives_every_target() {
        let profile: Vec<u8> = (0..600_u32).map(|i| (i % 251) as u8).collect();
        let mut r = raster(32, 32, |x, y| [x as u8 * 8, y as u8 * 8, 90, 255]);
        r.icc = Some(profile.clone());
        for target in [Format::Png, Format::Jpeg, Format::WebP] {
            let out = encode(&r, target, &OptimiseOptions::default()).unwrap();
            assert_eq!(decode(&out, target).unwrap().icc, Some(profile.clone()), "{target:?}");
        }
    }

    #[test]
    fn a_long_icc_profile_is_split_across_app2_segments() {
        let profile = vec![7_u8; 70_000];
        let mut r = raster(8, 8, |_, _| [10, 20, 30, 255]);
        r.icc = Some(profile.clone());
        let jpeg = encode(&r, Format::Jpeg, &OptimiseOptions::default()).unwrap();
        assert_eq!(decode(&jpeg, Format::Jpeg).unwrap().icc, Some(profile));
    }

    #[test]
    fn jpeg_orientation_is_applied_to_the_pixels() {
        // Orientation 6 means "rotate 90° clockwise to display": the dark left
        // half of the stored picture becomes the top half.
        let r = decode(&half_dark_jpeg(40, 20, 6), Format::Jpeg).unwrap();
        assert_eq!((r.width, r.height), (20, 40));
        assert!(pixel(&r, 10, 5)[0] < 60, "top is dark");
        assert!(pixel(&r, 10, 35)[0] > 200, "bottom is light");
        assert_eq!(display_dims(&half_dark_jpeg(40, 20, 6), Format::Jpeg), Some((20, 40)));
        // Re-encoded, it is upright and carries no orientation.
        let out = encode(&r, Format::Jpeg, &OptimiseOptions::default()).unwrap();
        assert_eq!(jpeg::orientation(&out), 1);
    }

    #[test]
    fn every_orientation_maps_the_corners() {
        // A 3×2 picture whose top-left pixel is red; orientations move it.
        let r = raster(3, 2, |x, y| if (x, y) == (0, 0) { [255, 0, 0, 255] } else { [0, 0, 0, 255] });
        let red = |r: &Raster| {
            (0..r.height)
                .flat_map(|y| (0..r.width).map(move |x| (x, y)))
                .find(|&(x, y)| pixel(r, x, y)[0] == 255)
                .unwrap()
        };
        let cases = [(1, (3, 2), (0, 0)), (2, (3, 2), (2, 0)), (3, (3, 2), (2, 1)), (4, (3, 2), (0, 1)),
                     (5, (2, 3), (0, 0)), (6, (2, 3), (1, 0)), (7, (2, 3), (1, 2)), (8, (2, 3), (0, 2))];
        for (orientation, size, at) in cases {
            let turned = orient(r.clone(), orientation);
            assert_eq!(((turned.width, turned.height), red(&turned)), (size, at), "orientation {orientation}");
        }
    }

    #[test]
    fn a_plain_crop_copies_pixels_without_resampling() {
        let r = raster(4, 2, |x, y| [x as u8, y as u8, 0, 255]);
        let cut = cut(&r, Rect { x: 1, y: 0, width: 2, height: 2 }, (2, 2)).unwrap();
        assert_eq!(cut.rgba, vec![1, 0, 0, 255, 2, 0, 0, 255, 1, 1, 0, 255, 2, 1, 0, 255]);
    }

    #[test]
    fn a_cut_can_also_resize() {
        let r = raster(100, 50, |x, _| [x as u8, 0, 0, 255]);
        let out = cut(&r, Rect { x: 0, y: 0, width: 100, height: 50 }, (50, 25)).unwrap();
        assert_eq!((out.width, out.height, out.rgba.len()), (50, 25, 50 * 25 * 4));
    }

    #[test]
    fn a_header_claiming_a_huge_picture_is_refused_before_decoding() {
        assert!(matches!(
            decode(&testutil::png_claiming_size(20_000, 20_000), Format::Png),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    #[test]
    fn gif_output_is_left_to_gifsicle() {
        let r = raster(2, 2, |_, _| [0, 0, 0, 255]);
        assert!(matches!(
            encode(&r, Format::Gif, &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }
}
```

Add `pub mod raster;` to `crates/ferry-core/src/edit/mod.rs` (alphabetical: geometry, raster, smart).

- [ ] **Step 3: Run the tests to verify they fail**

Run: `cargo test -p ferry-core edit::raster`
Expected: FAIL to compile — `cannot find type Raster`, functions `decode`, `encode`, `orient`, `cut`, `display_dims`.

- [ ] **Step 4: Codec hooks — PNG**

In `crates/ferry-core/src/optimise/png.rs`: make `MAX_PIXELS` `pub(crate) const`, add `use crate::edit::raster::Raster;`, and replace `optimise` with a decode-then-`pack` split:

```rust
pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let (width, height, rgba) = decode(input)?;
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    pack(width, height, rgba, kept_chunks(input, o.strip_metadata), None, o)
}

/// Any PNG as straight RGBA, with its ICC profile.
pub(crate) fn decode_raster(input: &[u8]) -> Result<Raster, OptimiseError> {
    let (width, height, pixels) = decode(input)?;
    let icc = png::Decoder::new(Cursor::new(input))
        .read_info()
        .ok()
        .and_then(|reader| reader.info().icc_profile.as_ref().map(|p| p.to_vec()));
    let rgba = pixels.iter().flat_map(|p| [p.r, p.g, p.b, p.a]).collect();
    Ok(Raster { width, height, rgba, icc })
}

/// Edited pixels as an optimised PNG.
pub(crate) fn encode_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let rgba = r
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
        .collect();
    pack(r.width, r.height, rgba, Vec::new(), r.icc.as_deref(), o)
}

/// Quantise with imagequant, then write with oxipng, adding `chunks` and the ICC profile.
fn pack(
    width: u32,
    height: u32,
    rgba: Vec<RGBA>,
    chunks: Vec<([u8; 4], Vec<u8>)>,
    icc: Option<&[u8]>,
    o: &OptimiseOptions,
) -> Result<Vec<u8>, OptimiseError> {
    let mut attr = imagequant::new();
    let (speed, preset) = effort(o);
    attr.set_speed(speed).map_err(encode_err)?;
    attr.set_quality(0, quality(o)).map_err(encode_err)?;
    let mut image = attr
        .new_image(rgba, width as usize, height as usize, 0.0)
        .map_err(encode_err)?;
    let mut quantised = attr.quantize(&mut image).map_err(encode_err)?;
    quantised.set_dithering_level(1.0).map_err(encode_err)?;
    let (palette, indexed) = quantised.remapped(&mut image).map_err(encode_err)?;
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }

    let palette = palette
        .iter()
        .map(|c| RGBA8::new(c.r, c.g, c.b, c.a))
        .collect();
    let mut raw = RawImage::new(
        width,
        height,
        ColorType::Indexed { palette },
        BitDepth::Eight,
        indexed,
    )
    .map_err(encode_err)?;
    for (name, data) in chunks {
        raw.add_png_chunk(name, data);
    }
    if let Some(icc) = icc {
        raw.add_icc_profile(icc);
    }
    raw.create_optimized_png(&Options::from_preset(preset))
        .map_err(encode_err)
}
```

Run: `cargo test -p ferry-core optimise::png`
Expected: PASS (the refactor changes nothing for `optimise`).

- [ ] **Step 5: Codec hooks — JPEG**

In `crates/ferry-core/src/optimise/mod.rs`, change `mod jpeg;` to `pub(crate) mod jpeg;`. In `crates/ferry-core/src/optimise/jpeg.rs`, add `use crate::edit::raster::Raster;` and these functions (after `run`):

```rust
/// The EXIF Orientation of a JPEG, read from its APP1 segments; 1 when absent.
pub(crate) fn orientation(data: &[u8]) -> u16 {
    let mut pos = 2; // past SOI
    while let (Some(&0xFF), Some(&marker)) = (data.get(pos), data.get(pos + 1)) {
        match marker {
            0xFF => pos += 1,
            0x01 | 0xD0..=0xD8 => pos += 2,
            0xD9 | 0xDA => break,
            _ => {
                let Some(len) = data.get(pos + 2..pos + 4) else {
                    break;
                };
                let len = usize::from(u16::from_be_bytes([len[0], len[1]]));
                if marker == 0xE1
                    && let Some(found) = data.get(pos + 4..pos + 2 + len).and_then(exif_orientation)
                {
                    return found;
                }
                pos += 2 + len;
            }
        }
    }
    1
}

/// The stored pixels as straight RGBA (not yet oriented), the ICC profile
/// reassembled from its APP2 segments, and the EXIF orientation.
pub(crate) fn decode_raster(input: &[u8]) -> Result<(Raster, u16), OptimiseError> {
    if !reaches_eoi(input) {
        return Err(OptimiseError::Decode("the JPEG is incomplete".into()));
    }
    let raster = catch_unwind(AssertUnwindSafe(|| read_raster(input)))
        .map_err(|_| OptimiseError::Decode("the JPEG is corrupt".into()))??;
    Ok((raster, orientation(input)))
}

fn read_raster(input: &[u8]) -> Result<Raster, OptimiseError> {
    let decode_err = |e: std::io::Error| OptimiseError::Decode(e.to_string());
    let decoder = Decompress::with_err(strict_error_mgr())
        .with_markers(&[Marker::APP(2)])
        .from_mem(input)
        .map_err(decode_err)?;
    let (width, height) = decoder.size();
    let mut parts: Vec<(u8, Vec<u8>)> = decoder
        .markers()
        .filter(|m| m.data.starts_with(ICC_PREFIX) && m.data.len() > ICC_PREFIX.len() + 2)
        .map(|m| (m.data[ICC_PREFIX.len()], m.data[ICC_PREFIX.len() + 2..].to_vec()))
        .collect();
    parts.sort_by_key(|(sequence, _)| *sequence);
    let icc = (!parts.is_empty()).then(|| parts.into_iter().flat_map(|(_, data)| data).collect());
    let rgba: Vec<u8> = match decoder.color_space() {
        ColorSpace::JCS_GRAYSCALE => decoder
            .grayscale()
            .and_then(|mut d| d.read_scanlines::<u8>())
            .map_err(decode_err)?
            .into_iter()
            .flat_map(|v| [v, v, v, 255])
            .collect(),
        ColorSpace::JCS_CMYK | ColorSpace::JCS_YCCK => {
            return Err(OptimiseError::Unsupported("CMYK JPEG".into()));
        }
        _ => decoder
            .rgb()
            .and_then(|mut d| d.read_scanlines::<u8>())
            .map_err(decode_err)?
            .as_chunks::<3>()
            .0
            .iter()
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
    };
    Ok(Raster {
        width: width as u32,
        height: height as u32,
        rgba,
        icc,
    })
}

/// Edited pixels as a progressive JPEG: alpha flattened onto white, the ICC
/// profile in APP2 segments, no EXIF (the pixels are already upright).
pub(crate) fn encode_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    catch_unwind(AssertUnwindSafe(|| write_raster(r, o)))
        .map_err(|_| OptimiseError::Encode("mozjpeg failed".into()))?
}

fn write_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let encode_err = |e: std::io::Error| OptimiseError::Encode(e.to_string());
    let rgb: Vec<u8> = r
        .rgba
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| {
            let alpha = u32::from(p[3]);
            let over_white = |c: u8| ((u32::from(c) * alpha + 255 * (255 - alpha) + 127) / 255) as u8;
            [over_white(p[0]), over_white(p[1]), over_white(p[2])]
        })
        .collect();
    let mut encoder = Compress::new(ColorSpace::JCS_RGB);
    encoder.set_size(r.width as usize, r.height as usize);
    encoder.set_quality(f32::from(quality(o)));
    encoder.set_progressive_mode();
    encoder.set_optimize_coding(true);
    let mut encoder = encoder.start_compress(Vec::new()).map_err(encode_err)?;
    if let Some(icc) = &r.icc {
        for segment in icc_segments(icc) {
            encoder.write_marker(Marker::APP(2), &segment);
        }
    }
    encoder.write_scanlines(&rgb).map_err(encode_err)?;
    encoder.finish().map_err(encode_err)
}

/// An ICC profile as APP2 payloads: `ICC_PROFILE\0`, sequence number (from
/// 1), segment count, then at most 65 519 bytes each.
fn icc_segments(icc: &[u8]) -> Vec<Vec<u8>> {
    let parts: Vec<&[u8]> = icc.chunks(65_519).collect();
    parts
        .iter()
        .enumerate()
        .map(|(i, part)| {
            let mut segment = ICC_PREFIX.to_vec();
            segment.extend_from_slice(&[i as u8 + 1, parts.len() as u8]);
            segment.extend_from_slice(part);
            segment
        })
        .collect()
}
```

`exif_orientation` already exists in this file (returns `Option<u16>` from an APP1 payload); reuse it as is.

- [ ] **Step 6: Codec hooks — WebP**

In `crates/ferry-core/src/optimise/webp.rs`, add `use crate::edit::raster::Raster;`, pull the encoder config into a function, and add the raster hooks:

```rust
/// Lossy, method 4, alpha kept at full quality.
fn config(o: &OptimiseOptions) -> Result<WebPConfig, OptimiseError> {
    let mut config =
        WebPConfig::new().map_err(|()| OptimiseError::Encode("libwebp config".into()))?;
    config.lossless = 0;
    config.quality = f32::from(quality(o));
    config.method = 4;
    config.alpha_compression = 1;
    config.alpha_quality = 100;
    Ok(config)
}

/// A still WebP as straight RGBA, with its ICC profile.
pub(crate) fn decode_raster(input: &[u8]) -> Result<Raster, OptimiseError> {
    let image = Decoder::new(input)
        .decode()
        .ok_or_else(|| OptimiseError::Decode("not a still WebP".into()))?;
    let data: &[u8] = &image;
    let rgba = if image.is_alpha() {
        data.to_vec()
    } else {
        data.as_chunks::<3>().0.iter().flat_map(|p| [p[0], p[1], p[2], 255]).collect()
    };
    let icc = chunks(input)
        .iter()
        .find(|(name, _)| name == b"ICCP")
        .map(|(_, data)| data.to_vec());
    Ok(Raster { width: image.width(), height: image.height(), rgba, icc })
}

/// Edited pixels as a lossy WebP with the ICC profile.
pub(crate) fn encode_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let encoded = Encoder::from_rgba(&r.rgba, r.width, r.height)
        .encode_advanced(&config(o)?)
        .map_err(|e| OptimiseError::Encode(format!("{e:?}")))?;
    let alpha = r.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255);
    let extras = Extras { icc: r.icc.clone(), ..Extras::default() };
    Ok(assemble(&encoded, r.width, r.height, alpha, &extras))
}
```

and in `optimise` replace the inline `WebPConfig` block with `let config = config(o)?;` (keep the rest).

Run: `cargo test -p ferry-core optimise`
Expected: PASS (refactors only).

- [ ] **Step 7: Implement `raster`**

Insert above the tests module in `crates/ferry-core/src/edit/raster.rs`:

```rust
use fast_image_resize::images::{Image, ImageRef};
use fast_image_resize::{FilterType, PixelType, ResizeAlg, ResizeOptions, Resizer};

use super::geometry::Rect;
use crate::optimise::{OptimiseError, OptimiseOptions, jpeg, png, webp};
use crate::sniff::{self, Format};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Raster {
    pub width: u32,
    pub height: u32,
    /// Straight (not premultiplied) RGBA, top row first.
    pub rgba: Vec<u8>,
    pub icc: Option<Vec<u8>>,
}

/// A still picture as it is displayed: JPEG orientation applied, a GIF's
/// first frame. Headers claiming more than `png::MAX_PIXELS` are refused
/// before any buffer is allocated.
pub fn decode(bytes: &[u8], format: Format) -> Result<Raster, OptimiseError> {
    if let Some((w, h)) = sniff::dimensions(bytes, format)
        && u64::from(w) * u64::from(h) > png::MAX_PIXELS
    {
        return Err(OptimiseError::Unsupported(format!(
            "{w} x {h} pixels is too large to edit"
        )));
    }
    match format {
        Format::Png => png::decode_raster(bytes),
        Format::Jpeg => jpeg::decode_raster(bytes).map(|(r, o)| orient(r, o)),
        Format::WebP => webp::decode_raster(bytes),
        Format::Gif => first_gif_frame(bytes),
    }
}

/// The size a card shows for the original: the header's, turned for JPEG
/// orientations 5–8.
pub fn display_dims(bytes: &[u8], format: Format) -> Option<(u32, u32)> {
    let (w, h) = sniff::dimensions(bytes, format)?;
    let turned = format == Format::Jpeg && matches!(jpeg::orientation(bytes), 5..=8);
    Some(if turned { (h, w) } else { (w, h) })
}

/// Apply an EXIF orientation (1–8) to the pixels.
pub fn orient(r: Raster, orientation: u16) -> Raster {
    if !(2..=8).contains(&orientation) {
        return r;
    }
    let (w, h) = (r.width, r.height);
    let (ow, oh) = if orientation >= 5 { (h, w) } else { (w, h) };
    let mut out = vec![0; r.rgba.len()];
    for dy in 0..oh {
        for dx in 0..ow {
            let (sx, sy) = match orientation {
                2 => (w - 1 - dx, dy),
                3 => (w - 1 - dx, h - 1 - dy),
                4 => (dx, h - 1 - dy),
                5 => (dy, dx),
                6 => (dy, h - 1 - dx),
                7 => (w - 1 - dy, h - 1 - dx),
                _ => (w - 1 - dy, dx),
            };
            let s = ((sy * w + sx) * 4) as usize;
            let d = ((dy * ow + dx) * 4) as usize;
            out[d..d + 4].copy_from_slice(&r.rgba[s..s + 4]);
        }
    }
    Raster { width: ow, height: oh, rgba: out, icc: r.icc }
}

/// Cut `crop` out of `r` and resize it to `out` (Lanczos3, alpha-aware). A
/// cut that keeps the size copies rows and never resamples.
pub fn cut(r: &Raster, crop: Rect, out: (u32, u32)) -> Result<Raster, OptimiseError> {
    if (crop.width, crop.height) == out {
        let row = (crop.width * 4) as usize;
        let mut rgba = Vec::with_capacity(row * crop.height as usize);
        for y in crop.y..crop.y + crop.height {
            let start = ((y * r.width + crop.x) * 4) as usize;
            rgba.extend_from_slice(&r.rgba[start..start + row]);
        }
        return Ok(Raster { width: out.0, height: out.1, rgba, icc: r.icc.clone() });
    }
    let resize_err = |e: &dyn std::fmt::Display| OptimiseError::Encode(format!("resize: {e}"));
    let src = ImageRef::new(r.width, r.height, &r.rgba, PixelType::U8x4).map_err(|e| resize_err(&e))?;
    let mut dst = Image::new(out.0, out.1, PixelType::U8x4);
    let options = ResizeOptions::new()
        .crop(
            f64::from(crop.x),
            f64::from(crop.y),
            f64::from(crop.width),
            f64::from(crop.height),
        )
        .resize_alg(ResizeAlg::Convolution(FilterType::Lanczos3));
    Resizer::new()
        .resize(&src, &mut dst, &options)
        .map_err(|e| resize_err(&e))?;
    Ok(Raster { width: out.0, height: out.1, rgba: dst.into_vec(), icc: r.icc.clone() })
}

/// Pixels as PNG, JPEG or WebP through the optimising encoders.
pub fn encode(r: &Raster, format: Format, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    match format {
        Format::Png => png::encode_raster(r, o),
        Format::Jpeg => jpeg::encode_raster(r, o),
        Format::WebP => webp::encode_raster(r, o),
        Format::Gif => Err(OptimiseError::Unsupported(
            "GIF output comes only from gifsicle".into(),
        )),
    }
}

/// The first frame of a GIF, drawn onto its transparent logical screen.
fn first_gif_frame(bytes: &[u8]) -> Result<Raster, OptimiseError> {
    let decode_err = |e: gif::DecodingError| OptimiseError::Decode(e.to_string());
    let mut options = gif::DecodeOptions::new();
    options.set_color_output(gif::ColorOutput::RGBA);
    let mut decoder = options.read_info(bytes).map_err(decode_err)?;
    let (width, height) = (u32::from(decoder.width()), u32::from(decoder.height()));
    let frame = decoder
        .read_next_frame()
        .map_err(decode_err)?
        .ok_or_else(|| OptimiseError::Decode("the GIF has no frames".into()))?;
    let (left, top) = (u32::from(frame.left), u32::from(frame.top));
    let (fw, fh) = (u32::from(frame.width), u32::from(frame.height));
    let cols = fw.min(width.saturating_sub(left));
    let mut rgba = vec![0; (width * height * 4) as usize];
    for row in 0..fh.min(height.saturating_sub(top)) {
        let src = (row * fw * 4) as usize;
        let dst = (((top + row) * width + left) * 4) as usize;
        rgba[dst..dst + (cols * 4) as usize].copy_from_slice(&frame.buffer[src..src + (cols * 4) as usize]);
    }
    Ok(Raster { width, height, rgba, icc: None })
}
```

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p ferry-core edit::raster && cargo test -p ferry-core`
Expected: PASS (10 new tests, existing suite green). If `the_icc_profile_survives_every_target` fails only for PNG because oxipng drops or rewrites the synthetic profile, record a ruling and switch the PNG path to writing the profile via `raw.add_png_chunk(*b"iCCP", …)` with a zlib-compressed payload built by `oxipng`'s re-exported deflater, or narrow the test to JPEG and WebP with a note — choose whichever keeps a real camera profile intact.

- [ ] **Step 9: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/Cargo.toml Cargo.lock crates/ferry-core/src/edit crates/ferry-core/src/optimise
git commit -m "feat(core): decode, orient, cut and encode pixels for edits

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Encoder-only runs and gifsicle crop/resize

**Files:**
- Modify: `crates/ferry-core/src/optimise/mod.rs` (`encode`; `pub(crate) mod gif;`)
- Modify: `crates/ferry-core/src/optimise/gif.rs` (`edit`, `edit_args`)

**Interfaces:**
- Consumes: `geometry::Rect` (Task 2).
- Produces:
  - `optimise::encode(input: &[u8], format: Format, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>` — the encoder's output whatever its size; `optimise()` = `encode` + `classify`.
  - `optimise::gif::edit(input: &[u8], crop: Option<Rect>, size: Option<(u32,u32)>, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>`.

- [ ] **Step 1: Write the failing tests**

Append to the tests module in `crates/ferry-core/src/optimise/mod.rs`:

```rust
    #[test]
    fn encode_returns_the_output_even_when_it_is_larger() {
        let tiny = testutil::png(1, 1);
        let out = encode(&tiny, Format::Png, &OptimiseOptions::default()).unwrap();
        assert!(out.len() > tiny.len());
        assert_eq!(crate::sniff::sniff(&out), Some(Format::Png));
    }
```

Append to the tests module in `crates/ferry-core/src/optimise/gif.rs`:

```rust
    #[test]
    fn crop_comes_before_resize_in_the_arguments() {
        let crop = Rect { x: 8, y: 4, width: 32, height: 24 };
        assert_eq!(
            edit_args(Some(crop), Some((16, 12))),
            ["--crop=8,4+32x24", "--resize=16x12"]
        );
        assert!(edit_args(None, None).is_empty());
    }

    #[test]
    fn an_animated_gif_is_cropped_and_resized_frame_by_frame() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let input = testutil::gif(3);
        let crop = Rect { x: 8, y: 8, width: 32, height: 24 };
        let out = edit(&input, Some(crop), Some((16, 12)), &o).unwrap();
        assert_eq!(gif_frame_count(&out), 3);
        assert_eq!(crate::sniff::dimensions(&out, crate::sniff::Format::Gif), Some((16, 12)));
    }
```

(`with_gifsicle` already exists in that tests module; add `use crate::edit::geometry::Rect;` at the top of `gif.rs`.)

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core optimise`
Expected: FAIL to compile — `cannot find function encode`, `edit_args`, `edit`.

- [ ] **Step 3: Implement**

In `crates/ferry-core/src/optimise/mod.rs`, change `mod gif;` to `pub(crate) mod gif;` and split `optimise`:

```rust
pub fn optimise(
    input: &[u8],
    format: Format,
    o: &OptimiseOptions,
) -> Result<Outcome, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    if crate::sniff::is_animated(input, format) && format != Format::Gif {
        return Ok(Outcome::AlreadyOptimal);
    }
    let output = encode(input, format, o)?;
    Ok(classify(input.len(), output))
}

/// The encoder's output for `input`, whatever its size; `optimise` grades it
/// and an explicit edit keeps it.
pub fn encode(input: &[u8], format: Format, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    let output = match format {
        Format::Png => png::optimise(input, o)?,
        Format::Jpeg => jpeg::optimise(input, o)?,
        Format::WebP => webp::optimise(input, o)?,
        Format::Gif => gif::optimise(input, o)?,
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    Ok(output)
}
```

In `crates/ferry-core/src/optimise/gif.rs`, replace `optimise` with:

```rust
pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    edit(input, None, None, o)
}

/// Crop, then resize, every frame, then optimise as usual. gifsicle applies
/// `--crop` before `--resize`, and keeps the frame count and timing.
pub fn edit(
    input: &[u8],
    crop: Option<Rect>,
    size: Option<(u32, u32)>,
    o: &OptimiseOptions,
) -> Result<Vec<u8>, OptimiseError> {
    if !gif_is_complete(input) {
        return Err(OptimiseError::Decode("the GIF is incomplete".into()));
    }
    let Some(gifsicle) = &o.gifsicle else {
        return Err(OptimiseError::Unsupported(
            "gifsicle missing — reinstall Ferry".into(),
        ));
    };
    let mut args = edit_args(crop, size);
    args.extend([
        "-O3".into(),
        format!("--lossy={}", lossy(quality(o))),
        "--colors".into(),
        "256".into(),
    ]);
    if o.strip_metadata {
        // gifsicle keeps comments and unknown extensions by default; the loop
        // count survives both flags.
        args.extend(["--no-comments".into(), "--no-extensions".into()]);
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    sidecar::run(gifsicle, &args, input, DEFAULT_TIMEOUT, &o.cancel).map_err(|e| match e {
        SidecarError::Cancelled => OptimiseError::Cancelled,
        SidecarError::Spawn(e) => {
            OptimiseError::Unsupported(format!("gifsicle could not start: {e}"))
        }
        other => OptimiseError::Encode(other.to_string()),
    })
}

fn edit_args(crop: Option<Rect>, size: Option<(u32, u32)>) -> Vec<String> {
    let mut args = Vec::new();
    if let Some(c) = crop {
        args.push(format!("--crop={},{}+{}x{}", c.x, c.y, c.width, c.height));
    }
    if let Some((w, h)) = size {
        args.push(format!("--resize={w}x{h}"));
    }
    args
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core optimise`
Expected: PASS, including every existing GIF test (with `FERRY_GIFSICLE` exported, the animated crop test runs; without it, it returns early).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/optimise
git commit -m "feat(core): encoder-only runs and gifsicle crop and resize

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Fit under size

**Files:**
- Create: `crates/ferry-core/src/edit/fit.rs`
- Modify: `crates/ferry-core/src/edit/mod.rs` (`pub mod fit;`)

**Interfaces:**
- Consumes: `OptimiseError`.
- Produces: `fit::Fitted { bytes: Vec<u8>, quality: u8, missed: bool }` and `fit::search(target: u64, encode: impl FnMut(u8) -> Result<Vec<u8>, OptimiseError>) -> Result<Fitted, OptimiseError>` — at most 7 encodes; `missed` when quality 5 is still over the target (its output is returned).

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/edit/fit.rs`:

```rust
//! Fit under size (R§2.5): the highest quality whose output is at most the
//! target, found with at most seven encodes.

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake encoder whose output grows by 1000 bytes per quality step.
    fn sized(calls: &mut Vec<u8>) -> impl FnMut(u8) -> Result<Vec<u8>, OptimiseError> + '_ {
        move |q| {
            calls.push(q);
            Ok(vec![0; usize::from(q) * 1000])
        }
    }

    #[test]
    fn it_finds_a_high_quality_under_the_target_in_seven_encodes() {
        let mut calls = Vec::new();
        let fitted = search(50_500, sized(&mut calls)).unwrap();
        assert!(!fitted.missed);
        assert!(fitted.bytes.len() <= 50_500);
        assert!(fitted.quality >= 47, "quality {}", fitted.quality);
        assert!(calls.len() <= 7, "{calls:?}");
        assert_eq!(calls[0], 5, "the floor is tried first");
    }

    #[test]
    fn exactly_the_target_fits() {
        let mut calls = Vec::new();
        let fitted = search(100_000, sized(&mut calls)).unwrap();
        assert_eq!((fitted.quality, fitted.bytes.len(), fitted.missed), (100, 100_000, false));
    }

    #[test]
    fn an_unreachable_target_returns_the_smallest_and_says_so() {
        let mut calls = Vec::new();
        let fitted = search(1_000, sized(&mut calls)).unwrap();
        assert_eq!((fitted.quality, fitted.bytes.len(), fitted.missed), (5, 5_000, true));
        assert_eq!(calls, [5]);
    }

    #[test]
    fn an_encoder_error_stops_the_search() {
        let result = search(1_000, |_| Err(OptimiseError::Cancelled));
        assert!(matches!(result, Err(OptimiseError::Cancelled)));
    }
}
```

Add `pub mod fit;` to `crates/ferry-core/src/edit/mod.rs` (alphabetical: fit, geometry, raster, smart).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core edit::fit`
Expected: FAIL to compile — `cannot find function search`.

- [ ] **Step 3: Implement**

Insert above the tests module:

```rust
use crate::optimise::OptimiseError;

const FLOOR: u8 = 5;

#[derive(Debug)]
pub struct Fitted {
    pub bytes: Vec<u8>,
    pub quality: u8,
    /// Even the floor quality is over the target; `bytes` is that smallest result.
    pub missed: bool,
}

/// Try the floor first (one encode settles an impossible target), then
/// binary-search 6..=100 with the six encodes left.
pub fn search(
    target: u64,
    mut encode: impl FnMut(u8) -> Result<Vec<u8>, OptimiseError>,
) -> Result<Fitted, OptimiseError> {
    let floor = encode(FLOOR)?;
    if floor.len() as u64 > target {
        return Ok(Fitted { bytes: floor, quality: FLOOR, missed: true });
    }
    let mut best = (FLOOR, floor);
    let (mut lo, mut hi) = (FLOOR + 1, 100_u8);
    for _ in 0..6 {
        if lo > hi {
            break;
        }
        let q = lo + (hi - lo).div_ceil(2);
        let out = encode(q)?;
        if out.len() as u64 <= target {
            best = (q, out);
            lo = q + 1;
        } else {
            hi = q - 1;
        }
    }
    Ok(Fitted { bytes: best.1, quality: best.0, missed: false })
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core edit::fit`
Expected: PASS (4 tests). `exactly_the_target_fits` relies on the upper-mid choice reaching 100 within six steps (6→… lo/hi narrow upward each time); if it lands one short, the search order is wrong — fix the code, not the test.

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/edit/fit.rs crates/ferry-core/src/edit/mod.rs
git commit -m "feat(core): fit-under-size quality search

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 7: `apply` — a recipe on the original

**Files:**
- Modify: `crates/ferry-core/src/edit/mod.rs` (add `apply`, `Applied`, `Work`, `prepare`, `place`)
- Modify: `src/lib/i18n/en.json` (add `"error.animated_edit": "Animated {format} can't be edited yet"`)

**Interfaces:**
- Consumes: Tasks 1–6 (`Edits`, `geometry::*`, `smart::frame`, `raster::*`, `fit::search`, `optimise::encode`, `optimise::gif::edit`).
- Produces:
  - `edit::Applied { bytes: Vec<u8>, format: Format, dims: (u32,u32), missed_target: bool }`.
  - `edit::apply(original: &[u8], source: Format, edits: &Edits, o: &OptimiseOptions) -> Result<Applied, OptimiseError>` — `o.compression`/`o.level` are the settings; the recipe overrides them. A recipe asking for GIF output keeps the source format.

- [ ] **Step 1: Write the failing tests**

Append to the tests module in `crates/ferry-core/src/edit/mod.rs`:

```rust
    use crate::optimise::OptimiseOptions;
    use crate::sniff::{dimensions, gif_frame_count, sniff};
    use crate::testutil;

    fn applied(bytes: &[u8], source: Format, f: impl FnOnce(&mut Edits)) -> Applied {
        let mut edits = Edits::default();
        f(&mut edits);
        apply(bytes, source, &edits, &OptimiseOptions::default()).unwrap()
    }

    #[test]
    fn a_downscale_halves_the_picture() {
        let a = applied(&testutil::png(300, 200), Format::Png, |e| e.scale = Some(50));
        assert_eq!((a.format, a.dims), (Format::Png, (150, 100)));
        assert_eq!(dimensions(&a.bytes, Format::Png), Some((150, 100)));
    }

    #[test]
    fn crop_scale_and_convert_run_in_one_pass() {
        let a = applied(&testutil::png(300, 200), Format::Png, |e| {
            e.aspect = Some((1, 1));
            e.scale = Some(50);
            e.format = Some(Format::WebP);
        });
        assert_eq!((a.format, a.dims), (Format::WebP, (100, 100)));
        assert_eq!(sniff(&a.bytes), Some(Format::WebP));
        assert_eq!(dimensions(&a.bytes, Format::WebP), Some((100, 100)));
    }

    #[test]
    fn a_lower_compression_keeps_the_format_and_size_and_shrinks_the_file() {
        let jpeg = testutil::jpeg(300, 200, 95, &testutil::JpegExtras::default());
        let high = applied(&jpeg, Format::Jpeg, |e| e.compression = Some(95));
        let low = applied(&jpeg, Format::Jpeg, |e| e.compression = Some(30));
        assert_eq!((low.format, low.dims), (Format::Jpeg, (300, 200)));
        assert!(low.bytes.len() < high.bytes.len());
    }

    #[test]
    fn fit_under_lands_under_the_target_or_says_it_missed() {
        let jpeg = testutil::jpeg(400, 300, 95, &testutil::JpegExtras::default());
        let full = applied(&jpeg, Format::Jpeg, |e| e.compression = Some(100));
        let target = full.bytes.len() as u64 / 2;
        let fitted = applied(&jpeg, Format::Jpeg, |e| e.fit_under = Some(target));
        assert!(fitted.bytes.len() as u64 <= target && !fitted.missed_target);
        let missed = applied(&jpeg, Format::Jpeg, |e| e.fit_under = Some(100));
        assert!(missed.missed_target);
    }

    #[test]
    fn a_gif_converts_to_a_still_from_its_first_frame() {
        let a = applied(&testutil::gif(3), Format::Gif, |e| e.format = Some(Format::Png));
        assert_eq!((a.format, a.dims), (Format::Png, (64, 48)));
        assert_eq!(sniff(&a.bytes), Some(Format::Png));
    }

    #[test]
    fn asking_for_gif_output_keeps_the_source_format() {
        let a = applied(&testutil::png(60, 40), Format::Png, |e| e.format = Some(Format::Gif));
        assert_eq!(a.format, Format::Png);
    }

    #[test]
    fn an_animated_webp_cannot_be_edited() {
        let edits = Edits { scale: Some(50), ..Edits::default() };
        let result = apply(&testutil::animated_webp(), Format::WebP, &edits, &OptimiseOptions::default());
        assert!(matches!(result, Err(OptimiseError::Unsupported(m)) if m == "Animated WebP can't be edited yet"));
    }

    #[test]
    fn an_animated_gif_is_downscaled_by_gifsicle_and_stays_animated() {
        let Some(gifsicle) = testutil::gifsicle() else {
            return;
        };
        let o = OptimiseOptions { gifsicle: Some(gifsicle), ..OptimiseOptions::default() };
        let edits = Edits { scale: Some(50), ..Edits::default() };
        let a = apply(&testutil::gif(3), Format::Gif, &edits, &o).unwrap();
        assert_eq!((a.format, a.dims), (Format::Gif, (32, 24)));
        assert_eq!(gif_frame_count(&a.bytes), 3);
        assert_eq!(dimensions(&a.bytes, Format::Gif), Some((32, 24)));
    }

    #[test]
    fn a_cancelled_token_stops_an_edit() {
        let o = OptimiseOptions::default();
        o.cancel.cancel();
        let edits = Edits { scale: Some(50), ..Edits::default() };
        assert!(matches!(
            apply(&testutil::png(60, 40), Format::Png, &edits, &o),
            Err(OptimiseError::Cancelled)
        ));
    }
```

Add to `src/lib/i18n/en.json` (next to the other `error.*` keys): `"error.animated_edit": "Animated {format} can't be edited yet",`

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core edit::tests`
Expected: FAIL to compile — `cannot find function apply`, `type Applied`.

- [ ] **Step 3: Implement**

Add to the imports at the top of `crates/ferry-core/src/edit/mod.rs`:

```rust
use crate::i18n::tf;
use crate::optimise::{self, OptimiseError, OptimiseOptions, gif};
use crate::sniff;
use geometry::Rect;
use raster::Raster;
```

and insert after `EditsPatch` / `present`:

```rust
/// What `apply` made: the bytes, their format and size, and whether a
/// fit-under-size target was out of reach.
#[derive(Debug)]
pub struct Applied {
    pub bytes: Vec<u8>,
    pub format: Format,
    pub dims: (u32, u32),
    pub missed_target: bool,
}

/// Run the whole recipe on the original (spec §4.2). `o` carries the
/// settings; the recipe's level and compression override them.
pub fn apply(
    original: &[u8],
    source: Format,
    edits: &Edits,
    o: &OptimiseOptions,
) -> Result<Applied, OptimiseError> {
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    if source != Format::Gif && sniff::is_animated(original, source) {
        return Err(OptimiseError::Unsupported(tf(
            "error.animated_edit",
            &[("format", source.label())],
        )));
    }
    let target = match edits.format {
        Some(format) if format != Format::Gif => format,
        _ => source,
    };
    let (work, dims) = prepare(original, source, target, edits)?;
    let mut base = o.clone();
    base.level = edits.level;
    if let Some(compression) = edits.compression {
        base.compression = compression;
    }
    let (bytes, missed_target) = match edits.fit_under {
        Some(limit) => {
            let fitted = fit::search(limit, |q| {
                work.run(&OptimiseOptions {
                    compression: q,
                    level: Level::Normal,
                    ..base.clone()
                })
            })?;
            (fitted.bytes, fitted.missed)
        }
        None => (work.run(&base)?, false),
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    Ok(Applied { bytes, format: target, dims, missed_target })
}

/// The encoder a recipe needs, prepared once so fit-under-size can run it
/// several times.
enum Work<'a> {
    /// GIF to GIF, animated or not: gifsicle crops and resizes every frame.
    Gifsicle { input: &'a [u8], crop: Option<Rect>, size: Option<(u32, u32)> },
    /// Same format, same pixels: the format's optimiser at a new quality, so
    /// Cycle 1's metadata rules stay exactly as they were.
    Encoder { input: &'a [u8], format: Format },
    /// Everything else: decoded, cut and re-encoded.
    Pixels { raster: Raster, format: Format },
}

impl Work<'_> {
    fn run(&self, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
        match self {
            Work::Gifsicle { input, crop, size } => gif::edit(input, *crop, *size, o),
            Work::Encoder { input, format } => optimise::encode(input, *format, o),
            Work::Pixels { raster, format } => raster::encode(raster, *format, o),
        }
    }
}

fn prepare<'a>(
    original: &'a [u8],
    source: Format,
    target: Format,
    edits: &Edits,
) -> Result<(Work<'a>, (u32, u32)), OptimiseError> {
    if source == Format::Gif && target == Format::Gif {
        let full = sniff::dimensions(original, Format::Gif)
            .ok_or_else(|| OptimiseError::Decode("the GIF is incomplete".into()))?;
        let window = geometry::crop_size(full, edits);
        // Smart framing judges the first frame.
        let first = (edits.framing == Framing::Smart && window != full)
            .then(|| raster::decode(original, Format::Gif))
            .transpose()?;
        let crop = place(full, window, edits.framing, first.as_ref());
        let out = geometry::output_size(window, edits);
        let work = Work::Gifsicle {
            input: original,
            crop: (window != full).then_some(crop),
            size: (out != window).then_some(out),
        };
        return Ok((work, out));
    }
    if !edits.reshapes() && target == source {
        let dims = raster::display_dims(original, source).unwrap_or_default();
        return Ok((Work::Encoder { input: original, format: source }, dims));
    }
    let decoded = raster::decode(original, source)?;
    let full = (decoded.width, decoded.height);
    let window = geometry::crop_size(full, edits);
    let crop = place(full, window, edits.framing, Some(&decoded));
    let out = geometry::output_size(window, edits);
    let shaped = if window == full && out == full {
        decoded
    } else {
        raster::cut(&decoded, crop, out)?
    };
    Ok((Work::Pixels { raster: shaped, format: target }, out))
}

/// Where the window sits: the busiest spot for Smart (when pixels are at
/// hand), else the centre.
fn place(full: (u32, u32), window: (u32, u32), framing: Framing, pixels: Option<&Raster>) -> Rect {
    match (framing, pixels) {
        (Framing::Smart, Some(r)) if window != full => smart::frame(&r.rgba, r.width, r.height, window),
        _ => geometry::centre(full, window),
    }
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core edit && cargo test -p ferry-core`
Expected: PASS (9 new tests, full suite green).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/edit/mod.rs src/lib/i18n/en.json
git commit -m "feat(core): apply an edit recipe to the original

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 8: Settings v2 and output locations

**Files:**
- Modify: `crates/ferry-core/src/settings.rs` (`VERSION = 2`, `Location`, `OutputRule`, `Output`, `Images.output`, `Images.crop_sizes`, `Floating.hover_hotkeys`, `sanitised`)
- Create: `crates/ferry-core/src/output.rs`
- Modify: `crates/ferry-core/src/lib.rs` (`pub mod output;`)

**Interfaces:**
- Consumes: `template::{Ctx, render, safe_file_name, validate}`, `settings::expand_home`, `edit::MAX_SIDE`.
- Produces:
  - `settings::Location { InPlace, SameFolder, Folder, Temporary }` (serde snake_case: `"in_place" | "same_folder" | "folder" | "temporary"`).
  - `settings::OutputRule { location: Location, folder: String, template: String, extra }`; `settings::Output { optimised: OutputRule, converted: OutputRule, extra }` (`Clone, PartialEq, Debug`, serde default).
  - `Images.output: Output`, `Images.crop_sizes: Vec<(u32, u32)>`, `Floating.hover_hotkeys: bool`.
  - `output::OutputKind { Optimised, Converted }` (`Debug, Clone, Copy, PartialEq, Eq`).
  - `output::Target<'a> { source: &'a Path, job: &'a str, ext: &'a str, dims: (u32,u32), kind: OutputKind }`.
  - `output::resolve(t: &Target, rules: &Output, work_dir: &Path, home: &Path, today: NaiveDate) -> PathBuf`.
  - `output::unique(path: &Path, own: Option<&Path>) -> PathBuf`.

The spec's `Location::Folder(PathBuf)` is stored as a unit variant plus the rule's `folder` string, matching the settings JSON in spec §5.6.

- [ ] **Step 1: Write the failing tests**

In the tests module of `crates/ferry-core/src/settings.rs`, change `assert_eq!(s.version, 1);` to `assert_eq!(s.version, 2);` and append:

```rust
    #[test]
    fn cycle_2_defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(
            serde_json::to_value(&s.images.output).unwrap(),
            json!({
                "optimised": { "location": "in_place", "folder": "~/Pictures/Ferry", "template": "%f_optimised.%e" },
                "converted": { "location": "in_place", "folder": "~/Pictures/Ferry", "template": "%f.%e" }
            })
        );
        assert_eq!(s.images.crop_sizes, [(1200, 628), (1080, 1080), (1920, 1080), (1280, 720)]);
        assert!(s.floating.hover_hotkeys);
    }

    #[test]
    fn a_version_1_file_loads_as_version_2_with_the_new_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, r#"{"version": 1, "images": {"compression": 70}}"#).unwrap();
        let (settings, _) = Settings::load(&path);
        assert_eq!(settings.version, 2);
        assert_eq!(settings.images.compression, 70);
        assert_eq!(settings.images.output, Output::default());
        assert!(settings.floating.hover_hotkeys);
    }

    #[test]
    fn broken_output_rules_and_crop_sizes_are_repaired() {
        let mut s = Settings::default();
        s.images.output.optimised.template = "%q".into();
        s.images.output.converted.template = "  ".into();
        s.images.output.converted.folder = String::new();
        s.images.crop_sizes = vec![(0, 100), (1200, 628), (40_000, 10)];
        let s = s.sanitised();
        let defaults = Output::default();
        assert_eq!(s.images.output.optimised.template, defaults.optimised.template);
        assert_eq!(s.images.output.converted.template, defaults.converted.template);
        assert_eq!(s.images.output.converted.folder, defaults.converted.folder);
        assert_eq!(s.images.crop_sizes, [(1200, 628)]);
    }
```

Create `crates/ferry-core/src/output.rs`:

```rust
//! Where a file job's result goes (spec §4.4): over the source, beside it
//! under a name template, in a chosen folder, or in the working folder.
//! Clipboard jobs always use their working file and never come here.

#[cfg(test)]
mod tests {
    use super::*;
    use crate::settings::{Location, Output};
    use chrono::NaiveDate;

    fn today() -> NaiveDate {
        NaiveDate::from_ymd_opt(2026, 10, 22).unwrap()
    }

    fn at(kind: OutputKind, ext: &str, rules: &Output) -> PathBuf {
        let source = Path::new("/u/Desktop/shot.png");
        let target = Target { source, job: "j1", ext, dims: (300, 200), kind };
        resolve(&target, rules, Path::new("/w"), Path::new("/h"), today())
    }

    fn rules(location: Location, template: &str) -> Output {
        let mut rules = Output::default();
        for rule in [&mut rules.optimised, &mut rules.converted] {
            rule.location = location;
            rule.template = template.into();
        }
        rules
    }

    #[test]
    fn in_place_overwrites_or_swaps_the_extension() {
        let defaults = Output::default();
        assert_eq!(at(OutputKind::Optimised, "png", &defaults), Path::new("/u/Desktop/shot.png"));
        assert_eq!(at(OutputKind::Converted, "webp", &defaults), Path::new("/u/Desktop/shot.webp"));
    }

    #[test]
    fn same_folder_renders_the_template_beside_the_source() {
        let defaults = Output { optimised: rules(Location::SameFolder, "%f_optimised.%e").optimised, ..Output::default() };
        assert_eq!(at(OutputKind::Optimised, "png", &defaults), Path::new("/u/Desktop/shot_optimised.png"));
        let tokens = rules(Location::SameFolder, "%P-%z-%y%m%d.%e");
        assert_eq!(at(OutputKind::Converted, "webp", &tokens), Path::new("/u/Desktop/Desktop-300x200-20261022.webp"));
    }

    #[test]
    fn a_specific_folder_expands_home() {
        let mut folder = rules(Location::Folder, "%f.%e");
        folder.converted.folder = "~/Pictures/Ferry".into();
        assert_eq!(
            at(OutputKind::Converted, "webp", &folder),
            Path::new("/h").join("Pictures/Ferry").join("shot.webp")
        );
    }

    #[test]
    fn temporary_goes_to_the_jobs_own_working_folder() {
        assert_eq!(
            at(OutputKind::Optimised, "png", &rules(Location::Temporary, "%f.%e")),
            Path::new("/w").join("files").join("j1").join("shot.png")
        );
    }

    #[test]
    fn a_template_cannot_escape_its_folder() {
        assert_eq!(
            at(OutputKind::Converted, "png", &rules(Location::SameFolder, "../%f.%e")),
            Path::new("/u/Desktop/.._shot.png")
        );
    }

    #[test]
    fn a_taken_name_gets_a_number_unless_it_is_our_own() {
        let dir = tempfile::tempdir().unwrap();
        let wanted = dir.path().join("shot.webp");
        assert_eq!(unique(&wanted, None), wanted);
        fs::write(&wanted, b"x").unwrap();
        assert_eq!(unique(&wanted, None), dir.path().join("shot (2).webp"));
        assert_eq!(unique(&wanted, Some(&wanted)), wanted);
        fs::write(dir.path().join("shot (2).webp"), b"x").unwrap();
        assert_eq!(unique(&wanted, None), dir.path().join("shot (3).webp"));
    }
}
```

Add `pub mod output;` to `crates/ferry-core/src/lib.rs` (alphabetical, after `pub mod optimise;`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core settings output`
Expected: FAIL to compile — `Output`, `Location`, `resolve`, `unique`, `Target`, `OutputKind` missing.

- [ ] **Step 3: Implement the settings model**

In `crates/ferry-core/src/settings.rs`: set `pub const VERSION: u32 = 2;`, add `use crate::{edit, template};`, then add the new types (after `Images`):

```rust
/// Where a file job's result goes (spec §4.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Location {
    InPlace,
    SameFolder,
    Folder,
    Temporary,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutputRule {
    pub location: Location,
    /// Used by `Folder`; stored with `~` (see `contract_home`).
    pub folder: String,
    /// File name only: `%f %e %z %y %m %d %P`.
    pub template: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for OutputRule {
    fn default() -> Self {
        Self {
            location: Location::InPlace,
            folder: "~/Pictures/Ferry".into(),
            template: "%f.%e".into(),
            extra: Map::new(),
        }
    }
}

/// Images row of the File handling pane: optimised results and conversions.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Output {
    pub optimised: OutputRule,
    pub converted: OutputRule,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Default for Output {
    fn default() -> Self {
        Self {
            optimised: OutputRule {
                template: "%f_optimised.%e".into(),
                ..OutputRule::default()
            },
            converted: OutputRule::default(),
            extra: Map::new(),
        }
    }
}
```

Add fields to `Images` (before `extra`): `pub output: Output,` and `/// Saved crop sizes, newest first. pub crop_sizes: Vec<(u32, u32)>,`; in `impl Default for Images` add `output: Output::default(), crop_sizes: vec![(1200, 628), (1080, 1080), (1920, 1080), (1280, 720)],`.

Add to `Floating` (before `extra`): `/// Space, Cmd/Ctrl+D, - and 1–9 act on the hovered card. pub hover_hotkeys: bool,` and `hover_hotkeys: true,` in its `Default`.

At the end of `sanitised` (before `self`):

```rust
        let defaults = Output::default();
        for (rule, default) in [
            (&mut self.images.output.optimised, &defaults.optimised),
            (&mut self.images.output.converted, &defaults.converted),
        ] {
            if rule.template.trim().is_empty() || template::validate(&rule.template).is_err() {
                rule.template = default.template.clone();
            }
            if rule.folder.trim().is_empty() {
                rule.folder = default.folder.clone();
            }
        }
        let side = 1..=edit::MAX_SIDE;
        self.images
            .crop_sizes
            .retain(|(w, h)| side.contains(w) && side.contains(h));
        self.version = self.version.max(VERSION);
```

- [ ] **Step 4: Implement `output.rs`**

Insert above the tests module of `crates/ferry-core/src/output.rs`:

```rust
use std::fs;
use std::path::{Path, PathBuf};

use chrono::NaiveDate;

use crate::settings::{self, Location, Output};
use crate::template;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OutputKind {
    /// Same format as the source (also downscale and crop).
    Optimised,
    Converted,
}

pub struct Target<'a> {
    /// The file job's own file.
    pub source: &'a Path,
    pub job: &'a str,
    /// The result's extension.
    pub ext: &'a str,
    /// The result's size, for `%z`.
    pub dims: (u32, u32),
    pub kind: OutputKind,
}

/// The path a result should have, before `unique`. A path equal to the
/// source means "overwrite the source".
pub fn resolve(t: &Target, rules: &Output, work_dir: &Path, home: &Path, today: NaiveDate) -> PathBuf {
    let rule = match t.kind {
        OutputKind::Optimised => &rules.optimised,
        OutputKind::Converted => &rules.converted,
    };
    let stem = t
        .source
        .file_stem()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    let folder = t.source.parent().unwrap_or(Path::new(""));
    let named = |dir: &Path| {
        let parent = folder
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        let ctx = template::Ctx {
            stem: &stem,
            ext: t.ext,
            width: t.dims.0,
            height: t.dims.1,
            parent: &parent,
            date: today,
        };
        dir.join(template::safe_file_name(&template::render(&rule.template, &ctx)))
    };
    match rule.location {
        Location::InPlace => match t.kind {
            OutputKind::Optimised => t.source.to_owned(),
            OutputKind::Converted => folder.join(format!("{stem}.{}", t.ext)),
        },
        Location::SameFolder => named(folder),
        Location::Folder => named(&settings::expand_home(&rule.folder, home)),
        Location::Temporary => work_dir
            .join("files")
            .join(t.job)
            .join(format!("{stem}.{}", t.ext)),
    }
}

/// `path`, or `name (2).ext`, `name (3).ext`, … when another file already
/// has it. `own` (this job's previous result) never counts as taken.
pub fn unique(path: &Path, own: Option<&Path>) -> PathBuf {
    if !path.exists() || own == Some(path) {
        return path.to_owned();
    }
    let stem = path.file_stem().map(|s| s.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    (2..)
        .map(|n| path.with_file_name(format!("{stem} ({n}){ext}")))
        .find(|candidate| !candidate.exists() || own == Some(candidate.as_path()))
        .expect("an unused name exists")
}
```

`fs` is only needed by the tests; if clippy flags the unused import, move `use std::fs;` into the tests module.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ferry-core settings && cargo test -p ferry-core output && cargo test -p ferry-core`
Expected: PASS (3 new settings tests, 6 output tests; `defaults_match_the_spec` now expects version 2).

- [ ] **Step 6: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/settings.rs crates/ferry-core/src/output.rs crates/ferry-core/src/lib.rs
git commit -m "feat(core): output locations and settings version 2

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 9: The job runs its recipe and owns one result

**Files:**
- Modify: `crates/ferry-core/src/job.rs` (new `JobUpdate` fields)
- Modify: `crates/ferry-core/src/pipeline.rs` (struct, constructors, `RunOptions`, `run`, `restore`, `update`, `detail`, `edit`, `fork`, new `put` / `put_back` / `undo` / `write_file` / `write_work_file`, `bytes_label`)
- Modify: `src-tauri/Cargo.toml` (`trash = "5.2"`), `src-tauri/src/jobs.rs` (`run_options`, `move_to_trash`)
- Modify: `src/lib/i18n/en.json` (`card.step.edit`, `card.note.fit_missed`)

**Interfaces:**
- Consumes: `edit::{Edits, EditsPatch, apply}` (Tasks 1, 7), `edit::raster::display_dims` (Task 4), `output::{resolve, unique, Target, OutputKind}` and `settings::{Output, Location}` (Task 8).
- Produces:
  - `JobUpdate` gains `dims_before: Option<(u32,u32)>`, `dims_after: Option<(u32,u32)>`, `format: Option<Format>`, `converted: bool`, `can_edit: bool`, `edits: Edits`, `note: Option<String>`.
  - `RunOptions` gains `output: settings::Output`, `work_dir: PathBuf`, `home: PathBuf`, `trash: fn(&Path) -> io::Result<()>`; `RunOptions::new(settings: &Settings, gifsicle: Option<PathBuf>, work_dir: PathBuf, trash: fn(&Path) -> io::Result<()>, cancel: CancelToken)`.
  - `Job::edit(&mut self, patch: EditsPatch)`, `Job::fork(&self) -> Job`; `Job.edits: Edits` is `pub`.
  - `Job::run`, `Job::restore`, `Job::update` keep their signatures.

- [ ] **Step 1: Write the failing tests**

In the tests module of `crates/ferry-core/src/pipeline.rs`, add the imports `use crate::edit::EditsPatch; use crate::settings::Location; use crate::sniff;` and replace `options()` with:

```rust
    /// Stands in for the system Trash: moves the file into `.trash` beside it.
    fn test_trash(path: &Path) -> io::Result<()> {
        let bin = path.parent().unwrap().join(".trash");
        fs::create_dir_all(&bin)?;
        let name = path.file_name().unwrap().to_string_lossy().into_owned();
        let dest = (0..)
            .map(|n| bin.join(if n == 0 { name.clone() } else { format!("{n}-{name}") }))
            .find(|p| !p.exists())
            .unwrap();
        fs::rename(path, dest)
    }

    fn options_with(settings: &Settings) -> RunOptions {
        RunOptions::new(
            settings,
            testutil::gifsicle(),
            std::env::temp_dir().join("ferry-test-work"),
            test_trash,
            CancelToken::new(),
        )
    }

    fn options() -> RunOptions {
        options_with(&Settings::default())
    }

    fn scale(percent: u8) -> EditsPatch {
        EditsPatch { scale: Some(Some(percent)), ..EditsPatch::default() }
    }

    fn convert(format: Format) -> EditsPatch {
        EditsPatch { format: Some(Some(format)), ..EditsPatch::default() }
    }
```

Then append these tests:

```rust
    #[test]
    fn a_downscale_re_encodes_from_the_original_and_never_compounds() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let o = options();
        run(&mut job, Level::Normal, &o, &env);
        job.edit(scale(50));
        let (first, seen) = run(&mut job, Level::Normal, &o, &env);
        let first = first.unwrap().update;
        assert_eq!(first.state, JobState::Done);
        assert_eq!(first.dims_after, Some((150, 100)));
        assert_eq!(first.detail, "300×200 → 150×100 · PNG");
        assert_eq!(first.edits.scale, Some(50));
        assert!(first.can_edit);
        let delivered = &seen.delivered.last().unwrap().1;
        assert_eq!(sniff::dimensions(delivered, Format::Png), Some((150, 100)));

        job.edit(scale(50));
        let (_, again) = run(&mut job, Level::Normal, &o, &env);
        let delivered = &again.delivered.last().unwrap().1;
        assert_eq!(sniff::dimensions(delivered, Format::Png), Some((150, 100)), "not 75×50");
    }

    #[test]
    fn convert_in_place_trashes_the_source_and_restore_brings_it_back() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        run(&mut job, Level::Normal, &o, &env);
        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;

        let webp = env.dir.path().join("shot.webp");
        assert_eq!(update.output, Some(webp.clone()));
        assert_eq!(sniff::sniff(&fs::read(&webp).unwrap()), Some(Format::WebP));
        assert!(!path.exists(), "the source went to the Trash");
        assert_eq!(fs::read(env.dir.path().join(".trash/shot.png")).unwrap(), input, "the original, not the optimised file");
        assert!(update.converted);
        assert_eq!((update.ext.as_str(), update.format), ("webp", Some(Format::WebP)));

        let restored = job.restore(&o, &mut |_, _, _| Ok(())).update;
        assert_eq!(restored.state, JobState::Restored);
        assert_eq!(fs::read(&path).unwrap(), input);
        assert!(!webp.exists());
        assert_eq!(restored.output, Some(path));
        assert!(!restored.converted);
        assert_eq!(restored.edits, Edits::default());
    }

    #[test]
    fn switching_from_a_same_folder_result_to_an_in_place_conversion_leaves_one_file() {
        let env = Env::new();
        let mut settings = Settings::default();
        settings.images.output.optimised.location = Location::SameFolder;
        let o = options_with(&settings);
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &settings, true).unwrap();

        let first = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        let optimised = env.dir.path().join("shot_optimised.png");
        assert_eq!(first.output, Some(optimised.clone()));
        assert_eq!(fs::read(&path).unwrap(), input, "the source is untouched");

        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &o, &env);
        assert!(env.dir.path().join("shot.webp").exists());
        assert!(!optimised.exists() && !path.exists());
    }

    #[test]
    fn choosing_the_source_format_again_undoes_the_conversion() {
        let env = Env::new();
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        job.edit(convert(Format::WebP));
        run(&mut job, Level::Normal, &o, &env);
        job.edit(convert(Format::Png));
        assert_eq!(job.edits.format, None);
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert!(!env.dir.path().join("shot.webp").exists());
        assert!(fs::metadata(&path).unwrap().len() < input.len() as u64);
        assert!(!update.converted);
    }

    #[test]
    fn an_explicit_edit_writes_its_result_even_when_larger() {
        let env = Env::new();
        let mut job = Job::clipboard(Format::Png, testutil::png(1, 1), false, "%f", &env.work(), now());
        job.edit(EditsPatch { compression: Some(Some(100)), ..EditsPatch::default() });
        let (finished, seen) = run(&mut job, Level::Normal, &options(), &env);
        let update = finished.unwrap().update;
        assert_eq!(update.state, JobState::Done);
        assert!(update.after.is_some());
        assert_eq!(seen.delivered.len(), 1);
    }

    #[test]
    fn an_unreachable_fit_target_is_noted_on_the_card() {
        let env = Env::new();
        let jpeg = testutil::jpeg(400, 300, 90, &testutil::JpegExtras::default());
        let mut job = Job::clipboard(Format::Jpeg, jpeg, false, "%f", &env.work(), now());
        job.edit(EditsPatch { fit_under: Some(Some(100)), ..EditsPatch::default() });
        let update = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(update.note.as_deref(), Some("Couldn't reach 100 B — try downscaling"));
    }

    #[test]
    fn an_animated_webp_offers_no_edits() {
        let env = Env::new();
        let mut job = Job::clipboard(Format::WebP, testutil::animated_webp(), false, "%f", &env.work(), now());
        let update = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
        assert!(!update.can_edit);
        let (mut png_job, _) = clipboard_png(&env, "%f");
        assert!(run(&mut png_job, Level::Normal, &options(), &env).0.unwrap().update.can_edit);
    }

    #[test]
    fn a_clipboard_conversion_renames_the_working_file_and_delivers_the_new_format() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let o = options();
        let first = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        job.edit(convert(Format::Jpeg));
        let (finished, seen) = run(&mut job, Level::Normal, &o, &env);
        let update = finished.unwrap().update;
        let output = update.output.unwrap();
        assert_eq!(output.extension().unwrap(), "jpg");
        assert!(!first.output.unwrap().exists(), "the old working file is gone");
        assert_eq!(seen.delivered.last().unwrap().0, Format::Jpeg);
        assert_eq!(update.ext, "jpg");
    }

    #[test]
    fn an_edit_after_the_source_vanished_still_writes_a_result() {
        let env = Env::new();
        let path = env.file("shot.png", &testutil::png(300, 200));
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        let o = options();
        run(&mut job, Level::Normal, &o, &env);
        fs::remove_file(&path).unwrap();
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert_eq!(update.state, JobState::Done);
        assert_eq!(sniff::dimensions(&fs::read(&path).unwrap(), Format::Png), Some((150, 100)));
    }

    #[test]
    fn an_unwritable_output_folder_is_an_error_and_the_source_keeps_the_original() {
        let env = Env::new();
        let blocker = env.file("blocker", b"not a folder");
        let mut settings = Settings::default();
        settings.images.output.converted.location = Location::Folder;
        settings.images.output.converted.folder = blocker.join("sub").to_string_lossy().into_owned();
        let o = options_with(&settings);
        let input = testutil::png(300, 200);
        let path = env.file("shot.png", &input);
        let mut job = Job::file(&path, &settings, true).unwrap();
        run(&mut job, Level::Normal, &o, &env);
        job.edit(convert(Format::WebP));
        let update = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        assert!(matches!(&update.state, JobState::Error { reason } if reason.starts_with("Can't write to")));
        assert_eq!(fs::read(&path).unwrap(), input);
    }

    #[test]
    fn a_fork_shares_the_original_and_leaves_its_parent_alone() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let o = options();
        let parent = run(&mut job, Level::Normal, &o, &env).0.unwrap().update;
        let mut twin = job.fork();
        assert_ne!(twin.id, job.id);
        twin.edit(convert(Format::WebP));
        let child = run(&mut twin, Level::Normal, &o, &env).0.unwrap().update;
        assert_eq!(child.thumb_path, parent.thumb_path, "same backup");
        assert_ne!(child.output, parent.output);
        assert!(parent.output.unwrap().exists());
        assert_eq!(child.format, Some(Format::WebP));
    }

    #[test]
    fn sizes_read_like_the_cards() {
        assert_eq!(bytes_label(100), "100 B");
        assert_eq!(bytes_label(1_000_000), "1 MB");
        assert_eq!(bytes_label(1_500_000), "1.5 MB");
        assert_eq!(bytes_label(250_000), "250 KB");
    }
```

Add to `src/lib/i18n/en.json`: `"card.step.edit": "Applying changes",` (next to the other `card.step.*`) and `"card.note.fit_missed": "Couldn't reach {size} — try downscaling",`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core pipeline`
Expected: FAIL to compile — `RunOptions::new` takes 3 arguments, `Job::edit`/`fork`, `JobUpdate.dims_after`/`edits`/`note`/`can_edit`/`converted`/`format`, `bytes_label` missing.

- [ ] **Step 3: Extend `JobUpdate`**

In `crates/ferry-core/src/job.rs`, add `use crate::edit::Edits; use crate::sniff::Format;` and append to `JobUpdate` (after `output`):

```rust
    /// The original's size as shown (JPEG orientation applied).
    pub dims_before: Option<(u32, u32)>,
    /// The result's size when an edit changed it.
    pub dims_after: Option<(u32, u32)>,
    /// The current result's format; `None` on a notice.
    pub format: Option<Format>,
    pub converted: bool,
    /// The edit tools apply: the original is still backed up, the job is idle,
    /// and it is not an animated WebP or APNG.
    pub can_edit: bool,
    /// The recipe the sliders and popovers start from.
    pub edits: Edits,
    /// An amber note, such as "Couldn't reach 1 MB — try downscaling".
    pub note: Option<String>,
```

In `pipeline::notice`, add `dims_before: None, dims_after: None, format: None, converted: false, can_edit: false, edits: Edits::default(), note: None,`.

- [ ] **Step 4: Extend `RunOptions` and the job's fields**

In `crates/ferry-core/src/pipeline.rs`, add imports `use chrono::Local; use crate::edit::{self, Edits, EditsPatch}; use crate::output::{self, OutputKind}; use crate::settings::{self as settings_model, Location};` (keep the existing `Settings` import), and replace `RunOptions`:

```rust
/// Settings a run reads, taken when the run starts.
#[derive(Debug, Clone)]
pub struct RunOptions {
    pub compression: u8,
    pub strip_metadata: bool,
    pub preserve_dates: bool,
    pub gifsicle: Option<PathBuf>,
    pub cancel: CancelToken,
    pub output: settings_model::Output,
    /// The working folder; Temporary results go in `files/<job id>/`.
    pub work_dir: PathBuf,
    pub home: PathBuf,
    /// Moves a file Ferry made, or a converted-away source, to the Trash.
    /// The app logs failures; here a failure only means the file stays.
    pub trash: fn(&Path) -> io::Result<()>,
}

impl RunOptions {
    pub fn new(
        settings: &Settings,
        gifsicle: Option<PathBuf>,
        work_dir: PathBuf,
        trash: fn(&Path) -> io::Result<()>,
        cancel: CancelToken,
    ) -> Self {
        Self {
            compression: settings.images.compression,
            strip_metadata: settings.images.strip_metadata,
            preserve_dates: settings.images.preserve_dates,
            gifsicle,
            cancel,
            output: settings.images.output.clone(),
            work_dir,
            home: std::env::home_dir().unwrap_or_default(),
            trash,
        }
    }
}
```

Replace the `Job` struct with:

```rust
#[derive(Debug)]
pub struct Job {
    pub id: JobId,
    pub source: Source,
    /// Shown on the card: the rendered template, or the file's own stem.
    pub name: String,
    /// The current result's extension: the source's own until a conversion.
    pub ext: String,
    /// The current result's format.
    pub format: Format,
    pub before: u64,
    /// The original's size as shown (JPEG orientation applied).
    pub dims: Option<(u32, u32)>,
    pub from_bitmap: bool,
    /// The working file of a clipboard job, or the file job's current result.
    pub output: PathBuf,
    /// The level of the latest run; retry and re-optimise use it again.
    pub level: Level,
    /// What the card asked for; every run applies all of it to the original.
    pub edits: Edits,
    source_format: Format,
    source_ext: String,
    animated: bool,
    /// A file job's own location: where an in-place result goes.
    source_path: Option<PathBuf>,
    /// A file job's current result, so the next run can take it back.
    last: Option<Written>,
    dims_after: Option<(u32, u32)>,
    note: Option<String>,
    /// The original, once stored. Every run starts from it (R§2.4).
    backup: Option<PathBuf>,
    /// A clipboard job's original until it is backed up.
    pending: Option<Vec<u8>>,
    /// A file job's times, read before its first write.
    times: Option<Times>,
    /// `output` (and the clipboard) hold a result, not the original.
    changed: bool,
}

/// A file job's result on disk (spec §4.4: one result per job).
#[derive(Debug, Clone)]
struct Written {
    path: PathBuf,
    /// Ferry made this file; it is not the source.
    created: bool,
    /// The source was overwritten or trashed for it.
    source_changed: bool,
}
```

In `Job::clipboard`, replace `let dims = sniff::dimensions(&bytes, format);` with `let dims = edit::raster::display_dims(&bytes, format);`, compute `let animated = sniff::is_animated(&bytes, format);` before `bytes` moves into `pending`, and set the new fields in the literal: `ext: ext.into(), source_ext: ext.into(), source_format: format, animated, edits: Edits::default(), source_path: None, last: None, dims_after: None, note: None,`.

In `Job::file`, set `dims: edit::raster::display_dims(&bytes, format), source_ext: ext.clone(), source_format: format, animated: sniff::is_animated(&bytes, format), edits: Edits::default(), source_path: Some(path.to_owned()), last: None, dims_after: None, note: None,` (keep `ext` as before; clone it for `source_ext` before the move).

- [ ] **Step 5: `update`, `detail`, `edit`, `fork`, `reason`**

Replace `update`:

```rust
    /// This job's card in `state`.
    pub fn update(&self, state: JobState, after: Option<u64>) -> JobUpdate {
        let done = state == JobState::Done;
        let busy = matches!(state, JobState::Queued | JobState::Optimising { .. });
        let editable = self.source_format == Format::Gif || !self.animated;
        JobUpdate {
            id: self.id.clone(),
            can_restore: done && self.backup.is_some(),
            can_retry: matches!(state, JobState::Error { .. }),
            detail: if done { self.detail() } else { String::new() },
            name: self.name.clone(),
            ext: self.ext.clone(),
            before: self.before,
            after,
            thumb_path: self.backup.clone(),
            source: self.source,
            output: self.output.exists().then(|| self.output.clone()),
            dims_before: self.dims,
            dims_after: if done { self.dims_after } else { None },
            format: Some(self.format),
            converted: self.format != self.source_format,
            can_edit: !busy && editable && self.backup.as_ref().is_some_and(|b| b.exists()),
            edits: self.edits.clone(),
            note: if done { self.note.clone() } else { None },
            state,
        }
    }
```

Replace the size part of `detail` and its format label:

```rust
    /// "2880×1800 · PNG" or "2880×1800 → 1440×900 · WebP", plus "aggressive"
    /// and "from bitmap" when they apply.
    fn detail(&self) -> String {
        let mut parts = Vec::new();
        match (self.dims, self.dims_after) {
            (Some((w, h)), Some((aw, ah))) => parts.push(format!("{w}×{h} → {aw}×{ah}")),
            (Some((w, h)), None) => parts.push(format!("{w}×{h}")),
            _ => {}
        }
        parts.push(self.format.label().to_owned());
        if self.level == Level::Aggressive {
            parts.push(t("card.detail.aggressive"));
        }
        if self.from_bitmap {
            parts.push(t("card.detail.bitmap"));
        }
        parts.join(" · ")
    }
```

In `reason`, use `self.source_format.label()` instead of `label(self.format)`, then delete the private `fn label` (now `Format::label`).

Add after `update`:

```rust
    /// Merge what the card asked for. Asking for the source format, or for
    /// GIF, clears the conversion.
    pub fn edit(&mut self, patch: EditsPatch) {
        self.edits.merge(patch);
        if matches!(self.edits.format, Some(f) if f == self.source_format || f == Format::Gif) {
            self.edits.format = None;
        }
        self.level = self.edits.level;
    }

    /// A second job on the same original, for "Convert and keep both". It has
    /// its own id, card and result; the backup is shared.
    pub fn fork(&self) -> Job {
        let id = JobId::generate();
        let output = match self.source {
            Source::Clipboard => self
                .output
                .parent()
                .and_then(Path::parent)
                .unwrap_or(Path::new(""))
                .join(id.as_str())
                .join(format!("{}.{}", self.name, self.source_ext)),
            Source::File => self.source_path.clone().unwrap_or_else(|| self.output.clone()),
        };
        Job {
            id,
            source: self.source,
            name: self.name.clone(),
            ext: self.source_ext.clone(),
            format: self.source_format,
            before: self.before,
            dims: self.dims,
            from_bitmap: self.from_bitmap,
            output,
            level: self.level,
            edits: self.edits.clone(),
            source_format: self.source_format,
            source_ext: self.source_ext.clone(),
            animated: self.animated,
            source_path: self.source_path.clone(),
            last: None,
            dims_after: None,
            note: None,
            backup: self.backup.clone(),
            pending: self.pending.clone(),
            times: self.times,
            changed: false,
        }
    }
```

- [ ] **Step 6: `run`, `restore` and the writers**

Replace `run`:

```rust
    /// Apply the recipe at `level` to the original, write the result and
    /// report each step. `None` means the job was cancelled before anything
    /// was written. A plain run that does not come out smaller leaves the
    /// original as the result (Cycle 1); an explicit edit always writes.
    pub fn run(
        &mut self,
        level: Level,
        o: &RunOptions,
        backups: &Backups,
        emit: &mut dyn FnMut(JobUpdate),
        deliver: Deliver,
    ) -> Option<Finished> {
        self.level = level;
        self.edits.level = level;
        if o.cancel.is_cancelled() {
            return None;
        }
        let original = match self.original(backups) {
            Ok(bytes) => bytes,
            Err(e) => return Some(self.fail(tf("error.backup", &[("why", &describe(&e))]), e)),
        };
        let plain = self.edits.is_plain();
        let step = t(match (plain, self.source_format) {
            (false, _) => "card.step.edit",
            (true, Format::Png) => "card.step.png",
            (true, Format::Jpeg) => "card.step.jpeg",
            (true, Format::WebP) => "card.step.webp",
            (true, Format::Gif) => "card.step.gif",
        });
        emit(self.update(JobState::Optimising { progress: 0.0, step }, None));
        let options = OptimiseOptions {
            level,
            compression: o.compression,
            strip_metadata: o.strip_metadata,
            gifsicle: o.gifsicle.clone(),
            cancel: o.cancel.clone(),
        };
        // (state, Some((bytes, format, size after, fit target missed)))
        let (state, result) = if plain {
            match optimise::optimise(&original, self.source_format, &options) {
                Ok(Outcome::Smaller { bytes }) => {
                    (JobState::Done, Some((bytes, self.source_format, None, false)))
                }
                Ok(Outcome::AlreadyOptimal) => (JobState::AlreadyOptimal, None),
                Ok(Outcome::Larger) => (JobState::Larger, None),
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        } else {
            match edit::apply(&original, self.source_format, &self.edits, &options) {
                Ok(a) => (JobState::Done, Some((a.bytes, a.format, Some(a.dims), a.missed_target))),
                Err(OptimiseError::Cancelled) => return None,
                Err(e) => return Some(self.fail(self.reason(&e), e)),
            }
        };
        // The last moment a cancel counts: after this the user's data changes.
        if o.cancel.is_cancelled() {
            return None;
        }
        let written = match result {
            Some((bytes, format, dims_after, missed)) => {
                self.dims_after = dims_after.filter(|d| Some(*d) != self.dims);
                self.note = missed.then(|| {
                    let target = bytes_label(self.edits.fit_under.unwrap_or_default());
                    tf("card.note.fit_missed", &[("size", &target)])
                });
                let after = bytes.len() as u64;
                self.put(&bytes, format, &original, o, deliver).map(|()| Some(after))
            }
            None => {
                self.dims_after = None;
                self.note = None;
                let replace = self.changed;
                self.put_back(&original, replace, o, deliver).map(|()| None)
            }
        };
        Some(match written {
            Ok(after) => Finished { update: self.update(state, after), cause: None },
            Err((reason, cause)) => self.fail(reason, cause),
        })
    }
```

Replace `restore`:

```rust
    /// Put the original back (§5.5) and clear the recipe: in place for a file
    /// (and the conversion to the Trash); in the working file and on the
    /// clipboard for a clipboard image.
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
        self.edits = Edits::default();
        self.dims_after = None;
        self.note = None;
        if let Err((reason, cause)) = self.put_back(&original, true, o, deliver) {
            return self.fail(reason, cause);
        }
        Finished {
            update: self.update(JobState::Restored, None),
            cause: None,
        }
    }
```

In `original`, use the source path for a file job's first backup: replace `self.times = fsutil::read_times(&self.output).ok(); backups.store_file(self.id.as_str(), &self.output)?` with

```rust
                    let source = self.source_path.clone().unwrap_or_else(|| self.output.clone());
                    self.times = fsutil::read_times(&source).ok();
                    backups.store_file(self.id.as_str(), &source)?
```

Delete the old `write` and add:

```rust
    /// Make `bytes`, in `format`, the job's one result. A file job's result
    /// goes where the output rules say; a previous result elsewhere is taken
    /// back first (spec §4.4).
    fn put(
        &mut self,
        bytes: &[u8],
        format: Format,
        original: &[u8],
        o: &RunOptions,
        deliver: Deliver,
    ) -> Result<(), (String, String)> {
        let ext = if format == self.source_format {
            self.source_ext.clone()
        } else {
            format.extension().to_owned()
        };
        match self.source {
            Source::Clipboard => {
                let path = self.output.with_file_name(format!("{}.{ext}", self.name));
                self.write_work_file(&path, bytes)?;
                deliver(format, bytes, &self.output).map_err(|e| (t("error.clipboard_write"), e))?;
            }
            Source::File => {
                let source = self.source_path.clone().unwrap_or_else(|| self.output.clone());
                let kind = if format == self.source_format {
                    OutputKind::Optimised
                } else {
                    OutputKind::Converted
                };
                let dims = self.dims_after.or(self.dims).unwrap_or_default();
                let target = output::Target { source: &source, job: self.id.as_str(), ext: &ext, dims, kind };
                let wanted = output::resolve(&target, &o.output, &o.work_dir, &o.home, Local::now().date_naive());
                let own = self.last.as_ref().map(|w| w.path.clone());
                let path = if wanted == source { wanted } else { output::unique(&wanted, own.as_deref()) };
                if let Some(last) = self.last.clone().filter(|w| w.path != path) {
                    self.undo(&last, original, o)?;
                    self.last = None;
                }
                self.write_file(&path, bytes, &source, o)?;
                let trash_source = kind == OutputKind::Converted
                    && o.output.converted.location == Location::InPlace
                    && path != source;
                if trash_source && source.exists() {
                    let _ = (o.trash)(&source);
                }
                let source_changed = path == source
                    || trash_source
                    || self.last.as_ref().is_some_and(|w| w.source_changed);
                self.last = Some(Written { path: path.clone(), created: path != source, source_changed });
                self.output = path;
            }
        }
        self.format = format;
        self.ext = ext;
        self.changed = true;
        Ok(())
    }

    /// Leave the original as the result: a clipboard job's working file holds
    /// it (on the clipboard too when `replace`); a file job's previous result
    /// is taken back.
    fn put_back(
        &mut self,
        original: &[u8],
        replace: bool,
        o: &RunOptions,
        deliver: Deliver,
    ) -> Result<(), (String, String)> {
        match self.source {
            Source::Clipboard => {
                let path = self.output.with_file_name(format!("{}.{}", self.name, self.source_ext));
                self.write_work_file(&path, original)?;
                if replace {
                    deliver(self.source_format, original, &self.output)
                        .map_err(|e| (t("error.clipboard_write"), e))?;
                }
            }
            Source::File => {
                if let Some(last) = self.last.clone() {
                    self.undo(&last, original, o)?;
                    self.last = None;
                }
                if let Some(source) = &self.source_path {
                    self.output = source.clone();
                }
            }
        }
        self.format = self.source_format;
        self.ext = self.source_ext.clone();
        self.changed = false;
        Ok(())
    }

    /// Take a file job's result back: a file Ferry made goes to the Trash; a
    /// source Ferry overwrote or trashed gets the original again.
    fn undo(&self, last: &Written, original: &[u8], o: &RunOptions) -> Result<(), (String, String)> {
        if last.created && last.path.exists() {
            let _ = (o.trash)(&last.path);
        }
        if last.source_changed
            && let Some(source) = &self.source_path
        {
            self.write_file(source, original, source, o)?;
        }
        Ok(())
    }

    /// Write a file job's result atomically, with the original's extended
    /// attributes, and its dates when it replaces the source.
    fn write_file(&self, path: &Path, bytes: &[u8], source: &Path, o: &RunOptions) -> Result<(), (String, String)> {
        let folder = path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(folder)
            .and_then(|()| fsutil::atomic_replace(path, bytes))
            .map_err(|e| (tf("error.write", &[("dir", &shown(folder))]), e.to_string()))?;
        // Best effort: the content is safe; a lost tag or date is not worth an Error card.
        if let Some(backup) = &self.backup {
            let _ = fsutil::copy_xattrs(backup, path);
        }
        if o.preserve_dates
            && path == source
            && let Some(times) = &self.times
        {
            let _ = fsutil::set_times(path, times);
        }
        Ok(())
    }

    /// A clipboard job's working file (drag and reveal use it), renamed when
    /// the result's extension changes.
    fn write_work_file(&mut self, path: &Path, bytes: &[u8]) -> Result<(), (String, String)> {
        let folder = path.parent().unwrap_or(Path::new(""));
        fs::create_dir_all(folder)
            .and_then(|()| fs::write(path, bytes))
            .map_err(|e| (tf("error.write", &[("dir", &shown(folder))]), e.to_string()))?;
        if path != self.output {
            let _ = fs::remove_file(&self.output);
            self.output = path.to_owned();
        }
        Ok(())
    }
```

Add near `describe`:

```rust
/// "1 MB", "250 KB", "1.5 MB": decimal units, like formatBytes in src/lib/format.ts.
fn bytes_label(bytes: u64) -> String {
    const UNITS: [&str; 5] = ["B", "KB", "MB", "GB", "TB"];
    let (mut value, mut unit) = (bytes as f64, 0);
    while value >= 1000.0 && unit < UNITS.len() - 1 {
        value /= 1000.0;
        unit += 1;
    }
    let text = if unit > 0 && value < 100.0 {
        let one_decimal = format!("{value:.1}");
        one_decimal.trim_end_matches(".0").to_owned()
    } else {
        format!("{value:.0}")
    };
    format!("{text} {}", UNITS[unit])
}
```

- [ ] **Step 7: Wire the app**

In `src-tauri/Cargo.toml` `[dependencies]` add `trash = "5.2"` (alphabetical, after `tracing-subscriber`). In `src-tauri/src/jobs.rs` replace `run_options` and add the Trash function:

```rust
fn run_options(app: &AppHandle, cancel: CancelToken) -> RunOptions {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap();
    RunOptions::new(
        &settings,
        state.gifsicle.clone(),
        state.work_dir.clone(),
        move_to_trash,
        cancel,
    )
}

/// Move a file to the Trash / Recycle Bin. A failure leaves it where it is
/// and is logged (spec §6).
fn move_to_trash(path: &Path) -> std::io::Result<()> {
    trash::delete(path).map_err(|e| {
        warn!(path = %path.display(), "could not move to the Trash: {e}");
        std::io::Error::other(e.to_string())
    })
}
```

`src-tauri/src/state.rs` tests build jobs with `Job::clipboard` and `job.update(...)` only; they compile unchanged.

- [ ] **Step 8: Run the tests to verify they pass**

Run: `cargo test -p ferry-core pipeline && cargo test --workspace`
Expected: PASS — the 12 new pipeline tests and every Cycle 1 pipeline test (`a_file_is_replaced_in_place_with_its_dates_kept`, `a_re_run_that_does_not_shrink_puts_the_original_back`, `restore_puts_the_original_back_in_place_and_on_the_clipboard`, `a_read_only_file_is_an_error_naming_its_folder_and_stays_untouched`, …) unchanged. If a Cycle 1 test fails, the new write path changed Cycle 1 behaviour: fix the code (`put` / `put_back` / `undo`), never the old test.

- [ ] **Step 9: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/job.rs crates/ferry-core/src/pipeline.rs src-tauri/Cargo.toml src-tauri/src/jobs.rs Cargo.lock src/lib/i18n/en.json
git commit -m "feat(core): jobs run their edit recipe and own one result

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 10: Rename

**Files:**
- Modify: `crates/ferry-core/src/pipeline.rs` (`RenameError`, `Job::rename`, `check_stem`)
- Modify: `src/lib/i18n/en.json` (`rename.empty`, `rename.invalid`, `rename.taken`, `rename.gone`)

**Interfaces:**
- Consumes: `Job.output`, `Job.source_path`, `Job.last` (Task 9).
- Produces:
  - `pipeline::RenameError { Empty, Invalid, Taken, Gone, Failed { reason: String } }` (`Debug, Clone, PartialEq, Eq, Serialize`, `#[serde(tag = "kind", rename_all = "snake_case")]`), with `RenameError::message(&self) -> String` from `en.json`.
  - `Job::rename(&mut self, stem: &str) -> Result<(), RenameError>` — renames the current result in its folder; later runs keep the new stem.

- [ ] **Step 1: Write the failing tests**

Append to the pipeline tests module:

```rust
    fn done_file(env: &Env, name: &str) -> (Job, PathBuf) {
        let path = env.file(name, &testutil::png(300, 200));
        let mut job = Job::file(&path, &Settings::default(), true).unwrap();
        run(&mut job, Level::Normal, &options(), env);
        (job, path)
    }

    #[test]
    fn rename_moves_the_file_and_later_runs_keep_the_name() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        job.rename("hero").unwrap();
        let hero = env.dir.path().join("hero.png");
        assert!(hero.exists() && !path.exists());
        assert_eq!(job.name, "hero");
        job.edit(scale(50));
        let update = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
        assert_eq!(update.output, Some(hero.clone()));
        assert_eq!(sniff::dimensions(&fs::read(&hero).unwrap(), Format::Png), Some((150, 100)));
    }

    #[test]
    fn rename_refuses_bad_and_taken_names() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        env.file("taken.png", b"x");
        assert_eq!(job.rename("  "), Err(RenameError::Empty));
        for bad in ["a/b", "a\\b", "a:b", "x.", "x ", "CON", "lpt1", "nul.txt"] {
            assert_eq!(job.rename(bad), Err(RenameError::Invalid), "{bad}");
        }
        assert_eq!(job.rename("taken"), Err(RenameError::Taken));
        assert_eq!(RenameError::Taken.message(), "Name taken");
    }

    #[test]
    fn a_case_only_rename_is_allowed() {
        let env = Env::new();
        let (mut job, _) = done_file(&env, "shot.png");
        job.rename("Shot").unwrap();
        let names: Vec<String> = fs::read_dir(env.dir.path())
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(names.contains(&"Shot.png".to_owned()), "{names:?}");
    }

    #[test]
    fn renaming_a_vanished_file_says_so() {
        let env = Env::new();
        let (mut job, path) = done_file(&env, "shot.png");
        fs::remove_file(path).unwrap();
        assert_eq!(job.rename("hero"), Err(RenameError::Gone));
        assert_eq!(job.name, "shot");
    }

    #[test]
    fn a_clipboard_rename_renames_the_working_file() {
        let env = Env::new();
        let (mut job, _) = clipboard_png(&env, "%f");
        let first = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
        job.rename("diagram").unwrap();
        assert_eq!(job.output.file_name().unwrap(), "diagram.png");
        assert!(job.output.exists() && !first.output.unwrap().exists());
    }
```

Add to `src/lib/i18n/en.json`: `"rename.empty": "Name can't be empty",`, `"rename.invalid": "Can't use / \\ : or a reserved name",`, `"rename.taken": "Name taken",`, `"rename.gone": "File no longer there",`.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core pipeline`
Expected: FAIL to compile — `RenameError`, `Job::rename` missing.

- [ ] **Step 3: Implement**

Add `use serde::Serialize;` to `pipeline.rs` and, after `Written`:

```rust
/// Why a rename was refused; the card shows `message()` inline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RenameError {
    Empty,
    /// Separators, characters Windows forbids, a trailing dot or space, or a
    /// reserved device name. Refused on every OS so names travel.
    Invalid,
    Taken,
    Gone,
    Failed { reason: String },
}

impl RenameError {
    pub fn message(&self) -> String {
        match self {
            Self::Empty => t("rename.empty"),
            Self::Invalid => t("rename.invalid"),
            Self::Taken => t("rename.taken"),
            Self::Gone => t("rename.gone"),
            Self::Failed { reason } => reason.clone(),
        }
    }
}

/// A file stem every OS accepts.
fn check_stem(stem: &str) -> Result<(), RenameError> {
    if stem.trim().is_empty() {
        return Err(RenameError::Empty);
    }
    let forbidden = |c: char| matches!(c, '/' | '\\' | ':' | '<' | '>' | '"' | '|' | '?' | '*') || c.is_control();
    let device = stem.split('.').next().unwrap_or_default().to_ascii_uppercase();
    let reserved = matches!(device.as_str(), "CON" | "PRN" | "AUX" | "NUL")
        || ((device.starts_with("COM") || device.starts_with("LPT"))
            && device.len() == 4
            && device.as_bytes()[3].is_ascii_digit()
            && device.as_bytes()[3] != b'0');
    if stem.contains(forbidden) || stem.ends_with(['.', ' ']) || reserved {
        return Err(RenameError::Invalid);
    }
    Ok(())
}
```

and in `impl Job`:

```rust
    /// Rename the current result in its folder (a file job's real file, a
    /// clipboard job's working file). Later runs keep the new stem.
    pub fn rename(&mut self, stem: &str) -> Result<(), RenameError> {
        check_stem(stem)?;
        if !self.output.exists() {
            return Err(RenameError::Gone);
        }
        let file = if self.ext.is_empty() {
            stem.to_owned()
        } else {
            format!("{stem}.{}", self.ext)
        };
        let to = self.output.with_file_name(file);
        // On a case-insensitive disk "Shot.png" exists because it is "shot.png".
        let same_file = fs::canonicalize(&to).ok() == fs::canonicalize(&self.output).ok();
        if to.exists() && !same_file {
            return Err(RenameError::Taken);
        }
        fs::rename(&self.output, &to).map_err(|e| RenameError::Failed { reason: describe(&e) })?;
        if self.source_path.as_ref() == Some(&self.output) {
            self.source_path = Some(to.clone());
        }
        if let Some(last) = self.last.as_mut().filter(|w| w.path == self.output) {
            last.path = to.clone();
        }
        self.output = to;
        self.name = stem.to_owned();
        Ok(())
    }
```

`check_stem("  ")` is `Empty` (trim), while `"x "` is `Invalid` (trailing space); the test expects exactly that. A name that only differs by case maps to the same file on macOS and Windows; `fs::rename` performs the case change there.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core pipeline && cargo test --workspace`
Expected: PASS (5 new tests).

- [ ] **Step 5: Lint and commit**

```bash
cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/pipeline.rs src/lib/i18n/en.json
git commit -m "feat(core): rename a card's file

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

## Final verification (after Task 10)

```bash
source "$HOME/.cargo/env"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
pnpm vitest run && pnpm svelte-check
```

Expected: all green; the GIF tests ran (not skipped) because `FERRY_GIFSICLE` is set. Then smoke the unchanged app: `pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'`, copy a screenshot, and check the card still reads "… → … −NN%" with the detail "W×H · PNG" exactly as in 0.1.1 (defaults keep Cycle 1 behaviour).
