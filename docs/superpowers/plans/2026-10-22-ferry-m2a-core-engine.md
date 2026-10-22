# Ferry M2a — `ferry-core` Engine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the platform-independent engine of Ferry Optimize in `crates/ferry-core`: in-memory PNG / JPEG / WebP / GIF optimisation with the never-worse rule, the sidecar runner, backups, safe file replacement, file-name templates, settings and the job types, all tested on Ubuntu, macOS and Windows.

**Architecture:** One crate with no Tauri or OS-UI dependency. Each module is one file with one job and takes its inputs explicitly (bytes, paths, a clock, a cancel token), so tests need no globals and no binary fixtures: every test image is generated in `testutil.rs`. `optimise(bytes, format, options)` decodes, re-encodes and grades the result as `Smaller`, `AlreadyOptimal` or `Larger`; the caller keeps the original unless it is `Smaller`. This is milestone **M2a** of three (M2b: app core loop; M2c: Settings and housekeeping). It produces no user-visible change on its own.

**Tech Stack:** Rust 2024 workspace; `imagequant` 4.4 + `oxipng` 10.2 + `png` 0.18 (PNG), `mozjpeg` 0.10 (JPEG), `webp` 0.3 (WebP), `gifsicle` 1.98 sidecar (GIF), `chrono` 0.4 (clock only), `serde` + `serde_json`, `libc` (macOS only), `tempfile` (tests).

**Spec:** `docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md` (approved; read §4.2, §5.7, §5.8 and §8 before starting). Requirements: `docs/requirements/ferry-optimize-requirements.md`.

## Global Constraints

Every task's requirements include these. Values are copied from the spec.

- Licence is `GPL-3.0-or-later`; the workspace uses Rust edition 2024 and `resolver = "3"`.
- Encoders (§2): "Hybrid: in-process Rust crates for PNG / JPEG / WebP; `gifsicle` as the one sidecar". PNG is "`imagequant` 4.x … then `oxipng` 10.x"; WebP is "`webp` 0.3.x (libwebp) for still images; animated WebP is kept unchanged"; GIF is "`gifsicle` 1.98 built from pinned source".
- Level mapping (§4.2), with q = `compression` and aggressive = `max(5, q − 20)`:

  | Format | Normal | Aggressive |
  | --- | --- | --- |
  | PNG | imagequant quality `0..=q`, speed 4; oxipng preset 2 | quality `0..=q−20`; oxipng preset 4 |
  | JPEG | mozjpeg quality q, progressive, optimised Huffman | quality q−20 |
  | WebP (still) | lossy quality q, method 4, alpha kept | quality q−20 |
  | GIF | `gifsicle -O3 --lossy=L --colors 256`, where `L = round((100 − q) × 1.5)` | the same with q−20 |

- Outcome (§4.2): `Smaller` is at least 1% smaller than the input; `AlreadyOptimal` is "within 1%, or animated WebP"; `Larger` is "encoder output bigger than input". "Never worse": the original is kept unless the result is `Smaller` (R§2.4).
- Metadata (§5.8): PNG keeps `iCCP`, `sRGB`, `gAMA`, `cHRM` and `pHYs` and strips the other non-rendering chunks; JPEG always keeps the ICC profile (APP2) and the JFIF density, and when stripping writes a minimal EXIF holding only `Orientation`; WebP keeps `ICCP` and drops `EXIF` and `XMP` when stripping; GIF drops comments and extensions but keeps the loop count.
- `sidecar::run` "writes stdin on its own thread and drains stdout and stderr continuously", kills the child "on timeout (60 s by default), on cancel, and when dropped", and puts stderr into the error.
- `atomic_replace` writes `target.ferry-tmp` in the same directory, `fsync`s it, then renames it over the target. `copy_xattrs` is macOS only and gives up after 5 s (NFR-P4).
- Backups live at `<root>/<yyyy-mm-dd>/<job-id>/<name>`; `prune` removes whole date folders older than the retention.
- Settings: "Unknown fields are kept when the file is saved; missing fields take their defaults." A corrupt file gives defaults and a `.bak` copy. Default hotkeys are `Ctrl+Shift+C/A/P` on macOS and `Ctrl+Alt+Shift+C/A/P` on Windows.
- Templates: tokens `%f %e %z %y %m %d %P`; `validate` "returns the unknown tokens".
- `ferry-core` tests run in CI on Ubuntu, macOS and Windows, and no test depends on a binary fixture in git.
- No `AGENTS.md` or `CONSTRAINTS.md` exists at the repo root (checked 2026-10-22). Reuse the patterns already in `crates/ferry-core` (module doc comment, `#[cfg(test)] mod tests`, `cargo fmt`, clippy `-D warnings`).

## Decisions made while planning

These are mine, not the spec's. Veto any of them before execution; each says what it costs to change.

1. **mozjpeg without `nasm_simd`.** Spec §2 says "`mozjpeg` 0.10.x with default `nasm_simd`". This plan uses `default-features = false`. SIMD needs NASM installed on this Mac (a download, so it needs your say-so) and in seven CI jobs (core ×3, bundle ×2, release ×2), and the x86_64 half of the universal macOS build compiles it on an arm64 runner. I could not measure the gain because NASM is not installed here, and the spec's only speed gate, NFR-P2 (§8 manual check 2), is about a 5 MB *PNG*. *If wrong:* a one-line `Cargo.toml` change plus a NASM install step per workflow.
2. **gifsicle strips comments explicitly.** §5.8 says `-O3` drops comments and extensions. gifsicle 1.98 keeps both under `-O3` (measured), so `strip_metadata` adds `--no-comments --no-extensions`; the loop count survives. *If wrong:* a GIF keeps its comment text.
3. **"Re-optimising gives AlreadyOptimal" holds for WebP only.** §8 claims it for every format. Measured: a second PNG or JPEG pass still finds a little more to remove. The plan pins WebP and does not claim the rest. Nothing loops, because the app records its own clipboard writes (§5.2).
4. **Dates are local time, via `chrono` with only the `clock` feature.** Backup folders and `%y %m %d` use the user's local date; UTC would put a Vietnam-time user on yesterday for the first 7 hours of every day. `%y` is the four-digit year.
5. **`Backups::prune(retention_days: u32)`** replaces the spec's `prune(retention: Duration, now)`: `Backups` already owns the clock, and the setting is whole days. Today's folder is never removed. `store_*` return `io::Result<PathBuf>` and the first copy for a job wins, so an optimised file can never overwrite its original.
6. **`copy_xattrs(from, to)` is a no-op off macOS** so callers need no `cfg`. The app calls it after `atomic_replace` with the *backed-up original* as `from`: `fs::copy` keeps xattrs on macOS (checked), and replacing a file drops the old ones.
7. **A truncated JPEG is refused.** libjpeg pads a truncated scan with grey and only warns, so a half-downloaded JPEG would be rewritten as a damaged one. `jpeg.rs` walks the marker structure and returns `Decode("the JPEG is incomplete")` if it never reaches EOI. Found while writing the broken-input test.
8. **Settings detail the spec leaves open:** `corner`, `card_size` (`default | compact`, R§4.4), `start_state` and `tray_style` are enums; `floating.auto_hide_secs = 0` means "never hide" (R§4.4 "or never"); `images.compression` is clamped to 5..=100 and both `max_size_mb` to at least 1 on load; a value of the wrong type counts as a corrupt file (defaults, `.bak` kept).
9. **`JobId`** is `<unix-millis-hex>-<counter-hex>`: unique across launches, because it names a backup folder and a counter alone would collide with yesterday's job 1.
10. **`scripts/build-gifsicle.sh` gains `x86_64-unknown-linux-gnu`** so the Ubuntu core job builds the same pinned source as the other OSes instead of using a distro package.
11. **PNG effort for Normal is imagequant speed 8 and oxipng preset 1, not the spec's 4 and 2.** Spec §4.2's table and its NFR-P2 target (a 5 MB image in under 1.5 s) disagree. On this Mac, with a 5.1 MB generated screenshot and other work running (load 7 to 17 on 8 cores, so these times are pessimistic): speed 4 + preset 2 took 2.0 to 2.5 s; speed 4 + preset 1 took 1.6 s; speed 8 + preset 1 took 0.8 to 1.1 s and wrote a file within 1% of the same size (635 KB against 630 KB). Normal now uses speed 8 + preset 1; Aggressive keeps the spec's speed 4 + preset 4, since it is a deliberate slower retry. *If wrong:* one function (`effort` in `png.rs`).
12. **JPEG and WebP miss NFR-P2 on 5 MB inputs, and M2a does not change that.** Measured on the same busy Mac with generated 12-megapixel images: a 4.3 MB JPEG took 4.4 to 4.9 s with mozjpeg's default profile (the trellis quantisation is about 3 s of that; libjpeg-turbo-style fast settings took 0.8 s but wrote a file about 20% bigger), and a 3.5 MB WebP took 2.3 s at method 4. The spec's manual check 2 only names a PNG, so this does not block M2a, but §1 criterion 2 says "a 5 MB clipboard image". Left open for M2b: add a `quick` flag to `OptimiseOptions` that clipboard jobs set (PNG speed 8 / preset 1, JPEG fast profile, WebP method 2) while file jobs keep the spec's settings, or accept the longer card time for large photos. Your call before M2b is planned.

## Review Focus

Failure modes the spec implies but no spec test names, most likely first. Each has a test in the task that owns the code.

1. **A truncated or corrupt image of any format** should leave the original alone: an error, never a panic and never a rewrite padded with grey. Tests: `a_truncated_jpeg_is_refused_not_padded_with_grey` (Task 5), `broken_input_is_an_error_not_a_panic` (Task 9).
2. **Animated images** (APNG, animated WebP, multi-frame GIF) should keep every frame. Tests: `png_with_an_animation_control_chunk_is_animated` (Task 2), `animated_webp_and_apng_are_left_alone` (Task 9), `an_animated_gif_keeps_every_frame_and_its_loop` (Task 8).
3. **A tiny or already-optimised image** should never come back bigger. Tests: `a_one_pixel_png_comes_back_larger_so_the_original_is_kept`, `re_optimising_a_webp_is_already_optimal` (Task 9).
4. **A hand-edited or half-written `settings.json`** (empty, wrong type, `compression` 0 or 200, extra keys) should still open the app with sensible values and keep the bad file as `.bak`. Tests: `an_empty_file_and_a_non_object_count_as_corrupt`, `out_of_range_values_are_clamped`, `unknown_fields_survive_a_load_and_save_at_every_level` (Task 13).
5. **Names containing separators, `..` or Windows-forbidden characters** (a template, a job id, a backup name) should never write outside their folder. Tests: `a_name_cannot_climb_out_of_the_backup_folder` (Task 10), `safe_file_name_blocks_separators_and_windows_characters` (Task 12). Related file-safety tests: a failed rename leaves the original untouched, permissions survive, a symlink target keeps its link (Task 11).

## File Map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/Cargo.toml` | Dependencies; the `sidecar-fixture` test binary | 1, 7 |
| `crates/ferry-core/src/lib.rs` | Module list | 1 to 14 |
| `src/cancel.rs` | `CancelToken`, a shared stop flag | 1 |
| `src/sniff.rs` (modify) | `is_animated`, `gif_frame_count` | 2 |
| `src/testutil.rs` | Generated test images, no binary fixtures (`#[cfg(test)]`) | 2, 5, 6, 8, 10 |
| `src/optimise/mod.rs` | `Level`, `OptimiseOptions`, `Outcome`, `OptimiseError`, `quality`, `classify`, `optimise` | 3, 9 |
| `src/optimise/png.rs` | imagequant + oxipng | 4 |
| `src/optimise/jpeg.rs` | mozjpeg, EXIF and ICC handling, truncation check | 5 |
| `src/optimise/webp.rs` | libwebp + RIFF assembly for ICCP/EXIF/XMP | 6 |
| `src/sidecar.rs`, `tests/sidecar.rs`, `tests/support/sidecar_fixture.rs` | Child-process runner and its tests | 7 |
| `src/optimise/gif.rs` | gifsicle through the sidecar runner | 8 |
| `src/backup.rs` | Originals under date and job folders; pruning | 10 |
| `src/fsutil.rs` | `atomic_replace`, `copy_times`, `copy_xattrs` | 11 |
| `src/template.rs` | `render`, `validate`, `safe_file_name` | 12 |
| `src/settings.rs` | Settings model, tolerant load, atomic save, `~` paths | 13 |
| `src/job.rs` | `JobId`, `JobState`, `JobUpdate` | 14 |
| `.github/workflows/ci.yml`, `scripts/build-gifsicle.sh`, `README.md` (modify) | Core job builds gifsicle and sets `FERRY_GIFSICLE` on three OSes | 15 |

## How to run things

Every Bash call needs the Rust environment: `source "$HOME/.cargo/env"`. GIF tests skip silently unless `FERRY_GIFSICLE` points at the sidecar; this Mac already has one built:

```bash
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
```

Dead-code warnings from `optimise/*` modules are expected until Task 9 wires them in; `cargo test` ignores them and clippy runs at the end.

---

### Task 1: Dependencies, module list and `CancelToken`

**Files:**
- Modify: `crates/ferry-core/Cargo.toml`
- Modify: `crates/ferry-core/src/lib.rs`
- Create: `crates/ferry-core/src/cancel.rs`
- Modify: `Cargo.lock` (by cargo)

**Interfaces:**
- Produces: `ferry_core::cancel::CancelToken` — `Clone + Default + Debug`; `CancelToken::new()`, `.cancel()`, `.is_cancelled() -> bool`. Clones share one flag.

- [ ] **Step 1: Start the branch**
```bash
git switch -c m2a-core-engine
```

Work in place on this branch (no worktree), as for M1.

- [ ] **Step 2: Write the failing test**

Create `crates/ferry-core/src/cancel.rs` with only the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clones_share_one_flag() {
        let token = CancelToken::new();
        let seen_by_job = token.clone();
        assert!(!seen_by_job.is_cancelled());
        token.cancel();
        assert!(seen_by_job.is_cancelled());
    }
}
```

Replace `crates/ferry-core/src/lib.rs` with:

```rust
//! Ferry's platform-independent core. No Tauri or OS UI dependencies, so every
//! module is unit-tested on macOS, Windows and Linux.

pub mod cancel;
pub mod geometry;
pub mod sniff;
```

- [ ] **Step 3: Run it to see it fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- cancel::`
Expected: FAIL to compile, `cannot find type CancelToken in this scope`.

- [ ] **Step 4: Implement `CancelToken` and add the dependencies**

Put this above the test module in `cancel.rs`:

```rust
//! A cheap flag a job's owner flips to stop its encoder or sidecar.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct CancelToken(Arc<AtomicBool>);

impl CancelToken {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.0.store(true, Ordering::Relaxed);
    }

    pub fn is_cancelled(&self) -> bool {
        self.0.load(Ordering::Relaxed)
    }
}
```

Replace `crates/ferry-core/Cargo.toml` with (the `[[bin]]` entry arrives in Task 7):

```toml
[package]
name = "ferry-core"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
chrono = { version = "0.4", default-features = false, features = ["clock"] }
imagequant = "4.4"
# No nasm_simd, so building Ferry never needs NASM. Revisit if JPEG latency matters.
mozjpeg = { version = "0.10", default-features = false }
oxipng = { version = "10.2", default-features = false, features = ["parallel"] }
png = "0.18"
serde = { version = "1", features = ["derive"] }
serde_json = "1"
# Default features pull in the `image` crate, which Ferry does not use.
webp = { version = "0.3", default-features = false }

[target.'cfg(target_os = "macos")'.dependencies]
libc = "0.2"

[dev-dependencies]
tempfile = "3"
```

- [ ] **Step 5: Run it to see it pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- cancel::`
Expected: PASS, 1 test. The first build compiles libwebp and mozjpeg from C source and takes a few minutes.

Run: `source "$HOME/.cargo/env" && cargo build --workspace`
Expected: finishes with no error (`src-tauri` still builds).

- [ ] **Step 6: Commit**
```bash
git add crates/ferry-core Cargo.lock
git commit -m "feat(core): add CancelToken and engine dependencies

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- cancel::`

---

### Task 2: Animation detection and the test-image helpers

**Files:**
- Modify: `crates/ferry-core/src/sniff.rs`
- Create: `crates/ferry-core/src/testutil.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `sniff::Format` (`Png | Jpeg | WebP | Gif`), already in `sniff.rs`.
- Produces: `sniff::is_animated(bytes: &[u8], format: Format) -> bool` (PNG `acTL`, GIF with more than one frame, WebP with an `ANIM` chunk or the VP8X animation flag; JPEG never); `sniff::gif_frame_count(bytes: &[u8]) -> usize`.
- Produces (tests only, `crate::testutil`): `screenshot_rgba(w, h) -> Vec<u8>`, `png(w, h) -> Vec<u8>`, `with_png_chunk(png, &[u8; 4], data) -> Vec<u8>`, `png_chunk_names(png) -> Vec<String>`, `gif(frames) -> Vec<u8>`, `animated_webp() -> Vec<u8>`.

- [ ] **Step 1: Create the generated-image helpers**

Create `crates/ferry-core/src/testutil.rs`. Images are generated, not stored: the screenshot has flat panels, a gradient bar and rows of speckles, so it has more than 256 colours but low entropy, like a real UI.

