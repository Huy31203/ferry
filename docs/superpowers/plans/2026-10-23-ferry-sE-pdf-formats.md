# Stream E: PDF and formats Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ferry optimises PDFs (image downsampling to a DPI, structural recompression), crops PDF pages to a ratio or a device and uncrops them, extracts pages as PNG or JPEG, and reads and writes TIFF, HEIC and AVIF and animated WebP, all through the Cycle 2 recipe model.

**Architecture:** `sniff::Format` gains `Tiff`, `Heic`, `Avif` and `Pdf`, so every existing path (jobs, cards, format bar, output rules, Restore) carries the new formats. Raster formats plug into `edit::raster::{decode, encode}` and `optimise::encode`:
- TIFF uses the `tiff` crate.
- HEIC uses the operating system's codec, through a new `oscodec` module: ImageIO on macOS, WIC on Windows.
- AVIF uses `avif-decode` and `ravif`.
- Animated WebP uses libwebp's animation API. GIF frames are composited in Rust, so a GIF converts to an animated WebP.

A new `pdf` module handles PDFs:
- It edits the file structure with `lopdf`: image XObjects, `/CropBox` and object streams.
- It renders pages with PDFium, a prebuilt BSD library loaded at run time.
- `pipeline::Job` sends PDF recipes to a child module (`pipeline_formats.rs`). That module writes a PDF result, or a folder of page images, under the same one-result-per-job rules.

The recipe (`edit::Edits`) gains `dpi`, `pages` and `uncrop`. There is no second edit model.

**Tech Stack:** Rust 2024 workspace (`crates/ferry-core`, `src-tauri`), Tauri 2.12, Svelte 5 + TypeScript, vitest. New: `lopdf` 0.45, `pdfium-render` 0.9.4 + PDFium chromium/7881, `tiff` 0.11, `avif-decode` 3.0, `ravif` 0.13, `moxcms` 0.9, `objc2-image-io` 0.3.2 (macOS), `windows` 0.62 (Windows), with `libwebp-sys`, `flate2` and `rayon` promoted from transitive to direct.

**Spec:** [Cycle 3 stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding; it wins over the requirements), [requirements](../../requirements/ferry-optimize-requirements.md):
- R§2.3 Supported formats (Image and PDF rows).
- R§2.4 Optimisation rules.
- R§2.5 Actions (Crop, Compression, Convert, PDF tools, Compare).
- R§2.6 Output and file handling.
- R§3.3 Reliability and data safety.
- R§4.3 Floating result card (thumbnail, format bar).
- R§4.6 Result context menu (PDF items).
- R§4.7 Inline controls (PDF DPI slider, Crop popover device list).
- R§4.11 Compare and preview (PDF page stepper).
- R§4.12 Settings (Images conversions; PDF pane; File handling).

It also builds on the [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md) (§4.2, §5.4, §5.8, §6, §9) and the [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) (§2, §4.2–§4.5, §5.2–§5.4).

---

## Engine decisions (read before Task 1)

These choices are made; the tasks implement them. The licence column is what each project states; "flag" marks a licence or patent issue the user must accept.

### PDF

| Option | Version / licence | Fits Ferry? | Verdict |
| --- | --- | --- | --- |
| **lopdf** (pure Rust object model) | 0.45.0, MIT. 0.46.0 is two days old; 0.45 has every API used here: `load_mem_with_options` with `max_decompressed_size`, `was_encrypted`, `Content::decode`, `decompressed_content` (Flate + PNG/TIFF predictors), `prune_objects`, `compress`, `save_modern` | Parses the whole object graph and rewrites only what Ferry changes: image XObject streams, page `/CropBox` entries, object streams. It never regenerates content streams, so text, fonts, links, outlines and forms stay byte-for-byte | **Use: the writer** (optimise, DPI downsampling, crop, uncrop) |
| **PDFium** via `pdfium-render` | `pdfium-render` 0.9.4, MIT OR Apache-2.0, feature `pdfium_7881`. Binaries: bblanchon/pdfium-binaries `chromium/7881` (PDFium 151.0.7881.0). PDFium is BSD-3-Clause (some parts Apache-2.0). The bundled third-party code is permissive: FreeType FTL (needs credit), libjpeg-turbo IJG/BSD-3, libpng, zlib, ICU Unicode-3.0, abseil Apache-2.0, AGG 2.3, lcms2 MIT, OpenJPEG BSD-2, libtiff, fast_float, simdutf, llvm-libc Apache-2.0 WITH LLVM-exception. All are GPL-3 compatible | Chrome's renderer: the best fidelity for page images, thumbnails and compare. `libpdfium.dylib` (universal, 15.2 MB) and `pdfium.dll` (x64, 7.2 MB) are loaded at run time with `Pdfium::bind_to_library`. The default `thread_safe` feature serialises calls. Its edit API regenerates content streams (`FPDFPage_GenerateContent`) and has no DCT image setter in the safe API, so it is not used for writing | **Use: the renderer** (extract pages, posters, compare) |
| qpdf | 11.x C++, Apache-2.0; `qpdf` crate 0.3.7, MIT/Apache-2.0 | Excellent structural optimiser, but it does not downsample images or render. Building it needs CMake + libjpeg per OS, and Apple has no official binary | Reject: lopdf covers the structure |
| MuPDF | 1.26, **AGPL-3.0** (flag); `mupdf` crate 0.8.0, AGPL-3.0 | Renders and rewrites images, but the AGPL network clause would govern the whole app, and it is a large C build with bindgen | Reject (licence) |
| Ghostscript | 10.x, **AGPL-3.0** (flag); listed in R§1 | `pdfwrite` downsamples well, but it re-distils the whole document (loses structure and tags, re-subsets fonts). It is 30–40 MB, has no macOS binary (a source build per arch), and has a long CVE history in its PostScript interpreter | Reject (licence, size, fidelity). This replaces R§1's toolchain row; the brief lets the stream choose |
| hayro | 0.8.0, Apache-2.0 OR MIT, pure Rust (vello_cpu) | No bundling, testable everywhere; used by Typst. Younger, with less edge-case fidelity and API churn | Fallback renderer if the user rejects bundling PDFium (open question 1) |

**Linked library, not a sidecar.** Rendering returns pixel buffers straight into the existing encoders (no PNG round trip). A page count or a poster costs milliseconds, and there is no per-page process start. The cost is that a PDFium crash would take Ferry down. PDFium is fuzzed continuously by Chrome's ClusterFuzz, so this is accepted (Risk 1). Static linking (`static` feature) is rejected: no static builds exist for both OSes, and it needs per-OS C++ runtime flags.

**Bundling** mirrors gifsicle (`scripts/build-gifsicle.sh` → `src-tauri/binaries/`, git-ignored):
- `scripts/fetch-pdfium.sh <triple>` downloads the pinned archive and checks its SHA-256.
- Platform config overlays ship it: `src-tauri/tauri.macos.conf.json` puts `libpdfium.dylib` in `Contents/Frameworks/`; `src-tauri/tauri.windows.conf.json` puts `pdfium.dll` beside `Ferry.exe`.
- Using overlays means `tauri.conf.json` is not touched.

**How the PDF operations work:**
- **DPI downsampling (`pdf::images`).** Walk every page's content stream with a CTM stack (`q`, `Q`, `cm`), recursing into Form XObjects. At each `Do` of an image XObject, the drawn size in points is `hypot(a, b) × hypot(c, d)` of the CTM, and `dpi = pixels × 72 / points` (the smaller axis). An image drawn several times keeps the smallest DPI, which is its largest drawing.
  - An image whose DPI is above `target × 1.1` is resampled (Lanczos3, `fast_image_resize`) to `target`.
  - DCT images are re-encoded as JPEG at the recipe's quality.
  - Flate or raw images become JPEG when photo-like (1024 or more distinct colours in a sample), else Flate level 9.
  - A replacement is kept only when it is smaller.
- **Crop (`pdf::crop`).** A page shows `CropBox ∩ MediaBox` (both inheritable from the page tree). Cropping to a ratio sets each page's own `/CropBox` to the largest centred box of that ratio inside the visible box. The ratio is turned to match the page's displayed orientation (`/Rotate` 90 or 270 swaps it). **Uncrop** sets `/CropBox` to the `/MediaBox`, showing parts an earlier tool cropped away. Ferry's own crops never compound: every run starts from the original.
- **Extract pages (`pdf::render`).** PDFium renders each requested page at `dpi` onto white (`points × dpi / 72` pixels, long side ≤ 30 000, ≤ 150 M pixels), with annotations and form fields. The existing encoders write PNG or JPEG.

### HEIC

| Option | Licence / patents | Verdict |
| --- | --- | --- |
| libheif 1.23.1 (`libheif-rs` 3.0.0 MIT, `libheif-sys` 5.3.1+1.23.1 MIT) + libde265 1.0.x | libheif and libde265 are **LGPL-3.0** (flag; compatible with GPL-3, but it needs a CMake/C++ build per OS). HEVC decoding is covered by **HEVC patent pools** (MPEG LA/Via LA, Access Advance; flag). Shipping a decoder exposes the distributor. Encoding would add x265, **GPL-2.0** (flag), plus encoder royalties | Reject |
| **OS codecs: macOS ImageIO** (`objc2-image-io` 0.3.2, Zlib OR Apache-2.0 OR MIT) | Apple licenses HEVC for its own codec. Decodes every HEIC (grids, HDR base image, `irot`/`imir` transforms) on macOS 13+. Encodes when `CGImageDestinationCopyTypeIdentifiers()` lists `public.heic` | **Use**: decode + encode on macOS |
| **OS codecs: Windows WIC** (`windows` 0.62.2, MIT OR Apache-2.0) | Microsoft licenses HEVC through its Store extensions. Decoding works only when the user has installed "HEIF Image Extensions" (free) and "HEVC Video Extensions" (USD 0.99, or the free OEM edition) | **Use**: decode only. Without the extensions the card says how to get them. Encoding on Windows is out of scope (the HEIC chip is hidden there) |

HDR HEIC → HDR JPEG (gain maps) is not done: ImageIO gives the SDR rendition, written as an SDR JPEG (open question 6).

### AVIF

| Need | Choice | Licence |
| --- | --- | --- |
| Decode | `avif-decode` 3.0.0 (Kornel Lesiński), on `rav1d` 1.1.0, a pure-Rust port of dav1d | avif-decode BSD-3-Clause; rav1d BSD-2-Clause; `avif-parse` 2.1.0 **MPL-2.0** (flag: weak, file-level copyleft, GPL-3 compatible); `yuv` 0.8.19 BSD-3-Clause OR Apache-2.0. rav1d's x86_64 assembly needs **NASM at build time** (flag: this undoes Cycle 1's "building Ferry never needs NASM" for x86_64; open question 3) |
| Encode | `ravif` 0.13.0 on `rav1e` 0.8.1, `default-features = false, features = ["threading"]` (no assembly, so no NASM for the encoder) | ravif BSD-3-Clause; rav1e BSD-2-Clause; avif-serialize BSD-3-Clause. **AV1 patents** (flag, low): AOMedia gives a royalty-free licence; Sisvel runs a pool aimed at devices |
| Colour | ravif writes sRGB (nclx) only. A source with an ICC profile is converted to sRGB with `moxcms` 0.9.1 (BSD-3-Clause OR Apache-2.0, pure Rust) before encoding | — |

`dav1d` 0.11.1 (MIT bindings) and `libavif` 0.14 (2024, stale) need system or CMake builds, so they are rejected. The `image` crate is **not** used by `ferry-core`: it is only in the lock file through Tauri, and its AVIF decode needs system dav1d.

### TIFF

- `tiff` 0.11.3 (MIT, pure Rust; default features `deflate`, `lzw`, `jpeg` via `zune-jpeg`, `fax`). It decodes every page (8/16-bit Gray, GrayA, RGB, RGBA, CMYK; None/LZW/Deflate/PackBits/JPEG/CCITT) and encodes multi-page TIFF with Deflate + horizontal predictor.
- **Optimise TIFF** is lossless: every page is re-encoded with Deflate. Pixels, ICC (tag 34675), resolution (282/283/296) and orientation (274) are kept.
- **Default output** (R§2.3) is JPEG for a single-page TIFF. A multi-page TIFF is never converted by default (that would silently drop pages). Converting one by hand takes the first page, and the chip says "First page only".
- Camera RAW files that are TIFF inside are not treated as TIFF: Canon CR2 (`CR` at byte 8), DNG (tag 50706) or anything with `SubIFDs` (tag 330).

### Animated WebP

- **Decode:** `webp` 0.3.1 `AnimDecoder` (already linked). It gives full-canvas RGBA frames with end timestamps, the loop count and the background colour.
- **Encode:** Ferry's own thin wrapper over `libwebp-sys` 0.9's `WebPAnimEncoder` (libwebp BSD-3-Clause, already vendored through `webp`). The `webp` crate's `AnimEncoder` flushes with timestamp 0, which loses the last frame's duration.
  - It encodes lossy at the recipe's quality, `method` 4, `allow_mixed = 1`, and keeps the loop count.
  - ICCP (always) and EXIF/XMP (when not stripping) are re-inserted into the RIFF.
- **Optimise:** an animated WebP is re-encoded instead of being left alone. If the result does not have the same frame count, it fails and keeps the original (R§2.4 animation safety net).
- **Edits:** crop and downscale apply to every frame.
- **Converting:** to PNG, JPEG, AVIF or HEIC takes the first frame. **GIF → WebP keeps the animation.** GIF frames are composited in Rust from the `gif` 0.14 frames, honouring disposal. The GIF chip still uses gifsicle, so GIF → GIF stays exactly as it is.

---

## Global Constraints

**Scope**
- **Out of scope:** secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP automation.
- **Also out of scope for this stream** (owned elsewhere or deferred; see open questions):
  - pipelines and `pipeline_steps.rs` (stream F);
  - automatic optimisation of copied PDF files (FR-T3);
  - adaptive DPI (stream F defines "adaptive");
  - HDR JPEG output;
  - HEIC encoding on Windows;
  - JPEG XL;
  - APNG, animated AVIF and HEIC sequences (they stay "can't be edited yet" and optimise to Already optimal).
- **The brief is binding.** Where it and the requirements disagree, the brief wins.

**Shared files**
- Additions go at the named anchors in the table below. Mark each with `sE pdf-formats` where the language allows a comment (`// sE pdf-formats`, `<!-- sE pdf-formats -->`).
- Never reorder or reformat existing lines. Where an existing line must change, the table says so.

**Recipe model**
- Every new behaviour is a field of `edit::Edits`, applied by `Job::run` to the backed-up original. There is no parallel edit model.
- Every run starts from the backup; repeating an edit never compounds (R§2.4).
- Plain optimise (recipe has only `level`) keeps Cycle 1's never-worse rule (`optimise::classify`). Any explicit edit always writes its result, even if larger. A default conversion (HEIC/TIFF → JPEG) is an explicit edit.

**Data safety**
- Outputs are written to a temporary name, then renamed into place (`fsutil::atomic_replace` for files; `<folder>.ferry-tmp/` for a pages folder).
- A failure or a cancel leaves the source and the previous result as they were. Files Ferry made go to the Trash, never deleted.
- **Animation safety net:** an optimisation or edit that would change an animation's frame count fails and keeps the original.

**Fixed values**
- **Formats:** `Format::{Png, Jpeg, Gif, WebP, Tiff, Heic, Avif, Pdf}`. Serde names `"tiff" "heic" "avif" "pdf"`, extensions `tiff heic avif pdf`, labels `TIFF HEIC AVIF PDF`.
- **DPI:**
  - stops `[300, 250, 200, 150, 100, 72, 48]`; defaults `pdf.dpi = 150`, `pdf.dpi_aggressive = 100`, `pdf.extract_dpi = 150`;
  - recipe DPI is clamped to 36..=1200;
  - an image is resampled only when its DPI is above `target × 1.1`.
- **PDF limits:**
  - encrypted PDFs (any `/Encrypt`) are refused with `pdf.error.encrypted`;
  - signed PDFs (`/AcroForm` `/SigFlags` bit 1, or a `/FT /Sig` field with `/V`) are refused for PDF output with `pdf.error.signed`. Extracting their pages is allowed;
  - `pdf.max_size_mb = 500`;
  - lopdf loads with `max_decompressed_size = 1 GiB`.
- **Licensing:**
  - never link libheif, libde265, x265, MuPDF or Ghostscript. HEIC goes through `oscodec` only;
  - PDFium is loaded at run time from the bundled library (`pdf::render::init`), never statically linked.
- **Sizes:**
  - pixel ceilings reuse `optimise::png::MAX_PIXELS` (150 000 000) and `edit::MAX_SIDE` (30 000);
  - decoded animation frames are capped at `webp_anim::MAX_ANIMATION_BYTES = 1_000_000_000`.
- **Settings:** two new sections, `[pdf]` and `[formats]`, each `#[serde(default)]` with a flattened `extra` map. `settings::VERSION` stays `2`.
- **i18n:** every new string is under `pdf.*` (also for TIFF, HEIC, AVIF and WebP strings, per the brief). All of them go in one block inserted in `src/lib/i18n/en.json` directly after the line `"card.first_frame": "First frame only",`. The first task that adds a key creates the block; later tasks append to its end, keeping each key's line in the order the tasks add them.
- **Dependencies:** exactly those in the table below. A `Cargo.lock` merge conflict is resolved by taking the other side's lock and running `cargo build -p ferry-core` (never `cargo update`), so cargo re-adds this stream's packages.

**Code rules**
- `ferry-core` stays free of Tauri and UI dependencies. ImageIO and WIC are codec APIs and are allowed only inside `oscodec/macos.rs` and `oscodec/windows.rs`, behind `cfg`.
- Every module opens with a `//!` doc comment and ends with `#[cfg(test)] mod tests`. Match the surrounding naming and comment density.
- Error text is one line that says what to do (C1§6). UI text lives in `en.json`, read in Rust with `crate::i18n::{t, tf}`.
- **Windows code:**
  - `crates/ferry-core/src/oscodec/windows.rs` and the `cfg(windows)` arms in `src-tauri/src/platform/windows_clipboard.rs` and `src-tauri/src/lib.rs` cannot be compiled on the Mac;
  - CI compiles and tests them (`core (windows-latest)` and `bundle (windows)`);
  - write them as specified and say "CI-verified only, not run on a Windows desktop" in the commit message.
