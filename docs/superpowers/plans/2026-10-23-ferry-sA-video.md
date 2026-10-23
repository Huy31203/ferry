# Stream A (video) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ferry optimises, downscales, crops, converts, recompresses, speeds up, mutes and turns into GIF any MP4, MOV, MKV or WebM video, with hardware encoders where present and software fallback (HEVC is written by hardware encoders only), from the same result cards, recipe, backups and compare window as images. It ships a pinned ffmpeg sidecar and the `ferry_core::ffmpeg` runner that streams B (audio) and F (pipelines) build on.

**Architecture:** `ferry_core::ffmpeg` runs the bundled `ffmpeg` sidecar. It drains its output continuously, reports progress from `-progress pipe:1`, and kills it on cancel, on a stall or when the app quits. A new `ferry_core::video` module holds pure logic: a probe parser for `ffmpeg -i` output, hardware-encoder detection, and an encode planner that turns the Cycle 2 `Edits` recipe into ffmpeg arguments. `pipeline::Job` stays the single job model. A video is a file job whose `Format` is one of six new video variants. Its run goes through a child module, `pipeline/video_job.rs`, that works on files rather than bytes, and the result is still placed by the existing `put` / `put_back` / output rules. The UI reuses the card, rail, sliders, crop and fit popovers, format bar, context menu and compare window, and adds a speed popover and a two-player video compare.

**Tech Stack:** Rust 2024 (`ferry-core`, `src-tauri`), Tauri 2.12, Svelte 5 + TypeScript, Vitest. New binary sidecar: FFmpeg 9.0.2, built from pinned sources with x264, libvpx, SVT-AV1, dav1d, Opus and LAME (plus nv-codec-headers, AMF headers and libvpl on Windows). **No x265**: HEVC output uses the OS/GPU hardware encoders only (decided 2026-10-23). No new crates or npm packages, except that `windows-sys` 0.61, already in `Cargo.lock`, becomes a Windows-only dependency of `ferry-core`.

**Spec:** [stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding; it wins over the requirements), [requirements](../../requirements/ferry-optimize-requirements.md) (R§; video parts: §1 toolchain, §2.3 Video row, §2.4, §2.5, §2.6, §3.1 NFR-P5/P6, §3.2 NFR-R3/R4, §3.3, §4.3, §4.7 speed popover, §4.11, §4.12 Videos pane, §5 hardware encode row), [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md) (C1§), [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) (C2§).

## Global Constraints

- **Out of scope:** secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP. Also: pipelines and step kinds (stream F), "Strip metadata" and "Re-optimise with encoder ▸" menu items (F), watchers (C), drop zone and batch (D), audio optimisation (B), automatic optimisation of copied video files (FR-T3; an integration task on `feat/c3-all` after A and I merge).
- **Shared files** (brief): add at the named anchor given in the "Shared-file edits" table; never reorder or reformat existing lines. In Rust and TS put a `// stream A: video` comment line above each block you add. In JSON, add one contiguous block (see i18n). Never hand-edit `Cargo.lock`; let cargo update it.
- **New `Format` variants never fall into a wildcard.** Every `match` on `sniff::Format` lists the six video variants explicitly (`Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => …`), so stream E's new image variants still force their own decision.
- **Original is sacred** (R§3.3): back up before any change; write results to a temp file and then atomically rename them into place; a failure leaves the source untouched. Every run starts from the backup and never compounds (R§2.4; "2× then 1.5×" gives 1.5×).
- **Never worse** for a plain optimise (a recipe with only `level`): a result that is not at least 1% smaller leaves the original (C1 rules). Any explicit edit always writes its result (C2§2).
- **Keep every source frame and all audio tracks** (R§2.3, R§3.3) unless the user chose "Drop frames", a frame-rate cap, or "Remove audio". Video frame sizes are even (yuv420).
- **Large files stay on disk.** A video is never read into memory. It moves between backup, ffmpeg and the destination as files. Only a GIF result (bounded by the 800 px cap) is held in memory for gifsicle.
- **No overall timeout on ffmpeg.** It is killed on cancel, on app exit, or after `ffmpeg::STALL` (120 s) with no output on either pipe.
- **Windows code that macOS cannot compile** stays behind `#[cfg(windows)]` and is listed in "Windows-only code" below. CI builds it; this Mac does not.
- **Strings:** every new UI string goes in `src/lib/i18n/en.json` under `video.*`, with the exact copy in the i18n table below. Rust gets it through `ferry_core::i18n::{t, tf}`. Never hard-code English in Rust card text.
- **JavaScript/TypeScript:** never use `var`; use `const`/`let`.
- `cargo fmt --all --check` and `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check` and `pnpm test` must be clean after every task. `cargo test` takes one name filter per command.
- **Live app:** ask the user before running `pnpm tauri dev` (it takes over the real clipboard). When driving the UI, keep focus in a TextEdit window; never click into other apps.
- **Git:** work on branch `feat/c3-sA-video` in its own worktree from `main`. Do not push, tag or merge without asking. End each commit with the trailer your session's instructions give; the handover names `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`.

## Review Focus

1. **Portrait phone clips (display-matrix rotation of ±90°)**: a person crops a vertical iPhone video to 9:16 and expects the crop in what they see, not in sensor orientation. Pinned in Task 6 (`a_rotated_stream_reports_display_dimensions`) and Task 10 (`a_portrait_clip_is_cropped_as_it_is_seen`).
2. **10-bit HDR HEVC (iPhone HLG / Dolby Vision)**: a plain optimise keeps HEVC Main 10 with its colour tags, through the hardware HEVC encoder (every mode: there is no software HEVC encoder). Converting to H.264, WebM or GIF writes the result and shows the amber note "HDR colours flattened to SDR". Pinned in Task 8 (`ten_bit_hdr_stays_main10_on_plain_hevc`, `hdr_to_h264_is_flagged_flattened`).
3. **Odd frame sizes from a crop or scale** (50% of 1366×767, a 1.91:1 crop of 1080p): yuv420 encoders reject odd sizes, so a person expects a result, not an encoder error. Pinned in Task 8 (`frames_are_rounded_down_to_even_sizes`).
4. **Disk full or a broken file mid-encode**: expected: an Error card with a plain reason ("Not enough disk space to encode" or "Couldn't encode the video — see Details"), the source unchanged, and no temp folder left behind (NFR-R4). Pinned in Task 1 (`a_failure_carries_the_code_and_the_end_of_stderr`) and Task 10 (`a_broken_video_is_an_error_and_leaves_no_temp_folder`).
5. **Several audio tracks, no audio, or cover art in a video file**: every audio track survives. A silent clip gets no audio track invented. An attached cover picture is never taken as the video. Pinned in Task 6 (`an_attached_picture_is_not_the_video`), Task 8 (`every_audio_track_is_mapped_and_none_is_required`) and Task 10 (`two_audio_tracks_survive_an_optimise`).
6. **No hardware HEVC encoder** (an older Windows GPU, a VM, the hardware HEVC encode failing, e.g. 10-bit on an older Intel Mac): the build has no x265, so a person must never get an encoder error or a silent downgrade. Expected: the HEVC chip and "Convert to ▸ HEVC" are not offered; a recipe that still asks for HEVC (a pipeline, a hardware attempt that failed) is written as H.264 with the note "Saved as H.264 — this computer has no HEVC encoder"; a **plain** optimise of an HEVC video leaves the file as it is ("already optimal") instead of turning it into larger, SDR H.264. Pinned in Task 8 (`hevc_without_a_hardware_encoder_is_written_as_h264`, `a_plain_hevc_optimise_never_becomes_h264`), Task 10 (`a_hevc_request_without_a_hevc_encoder_is_saved_as_h264`), Task 11 (`without_a_hevc_encoder_hevc_is_not_offered`) and Task 12 (`hides_hevc_without_a_hardware_encoder`).

---

## Decisions

### ffmpeg build and licence

| Topic | Decision | Why |
| --- | --- | --- |
| Version | **FFmpeg 9.0.2** (git tag `n9.0.2`, commit `946fcce07b6dcd0331c8cc609192aeff5e1924f8`, 2026-09-18), the newest point release on 2026-10-23 | Newest stable release. Its `configure` accepts every library below (checked: SVT-AV1 ≥ 4.0 API, libvpl ≥ 2.6, AMF ≥ 1.5.2, ffnvcodec ≥ 12.1.14.0) |
| Licence of the build | **GPL-3.0-or-later**: `--enable-gpl --enable-version3`, never `--enable-nonfree` | x264 is GPL-2.0-or-later, and FFmpeg's LICENSE.md says it forces `--enable-gpl`. It is the only good software H.264 encoder, which the "Small" and "Visually lossless" modes need for H.264 (R§2.4). Ferry is already GPL-3.0-or-later (imagequant, gifsicle; C1§2), so a GPL ffmpeg adds no new obligation in kind. `--enable-version3` matches Ferry's GPL-3 and keeps Apache-2.0 libraries possible later. Without `nonfree` the binary stays redistributable |
| Hardware encoders | macOS: VideoToolbox (`h264_videotoolbox`, `hevc_videotoolbox`; system framework). Windows: NVENC (`*_nvenc`, nv-codec-headers, MIT, runtime from the NVIDIA driver), Quick Sync (`*_qsv`, libvpl, MIT), AMF (`*_amf`, headers MIT, runtime from the AMD driver) | NFR-P5. None of these forces GPL; they are LGPL-compatible. The GPL licence comes only from x264 |
| Media Foundation | **Not enabled** (`h264_mf` / `hevc_mf` are left out) | FFmpeg links `mfplat.dll` directly. Windows "N" editions lack it, so `ffmpeg.exe` would not even start there. NVENC/QSV/AMF cover the same GPUs and load their runtimes on demand |
| Software encoders | libx264 (H.264), libsvtav1 (AV1), libvpx (VP9 for WebM). **No libx265**: never fetch, build, link or `--enable-libx265` it | R§2.3/§4.3 format bar: MP4, HEVC, AV1, WebM, GIF. x265 dropped by the user's decision (patents: the HEVC pools are the more aggressive) |
| HEVC output | **Hardware encoders only**: `hevc_videotoolbox` (macOS), `hevc_nvenc` / `hevc_qsv` / `hevc_amf` (Windows). Every encoder mode (Fast, Small, Visually lossless) uses the hardware HEVC encoder for HEVC. **Where no hardware HEVC encoder works, HEVC is not offered** (no HEVC chip, no "Convert to ▸ HEVC"), and anything that still needs HEVC **falls back to H.264** (libx264, or the hardware H.264 encoder in Fast) with the card note `video.note.hevc_as_h264`. One exception: a **plain** optimise of an HEVC video is never turned into H.264; with no hardware HEVC encoder it leaves the file as it is (`AlreadyOptimal`). Linux (CI only) has no hardware encoders, so HEVC always takes the fallback there | VideoToolbox, NVENC, Quick Sync and AMF cover HEVC on most current machines, and the OS or GPU vendor normally carries the HEVC patent licence. H.264 at the same quality is larger and 8-bit, so a plain optimise would only lose (size and HDR) |
| Decoders | FFmpeg native, plus libdav1d (AV1) | Native AV1 decoding needs a hardware accelerator, and R§2.3 accepts AV1 input |
| Audio | native `aac`, libopus (WebM/AV1 audio; B's Opus), libmp3lame (B's MP3); native FLAC/ALAC/PCM | Stream B consumes this build. No `libfdk_aac` (non-free) |
| GIF export | ffmpeg `palettegen`/`paletteuse`, then the bundled gifsicle `-O3 --lossy` | R§1 names gifski (AGPL-3.0 or commercial). This route needs no new AGPL sidecar and reuses gifsicle. Decided 2026-10-23: no gifski |
| Programs | `ffmpeg` only (`--disable-ffprobe --disable-ffplay`); probing parses `ffmpeg -i` stderr | Stays under NFR-R3 (installer < 120 MB). A second fully linked binary would roughly double the size |
| Linking | Static external libraries. macOS links only system libraries and frameworks. Windows `ffmpeg.exe` is built with MSYS2 MINGW64 `-static` and depends on no `libwinpthread`, `libstdc++` or `libgcc` DLL | One self-contained file per triple, like gifsicle |
| Integrity | Every git source is pinned by commit and verified with `git rev-parse`. LAME 3.100 is a tarball checked by SHA-256 | Same rule as `build-gifsicle.sh` (C1§9) |
| Attribution | `THIRD-PARTY-NOTICES.md` (repo root) plus each component's licence files, shipped in the bundle under `licenses/`. It includes the IJG credit FFmpeg's LICENSE.md requires | GPL §4/§6, BSD/MIT notice clauses |
| Corresponding source | `release.yml` attaches `ferry-<tag>-third-party-sources.tar.xz` (every pinned source plus `scripts/`) to each GitHub Release | GPL-3 §6(d): equivalent access from the same place as the binaries |

**Patent-encumbered (decided 2026-10-23, option (b)):** H.264 (Via LA AVC pool), HEVC (Access Advance, Via LA and independent licensors), AAC (Via LA AAC; core patents mostly expired). Shipping a software encoder makes Ferry a distributor of an encoder; with the OS or GPU hardware encoders, the OS or GPU vendor normally carries that licence. So x265 is **not** shipped and HEVC is encoded by hardware only; x264 (H.264) and native AAC stay, with legal advice before 1.0. HEVC *decoding* (FFmpeg's native decoder) stays, so HEVC sources can still be read, converted and thumbnailed. VP9, AV1 and Opus come with royalty-free grants (Google, AOM, Xiph), though third-party pools (Sisvel, Avanci Video) assert patents. MP3 patents have expired.

### Pinned sources

| Component | Version | Source | Pin | Licence |
| --- | --- | --- | --- | --- |
| FFmpeg | 9.0.2 | `https://github.com/FFmpeg/FFmpeg.git`, tag `n9.0.2` | `946fcce07b6dcd0331c8cc609192aeff5e1924f8` | GPL-3.0-or-later as configured (sources LGPL-2.1+/GPL-2+) |
| x264 | stable branch | `https://code.videolan.org/videolan/x264.git` (mirror `https://github.com/mirror/x264.git`) | `31e19f92f00c7003fa115047ce50978bc98c3a0d` | GPL-2.0-or-later |
| libvpx | 1.17.0 | `https://github.com/webmproject/libvpx.git`, tag `v1.17.0` | `6df3ec34557879fff673706f4a1d9fbd0f3a6f0e` | BSD-3-Clause + Google patent grant |
| SVT-AV1 | 4.2.0 | `https://gitlab.com/AOMediaCodec/SVT-AV1.git`, tag `v4.2.0` | `9292ec8e32bce26f781f277ec8739b53426c4300` | BSD-3-Clause-Clear + AOM Patent License 1.0 |
| dav1d | 1.5.4 | `https://code.videolan.org/videolan/dav1d.git` (mirror `https://github.com/videolan/dav1d.git`), tag `1.5.4` | `54706fc6bc0cdecab7e9593974a4039cc038fca7` | BSD-2-Clause |
| Opus | 1.6.1 | `https://github.com/xiph/opus.git`, tag `v1.6.1` | `22244de5a79bd1d6d623c32e72bf1954b56235be` | BSD-3-Clause |
| LAME | 3.100 | `https://downloads.sourceforge.net/project/lame/lame/3.100/lame-3.100.tar.gz` | SHA-256 `ddfe36cab873794038ae2c1210557ad34857a4b6bdc515785d1da9e175b1da1e` | LGPL-2.0-or-later |
| nv-codec-headers (Windows) | 12.1.14.0 | `https://github.com/FFmpeg/nv-codec-headers.git`, tag `n12.1.14.0` | `1889e62e2d35ff7aa9baca2bceb14f053785e6f1` | MIT. The oldest version FFmpeg 9.0 accepts, so the NVIDIA driver floor stays lowest (≥ 531.61) |
| AMF headers (Windows) | 1.5.2 | `https://github.com/GPUOpen-LibrariesAndSDKs/AMF.git`, tag `v1.5.2`, sparse `amf/public/include` | `eadd00804d5f7e5cd8c85d540073198312870776` | MIT. The minimum FFmpeg 9.0 accepts |
| libvpl (Windows) | 2.17.0 | `https://github.com/intel/libvpl.git`, tag `v2.17.0` | `d77f9195cf495b937631607333288fd917ae8939` | MIT |

If the LAME checksum does not match on first download, stop and ask. Do not overwrite it. Tagged repos are fetched with `git fetch --depth 1 origin refs/tags/<tag>:refs/tags/<tag>`, so `git describe` works for builds that need it (Opus). x264 is fetched by commit.

### Video model

| Topic | Decision |
| --- | --- |
| Formats | `sniff::Format` gains `Mp4, Hevc, Av1, WebM, Mov, Mkv` (serde `mp4 hevc av1 webm mov mkv`; labels `MP4 HEVC AV1 WebM MOV MKV`; extensions `mp4 mp4 mkv webm mov mkv`). Sniffing by content gives the container (`Mp4`, `Mov`, `Mkv`, `WebM`). After the first probe, the job's source format is refined: MP4 + HEVC → `Hevc`, MP4/MKV + AV1 → `Av1` |
| Convert targets (format bar, "Convert to ▸") | `Mp4` (H.264), `Hevc` (H.265 in MP4, tag `hvc1`), `Av1` (AV1 in MKV, R§2.3), `WebM` (VP9 + Opus), `Gif` (video to GIF). `Mov`/`Mkv` are never targets |
| Plain optimise target | Same as the source format, except a `Mov` with `video.convert_mov` on becomes `Hevc` (HEVC source) or `Mp4`: a Converted result that, In place, replaces the `.mov` (R§2.3). A target equal to the source keeps the source's own extension (`.m4v` stays `.m4v`) |
| Recipe | Cycle 2 `Edits` plus `speed: Option<Speed>`, `mute: bool` (stream A), and stream F's `encoder: Option<optimise::Encoder>` (see "Encoder choice" below). Crop, downscale, compression, fit-under and format reuse the image fields and geometry |
| Encoder modes | `video.encoder` (type `optimise::Encoder`): **Fast** (default; hardware H.264/HEVC, else software at fast presets), **Small** (software, slow presets), **Visually lossless** (software, fixed high quality; ignored when the recipe sets compression or fit-under). **HEVC is the exception:** it has no software encoder, so every mode uses the hardware HEVC encoder (Visually lossless aims its bitrate at quality 95); see "HEVC output" above |
| Hardware detection | A 1-second test encode per candidate, run once on a background thread at startup (R§5), cached for the session. A video job waits for that probe to finish before planning, so a clip copied right after launch still finds the HEVC encoder. `Hardware.hevc == None` means this machine cannot write HEVC. A hardware H.264 encode that fails is retried once in software (NFR-P5); a hardware HEVC encode that fails is retried once as software H.264 (or, for a plain optimise of an HEVC source, the file is left as it is) |
| Concurrency | At most one video job encodes at a time (`AppState::video_permits`, 1 permit), on top of the per-CPU permits. This respects consumer GPU session limits (NFR-P6), and x264/SVT use every core themselves |
| Thumbnail | First frame, at most 600 px wide, JPEG, stored as `.ferry-thumb.jpg` in the job's backup folder (already inside the asset-protocol scope and pruned with the backup) |
| Speed | `setpts=PTS*100/P` and `atempo=P/100`, with P = percent 50..=400. "Keep frames" leaves every frame (`-fps_mode passthrough`). "Drop frames" adds `fps=<source fps>` |
| GIF | 15 fps, long edge capped at 800 px unless the recipe sets a size or scale, `palettegen=stats_mode=diff`, `paletteuse=dither=sierra2_4a:diff_mode=rectangle`, `-loop 0`, then gifsicle at the recipe's quality |
| Subtitles, data tracks | Subtitles are copied only when the result keeps the source's container; otherwise dropped. Timecode/data tracks are dropped |
| Metadata | `-map_metadata 0`. MP4/MOV get `-movflags +faststart+use_metadata_tags`. Extended attributes (screen-recording marker) and, with `video.preserve_dates`, dates are carried by the existing `write_file` |

### Encoder choice (shared with stream F)

Stream F (pipelines, plan `2026-10-23-ferry-sF-pipelines.md`, its Task 1) owns the encoder choice. A **does not define its own encoder enum**; it uses F's, for the `video.encoder` setting and for the recipe:

```rust
// crates/ferry-core/src/optimise/mod.rs (F's definition, exactly)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoder { Fast, Small, VisuallyLossless }        // serde: "fast", "small", "visually_lossless"

// crates/ferry-core/src/edit/mod.rs (F's fields, exactly)
pub struct Edits { …, pub encoder: Option<Encoder> }                          // None: the settings' choice
pub struct EditsPatch { …, #[serde(deserialize_with = "present")] pub encoder: Option<Option<Encoder>> }
// merge: `if let Some(encoder) = patch.encoder { self.encoder = encoder; }` (set or clear)
// is_plain(): NOT affected by `encoder` (F's rule: a plain run with an encoder still follows never-worse)
```

```ts
// src/lib/ipc.ts, interface Edits (F's field, exactly)
encoder: 'fast' | 'small' | 'visually_lossless' | null;
```

**A and F are built in parallel worktrees from `main`, so F's definition may not be on A's branch yet.** Check first: `grep -n "pub enum Encoder" crates/ferry-core/src/optimise/mod.rs` and `grep -n "pub encoder" crates/ferry-core/src/edit/mod.rs`.
- **Present** (F merged first): use it as it is. Add nothing, rename nothing.
- **Absent:** A adds it **verbatim as above**, in the same files, with the same names, derives and serde, and nothing else of F's (no `OptimiseOptions.encoder`/`adaptive`, no `strip_metadata`, no `from_recipe`, no image-encoder mapping; F's Task 1 owns those). Put the enum right after `pub enum Level` in `optimise/mod.rs`, and the `Edits`/`EditsPatch`/`merge` lines at the end of each, under a `// stream F: pipelines (encoder; added early by stream A, identical to F's Task 1)` comment line, where F's Task 1 puts them. The enum gets **no** `Default` derive (F's has none); `Video::default()` names `Encoder::Fast` itself.