```rust
//! Deterministic synthetic images for tests, so no binary fixtures live in git.

/// A screenshot-like RGBA image: flat panels, a gradient bar and rows of dark
/// "text" speckles. More than 256 colours, but low entropy, like a real UI.
pub fn screenshot_rgba(w: u32, h: u32) -> Vec<u8> {
    let mut seed = 0x2545_F491_u32;
    let mut noise = move || {
        seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (seed >> 24) as u8
    };
    let mut px = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let (mut r, mut g, mut b) = if x < w / 5 {
                (233, 233, 237) // sidebar
            } else if y < h / 8 {
                // gradient header
                let t = (x * 255 / w) as u8;
                (t, 100 + t / 3, 240 - t / 2)
            } else {
                (250, 250, 252) // content
            };
            if x >= w / 5 && y > h / 8 && (y / 6) % 3 == 0 && noise() > 150 {
                (r, g, b) = (40 + noise() % 30, 40 + noise() % 30, 48 + noise() % 30);
            }
            px.extend_from_slice(&[r, g, b, 255]);
        }
    }
    px
}

/// A plain (unoptimised) RGBA PNG of `screenshot_rgba`.
pub fn png(w: u32, h: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut enc = png::Encoder::new(&mut out, w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    let mut writer = enc.write_header().unwrap();
    writer.write_image_data(&screenshot_rgba(w, h)).unwrap();
    writer.finish().unwrap();
    out
}

/// Insert an ancillary chunk right after IHDR.
pub fn with_png_chunk(png: &[u8], name: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let ihdr_end = 8 + 12 + 13;
    let mut out = png[..ihdr_end].to_vec();
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = name.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
    out.extend_from_slice(&png[ihdr_end..]);
    out
}

/// The names of every chunk in a PNG, in file order.
pub fn png_chunk_names(png: &[u8]) -> Vec<String> {
    let mut names = Vec::new();
    let mut pos = 8;
    while let Some(head) = png.get(pos..pos + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        names.push(String::from_utf8_lossy(&head[4..8]).into_owned());
        pos += 12 + len;
    }
    names
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for &byte in bytes {
        crc ^= u32::from(byte);
        for _ in 0..8 {
            crc = (crc >> 1) ^ (0xEDB8_8320 & 0_u32.wrapping_sub(crc & 1));
        }
    }
    !crc
}

/// A GIF with `frames` frames of `screenshot_rgba`-like content, 64×48, stored
/// without LZW compression (a clear code every 200 pixels keeps codes 9 bits).
pub fn gif(frames: usize) -> Vec<u8> {
    let (w, h) = (64_u16, 48_u16);
    let mut out = b"GIF89a".to_vec();
    out.extend_from_slice(&w.to_le_bytes());
    out.extend_from_slice(&h.to_le_bytes());
    out.extend_from_slice(&[0xF7, 0, 0]); // global table, 256 colours
    for i in 0..256_u32 {
        out.extend_from_slice(&[i as u8, (i * 7) as u8, (255 - i) as u8]);
    }
    if frames > 1 {
        out.extend_from_slice(b"\x21\xFF\x0BNETSCAPE2.0\x03\x01\x00\x00\x00"); // loop forever
    }
    for f in 0..frames {
        out.extend_from_slice(&[0x21, 0xF9, 4, 0, 10, 0, 0, 0]); // 100 ms per frame
        out.push(0x2C);
        out.extend_from_slice(&[0, 0, 0, 0]);
        out.extend_from_slice(&w.to_le_bytes());
        out.extend_from_slice(&h.to_le_bytes());
        out.push(0);
        out.push(8); // LZW minimum code size
        let pixels: Vec<u8> = (0..u32::from(w) * u32::from(h))
            .map(|i| ((i / 24 + f as u32 * 40) % 256) as u8)
            .collect();
        for block in lzw_stored(&pixels).chunks(255) {
            out.push(block.len() as u8);
            out.extend_from_slice(block);
        }
        out.push(0);
    }
    out.push(0x3B);
    out
}

/// GIF LZW with min code size 8 that never compresses: 9-bit codes, a clear
/// code (256) before every 200 pixels, end code (257) last.
fn lzw_stored(pixels: &[u8]) -> Vec<u8> {
    struct Bits {
        out: Vec<u8>,
        acc: u32,
        n: u32,
    }
    impl Bits {
        fn put(&mut self, code: u32) {
            self.acc |= code << self.n;
            self.n += 9;
            while self.n >= 8 {
                self.out.push(self.acc as u8);
                self.acc >>= 8;
                self.n -= 8;
            }
        }
    }
    let mut bits = Bits {
        out: Vec::new(),
        acc: 0,
        n: 0,
    };
    for run in pixels.chunks(200) {
        bits.put(256);
        for &p in run {
            bits.put(u32::from(p));
        }
    }
    bits.put(257);
    if bits.n > 0 {
        bits.out.push(bits.acc as u8);
    }
    bits.out
}

/// A minimal RIFF/WebP container with the animation flag and an ANIM chunk.
pub fn animated_webp() -> Vec<u8> {
    let mut body = b"WEBP".to_vec();
    body.extend_from_slice(b"VP8X");
    body.extend_from_slice(&10_u32.to_le_bytes());
    body.extend_from_slice(&[0x02, 0, 0, 0, 0, 0, 0, 0, 0, 0]);
    body.extend_from_slice(b"ANIM");
    body.extend_from_slice(&6_u32.to_le_bytes());
    body.extend_from_slice(&[0; 6]);
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}
```

Add to `lib.rs`, after the `pub mod` lines:

```rust
#[cfg(test)]
mod testutil;
```

- [ ] **Step 2: Write the failing tests**

Append these inside `mod tests` at the bottom of `crates/ferry-core/src/sniff.rs`, before its final `}`:

```rust
    #[test]
    fn png_with_an_animation_control_chunk_is_animated() {
        let plain = crate::testutil::png(8, 8);
        assert!(!is_animated(&plain, Format::Png));
        let apng = crate::testutil::with_png_chunk(&plain, b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]);
        assert!(is_animated(&apng, Format::Png));
    }

    #[test]
    fn gif_is_animated_only_with_more_than_one_frame() {
        assert!(!is_animated(&crate::testutil::gif(1), Format::Gif));
        assert!(is_animated(&crate::testutil::gif(2), Format::Gif));
        assert!(!is_animated(b"GIF89a", Format::Gif)); // truncated
    }

    #[test]
    fn webp_is_animated_when_it_carries_an_anim_chunk() {
        assert!(is_animated(&crate::testutil::animated_webp(), Format::WebP));
        assert!(!is_animated(b"RIFF\x24\0\0\0WEBPVP8 ", Format::WebP));
    }

    #[test]
    fn jpeg_is_never_animated() {
        assert!(!is_animated(&[0xFF, 0xD8, 0xFF, 0xDB], Format::Jpeg));
    }
```

- [ ] **Step 3: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- sniff::`
Expected: FAIL to compile, `cannot find function is_animated in this scope`.

- [ ] **Step 4: Implement**

Insert this in `sniff.rs` between the `sniff` function and `#[cfg(test)]`:

```rust
/// True when the image has more than one frame: a GIF with several image
/// descriptors, a WebP with an animation chunk, or a PNG with an `acTL` chunk.
pub fn is_animated(bytes: &[u8], format: Format) -> bool {
    match format {
        Format::Png => png_has_actl(bytes),
        Format::Gif => gif_frame_count(bytes) > 1,
        Format::WebP => webp_has_animation(bytes),
        Format::Jpeg => false,
    }
}

fn png_has_actl(bytes: &[u8]) -> bool {
    let mut pos = 8; // after the signature
    while let Some(head) = bytes.get(pos..pos + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        match &head[4..8] {
            b"acTL" => return true,
            b"IDAT" | b"IEND" => return false,
            _ => pos += 12 + len, // length + type + data + crc
        }
    }
    false
}

fn webp_has_animation(bytes: &[u8]) -> bool {
    let mut pos = 12; // after "RIFF<size>WEBP"
    while let Some(head) = bytes.get(pos..pos + 8) {
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        match &head[0..4] {
            b"ANIM" => return true,
            // VP8X flags: bit 1 is the animation flag.
            b"VP8X" if bytes.get(pos + 8).is_some_and(|flags| flags & 0x02 != 0) => return true,
            _ => pos += 8 + len + (len & 1), // chunks are padded to even length
        }
    }
    false
}

/// The number of image descriptors (frames) in a GIF; 0 for anything truncated
/// before its first frame.
pub fn gif_frame_count(bytes: &[u8]) -> usize {
    let Some(&screen_flags) = bytes.get(10) else {
        return 0;
    };
    let mut pos = 13;
    if screen_flags & 0x80 != 0 {
        pos += 3 << ((screen_flags & 7) + 1);
    }
    let mut frames = 0;
    while let Some(&block) = bytes.get(pos) {
        match block {
            0x21 => pos = skip_sub_blocks(bytes, pos + 2), // extension: label, then sub-blocks
            0x2C => {
                frames += 1;
                let Some(&image_flags) = bytes.get(pos + 9) else {
                    return frames;
                };
                pos += 10;
                if image_flags & 0x80 != 0 {
                    pos += 3 << ((image_flags & 7) + 1);
                }
                pos = skip_sub_blocks(bytes, pos + 1); // LZW minimum code size, then data
            }
            _ => return frames, // trailer (0x3B) or garbage
        }
    }
    frames
}

/// Position after a run of GIF data sub-blocks that starts at `pos`.
fn skip_sub_blocks(bytes: &[u8], mut pos: usize) -> usize {
    while let Some(&len) = bytes.get(pos) {
        pos += 1 + len as usize;
        if len == 0 {
            break;
        }
    }
    pos
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- sniff::`
Expected: PASS, 11 tests (the earlier ones plus 4 new).

- [ ] **Step 6: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): detect animated PNG, GIF and WebP

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- sniff::`

---

### Task 3: Optimise types, quality mapping and the never-worse grade

**Files:**
- Create: `crates/ferry-core/src/optimise/mod.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Produces: `optimise::{Level, OptimiseOptions, Outcome, OptimiseError, quality, classify}`:
  - `Level::{Normal, Aggressive}`.
  - `OptimiseOptions { level, compression: u8, strip_metadata: bool, gifsicle: Option<PathBuf>, cancel: CancelToken }`; `Default` is Normal, 80, `true`, `None`, a fresh token.
  - `Outcome::{Smaller { bytes: Vec<u8> }, AlreadyOptimal, Larger}`.
  - `OptimiseError::{Decode(String), Encode(String), Unsupported(String), Cancelled}` with `Display` and `Error`.
  - `quality(&OptimiseOptions) -> u8`: `compression` clamped to 5..=100, or 20 lower (never below 5) when aggressive.
  - `classify(input_len: usize, output: Vec<u8>) -> Outcome`: at most 99% of the input is `Smaller`, more than 100% is `Larger`, anything between is `AlreadyOptimal`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/optimise/mod.rs` with only this test module, and add `pub mod optimise;` to `lib.rs` (the list stays alphabetical: `cancel`, `geometry`, `optimise`, `sniff`).

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn opts(level: Level, compression: u8) -> OptimiseOptions {
        OptimiseOptions {
            level,
            compression,
            ..OptimiseOptions::default()
        }
    }

    #[test]
    fn aggressive_is_twenty_lower_and_never_below_five() {
        assert_eq!(quality(&opts(Level::Normal, 80)), 80);
        assert_eq!(quality(&opts(Level::Aggressive, 80)), 60);
        assert_eq!(quality(&opts(Level::Aggressive, 20)), 5);
        assert_eq!(quality(&opts(Level::Aggressive, 5)), 5);
        assert_eq!(quality(&opts(Level::Normal, 0)), 5);
    }

    #[test]
    fn classify_boundaries() {
        // 1000-byte input: 990 is exactly 1% smaller.
        assert!(matches!(
            classify(1000, vec![0; 990]),
            Outcome::Smaller { .. }
        ));
        assert_eq!(classify(1000, vec![0; 991]), Outcome::AlreadyOptimal);
        assert_eq!(classify(1000, vec![0; 1000]), Outcome::AlreadyOptimal);
        assert_eq!(classify(1000, vec![0; 1001]), Outcome::Larger);
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::`
Expected: FAIL to compile, `cannot find type OptimiseOptions in this scope`.

- [ ] **Step 3: Implement**

Put this above the test module in `optimise/mod.rs`. (`optimise()` itself arrives in Task 9, once every format exists.)

```rust
//! Optimise one image from memory (spec §4.2). Never worse than the input:
//! the caller keeps the original unless the result is `Smaller`.

use std::{fmt, path::PathBuf};

use crate::cancel::CancelToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Level {
    Normal,
    Aggressive,
}

#[derive(Debug, Clone)]
pub struct OptimiseOptions {
    pub level: Level,
    /// 5..=100, from settings (default 80).
    pub compression: u8,
    pub strip_metadata: bool,
    /// Resolved sidecar path; `None` makes GIF return `Unsupported`.
    pub gifsicle: Option<PathBuf>,
    pub cancel: CancelToken,
}

impl Default for OptimiseOptions {
    fn default() -> Self {
        Self {
            level: Level::Normal,
            compression: 80,
            strip_metadata: true,
            gifsicle: None,
            cancel: CancelToken::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Outcome {
    /// At least 1% smaller than the input.
    Smaller { bytes: Vec<u8> },
    /// Within 1% of the input, or an animation Ferry keeps unchanged.
    AlreadyOptimal,
    /// The encoder's output is bigger than the input.
    Larger,
}

#[derive(Debug)]
pub enum OptimiseError {
    Decode(String),
    Encode(String),
    Unsupported(String),
    Cancelled,
}

impl fmt::Display for OptimiseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Decode(m) => write!(f, "could not read the image: {m}"),
            Self::Encode(m) => write!(f, "encoder failed: {m}"),
            Self::Unsupported(m) => write!(f, "unsupported: {m}"),
            Self::Cancelled => write!(f, "cancelled"),
        }
    }
}

impl std::error::Error for OptimiseError {}

/// The encoder quality for these options: `compression`, or 20 lower (never
/// below 5) when aggressive.
pub fn quality(o: &OptimiseOptions) -> u8 {
    let q = o.compression.clamp(5, 100);
    match o.level {
        Level::Normal => q,
        Level::Aggressive => q.saturating_sub(20).max(5),
    }
}

/// Grade an encoder's output against the input it came from.
pub fn classify(input_len: usize, output: Vec<u8>) -> Outcome {
    // `output <= 99% of input` in integers, so exactly 1% smaller counts as Smaller.
    if output.len() * 100 <= input_len * 99 {
        Outcome::Smaller { bytes: output }
    } else if output.len() > input_len {
        Outcome::Larger
    } else {
        Outcome::AlreadyOptimal
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::`
Expected: PASS, 2 tests (`aggressive_is_twenty_lower_and_never_below_five`, `classify_boundaries`).

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): optimise options, quality mapping and never-worse grading

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::`

---

### Task 4: PNG — imagequant then oxipng

**Files:**
- Create: `crates/ferry-core/src/optimise/png.rs`
- Modify: `crates/ferry-core/src/optimise/mod.rs` (add `mod png;`)

**Interfaces:**
- Consumes: `OptimiseOptions`, `OptimiseError`, `quality` (Task 3); `testutil::{png, with_png_chunk, png_chunk_names}` (Task 2).
- Produces: `optimise::png::optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>` (private module; `optimise()` calls it in Task 9). The output is an indexed PNG. Effort follows Decision 11: Normal imagequant speed 8 + oxipng preset 1, Aggressive speed 4 + preset 4. Rendering chunks (`iCCP sRGB gAMA cHRM pHYs`) are always kept; with `strip_metadata == false` every other ancillary chunk except the ones the new file rebuilds is kept too.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/optimise/png.rs` containing only this test module, and add `mod png;` to the top of `optimise/mod.rs`, under the module doc comment:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn decoded_size(bytes: &[u8]) -> (u32, u32) {
        let (w, h, _) = decode(bytes).expect("output decodes");
        (w, h)
    }

    #[test]
    fn a_screenshot_shrinks_and_still_decodes_to_the_same_size() {
        let input = testutil::png(600, 400);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        assert_eq!(decoded_size(&out), (600, 400));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let input = testutil::png(600, 400);
        let normal = optimise(&input, &OptimiseOptions::default()).unwrap();
        let aggressive = optimise(
            &input,
            &OptimiseOptions {
                level: super::super::Level::Aggressive,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        assert!(aggressive.len() <= normal.len());
    }

    #[test]
    fn rendering_chunks_survive_and_text_is_stripped() {
        let base = testutil::png(64, 64);
        let input = testutil::with_png_chunk(&base, b"pHYs", &[0, 0, 11, 19, 0, 0, 11, 19, 1]);
        let input = testutil::with_png_chunk(&input, b"tEXt", b"Comment\0hello");
        let names =
            testutil::png_chunk_names(&optimise(&input, &OptimiseOptions::default()).unwrap());
        assert!(names.contains(&"pHYs".to_string()), "{names:?}");
        assert!(!names.contains(&"tEXt".to_string()), "{names:?}");
    }

    #[test]
    fn text_is_kept_when_metadata_is_not_stripped() {
        let input = testutil::with_png_chunk(&testutil::png(64, 64), b"tEXt", b"Comment\0hello");
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let names = testutil::png_chunk_names(&optimise(&input, &o).unwrap());
        assert!(names.contains(&"tEXt".to_string()), "{names:?}");
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            optimise(b"\x89PNG\r\n\x1a\nnot really", &OptimiseOptions::default()),
            Err(OptimiseError::Decode(_))
        ));
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::png::`
Expected: FAIL to compile, unresolved `optimise` / `decode`.

- [ ] **Step 3: Implement**

Put this above the test module:

```rust
//! PNG: quantise with imagequant (pngquant's engine), then recompress with oxipng.

use std::io::Cursor;

use imagequant::RGBA;
use oxipng::{BitDepth, ColorType, Options, RGBA8, RawImage};

use super::{OptimiseError, OptimiseOptions, quality};

/// Chunks that change how the picture looks, kept whatever the metadata setting.
const RENDERING: [[u8; 4]; 5] = [*b"iCCP", *b"sRGB", *b"gAMA", *b"cHRM", *b"pHYs"];
/// Chunks that describe the old pixel data or file layout; invalid after re-encoding.
const REBUILT: [[u8; 4]; 7] = [
    *b"IHDR", *b"PLTE", *b"IDAT", *b"IEND", *b"tRNS", *b"bKGD", *b"hIST",
];

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let (width, height, rgba) = decode(input)?;
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }

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
    for (name, data) in kept_chunks(input, o.strip_metadata) {
        raw.add_png_chunk(name, data);
    }
    raw.create_optimized_png(&Options::from_preset(preset))
        .map_err(encode_err)
}

