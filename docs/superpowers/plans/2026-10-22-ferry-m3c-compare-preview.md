# M3c Compare and Preview Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. One Sonnet implementer runs every task in order with no per-task review; one Opus reviewer checks the whole branch at the end. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give every result card a compare window (original beside the current result, Slider / Side by side, Fit / 100 % / 200 %) and a quick preview (Quick Look on macOS, the compare window in single-image mode on Windows), opened by two Tauri commands that M3b's rail, menu and hover hotkeys call.

**Architecture:** One reusable `compare` webview window (`src/compare.html`, `Compare.svelte`) is built on first use, hidden on close and reshown for the next card. Rust builds a small `CompareView` from the card's latest `JobUpdate` (backup path, result path, sizes, formats), allows both files on the asset protocol, sizes the window to the image (at most 80 % of the cards' screen, a pure tested function in `ferry-core::geometry`), and remembers which app had focus so closing hands focus back. On macOS `quick_preview` shows Quick Look's shared panel (falling back to `qlmanage -p`); on Windows it opens the compare window with the result alone. The compare window and the Quick Look panel follow "Show app UI in screenshots", as the stack does.

**Tech Stack:** Tauri 2.12 (`WebviewWindowBuilder`, asset protocol scope), Svelte 5 + TypeScript, vitest 5; Rust 2024; `objc2` 0.6.5 / `objc2-app-kit` 0.3.2 (already in `src-tauri`), new `objc2-quick-look-ui` 0.3.2 (macOS only, Zlib/Apache-2.0/MIT); `windows-sys` 0.61 (already in `src-tauri`).

**Spec:** [docs/superpowers/specs/2026-10-22-ferry-cycle2-card-tools-design.md](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) (M3c row of §8; §2 "Quick preview", §4.1, §4.5 "Compare window" and "Quick preview", §6 "Backup expired", §9 "Quick Look from an inactive app"). Approved mockup: `.superpowers/brainstorm/78228-1792661273/content/compare.html` (local, git-ignored).

**Depends on:** M3a (`docs/superpowers/plans/2026-10-22-ferry-m3a-edit-engine.md`) merged into `main`. This plan reads M3a's `JobUpdate` fields and `Format::label()`. It does not need M3b; M3b calls this plan's two commands.

## Global Constraints

- Every Rust module opens with a `//!` doc comment; tests live in a `#[cfg(test)] mod tests` at the end of the file. Match the surrounding comment density and naming.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings` must be clean after every task. `pnpm check` (svelte-check) and `pnpm test` (vitest) must be clean after every task that touches `src/`.
- UI text lives in `src/lib/i18n/en.json` (flat map, shared with Rust through `ferry_core::i18n::{t, tf}`). Never hard-code English UI text in Rust or Svelte.
- Never use `var` in JavaScript or TypeScript.
- Do not add a test stack. The repo has no jsdom or testing-library and Tauri's `test` feature is off, so component behaviour is tested through pure helpers (`src/compare/view.ts`, `compare::view`) and the manual checks in Task 6.
- Exact copy from the spec and mockup: "Original no longer kept"; labels "Original", "Now", "Slider", "Side by side", "Fit", "100%", "200%".
- The compare window is at most 80 % of the cards' screen each way. A downscaled result is drawn at the original's size so pixels line up. Esc or Space closes; the window takes focus while open and gives it back on close.
- Quick preview: macOS `QLPreviewPanel`, fallback `qlmanage -p`; Windows: the compare window in single-image mode.
- The compare window and the Quick Look panel follow `general.show_in_screenshots` (default false = hidden from screen capture) at creation and live when the setting changes. The Settings window is not affected.
- Windows behaviour is built but not verified on a real desktop (spec §1). No Windows Rust target is installed here, so Windows-only code is checked by reading it against the `windows-sys` 0.61 signatures quoted in Task 4.
- Implementers run on model `sonnet`; plan writing and reviews run on `opus` (never `fable`).
- Commit messages end with `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **A result rewritten at the same path** (convert, then downscale, then ⌘D again). Expected: the window shows the new result, not the webview's cached copy of the old one. Pinned in Task 2 (`versioned` test) and Task 4 (`a_finished_card_shows_its_original_beside_its_result` asserts `version > 0`).
2. **The backup expired or was pruned** while the card is still on screen. Expected: compare refuses with "Original no longer kept", but quick preview still shows the result. Pinned in Task 4 (`an_expired_backup_means_the_original_is_no_longer_kept`, `single_image_mode_needs_no_backup`).
3. **A cropped result** (another shape than the original). Expected: it is not stretched into the original's box; it is shown at its own size, centred. Pinned in Task 2 (`nowBox` "keeps a cropped result at its own size").
4. **Very large, tiny or thin images** (a 6000×4000 photo, a 16×16 icon, a 100×5000 strip, unknown size). Expected: a window no larger than 80 % of the screen and never too small for its toolbar. Pinned in Task 1 (`a_tiny_or_thin_image_still_gets_a_usable_window`, `the_minimum_never_outgrows_a_small_screen`).
5. **Pressing ⌘D or Space again while the window is open.** The hover hotkey takes Space before the page sees it. Expected: the second press closes the window instead of reloading it. Pinned in Task 4 (`asking_again_for_what_is_on_show_toggles_it`) and checked by hand in Task 6.

## Shared files (M3b runs in parallel)

M3b (card UI) is written and may be executed at the same time in another worktree. These files are edited by both plans. Every edit here is additive at the anchor named, so a merge conflict, if any, is resolved by keeping both sides:

| File | This plan's edit | Anchor |
| --- | --- | --- |
| `src-tauri/src/lib.rs` | `mod compare;` and `mod preview;`; four command lines in `generate_handler!` | `mod compare;` right after `mod commands;`; `mod preview;` right after `mod prefs;`; commands right after `windows::stack_resize,` (the first entry), not at the end |
| `src-tauri/src/windows.rs` | `follows_screenshot_setting` added and `set_content_protected` body replaced; a `tests` module appended at the end of the file | the existing `set_content_protected` (lines 48–53 on `main`) |
| `src-tauri/src/prefs.rs` | **not modified**: its call `windows::set_content_protected(app, !new.general.show_in_screenshots)` keeps its signature and now reaches the compare window and Quick Look too | — |
| `src-tauri/src/platform/mod.rs` | one `pub mod preview;` block per OS | rustfmt sorts it after `mod macos;` (macOS) and before `mod windows;` (Windows); M3b's edits to the `pub use` lines sit next to it, so keep both if git conflicts |
| `src-tauri/Cargo.toml` | one `objc2-quick-look-ui` line | right after the `objc2-foundation` line in the macOS section; the existing `objc2-app-kit` features line is not touched |
| `Cargo.lock` | `objc2-quick-look-ui` and its dependencies | if it conflicts, take either side and run `cargo check -p ferry` to regenerate |
| `src-tauri/capabilities/default.json` | `"compare"` appended to `windows` | the `windows` array |
| `src/lib/i18n/en.json` | 13 `compare.*` keys | right after `"stack.window_title": "Ferry results",`, not at the end |
| `vite.config.ts` | `compare` page in `rolldownOptions.input` | the `input` object |

This plan does **not** touch `src/lib/ipc.ts`, `src-tauri/src/commands.rs`, `src-tauri/src/jobs.rs` or `src-tauri/src/state.rs`. The TS `JobUpdate` type stays M3b's job (per M3a's handoff); the compare page has its own types in `src/compare/view.ts`.

## Interfaces this plan provides to M3b

- `invoke('open_compare', { id })`: `id` is the card's `JobUpdate.id`. Resolves when the window is shown. Calling it again for the card already on show, while the window has focus, closes the window (⌘D toggles). Rejects with a plain string: `"Original no longer kept"` (backup missing), `"File no longer there"` (result missing), `"the job is still running"` or `"this card's job is gone"`.
- `invoke('quick_preview', { id })`: macOS toggles Quick Look on the card's current result; Windows toggles the compare window in single-image mode. When the compare window has focus, any call closes it (Space closes it even though the hover hotkey takes the key). Rejects with `"File no longer there"`, `"the job is still running"` or `"this card's job is gone"`. It does not need the backup.
- Both read the card's latest `Entry::last` (after a convert or rename), never the stale `Entry::output`.
- While the compare window or Quick Look has focus and the pointer still rests on a card, M3b's other bare-key hover hotkeys (`1`–`9`, `-`) would still edit that card. M3c handles Space and ⌘D; M3b may want to suspend the rest while either is open.
- Per spec §6, M3b disables the Compare button and menu item when `can_edit` is false, with the tooltip "Original no longer kept". This plan adds the key `compare.original_gone` with that text; M3b may reuse it.

## M3a interfaces this plan consumes

- `ferry_core::job::JobUpdate` fields `id`, `state`, `name`, `ext`, `thumb_path` (the backup of the original), `output` (the current result, `None` when missing), `dims_before: Option<(u32, u32)>`, `dims_after: Option<(u32, u32)>` (`None` when the size did not change), plus `format`, `converted` and the rest only through `..pipeline::notice(..)` in tests.
- `ferry_core::sniff::Format::label(self) -> &'static str` (`"PNG"`, `"JPEG"`, `"GIF"`, `"WebP"`) and `ferry_core::sniff::sniff(&[u8]) -> Option<Format>`.
- `ferry_core::pipeline::notice(reason: String) -> JobUpdate` (its literal fills every M3a field).
- `crate::state::{AppState, Entry}` with `Entry::last: Mutex<JobUpdate>` (unchanged since M2b).

---

## Setup (once, before Task 1)

```bash
source "$HOME/.cargo/env"
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
git checkout main && git pull --ff-only 2>/dev/null; git checkout -b feat/m3c-compare-preview
grep -n "pub dims_before" crates/ferry-core/src/job.rs
```

