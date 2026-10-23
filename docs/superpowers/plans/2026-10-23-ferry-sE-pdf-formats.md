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