/// imagequant speed and oxipng preset. Spec §4.2 asks for speed 4 and preset 2, but
/// that takes about 2.5 s on a 5 MB screenshot and NFR-P2 allows 1.5 s, so Normal uses
/// speed 8 and preset 1: the same palette size and output about 1% bigger. Aggressive
/// is a deliberate slower retry and keeps the spec's settings (speed 4, preset 4).
fn effort(o: &OptimiseOptions) -> (i32, u8) {
    match o.level {
        super::Level::Normal => (8, 1),
        super::Level::Aggressive => (4, 4),
    }
}

fn encode_err(e: impl std::fmt::Display) -> OptimiseError {
    OptimiseError::Encode(e.to_string())
}

/// Decode any PNG to 8-bit RGBA.
fn decode(input: &[u8]) -> Result<(u32, u32, Vec<RGBA>), OptimiseError> {
    let decode_err = |e: png::DecodingError| OptimiseError::Decode(e.to_string());
    let mut decoder = png::Decoder::new(Cursor::new(input));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().map_err(decode_err)?;
    let size = reader
        .output_buffer_size()
        .ok_or_else(|| OptimiseError::Decode("image is too large".into()))?;
    let mut buf = vec![0; size];
    let info = reader.next_frame(&mut buf).map_err(decode_err)?;
    let data = &buf[..info.buffer_size()];
    let pixels = match info.color_type {
        png::ColorType::Rgba => data
            .as_chunks::<4>()
            .0
            .iter()
            .map(|p| RGBA::new(p[0], p[1], p[2], p[3]))
            .collect(),
        png::ColorType::Rgb => data
            .as_chunks::<3>()
            .0
            .iter()
            .map(|p| RGBA::new(p[0], p[1], p[2], 255))
            .collect(),
        png::ColorType::GrayscaleAlpha => data
            .as_chunks::<2>()
            .0
            .iter()
            .map(|p| RGBA::new(p[0], p[0], p[0], p[1]))
            .collect(),
        png::ColorType::Grayscale => data.iter().map(|&v| RGBA::new(v, v, v, 255)).collect(),
        png::ColorType::Indexed => {
            return Err(OptimiseError::Decode("palette was not expanded".into()));
        }
    };
    Ok((info.width, info.height, pixels))
}

/// The ancillary chunks of `input` to carry into the output.
fn kept_chunks(input: &[u8], strip_metadata: bool) -> Vec<([u8; 4], Vec<u8>)> {
    let mut kept = Vec::new();
    let mut pos = 8;
    while let Some(head) = input.get(pos..pos + 8) {
        let len = u32::from_be_bytes([head[0], head[1], head[2], head[3]]) as usize;
        let name: [u8; 4] = head[4..8].try_into().expect("slice is 4 bytes");
        let Some(data) = input.get(pos + 8..pos + 8 + len) else {
            break;
        };
        let rendering = RENDERING.contains(&name);
        let ancillary = name[0] & 0x20 != 0; // lower-case first letter
        if rendering || (!strip_metadata && ancillary && !REBUILT.contains(&name)) {
            kept.push((name, data.to_vec()));
        }
        pos += 12 + len;
    }
    kept
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::png::`
Expected: PASS, 5 tests.

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): PNG optimiser (imagequant + oxipng)

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::png::`

---

### Task 5: JPEG — mozjpeg, EXIF and ICC, truncation check

**Files:**
- Create: `crates/ferry-core/src/optimise/jpeg.rs`
- Modify: `crates/ferry-core/src/optimise/mod.rs` (add `mod jpeg;`)
- Modify: `crates/ferry-core/src/testutil.rs`

**Interfaces:**
- Consumes: `OptimiseOptions`, `OptimiseError`, `quality` (Task 3).
- Produces: `optimise::jpeg::optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>`. Output is progressive with optimised Huffman tables, keeps the ICC profile and pixel density, and handles EXIF per `strip_metadata` (minimal EXIF holding only a non-default `Orientation` when stripping; every APP1 copied when keeping). CMYK is `Unsupported`. A file whose markers never reach EOI is `Decode("the JPEG is incomplete")`.
- Produces (tests only): `testutil::{JpegExtras { exif, icc, dpi }, jpeg(w, h, quality, &JpegExtras), jpeg_gray(w, h, quality), exif_with_orientation(orientation: u16, padding: usize)}`.

- [ ] **Step 1: Add the JPEG test-image helpers**

Append to `crates/ferry-core/src/testutil.rs`:

```rust
/// Extra segments for `jpeg`.
#[derive(Default)]
pub struct JpegExtras {
    /// A complete APP1 payload (`Exif\0\0` + TIFF).
    pub exif: Option<Vec<u8>>,
    /// A complete APP2 payload (`ICC_PROFILE\0` + sequence bytes + profile).
    pub icc: Option<Vec<u8>>,
    pub dpi: Option<u16>,
}

/// A baseline RGB JPEG of `screenshot_rgba`.
pub fn jpeg(w: u32, h: u32, quality: u8, extras: &JpegExtras) -> Vec<u8> {
    let rgb: Vec<u8> = screenshot_rgba(w, h)
        .as_chunks::<4>()
        .0
        .iter()
        .flat_map(|p| [p[0], p[1], p[2]])
        .collect();
    let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_RGB);
    enc.set_size(w as usize, h as usize);
    enc.set_quality(f32::from(quality));
    if let Some(dpi) = extras.dpi {
        enc.set_pixel_density(mozjpeg::PixelDensity {
            unit: mozjpeg::PixelDensityUnit::Inches,
            x: dpi,
            y: dpi,
        });
    }
    let mut enc = enc.start_compress(Vec::new()).unwrap();
    if let Some(icc) = &extras.icc {
        enc.write_marker(mozjpeg::Marker::APP(2), icc);
    }
    if let Some(exif) = &extras.exif {
        enc.write_marker(mozjpeg::Marker::APP(1), exif);
    }
    enc.write_scanlines(&rgb).unwrap();
    enc.finish().unwrap()
}

/// A grayscale JPEG of the red channel of `screenshot_rgba`.
pub fn jpeg_gray(w: u32, h: u32, quality: u8) -> Vec<u8> {
    let gray: Vec<u8> = screenshot_rgba(w, h)
        .as_chunks::<4>()
        .0
        .iter()
        .map(|p| p[0])
        .collect();
    let mut enc = mozjpeg::Compress::new(mozjpeg::ColorSpace::JCS_GRAYSCALE);
    enc.set_size(w as usize, h as usize);
    enc.set_quality(f32::from(quality));
    let mut enc = enc.start_compress(Vec::new()).unwrap();
    enc.write_scanlines(&gray).unwrap();
    enc.finish().unwrap()
}

/// A little-endian EXIF APP1 payload: an Orientation entry plus `padding`
/// bytes of junk, standing in for a camera's large EXIF block.
pub fn exif_with_orientation(orientation: u16, padding: usize) -> Vec<u8> {
    let mut out = b"Exif\0\0II\x2A\0\x08\0\0\0".to_vec();
    out.extend_from_slice(&[1, 0]); // one entry
    out.extend_from_slice(&[0x12, 0x01, 3, 0, 1, 0, 0, 0]);
    out.extend_from_slice(&orientation.to_le_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    out.extend(std::iter::repeat_n(0xAB, padding));
    out
}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/ferry-core/src/optimise/jpeg.rs` containing only this test module, and add `mod jpeg;` to `optimise/mod.rs` after `mod png;`. The last four tests cover the truncation check (Review Focus 1): a file cut at three places is refused, bytes after the end marker are fine, an `FF D9` inside a segment (like an EXIF thumbnail) does not count as the end, and mozjpeg's own multi-scan output passes the check.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn markers(jpeg: &[u8]) -> Vec<(u8, Vec<u8>)> {
        let d = Decompress::with_markers(mozjpeg::ALL_MARKERS)
            .from_mem(jpeg)
            .unwrap();
        d.markers()
            .map(|m| {
                let n = match m.marker {
                    Marker::APP(n) => n,
                    Marker::COM => 255,
                };
                (n, m.data.to_vec())
            })
            .collect()
    }

    #[test]
    fn a_high_quality_jpeg_shrinks_and_keeps_its_size() {
        let input = testutil::jpeg(400, 300, 95, &testutil::JpegExtras::default());
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        let d = Decompress::new_mem(&out).unwrap();
        assert_eq!(d.size(), (400, 300));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let input = testutil::jpeg(400, 300, 95, &testutil::JpegExtras::default());
        let normal = optimise(&input, &OptimiseOptions::default()).unwrap();
        let aggressive = optimise(
            &input,
            &OptimiseOptions {
                level: super::super::Level::Aggressive,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        assert!(aggressive.len() <= normal.len());
    }

    #[test]
    fn stripping_exif_keeps_only_the_orientation() {
        let exif = testutil::exif_with_orientation(6, 4000);
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                exif: Some(exif),
                ..Default::default()
            },
        );
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        let app1: Vec<_> = markers(&out).into_iter().filter(|(n, _)| *n == 1).collect();
        assert_eq!(app1.len(), 1);
        assert_eq!(exif_orientation(&app1[0].1), Some(6));
        assert!(
            app1[0].1.len() < 40,
            "kept {} bytes of EXIF",
            app1[0].1.len()
        );
    }

    #[test]
    fn no_exif_is_written_for_the_default_orientation() {
        let exif = testutil::exif_with_orientation(1, 100);
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                exif: Some(exif),
                ..Default::default()
            },
        );
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(markers(&out).iter().all(|(n, _)| *n != 1));
    }

    #[test]
    fn exif_is_copied_untouched_when_metadata_is_kept() {
        let exif = testutil::exif_with_orientation(3, 500);
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                exif: Some(exif.clone()),
                ..Default::default()
            },
        );
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let out = optimise(&input, &o).unwrap();
        assert!(markers(&out).contains(&(1, exif)));
    }

    #[test]
    fn the_icc_profile_and_density_survive() {
        let icc = {
            let mut p = ICC_PREFIX.to_vec();
            p.extend_from_slice(&[1, 1]);
            p.extend_from_slice(&[7; 300]);
            p
        };
        let input = testutil::jpeg(
            200,
            100,
            95,
            &testutil::JpegExtras {
                icc: Some(icc.clone()),
                dpi: Some(144),
                ..Default::default()
            },
        );
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(markers(&out).contains(&(2, icc)));
        let mut d = Decompress::new_mem(&out).unwrap();
        let density = d.pixel_density().unwrap();
        assert_eq!((density.x, density.y), (144, 144));
    }

    #[test]
    fn a_grayscale_jpeg_stays_grayscale() {
        let input = testutil::jpeg_gray(200, 100, 95);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        let d = Decompress::new_mem(&out).unwrap();
        assert_eq!(d.color_space(), ColorSpace::JCS_GRAYSCALE);
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            optimise(
                &[0xFF, 0xD8, 0xFF, 0x00, 0x01, 0x02],
                &OptimiseOptions::default()
            ),
            Err(OptimiseError::Decode(_))
        ));
    }

    #[test]
    fn a_truncated_jpeg_is_refused_not_padded_with_grey() {
        let full = testutil::jpeg(300, 200, 90, &testutil::JpegExtras::default());
        for cut in [full.len() / 2, full.len() - 3, full.len() - 1] {
            let err = optimise(&full[..cut], &OptimiseOptions::default()).unwrap_err();
            assert!(
                matches!(&err, OptimiseError::Decode(m) if m.contains("incomplete")),
                "cut at {cut}: {err}"
            );
        }
    }

    #[test]
    fn bytes_after_the_end_marker_do_not_matter() {
        let mut full = testutil::jpeg(300, 200, 95, &testutil::JpegExtras::default());
        full.extend_from_slice(&[0; 64]);
        assert!(optimise(&full, &OptimiseOptions::default()).is_ok());
    }

    #[test]
    fn an_end_marker_inside_a_segment_does_not_count() {
        // SOI, then an APP1 whose payload contains FF D9 (like an EXIF thumbnail), then nothing.
        let truncated = [0xFF, 0xD8, 0xFF, 0xE1, 0x00, 0x06, 0xFF, 0xD9, 0x00, 0x00];
        assert!(!reaches_eoi(&truncated));
    }

    #[test]
    fn our_own_multi_scan_output_reaches_the_end_marker() {
        let full = testutil::jpeg(300, 200, 95, &testutil::JpegExtras::default());
        let out = optimise(&full, &OptimiseOptions::default()).unwrap();
        assert!(reaches_eoi(&out));
    }
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::jpeg::`
Expected: FAIL to compile, unresolved `optimise` / `reaches_eoi`.

- [ ] **Step 4: Implement**

Put this above the test module. mozjpeg reports corrupt data by unwinding, so the whole call sits in `catch_unwind`; `reaches_eoi` exists because libjpeg pads a truncated scan with grey and only warns.

```rust
//! JPEG: decode and re-encode with mozjpeg (progressive, optimised Huffman).
//! The ICC profile and pixel density always survive; EXIF follows the
//! metadata setting but never loses the orientation (spec §5.8).

use std::panic::{AssertUnwindSafe, catch_unwind};

use mozjpeg::{ColorSpace, Compress, Decompress, Marker};

use super::{OptimiseError, OptimiseOptions, quality};

const EXIF_PREFIX: &[u8] = b"Exif\0\0";
const ICC_PREFIX: &[u8] = b"ICC_PROFILE\0";

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    if !reaches_eoi(input) {
        return Err(OptimiseError::Decode("the JPEG is incomplete".into()));
    }
    // mozjpeg reports corrupt data by unwinding; turn that into an error.
    catch_unwind(AssertUnwindSafe(|| run(input, o)))
        .map_err(|_| OptimiseError::Decode("the JPEG is corrupt".into()))?
}

/// Whether the marker structure reaches the end-of-image marker. libjpeg pads a
/// truncated picture with grey and only warns, so without this check a
/// half-downloaded file would be "optimised" into a damaged one.
fn reaches_eoi(data: &[u8]) -> bool {
    let mut pos = 2; // past SOI
    loop {
        let (Some(&0xFF), Some(&marker)) = (data.get(pos), data.get(pos + 1)) else {
            return false;
        };
        match marker {
            0xD9 => return true,
            0xFF => pos += 1,                      // fill byte
            0x00 | 0x01 | 0xD0..=0xD8 => pos += 2, // markers without a length
            _ => {
                let Some(len) = data.get(pos + 2..pos + 4) else {
                    return false;
                };
                pos += 2 + usize::from(u16::from_be_bytes([len[0], len[1]]));
                if marker == 0xDA {
                    // The entropy-coded scan runs to the next marker that is not
                    // byte stuffing (FF00) or a restart (FFD0..FFD7).
                    let rest = data.get(pos..).unwrap_or_default();
                    let Some(end) = rest.windows(2).position(|w| {
                        w[0] == 0xFF && w[1] != 0x00 && !(0xD0..=0xD7).contains(&w[1])
                    }) else {
                        return false;
                    };
                    pos += end;
                }
            }
        }
    }
}

fn run(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let decode_err = |e: std::io::Error| OptimiseError::Decode(e.to_string());
    let encode_err = |e: std::io::Error| OptimiseError::Encode(e.to_string());

    let mut decoder = Decompress::with_markers(&[Marker::APP(1), Marker::APP(2)])
        .from_mem(input)
        .map_err(decode_err)?;
    let (width, height) = decoder.size();
    let density = decoder.pixel_density();
    let mut app1 = Vec::new();
    let mut icc = Vec::new();
    for m in decoder.markers() {
        match m.marker {
            Marker::APP(1) => app1.push(m.data.to_vec()),
            Marker::APP(2) if m.data.starts_with(ICC_PREFIX) => icc.push(m.data.to_vec()),
            _ => {}
        }
    }

    let (pixels, space) = match decoder.color_space() {
        ColorSpace::JCS_GRAYSCALE => (
            decoder
                .grayscale()
                .and_then(|mut d| d.read_scanlines::<u8>())
                .map_err(decode_err)?,
            ColorSpace::JCS_GRAYSCALE,
        ),
        ColorSpace::JCS_CMYK | ColorSpace::JCS_YCCK => {
            return Err(OptimiseError::Unsupported("CMYK JPEG".into()));
        }
        _ => (
            decoder
                .rgb()
                .and_then(|mut d| d.read_scanlines::<u8>())
                .map_err(decode_err)?,
            ColorSpace::JCS_RGB,
        ),
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }

    let mut encoder = Compress::new(space);
    encoder.set_size(width, height);
    encoder.set_quality(f32::from(quality(o)));
    encoder.set_progressive_mode();
    encoder.set_optimize_coding(true);
    if let Some(density) = density {
        encoder.set_pixel_density(density);
    }
    let mut encoder = encoder.start_compress(Vec::new()).map_err(encode_err)?;
    for segment in &icc {
        encoder.write_marker(Marker::APP(2), segment);
    }
    for segment in exif_segments(&app1, o.strip_metadata) {
        encoder.write_marker(Marker::APP(1), &segment);
    }
    encoder.write_scanlines(&pixels).map_err(encode_err)?;
    encoder.finish().map_err(encode_err)
}

/// The APP1 segments to write: all of them when keeping metadata, otherwise a
/// minimal EXIF that holds only a non-default orientation.
fn exif_segments(original: &[Vec<u8>], strip_metadata: bool) -> Vec<Vec<u8>> {
    if !strip_metadata {
        return original.to_vec();
    }
    original
        .iter()
        .find_map(|segment| exif_orientation(segment))
        .filter(|&orientation| orientation != 1)
        .map(|orientation| vec![minimal_exif(orientation)])
        .unwrap_or_default()
}