Expected: the last command prints one line. If it prints nothing, M3a is not merged yet: stop and report.

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/geometry.rs` (modify) | `fit_image`, `center`: the compare window's size and position | 1 |
| `src/compare/view.ts` (create) | page types and pure helpers: labels, result box, zoom scale, pan, divider, cache-busting URL | 2 |
| `src/compare/view.test.ts` (create) | vitest for `view.ts` | 2 |
| `src/compare.html`, `src/compare/main.ts`, `src/compare/Compare.svelte` (create) | the compare page | 3 |
| `vite.config.ts` (modify) | build the compare page | 3 |
| `src/lib/i18n/en.json` (modify) | `compare.*` strings | 3 |
| `src-tauri/src/compare.rs` (create) | `CompareView`, the window, `open_compare` / `compare_ready` / `compare_close` | 4 |
| `src-tauri/src/platform/macos_preview.rs` (create) | remember / give back focus (Task 4); Quick Look (Task 5) | 4, 5 |
| `src-tauri/src/platform/windows_preview.rs` (create) | remember / give back focus; no-op `set_protected` | 4, 5 |
| `src-tauri/src/platform/mod.rs` (modify) | `pub mod preview` per OS | 4 |
| `src-tauri/src/windows.rs` (modify) | screenshot setting reaches the compare window and Quick Look | 4, 5 |
| `src-tauri/src/lib.rs` (modify) | register modules and commands | 4, 5 |
| `src-tauri/capabilities/default.json` (modify) | IPC for the `compare` window | 4 |
| `src-tauri/src/preview.rs` (create) | `quick_preview` | 5 |
| `src-tauri/Cargo.toml` (modify) | `objc2-quick-look-ui` | 5 |

---

### Task 1: Window size maths

**Files:**
- Modify: `crates/ferry-core/src/geometry.rs` (module doc line 1; add two functions before `#[cfg(test)]`; add tests at the end of `mod tests`)

**Interfaces:**
- Consumes: `geometry::Rect { x: i32, y: i32, width: u32, height: u32 }` (existing).
- Produces:
  - `ferry_core::geometry::fit_image(area: Rect, image: (u32, u32), chrome: (u32, u32), share: f64, min: (u32, u32)) -> (u32, u32)`: window size in the same units as `area` (physical pixels).
  - `ferry_core::geometry::center(area: Rect, size: (u32, u32)) -> (i32, i32)`: top-left position.

- [ ] **Step 1: Write the failing tests**

In `crates/ferry-core/src/geometry.rs`, inside `mod tests`, after the last test (`fit_clamps_width_and_never_underflows`) and before the module's closing `}`, add:

```rust
    const CHROME: (u32, u32) = (0, 80);
    const MIN: (u32, u32) = (520, 360);

    #[test]
    fn a_small_image_is_shown_at_its_own_size() {
        assert_eq!(fit_image(FULL_HD, (800, 600), CHROME, 0.8, MIN), (800, 680));
    }

    #[test]
    fn a_large_image_shrinks_to_eighty_percent_keeping_its_shape() {
        // 80% of 1080 is 864; 784 px are left for the image under the bars.
        assert_eq!(
            fit_image(FULL_HD, (2880, 1800), CHROME, 0.8, MIN),
            (1254, 864)
        );
    }

    #[test]
    fn a_tiny_or_thin_image_still_gets_a_usable_window() {
        assert_eq!(fit_image(FULL_HD, (16, 16), CHROME, 0.8, MIN), (520, 360));
        assert_eq!(
            fit_image(FULL_HD, (100, 5000), CHROME, 0.8, MIN),
            (520, 864)
        );
        assert_eq!(fit_image(FULL_HD, (0, 0), CHROME, 0.8, MIN), (520, 360));
    }

    #[test]
    fn the_minimum_never_outgrows_a_small_screen() {
        let small = Rect {
            x: 0,
            y: 0,
            width: 400,
            height: 300,
        };
        assert_eq!(fit_image(small, (16, 16), CHROME, 0.8, MIN), (400, 300));
    }

    #[test]
    fn center_puts_the_window_in_the_middle_of_its_monitor() {
        assert_eq!(center(FULL_HD, (1254, 864)), (333, 108));
        let left = Rect {
            x: -1440,
            y: 0,
            width: 1440,
            height: 900,
        };
        assert_eq!(center(left, (500, 300)), (-970, 300));
        assert_eq!(center(FULL_HD, (2000, 1200)), (0, 0));
    }
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p ferry-core --lib geometry`
Expected: FAIL to compile with "cannot find function `fit_image` in this scope" and "cannot find function `center`".

- [ ] **Step 3: Implement**

Change the module doc (line 1) to:

```rust
//! Where Ferry's windows go on screen: the results stack and the compare window.
```

Add these two functions right after `fit` and before `#[cfg(test)]`:

```rust
/// The size of a window that shows an image of `image` pixels at 1:1 plus
/// `chrome` (its toolbars, in pixels). A larger image is shrunk, keeping its
/// shape, until the window takes at most `share` of `area` each way. The
/// window is never smaller than `min`, nor larger than `area`.
pub fn fit_image(
    area: Rect,
    image: (u32, u32),
    chrome: (u32, u32),
    share: f64,
    min: (u32, u32),
) -> (u32, u32) {
    let most = |extent: u32| (f64::from(extent) * share).floor();
    let (most_w, most_h) = (most(area.width), most(area.height));
    let (w, h) = (f64::from(image.0.max(1)), f64::from(image.1.max(1)));
    let room_w = (most_w - f64::from(chrome.0)).max(1.0);
    let room_h = (most_h - f64::from(chrome.1)).max(1.0);
    let k = (room_w / w).min(room_h / h).min(1.0);
    let side = |pixels: f64, chrome: u32, min: u32, extent: u32| {
        ((pixels * k).round() as u32 + chrome).max(min).min(extent)
    };
    (
        side(w, chrome.0, min.0, area.width),
        side(h, chrome.1, min.1, area.height),
    )
}

/// Top-left position that centres a window of `size` in `area`; a window
/// larger than the area starts at its top-left corner.
pub fn center(area: Rect, size: (u32, u32)) -> (i32, i32) {
    let gap = |extent: u32, side: u32| i64::from(extent.saturating_sub(side) / 2);
    (
        (i64::from(area.x) + gap(area.width, size.0)) as i32,
        (i64::from(area.y) + gap(area.height, size.1)) as i32,
    )
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `cargo test -p ferry-core --lib geometry && cargo fmt --all --check && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: 0 failed, and the five new tests (`geometry::tests::a_small_image_is_shown_at_its_own_size`, `a_large_image_shrinks_to_eighty_percent_keeping_its_shape`, `a_tiny_or_thin_image_still_gets_a_usable_window`, `the_minimum_never_outgrows_a_small_screen`, `center_puts_the_window_in_the_middle_of_its_monitor`) are listed as `ok`. The filter also runs M3a's `edit::geometry` tests, so the total is higher than the 13 in this file. fmt and clippy print nothing else and exit 0.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/geometry.rs
git commit -m "feat(core): compare window size and centring maths

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 2: Compare page helpers

**Files:**
- Create: `src/compare/view.ts`
- Test: `src/compare/view.test.ts`

**Interfaces:**
- Consumes: `formatBytes(bytes: number): string` and `savingPercent(before: number, after: number): string` from `src/lib/format.ts` (existing).
- Produces (all exported from `src/compare/view.ts`):
  - `interface Shot { path: string; bytes: number; dims: [number, number] | null; format: string; version: number }` (mirrors Rust `compare::Shot`, Task 4).
  - `interface CompareView { id: string; name: string; original: Shot | null; now: Shot }` (mirrors Rust `compare::CompareView`).
  - `type Mode = 'slider' | 'sides'`, `type Zoom = 'fit' | 1 | 2`, `type Size = [number, number]`.
  - `shotLabel(shot: Shot, before?: number): string`
  - `nowBox(original: Size, now: Size): Size`
  - `cssScale(zoom: Zoom, box: Size, stage: Size, dpr: number): number`
  - `clampPan(pan: Size, content: Size, stage: Size): Size`
  - `dividerAt(x: number, left: number, width: number): number`
  - `versioned(url: string, shot: Shot): string`

- [ ] **Step 1: Write the failing tests**

Create `src/compare/view.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { clampPan, cssScale, dividerAt, nowBox, shotLabel, versioned, type Shot } from './view';

const original: Shot = { path: '/b/shot.png', bytes: 2_400_000, dims: [2880, 1800], format: 'PNG', version: 0 };
const now: Shot = { path: '/w/shot.webp', bytes: 380_000, dims: [1440, 900], format: 'WebP', version: 1_700_000 };

describe('shotLabel', () => {
  it('reads size, dimensions and format, and the change for the result', () => {
    expect(shotLabel(original)).toBe('2.4 MB · 2880×1800 · PNG');
    expect(shotLabel(now, original.bytes)).toBe('380 KB · 1440×900 · WebP (−84%)');
  });

  it('leaves out what it does not know', () => {
    expect(shotLabel({ ...now, dims: null, format: '' })).toBe('380 KB');
  });
});

describe('nowBox', () => {
  it('stretches a downscaled result to the original size so pixels line up', () => {
    expect(nowBox([2880, 1800], [1440, 900])).toEqual([2880, 1800]);
    expect(nowBox([2880, 1800], [1441, 900])).toEqual([2880, 1800]);
  });

  it('keeps a cropped result at its own size', () => {
    expect(nowBox([2880, 1800], [1600, 900])).toEqual([1600, 900]);
    expect(nowBox([2880, 1800], [1800, 1800])).toEqual([1800, 1800]);
  });

  it('survives a zero side', () => {
    expect(nowBox([0, 0], [0, 0])).toEqual([0, 0]);
  });
});

describe('cssScale', () => {
  it('shows one image pixel per screen pixel at 100 % and two at 200 %', () => {
    expect(cssScale(1, [2880, 1800], [1000, 700], 2)).toBe(0.5);
    expect(cssScale(2, [2880, 1800], [1000, 700], 2)).toBe(1);
    expect(cssScale(1, [800, 600], [1000, 700], 1)).toBe(1);
  });

  it('fits a large image into the stage', () => {
    expect(cssScale('fit', [2000, 1000], [1000, 700], 1)).toBe(0.5);
    expect(cssScale('fit', [1000, 2000], [1000, 700], 1)).toBe(0.35);
  });

  it('never enlarges a small image past 100 % to fit', () => {
    expect(cssScale('fit', [100, 50], [1000, 700], 1)).toBe(1);
    expect(cssScale('fit', [100, 50], [1000, 700], 2)).toBe(0.5);
  });

  it('treats a missing pixel ratio as 1', () => {
    expect(cssScale(1, [100, 50], [1000, 700], 0)).toBe(1);
  });
});

describe('clampPan', () => {
  it('lets a zoomed image pan as far as it overhangs, no further', () => {
    expect(clampPan([500, -500], [1600, 1000], [1000, 700])).toEqual([300, -150]);
    expect(clampPan([-20, 10], [1600, 1000], [1000, 700])).toEqual([-20, 10]);
  });

  it('keeps an image that fits centred', () => {
    expect(clampPan([40, 40], [800, 600], [1000, 700])).toEqual([0, 0]);
  });
});

describe('dividerAt', () => {
  it('follows the pointer and stops at the edges', () => {
    expect(dividerAt(150, 100, 200)).toBe(0.25);
    expect(dividerAt(0, 100, 200)).toBe(0);
    expect(dividerAt(900, 100, 200)).toBe(1);
  });

  it('stays centred on a stage with no width', () => {
    expect(dividerAt(10, 0, 0)).toBe(0.5);
  });
});

describe('versioned', () => {
  it('adds the file version as a query the asset protocol ignores', () => {
    expect(versioned('http://asset.localhost/%2Fw%2Fshot.webp', now)).toBe(
      'http://asset.localhost/%2Fw%2Fshot.webp?v=1700000',
    );
  });
});
```