- **JavaScript/TypeScript:** never use `var`.
- `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check` and `pnpm test` are clean after every task.

**Working rules**
- **Live app:** ask the user before running `pnpm tauri dev` (it takes over the real clipboard). Use a TextEdit window as the focus target; don't click into other apps. Every manual check is run by, or with the permission of, the user.
- **Git:** work on branch `feat/c3-sE-pdf-formats` in its own worktree from `main`. Never push, push tags, merge, or delete branches or worktrees without asking. End each commit message with the attribution lines your session's instructions give (the handover rule is `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).
- **Environment**, from the worktree root before any command:
  ```bash
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  export FERRY_PDFIUM="$PWD/src-tauri/binaries/libpdfium.dylib"   # after Task 12
  ```
  - A new worktree needs the git-ignored `src-tauri/binaries/` copied from the main checkout.
  - `cargo test` takes one name filter per command.
  - The app crate's tests and the workspace clippy need a built frontend: run `pnpm build` once.
  - From Task 9 on, x86_64 builds need NASM. The Mac is Intel: `brew install nasm` (user action; ask first).
  - From Task 12 on, `src-tauri` needs `scripts/fetch-pdfium.sh` to have run once.

### New dependencies (every one)

| Where | Dependency | Version | Licence | Task |
| --- | --- | --- | --- | --- |
| `ferry-core` | `lopdf` | `0.45` (`default-features = false`) | MIT | 12 |
| `ferry-core` | `pdfium-render` | `0.9.4` (`default-features = false, features = ["pdfium_7881", "thread_safe"]`) | MIT OR Apache-2.0 | 12 |
| `ferry-core` | `tiff` | `0.11` (0.11.3, default features) | MIT | 5 |
| `ferry-core` | `avif-decode` | `3.0` (`default-features = false`) → `rav1d` 1.1.0, `avif-parse` 2.1.0, `yuv` 0.8.19, `imgref` 1.12 | BSD-3-Clause → BSD-2-Clause, **MPL-2.0** (flag), BSD-3-Clause OR Apache-2.0, CC0-1.0 OR Apache-2.0 | 9 |
| `ferry-core` | `ravif` | `0.13` (`default-features = false, features = ["threading"]`) → `rav1e` 0.8.1, `avif-serialize` 0.8, `loop9` 0.1 | BSD-3-Clause → BSD-2-Clause, BSD-3-Clause, MIT | 9 |
| `ferry-core` | `moxcms` | `0.9` (0.9.1) | BSD-3-Clause OR Apache-2.0 | 9 |
| `ferry-core` | `libwebp-sys` | `0.9` (0.9.6, already locked through `webp`) | MIT (vendored libwebp BSD-3-Clause) | 10 |
| `ferry-core` | `flate2` | `1.1` (1.1.10, already locked) | MIT OR Apache-2.0 | 14 |
| `ferry-core` | `rayon` | `1.12` (1.12.0, already locked) | MIT OR Apache-2.0 | 14 |
| `ferry-core` macOS | `objc2` `0.6` (0.6.5 locked), `objc2-core-foundation` `0.3.2` (locked), `objc2-core-graphics` `0.3.2` (locked), `objc2-image-io` `0.3.2` (new) | — | MIT / Zlib OR Apache-2.0 OR MIT | 7 |
| `ferry-core` Windows | `windows` | `0.62` (0.62.2, already locked through Tauri) | MIT OR Apache-2.0 | 8 |
| binary | PDFium `chromium/7881` from bblanchon/pdfium-binaries: `pdfium-mac-univ.tgz` (SHA-256 `df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3`), `pdfium-win-x64.tgz` (SHA-256 `73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08`) | 151.0.7881.0 | BSD-3-Clause / Apache-2.0 + third-party notices (see Engine decisions) | 12 |
| build tool | NASM (x86_64 only) and CI action `ilammy/setup-nasm@v1` | NASM 2.16+ | BSD-2-Clause; action MIT | 9 |
| npm | none | — | — | — |

### Shared-file edits (all of them)