/// The Orientation tag (0x0112) of an EXIF APP1 segment, if it has one.
fn exif_orientation(segment: &[u8]) -> Option<u16> {
    let tiff = segment.strip_prefix(EXIF_PREFIX)?;
    let big_endian = match tiff.get(..2)? {
        b"MM" => true,
        b"II" => false,
        _ => return None,
    };
    let u16_at = |at: usize| -> Option<u16> {
        let b: [u8; 2] = tiff.get(at..at + 2)?.try_into().ok()?;
        Some(if big_endian {
            u16::from_be_bytes(b)
        } else {
            u16::from_le_bytes(b)
        })
    };
    let u32_at = |at: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(at..at + 4)?.try_into().ok()?;
        Some(if big_endian {
            u32::from_be_bytes(b)
        } else {
            u32::from_le_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    (0..usize::from(u16_at(ifd)?))
        .map(|i| ifd + 2 + i * 12)
        .find(|&entry| u16_at(entry) == Some(0x0112))
        .and_then(|entry| u16_at(entry + 8)) // a SHORT value sits in the first two value bytes
}

/// An EXIF APP1 payload with a single Orientation entry (big-endian TIFF).
fn minimal_exif(orientation: u16) -> Vec<u8> {
    let mut out = EXIF_PREFIX.to_vec();
    out.extend_from_slice(b"MM\0\x2A\0\0\0\x08"); // byte order, magic, IFD0 at offset 8
    out.extend_from_slice(&[0, 1]); // one entry
    out.extend_from_slice(&[0x01, 0x12, 0, 3, 0, 0, 0, 1]); // Orientation, SHORT, count 1
    out.extend_from_slice(&orientation.to_be_bytes());
    out.extend_from_slice(&[0, 0, 0, 0, 0, 0]); // value padding, no next IFD
    out
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::jpeg::`
Expected: PASS, 12 tests.

- [ ] **Step 6: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): JPEG optimiser with EXIF/ICC handling and truncation check

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::jpeg::`

---

### Task 6: WebP — libwebp lossy with ICC, EXIF and XMP kept or dropped

**Files:**
- Create: `crates/ferry-core/src/optimise/webp.rs`
- Modify: `crates/ferry-core/src/optimise/mod.rs` (add `pub(crate) mod webp;`)
- Modify: `crates/ferry-core/src/testutil.rs`

**Interfaces:**
- Consumes: `OptimiseOptions`, `OptimiseError`, `quality` (Task 3).
- Produces: `optimise::webp::optimise(input, o) -> Result<Vec<u8>, OptimiseError>` (lossy, method 4, alpha quality 100; ICC always kept, EXIF and XMP only when `strip_metadata == false`), and `optimise::webp::Extras { icc, exif, xmp }` with `Default`, used by `testutil::webp`. The `webp` crate cannot write ICCP/EXIF/XMP, so `assemble` builds the RIFF container by hand (`VP8X` + `ICCP` + image chunks + `EXIF` + `XMP `).
- Produces (tests only): `testutil::webp(w, h, quality, &Extras) -> Vec<u8>` and `testutil::webp_with_transparent_corner(w, h) -> Vec<u8>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/optimise/webp.rs` containing only this test module, and add `pub(crate) mod webp;` to `optimise/mod.rs` after `mod png;` / `mod jpeg;` (it is `pub(crate)` because `testutil` builds `Extras`).

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil;

    fn names(webp: &[u8]) -> Vec<String> {
        chunks(webp)
            .iter()
            .map(|(n, _)| String::from_utf8_lossy(n).into_owned())
            .collect()
    }

    #[test]
    fn a_high_quality_webp_shrinks_and_keeps_its_size() {
        let input = testutil::webp(400, 300, 95, &Extras::default());
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        let decoded = Decoder::new(&out).decode().unwrap();
        assert_eq!((decoded.width(), decoded.height()), (400, 300));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let input = testutil::webp(400, 300, 95, &Extras::default());
        let normal = optimise(&input, &OptimiseOptions::default()).unwrap();
        let aggressive = optimise(
            &input,
            &OptimiseOptions {
                level: super::super::Level::Aggressive,
                ..OptimiseOptions::default()
            },
        )
        .unwrap();
        assert!(aggressive.len() <= normal.len());
    }

    #[test]
    fn transparency_survives() {
        let input = testutil::webp_with_transparent_corner(64, 64);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        let decoded = Decoder::new(&out).decode().unwrap();
        assert!(decoded.is_alpha());
        assert_eq!(decoded[3], 0, "top-left pixel stays transparent");
        assert_eq!(decoded[(32 * 64 + 32) * 4 + 3], 255, "centre stays opaque");
    }

    #[test]
    fn the_icc_profile_survives_and_exif_is_dropped_when_stripping() {
        let extras = Extras {
            icc: Some(vec![9; 101]),
            exif: Some(b"Exif\0\0II*\0".to_vec()),
            xmp: Some(b"<x:xmpmeta/>".to_vec()),
        };
        let input = testutil::webp(128, 96, 95, &extras);
        assert_eq!(names(&input), ["VP8X", "ICCP", "VP8 ", "EXIF", "XMP "]);
        let out = optimise(&input, &OptimiseOptions::default()).unwrap();
        assert_eq!(names(&out), ["VP8X", "ICCP", "VP8 "]);
        let icc = chunks(&out)
            .into_iter()
            .find(|(n, _)| n == b"ICCP")
            .unwrap()
            .1;
        assert_eq!(icc, &[9; 101][..]);
        assert!(Decoder::new(&out).decode().is_some(), "still decodes");
    }

    #[test]
    fn exif_and_xmp_are_kept_when_metadata_is_kept() {
        let extras = Extras {
            exif: Some(b"Exif\0\0II*\0".to_vec()),
            xmp: Some(b"<x:xmpmeta/>".to_vec()),
            ..Extras::default()
        };
        let input = testutil::webp(128, 96, 95, &extras);
        let o = OptimiseOptions {
            strip_metadata: false,
            ..OptimiseOptions::default()
        };
        let out = optimise(&input, &o).unwrap();
        assert_eq!(names(&out), ["VP8X", "VP8 ", "EXIF", "XMP "]);
    }

    #[test]
    fn garbage_is_a_decode_error() {
        assert!(matches!(
            optimise(b"RIFF\x04\0\0\0WEBP", &OptimiseOptions::default()),
            Err(OptimiseError::Decode(_))
        ));
    }
}
```

- [ ] **Step 2: Add the WebP test-image helpers**

Append to `crates/ferry-core/src/testutil.rs`:

```rust
/// A lossy WebP of `screenshot_rgba`, wrapped with `extras` when given.
pub fn webp(w: u32, h: u32, quality: u8, extras: &crate::optimise::webp::Extras) -> Vec<u8> {
    let rgba = screenshot_rgba(w, h);
    let encoded = webp::Encoder::from_rgba(&rgba, w, h).encode(f32::from(quality));
    crate::optimise::webp::assemble(&encoded, w, h, false, extras)
}

/// A lossy WebP whose top-left quadrant is fully transparent.
pub fn webp_with_transparent_corner(w: u32, h: u32) -> Vec<u8> {
    let mut rgba = screenshot_rgba(w, h);
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            rgba[((y * w + x) * 4 + 3) as usize] = 0;
        }
    }
    webp::Encoder::from_rgba(&rgba, w, h).encode(95.0).to_vec()
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::webp::`
Expected: FAIL to compile, unresolved `Extras` / `optimise`.

- [ ] **Step 4: Implement**

Put this above the test module:

```rust
//! WebP (still images): decode and re-encode lossy with libwebp, method 4,
//! alpha kept. The ICC profile always survives; EXIF and XMP follow the
//! metadata setting (spec §5.8). Animations never reach this module.

use webp::{Decoder, Encoder, WebPConfig};

use super::{OptimiseError, OptimiseOptions, quality};

const ICC_FLAG: u8 = 0x20;
const ALPHA_FLAG: u8 = 0x10;
const EXIF_FLAG: u8 = 0x08;
const XMP_FLAG: u8 = 0x04;

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let image = Decoder::new(input)
        .decode()
        .ok_or_else(|| OptimiseError::Decode("not a still WebP".into()))?;
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }

    let mut config =
        WebPConfig::new().map_err(|()| OptimiseError::Encode("libwebp config".into()))?;
    config.lossless = 0;
    config.quality = f32::from(quality(o));
    config.method = 4;
    config.alpha_compression = 1;
    config.alpha_quality = 100;
    let encoded = Encoder::new(&image, image.layout(), image.width(), image.height())
        .encode_advanced(&config)
        .map_err(|e| OptimiseError::Encode(format!("{e:?}")))?;

    let original = chunks(input);
    let find = |name: &[u8; 4]| {
        original
            .iter()
            .find(|(n, _)| n == name)
            .map(|(_, data)| data.to_vec())
    };
    let keep_metadata = !o.strip_metadata;
    let extras = Extras {
        icc: find(b"ICCP"),
        exif: find(b"EXIF").filter(|_| keep_metadata),
        xmp: find(b"XMP ").filter(|_| keep_metadata),
    };
    Ok(assemble(
        &encoded,
        image.width(),
        image.height(),
        image.is_alpha(),
        &extras,
    ))
}

#[derive(Default)]
pub(crate) struct Extras {
    pub icc: Option<Vec<u8>>,
    pub exif: Option<Vec<u8>>,
    pub xmp: Option<Vec<u8>>,
}

/// The chunks of a RIFF/WebP file as `(fourcc, payload)`; empty for anything else.
pub(crate) fn chunks(bytes: &[u8]) -> Vec<([u8; 4], &[u8])> {
    let mut out = Vec::new();
    if bytes.len() < 12 || &bytes[0..4] != b"RIFF" || &bytes[8..12] != b"WEBP" {
        return out;
    }
    let mut pos = 12;
    while let Some(head) = bytes.get(pos..pos + 8) {
        let name: [u8; 4] = head[0..4].try_into().expect("slice is 4 bytes");
        let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]) as usize;
        let Some(data) = bytes.get(pos + 8..pos + 8 + len) else {
            break;
        };
        out.push((name, data));
        pos += 8 + len + (len & 1); // chunks are padded to even length
    }
    out
}