Whichever stream lands second then has nothing to change: the two copies are identical, so the integrator keeps one copy of each line and drops the other. Do not add a `VideoEncoder` type or any second encoder enum.

### Hooks for other streams

- **B (audio)** may start once Task 1 is merged. It uses `ferry_core::ffmpeg::{run, run_expecting, capture, FerryError}`. `capture` returns the full stderr, which holds the `loudnorm` JSON. The binary from Task 2 contains `aac`, `libopus`, `libmp3lame`, `flac`, `alac`, `pcm_*`, `mjpeg`/`png` (cover art), the `loudnorm`/`atempo` filters and the `mp3`/`ipod`/`opus`/`flac`/`wav`/`image2` muxers. The `ftyp` brands `M4A `, `M4B `, `M4P ` are not claimed as video (Task 4), so B can sniff them.
- **C (watchers)**, merge-time step after both land: in `src-tauri/src/watch/hooks.rs` set `pub const VIDEO: bool = true;`. Replace the body of `video_job` with `Some(Job::file(path, settings, app.state::<AppState>().gifsicle.is_some()))`, because `Job::file` accepts videos after Task 10.
- **D (drop zone, batch)** calls `Job::file`; videos need nothing extra. `ferry_core::video::is_video_file(path)` tells the caller whether a file is a video (from its first 64 bytes).
- **F (pipelines)** later registers video step kinds in its `crates/ferry-core/src/pipeline_steps.rs` registry. Each one only builds an `EditsPatch` from this plan's fields: `changeSpeed(factor, frames)` → `speed: Some(Some(Speed { percent, keep_frames }))`, `removeAudio` → `mute: Some(true)`, `convert(to)` → `format: Some(Some(Format::Mp4 | Hevc | Av1 | WebM | Gif))`, `optimise(encoder)` → `encoder: Some(Some(optimise::Encoder::…))` (F's own enum and field, shared as above), and downscale/crop as for images. F's "Re-optimise with encoder ▸" menu item uses `encoder` too, so on a video card it re-runs with that mode. `convert(hevc)` on a machine without a hardware HEVC encoder writes H.264 with the `video.note.hevc_as_h264` note (Task 10). Merge note: if F's `EditsPatch::from_recipe` exists when A merges (or A's fields when F merges), it must also copy `speed` and `mute`. Apart from the shared `Encoder` lines above, nothing in this plan touches F's files.
- **E (pdf-formats)** also adds `Format` variants. At merge, keep both blocks in the enum and in every `match`.

## New dependencies

| Dependency | Kind | Version | Licence | Flag |
| --- | --- | --- | --- | --- |
| FFmpeg (sidecar `ffmpeg-<triple>`) | binary, built in CI and locally | 9.0.2 | **GPL-3.0-or-later** (configured) | GPL; links the patent-encumbered x264 (H.264) and native AAC encoders. HEVC is encoded only through OS/GPU hardware encoders (no x265) |
| x264 | static library in the sidecar | stable `31e19f92` | **GPL-2.0-or-later** | GPL; H.264 patents |
| libvpx | static library | 1.17.0 | BSD-3-Clause + patent grant | third-party VP9 pool claims |
| SVT-AV1 | static library | 4.2.0 | BSD-3-Clause-Clear + AOM patent licence | third-party AV1 pool claims |
| dav1d | static library | 1.5.4 | BSD-2-Clause | |
| Opus | static library | 1.6.1 | BSD-3-Clause | |
| LAME | static library | 3.100 | **LGPL-2.0-or-later** | LGPL (statically linked into a GPL-3 binary: compatible) |
| nv-codec-headers | headers (Windows) | 12.1.14.0 | MIT | |
| AMF headers | headers (Windows) | 1.5.2 | MIT | |
| libvpl | static library (Windows) | 2.17.0 | MIT | |
| `windows-sys` | crate, `ferry-core` `[target.'cfg(windows)'.dependencies]` | 0.61 (already in `Cargo.lock`), features `Win32_Foundation`, `Win32_System_JobObjects`, `Win32_System_Threading` | MIT OR Apache-2.0 | |
| `actions/cache` | CI action | `v6` | MIT | |
| `msys2/setup-msys2` | CI action | `v2` | MIT | |
| Homebrew `nasm pkgconf meson ninja` (macOS build tools) | build-time only | runner's current | BSD-2 / GPL-2+ / Apache-2.0 / Apache-2.0 | build tools, not shipped |
| MSYS2 `mingw-w64-x86_64-{gcc,nasm,cmake,meson,ninja,pkgconf,zlib}`, `make`, `git`, `diffutils` | build-time only | runner's current | various | zlib is linked statically (zlib licence) |

## Settings keys (new `[video]` section, `#[serde(default)]`, no version bump)

| Key | Type | Default | Notes |
| --- | --- | --- | --- |
| `video.encoder` | `"fast" \| "small" \| "visually_lossless"` | `"fast"` | Rust type `ferry_core::optimise::Encoder`, stream F's enum (see "Encoder choice") |
| `video.compression` | u8 5..=100 | `80` | Clamped |
| `video.convert_mov` | bool | `true` | |
| `video.audio_to_aac` | bool | `false` | |
| `video.remove_audio` | bool | `false` | Automatic transform on every run |
| `video.fps_cap` | `"off" \| "24" \| "30" \| "60"` | `"off"` | |
| `video.speed_frames` | `"keep" \| "drop"` | `"keep"` | The speed popover's starting toggle |
| `video.preserve_dates` | bool | `true` | |
| `video.ignored_extensions` | string[] | `[]` | Trimmed, empty entries dropped |
| `video.max_size_mb` | u32 ≥ 1 | `4000` | |
| `video.output.optimised` | OutputRule | `{ location: "in_place", folder: "~/Movies/Ferry" (Windows `"~/Videos/Ferry"`), template: "%f_optimised.%e" }` | Template validated like images |
| `video.output.converted` | OutputRule | `{ location: "in_place", folder: same, template: "%f.%e" }` | |

## i18n keys (all new; one contiguous block at the end of `src/lib/i18n/en.json`, before the closing `}`)

| Key | English | Task |
| --- | --- | --- |
| `video.error.ffmpeg_missing` | `ffmpeg missing — reinstall Ferry` | 1 |
| `video.error.spawn` | `ffmpeg could not start — reinstall Ferry` | 1 |
| `video.error.encode` | `Couldn't encode the video — see Details` | 1 |
| `video.error.stalled` | `ffmpeg stopped responding` | 1 |
| `video.error.disk_full` | `Not enough disk space to encode` | 1 |
| `video.error.read` | `Couldn't read this video` | 6 |
| `video.error.no_video` | `No video track in {name}` | 6 |
| `video.step.backup` | `Backing up` | 10 |
| `video.step.reading` | `Reading video` | 10 |
| `video.step.encode` | `Encoding {codec}` | 10 |
| `video.step.encode_software` | `Encoding {codec} (software)` | 10 |
| `video.step.gif` | `Making GIF` | 10 |
| `video.step.progress` | `{step} · {percent} %` | 10 |
| `video.step.eta` | `{step} · {percent} % · {left} left` | 10 |
| `video.detail.fps` | `{n} fps` | 10 |
| `video.detail.speed` | `{n}×` | 10 |
| `video.detail.no_audio` | `no audio` | 10 |
| `video.note.hdr` | `HDR colours flattened to SDR` | 10 |
| `video.note.hevc_as_h264` | `Saved as H.264 — this computer has no HEVC encoder` | 10 |
| `video.menu.speed` | `Change speed…` | 11 |
| `video.menu.mute` | `Remove audio` | 11 |
| `video.menu.unmute` | `Keep audio` | 11 |
| `video.speed.title` | `Playback speed` | 12 |
| `video.speed.custom` | `Custom` | 12 |
| `video.speed.custom_placeholder` | `e.g. 1.75` | 12 |
| `video.speed.keep_frames` | `Keep frames` | 12 |
| `video.speed.drop_frames` | `Drop frames` | 12 |
| `video.speed.apply` | `Apply` | 12 |
| `video.speed.reset` | `Normal speed` | 12 |
| `video.speed.invalid` | `Use 0.5 to 4` | 12 |
| `video.card.duration` | `Duration {time}` | 12 |
| `video.pane` | `Videos` | 13 |
| `video.group` | `Videos` | 13 |
| `video.encoder` | `Encoder` | 13 |
| `video.encoder.help` | `How videos are re-encoded. HEVC always uses the hardware encoder.` | 13 |
| `video.encoder.fast` | `Fast` | 13 |
| `video.encoder.fast.help` | `Hardware encoder: quick and light on the CPU, larger files.` | 13 |
| `video.encoder.small` | `Small` | 13 |
| `video.encoder.small.help` | `Software encoder: slower, smallest files (HEVC stays on hardware).` | 13 |
| `video.encoder.visually_lossless` | `Visually lossless` | 13 |
| `video.encoder.visually_lossless.help` | `Software, close to the original: larger files, no visible loss.` | 13 |
| `video.compression` | `Compression` | 13 |
| `video.compression.help` | `Quality for re-encoded videos; lower is smaller.` | 13 |
| `video.convert_mov` | `Convert MOV to MP4` | 13 |
| `video.convert_mov.help` | `QuickTime movies become MP4, replacing the .mov.` | 13 |
| `video.audio_to_aac` | `Convert audio to AAC` | 13 |
| `video.audio_to_aac.help` | `Re-encode other audio formats to AAC so every player can play it.` | 13 |
| `video.remove_audio` | `Remove audio` | 13 |
| `video.remove_audio.help` | `Drop every audio track from optimised videos.` | 13 |
| `video.fps_cap` | `Frame rate cap` | 13 |
| `video.fps_cap.help` | `Drop frames above this rate to save space.` | 13 |
| `video.fps_cap.off` | `Off` | 13 |
| `video.fps_cap.24` | `24 fps` | 13 |
| `video.fps_cap.30` | `30 fps` | 13 |
| `video.fps_cap.60` | `60 fps` | 13 |
| `video.speed_frames` | `Frames when sped up` | 13 |
| `video.speed_frames.help` | `Keep every frame for smoothness, or drop frames for size.` | 13 |
| `video.preserve_dates` | `Preserve dates` | 13 |
| `video.preserve_dates.help` | `Keep the original's creation and modification dates.` | 13 |
| `video.ignored_extensions` | `Ignored extensions` | 13 |
| `video.ignored_extensions.help` | `Videos with these extensions are left alone.` | 13 |
| `video.max_size` | `Maximum size` | 13 |
| `video.max_size.help` | `Larger videos are left alone.` | 13 |
| `video.output.optimised.help` | `Where optimised, downscaled, cropped and sped-up videos go.` | 13 |
| `video.output.converted.help` | `Where videos converted to another format or to GIF go.` | 13 |
| `video.compare.play` | `Play` | 14 |
| `video.compare.pause` | `Pause` | 14 |
| `video.compare.back` | `Previous frame` | 14 |
| `video.compare.forward` | `Next frame` | 14 |
| `video.compare.swap` | `Swap sides` | 14 |
| `video.compare.position` | `Position` | 14 |
| `video.compare.cant_play` | `Can't play {format} here — open the file instead` | 14 |

Existing keys reused unchanged: `error.cancelled`, `error.too_large`, `error.ignored_extension`, `error.read`, `error.not_an_image`, `error.source_changed`, `error.backup`, `error.backup_missing`, `card.note.fit_missed`, `card.detail.aggressive`, `output.optimised`, `output.converted`, `format.tip.mac`, `format.tip.windows`.

## Shared-file edits

| File | Anchor | What | Task |
| --- | --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | new group after `pub mod workdir;`: blank line, `// stream A: video`, `pub mod error; pub mod ffmpeg; pub mod video; pub use error::FerryError;` (the blank line stops rustfmt re-sorting across groups) | modules | 1, 6 |
| `crates/ferry-core/src/settings.rs` | `Settings` struct: field `pub video: Video,` after `pub hotkeys: Hotkeys,`; `Default for Settings`: `video: Video::default(),`; new block `// stream A: video settings` after `impl Default for Backups` holding `Video`, `SpeedFrames`, `FpsCap`, `impl Default for Video`, `Output::with_folder` (the encoder type is F's `optimise::Encoder`, not a new enum); in `sanitised()` a new block before `self.version = …` | `[video]` section | 5 |
| `crates/ferry-core/src/optimise/mod.rs` | only if F's `Encoder` is not on the branch yet: F's enum right after `pub enum Level` (see "Encoder choice") | shared encoder enum | 5 |
| `crates/ferry-core/src/edit/mod.rs` | `Edits`, `EditsPatch`, `merge`: A's `speed`/`mute` at the end under `// stream A: video`; F's `encoder` only if absent, under the F comment (see "Encoder choice") | recipe | 7 |
| `src/lib/i18n/en.json` | one block at the end of the file | `video.*` strings | 1, 6, 10–14 |
| `src/lib/types.ts` | does not exist in this repo; the TS types live in `src/lib/ipc.ts` and `src/lib/settings.ts` | none | — |
| `src/lib/ipc.ts` | `Format` union (append video members); `Edits` (append `speed`, `mute`; F's `encoder` only if absent); `JobUpdate` (append `video`); `CardAction.action` (append `'speed'`); `StackConfig` (append `video_compression`, `speed_frames`); new `VideoMeta`, `Speed` interfaces under `// stream A: video` | types | 4, 7, 12 |
| `src/lib/settings.ts` | `Settings` interface: `video: VideoSettings` field after `hotkeys`; new `VideoSettings` interface under `// stream A: video` | types | 5 |
| `src-tauri/tauri.conf.json` | `bundle.externalBin` append `"binaries/ffmpeg"`; new `bundle.resources` `{ "binaries/licenses/": "licenses/" }`; `app.security.csp` add ` media-src 'self' asset: http://asset.localhost;` after the `img-src …;` directive | sidecar, licences, video playback | 3 |
| `src-tauri/src/lib.rs` | `setup()`: call `video_startup(app);` just before `Ok(())`; new `fn video_startup` under `// stream A: video` at file end; `.run(|_app, event| match event {…})`: new arm `tauri::RunEvent::Exit => ferry_core::ffmpeg::kill_all(),` before `_ => {}` | startup probe, kill on quit | 11 |
| `src-tauri/src/state.rs` | `AppState`: field `pub video_permits: Arc<Semaphore>,` after `pub permits`; initialised in `new` after `permits:` | one video job at a time | 11 |
| `src-tauri/src/commands.rs` | `StackConfig`: fields `video_compression`, `speed_frames` at the end; `stack_ready`: fill them; `output_preview`: new trailing parameter `media: Option<String>` | stack config, File handling example | 11, 13 |
| `src-tauri/src/menu.rs` | `Action`: append `Speed, Mute` after `Close`; `FORMATS`: append the six video formats; `NAMED`: append `("speed", Action::Speed), ("mute", Action::Mute)`; new `fn format_key` under `// stream A: video`; `convert_formats`, `enabled`, `page`, `build`, `run`: video branches marked `// stream A: video` | video menu | 11 |
| `src/stack/Card.svelte` | thumbnail block (film placeholder, duration badge), ext chip (`extLabel`), progress ring arc; each marked `<!-- stream A: video -->` | card | 12 |
| `src/stack/Stack.svelte` | `config` defaults and `settings:changed` (video fields); `clickAway` kinds; `{:else if open === 'speed'}` popover branch; `compression` prop per media | speed popover | 12 |
| `src-tauri/capabilities/*` | no change | — | — |
| `Cargo.lock` | updated by cargo only (ferry-core gains `windows-sys`) | — | 1 |

Non-shared files this plan also edits: `crates/ferry-core/src/{sniff,job,pipeline,fsutil,output,testutil}.rs`, `optimise/mod.rs` (Task 4's video arm in `encode`), `edit/raster.rs`, `bitmap.rs`, `src-tauri/src/{jobs,compare}.rs`, `src-tauri/src/platform/windows_clipboard.rs`, `src/lib/{edits,cards,schema}.ts`, `src/lib/{card,settings}.fixture.ts`, `src/stack/FormatBar.svelte` (no change expected), `src/settings/Row.svelte`, `src/settings/controls/OutputRule.svelte`, `src/compare/Compare.svelte`, `.github/workflows/{ci,release}.yml`, `scripts/build-gifsicle.sh`, `package.json`, `README.md`.

## Windows-only code (compiled in CI, not on this Mac)

- `crates/ferry-core/src/ffmpeg.rs`: `CREATE_NO_WINDOW` creation flag and the kill-on-close Job Object (`#[cfg(windows)] mod win`).
- `src-tauri/src/platform/windows_clipboard.rs`: the video arm of `format_name` (the whole module is `cfg(windows)`).
- `scripts/build-ffmpeg.sh` `x86_64-pc-windows-msvc` branch (runs in MSYS2 MINGW64 on the CI runner).
- Hardware candidate lists use `cfg!(windows)` / `cfg!(target_os = "macos")` expressions, which compile everywhere.

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/error.rs` (create) | `FerryError` and its card reason | 1 |
| `crates/ferry-core/src/ffmpeg.rs` (create) | locate, run, progress, stall, cancel, kill on exit | 1 |
| `crates/ferry-core/tests/support/sidecar_fixture.rs` (modify) | `ffmpeg-*` fake modes | 1 |
| `crates/ferry-core/tests/ffmpeg_runner.rs`, `tests/ffmpeg_kill_all.rs` (create) | runner behaviour on every OS | 1 |
| `scripts/build-ffmpeg.sh` (create) | pinned, verified, per-triple build; `--sources` mode; licence collection | 2 |
| `THIRD-PARTY-NOTICES.md` (create) | attribution for every shipped component | 2 |
| `crates/ferry-core/tests/ffmpeg_real.rs` (create) | the real binary's encoders, licence flags, progress, cancel | 3 |
| `.github/workflows/ci.yml`, `release.yml` (modify) | build, cache, test, smoke, size, sources | 3 |
| `crates/ferry-core/src/sniff.rs` (modify) | video containers, `is_video` | 4 |
| `crates/ferry-core/src/video/mod.rs` (create) | `Codec`, labels, ids, targets, source refinement, `is_video_file`, `step_text` | 4, 6, 10 |
| `crates/ferry-core/src/video/probe.rs` (create) | parse `ffmpeg -i` | 6 |
| `crates/ferry-core/src/video/hardware.rs` (create) | encoder detection | 6 |
| `crates/ferry-core/src/video/plan.rs` (create) | recipe → ffmpeg arguments | 8 |
| `crates/ferry-core/src/video/fit.rs` (create) | fit-under bitrates, hardware bitrate | 8 |
| `crates/ferry-core/src/settings.rs`, `output.rs` (modify) | `[video]`, output example for videos | 5 |
| `crates/ferry-core/src/optimise/mod.rs` (modify, only if F's `Encoder` is absent) | F's `Encoder` enum, verbatim | 5 |
| `crates/ferry-core/src/edit/mod.rs` (modify) | `Speed`, `mute`; F's `encoder` field only if absent | 7 |
| `crates/ferry-core/src/fsutil.rs` (modify) | `atomic_replace_from` | 9 |
| `crates/ferry-core/src/pipeline.rs` (modify) | `Content` (bytes or file), video dispatch, video output rules | 9, 10 |
| `crates/ferry-core/src/pipeline/video_job.rs` (create) | video file jobs end to end | 10 |
| `crates/ferry-core/src/job.rs` (modify) | `VideoMeta`, `JobUpdate::video` | 10 |
| `src-tauri/src/{lib,state,jobs,menu,commands}.rs` (modify) | wiring | 11, 13 |
| `src/lib/video.ts`, `src/lib/video.test.ts` (create) | card helpers | 12 |
| `src/stack/SpeedPopover.svelte` (create) | speed popover | 12 |
| `src/settings/controls/EncoderPicker.svelte` (create) | three encoder cards | 13 |
| `src-tauri/src/compare.rs` (modify), `src/compare/VideoCompare.svelte`, `src/compare/video.ts`, `src/compare/video.test.ts` (create) | video compare | 14 |

## Setup (once, before Task 1)

```bash
source "$HOME/.cargo/env"
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
TRIPLE="$(rustc -vV | sed -n 's/^host: //p')"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$TRIPLE"
export FERRY_FFMPEG="$PWD/src-tauri/binaries/ffmpeg-$TRIPLE"   # exists after Task 2
```

A new worktree needs the git-ignored `src-tauri/binaries/` copied from the main checkout. Once Task 3 lists `binaries/ffmpeg` in `externalBin`, `cargo build -p ferry` fails until `src-tauri/binaries/ffmpeg-<triple>` exists. Run Task 2's build before Task 3. Tests that need ffmpeg skip when `FERRY_FFMPEG` is unset, and CI always sets it (like gifsicle).

---

## Task group 1: the ffmpeg contract and sidecar (stream B starts after Task 1)

### Task 1: `FerryError` and the ffmpeg runner

**Files:**
- Create: `crates/ferry-core/src/error.rs`, `crates/ferry-core/src/ffmpeg.rs`
- Modify: `crates/ferry-core/src/lib.rs` (anchor in the table), `crates/ferry-core/Cargo.toml` (Windows `windows-sys`), `crates/ferry-core/tests/support/sidecar_fixture.rs`, `src/lib/i18n/en.json` (`video.error.ffmpeg_missing`, `.spawn`, `.encode`, `.stalled`, `.disk_full`)
- Test: `crates/ferry-core/tests/ffmpeg_runner.rs`, `crates/ferry-core/tests/ffmpeg_kill_all.rs`, unit tests in `ffmpeg.rs` and `error.rs`

**Interfaces:**
- Consumes: `ferry_core::cancel::CancelToken`, `ferry_core::i18n::{t}`.
- Produces (the cross-stream contract, exactly):

```rust
// error.rs
#[derive(Debug)]
pub enum FerryError {
    /// No ffmpeg next to Ferry, and FERRY_FFMPEG unset or not a file.
    FfmpegMissing,
    /// It could not be started (permissions, a broken binary).
    Spawn(std::io::Error),
    Cancelled,
    /// No output on stdout or stderr for the stall limit.
    Stalled,
    /// It ran and exited non-zero (or was killed by a signal: code None).
    Failed { code: Option<i32>, command: String, stderr: String },
    Io(std::io::Error),
    /// Ferry cannot handle this input; the text is the card's reason.
    Unsupported(String),
}
impl FerryError { pub fn reason(&self) -> String }      // one-line card text
impl std::fmt::Display for FerryError {}                 // technical, for the log: includes command and stderr tail
impl std::error::Error for FerryError {}
impl From<std::io::Error> for FerryError {}             // → Io

// ffmpeg.rs
pub use crate::error::FerryError;
pub const ENV: &str = "FERRY_FFMPEG";
pub const STALL: std::time::Duration = std::time::Duration::from_secs(120);
pub fn locate() -> Result<PathBuf, FerryError>;
pub fn run(args: &[OsString], cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<(), FerryError>;
pub fn run_expecting(args: &[OsString], expected: Duration, cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<(), FerryError>;
pub fn capture(args: &[OsString], cancel: &CancelToken) -> Result<Captured, FerryError>;
pub struct Captured { pub code: Option<i32>, pub stderr: String }
pub struct Exec { pub expected: Option<Duration>, pub stall: Duration, pub check_status: bool } // Default: None, STALL, true
pub fn exec(program: &Path, args: &[OsString], how: &Exec, cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<Captured, FerryError>;
pub fn kill_all();
```

Rules `exec` implements:
- It puts these global options **before** the caller's `args`: `-hide_banner -nostdin -y -nostats -progress pipe:1 -stats_period 0.5`. Callers never pass them and never write their own output to stdout.
- It reads stdout and stderr on their own threads and sends each line into one `mpsc` channel. The main loop waits on `recv_timeout(50 ms)` and on each wake checks `cancel`, the kill-all epoch, the stall clock (time since the last line > `how.stall` → kill, `Stalled`) and `try_wait`.
- **Progress:** the total is `how.expected`, else the first `Duration: HH:MM:SS.cc` on stderr (`N/A` gives no total). `out_time_us=<µs>` (also accept `out_time_ms=`, which ffmpeg prints in µs as well) gives `min(1.0, out/total)`. `progress=end` gives `1.0`. `on_progress` is called only when the value rises. With no total, it is never called except for `1.0` at `progress=end`.
- **stderr kept:** the last 1 MiB (a ring buffer). `Captured::stderr` is that text. `Failed::stderr` is its last 4 KiB.
- `check_status: true` and a non-zero exit gives `Failed { code, command, stderr }`, where `command` is `program` plus the full argument list, space-joined. `false` returns `Ok(Captured)` for any exit.
- It kills and waits for the child on every error path, and through `Drop` on unwind (as `sidecar.rs` does).
- `#[cfg(windows)]`: `CREATE_NO_WINDOW` (copy the `sidecar.rs` block). The child is also assigned to a process-wide Job Object created once (`OnceLock<usize>` holding the `HANDLE`) with `JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE`, so ffmpeg dies if Ferry crashes: `CreateJobObjectW(null, null)`, `SetInformationJobObject(job, JobObjectExtendedLimitInformation, &JOBOBJECT_EXTENDED_LIMIT_INFORMATION, size)`, `AssignProcessToJobObject(job, child.as_raw_handle())`. Failures are ignored (best effort).
- `kill_all()` bumps a global `AtomicU64` epoch. Each `exec` records the epoch at start, kills its child when the epoch changes and returns `Cancelled`. `kill_all` then waits until a global running counter reaches 0, at most 1 s.
- `locate()` = `locate_in(std::env::var_os(ENV), std::env::current_exe().ok())`. The private `fn locate_in(env: Option<OsString>, exe: Option<PathBuf>) -> Result<PathBuf, FerryError>` takes the env path if set and a file; if the env var is set but not a file, `FfmpegMissing`. Otherwise it takes `exe.with_file_name(format!("ffmpeg{}", std::env::consts::EXE_SUFFIX))` if that is a file, else `FfmpegMissing`. This mirrors `src-tauri/src/lib.rs::gifsicle()`. Tauri copies `externalBin` next to the executable in dev and in bundles.
- `run` = `exec(locate()?, args, &Exec::default(), …).map(drop)`. `run_expecting` is the same with `expected: Some(..)`. `capture` uses `check_status: false` and a no-op progress closure.
- `reason()`: `FfmpegMissing` → `t("video.error.ffmpeg_missing")`; `Spawn` → `t("video.error.spawn")`; `Cancelled` → `t("error.cancelled")`; `Stalled` → `t("video.error.stalled")`; `Failed` whose stderr contains `No space left on device` → `t("video.error.disk_full")`; other `Failed` → `t("video.error.encode")`; `Io(e)` with `ErrorKind::StorageFull` → `t("video.error.disk_full")`, other `Io(e)` → `e.to_string()`; `Unsupported(m)` → `m`.

- [ ] **Step 1: Add the fixture modes.** In `sidecar_fixture.rs`, before the existing `match`: if the **last** argument starts with `ffmpeg-`, run that mode (the runner's global options come first, so the mode is last):
  - `ffmpeg-progress`: stderr `"  Duration: 00:00:02.00, start: 0.000000, bitrate: 1 kb/s\n"`, then on stdout, four times 100 ms apart, `out_time_us=<i*500000>\nprogress=continue\n`, then `progress=end\n`; exit 0.
  - `ffmpeg-args`: write every argument, one per line, to stderr; exit 0.
  - `ffmpeg-fail`: stderr `"Error opening output out.mp4: No space left on device\n"`; exit 1.
  - `ffmpeg-hang`: sleep 30 s, printing nothing.
  - `ffmpeg-slow`: on stdout `out_time_us=<i*100000>\nprogress=continue\n` every 300 ms for 2 s, then `progress=end\n`; exit 0.
  - `ffmpeg-flood`: write 8 MiB of `x` lines (64 KiB each) to stderr, then the line `flood-end`; exit 0.

- [ ] **Step 2: Write the failing tests.** In `tests/ffmpeg_runner.rs`, `fixture()` is `PathBuf::from(env!("CARGO_BIN_EXE_sidecar-fixture"))` as in `tests/sidecar.rs`, and `os(&["ffmpeg-progress"])` makes the `OsString` args.

```rust
#[test] fn progress_climbs_to_one_from_the_duration_on_stderr() {
    // seen = values passed to on_progress
    assert!(seen.windows(2).all(|w| w[0] < w[1]));
    assert!((seen[0] - 0.25).abs() < 1e-6);
    assert_eq!(*seen.last().unwrap(), 1.0);
}
#[test] fn an_expected_duration_overrides_the_inputs() {
    // Exec { expected: Some(Duration::from_secs(4)), ..Default::default() }
    assert!(seen.contains(&0.5));  // 2 s of 4 s before the end
    assert_eq!(*seen.last().unwrap(), 1.0);
}
#[test] fn the_runner_adds_its_global_options() {
    // check_status: false, mode ffmpeg-args; lines of Captured::stderr
    for want in ["-hide_banner", "-nostdin", "-y", "-nostats", "-progress", "pipe:1"] { assert!(lines.contains(&want)); }
    assert_eq!(lines.last(), Some(&"ffmpeg-args"));
}
#[test] fn a_failure_carries_the_code_and_the_end_of_stderr() {
    let err = /* exec ffmpeg-fail */;
    assert!(matches!(&err, FerryError::Failed { code: Some(1), stderr, .. } if stderr.contains("No space left on device")));
    assert_eq!(err.reason(), "Not enough disk space to encode");
}
#[test] fn capture_returns_any_exit_code() { /* check_status false, ffmpeg-fail → Ok(Captured { code: Some(1), .. }) */ }
#[test] fn cancel_kills_the_child_quickly() { /* ffmpeg-hang; another thread cancels after 200 ms → Err(Cancelled), elapsed < 2 s */ }
#[test] fn silence_longer_than_the_stall_limit_is_stalled() { /* ffmpeg-hang, stall 500 ms → Err(Stalled), elapsed < 3 s */ }
#[test] fn steady_progress_never_counts_as_a_stall() { /* ffmpeg-slow, stall 1 s → Ok, last progress 1.0 */ }
#[test] fn a_flood_of_stderr_does_not_block_and_keeps_its_tail() {
    assert!(c.stderr.len() <= 1_048_576);
    assert!(c.stderr.trim_end().ends_with("flood-end"));
}
#[test] fn a_program_that_is_not_there_cannot_spawn() { /* exec("/no/such/ffmpeg") → Err(Spawn(_)) */ }
```

In `tests/ffmpeg_kill_all.rs` (its own process, because it touches a global):

```rust
#[test] fn kill_all_stops_every_running_ffmpeg() { /* ffmpeg-hang on a thread; after 300 ms kill_all(); thread returns Err(Cancelled) within 2 s */ }
```

Unit tests in `ffmpeg.rs`:

```rust
#[test] fn the_inputs_duration_is_read_from_stderr() {
    assert_eq!(parse_duration("  Duration: 00:01:02.50, start: 0.000000, bitrate: 1 kb/s"), Some(Duration::from_millis(62_500)));
    assert_eq!(parse_duration("  Duration: N/A, start: 0.000000, bitrate: N/A"), None);
    assert_eq!(parse_duration("frame=1"), None);
}
#[test] fn progress_lines_become_fractions_of_the_total() {
    let mut p = Progress::new(Some(Duration::from_secs(10)));
    assert_eq!(p.feed("out_time_us=2500000"), Some(0.25));
    assert_eq!(p.feed("out_time_ms=5000000"), Some(0.5));
    assert_eq!(p.feed("out_time_us=N/A"), None);
    assert_eq!(p.feed("out_time_us=99000000"), Some(1.0));   // clamped
    assert_eq!(p.feed("out_time_us=1"), None);               // never goes back
    assert_eq!(Progress::new(None).feed("progress=end"), Some(1.0));
}
#[test] fn ffmpeg_is_found_from_the_env_or_beside_ferry() {
    // temp dir with a file "ffmpeg" (+ EXE_SUFFIX) next to a fake exe path
    // env set to an existing file → that file; env set to a missing path → FfmpegMissing;
    // env unset → sibling of exe; neither → FfmpegMissing
}
```

Unit test in `error.rs`: `every_error_has_a_one_line_reason`. Every variant's `reason()` is non-empty and has no `\n`. `FfmpegMissing` → `"ffmpeg missing — reinstall Ferry"`.

- [ ] **Step 3: Run them and watch them fail.** `cargo test -p ferry-core --test ffmpeg_runner` → FAIL (no `ffmpeg` module).
- [ ] **Step 4: Implement** `error.rs` and `ffmpeg.rs` as specified; add the lib.rs anchor (`pub mod error; pub mod ffmpeg; pub use error::FerryError;`, `video` comes in Task 4). Add to `crates/ferry-core/Cargo.toml`:

```toml
[target.'cfg(windows)'.dependencies]
windows-sys = { version = "0.61", features = ["Win32_Foundation", "Win32_System_JobObjects", "Win32_System_Threading"] }
```

  Add the five `video.error.*` strings.
- [ ] **Step 5: Run the tests.** `cargo test -p ferry-core --test ffmpeg_runner`, `cargo test -p ferry-core --test ffmpeg_kill_all`, `cargo test -p ferry-core ffmpeg`, `cargo test -p ferry-core error`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS, clean.
- [ ] **Step 6: Commit.** `git commit -m "feat(core): ferry_core::ffmpeg runner and FerryError"` (add the files listed above and `Cargo.lock`).

### Task 2: Build the ffmpeg sidecar from pinned sources (manual check)

**Files:**
- Create: `scripts/build-ffmpeg.sh`, `THIRD-PARTY-NOTICES.md`
- Modify: `scripts/build-gifsicle.sh` (copy `COPYING` to `src-tauri/binaries/licenses/gifsicle/COPYING` after the build), `package.json` (`"sidecar:ffmpeg": "bash scripts/build-ffmpeg.sh"` after `"sidecar"`), `README.md` (Develop section: `pnpm sidecar:ffmpeg`, the `FERRY_FFMPEG` export, build time ~30–45 min, "universal builds need Apple Silicon")

**Interfaces:**
- Produces: `src-tauri/binaries/ffmpeg-<triple>[.exe]`. For `universal-apple-darwin`: a fat binary, also copied as `ffmpeg-aarch64-apple-darwin` and `ffmpeg-x86_64-apple-darwin`, as `build-gifsicle.sh` does. Also `src-tauri/binaries/licenses/<component>/…` plus `src-tauri/binaries/licenses/THIRD-PARTY-NOTICES.md`.
- CLI: `scripts/build-ffmpeg.sh [triple]` (default: host triple); `scripts/build-ffmpeg.sh --sources <dir>` fetches every pinned source for every platform into `<dir>/<name>/` without `.git` (git sources via `git archive HEAD | tar -x`) and builds nothing.

Script layout (bash, `set -euo pipefail`, same style as `build-gifsicle.sh`):
- The pins table above as variables. `fetch_git <name> <url> <tag-or-empty> <commit>` clones into `target/ffmpeg-build/src/<name>` (`git init`, `git fetch --depth 1 origin refs/tags/<tag>:refs/tags/<tag>` or `git fetch --depth 1 origin <commit>`, `git checkout`), then **fails** unless `git rev-parse HEAD` equals the pinned commit. It reuses an existing checkout at the right commit. For AMF use `--filter=blob:none` and `git sparse-checkout set amf/public/include`. `fetch_tar` downloads LAME and checks SHA-256 with the `sha256` helper from `build-gifsicle.sh`; on a mismatch it deletes the file and fails.
- Prefix `target/ffmpeg-build/<triple>/prefix`. `export PKG_CONFIG_LIBDIR="$PREFIX/lib/pkgconfig"`, so Homebrew or MSYS2 libraries are never linked by accident. `-j` = CPU count.
- **Apple triples:** `x86_64-apple-darwin` and `aarch64-apple-darwin` build natively and fail with a hint unless `uname -m` matches (`x86_64` / `arm64`). Every compiler gets `-arch <arch> -mmacosx-version-min=13.0` (`CC="clang -arch …"`, `CXX`, `CFLAGS`, `LDFLAGS`, `-DCMAKE_OSX_ARCHITECTURES=<arch> -DCMAKE_OSX_DEPLOYMENT_TARGET=13.0`). `universal-apple-darwin` requires `uname -m` = `arm64`. It runs `arch -arm64 bash "$0" aarch64-apple-darwin` and `arch -x86_64 bash "$0" x86_64-apple-darwin` (Rosetta), then `lipo -create`, then checks `lipo -archs` = `x86_64 arm64`.
- **`x86_64-pc-windows-msvc`:** must run in an MSYS2 MINGW64 shell (fail unless `$MSYSTEM` = `MINGW64`). The binary is built with mingw-w64 gcc; the Tauri triple name is only the file name.
- **`x86_64-unknown-linux-gnu`:** native gcc, no hardware encoders (parity with gifsicle).
- Libraries, in order (all static, installed into the prefix):
  - x264: `./configure --prefix=$PREFIX --enable-static --disable-cli --enable-pic --disable-opencl`.
  - **No x265.** The script never fetches or builds it (HEVC is encoded by hardware only).
  - libvpx: `./configure --prefix=$PREFIX --target=<x86_64-darwin20-gcc | arm64-darwin20-gcc | x86_64-win64-gcc | x86_64-linux-gcc> --enable-static --disable-shared --enable-pic --enable-vp9-highbitdepth --disable-examples --disable-tools --disable-docs --disable-unit-tests`.
  - SVT-AV1: `cmake -B build -DBUILD_SHARED_LIBS=OFF -DBUILD_APPS=OFF -DBUILD_DEC=OFF -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$PREFIX`.
  - dav1d: `meson setup build --prefix=$PREFIX --libdir=lib --default-library=static --buildtype=release -Denable_tools=false -Denable_tests=false && ninja -C build install`.
  - Opus: `cmake -B build -DOPUS_BUILD_SHARED_LIBRARY=OFF -DOPUS_BUILD_TESTING=OFF -DOPUS_BUILD_PROGRAMS=OFF -DCMAKE_BUILD_TYPE=Release -DCMAKE_INSTALL_PREFIX=$PREFIX`.
  - LAME: first `sed -i.bak '/lame_init_old/d' include/libmp3lame.sym` (the known macOS link fix). Then `./configure --prefix=$PREFIX --enable-static --disable-shared --disable-frontend --disable-decoder`, adding `--enable-nasm` on x86_64.
  - Windows only: nv-codec-headers `make install PREFIX=$PREFIX`; AMF `mkdir -p $PREFIX/include/AMF && cp -R amf/public/include/* $PREFIX/include/AMF/`; libvpl `cmake -B build -DBUILD_SHARED_LIBS=OFF -DBUILD_TESTS=OFF -DBUILD_EXAMPLES=OFF -DINSTALL_EXAMPLES=OFF -DBUILD_TOOLS=OFF -DCMAKE_INSTALL_PREFIX=$PREFIX && cmake --build build --target install`.
- FFmpeg `./configure`, always: `--prefix=$PREFIX --pkg-config-flags=--static --extra-cflags=-I$PREFIX/include --extra-ldflags=-L$PREFIX/lib --enable-gpl --enable-version3 --disable-autodetect --disable-shared --enable-static --disable-doc --disable-debug --enable-stripping --disable-ffplay --disable-ffprobe --disable-network --enable-zlib --enable-libx264 --enable-libvpx --enable-libsvtav1 --enable-libdav1d --enable-libopus --enable-libmp3lame`. macOS adds `--enable-videotoolbox --enable-audiotoolbox` and `-mmacosx-version-min=13.0` in both extra flags. Windows adds `--enable-ffnvcodec --enable-nvenc --enable-amf --enable-libvpl --extra-ldflags=-static`. Then `make` and `strip`.
- **Self-checks the script runs and fails on:**
  - `ffmpeg -hide_banner -buildconf` (host-arch binaries only) contains `--enable-gpl` and `--enable-version3`, and contains neither `--enable-nonfree` nor `--enable-libx265`.
  - macOS: every line of `otool -L` is under `/usr/lib/` or `/System/Library/`.
  - Windows: `objdump -p ffmpeg.exe | grep 'DLL Name'` names none of `libwinpthread-1.dll`, `libstdc++-6.dll`, `libgcc_s_seh-1.dll`, `zlib1.dll`.
- **Licences:** copy into `src-tauri/binaries/licenses/`: `ffmpeg/{COPYING.GPLv3,LICENSE.md}`, `x264/COPYING`, `libvpx/{LICENSE,PATENTS}`, `svt-av1/{LICENSE.md,PATENTS.md}`, `dav1d/COPYING`, `opus/COPYING`, `lame/COPYING`; on Windows also `libvpl/LICENSE`, `amf/LICENSE.txt`, and `nv-codec-headers/LICENSE`, made from the leading comment of `include/ffnvcodec/nvEncodeAPI.h` (`sed -n '1,/\*\//p'`). Then copy the repo's `THIRD-PARTY-NOTICES.md`.
- Print `ffmpeg -version | head -1`, the file size in MB, and `built <path>`.

`THIRD-PARTY-NOTICES.md` holds one section per shipped component (gifsicle 1.98 too) with its name, version, pin (commit or SHA-256), source URL, licence, the licence file's path inside the bundle, and the IJG sentence: "This software is based in part on the work of the Independent JPEG Group." (FFmpeg's `jfdctfst.c`, `jfdctint_template.c` and `jrevdct.c`, unchanged). It also states that the build is GPL-3.0-or-later, that it contains no x265 (HEVC is encoded only by the OS or GPU hardware encoders), and that the corresponding source is attached to each GitHub Release as `ferry-<tag>-third-party-sources.tar.xz`.

- [ ] **Step 1: Write the script, the notices file and the gifsicle licence copy.**
- [ ] **Step 2: Run `bash -n scripts/build-ffmpeg.sh` and `shellcheck scripts/build-ffmpeg.sh` (if installed).** Expected: no errors.
- [ ] **Step 3: Manual check: local build on this Mac (Intel).** Install the tools first (`brew install nasm pkgconf meson ninja cmake`), then run `bash scripts/build-ffmpeg.sh`. Look at:
  1. The script ends with `built …/src-tauri/binaries/ffmpeg-x86_64-apple-darwin` and prints a size, expected 30–75 MB.
  2. `"$FERRY_FFMPEG" -hide_banner -encoders | grep -E ' (libx264|libsvtav1|libvpx-vp9|libopus|libmp3lame|aac|h264_videotoolbox|hevc_videotoolbox|gif) '` lists all nine, and `"$FERRY_FFMPEG" -hide_banner -encoders | grep libx265` prints nothing.
  3. `"$FERRY_FFMPEG" -hide_banner -f lavfi -i testsrc2=d=1 -c:v hevc_videotoolbox -f null -` exits 0, which shows the hardware HEVC encoder works on this Mac. Also try 10-bit: the same command with `-pix_fmt p010le -profile:v main10` before `-f null`; record whether it exits 0 (older Intel Macs have no 10-bit HEVC encode, and Ferry then falls back as the "HEVC output" decision says).
  4. `ls src-tauri/binaries/licenses` shows `ffmpeg x264 libvpx svt-av1 dav1d opus lame gifsicle THIRD-PARTY-NOTICES.md` (no `x265`) (run `pnpm sidecar` once for gifsicle).
  5. A second run reuses the checkouts and skips the fetches.

  Record the build time and size in the commit message.
- [ ] **Step 4: Commit.** `git commit -m "build: pinned ffmpeg 9.0.2 sidecar with licences and notices"`.

### Task 3: Ship the sidecar and build it in CI (manual check of the Actions run)

**Files:**
- Modify: `src-tauri/tauri.conf.json` (anchors in the table), `.github/workflows/ci.yml`, `.github/workflows/release.yml`, `crates/ferry-core/src/testutil.rs` (`pub fn ffmpeg() -> Option<PathBuf>`, like `gifsicle()` but reading `FERRY_FFMPEG`)
- Create: `crates/ferry-core/tests/ffmpeg_real.rs`
- Test: `ci_always_has_ffmpeg` in `ffmpeg.rs` tests

**Interfaces:**
- Consumes: Task 1 `ffmpeg::{run, capture}`; Task 2's script.
- Produces: in CI, `FERRY_FFMPEG` set in the `core` job; installers that contain `ffmpeg[.exe]` and `licenses/`.

`tauri.conf.json`: `"externalBin": ["binaries/gifsicle", "binaries/ffmpeg"]`, `"resources": { "binaries/licenses/": "licenses/" }`. The CSP becomes `default-src 'self'; img-src 'self' asset: http://asset.localhost data: blob:; media-src 'self' asset: http://asset.localhost; style-src 'self' 'unsafe-inline'; connect-src ipc: http://ipc.localhost`.

CI (`ci.yml`), in both the `core` and `bundle` jobs, before the existing gifsicle step:
- `- id: ffmpeg-cache` with `uses: actions/cache@v6`, `path: | src-tauri/binaries/ffmpeg-* \n src-tauri/binaries/licenses`, `key: ffmpeg-${{ runner.os }}-${{ hashFiles('scripts/build-ffmpeg.sh', 'THIRD-PARTY-NOTICES.md') }}`.
- macOS, if not a cache hit: `softwareupdate --install-rosetta --agree-to-license || true`, `brew install nasm pkgconf meson ninja`, then `bash scripts/build-ffmpeg.sh universal-apple-darwin`. The macOS `core` job uses the fat binary, under the name `ffmpeg-aarch64-apple-darwin`.
- Windows, if not a cache hit: `uses: msys2/setup-msys2@v2` with `msystem: MINGW64`, `update: true`, `install: git make diffutils mingw-w64-x86_64-gcc mingw-w64-x86_64-nasm mingw-w64-x86_64-cmake mingw-w64-x86_64-meson mingw-w64-x86_64-ninja mingw-w64-x86_64-pkgconf mingw-w64-x86_64-zlib`; then a step with `shell: msys2 {0}` that runs `bash scripts/build-ffmpeg.sh x86_64-pc-windows-msvc`.
- The gifsicle step runs on every build, cache hit or not, because it rewrites `licenses/gifsicle`.
- `core` job `env`: add `FERRY_FFMPEG: ${{ github.workspace }}/src-tauri/binaries/ffmpeg-${{ matrix.triple }}${{ matrix.exe }}`.
- Windows smoke test: after the gifsicle check, add `if (-not (Test-Path (Join-Path $env:LOCALAPPDATA 'Ferry\ffmpeg.exe'))) { throw "ffmpeg sidecar missing from install" }` and the same for `Ferry\licenses\ffmpeg\COPYING.GPLv3`.
- Size check (NFR-R3), a new step after "Build installer" (`shell: bash`): find the `.exe` / `.dmg` and fail if it is larger than 120 MB (`[ "$(du -m "$f" | cut -f1)" -le 120 ]`).

`release.yml`: the same cache, ffmpeg-build and gifsicle steps before `tauri-action`. Add a job `sources` with `needs: release`, `runs-on: ubuntu-latest` and `permissions: contents: write`. It checks out, runs `bash scripts/build-gifsicle.sh x86_64-unknown-linux-gnu` (for the cached tarball) and `bash scripts/build-ffmpeg.sh --sources "$RUNNER_TEMP/src"`. It copies `scripts/`, `THIRD-PARTY-NOTICES.md` and `target/gifsicle-build/gifsicle-1.98.tar.gz` into that folder, runs `tar -cJf "ferry-${GITHUB_REF_NAME}-third-party-sources.tar.xz" -C "$RUNNER_TEMP" src`, then `gh release upload "$GITHUB_REF_NAME" ferry-*-third-party-sources.tar.xz --clobber` with `GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}`.

- [ ] **Step 1: Write the failing tests** (`tests/ffmpeg_real.rs`; each returns early when `testutil`-style `std::env::var_os("FERRY_FFMPEG")` is unset or missing):

```rust
#[test] fn the_sidecar_is_a_gpl3_build_without_nonfree_parts() {
    let out = Command::new(ffmpeg).args(["-hide_banner", "-buildconf"]).output().unwrap(); let conf = String::from_utf8_lossy(&out.stdout);
    assert!(conf.contains("--enable-gpl") && conf.contains("--enable-version3"));
    assert!(!conf.contains("--enable-nonfree"));
    assert!(!conf.contains("--enable-libx265"), "x265 must not be in the build: HEVC is hardware only");
}
#[test] fn the_sidecar_has_every_encoder_ferry_uses() {
    let out = Command::new(ffmpeg).args(["-hide_banner", "-encoders"]).output().unwrap(); // has(e): a stdout line whose second column is e
    for e in ["libx264", "libsvtav1", "libvpx-vp9", "libopus", "libmp3lame", "aac", "gif", "mjpeg", "png"] { assert!(has(e)); }
    assert!(!has("libx265"), "no software HEVC encoder may ship");
    if cfg!(target_os = "macos") { for e in ["h264_videotoolbox", "hevc_videotoolbox"] { assert!(has(e)); } }
    if cfg!(windows) { for e in ["h264_nvenc", "hevc_nvenc", "h264_qsv", "hevc_qsv", "h264_amf", "hevc_amf"] { assert!(has(e)); } }
}
#[test] fn a_real_encode_reports_progress_to_the_end() {
    // run(["-f","lavfi","-i","testsrc2=size=320x240:rate=30:duration=2","-c:v","libx264","-preset","ultrafast","-f","null","-"])
    assert_eq!(*seen.last().unwrap(), 1.0);
}
#[test] fn cancelling_a_real_encode_stops_it() { /* testsrc2 duration=600 to null; cancel after the first progress value → Err(Cancelled) within 3 s */ }
```

  `-encoders` and `-buildconf` print to **stdout**, which `exec` uses for progress. So these two tests call `std::process::Command::new(ffmpeg).args([...]).output()` directly and read stdout. Only the encode/cancel tests go through `run`.

  In `ffmpeg.rs` tests: `ci_always_has_ffmpeg`. When `CI` is set, `testutil::ffmpeg().is_some()`, with the message `"FERRY_FFMPEG must be set in CI"`.
- [ ] **Step 2: Run them locally.** `cargo test -p ferry-core --test ffmpeg_real` with `FERRY_FFMPEG` exported → PASS on the Mac (Task 2's binary); unset → PASS by skipping.
- [ ] **Step 3: Edit `tauri.conf.json`, `ci.yml` and `release.yml`** as specified. `cargo build -p ferry` must succeed locally (the sidecar exists).
- [ ] **Step 4: Manual check: the CI run.** Ask the user before pushing the branch. Once a push is allowed, look at the GitHub Actions run for the branch:
  1. `core (macos-latest)` and `core (windows-latest)` are green. The ffmpeg build steps ran (first run) and `ci_always_has_ffmpeg` passed.
  2. `bundle (windows)`: the smoke test log shows no "ffmpeg sidecar missing" and the installer size step printed ≤ 120.
  3. `bundle (macos)`: the size step printed ≤ 120 MB for the `.dmg`.
  4. Re-run the workflow. The `ffmpeg-cache` step reports a cache hit and the build steps are skipped.

  Record the first-run build times per OS. If the Windows MSYS2 build fails, fix it within `scripts/build-ffmpeg.sh` only.
- [ ] **Step 5: Commit.** `git commit -m "ci: build, cache, bundle and attribute the ffmpeg sidecar"`.

---

## Task group 2: the video core

### Task 4: Video formats in `sniff::Format`

**Files:**
- Modify: `crates/ferry-core/src/sniff.rs`, `crates/ferry-core/src/optimise/mod.rs` (`encode`), `crates/ferry-core/src/edit/raster.rs` (`decode`, `encode`), `crates/ferry-core/src/bitmap.rs` (`decode_rgba`), `crates/ferry-core/src/pipeline.rs` (`run_recipe` step-text `match`), `src-tauri/src/platform/windows_clipboard.rs` (`format_name`, **cfg(windows)**), `crates/ferry-core/src/lib.rs` (`pub mod video;` joins the Task 1 group), `src/lib/ipc.ts` (`Format`), `src/lib/edits.ts` (`FORMAT_LABELS`)
- Create: `crates/ferry-core/src/video/mod.rs`
- Test: `sniff.rs` tests, `video/mod.rs` tests, `optimise/mod.rs` test, `src/lib/edits.test.ts`

**Interfaces:**
- Produces:

```rust
// sniff.rs (block marked // stream A: video at the end of the enum and of each match)
pub enum Format { Png, Jpeg, Gif, WebP, Mp4, Hevc, Av1, WebM, Mov, Mkv }
impl Format { pub fn is_video(self) -> bool }   // the six video variants
// extension(): mp4 mp4 mkv webm mov mkv; label(): MP4 HEVC AV1 WebM MOV MKV; serde: mp4 hevc av1 webm mov mkv
// sniff(): see rules below; is_animated(): false for video; dimensions(): None for video

// video/mod.rs
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Codec { H264, Hevc, Av1, Vp9 }
impl Codec { pub fn label(self) -> &'static str }        // "H.264", "HEVC", "AV1", "VP9"
pub fn format_id(f: Format) -> &'static str;             // serde name: "mp4", "hevc", … (menu ids)
pub fn is_target(f: Format) -> bool;                     // Mp4 | Hevc | Av1 | WebM | Gif
pub fn is_video_file(path: &Path) -> bool;               // sniff of the first 64 bytes is a video format
```

Sniff rules (content only):
- **ISO BMFF:** `bytes[4..8] == b"ftyp"`, with major brand `bytes[8..12]`. `qt  ` → `Mov`. One of `isom iso2 iso3 iso4 iso5 iso6 iso7 iso8 iso9 mp41 mp42 mp71 avc1 M4V  M4VH M4VP MSNV dash 3gp4 3gp5 3gp6 3g2a mmp4 f4v  XAVC` → `Mp4`. Any other brand (`heic`, `heix`, `mif1`, `msf1`, `avif`, `M4A `, `M4B `, `M4P `, …) → not a video, so streams E and B can claim it.
- **Old QuickTime without `ftyp`:** `bytes[4..8]` ∈ {`moov`, `mdat`, `wide`, `free`, `skip`} → `Mov`.
- **EBML** (`1A 45 DF A3`): find the DocType element (`42 82`, then a one-byte size `0x80 | n`, then `n` bytes) in the first 64 bytes. `webm` → `WebM`, `matroska` → `Mkv`.

- [ ] **Step 1: Write the failing tests.**

```rust
// sniff.rs
#[test] fn video_containers_are_sniffed_by_content() {
    let ftyp = |brand: &[u8; 4]| [&[0, 0, 0, 0x18][..], b"ftyp", brand, &[0; 12]].concat();
    assert_eq!(sniff(&ftyp(b"isom")), Some(Format::Mp4));
    assert_eq!(sniff(&ftyp(b"M4V ")), Some(Format::Mp4));
    assert_eq!(sniff(&ftyp(b"qt  ")), Some(Format::Mov));
    assert_eq!(sniff(&[&[0, 0, 0, 8][..], b"wide", &[0; 8]].concat()), Some(Format::Mov));
    assert_eq!(sniff(&ftyp(b"heic")), None);
    assert_eq!(sniff(&ftyp(b"avif")), None);
    assert_eq!(sniff(&ftyp(b"M4A ")), None);
    let ebml = |doc: &[u8]| [&[0x1A, 0x45, 0xDF, 0xA3, 0x9F, 0x42, 0x86, 0x81, 0x01, 0x42, 0x82, 0x80 | doc.len() as u8][..], doc, &[0; 16]].concat();
    assert_eq!(sniff(&ebml(b"webm")), Some(Format::WebM));
    assert_eq!(sniff(&ebml(b"matroska")), Some(Format::Mkv));
}
#[test] fn video_formats_have_names_extensions_and_serde_ids() {
    assert_eq!((Format::Hevc.label(), Format::Hevc.extension()), ("HEVC", "mp4"));
    assert_eq!((Format::Av1.label(), Format::Av1.extension()), ("AV1", "mkv"));
    assert_eq!(serde_json::to_string(&Format::WebM).unwrap(), "\"webm\"");
    assert_eq!(serde_json::from_str::<Format>("\"mov\"").unwrap(), Format::Mov);
    assert!(Format::Mkv.is_video() && !Format::Gif.is_video() && !Format::Png.is_video());
}
// video/mod.rs
#[test] fn targets_are_the_format_bar_chips() {
    assert!([Format::Mp4, Format::Hevc, Format::Av1, Format::WebM, Format::Gif].into_iter().all(is_target));
    assert!(!is_target(Format::Mov) && !is_target(Format::Mkv) && !is_target(Format::Png));
    assert_eq!(format_id(Format::Hevc), "hevc");
}
#[test] fn a_file_is_a_video_by_its_first_bytes() { /* temp file "x.png" holding ftyp isom → true; a PNG named x.mp4 → false; missing file → false */ }
// optimise/mod.rs
#[test] fn video_formats_are_not_image_encoders() { assert!(matches!(encode(b"x", Format::Mp4, &OptimiseOptions::default()), Err(OptimiseError::Unsupported(_)))); }
```

```ts
// edits.test.ts
it('keeps the image chips unchanged now that formats include video', () => {
  expect(formatChoices(card({ kind: 'done' })).map((c) => c.label)).toEqual(['PNG', 'JPEG', 'WebP']);
});
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core sniff` → FAIL.
- [ ] **Step 3: Implement.** Add the variants and the explicit video arms:
  - `optimise::encode`, `raster::decode`/`encode`: `Err(OptimiseError::Unsupported("video is not an image".into()))`.
  - `bitmap::decode_rgba`: `Err("video is not an image".into())`.
  - `pipeline::run_recipe` step text: `"video.step.reading"`. It is never reached once Task 10 dispatches videos earlier.
  - `windows_clipboard::format_name`: `""`, with a comment that a video never goes on the clipboard as data.
  - TS: append `| 'mp4' | 'hevc' | 'av1' | 'webm' | 'mov' | 'mkv'` to `Format` in `ipc.ts`, and the six labels to `FORMAT_LABELS`.
- [ ] **Step 4: Run.** `cargo test -p ferry-core sniff`, `cargo test -p ferry-core video`, `cargo test -p ferry-core optimise`, `pnpm test`, `pnpm check`, then clippy for the workspace. Expected: PASS. The Windows arm is checked by CI.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): sniff MP4, MOV, MKV and WebM as video formats"`.

### Task 5: The `[video]` settings section

**Files:**
- Modify: `crates/ferry-core/src/settings.rs` (anchors in the table), `crates/ferry-core/src/output.rs`, `src/lib/settings.ts`, `src/lib/settings.fixture.ts`; `crates/ferry-core/src/optimise/mod.rs` **only if** stream F's `Encoder` is not on the branch yet (Step 0)
- Test: `settings.rs` tests, `output.rs` tests, `src/lib/settings.test.ts`

**Interfaces:**
- Consumes: stream F's `ferry_core::optimise::Encoder { Fast, Small, VisuallyLossless }` (see Decisions, "Encoder choice"). There is no `VideoEncoder`.
- Produces:

```rust
use crate::optimise::Encoder;   // F's enum; add to settings.rs's imports
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
pub enum SpeedFrames { #[default] Keep, Drop }
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FpsCap { #[default] #[serde(rename = "off")] Off, #[serde(rename = "24")] Fps24, #[serde(rename = "30")] Fps30, #[serde(rename = "60")] Fps60 }
impl FpsCap { pub fn fps(self) -> Option<u32> }   // None, 24, 30, 60
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(default)]
pub struct Video {
    pub encoder: Encoder, pub compression: u8, pub convert_mov: bool, pub audio_to_aac: bool,
    pub remove_audio: bool, pub fps_cap: FpsCap, pub speed_frames: SpeedFrames, pub preserve_dates: bool,
    pub ignored_extensions: Vec<String>, pub max_size_mb: u32, pub output: Output,
    #[serde(flatten)] pub extra: Map<String, Value>,
}
impl Output { pub fn with_folder(folder: &str) -> Output }   // Output::default() with both rules' folder replaced
// Settings gains `pub video: Video`

// output.rs
pub struct Sample { pub file: &'static str, pub converted_ext: &'static str, pub dims: (u32, u32) }
impl Sample { pub const IMAGE: Sample /* "shot.png", "webp", (1920, 1080) */; pub const VIDEO: Sample /* "clip.mp4", "webm", (1920, 1080) */; }
pub fn example_for(kind: OutputKind, rule: &OutputRule, home: &Path, work_dir: &Path, today: NaiveDate, sample: &Sample) -> Result<String, Vec<String>>;
// `example` keeps its signature and calls example_for with Sample::IMAGE.
```

```ts
// settings.ts
export interface VideoSettings {
  encoder: 'fast' | 'small' | 'visually_lossless'; compression: number; convert_mov: boolean; audio_to_aac: boolean;
  remove_audio: boolean; fps_cap: 'off' | '24' | '30' | '60'; speed_frames: 'keep' | 'drop'; preserve_dates: boolean;
  ignored_extensions: string[]; max_size_mb: number; output: { optimised: OutputRule; converted: OutputRule };
}
// Settings gains `video: VideoSettings` after `hotkeys`
```

`impl Default for Video` sets `encoder: Encoder::Fast` explicitly (F's enum has no `Default`; do not add one).

`sanitised()` adds: compression clamped to 5..=100; `max_size_mb.max(1)`; `ignored_extensions` trimmed with empty entries dropped; both video output rules repaired as the image rules are (invalid or empty template → default, empty folder → default) by a new private `fn repair_output(out: &mut Output, defaults: &Output)` that leaves the existing image loop untouched. The defaults come from `Video::default().output`.

- [ ] **Step 0: Make sure F's encoder enum exists.** Run `grep -n "pub enum Encoder" crates/ferry-core/src/optimise/mod.rs`. If it prints a line, stream F's Task 1 is already on the branch: use that enum and skip to Step 1. If it prints nothing, add F's definition verbatim right after `pub enum Level { … }` (the file already imports `serde::{Deserialize, Serialize}`):

```rust
// stream F: pipelines (encoder; added early by stream A, identical to F's Task 1)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Encoder {
    Fast,
    Small,
    VisuallyLossless,
}
```

  Add nothing else of F's (no `OptimiseOptions.encoder`, no `adaptive`, no image-encoder mapping, no tests in `optimise/mod.rs`); the settings test below covers its serde names.

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn video_defaults_match_the_plan() {
    let v = Settings::default().video;
    assert_eq!((v.encoder, v.compression, v.convert_mov, v.audio_to_aac, v.remove_audio), (Encoder::Fast, 80, true, false, false));
    assert_eq!((v.fps_cap, v.speed_frames, v.preserve_dates, v.max_size_mb), (FpsCap::Off, SpeedFrames::Keep, true, 4000));
    let folder = if cfg!(windows) { "~/Videos/Ferry" } else { "~/Movies/Ferry" };
    assert_eq!(v.output.optimised.folder, folder);
    assert_eq!(v.output.optimised.template, "%f_optimised.%e");
    assert_eq!(v.output.converted.template, "%f.%e");
}
#[test] fn a_file_without_a_video_section_loads_its_defaults() { /* version-2 JSON with only images → video == Video::default() */ }
#[test] fn video_values_out_of_range_are_repaired() { /* compression 0 → 5, 250 → 100; max_size_mb 0 → 1; template "%q" → default; " mov " → "mov" */ }
#[test] fn the_fps_cap_reads_and_writes_as_text() { assert_eq!(serde_json::to_string(&FpsCap::Fps30).unwrap(), "\"30\""); assert_eq!(FpsCap::Fps60.fps(), Some(60)); }
#[test] fn the_video_encoder_setting_reads_snake_case() { /* {"video":{"encoder":"visually_lossless"}} loads Encoder::VisuallyLossless; "small" → Encoder::Small */ }
// output.rs
#[test] fn the_video_example_uses_a_clip() {
    // Location::SameFolder, template "%f_optimised.%e", Optimised → "~/Desktop/clip_optimised.mp4"
    // Location::InPlace, Converted → "~/Desktop/clip.webm"
}
```

```ts
// settings.test.ts: sample.video exists and round-trips getPath('video.encoder') === 'fast'
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core settings` → FAIL.
- [ ] **Step 3: Implement**, then update `settings.fixture.ts` with the `video` defaults (macOS folder `~/Movies/Ferry`).
- [ ] **Step 4: Run.** `cargo test -p ferry-core settings`, `cargo test -p ferry-core output`, `cargo test -p ferry-core optimise`, `pnpm test`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): video settings section"` (if Step 0 added F's enum, say so in the message body: "adds stream F's optimise::Encoder verbatim").

### Task 6: Probe a video, detect hardware encoders

**Files:**
- Create: `crates/ferry-core/src/video/probe.rs`, `crates/ferry-core/src/video/hardware.rs`
- Modify: `crates/ferry-core/src/video/mod.rs` (`pub mod probe; pub mod hardware;`, `codec_label`, `source_format`), `crates/ferry-core/src/testutil.rs` (`VideoSpec`, `video`), `src/lib/i18n/en.json` (`video.error.read`, `video.error.no_video`)
- Test: unit tests in `probe.rs`, `hardware.rs`, `video/mod.rs`

**Interfaces:**
- Consumes: Task 1 `ffmpeg::capture`, `ffmpeg::run`.
- Produces:

```rust
// probe.rs
#[derive(Debug, Clone, PartialEq)]
pub struct Probe { pub duration: Duration, pub bitrate: Option<u64>, pub video: VideoStream, pub audio: Vec<AudioStream>, pub subtitles: usize }
#[derive(Debug, Clone, PartialEq)]
pub struct VideoStream {
    pub codec: String,               // ffmpeg's name: "h264", "hevc", "av1", "vp9", "prores", …
    pub width: u32, pub height: u32, // as displayed: a ±90° rotation swaps them
    pub fps: Option<f32>, pub bitrate: Option<u64>,   // bits per second
    pub ten_bit: bool,               // pixel format name holds "10" or "12" (yuv420p10le, p010le, …)
    pub hdr: bool,                   // colour description names smpte2084 or arib-std-b67
}
#[derive(Debug, Clone, PartialEq)]
pub struct AudioStream { pub codec: String, pub bitrate: Option<u64> }
pub fn parse(stderr: &str) -> Option<Probe>;                    // None: no "Input #0" or no real video stream
pub fn probe(path: &Path, cancel: &CancelToken) -> Result<Probe, FerryError>;
// capture(["-i", path]); exit code 1 is normal ("At least one output file must be specified").
// No "Input #0" → Unsupported(t("video.error.read")); no video stream → Unsupported(tf("video.error.no_video", name)).

// hardware.rs
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Hardware { pub h264: Option<&'static str>, pub hevc: Option<&'static str> }
// hevc == None: this machine cannot write HEVC at all (the build has no x265), so HEVC is not offered
// and anything that needs it falls back to H.264 (Decisions, "HEVC output").
pub fn candidates(codec: Codec) -> &'static [&'static str];
// macOS: H264 ["h264_videotoolbox"], Hevc ["hevc_videotoolbox"];
// Windows: H264 ["h264_nvenc", "h264_qsv", "h264_amf"], Hevc ["hevc_nvenc", "hevc_qsv", "hevc_amf"]; else and Av1/Vp9: []
pub fn detect(works: impl Fn(&str) -> bool) -> Hardware;  // first working candidate per codec, in order
pub fn probe_hardware() -> Hardware;   // once per process (OnceLock::get_or_init: a second caller blocks until the first probe is done); each test encode:
// run(["-f","lavfi","-i","color=c=black:s=256x256:r=30:d=1","-pix_fmt","nv12","-c:v",<enc>,"-f","null","-"]).is_ok()
pub fn hardware() -> Hardware;         // non-blocking: the probed value, or Hardware::default() before probe_hardware finished (OnceLock::get)

// video/mod.rs
pub fn codec_label(ffmpeg_name: &str) -> String;     // h264 H.264, hevc HEVC, av1 AV1, vp9 VP9, vp8 VP8, prores ProRes, mpeg4 MPEG-4, else upper case
pub fn source_format(container: Format, probe: &Probe) -> Format; // (Mp4, hevc) → Hevc; (Mp4 | Mkv, av1) → Av1; else container

// testutil.rs
pub struct VideoSpec { pub size: (u32, u32), pub secs: u32, pub fps: u32, pub codec: &'static str, pub audio_tracks: u32 }  // codec: "libx264" or "libvpx-vp9" (no libx265 in the build)
pub fn video(dir: &Path, file: &str, spec: &VideoSpec) -> Option<PathBuf>; // None without FERRY_FFMPEG
// lavfi testsrc2 + one sine per track (440 Hz × k), -map each; libx264 "-preset ultrafast -crf 10",
// libvpx-vp9 "-deadline realtime -cpu-used 8 -crf 10 -b:v 0"; -pix_fmt yuv420p; audio aac (libopus for .webm); -shortest
```

Parsing rules for `parse`:
- Read only from `Input #0` up to the first `Output #` or `Stream mapping:` line.
- `Duration:` line: `HH:MM:SS.cc` → `duration` (`N/A` → `Duration::ZERO`); `, bitrate: N kb/s` → `N*1000`.
- A line containing `Stream #0:` and `: Video: ` that does **not** contain `(attached pic)` is the video stream. Only the first such stream is used; later ones are ignored.
- Split the text after `Video: ` on `", "` at parenthesis depth 0 (a small `split_top_level` helper; `[…]` counts as depth too). In the first item, the first word is the codec. The second item's name before `(` is the pixel format (sets `ten_bit`), and its parenthesised part sets `hdr`. Of the other items: the first whose leading word is `<w>x<h>` gives the dimensions, one ending ` kb/s` gives the bitrate, and one ending ` fps` gives fps (if it parses as `f32`).
- `: Audio: ` lines: the codec is the first word, the bitrate an item ending ` kb/s`. `: Subtitle: ` lines are counted.
- A `displaymatrix: rotation of <deg> degrees` line after the video stream line and before the next `Stream #` line: if `round(|deg|) % 180 == 90`, swap width and height.

- [ ] **Step 1: Write the failing tests.** Embed these samples as `const` strings:

```text
HDR_PORTRAIT_MOV =
Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'IMG_0001.MOV':
  Metadata:
    major_brand     : qt  
  Duration: 00:00:01.00, start: 0.000000, bitrate: 688 kb/s
  Stream #0:0[0x1]: Video: hevc (Main 10) (hvc1 / 0x31637668), yuv420p10le(tv, bt2020nc/bt2020/arib-std-b67, progressive), 640x360 [SAR 1:1 DAR 16:9], 500 kb/s, 30 fps, 30 tbr, 15360 tbn (default)
    Side data:
      displaymatrix: rotation of 90.00 degrees
  Stream #0:1[0x2]: Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, mono, fltp, 70 kb/s (default)
  Stream #0:2[0x3]: Audio: aac (LC) (mp4a / 0x6134706D), 44100 Hz, mono, fltp, 68 kb/s
At least one output file must be specified

COVER_FIRST_MKV =
Input #0, matroska,webm, from 'talk.mkv':
  Duration: 00:01:02.50, start: 0.000000, bitrate: 379 kb/s
  Stream #0:0: Video: png, rgb24(pc, gbr/unknown/unknown), 64x64 [SAR 1:1 DAR 1:1], 90k tbr, 90k tbn (attached pic)
  Stream #0:1(eng): Video: h264 (High), yuv420p(progressive), 1920x1080 [SAR 1:1 DAR 16:9], 29.97 fps, 29.97 tbr, 1k tbn (default)
  Stream #0:2(eng): Audio: aac (LC), 48000 Hz, stereo, fltp (default)
  Stream #0:3(eng): Subtitle: subrip

WEBM =
Input #0, matroska,webm, from 'a.webm':
  Duration: 00:00:01.01, start: -0.007000, bitrate: 395 kb/s
  Stream #0:0: Video: vp9 (Profile 0), yuv420p(tv, progressive), 320x240, SAR 1:1 DAR 4:3, 25 fps, 25 tbr, 1k tbn
  Stream #0:1: Audio: opus, 48000 Hz, mono, fltp
```

```rust
#[test] fn a_rotated_stream_reports_display_dimensions() {
    let p = parse(HDR_PORTRAIT_MOV).unwrap();
    assert_eq!((p.video.width, p.video.height), (360, 640));
    assert_eq!(p.video.codec, "hevc");
    assert!(p.video.ten_bit && p.video.hdr);
    assert_eq!((p.video.fps, p.video.bitrate), (Some(30.0), Some(500_000)));
    assert_eq!(p.duration, Duration::from_secs(1));
    assert_eq!(p.bitrate, Some(688_000));
    assert_eq!(p.audio.len(), 2);
    assert_eq!(p.audio[1].bitrate, Some(68_000));
}
#[test] fn an_attached_picture_is_not_the_video() {
    let p = parse(COVER_FIRST_MKV).unwrap();
    assert_eq!((p.video.codec.as_str(), p.video.width, p.video.height), ("h264", 1920, 1080));
    assert_eq!(p.video.fps, Some(29.97));
    assert!(!p.video.ten_bit && !p.video.hdr);
    assert_eq!((p.audio.len(), p.audio[0].bitrate, p.subtitles), (1, None, 1));
    assert_eq!(p.duration, Duration::from_millis(62_500));
}
#[test] fn a_webm_without_stream_bitrates_still_parses() { let p = parse(WEBM).unwrap(); assert_eq!((p.video.codec.as_str(), p.video.width, p.video.bitrate), ("vp9", 320, None)); }
#[test] fn audio_only_or_garbage_is_not_a_video() {
    assert_eq!(parse("Input #0, mp3, from 'a.mp3':\n  Duration: 00:00:01.00, start: 0, bitrate: 128 kb/s\n  Stream #0:0: Audio: mp3, 44100 Hz, stereo, fltp, 128 kb/s\n"), None);
    assert_eq!(parse("a.mp4: Invalid data found when processing input"), None);
}
#[test] fn a_real_clip_probes_like_its_recipe() {   // skips without FERRY_FFMPEG
    // testutil::video(dir, "a.mp4", &VideoSpec { size: (320, 240), secs: 2, fps: 30, codec: "libx264", audio_tracks: 2 })
    // probe → codec "h264", 320×240, duration within 50 ms of 2 s, 2 audio streams
}
// hardware.rs
#[test] fn the_first_working_candidate_wins_per_codec() {
    let hw = detect(|e| e.ends_with("_qsv") || e == "hevc_amf");
    if cfg!(windows) { assert_eq!(hw, Hardware { h264: Some("h264_qsv"), hevc: Some("hevc_qsv") }); }
    let none = detect(|_| false);
    assert_eq!(none, Hardware::default());
}
#[test] fn macos_offers_videotoolbox() { if cfg!(target_os = "macos") { assert_eq!(candidates(Codec::Hevc), ["hevc_videotoolbox"]); } assert!(candidates(Codec::Av1).is_empty()); }
#[test] fn hevc_has_hardware_candidates_only_where_an_os_encoder_exists() {
    // HEVC has no software fallback, so its list is the whole story: macOS and Windows non-empty, every other OS empty
    assert_eq!(candidates(Codec::Hevc).is_empty(), !(cfg!(target_os = "macos") || cfg!(windows)));
    assert!(candidates(Codec::Hevc).iter().all(|e| e.starts_with("hevc_")));
}
// video/mod.rs
#[test] fn the_source_format_follows_the_codec() { /* Mp4+hevc → Hevc; Mkv+av1 → Av1; Mov+hevc → Mov; WebM+av1 → WebM; Mp4+h264 → Mp4 */ }
#[test] fn codecs_get_their_usual_names() { assert_eq!(codec_label("h264"), "H.264"); assert_eq!(codec_label("prores"), "ProRes"); assert_eq!(codec_label("dnxhd"), "DNXHD"); }
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core probe` → FAIL.
- [ ] **Step 3: Implement** as specified, and add the two strings.
- [ ] **Step 4: Run.** `cargo test -p ferry-core probe`, `cargo test -p ferry-core hardware`, `cargo test -p ferry-core video`. Expected: PASS (the real-clip test runs locally with `FERRY_FFMPEG` set).
- [ ] **Step 5: Commit.** `git commit -m "feat(core): probe videos and detect hardware encoders"`.

### Task 7: Speed and mute in the recipe (encoder: stream F's field)

**Files:**
- Modify: `crates/ferry-core/src/edit/mod.rs`, `src/lib/ipc.ts` (`Edits`, `Speed`), `src/lib/card.fixture.ts`
- Test: `edit/mod.rs` tests; `pnpm check`

**Interfaces:**
- Consumes: stream F's `optimise::Encoder` (on the branch since Task 5 Step 0) and F's `Edits.encoder` / `EditsPatch.encoder` (Decisions, "Encoder choice").
- Produces:

```rust
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Speed { pub percent: u16, pub keep_frames: bool }  // 50..=400; 100 means no change
// Edits gains (at the end, under // stream A: video):
pub speed: Option<Speed>,
pub mute: bool,
// EditsPatch gains (under // stream A: video):
#[serde(deserialize_with = "present")] pub speed: Option<Option<Speed>>,
pub mute: Option<bool>,
// F's encoder field, ONLY if `grep -n "pub encoder" crates/ferry-core/src/edit/mod.rs` prints nothing
// (under // stream F: pipelines (encoder; added early by stream A, identical to F's Task 1)):
pub encoder: Option<Encoder>,                                                   // in Edits; None: the settings' encoder
#[serde(deserialize_with = "present")] pub encoder: Option<Option<Encoder>>,    // in EditsPatch
```

- `is_plain()` is also false when `speed.is_some() || mute`. It is **not** affected by `encoder` (F's rule: a plain run with an encoder choice still follows never-worse). `reshapes()` is unchanged.
- `merge`: speed `percent` clamped to 50..=400, then `None` if it equals 100; mute set. If A adds F's `encoder`: `if let Some(encoder) = patch.encoder { self.encoder = encoder; }` (set or clear), in the F-commented block.
- If F's `EditsPatch::from_recipe` is already on the branch, extend it with `speed: Some(e.speed)` and `mute: Some(e.mute)` so it still makes the target equal to `e`. If it is not, add nothing (F's own Task 1 must then include A's fields; see the F hook).
- Image runs ignore `speed` and `mute` (`edit::apply` is unchanged by this stream).

```ts
export interface Speed { percent: number; keep_frames: boolean }
// Edits, under // stream A: video: speed: Speed | null; mute: boolean;
// Edits, F's field, only if absent (under // stream F: pipelines): encoder: 'fast' | 'small' | 'visually_lossless' | null;
```

- [ ] **Step 1: Write the failing tests.**

```rust
#[test] fn speed_is_clamped_and_normal_speed_is_none() {
    let mut e = Edits::default();
    e.merge(EditsPatch { speed: Some(Some(Speed { percent: 1000, keep_frames: true })), ..Default::default() });
    assert_eq!(e.speed, Some(Speed { percent: 400, keep_frames: true }));
    e.merge(EditsPatch { speed: Some(Some(Speed { percent: 100, keep_frames: false })), ..Default::default() });
    assert_eq!(e.speed, None);
    e.merge(EditsPatch { speed: Some(Some(Speed { percent: 10, keep_frames: false })), ..Default::default() });
    assert_eq!(e.speed.map(|s| s.percent), Some(50));
}
#[test] fn speed_and_mute_are_explicit_edits_but_an_encoder_is_not() {
    for patch in [
        EditsPatch { speed: Some(Some(Speed { percent: 200, keep_frames: true })), ..Default::default() },
        EditsPatch { mute: Some(true), ..Default::default() },
    ] { let mut e = Edits::default(); e.merge(patch); assert!(!e.is_plain()); }
    let mut e = Edits::default();
    e.merge(EditsPatch { encoder: Some(Some(Encoder::Small)), ..Default::default() });
    assert_eq!(e.encoder, Some(Encoder::Small));
    assert!(e.is_plain());
}
#[test] fn the_new_fields_read_from_card_json() {
    let p: EditsPatch = serde_json::from_str(r#"{"speed":{"percent":150,"keep_frames":false},"mute":true,"encoder":null}"#).unwrap();
    assert_eq!(p.speed, Some(Some(Speed { percent: 150, keep_frames: false })));
    assert_eq!((p.mute, p.encoder), (Some(true), Some(None)));
    let old: Edits = serde_json::from_str(r#"{"scale":50}"#).unwrap();   // a Cycle 2 recipe
    assert_eq!((old.speed, old.mute, old.encoder), (None, false, None));
}
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core edit` → FAIL.
- [ ] **Step 3: Implement**; update `ipc.ts` and `card.fixture.ts` (`speed: null, mute: false`, plus `encoder: null` only if F's fixture line is absent).
- [ ] **Step 4: Run.** `cargo test -p ferry-core edit`, `pnpm check`, `pnpm test`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): speed and mute in the edit recipe"` (if this task added F's `encoder` field, say so in the body: "adds stream F's Edits.encoder verbatim").

### Task 8: Plan an encode

**Files:**
- Create: `crates/ferry-core/src/video/plan.rs`, `crates/ferry-core/src/video/fit.rs`
- Modify: `crates/ferry-core/src/video/mod.rs` (`pub mod plan; pub mod fit;`, constants `GIF_FPS: u32 = 15`, `GIF_LONG_EDGE: u32 = 800`, `THUMB_WIDTH: u32 = 600`)
- Test: unit tests in `plan.rs`, `fit.rs`

**Interfaces:**
- Consumes: `Probe` (Task 6), `Hardware` (Task 6), `Edits`/`Speed` (Task 7) with F's `Edits.encoder` and `optimise::Encoder`, `settings::Video` (Task 5), `edit::geometry::{crop_size, output_size, Rect}`, `optimise::{quality, OptimiseOptions}`.
- Produces:

```rust
// plan.rs
pub struct PlanInput<'a> {
    pub input: &'a Path, pub output: &'a Path, pub probe: &'a Probe,
    pub source: Format, pub target: Format, pub edits: &'a Edits, pub settings: &'a settings::Video,
    pub hardware: Hardware, pub crop: Option<Rect>, pub out: (u32, u32),
    pub software: bool,               // forced: the hardware attempt failed (H.264 → libx264; HEVC → H.264, see rule 5)
    pub video_bitrate: Option<u64>,   // fit under size: the bitrate to aim at
    pub same_container: bool,         // the result keeps the source's extension: subtitles are copied
}
#[derive(Debug, Clone, PartialEq)]
pub struct Plan {
    pub args: Vec<OsString>, pub expected: Duration, pub encoder: &'static str, pub hardware: bool,
    pub codec_label: &'static str, pub fps: Option<f32>, pub audio: bool, pub hdr_flattened: bool,
    pub hevc_as_h264: bool,           // the target asked for HEVC, no hardware HEVC encoder may be used, so H.264 was planned
}
pub fn plan(p: &PlanInput) -> Plan;
pub fn plain_target(source: Format, probe: &Probe, settings: &settings::Video) -> Format;
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Route { Encode(Format), Keep }
pub fn route(target: Format, probe: &Probe, hevc_ok: bool, plain: bool) -> Route;  // rules below; hevc_ok = hardware.hevc.is_some() && !software
pub struct Framed { pub window: (u32, u32), pub out: (u32, u32) }
pub fn frame(display: (u32, u32), edits: &Edits, target: Format) -> Framed;
pub fn codec_for(target: Format, probe: &Probe) -> Option<Codec>;  // the codec the target asks for: Mp4 H264, Hevc Hevc, Av1 Av1, WebM Vp9, Mov|Mkv: Hevc if the source is hevc else H264, Gif None
pub fn muxer(ext: &str) -> &'static str;   // mp4|m4v "mp4", mov "mov", mkv "matroska", webm "webm", gif "gif"
pub fn thumb_args(input: &Path, output: &Path) -> Vec<OsString>;  // -i in -map 0:v:0 -frames:v 1 -vf scale='min(600,iw)':-2 -c:v mjpeg -q:v 3 -f image2 out
pub fn frame_args(input: &Path, output: &Path) -> Vec<OsString>;  // -i in -map 0:v:0 -frames:v 1 -c:v png -f image2 out
pub fn crf_x264(q: u8) -> u8;  // 18 + round((100 − q) × 24 / 95)
pub fn crf_svt(q: u8) -> u8;   // 20 + round((100 − q) × 35 / 95)
pub fn crf_vp9(q: u8) -> u8;   // 15 + round((100 − q) × 35 / 95)

// fit.rs
pub const MIN_VIDEO_BPS: u64 = 50_000;
pub fn budget(limit_bytes: u64, duration: Duration, audio_bps: u64) -> (u64, bool); // (video b/s, reachable)
// total = limit × 8 × 0.96 / secs (f64, rounded); video = total.saturating_sub(audio); reachable = video ≥ MIN; video.max(MIN)
pub fn next_bitrate(video_bps: u64, got_bytes: u64, limit_bytes: u64) -> u64;      // round(v × limit / got × 0.95).max(MIN)
pub fn hardware_bitrate(q: u8, probe: &Probe, out: (u32, u32), fps: Option<f32>, codec: Codec) -> u64;
// bpp = 0.04 + 0.12 × (q/100)², × 0.7 for HEVC; cap = bpp × w × h × fps (fps None → 30);
// from_source = source video bitrate × (0.25 + 0.5 × (q/100)²) × (out pixels / source pixels), when the bitrate is known;
// result = round(min(cap, from_source)).max(100_000) as u64
```

Planning rules (`plan`), in argument order:
1. `-i <input>`.
2. Maps: `-map 0:v:0`. Then `-map 0:a?` unless audio is off (`edits.mute || settings.remove_audio` or the target is `Gif`). With `same_container`, `-map 0:s? -c:s copy`; otherwise `-sn`. Then `-map_metadata 0`.
3. Video filter chain (`-vf`, comma-joined, only when non-empty): `crop=w:h:x:y` (when `crop` is Some), `scale=W:H:flags=lanczos` (when `out` ≠ the window), `setpts=PTS*100/<percent>` (speed), then `fps=<f>`. f is the smaller of the source fps (rounded to 3 decimals) when speed has `keep_frames: false` and `settings.fps_cap.fps()` when the source fps exceeds the cap by more than 0.5. Any `fps=` filter means `-fps_mode cfr`; otherwise `-fps_mode passthrough`.
4. q = `optimise::quality(&OptimiseOptions { level: edits.level, compression: edits.compression.unwrap_or(settings.compression), ..Default::default() })`. The mode is `edits.encoder.unwrap_or(settings.encoder)`. `VisuallyLossless` counts only when `edits.compression` and `edits.fit_under` are both `None`; otherwise it acts as `Small`.
5. Encoder:
   - Start from `codec_for(target, probe)`. **HEVC (hardware only, no x265):** `hevc_ok = hardware.hevc.is_some() && !software`. If the codec is HEVC and `hevc_ok`, use `hardware.hevc` in **every** mode (Fast, Small, Visually lossless). If the codec is HEVC and not `hevc_ok`, plan **H.264** instead (the rules below for H.264 apply, with `software` still forcing libx264) and set `hevc_as_h264: true`. The container stays as given (`.mp4`, `.mov` and `.mkv` all hold H.264). In a job, `route` (below) has usually turned an `Hevc` target into `Mp4` already, so `plan` sees `Mp4` and `hevc_as_h264` stays false; `run_video` then sets the note itself (Task 10, step 4). `hevc_as_h264` is set by `plan` for `Mov`/`Mkv` targets holding HEVC and for a direct `Hevc` target.
   - H.264 uses hardware when the mode is `Fast`, `!software`, and `hardware.h264` is Some; otherwise libx264. AV1 and VP9 are always software.
   - 10-bit only when `probe.video.ten_bit` and the codec is HEVC (hardware) or AV1. Pixel format: software 8-bit `yuv420p`, 10-bit `yuv420p10le` (AV1 only); hardware 8-bit `nv12`, 10-bit `p010le` (HEVC only; hardware H.264 is always `nv12`).
   - Hardware bitrate B = `hardware_bitrate(q, …)`, except Visually lossless (when it counts, rule 4) on hardware HEVC: `hardware_bitrate(95, …)`. Small on hardware HEVC uses q like Fast.
   - With `video_bitrate: Some(b)` every encoder uses `-b:v b`, and x264/hardware also get `-maxrate 1.5b -bufsize 2b` (VideoToolbox: no `-maxrate`, see below), in place of any crf or quality option.

| Encoder | Fast (software fallback) | Small | Visually lossless |
| --- | --- | --- | --- |
| `libx264` | `-preset veryfast -crf crf_x264(q) -profile:v high` | `-preset slow -crf crf_x264(q) -profile:v high` | `-preset slow -crf 16 -profile:v high` |
| `libsvtav1` | `-preset 10 -crf crf_svt(q)` | `-preset 6 -crf crf_svt(q)` | `-preset 4 -crf 18` |
| `libvpx-vp9` | `-deadline good -cpu-used 5 -row-mt 1 -b:v 0 -crf crf_vp9(q)` | `… -cpu-used 2 …` | `… -cpu-used 1 … -crf 12` |
| `h264_videotoolbox` / `hevc_videotoolbox` | `-b:v B` with B = `hardware_bitrate`, `-profile:v high` / `-profile:v main` or `main10`, HEVC adds `-tag:v hvc1` | H.264: not used (libx264). HEVC: same as Fast | H.264: not used (libx264). HEVC: same as Fast with B at quality 95 |
| `*_nvenc` | `-preset p5 -tune hq -rc vbr -b:v B -maxrate 1.5B -bufsize 2B` (HEVC adds `-tag:v hvc1`, and `-profile:v main10` when 10-bit) | as for VideoToolbox | as for VideoToolbox |
| `*_qsv` | `-preset slow -b:v B -maxrate 1.5B -bufsize 2B` (HEVC as for NVENC) | as for VideoToolbox | as for VideoToolbox |
| `*_amf` | `-quality quality -rc vbr_peak -b:v B -maxrate 1.5B -bufsize 2B` (HEVC as for NVENC) | as for VideoToolbox | as for VideoToolbox |

   There is no `libx265` row: the build has no x265 (Decisions, "HEVC output").

   VideoToolbox gets no `-maxrate` (Intel Macs reject data-rate limits on HEVC) and no `-q:v`. FFmpeg allows `-q:v` on VideoToolbox only on Apple Silicon, and this Mac is Intel.
6. Audio (when kept). The muxer comes from the output's extension. **Copy** (`-c:a copy`) when there is no speed, the codec fits the muxer (mp4/mov: aac, mp3, alac, ac3, eac3; matroska: those plus opus, vorbis, flac, pcm_*; webm: opus, vorbis), and not (`settings.audio_to_aac` and codec ≠ aac). Otherwise **encode**: webm → `-c:a libopus -b:a <min(src or 128k, 128k), at least 48k>`; others → `-c:a aac -b:a <min(src or 160k, 192k), at least 64k>`. With `fit_under` set, the cap is 128k for AAC and 96k for Opus. Speed adds `-af atempo=<percent/100 as decimal, e.g. 1.5>`.
7. Muxer flags: mp4/mov `-movflags +faststart+use_metadata_tags`. Then `-f <muxer> <output>`.
8. **GIF target:** `-i in -an -sn -filter_complex "[0:v:0]<crop,scale,setpts as above>,fps=15,split[a][b];[a]palettegen=stats_mode=diff[p];[b][p]paletteuse=dither=sierra2_4a:diff_mode=rectangle[o]" -map "[o]" -loop 0 -f gif out`. The codec label is `"GIF"`, encoder `"gif"`, fps `Some(15.0)`.
9. `expected` = duration × 100 / speed percent (the duration when there is no speed). `fps` = the output fps: the fps filter value, else the source fps × percent/100 with `keep_frames`, else the source fps. `audio` = audio is kept and the source has audio. `hdr_flattened` = `probe.video.hdr` and the planned codec is H264/Vp9 (including an HEVC request planned as H.264) or the target is Gif. `hevc_as_h264` as in rule 5. `codec_label`/`encoder` name what was actually planned ("H.264", "libx264" for a fallback).

`frame(display, edits, target)`: window = `geometry::crop_size(display, edits)`, out = `geometry::output_size(window, edits)`. For `Gif` with no `size` and no `scale`, out is shrunk so its long edge is ≤ 800 (never enlarged). For every other target, window and out are rounded **down** to even numbers, at least 2.

`plain_target`: `Mov` → `Hevc` if the source codec is `hevc` else `Mp4` when `convert_mov`, else `Mov`. Any other source returns itself.

`route(target, probe, hevc_ok, plain)` decides, before planning, what happens when the target asks for HEVC (`codec_for(target, probe) == Some(Codec::Hevc)`) and `hevc_ok` is false. `run_video` calls it (Task 10):
- The target does not ask for HEVC, or `hevc_ok` → `Encode(target)`.
- `plain` and the source codec is `hevc` → `Keep`: a plain optimise never turns an HEVC video into H.264 (it would be larger and lose 10-bit/HDR); the job leaves the file as it is.
- `target == Hevc` → `Encode(Mp4)`: the result is recorded as MP4 (H.264), which is what it is.
- Otherwise (`Mov`/`Mkv` holding HEVC, with an explicit edit) → `Encode(target)`: same container, H.264 inside (`plan` sets `hevc_as_h264`).

- [ ] **Step 1: Write the failing tests.** `probe_1080()` is a 1920×1080 H.264 30 fps 6 Mb/s probe with one AAC 128k track, 10 s long. `input(..)` builds a `PlanInput` with `Settings::default().video`, target `Mp4`, out (1920, 1080), no crop, `Hardware::default()`, paths `/b/in.mp4` and `/w/out.mp4`. `has(args, &["-crf", "23"])` checks for a consecutive pair.

```rust
#[test] fn quality_maps_to_each_encoders_scale() {
    assert_eq!((crf_x264(80), crf_x264(100), crf_x264(5), crf_x264(60)), (23, 18, 42, 28));
    assert_eq!((crf_svt(80), crf_svt(5), crf_vp9(80), crf_vp9(5)), (27, 55, 22, 50));
}
#[test] fn a_plain_h264_optimise_keeps_every_frame_and_every_track() {
    let plan = plan(&input(..));  // encoder setting Small
    assert_eq!(plan.encoder, "libx264");
    assert!(has(&plan.args, &["-crf", "23"]) && has(&plan.args, &["-preset", "slow"]) && has(&plan.args, &["-pix_fmt", "yuv420p"]));
    assert!(has(&plan.args, &["-fps_mode", "passthrough"]) && has(&plan.args, &["-map", "0:a?"]) && has(&plan.args, &["-c:a", "copy"]));
    assert!(has(&plan.args, &["-movflags", "+faststart+use_metadata_tags"]) && has(&plan.args, &["-f", "mp4"]));
    assert!(!plan.args.iter().any(|a| a == "-vf"));
}
#[test] fn fast_uses_the_hardware_encoder_with_a_bitrate() {
    // Hardware { h264: Some("h264_videotoolbox"), .. }, encoder Fast
    assert_eq!((plan.encoder, plan.hardware), ("h264_videotoolbox", true));
    assert!(has(&plan.args, &["-b:v", "3420000"]) && !plan.args.iter().any(|a| a == "-crf" || a == "-q:v" || a == "-maxrate"));
    // 6 Mb/s × (0.25 + 0.5×0.64) = 3 420 000 < cap 7 265 894
}
#[test] fn a_failed_hardware_encode_is_planned_again_in_software() { /* software: true → "libx264", hardware false */ }
#[test] fn ten_bit_hdr_stays_main10_on_plain_hevc() {
    // probe hevc ten_bit hdr, target Hevc, Hardware { hevc: Some("hevc_videotoolbox"), .. }:
    // for each mode Fast, Small, VisuallyLossless → encoder "hevc_videotoolbox", hardware true, -pix_fmt p010le,
    //   -profile:v main10, -tag:v hvc1, no "-crf", hdr_flattened false, hevc_as_h264 false
    // VisuallyLossless → "-b:v" is hardware_bitrate(95, &probe, out, fps, Codec::Hevc); Small → the same "-b:v" as Fast
    // no arg anywhere equals "libx265"
}
#[test] fn hevc_without_a_hardware_encoder_is_written_as_h264() {
    // same probe, target Hevc, Hardware::default(), encoder Small → "libx264", -pix_fmt yuv420p, codec_label "H.264",
    //   hevc_as_h264 true, hdr_flattened true
    // Hardware { hevc: Some("hevc_videotoolbox"), h264: Some("h264_videotoolbox") }, software: true → "libx264", hevc_as_h264 true
    // Hardware { hevc: None, h264: Some("h264_videotoolbox") }, Fast → "h264_videotoolbox" (nv12), hevc_as_h264 true
    // target Mp4 never sets hevc_as_h264
}
#[test] fn a_plain_hevc_optimise_never_becomes_h264() {
    // hevc probe (hp) and h264 probe (p):
    assert_eq!(route(Format::Hevc, &hp, false, true), Route::Keep);
    assert_eq!(route(Format::Mov, &hp, false, true), Route::Keep);              // a MOV holding HEVC, convert_mov off
    assert_eq!(route(Format::Hevc, &hp, false, false), Route::Encode(Format::Mp4));
    assert_eq!(route(Format::Mkv, &hp, false, false), Route::Encode(Format::Mkv));
    assert_eq!(route(Format::Hevc, &p, false, false), Route::Encode(Format::Mp4)); // "Convert to HEVC" from a pipeline
    assert_eq!(route(Format::Hevc, &hp, true, true), Route::Encode(Format::Hevc));
    assert_eq!(route(Format::Mp4, &p, false, true), Route::Encode(Format::Mp4));
}
#[test] fn hdr_to_h264_is_flagged_flattened() { /* same probe, target Mp4 → libx264, yuv420p, hdr_flattened true; target Gif → true */ }
#[test] fn frames_are_rounded_down_to_even_sizes() {
    let e = |f: &dyn Fn(&mut Edits)| { let mut e = Edits::default(); f(&mut e); e };
    assert_eq!(frame((1366, 767), &e(&|e| e.scale = Some(50)), Format::Mp4).out, (682, 384));
    let f = frame((1920, 1080), &e(&|e| e.aspect = Some((191, 100))), Format::Mp4);
    assert_eq!(f.window, (1920, 1004));   // fit_ratio gives 1005, rounded down to even
    assert_eq!(frame((3, 3), &e(&|e| e.scale = Some(10)), Format::Mp4).out, (2, 2));
}
#[test] fn a_gif_is_capped_at_800_px_unless_sized() {
    assert_eq!(frame((1920, 1080), &Edits::default(), Format::Gif).out, (800, 450));
    assert_eq!(frame((640, 360), &Edits::default(), Format::Gif).out, (640, 360));
    let mut e = Edits::default(); e.size = Some(Size::Width { width: 1200 });
    assert_eq!(frame((1920, 1080), &e, Format::Gif).out, (1200, 675));
}
#[test] fn crop_scale_and_speed_share_one_filter_chain() {
    // crop Some(Rect{x:0,y:28,width:1920,height:1024}), out (960, 512), speed {200, keep_frames: true}
    // → -vf "crop=1920:1024:0:28,scale=960:512:flags=lanczos,setpts=PTS*100/200", -fps_mode passthrough,
    //   -af atempo=2, audio re-encoded "-c:a aac -b:a 128000", expected 5 s, fps Some(60.0)
}
#[test] fn drop_frames_and_the_fps_cap_use_one_fps_filter() {
    // speed {150, keep_frames: false} on 30 fps → "fps=30" and -fps_mode cfr, fps Some(30.0)
    // fps_cap Fps24 on a 60 fps source without speed → "fps=24"; on a 24 fps source → no fps filter
}
#[test] fn every_audio_track_is_mapped_and_none_is_required() {
    // "-map 0:a?" present (the ? makes a silent source fine); mute → no "0:a?", has "-an"; remove_audio setting → same
}
#[test] fn webm_gets_vp9_and_opus() {
    // target WebM, out path /w/out.webm, AAC source → libvpx-vp9 "-b:v 0 -crf 22 -row-mt 1", "-c:a libopus -b:a 128000", -f webm, no -movflags
}
#[test] fn av1_goes_to_matroska_with_svt() { /* target Av1, /w/out.mkv → libsvtav1 -crf 27 -preset 10 (Fast without hardware), -f matroska */ }
#[test] fn visually_lossless_yields_to_an_explicit_compression() {
    // encoder VisuallyLossless → x264 -crf 16; with edits.compression Some(60) → -crf 28 -preset slow
}
#[test] fn mov_becomes_mp4_or_hevc_when_converting_mov_is_on() {
    // plain_target(Mov, h264) → Mp4; (Mov, hevc) → Hevc; convert_mov off → Mov; plain_target(Hevc, _) → Hevc
}
#[test] fn subtitles_ride_along_only_in_the_same_container() { /* same_container → "-map 0:s?" "-c:s copy"; else "-sn" */ }
#[test] fn a_video_to_gif_uses_a_palette() {
    // target Gif, /w/out.gif → "-filter_complex" whose value contains "fps=15", "palettegen=stats_mode=diff", "paletteuse=dither=sierra2_4a"; "-loop" "0"; "-f" "gif"; "-an"
}
#[test] fn fit_under_size_uses_a_bitrate_in_every_encoder() { /* video_bitrate Some(800_000) → x264 "-b:v 800000 -maxrate 1200000 -bufsize 1600000", no "-crf"; AAC capped at 128000 */ }
// fit.rs
#[test] fn the_budget_leaves_room_for_audio_and_the_container() {
    assert_eq!(budget(10_000_000, Duration::from_secs(10), 128_000), (7_552_000, true));
    assert_eq!(budget(100_000, Duration::from_secs(60), 128_000), (MIN_VIDEO_BPS, false));
}
#[test] fn a_miss_scales_the_next_bitrate() { assert_eq!(next_bitrate(1_000_000, 12_000_000, 10_000_000), 791_667); }
#[test] fn hardware_bitrate_follows_quality_and_size() {
    let mut p = probe_1080(); p.video.bitrate = None;
    assert_eq!(hardware_bitrate(80, &p, (1920, 1080), Some(30.0), Codec::H264), 7_265_894);
    assert_eq!(hardware_bitrate(80, &p, (1920, 1080), Some(30.0), Codec::Hevc), 5_086_126);
    assert!(hardware_bitrate(5, &p, (16, 16), None, Codec::H264) == 100_000);
}
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core plan` → FAIL.
- [ ] **Step 3: Implement** `plan.rs` and `fit.rs` as specified. Build args as `Vec<OsString>`; format numbers with `to_string()` and decimals without a trailing `.0` (`atempo=2`, `atempo=1.5`, `fps=29.97`).
- [ ] **Step 4: Run.** `cargo test -p ferry-core plan`, `cargo test -p ferry-core fit`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): plan video encodes from the edit recipe"`.

### Task 9: Results as bytes or files in the pipeline (no behaviour change)

**Files:**
- Modify: `crates/ferry-core/src/fsutil.rs`, `crates/ferry-core/src/pipeline.rs`
- Test: `fsutil.rs` tests; every existing `pipeline` test unchanged

**Interfaces:**
- Produces:

```rust
// fsutil.rs
/// `atomic_replace`, but the new content is a copy of the file `from` (which stays).
pub fn atomic_replace_from(target: &Path, from: &Path) -> io::Result<()>;
// Both share a private `fn replace_with(target: &Path, fill: impl FnOnce(&Path) -> io::Result<()>) -> io::Result<()>`
// holding today's symlink, read-only, permissions, fsync and rename logic. `fill` writes the temp path:
// atomic_replace: write_synced; atomic_replace_from: fs::copy(from, tmp) then File::open(tmp)?.sync_all().

// pipeline.rs (private)
#[derive(Clone, Copy)]
enum Content<'a> { Bytes(&'a [u8]), File(&'a Path) }
// put(&mut self, result: Content, format, original: Content, o, deliver)
// put_back(&mut self, original: Content, replace, o, deliver)
// restore_source(&mut self, original: Content, o)
// write_file(&self, path, content: Content, source, o)   // Bytes → atomic_replace, File → atomic_replace_from
// fn backup_path(&mut self, backups: &Backups) -> io::Result<PathBuf>   // today's `original` without the read
// fn original(&mut self, backups) -> io::Result<Vec<u8>>                // fs::read(self.backup_path(backups)?)
```

The Clipboard branches of `put` and `put_back` need `Content::Bytes`. Given a `File`, they return `Err((t("error.unexpected"), "a clipboard result must be in memory".into()))`. The existing image paths pass `Content::Bytes(&bytes)` and `Content::Bytes(&original)`.

- [ ] **Step 1: Write the failing tests** (in `fsutil.rs`):

```rust
#[test] fn replacing_from_a_file_copies_it_and_keeps_the_source_file() { /* target holds "old", from holds "new" → target "new", from still exists with "new" */ }
#[test] fn replacing_from_a_file_keeps_the_targets_permissions() { /* unix: 0o600 target stays 0o600 */ }
#[test] fn replacing_from_a_missing_file_leaves_the_target_and_no_temp() { /* Err; target "old"; no "*.ferry-tmp" beside it */ }
#[test] fn replacing_from_a_file_refuses_a_read_only_target() { /* PermissionDenied, target unchanged */ }
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core fsutil` → FAIL.
- [ ] **Step 3: Implement** the fsutil helper, then the `Content` refactor. Mechanical: the function bodies are unchanged except where the bytes are written.
- [ ] **Step 4: Run.** `cargo test -p ferry-core fsutil` and `cargo test -p ferry-core pipeline`. Expected: PASS; every existing pipeline test passes unchanged.
- [ ] **Step 5: Commit.** `git commit -m "refactor(core): job results can be files as well as bytes"`.

### Task 10: Video file jobs end to end

**Files:**
- Create: `crates/ferry-core/src/pipeline/video_job.rs` (declared in `pipeline.rs` as `mod video_job;`, a child module, so it sees `Job`'s private fields; named `video_job` so it never shadows the crate's `video` module, which `pipeline.rs` uses as `video::…`)
- Modify: `crates/ferry-core/src/pipeline.rs`, `crates/ferry-core/src/job.rs`, `crates/ferry-core/src/video/mod.rs` (`step_text`), `src/lib/i18n/en.json` (`video.step.*`, `video.detail.*`, `video.note.hdr`, `video.note.hevc_as_h264`), `src/lib/ipc.ts` (`VideoMeta` with `hevc_encoder: boolean`, `JobUpdate.video`), `src/lib/card.fixture.ts` (`video: null`)
- Test: `pipeline/video_job.rs` tests (real ffmpeg; skip without `FERRY_FFMPEG`), `job.rs` test, `video/mod.rs` test

**Interfaces:**
- Consumes: Tasks 1, 4–9.
- Produces:

```rust
// job.rs
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct VideoMeta {
    pub duration_ms: u64,          // the current result's; the original's while the original is the result
    pub codec: String,             // the current result's codec label, e.g. "HEVC"
    pub fps: Option<f32>,
    pub audio: bool,               // the current result has audio
    pub original: Option<PathBuf>, // the backup: compare's left side
    pub original_duration_ms: u64,
    pub hevc_encoder: bool,        // the job's hardware has an HEVC encoder: the format bar and menu offer HEVC
}
// JobUpdate gains, last: #[serde(default)] pub video: Option<VideoMeta>,   (notice(): None)

// pipeline.rs
pub struct RunOptions { …, pub video: settings::Video, pub hardware: Option<video::hardware::Hardware> }
// RunOptions::new: video from settings.video, hardware None. None means run_video calls video::hardware::probe_hardware(),
// which waits on the worker thread for the startup probe if it is still running (so HEVC is not missed right after launch).
// Tests set Some(Hardware::default()) for deterministic software encodes.
impl Job { pub fn is_video(&self) -> bool }
// Job gains (last field, // stream A: video): video: Option<video_job::VideoState>   (clipboard/file: None; fork: clone)

// pipeline/video_job.rs
#[derive(Debug, Clone, Default)]
pub(crate) struct VideoState { pub probe: Option<Probe>, pub thumb: Option<PathBuf>, pub result: Option<Made>, pub hardware: Option<Hardware> }
// hardware: the `hw` run_video used (step 4); video_meta's hevc_encoder = self.hardware.unwrap_or_else(video::hardware::hardware).hevc.is_some()
#[derive(Debug, Clone)]
pub(crate) struct Made { pub codec: &'static str, pub fps: Option<f32>, pub duration: Duration, pub audio: bool, pub speed: Option<u16> }
impl Job {
    pub(super) fn video_file(path: &Path, settings: &Settings, container: Format, len: u64) -> Result<Job, String>;
    pub(super) fn run_video(&mut self, o: &RunOptions, backups: &Backups, emit: &mut dyn FnMut(JobUpdate)) -> Option<Finished>;
    pub(super) fn restore_video(&mut self, o: &RunOptions) -> Finished;
    pub(super) fn video_meta(&self) -> Option<VideoMeta>;
    pub(super) fn video_detail(&self) -> String;
}

// video/mod.rs
pub fn step_text(step: &str, progress: f32, elapsed: Duration) -> String;
// progress < 0.01 → step; elapsed < 5 s or progress < 0.02 → tf("video.step.progress", step, percent);
// else tf("video.step.eta", step, percent, left) with left = elapsed × (1 − p) / p as "m:ss" (or "h:mm:ss")
```

Changes in `pipeline.rs` (each marked `// stream A: video`):
- **`Job::file`:** read the metadata length, then the first 64 bytes, and sniff them. If the format `is_video()`, `return Job::video_file(path, settings, format, len)`. Otherwise carry on with today's image checks and full read, unchanged in order and messages.
- **`run_recipe`:** after the first `if o.cancel.is_cancelled() { return None; }`, `if self.video.is_some() { return self.run_video(o, backups, emit); }`.
- **`restore`:** first line `if self.video.is_some() { return self.restore_video(o); }`.
- **`edit`:** for a video job, the format is cleared when it equals `source_format` or when `!video::is_target(f)`. `Gif` is a real target for video and is **not** cleared. Images keep today's rule.
- **`put`:** the rules come from `if self.video.is_some() { &o.video.output } else { &o.output }`, used both for `resolve` and for the `converted.location` check.
- **`write_file`:** keep dates if `self.video.is_some()` ? `o.video.preserve_dates` : `o.preserve_dates`.
- **`update`:** `thumb_path` is the video thumb for video jobs (the backup otherwise); `editable` is true for video; `video: self.video_meta()`.
- **`detail`:** for a video job, return `self.video_detail()`.

`Job::video_file`: refuses an extension in `video.ignored_extensions` (`Skip::IgnoredExtension(..).reason()`) and a length over `video.max_size_mb × MB` (`Skip::TooLarge`). It builds a `Source::File` job like `Job::file`'s, with `format`/`source_format` = container, `dims: None` and `video: Some(VideoState::default())`.

`run_video`:
1. `emit(Optimising{0, t("video.step.backup")})` on the first run; `backup_path(backups)` (failure → `error.backup`).
2. With no probe yet: emit `video.step.reading`, run `probe(backup)`, set `source_format = video::source_format(container, &probe)` (and `format` too while `!changed`), and set `dims` to the display size.
3. With no thumb: `run(thumb_args(backup, <tmp>/thumb.jpg))`, then `backups.store_bytes(id, ".ferry-thumb.jpg", bytes)`. A failure is ignored (the card keeps its placeholder).
4. `hw = o.hardware.unwrap_or_else(video::hardware::probe_hardware)`, stored in `VideoState::hardware`. `target = edits.format.unwrap_or(plain_target(..))`; `plain = edits.is_plain() && !twin` (an `encoder` choice alone keeps it plain, F's rule). Then `route(target, &probe, hw.hevc.is_some(), plain)`:
   - `Keep` → `put_back(Content::File(backup), self.changed, …)`, clear the result info and finish `AlreadyOptimal`, with no encode (log at `info`: "HEVC kept: no hardware HEVC encoder").
   - `Encode(t)` → `target = t`; remember `hevc_fallback = t != the target asked for` (an `Hevc` request routed to `Mp4`).
5. Geometry: `frame(dims, &edits, target)`. Crop rect: `Framing::Smart` with window ≠ display → `run(frame_args(backup, <tmp>/frame.png))`, then `raster::decode` (PNG) and `smart::frame`. Otherwise `geometry::centre`.
6. Temp folder `<o.work_dir>/video/<job id>/`, removed by a guard on **every** exit, including cancel and failure. Output `<tmp>/out.<ext>`. ext = `source_ext` when target == `source_format`, else `target.extension()`, exactly as `put` computes it.
7. Run the plan with `run_expecting(args, plan.expected, …)`. Emit `Optimising{p, step_text(<video.step.encode{codec}> | <…_software> | <video.step.gif>, p, elapsed)}` only when the whole percent changes. `Failed` with `plan.hardware` → log the error at `warn`, then call `route` again with `hevc_ok = false` (a failed HEVC attempt means no HEVC on this run): `Keep` → as in step 4 (`AlreadyOptimal`, file untouched); `Encode(t)` → plan again with `target = t` and `software: true` (H.264 via libx264) and run once more, setting `hevc_fallback` as in step 4. `Cancelled` → `None`.
8. **GIF:** read `<tmp>/out.gif`. With `o.gifsicle`, `optimise::encode(bytes, Format::Gif, …)` at quality q, or with `fit_under`, `edit::fit::search(limit, |q| optimise::encode(…))` (its `missed` sets the note). Write the bytes back to `<tmp>/out.gif`.
9. **Fit (non-GIF):** audio b/s as the plan caps it (0 when muted or silent) gives `budget`. Encode at most 3 times (`next_bitrate` after each miss). Keep the smallest result; still over the limit → note `card.note.fit_missed`.
10. **Never worse:** for a plain run, out ≤ 99% of `before` → `Done`. Out > `before` → `Larger`; otherwise `AlreadyOptimal`. In both of those cases: `put_back(Content::File(backup), self.changed, …)`, and the result info is cleared.
11. Otherwise `put(Content::File(out), target, Content::File(backup), o, &mut |_, _, _| Ok(()))`. Set `result = Some(Made{..})` from the plan, `dims_after` = out when ≠ dims, and `note` = fit missed, else `video.note.hevc_as_h264` when `hevc_fallback || plan.hevc_as_h264`, else `video.note.hdr` when `hdr_flattened`.
12. Map errors: a `FerryError` becomes `self.fail(e.reason(), e)`; an output error from `put` stays as it is.

`restore_video`: `put_back(Content::File(backup), true, o, …)`; clear the recipe, `dims_after`, `note` and `result`; return `Restored` (like `restore`, without reading bytes).

`video_detail`: `"W×H"` or `"W×H → w×h"`, the codec label, `tf("video.detail.fps", n)` (n without a trailing `.0`: `30`, `29.97`), `tf("video.detail.speed", "1.5")` when sped up, `t("video.detail.no_audio")` when the original had audio and the result has none, `card.detail.aggressive` when aggressive. Joined with `" · "`.

- [ ] **Step 1: Write the failing tests.** Each starts with `let Some(src) = testutil::video(dir, "clip.mp4", &spec) else { return };`. Options: `RunOptions::new(&settings, testutil::gifsicle(), env.work(), test_trash, CancelToken::new())` with `settings.video.encoder = Encoder::Small` (F's `optimise::Encoder`) and then `o.hardware = Some(Hardware::default())` (deterministic software encodes, and the same result on a Mac with VideoToolbox as in CI). The `Env`, `test_trash` and `run` helpers are copied in small from the `pipeline` tests. "probe" below means `video::probe::probe(path, &CancelToken::new())`.

```rust
#[test] fn a_video_file_is_backed_up_optimised_in_place_and_keeps_its_dates() {
    // 320×240 2 s libx264 crf 10, 1 track; set the source mtime to 2020-01-01; run Normal
    // → Done, after < before; output == src; probe(src) h264 320×240; the backup equals the original bytes (compare lengths + first 4 KiB);
    //   mtime still 2020-01-01; card.thumb_path ends with ".ferry-thumb.jpg" and exists; card.video.codec == "H.264"
}
#[test] fn a_mov_becomes_an_mp4_and_the_mov_goes_to_the_trash() {
    // "clip.mov" (libx264) → Done; output "clip.mp4" exists; "clip.mov" is in .trash; card.converted; format Mp4
}
#[test] fn restore_brings_the_mov_back_and_trashes_the_mp4() { /* after the above, restore → Restored; clip.mov back with the original length; clip.mp4 in .trash */ }
#[test] fn downscaling_twice_never_compounds() { /* edit scale 50 → run; edit scale 50 again → run; probe(output) is 160×120 both times */ }
#[test] fn a_portrait_clip_is_cropped_as_it_is_seen() {
    // generate 640×360, then remux with "-display_rotation 90" (ffmpeg input option before -i) to "portrait.mp4";
    // job dims_before == (360, 640); edit aspect (9, 16) → probe(output) is 360×640 (already 9:16: unchanged) and
    // aspect (1, 1) → 360×360
}
#[test] fn twice_the_speed_halves_the_duration_and_keeps_audio() { /* speed {200, keep_frames: true} → duration within 100 ms of 1 s; audio present; detail contains "2×" */ }
#[test] fn removing_audio_leaves_a_silent_video() { /* mute → probe(output).audio is empty; card.video.audio false; detail contains "no audio" */ }
#[test] fn two_audio_tracks_survive_an_optimise() { /* audio_tracks: 2 → probe(output).audio.len() == 2 */ }
#[test] fn a_video_becomes_an_animated_gif() {
    // convert Gif → output "clip.gif"; sniff Gif; gif_frame_count > 1; long edge ≤ 800; card.format Gif
}
#[test] fn fit_under_size_lands_under_the_limit() { /* fit_under = 60% of before → after ≤ limit, note None */ }
#[test] fn a_hevc_request_without_a_hevc_encoder_is_saved_as_h264() {
    // o.hardware = Some(Hardware::default()); edit format Some(Format::Hevc) on the libx264 clip
    // → Done; probe(output).video.codec == "h264"; card.format == Some(Format::Mp4); card.video.codec == "H.264";
    //   note == "Saved as H.264 — this computer has no HEVC encoder"; card.video.hevc_encoder == false
}
#[test] fn a_cancelled_encode_changes_nothing_and_leaves_no_temp_folder() {
    // 10 s 1280×720 source; cancel from emit when progress > 0 → run returns None; source bytes unchanged; <work>/video/<id> does not exist
}
#[test] fn a_broken_video_is_an_error_and_leaves_no_temp_folder() {
    // truncate a generated mp4 to its first 2 KiB → Error with reason "Couldn't read this video" or "Couldn't encode the video — see Details";
    // the source is unchanged; no <work>/video/<id>
}
#[test] fn an_already_small_video_stays_as_it_was() { /* run Normal once, then use the output as a new job's source at the same settings → AlreadyOptimal or Larger; the file is unchanged */ }
#[test] fn the_settings_exclude_large_and_ignored_videos() {
    // pure, no ffmpeg: a file holding an ftyp isom header named "a.mp4"; video.max_size_mb = 0 after sanitise → 1, len 2 MB → TooLarge text;
    // video.ignored_extensions ["mp4"] → ".mp4 files are ignored in Settings"
}
// job.rs
#[test] fn a_video_update_round_trips_through_json() { /* JobUpdate with video Some(..) → JSON → same; JSON without "video" → None */ }
// video/mod.rs
#[test] fn step_text_adds_percent_then_time_left() {
    assert_eq!(step_text("Encoding HEVC", 0.0, Duration::ZERO), "Encoding HEVC");
    assert_eq!(step_text("Encoding HEVC", 0.42, Duration::from_secs(3)), "Encoding HEVC · 42 %");
    assert_eq!(step_text("Encoding HEVC", 0.25, Duration::from_secs(20)), "Encoding HEVC · 25 % · 1:00 left");
}
```

- [ ] **Step 2: Run.** `cargo test -p ferry-core pipeline::video_job` (with `FERRY_FFMPEG` set) → FAIL.
- [ ] **Step 3: Implement** as specified, and add the strings and the TS types (`video: VideoMeta | null` on `JobUpdate`).
- [ ] **Step 4: Run.** `cargo test -p ferry-core pipeline`, `cargo test -p ferry-core job`, `cargo test -p ferry-core video`, `pnpm check`, `pnpm test`, then clippy for the workspace. Expected: PASS. The video tests take under ~30 s on this Mac.
- [ ] **Step 5: Commit.** `git commit -m "feat(core): optimise, edit and restore video files"`.

### Task 11: App wiring: one video at a time, startup probe, kill on quit, menu

**Files:**
- Modify: `src-tauri/src/state.rs`, `src-tauri/src/jobs.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/commands.rs` (`StackConfig`), `src-tauri/src/menu.rs`, `src/lib/i18n/en.json` (`video.menu.*`), `src/lib/ipc.ts` (`StackConfig`, `CardAction`)
- Test: `menu.rs` tests, `state.rs` test

**Interfaces:**
- Consumes: `Job::is_video`, `video::hardware::probe_hardware`, `ffmpeg::{kill_all, locate}`, `video::format_id`, `EditsPatch.mute`.
- Produces:
  - `AppState::video_permits: Arc<Semaphore>` (1 permit).
  - `jobs::toggle_mute(app: &AppHandle, id: &JobId) -> Result<(), String>`: reads `relock(&entry.job).edits.mute` and calls `edit` with `mute: Some(!mute)`.
  - `StackConfig { …, video_compression: u8, speed_frames: SpeedFrames }`.
  - `menu::Action::{Speed, Mute}`; `Action::page(Speed) == Some("speed")`.
  - TS: `StackConfig.video_compression: number; speed_frames: 'keep' | 'drop'`; `CardAction.action` gains `'speed'`.

Rules:
- **`jobs::spawn`:** before taking the CPU permit, if `relock(&entry.job).is_video()`, `acquire_owned` a `video_permits` permit and hold it for the run.
- **`lib.rs` `video_startup(app)`:** spawns a thread named `ferry-video-probe` that logs `ffmpeg::locate()` (`info!` with the path, or `warn!` "ffmpeg missing"), then `let hw = probe_hardware(); info!(?hw, "video encoders")`, and `warn!("no hardware HEVC encoder: HEVC is not offered")` when `hw.hevc` is `None` (the build has no x265). The `.run` closure gets the arm `tauri::RunEvent::Exit => ferry_core::ffmpeg::kill_all(),`.
- **`menu.rs`:**
  - `fn format_key(f: Format) -> &'static str`: `video::format_id(f)` for video, `f.extension()` otherwise. `Action::key`/`parse` use it, so `Mp4` and `Hevc` get distinct ids.
  - `convert_formats(card)`: for a card whose `source_format` is video, `[Mp4, Hevc, Av1, WebM, Gif]`, without `Hevc` unless `card.video.as_ref().is_some_and(|v| v.hevc_encoder)` (HEVC is hardware only; Decisions, "HEVC output").
  - `enabled`: `Speed | Mute` need `card.can_edit` and a video card.
  - `build`: for video cards, insert `item(Action::Speed, t("video.menu.speed"))` and `item(Action::Mute, t(if card.edits.mute { "video.menu.unmute" } else { "video.menu.mute" }))` right after Crop. Build the item list as a `Vec<&dyn IsMenuItem<Wry>>`.
  - `run`: `Mute` → `jobs::toggle_mute`.

- [ ] **Step 1: Write the failing tests** (in `menu.rs`; `video_card()` is `done()` with `format`/`source_format` `Some(Format::Mp4)` and `video: Some(VideoMeta { hevc_encoder: true, … })`):

```rust
#[test] fn video_formats_get_distinct_menu_ids() {
    let card = video_card();
    for action in [Action::Convert(Format::Mp4), Action::Convert(Format::Hevc), Action::KeepBoth(Format::Av1), Action::Convert(Format::Gif), Action::Speed, Action::Mute] {
        assert_eq!(parse_id(&item_id(&card.id, action)), Some((card.id.clone(), action)));
    }
    assert_ne!(item_id(&card.id, Action::Convert(Format::Mp4)), item_id(&card.id, Action::Convert(Format::Hevc)));
}
#[test] fn a_video_card_converts_to_video_targets_and_gif() {
    assert_eq!(convert_formats(&video_card()), [Format::Mp4, Format::Hevc, Format::Av1, Format::WebM, Format::Gif]);
    assert!(!enabled(&video_card(), Action::Convert(Format::Mp4)));
    assert!(enabled(&video_card(), Action::Convert(Format::Gif)));
}
#[test] fn without_a_hevc_encoder_hevc_is_not_offered() {
    let mut c = video_card(); c.video.as_mut().unwrap().hevc_encoder = false;
    assert_eq!(convert_formats(&c), [Format::Mp4, Format::Av1, Format::WebM, Format::Gif]);
}
#[test] fn speed_and_mute_are_for_video_cards_with_their_original() {
    assert!(enabled(&video_card(), Action::Speed) && enabled(&video_card(), Action::Mute));
    assert!(!enabled(&done(), Action::Speed) && !enabled(&done(), Action::Mute));
    let mut c = video_card(); c.can_edit = false; assert!(!enabled(&c, Action::Speed));
    assert_eq!(Action::Speed.page(), Some("speed"));
}
```

  Update `ALL` and the `without_the_original_only_file_actions_stay` / `a_busy_card_offers_only_close` expectations so `Speed`/`Mute` are off on the image card. In `state.rs`: `there_is_one_video_permit` (`AppState::new(..).video_permits.available_permits() == 1`).
- [ ] **Step 2: Run.** `cargo test -p ferry menu` → FAIL.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run.** `cargo test -p ferry menu`, `cargo test -p ferry state`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Commit.** `git commit -m "feat(app): video jobs one at a time, encoder probe, menu items"`.

---

## Task group 3: the UI

### Task 12: Video cards and the speed popover (manual check)

**Files:**
- Create: `src/lib/video.ts`, `src/lib/video.test.ts`, `src/stack/SpeedPopover.svelte`
- Modify: `src/lib/edits.ts` (`formatChoices`), `src/lib/edits.test.ts`, `src/lib/cards.ts` (`Tool` gains `'speed'`), `src/stack/Card.svelte`, `src/stack/Stack.svelte`, `src/lib/i18n/en.json` (`video.speed.*`, `video.card.duration`)
- Test: `src/lib/video.test.ts`, `src/lib/edits.test.ts`, `src/lib/cards.test.ts`

**Interfaces:**
- Consumes: `JobUpdate.video`, `Edits.speed/mute`, `StackConfig.video_compression/speed_frames`, `card:action` `'speed'`, `card_edit`.
- Produces (`src/lib/video.ts`):

```ts
export const VIDEO_FORMATS: Format[] = ['mp4', 'hevc', 'av1', 'webm', 'mov', 'mkv'];
export function isVideo(format: Format | null): boolean;
export function formatDuration(ms: number): string;        // 0 → "0:00", 5400 → "0:05", 62000 → "1:02", 3723000 → "1:02:03"
export const SPEED_CHIPS: number[] = [125, 150, 200, 300];
export function speedLabel(percent: number): string;        // 125 → "1.25×", 150 → "1.5×", 200 → "2×"
export function parseSpeed(text: string): number | null;    // "1.5" 150, "1,5" 150, "2x" 200, "2×" 200, "0.5" 50, "4" 400; "0.4", "5", "", "abc" → null
export function speedPatch(percent: number | null, keepFrames: boolean): EditsPatch; // null or 100 → { speed: null }
export function extLabel(card: JobUpdate): string;          // "mp4 · HEVC" with card.video, else card.ext
export const RING = 2 * Math.PI * 24;
export function ringDash(progress: number): string;         // `${(p*RING).toFixed(1)} ${RING.toFixed(1)}`; 0.25 → "37.7 150.8"
```

UI rules:
- **`formatChoices`:** for a card where `isVideo(card.source_format ?? card.format)`, the chips are `['mp4', 'hevc', 'av1', 'webm', 'gif']` with labels from `FORMAT_LABELS`, `current` = `card.format === f`, and `firstFrame: false`.
- **`Card.svelte`:**
  - With no `thumb_path` on a video card, show a Lucide "film" outline icon instead of the image icon.
  - When `card.video` is set, a duration badge at the thumbnail's top-left: `formatDuration(card.video.duration_ms)`, 11 px tabular figures on the grey pill, with `aria-label` `t('video.card.duration', { time })`.
  - The ext chip shows `extLabel(card)`.
  - While Optimising with `progress > 0`, the second ring circle uses `stroke-dasharray={ringDash(progress)}`, rotated −90°, instead of the fixed `40 151`.
- **`Stack.svelte`:**
  - Pass `compression={isVideo(card.format) ? config.video_compression : config.compression}`.
  - Read `video.compression` / `video.speed_frames` in `settings:changed`.
  - `clickAway` also handles `'speed'`.
  - Add `{:else if open === 'speed'} <SpeedPopover {card} framesDefault={config.speed_frames} onapply={(edits) => apply(card.id, edits)} onclose={() => setTool(card.id, null)} />`.
- **`SpeedPopover.svelte`** (structure of `FitPopover.svelte`, `data-popover`, `role="dialog"`, `aria-label={t('video.speed.title')}`):
  - Chips from `SPEED_CHIPS`; the current one is pressed.
  - A custom text field (placeholder `video.speed.custom_placeholder`, `aria-invalid` when `parseSpeed` is null), with error text `video.speed.invalid`.
  - A Keep/Drop frames toggle. It starts from `card.edits.speed?.keep_frames`, else `framesDefault === 'keep'`.
  - A **Normal speed** button (`speedPatch(null, …)`) and **Apply** (Enter). Esc closes.

- [ ] **Step 1: Write the failing tests** (`video.test.ts`) with the exact values in the comments above, plus:

```ts
it('offers video targets and GIF on a video card', () => {
  const c = card({ kind: 'done' }, { format: 'hevc', source_format: 'mp4', ext: 'mp4' });
  expect(formatChoices(c).map((x) => [x.format, x.current, x.firstFrame])).toEqual([
    ['mp4', false, false], ['hevc', true, false], ['av1', false, false], ['webm', false, false], ['gif', false, false],
  ]);
});
it('names the codec beside the extension', () => {
  expect(extLabel(card({ kind: 'done' }, { ext: 'mp4', video: { duration_ms: 1000, codec: 'HEVC', fps: 30, audio: true, original: '/b/a.mp4', original_duration_ms: 1000 } }))).toBe('mp4 · HEVC');
  expect(extLabel(card({ kind: 'done' }))).toBe('png');
});
```

- [ ] **Step 2: Run.** `pnpm test` → FAIL.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run.** `pnpm test`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Manual check** (ask the user before running `pnpm tauri dev`; TextEdit as the focus target). Copy `~/Movies/<any .mov screen recording>` in Finder and press ⌃⇧C. Look at:
  1. A card appears at once with a film icon and "Waiting…".
  2. It shows "Backing up", then "Reading video", then "Encoding H.264 · NN %" with a filling ring, and after 5 s "· m:ss left".
  3. Done shows the first frame, a duration badge such as "0:12", "mp4 · H.264", a smaller size with a green pill, and the detail "2880×1800 · H.264 · 60 fps". Finder now has the `.mp4`, and the `.mov` is in the Trash.
  4. Drag Downscale to 50%: the detail reads "2880×1800 → 1440×900".
  5. Context menu → Change speed… → 2× → Apply: the duration badge halves and the detail adds "2×".
  6. Context menu → Remove audio: the detail adds "no audio" and the menu item now reads "Keep audio".
  7. Click the format bar's GIF chip: a `.gif` replaces the `.mp4` and plays animated in Quick Look (Space).
  8. Restore: the `.mov` is back in Finder.
  9. Hovering the card and pressing `5` downscales to 50%.

  Note any step that fails in the commit message.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui): video cards, duration badge, progress ring, speed popover"`.

### Task 13: The Videos settings pane and File handling rows (manual check)

**Files:**
- Create: `src/settings/controls/EncoderPicker.svelte`
- Modify: `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src/settings/controls/OutputRule.svelte`, `src-tauri/src/commands.rs` (`output_preview`), `src/lib/i18n/en.json` (settings strings)
- Test: `src/lib/schema.test.ts`; a `commands.rs` unit test is not possible (Tauri state), so the example logic stays covered by Task 5's `output.rs` test

**Interfaces:**
- Consumes: `settings.video` (Task 5), `output::{example_for, Sample}`.
- Produces:
  - `PaneId` gains `'videos'`; `panes` gets `{ id: 'videos', label: 'video.pane' }` after `images`.
  - `Control` gains `{ kind: 'encoder' }`, and `output` gains an optional `media?: 'video'`.
  - Rows (pane `videos`, in this order, ids prefixed `video_`): `video_encoder` (`video.encoder`, control `encoder`), `video_compression` (slider 5..100), `video_convert_mov`, `video_audio_to_aac`, `video_remove_audio` (switches), `video_fps_cap` (select `off/24/30/60` with labels `video.fps_cap.*`), `video_speed_frames` (select `keep`/`drop` with labels `video.speed.keep_frames`/`video.speed.drop_frames`), `video_preserve_dates` (switch), `video_ignored_extensions` (`extensions`), `video_max_size` (number 1..100000, `unit.mb`). Each uses its label and help key from the i18n table.
  - Pane `files`, group `video.group`: `video_output_optimised` (`output.optimised` / `video.output.optimised.help`, path `video.output.optimised`, `{ kind: 'output', converted: false, media: 'video' }`) and `video_output_converted` (the same with `converted: true`).
  - `output_preview(app, rule, converted, media: Option<String>)`: `media.as_deref() == Some("video")` → `Sample::VIDEO`, else `Sample::IMAGE`. `OutputRule.svelte` passes `media` when the control has one.
  - `EncoderPicker.svelte`: a `role="radiogroup"` of three buttons (`role="radio"`, `aria-checked`), each with a title (`video.encoder.fast|small|visually_lossless`) and a one-line `.help` text, laid out as cards on the 8 px grid using theme tokens. Arrow keys move the choice.

- [ ] **Step 1: Write the failing tests** (`schema.test.ts`):

```ts
it('has a Videos pane with the encoder cards first', () => {
  const videos = all.filter((s) => s.pane === 'videos');
  expect(videos[0].control.kind).toBe('encoder');
  expect(videos.map((s) => s.path)).toEqual([
    'video.encoder', 'video.compression', 'video.convert_mov', 'video.audio_to_aac', 'video.remove_audio',
    'video.fps_cap', 'video.speed_frames', 'video.preserve_dates', 'video.ignored_extensions', 'video.max_size_mb',
  ]);
});
```

  Update the existing File handling test: the `files` pane paths are `['images.output.optimised', 'images.output.converted', 'video.output.optimised', 'video.output.converted']`. The existing "points every value setting at a field that exists" and "has real strings" tests now cover the new rows through `sample.video`.
- [ ] **Step 2: Run.** `pnpm test` → FAIL.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run.** `pnpm test`, `pnpm check`, `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Manual check** (ask before running the app). Open Settings:
  1. The sidebar lists **Videos** after Images. The pane shows three encoder cards (Fast selected) with their one-line trade-offs, and arrow keys move the selection.
  2. Setting Frame rate cap to 30 fps and reopening Settings keeps the choice (`settings.json` holds `"fps_cap": "30"`).
  3. File handling shows a **Videos** group. Set Optimised to Same folder: its example reads `~/Desktop/clip_optimised.mp4`.
  4. Search "frame rate" finds the row and jumps to it.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui): Videos settings pane and video output locations"`.

### Task 14: Compare and preview for video (manual check)

**Files:**
- Create: `src/compare/VideoCompare.svelte`, `src/compare/video.ts`, `src/compare/video.test.ts`
- Modify: `src-tauri/src/compare.rs`, `src/compare/view.ts` (`Shot` gains `kind`, `duration_ms`, `fps`), `src/compare/Compare.svelte`, `src/lib/i18n/en.json` (`video.compare.*`)
- Test: `compare.rs` tests, `src/compare/video.test.ts`

**Interfaces:**
- Consumes: `JobUpdate.video` (`original`, `duration_ms`, `fps`, `original_duration_ms`).
- Produces:

```rust
// compare.rs: Shot gains
pub kind: &'static str,        // "image" | "video"
pub duration_ms: Option<u64>,
pub fps: Option<f32>,
```

  - `view()`: for a card with `video: Some(v)`, the original's path is `v.original` (never `thumb_path`, which is the thumbnail); a missing one gives `compare.original_gone`.
  - Both shots get `kind: "video"`. `duration_ms` is `v.original_duration_ms` for the original and `v.duration_ms` for now; `fps` is `v.fps` for now. The original's fps is unknown (`None`).
  - `head_format` reads 64 bytes, so EBML doc types are found.
  - `place()`: for a video view with an original, size for `(2 × w + 1, h)` with chrome `(0, 84 + 64)`. A single video uses `(w, h)` and the same chrome.
- `src/compare/video.ts`:

```ts
export function ratio(originalMs: number | null, nowMs: number | null): number;  // original / now; 1 when either is missing or 0
export function followerTime(leaderTime: number, r: number): number;             // leaderTime / r
export function followerRate(r: number): number;                                 // 1 / r (the result plays at the original's pace)
export function frameStep(time: number, fps: number | null, dir: 1 | -1, duration: number): number; // ± 1/fps (fps null → 1/30), clamped to 0..duration
export function drifted(a: number, b: number, fps: number | null): boolean;      // |a − b| > one frame
```

- `VideoCompare.svelte`:
  - Two `<video preload="auto" playsinline muted>` elements side by side (one in single mode). Their `src` is `versioned(convertFileSrc(path), shot)`, with the labels from `shotLabel`.
  - The original leads. The result follows with `followerRate(ratio)` and is re-seeked to `followerTime` on `seeked`, and on `timeupdate` when `drifted`.
  - Shared controls: play/pause (`video.compare.play`/`pause`, also the `k` key), a range scrubber over the original's duration (`aria-label` `video.compare.position`), ◀/▶ frame step (`video.compare.back`/`forward`, also ←/→), and a **Swap sides** toggle (`video.compare.swap`) that swaps which file is on which side and its label.
  - Esc and Space close the window as for images (unchanged).
  - A `<video>` `error` event shows `t('video.compare.cant_play', { format: shot.format })` in that pane.
  - Sound stays muted (the original's audio is on the left anyway). A player whose file cannot play shows the message, and the other keeps playing.
- `Compare.svelte`: `{#if view?.now.kind === 'video'} <VideoCompare {view} onclose={close} /> {:else} …today's markup… {/if}`.

- [ ] **Step 1: Write the failing tests.**

```rust
// compare.rs
#[test] fn a_video_card_compares_its_backup_not_its_thumbnail() {
    // backup.mp4 (ftyp isom header) and out.mp4; card.thumb_path = thumb.jpg; card.video = Some(VideoMeta { original: Some(backup), duration_ms: 5000, original_duration_ms: 10_000, fps: Some(30.0), .. })
    // view(&card, false): original.path == backup, original.kind == "video", original.duration_ms == Some(10_000), now.duration_ms == Some(5000), now.fps == Some(30.0), now.format == "MP4"
}
#[test] fn a_video_without_its_backup_cannot_be_compared() { /* original None → Err("Original no longer kept"); single mode still works */ }
```

```ts
// video.test.ts
expect(ratio(10_000, 5_000)).toBe(2);
expect(ratio(null, 5_000)).toBe(1);
expect(followerTime(4, 2)).toBe(2);
expect(followerRate(2)).toBe(0.5);
expect(frameStep(1, 25, 1, 10)).toBeCloseTo(1.04);
expect(frameStep(0, null, -1, 10)).toBe(0);
expect(drifted(1, 1.02, 30)).toBe(false);
expect(drifted(1, 1.05, 30)).toBe(true);
```

- [ ] **Step 2: Run.** `cargo test -p ferry compare`, `pnpm test` → FAIL.
- [ ] **Step 3: Implement** as specified.
- [ ] **Step 4: Run.** `cargo test -p ferry compare`, `pnpm test`, `pnpm check`, then clippy for the workspace. Expected: PASS.
- [ ] **Step 5: Manual check** (ask before running the app). On a finished video card downscaled to 50% and sped up 2×, press ⌘D:
  1. The window opens sized for two players side by side, labelled Original / Now with sizes, resolutions and "MP4".
  2. Play: both play, and the result keeps pace with the original (same scene at the same moment).
  3. Scrubbing moves both, ▶ / → steps one frame, and Swap sides swaps them.
  4. Esc closes the window and focus returns to TextEdit.
  5. Convert the card to AV1 (MKV) and press ⌘D. The right pane shows "Can't play AV1 here — open the file instead", and the left still plays.
  6. Hover a video card and press Space: Quick Look plays the video.

  The Windows preview window (single player) is built but not verified on a real desktop; say so in the commit message.
- [ ] **Step 6: Commit.** `git commit -m "feat(ui): side-by-side video compare"`.

### Task 15: Whole-stream check on this Mac and in CI (manual check)

**Files:** none (fixes found here go into the task that owns the code, with a regression test there).

- [ ] **Step 1: Automated.** Run `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ferry-core`, `cargo test -p ferry`, `pnpm check`, `pnpm test`, with `FERRY_GIFSICLE` and `FERRY_FFMPEG` set. Expected: all green.
- [ ] **Step 2: Manual check on this Mac** (ask before running the app; TextEdit as the focus target):
  1. The log (`~/Library/Logs/io.ferry.desktop/ferry.log`) shows `video encoders` with `h264: Some("h264_videotoolbox"), hevc: Some("hevc_videotoolbox")` within a few seconds of launch.
  2. A 1-minute 4K screen recording with the default Fast encoder finishes noticeably faster than with Small (Settings → Videos → Small, then card ⋯ → Re-optimise). Small gives the smaller file. Record both times and sizes.
  3. Start a long encode, then Quit Ferry from the tray. `pgrep ffmpeg` prints nothing afterwards.
  4. Two videos copied together (with "skip when more than one file" off) encode one after the other (the second card says "Waiting…" until the first is done).
  5. An iPhone portrait HDR clip (if one is at hand): plain optimise keeps it portrait, and the detail reads "HEVC". Convert to MP4: the amber "HDR colours flattened to SDR" note appears.
  6. `Ferry.app/Contents/Resources/licenses/ffmpeg/COPYING.GPLv3` and `…/licenses/THIRD-PARTY-NOTICES.md` exist in the built `.app` (`pnpm tauri build --bundles app`).
- [ ] **Step 3: Manual check in CI** (after the user allows a push): the branch run is green on both OSes. The Windows smoke test passed with ffmpeg present, and both installers are ≤ 120 MB. Status reports must say Windows video behaviour (hardware encoders, preview window, playback in WebView2) is **built but not verified on a real desktop**.
- [ ] **Step 4: Commit any fixes** (each with its test), then hand back for review.

---

## Open questions for the user

1. **Patents (H.264, HEVC, AAC).** Shipping x264/x265 software encoders in a free download may need patent-pool licences (Via LA AVC/AAC, Access Advance/Via LA HEVC). Options: (a) ship as planned and get legal advice before 1.0; (b) drop x265 and leave HEVC to the hardware encoders only, with "Small" on HEVC falling back to H.264; (c) drop both and keep only hardware H.264/HEVC plus AV1/VP9 in software. The plan assumes (a).
2. **GPL source offer.** The plan attaches the third-party sources to each GitHub Release. That also closes an existing gap for gifsicle, which shipped without its licence file or a source archive in 0.1.x/0.2.0. OK to add gifsicle's licence and source to the same archive (Task 2 and Task 3 do this)?
3. **gifski.** R§1 names gifski (AGPL-3.0) for video to GIF. The plan uses ffmpeg's palette filters plus gifsicle instead (no new AGPL component, no new sidecar). Is that acceptable, or do you want gifski's quality?
4. **FR-T3, automatic optimisation of copied video files.** No stream owns it. After this stream, video files are optimised from the hotkey (Finder copy + ⌃⇧C), from C's watchers and from D's drop zone, but not automatically on copy. Should A add a `clipboard.videos` switch, or should stream I?
5. **CI time.** The first ffmpeg build per OS will likely take 30–60 min (universal macOS builds both arches; Windows builds every library in MSYS2). Later runs use the cache. Is that acceptable, or do you prefer pinned prebuilt binaries mirrored into a Ferry release (faster CI, but a third-party build to trust)?
6. **AVI and other containers.** R§2.3 lists MP4, MOV, MKV, WebM. Stream C's watcher list includes `.avi`, which this plan does not accept (it would show an Error card). Drop AVI from C, or add AVI input (re-encoded to MP4)?