| File | Anchor | Edit | Task |
| --- | --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | `pub mod formats;` after `pub mod edit;`; `pub mod isobmff;` after `pub mod i18n;`; `pub mod oscodec;` after `pub mod optimise;`; `pub mod pdf;` and `pub mod poster;` after `pub mod output;` | module lines | 1, 3, 4, 12 |
| `crates/ferry-core/src/settings.rs` | `pub pdf: Pdf,` and `pub formats: Formats,` after `pub hotkeys: Hotkeys,` in `struct Settings`; matching lines after `hotkeys: Hotkeys::default(),` in `impl Default for Settings`; new structs immediately before `/// How \`Settings::load\` found the file.`; one call `self = self.sanitised_pdf_formats();` immediately before `self.version = self.version.max(VERSION);`; tests appended at the end of `mod tests` | 2 |
| `src-tauri/src/lib.rs` | `mod pdf_commands;` after `mod windows;`; handler entries after `commands::pick_folder,`; one `if let` line after `windows::create_stack(app)?;`; `fn pdfium()` after `fn gifsicle()` | 12, 13, 15, 18, 19 |
| `src-tauri/src/commands.rs` | none (PDF commands live in the new `pdf_commands.rs`) | — | — |
| `src-tauri/src/menu.rs` | `const FORMATS` line replaced by `Format::ALL` (Task 1). New `Action` variants after `Close,`. New `NAMED` entry and match arms at the end of each `match`. In `build()`, one `let` before `Menu::with_items(` and one block after it (Task 17). `convert_formats` body replaced (Task 3) | 1, 3, 16, 17 |
| `src-tauri/tauri.conf.json`, `src-tauri/capabilities/*` | none (bundling uses the new `tauri.macos.conf.json` / `tauri.windows.conf.json` overlays) | — | 12 |
| `src/lib/i18n/en.json` | the `pdf.*` block after `"card.first_frame": "First frame only",` | append keys | 1–19 |
| `src/stack/Card.svelte` | the thumbnail `<img src=…>` line (`card.poster ?? card.thumb_path`); page badge after the `.thumb` div; the progress `<circle … stroke-dasharray="40 151" />` line; the DPI rail button after the `{#if rail.includes('downscale')}` block; `SliderKind`, `slideLabel`, `slideStops`, `slideYs`, `slideLabelled`, `step`, `tipText` (DPI branches); `formatTip` (first-page tip) | 3, 4, 16, 17 |
| `src/stack/Stack.svelte` | `{#if open === 'crop'}` becomes `{#if open === 'crop' && card.source_format === 'pdf'}` + `PdfCropPopover`, then `{:else if open === 'crop'}`; a `{:else if open === 'pages'}` branch after the fit branch; the `clickAway` condition uses `isPopover(tool.kind)`; two `import` lines | 15, 16 |
| `src/lib/ipc.ts` (the brief's "`src/lib/types.ts`"; that file does not exist, and `ipc.ts` holds the types) | `Format` union; `ConvertChoice`, `PageRange` types after `Size`; new `Edits` and `JobUpdate` fields at the end of each interface; `'pages'` in `CardAction.action` | 1, 3, 16 |
| `Cargo.lock` | regenerated by cargo | — | 5, 7, 8, 9, 10, 12, 14 |

These files are not on the brief's list but other streams may touch them, so they are also edited only at named spots:
- `crates/ferry-core/src/pipeline.rs` (see Task 3, Task 13);
- `crates/ferry-core/src/job.rs` (new `JobUpdate` fields at the end of the struct);
- `crates/ferry-core/src/sniff.rs`;
- `src-tauri/src/clipboard.rs` (one call in `start_clipboard_job`);
- `src-tauri/src/compare.rs` (an `// sE pdf-formats` section at the end of the file before `#[cfg(test)]`);
- `src-tauri/src/hover_keys.rs` (the `Key::StepDown` arm);
- `.github/workflows/{ci,release}.yml` (steps after "Build gifsicle sidecar");
- `src/lib/schema.ts`, `src/lib/settings.ts`, `src/lib/settings.fixture.ts` (after the `hotkeys` entries).

### Cross-stream notes

- **Pipelines (stream F) register PDF steps later; E adds no pipeline code.** F maps its steps onto this stream's recipe through `jobs::edit` / `Job::edit`:
  - `optimise(dpi)` → `EditsPatch { dpi: Some(Some(d)) }`;
  - `crop` on a PDF → `EditsPatch { aspect, uncrop }`;
  - `extractPagesAsImages` → `EditsPatch { format: Some(Some(Png | Jpeg)), pages: Some(Some(PageRange { first, last })), dpi }`;
  - convert to TIFF, HEIC or AVIF → `EditsPatch { format }`.

  `formats::choices` says which targets a source accepts. F's step-2 hook goes in `pipeline_steps.rs`, beside its image steps.
- **Streams A and B:**
  - the `JobUpdate.poster` field and `poster::needs_poster` are generic, and A can reuse them for a video's first frame;
  - new `Format` variants from A or B go after this stream's block in `sniff.rs`;
  - `RunOptions` gains `pdf_output`, so A and B add their own `*_output` the same way.
- **Stream I** rewrites `jobs::copy` and touches `clipboard.rs`. This stream changes only one line there (`.with_default_conversion(…)` in `start_clipboard_job`).

## Review Focus

1. **A signed PDF** (a filled-in contract). Rewriting it would invalidate the signature. Expected: optimise, crop and DPI refuse with "This PDF is signed — changing it would break the signature", and the file is untouched; extracting its pages still works. Pinned in Task 13 (`a_signed_pdf_is_never_rewritten`) and Task 16 (`a_signed_pdf_can_still_be_extracted`).
2. **One image drawn at different sizes** (a logo on every page, a figure shown full-page and as a thumbnail). Expected: the image is downsampled for its largest drawing, so no use falls below the chosen DPI. Pinned in Task 14 (`a_shared_image_keeps_the_dpi_of_its_largest_use`).
3. **Camera RAW files that are TIFF inside** (DNG, CR2, NEF). Expected: Ferry does not treat them as TIFF, and never "converts" them to a JPEG of the embedded thumbnail. Pinned in Task 1 (`camera_raw_files_are_not_tiff`).
4. **A multi-page TIFF** (a fax or scan). Expected: no default conversion, so pages are never dropped silently. Optimising keeps every page, and converting by hand says "First page only". Pinned in Task 3 (`default_targets_follow_the_settings`) and Task 6 (`optimising_keeps_every_page_and_every_pixel`).
5. **HEIC on a Windows PC without Microsoft's HEIF/HEVC extensions.** Expected: an Error card that says which extensions to install, and the original untouched. Pinned in Task 8 (`a_missing_windows_codec_says_what_to_install`, Windows CI). A Windows-desktop check is listed as unverified in Task 20.

---

## Setup (once, before Task 1)

```bash
git worktree add ../ferry-sE -b feat/c3-sE-pdf-formats main
cd ../ferry-sE
cp -R ../ferry/src-tauri/binaries src-tauri/
pnpm install --frozen-lockfile && pnpm build
```

Then the environment block from Global Constraints.

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/sniff.rs` (modify) | new `Format` variants, `Format::ALL`, TIFF/ISOBMFF/PDF sniffing, RAW rejection, TIFF IFD reading, dimensions | 1 |
| `crates/ferry-core/src/isobmff.rs` (create) | HEIF/AVIF `ftyp` brands, sequences, primary `ispe` size | 1 |
| `crates/ferry-core/src/formats.rs` (create) | `ConvertChoice`, `choices`, `default_target` | 3 |
| `crates/ferry-core/src/oscodec/{mod,macos,windows}.rs` (create) | OS image codecs (HEIC) | 3, 7, 8 |
| `crates/ferry-core/src/poster.rs` (create) | poster PNGs for formats the webviews can't show; compare's `viewable` | 4, 18 |
| `crates/ferry-core/src/optimise/tiff.rs` (create) | TIFF decode, lossless optimise, encode | 5, 6 |
| `crates/ferry-core/src/optimise/heic.rs` (create) | HEIC optimise through `oscodec` | 7 |
| `crates/ferry-core/src/optimise/avif.rs` (create) | AVIF decode, encode (sRGB), optimise | 9 |
| `crates/ferry-core/src/optimise/webp_anim.rs` (create) | animated WebP decode / encode / optimise / frame edits | 10, 11 |
| `crates/ferry-core/src/optimise/gif_frames.rs` (create) | GIF → composited frames | 11 |
| `crates/ferry-core/src/optimise/{mod,jpeg,webp}.rs` (modify) | encode dispatch; `jpeg::encode_gray`; animated WebP routing | 1, 5–11, 14 |
| `crates/ferry-core/src/edit/{mod,raster}.rs` (modify) | `Edits.{dpi,pages,uncrop}`; new `Work` variants; decode/encode arms | 1, 3, 5–11 |
| `crates/ferry-core/src/pdf/mod.rs` (create) | `Defaults`, `Facts`, `facts`, `optimise`, `apply`, `PdfResult`, `target_dpi`, `DPI_STOPS` | 12–16 |
| `crates/ferry-core/src/pdf/render.rs` (create) | PDFium binding and page rendering | 12 |
| `crates/ferry-core/src/pdf/images.rs` (create) | image DPI analysis, recompress, downsample | 14 |
| `crates/ferry-core/src/pdf/crop.rs` (create) | crop box maths, crop and uncrop | 15 |
| `crates/ferry-core/src/pipeline.rs` (modify) | `with_default_conversion`, new `Job` fields, `RunOptions.{pdf_output,pdf}`, `place_result` extracted from `put`, PDF branch | 3, 4, 13, 16 |
| `crates/ferry-core/src/pipeline_formats.rs` (create; child module of `pipeline`) | `capture_facts`, `finish_pdf`, `put_pages`, `pdf_detail` | 4, 13, 16 |
| `crates/ferry-core/src/{job,settings,output,trigger,bitmap,testutil}.rs` (modify) | `JobUpdate` fields; `[pdf]` `[formats]`; `example_named`; `tif`/`heif` aliases; bitmap arms; fixtures | 1–19 |
| `scripts/fetch-pdfium.sh` (create), `src-tauri/tauri.{macos,windows}.conf.json` (create), `.github/workflows/{ci,release}.yml` (modify) | PDFium bundling; NASM | 9, 12 |
| `src-tauri/src/{lib,pdf_commands,compare,menu,hover_keys,clipboard}.rs`, `platform/{macos,windows}_clipboard.rs` (modify/create) | app wiring | 1, 3, 6, 7, 9, 12–19 |
| `src/lib/{ipc,edits,cards,pdf,pdfConfig.svelte,schema,settings,settings.fixture,card.fixture}.ts` | types and pure helpers | 1, 2, 3, 15–19 |
| `src/stack/{Card,Stack,FormatBar,PdfCropPopover,PagesPopover}.svelte`, `src/compare/{Compare.svelte,view.ts}`, `src/settings/{Row.svelte,controls/OutputRule.svelte,controls/DeviceList.svelte}` | UI | 3, 4, 15–19 |

---

## Foundation

### Task 1: TIFF, HEIC, AVIF and PDF are recognised

**Files:**
- Modify: `crates/ferry-core/src/sniff.rs`
- Create: `crates/ferry-core/src/isobmff.rs`
- Modify: `crates/ferry-core/src/lib.rs` (`pub mod isobmff;` after `pub mod i18n;`)
- Modify: `crates/ferry-core/src/optimise/mod.rs` (`encode` arms)
- Modify: `crates/ferry-core/src/edit/raster.rs` (`decode`, `encode`, `display_dims` arms)
- Modify: `crates/ferry-core/src/pipeline.rs` (step-label `match` in `run_recipe`)
- Modify: `crates/ferry-core/src/bitmap.rs` (`decode_rgba` arm)
- Modify: `crates/ferry-core/src/trigger.rs` (`normalise_ext`)
- Modify: `crates/ferry-core/src/testutil.rs`
- Modify: `src-tauri/src/platform/macos_clipboard.rs` (`fn uti`)
- Modify: `src-tauri/src/platform/windows_clipboard.rs` (`fn format_name`; `cfg(windows)`, CI-verified only)
- Modify: `src-tauri/src/menu.rs` (`FORMATS`)
- Modify: `src/lib/ipc.ts`
- Modify: `src/lib/edits.ts` (`FORMAT_LABELS`)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: nothing new.
- Produces:
  - `sniff::Format::{Tiff, Heic, Avif, Pdf}`, added after `WebP,` under a `// sE pdf-formats` comment.
    - `extension()`: `"tiff" "heic" "avif" "pdf"`.
    - `label()`: `"TIFF" "HEIC" "AVIF" "PDF"`.
    - serde: `"tiff" "heic" "avif" "pdf"`.
  - `Format::ALL: [Format; 8]` = `[Png, Jpeg, Gif, WebP, Tiff, Heic, Avif, Pdf]`.
  - `sniff::tiff_page_count(bytes: &[u8]) -> u32`: 0 when not a classic TIFF.
  - `sniff::tiff_orientation(bytes: &[u8]) -> u16`: IFD0 tag 274, else 1.
  - `isobmff::brands(bytes: &[u8]) -> Option<([u8; 4], Vec<[u8; 4]>)>`: the major brand and the compatible brands.
  - `isobmff::image_format(bytes: &[u8]) -> Option<Format>`.
  - `isobmff::is_sequence(bytes: &[u8]) -> bool`.
  - `isobmff::primary_size(bytes: &[u8]) -> Option<(u32, u32)>`.
  - `testutil::ftyp(major: &[u8; 4], compatible: &[&[u8; 4]]) -> Vec<u8>`.
  - `testutil::heif(major: &[u8; 4], sizes: &[(u32, u32)], irot: Option<u8>) -> Vec<u8>`: `ftyp` + `meta` → `iprp` → `ipco` holding one `ispe` per size and an optional `irot`.
  - `testutil::tiff_ifds(pages: &[(u32, u32)], extra_tag: Option<u16>) -> Vec<u8>`: a little-endian TIFF header and one IFD per page, holding tags 256/257 (LONG) and `extra_tag` (SHORT 1). No pixel data.

**Sniff rules**, checked after the existing four, in this order:
1. **TIFF:** `II*\0`, `MM\0*` or BigTIFF `II+\0`/`MM\0+` → `Tiff`, unless the file is camera RAW: bytes 8..10 are `CR`, or IFD0 holds tag 50706 (DNGVersion) or 330 (SubIFDs). RAW → `None`.
2. **ISOBMFF:** a box at offset 0 of type `ftyp`. Brands: any `avif` or `avis` → `Avif`; else any of `heic heix heim heis hevc hevx mif1 msf1` → `Heic`; else `None` (an MP4 is not an image).
3. **PDF:** `%PDF-` anywhere in the first 1024 bytes → `Pdf`.

**IFD reading** (classic TIFF only; BigTIFF gives page count 1 and no tags):
- Byte order from bytes 0..2. The first IFD offset is u32 at 4.
- Each IFD is a u16 entry count, 12-byte entries (tag u16, type u16, count u32, value u32; a SHORT value sits in the entry's first two value bytes in the file's byte order), then the next IFD's u32 offset.
- Stop at offset 0, an offset out of range, an offset already seen, or after 10 000 IFDs.

**Other rules:**
- `is_animated`: `Tiff` and `Pdf` → false; `Heic` and `Avif` → `isobmff::is_sequence` (major brand `avis`, `msf1`, `hevc` or `hevx`).
- `dimensions`: `Tiff` → IFD0 tags 256/257; `Heic`/`Avif` → `isobmff::primary_size`; `Pdf` → `None`.
- `primary_size`: walk `meta` (a full box: skip 4 bytes of version/flags) → `iprp` → `ipco`, and take the largest `ispe` by area. Swap the sides when an `irot` with angle 1 or 3 is present.
- `raster::display_dims`: for `Tiff`, swap when `tiff_orientation` is 5..=8.

- [ ] **Step 1: Write the failing tests** in `sniff.rs` `mod tests` and a new `isobmff.rs` `mod tests`

```rust
#[test]
fn detects_tiff_in_both_byte_orders() {
    assert_eq!(sniff(&testutil::tiff_ifds(&[(4, 3)], None)), Some(Format::Tiff));
    let mut be = b"MM\0*\0\0\0\x08".to_vec();
    be.extend_from_slice(&[0, 0, 0, 0, 0, 0]);
    assert_eq!(sniff(&be), Some(Format::Tiff));
}

#[test]
fn camera_raw_files_are_not_tiff() {
    assert_eq!(sniff(b"II*\0\x10\0\0\0CR\x02\0\0\0\0\0"), None);
    assert_eq!(sniff(&testutil::tiff_ifds(&[(4, 3)], Some(50706))), None);
    assert_eq!(sniff(&testutil::tiff_ifds(&[(4, 3)], Some(330))), None);
}

#[test]
fn detects_heic_and_avif_by_brand() {
    assert_eq!(sniff(&testutil::ftyp(b"heic", &[b"mif1", b"heic"])), Some(Format::Heic));
    assert_eq!(sniff(&testutil::ftyp(b"avif", &[b"mif1", b"miaf"])), Some(Format::Avif));
    assert_eq!(sniff(&testutil::ftyp(b"mif1", &[b"avif"])), Some(Format::Avif));
    assert_eq!(sniff(&testutil::ftyp(b"isom", &[b"mp41"])), None);
}

#[test]
fn detects_pdf_even_after_leading_bytes() {
    assert_eq!(sniff(b"%PDF-1.7\n%\xE2\xE3"), Some(Format::Pdf));
    let mut junk = vec![b' '; 100];
    junk.extend_from_slice(b"%PDF-1.4\n");
    assert_eq!(sniff(&junk), Some(Format::Pdf));
    let mut late = vec![b' '; 1100];
    late.extend_from_slice(b"%PDF-1.4\n");
    assert_eq!(sniff(&late), None);
}

#[test]
fn image_sequences_are_animated() {
    assert!(is_animated(&testutil::ftyp(b"avis", &[b"avif"]), Format::Avif));
    assert!(is_animated(&testutil::ftyp(b"msf1", &[b"heic"]), Format::Heic));
    assert!(!is_animated(&testutil::ftyp(b"heic", &[b"mif1"]), Format::Heic));
    assert!(!is_animated(&testutil::tiff_ifds(&[(4, 3), (4, 3)], None), Format::Tiff));
}

#[test]
fn every_format_has_a_name_an_extension_and_a_serde_name() {
    let names: Vec<_> = Format::ALL.iter().map(|f| serde_json::to_value(f).unwrap()).collect();
    assert_eq!(names, ["png", "jpeg", "gif", "webp", "tiff", "heic", "avif", "pdf"]);
    assert_eq!(
        (Format::Tiff.extension(), Format::Heic.label(), Format::Pdf.extension()),
        ("tiff", "HEIC", "pdf")
    );
}

#[test]
fn tiff_size_pages_and_orientation_come_from_the_ifds() {
    let two = testutil::tiff_ifds(&[(40, 30), (20, 10)], None);
    assert_eq!(dimensions(&two, Format::Tiff), Some((40, 30)));
    assert_eq!(tiff_page_count(&two), 2);
    assert_eq!(tiff_orientation(&two), 1);
    assert_eq!(tiff_page_count(b"not a tiff"), 0);
}

// isobmff.rs
#[test]
fn the_largest_ispe_is_the_primary_size_and_irot_turns_it() {
    let thumb_and_full = testutil::heif(b"heic", &[(8, 6), (64, 48)], None);
    assert_eq!(primary_size(&thumb_and_full), Some((64, 48)));
    assert_eq!(primary_size(&testutil::heif(b"heic", &[(64, 48)], Some(1))), Some((48, 64)));
    assert_eq!(primary_size(&testutil::ftyp(b"heic", &[])), None);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core sniff` then `cargo test -p ferry-core isobmff`
Expected: compile errors (`Format::Tiff`, `testutil::ftyp`, `isobmff` not defined).

- [ ] **Step 3: Implement the variants, sniff rules, `isobmff` and the test helpers as specified above**

The compiler then lists every non-exhaustive `match`. Fill each one:

| Site | Arm |
| --- | --- |
| `optimise::encode` | new formats → `Err(OptimiseError::Unsupported(tf("pdf.read_unsupported", &[("format", format.label())])))` |
| `raster::decode` | the same |
| `raster::encode` | `Tiff \| Heic \| Avif \| Pdf` → `Unsupported(tf("pdf.write_unsupported", …))` |
| step label in `run_recipe` | `(true, Format::Tiff \| Format::Heic \| Format::Avif \| Format::Pdf) => "card.step.edit"` (Tasks 6, 7, 9 and 13 replace it) |
| `bitmap::decode_rgba` | `Err(format!("no bitmap for {}", format.label()))` |
| `macos_clipboard.rs` `fn uti` | rewrite as an exhaustive `match`: the four existing UTIs plus `Tiff → "public.tiff"`, `Heic → "public.heic"`, `Avif → "public.avif"`, `Pdf → "com.adobe.pdf"`. `IMAGE_TYPES` is unchanged in this task |
| `windows_clipboard.rs` `fn format_name` | `Tiff → "image/tiff"`, `Heic → "image/heic"`, `Avif → "image/avif"`, `Pdf → "application/pdf"` |

Further edits:
- `menu.rs`: replace `const FORMATS: [Format; 4] = …;` with `const FORMATS: [Format; 8] = Format::ALL;`.
- `trigger::normalise_ext`: also maps `"tif"` → `"tiff"` and `"heif"` → `"heic"` (add one test `tif_and_heif_count_as_tiff_and_heic`).
- `src/lib/ipc.ts`: `export type Format = 'png' | 'jpeg' | 'gif' | 'webp' | 'tiff' | 'heic' | 'avif' | 'pdf';`.
- `src/lib/edits.ts`: `FORMAT_LABELS` gains `tiff: 'TIFF', heic: 'HEIC', avif: 'AVIF', pdf: 'PDF'`.

Create the `pdf.*` block in `en.json`:

```json
  "pdf.read_unsupported": "Ferry can't open {format} files on this computer",
  "pdf.write_unsupported": "Ferry can't save {format} files on this computer",
```

- [ ] **Step 4: Run the tests and the checks**

Run: `cargo test -p ferry-core sniff`, `cargo test -p ferry-core isobmff`, `cargo test -p ferry-core trigger`, `cargo test -p ferry-core` (all), `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check`, `pnpm test`
Expected: all PASS, no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core src-tauri/src src/lib
git commit -m "feat(core): recognise TIFF, HEIC, AVIF and PDF by content"
```

The message also states that `windows_clipboard.rs` is CI-verified only.

---

### Task 2: `[pdf]` and `[formats]` settings sections

**Files:**
- Modify: `crates/ferry-core/src/settings.rs` (anchors in the shared-file table)
- Modify: `src/lib/settings.ts`, `src/lib/settings.fixture.ts` (after the `hotkeys` entries)

**Interfaces:**
- Consumes: `settings::{Output, OutputRule, Location}`, `edit::MAX_SIDE`, `sniff::Format`, `template::validate`.
- Produces:
  - `settings::Pdf` (`#[serde(default)]`, flattened `extra`):
    - `dpi: u16` (150), `dpi_aggressive: u16` (100), `extract_dpi: u16` (150), `max_size_mb: u32` (500);
    - `output: Output` (optimised `{ InPlace, "~/Documents/Ferry", "%f_optimised.%e" }`, converted `{ SameFolder, "~/Documents/Ferry", "%f.%e" }`);
    - `devices: Vec<Device>`.
  - `settings::Device { name: String, width: u32, height: u32 }` (serde default). The default list, in order:

    | Name | Size |
    | --- | --- |
    | `"iPhone"` | 1179×2556 |
    | `"iPhone Pro Max"` | 1320×2868 |
    | `"iPad"` | 1640×2360 |
    | `"iPad Pro 13-inch"` | 2064×2752 |
    | `"Kindle"` | 1236×1648 |
    | `"reMarkable"` | 1404×1872 |

  - `settings::Formats` (`#[serde(default)]`, flattened `extra`): `heic_to: ConvertTo` (`Jpeg`), `tiff_to: ConvertTo` (`Jpeg`), `avif_to: ConvertTo` (`Keep`).
  - `settings::ConvertTo { Keep, Jpeg, Png, WebP }` (serde snake_case `"keep" "jpeg" "png" "web_p"`; add `#[serde(rename = "webp")]` on `WebP`), with `ConvertTo::format(self) -> Option<Format>`.
  - `Settings.pdf: Pdf`, `Settings.formats: Formats`.
  - `Settings::sanitised_pdf_formats(self) -> Self`, called from `sanitised` at the anchor:
    - the three DPI values are clamped to `36..=1200`;
    - `max_size_mb` is at least 1;
    - the PDF output templates and folders get the same repair as `images.output` (an invalid template → the `Pdf::default()` one);
    - devices are trimmed, and a device with an empty name or a side outside `1..=MAX_SIDE` is dropped.
  - TS: `Settings.pdf: { dpi: number; dpi_aggressive: number; extract_dpi: number; max_size_mb: number; output: { optimised: OutputRule; converted: OutputRule }; devices: Device[] }`, `Settings.formats: { heic_to: ConvertTo; tiff_to: ConvertTo; avif_to: ConvertTo }`, `export type ConvertTo = 'keep' | 'jpeg' | 'png' | 'webp'`, `export interface Device { name: string; width: number; height: number }`. `sample` in `settings.fixture.ts` mirrors the Rust defaults.

- [ ] **Step 1: Write the failing tests** (end of `settings.rs` `mod tests`)

```rust
#[test]
fn pdf_and_format_defaults() {
    let s = Settings::default();
    assert_eq!((s.pdf.dpi, s.pdf.dpi_aggressive, s.pdf.extract_dpi, s.pdf.max_size_mb), (150, 100, 150, 500));
    assert_eq!(s.pdf.output.optimised.location, Location::InPlace);
    assert_eq!(s.pdf.output.optimised.template, "%f_optimised.%e");
    assert_eq!(s.pdf.output.converted.location, Location::SameFolder);
    assert_eq!(s.pdf.output.converted.folder, "~/Documents/Ferry");
    assert_eq!(s.pdf.devices.len(), 6);
    assert_eq!((s.pdf.devices[2].name.as_str(), s.pdf.devices[2].width, s.pdf.devices[2].height), ("iPad", 1640, 2360));
    assert_eq!((s.formats.heic_to, s.formats.tiff_to, s.formats.avif_to), (ConvertTo::Jpeg, ConvertTo::Jpeg, ConvertTo::Keep));
    assert_eq!(s.version, 2);
}

#[test]
fn a_version_2_file_without_the_new_sections_gets_their_defaults() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, br#"{"version":2,"images":{"compression":70}}"#).unwrap();
    let (s, _) = Settings::load(&path);
    assert_eq!((s.pdf, s.formats, s.images.compression), (Pdf::default(), Formats::default(), 70));
}

#[test]
fn pdf_values_are_sanitised() {
    let mut s = Settings::default();
    s.pdf.dpi = 5;
    s.pdf.extract_dpi = 60_000;
    s.pdf.max_size_mb = 0;
    s.pdf.output.converted.template = "%q".into();
    s.pdf.devices.push(Device { name: "  ".into(), width: 10, height: 10, ..Device::default() });
    s.pdf.devices.push(Device { name: "Huge".into(), width: 40_000, height: 10, ..Device::default() });
    let s = s.sanitised();
    assert_eq!((s.pdf.dpi, s.pdf.extract_dpi, s.pdf.max_size_mb), (36, 1200, 1));
    assert_eq!(s.pdf.output.converted.template, "%f.%e");
    assert_eq!(s.pdf.devices.len(), 6);
}

#[test]
fn convert_to_names_a_format_or_keeps_it() {
    assert_eq!(ConvertTo::Keep.format(), None);
    assert_eq!(ConvertTo::Jpeg.format(), Some(Format::Jpeg));
    assert_eq!(serde_json::to_value(ConvertTo::WebP).unwrap(), "webp");
}

#[test]
fn the_new_sections_reach_the_ui_as_plain_json() {
    let json = serde_json::to_value(Settings::default()).unwrap();
    assert_eq!(json["pdf"]["devices"][0], json!({ "name": "iPhone", "width": 1179, "height": 2556 }));
    assert_eq!(json["formats"]["heic_to"], "jpeg");
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core settings`
Expected: FAIL (`Pdf`, `Formats`, `ConvertTo`, `Device` not defined).

- [ ] **Step 3: Implement the structs, defaults and `sanitised_pdf_formats` as specified**

Add `#[derive(Default)]`-style `impl Default` blocks matching the listed values. `Device` gets a flattened `extra` map like the other settings structs, so the test's `..Device::default()` is valid.

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core settings`, `pnpm check`, `pnpm test`
Expected: PASS. `src/lib/schema.test.ts` and `settings.test.ts` stay green with the extended fixture.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/settings.rs src/lib/settings.ts src/lib/settings.fixture.ts
git commit -m "feat(core): pdf and formats settings sections"
```

---

### Task 3: The recipe and the card learn about the new formats

**Files:**
- Modify: `crates/ferry-core/src/edit/mod.rs` (`Edits`, `EditsPatch`, `merge`, `is_plain`, tests)
- Create: `crates/ferry-core/src/formats.rs`; `crates/ferry-core/src/lib.rs` (`pub mod formats;` after `pub mod edit;`)
- Create: `crates/ferry-core/src/oscodec/mod.rs` (skeleton); `lib.rs` (`pub mod oscodec;` after `pub mod optimise;`)
- Modify: `crates/ferry-core/src/job.rs` (three `JobUpdate` fields at the end of the struct, under `// sE pdf-formats`)
- Modify: `crates/ferry-core/src/pipeline.rs`:
  - `Job` fields `poster`, `pages` at the end of the struct;
  - `Job::clipboard` and `Job::file` set `pages`;
  - `Job::file` calls `with_default_conversion`;
  - new `with_default_conversion` after `fn fork`;
  - `edit()` validation;
  - `update()` fills the new fields;
  - `fork()` copies them;
  - `notice()` sets them empty.
- Modify: `src-tauri/src/clipboard.rs` (`start_clipboard_job`: `Job::clipboard(…)` gets `.with_default_conversion(&state.settings.read().unwrap().formats)`)
- Modify: `src-tauri/src/menu.rs` (`convert_formats` body; `enabled` for `Convert`; test `a_gif_card_can_convert_back_to_gif`)
- Modify: `src/lib/ipc.ts`, `src/lib/card.fixture.ts`, `src/lib/edits.ts` (`formatChoices`), `src/lib/edits.test.ts`, `src/stack/Card.svelte` (`formatTip`), `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `settings::{Formats, ConvertTo}` (Task 2), `sniff::tiff_page_count` (Task 1).
- Produces:
  - `edit::PageRange { first: u32, last: u32 }` (Clone, Copy, PartialEq, Eq, Debug, Serialize, Deserialize), 1-based and inclusive.
  - `edit::MIN_DPI: u16 = 36`, `edit::MAX_DPI: u16 = 1200`.
  - `Edits` gains, after `level`:
    - `pub dpi: Option<u16>`: image resolution for a PDF, or render resolution for extracted pages;
    - `pub pages: Option<PageRange>`: which pages to extract; `None` = all;
    - `pub uncrop: bool`.
  - `EditsPatch` gains `dpi: Option<Option<u16>>`, `pages: Option<Option<PageRange>>` (both `deserialize_with = "present"`) and `uncrop: Option<bool>`.
  - `merge`: `dpi` is clamped to `MIN_DPI..=MAX_DPI`; `pages` with `first == 0` or `last < first` becomes `None`; `uncrop` is set.
  - `is_plain()` is also false when `dpi.is_some()` or `uncrop`. `pages` alone does not count.
  - `formats::ConvertChoice { pub format: Format, pub first_only: bool }` (Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug).
  - `formats::choices(source: Format, animated: bool, pages: u32, heic_encoder: bool) -> Vec<ConvertChoice>`:
    - `Pdf` → `[Pdf, Png, Jpeg]`;
    - otherwise `standard = [Png, Jpeg, WebP, Avif]` plus `Heic` when `heic_encoder`. The result is `source` first when `source` is not in `standard`, then `standard`;
    - `first_only` is true for a target other than the source when the source is animated (except `WebP`, which keeps animation; the source `Gif` keeps it too), or the source is `Tiff` with `pages > 1`.
  - `formats::default_target(source: Format, pages: u32, f: &settings::Formats) -> Option<Format>`: `Heic` → `f.heic_to.format()`, `Tiff` → `f.tiff_to.format()` only when `pages <= 1`, `Avif` → `f.avif_to.format()`, others `None`.
  - `oscodec::can_encode(format: Format) -> bool` (false for now; Task 7 fills it), `oscodec::decode(bytes: &[u8], format: Format) -> Result<Raster, OptimiseError>` (Unsupported for now).
  - `JobUpdate` gains `pub poster: Option<PathBuf>`, `pub pages: Option<u32>` and `pub choices: Vec<ConvertChoice>`.
  - `Job` gains `poster: Option<PathBuf>` (Task 4 fills it) and `pages: Option<u32>` (a TIFF's page count when above 1 at creation; a PDF's after its first run, in Task 13).
  - `Job::with_default_conversion(self, formats: &settings::Formats) -> Job`: sets `edits.format` to `default_target`, if that format is in `choices`. `Job::file` calls it itself.
  - `Job::edit` now drops:
    - `format` when it is the source format or not in `choices` (this replaces today's "or GIF" rule; GIF is not in a non-GIF source's choices);
    - `dpi`, `pages` and `uncrop` for a non-PDF source;
    - `size` and `scale` for a PDF source.
  - TS:
    - `ConvertChoice { format: Format; first_only: boolean }`, `PageRange { first: number; last: number }`;
    - `Edits` adds `dpi: number | null; pages: PageRange | null; uncrop: boolean`;
    - `JobUpdate` adds `poster: string | null; pages: number | null; choices: ConvertChoice[]`;
    - `FormatChoice` adds `firstPage: boolean`;
    - `formatChoices(card)` maps `card.choices` (label from `FORMAT_LABELS`, `current = card.format === format`). `firstFrame = first_only && source !== 'tiff'`, `firstPage = first_only && source === 'tiff'`.
  - Rust `menu::convert_formats(card)` returns `card.choices` formats; `enabled(Convert(f))` additionally needs `f` in `card.choices`.

- [ ] **Step 1: Write the failing tests**

`edit/mod.rs`. Also update `the_card_sees_the_recipe_as_plain_json`: its expected JSON gains `"dpi": null, "pages": null, "uncrop": false`.

```rust
#[test]
fn dpi_pages_and_uncrop_merge_and_clamp() {
    let mut e = Edits::default();
    e.merge(patch(json!({ "dpi": 10 })));
    assert_eq!(e.dpi, Some(36));
    e.merge(patch(json!({ "dpi": 5000 })));
    assert_eq!(e.dpi, Some(1200));
    e.merge(patch(json!({ "dpi": null, "pages": { "first": 2, "last": 5 }, "uncrop": true })));
    assert_eq!((e.dpi, e.pages, e.uncrop), (None, Some(PageRange { first: 2, last: 5 }), true));
    for bad in [json!({ "first": 0, "last": 2 }), json!({ "first": 3, "last": 1 })] {
        e.merge(patch(json!({ "pages": bad })));
        assert_eq!(e.pages, None);
    }
}

#[test]
fn dpi_and_uncrop_are_explicit_edits_but_a_page_range_alone_is_not() {
    assert!(!Edits { dpi: Some(150), ..Edits::default() }.is_plain());
    assert!(!Edits { uncrop: true, ..Edits::default() }.is_plain());
    assert!(Edits { pages: Some(PageRange { first: 1, last: 1 }), ..Edits::default() }.is_plain());
}
```

`formats.rs`:

```rust
fn formats(c: &[ConvertChoice]) -> Vec<Format> { c.iter().map(|c| c.format).collect() }

#[test]
fn a_png_offers_the_standard_targets() {
    assert_eq!(formats(&choices(Format::Png, false, 1, false)), [Format::Png, Format::Jpeg, Format::WebP, Format::Avif]);
    assert_eq!(formats(&choices(Format::Png, false, 1, true)), [Format::Png, Format::Jpeg, Format::WebP, Format::Avif, Format::Heic]);
    assert!(choices(Format::Jpeg, false, 1, true).iter().all(|c| !c.first_only));
}

#[test]
fn an_unusual_source_comes_first() {
    assert_eq!(formats(&choices(Format::Tiff, false, 1, false))[0], Format::Tiff);
    assert_eq!(formats(&choices(Format::Heic, false, 1, false)), [Format::Heic, Format::Png, Format::Jpeg, Format::WebP, Format::Avif]);
    assert_eq!(formats(&choices(Format::Gif, true, 1, false)), [Format::Gif, Format::Png, Format::Jpeg, Format::WebP, Format::Avif]);
}

#[test]
fn a_pdf_converts_only_to_page_images() {
    assert_eq!(formats(&choices(Format::Pdf, false, 12, true)), [Format::Pdf, Format::Png, Format::Jpeg]);
}

#[test]
fn stills_from_a_sequence_say_so() {
    let first = |c: &[ConvertChoice], f: Format| c.iter().find(|c| c.format == f).unwrap().first_only;
    let gif = choices(Format::Gif, true, 1, false);
    assert!(first(&gif, Format::Png) && first(&gif, Format::Avif));
    assert!(!first(&gif, Format::WebP) && !first(&gif, Format::Gif));
    let webp = choices(Format::WebP, true, 1, false);
    assert!(first(&webp, Format::Jpeg) && !first(&webp, Format::WebP));
    let tiff = choices(Format::Tiff, false, 3, false);
    assert!(first(&tiff, Format::Png) && !first(&tiff, Format::Tiff));
    assert!(!first(&choices(Format::Gif, false, 1, false), Format::Png));
}

#[test]
fn default_targets_follow_the_settings() {
    let f = settings::Formats::default();
    assert_eq!(default_target(Format::Heic, 1, &f), Some(Format::Jpeg));
    assert_eq!(default_target(Format::Tiff, 1, &f), Some(Format::Jpeg));
    assert_eq!(default_target(Format::Tiff, 2, &f), None);
    assert_eq!(default_target(Format::Avif, 1, &f), None);
    let png = settings::Formats { avif_to: settings::ConvertTo::Png, ..f.clone() };
    assert_eq!(default_target(Format::Avif, 1, &png), Some(Format::Png));
    assert_eq!(default_target(Format::Png, 1, &f), None);
}
```

`pipeline.rs` (use the existing `Env` / `options()` helpers):

```rust
#[test]
fn a_heic_file_starts_with_its_default_conversion() {
    let env = Env::new();
    let path = env.file("photo.heic", &testutil::heif(b"heic", &[(64, 48)], None));
    let job = Job::file(&path, &Settings::default(), true).unwrap();
    assert_eq!(job.edits.format, Some(Format::Jpeg));
    let card = job.update(JobState::Queued, None);
    assert_eq!(card.choices[0].format, Format::Heic);
    assert_eq!(card.dims_before, Some((64, 48)));
}

#[test]
fn a_multi_page_tiff_is_not_converted_by_default() {
    let env = Env::new();
    let path = env.file("fax.tiff", &testutil::tiff_ifds(&[(8, 8), (8, 8)], None));
    let job = Job::file(&path, &Settings::default(), true).unwrap();
    assert_eq!((job.edits.format, job.update(JobState::Queued, None).pages), (None, Some(2)));
}

#[test]
fn a_conversion_the_source_cannot_take_is_dropped() {
    let env = Env::new();
    let (mut job, _) = clipboard_png(&env, "%f");
    job.edit(convert(Format::Pdf));
    assert_eq!(job.edits.format, None);
    job.edit(convert(Format::Avif));
    assert_eq!(job.edits.format, Some(Format::Avif));
}

#[test]
fn pdf_only_edits_are_dropped_on_images() {
    let env = Env::new();
    let (mut job, _) = clipboard_png(&env, "%f");
    job.edit(serde_json::from_value(serde_json::json!({ "dpi": 150, "uncrop": true })).unwrap());
    assert_eq!((job.edits.dpi, job.edits.uncrop), (None, false));
}
```

Helpers used here and in later pipeline tests are the existing ones in `pipeline.rs` `mod tests`: `Env::new()`, `env.file(name, bytes)`, `env.work()`, `env.backups`, `options()`, `options_with(&settings)`, `run(&mut job, level, &o, &env)`, `scale(n)`, `convert(format)` and `clipboard_png(&env, template)`.

`menu.rs`: rewrite `a_gif_card_can_convert_back_to_gif` so `done()` carries `choices: formats::choices(Format::Png, false, 1, false)`. It asserts `convert_formats` equals those formats, and with `choices(Format::Gif, true, 1, false)` the list starts with GIF and `enabled(Convert(Gif))` holds. Add `a_format_the_card_does_not_offer_is_disabled`: `!enabled(&card, Action::Convert(Format::Pdf))` on a PNG card.

`src/lib/edits.test.ts`: replace the three `formatChoices` expectations with ones built from `card(…, { choices: [...] })`. `formatChoices(card({kind:'done'}, { source_format: 'tiff', format: 'tiff', choices: [{format:'tiff',first_only:false},{format:'png',first_only:true}] }))` gives `[{ format: 'tiff', label: 'TIFF', current: true, firstFrame: false, firstPage: false }, { format: 'png', label: 'PNG', current: false, firstFrame: false, firstPage: true }]`. A GIF card with `first_only` PNG gives `firstFrame: true`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core formats`, `cargo test -p ferry-core edit::tests`, `cargo test -p ferry-core pipeline`, `pnpm test`
Expected: FAIL (missing types and fields).

- [ ] **Step 3: Implement as specified**

Details:
- `Job::update` computes `choices = formats::choices(self.source_format, self.animated, self.pages.unwrap_or(1), oscodec::can_encode(Format::Heic))`.
- `Job::clipboard` and `Job::file` set `pages = Some(n)` when `self.source_format == Tiff` and `n = sniff::tiff_page_count(&bytes) > 1`.
- `card.fixture.ts` gains `poster: null, pages: null, choices: [png, jpeg, webp, avif]` (all `first_only: false`) and the three new `edits` fields.
- In `Card.svelte` `formatTip`, add `if (choice.firstPage) return t('pdf.first_page');` directly after the `firstFrame` line.

Append to the `pdf.*` block:

```json
  "pdf.first_page": "First page only",
```

- [ ] **Step 4: Run the tests and checks**

Run: `cargo test -p ferry-core`, `cargo test -p ferry` (after `pnpm build`), `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check`, `pnpm test`
Expected: PASS.

**Manual check (ask the user first):** with `pnpm tauri dev`, hover a PNG clipboard card. The format bar shows `PNG · JPEG · WebP · AVIF`. Clicking AVIF fails with "Ferry can't save AVIF files on this computer" (filled in by Task 9). That is expected at this point.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core src-tauri/src src/lib src/stack/Card.svelte
git commit -m "feat(core): recipe dpi/pages/uncrop, per-source convert choices, default conversions"
```

---

### Task 4: Posters for originals the webviews can't draw

**Files:**
- Create: `crates/ferry-core/src/poster.rs`; `lib.rs` (`pub mod poster;` after `pub mod output;`)
- Create: `crates/ferry-core/src/pipeline_formats.rs`
- Modify: `crates/ferry-core/src/pipeline.rs`:
  - `#[path = "pipeline_formats.rs"] mod formats_run;` directly after the `use` block, under `// sE pdf-formats`;
  - one call in `run_recipe` right after `let original = match self.original(backups) { … };`;
  - `update()` sets `poster`;
  - `fork()` copies `poster`.
- Modify: `src/stack/Card.svelte` (thumbnail `src`; page badge)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `edit::raster::{decode, cut}`, `edit::geometry::Rect`, `Backups::store_bytes`.
- Produces:
  - `poster::CARD_EDGE: u32 = 600` and `poster::VIEW_EDGE: u32 = 2400`.
  - `poster::needs_poster(format: Format) -> bool`: true for `Tiff`, `Heic`, `Pdf`. WKWebView and WebView2 both draw PNG, JPEG, GIF, WebP and AVIF.
  - `poster::png(bytes: &[u8], format: Format, page: u32, long_edge: u32) -> Result<(Vec<u8>, (u32, u32)), OptimiseError>`:
    - a PNG (png crate, `Compression::Fast`, RGBA8) scaled so its long side is at most `long_edge` (never upscaled), and its size;
    - `Pdf` renders `page` (1-based) through `pdf::render`. Until Task 12 that arm returns `Unsupported`; every other format uses `raster::decode`.
  - Private `Job::capture_poster(&mut self, original: &[u8], backups: &Backups)` in `pipeline_formats.rs`:
    - when `needs_poster(self.source_format)` and `self.poster` is `None`, it stores the poster with `backups.store_bytes(self.id.as_str(), ".ferry-poster.png", &png)`;
    - it sets `self.poster`. Any error leaves `poster` `None` and is not a job error;
    - it lives in the backup folder, so the asset scope (`$APPDATA/backups/**`) already allows it, and pruning removes it.
  - Card: `<img src={convertFileSrc(card.poster ?? card.thumb_path)} …>` (the `{#if card.thumb_path}` guard becomes `{#if card.poster ?? card.thumb_path}`). A badge `<span class="pages" aria-hidden="true">{t('pdf.badge.pages', { n: card.pages })}</span>` shows when `card.pages !== null && card.pages > 1`, top-left, 11 px, on `rgba(0,0,0,.55)`.

- [ ] **Step 1: Write the failing tests** (`poster.rs`)

```rust
#[test]
fn a_poster_fits_its_long_edge_and_keeps_the_ratio() {
    let (png, size) = png(&testutil::png(1200, 800), Format::Png, 1, CARD_EDGE).unwrap();
    assert_eq!(size, (600, 400));
    assert_eq!(crate::sniff::dimensions(&png, Format::Png), Some((600, 400)));
}

#[test]
fn a_small_picture_is_never_enlarged() {
    assert_eq!(png(&testutil::png(120, 80), Format::Png, 1, CARD_EDGE).unwrap().1, (120, 80));
}

#[test]
fn only_formats_the_webviews_cannot_draw_need_a_poster() {
    let need: Vec<Format> = Format::ALL.into_iter().filter(|&f| needs_poster(f)).collect();
    assert_eq!(need, [Format::Tiff, Format::Heic, Format::Pdf]);
}

#[test]
fn an_unreadable_file_has_no_poster() {
    assert!(png(b"II*\0garbage", Format::Tiff, 1, CARD_EDGE).is_err());
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core poster`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement `poster.rs`, `capture_poster` and the Card changes as specified**

Append to the `pdf.*` block:

```json
  "pdf.badge.pages": "{n} pages",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core poster`, `cargo test -p ferry-core pipeline`, `pnpm check`
Expected: PASS. Task 5 adds the end-to-end `a_tiff_card_gets_a_poster` once TIFF decodes.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core src/stack/Card.svelte src/lib/i18n/en.json
git commit -m "feat(core): poster thumbnails for TIFF, HEIC and PDF cards"
```

---

## TIFF

### Task 5: Reading TIFF

**Files:**
- Modify: `crates/ferry-core/Cargo.toml` (`tiff = "0.11"`)
- Create: `crates/ferry-core/src/optimise/tiff.rs`; `optimise/mod.rs` (`pub(crate) mod tiff;` after `pub(crate) mod png;`)
- Modify: `crates/ferry-core/src/edit/raster.rs` (`decode` arm `Tiff`)
- Modify: `crates/ferry-core/src/testutil.rs`

**Interfaces:**
- Consumes: `raster::{Raster, orient}`, `png::MAX_PIXELS`, `sniff::tiff_orientation`.
- Produces:
  - `tiff::decode_pages(bytes: &[u8]) -> Result<Vec<Raster>, OptimiseError>`: every page as straight RGBA8, oriented by its tag 274, carrying its ICC (tag 34675).
  - `tiff::decode_raster(bytes: &[u8]) -> Result<Raster, OptimiseError>`: the first page only. It must not decode the other pages.
  - `raster::decode(_, Format::Tiff)` calls `tiff::decode_raster`.
  - `testutil::tiff(pages: &[TiffPage]) -> Vec<u8>`, written with the `tiff` encoder (uncompressed) from `screenshot_rgba` pixels.
  - `testutil::TiffPage { size: (u32, u32), colour: TiffColour, orientation: u16, icc: Option<Vec<u8>> }` with `TiffColour::{Rgb8, Rgba8, Gray8, Gray16, Cmyk8}` and `Default` (`(64, 48)`, `Rgb8`, 1, `None`).

**Conversion to RGBA8:**
- 16-bit samples are narrowed with `>> 8`.
- Gray becomes `[v, v, v, 255]`.
- CMYK becomes `[255 − min(255, c + k), …, 255]`. This is naive, with no CMS; the note is in the doc comment.
- Any other colour type, or a page over `MAX_PIXELS` (checked from the header first), is `OptimiseError::Unsupported(t("pdf.tiff.unsupported"))`.
- A decode error is `OptimiseError::Decode(…)`.

- [ ] **Step 1: Write the failing tests** (`optimise/tiff.rs` and `pipeline.rs`)

```rust
#[test]
fn every_supported_colour_type_reads_as_rgba() {
    for colour in [TiffColour::Rgb8, TiffColour::Rgba8, TiffColour::Gray8, TiffColour::Gray16, TiffColour::Cmyk8] {
        let r = decode_raster(&testutil::tiff(&[TiffPage { colour, ..TiffPage::default() }])).unwrap();
        assert_eq!((r.width, r.height, r.rgba.len()), (64, 48, 64 * 48 * 4), "{colour:?}");
    }
}

#[test]
fn orientation_and_icc_are_honoured_per_page() {
    let icc = vec![7_u8; 300];
    let bytes = testutil::tiff(&[
        TiffPage { orientation: 6, icc: Some(icc.clone()), ..TiffPage::default() },
        TiffPage { size: (10, 20), ..TiffPage::default() },
    ]);
    let pages = decode_pages(&bytes).unwrap();
    assert_eq!(pages.len(), 2);
    assert_eq!((pages[0].width, pages[0].height, pages[0].icc.clone()), (48, 64, Some(icc)));
    assert_eq!((pages[1].width, pages[1].height), (10, 20));
}

#[test]
fn a_cut_short_tiff_is_a_decode_error() {
    let bytes = testutil::tiff(&[TiffPage::default()]);
    assert!(matches!(decode_raster(&bytes[..bytes.len() / 2]), Err(OptimiseError::Decode(_))));
}

// pipeline.rs
#[test]
fn a_tiff_file_becomes_a_jpeg_beside_it_and_gets_a_poster() {
    let env = Env::new();
    let path = env.file("scan.tiff", &testutil::tiff(&[TiffPage { size: (300, 200), ..TiffPage::default() }]));
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    let (finished, _) = run(&mut job, Level::Normal, &options(), &env);
    let card = finished.unwrap().update;
    assert_eq!((card.state, card.format, card.converted), (JobState::Done, Some(Format::Jpeg), true));
    assert!(path.with_extension("jpg").exists());
    let poster = card.poster.expect("a TIFF card has a poster");
    assert_eq!(crate::sniff::dimensions(&fs::read(poster).unwrap(), Format::Png), Some((300, 200)));
}
```

(Images' Converted default is In place, so `scan.tiff` goes to the test Trash; Restore brings it back, as with any conversion.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core tiff`
Expected: FAIL.

- [ ] **Step 3: Implement `decode_pages`, `decode_raster`, the `raster::decode` arm and the fixtures**

Append to the `pdf.*` block:

```json
  "pdf.tiff.unsupported": "This TIFF's colour format isn't supported",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core tiff`, `cargo test -p ferry-core pipeline`, `cargo test -p ferry-core raster`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core Cargo.lock src/lib/i18n/en.json
git commit -m "feat(core): read TIFF pages with orientation and ICC"
```

### Task 6: Writing TIFF and TIFF from the clipboard

**Files:**
- Modify: `crates/ferry-core/src/optimise/tiff.rs`
- Modify: `crates/ferry-core/src/optimise/mod.rs` (`encode` arm `Tiff` → `tiff::optimise`)
- Modify: `crates/ferry-core/src/edit/raster.rs` (`encode` arm `Tiff` → `tiff::encode_rasters(&[r.clone()])`)
- Modify: `crates/ferry-core/src/edit/mod.rs` (`Work::TiffPages`, a branch in `prepare`)
- Modify: `crates/ferry-core/src/pipeline.rs` (step label `(true, Format::Tiff) => "pdf.step.tiff"`)
- Modify: `src-tauri/src/platform/macos_clipboard.rs` (`IMAGE_TYPES` gains `("public.tiff", Format::Tiff)` as the last entry; the array length becomes 5)
- Modify: `src/stack/Card.svelte`, `src/lib/cards.ts` (no Compression button for TIFF)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 5's decode, `geometry::{crop_size, output_size, centre}`, `raster::cut`.
- Produces:
  - `tiff::optimise(input: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>`: lossless.
    - Every page is decoded in its own colour type and bit depth (no RGBA conversion), then written with `Compression::Deflate(DeflateLevel::Best)` and `Predictor::Horizontal`.
    - Each page keeps ICC 34675, XResolution 282, YResolution 283, ResolutionUnit 296 and Orientation 274.
    - Supported colour types: Gray8, Gray16, GrayA8, RGB8, RGB16, RGBA8, RGBA16, CMYK8. Others → `Unsupported(t("pdf.tiff.unsupported"))`.
  - `tiff::encode_rasters(pages: &[Raster]) -> Result<Vec<u8>, OptimiseError>`: RGBA8 pages, written as RGB8 when every alpha is 255, else RGBA8. Each keeps its ICC. Deflate + predictor.
  - `edit::Work::TiffPages(Vec<Raster>)` → `run` calls `tiff::encode_rasters`. `prepare` uses it when `source == Tiff && target == Tiff && edits.reshapes() && tiff_page_count > 1`: each page is cut with its own window (`crop_size`, `place` with the page's own pixels, `output_size`). `Applied.dims` is the first page's output size.
  - `railActions(card)` leaves out `'compression'` when `card.format === 'tiff'` (lossless output).

- [ ] **Step 1: Write the failing tests**

```rust
// optimise/tiff.rs
#[test]
fn optimising_keeps_every_page_and_every_pixel() {
    let icc = vec![3_u8; 200];
    let input = testutil::tiff(&[
        TiffPage { size: (200, 150), icc: Some(icc.clone()), ..TiffPage::default() },
        TiffPage { size: (100, 80), colour: TiffColour::Gray16, ..TiffPage::default() },
    ]);
    let out = optimise(&input, &OptimiseOptions::default()).unwrap();
    assert!(out.len() * 100 <= input.len() * 99, "{} -> {}", input.len(), out.len());
    assert_eq!(crate::sniff::tiff_page_count(&out), 2);
    assert_eq!(decode_pages(&out).unwrap(), decode_pages(&input).unwrap());
    assert_eq!(decode_pages(&out).unwrap()[0].icc, Some(icc));
}

#[test]
fn resolution_survives_optimising() {
    let input = testutil::tiff_with_resolution(300); // add to testutil: one page, 300 dpi, inch unit
    let out = optimise(&input, &OptimiseOptions::default()).unwrap();
    assert_eq!(testutil::tiff_resolution(&out), Some(300));
}

#[test]
fn a_tiff_already_deflated_comes_out_already_optimal() {
    let once = optimise(&testutil::tiff(&[TiffPage::default()]), &OptimiseOptions::default()).unwrap();
    assert_eq!(crate::optimise::optimise(&once, Format::Tiff, &OptimiseOptions::default()).unwrap(), Outcome::AlreadyOptimal);
}

// edit/mod.rs
#[test]
fn cropping_a_multi_page_tiff_crops_every_page() {
    let input = testutil::tiff(&[TiffPage { size: (300, 200), ..TiffPage::default() }, TiffPage { size: (100, 100), ..TiffPage::default() }]);
    let a = applied(&input, Format::Tiff, |e| e.aspect = Some((1, 1)));
    let pages = crate::optimise::tiff::decode_pages(&a.bytes).unwrap();
    assert_eq!(pages.iter().map(|p| (p.width, p.height)).collect::<Vec<_>>(), [(200, 200), (100, 100)]);
    assert_eq!((a.format, a.dims), (Format::Tiff, (200, 200)));
}

#[test]
fn a_single_page_tiff_crops_through_the_raster_path() {
    let a = applied(&testutil::tiff(&[TiffPage { size: (300, 200), ..TiffPage::default() }]), Format::Tiff, |e| e.scale = Some(50));
    assert_eq!((a.format, a.dims), (Format::Tiff, (150, 100)));
}
```

`src-tauri/src/platform/macos_clipboard.rs` test, in the file's existing private-pasteboard style:

```rust
#[test]
fn a_tiff_on_the_clipboard_is_read_when_nothing_better_is_there() {
    let pb = private_pasteboard(); // the helper the existing tests use (`pb.0`)
    let tiff = tiny_tiff(); // built inline, see the note below
    pb.0.clearContents();
    pb.0.setData_forType(Some(&NSData::with_bytes(&tiff)), &NSString::from_str("public.tiff"));
    assert_eq!(read_from(&pb.0).image.map(|(f, _)| f), Some(Format::Tiff));
}
```

Note: `ferry-core`'s `testutil` is `#[cfg(test)]`-only and not visible from `src-tauri`. Build the TIFF bytes inline in the test: an 8-byte header plus one IFD with tags 256, 257, 258, 259, 262, 273, 277, 279 for a 1×1 RGB8 strip. Write a small `fn tiny_tiff() -> Vec<u8>` in the test module.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core tiff`, `cargo test -p ferry-core edit::tests`, `cargo test -p ferry a_tiff_on_the_clipboard`
Expected: FAIL.

- [ ] **Step 3: Implement as specified**

Append to the `pdf.*` block:

```json
  "pdf.step.tiff": "Compressing TIFF",
```

- [ ] **Step 4: Run the tests**

Run: the three commands above, then `cargo test -p ferry-core`, `pnpm test`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core src-tauri/src/platform/macos_clipboard.rs src/lib src/stack/Card.svelte
git commit -m "feat(core): lossless TIFF optimise, multi-page TIFF edits, TIFF from the clipboard"
```

---

## HEIC

### Task 7: HEIC on macOS through ImageIO

**Files:**
- Modify: `crates/ferry-core/Cargo.toml`. Under `[target.'cfg(target_os = "macos")'.dependencies]`:
  - `objc2 = "0.6"`;
  - `objc2-core-foundation = { version = "0.3.2", default-features = false, features = ["std", "CFData", "CFDictionary", "CFString", "CFNumber", "CFArray"] }`;
  - `objc2-core-graphics = { version = "0.3.2", default-features = false, features = ["std", "CGImage", "CGColorSpace", "CGContext", "CGBitmapContext", "CGDataProvider"] }`;
  - `objc2-image-io = { version = "0.3.2", default-features = false, features = ["std", "CGImageSource", "CGImageDestination", "CGImageProperties", "objc2-core-graphics"] }`.
- Create: `crates/ferry-core/src/oscodec/macos.rs`
- Modify: `crates/ferry-core/src/oscodec/mod.rs` (route to `macos` under `#[cfg(target_os = "macos")]`; keep the stubs for other OSes)
- Create: `crates/ferry-core/src/optimise/heic.rs`; `optimise/mod.rs` (`pub(crate) mod heic;`, `encode` arm `Heic` → `heic::optimise`)
- Modify: `crates/ferry-core/src/edit/raster.rs` (`decode` `Heic` → `oscodec::decode`; `encode` `Heic` → `oscodec::encode`)
- Modify: `crates/ferry-core/src/pipeline.rs` (step label `(true, Format::Heic) => "pdf.step.heic"`)
- Modify: `src-tauri/src/platform/macos_clipboard.rs` (`IMAGE_TYPES` gains `("public.heic", Format::Heic)` before the `public.tiff` entry)
- Modify: `crates/ferry-core/src/testutil.rs` (macOS only: `heic(w, h, orientation) -> Option<Vec<u8>>`, written through `oscodec::encode` with orientation property set; `None` when there is no encoder)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `raster::Raster`, `png::MAX_PIXELS`, `optimise::quality`.
- Produces:
  - `oscodec::decode(bytes, Format::Heic) -> Result<Raster, OptimiseError>` (macOS):
    - `CGImageSourceCreateWithData`, then `CGImageSourceGetPrimaryImageIndex`;
    - check `kCGImagePropertyPixelWidth/Height` against `MAX_PIXELS`;
    - `CGImageSourceCreateThumbnailAtIndex` with `kCGImageSourceCreateThumbnailFromImageAlways = true`, `kCGImageSourceCreateThumbnailWithTransform = true` and `kCGImageSourceThumbnailMaxPixelSize = max(w, h)`. This gives full-size pixels with EXIF orientation and HEIF `irot`/`imir` applied;
    - draw into a `CGBitmapContext` (8 bpc, `kCGImageAlphaPremultipliedLast`) in the image's own colour space when its model is RGB, else sRGB;
    - un-premultiply. `icc = CGColorSpaceCopyICCData` (RGB only).
  - `oscodec::encode(r: &Raster, Format::Heic, quality: u8) -> Result<Vec<u8>, OptimiseError>`: a `CGImage` from straight RGBA (`kCGImageAlphaLast`), with a colour space from `r.icc` (`CGColorSpaceCreateWithICCData`, else sRGB). Then `CGImageDestinationCreateWithData(…, "public.heic", 1, None)`, `AddImage` with `kCGImageDestinationLossyCompressionQuality = quality / 100`, and `Finalize`.
  - `oscodec::reencode(bytes: &[u8], Format::Heic, quality: u8, strip_metadata: bool) -> Result<Vec<u8>, OptimiseError>`: `CGImageDestinationAddImageFromSource` (keeps orientation, colour and metadata) with the quality, plus `kCGImageMetadataShouldExcludeGPS = strip_metadata`.
  - `oscodec::can_encode(Format::Heic)`: `CGImageDestinationCopyTypeIdentifiers()` contains `public.heic`, cached in a `OnceLock<bool>`.
  - `heic::optimise(input, o)` = `oscodec::reencode(input, Format::Heic, quality(o), o.strip_metadata)`.
  - Errors:
    - ImageIO returns null for unreadable data → `OptimiseError::Decode("ImageIO could not read the HEIC")`;
    - on a non-mac non-Windows OS → `Unsupported(tf("pdf.read_unsupported", …))` / `pdf.write_unsupported`.

- [ ] **Step 1: Write the failing tests** (`oscodec/macos.rs`, all `#[cfg(target_os = "macos")]`)

```rust
#[test]
fn a_heic_round_trips_with_its_size() {
    let Some(bytes) = testutil::heic(120, 80, 1) else { return };
    assert_eq!(crate::sniff::sniff(&bytes), Some(Format::Heic));
    let r = decode(&bytes).unwrap();
    assert_eq!((r.width, r.height, r.rgba.len()), (120, 80, 120 * 80 * 4));
    assert!(r.icc.is_some(), "an RGB HEIC carries its colour profile");
}

#[test]
fn orientation_is_applied_to_the_pixels() {
    let Some(bytes) = testutil::heic(120, 80, 6) else { return };
    let r = decode(&bytes).unwrap();
    assert_eq!((r.width, r.height), (80, 120));
}

#[test]
fn a_heic_converts_to_jpeg_through_the_recipe() {
    let Some(bytes) = testutil::heic(120, 80, 1) else { return };
    let edits = Edits { format: Some(Format::Jpeg), ..Edits::default() };
    let a = crate::edit::apply(&bytes, Format::Heic, &edits, &OptimiseOptions::default()).unwrap();
    assert_eq!((a.format, a.dims, crate::sniff::sniff(&a.bytes)), (Format::Jpeg, (120, 80), Some(Format::Jpeg)));
}

#[test]
fn a_lower_quality_heic_is_smaller() {
    let Some(bytes) = testutil::heic(400, 300, 1) else { return };
    let high = reencode(&bytes, Format::Heic, 95, true).unwrap();
    let low = reencode(&bytes, Format::Heic, 30, true).unwrap();
    assert!(low.len() < high.len());
}

#[test]
fn garbage_is_a_decode_error_not_a_crash() {
    assert!(matches!(decode(&testutil::ftyp(b"heic", &[b"mif1"])), Err(OptimiseError::Decode(_))));
}

#[test]
fn macos_ci_can_encode_heic() {
    if std::env::var_os("CI").is_some() {
        assert!(can_encode(Format::Heic), "GitHub's macOS runners have the HEVC encoder");
    }
}
```

In `macos_clipboard.rs`, add a test like Task 6's: a `public.heic` entry beside `public.tiff` → read gives `Heic`. Build the bytes with the helper `ferry_core::oscodec::encode` on a 2×2 raster, which is public.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core oscodec`
Expected: FAIL (functions missing).

- [ ] **Step 3: Implement `oscodec/macos.rs`, `heic.rs` and the arms as specified**

All CoreFoundation objects are `CFRetained`. Every `unsafe` block gets a one-line `// SAFETY:` comment.

Append to the `pdf.*` block:

```json
  "pdf.step.heic": "Encoding HEIC",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core oscodec`, `cargo test -p ferry-core`, `cargo test -p ferry heic`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS on this Mac.

**Manual check (ask first):** copy a `.heic` photo in Finder and press ⌃⇧C. The card shows a poster, then `photo.jpg` with "converted"; the HEIC is in the Trash. Restore brings it back. The format bar shows `HEIC · PNG · JPEG · WebP · AVIF` (plus HEIC in the standard list on a PNG card when this Mac can encode).

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core Cargo.lock src-tauri/src/platform/macos_clipboard.rs src/lib/i18n/en.json
git commit -m "feat(core): HEIC decode and encode through macOS ImageIO"
```

### Task 8: HEIC on Windows through WIC (decode only)

**Windows-only code; it cannot be compiled on this Mac. CI's `core (windows-latest)` and `bundle (windows)` jobs compile and test it.**

**Files:**
- Modify: `crates/ferry-core/Cargo.toml`: `[target.'cfg(windows)'.dependencies] windows = { version = "0.62", features = ["Win32_Foundation", "Win32_Graphics_Imaging", "Win32_System_Com", "Win32_System_Com_StructuredStorage", "Win32_System_Variant"] }`
- Create: `crates/ferry-core/src/oscodec/windows.rs` (`#[cfg(windows)]` in `oscodec/mod.rs`)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Produces: `oscodec::decode(bytes, Format::Heic)` on Windows:
  - `CoInitializeEx(None, COINIT_MULTITHREADED)`; `RPC_E_CHANGED_MODE` is fine. Pair a successful init with `CoUninitialize` in a drop guard.
  - `CoCreateInstance(&CLSID_WICImagingFactory)`, then `IWICStream::InitializeFromMemory`, then `CreateDecoderFromStream(…, WICDecodeMetadataCacheOnDemand)`, then `GetFrame(0)`.
  - Check the size against `MAX_PIXELS`. Use `IWICFormatConverter` to `GUID_WICPixelFormat32bppRGBA`, then `CopyPixels`.
  - Orientation: `GetMetadataQueryReader().GetMetadataByName("System.Photo.Orientation")` as u16, applied with `raster::orient`.
  - ICC: `GetColorContexts`, the first `WICColorContextProfile`, `GetProfileBytes`.
  - `HRESULT` `WINCODEC_ERR_COMPONENTNOTFOUND` (`0x88982F50`) from `CreateDecoderFromStream` → `OptimiseError::Unsupported(t("pdf.heic.windows_codec"))`. Any other failure → `Decode(e.message())`.
- `can_encode(Heic)` stays `false` and `encode`/`reencode` return `Unsupported(pdf.write_unsupported)` on Windows.

- [ ] **Step 1: Write the failing test** (`oscodec/windows.rs`, `#[cfg(windows)]`)

```rust
#[test]
fn a_missing_windows_codec_says_what_to_install() {
    // GitHub's Windows runners have no HEIF extension, so the codec lookup fails;
    // with the extension installed, the junk below is a decode error instead.
    let junk = testutil::heif(b"heic", &[(64, 48)], None);
    match decode(&junk) {
        Err(OptimiseError::Unsupported(m)) => assert_eq!(m, crate::i18n::t("pdf.heic.windows_codec")),
        Err(OptimiseError::Decode(_)) => {}
        other => panic!("{other:?}"),
    }
}
```

- [ ] **Step 2: Check that the non-Windows build is unaffected**

Run: `cargo check -p ferry-core` on the Mac.
Expected: PASS. The Windows test can only fail or pass in CI, which runs when the user pushes (never push without asking). Until then, the commit message says "CI-verified pending".

- [ ] **Step 3: Implement as specified**

Append to the `pdf.*` block:

```json
  "pdf.heic.windows_codec": "Opening HEIC needs Microsoft's HEIF and HEVC extensions — get them from the Microsoft Store",
```

- [ ] **Step 4: Verify**

Run: `cargo test -p ferry-core` and `cargo clippy -p ferry-core --all-targets -- -D warnings` on the Mac.
Expected: PASS (the Windows file is not compiled here). On Windows CI: `core (windows-latest)` passes `a_missing_windows_codec_says_what_to_install`.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core Cargo.lock src/lib/i18n/en.json
git commit -m "feat(core): HEIC decode through Windows WIC (CI-verified only, not run on a Windows desktop)"
```

---

## AVIF

### Task 9: AVIF decode and encode

**Files:**
- Modify: `crates/ferry-core/Cargo.toml`:
  - `avif-decode = { version = "3.0", default-features = false }`;
  - `ravif = { version = "0.13", default-features = false, features = ["threading"] }`;
  - `moxcms = "0.9"`.
- Create: `crates/ferry-core/src/optimise/avif.rs`; `optimise/mod.rs` (`pub(crate) mod avif;`, `encode` arm)
- Modify: `crates/ferry-core/src/edit/raster.rs` (`decode`/`encode` arms `Avif`)
- Modify: `crates/ferry-core/src/pipeline.rs` (step label `(true, Format::Avif) => "pdf.step.avif"`)
- Modify: `src-tauri/src/platform/macos_clipboard.rs` (`("public.avif", Format::Avif)` before `public.heic`)
- Modify: `.github/workflows/ci.yml` and `.github/workflows/release.yml`: a step `- uses: ilammy/setup-nasm@v1` directly before "Build gifsicle sidecar" in `core`, `bundle` and `release`
- Modify: `crates/ferry-core/src/testutil.rs` (`avif(w: u32, h: u32, alpha: bool) -> Vec<u8>` via `avif::encode_raster` at quality 90)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Produces:
  - `avif::decode_raster(bytes: &[u8]) -> Result<Raster, OptimiseError>`:
    - `avif_decode::Decoder::from_avif(bytes)?.to_image()?`;
    - `Rgb8`/`Rgba8`/`Gray8` map directly, and the 16-bit variants with `>> 8`;
    - check the header size (`isobmff::primary_size`) against `MAX_PIXELS` first;
    - `icc: None`.
  - `avif::encode_raster(r: &Raster, o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>`:
    - when `r.icc` is `Some` and parses (`moxcms::ColorProfile::new_from_slice`), first transform RGBA to sRGB (`create_transform_8bit(Layout::Rgba, &ColorProfile::new_srgb(), Layout::Rgba, TransformOptions::default())`); an unparsable profile leaves the pixels as they are;
    - then `ravif::Encoder::new().with_quality(f32::from(quality(o))).with_alpha_quality(f32::from(quality(o))).with_speed(6).encode_rgba(Img::new(&pixels, w, h))`, returning `avif_file`.
  - `avif::optimise(input, o)` = decode, then `encode_raster`. The caller's `classify` keeps the original when it is not smaller.

- [ ] **Step 1: Write the failing tests** (`optimise/avif.rs`)

```rust
#[test]
fn an_avif_round_trips_with_its_size_and_alpha() {
    let bytes = testutil::avif(64, 48, true);
    assert_eq!(crate::sniff::sniff(&bytes), Some(Format::Avif));
    assert_eq!(crate::sniff::dimensions(&bytes, Format::Avif), Some((64, 48)));
    let r = decode_raster(&bytes).unwrap();
    assert_eq!((r.width, r.height), (64, 48));
    assert!(r.rgba.as_chunks::<4>().0.iter().any(|p| p[3] < 255), "alpha survives");
}

#[test]
fn every_still_format_converts_to_avif() {
    for (format, bytes) in [
        (Format::Png, testutil::png(120, 80)),
        (Format::Jpeg, testutil::jpeg(120, 80, 90, &testutil::JpegExtras::default())),
        (Format::Tiff, testutil::tiff(&[testutil::TiffPage { size: (120, 80), ..Default::default() }])),
    ] {
        let edits = Edits { format: Some(Format::Avif), ..Edits::default() };
        let a = crate::edit::apply(&bytes, format, &edits, &OptimiseOptions::default()).unwrap();
        assert_eq!((a.format, crate::sniff::sniff(&a.bytes), a.dims), (Format::Avif, Some(Format::Avif), (120, 80)), "{format:?}");
    }
}

#[test]
fn a_wide_gamut_source_is_converted_to_srgb() {
    let mut r = Raster { width: 8, height: 8, rgba: [0, 255, 0, 255].repeat(64), icc: None };
    let plain = decode_raster(&encode_raster(&r, &OptimiseOptions::default()).unwrap()).unwrap();
    r.icc = Some(moxcms::ColorProfile::new_display_p3().encode().unwrap());
    let p3 = decode_raster(&encode_raster(&r, &OptimiseOptions::default()).unwrap()).unwrap();
    assert_ne!(plain.rgba[..4], p3.rgba[..4], "P3 green becomes a different sRGB value");
}

#[test]
fn a_lower_quality_avif_is_smaller_and_plain_optimise_is_graded() {
    let r = decode_raster(&testutil::avif(200, 150, false)).unwrap();
    let q = |compression| OptimiseOptions { compression, ..OptimiseOptions::default() };
    assert!(encode_raster(&r, &q(30)).unwrap().len() < encode_raster(&r, &q(95)).unwrap().len());
    let high = encode_raster(&r, &q(100)).unwrap();
    assert!(matches!(crate::optimise::optimise(&high, Format::Avif, &q(60)).unwrap(), Outcome::Smaller { .. }));
}

#[test]
fn junk_is_a_decode_error() {
    assert!(matches!(decode_raster(&testutil::ftyp(b"avif", &[b"mif1"])), Err(OptimiseError::Decode(_))));
}
```

(If `moxcms` 0.9 names its Display P3 constructor differently, use the crate's equivalent; the assertion stays.)

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core avif` (NASM installed: `nasm -v`)
Expected: FAIL (module missing). The first build compiles rav1d and rav1e and takes a few minutes.

- [ ] **Step 3: Implement as specified**

Append to the `pdf.*` block:

```json
  "pdf.step.avif": "Encoding AVIF",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core avif`, `cargo test -p ferry-core`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

**Manual check (ask first):** click AVIF on a PNG card. The card shows `shot.avif`. Compare (⌘D) shows it, since the webviews draw AVIF natively.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core Cargo.lock src-tauri/src/platform/macos_clipboard.rs .github/workflows src/lib/i18n/en.json
git commit -m "feat(core): AVIF decode (rav1d) and encode (ravif), NASM in CI"
```

---

## Animated WebP

### Task 10: Animated WebP is optimised, not left alone

**Files:**
- Modify: `crates/ferry-core/Cargo.toml` (`libwebp-sys = "0.9"`)
- Create: `crates/ferry-core/src/optimise/webp_anim.rs`; `optimise/mod.rs` (`pub(crate) mod webp_anim;`)
- Modify: `crates/ferry-core/src/optimise/mod.rs`:
  - `optimise()`'s early return becomes `if is_animated(..) && !matches!(format, Format::Gif | Format::WebP) { return Ok(Outcome::AlreadyOptimal) }`;
  - `encode` arm `WebP` calls `webp_anim::optimise` when `is_animated`, else `webp::optimise`.
- Modify: `crates/ferry-core/src/testutil.rs` (`animated_webp_frames(frames: &[u32], w: u32, h: u32) -> Vec<u8>`: a real animation, one frame per duration in ms, each frame a shifted `screenshot_rgba`, encoded lossless through `webp_anim::encode`. The existing header-only `animated_webp()` stays for the sniff tests)
- Modify existing tests:
  - `optimise::tests::animated_webp_and_apng_are_left_alone` keeps only the APNG half; it is renamed `apng_is_left_alone`;
  - `edit::tests::an_animated_webp_cannot_be_edited` becomes `an_apng_cannot_be_edited` (use `testutil::with_png_chunk(&testutil::png(64, 64), b"acTL", &[0, 0, 0, 2, 0, 0, 0, 0])`; the message is "Animated PNG can't be edited yet");
  - `pipeline::tests::an_animated_webp_offers_no_edits` becomes `an_apng_offers_no_edits`;
  - `pipeline::tests::a_card_says_why_its_tools_are_off`: switch its animated example to APNG if it uses WebP.
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Produces:
  - `webp_anim::Animation { pub width: u32, pub height: u32, pub frames: Vec<Frame>, pub loop_count: u32, pub icc: Option<Vec<u8>> }`.
  - `webp_anim::Frame { pub rgba: Vec<u8>, pub duration_ms: u32 }`: a full canvas, straight RGBA.
  - `webp_anim::MAX_ANIMATION_BYTES: u64 = 1_000_000_000`.
  - `webp_anim::frame_count(bytes: &[u8]) -> usize`: counts `ANMF` chunks with `webp::chunks`.
  - `webp_anim::decode(bytes) -> Result<Animation, OptimiseError>`:
    - first check `frame_count × w × h × 4` (size from `VP8X`) against `MAX_ANIMATION_BYTES` → `Unsupported(t("pdf.anim.too_large"))`;
    - then `webp::AnimDecoder::new(bytes).decode()`. A frame's duration is its end timestamp minus the previous one; a 0 duration becomes 100;
    - `icc` comes from the source's `ICCP` chunk.
  - `webp_anim::encode(a: &Animation, o: &OptimiseOptions, extras: &webp::Extras) -> Result<Vec<u8>, OptimiseError>`, a direct `libwebp-sys` wrapper:
    - `WebPAnimEncoderOptionsInitInternal(…, WebPGetMuxABIVersion())`, with `anim_params.loop_count = a.loop_count` and `allow_mixed = 1`;
    - `WebPAnimEncoderNewInternal`. For each frame: a `WebPPicture` (`use_argb = 1`, `WebPPictureImportRGBA`), then `WebPAnimEncoderAdd(enc, pic, start_ms, &config)`, where `start_ms` is the sum of the earlier durations and `config` is lossy, `quality(o)`, `method` 4;
    - then `WebPAnimEncoderAdd(enc, null, total_ms, null)`, `WebPAnimEncoderAssemble`, and copy out the bytes;
    - `WebPAnimEncoderDelete`, `WebPPictureFree` and `WebPDataClear` run on every path (a drop guard);
    - finally insert the extras: `ICCP` directly after `VP8X`, `EXIF` and `XMP ` at the end, and set the `VP8X` flags. Use the module's `chunks`/`push_chunk` helpers, made `pub(crate)`.
  - `webp_anim::optimise(input, o)`: decode, then encode with `Extras { icc, exif/xmp unless o.strip_metadata }`.
    - The animation safety net: when `frame_count(&out) != frame_count(input)` → `Err(OptimiseError::Encode(t("pdf.anim.flattened")))`.
  - `webp_anim::first_frame(bytes) -> Result<Raster, OptimiseError>`: frame 0 with its ICC (used by Task 11).

- [ ] **Step 1: Write the failing tests** (`optimise/webp_anim.rs`)

```rust
#[test]
fn an_animation_round_trips_its_frames_timing_and_loop() {
    let bytes = testutil::animated_webp_frames(&[100, 200, 300], 64, 48);
    let a = decode(&bytes).unwrap();
    assert_eq!((a.width, a.height, a.frames.len()), (64, 48, 3));
    assert_eq!(a.frames.iter().map(|f| f.duration_ms).collect::<Vec<_>>(), [100, 200, 300]);
    assert_eq!(a.loop_count, 0);
}

#[test]
fn optimising_keeps_every_frame_and_the_last_frames_duration() {
    let input = testutil::animated_webp_frames(&[100, 200, 300], 160, 120);
    let out = optimise(&input, &OptimiseOptions::default()).unwrap();
    assert_eq!(frame_count(&out), 3);
    assert_eq!(decode(&out).unwrap().frames.last().unwrap().duration_ms, 300);
    assert!(crate::sniff::is_animated(&out, Format::WebP));
}

#[test]
fn the_colour_profile_survives() {
    let mut a = decode(&testutil::animated_webp_frames(&[100, 100], 32, 32)).unwrap();
    a.icc = Some(vec![5; 90]);
    let extras = Extras { icc: a.icc.clone(), ..Extras::default() };
    let out = encode(&a, &OptimiseOptions::default(), &extras).unwrap();
    assert_eq!(decode(&out).unwrap().icc, Some(vec![5; 90]));
}

#[test]
fn a_huge_animation_is_refused_before_decoding() {
    let mut bytes = testutil::animated_webp_frames(&[100], 16, 16);
    // VP8X canvas 16 383 × 16 383: width − 1 and height − 1 as 24-bit LE at 24 and 27.
    bytes[24..27].copy_from_slice(&[0xFE, 0x3F, 0]);
    bytes[27..30].copy_from_slice(&[0xFE, 0x3F, 0]);
    assert!(matches!(decode(&bytes), Err(OptimiseError::Unsupported(m)) if m == t("pdf.anim.too_large")));
}

// optimise/mod.rs
#[test]
fn an_animated_webp_is_re_encoded_and_stays_animated() {
    let input = testutil::animated_webp_frames(&[80, 80, 80, 80], 200, 150);
    match optimise(&input, Format::WebP, &OptimiseOptions::default()).unwrap() {
        Outcome::Smaller { bytes } => assert_eq!(webp_anim::frame_count(&bytes), 4),
        other => assert_ne!(other, Outcome::Larger, "graded like any other format"),
    }
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core webp_anim`
Expected: FAIL.

- [ ] **Step 3: Implement as specified, and update the four existing tests listed above**

Append to the `pdf.*` block:

```json
  "pdf.anim.too_large": "This animation is too large to re-encode",
  "pdf.anim.flattened": "The animation would lose frames, so the original was kept",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core webp_anim`, `cargo test -p ferry-core optimise`, `cargo test -p ferry-core edit`, `cargo test -p ferry-core pipeline`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core Cargo.lock src/lib/i18n/en.json
git commit -m "feat(core): re-encode animated WebP with libwebp's animation encoder"
```

### Task 11: Editing animations, and GIF → animated WebP

**Files:**
- Create: `crates/ferry-core/src/optimise/gif_frames.rs`; `optimise/mod.rs` (`pub(crate) mod gif_frames;`)
- Modify: `crates/ferry-core/src/edit/mod.rs`:
  - `apply`'s early `Unsupported(error.animated_edit)` excludes `WebP` as well as `Gif`;
  - new `Work::Animation`;
  - branches in `prepare`.
- Modify: `crates/ferry-core/src/edit/raster.rs` (`decode(_, WebP)`: when `sniff::is_animated` → `webp_anim::first_frame`)
- Modify: `crates/ferry-core/src/pipeline.rs` (`update()`: `editable = !self.animated || matches!(self.source_format, Format::Gif | Format::WebP)`)

**Interfaces:**
- Consumes: Task 10's `Animation`, `encode`, `first_frame`; `raster::cut`; `geometry::*`; `smart::frame`.
- Produces:
  - `gif_frames::decode(bytes: &[u8]) -> Result<Animation, OptimiseError>`: composited full-canvas frames, with the same `MAX_ANIMATION_BYTES` check (frame count from `sniff::gif_frame_count`).
    - `duration_ms = delay × 10`, and a delay under 2 becomes 100 ms (what browsers do).
    - `loop_count` from `decoder.repeat()`: `Infinite → 0`, `Finite(n) → n`.
    - `icc: None`.
    - Compositing, per frame on a transparent canvas the size of the logical screen:

      ```text
      before = canvas.clone() if frame.dispose == Previous
      for each pixel of the frame inside the screen: if alpha > 0 { canvas[pixel] = frame pixel }
      push canvas.clone() as the frame
      match frame.dispose {
          Background => clear the frame's rectangle (inside the screen) to [0, 0, 0, 0],
          Previous   => canvas = before,
          Keep | Any => {}
      }
      ```
  - `edit::Work::Animation { anim: Animation, extras: webp::Extras }`: `run` calls `webp_anim::encode`. `prepare` uses it when the source is animated `WebP` with target `WebP`, or animated `Gif` with target `WebP`.
    - The window comes from `geometry::crop_size((w, h), edits)` and `place(…, Some(&first frame raster))`; the size from `output_size`.
    - Every frame is cut with `raster::cut` to that size; durations and loop count are kept.
    - `extras.icc` = the source's ICC.
    - The safety net applies: the output frame count must equal the input's, else `Encode(t("pdf.anim.flattened"))`.
  - An animated WebP converted to PNG/JPEG/AVIF/HEIC goes down the `Pixels` path from `raster::decode` (first frame).

- [ ] **Step 1: Write the failing tests**

```rust
// gif_frames.rs
#[test]
fn frames_are_composited_on_the_screen() {
    let a = decode(&testutil::gif(3)).unwrap();
    assert_eq!((a.width, a.height, a.frames.len()), (64, 48, 3));
    assert!(a.frames.iter().all(|f| f.rgba.len() == 64 * 48 * 4 && f.duration_ms >= 20));
}

#[test]
fn background_disposal_clears_and_previous_restores() {
    // testutil::gif_with_disposal (frames counted from 0): frame 0 fills the screen red (Keep);
    // frame 1 draws a 4×4 blue square at 0,0 (Background); frame 2 draws a 1×1 green dot at
    // 10,10 (Previous); frame 3 draws a 1×1 white dot at 20,20 (Keep).
    let a = decode(&testutil::gif_with_disposal()).unwrap();
    let px = |f: usize, x: u32, y: u32| a.frames[f].rgba[((y * a.width + x) * 4) as usize..][..4].to_vec();
    assert_eq!(px(1, 0, 0), [0, 0, 255, 255]);
    assert_eq!(px(2, 0, 0), [0, 0, 0, 0], "frame 1's square was cleared to background");
    assert_eq!(px(2, 10, 10), [0, 255, 0, 255]);
    assert_eq!(px(3, 10, 10), [255, 0, 0, 255], "frame 2's dot was undone back to red");
}

// edit/mod.rs
#[test]
fn an_animated_webp_downscales_every_frame() {
    let a = applied(&testutil::animated_webp_frames(&[100, 100, 100], 200, 100), Format::WebP, |e| e.scale = Some(50));
    assert_eq!((a.format, a.dims), (Format::WebP, (100, 50)));
    let back = crate::optimise::webp_anim::decode(&a.bytes).unwrap();
    assert_eq!((back.width, back.frames.len()), (100, 3));
}

#[test]
fn a_gif_converts_to_an_animated_webp_with_every_frame() {
    let a = applied(&testutil::gif(3), Format::Gif, |e| e.format = Some(Format::WebP));
    assert_eq!(a.format, Format::WebP);
    assert_eq!(crate::optimise::webp_anim::frame_count(&a.bytes), 3);
}

#[test]
fn an_animated_webp_converts_to_a_still_png_from_its_first_frame() {
    let a = applied(&testutil::animated_webp_frames(&[100, 100], 64, 48), Format::WebP, |e| e.format = Some(Format::Png));
    assert_eq!((a.format, a.dims), (Format::Png, (64, 48)));
}

// pipeline.rs
#[test]
fn an_animated_webp_card_offers_its_tools() {
    let env = Env::new();
    let path = env.file("loop.webp", &testutil::animated_webp_frames(&[100, 100], 64, 48));
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    let (finished, _) = run(&mut job, Level::Normal, &options(), &env);
    let card = finished.unwrap().update;
    assert!(card.can_edit && card.edit_hint.is_none());
}
```

Add `testutil::gif_with_disposal()`: the 4-frame, 32×32 GIF described in the comment, written with the `gif` encoder, delays 10 cs.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core gif_frames`, `cargo test -p ferry-core edit::tests`
Expected: FAIL.

- [ ] **Step 3: Implement as specified**

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core` (all)
Expected: PASS, including the gifsicle GIF tests, unchanged.

**Manual check (ask first):** copy an animated GIF file, press ⌃⇧C, then click WebP. Open the result in Safari: it animates at the same speed, and its "First frame only" tooltip is gone from the WebP chip (still on PNG/JPEG). Downscale an animated WebP card to 50 %; the result still animates.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core
git commit -m "feat(core): crop and downscale animated WebP; GIF converts to animated WebP"
```

---

## PDF

### Task 12: PDFium, bundled and loaded at run time

**Files:**
- Create: `scripts/fetch-pdfium.sh`
- Create: `src-tauri/tauri.macos.conf.json`, `src-tauri/tauri.windows.conf.json`
- Modify: `package.json` (`"pdfium": "bash scripts/fetch-pdfium.sh"` after `"sidecar"`)
- Modify: `.github/workflows/ci.yml`:
  - `core`: a step "Fetch PDFium" (`shell: bash`, `run: scripts/fetch-pdfium.sh ${{ matrix.triple }}`) after "Build gifsicle sidecar", plus env `FERRY_PDFIUM: ${{ github.workspace }}/src-tauri/binaries/${{ runner.os == 'Windows' && 'pdfium.dll' || 'libpdfium.dylib' }}`;
  - `bundle`: the same step with `${{ matrix.target }}`, after "Build gifsicle sidecar";
  - the Windows smoke test gains `if (-not (Test-Path (Join-Path $env:LOCALAPPDATA 'Ferry\pdfium.dll'))) { throw "pdfium.dll missing from install" }` after the gifsicle check.
- Modify: `.github/workflows/release.yml` (the fetch step after "Build gifsicle sidecar")
- Modify: `crates/ferry-core/Cargo.toml`:
  - `lopdf = { version = "0.45", default-features = false }`;
  - `pdfium-render = { version = "0.9.4", default-features = false, features = ["pdfium_7881", "thread_safe"] }`.
- Create: `crates/ferry-core/src/pdf/mod.rs` (module doc, `pub mod render;`, `DPI_STOPS`, `Defaults`), `crates/ferry-core/src/pdf/render.rs`; `lib.rs` (`pub mod pdf;` after `pub mod output;`)
- Modify: `crates/ferry-core/src/poster.rs` (`Pdf` arm renders the page)
- Modify: `crates/ferry-core/src/testutil.rs` (`pdfium() -> bool`, `blank_pdf(pages: &[(f32, f32)]) -> Vec<u8>`)
- Modify: `src-tauri/src/lib.rs` (`fn pdfium()` after `fn gifsicle()`; in `setup`, after `windows::create_stack(app)?;`, add `if let Some(path) = pdfium() { ferry_core::pdf::render::init(path); }`)
- Modify: `src/lib/i18n/en.json`

**`scripts/fetch-pdfium.sh <triple>`** follows `build-gifsicle.sh`: `set -euo pipefail`, a cache in `target/pdfium-download/`, a SHA-256 check (with `GIFSICLE_SHA256`-style override `PDFIUM_SHA256`), and the cached file deleted on mismatch.

| Triple | Asset | SHA-256 | Copies |
| --- | --- | --- | --- |
| `*-apple-darwin`, `universal-apple-darwin` | `pdfium-mac-univ.tgz` | `df451a413c3609585e84a4a91110a9bc889cff05fe3b2db0ed817c9e90c3f7d3` | `lib/libpdfium.dylib` → `src-tauri/binaries/libpdfium.dylib` |
| `x86_64-pc-windows-msvc` | `pdfium-win-x64.tgz` | `73cc0de638ac2095e7445bf56a38200a5b7c7ca0e9f4ba144598f2457377ac08` | `bin/pdfium.dll` → `src-tauri/binaries/pdfium.dll` |

- The URL is `https://github.com/bblanchon/pdfium-binaries/releases/download/chromium%2F7881/<asset>`.
- Both write `src-tauri/binaries/pdfium-NOTICE.txt`: the archive's `LICENSE`, then every `licenses/*.txt`/`*.md`, each under a `== <file name> ==` heading.
- Any other triple fails with `unsupported target`.
- The script ends by printing `fetched <path>`.

**Overlays** (merged by Tauri over `tauri.conf.json`):

```json
// src-tauri/tauri.macos.conf.json
{ "bundle": { "macOS": { "frameworks": ["binaries/libpdfium.dylib"] },
              "resources": { "binaries/pdfium-NOTICE.txt": "pdfium-NOTICE.txt" } } }
// src-tauri/tauri.windows.conf.json
{ "bundle": { "resources": { "binaries/pdfium.dll": "pdfium.dll",
                             "binaries/pdfium-NOTICE.txt": "pdfium-NOTICE.txt" } } }
```

(No comments in the real files.) Because `tauri-build` copies resources at compile time, `src-tauri` does not build until the fetch script has run. CI runs it before clippy.

**Interfaces:**
- Produces:
  - `pdf::DPI_STOPS: [u16; 7] = [300, 250, 200, 150, 100, 72, 48]`.
  - `pdf::Defaults { pub dpi: u16, pub dpi_aggressive: u16, pub extract_dpi: u16 }` with `Default` `(150, 100, 150)` and `From<&settings::Pdf>`.
  - `pdf::render::init(library: PathBuf)`: stores the path in a `OnceLock`; the first call wins. Binding is lazy: the first render calls `Pdfium::bind_to_library(path)` once, held in a `OnceLock<Result<Pdfium, String>>`.
  - `pdf::render::available() -> bool`: a path is set and the file exists.
  - `pdf::render::page_count(bytes: &[u8]) -> Result<u32, OptimiseError>`.
  - `pdf::render::render_pages(bytes: &[u8], pages: RangeInclusive<u32>, dpi: u16, cancel: &CancelToken, on_page: &mut dyn FnMut(u32, Raster) -> Result<(), OptimiseError>) -> Result<(), OptimiseError>`: loads once and renders 1-based pages in order.
    - Size: `w = round(width_pt × dpi / 72)`, `h` the same, at least 1. When the long side exceeds `MAX_SIDE` or `w × h` exceeds `MAX_PIXELS`, both shrink by the same factor.
    - `PdfRenderConfig` uses `set_target_width(w)`, `set_maximum_height(h)`, `render_form_data(true)`, `render_annotations(true)`, `set_clear_color(PdfColor::WHITE)`.
    - Pixels come from `as_rgba_bytes()`; `icc: None`.
    - The cancel token is checked before each page → `Err(OptimiseError::Cancelled)`.
  - `pdf::render::render_page(bytes, page: u32, dpi: u16, max_side: u32) -> Result<Raster, OptimiseError>`: one page, with its long side also capped at `max_side`.
  - Errors:
    - no library, or it fails to load → `Unsupported(t("pdf.error.renderer"))`;
    - PDFium rejects the file → `Decode(t("pdf.error.read"))`;
    - a password-protected file → `Unsupported(t("pdf.error.encrypted"))`.
  - `poster::png(bytes, Pdf, page, long_edge)` renders `page` at 72 dpi with `max_side = long_edge`.
  - `testutil::pdfium() -> bool`: `init`s from `FERRY_PDFIUM`, else from `<CARGO_MANIFEST_DIR>/../../src-tauri/binaries/<Pdfium::pdfium_platform_library_name()>`; true when `render::available()`.
  - `testutil::blank_pdf(pages: &[(f32, f32)]) -> Vec<u8>`: lopdf, one page per `(width_pt, height_pt)`, each with a small filled rectangle content stream.
  - `src-tauri` `fn pdfium() -> Option<PathBuf>`:
    - macOS: `exe/../Frameworks/libpdfium.dylib`;
    - Windows (`cfg(windows)`): `exe_dir/pdfium.dll`;
    - in debug builds only, fall back to `concat!(env!("CARGO_MANIFEST_DIR"), "/binaries/")` + the platform name, so `pnpm tauri dev` works;
    - the first existing path wins.

- [ ] **Step 1: Write the failing tests** (`pdf/render.rs`)

```rust
#[test]
fn ci_always_has_pdfium() {
    if std::env::var_os("CI").is_some() {
        assert!(testutil::pdfium(), "FERRY_PDFIUM must point at the fetched library in CI");
    }
}

#[test]
fn a_letter_page_renders_at_the_asked_dpi_on_white() {
    if !testutil::pdfium() { return; }
    let r = render_page(&testutil::blank_pdf(&[(612.0, 792.0)]), 1, 150, u32::MAX).unwrap();
    assert_eq!((r.width, r.height), (1275, 1650));
    assert_eq!(r.rgba[..4], [255, 255, 255, 255]);
}

#[test]
fn the_long_side_is_capped() {
    if !testutil::pdfium() { return; }
    let r = render_page(&testutil::blank_pdf(&[(612.0, 792.0)]), 1, 150, 600).unwrap();
    assert_eq!(r.height, 600);
}

#[test]
fn every_page_is_counted_and_rendered_in_order() {
    if !testutil::pdfium() { return; }
    let pdf = testutil::blank_pdf(&[(100.0, 100.0), (200.0, 100.0), (100.0, 300.0)]);
    assert_eq!(page_count(&pdf).unwrap(), 3);
    let mut seen = Vec::new();
    render_pages(&pdf, 2..=3, 72, &CancelToken::new(), &mut |n, r| { seen.push((n, r.width, r.height)); Ok(()) }).unwrap();
    assert_eq!(seen, [(2, 200, 100), (3, 100, 300)]);
}

#[test]
fn a_cancel_stops_before_the_next_page() {
    if !testutil::pdfium() { return; }
    let pdf = testutil::blank_pdf(&[(100.0, 100.0); 3]);
    let cancel = CancelToken::new();
    let mut pages = 0;
    let result = render_pages(&pdf, 1..=3, 72, &cancel, &mut |_, _| { pages += 1; cancel.cancel(); Ok(()) });
    assert!(matches!(result, Err(OptimiseError::Cancelled)));
    assert_eq!(pages, 1);
}

#[test]
fn a_library_that_is_not_there_says_reinstall() {
    assert_eq!(load(Path::new("/nowhere/libpdfium.dylib")).err(), Some(t("pdf.error.renderer")));
}
```

`load(path) -> Result<Pdfium, String>` is the private binding helper. A missing file fails in `Library::new`, before PDFium's global bindings are touched, so this test is safe beside the others.

Add to `poster.rs`:

```rust
#[test]
fn a_pdf_poster_is_its_first_page() {
    if !testutil::pdfium() { return; }
    assert_eq!(png(&testutil::blank_pdf(&[(612.0, 792.0)]), Format::Pdf, 1, CARD_EDGE).unwrap().1, (464, 600));
}
```

- [ ] **Step 2: Fetch PDFium and run the tests to see them fail**

Run: `bash scripts/fetch-pdfium.sh "$(rustc -vV | sed -n 's/^host: //p')"` (expect `fetched …/libpdfium.dylib`), then `cargo test -p ferry-core render`
Expected: FAIL (module missing).

- [ ] **Step 3: Implement the script, the overlays, the CI steps, `pdf::render`, the poster arm and the app path**

Append to the `pdf.*` block:

```json
  "pdf.error.renderer": "PDF renderer missing — reinstall Ferry",
  "pdf.error.read": "Can't read this PDF — it may be damaged",
  "pdf.error.encrypted": "This PDF is encrypted — Ferry leaves it as it is",
```

- [ ] **Step 4: Run the tests and a local bundle check**

Run: `cargo test -p ferry-core render`, `cargo test -p ferry-core poster`, `cargo build -p ferry`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

**Manual check (ask first; this takes several minutes):** `pnpm tauri build --bundles app`, then `ls target/release/bundle/macos/Ferry.app/Contents/Frameworks/`. It should list `libpdfium.dylib`, and `Contents/Resources/pdfium-NOTICE.txt` should exist.

- [ ] **Step 5: Commit**

```bash
git add scripts/fetch-pdfium.sh src-tauri/tauri.macos.conf.json src-tauri/tauri.windows.conf.json package.json .github/workflows crates/ferry-core Cargo.lock src-tauri/src/lib.rs src/lib/i18n/en.json
git commit -m "build: bundle PDFium chromium/7881 and render PDF pages through it"
```

The message also states that the Windows path and smoke-test line are CI-verified only.

### Task 13: PDF jobs: facts, refusals, structural optimise, PDF output rules

**Files:**
- Modify: `crates/ferry-core/src/pdf/mod.rs` (`Facts`, `facts`, `optimise`, `PdfResult`, `apply` skeleton, `target_dpi`)
- Modify: `crates/ferry-core/src/pipeline.rs`:
  - `RunOptions` fields `pdf_output: settings_model::Output` and `pdf: pdf::Defaults` at the end of the struct, filled in `RunOptions::new` from `settings.pdf`;
  - `Job::file`: the size limit picks `settings.pdf.max_size_mb` for a PDF;
  - `run_recipe`: right after the existing `original()` match, `if let Err(reason) = self.capture_facts(&original, backups) { return Some(self.fail(reason.clone(), reason)); }`. This replaces Task 4's poster call; `capture_facts` calls `capture_poster`;
  - right after the `emit(Optimising …)` call: `if self.source_format == Format::Pdf { return self.finish_pdf(&original, &options, o, emit); }`;
  - extract the location logic of `put`'s `Source::File` branch into `fn place_result(&mut self, ext: &str, kind: OutputKind, original: &[u8], o: &RunOptions, write: &mut dyn FnMut(&Self, &Path) -> Result<(), (String, String)>) -> Result<PathBuf, (String, String)>`. It contains everything from `let source = …` to `self.output = path;`, with `self.write_file(&path, bytes, &source, o)?` replaced by `write(self, &path)?`. The rules are `&o.pdf_output` when `self.source_format == Format::Pdf`, else `&o.output`. `put` calls it;
  - `detail()` starts with `if self.source_format == Format::Pdf { return self.pdf_detail(); }`.
- Modify: `crates/ferry-core/src/pipeline_formats.rs` (`capture_facts`, `finish_pdf`, `pdf_detail`)
- Modify: `crates/ferry-core/src/testutil.rs` (`pdf(pages: &[PdfPage], extras: &PdfExtras) -> Vec<u8>`)
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 12's renderer (posters), lopdf.
- Produces:
  - `pdf::Facts { pub pages: u32, pub size: (u32, u32), pub encrypted: bool, pub signed: bool, pub pdfa1: bool }`.
  - `pdf::facts(bytes: &[u8]) -> Result<Facts, OptimiseError>`:
    - `Document::load_mem_with_options(bytes, LoadOptions::with_max_decompressed_size(1 << 30))`;
    - `encrypted = doc.was_encrypted() || doc.trailer.has(b"Encrypt")`. A load error on bytes that contain `/Encrypt` → `Ok(Facts { encrypted: true, .. })`; any other load error → `Decode(t("pdf.error.read"))`;
    - `size` = page 1's visible box (`crop::visible_box`, written in Task 15; until then `MediaBox` ∩ `CropBox` inline) in points, rounded, swapped when `/Rotate` is 90 or 270;
    - `signed` = catalog `/AcroForm` `/SigFlags & 1`, or any field with `/FT /Sig` and a `/V`;
    - `pdfa1` = the catalog's `/Metadata` stream contains `pdfaid:part="1"` or `<pdfaid:part>1<`.
  - `pdf::optimise(input: &[u8], dpi: u16, o: &OptimiseOptions, progress: &mut dyn FnMut(f32)) -> Result<Vec<u8>, OptimiseError>`:
    - load as above, then (Task 14 inserts image recompression here) `doc.prune_objects()`, `doc.delete_zero_length_streams()`, `doc.compress()`;
    - save with `save_modern`, or with `save_to` (classic xref, no object streams) when `pdfa1`;
    - call `progress(1.0)`.
  - `pdf::PdfResult { Pdf { bytes: Vec<u8>, missed_target: bool }, Pages { format: Format, pages: Vec<(u32, Vec<u8>)>, size: (u32, u32) } }`.
  - `pdf::target_dpi(edits: &Edits, d: &Defaults, extracting: bool) -> u16`: `edits.dpi`, else `d.extract_dpi` when extracting, else `d.dpi_aggressive` for `Level::Aggressive`, else `d.dpi`.
  - `pdf::apply(original: &[u8], edits: &Edits, o: &OptimiseOptions, d: &Defaults, progress: &mut dyn FnMut(f32, String)) -> Result<PdfResult, OptimiseError>`:
    - for PDF output: `fit_under` searches quality with `edit::fit::search` over `optimise`, else one `optimise` at `edits.compression` or `o.compression`;
    - a recipe with `aspect` or `uncrop` (Task 15) or a `Png`/`Jpeg` format (Task 16) returns `Unsupported(tf("pdf.write_unsupported", …))` until those tasks replace it; no `todo!`.
  - `Job::capture_facts(&mut self, original: &[u8], backups: &Backups) -> Result<(), String>` (private, `pipeline_formats.rs`): for a PDF, runs `pdf::facts`; a read error → `Err(t("pdf.error.read"))`, encrypted → `Err(t("pdf.error.encrypted"))`; it sets `self.pages = Some(facts.pages)` and `self.dims = Some(facts.size)`, then `capture_poster`.
  - `Job::finish_pdf(&mut self, original: &[u8], options: &OptimiseOptions, o: &RunOptions, emit: &mut dyn FnMut(JobUpdate)) -> Option<Finished>`:
    - a recipe whose `format` is `Png`/`Jpeg` goes to Task 16 (`Unsupported` placeholder until then);
    - otherwise a signed PDF → `fail(t("pdf.error.signed"))`;
    - otherwise a plain recipe runs `pdf::optimise` at `target_dpi`, graded with `optimise::classify` as images are;
    - a non-plain recipe runs `pdf::apply` and always writes;
    - progress goes to `emit(self.update(Optimising { progress, step }, None))`, emitted only when progress rose by at least 0.02 since the last emit, or reached 1.0, with step `t("pdf.step.pdf")`;
    - cancel checks follow `run_recipe`'s: return `None` before anything is written;
    - writing reuses `put` (single file) or `put_back`.
  - `Job::pdf_detail(&self) -> String`: `"3 pages · 150 dpi"`; `"1 page"` for one page; `" · cropped"` when `edits.aspect` is set or `edits.uncrop`; plus `card.detail.aggressive` when aggressive.
  - `testutil::PdfPage { pub media: [f32; 4], pub crop: Option<[f32; 4]>, pub rotate: i64, pub images: Vec<PdfImage>, pub padding: usize }` (`padding`: bytes of uncompressed comment content, to make a bloated file).
  - `testutil::PdfImage { pub px: (u32, u32), pub drawn: (f32, f32), pub kind: ImageKind }`, with `ImageKind::{DctRgb, DctGray, DctCmyk, FlateScreenshot, FlatePhoto, OneBit, Jpx}`.
  - `testutil::PdfExtras { pub signed: bool, pub pdfa1: bool, pub encrypt_dict: bool, pub shared_image: bool }` (`Default` everywhere, letter-size `media`).

- [ ] **Step 1: Write the failing tests**

```rust
// pdf/mod.rs
#[test]
fn facts_count_pages_and_turn_rotated_pages() {
    let pdf = testutil::pdf(&[PdfPage { rotate: 90, ..PdfPage::default() }, PdfPage::default()], &PdfExtras::default());
    let f = facts(&pdf).unwrap();
    assert_eq!((f.pages, f.size, f.encrypted, f.signed), (2, (792, 612), false, false));
}

#[test]
fn signatures_encryption_and_pdfa_are_noticed() {
    assert!(facts(&testutil::pdf(&[PdfPage::default()], &PdfExtras { signed: true, ..Default::default() })).unwrap().signed);
    assert!(facts(&testutil::pdf(&[PdfPage::default()], &PdfExtras { encrypt_dict: true, ..Default::default() })).unwrap().encrypted);
    assert!(facts(&testutil::pdf(&[PdfPage::default()], &PdfExtras { pdfa1: true, ..Default::default() })).unwrap().pdfa1);
}

#[test]
fn a_bloated_pdf_shrinks_and_keeps_its_pages() {
    let input = testutil::pdf(&[PdfPage { padding: 200_000, ..PdfPage::default() }; 3], &PdfExtras::default());
    let out = optimise(&input, 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert!(out.len() * 2 < input.len());
    assert_eq!(facts(&out).unwrap().pages, 3);
}

#[test]
fn pdfa1_is_saved_without_object_streams() {
    let input = testutil::pdf(&[PdfPage { padding: 50_000, ..PdfPage::default() }], &PdfExtras { pdfa1: true, ..Default::default() });
    let out = optimise(&input, 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert!(!out.windows(7).any(|w| w == b"/ObjStm"));
}

#[test]
fn the_dpi_follows_recipe_level_and_extraction() {
    let d = Defaults::default();
    assert_eq!(target_dpi(&Edits::default(), &d, false), 150);
    assert_eq!(target_dpi(&Edits { level: Level::Aggressive, ..Edits::default() }, &d, false), 100);
    assert_eq!(target_dpi(&Edits::default(), &d, true), 150);
    assert_eq!(target_dpi(&Edits { dpi: Some(72), ..Edits::default() }, &d, true), 72);
}
```

`pipeline.rs`:

```rust
fn pdf_file(env: &Env, name: &str, pages: &[PdfPage], extras: &PdfExtras) -> (PathBuf, Vec<u8>) {
    let bytes = testutil::pdf(pages, extras);
    (env.file(name, &bytes), bytes)
}

#[test]
fn a_bloated_pdf_is_optimised_in_place() {
    let env = Env::new();
    let (path, input) = pdf_file(&env, "doc.pdf", &[PdfPage { padding: 200_000, ..PdfPage::default() }; 3], &PdfExtras::default());
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    let card = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
    assert_eq!((card.state, card.pages, card.dims_before), (JobState::Done, Some(3), Some((612, 792))));
    assert_eq!(card.detail, "3 pages · 150 dpi");
    assert!(fs::read(&path).unwrap().len() < input.len());
}

#[test]
fn an_already_tight_pdf_is_already_optimal() {
    let env = Env::new();
    let (path, _) = pdf_file(&env, "doc.pdf", &[PdfPage::default()], &PdfExtras::default());
    let tight = crate::pdf::optimise(&fs::read(&path).unwrap(), 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    fs::write(&path, &tight).unwrap();
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    assert_eq!(run(&mut job, Level::Normal, &options(), &env).0.unwrap().update.state, JobState::AlreadyOptimal);
}

#[test]
fn an_encrypted_pdf_is_refused_and_left_alone() {
    let env = Env::new();
    let (path, input) = pdf_file(&env, "doc.pdf", &[PdfPage::default()], &PdfExtras { encrypt_dict: true, ..Default::default() });
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    let card = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
    assert_eq!(card.state, JobState::Error { reason: t("pdf.error.encrypted") });
    assert_eq!(fs::read(&path).unwrap(), input);
}

#[test]
fn a_signed_pdf_is_never_rewritten() {
    let env = Env::new();
    let (path, input) = pdf_file(&env, "doc.pdf", &[PdfPage { padding: 100_000, ..PdfPage::default() }], &PdfExtras { signed: true, ..Default::default() });
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    let card = run(&mut job, Level::Normal, &options(), &env).0.unwrap().update;
    assert_eq!(card.state, JobState::Error { reason: t("pdf.error.signed") });
    assert_eq!(fs::read(&path).unwrap(), input);
}

#[test]
fn a_damaged_pdf_is_an_error() {
    let env = Env::new();
    let path = env.file("doc.pdf", b"%PDF-1.7\nthis is not a PDF body");
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    assert_eq!(run(&mut job, Level::Normal, &options(), &env).0.unwrap().update.state, JobState::Error { reason: t("pdf.error.read") });
}

#[test]
fn pdfs_have_their_own_size_limit() {
    let env = Env::new();
    let (path, _) = pdf_file(&env, "doc.pdf", &[PdfPage { padding: 1_200_000, ..PdfPage::default() }], &PdfExtras::default());
    let mut settings = Settings::default();
    settings.images.max_size_mb = 1;
    assert!(Job::file(&path, &settings, true).is_ok());
    settings.pdf.max_size_mb = 1;
    assert!(Job::file(&path, &settings, true).is_err());
}

#[test]
fn pdf_results_follow_the_pdf_output_rules() {
    let env = Env::new();
    let (path, input) = pdf_file(&env, "doc.pdf", &[PdfPage { padding: 200_000, ..PdfPage::default() }], &PdfExtras::default());
    let mut settings = Settings::default();
    settings.pdf.output.optimised.location = Location::SameFolder;
    let mut job = Job::file(&path, &settings, true).unwrap();
    run(&mut job, Level::Normal, &options_with(&settings), &env);
    assert!(path.with_file_name("doc_optimised.pdf").exists());
    assert_eq!(fs::read(&path).unwrap(), input);
}

#[test]
fn image_only_edits_are_dropped_on_pdfs() {
    let env = Env::new();
    let (path, _) = pdf_file(&env, "doc.pdf", &[PdfPage::default()], &PdfExtras::default());
    let mut job = Job::file(&path, &Settings::default(), true).unwrap();
    job.edit(scale(50));
    assert_eq!(job.edits.scale, None);
}
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core pdf::`, `cargo test -p ferry-core pipeline`
Expected: FAIL.

- [ ] **Step 3: Implement as specified**

The `place_result` extraction must leave every existing pipeline test green. Run `cargo test -p ferry-core pipeline` after the extraction alone, before adding the PDF branch.

Append to the `pdf.*` block:

```json
  "pdf.error.signed": "This PDF is signed — changing it would break the signature",
  "pdf.step.pdf": "Recompressing PDF images",
  "pdf.detail.page": "1 page",
  "pdf.detail.pages": "{n} pages",
  "pdf.detail.dpi": "{n} dpi",
  "pdf.detail.cropped": "cropped",
```

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

**Manual check (ask first):** copy a PDF in Finder and press ⌃⇧C. The card shows page 1 as its thumbnail, a "3 pages" badge, and "3 pages · 150 dpi". Open the PDF in Preview: text, links and outline are intact.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core src/lib/i18n/en.json
git commit -m "feat(core): PDF jobs with structural optimise, encrypted and signed refusals, PDF output rules"
```

### Task 14: Downsample PDF images to a DPI

**Files:**
- Modify: `crates/ferry-core/Cargo.toml` (`flate2 = "1.1"`, `rayon = "1.12"`)
- Create: `crates/ferry-core/src/pdf/images.rs`; `pdf/mod.rs` (`mod images;`; `optimise` calls `images::recompress_all` before pruning)
- Modify: `crates/ferry-core/src/optimise/jpeg.rs` (`encode_gray`, appended before `#[cfg(test)]` under `// sE pdf-formats`)

**Interfaces:**
- Consumes: lopdf `Document`, `Stream`, `Object`, `content::Content`; `raster::cut`; `jpeg::{decode_raster, encode_raster}`; `optimise::quality`.
- Produces:
  - `images::concat(m: [f32; 6], ctm: [f32; 6]) -> [f32; 6]`: PDF's `cm` rule, new CTM = `m × ctm`:

    ```text
    a = m.a*c.a + m.b*c.c          b = m.a*c.b + m.b*c.d
    c = m.c*c.a + m.d*c.c          d = m.c*c.b + m.d*c.d
    e = m.e*c.a + m.f*c.c + c.e    f = m.e*c.b + m.f*c.d + c.f
    ```
  - `images::image_dpis(doc: &Document) -> HashMap<ObjectId, f32>`:
    - for each page, take its content (`get_page_content`), `Content::decode`, and the resources dict (the page's own `/Resources`, else the nearest ancestor's);
    - walk the operators with a CTM stack: `q` pushes, `Q` pops (never below the start), `cm` concatenates, `Do /Name` looks up `/XObject`;
    - for an Image, `w_pt = hypot(a, b)` and `h_pt = hypot(c, d)`. Skip the use when either is below 0.01. `dpi = min(px_w × 72 / w_pt, px_h × 72 / h_pt)`; keep the minimum per ObjectId;
    - for a Form, recurse with `concat(form /Matrix or identity, ctm)`, the form's `/Resources` (else the parent's) and its decoded content. Stop at depth 12, and skip a form already on the current chain;
    - inline images and images never drawn are absent.
  - `images::photo_like(pixels: &[u8], channels: usize) -> bool`: samples at most 65 536 pixels at an even stride; true when there are 1024 or more distinct colours.
  - `images::DOWNSAMPLE_SLACK: f32 = 1.1`.
  - `images::recompress_all(doc: &mut Document, target_dpi: u16, o: &OptimiseOptions, progress: &mut dyn FnMut(f32)) -> Result<usize, OptimiseError>`:
    - collect candidate image XObjects;
    - recompress them in parallel with `rayon` (`par_iter`, cancel checked per image → `Err(Cancelled)`);
    - write the replacements back in ObjectId order, calling `progress(done / total)` as each finishes (from the collecting thread);
    - return the number replaced.
  - Candidates: `/Subtype /Image`, `/BitsPerComponent 8`, no `/ImageMask true`, no `/Mask`, no `/Decode`, no `/SMask` that has `/Matte`.
    - `/ColorSpace` is `/DeviceRGB`, `/DeviceGray`, or `[/ICCBased ref]` with `/N` 1 or 3 (the colour space object is kept as is).
    - `/Filter` is absent, `/FlateDecode`, `/DCTDecode`, or a one-element array of those.
    - A DCT image must not be CMYK (`decode_raster` is not called on anything with a 4-component colour space).
    - `width × height ≤ MAX_PIXELS`.
  - One image:
    - `f = target / dpi` when `dpi > target × DOWNSAMPLE_SLACK`, else 1; the new size is `round(side × f)`, at least 1;
    - DCT source → JPEG at `quality(o)`;
    - Flate/raw source → JPEG when `photo_like` (on the resized pixels), else Flate (`flate2`, `Compression::best()`);
    - Gray stays Gray (`jpeg::encode_gray`; Flate keeps one channel);
    - an `/SMask` is resized to the same size when the image is, and Flate-encoded;
    - the dict gets `Width`, `Height`, `BitsPerComponent 8`, `Filter`, and loses `DecodeParms`; everything else stays;
    - the replacement is used only when image + smask bytes are smaller than before.
  - `jpeg::encode_gray(width: u32, height: u32, luma: &[u8], o: &OptimiseOptions) -> Result<Vec<u8>, OptimiseError>`: mozjpeg `JCS_GRAYSCALE`, progressive, optimised coding, `quality(o)`.

- [ ] **Step 1: Write the failing tests** (`pdf/images.rs`; `width_of(pdf, n)` reads image n's `/Width` with lopdf, a test helper)

```rust
#[test]
fn concat_scales_and_nests() {
    let id = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
    assert_eq!(concat([720.0, 0.0, 0.0, 360.0, 0.0, 0.0], id), [720.0, 0.0, 0.0, 360.0, 0.0, 0.0]);
    assert_eq!(concat([720.0, 0.0, 0.0, 360.0, 10.0, 0.0], [0.5, 0.0, 0.0, 0.5, 0.0, 0.0]), [360.0, 0.0, 0.0, 180.0, 5.0, 0.0]);
}

#[test]
fn an_image_drawn_across_ten_inches_is_three_hundred_dpi() {
    let pdf = testutil::pdf(&[PdfPage { images: vec![PdfImage { px: (3000, 1500), drawn: (720.0, 360.0), kind: ImageKind::DctRgb }], ..PdfPage::default() }], &PdfExtras::default());
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert_eq!(image_dpis(&doc).values().copied().collect::<Vec<_>>(), [300.0]);
}

#[test]
fn a_shared_image_keeps_the_dpi_of_its_largest_use() {
    // shared_image: page 1 draws the image 720 pt wide, page 2 draws the same object 360 pt wide.
    let pdf = testutil::pdf(&[PdfPage { images: vec![PdfImage { px: (3000, 1500), drawn: (720.0, 360.0), kind: ImageKind::DctRgb }], ..PdfPage::default() }, PdfPage::default()], &PdfExtras { shared_image: true, ..Default::default() });
    let doc = lopdf::Document::load_mem(&pdf).unwrap();
    assert_eq!(image_dpis(&doc).values().copied().collect::<Vec<_>>(), [300.0]);
    let out = crate::pdf::optimise(&pdf, 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(width_of(&out, 0), 1500);
}

#[test]
fn a_300_dpi_scan_comes_down_to_150() {
    let pdf = scan(2550, 3300, ImageKind::DctRgb); // helper: one letter page, image drawn full page
    let out = crate::pdf::optimise(&pdf, 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert!((1274..=1276).contains(&width_of(&out, 0)));
    assert!(out.len() < pdf.len());
}

#[test]
fn an_image_just_above_the_target_keeps_its_size() {
    let out = crate::pdf::optimise(&scan(1360, 1760, ImageKind::DctRgb), 150, &OptimiseOptions::default(), &mut |_| {}).unwrap(); // 160 dpi
    assert_eq!(width_of(&out, 0), 1360);
}

#[test]
fn a_screenshot_stays_lossless_and_a_photo_becomes_jpeg() {
    let shot = crate::pdf::optimise(&scan(2550, 3300, ImageKind::FlateScreenshot), 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(filter_of(&shot, 0), b"FlateDecode");
    let photo = crate::pdf::optimise(&scan(2550, 3300, ImageKind::FlatePhoto), 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(filter_of(&photo, 0), b"DCTDecode");
}

#[test]
fn gray_stays_gray() {
    let out = crate::pdf::optimise(&scan(2550, 3300, ImageKind::DctGray), 150, &OptimiseOptions::default(), &mut |_| {}).unwrap();
    assert_eq!(colour_space_of(&out, 0), b"DeviceGray");
}

#[test]
fn images_the_engine_cannot_handle_are_left_alone() {
    for kind in [ImageKind::DctCmyk, ImageKind::OneBit, ImageKind::Jpx] {
        let pdf = scan(2550, 3300, kind);
        let out = crate::pdf::optimise(&pdf, 72, &OptimiseOptions::default(), &mut |_| {}).unwrap();
        assert_eq!(image_bytes(&out, 0), image_bytes(&pdf, 0), "{kind:?}");
    }
}

#[test]
fn photo_like_counts_colours() {
    assert!(!photo_like(&[10, 20, 30].repeat(10_000), 3));
    let gradient: Vec<u8> = (0..90_000_u32).flat_map(|i| [(i % 251) as u8, (i / 251 % 251) as u8, (i % 7) as u8]).collect();
    assert!(photo_like(&gradient, 3));
}

#[test]
fn a_cancel_stops_recompressing() {
    let o = OptimiseOptions::default();
    o.cancel.cancel();
    assert!(matches!(crate::pdf::optimise(&scan(2550, 3300, ImageKind::DctRgb), 150, &o, &mut |_| {}), Err(OptimiseError::Cancelled)));
}
```

Add the test helpers `scan`, `width_of`, `filter_of`, `colour_space_of` and `image_bytes` in the test module, using lopdf. Add one pipeline test, `aggressive_and_the_dpi_recipe_choose_the_resolution`: a 300 dpi scan job run aggressive gives width ≈ 850 (100 dpi); after `job.edit(dpi 72)` it gives ≈ 612.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core images`
Expected: FAIL.

- [ ] **Step 3: Implement as specified**

- [ ] **Step 4: Run the tests**

Run: `cargo test -p ferry-core`, `cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS.

**Manual check (ask first):** a 20+ MB scanned PDF shrinks substantially and looks right at 100 % in Preview. A 400-page PDF finishes, and Activity Monitor shows more than one core busy during the run.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core Cargo.lock
git commit -m "feat(core): downsample and recompress PDF images to a DPI"
```