/// Wrap libwebp's output in a container that carries `extras`. Without any
/// extras the encoder's own output is returned unchanged.
pub(crate) fn assemble(
    encoded: &[u8],
    width: u32,
    height: u32,
    alpha: bool,
    extras: &Extras,
) -> Vec<u8> {
    if extras.icc.is_none() && extras.exif.is_none() && extras.xmp.is_none() {
        return encoded.to_vec();
    }
    let mut flags = 0;
    if extras.icc.is_some() {
        flags |= ICC_FLAG;
    }
    if alpha {
        flags |= ALPHA_FLAG;
    }
    if extras.exif.is_some() {
        flags |= EXIF_FLAG;
    }
    if extras.xmp.is_some() {
        flags |= XMP_FLAG;
    }
    let mut vp8x = vec![flags, 0, 0, 0];
    vp8x.extend_from_slice(&(width - 1).to_le_bytes()[..3]);
    vp8x.extend_from_slice(&(height - 1).to_le_bytes()[..3]);

    let mut body = b"WEBP".to_vec();
    push_chunk(&mut body, b"VP8X", &vp8x);
    if let Some(icc) = &extras.icc {
        push_chunk(&mut body, b"ICCP", icc);
    }
    for (name, data) in chunks(encoded) {
        if &name != b"VP8X" {
            push_chunk(&mut body, &name, data); // ALPH, VP8, VP8L
        }
    }
    if let Some(exif) = &extras.exif {
        push_chunk(&mut body, b"EXIF", exif);
    }
    if let Some(xmp) = &extras.xmp {
        push_chunk(&mut body, b"XMP ", xmp);
    }
    let mut out = b"RIFF".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

fn push_chunk(out: &mut Vec<u8>, name: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(name);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::webp::`
Expected: PASS, 6 tests.

- [ ] **Step 6: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): WebP optimiser with RIFF metadata assembly

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::webp::`

---

### Task 7: Sidecar runner

**Files:**
- Create: `crates/ferry-core/src/sidecar.rs`
- Create: `crates/ferry-core/tests/sidecar.rs`
- Create: `crates/ferry-core/tests/support/sidecar_fixture.rs`
- Modify: `crates/ferry-core/Cargo.toml` (the fixture binary)
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `cancel::CancelToken` (Task 1).
- Produces: `sidecar::run(path: &Path, args: &[&str], stdin: &[u8], timeout: Duration, cancel: &CancelToken) -> Result<Vec<u8>, SidecarError>`; `sidecar::DEFAULT_TIMEOUT` (60 s); `SidecarError::{Spawn(io::Error), Timeout, Cancelled, Failed { code: Option<i32>, stderr: String }, Io(io::Error)}` with `Display`.

The tests drive a small helper binary instead of gifsicle, so they behave the same on every OS and need no sidecar: modes `echo`, `noisy` (8 MB on stderr before stdin is read), `fail` (exit 3), `sleep-then-touch`.

- [ ] **Step 1: Write the fixture and the failing tests**

Create `crates/ferry-core/tests/support/sidecar_fixture.rs`:

```rust
//! Test helper for tests/sidecar.rs: a child process with scripted behaviour,
//! so the runner is tested the same way on every OS.

use std::{
    env, fs,
    io::{self, Read, Write},
    thread,
    time::Duration,
};

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        // Copy stdin to stdout.
        Some("echo") => {
            let mut input = Vec::new();
            io::stdin().read_to_end(&mut input).unwrap();
            io::stdout().write_all(&input).unwrap();
        }
        // Write 8 MB to stderr while stdin is still unread, then answer "done".
        Some("noisy") => {
            let noise = [b'x'; 64 * 1024];
            let mut stderr = io::stderr();
            for _ in 0..128 {
                stderr.write_all(&noise).unwrap();
            }
            io::stdin().read_to_end(&mut Vec::new()).unwrap();
            print!("done");
        }
        // Print a complaint and exit 3.
        Some("fail") => {
            eprintln!("boom");
            std::process::exit(3);
        }
        // Sleep, then create the file in args[2]; being killed first means no file.
        Some("sleep-then-touch") => {
            thread::sleep(Duration::from_millis(1500));
            fs::write(&args[1], b"alive").unwrap();
        }
        other => panic!("unknown mode {other:?}"),
    }
}
```

Create `crates/ferry-core/tests/sidecar.rs`:

```rust
use std::{
    path::PathBuf,
    thread,
    time::{Duration, Instant},
};

use ferry_core::{
    cancel::CancelToken,
    sidecar::{self, SidecarError},
};

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_sidecar-fixture"))
}

fn run(args: &[&str], stdin: &[u8], timeout: Duration) -> Result<Vec<u8>, SidecarError> {
    sidecar::run(&fixture(), args, stdin, timeout, &CancelToken::new())
}

#[test]
fn bytes_round_trip_through_the_child() {
    let out = run(&["echo"], b"hello sidecar", Duration::from_secs(10)).unwrap();
    assert_eq!(out, b"hello sidecar");
}

#[test]
fn a_large_input_and_output_do_not_deadlock() {
    let input = vec![7_u8; 8 * 1024 * 1024];
    let out = run(&["echo"], &input, Duration::from_secs(30)).unwrap();
    assert_eq!(out.len(), input.len());
}

#[test]
fn large_stderr_is_drained_while_stdin_is_still_unread() {
    let out = run(&["noisy"], &[1; 1024], Duration::from_secs(30)).unwrap();
    assert_eq!(out, b"done");
}

#[test]
fn a_failing_child_reports_its_code_and_stderr() {
    match run(&["fail"], b"", Duration::from_secs(10)) {
        Err(SidecarError::Failed { code, stderr }) => {
            assert_eq!(code, Some(3));
            assert!(stderr.contains("boom"), "{stderr:?}");
        }
        other => panic!("expected Failed, got {other:?}"),
    }
}

#[test]
fn a_missing_program_is_a_spawn_error() {
    let missing = PathBuf::from("definitely-not-a-ferry-sidecar");
    let result = sidecar::run(
        &missing,
        &[],
        b"",
        Duration::from_secs(1),
        &CancelToken::new(),
    );
    assert!(matches!(result, Err(SidecarError::Spawn(_))));
}

#[test]
fn a_timeout_kills_the_child() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("alive.txt");
    let started = Instant::now();
    let result = run(
        &["sleep-then-touch", marker.to_str().unwrap()],
        b"",
        Duration::from_millis(300),
    );
    assert!(matches!(result, Err(SidecarError::Timeout)), "{result:?}");
    assert!(started.elapsed() < Duration::from_millis(1200));
    thread::sleep(Duration::from_millis(2000)); // longer than the child's own sleep
    assert!(!marker.exists(), "the child kept running after the timeout");
}

#[test]
fn cancelling_kills_the_child_promptly() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("alive.txt");
    let cancel = CancelToken::new();
    let canceller = cancel.clone();
    thread::spawn(move || {
        thread::sleep(Duration::from_millis(200));
        canceller.cancel();
    });
    let started = Instant::now();
    let result = sidecar::run(
        &fixture(),
        &["sleep-then-touch", marker.to_str().unwrap()],
        b"",
        Duration::from_secs(30),
        &cancel,
    );
    assert!(matches!(result, Err(SidecarError::Cancelled)), "{result:?}");
    assert!(started.elapsed() < Duration::from_millis(1200));
    thread::sleep(Duration::from_millis(2000));
    assert!(!marker.exists(), "the child kept running after the cancel");
}
```

Add to `crates/ferry-core/Cargo.toml`, after `[package]` and before `[dependencies]`:

```toml
[[bin]]
name = "sidecar-fixture"
path = "tests/support/sidecar_fixture.rs"
test = false
```

Add `pub mod sidecar;` to `lib.rs` (after `optimise`, before `sniff`).

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --test sidecar`
Expected: FAIL to compile, `unresolved import ferry_core::sidecar`.

- [ ] **Step 3: Implement**

Create `crates/ferry-core/src/sidecar.rs`. It writes stdin and drains stdout and stderr on their own threads inside `thread::scope`, polls `try_wait` every 5 ms for the timeout and the cancel flag, and a `KillOnDrop` guard kills the child however the function exits.

```rust
//! Run a bundled helper program (gifsicle, later ffmpeg) with bytes on stdin
//! and bytes back on stdout. The child can never block: stdin is written and
//! stdout/stderr are drained on their own threads. It is killed on timeout, on
//! cancel, and if this function unwinds.

use std::{
    fmt,
    io::{self, Read, Write},
    path::Path,
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::cancel::CancelToken;

/// How long a sidecar may run unless the caller says otherwise.
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(60);

#[derive(Debug)]
pub enum SidecarError {
    /// The program could not be started (missing, not executable).
    Spawn(io::Error),
    Timeout,
    Cancelled,
    /// It ran and exited non-zero; `stderr` is what it printed.
    Failed {
        code: Option<i32>,
        stderr: String,
    },
    Io(io::Error),
}

impl fmt::Display for SidecarError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Spawn(e) => write!(f, "could not start the helper: {e}"),
            Self::Timeout => write!(f, "the helper timed out"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Failed { code, stderr } => match code {
                Some(code) => write!(f, "the helper exited with {code}: {}", stderr.trim()),
                None => write!(f, "the helper was stopped: {}", stderr.trim()),
            },
            Self::Io(e) => write!(f, "helper I/O failed: {e}"),
        }
    }
}

impl std::error::Error for SidecarError {}

struct KillOnDrop(Child);

impl Drop for KillOnDrop {
    fn drop(&mut self) {
        // Harmless when the child already exited.
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

pub fn run(
    path: &Path,
    args: &[&str],
    stdin: &[u8],
    timeout: Duration,
    cancel: &CancelToken,
) -> Result<Vec<u8>, SidecarError> {
    let mut child = Command::new(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(SidecarError::Spawn)?;
    let mut to_child = child.stdin.take().expect("stdin is piped");
    let mut from_child = child.stdout.take().expect("stdout is piped");
    let mut errors = child.stderr.take().expect("stderr is piped");
    let mut child = KillOnDrop(child);

    thread::scope(|scope| {
        scope.spawn(move || {
            // The child may exit before reading everything; that is its call.
            let _ = to_child.write_all(stdin);
        });
        let stdout = scope.spawn(move || {
            let mut bytes = Vec::new();
            let _ = from_child.read_to_end(&mut bytes);
            bytes
        });
        let stderr = scope.spawn(move || {
            let mut bytes = Vec::new();
            let _ = errors.read_to_end(&mut bytes);
            String::from_utf8_lossy(&bytes).into_owned()
        });

        let deadline = Instant::now() + timeout;
        let status = loop {
            match child.0.try_wait() {
                Ok(Some(status)) => break Ok(status),
                Ok(None) => {}
                Err(e) => break Err(SidecarError::Io(e)),
            }
            if cancel.is_cancelled() {
                break Err(SidecarError::Cancelled);
            }
            if Instant::now() >= deadline {
                break Err(SidecarError::Timeout);
            }
            thread::sleep(Duration::from_millis(5));
        };
        if status.is_err() {
            // Closes the pipes, which lets the reader threads finish.
            let _ = child.0.kill();
            let _ = child.0.wait();
        }

        let stdout = stdout.join().expect("stdout reader does not panic");
        let stderr = stderr.join().expect("stderr reader does not panic");
        let status = status?;
        if status.success() {
            Ok(stdout)
        } else {
            Err(SidecarError::Failed {
                code: status.code(),
                stderr,
            })
        }
    })
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --test sidecar`
Expected: PASS, 7 tests (round trip, 8 MB without deadlock, noisy stderr, failure code and stderr, missing program, timeout kills the child, cancel kills the child).

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): sidecar runner with timeout, cancel and drained output

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --test sidecar`

---

### Task 8: GIF — gifsicle through the sidecar runner

**Files:**
- Create: `crates/ferry-core/src/optimise/gif.rs`
- Modify: `crates/ferry-core/src/optimise/mod.rs` (add `mod gif;`)
- Modify: `crates/ferry-core/src/testutil.rs`

**Interfaces:**
- Consumes: `sidecar::{run, DEFAULT_TIMEOUT, SidecarError}` (Task 7); `OptimiseOptions.gifsicle` (Task 3); `sniff::gif_frame_count` (Task 2); `testutil::gif` (Task 2).
- Produces: `optimise::gif::optimise(input, o) -> Result<Vec<u8>, OptimiseError>`. No `gifsicle` path gives `Unsupported("gifsicle missing — reinstall Ferry")`; a cancelled run gives `Cancelled`; other sidecar failures give `Encode`. Args are `-O3 --lossy=L --colors 256`, plus `--no-comments --no-extensions` when `strip_metadata` (Decision 2).
- Produces (tests only): `testutil::gifsicle() -> Option<PathBuf>` (the `FERRY_GIFSICLE` env var, when the file exists) and `testutil::with_gif_comment(gif, text) -> Vec<u8>`.

GIF tests that need the real binary return early when `FERRY_GIFSICLE` is unset, except `ci_always_has_gifsicle`, which fails when `CI` is set without it, so CI can never pass by skipping.

- [ ] **Step 1: Add the GIF test helpers**

Append to `crates/ferry-core/src/testutil.rs`:

```rust
/// `FERRY_GIFSICLE` (the sidecar's path), when it is set and exists. GIF tests
/// skip without it; CI sets it and fails if it is missing.
pub fn gifsicle() -> Option<std::path::PathBuf> {
    std::env::var_os("FERRY_GIFSICLE")
        .map(std::path::PathBuf::from)
        .filter(|path| path.exists())
}

/// Insert a comment extension right after the global colour table.
pub fn with_gif_comment(gif: &[u8], text: &str) -> Vec<u8> {
    let at = 13 + 768; // header + screen descriptor + 256-colour table
    let mut out = gif[..at].to_vec();
    out.extend_from_slice(&[0x21, 0xFE, text.len() as u8]);
    out.extend_from_slice(text.as_bytes());
    out.push(0);
    out.extend_from_slice(&gif[at..]);
    out
}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/ferry-core/src/optimise/gif.rs` containing only this test module, and add `mod gif;` to `optimise/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{optimise::Level, sniff::gif_frame_count, testutil};

    #[test]
    fn lossy_strength_follows_quality() {
        assert_eq!(lossy(100), 0);
        assert_eq!(lossy(80), 30);
        assert_eq!(lossy(60), 60);
        assert_eq!(lossy(5), 143);
    }

    #[test]
    fn without_gifsicle_it_is_unsupported() {
        assert!(matches!(
            optimise(&testutil::gif(1), &OptimiseOptions::default()),
            Err(OptimiseError::Unsupported(_))
        ));
    }

    fn with_gifsicle(o: OptimiseOptions) -> Option<OptimiseOptions> {
        testutil::gifsicle().map(|path| OptimiseOptions {
            gifsicle: Some(path),
            ..o
        })
    }

    #[test]
    fn ci_always_has_gifsicle() {
        if std::env::var_os("CI").is_some() {
            assert!(
                testutil::gifsicle().is_some(),
                "FERRY_GIFSICLE must be set in CI"
            );
        }
    }

    #[test]
    fn a_static_gif_shrinks_and_stays_a_gif() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let input = testutil::gif(1);
        let out = optimise(&input, &o).unwrap();
        assert!(
            out.len() * 100 <= input.len() * 99,
            "{} -> {}",
            input.len(),
            out.len()
        );
        assert_eq!(crate::sniff::sniff(&out), Some(crate::sniff::Format::Gif));
        assert_eq!(gif_frame_count(&out), 1);
    }

    #[test]
    fn an_animated_gif_keeps_every_frame_and_its_loop() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let out = optimise(&testutil::gif(3), &o).unwrap();
        assert_eq!(gif_frame_count(&out), 3);
        assert!(
            out.windows(11).any(|w| w == b"NETSCAPE2.0"),
            "loop extension was lost"
        );
    }

    #[test]
    fn comments_follow_the_metadata_setting() {
        let input = testutil::with_gif_comment(&testutil::gif(1), "hello");
        let contains_comment = |gif: &[u8]| gif.windows(5).any(|w| w == b"hello");
        let Some(strip) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        assert!(!contains_comment(&optimise(&input, &strip).unwrap()));
        let keep = OptimiseOptions {
            strip_metadata: false,
            ..strip
        };
        assert!(contains_comment(&optimise(&input, &keep).unwrap()));
    }

    #[test]
    fn aggressive_is_no_bigger_than_normal() {
        let Some(normal) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        let input = testutil::gif(2);
        let aggressive = OptimiseOptions {
            level: Level::Aggressive,
            ..normal.clone()
        };
        assert!(
            optimise(&input, &aggressive).unwrap().len()
                <= optimise(&input, &normal).unwrap().len()
        );
    }

    #[test]
    fn garbage_is_an_encode_error() {
        let Some(o) = with_gifsicle(OptimiseOptions::default()) else {
            return;
        };
        assert!(matches!(
            optimise(b"GIF89a-but-not-really", &o),
            Err(OptimiseError::Encode(_))
        ));
    }
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo test -p ferry-core --lib -- optimise::gif::`
Expected: FAIL to compile, unresolved `lossy` / `optimise`.

- [ ] **Step 4: Implement**

Put this above the test module:

```rust
//! GIF: run the bundled gifsicle (stdin → stdout). gifsicle keeps every frame,
//! so animated GIFs stay animated with the same frame count.

use super::{OptimiseError, OptimiseOptions, quality};
use crate::sidecar::{self, DEFAULT_TIMEOUT, SidecarError};

/// gifsicle `--lossy` strength for a quality: 0 at q100, 30 at q80, 90 at q40.
fn lossy(q: u8) -> u32 {
    (f32::from(100 - q.min(100)) * 1.5).round() as u32
}

pub fn optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError> {
    let Some(gifsicle) = &o.gifsicle else {
        return Err(OptimiseError::Unsupported(
            "gifsicle missing — reinstall Ferry".into(),
        ));
    };
    let lossy = format!("--lossy={}", lossy(quality(o)));
    let mut args = vec!["-O3", lossy.as_str(), "--colors", "256"];
    if o.strip_metadata {
        // gifsicle keeps comments and unknown extensions by default; the loop
        // count survives both flags.
        args.extend(["--no-comments", "--no-extensions"]);
    }
    sidecar::run(gifsicle, &args, input, DEFAULT_TIMEOUT, &o.cancel).map_err(|e| match e {
        SidecarError::Cancelled => OptimiseError::Cancelled,
        SidecarError::Spawn(e) => {
            OptimiseError::Unsupported(format!("gifsicle could not start: {e}"))
        }
        other => OptimiseError::Encode(other.to_string()),
    })
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo test -p ferry-core --lib -- optimise::gif::`
Expected: PASS, 8 tests, including the ones that run gifsicle (check that `an_animated_gif_keeps_every_frame_and_its_loop` is listed `ok`, not skipped; a skipped test still prints `ok`, so also confirm `echo $FERRY_GIFSICLE` is set and the file exists).

Run the same command with the variable unset: `env -u FERRY_GIFSICLE cargo test -p ferry-core --lib -- optimise::gif::`
Expected: PASS (binary-dependent tests return early).

- [ ] **Step 6: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): GIF optimiser through the gifsicle sidecar

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo test -p ferry-core --lib -- optimise::gif::`

---

### Task 9: `optimise()` — dispatch, cancel and the integration tests

**Files:**
- Modify: `crates/ferry-core/src/optimise/mod.rs`

**Interfaces:**
- Consumes: the four per-format `optimise` functions (Tasks 4, 5, 6, 8), `sniff::{Format, is_animated}` (Task 2), `classify` (Task 3).
- Produces: `optimise::optimise(input: &[u8], format: Format, o: &OptimiseOptions) -> Result<Outcome, OptimiseError>`. Checks the cancel token before and after the work; any animated non-GIF (APNG, animated WebP) returns `AlreadyOptimal` untouched (GIF animation is handled by gifsicle, which keeps every frame); then encodes and grades with `classify`.

- [ ] **Step 1: Write the failing tests**

In `optimise/mod.rs`, add `use crate::testutil;` under `use super::*;` in `mod tests`, then append these tests inside it. The last one is `#[ignore]`d: it times the spec's NFR-P2 target (a 5 MB PNG in under 1.5 s), which depends on the machine, so it runs on demand with `--release`.

```rust
    #[test]
    fn a_cancelled_token_stops_before_any_work() {
        let o = OptimiseOptions::default();
        o.cancel.cancel();
        assert!(matches!(
            optimise(b"not even an image", Format::Png, &o),
            Err(OptimiseError::Cancelled)
        ));
    }

    #[test]
    fn a_one_pixel_png_comes_back_larger_so_the_original_is_kept() {
        let tiny = testutil::png(1, 1);
        assert_eq!(
            optimise(&tiny, Format::Png, &OptimiseOptions::default()).unwrap(),
            Outcome::Larger
        );
    }

    #[test]
    fn re_optimising_a_webp_is_already_optimal() {
        let o = OptimiseOptions::default();
        let source = testutil::webp(400, 300, 95, &webp::Extras::default());
        let Outcome::Smaller { bytes: once } = optimise(&source, Format::WebP, &o).unwrap() else {
            panic!("the first pass should shrink a quality-95 WebP");
        };
        assert_eq!(
            optimise(&once, Format::WebP, &o).unwrap(),
            Outcome::AlreadyOptimal
        );
    }

    #[test]
    fn animated_webp_and_apng_are_left_alone() {
        let o = OptimiseOptions::default();
        assert_eq!(
            optimise(&testutil::animated_webp(), Format::WebP, &o).unwrap(),
            Outcome::AlreadyOptimal
        );
        // acTL (frames = 2, plays = 0) marks a PNG as animated.
        let apng =
            testutil::with_png_chunk(&testutil::png(64, 64), b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0]);
        assert_eq!(
            optimise(&apng, Format::Png, &o).unwrap(),
            Outcome::AlreadyOptimal
        );
    }

    #[test]
    fn broken_input_is_an_error_not_a_panic() {
        let o = OptimiseOptions::default();
        let png = testutil::png(200, 200);
        let jpeg = testutil::jpeg(200, 200, 90, &testutil::JpegExtras::default());
        let webp = testutil::webp(200, 200, 90, &webp::Extras::default());
        for (format, valid) in [
            (Format::Png, png),
            (Format::Jpeg, jpeg),
            (Format::WebP, webp),
        ] {
            for broken in [
                valid[..valid.len() / 2].to_vec(),
                b"definitely not an image".to_vec(),
                Vec::new(),
            ] {
                assert!(
                    matches!(optimise(&broken, format, &o), Err(OptimiseError::Decode(_))),
                    "{format:?} with {} bytes",
                    broken.len()
                );
            }
        }
    }

    /// NFR-P2: a 5 MB clipboard image in under 1.5 s. Timing depends on the
    /// machine, so run it on purpose: `cargo test --release -- --ignored five_megabyte`.
    #[test]
    #[ignore = "timing; run with --release --ignored"]
    fn five_megabyte_png_is_optimised_within_the_clipboard_budget() {
        let input = testutil::png(4096, 2560);
        assert!(
            input.len() >= 4_500_000,
            "fixture is only {} bytes",
            input.len()
        );
        let start = std::time::Instant::now();
        let outcome = optimise(&input, Format::Png, &OptimiseOptions::default()).unwrap();
        let took = start.elapsed();
        println!("5 MB PNG optimised in {took:?}");
        assert!(matches!(outcome, Outcome::Smaller { .. }));
        assert!(took.as_millis() < 1500, "took {took:?}");
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- optimise::tests`
Expected: FAIL to compile, `cannot find function optimise in this scope`.

- [ ] **Step 3: Implement**

Change the import near the top of `optimise/mod.rs` to `use crate::{cancel::CancelToken, sniff::Format};`, then add this after `classify` and before `#[cfg(test)]`:

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
    let output = match format {
        Format::Png => png::optimise(input, o)?,
        Format::Jpeg => jpeg::optimise(input, o)?,
        Format::WebP => webp::optimise(input, o)?,
        Format::Gif => gif::optimise(input, o)?,
    };
    if o.cancel.is_cancelled() {
        return Err(OptimiseError::Cancelled);
    }
    Ok(classify(input.len(), output))
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo test -p ferry-core --lib -- optimise::`
Expected: PASS, 38 passed, 1 ignored (`five_megabyte_png_is_optimised_within_the_clipboard_budget`).

- [ ] **Step 5: Check the 5 MB budget on this machine**

Close heavy apps first; a busy machine can fail this for reasons unrelated to the code. Add `--nocapture` to see the time.

Run: `source "$HOME/.cargo/env" && cargo test --release -p ferry-core --lib -- --ignored five_megabyte`
Expected: PASS (under 1.5 s). Measured on this Mac while other work was running (load average 13 to 14 on 8 cores): 0.79 to 0.87 s. With the spec's speed 4 and preset 2 the same input took 2.0 to 2.5 s (Decision 11).

- [ ] **Step 6: Run the whole crate, clippy and the format check**

Run: `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo fmt --all --check && cargo clippy -p ferry-core --all-targets -- -D warnings && cargo test -p ferry-core`
Expected: no format diff, no clippy output (the dead-code warnings are gone now that `optimise()` calls every format), all tests pass.

- [ ] **Step 7: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): optimise() dispatch with cancel, animation guard and never-worse grading

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo test -p ferry-core --lib -- optimise::`

---

### Task 10: Backups

**Files:**
- Create: `crates/ferry-core/src/backup.rs`
- Modify: `crates/ferry-core/src/testutil.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `chrono` (Task 1).
- Produces: `backup::Backups` — `Backups::new(root: impl Into<PathBuf>)` (today = local date), `Backups::with_clock(root, today: impl Fn() -> NaiveDate + Send + Sync + 'static)`, `store_bytes(&self, job: &str, name: &str, bytes: &[u8]) -> io::Result<PathBuf>`, `store_file(&self, job: &str, source: &Path) -> io::Result<PathBuf>`, `prune(&self, retention_days: u32) -> io::Result<PruneReport>`; `PruneReport { removed: Vec<String> }` (date-folder names, ascending). The path is `<root>/<yyyy-mm-dd>/<job>/<name>`. Only the last component of `job` and `name` is used. The first copy for a job wins. `prune` keeps today and any folder that is not an exact `yyyy-mm-dd`.
- Produces (tests only): `testutil::{set_xattr, get_xattr}` (macOS), used again in Task 11.

Restore is not a `Backups` method: the app reads the stored path back and passes it to `fsutil::atomic_replace` (Task 11). The round-trip test below proves a stored original reads back byte for byte.

- [ ] **Step 1: Add the xattr test helpers**

Append to `crates/ferry-core/src/testutil.rs`:

```rust
/// Set an extended attribute (macOS tests).
#[cfg(target_os = "macos")]
pub fn set_xattr(path: &std::path::Path, name: &std::ffi::CStr, value: &[u8]) {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let rc = unsafe {
        libc::setxattr(
            path.as_ptr(),
            name.as_ptr(),
            value.as_ptr().cast(),
            value.len(),
            0,
            0,
        )
    };
    assert_eq!(rc, 0, "{}", std::io::Error::last_os_error());
}

#[cfg(target_os = "macos")]
pub fn get_xattr(path: &std::path::Path, name: &std::ffi::CStr) -> Vec<u8> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
    let mut buf = vec![0u8; 256];
    let len = unsafe {
        libc::getxattr(
            path.as_ptr(),
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            buf.len(),
            0,
            0,
        )
    };
    assert!(len >= 0, "{}", std::io::Error::last_os_error());
    buf.truncate(len as usize);
    buf
}
```

- [ ] **Step 2: Write the failing tests**

Create `crates/ferry-core/src/backup.rs` containing only this test module, and add `pub mod backup;` to `lib.rs` (first in the list):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    fn backups(root: &Path) -> Backups {
        Backups::with_clock(root, || day(2026, 10, 22))
    }

    #[test]
    fn only_exact_iso_dates_are_date_folders() {
        assert_eq!(parse_date("2026-10-22"), Some(day(2026, 10, 22)));
        assert_eq!(parse_date("2026-02-31"), None);
        assert_eq!(parse_date("2026-2-3"), None);
        assert_eq!(parse_date("notes"), None);
    }

    #[test]
    fn bytes_are_stored_under_date_and_job_and_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let path = backups(dir.path())
            .store_bytes("job1", "shot.png", b"original")
            .unwrap();
        assert_eq!(path, dir.path().join("2026-10-22/job1/shot.png"));
        assert_eq!(fs::read(path).unwrap(), b"original");
    }

    #[test]
    fn a_file_is_copied_under_its_own_name() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("photo.jpg");
        fs::write(&source, b"pixels").unwrap();
        let path = backups(&dir.path().join("backups"))
            .store_file("job2", &source)
            .unwrap();
        assert!(path.ends_with("2026-10-22/job2/photo.jpg"));
        assert_eq!(fs::read(path).unwrap(), b"pixels");
        assert_eq!(
            fs::read(&source).unwrap(),
            b"pixels",
            "source stays in place"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_stored_file_keeps_its_screenshot_marker() {
        use crate::testutil::{get_xattr, set_xattr};
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("Screenshot.png");
        fs::write(&source, b"pixels").unwrap();
        let marker = c"com.apple.metadata:kMDItemIsScreenCapture";
        set_xattr(&source, marker, b"1");
        let path = backups(&dir.path().join("backups"))
            .store_file("job", &source)
            .unwrap();
        assert_eq!(get_xattr(&path, marker), b"1");
    }

    #[test]
    fn the_first_copy_for_a_job_is_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let b = backups(dir.path());
        b.store_bytes("job", "a.png", b"original").unwrap();
        let path = b.store_bytes("job", "a.png", b"already optimised").unwrap();
        assert_eq!(fs::read(path).unwrap(), b"original");
    }

    #[test]
    fn a_name_cannot_climb_out_of_the_backup_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("backups");
        let path = backups(&root)
            .store_bytes("../job", "../../evil.png", b"x")
            .unwrap();
        assert!(path.starts_with(&root), "{path:?}");
        assert!(!dir.path().join("evil.png").exists());
    }

    #[test]
    fn prune_removes_whole_folders_older_than_retention_only() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for name in [
            "2026-10-10",
            "2026-10-14",
            "2026-10-15",
            "2026-10-22",
            "keep-me",
        ] {
            fs::create_dir_all(root.join(name).join("job")).unwrap();
            fs::write(root.join(name).join("job/f.png"), b"x").unwrap();
        }
        // 7 days before 2026-10-22 is 2026-10-15, which stays.
        let report = backups(root).prune(7).unwrap();
        assert_eq!(report.removed, ["2026-10-10", "2026-10-14"]);
        assert!(!root.join("2026-10-14").exists());
        for name in ["2026-10-15", "2026-10-22", "keep-me"] {
            assert!(root.join(name).exists(), "{name} was removed");
        }
    }

    #[test]
    fn prune_with_zero_days_keeps_today() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["2026-10-21", "2026-10-22"] {
            fs::create_dir_all(dir.path().join(name)).unwrap();
        }
        let report = backups(dir.path()).prune(0).unwrap();
        assert_eq!(report.removed, ["2026-10-21"]);
        assert!(dir.path().join("2026-10-22").exists());
    }

    #[test]
    fn prune_on_a_missing_root_is_a_no_op() {
        let dir = tempfile::tempdir().unwrap();
        let report = backups(&dir.path().join("never-created")).prune(1).unwrap();
        assert!(report.removed.is_empty());
    }
}
```

- [ ] **Step 3: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- backup::`
Expected: FAIL to compile, `cannot find type Backups in this scope`.