- [ ] **Step 2: Run the tests to see them fail**

Run: `pnpm test`
Expected: FAIL: `src/compare/view.test.ts` cannot resolve `./view` ("Failed to resolve import" / "Cannot find module"). Every other test file passes.

- [ ] **Step 3: Implement**

Create `src/compare/view.ts`:

```ts
import { formatBytes, savingPercent } from '../lib/format';

/** Mirrors compare::Shot (src-tauri/src/compare.rs). */
export interface Shot {
  path: string;
  bytes: number;
  /** As shown (JPEG orientation applied); null when unknown. */
  dims: [number, number] | null;
  format: string;
  /** Changes whenever the file is rewritten. */
  version: number;
}

/** Mirrors compare::CompareView. `original` is null in single-image mode (Windows quick preview). */
export interface CompareView {
  id: string;
  name: string;
  original: Shot | null;
  now: Shot;
}

export type Mode = 'slider' | 'sides';
export type Zoom = 'fit' | 1 | 2;
export type Size = [number, number];

/** "2.4 MB · 2880×1800 · PNG"; the result adds its change: "380 KB · 1440×900 · WebP (−84%)". */
export function shotLabel(shot: Shot, before?: number): string {
  const parts = [formatBytes(shot.bytes)];
  if (shot.dims) parts.push(`${shot.dims[0]}×${shot.dims[1]}`);
  if (shot.format) parts.push(shot.format);
  const text = parts.join(' · ');
  return before === undefined ? text : `${text} (${savingPercent(before, shot.bytes)})`;
}

/**
 * The size, in image pixels, the result is drawn at. A downscaled result has
 * the original's shape and is stretched to the original's size so pixels line
 * up; a cropped one has another shape and keeps its own size, centred.
 */
export function nowBox(original: Size, now: Size): Size {
  const shape = (size: Size) => size[0] / Math.max(1, size[1]);
  return Math.abs(shape(original) / shape(now) - 1) < 0.01 ? original : now;
}

/**
 * CSS pixels per image pixel. 100 % is one image pixel per screen pixel, so a
 * Retina screenshot shows at its real sharpness. Fit shrinks the image into the
 * stage but never enlarges it past 100 %.
 */
export function cssScale(zoom: Zoom, box: Size, stage: Size, dpr: number): number {
  const actual = 1 / Math.max(1, dpr);
  if (zoom !== 'fit') return actual * zoom;
  return Math.min(actual, stage[0] / Math.max(1, box[0]), stage[1] / Math.max(1, box[1]));
}

/** Keep a pan offset within what the image overhangs the stage; no overhang, no pan. */
export function clampPan(pan: Size, content: Size, stage: Size): Size {
  const limit = (i: 0 | 1) => Math.max(0, (content[i] - stage[i]) / 2);
  const clamp = (value: number, max: number) => Math.min(max, Math.max(-max, value)) || 0;
  return [clamp(pan[0], limit(0)), clamp(pan[1], limit(1))];
}

/** The divider's position (0 = left edge, 1 = right edge) for a pointer at `x` over a stage spanning `left`..`left + width`. */
export function dividerAt(x: number, left: number, width: number): number {
  if (width <= 0) return 0.5;
  return Math.min(1, Math.max(0, (x - left) / width));
}

/** An asset URL that changes when the file does, so the webview never shows a cached older result. */
export function versioned(url: string, shot: Shot): string {
  return `${url}?v=${shot.version}`;
}
```

- [ ] **Step 4: Run the tests to see them pass**