- [ ] **Step 4: Implement**

Put this above the test module:

```rust
//! Originals are copied here before Ferry overwrites a file (R§2.4), under
//! `<root>/<yyyy-mm-dd>/<job-id>/<name>`, with the date in local time.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use chrono::{Days, Local, NaiveDate};

type Clock = Box<dyn Fn() -> NaiveDate + Send + Sync>;

const DATE_FORMAT: &str = "%Y-%m-%d";

pub struct Backups {
    root: PathBuf,
    today: Clock,
}

/// What `prune` removed, as date-folder names in ascending order.
#[derive(Debug, PartialEq, Eq)]
pub struct PruneReport {
    pub removed: Vec<String>,
}

impl Backups {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self::with_clock(root, || Local::now().date_naive())
    }

    pub fn with_clock(
        root: impl Into<PathBuf>,
        today: impl Fn() -> NaiveDate + Send + Sync + 'static,
    ) -> Self {
        Self {
            root: root.into(),
            today: Box::new(today),
        }
    }

    /// Back up `bytes` as `name`. The first copy for a job wins: a second call
    /// returns the existing path, so an optimised file can never replace its original.
    pub fn store_bytes(&self, job: &str, name: &str, bytes: &[u8]) -> io::Result<PathBuf> {
        let dest = self.destination(job, name)?;
        if !dest.exists() {
            fs::write(&dest, bytes)?;
        }
        Ok(dest)
    }

    /// Back up the file at `source` under its own file name; first copy wins.
    /// `fs::copy` keeps extended attributes on macOS, so the copy is a valid
    /// source for `fsutil::copy_xattrs`.
    pub fn store_file(&self, job: &str, source: &Path) -> io::Result<PathBuf> {
        let name = source
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?;
        let dest = self.destination(job, name)?;
        if !dest.exists() {
            fs::copy(source, &dest)?;
        }
        Ok(dest)
    }

    /// Remove date folders more than `retention_days` days before today; today's
    /// folder always stays. Folders whose names are not dates are left alone.
    pub fn prune(&self, retention_days: u32) -> io::Result<PruneReport> {
        let mut removed = Vec::new();
        let Some(cutoff) = (self.today)().checked_sub_days(Days::new(retention_days.into())) else {
            return Ok(PruneReport { removed });
        };
        let entries = match fs::read_dir(&self.root) {
            Ok(entries) => entries,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(PruneReport { removed }),
            Err(e) => return Err(e),
        };
        for entry in entries {
            let entry = entry?;
            let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                continue;
            };
            if parse_date(&name).is_some_and(|day| day < cutoff) && entry.file_type()?.is_dir() {
                fs::remove_dir_all(entry.path())?;
                removed.push(name);
            }
        }
        removed.sort();
        Ok(PruneReport { removed })
    }

    fn destination(&self, job: &str, name: &str) -> io::Result<PathBuf> {
        // Only the last component of each, so neither can climb out of the root.
        let name = Path::new(name)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty backup name"))?;
        let job = Path::new(job)
            .file_name()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "empty job id"))?;
        let dir = self
            .root
            .join((self.today)().format(DATE_FORMAT).to_string())
            .join(job);
        fs::create_dir_all(&dir)?;
        Ok(dir.join(name))
    }
}

/// Exactly `yyyy-mm-dd`; `2026-2-3` or `2026-02-31` are not Ferry's folders.
fn parse_date(name: &str) -> Option<NaiveDate> {
    let date = NaiveDate::parse_from_str(name, DATE_FORMAT).ok()?;
    (date.format(DATE_FORMAT).to_string() == name).then_some(date)
}
```

- [ ] **Step 5: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- backup::`
Expected: PASS, 9 tests on macOS (one fewer elsewhere: `a_stored_file_keeps_its_screenshot_marker` is macOS-only).

- [ ] **Step 6: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): backups by date and job with age-based pruning

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- backup::`

---

### Task 11: `fsutil` — atomic replace, times, extended attributes

**Files:**
- Create: `crates/ferry-core/src/fsutil.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `testutil::{set_xattr, get_xattr}` (Task 10), `libc` on macOS (Task 1).
- Produces:
  - `fsutil::atomic_replace(target: &Path, bytes: &[u8]) -> io::Result<()>`: writes `<target>.ferry-tmp`, `fsync`s, copies the original's permissions, renames over the target; removes the temp file and leaves the original untouched on any failure; follows a symlink target.
  - `fsutil::copy_times(from: &Path, to: &Path) -> io::Result<()>`: modified, accessed and (macOS, Windows) created times.
  - `fsutil::copy_xattrs(from: &Path, to: &Path) -> io::Result<()>`: macOS copies every extended attribute (including `com.apple.metadata:kMDItemIsScreenCapture`) and gives up after `XATTR_TIMEOUT` (5 s) with `ErrorKind::TimedOut`; other platforms return `Ok(())` (Decision 6).
  - `fsutil::XATTR_TIMEOUT: Duration`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/fsutil.rs` containing only this test module, and add `pub mod fsutil;` to `lib.rs` (after `cancel`). Tests cover: content swapped and no temp file left; a rename that cannot happen (target is a non-empty directory, which fails on every OS) leaves the original and cleans up; a `0600` file stays `0600`; a symlink survives and its destination changes; times round-trip; the timeout helper gives up and also passes results through; macOS attributes (binary values included) are copied and a missing source is an error.

```rust
#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(target_os = "macos")]
    use crate::testutil::{get_xattr, set_xattr};
    use std::time::{Instant, UNIX_EPOCH};

    #[test]
    fn atomic_replace_swaps_the_content_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("a.png");
        fs::write(&target, b"old").unwrap();
        atomic_replace(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new");
        assert!(!dir.path().join("a.png.ferry-tmp").exists());
    }

    #[test]
    fn a_failed_rename_leaves_the_original_untouched_and_cleans_up() {
        let dir = tempfile::tempdir().unwrap();
        // Renaming a file over a non-empty directory fails on every OS.
        let target = dir.path().join("a.png");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("inner.txt"), b"keep").unwrap();
        assert!(atomic_replace(&target, b"new").is_err());
        assert_eq!(fs::read(target.join("inner.txt")).unwrap(), b"keep");
        assert!(!dir.path().join("a.png.ferry-tmp").exists());
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_keeps_the_original_permissions() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("private.png");
        fs::write(&target, b"old").unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
        atomic_replace(&target, b"new").unwrap();
        let mode = fs::metadata(&target).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
    }

    #[cfg(unix)]
    #[test]
    fn atomic_replace_through_a_symlink_keeps_the_link() {
        let dir = tempfile::tempdir().unwrap();
        let real = dir.path().join("real.png");
        let link = dir.path().join("link.png");
        fs::write(&real, b"old").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        atomic_replace(&link, b"new").unwrap();
        assert!(fs::symlink_metadata(&link).unwrap().is_symlink());
        assert_eq!(fs::read(&real).unwrap(), b"new");
    }

    #[test]
    fn copy_times_carries_modified_and_created_times() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from.png");
        let to = dir.path().join("to.png");
        fs::write(&from, b"a").unwrap();
        fs::write(&to, b"b").unwrap();
        // 2001-09-09, long before either file existed.
        let then = UNIX_EPOCH + Duration::from_secs(1_000_000_000);
        #[allow(unused_mut)]
        let mut times = FileTimes::new().set_modified(then).set_accessed(then);
        #[cfg(target_os = "macos")]
        {
            use std::os::macos::fs::FileTimesExt;
            times = times.set_created(then);
        }
        #[cfg(windows)]
        {
            use std::os::windows::fs::FileTimesExt;
            times = times.set_created(then);
        }
        OpenOptions::new()
            .write(true)
            .open(&from)
            .unwrap()
            .set_times(times)
            .unwrap();

        copy_times(&from, &to).unwrap();

        let meta = fs::metadata(&to).unwrap();
        assert_eq!(meta.modified().unwrap(), then);
        #[cfg(any(target_os = "macos", windows))]
        assert_eq!(meta.created().unwrap(), then);
    }

    #[test]
    fn with_timeout_gives_up_on_slow_work() {
        let start = Instant::now();
        let err = with_timeout(Duration::from_millis(50), || {
            thread::sleep(Duration::from_secs(2));
            Ok(())
        })
        .unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::TimedOut);
        assert!(start.elapsed() < Duration::from_secs(1));
    }

    #[test]
    fn with_timeout_returns_the_work_result() {
        assert_eq!(with_timeout(Duration::from_secs(1), || Ok(7)).unwrap(), 7);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_xattrs_carries_the_screenshot_marker_and_binary_values() {
        let dir = tempfile::tempdir().unwrap();
        let from = dir.path().join("from.png");
        let to = dir.path().join("to.png");
        fs::write(&from, b"a").unwrap();
        fs::write(&to, b"b").unwrap();
        let marker = c"com.apple.metadata:kMDItemIsScreenCapture";
        let value = [0u8, 1, 2, 255, 254];
        set_xattr(&from, marker, &value);
        set_xattr(&from, c"user.note", b"hello");

        copy_xattrs(&from, &to).unwrap();

        assert_eq!(get_xattr(&to, marker), value);
        assert_eq!(get_xattr(&to, c"user.note"), b"hello");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn copy_xattrs_from_a_missing_file_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let to = dir.path().join("to.png");
        fs::write(&to, b"b").unwrap();
        assert!(copy_xattrs(&dir.path().join("missing.png"), &to).is_err());
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- fsutil::`
Expected: FAIL to compile, unresolved `atomic_replace` / `with_timeout`.

- [ ] **Step 3: Implement**

Put this above the test module:

```rust
//! Writing results back over the user's files without ever leaving a
//! half-written one, and carrying file metadata across the replacement.

use std::fs::{self, File, FileTimes, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

/// How long extended attributes may take to copy (NFR-P4).
pub const XATTR_TIMEOUT: Duration = Duration::from_secs(5);

/// Replace `target` with `bytes`: write `<target>.ferry-tmp` beside it, `fsync`
/// it, then rename it over the target. If anything fails the original is untouched
/// and the temporary file is removed. A symlink target is followed, so the link
/// survives and its destination is the file that changes.
pub fn atomic_replace(target: &Path, bytes: &[u8]) -> io::Result<()> {
    let target = if fs::symlink_metadata(target).is_ok_and(|m| m.is_symlink()) {
        fs::canonicalize(target)?
    } else {
        target.to_owned()
    };
    let tmp = temp_path(&target)?;
    let result = write_synced(&tmp, bytes)
        .and_then(|()| match fs::metadata(&target) {
            // Keep the original's permissions (a 0600 file must not become 0644).
            Ok(meta) => fs::set_permissions(&tmp, meta.permissions()),
            Err(_) => Ok(()),
        })
        .and_then(|()| fs::rename(&tmp, &target));
    if result.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    result
}

fn temp_path(target: &Path) -> io::Result<PathBuf> {
    let mut name = target
        .file_name()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "path has no file name"))?
        .to_owned();
    name.push(".ferry-tmp");
    Ok(target.with_file_name(name))
}

fn write_synced(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(bytes)?;
    file.sync_all()
}

/// Give `to` the modified, accessed and (macOS, Windows) created times of `from`.
pub fn copy_times(from: &Path, to: &Path) -> io::Result<()> {
    let meta = fs::metadata(from)?;
    #[allow(unused_mut)]
    let mut times = FileTimes::new()
        .set_accessed(meta.accessed()?)
        .set_modified(meta.modified()?);
    #[cfg(target_os = "macos")]
    if let Ok(created) = meta.created() {
        use std::os::macos::fs::FileTimesExt;
        times = times.set_created(created);
    }
    #[cfg(windows)]
    if let Ok(created) = meta.created() {
        use std::os::windows::fs::FileTimesExt;
        times = times.set_created(created);
    }
    OpenOptions::new().write(true).open(to)?.set_times(times)
}

/// Copy every extended attribute from `from` to `to`, including
/// `com.apple.metadata:kMDItemIsScreenCapture`, giving up after [`XATTR_TIMEOUT`].
/// Replacing a file drops its attributes, so call this after `atomic_replace`
/// with the backed-up original as `from`.
#[cfg(target_os = "macos")]
pub fn copy_xattrs(from: &Path, to: &Path) -> io::Result<()> {
    let (from, to) = (from.to_owned(), to.to_owned());
    with_timeout(XATTR_TIMEOUT, move || xattr::copy(&from, &to))
}

/// Other platforms have no attribute to carry across; Windows alternate data
/// streams such as `Zone.Identifier` belong to the old file.
#[cfg(not(target_os = "macos"))]
pub fn copy_xattrs(_from: &Path, _to: &Path) -> io::Result<()> {
    Ok(())
}

/// Run `work` on its own thread and stop waiting after `limit`. A blocked
/// system call cannot be cancelled, so the thread is left to finish by itself.
fn with_timeout<T: Send + 'static>(
    limit: Duration,
    work: impl FnOnce() -> io::Result<T> + Send + 'static,
) -> io::Result<T> {
    let (tx, rx) = mpsc::channel();
    thread::spawn(move || {
        let _ = tx.send(work());
    });
    rx.recv_timeout(limit)
        .unwrap_or_else(|_| Err(io::Error::new(io::ErrorKind::TimedOut, "timed out")))
}

#[cfg(target_os = "macos")]
mod xattr {
    use std::ffi::{CStr, CString, OsStr};
    use std::io;
    use std::os::unix::ffi::OsStrExt;
    use std::path::Path;

    pub fn copy(from: &Path, to: &Path) -> io::Result<()> {
        let (from, to) = (c_path(from)?, c_path(to)?);
        for name in names(&from)? {
            let value = get(&from, &name)?;
            // SAFETY: both pointers are NUL-terminated and live for the call.
            let rc = unsafe {
                libc::setxattr(
                    to.as_ptr(),
                    name.as_ptr(),
                    value.as_ptr().cast(),
                    value.len(),
                    0,
                    0,
                )
            };
            if rc != 0 {
                return Err(io::Error::last_os_error());
            }
        }
        Ok(())
    }

    fn c_path(path: &Path) -> io::Result<CString> {
        CString::new(OsStr::as_bytes(path.as_os_str()))
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "path contains NUL"))
    }

    fn names(path: &CString) -> io::Result<Vec<CString>> {
        // SAFETY: a null buffer with size 0 asks only for the needed length.
        let len = unsafe { libc::listxattr(path.as_ptr(), std::ptr::null_mut(), 0, 0) };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut buf = vec![0u8; len as usize];
        // SAFETY: `buf` is `len` bytes long.
        let len = unsafe { libc::listxattr(path.as_ptr(), buf.as_mut_ptr().cast(), buf.len(), 0) };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        buf.truncate(len as usize);
        Ok(buf
            .split(|&b| b == 0)
            .filter(|n| !n.is_empty())
            .map(|n| {
                CStr::from_bytes_with_nul(&[n, &[0]].concat())
                    .expect("one NUL")
                    .to_owned()
            })
            .collect())
    }

    fn get(path: &CString, name: &CString) -> io::Result<Vec<u8>> {
        // SAFETY: a null buffer with size 0 asks only for the needed length.
        let len =
            unsafe { libc::getxattr(path.as_ptr(), name.as_ptr(), std::ptr::null_mut(), 0, 0, 0) };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        let mut value = vec![0u8; len as usize];
        // SAFETY: `value` is `len` bytes long.
        let len = unsafe {
            libc::getxattr(
                path.as_ptr(),
                name.as_ptr(),
                value.as_mut_ptr().cast(),
                value.len(),
                0,
                0,
            )
        };
        if len < 0 {
            return Err(io::Error::last_os_error());
        }
        value.truncate(len as usize);
        Ok(value)
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- fsutil::`
Expected: PASS, 9 tests on macOS (fewer elsewhere: the xattr tests are macOS-only, the permission and symlink tests Unix-only).

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): atomic replace, time copy and xattr copy

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- fsutil::`

---

### Task 12: File-name templates

**Files:**
- Create: `crates/ferry-core/src/template.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `chrono::NaiveDate` (Task 1).
- Produces:
  - `template::Ctx<'a> { stem: &'a str, ext: &'a str, width: u32, height: u32, parent: &'a str, date: NaiveDate }`.
  - `template::render(tpl: &str, ctx: &Ctx) -> String`: `%f` stem, `%e` extension, `%z` `WxH`, `%y` four-digit year, `%m` and `%d` two-digit, `%P` parent folder name, `%%` a literal `%`. Unknown tokens stay as typed.
  - `template::validate(tpl: &str) -> Result<(), Vec<String>>`: the unknown tokens, once each, in order of appearance (a trailing lone `%` is the token `"%"`).
  - `template::safe_file_name(name: &str) -> String`: replaces `< > : " / \ | ? *` and control characters with `_`, trims trailing dots and spaces, and returns `_` for an empty result. M2b applies it to a rendered template before writing `work/clipboard/<name>.<ext>`.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/template.rs` containing only this test module, and add `pub mod template;` to `lib.rs` (after `sniff`):

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn ctx() -> Ctx<'static> {
        Ctx {
            stem: "shot",
            ext: "png",
            width: 1920,
            height: 1080,
            parent: "Desktop",
            date: NaiveDate::from_ymd_opt(2026, 3, 5).unwrap(),
        }
    }

    #[test]
    fn every_token_expands() {
        assert_eq!(render("%f_%z.%e", &ctx()), "shot_1920x1080.png");
        assert_eq!(render("%y-%m-%d", &ctx()), "2026-03-05");
        assert_eq!(render("%P/%f", &ctx()), "Desktop/shot");
    }

    #[test]
    fn a_double_percent_is_a_literal_percent() {
        assert_eq!(render("100%%_%f", &ctx()), "100%_shot");
        assert_eq!(validate("100%%"), Ok(()));
    }

    #[test]
    fn unknown_tokens_stay_as_typed_when_rendering() {
        assert_eq!(render("%f_%q_%", &ctx()), "shot_%q_%");
    }

    #[test]
    fn a_plain_name_and_the_default_template_are_valid() {
        assert_eq!(validate("%f"), Ok(()));
        assert_eq!(validate("~/Pictures/twitter/%f_%z.%e"), Ok(()));
        assert_eq!(validate(""), Ok(()));
    }

    #[test]
    fn validate_lists_each_unknown_token_once_in_order() {
        assert_eq!(
            validate("%f_%q%x%q"),
            Err(vec!["%q".to_string(), "%x".to_string()])
        );
        assert_eq!(validate("name%"), Err(vec!["%".to_string()]));
    }

    #[test]
    fn non_ascii_text_passes_through() {
        assert_eq!(render("Ảnh_%f", &ctx()), "Ảnh_shot");
    }

    #[test]
    fn safe_file_name_blocks_separators_and_windows_characters() {
        assert_eq!(safe_file_name("../../etc/passwd"), ".._.._etc_passwd");
        assert_eq!(safe_file_name(r"a\b:c*d?e"), "a_b_c_d_e");
        assert_eq!(safe_file_name("shot.png"), "shot.png");
        assert_eq!(safe_file_name("name. "), "name");
        assert_eq!(safe_file_name(".."), "_");
        assert_eq!(safe_file_name(""), "_");
        assert_eq!(safe_file_name("a\nb"), "a_b");
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- template::`
Expected: FAIL to compile, unresolved `Ctx` / `render`.

- [ ] **Step 3: Implement**

Put this above the test module:

```rust
//! File-name templates: `%f` name without extension, `%e` extension, `%z` size
//! (`1920x1080`), `%y` `%m` `%d` year, month and day, `%P` parent folder name,
//! and `%%` for a literal percent sign. The Settings preview calls this too,
//! through a Tauri command, so there is one implementation.

use chrono::{Datelike, NaiveDate};

pub struct Ctx<'a> {
    pub stem: &'a str,
    pub ext: &'a str,
    pub width: u32,
    pub height: u32,
    pub parent: &'a str,
    pub date: NaiveDate,
}

enum Piece {
    Literal(char),
    Token(char),
    Unknown(String),
}

fn pieces(tpl: &str) -> impl Iterator<Item = Piece> + '_ {
    let mut chars = tpl.chars();
    std::iter::from_fn(move || {
        let c = chars.next()?;
        if c != '%' {
            return Some(Piece::Literal(c));
        }
        Some(match chars.next() {
            Some('%') => Piece::Literal('%'),
            Some(t @ ('f' | 'e' | 'z' | 'y' | 'm' | 'd' | 'P')) => Piece::Token(t),
            Some(other) => Piece::Unknown(format!("%{other}")),
            None => Piece::Unknown("%".into()),
        })
    })
}

/// Expand `tpl`. Unknown tokens are kept as typed, so a half-typed template
/// shows what is wrong in the live preview.
pub fn render(tpl: &str, ctx: &Ctx) -> String {
    let mut out = String::new();
    for piece in pieces(tpl) {
        match piece {
            Piece::Literal(c) => out.push(c),
            Piece::Unknown(text) => out.push_str(&text),
            Piece::Token(t) => match t {
                'f' => out.push_str(ctx.stem),
                'e' => out.push_str(ctx.ext),
                'z' => out.push_str(&format!("{}x{}", ctx.width, ctx.height)),
                'y' => out.push_str(&format!("{:04}", ctx.date.year())),
                'm' => out.push_str(&format!("{:02}", ctx.date.month())),
                'd' => out.push_str(&format!("{:02}", ctx.date.day())),
                _ => out.push_str(ctx.parent),
            },
        }
    }
    out
}

/// `Err` lists the unknown tokens (`%q`, a trailing `%`) once each, in order of appearance.
pub fn validate(tpl: &str) -> Result<(), Vec<String>> {
    let mut unknown: Vec<String> = Vec::new();
    for piece in pieces(tpl) {
        if let Piece::Unknown(text) = piece
            && !unknown.contains(&text)
        {
            unknown.push(text);
        }
    }
    if unknown.is_empty() {
        Ok(())
    } else {
        Err(unknown)
    }
}

/// Make a rendered template safe as a single file name: path separators and
/// characters Windows forbids become `_`, trailing dots and spaces go, and an
/// empty result becomes `_`.
pub fn safe_file_name(name: &str) -> String {
    let replaced: String = name
        .chars()
        .map(|c| match c {
            '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*' => '_',
            c if c.is_control() => '_',
            c => c,
        })
        .collect();
    let trimmed = replaced.trim_end_matches(['.', ' ']);
    if trimmed.is_empty() {
        "_".into()
    } else {
        trimmed.into()
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- template::`
Expected: PASS, 7 tests.

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): file-name templates with validation and safe names

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- template::`

---

### Task 13: Settings

**Files:**
- Create: `crates/ferry-core/src/settings.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `fsutil::atomic_replace` (Task 11), `serde`, `serde_json` (Task 1).
- Produces:
  - `settings::Settings { version, general, clipboard, images, floating, backups, hotkeys, extra }` with the defaults of spec §5.7 (`Settings::default()`); every nested struct also carries `#[serde(flatten)] extra: Map<String, Value>` so unknown fields survive a load and save at every level.
  - Enums `StartState {Running, Paused, Stopped}`, `TrayStyle {Default, Classic, Flat}`, `Corner {TopLeft, TopRight, BottomLeft, BottomRight}`, `CardSize {Default, Compact}`, all `snake_case` in JSON.
  - `Settings::load(path: &Path) -> (Settings, LoadStatus)` where `LoadStatus::{Loaded, Missing, Recovered { backup: Option<PathBuf> }}`; never fails. A corrupt, empty or wrongly typed file gives defaults and a copy at `<path>.bak`; `images.compression` is clamped to 5..=100 and both `max_size_mb` to at least 1.
  - `Settings::save(&self, path: &Path) -> io::Result<()>`: creates the folder, writes pretty JSON atomically.
  - `Hotkeys::for_platform(mac: bool)`; `Default` uses `cfg!(target_os = "macos")`.
  - `settings::expand_home(path: &str, home: &Path) -> PathBuf` and `settings::contract_home(path: &Path, home: &Path) -> String` (`~` storage so synced settings work under another username).

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/settings.rs` containing only this test module, and add `pub mod settings;` to `lib.rs` (before `sidecar`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn read_json(path: &Path) -> Value {
        serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
    }

    #[test]
    fn defaults_match_the_spec() {
        let s = Settings::default();
        assert_eq!(s.version, 1);
        assert_eq!(s.images.compression, 80);
        assert!(s.images.strip_metadata && s.images.preserve_dates);
        assert_eq!(s.clipboard.file_template, "%f");
        assert_eq!(s.clipboard.max_size_mb, 50);
        assert_eq!(s.floating.corner, Corner::BottomRight);
        assert_eq!(s.floating.auto_hide_secs, 6);
        assert_eq!(s.backups.retention_days, 7);
        assert_eq!(s.general.start_state, StartState::Running);
        assert!(!s.general.show_in_screenshots);
    }

    #[test]
    fn hotkeys_default_per_platform() {
        let mac = Hotkeys::for_platform(true);
        assert_eq!(
            (
                mac.optimise.as_str(),
                mac.aggressive.as_str(),
                mac.cycle_state.as_str()
            ),
            ("Ctrl+Shift+C", "Ctrl+Shift+A", "Ctrl+Shift+P")
        );
        assert_eq!(Hotkeys::for_platform(false).optimise, "Ctrl+Alt+Shift+C");
        assert_eq!(Hotkeys::for_platform(false).cycle_state, "Ctrl+Alt+Shift+P");
    }

    #[test]
    fn a_missing_file_gives_defaults_and_creates_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let (settings, status) = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        assert_eq!(status, LoadStatus::Missing);
        assert!(!path.exists());
    }

    #[test]
    fn missing_fields_take_their_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"images": {"compression": 60}, "floating": {"corner": "top_left"}}"#,
        )
        .unwrap();
        let (settings, status) = Settings::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(settings.images.compression, 60);
        assert!(
            settings.images.strip_metadata,
            "sibling field keeps its default"
        );
        assert_eq!(settings.floating.corner, Corner::TopLeft);
        assert_eq!(settings.clipboard, Clipboard::default());
    }

    #[test]
    fn unknown_fields_survive_a_load_and_save_at_every_level() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"future": {"a": 1}, "images": {"compression": 70, "sharpen": true},
                "hotkeys": {"paste_special": "Ctrl+Alt+V"}}"#,
        )
        .unwrap();
        let (settings, _) = Settings::load(&path);
        settings.save(&path).unwrap();
        let saved = read_json(&path);
        assert_eq!(saved["future"], json!({"a": 1}));
        assert_eq!(saved["images"]["sharpen"], json!(true));
        assert_eq!(saved["images"]["compression"], json!(70));
        assert_eq!(saved["hotkeys"]["paste_special"], json!("Ctrl+Alt+V"));
        assert_eq!(
            saved["images"]["strip_metadata"],
            json!(true),
            "defaults are written out"
        );
    }

    #[test]
    fn a_corrupt_file_gives_defaults_and_keeps_a_bak_copy() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(&path, b"{ not json").unwrap();
        let (settings, status) = Settings::load(&path);
        assert_eq!(settings, Settings::default());
        let bak = dir.path().join("settings.json.bak");
        assert_eq!(
            status,
            LoadStatus::Recovered {
                backup: Some(bak.clone())
            }
        );
        assert_eq!(fs::read(bak).unwrap(), b"{ not json");
    }

    #[test]
    fn an_empty_file_and_a_non_object_count_as_corrupt() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        for content in ["", "[]", "42", r#"{"images": {"compression": "high"}}"#] {
            fs::write(&path, content).unwrap();
            let (settings, status) = Settings::load(&path);
            assert_eq!(settings, Settings::default(), "{content:?}");
            assert!(
                matches!(status, LoadStatus::Recovered { .. }),
                "{content:?}"
            );
        }
    }

    #[test]
    fn out_of_range_values_are_clamped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        fs::write(
            &path,
            r#"{"images": {"compression": 0, "max_size_mb": 0}, "clipboard": {"max_size_mb": 0}}"#,
        )
        .unwrap();
        let (settings, status) = Settings::load(&path);
        assert_eq!(status, LoadStatus::Loaded);
        assert_eq!(settings.images.compression, 5);
        assert_eq!(settings.images.max_size_mb, 1);
        assert_eq!(settings.clipboard.max_size_mb, 1);
        fs::write(&path, r#"{"images": {"compression": 200}}"#).unwrap();
        assert_eq!(Settings::load(&path).0.images.compression, 100);
    }

    #[test]
    fn save_creates_the_folder_and_leaves_no_temp_file() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config/ferry/settings.json");
        Settings::default().save(&path).unwrap();
        assert_eq!(
            Settings::load(&path),
            (Settings::default(), LoadStatus::Loaded)
        );
        assert!(
            !dir.path()
                .join("config/ferry/settings.json.ferry-tmp")
                .exists()
        );
    }

    #[test]
    fn a_changed_setting_round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("settings.json");
        let mut settings = Settings::default();
        settings.images.compression = 55;
        settings.clipboard.ignored_apps = vec!["com.1password.1password".into()];
        settings.floating.card_size = CardSize::Compact;
        settings.save(&path).unwrap();
        assert_eq!(Settings::load(&path).0, settings);
    }

    #[test]
    fn home_is_stored_as_a_tilde_and_expanded_back() {
        let home = Path::new("/Users/gos");
        assert_eq!(
            contract_home(Path::new("/Users/gos/Pictures/x"), home),
            "~/Pictures/x"
        );
        assert_eq!(contract_home(Path::new("/Users/gos"), home), "~");
        assert_eq!(
            contract_home(Path::new("/Volumes/Photos"), home),
            "/Volumes/Photos"
        );
        assert_eq!(
            contract_home(Path::new("/Users/gosling/x"), home),
            "/Users/gosling/x"
        );
        assert_eq!(
            expand_home("~/Pictures/x", home),
            Path::new("/Users/gos/Pictures/x")
        );
        assert_eq!(expand_home("~", home), Path::new("/Users/gos"));
        assert_eq!(
            expand_home("/Volumes/Photos", home),
            Path::new("/Volumes/Photos")
        );
        assert_eq!(expand_home("~other/x", home), Path::new("~other/x"));
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- settings::`
Expected: FAIL to compile, unresolved `Settings` / `LoadStatus`.