Run: `pnpm test && pnpm check`
Expected: 0 failed, and `src/compare/view.test.ts` is among the passed files (8 files on a `main` with M3a only; more if M3b's tests are merged); svelte-check ends with `0 ERRORS 0 WARNINGS`.

- [ ] **Step 5: Commit**

```bash
git add src/compare/view.ts src/compare/view.test.ts
git commit -m "feat(ui): compare page helpers (labels, zoom, pan, divider)

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 3: The compare page

**Files:**
- Create: `src/compare.html`, `src/compare/main.ts`, `src/compare/Compare.svelte`
- Modify: `vite.config.ts` (the `input` object), `src/lib/i18n/en.json` (after `"stack.window_title"`)

**Interfaces:**
- Consumes: everything `src/compare/view.ts` exports (Task 2); `t(key)` from `src/lib/i18n.ts`; `applyAccent`, `applyOsClass` from `src/lib/ipc.ts`; Tauri `convertFileSrc`, `invoke`, `listen`.
- Produces (used by Task 4's Rust):
  - Page `compare.html`, loaded as `WebviewUrl::App("compare.html")`.
  - On load it calls `invoke<CompareView | null>('compare_ready')`; it listens to `compare:show` (payload `CompareView`) and `compare:hidden` (no payload); Esc or Space calls `invoke('compare_close')`.
  - Its toolbar heights add up to 84 logical px (36 px header + 48 px footer). Task 4's `CHROME` constant must match.
  - i18n keys `compare.window_title` (`"Compare {name}"`), `compare.original_gone`, `compare.file_gone` used by Rust in Task 4.

- [ ] **Step 1: Add the strings**

In `src/lib/i18n/en.json`, right after the line `  "stack.window_title": "Ferry results",` insert:

```json
  "compare.window_title": "Compare {name}",
  "compare.original": "Original",
  "compare.now": "Now",
  "compare.view": "View",
  "compare.slider": "Slider",
  "compare.sides": "Side by side",
  "compare.zoom": "Zoom",
  "compare.fit": "Fit",
  "compare.zoom_100": "100%",
  "compare.zoom_200": "200%",
  "compare.divider": "Divider between the original and the result",
  "compare.original_gone": "Original no longer kept",
  "compare.file_gone": "File no longer there",
```

- [ ] **Step 2: Add the page to the build**

In `vite.config.ts`, replace

```ts
      input: { stack: page('stack.html'), settings: page('settings.html') },
```

with

```ts
      input: {
        stack: page('stack.html'),
        settings: page('settings.html'),
        compare: page('compare.html'),
      },
```

- [ ] **Step 3: Create the HTML entry and the mount script**

Create `src/compare.html`:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Ferry Compare</title>
  </head>
  <body class="window">
    <div id="app"></div>
    <script type="module" src="/compare/main.ts"></script>
  </body>
</html>
```

Create `src/compare/main.ts`:

```ts
import { mount } from 'svelte';
import '../lib/theme.css';
import { applyAccent, applyOsClass } from '../lib/ipc';
import Compare from './Compare.svelte';

applyOsClass();
void applyAccent();
mount(Compare, { target: document.getElementById('app')! });
```

- [ ] **Step 4: Create the component**

Create `src/compare/Compare.svelte`. Layout follows the approved mockup: a header with "Original …" left and "Now …" right, a checkerboard stage, a footer with the two segmented controls. In slider mode both images are stacked in full-stage layers, the result layer is clipped from the divider rightwards, and the divider is draggable (or moved with ←/→ when focused). Side by side puts two panes next to each other with one shared zoom and pan. Dragging anywhere else pans (only when zoomed past the stage). Single-image mode (no `original`) hides the divider, the tags and the Slider / Side by side control.

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { t } from '../lib/i18n';
  import {
    clampPan,
    cssScale,
    dividerAt,
    nowBox,
    shotLabel,
    versioned,
    type CompareView,
    type Mode,
    type Shot,
    type Size,
    type Zoom,
  } from './view';

  let view = $state<CompareView | null>(null);
  let mode = $state<Mode>('slider');
  let zoom = $state<Zoom>('fit');
  let divider = $state(0.5);
  let pan = $state<Size>([0, 0]);
  let stageWidth = $state(0);
  let stageHeight = $state(0);
  let dpr = $state(window.devicePixelRatio || 1);
  let stage = $state<HTMLElement>();
  /** What a pointer drag moves, and where it started. */
  let drag: { what: 'divider' | 'pan'; x: number; y: number; from: Size } | null = null;

  const single = $derived(view !== null && view.original === null);
  const sides = $derived(mode === 'sides' && !single);
  /** Image-pixel boxes: the original's own size, the result's per `nowBox`. */
  const originalBox = $derived<Size>(dimsOf(view?.original ?? view?.now));
  const resultBox = $derived<Size>(
    view?.original ? nowBox(originalBox, dimsOf(view.now)) : dimsOf(view?.now),
  );
  /** The room one image has: the whole stage, or half of it side by side. */
  const pane = $derived<Size>([sides ? (stageWidth - 1) / 2 : stageWidth, stageHeight]);
  const scale = $derived(cssScale(zoom, originalBox, pane, dpr));
  const offset = $derived(clampPan(pan, [originalBox[0] * scale, originalBox[1] * scale], pane));

  function dimsOf(shot: Shot | undefined): Size {
    return shot?.dims ?? [1, 1];
  }

  function src(shot: Shot): string {
    return versioned(convertFileSrc(shot.path), shot);
  }

  /** CSS for an image of `box` image pixels, centred in its pane and panned. */
  function placed(box: Size): string {
    const width = box[0] * scale;
    const height = box[1] * scale;
    return `width:${width}px;height:${height}px;transform:translate(${offset[0] - width / 2}px,${offset[1] - height / 2}px)`;
  }

  function show(next: CompareView | null) {
    view = next;
    zoom = 'fit';
    divider = 0.5;
    pan = [0, 0];
  }

  function close() {
    show(null);
    void invoke('compare_close');
  }

  function onkeydown(event: KeyboardEvent) {
    if (event.key === 'Escape' || event.key === ' ') {
      event.preventDefault();
      close();
    }
  }

  function onpointerdown(event: PointerEvent) {
    if (!stage || event.button !== 0) return;
    const onDivider = (event.target as Element).closest('[data-divider]') !== null;
    drag = { what: onDivider ? 'divider' : 'pan', x: event.clientX, y: event.clientY, from: offset };
    stage.setPointerCapture(event.pointerId);
    if (onDivider) event.preventDefault();
  }

  function onpointermove(event: PointerEvent) {
    if (!drag || !stage) return;
    if (drag.what === 'divider') {
      const rect = stage.getBoundingClientRect();
      divider = dividerAt(event.clientX, rect.left, rect.width);
    } else {
      pan = [drag.from[0] + event.clientX - drag.x, drag.from[1] + event.clientY - drag.y];
    }
  }

  function onpointerup() {
    drag = null;
  }

  function setZoom(next: Zoom) {
    zoom = next;
    pan = [0, 0];
  }

  onMount(() => {
    // Listen first, then ask what to show, so nothing falls in between.
    const unlisten = Promise.all([
      listen<CompareView>('compare:show', (event) => show(event.payload)),
      listen('compare:hidden', () => show(null)),
    ]).then(async (stops) => {
      show(await invoke<CompareView | null>('compare_ready'));
      return stops;
    });
    // The window can move to a screen with another pixel ratio.
    const ratio = () => {
      dpr = window.devicePixelRatio || 1;
    };
    window.addEventListener('resize', ratio);
    return () => {
      window.removeEventListener('resize', ratio);
      unlisten.then((stops) => stops.forEach((stop) => stop()));
    };
  });
</script>

<svelte:window {onkeydown} />

{#if view}
  <div class="compare">
    <header class:single>
      {#if view.original}
        <span><b>{t('compare.original')}</b> {shotLabel(view.original)}</span>
        <span><b>{t('compare.now')}</b> {shotLabel(view.now, view.original.bytes)}</span>
      {:else}
        <span>{shotLabel(view.now)}</span>
      {/if}
    </header>

    <div
      class="stage"
      class:sides
      bind:this={stage}
      bind:clientWidth={stageWidth}
      bind:clientHeight={stageHeight}
      role="presentation"
      {onpointerdown}
      {onpointermove}
      {onpointerup}
      onpointercancel={onpointerup}
    >
      {#if view.original && sides}
        <div class="pane"><img src={src(view.original)} alt={t('compare.original')} style={placed(originalBox)} draggable="false" /></div>
        <div class="pane"><img src={src(view.now)} alt={t('compare.now')} style={placed(resultBox)} draggable="false" /></div>
      {:else if view.original}
        <div class="pane"><img src={src(view.original)} alt={t('compare.original')} style={placed(originalBox)} draggable="false" /></div>
        <div class="pane now" style="clip-path: inset(0 0 0 {divider * 100}%)">
          <img src={src(view.now)} alt={t('compare.now')} style={placed(resultBox)} draggable="false" />
        </div>
        <div
          class="divider"
          data-divider
          style="left: {divider * 100}%"
          role="slider"
          tabindex="0"
          aria-label={t('compare.divider')}
          aria-valuemin={0}
          aria-valuemax={100}
          aria-valuenow={Math.round(divider * 100)}
          onkeydown={(event) => {
            if (event.key === 'ArrowLeft') divider = Math.max(0, divider - 0.05);
            if (event.key === 'ArrowRight') divider = Math.min(1, divider + 0.05);
          }}
        ><b>⇔</b></div>
        <span class="tag left">{t('compare.original')}</span>
        <span class="tag right">{t('compare.now')}</span>
      {:else}
        <div class="pane"><img src={src(view.now)} alt={view.name} style={placed(resultBox)} draggable="false" /></div>
      {/if}
    </div>

    <footer>
      {#if !single}
        <div class="seg" role="radiogroup" aria-label={t('compare.view')}>
          <button type="button" role="radio" aria-checked={mode === 'slider'} onclick={() => (mode = 'slider')}>{t('compare.slider')}</button>
          <button type="button" role="radio" aria-checked={mode === 'sides'} onclick={() => (mode = 'sides')}>{t('compare.sides')}</button>
        </div>
      {/if}
      <div class="seg" role="radiogroup" aria-label={t('compare.zoom')}>
        <button type="button" role="radio" aria-checked={zoom === 'fit'} onclick={() => setZoom('fit')}>{t('compare.fit')}</button>
        <button type="button" role="radio" aria-checked={zoom === 1} onclick={() => setZoom(1)}>{t('compare.zoom_100')}</button>
        <button type="button" role="radio" aria-checked={zoom === 2} onclick={() => setZoom(2)}>{t('compare.zoom_200')}</button>
      </div>
    </footer>
  </div>
{/if}

<style>
  .compare {
    display: flex;
    flex-direction: column;
    height: 100vh;
    user-select: none;
  }
  header,
  footer {
    display: flex;
    flex: 0 0 auto;
    align-items: center;
    gap: 10px;
    padding: 0 12px;
    background: var(--bg-group);
  }
  header {
    height: 36px;
    justify-content: space-between;
    border-bottom: 1px solid var(--rail-line);
  }
  header.single {
    justify-content: center;
  }
  footer {
    height: 48px;
    justify-content: center;
    border-top: 1px solid var(--rail-line);
  }
  .stage {
    position: relative;
    display: flex;
    flex: 1 1 auto;
    gap: 1px;
    overflow: hidden;
    cursor: grab;
    background: repeating-conic-gradient(#2a2a2a 0 25%, #222 0 50%) 0 0 / 16px 16px;
  }
  .pane {
    position: absolute;
    inset: 0;
    overflow: hidden;
  }
  .sides .pane {
    position: relative;
    flex: 1 1 0;
  }
  .pane img {
    position: absolute;
    top: 50%;
    left: 50%;
    max-width: none;
    image-rendering: auto;
  }
  .divider {
    position: absolute;
    top: 0;
    bottom: 0;
    width: 2px;
    margin-left: -1px;
    background: #fff;
    cursor: ew-resize;
  }
  /* A wider grab area than the 2 px line. */
  .divider::before {
    position: absolute;
    inset: 0 -8px;
    content: '';
  }
  .divider b {
    position: absolute;
    top: 50%;
    left: -11px;
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    margin-top: -12px;
    font-size: 11px;
    color: #111;
    background: #fff;
    border-radius: 50%;
  }
  .divider:focus-visible {
    outline: 2px solid var(--accent-text);
  }
  .tag {
    position: absolute;
    top: 10px;
    padding: 2px 8px;
    color: #fff;
    background: rgba(0, 0, 0, 0.6);
    border-radius: 7px;
    pointer-events: none;
  }
  .tag.left {
    left: 10px;
  }
  .tag.right {
    right: 10px;
  }
  .seg {
    display: flex;
    padding: 2px;
    background: var(--bg-content);
    border-radius: 7px;
  }
  .seg button {
    padding: 2px 10px;
    font: inherit;
    color: var(--fg);
    background: none;
    border: 0;
    border-radius: 5px;
  }
  .seg button[aria-checked='true'] {
    background: var(--bg-group);
    box-shadow: 0 0 0 1px var(--rail-line);
  }
  .seg button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
```

- [ ] **Step 5: Check, test and build**

Run: `pnpm check && pnpm test && pnpm build && cargo test -p ferry-core --lib i18n`
Expected: svelte-check `0 ERRORS 0 WARNINGS`; vitest 0 failed with `src/compare/view.test.ts` passing; the build lists `dist/assets/compare-*.js` and `dist/assets/compare-*.css` and ends with `✓ built`; the i18n test passes (en.json is still a valid flat map).

- [ ] **Step 6: Commit**

```bash
git add src/compare.html src/compare/main.ts src/compare/Compare.svelte vite.config.ts src/lib/i18n/en.json
git commit -m "feat(ui): compare page with slider, side by side and zoom

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 4: The compare window

**Files:**
- Create: `src-tauri/src/compare.rs`
- Create: `src-tauri/src/platform/macos_preview.rs`, `src-tauri/src/platform/windows_preview.rs`
- Modify: `src-tauri/src/platform/mod.rs`, `src-tauri/src/windows.rs:48-53` and end of file, `src-tauri/src/lib.rs` (module list, `generate_handler!`), `src-tauri/capabilities/default.json`

**Interfaces:**
- Consumes: `ferry_core::geometry::{Rect, fit_image, center}` (Task 1); the page contract from Task 3 (`compare.html`, `compare_ready`, `compare:show`, `compare:hidden`, `compare_close`, 84 px of bars); M3a's `JobUpdate` fields and `Format::label` (see "M3a interfaces" above); `crate::state::AppState` (`jobs`, `settings`); `crate::windows::STACK`.
- Produces:
  - `pub const compare::COMPARE: &str = "compare"`.
  - `compare::Shot { path: PathBuf, bytes: u64, dims: Option<(u32, u32)>, format: &'static str, version: u64 }` and `compare::CompareView { id: JobId, name: String, original: Option<Shot>, now: Shot }`, both `Serialize` (JSON: `dims` is `[w, h]` or `null`).
  - `pub fn compare::view(card: &JobUpdate, single: bool) -> Result<CompareView, String>`
  - `pub fn compare::card(app: &AppHandle, id: &JobId) -> Result<JobUpdate, String>`
  - `pub fn compare::is_active(app: &AppHandle) -> bool`
  - `pub fn compare::show(app: &AppHandle, id: &JobId, single: bool) -> Result<(), String>`
  - `pub fn compare::close(app: &AppHandle)`
  - Commands `compare::open_compare(app, id: JobId) -> Result<(), String>` (async), `compare::compare_ready() -> Option<CompareView>`, `compare::compare_close(app)`.
  - `platform::preview::remember_front()` and `platform::preview::give_back_focus()` on both OSes.
  - `windows::follows_screenshot_setting(label: &str) -> bool`.

- [ ] **Step 1: Write the failing tests**

Create `src-tauri/src/compare.rs` containing only the doc comment and the tests for now:

```rust
//! The compare window (spec §4.5): a card's original beside its current
//! result, in one window that is reused and sized to the image. On Windows it
//! is also the quick preview, showing the result alone.

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n rest of a png";
    const WEBP: &[u8] = b"RIFF\0\0\0\0WEBPVP8 rest";

    /// A finished card whose backup is a PNG and whose result is a WebP.
    fn converted(dir: &Path) -> JobUpdate {
        let backup = dir.join("backup.png");
        let output = dir.join("shot.webp");
        fs::write(&backup, PNG).unwrap();
        fs::write(&output, WEBP).unwrap();
        JobUpdate {
            state: JobState::Done,
            name: "shot".into(),
            ext: "webp".into(),
            before: PNG.len() as u64,
            after: Some(WEBP.len() as u64),
            thumb_path: Some(backup),
            output: Some(output),
            dims_before: Some((2880, 1800)),
            dims_after: Some((1440, 900)),
            format: Some(Format::WebP),
            converted: true,
            ..ferry_core::pipeline::notice(String::new())
        }
    }

    #[test]
    fn a_finished_card_shows_its_original_beside_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        let view = view(&card, false).unwrap();
        assert_eq!(view.id, card.id);
        assert_eq!(view.name, "shot.webp");
        let original = view.original.unwrap();
        assert_eq!(original.path, dir.path().join("backup.png"));
        assert_eq!(original.format, "PNG");
        assert_eq!(original.dims, Some((2880, 1800)));
        assert_eq!(original.bytes, PNG.len() as u64);
        assert_eq!(view.now.path, dir.path().join("shot.webp"));
        assert_eq!(view.now.format, "WebP");
        assert_eq!(view.now.dims, Some((1440, 900)));
        assert_eq!(view.now.bytes, WEBP.len() as u64);
        assert!(view.now.version > 0);
    }

    #[test]
    fn a_result_that_kept_its_size_shows_the_original_size() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            dims_after: None,
            ..converted(dir.path())
        };
        assert_eq!(view(&card, false).unwrap().now.dims, Some((2880, 1800)));
    }

    #[test]
    fn an_expired_backup_means_the_original_is_no_longer_kept() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        fs::remove_file(card.thumb_path.as_ref().unwrap()).unwrap();
        assert_eq!(view(&card, false), Err("Original no longer kept".into()));
        let card = JobUpdate {
            thumb_path: None,
            ..card
        };
        assert_eq!(view(&card, false), Err("Original no longer kept".into()));
    }

    #[test]
    fn single_image_mode_needs_no_backup() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            thumb_path: None,
            ..converted(dir.path())
        };
        let view = view(&card, true).unwrap();
        assert_eq!(view.original, None);
        assert_eq!(view.now.format, "WebP");
    }

    #[test]
    fn a_result_that_is_gone_cannot_be_shown() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        fs::remove_file(card.output.as_ref().unwrap()).unwrap();
        assert_eq!(view(&card, true), Err("File no longer there".into()));
        let notice = ferry_core::pipeline::notice("Nothing to optimise".into());
        assert_eq!(view(&notice, true), Err("File no longer there".into()));
    }

    #[test]
    fn a_busy_card_waits_for_its_result() {
        let dir = tempfile::tempdir().unwrap();
        let card = JobUpdate {
            state: JobState::Queued,
            ..converted(dir.path())
        };
        assert!(view(&card, false).is_err());
    }

    #[test]
    fn asking_again_for_what_is_on_show_toggles_it() {
        let dir = tempfile::tempdir().unwrap();
        let card = converted(dir.path());
        let both = view(&card, false).unwrap();
        let alone = view(&card, true).unwrap();
        assert!(same_view(Some(&both), &card.id, false));
        assert!(same_view(Some(&alone), &card.id, true));
        assert!(!same_view(Some(&both), &card.id, true));
        assert!(!same_view(Some(&both), &JobId::generate(), false));
        assert!(!same_view(None, &card.id, false));
    }

    #[test]
    fn the_page_gets_plain_json() {
        let dir = tempfile::tempdir().unwrap();
        let json = serde_json::to_value(view(&converted(dir.path()), false).unwrap()).unwrap();
        assert_eq!(json["now"]["dims"], serde_json::json!([1440, 900]));
        assert_eq!(json["original"]["format"], "PNG");
        assert!(json["id"].is_string());
    }
}
```

In `src-tauri/src/lib.rs`, add `mod compare;` on its own line right after `mod commands;`.

- [ ] **Step 2: Run the tests to see them fail**

Run: `cargo test -p ferry compare::`
Expected: FAIL to compile with errors such as "cannot find type `Path`", "cannot find function `view`" and "cannot find function `same_view`".

- [ ] **Step 3: Implement the view model**

In `src-tauri/src/compare.rs`, between the `//!` doc comment and `#[cfg(test)]`, insert:

```rust
use std::io::Read;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::UNIX_EPOCH;

use ferry_core::geometry::{Rect, center, fit_image};
use ferry_core::i18n::{t, tf};
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::sniff::{self, Format};
use serde::Serialize;
use tauri::{
    AppHandle, Emitter, Manager, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
use tracing::warn;

use crate::platform;
use crate::state::AppState;
use crate::windows::STACK;

pub const COMPARE: &str = "compare";

/// The bars above and below the image in compare.html, in logical pixels.
const CHROME: (f64, f64) = (0.0, 84.0);
/// Room for the toolbar however small the image.
const MIN_SIZE: (f64, f64) = (520.0, 360.0);
/// The window takes at most this share of its screen each way.
const SCREEN_SHARE: f64 = 0.8;

/// What the window shows; the page collects it when it loads (`compare_ready`).
static SHOWN: Mutex<Option<CompareView>> = Mutex::new(None);

/// One image in the window and what its bar says about it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Shot {
    pub path: PathBuf,
    pub bytes: u64,
    /// As shown (JPEG orientation applied); `None` when unknown.
    pub dims: Option<(u32, u32)>,
    /// "PNG", "JPEG", "GIF" or "WebP"; empty for a file Ferry can't read.
    pub format: &'static str,
    /// Changes when the file is rewritten, so the page never shows a cached older result.
    pub version: u64,
}

/// Everything the page needs. `original` is `None` in single-image mode.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CompareView {
    pub id: JobId,
    /// "shot.png".
    pub name: String,
    pub original: Option<Shot>,
    pub now: Shot,
}

/// What the window shows for `card`, or why it can't. `single` leaves the
/// original out, so an expired backup does not matter.
pub fn view(card: &JobUpdate, single: bool) -> Result<CompareView, String> {
    if matches!(card.state, JobState::Queued | JobState::Optimising { .. }) {
        return Err("the job is still running".into());
    }
    let output = existing(card.output.as_deref()).ok_or_else(|| t("compare.file_gone"))?;
    let original = if single {
        None
    } else {
        let backup =
            existing(card.thumb_path.as_deref()).ok_or_else(|| t("compare.original_gone"))?;
        Some(shot(backup, card.dims_before))
    };
    Ok(CompareView {
        id: card.id.clone(),
        name: if card.ext.is_empty() {
            card.name.clone()
        } else {
            format!("{}.{}", card.name, card.ext)
        },
        original,
        now: shot(output, card.dims_after.or(card.dims_before)),
    })
}

/// Whether `shown` is card `id` in the same mode, so a second ⌘D or Space
/// closes the window rather than reloading it.
fn same_view(shown: Option<&CompareView>, id: &JobId, single: bool) -> bool {
    shown.is_some_and(|v| v.id == *id && v.original.is_none() == single)
}

fn existing(path: Option<&Path>) -> Option<&Path> {
    path.filter(|p| p.is_file())
}

fn shot(path: &Path, dims: Option<(u32, u32)>) -> Shot {
    let meta = std::fs::metadata(path).ok();
    Shot {
        path: path.to_owned(),
        bytes: meta.as_ref().map_or(0, |m| m.len()),
        dims,
        format: head_format(path).map_or("", Format::label),
        version: meta
            .and_then(|m| m.modified().ok())
            .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64),
    }
}

/// A file's format from its first bytes: the card only knows the result's
/// format, and the backup may differ after a conversion.
fn head_format(path: &Path) -> Option<Format> {
    let mut head = Vec::with_capacity(12);
    std::fs::File::open(path)
        .ok()?
        .take(12)
        .read_to_end(&mut head)
        .ok()?;
    sniff::sniff(&head)
}
```

This leaves the `Rect`, `center`, `fit_image`, `tf`, `Emitter`, `Manager`, window, `warn`, `platform`, `AppState`, `STACK` and `SHOWN` items unused until Step 5; the test run below only warns about them.

- [ ] **Step 4: Run the view tests**

Run: `cargo test -p ferry compare::`
Expected: `test result: ok. 8 passed` (with unused-import warnings). If a compile error names a `JobUpdate` field or `Format::label`, M3a's interface differs from this plan: stop and report.

- [ ] **Step 5: Implement the window, the commands and the focus hand-back**

5a. In `src-tauri/src/compare.rs`, insert right before `#[cfg(test)]`:

```rust
/// The card's latest state. `Entry::last` follows a conversion or rename;
/// `Entry::output` does not.
pub fn card(app: &AppHandle, id: &JobId) -> Result<JobUpdate, String> {
    let state = app.state::<AppState>();
    let entry = state
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .cloned()
        .ok_or("this card's job is gone")?;
    let card = entry.last.lock().unwrap().clone();
    Ok(card)
}

/// Whether the window is on screen and has focus, so Space and ⌘D close it.
pub fn is_active(app: &AppHandle) -> bool {
    app.get_webview_window(COMPARE)
        .is_some_and(|w| w.is_visible().unwrap_or(false) && w.is_focused().unwrap_or(false))
}

/// Show card `id` (`single`: its result alone) and give the window focus.
/// Asking again for what is already on show closes the window instead.
pub fn show(app: &AppHandle, id: &JobId, single: bool) -> Result<(), String> {
    let view = view(&card(app, id)?, single)?;
    if same_view(SHOWN.lock().unwrap().as_ref(), id, single) && is_active(app) {
        close(app);
        return Ok(());
    }
    // The configured scope only covers backups; results can be anywhere.
    let scope = app.asset_protocol_scope();
    for shot in view.original.iter().chain([&view.now]) {
        scope.allow_file(&shot.path).map_err(|e| e.to_string())?;
    }
    let image = view.original.as_ref().unwrap_or(&view.now).dims;
    let title = if single {
        view.name.clone()
    } else {
        tf("compare.window_title", &[("name", &view.name)])
    };
    if !is_active(app) {
        platform::preview::remember_front();
    }
    *SHOWN.lock().unwrap() = Some(view.clone());
    let window = match app.get_webview_window(COMPARE) {
        Some(window) => {
            if let Err(e) = app.emit_to(COMPARE, "compare:show", &view) {
                warn!("could not update the compare window: {e}");
            }
            window
        }
        None => build(app).map_err(|e| e.to_string())?,
    };
    let _ = window.set_title(&title);
    place(app, &window, image.unwrap_or((0, 0)));
    window.show().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())
}

/// Hide the window (it is reused), let the page drop its images, and hand
/// focus back to the app that had it.
pub fn close(app: &AppHandle) {
    *SHOWN.lock().unwrap() = None;
    let Some(window) = app.get_webview_window(COMPARE) else {
        return;
    };
    let focused = window.is_focused().unwrap_or(false);
    let _ = window.hide();
    let _ = app.emit_to(COMPARE, "compare:hidden", ());
    if focused {
        platform::preview::give_back_focus();
    }
}

fn build(app: &AppHandle) -> tauri::Result<WebviewWindow> {
    let show_in_screenshots = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .general
        .show_in_screenshots;
    let window = WebviewWindowBuilder::new(app, COMPARE, WebviewUrl::App("compare.html".into()))
        .title(t("app.name"))
        .content_protected(!show_in_screenshots)
        .inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .min_inner_size(MIN_SIZE.0, MIN_SIZE.1)
        .visible(false)
        .build()?;
    // The close button hides the window like Esc, so the page stays loaded.
    let handle = app.clone();
    window.on_window_event(move |event| {
        if let WindowEvent::CloseRequested { api, .. } = event {
            api.prevent_close();
            close(&handle);
        }
    });
    Ok(window)
}

/// Size the window to `image` (pixels) and centre it on the cards' screen.
fn place(app: &AppHandle, window: &WebviewWindow, image: (u32, u32)) {
    let monitor = app
        .get_webview_window(STACK)
        .and_then(|stack| stack.current_monitor().ok().flatten())
        .or_else(|| app.primary_monitor().ok().flatten());
    let Some(monitor) = monitor else {
        return;
    };
    let scale = monitor.scale_factor();
    let work = monitor.work_area();
    let area = Rect {
        x: work.position.x,
        y: work.position.y,
        width: work.size.width,
        height: work.size.height,
    };
    let px = |logical: f64| (logical * scale).round() as u32;
    let size = fit_image(
        area,
        image,
        (px(CHROME.0), px(CHROME.1)),
        SCREEN_SHARE,
        (px(MIN_SIZE.0), px(MIN_SIZE.1)),
    );
    let (x, y) = center(area, size);
    // As in `windows::stack_resize`: macOS positions windows in points.
    #[cfg(target_os = "macos")]
    {
        use tauri::{LogicalPosition, LogicalSize};
        let _ = window.set_size(LogicalSize::new(
            f64::from(size.0) / scale,
            f64::from(size.1) / scale,
        ));
        let _ = window.set_position(LogicalPosition::new(
            f64::from(x) / scale,
            f64::from(y) / scale,
        ));
    }
    #[cfg(not(target_os = "macos"))]
    {
        use tauri::{PhysicalPosition, PhysicalSize};
        let _ = window.set_position(PhysicalPosition::new(x, y));
        let _ = window.set_size(PhysicalSize::new(size.0, size.1));
        let _ = window.set_position(PhysicalPosition::new(x, y));
    }
}

/// ⌘D, the rail's Compare button and "Compare with original" (M3b).
#[tauri::command]
pub async fn open_compare(app: AppHandle, id: JobId) -> Result<(), String> {
    show(&app, &id, false)
}

/// The page has loaded: what to show, if anything yet.
#[tauri::command]
pub fn compare_ready() -> Option<CompareView> {
    SHOWN.lock().unwrap().clone()
}

/// Esc or Space in the page.
#[tauri::command]
pub fn compare_close(app: AppHandle) {
    close(&app);
}
```

`open_compare` must stay `async`: a synchronous command runs on the main thread, and building a window there deadlocks on Windows.

5b. Create `src-tauri/src/platform/macos_preview.rs` (Task 5 replaces it with the full Quick Look version):

```rust
//! macOS focus hand-back: Ferry is an accessory app that is never frontmost,
//! so when the compare window closes it hands focus back to the app the user
//! was in.

use std::sync::Mutex;

use objc2::rc::Retained;
use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};

/// The app that was frontmost before Ferry took focus.
static FRONT: Mutex<Option<Retained<NSRunningApplication>>> = Mutex::new(None);

/// Note the frontmost app before Ferry takes focus; nothing while Ferry
/// itself is active (Settings in front).
pub fn remember_front() {
    let front = if NSRunningApplication::currentApplication().isActive() {
        None
    } else {
        NSWorkspace::sharedWorkspace().frontmostApplication()
    };
    *FRONT.lock().unwrap() = front;
}

/// Activate the app `remember_front` noted, once.
pub fn give_back_focus() {
    if let Some(app) = FRONT.lock().unwrap().take() {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}
```

5c. Create `src-tauri/src/platform/windows_preview.rs`:

```rust
//! Windows: the quick preview is the compare window (spec §4.5). This hands
//! focus back to the window the user was in when the compare window closes.

use std::sync::Mutex;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::System::Threading::GetCurrentProcessId;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
};

/// The foreground window before Ferry took focus, as an address (`HWND` is not
/// `Send`); 0 for none.
static FRONT: Mutex<usize> = Mutex::new(0);

/// Note the foreground window before Ferry takes focus; nothing when it is
/// one of Ferry's own (Settings).
pub fn remember_front() {
    // SAFETY: plain queries; no pointer is kept beyond its address.
    let front = unsafe {
        let hwnd = GetForegroundWindow();
        let mut pid: u32 = 0;
        GetWindowThreadProcessId(hwnd, &mut pid);
        if hwnd.is_null() || pid == GetCurrentProcessId() {
            0
        } else {
            hwnd as usize
        }
    };
    *FRONT.lock().unwrap() = front;
}

/// Bring back the window `remember_front` noted, once, if it still exists.
pub fn give_back_focus() {
    let front = std::mem::take(&mut *FRONT.lock().unwrap());
    if front == 0 {
        return;
    }
    let hwnd = front as HWND;
    // SAFETY: IsWindow rejects a window that closed in the meantime.
    unsafe {
        if IsWindow(hwnd) != 0 {
            SetForegroundWindow(hwnd);
        }
    }
}
```

The Windows code cannot be compiled here. It uses only these `windows-sys` 0.61 items, all behind features already enabled in `src-tauri/Cargo.toml` (`Win32_Foundation`, `Win32_System_Threading`, `Win32_UI_WindowsAndMessaging`): `HWND = *mut c_void`; `GetForegroundWindow() -> HWND`; `GetWindowThreadProcessId(hwnd: HWND, lpdwprocessid: *mut u32) -> u32`; `GetCurrentProcessId() -> u32`; `IsWindow(hwnd: HWND) -> BOOL`; `SetForegroundWindow(hwnd: HWND) -> BOOL` (`BOOL` is `i32`). Read the file once against that list.

5d. In `src-tauri/src/platform/mod.rs`, make the two OS blocks read exactly (rustfmt keeps `mod` lines sorted, so `preview` goes after `macos` and before `windows`):

```rust
#[cfg(target_os = "macos")]
#[path = "macos_clipboard.rs"]
pub mod clipboard;
#[cfg(target_os = "macos")]
mod macos;
#[cfg(target_os = "macos")]
#[path = "macos_preview.rs"]
pub mod preview;
#[cfg(target_os = "macos")]
pub use macos::{accent_rgb, app_identifier, hide, make_non_activating, show_without_focus};

#[cfg(windows)]
#[path = "windows_clipboard.rs"]
pub mod clipboard;
#[cfg(windows)]
#[path = "windows_preview.rs"]
pub mod preview;
#[cfg(windows)]
mod windows;
#[cfg(windows)]
pub use self::windows::{
    accent_rgb, app_identifier, hide, make_non_activating, show_without_focus,
};
```

If M3b has already added names to either `pub use` line in your branch, keep its names; only add the `preview` lines.

5e. In `src-tauri/src/windows.rs`, first append this test module at the very end of the file:

```rust

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_stack_and_the_compare_window_follow_the_screenshot_setting() {
        assert!(follows_screenshot_setting(STACK));
        assert!(follows_screenshot_setting(crate::compare::COMPARE));
        assert!(!follows_screenshot_setting(SETTINGS));
    }
}
```

Run `cargo test -p ferry windows::` and expect FAIL to compile with "cannot find function `follows_screenshot_setting`". Then replace

```rust
/// Hide the stack from screenshots and screen sharing, or show it (R§3.1).
pub fn set_content_protected(app: &AppHandle, protected: bool) {
    if let Some(stack) = app.get_webview_window(STACK) {
        let _ = stack.set_content_protected(protected);
    }
}
```

with

```rust
/// Whether the window labelled `label` follows "Show app UI in screenshots":
/// the stack and the compare window do; Settings always shows.
pub fn follows_screenshot_setting(label: &str) -> bool {
    label == STACK || label == crate::compare::COMPARE
}

/// Hide Ferry's floating UI from screenshots and screen sharing, or show it
/// (R§3.1): every open window that follows the setting.
pub fn set_content_protected(app: &AppHandle, protected: bool) {
    for (label, window) in app.webview_windows() {
        if follows_screenshot_setting(&label) {
            let _ = window.set_content_protected(protected);
        }
    }
}
```

`prefs.rs` already calls `windows::set_content_protected(app, !new.general.show_in_screenshots)` when the setting changes (Settings switch and tray toggle); do not change it.

5f. In `src-tauri/src/lib.rs`, in `tauri::generate_handler![`, insert right after `windows::stack_resize,`:

```rust
            compare::open_compare,
            compare::compare_ready,
            compare::compare_close,
```

5g. In `src-tauri/capabilities/default.json`, change `"windows": ["stack", "settings"]` to `"windows": ["stack", "settings", "compare"]`. The page needs `core:default` for `listen`; app commands need no permission entry.

- [ ] **Step 6: Run the tests, fmt and clippy**

Run: `cargo fmt --all && cargo test -p ferry && cargo clippy --workspace --all-targets -- -D warnings`
Expected: the `ferry` lib reports 0 failed and at least 31 passed (22 on `main` before M3a, + 8 compare, + 1 windows; more if M3a added app tests); clippy prints no warning or error. If `cargo fmt` moved lines, keep its formatting.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/src/compare.rs src-tauri/src/platform/macos_preview.rs src-tauri/src/platform/windows_preview.rs src-tauri/src/platform/mod.rs src-tauri/src/windows.rs src-tauri/src/lib.rs src-tauri/capabilities/default.json
git commit -m "feat(app): compare window with focus hand-back and screenshot protection

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 5: Quick preview

**Files:**
- Create: `src-tauri/src/preview.rs`
- Modify (replace whole file): `src-tauri/src/platform/macos_preview.rs`
- Modify: `src-tauri/src/platform/windows_preview.rs` (append `set_protected`, add an import), `src-tauri/src/windows.rs` (`set_content_protected`), `src-tauri/src/lib.rs` (module, command), `src-tauri/Cargo.toml` (macOS dependency)

**Interfaces:**
- Consumes: `compare::{view, card, show, close, is_active}` (Task 4); `platform::preview::{remember_front, give_back_focus}` (Task 4); `AppState::settings` (`general.show_in_screenshots`).
- Produces:
  - Command `preview::quick_preview(app, id: JobId) -> Result<(), String>` (async).
  - macOS: `platform::preview::quick_look(app: &AppHandle, path: PathBuf, protected: bool) -> Result<(), String>`.
  - Both OSes: `platform::preview::set_protected(app: &AppHandle, protected: bool)`.

Background for the implementer: Quick Look's shared panel normally asks the key window's responder chain for a controller. Ferry has no key window (the stack is a non-activating panel), so this code sets the panel's data source and delegate directly. A spike on this Mac (macOS 14.6) confirmed that this shows the file in the real Quick Look panel from an accessory process without an exception, and that the delegate's `windowWillClose:` fires when the panel closes. Ferry activates itself first (as tao's `set_focus` does for Settings) so the panel becomes key and Esc reaches it. `USE_PANEL` is the switch for the spec's `qlmanage -p` fallback (Task 6 decides it).

- [ ] **Step 1: Add the dependency**

In `src-tauri/Cargo.toml`, in `[target.'cfg(target_os = "macos")'.dependencies]`, add right after the `objc2-foundation = …` line:

```toml
objc2-quick-look-ui = { version = "0.3.2", default-features = false, features = ["std", "QLPreviewPanel", "QLPreviewItem", "objc2-app-kit"] }
```

Its `objc2-app-kit` feature turns on the AppKit features it needs (`NSPanel`, `NSWindow`, `NSResponder` among them), so the existing `objc2-app-kit` line stays as it is.

- [ ] **Step 2: Replace the macOS platform file**

Replace the whole of `src-tauri/src/platform/macos_preview.rs` with:

```rust
//! macOS quick preview and focus hand-back. Quick Look's shared panel shows a
//! card's result. Ferry is an accessory app that is never frontmost, so it
//! activates to make the panel key (Esc and Space then reach it), and hands
//! focus back to the app the user was in when the panel or the compare window
//! closes.

use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSWindowDelegate,
    NSWindowSharingType, NSWorkspace,
};
use objc2_foundation::{NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_quick_look_ui::{
    QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource, QLPreviewPanelDelegate,
};
use tauri::AppHandle;
use tracing::warn;

/// `false` sends every preview to `qlmanage -p`, the plainer fallback for when
/// the panel cannot take keys from an accessory app (spec §9).
const USE_PANEL: bool = true;

/// The app that was frontmost before Ferry took focus.
static FRONT: Mutex<Option<Retained<NSRunningApplication>>> = Mutex::new(None);
/// The `qlmanage -p` fallback while it runs.
static QLMANAGE: Mutex<Option<Child>> = Mutex::new(None);

thread_local! {
    /// The panel's data source and delegate; the panel does not retain them.
    static PREVIEW: OnceCell<Retained<Preview>> = const { OnceCell::new() };
}

#[derive(Default)]
struct Ivars {
    url: RefCell<Option<Retained<NSURL>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Preview;

    unsafe impl NSObjectProtocol for Preview {}

    unsafe impl QLPreviewPanelDataSource for Preview {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn count(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            NSInteger::from(self.ivars().url.borrow().is_some())
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(
            &self,
            _panel: Option<&QLPreviewPanel>,
            _index: NSInteger,
        ) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            self.ivars()
                .url
                .borrow()
                .clone()
                .map(ProtocolObject::from_retained)
        }
    }

    unsafe impl NSWindowDelegate for Preview {
        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _notification: &NSNotification) {
            give_back_focus();
        }
    }

    unsafe impl QLPreviewPanelDelegate for Preview {}
);

impl Preview {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// Note the frontmost app before Ferry takes focus; nothing while Ferry
/// itself is active (Settings in front).
pub fn remember_front() {
    let front = if NSRunningApplication::currentApplication().isActive() {
        None
    } else {
        NSWorkspace::sharedWorkspace().frontmostApplication()
    };
    *FRONT.lock().unwrap() = front;
}

/// Activate the app `remember_front` noted, once.
pub fn give_back_focus() {
    if let Some(app) = FRONT.lock().unwrap().take() {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}

/// Show `path` in Quick Look, or close Quick Look if it is open. `protected`
/// hides the panel from screenshots and screen sharing, like the stack.
pub fn quick_look(app: &AppHandle, path: PathBuf, protected: bool) -> Result<(), String> {
    app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if !(USE_PANEL && toggle_panel(mtm, &path, protected)) {
            toggle_qlmanage(&path);
        }
    })
    .map_err(|e| e.to_string())
}

/// False when the shared panel is unavailable.
fn toggle_panel(mtm: MainThreadMarker, path: &Path, protected: bool) -> bool {
    // SAFETY: called on the main thread, as the marker proves.
    let Some(panel) = (unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }) else {
        return false;
    };
    if panel.isVisible() {
        panel.close(); // windowWillClose: hands focus back
        return true;
    }
    let preview = PREVIEW.with(|cell| cell.get_or_init(|| Preview::new(mtm)).clone());
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    *preview.ivars().url.borrow_mut() = Some(url);
    // SAFETY: `preview` lives in PREVIEW for the rest of the app's life.
    unsafe {
        panel.setDataSource(Some(ProtocolObject::from_ref(&*preview)));
        panel.setDelegate(Some(&preview));
    }
    panel.setSharingType(sharing(protected));
    remember_front();
    // What tao's set_focus does for the Settings window; NSApplication.activate()
    // needs macOS 14 and Ferry supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    panel.makeKeyAndOrderFront(None);
    // SAFETY: the data source is set.
    unsafe { panel.reloadData() };
    true
}

/// Follow "Show app UI in screenshots" live (`windows::set_content_protected`).
/// `qlmanage` is another process and always shows.
pub fn set_protected(app: &AppHandle, protected: bool) {
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        // SAFETY: on the main thread; only an existing panel is touched.
        if unsafe { QLPreviewPanel::sharedPreviewPanelExists(mtm) }
            && let Some(panel) = unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }
        {
            panel.setSharingType(sharing(protected));
        }
    });
}

fn sharing(protected: bool) -> NSWindowSharingType {
    if protected {
        NSWindowSharingType::None
    } else {
        NSWindowSharingType::ReadOnly
    }
}

/// `qlmanage -p` runs in its own process, takes focus and gives it back when
/// Esc closes it. A second press while it runs closes it.
fn toggle_qlmanage(path: &Path) {
    let mut running = QLMANAGE.lock().unwrap();
    if let Some(mut child) = running.take()
        && matches!(child.try_wait(), Ok(None))
    {
        let _ = child.kill();
        let _ = child.wait();
        return;
    }
    match Command::new("/usr/bin/qlmanage")
        .arg("-p")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => *running = Some(child),
        Err(e) => warn!("could not start qlmanage: {e}"),
    }
}
```

- [ ] **Step 3: Give Windows the same `set_protected`**

In `src-tauri/src/platform/windows_preview.rs`, add `use tauri::AppHandle;` on the line before `use windows_sys::Win32::Foundation::HWND;`, and append at the end of the file:

```rust

/// Windows has no Quick Look; the compare window follows the setting itself.
pub fn set_protected(_app: &AppHandle, _protected: bool) {}
```

- [ ] **Step 4: Let the screenshot setting reach Quick Look**

In `src-tauri/src/windows.rs`, in `set_content_protected`, change the doc comment's last line from `/// (R§3.1): every open window that follows the setting.` to `/// (R§3.1): every open window that follows the setting, and Quick Look.` and add one line after the `for` loop, so the function reads:

```rust
/// Hide Ferry's floating UI from screenshots and screen sharing, or show it
/// (R§3.1): every open window that follows the setting, and Quick Look.
pub fn set_content_protected(app: &AppHandle, protected: bool) {
    for (label, window) in app.webview_windows() {
        if follows_screenshot_setting(&label) {
            let _ = window.set_content_protected(protected);
        }
    }
    platform::preview::set_protected(app, protected);
}
```

- [ ] **Step 5: The command**

Create `src-tauri/src/preview.rs`:

```rust
//! Quick preview (Space, spec §4.5): Quick Look on macOS; on Windows the
//! compare window showing the result alone.

use ferry_core::job::JobId;
use tauri::AppHandle;

use crate::compare;

/// The card's hover Space and the menu's "Preview" (M3b). A second press closes the preview.
#[tauri::command]
pub async fn quick_preview(app: AppHandle, id: JobId) -> Result<(), String> {
    // While the pointer rests on a card, the hover hotkey takes Space before the
    // compare page sees it; Space still closes the window.
    if compare::is_active(&app) {
        compare::close(&app);
        return Ok(());
    }
    #[cfg(target_os = "macos")]
    {
        use tauri::Manager;
        let view = compare::view(&compare::card(&app, &id)?, true)?;
        let protected = !app
            .state::<crate::state::AppState>()
            .settings
            .read()
            .unwrap()
            .general
            .show_in_screenshots;
        crate::platform::preview::quick_look(&app, view.now.path, protected)
    }
    #[cfg(not(target_os = "macos"))]
    {
        compare::show(&app, &id, true)
    }
}
```

`use tauri::Manager;` sits inside the macOS block on purpose: at the top it would be an unused import on Windows and fail clippy there.

In `src-tauri/src/lib.rs`, add `mod preview;` on its own line right after `mod prefs;`, and in `generate_handler!` add `preview::quick_preview,` right after `compare::compare_close,`. The start of the list then reads:

```rust
        .invoke_handler(tauri::generate_handler![
            windows::stack_resize,
            compare::open_compare,
            compare::compare_ready,
            compare::compare_close,
            preview::quick_preview,
            commands::stack_ready,
```

- [ ] **Step 6: Build, fmt, clippy, test**

Run: `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferry`
Expected: `objc2-quick-look-ui v0.3.2` compiles; clippy prints no warning or error; the `ferry` lib tests report 0 failed (same count as after Task 4). A `deprecated` error means the `#[allow(deprecated)]` above `activateIgnoringOtherApps` was lost; put it back. Do not switch to `NSApplicationActivationOptions::ActivateIgnoringOtherApps` (deprecated in macOS 14 and ignored) or `NSApplication::activate()` (macOS 14 only; Ferry supports 13).

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Cargo.toml Cargo.lock src-tauri/src/preview.rs src-tauri/src/platform/macos_preview.rs src-tauri/src/platform/windows_preview.rs src-tauri/src/windows.rs src-tauri/src/lib.rs
git commit -m "feat(app): quick preview with Quick Look on macOS

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

---

### Task 6: Verify on this Mac

**Files:**
- Modify only if check 6 fails: `src-tauri/src/platform/macos_preview.rs` (`USE_PANEL`)

**Interfaces:**
- Consumes: the commands from Tasks 4 and 5.
- Produces: the record of spec §1 criterion 5 for M3c's half (⌘D opens compare with a working divider; Space opens Quick Look) in the final report.

These checks copy images, which replaces the real clipboard. Ask the user before running them. Use the dev identifier so the user's own Ferry settings and backups are not touched. Quit any running Ferry first, because single-instance would hand the launch to it.

- [ ] **Step 1: Full automated suite**

Run:

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
pnpm check && pnpm test && pnpm build
```

Expected: everything green; the GIF tests ran because `FERRY_GIFSICLE` is set.

- [ ] **Step 2: Start the dev app and make a card**

Run: `pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'`

Take a screenshot to the clipboard (⌘⌃⇧4, drag a large area). A card appears. Find its id:

```bash
grep "job finished" ~/Library/Logs/io.ferry.desktop.dev/ferry.$(date +%Y-%m-%d).log | tail -1
```

Expected: a line containing `job=` followed by the id (for example `job="19a0c3f2b11-0"`). Copy the id without quotes.

- [ ] **Step 3: Get a console that can call the commands**

If M3b is already merged into this branch, use the card instead: ⌘D (pointer on the card) for compare, Space for preview. Otherwise open Settings from the tray, right-click inside it, choose **Inspect Element**, and use the Console tab. Every call below is typed there. `setTimeout` gives you 4 s to click into another app first, so focus hand-back can be seen:

```js
const id = '<id>';
setTimeout(() => window.__TAURI_INTERNALS__.invoke('open_compare', { id }).then(console.log, console.error), 4000);
```

- [ ] **Step 4: Compare window checks**

Run the snippet, then click into TextEdit within 4 s. Check each item and write down pass or fail:

1. A window titled "Compare <name>.png" opens centred on the cards' screen, no larger than about 80 % of it, and has focus.
2. The header reads "Original <size> · <W×H> · PNG" on the left and "Now <size> · <W×H> · PNG (−NN%)" on the right; the stage shows the screenshot with the divider in the middle and the "Original" / "Now" tags.
3. Dragging the divider reveals more or less of each side; ← and → move it when it has focus.
4. **Side by side** shows two panes. **100%** shows the image at real pixel sharpness, **200%** larger; dragging the image pans both panes together; **Fit** recentres.
5. Esc closes the window and TextEdit is frontmost again (type a letter: it lands in TextEdit). Repeat with Space, then with the red close button.
6. Toggle: run
   ```js
   setTimeout(async () => { await window.__TAURI_INTERNALS__.invoke('open_compare', { id }); setTimeout(() => window.__TAURI_INTERNALS__.invoke('open_compare', { id }), 2000); }, 1000);
   ```
   Expected: the window opens with focus, then closes by itself 2 s later (the second call for the same card while it has focus closes it).
7. With "Show app UI in screenshots" off (default), open the window and press ⌘⇧3: the saved screenshot does not show the compare window. Turn the setting on from the tray menu while the window is open, press ⌘⇧3 again: now it shows.

- [ ] **Step 5: Quick preview checks**

```js
setTimeout(() => window.__TAURI_INTERNALS__.invoke('quick_preview', { id }).then(console.log, console.error), 4000);
```

Run it, then click into TextEdit within 4 s.

8. The Quick Look panel opens on the card's result (title is the file name, "Open with Preview" button).
9. Esc closes it and TextEdit is frontmost again. Then toggle: run
   ```js
   setTimeout(async () => { await window.__TAURI_INTERNALS__.invoke('quick_preview', { id }); setTimeout(() => window.__TAURI_INTERNALS__.invoke('quick_preview', { id }), 2000); }, 1000);
   ```
   Expected: the panel opens, then closes by itself 2 s later.
10. With "Show app UI in screenshots" off, ⌘⇧3 while the panel is open: the panel is not in the screenshot.

- [ ] **Step 6: Decide the Quick Look path**

If check 8 or 9 fails because the panel shows but Esc does not reach it (the panel is not key) or the panel never appears, set `const USE_PANEL: bool = false;` in `src-tauri/src/platform/macos_preview.rs`, rerun Step 1's Rust commands, restart the dev app and repeat checks 8–9. Expected with `qlmanage`: a plainer Quick Look window whose title starts with `[DEBUG]`; Esc closes it and focus returns; check 10 cannot pass (another process). Record which path shipped and why in the final report. If you changed the constant, commit:

```bash
git add src-tauri/src/platform/macos_preview.rs
git commit -m "fix(app): quick preview falls back to qlmanage

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

- [ ] **Step 7: Expired backup**

```bash
ls ~/Library/Application\ Support/io.ferry.desktop.dev/backups/
```

Move the folder named after the job id to the Desktop (keep it to put back). In the console run `window.__TAURI_INTERNALS__.invoke('open_compare', { id }).then(console.log, console.error)`.

11. Expected: the console prints the error `Original no longer kept` and no window opens; `quick_preview` for the same id still opens Quick Look. Move the folder back.

- [ ] **Step 8: Report**

Stop the dev app. In the final report, list checks 1–11 with pass or fail, the Quick Look path that shipped (panel or `qlmanage`), and anything that looked wrong. No commit unless Step 6 changed code.

---

## Rulings made while planning (for the reviewer)

1. **Command names.** The spec's §4.5 says `card_compare(id)` / `card_preview(id)`. This plan uses `open_compare(id)` / `quick_preview(id)` because M3b, written in parallel, was given those names. The wire shape `{ id }` is the same.
2. **What "100 %" means.** One image pixel per screen pixel (`1 / devicePixelRatio` CSS px), so a Retina screenshot at 100 % is sharp; Fit never enlarges past that.
3. **Cropped results do not line up.** `JobUpdate` carries no crop offset, so a result with another shape is drawn at its own size, centred, not stretched. Only a result with the original's shape (a downscale) is stretched to the original's size.
4. **The original's format** is read from the backup's first bytes (`sniff`), because `JobUpdate.format` is the result's format.
5. **Guards.** Compare needs the backup file to exist and the job to be idle; it does not check `can_edit`, which is also false for animated WebP/APNG that can still be compared. Quick preview needs only the result. M3b still disables its Compare button on `can_edit` per spec §6.
6. **Toggling.** ⌘D or Space for the card already on show closes the window; any `quick_preview` call closes a focused compare window, because the hover hotkey takes Space before the page sees it. Quick Look open on card A plus Space on card B closes Quick Look rather than switching.
7. **Screenshot setting.** Stack and compare window follow it; the Settings window does not; the Quick Look panel follows it through `NSWindowSharingType`. The `qlmanage` fallback cannot.
8. **Quick Look without a controller.** The data source and delegate are set directly on the shared panel. A spike confirmed it renders and closes cleanly from an accessory process. Whether it becomes key in the bundled app is checked in Task 6, with `USE_PANEL = false` (`qlmanage -p`) as the fallback.
9. **State.** The shown view lives in a `static Mutex` in `compare.rs`, not in `AppState`, so this plan does not touch `state.rs` or `setup()`. M3b edits those.
10. **No component tests.** The repo has no DOM test stack and adding one is out of bounds. Page logic is in `view.ts` (vitest); the rest is checked by hand in Task 6.

## Self-review

- **Spec coverage (M3c row):** compare window (Tasks 3, 4), Quick Look / Windows preview (Task 5), ⌘D and Space wiring (commands for M3b: Tasks 4, 5; toggle semantics: Task 4 test, Task 6 checks), "Original no longer kept" (Task 4), sized ≤ 80 % (Task 1), downscaled result at original size (Task 2), focus hand-back (Tasks 4, 5, 6), screenshot setting (Tasks 4, 5, 6), §1 criterion 5's compare/preview half (Task 6). The hover registration, rail button and menu items are M3b's.
- **Placeholders:** none; every code step has its full code.
- **Type consistency:** `CompareView` / `Shot` fields match between `compare.rs` (Task 4) and `view.ts` (Task 2): `id`, `name`, `original`, `now`, `path`, `bytes`, `dims` (`[w, h] | null`), `format`, `version`. Event names `compare:show` / `compare:hidden` and commands `compare_ready` / `compare_close` match between Tasks 3 and 4. `CHROME` 84 px = 36 px header + 48 px footer.
- **Verified before writing:** all Rust code in Tasks 1, 4 and 5 was compiled, clippy-checked (`-D warnings`) and tested on this Mac in a scratch copy of the repo with M3a's `JobUpdate` fields stubbed with M3a's exact types (31 `ferry` tests, 13 geometry tests). The Task 4 intermediate state (before Quick Look) was compiled separately. `src/compare/*` passed `pnpm test` (8 files on that copy), `pnpm check` (0 errors) and `pnpm build`. Windows-only code was not compiled (no target installed) and was checked against the `windows-sys` 0.61 source.