- [ ] **Step 3: Implement**

Put this above the test module:

```rust
//! `settings.json`: the model with its defaults (spec §5.7), tolerant loading
//! and an atomic save. Unknown fields at any level survive a load and save, so
//! a newer or hand-edited file loses nothing.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

use crate::fsutil;

pub const VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub version: u32,
    pub general: General,
    pub clipboard: Clipboard,
    pub images: Images,
    pub floating: Floating,
    pub backups: Backups,
    pub hotkeys: Hotkeys,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StartState {
    Running,
    Paused,
    Stopped,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrayStyle {
    Default,
    Classic,
    Flat,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Corner {
    TopLeft,
    TopRight,
    BottomLeft,
    BottomRight,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CardSize {
    Default,
    Compact,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub launch_at_login: bool,
    pub start_state: StartState,
    pub show_dock_icon: bool,
    pub show_in_screenshots: bool,
    pub tray_style: TrayStyle,
    pub language: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Clipboard {
    pub enabled: bool,
    pub skip_multiple_files: bool,
    pub max_size_mb: u32,
    pub ignored_apps: Vec<String>,
    pub accumulate: bool,
    pub copy_paths: bool,
    pub file_template: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Images {
    /// 5 to 100; the encoders' quality `q`.
    pub compression: u8,
    /// Stored and shown, but no effect this cycle ("Coming soon").
    pub adaptive: bool,
    /// Stored and shown, but no effect this cycle ("Coming soon").
    pub switch_png_jpeg: bool,
    pub strip_metadata: bool,
    pub preserve_dates: bool,
    pub ignored_extensions: Vec<String>,
    pub max_size_mb: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Floating {
    pub enabled: bool,
    pub corner: Corner,
    pub card_size: CardSize,
    /// Seconds before a card hides; `0` keeps it until dismissed.
    pub auto_hide_secs: u32,
    pub follow_cursor: bool,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Backups {
    pub retention_days: u32,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Hotkeys {
    pub optimise: String,
    pub aggressive: String,
    pub cycle_state: String,
    #[serde(flatten)]
    pub extra: Map<String, Value>,
}

impl Hotkeys {
    /// `Ctrl+Shift+C/A/P` on macOS; Windows adds Alt because Ctrl+Shift+C opens
    /// browser dev tools.
    pub fn for_platform(mac: bool) -> Self {
        let modifiers = if mac { "Ctrl+Shift" } else { "Ctrl+Alt+Shift" };
        Self {
            optimise: format!("{modifiers}+C"),
            aggressive: format!("{modifiers}+A"),
            cycle_state: format!("{modifiers}+P"),
            extra: Map::new(),
        }
    }
}

impl Default for Hotkeys {
    fn default() -> Self {
        Self::for_platform(cfg!(target_os = "macos"))
    }
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            version: VERSION,
            general: General::default(),
            clipboard: Clipboard::default(),
            images: Images::default(),
            floating: Floating::default(),
            backups: Backups::default(),
            hotkeys: Hotkeys::default(),
            extra: Map::new(),
        }
    }
}

impl Default for General {
    fn default() -> Self {
        Self {
            launch_at_login: true,
            start_state: StartState::Running,
            show_dock_icon: false,
            show_in_screenshots: false,
            tray_style: TrayStyle::Default,
            language: "en".into(),
            extra: Map::new(),
        }
    }
}

impl Default for Clipboard {
    fn default() -> Self {
        Self {
            enabled: true,
            skip_multiple_files: true,
            max_size_mb: 50,
            ignored_apps: Vec::new(),
            accumulate: false,
            copy_paths: false,
            file_template: "%f".into(),
            extra: Map::new(),
        }
    }
}

impl Default for Images {
    fn default() -> Self {
        Self {
            compression: 80,
            adaptive: false,
            switch_png_jpeg: false,
            strip_metadata: true,
            preserve_dates: true,
            ignored_extensions: Vec::new(),
            max_size_mb: 50,
            extra: Map::new(),
        }
    }
}

impl Default for Floating {
    fn default() -> Self {
        Self {
            enabled: true,
            corner: Corner::BottomRight,
            card_size: CardSize::Default,
            auto_hide_secs: 6,
            follow_cursor: true,
            extra: Map::new(),
        }
    }
}

impl Default for Backups {
    fn default() -> Self {
        Self {
            retention_days: 7,
            extra: Map::new(),
        }
    }
}

/// How `Settings::load` found the file.
#[derive(Debug, PartialEq, Eq)]
pub enum LoadStatus {
    Loaded,
    Missing,
    /// The file could not be parsed. Defaults are in use; `backup` is where the
    /// unreadable file was copied, or `None` if that copy failed too.
    Recovered {
        backup: Option<PathBuf>,
    },
}

impl Settings {
    /// Read `path`. A missing file gives defaults; a corrupt one gives defaults
    /// and is copied to `<path>.bak` first. Out-of-range values are clamped.
    pub fn load(path: &Path) -> (Settings, LoadStatus) {
        let bytes = match fs::read(path) {
            Ok(bytes) => bytes,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                return (Settings::default(), LoadStatus::Missing);
            }
            Err(_) => return (Settings::default(), recover(path)),
        };
        match serde_json::from_slice::<Settings>(&bytes) {
            Ok(settings) => (settings.clamped(), LoadStatus::Loaded),
            Err(_) => (Settings::default(), recover(path)),
        }
    }

    /// Write the file atomically, creating its folder if needed.
    pub fn save(&self, path: &Path) -> io::Result<()> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir)?;
        }
        let json = serde_json::to_vec_pretty(self).map_err(io::Error::other)?;
        fsutil::atomic_replace(path, &json)
    }

    /// A hand-edited file must not turn the encoders or the size limit into nonsense.
    fn clamped(mut self) -> Self {
        self.images.compression = self.images.compression.clamp(5, 100);
        self.images.max_size_mb = self.images.max_size_mb.max(1);
        self.clipboard.max_size_mb = self.clipboard.max_size_mb.max(1);
        self
    }
}

fn recover(path: &Path) -> LoadStatus {
    let mut name = path.file_name().unwrap_or_default().to_owned();
    name.push(".bak");
    let backup = path.with_file_name(name);
    let backup = fs::copy(path, &backup).is_ok().then_some(backup);
    LoadStatus::Recovered { backup }
}

/// Expand a leading `~` using `home`. Anything else is returned unchanged.
pub fn expand_home(path: &str, home: &Path) -> PathBuf {
    match path.strip_prefix('~') {
        Some("") => home.to_owned(),
        Some(rest) if rest.starts_with(['/', '\\']) => home.join(&rest[1..]),
        _ => PathBuf::from(path),
    }
}

/// Store `path` with a leading `~` when it is inside `home`, so a settings file
/// synced to another account still points at that account's folders.
pub fn contract_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".into(),
        Ok(rest) => format!("~/{}", rest.to_string_lossy().replace('\\', "/")),
        Err(_) => path.to_string_lossy().into_owned(),
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- settings::`
Expected: PASS, 11 tests.

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): settings model with tolerant load and atomic save

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- settings::`

---

### Task 14: Job types

**Files:**
- Create: `crates/ferry-core/src/job.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Consumes: `serde`, `serde_json` (Task 1).
- Produces:
  - `job::JobId` — `JobId::generate()`, `.as_str()`; serialises as a plain string; unique across launches; only `0-9 a-f -`.
  - `job::JobState` — `Queued | Optimising { progress: f32, step: String } | Done | AlreadyOptimal | Larger | Error { reason: String } | Restored`, serialised with `{"kind": "snake_case_name", ...}`.
  - `job::Source::{Clipboard, File}` (`snake_case`).
  - `job::JobUpdate { id: JobId, state: JobState, name: String, ext: String, before: u64, after: Option<u64>, detail: String, thumb_path: Option<PathBuf>, source: Source, can_restore: bool }` — everything a result card shows. M2b maps `thumb_path` to the card's `thumb` URL and `after: None` to the card's pending state.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/job.rs` containing only this test module, and add `pub mod job;` to `lib.rs` (after `geometry`):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    #[test]
    fn job_ids_are_unique_and_safe_as_folder_names() {
        let ids: HashSet<String> = (0..1000)
            .map(|_| JobId::generate().as_str().to_owned())
            .collect();
        assert_eq!(ids.len(), 1000);
        assert!(
            ids.iter()
                .all(|id| id.chars().all(|c| c.is_ascii_hexdigit() || c == '-'))
        );
    }

    #[test]
    fn states_serialise_with_a_kind_tag() {
        let json = |s: &JobState| serde_json::to_string(s).unwrap();
        assert_eq!(json(&JobState::Queued), r#"{"kind":"queued"}"#);
        assert_eq!(
            json(&JobState::AlreadyOptimal),
            r#"{"kind":"already_optimal"}"#
        );
        assert_eq!(
            json(&JobState::Optimising {
                progress: 0.5,
                step: "Encoding JPEG".into()
            }),
            r#"{"kind":"optimising","progress":0.5,"step":"Encoding JPEG"}"#
        );
        assert_eq!(
            json(&JobState::Error {
                reason: "Read-only folder".into()
            }),
            r#"{"kind":"error","reason":"Read-only folder"}"#
        );
    }

    #[test]
    fn an_update_round_trips_through_json() {
        let update = JobUpdate {
            id: JobId::generate(),
            state: JobState::Done,
            name: "shot".into(),
            ext: "png".into(),
            before: 2_000_000,
            after: Some(310_000),
            detail: "Quantised to 256 colours".into(),
            thumb_path: Some(PathBuf::from("/tmp/thumb.png")),
            source: Source::Clipboard,
            can_restore: true,
        };
        let json = serde_json::to_string(&update).unwrap();
        assert!(json.contains(r#""source":"clipboard""#), "{json}");
        assert_eq!(serde_json::from_str::<JobUpdate>(&json).unwrap(), update);
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- job::`
Expected: FAIL to compile, unresolved `JobId` / `JobState` / `JobUpdate`.

- [ ] **Step 3: Implement**

Put this above the test module:

```rust
//! The shape of a job as it travels from the pipeline to the result cards.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Unique across app launches, because it names the job's backup folder.
/// Only `0-9`, `a-f` and `-`, so it is safe as a folder name on every OS.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct JobId(String);

impl JobId {
    pub fn generate() -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let millis = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        Self(format!(
            "{millis:x}-{:x}",
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ))
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum JobState {
    Queued,
    Optimising { progress: f32, step: String },
    Done,
    AlreadyOptimal,
    Larger,
    Error { reason: String },
    Restored,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    Clipboard,
    File,
}

/// Everything a result card shows, sent each time a job changes. Sizes are
/// bytes; `after` is `None` until the output exists.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JobUpdate {
    pub id: JobId,
    pub state: JobState,
    pub name: String,
    pub ext: String,
    pub before: u64,
    pub after: Option<u64>,
    pub detail: String,
    pub thumb_path: Option<PathBuf>,
    pub source: Source,
    pub can_restore: bool,
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- job::`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**
```bash
git add crates/ferry-core
git commit -m "feat(core): job id, state and update types

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

**Task test command:** `source "$HOME/.cargo/env" && cargo test -p ferry-core --lib -- job::`

---

### Task 15: CI builds gifsicle for the core tests, then prove all three OSes

**Files:**
- Modify: `scripts/build-gifsicle.sh`
- Modify: `.github/workflows/ci.yml`
- Modify: `README.md`

**Interfaces:**
- Consumes: the whole crate (Tasks 1 to 14); `ci_always_has_gifsicle` (Task 8), which fails when `CI` is set and `FERRY_GIFSICLE` is not.
- Produces: a `core` job that, on Ubuntu, macOS (arm64) and Windows, builds the pinned gifsicle for the runner's own triple, exports `FERRY_GIFSICLE`, and runs fmt (Ubuntu only), clippy `-D warnings` and `cargo test -p ferry-core`.

- [ ] **Step 1: Teach the gifsicle build script about Linux**

In `scripts/build-gifsicle.sh`, change the first `case "$TRIPLE"` block and add a Linux branch to the second:

```bash
  x86_64-pc-windows-msvc) ARCHS="" ;;
  x86_64-unknown-linux-gnu) ARCHS="" ;;
  *) echo "unsupported target: $TRIPLE" >&2; exit 1 ;;
```

and, in the second `case`, insert before the `x86_64-pc-windows-msvc)` branch:

```bash
  x86_64-unknown-linux-gnu)
    ./configure --disable-gifview --disable-gifdiff CFLAGS="-O2" >/dev/null
    make -j4 >/dev/null
    BIN="src/gifsicle"
    EXT=""
    ;;
```

and change the strip line to `case "$TRIPLE" in *-apple-darwin|*-linux-gnu) strip "$DEST" ;; esac`.

Run: `bash -n scripts/build-gifsicle.sh`
Expected: no output (syntax is valid). The Linux branch is proven by CI in Step 6, since this Mac cannot run it.

- [ ] **Step 2: Update the core job**

In `.github/workflows/ci.yml`, replace the start of the `core` job:

```yaml
  core:
    name: core (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - if: runner.os == 'Linux'
        run: cargo fmt --all --check
```

with:

```yaml
  core:
    name: core (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        include:
          - os: ubuntu-latest
            triple: x86_64-unknown-linux-gnu
            exe: ''
          - os: macos-latest
            triple: aarch64-apple-darwin
            exe: ''
          - os: windows-latest
            triple: x86_64-pc-windows-msvc
            exe: .exe
    runs-on: ${{ matrix.os }}
    env:
      # The GIF tests run the real sidecar; ci_always_has_gifsicle fails if this is unset.
      FERRY_GIFSICLE: ${{ github.workspace }}/src-tauri/binaries/gifsicle-${{ matrix.triple }}${{ matrix.exe }}
    steps:
      - uses: actions/checkout@v7
      - uses: dtolnay/rust-toolchain@stable
        with:
          components: rustfmt, clippy
      - uses: Swatinem/rust-cache@v2
      - if: runner.os == 'Windows'
        uses: ilammy/msvc-dev-cmd@v1
      - name: Build gifsicle sidecar
        shell: bash
        run: scripts/build-gifsicle.sh ${{ matrix.triple }}
      - if: runner.os == 'Linux'
        run: cargo fmt --all --check
```

The `clippy` and `test` steps below stay as they are.

Run: `ruby -ryaml -e 'YAML.load_file(".github/workflows/ci.yml"); puts "yaml ok"'`
Expected: `yaml ok`.

- [ ] **Step 3: Document the variable**

In `README.md`, directly under the `Checks:` line, add:

```markdown
GIF tests run the real sidecar when `FERRY_GIFSICLE` points at it, for example
`export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"`.
Without it they skip; CI always sets it.
```

- [ ] **Step 4: Run everything locally once more**

Run: `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test -p ferry-core`
Expected: clean format and clippy; 97 lib tests passed with 1 ignored, 7 sidecar tests passed.

- [ ] **Step 5: Commit and push the branch**
```bash
git add scripts .github README.md
git commit -m "ci: build gifsicle and run the core tests with it on every OS

Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>"
```

Then push the branch so CI runs on all three systems (pushing a feature branch is part of this plan; do not touch `main`):

```bash
git push -u origin m2a-core-engine
```

- [ ] **Step 6: Watch CI to a result**

Run: `~/.local/bin/gh run list --branch m2a-core-engine --limit 1` for the run id, then `~/.local/bin/gh run watch <id> --exit-status` (`gh` lives in `~/.local/bin` on this Mac and is not on `PATH`).
Expected: `core (ubuntu-latest)`, `core (macos-latest)`, `core (windows-latest)`, `web`, and both `bundle` jobs all green. The bundle jobs now compile the new C dependencies for `universal-apple-darwin` and Windows.

If a job fails, read its log (`~/.local/bin/gh run view <id> --log-failed`), find the cause with superpowers:systematic-debugging, and fix it with a test that fails first where the code is at fault. Suspects, most likely first: the Windows runner's C toolchain for `mozjpeg-sys` or `libwebp-sys`; the Linux `configure` step of gifsicle; a Unix-only assumption in a test (paths, permissions). Record each fix as a ledger `Ruling:`. Do not weaken a test to make CI green.

- [ ] **Step 7: Finish**

Report the three core results and the state of the bundle jobs. Then use superpowers:finishing-a-development-branch (it asks you to choose merge, PR or keep; do not merge without that choice).

**Task test command:** `source "$HOME/.cargo/env" && export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')" && cargo test -p ferry-core`

---

## After M2a

- **M2b — app core loop** (own plan): `platform` clipboard watch, read and write (`NSPasteboard` poll on macOS, `AddClipboardFormatListener` on Windows), the job pipeline on `spawn_blocking` behind a semaphore (`sniff` → `optimise` → write → `Done`), card states over the stack window, copy / reveal / restore / try-aggressive actions, the three global hotkeys and the run-state cycle, concealed-clipboard and ignored-app rules. It uses `Backups`, `fsutil`, `template`, `Settings` and `JobUpdate` from this plan, and re-checks the NFR-P2 timing in the app log (§8 manual check 2).
- **M2c — Settings and housekeeping** (own plan): the Settings window panes (General, Clipboard, Images, Floating results) with search and the template live preview, autostart, `tracing` logs, startup `Backups::prune`, and the working-directory force-clean.
