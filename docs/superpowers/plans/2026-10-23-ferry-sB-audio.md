# Ferry Cycle 3 · Stream B (audio) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ferry optimises audio files (WAV, AIFF, FLAC, MP3, M4A, Ogg, Opus). It converts them by a per-format rule, never raises the bitrate, normalises loudness in two passes to the Streaming, Apple Music, Podcast or Broadcast target, and keeps, optimises, downscales, removes or extracts cover art. All of this works from the same result card and recipe model as images.

**Architecture:** Audio formats join `sniff::Format`. An audio file becomes an ordinary file `Job`, built by `Job::file`. Its recipe is the Cycle 2 `Edits` with three new fields (`bitrate`, `loudness`, `cover`), and the existing `size`/`scale` fields downscale the cover. Every run starts from the backed-up original. `Job::run` hands audio jobs to `pipeline/audio.rs`, which drives ffmpeg through a small `AudioTool` seam over stream A's `crate::ffmpeg` helper. The passes are: probe the streams, extract the cover, measure loudness, encode. The result is then written through the existing `put`/`put_back`/`source_state` logic, which this plan makes accept a file as well as bytes. Covers are processed by Ferry's own image engine (mozjpeg/oxipng and `edit::apply`), not by ffmpeg.

**Tech Stack:** Rust 2024 (`ferry-core`, Tauri 2 app), the ffmpeg sidecar from stream A, Svelte 5 + TypeScript, vitest.

**Spec:** `docs/superpowers/specs/2026-10-23-ferry-cycle3-streams.md` (binding; it wins over the requirements), `docs/requirements/ferry-optimize-requirements.md` (R§2.3 audio row, R§2.5 "Audio tools", R§4.3 card, R§4.6 menu, R§4.12 Audio pane), `docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md` (C1), `docs/superpowers/specs/2026-10-22-ferry-cycle2-card-tools-design.md` (C2).

## Start condition

**Start B only after stream A's ffmpeg task group is merged into the branch you start from.** That group is A's Tasks 1–4 in `docs/superpowers/plans/2026-10-23-ferry-sA-video.md`. B needs these from it:

- Task 1: `crates/ferry-core/src/ffmpeg.rs` with `locate`, `run`, `exec`, `capture`, `Exec`, `Captured`, and `FerryError` in `crates/ferry-core/src/error.rs` (re-exported as `ffmpeg::FerryError`).
- Task 2: the `src-tauri/binaries/ffmpeg-<triple>` sidecar (FFmpeg 9.0.2 with LAME and Opus, and on macOS AudioToolbox: A's macOS configure line has `--enable-audiotoolbox`, which gives the `aac_at` encoder B prefers for M4A).
- Task 3: CI sets `FERRY_FFMPEG` in the `core` job.
- Task 4: the video variants in `sniff::Format`, so `sniff.rs` merges without conflict.
- A's Task 9 is the same change as B's Task 9. Whichever lands second is skipped.

**ffmpeg's log.** `ffmpeg::run` returns `()`, but loudness measurement needs ffmpeg's log: `loudnorm` prints its pass-1 JSON to stderr, and ffmpeg has no option to write it to a file. Stream information (codec, bitrate, sample rate, cover) also comes only from the log. A's plan (`2026-10-23-ferry-sA-video.md`, Task 1) covers this with `pub fn exec(program: &Path, args: &[OsString], how: &Exec, cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<Captured, FerryError>`, where `Captured { code, stderr }` holds the last 1 MiB of stderr. B's production tool calls `ffmpeg::exec(&ffmpeg::locate()?, args, &Exec::default(), cancel, on_progress)` and returns `Captured::stderr`. `exec` reads ffmpeg's **stdout** as progress lines and does not return it, so lists that ffmpeg prints to stdout (`-encoders`, `-buildconf`) cannot be read through it. That is why B detects `aac_at` with a tiny test encode (Task 1), not by parsing `-encoders`. B changes nothing in `ffmpeg.rs`. A's `exec` puts `-hide_banner -nostdin -y -nostats -progress pipe:1 -stats_period 0.5` before the caller's arguments, so **B's argument lists never contain those flags**. Its stall limit (`STALL`, 120 s without an output line) is never reached, because `-progress` prints every 0.5 s.

## Global Constraints

- **No new dependencies.** No crate, npm package or binary is added by B. `Cargo.toml`, `Cargo.lock`, `package.json` and `pnpm-lock.yaml` are not touched.
- **B never starts ffmpeg itself.** No `Command::new`, and no `sidecar::run` with ffmpeg. Every ffmpeg call goes through `audio::tool::AudioTool`, whose production implementation calls `crate::ffmpeg`.
- **Encoders B needs in A's ffmpeg build.** Task 1 checks them:

  | Need | ffmpeg component | Licence | Note |
  | --- | --- | --- | --- |
  | MP3 out | `libmp3lame` encoder (LAME 3.100, pinned by A) | LGPL-2.0-or-later | ffmpeg has no native MP3 encoder. MP3 patents expired in 2017 |
  | M4A out (Windows, Linux, and macOS fallback) | native `aac` encoder | LGPL-2.1-or-later (part of FFmpeg) | **Flag:** AAC-LC may still carry patents in some jurisdictions (Via LA pool). `libfdk_aac` is **not** allowed: it needs `--enable-nonfree`, which makes the binary non-redistributable, and it is GPL-incompatible |
  | M4A out (macOS, preferred) | `aac_at` encoder: FFmpeg's wrapper around Apple's AudioToolbox AAC encoder. Present only when A's macOS build is configured with `--enable-audiotoolbox` (A configures with `--disable-autodetect`, so the flag must be explicit; A's Task 2 has it) | wrapper LGPL-2.1-or-later; AudioToolbox is a macOS system framework, not shipped by Ferry | Better quality than native `aac` at the same bitrate. The encoder is Apple's, so the AAC patent licence normally sits with Apple, as with A's VideoToolbox encoders (not legal advice). Chosen only when the start-up test encode succeeds (Task 1) |
  | Opus out | `libopus` encoder (Opus 1.6.1, pinned by A) | BSD-3-Clause | Royalty-free. The native `opus` encoder is experimental, so do not use it |
  | WAV/FLAC out | `pcm_s16le`, `pcm_s24le`, `flac` encoders | LGPL-2.1-or-later | |
  | Inputs | demuxers `mp3 mov wav aiff flac ogg`; decoders `mp3float aac alac flac pcm_* vorbis opus` | LGPL-2.1-or-later | |
  | Muxers | `mp3 ipod flac opus wav image2 null` | LGPL-2.1-or-later | `ipod` is the `.m4a` flavour of the MP4 muxer |
  | Filters | `loudnorm`, `aresample`, `anullsrc` | LGPL-2.1-or-later | `anullsrc` (silence) feeds the `aac_at` test encode |
  | Input device | `lavfi` | LGPL-2.1-or-later | The `aac_at` test encode reads `-f lavfi -i anullsrc=…`. A's own checks use `-f lavfi` too |

  A's build is FFmpeg 9.0.2 configured `--enable-gpl --enable-version3`, never `--enable-nonfree`, so the sidecar is **GPL-3.0-or-later**. That comes from x264/x265, which A needs for video. LAME (LGPL-2.0-or-later) and Opus (BSD-3-Clause) are linked into it, and both are compatible. Ferry itself is already GPL-3.0-or-later, so B adds no licence obligation. A's `THIRD-PARTY-NOTICES.md` and `licenses/` bundle already cover LAME and Opus.
- **Original is sacred (C1§3.3).** Back up first. Write to the working folder, then atomically replace or move into place. Never read a whole audio file into memory: audio is passed by path (`Content::File`). Delete working files when the run ends.
- **Never raise the bitrate above the original. Lossless targets show no bitrate** (R§2.3).
- **Settings:** one new `[audio]` section (`#[serde(default)]`, `extra` map). `settings::VERSION` stays `2`.
- **i18n:** every new string lives under `audio.*` in `src/lib/i18n/en.json`, a flat map. Rust reads strings with `t`/`tf`, Svelte with `t`. Existing non-audio keys may be reused but not changed.
- **Shared files:** add at the named anchors in the table below. Never reorder or reformat existing lines. rustfmt still runs, and keeps `mod` lines sorted.
- **Pipelines are stream F's.** B adds no pipeline code (see "Later: stream F").
- **JavaScript/TypeScript:** never `var`, use `const`/`let`.
- **Platforms:** the dev machine is an Intel Mac and Windows is built in CI. Windows-only code goes behind `cfg(windows)` and is called out per task. Only Task 2 has any. One behaviour is macOS-only: the `aac_at` detection (Task 1) runs a test encode only under `cfg!(target_os = "macos")`; Windows and Linux always use native `aac` and never run it.
- **Out of scope:** secure send, onboarding, Vietnamese, signing/notarisation, Windows ARM64, CLI/SDK/MCP, audio A/B compare (see User decisions), the `Cmd+minus` bitrate hotkey, watched folders (stream C).
- **Environment**, from the worktree root (copy the git-ignored `src-tauri/binaries/` from the main checkout first):
  ```bash
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  export FERRY_FFMPEG="$PWD/src-tauri/binaries/ffmpeg-$(rustc -vV | sed -n 's/^host: //p')"   # use A's variable name if it differs
  ```
  `cargo test` takes one name filter per command. Never run `pnpm tauri dev` without asking the user, because it takes over the real clipboard.
- **Commits:** one per task, message style `feat(core): …` / `feat(ui): …` / `test(core): …`, ending with the attribution trailer your session instructions give (the handover uses `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).

## Review Focus

The inputs and failures most likely to hurt a user that the spec does not spell out, most likely first. Each has a test in the task named.

1. **Files whose loudness cannot be measured** (digital silence, clips under 0.4 s; `loudnorm` reports `-inf`). The file must still convert, with its loudness left alone and an amber note. It must not crash or produce a `measured_I=-inf` filter. Tests: Task 4 `silence_is_not_measurable`, Task 11 `an_unmeasurable_file_keeps_its_loudness_and_says_so`.
2. **Cancel, or an ffmpeg failure, in either pass.** The source stays byte-identical and no `out.*` or `.ferry-tmp` is left behind. A cancel puts the card back. A failure gives an Error card with Retry. Tests: Task 11 `a_cancel_during_the_encode_changes_nothing_and_leaves_no_temp_file`, `an_encoder_failure_is_an_error_with_retry_and_the_source_is_untouched`.
3. **An already-efficient lossy file** (a 128 kbps MP3 when the setting says 192). It is never re-encoded upward and never re-encoded at all for no gain: the result is Already optimal, after one probe call and no encode. Tests: Task 7 `the_bitrate_never_rises_above_a_lossy_source`, Task 11 `a_low_bitrate_mp3_is_already_optimal_without_encoding`.
4. **Things that look like audio but are not, and audio that looks like video.** An `ftyp isom`/`mp42` MP4 video, raw ADTS `.aac`, and AVI-in-RIFF must not become audio jobs. An `ftyp M4A ` file must become one, even when A's video sniffing also inspects `ftyp`. Tests: Task 2 `video_and_raw_streams_are_not_audio`, Task 10 `an_mp4_video_is_not_taken_for_audio`.
5. **Awkward file names and big files:** spaces, non-ASCII, a name starting with `-` (`-intro.wav`), and hour-long WAVs. Every path handed to ffmpeg is absolute, so nothing is read as an option, and audio is never buffered in memory. Tests: Task 7 `every_path_argument_is_absolute`, Task 11 `a_source_named_like_an_option_is_passed_safely`, Task 9 `a_file_content_is_copied_not_read_into_memory` (asserts the `Content::File` path is used).
6. **A Mac whose ffmpeg lacks `aac_at`, or where it fails to open.** M4A must still work, with native `aac`, and nothing may break on Windows, which never asks. AudioToolbox also rounds a bitrate to the nearest rate it supports, which can be *higher*, so B snaps down first to keep "never raise". Tests: Task 1 `audiotoolbox_aac_is_chosen_only_when_a_test_encode_succeeds`, Task 7 `m4a_uses_audiotoolbox_when_offered_and_native_aac_otherwise`, `audiotoolbox_bitrates_snap_down_never_up`, Task 11 `an_m4a_on_audiotoolbox_reports_the_snapped_bitrate`.

---

## Decisions (what this plan fixes, and why)

### Formats and default outputs (R§2.3, R§4.3 format bar)

| Source (sniffed) | `Format` | ext / label | Default output (`audio.outputs.*`) | Kind |
| --- | --- | --- | --- | --- |
| WAV (`RIFF`/`RF64`/`BW64` + `WAVE`) | `Wav` | `wav` / `WAV` | `mp3` | Converted |
| AIFF (`FORM` + `AIFF`/`AIFC`) | `Aiff` | `aiff` / `AIFF` | `m4a` | Converted |
| FLAC (`fLaC`, also after an ID3 tag) | `Flac` | `flac` / `FLAC` | `m4a` | Converted |
| MP3 (ID3 tag, or an MPEG-1/2 Layer III frame sync) | `Mp3` | `mp3` / `MP3` | `mp3` | Optimised |
| M4A (`ftyp` major or compatible brand `M4A `/`M4B `/`M4P `) | `M4a` | `m4a` / `M4A` | `m4a` (AAC) | Optimised |
| Ogg Vorbis/FLAC (`OggS`, first packet not `OpusHead`) | `Ogg` | `ogg` / `Ogg` | `opus` | Converted |
| Ogg Opus (`OggS` + `OpusHead`) | `Opus` | `opus` / `Opus` | `opus` | Optimised |

- **Output targets** (`audio::TARGETS`): `Mp3, M4a, Opus, Wav, Flac`. **Format bar** (`audio::BAR`, R§4.3): `Mp3, M4a, Opus, Wav`. FLAC can only be chosen as a per-format output in Settings. Ogg Vorbis is never an output, because a good Vorbis encoder (libvorbis) is not in B's list. AIFF is never an output either.
- **Encoders:** `libmp3lame` at CBR `-b:a`, for predictable size, exact fit-under-size and simple never-raise maths. AAC (M4A): `aac_at` on macOS when the sidecar offers it, else native `aac`, both at `-b:a` (see "AAC encoder" below). `libopus -vbr on` at `-b:a`. `pcm_s16le`, or `pcm_s24le` when the source has more than 16 bits. `flac -compression_level 8`, the reference "best"; levels 9–12 are slow ffmpeg extensions.
- **Containers:** MP3 gets `-id3v2_version 3`, because ID3v2.3 is what Windows Explorer and older players read pictures from. M4A uses `-f ipod`. Opus uses `-f opus`. WAV gets `-rf64 auto`, so files over 4 GB stay valid. FLAC uses `-f flac`. Global tags are always kept with `-map_metadata 0`.
- **Sample rate** (`plan::output_rate`): Opus is always 48000 (libopus works at 48 kHz). MP3 keeps 32000/44100/48000 and the MPEG-2 rates 8000–24000. Above 48 kHz it takes 44100 when the rate is a multiple of 44100, else 48000. Any other rate becomes 44100. M4A keeps the standard AAC rates up to 48000 and maps higher rates the same way. WAV and FLAC keep the source rate. `-ar` is always passed when re-encoding, because `loudnorm` resamples to 192 kHz internally.
- **Channels:** MP3 downmixes to stereo (`-ac 2`) when the source has more than 2 channels, since MP3 has no multichannel mode. Other targets keep the source's channels.

### AAC encoder: `aac_at` on macOS, native `aac` otherwise (user decision)

- **Vocabulary** (`audio/tool.rs`, Task 1): `AacEncoder { Native, AudioToolbox }`, default `Native`. `codec()` gives the ffmpeg encoder name, `"aac"` or `"aac_at"`. There is no setting and no UI for it; the card says "M4A" either way.
- **Detection, once per process:** `audio::tool::aac_encoder()` caches its answer in a `std::sync::OnceLock<AacEncoder>`.
  - On macOS (`cfg!(target_os = "macos")`), when `installed()` finds ffmpeg, it runs `detect_aac`: **one test encode** of 0.1 s of silence, `-f lavfi -i anullsrc=r=48000:cl=stereo -t 0.1 -c:a aac_at -b:a 128k -f null -`, through the `AudioTool` seam (so through A's `ffmpeg::exec`). `Ok` means `AudioToolbox`; any error means `Native`.
  - Everywhere else, and on a Mac without ffmpeg, it returns `Native` without running anything.
  - Why a test encode and not `ffmpeg -encoders`: `-encoders` prints to stdout, which A's `exec` consumes as progress and does not return (A's Task 3 note), and B may not start ffmpeg itself. A test encode also proves the encoder opens, not just that it was compiled in. It takes well under a second.
  - "At start-up": the app calls `aac_encoder()` on a background thread in `setup()` and logs the answer (Task 13). `RunOptions::new` reads the cached value (Task 10). A job that starts before the warm-up ends waits for it (`OnceLock` blocks the second caller), it never runs a second probe.
- **Arguments:**
  - Native: `-c:a aac -b:a <N>k` (unchanged).
  - `aac_at`: `-c:a aac_at -b:a <N>k -aac_at_mode cvbr`, or `-aac_at_mode cbr` when the plan has `fit_under`. `aac_at`'s default mode (`auto`) is CBR unless a global quality is given. Constrained VBR (`cvbr`) is the mode Apple's own AAC releases use: better quality for about the same average bitrate, kept close to `-b:a`. Fit under size needs CBR's predictable size.
  - Never pass `-q:a`, `-global_quality` or `-aac_at_mode vbr`. True VBR ignores `-b:a`, which would break never-raise and fit under size. Do not pass `-aac_at_quality` either; its default is the encoder's best-quality setting.
- **Bitrate snap:** AudioToolbox replaces a requested bitrate with the nearest one it supports, which can be *higher* (150 can become 160). So with `aac_at`, `plan::encoder_kbps` snaps an M4A bitrate **down** to the `MP3_KBPS` ladder (32 … 320, the standard rates AudioToolbox offers for stereo 44.1/48 kHz). The snap runs after the stream-copy check, so copying decides on the unsnapped value, and again on every fit-under retry. At the very bottom of the range (≤ 48 kbps) AudioToolbox may still pick a slightly higher rate; that can only make Fit under size miss, which the retry loop then notes.
- **Unchanged:** sample rates (`output_rate` already gives standard AAC rates ≤ 48 kHz), channels, the `ipod` muxer, cover handling, and the stream-copy rule (an `aac_at` result probes as codec `aac`).

### Bitrate (R§2.3 "never raise")

- Settings: `audio.bitrate_kbps` (MP3, M4A; default 192) and `audio.opus_bitrate_kbps` (default 128; Opus at 128 kbps is roughly as transparent as MP3 at 192).
- Card slider stops (`BITRATE_STOPS`): `320, 256, 192, 160, 128, 96, 64`. A plain click moves to the next lower stop. The recipe field is `Edits.bitrate`.
- `plan::kbps` order: requested (recipe, else settings) → **Aggressive** is two stops lower (192 → 128, never below 64) → **never raise** (lossy source with a known bitrate caps the result at it) → clamp (MP3 32–320, M4A 32–320, Opus 16–256) → MP3 snaps down to a legal Layer III rate when the output rate is ≥ 32 kHz. Then, outside `kbps`, M4A on `aac_at` snaps down to `MP3_KBPS` (`plan::encoder_kbps`, see "AAC encoder").
- **Stream copy:** when the target equals the source, the source is lossy, the codec matches the container (`mp3`/`aac`/`opus`), no loudness or fit target is set, and the planned bitrate is ≥ 95 % of the source's, the audio is copied (`-c:a copy`). This avoids generation loss when only the cover changes. When the cover would not change either, the run is **Already optimal** without encoding (`AudioPlan::nothing_to_do`).
- **Fit under size:** the target bitrate is `floor((limit × 0.98 − cover bytes − 4096) × 8 / duration / 1000)` kbps, then the rules above apply. After encoding, if the file is still over the limit, the run retries at most twice at `kbps × limit × 0.97 / size`. A target that is still missed gets the existing note `card.note.fit_missed`. A lossless target cannot be fitted, so it gets the note when it is over.

### Loudness targets (two-pass `loudnorm`, EBU R 128 / ITU-R BS.1770 measurement)

| Target (`Loudness`) | I (LUFS) | TP (dBTP) | LRA (LU) | Mono input | Source |
| --- | --- | --- | --- | --- | --- |
| `Streaming` | −14 | −1 | 20 | `dual_mono=true` | Spotify, YouTube and Tidal normalise playback to −14 LUFS (ITU-R BS.1770). Spotify asks masters to stay under −1 dBTP |
| `AppleMusic` | −16 | −1 | 20 | `dual_mono=true` | Apple Music Sound Check plays at −16 LUFS. Apple Digital Masters asks for true peak ≤ −1 dBTP |
| `Podcast` | −16 | −1 | 11 | `dual_mono=true` (≈ −19 LUFS mono) | Apple Podcasts: −16 LKFS ±1 dB, true peak ≤ −1 dBFS. AES TD1004: −16 stereo / −19 mono |
| `Broadcast` | −23 | −1 | 20 | measured as mono | EBU R 128: −23 LUFS ±0.5 LU, maximum true peak −1 dBTP (US ATSC A/85 is −24 LKFS / −2 dBTP; not offered, see User decisions) |
| `Off` (default) | — | — | — | — | Normalising changes the audio, so the user opts in |

- **Why LRA 20 for music and broadcast:** in pass 2, `loudnorm` stays in *linear* mode (pure gain, dynamics untouched) only if the measured LRA ≤ target LRA *and* the measured peak plus the gain stays ≤ target TP. Otherwise it falls back to *dynamic* mode, which compresses. 20 is the highest LRA every `loudnorm` version accepts (newer ones allow 50), so music keeps its dynamics. Podcast uses LRA 11, the value from the `loudnorm` documentation, so speech with wide swings gets gently levelled, which podcasts want.
- **`dual_mono`:** a mono file played on two speakers sounds 3 dB louder than its mono measurement. `dual_mono=true` compensates for that, and it is the R 128-consistent way to reach "−19 LUFS mono" for consumer playback. Broadcast measures mono as mono.
- **Pass 1 (measure):** `-i <original> -map 0:a:0 -af loudnorm=I=…:TP=…:LRA=…[:dual_mono=true]:print_format=json -f null -`. The JSON is the `{…}` block after the last `[Parsed_loudnorm_` line of the log. It has `input_i`, `input_tp`, `input_lra`, `input_thresh` and `target_offset` as *strings*, which can be `"-inf"`.
- **Pass 2 (normalise):** `loudnorm=I=…:TP=…:LRA=…:measured_I=<input_i>:measured_TP=<input_tp>:measured_LRA=<max(input_lra,0.01)>:measured_thresh=<input_thresh>:offset=<target_offset>:linear=true[:dual_mono=true]:print_format=json`. `measured_LRA` is floored at 0.01 because `loudnorm` treats an exact 0 as "not measured" and would leave linear mode for a perfectly steady signal. Pass 2's JSON `normalization_type` reports `"dynamic"` when the limiter had to act, and the card then notes "Peaks limited to reach −14 LUFS".
- **Not measurable:** when `input_i` is not finite or is ≤ −70, or `input_thresh` is not finite, loudness is skipped and the note says "Too quiet to measure — loudness left as it was".
- **Progress across two passes:** pass 1 maps to 0.0–0.3 and pass 2 to 0.3–1.0 (`loudness::MEASURE_SHARE = 0.3`). Pass 1 only decodes and runs one filter, which is typically several times faster than encoding, so the bar moves at a roughly even pace. The step text is "Step 1 of 2 · Measuring loudness", then "Step 2 of 2 · Encoding MP3". Without loudness there is one pass over 0.0–1.0, labelled "Encoding MP3". Updates are throttled to 1 % steps.

### Cover art (R§2.5, R§4.6)

| Container | Holds cover? | How ffmpeg writes it |
| --- | --- | --- |
| MP3 | yes | ID3v2.3 `APIC`, from an `mjpeg`/`png` stream with `-disposition:v:0 attached_pic` |
| M4A | yes | `covr` atom, same mapping (`ipod` muxer) |
| FLAC | yes | `METADATA_BLOCK_PICTURE`, same mapping |
| Opus | **no** | ffmpeg's Ogg muxer cannot write pictures, so the cover is dropped with a note |
| WAV | **no** | no standard picture chunk, so the cover is dropped with a note |

- **Policy** (`CoverPolicy`, setting `audio.cover`, default `Optimise`):
  - `Keep`: copy the first picture (`-map 0:v:0 -c:v copy`).
  - `Optimise`: Ferry's own optimiser recompresses it in its own format.
  - `Downscale`: resize to `audio.cover_size` (Square N = centre-crop to N×N; Long edge N), output JPEG q85.
  - `Remove`: no picture.
- **Recipe precedence:** `edits.cover == Remove` beats everything. Otherwise `edits.size`/`edits.scale` (the card's Downscale slider, the `1`–`9` hover keys, the menu's "Square N px") downscale the cover. Otherwise `edits.cover`, falling back to the setting. Covers are never enlarged, because `edit::apply` clamps.
- **Only `mjpeg` and `png` covers are used.** Any other picture codec is dropped with "Cover art couldn't be read — removed". Only the first picture is carried.
- **Card thumbnail:** after the first probe, the original's cover is stored *beside the backup* as `.ferry-cover.<jpg|png>` via `Backups::store_bytes`. The stack's asset scope is `$APPDATA/backups/**`, so this needs no change to `tauri.conf.json`. With no cover, the card shows a waveform icon.
- **Extract** (menu): copies that stored cover to `<output folder>/<name> cover.<ext>`, with a unique name, and reveals it in Finder/Explorer.

### Recipe and card model (C2 extended, no parallel model)

- `Edits` gains `bitrate: Option<u16>`, `loudness: Option<Loudness>` and `cover: Option<CoverPolicy>`. `EditsPatch` gains the matching `Option<Option<…>>` fields. `is_plain` also requires all three to be `None`.
- An audio job keeps an explicit `edits.format == Some(source)`, meaning "same format, overriding the settings rule". It refuses image formats, and image jobs refuse audio formats.
- **Plain vs explicit:** a plain audio run follows C1's never-worse rule: a result that is not ≥ 1 % smaller leaves the original. A run is *explicit*, and always writes, when the recipe is not plain *or* the resolved loudness is not `Off`. The user chose that loudness.
- `JobUpdate.audio: Option<AudioCard>` (`bitrate_before`, `bitrate_after`, `lossless`, `has_cover`). On audio cards, `dims_before`/`dims_after` are the cover's size, so the existing Downscale slider label ("50 % · 1500×1500") works unchanged. `thumb_path` is the stored cover.
- Card rail for audio: Downscale (cover, only with a cover) · Compression (bitrate, hidden for lossless results and in compact) · Restore · Copy · ⋯. There is no Crop or Compare.
- Detail line: `"320 → 192 kbps · Cover 3000×3000 → 600×600 · MP3 · −14 LUFS"`. The parts are bitrate (not for lossless), cover, format label, loudness target and "aggressive", joined by `" · "`.

---

## Shared-file edits (anchors)

| File | Anchor | What B adds |
| --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | `pub mod audio;` in sorted position, trailing comment `// stream B` | module |
| `crates/ferry-core/src/settings.rs` | field after `hotkeys` with comment `// stream B: audio`; `Default for Settings`; one line at the end of `sanitised` before `self.version = …` | `pub audio: crate::audio::settings::Audio` |
| `crates/ferry-core/src/ffmpeg.rs` (A's) | — | **not touched** unless Task 1's fallback applies (`// ---- stream B: logged run ----`) |
| `src/lib/i18n/en.json` | one contiguous block of `"audio.*"` keys just before the closing `}` | all strings (Task 1) |
| `src/lib/ipc.ts` (the brief's "`src/lib/types.ts`" does not exist; `ipc.ts` holds the shared types) | `// ---- stream B: audio ----` | `Format` members, `Loudness`, `CoverPolicy`, `AudioCard`, `Edits`/`JobUpdate` fields |
| `src/stack/Card.svelte` | comments `<!-- stream B: audio -->` / `// stream B: audio` at each edit | placeholder icon, compression-model calls, tooltips |
| `src-tauri/src/lib.rs` | `mod audio; // stream B` (sorted); handler entries after `commands::pick_folder,` under `// stream B: audio`; in `setup()`, one line `audio::startup(); // stream B: audio` just before the final `Ok(())` (A adds `video_startup(app);` at the same place; keep both) | 2 commands, `aac_at` warm-up |
| `src-tauri/src/menu.rs` | `// ---- stream B: audio ----` at the `Action` enum, `FORMATS`, `NAMED`, `key`/`parse`, `enabled`, `convert_formats`, `build`, `run` | audio menu |
| `Cargo.lock`, `tauri.conf.json`, `capabilities/*`, `state.rs`, `commands.rs`, `tray.rs`, `windows.rs`, `platform/mod.rs`, `Stack.svelte` | — | **not touched** |

Other files B edits that other streams may also edit (use the same `stream B: audio` comment): `sniff.rs`, `pipeline.rs`, `edit/mod.rs`, `job.rs`, `edit/raster.rs`, `optimise/mod.rs`, `bitmap.rs`, `fsutil.rs`, `output.rs`, `trigger.rs`, `testutil.rs`, `src-tauri/src/compare.rs`, `src-tauri/src/platform/windows_clipboard.rs`, `src/lib/{edits,cards,schema,settings,settings.fixture,card.fixture}.ts`, `src/settings/Row.svelte`, `src/settings/controls/OutputRule.svelte`.

## New settings keys (`[audio]`, settings version stays 2)

| Key | Type / range | Default |
| --- | --- | --- |
| `audio.outputs.wav` · `.aiff` · `.flac` · `.mp3` · `.m4a` · `.ogg` · `.opus` | one of `mp3 m4a opus wav flac` | `mp3` · `m4a` · `m4a` · `mp3` · `m4a` · `opus` · `opus` |
| `audio.bitrate_kbps` | 32–320 | 192 |
| `audio.opus_bitrate_kbps` | 16–256 | 128 |
| `audio.loudness` | `off streaming apple_music podcast broadcast` | `off` |
| `audio.cover` | `keep optimise downscale remove` | `optimise` |
| `audio.cover_size.shape` | `square long_edge` | `square` |
| `audio.cover_size.side` | 100–3000 px | 1000 |
| `audio.ignored_extensions` | list of extensions | `[]` |
| `audio.max_size_mb` | ≥ 1 | 2000 |
| `audio.output.optimised` | OutputRule | `in_place`, `~/Music/Ferry`, `%f_optimised.%e` |
| `audio.output.converted` | OutputRule | `in_place`, `~/Music/Ferry`, `%f.%e` |

Audio jobs follow the existing `images.preserve_dates` and `backups.retention_days`.

## i18n keys (all added in Task 1, exact copy)

```json
"audio.step.probe": "Reading audio",
"audio.step.encode": "Encoding {format}",
"audio.step.measure_of": "Step 1 of 2 · Measuring loudness",
"audio.step.encode_of": "Step 2 of 2 · Encoding {format}",
"audio.error.ffmpeg_missing": "ffmpeg missing — reinstall Ferry",
"audio.error.measure": "Couldn't measure the loudness",
"audio.note.too_quiet": "Too quiet to measure — loudness left as it was",
"audio.note.limited": "Peaks limited to reach {target}",
"audio.note.cover_dropped": "{format} can't hold cover art — cover removed",
"audio.note.cover_unreadable": "Cover art couldn't be read — removed",
"audio.detail.kbps": "{n} kbps",
"audio.detail.kbps_change": "{from} → {to} kbps",
"audio.detail.cover": "Cover {w}×{h}",
"audio.detail.cover_change": "Cover {w}×{h} → {aw}×{ah}",
"audio.detail.cover_removed": "Cover removed",
"audio.detail.lufs": "{n} LUFS",
"audio.loudness.off": "Off",
"audio.loudness.streaming": "Streaming (−14 LUFS)",
"audio.loudness.apple_music": "Apple Music (−16 LUFS)",
"audio.loudness.podcast": "Podcast (−16 LUFS)",
"audio.loudness.broadcast": "Broadcast (−23 LUFS)",
"audio.cover.keep": "Keep as is",
"audio.cover.optimise": "Optimise",
"audio.cover.downscale": "Downscale",
"audio.cover.remove": "Remove",
"audio.cover.extract": "Extract",
"audio.cover.square": "Square {n} px",
"audio.cover.none": "This file has no cover art",
"audio.menu.loudness": "Normalise loudness",
"audio.menu.cover": "Cover art",
"audio.card.bitrate": "Bitrate",
"audio.card.cover_downscale": "Downscale cover",
"audio.step.progress": "{step} · {n} %",
"audio.slider.kbps": "{n} kbps",
"audio.compare_unavailable": "Audio compare isn't available yet",
"audio.pane": "Audio",
"audio.group.formats": "Output format",
"audio.group.files": "Audio",
"audio.outputs.wav": "WAV becomes",
"audio.outputs.aiff": "AIFF becomes",
"audio.outputs.flac": "FLAC becomes",
"audio.outputs.mp3": "MP3 becomes",
"audio.outputs.m4a": "M4A becomes",
"audio.outputs.ogg": "Ogg Vorbis becomes",
"audio.outputs.opus": "Opus becomes",
"audio.outputs.help": "Ferry never raises a file's bitrate. WAV and FLAC are lossless and have no bitrate.",
"audio.format.mp3": "MP3",
"audio.format.m4a": "M4A (AAC)",
"audio.format.opus": "Opus",
"audio.format.wav": "WAV",
"audio.format.flac": "FLAC",
"audio.bitrate": "Bitrate for MP3 and M4A",
"audio.bitrate.help": "192 kbps sounds the same as the original for most music.",
"audio.opus_bitrate": "Bitrate for Opus",
"audio.opus_bitrate.help": "Opus needs less: 128 kbps matches 192 kbps MP3.",
"audio.loudness": "Loudness target",
"audio.loudness.help": "Two-pass EBU R 128 normalisation, peaks kept under −1 dBTP. Streaming −14 LUFS (Spotify, YouTube), Apple Music −16, Podcast −16 (−19 for mono), Broadcast −23 (EBU R 128).",
"audio.cover": "Cover art",
"audio.cover.help": "Optimise recompresses the picture; Downscale also resizes it to the size below.",
"audio.cover_shape": "Downscaled cover",
"audio.cover_shape.help": "Square crops to the centre; Long edge keeps the shape.",
"audio.cover_shape.square": "Square",
"audio.cover_shape.long_edge": "Long edge",
"audio.cover_side": "Cover size",
"audio.cover_side.help": "Used by Downscale. Ferry never enlarges a cover.",
"audio.ignored_extensions": "Ignored extensions",
"audio.ignored_extensions.help": "Audio files with these extensions are left alone.",
"audio.max_size": "Maximum size",
"audio.max_size.help": "Larger audio files are left alone.",
"audio.unit.kbps": "kbps",
"audio.unit.px": "px"
```

Existing keys B reuses without changing them: `error.decode`, `error.encode`, `error.backup`, `error.write`, `error.too_large`, `error.ignored_extension`, `card.note.fit_missed`, `card.detail.aggressive`, `card.no_original`, `menu.*`, `card.*`, `group.images`, `unit.mb`, `output.*`.

## File structure

**ferry-core (new):**
- `src/audio/mod.rs`: vocabulary: `Loudness`, `CoverPolicy`, `AudioCard`, `TARGETS`, `BAR`, `supports_cover`, `is_lossless_format`.
- `src/audio/tool.rs`: `AudioTool` seam, `ToolError`, `Ffmpeg` (production), `installed()`; `AacEncoder`, `detect_aac`, `aac_encoder()` (the cached macOS `aac_at` check).
- `src/audio/probe.rs`: probe arguments and the parser for ffmpeg's stream info → `AudioInfo`.
- `src/audio/loudness.rs`: target table, pass-1 arguments, JSON parsing, pass-2 filter.
- `src/audio/settings.rs`: the `[audio]` settings section.
- `src/audio/plan.rs`: resolving a run (`AudioPlan`): target, bitrate, rate, channels, cover plan, copy/nothing-to-do; `encode_args`; `grade`.
- `src/audio/cover.rs`: cover extraction arguments and cover processing through the image engine.
- `src/pipeline/audio.rs`: `AudioJob`, `AudioRunOptions`, audio job creation, `run_audio`, `audio_detail`, `extract_cover`.
- `src/pipeline/audio/ffmpeg_tests.rs`: `#[cfg(test)]` end-to-end tests against the real ffmpeg.

**ferry-core (modified):** `lib.rs`, `sniff.rs`, `settings.rs`, `edit/mod.rs`, `edit/raster.rs`, `optimise/mod.rs`, `bitmap.rs`, `job.rs`, `pipeline.rs`, `fsutil.rs`, `output.rs`, `trigger.rs`, `testutil.rs`, and `ffmpeg.rs` (Task 1, only if needed).

**App:** new `src-tauri/src/audio.rs` (commands, `startup`). Modified: `lib.rs`, `menu.rs`, `compare.rs`, `platform/windows_clipboard.rs` (`cfg(windows)`).

**Frontend (modified):** `src/lib/ipc.ts`, `edits.ts`, `cards.ts`, `schema.ts`, `settings.ts`, `settings.fixture.ts`, `card.fixture.ts`, `i18n/en.json`, `src/stack/Card.svelte`, `src/settings/Row.svelte`, `src/settings/controls/OutputRule.svelte`, plus tests `edits.test.ts`, `cards.test.ts`, `schema.test.ts`.

---

### Task 1: ffmpeg log access, the `AudioTool` seam, the AAC encoder check, test fixtures and strings

**Files:**
- Modify (fallback only, Step 2): `crates/ferry-core/src/ffmpeg.rs` (A's), anchor `// ---- stream B: logged run ----`
- Create: `crates/ferry-core/src/audio/mod.rs` (only `pub mod tool;` for now), `crates/ferry-core/src/audio/tool.rs`
- Modify: `crates/ferry-core/src/lib.rs` (`pub mod audio; // stream B`)
- Modify: `crates/ferry-core/src/testutil.rs`, anchor `// ---- stream B: audio fixtures ----`
- Modify: `src/lib/i18n/en.json` (the whole `audio.*` block above)

**Interfaces:**
- Consumes: `crate::ffmpeg::{locate, exec, Exec, Captured, FerryError}` (A's Task 1). `FerryError::Cancelled` exists, and every variant implements `Display`.
- Produces:
  ```rust
  // audio/tool.rs
  pub trait AudioTool: Send + Sync + std::fmt::Debug {
      /// Run ffmpeg with `args`; Ok is ffmpeg's log (its stderr text).
      fn run(&self, args: &[OsString], cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<String, ToolError>;
  }
  #[derive(Debug, Clone, PartialEq, Eq)] pub enum ToolError { Cancelled, Failed(String) }
  #[derive(Debug, Default)] pub struct Ffmpeg;            // impl AudioTool via crate::ffmpeg
  pub fn installed() -> Option<Arc<dyn AudioTool>>;      // Some(Arc::new(Ffmpeg)) when ffmpeg::locate() is Ok
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
  pub enum AacEncoder { #[default] Native, AudioToolbox }
  impl AacEncoder { pub fn codec(self) -> &'static str; }  // "aac" | "aac_at"
  pub fn aac_at_probe_args() -> Vec<OsString>;            // -f lavfi -i anullsrc=r=48000:cl=stereo -t 0.1 -c:a aac_at -b:a 128k -f null -
  pub fn detect_aac(tool: &dyn AudioTool) -> AacEncoder;  // one test encode: Ok → AudioToolbox, any Err → Native
  pub fn aac_encoder() -> AacEncoder;                     // cached (OnceLock); macOS: installed() → detect_aac, else Native; other OS: Native, no ffmpeg run
  // testutil.rs (#[cfg(test)])
  pub fn wav_tone(segments: &[(f32, f32)], rate: u32, channels: u16) -> Vec<u8>; // (seconds, amplitude 0..1) runs of a 440 Hz sine, 16-bit PCM
  pub enum FakeStep { Log(String), Write { log: String, bytes: Vec<u8> }, Fail(String) }
  #[derive(Debug, Default)] pub struct FakeTool { pub calls: Mutex<Vec<Vec<OsString>>>, pub script: Mutex<VecDeque<FakeStep>>, pub cancel_on_call: Mutex<Option<usize>> }
  impl FakeTool { pub fn new(steps: Vec<FakeStep>) -> Arc<FakeTool>; pub fn calls(&self) -> Vec<Vec<String>>; }
  ```
  `FakeTool::run` records the arguments, then (when `cancel_on_call == Some(this call's index)`) cancels `cancel` and returns `Err(ToolError::Cancelled)`. Otherwise it calls `on_progress(0.5)` and `on_progress(1.0)`, pops the next step, and acts on it. `Write` writes `bytes` to the **last argument** as a path, unless that argument is `-`. `Fail` returns `Failed`. An empty script returns `Ok(String::new())`.

- [ ] **Step 1: Check A's contract and build.** Run `grep -n "pub fn\|pub struct\|pub enum" crates/ferry-core/src/ffmpeg.rs crates/ferry-core/src/error.rs`. Expected: `locate`, `run`, `exec`, `capture`, `Exec`, `Captured` and `FerryError`. Then, with `F="$FERRY_FFMPEG"`, run:
  ```bash
  "$F" -hide_banner -encoders | grep -E ' libmp3lame | aac | libopus | flac | pcm_s16le | pcm_s24le '
  "$F" -hide_banner -filters  | grep -E ' loudnorm | aresample '
  "$F" -hide_banner -muxers   | grep -E ' mp3 | ipod | opus | flac | wav | image2 | null '
  ```
  Then the test-encode inputs on every platform, and `aac_at` on macOS only:
  ```bash
  "$F" -hide_banner -f lavfi -i anullsrc=r=48000:cl=stereo -t 0.1 -c:a aac -b:a 128k -f null - && echo native-aac-ok
  # macOS only:
  "$F" -hide_banner -encoders | grep -E ' aac_at '
  "$F" -hide_banner -f lavfi -i anullsrc=r=48000:cl=stereo -t 0.1 -c:a aac_at -b:a 128k -aac_at_mode cvbr -f null - && echo aac_at-ok
  ```
  Expected: every name appears and each `echo` prints. If one is missing, **stop and report to the controller**. A's build must add it, because B cannot work around a missing encoder. A missing `aac_at` on macOS means A's macOS configure lacks `--enable-audiotoolbox` (needed because A configures with `--disable-autodetect`). Ferry would still work with native `aac`, but the user decided on `aac_at`, so report it rather than carrying on.
- [ ] **Step 2: Fallback only.** If A's merged `ffmpeg.rs` has `exec` returning `Captured`, skip this step. If it does not, add `pub fn run_logged(args: &[OsString], cancel: &CancelToken, on_progress: &mut dyn FnMut(f32)) -> Result<String, FerryError>` at the anchor. Move the body of A's `run` into it so that it returns the stderr text `run` already collects, and make `run` a one-liner: `run_logged(args, cancel, on_progress).map(|_| ())`. The log level must stay at least `info`, because `loudnorm` JSON and stream info print at `info`. If A's runner does not collect stderr at all, stop and report. A's existing `ffmpeg` tests must still pass: `cargo test -p ferry-core ffmpeg`.
- [ ] **Step 3: Write the failing tests** in `audio/tool.rs`:
  ```rust
  #[test] fn a_cancelled_token_never_starts_ffmpeg() {
      let c = CancelToken::new(); c.cancel();
      assert_eq!(Ffmpeg.run(&[], &c, &mut |_| {}), Err(ToolError::Cancelled));
  }
  #[test] fn ffmpeg_hands_back_its_log() {           // skipped without ffmpeg
      let Some(tool) = installed() else { return };
      let dir = tempfile::tempdir().unwrap();
      let wav = dir.path().join("tone.wav");
      std::fs::write(&wav, testutil::wav_tone(&[(0.5, 0.5)], 44100, 2)).unwrap();
      let args: Vec<OsString> = ["-i"].map(OsString::from).into_iter()
          .chain([wav.into_os_string()]).chain(["-f", "null", "-"].map(OsString::from)).collect();
      let log = tool.run(&args, &CancelToken::new(), &mut |_| {}).unwrap();
      assert!(log.contains("Input #0, wav"), "{log}");
      assert!(log.contains("44100 Hz"), "{log}");
  }
  #[test] fn a_failing_run_is_failed_with_ffmpegs_reason() {   // skipped without ffmpeg
      let Some(tool) = installed() else { return };
      let args = ["-i", "/no/such/file.wav", "-f", "null", "-"].map(OsString::from);
      assert!(matches!(tool.run(&args, &CancelToken::new(), &mut |_| {}), Err(ToolError::Failed(m)) if !m.is_empty()));
  }
  #[test] fn the_aac_at_probe_encodes_a_tenth_of_a_second_of_silence() {
      let a: Vec<String> = aac_at_probe_args().iter().map(|s| s.to_string_lossy().into()).collect();
      assert_eq!(a, ["-f", "lavfi", "-i", "anullsrc=r=48000:cl=stereo", "-t", "0.1", "-c:a", "aac_at", "-b:a", "128k", "-f", "null", "-"]);
  }
  #[test] fn audiotoolbox_aac_is_chosen_only_when_a_test_encode_succeeds() {   // Review Focus 6
      let yes = testutil::FakeTool::new(vec![testutil::FakeStep::Log(String::new())]);
      assert_eq!(detect_aac(&*yes), AacEncoder::AudioToolbox);
      let probe: Vec<String> = aac_at_probe_args().iter().map(|s| s.to_string_lossy().into()).collect();
      assert_eq!(yes.calls(), vec![probe]);
      let no = testutil::FakeTool::new(vec![testutil::FakeStep::Fail("Unknown encoder 'aac_at'".into())]);
      assert_eq!(detect_aac(&*no), AacEncoder::Native);
      assert_eq!((AacEncoder::default(), AacEncoder::Native.codec(), AacEncoder::AudioToolbox.codec()), (AacEncoder::Native, "aac", "aac_at"));
  }
  #[cfg(target_os = "macos")]
  #[test] fn the_macos_sidecar_offers_audiotoolbox_aac() {   // skipped without ffmpeg
      let Some(tool) = installed() else { return };
      assert_eq!(detect_aac(&*tool), AacEncoder::AudioToolbox, "A's macOS ffmpeg build needs --enable-audiotoolbox");
      assert_eq!(aac_encoder(), AacEncoder::AudioToolbox);
  }
  #[cfg(not(target_os = "macos"))]
  #[test] fn other_systems_use_native_aac_without_asking_ffmpeg() {
      assert_eq!(aac_encoder(), AacEncoder::Native);
  }
  ```
  In `testutil.rs`, test the fixtures:
  ```rust
  #[test] fn wav_tone_is_a_valid_pcm_wav() {
      let w = wav_tone(&[(1.0, 0.5), (0.5, 0.1)], 8000, 2);
      assert_eq!((&w[0..4], &w[8..12], &w[36..40]), (&b"RIFF"[..], &b"WAVE"[..], &b"data"[..]));
      assert_eq!(w.len(), 44 + 12_000 * 2 * 2);           // 1.5 s × 8000 Hz × 2 ch × 2 bytes
  }
  #[test] fn the_fake_records_reports_progress_and_writes_the_last_argument() {
      let dir = tempfile::tempdir().unwrap(); let out = dir.path().join("out.mp3");
      let fake = FakeTool::new(vec![FakeStep::Write { log: "done".into(), bytes: b"xyz".to_vec() }]);
      let mut seen = Vec::new();
      let log = fake.run(&[OsString::from("-i"), out.clone().into()], &CancelToken::new(), &mut |p| seen.push(p)).unwrap();
      assert_eq!((log.as_str(), seen, std::fs::read(&out).unwrap()), ("done", vec![0.5, 1.0], b"xyz".to_vec()));
      assert_eq!(fake.calls(), vec![vec!["-i".to_string(), out.display().to_string()]]);
  }
  ```
- [ ] **Step 4: Run them and confirm they fail:** `cargo test -p ferry-core audio::tool`. Expected: FAIL, because `Ffmpeg`, `installed`, `AacEncoder`, `detect_aac` and `aac_encoder` are not defined.
- [ ] **Step 5: Implement.** `Ffmpeg::run` returns `Err(Cancelled)` when `cancel.is_cancelled()` before calling. It then calls `crate::ffmpeg::exec(&ffmpeg::locate()?, args, &ffmpeg::Exec::default(), cancel, on_progress)` and returns `Captured::stderr` (or `run_logged` in the fallback). A `locate` error maps to `Failed`. On `Err(e)` it returns `Cancelled` for `FerryError::Cancelled` or when the token is now cancelled, else `Failed(e.to_string())`.
  - `detect_aac(tool)` runs `tool.run(&aac_at_probe_args(), &CancelToken::new(), &mut |_| {})` once and maps `Ok(_)` to `AudioToolbox`, any `Err` to `Native`. It does not look at the log.
  - `aac_encoder()`: `static AAC: OnceLock<AacEncoder>`; `*AAC.get_or_init(|| if cfg!(target_os = "macos") { installed().map_or(AacEncoder::Native, |t| detect_aac(&*t)) } else { AacEncoder::Native })`. Use `cfg!` (not `#[cfg]`), so the code compiles and is linted on every OS.
  - `wav_tone` writes a 44-byte canonical header (`fmt ` chunk PCM 1, `channels`, `rate`, 16 bits) followed by interleaved `i16` samples of `amp × sin(2π·440·t)`. Add the `audio.*` JSON block to `en.json`, keeping the file valid flat JSON.
- [ ] **Step 6: Run** `cargo test -p ferry-core audio::tool`, then `cargo test -p ferry-core testutil`, then `pnpm test -- i18n`. Expected: PASS. The real-ffmpeg tests return early when ffmpeg is absent. On the Mac with `FERRY_FFMPEG` set, `the_macos_sidecar_offers_audiotoolbox_aac` must pass; if it fails, stop and report (see Step 1).
- [ ] **Step 7: Commit** `feat(core): audio tool seam over the ffmpeg helper, with its log and the aac_at check`.

---

### Task 2: Audio vocabulary: formats, sniffing, targets

**Files:**
- Modify: `crates/ferry-core/src/sniff.rs` (variants and detection under `// ---- stream B: audio ----`)
- Modify (one arm each, comment `// stream B: audio never reaches the image engine`): `edit/raster.rs` (`decode`, `encode`), `optimise/mod.rs` (`encode`), `bitmap.rs` (`decode_rgba`), `pipeline.rs` (`run_recipe` step-label match)
- Modify, **Windows-only:** `src-tauri/src/platform/windows_clipboard.rs` (`format_name`). Compiled only by the Windows CI bundle job, not on the Mac.
- Modify: `crates/ferry-core/src/trigger.rs` (`normalise_ext`: `aif`/`aifc` → `aiff`)
- Modify: `crates/ferry-core/src/audio/mod.rs`

**Interfaces:**
- Produces:
  ```rust
  // sniff.rs: new variants, appended after WebP (serde lowercase: "mp3","m4a","flac","wav","aiff","ogg","opus")
  pub enum Format { Png, Jpeg, Gif, WebP, /* stream B: audio */ Mp3, M4a, Flac, Wav, Aiff, Ogg, Opus }
  impl Format { pub fn is_audio(self) -> bool; }
  pub const HEAD: usize = 64 * 1024;           // bytes Job::file reads to sniff
  // audio/mod.rs
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
  pub enum Loudness { #[default] Off, Streaming, AppleMusic, Podcast, Broadcast }
  #[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
  pub enum CoverPolicy { Keep, #[default] Optimise, Downscale, Remove }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
  pub struct AudioCard { pub bitrate_before: Option<u16>, pub bitrate_after: Option<u16>, pub lossless: bool, pub has_cover: bool }
  pub const TARGETS: [Format; 5] = [Format::Mp3, Format::M4a, Format::Opus, Format::Wav, Format::Flac];
  pub const BAR: [Format; 4] = [Format::Mp3, Format::M4a, Format::Opus, Format::Wav];
  pub fn supports_cover(f: Format) -> bool;     // Mp3 | M4a | Flac
  pub fn is_lossless_format(f: Format) -> bool; // Wav | Flac | Aiff
  ```
  Extensions and labels are those in the Decisions table. `is_animated` is `false` and `dimensions` is `None` for every audio format.

- [ ] **Step 1: Write the failing tests** in `sniff.rs`:
  ```rust
  fn ftyp(major: &[u8; 4], compat: &[&[u8; 4]]) -> Vec<u8> {   // test helper: an ftyp box
      let size = 16 + 4 * compat.len();
      let mut b = (size as u32).to_be_bytes().to_vec();
      b.extend_from_slice(b"ftyp"); b.extend_from_slice(major); b.extend_from_slice(&[0, 0, 2, 0]);
      for c in compat { b.extend_from_slice(*c); }
      b
  }
  fn ogg(packet: &[u8]) -> Vec<u8> {                           // one Ogg page, one segment
      let mut b = b"OggS\0\x02".to_vec(); b.extend_from_slice(&[0; 20]); b.push(1); b.push(packet.len() as u8);
      b.extend_from_slice(packet); b
  }
  #[test] fn detects_each_audio_container() {
      assert_eq!(sniff(&crate::testutil::wav_tone(&[(0.01, 0.5)], 8000, 1)), Some(Format::Wav));
      assert_eq!(sniff(b"RF64\xff\xff\xff\xffWAVEds64"), Some(Format::Wav));
      assert_eq!(sniff(b"FORM\0\0\0\x10AIFFCOMM"), Some(Format::Aiff));
      assert_eq!(sniff(b"FORM\0\0\0\x10AIFCFVER"), Some(Format::Aiff));
      assert_eq!(sniff(b"fLaC\0\0\0\x22"), Some(Format::Flac));
      assert_eq!(sniff(b"ID3\x03\0\0\0\0\0\x00\xff\xfb\x90\x64"), Some(Format::Mp3));
      assert_eq!(sniff(b"ID3\x04\0\0\0\0\0\x00fLaC"), Some(Format::Flac));
      assert_eq!(sniff(&[0xFF, 0xFB, 0x90, 0x64, 0, 0]), Some(Format::Mp3));   // MPEG-1 Layer III, 128 kbps, 44.1 kHz
      assert_eq!(sniff(&[0xFF, 0xF3, 0x48, 0xC4, 0, 0]), Some(Format::Mp3));   // MPEG-2 Layer III
      assert_eq!(sniff(&ogg(b"OpusHead\x01\x02")), Some(Format::Opus));
      assert_eq!(sniff(&ogg(b"\x01vorbis\0\0")), Some(Format::Ogg));
      assert_eq!(sniff(&ftyp(b"M4A ", &[b"M4A ", b"mp42", b"isom"])), Some(Format::M4a));
      assert_eq!(sniff(&ftyp(b"mp42", &[b"isom", b"M4A "])), Some(Format::M4a));
      assert_eq!(sniff(&ftyp(b"M4B ", &[])), Some(Format::M4a));
  }
  #[test] fn video_and_raw_streams_are_not_audio() {           // Review Focus 4
      // Stream A may claim the MP4/MOV ones as video; they must never be audio.
      let not_audio = |b: &[u8]| !sniff(b).is_some_and(Format::is_audio);
      assert!(not_audio(&ftyp(b"isom", &[b"isom", b"iso2", b"avc1", b"mp41"])));
      assert!(not_audio(&ftyp(b"qt  ", &[b"qt  "])));
      assert!(not_audio(&[0xFF, 0xF1, 0x50, 0x80, 0, 0]));     // ADTS AAC (layer bits 00)
      assert!(not_audio(b"RIFF\x24\0\0\0AVI LIST"));
      assert!(not_audio(&ogg(b"\x80theora")));
      assert_eq!(sniff(&[0xFF, 0xD8, 0xFF, 0xE0]), Some(Format::Jpeg));
  }
  #[test] fn audio_formats_have_names_and_no_pixels() {
      for (f, ext, label) in [(Format::Mp3, "mp3", "MP3"), (Format::M4a, "m4a", "M4A"), (Format::Flac, "flac", "FLAC"),
          (Format::Wav, "wav", "WAV"), (Format::Aiff, "aiff", "AIFF"), (Format::Ogg, "ogg", "Ogg"), (Format::Opus, "opus", "Opus")] {
          assert_eq!((f.extension(), f.label(), f.is_audio()), (ext, label, true));
          assert_eq!(serde_json::to_string(&f).unwrap(), format!("\"{ext}\""));
          assert!(!is_animated(b"", f) && dimensions(b"", f).is_none());
      }
      assert!(!Format::Png.is_audio());
  }
  ```
  Change the existing `rejects_other_riff_files` to use `b"RIFF\x24\0\0\0AVI LIST"`. WAVE is now audio. In `trigger.rs` tests, add `assert_eq!(normalise_ext(".AIF"), "aiff"); assert_eq!(normalise_ext("aifc"), "aiff");`. In `audio/mod.rs`, add:
  ```rust
  #[test] fn targets_and_covers() {
      assert_eq!(serde_json::to_string(&Loudness::AppleMusic).unwrap(), "\"apple_music\"");
      assert_eq!(serde_json::to_string(&CoverPolicy::default()).unwrap(), "\"optimise\"");
      assert!(supports_cover(Format::Flac) && !supports_cover(Format::Opus) && !supports_cover(Format::Wav));
      assert!(is_lossless_format(Format::Aiff) && !is_lossless_format(Format::M4a));
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core sniff`. Expected: FAIL to compile, because the variants are not defined.
- [ ] **Step 3: Implement** the detection in `sniff()`, after the existing image checks. Order: RIFF/RF64/BW64+`WAVE`; FORM+AIFF/AIFC; `fLaC`; `ID3`; `OggS`; `ftyp`; MP3 frame sync.
  - **ID3:** size = syncsafe `((b6&0x7f)<<21)|((b7&0x7f)<<14)|((b8&0x7f)<<7)|(b9&0x7f)`, plus 10, plus 10 more if `b5 & 0x10`. After the tag, `fLaC` means Flac. Anything else, including a tag longer than the bytes given, means Mp3.
  - **Ogg:** the payload starts at `27 + bytes[26]`.
  - **`ftyp`:** the box size is the big-endian `u32` at 0 (clamp to `len`). The major brand is at 8..12 and compatible brands are 4-byte steps from 16. The B check must sit **above any generic `ftyp` (MP4/MOV) check A added**.
  - **MP3 frame sync:** `b0 == 0xFF && b1 & 0xE0 == 0xE0 && (b1 >> 1) & 3 == 1 && (b1 >> 3) & 3 != 1 && b2 >> 4 != 0xF && (b2 >> 2) & 3 != 3`.

  Add the image-engine arms. `raster::decode` and `raster::encode` return `Err(OptimiseError::Unsupported("audio has no pixels".into()))`. `optimise::encode` returns `Err(OptimiseError::Unsupported("audio goes through ffmpeg".into()))`. `bitmap::decode_rgba` returns `Err("not an image".into())`. The `pipeline.rs` step label uses `(true, Format::Mp3 | Format::M4a | Format::Flac | Format::Wav | Format::Aiff | Format::Ogg | Format::Opus) => "card.step.edit"`; this is unreachable, because Task 11 dispatches audio first. **Windows-only:** in `windows_clipboard.rs` `format_name`, add `Format::Mp3 | Format::M4a | Format::Flac | Format::Wav | Format::Aiff | Format::Ogg | Format::Opus => unreachable!("audio never goes on the clipboard as an image")`. This matches macOS's `uti()`, which already panics on a format without a pasteboard type.
- [ ] **Step 4: Run** `cargo test -p ferry-core sniff`, then `cargo test -p ferry-core trigger`, then `cargo test -p ferry-core audio`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS and no warnings. `windows_clipboard.rs` is checked by CI only.
- [ ] **Step 5: Commit** `feat(core): sniff audio containers by content`.

---

### Task 3: Probe: ffmpeg's stream info → `AudioInfo`

**Files:** Create `crates/ferry-core/src/audio/probe.rs`. Modify `audio/mod.rs` (`pub mod probe;`).

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq)]
  pub struct AudioInfo { pub codec: String, pub sample_rate: u32, pub channels: u16, pub bits: Option<u8>,
                         pub duration_secs: Option<f64>, pub kbps: Option<u16>, pub cover: Option<CoverInfo> }
  #[derive(Debug, Clone, PartialEq, Eq)] pub struct CoverInfo { pub codec: String, pub dims: (u32, u32) }
  impl AudioInfo { pub fn lossless(&self) -> bool; }   // codec is flac, alac, wavpack, ape, tta, or starts with "pcm_"
  #[derive(Debug, Clone, PartialEq, Eq)] pub enum ProbeError { NoAudio, Unreadable(String) }
  pub fn args(input: &Path) -> Vec<OsString>;   // -i <input> -map 0:a:0 -frames:a 1 -f null -   (exec adds the global flags)
  pub fn parse(log: &str) -> Result<AudioInfo, ProbeError>;
  #[cfg(test)] pub(crate) mod samples { pub const MP3_COVER: &str; pub const M4A_PNG: &str; pub const FLAC_MONO_24: &str; pub const WAV_51: &str; pub const WAV_STEREO: &str; pub const PNG_ONLY: &str; }
  ```

- [ ] **Step 1: Write the samples and failing tests.** The samples are verbatim ffmpeg 6/7 output, so keep the leading spaces:
  ```rust
  pub(crate) const MP3_COVER: &str = "Input #0, mp3, from '/b/song.mp3':
    Metadata:
      title           : Song, with a comma
    Duration: 00:03:21.45, start: 0.025057, bitrate: 322 kb/s
    Stream #0:0: Audio: mp3 (mp3float), 44100 Hz, stereo, fltp, 320 kb/s
    Stream #0:1: Video: mjpeg (Baseline), yuvj420p(pc, bt470bg/unknown/unknown), 3000x3000 [SAR 1:1 DAR 1:1], 90k tbr, 90k tbn (attached pic)
      Metadata:
        comment         : Cover (front)
  Stream mapping:
    Stream #0:0 -> #0:0 (mp3 (mp3float) -> pcm_s16le (native))
  Output #0, null, to 'pipe:':
    Stream #0:0: Audio: pcm_s16le, 48000 Hz, mono, s16, 768 kb/s
  ";
  pub(crate) const M4A_PNG: &str = "Input #0, mov,mp4,m4a,3gp,3g2,mj2, from 'in.m4a':
    Duration: 00:00:30.02, start: 0.000000, bitrate: 261 kb/s
    Stream #0:0[0x1](und): Audio: aac (LC) (mp4a / 0x6134706D), 48000 Hz, stereo, fltp, 256 kb/s (default)
    Stream #0:1[0x0]: Video: png, rgb24(pc), 1400x1400, 90k tbr, 90k tbn (attached pic)
  ";
  pub(crate) const FLAC_MONO_24: &str = "Input #0, flac, from 'take.flac':
    Duration: 00:10:00.00, start: 0.000000, bitrate: 1650 kb/s
    Stream #0:0: Audio: flac, 96000 Hz, mono, s32 (24 bit)
  ";
  pub(crate) const WAV_51: &str = "Input #0, wav, from 'mix.wav':
    Duration: N/A, bitrate: 6912 kb/s
    Stream #0:0: Audio: pcm_s24le ([1][0][0][0] / 0x0001), 48000 Hz, 5.1, s32 (24 bit), 6912 kb/s
  ";
  pub(crate) const WAV_STEREO: &str = "Input #0, wav, from 'tone.wav':
    Duration: 00:00:01.00, bitrate: 1411 kb/s
    Stream #0:0: Audio: pcm_s16le ([1][0][0][0] / 0x0001), 44100 Hz, 2 channels, s16, 1411 kb/s
  ";
  pub(crate) const PNG_ONLY: &str = "Input #0, png_pipe, from 'x.png':
    Duration: N/A, bitrate: N/A
    Stream #0:0: Video: png, rgb24(pc), 64x48, 25 tbr, 25 tbn
  ";
  ```
  Tests:
  ```rust
  #[test] fn an_mp3_with_cover_reads_only_its_input_block() {
      let i = parse(samples::MP3_COVER).unwrap();
      assert_eq!((i.codec.as_str(), i.sample_rate, i.channels, i.bits, i.kbps), ("mp3", 44100, 2, None, Some(320)));
      assert!((i.duration_secs.unwrap() - 201.45).abs() < 1e-6);
      assert_eq!(i.cover, Some(CoverInfo { codec: "mjpeg".into(), dims: (3000, 3000) }));
      assert!(!i.lossless());
  }
  #[test] fn an_m4a_stream_line_with_ids_and_default_parses() {
      let i = parse(samples::M4A_PNG).unwrap();
      assert_eq!((i.codec.as_str(), i.sample_rate, i.kbps), ("aac", 48000, Some(256)));
      assert_eq!(i.cover.unwrap().dims, (1400, 1400));
  }
  #[test] fn lossless_sources_report_depth_layout_and_container_bitrate() {
      let f = parse(samples::FLAC_MONO_24).unwrap();
      assert_eq!((f.channels, f.bits, f.kbps, f.lossless(), f.cover.is_none()), (1, Some(24), Some(1650), true, true));
      let w = parse(samples::WAV_51).unwrap();
      assert_eq!((w.codec.as_str(), w.channels, w.bits, w.duration_secs), ("pcm_s24le", 6, Some(24), None));
      assert_eq!(parse(samples::WAV_STEREO).unwrap().channels, 2);
  }
  #[test] fn no_audio_stream_is_no_audio() {
      assert_eq!(parse(samples::PNG_ONLY), Err(ProbeError::NoAudio));
      assert_eq!(parse(""), Err(ProbeError::NoAudio));
  }
  #[test] fn the_probe_names_the_input_and_reads_one_frame() {
      let a: Vec<String> = args(Path::new("/b/my song.wav")).iter().map(|s| s.to_string_lossy().into()).collect();
      assert_eq!(a, ["-i", "/b/my song.wav", "-map", "0:a:0", "-frames:a", "1", "-f", "null", "-"]);
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core audio::probe`. Expected: FAIL.
- [ ] **Step 3: Implement `parse`.**
  - Use only the text before the first line whose trimmed form starts with `Output #`.
  - `Duration: ` gives `HH:MM:SS.ff` (or `N/A`), then `, bitrate: N kb/s` (or `N/A`).
  - The first line containing `: Audio: ` is the audio stream. Split the text after it on commas **at parenthesis depth 0** (`split_top_level`).
    - The first word of part 0 is the codec.
    - The part ending in ` Hz` is the rate.
    - The next part is the layout: `mono`=1, `stereo`=2, `N channels`=N, `2.1`=3, `quad`/`4.0`=4, `5.0`=5, `5.1`/`5.1(side)`=6, `6.1`=7, `7.1`=8, anything else 2.
    - Bits come from `(NN bit)` in any part, else from a `pcm_sNN*` / `pcm_fNN*` codec, else from a part equal to `s16`/`s16p` (16), else `None`.
    - Kbps comes from the part containing ` kb/s`, by taking the number before it. Fall back to the Duration line's bitrate.
  - The first `: Video: ` line containing `(attached pic)` is the cover. Its codec is the first word, and its dims are the first token of the form `<digits>x<digits>`.
  - A rate that does not parse is `Unreadable`. No audio line is `NoAudio`.
- [ ] **Step 4: Run** `cargo test -p ferry-core audio::probe`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): read an audio file's streams from ffmpeg's log`.

---

### Task 4: Loudness targets and the two `loudnorm` passes

**Files:** Create `crates/ferry-core/src/audio/loudness.rs`. Modify `audio/mod.rs`.

**Interfaces:**
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq)] pub struct Spec { pub i: f32, pub tp: f32, pub lra: f32, pub dual_mono: bool }
  pub fn spec(target: Loudness, channels: u16) -> Option<Spec>;   // None for Off; table in Decisions
  pub fn label(spec: &Spec) -> String;                             // "−14 LUFS" (U+2212 minus), via audio.detail.lufs
  pub const MEASURE_SHARE: f32 = 0.3;
  #[derive(Debug, Clone, Copy, PartialEq)] pub struct Measured { pub i: f64, pub tp: f64, pub lra: f64, pub thresh: f64, pub offset: f64 }
  impl Measured { pub fn is_measurable(&self) -> bool; }  // i finite && i > -70 && thresh finite && tp finite
  #[derive(Debug, Clone, PartialEq, Eq)] pub enum LoudnessError { NoReport }
  pub fn measure_args(input: &Path, spec: &Spec) -> Vec<OsString>;
  pub fn parse_measurement(log: &str) -> Result<Measured, LoudnessError>;
  pub fn filter(spec: &Spec, m: &Measured) -> String;      // the pass-2 -af value
  pub fn was_limited(log: &str) -> bool;                    // pass-2 JSON normalization_type == "dynamic"
  #[cfg(test)] pub(crate) mod samples { pub const MEASURED: &str; pub const SILENT: &str; pub const PASS2_LINEAR: &str; pub const PASS2_DYNAMIC: &str; }
  ```

- [ ] **Step 1: Write the samples and failing tests:**
  ```rust
  pub(crate) const MEASURED: &str = "size=N/A time=00:00:30.00 bitrate=N/A speed= 412x
  [Parsed_loudnorm_0 @ 0x600000f0c000] 
  {
  \t\"input_i\" : \"-27.61\",
  \t\"input_tp\" : \"-4.47\",
  \t\"input_lra\" : \"18.06\",
  \t\"input_thresh\" : \"-39.20\",
  \t\"output_i\" : \"-16.58\",
  \t\"output_tp\" : \"-1.50\",
  \t\"output_lra\" : \"14.78\",
  \t\"output_thresh\" : \"-27.71\",
  \t\"normalization_type\" : \"dynamic\",
  \t\"target_offset\" : \"0.58\"
  }
  ";
  // SILENT: the same shape with input_i, input_tp and input_thresh "-inf", input_lra "0.00", target_offset "inf".
  // PASS2_LINEAR / PASS2_DYNAMIC: the same shape with normalization_type "linear" / "dynamic".
  #[test] fn every_target_matches_its_standard() {
      let s = |t| spec(t, 2).unwrap();
      assert_eq!(s(Loudness::Streaming), Spec { i: -14.0, tp: -1.0, lra: 20.0, dual_mono: false });
      assert_eq!(s(Loudness::AppleMusic), Spec { i: -16.0, tp: -1.0, lra: 20.0, dual_mono: false });
      assert_eq!(s(Loudness::Podcast), Spec { i: -16.0, tp: -1.0, lra: 11.0, dual_mono: false });
      assert_eq!(s(Loudness::Broadcast), Spec { i: -23.0, tp: -1.0, lra: 20.0, dual_mono: false });
      assert_eq!(spec(Loudness::Off, 2), None);
      assert!(spec(Loudness::Podcast, 1).unwrap().dual_mono && !spec(Loudness::Broadcast, 1).unwrap().dual_mono);
      assert_eq!(label(&s(Loudness::Streaming)), "−14 LUFS");
  }
  #[test] fn pass_one_measures_the_first_audio_stream() {
      let a: Vec<String> = measure_args(Path::new("/b/x.wav"), &spec(Loudness::Podcast, 1).unwrap())
          .iter().map(|s| s.to_string_lossy().into()).collect();
      assert_eq!(a, ["-i", "/b/x.wav", "-map", "0:a:0", "-af",
          "loudnorm=I=-16.0:TP=-1.0:LRA=11.0:dual_mono=true:print_format=json", "-f", "null", "-"]);
  }
  #[test] fn the_json_after_the_last_loudnorm_line_is_read() {
      let m = parse_measurement(samples::MEASURED).unwrap();
      assert_eq!((m.i, m.tp, m.lra, m.thresh, m.offset), (-27.61, -4.47, 18.06, -39.20, 0.58));
      assert!(m.is_measurable());
      assert_eq!(parse_measurement("no json here"), Err(LoudnessError::NoReport));
  }
  #[test] fn silence_is_not_measurable() {                           // Review Focus 1
      let m = parse_measurement(samples::SILENT).unwrap();
      assert!(m.i.is_infinite() && !m.is_measurable());
  }
  #[test] fn pass_two_feeds_the_measurement_back_in_linear_mode() {
      let m = parse_measurement(samples::MEASURED).unwrap();
      assert_eq!(filter(&spec(Loudness::Streaming, 2).unwrap(), &m),
          "loudnorm=I=-14.0:TP=-1.0:LRA=20.0:measured_I=-27.61:measured_TP=-4.47:measured_LRA=18.06:measured_thresh=-39.20:offset=0.58:linear=true:print_format=json");
      let steady = Measured { lra: 0.0, ..m };
      assert!(filter(&spec(Loudness::Streaming, 2).unwrap(), &steady).contains("measured_LRA=0.01"));
  }
  #[test] fn a_dynamic_second_pass_was_limited() {
      assert!(was_limited(samples::PASS2_DYNAMIC) && !was_limited(samples::PASS2_LINEAR) && !was_limited(""));
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core audio::loudness`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Spec numbers print with `{:.1}` and measured numbers with `{:.2}`. Both are locale-independent.
  - `dual_mono=true` is written only when `spec.dual_mono`, which is `channels == 1` and the target is not Broadcast. It goes after `LRA` in pass 1, and after `linear=true` in pass 2.
  - Parsing: take the text after the last `[Parsed_loudnorm_`, find the first `{` and its following `}`, and deserialise it into a private struct of `String` fields with `serde_json`. Convert each value with `"-inf" → NEG_INFINITY`, `"inf" → INFINITY`, else `parse::<f64>()`. A parse failure is `NoReport`.
- [ ] **Step 4: Run** `cargo test -p ferry-core audio::loudness`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): loudness targets and two-pass loudnorm`.

---

### Task 5: The `[audio]` settings section

**Files:** Create `crates/ferry-core/src/audio/settings.rs`. Modify `crates/ferry-core/src/settings.rs` (anchors in the shared-file table), `audio/mod.rs`.

**Interfaces:**
- Consumes: `settings::{Output, OutputRule, Location}`, `edit::Size`, `audio::{Loudness, CoverPolicy, TARGETS}`, `template::validate`.
- Produces:
  ```rust
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(default)]
  pub struct Audio { pub outputs: Outputs, pub bitrate_kbps: u16, pub opus_bitrate_kbps: u16, pub loudness: Loudness,
      pub cover: CoverPolicy, pub cover_size: CoverSize, pub ignored_extensions: Vec<String>, pub max_size_mb: u32,
      pub output: Output, #[serde(flatten)] pub extra: Map<String, Value> }
  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)] #[serde(default)]
  pub struct Outputs { pub wav: Format, pub aiff: Format, pub flac: Format, pub mp3: Format, pub m4a: Format, pub ogg: Format, pub opus: Format,
      #[serde(flatten)] pub extra: Map<String, Value> }
  impl Outputs { pub fn for_source(&self, source: Format) -> Format; }  // image formats return themselves
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")] pub enum CoverShape { Square, LongEdge }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(default)] pub struct CoverSize { pub shape: CoverShape, pub side: u32 }
  impl CoverSize { pub fn as_size(self) -> Size; }   // Square → Exact{side, side}; LongEdge → LongEdge{edge: side}
  impl Audio { pub fn sanitise(&mut self); }
  // settings.rs: Settings gains `pub audio: crate::audio::settings::Audio`
  ```

- [ ] **Step 1: Write the failing tests** in `audio/settings.rs`:
  ```rust
  #[test] fn defaults_match_the_plan() {
      let a = Audio::default();
      let o = &a.outputs;
      assert_eq!([o.wav, o.aiff, o.flac, o.mp3, o.m4a, o.ogg, o.opus],
          [Format::Mp3, Format::M4a, Format::M4a, Format::Mp3, Format::M4a, Format::Opus, Format::Opus]);
      assert_eq!((a.bitrate_kbps, a.opus_bitrate_kbps, a.loudness, a.cover), (192, 128, Loudness::Off, CoverPolicy::Optimise));
      assert_eq!(a.cover_size, CoverSize { shape: CoverShape::Square, side: 1000 });
      assert_eq!((a.max_size_mb, a.ignored_extensions.len()), (2000, 0));
      assert_eq!((a.output.optimised.folder.as_str(), a.output.optimised.template.as_str()), ("~/Music/Ferry", "%f_optimised.%e"));
      assert_eq!((a.output.converted.location, a.output.converted.template.as_str()), (Location::InPlace, "%f.%e"));
      assert_eq!(Settings::default().version, 2);
  }
  #[test] fn a_file_without_an_audio_section_gets_the_defaults_and_keeps_unknown_fields() {
      let dir = tempfile::tempdir().unwrap(); let path = dir.path().join("settings.json");
      std::fs::write(&path, r#"{"images":{"compression":60},"audio":{"bitrate_kbps":160,"future":1}}"#).unwrap();
      let (s, _) = Settings::load(&path);
      assert_eq!((s.audio.bitrate_kbps, s.audio.opus_bitrate_kbps), (160, 128));
      s.save(&path).unwrap();
      let saved: serde_json::Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
      assert_eq!(saved["audio"]["future"], 1);
      assert_eq!(saved["audio"]["outputs"]["wav"], "mp3");
  }
  #[test] fn nonsense_is_repaired() {
      let mut a = Audio { bitrate_kbps: 5, opus_bitrate_kbps: 999, max_size_mb: 0, ..Audio::default() };
      a.outputs.wav = Format::Png; a.outputs.flac = Format::Aiff;          // not output targets
      a.cover_size.side = 9;
      a.ignored_extensions = vec![" aif ".into(), "".into()];
      a.output.optimised.template = "%q".into();
      a.sanitise();
      assert_eq!((a.bitrate_kbps, a.opus_bitrate_kbps, a.max_size_mb, a.cover_size.side), (32, 256, 1, 100));
      assert_eq!((a.outputs.wav, a.outputs.flac), (Format::Mp3, Format::M4a));
      assert_eq!(a.ignored_extensions, ["aif"]);
      assert_eq!(a.output.optimised.template, "%f_optimised.%e");
  }
  #[test] fn the_cover_size_becomes_an_edit_size() {
      assert_eq!(CoverSize { shape: CoverShape::Square, side: 600 }.as_size(), Size::Exact { width: 600, height: 600 });
      assert_eq!(CoverSize { shape: CoverShape::LongEdge, side: 800 }.as_size(), Size::LongEdge { edge: 800 });
      assert_eq!(Outputs::default().for_source(Format::Ogg), Format::Opus);
      assert_eq!(Outputs::default().for_source(Format::Png), Format::Png);
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core audio::settings`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `sanitise`: clamps `bitrate_kbps` to 32..=320, `opus_bitrate_kbps` to 16..=256, `cover_size.side` to 100..=3000 and `max_size_mb` to ≥ 1.
  - It resets any output not in `TARGETS` to its default.
  - It trims the extensions list and drops empty entries.
  - It repairs `output.*` rules exactly as `Settings::sanitised` repairs `images.output`: same template check, blank folder → the default `~/Music/Ferry`.
  - Then wire `settings.rs` at the anchors, with `self.audio.sanitise();` in `sanitised`.
- [ ] **Step 4: Run** `cargo test -p ferry-core audio::settings`, then `cargo test -p ferry-core settings`. Expected: PASS. The existing settings tests are unchanged.
- [ ] **Step 5: Commit** `feat(core): audio settings section`.

---

### Task 6: The recipe learns audio (`Edits`, `EditsPatch`, `JobUpdate.audio`)

**Files:** Modify `crates/ferry-core/src/edit/mod.rs` (fields under `// ---- stream B: audio ----`), `crates/ferry-core/src/job.rs` (field `audio`), `crates/ferry-core/src/pipeline.rs` (`notice()` sets `audio: None`).

**Interfaces:**
- Produces:
  ```rust
  // Edits (serde default, so old JSON still loads)
  pub bitrate: Option<u16>,              // kbps, lossy audio results only
  pub loudness: Option<Loudness>,        // None: the settings' target
  pub cover: Option<CoverPolicy>,        // None: the settings' policy; size/scale downscale the cover
  // EditsPatch: the same three as Option<Option<T>> with #[serde(deserialize_with = "present")]
  // JobUpdate
  #[serde(default)] pub audio: Option<crate::audio::AudioCard>,   // Some on audio jobs only
  ```
  Merge rules, added to `Edits::merge`:
  - `bitrate` is clamped to 16..=320, and setting it clears `fit_under`. Setting `fit_under` also clears `bitrate`.
  - Setting `cover` to any value clears `size` and `scale`.
  - Setting `size` or `scale` while `cover == Some(Remove)` resets `cover` to `None`, because downscaling a cover brings it back.
  - `is_plain()` also requires `bitrate`, `loudness` and `cover` to be `None`.

- [ ] **Step 1: Write the failing tests** in `edit/mod.rs`:
  ```rust
  #[test] fn audio_fields_merge_and_exclude_each_other() {
      let mut e = Edits::default();
      e.merge(patch(json!({ "bitrate": 500 })));
      assert_eq!(e.bitrate, Some(320));
      e.merge(patch(json!({ "fit_under": 1_000_000 })));
      assert_eq!((e.bitrate, e.fit_under), (None, Some(1_000_000)));
      e.merge(patch(json!({ "bitrate": 128 })));
      assert_eq!((e.bitrate, e.fit_under), (Some(128), None));
      e.merge(patch(json!({ "scale": 50 })));
      e.merge(patch(json!({ "cover": "remove" })));
      assert_eq!((e.cover, e.scale), (Some(CoverPolicy::Remove), None));
      e.merge(patch(json!({ "size": { "kind": "exact", "width": 600, "height": 600 } })));
      assert_eq!(e.cover, None);
      e.merge(patch(json!({ "loudness": "podcast" })));
      assert_eq!(e.loudness, Some(Loudness::Podcast));
      assert!(!e.is_plain());
      assert!(!Edits { loudness: Some(Loudness::Off), ..Edits::default() }.is_plain());
  }
  ```
  Update `the_card_sees_the_recipe_as_plain_json` to expect `"bitrate": null, "loudness": null, "cover": null` as well. In `job.rs`, add `audio: None` to the round-trip test literal. Add a second round trip with `audio: Some(AudioCard { bitrate_before: Some(320), bitrate_after: Some(192), lossless: false, has_cover: true })`, and assert the JSON contains `"has_cover":true`.
- [ ] **Step 2: Run** `cargo test -p ferry-core edit::tests`. Expected: FAIL.
- [ ] **Step 3: Implement** the fields, the patch fields and the merge rules above. Add `audio: None` in `pipeline::notice` and in `Job::update`; Task 10 fills `update`.
- [ ] **Step 4: Run** `cargo test -p ferry-core edit`, then `cargo test -p ferry-core job`, then `cargo test -p ferry-core pipeline`. Expected: PASS. Image behaviour is unchanged.
- [ ] **Step 5: Commit** `feat(core): the card recipe carries bitrate, loudness and cover`.

---

### Task 7: Resolving an audio run and its ffmpeg arguments

**Files:** Create `crates/ferry-core/src/audio/plan.rs`. Modify `audio/mod.rs`.

**Interfaces:**
- Consumes: `AudioInfo` (Task 3), `audio::settings::Audio` (Task 5), `Edits`/`Level` (Task 6), `audio::tool::AacEncoder` (Task 1).
- Produces:
  ```rust
  pub const BITRATE_STOPS: [u16; 7] = [320, 256, 192, 160, 128, 96, 64];
  pub const MP3_KBPS: [u16; 14] = [32, 40, 48, 56, 64, 80, 96, 112, 128, 160, 192, 224, 256, 320];
  pub struct Inputs<'a> { pub info: &'a AudioInfo, pub source: Format, pub edits: &'a Edits, pub level: Level, pub settings: &'a Audio }
  #[derive(Debug, Clone, PartialEq)]
  pub struct AudioPlan { pub target: Format, pub kbps: Option<u16>, pub copy_audio: bool, pub sample_rate: u32,
      pub downmix: bool, pub pcm_bits: u8, pub loudness: Option<Loudness>, pub cover: CoverPlan,
      pub explicit: bool, pub fit_under: Option<u64>,
      pub aac: AacEncoder }                                   // which AAC encoder an M4A target uses; ignored for other targets
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum DropReason { Container, Unreadable }
  #[derive(Debug, Clone, Copy, PartialEq, Eq)]
  pub enum CoverPlan { Absent, Copy, Optimise, Resize(Size), Scale(u8), Remove, Drop(DropReason) }
  impl AudioPlan { pub fn nothing_to_do(&self, source: Format) -> bool; }
  pub fn resolve(i: &Inputs) -> AudioPlan;                        // == resolve_with(i, AacEncoder::Native)
  pub fn resolve_with(i: &Inputs, aac: AacEncoder) -> AudioPlan;  // Task 11 passes o.audio.aac
  pub fn encoder_kbps(target: Format, aac: AacEncoder, kbps: u16) -> u16;  // M4a + AudioToolbox: snap down to MP3_KBPS; else unchanged
  pub fn target_for(source: Format, edits: &Edits, settings: &Audio) -> Format;  // edits.format if in TARGETS, else settings.outputs.for_source
  pub fn kbps(target: Format, requested: u16, level: Level, info: &AudioInfo, rate: u32) -> u16;
  pub fn fit_kbps(limit: u64, duration_secs: f64, cover_bytes: u64) -> u16;
  pub fn retry_kbps(target: Format, current: u16, limit: u64, size: u64) -> Option<u16>;  // None when already at the codec minimum
  pub fn output_rate(target: Format, source_rate: u32) -> u32;
  pub enum CoverInput<'a> { None, Source, File(&'a Path) }
  pub struct EncodeArgs<'a> { pub input: &'a Path, pub cover: CoverInput<'a>, pub plan: &'a AudioPlan, pub filter: Option<&'a str>, pub output: &'a Path }
  pub fn encode_args(a: &EncodeArgs) -> Vec<OsString>;
  pub fn muxer(target: Format) -> &'static str;          // mp3 ipod opus wav flac
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Grade { Smaller, Same, Larger }
  pub fn grade(before: u64, after: u64) -> Grade;        // same 1 % rule as optimise::classify
  ```
  Resolution rules (all from Decisions):
  - **`kbps`:** `None` for `Wav`/`Flac`. Otherwise: requested is `edits.bitrate`, else `settings.opus_bitrate_kbps` for Opus and `settings.bitrate_kbps` for others. With `fit_under` set and a known duration, it is `fit_kbps(limit, duration, cover bytes estimated as 0)`. The result then goes through `plan::kbps`.
  - **`copy_audio`:** see Decisions. It needs `fit_under.is_none()`, `loudness.is_none()`, the target equal to the source, a lossy source, a codec matching the container (`Mp3`↔`mp3`, `M4a`↔`aac`, `Opus`↔`opus`), and `kbps × 100 ≥ source kbps × 95`. With copy, `kbps` is the source's.
  - **`aac`:** the `aac` argument of `resolve_with`, stored as is. Then, only when **not** `copy_audio`, `kbps = kbps.map(|k| encoder_kbps(target, aac, k))`. The copy check above uses the value before this snap.
  - **`pcm_bits`:** 24 when `info.bits > 16`, else 16. **`downmix`:** `target == Mp3 && channels > 2`.
  - **`loudness`:** `edits.loudness.unwrap_or(settings.loudness)`, then `None` when `Off`. **`explicit`:** `!edits.is_plain() || loudness.is_some()`.
  - **`cover`:**
    - `Absent` without a source cover.
    - `Remove` when `edits.cover == Some(Remove)`, or when the policy resolves to `Remove`.
    - `Drop(Container)` when `!supports_cover(target)`.
    - `Drop(Unreadable)` when the cover codec is not `mjpeg`/`png`.
    - `Resize(size)` when `edits.size` is set.
    - `Scale(p)` when `edits.scale` is set.
    - Otherwise by policy: `Keep → Copy`, `Optimise → Optimise`, `Downscale → Resize(settings.cover_size.as_size())`.
  - **`nothing_to_do`:** `copy_audio && matches!(cover, Absent | Copy)`, or `source == Wav && target == Wav && loudness.is_none() && fit_under.is_none()`.

- [ ] **Step 1: Write the failing tests** (`info()` is a test helper building `AudioInfo` from `probe::samples`):
  ```rust
  #[test] fn the_bitrate_never_rises_above_a_lossy_source() {        // Review Focus 3
      let mp3_128 = AudioInfo { kbps: Some(128), ..parse(samples::MP3_COVER).unwrap() };
      let lossless = parse(samples::WAV_STEREO).unwrap();
      assert_eq!(kbps(Format::Mp3, 192, Level::Normal, &lossless, 44100), 192);
      assert_eq!(kbps(Format::Mp3, 192, Level::Aggressive, &lossless, 44100), 128);
      assert_eq!(kbps(Format::Mp3, 64, Level::Aggressive, &lossless, 44100), 64);
      assert_eq!(kbps(Format::Mp3, 192, Level::Normal, &mp3_128, 44100), 128);
      assert_eq!(kbps(Format::M4a, 192, Level::Normal, &mp3_128, 44100), 128);
      assert_eq!(kbps(Format::Mp3, 150, Level::Normal, &lossless, 44100), 128);   // snapped to a Layer III rate
      assert_eq!(kbps(Format::M4a, 150, Level::Normal, &lossless, 44100), 150);
      assert_eq!(kbps(Format::Opus, 300, Level::Normal, &lossless, 48000), 256);
      assert_eq!(kbps(Format::Mp3, 20, Level::Normal, &lossless, 44100), 32);
  }
  #[test] fn sample_rates_suit_each_encoder() {
      assert_eq!([output_rate(Format::Opus, 44100), output_rate(Format::Mp3, 44100), output_rate(Format::Mp3, 96000),
                  output_rate(Format::Mp3, 88200), output_rate(Format::Mp3, 22050), output_rate(Format::Mp3, 37800),
                  output_rate(Format::M4a, 96000), output_rate(Format::M4a, 8000), output_rate(Format::Wav, 96000),
                  output_rate(Format::Flac, 192000)],
                 [48000, 44100, 48000, 44100, 22050, 44100, 48000, 8000, 96000, 192000]);
  }
  #[test] fn a_plain_wav_becomes_a_192_kbps_mp3() {
      let p = resolve(&Inputs { info: &parse(samples::WAV_STEREO).unwrap(), source: Format::Wav,
          edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() });
      assert_eq!((p.target, p.kbps, p.copy_audio, p.sample_rate, p.cover, p.explicit), (Format::Mp3, Some(192), false, 44100, CoverPlan::Absent, false));
  }
  #[test] fn a_low_bitrate_mp3_has_nothing_to_do() {
      let info = AudioInfo { kbps: Some(128), cover: None, ..parse(samples::MP3_COVER).unwrap() };
      let p = resolve(&Inputs { info: &info, source: Format::Mp3, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() });
      assert!(p.copy_audio && p.nothing_to_do(Format::Mp3));
      let keep_cover = Audio { cover: CoverPolicy::Keep, ..Audio::default() };
      let with_cover = AudioInfo { kbps: Some(128), ..parse(samples::MP3_COVER).unwrap() };
      let q = resolve(&Inputs { info: &with_cover, source: Format::Mp3, edits: &Edits::default(), level: Level::Normal, settings: &keep_cover });
      assert!(q.nothing_to_do(Format::Mp3));
      let r = resolve(&Inputs { info: &with_cover, source: Format::Mp3, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() });
      assert!(r.copy_audio && r.cover == CoverPlan::Optimise && !r.nothing_to_do(Format::Mp3));  // remux with a smaller cover
  }
  #[test] fn cover_plans_follow_the_recipe_then_the_settings() {
      let info = parse(samples::MP3_COVER).unwrap();
      let plan = |edits: Edits, target| resolve(&Inputs { info: &info, source: Format::Mp3,
          edits: &Edits { format: Some(target), ..edits }, level: Level::Normal, settings: &Audio::default() }).cover;
      assert_eq!(plan(Edits::default(), Format::Mp3), CoverPlan::Optimise);
      assert_eq!(plan(Edits { scale: Some(50), ..Edits::default() }, Format::Mp3), CoverPlan::Scale(50));
      assert_eq!(plan(Edits { cover: Some(CoverPolicy::Remove), ..Edits::default() }, Format::M4a), CoverPlan::Remove);
      assert_eq!(plan(Edits::default(), Format::Opus), CoverPlan::Drop(DropReason::Container));
      let bmp = AudioInfo { cover: Some(CoverInfo { codec: "bmp".into(), dims: (10, 10) }), ..info.clone() };
      assert_eq!(resolve(&Inputs { info: &bmp, source: Format::Mp3, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() }).cover,
          CoverPlan::Drop(DropReason::Unreadable));
  }
  #[test] fn fit_under_size_picks_a_bitrate_from_the_duration() {
      assert_eq!(fit_kbps(1_000_000, 60.0, 0), 130);
      assert_eq!(retry_kbps(Format::Mp3, 128, 1_000_000, 1_100_000), Some(112));
      assert_eq!(retry_kbps(Format::Mp3, 32, 1_000_000, 1_100_000), None);
  }
  #[test] fn encode_arguments_for_the_common_cases() {
      let s = |v: Vec<OsString>| v.iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
      let wav = resolve(&Inputs { info: &parse(samples::WAV_STEREO).unwrap(), source: Format::Wav, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() });
      assert_eq!(s(encode_args(&EncodeArgs { input: Path::new("/b/in.wav"), cover: CoverInput::None, plan: &wav, filter: None, output: Path::new("/w/out.mp3") })),
          "-i /b/in.wav -map 0:a:0 -map_metadata 0 -c:a libmp3lame -b:a 192k -ar 44100 -id3v2_version 3 -f mp3 /w/out.mp3");
      let flac = resolve(&Inputs { info: &AudioInfo { cover: Some(CoverInfo { codec: "mjpeg".into(), dims: (1200, 1200) }), ..parse(samples::FLAC_MONO_24).unwrap() },
          source: Format::Flac, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() });
      assert_eq!(s(encode_args(&EncodeArgs { input: Path::new("/b/in.flac"), cover: CoverInput::File(Path::new("/w/cover.jpg")), plan: &flac, filter: None, output: Path::new("/w/out.m4a") })),
          "-i /b/in.flac -i /w/cover.jpg -map 0:a:0 -map 1:v:0 -map_metadata 0 -c:a aac -b:a 192k -ar 48000 -c:v copy -disposition:v:0 attached_pic -f ipod /w/out.m4a");
      let loud = AudioPlan { target: Format::Wav, kbps: None, pcm_bits: 24, sample_rate: 48000, ..wav.clone() };
      assert_eq!(s(encode_args(&EncodeArgs { input: Path::new("/b/in.wav"), cover: CoverInput::None, plan: &loud, filter: Some("loudnorm=I=-23.0"), output: Path::new("/w/out.wav") })),
          "-i /b/in.wav -map 0:a:0 -map_metadata 0 -af loudnorm=I=-23.0 -c:a pcm_s24le -ar 48000 -rf64 auto -f wav /w/out.wav");
      let copy = AudioPlan { copy_audio: true, target: Format::Mp3, ..wav.clone() };
      assert_eq!(s(encode_args(&EncodeArgs { input: Path::new("/b/a.mp3"), cover: CoverInput::Source, plan: &copy, filter: None, output: Path::new("/w/out.mp3") })),
          "-i /b/a.mp3 -map 0:a:0 -map 0:v:0 -map_metadata 0 -c:a copy -c:v copy -disposition:v:0 attached_pic -id3v2_version 3 -f mp3 /w/out.mp3");
  }
  #[test] fn every_path_argument_is_absolute() {                    // Review Focus 5
      let wav = resolve(&Inputs { info: &parse(samples::WAV_STEREO).unwrap(), source: Format::Wav, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() });
      let a = encode_args(&EncodeArgs { input: Path::new("/b/-intro ü.wav"), cover: CoverInput::None, plan: &wav, filter: None, output: Path::new("/w/out.mp3") });
      let after_i = a.iter().position(|x| x == "-i").unwrap() + 1;
      assert_eq!(a[after_i], OsString::from("/b/-intro ü.wav"));
      assert!(Path::new(a.last().unwrap()).is_absolute());
  }
  #[test] fn grading_uses_the_one_percent_rule() {
      assert_eq!([grade(1000, 990), grade(1000, 991), grade(1000, 1001)], [Grade::Smaller, Grade::Same, Grade::Larger]);
  }
  #[test] fn m4a_uses_audiotoolbox_when_offered_and_native_aac_otherwise() {   // Review Focus 6
      let s = |v: Vec<OsString>| v.iter().map(|a| a.to_string_lossy().into_owned()).collect::<Vec<_>>().join(" ");
      let flac = parse(samples::FLAC_MONO_24).unwrap();                    // 96 kHz mono, no cover; FLAC defaults to M4A
      let inputs = Inputs { info: &flac, source: Format::Flac, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() };
      let args = |p: &AudioPlan| s(encode_args(&EncodeArgs { input: Path::new("/b/in.flac"), cover: CoverInput::None, plan: p, filter: None, output: Path::new("/w/out.m4a") }));
      let native = resolve_with(&inputs, AacEncoder::Native);
      assert_eq!(native, resolve(&inputs));
      assert_eq!(args(&native), "-i /b/in.flac -map 0:a:0 -map_metadata 0 -c:a aac -b:a 192k -ar 48000 -f ipod /w/out.m4a");
      let at = resolve_with(&inputs, AacEncoder::AudioToolbox);
      assert_eq!(at.aac, AacEncoder::AudioToolbox);
      assert_eq!(args(&at), "-i /b/in.flac -map 0:a:0 -map_metadata 0 -c:a aac_at -b:a 192k -aac_at_mode cvbr -ar 48000 -f ipod /w/out.m4a");
      let fit = AudioPlan { fit_under: Some(1_000_000), ..at.clone() };
      assert!(args(&fit).contains(" -c:a aac_at -b:a 192k -aac_at_mode cbr -ar 48000 "));
      for p in [&at, &fit] { let a = args(p); assert!(!a.contains("-q:a") && !a.contains("global_quality") && !a.contains("aac_at_quality") && !a.contains("mode vbr")); }
      // Other targets ignore the AAC encoder.
      let mp3 = resolve_with(&Inputs { edits: &Edits { format: Some(Format::Mp3), ..Edits::default() }, ..inputs }, AacEncoder::AudioToolbox);
      assert!(args(&mp3).contains("-c:a libmp3lame") && !args(&mp3).contains("aac"));
  }
  #[test] fn audiotoolbox_bitrates_snap_down_never_up() {             // Review Focus 6
      assert_eq!(encoder_kbps(Format::M4a, AacEncoder::AudioToolbox, 150), 128);
      assert_eq!(encoder_kbps(Format::M4a, AacEncoder::AudioToolbox, 192), 192);
      assert_eq!(encoder_kbps(Format::M4a, AacEncoder::AudioToolbox, 33), 32);
      assert_eq!(encoder_kbps(Format::M4a, AacEncoder::Native, 150), 150);
      assert_eq!(encoder_kbps(Format::Opus, AacEncoder::AudioToolbox, 150), 150);   // only M4A is affected
      let wav = parse(samples::WAV_STEREO).unwrap();
      let edits = Edits { format: Some(Format::M4a), bitrate: Some(150), ..Edits::default() };
      let inputs = Inputs { info: &wav, source: Format::Wav, edits: &edits, level: Level::Normal, settings: &Audio::default() };
      assert_eq!(resolve_with(&inputs, AacEncoder::AudioToolbox).kbps, Some(128));
      assert_eq!(resolve_with(&inputs, AacEncoder::Native).kbps, Some(150));
      // The copy check sees the unsnapped bitrate: a 150 kbps AAC source is still copied, not re-encoded at 128.
      let aac_150 = AudioInfo { kbps: Some(150), cover: None, ..parse(samples::M4A_PNG).unwrap() };
      let copy = resolve_with(&Inputs { info: &aac_150, source: Format::M4a, edits: &Edits::default(), level: Level::Normal, settings: &Audio::default() }, AacEncoder::AudioToolbox);
      assert!(copy.copy_audio && copy.kbps == Some(150) && copy.nothing_to_do(Format::M4a));
  }
  ```
  `every_path_argument_is_absolute` uses Unix paths. Run it under `#[cfg(unix)]`, and add a `#[cfg(windows)]` twin with `C:\b\-intro ü.wav`.
- [ ] **Step 2: Run** `cargo test -p ferry-core audio::plan`. Expected: FAIL.
- [ ] **Step 3: Implement** `resolve` and the helpers above.
  - Argument order is the one the expected strings show. The optional pieces, in order:
    - `-i <cover file>` after the main input;
    - `-map 0:v:0` (Source) or `-map 1:v:0` (File) after `-map 0:a:0`;
    - `-af <filter>` before `-c:a`;
    - `-ac 2` after `-ar` when `downmix`;
    - `-c:v copy -disposition:v:0 attached_pic` when a cover is mapped;
    - the container options;
    - `-f <muxer> <output>` last.
  - `-ar` and `-b:a` are omitted with `copy_audio`. `-b:a` is omitted for lossless targets.
  - Codec arguments: `libmp3lame`; M4A `plan.aac.codec()`, so `aac` or `aac_at`; `libopus … -vbr on` (`-vbr on` right after `-b:a`); `pcm_s16le`/`pcm_s24le`; `flac -compression_level 8`.
  - `aac_at` only: `-aac_at_mode cvbr` right after `-b:a`, or `-aac_at_mode cbr` when `plan.fit_under.is_some()`. With `copy_audio` there is no `-c:a aac_at` and no mode (the audio is copied). Never emit `-q:a`, `-global_quality` or `-aac_at_quality`.
  - `encoder_kbps`: for `(Format::M4a, AacEncoder::AudioToolbox)`, the largest `MP3_KBPS` value ≤ `kbps` (32 when `kbps` < 32); otherwise `kbps` unchanged. `resolve(i)` is `resolve_with(i, AacEncoder::Native)`.
  - Add `debug_assert!(input.is_absolute() && output.is_absolute())` in `encode_args`.
- [ ] **Step 4: Run** `cargo test -p ferry-core audio::plan`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): plan an audio run and its encoder arguments`.

---

### Task 8: Cover art: extraction and processing through the image engine

**Files:** Create `crates/ferry-core/src/audio/cover.rs`. Modify `audio/mod.rs`.

**Interfaces:**
- Consumes: `optimise::{optimise, OptimiseOptions, Outcome}`, `edit::{apply, Edits, Framing, Size}`, `edit::raster::display_dims`, `sniff::sniff`, `CoverPlan` (Task 7).
- Produces:
  ```rust
  pub fn file_ext(codec: &str) -> Option<&'static str>;           // "mjpeg" → "jpg", "png" → "png", else None
  pub fn extract_args(input: &Path, out: &Path) -> Vec<OsString>; // -i <in> -map 0:v:0 -c copy -frames:v 1 -f image2 <out>
  #[derive(Debug)] pub struct Processed { pub bytes: Vec<u8>, pub ext: &'static str, pub dims: (u32, u32) }
  pub fn process(bytes: &[u8], plan: CoverPlan, o: &OptimiseOptions) -> Result<Processed, OptimiseError>;
  ```
  `process`:
  - `Optimise` keeps the cover's own format. It uses the optimiser's bytes only when `Outcome::Smaller`, else the input.
  - `Resize(size)` and `Scale(p)` run `edit::apply` with `Edits { size / scale, framing: Centre, format: Some(Jpeg), compression: Some(85), ..Default }` and give a JPEG (`ext "jpg"`).
  - Any other plan, or input that is not PNG/JPEG, is `Err(OptimiseError::Unsupported)`.

- [ ] **Step 1: Write the failing tests:**
  ```rust
  #[test] fn optimise_keeps_the_size_and_format_and_shrinks() {
      let src = testutil::jpeg(1200, 1200, 95, &Default::default());
      let p = process(&src, CoverPlan::Optimise, &OptimiseOptions::default()).unwrap();
      assert_eq!((p.ext, p.dims), ("jpg", (1200, 1200)));
      assert!(p.bytes.len() < src.len());
  }
  #[test] fn downscale_squares_or_scales_and_never_enlarges() {
      let o = OptimiseOptions::default();
      let sq = process(&testutil::jpeg(1600, 1200, 90, &Default::default()), CoverPlan::Resize(Size::Exact { width: 600, height: 600 }), &o).unwrap();
      assert_eq!((sq.ext, sq.dims), ("jpg", (600, 600)));
      let half = process(&testutil::png(1200, 800), CoverPlan::Scale(50), &o).unwrap();
      assert_eq!((half.ext, half.dims, sniff(&half.bytes)), ("jpg", (600, 400), Some(Format::Jpeg)));
      let big = process(&testutil::png(500, 500), CoverPlan::Resize(Size::LongEdge { edge: 3000 }), &o).unwrap();
      assert_eq!(big.dims, (500, 500));
  }
  #[test] fn unreadable_covers_are_an_error() {
      assert!(process(b"BM not a jpeg", CoverPlan::Optimise, &OptimiseOptions::default()).is_err());
      assert_eq!((file_ext("mjpeg"), file_ext("png"), file_ext("bmp")), (Some("jpg"), Some("png"), None));
  }
  #[test] fn extraction_copies_the_first_picture() {
      let a: Vec<String> = extract_args(Path::new("/b/a.mp3"), Path::new("/w/cover-original.jpg")).iter().map(|s| s.to_string_lossy().into()).collect();
      assert_eq!(a, ["-i", "/b/a.mp3", "-map", "0:v:0", "-c", "copy", "-frames:v", "1", "-f", "image2", "/w/cover-original.jpg"]);
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core audio::cover`. Expected: FAIL.
- [ ] **Step 3: Implement** as specified. `process` takes the caller's options and overrides only `compression` (85 for Resize/Scale) and `level` (Normal).
- [ ] **Step 4: Run** `cargo test -p ferry-core audio::cover`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(core): optimise and downscale cover art with the image engine`.

---

### Task 9: Results and originals written from a file (`Content`, same as A's Task 9)

**Skip condition:** this task is identical to stream A's Task 9 (same names: `Content`, `backup_path`, `atomic_replace_from`, `replace_with`). If A's Task 9 is already merged (`grep -n "enum Content\|atomic_replace_from" crates/ferry-core/src/pipeline.rs crates/ferry-core/src/fsutil.rs`), skip this task and only add the test below if it is missing. If it is not merged, do it exactly as specified here, so A's merge becomes a no-op.

**Files:** Modify `crates/ferry-core/src/fsutil.rs`, `crates/ferry-core/src/pipeline.rs`.

**Interfaces:**
- Produces:
  ```rust
  // fsutil.rs
  pub fn atomic_replace_from(target: &Path, from: &Path) -> io::Result<()>;   // same contract as atomic_replace, streaming by fs::copy
  // pipeline.rs
  #[derive(Clone, Copy)] enum Content<'a> { Bytes(&'a [u8]), File(&'a Path) }   // private; pipeline/audio.rs is a child module and sees it
  impl Job { fn backup_path(&mut self, backups: &Backups) -> io::Result<PathBuf>; }  // today's `original()` without the read
  ```
  `put(result: Content, format, original: Content, o, deliver)`, `put_back(original: Content, …)`, `restore_source(original: Content, o)` and `write_file(path, content: Content, source, o)` take `Content` where they take `&[u8]` today. `write_file` uses `atomic_replace` for `Bytes` and `atomic_replace_from` for `File`. `original()` becomes `fs::read(self.backup_path(backups)?)`. The image paths pass `Content::Bytes`. The Clipboard branches of `put` and `put_back` need `Bytes`; given a `File`, they return `Err((t("error.unexpected"), "a clipboard result must be in memory".into()))`. `restore()` is unchanged; audio jobs get their own restore in Task 11.

- [ ] **Step 1: Write the failing tests.** In `fsutil.rs`, add A's four tests: `replacing_from_a_file_copies_it_and_keeps_the_source_file`, `replacing_from_a_file_keeps_the_targets_permissions` (unix: 0o600 stays 0o600), `replacing_from_a_missing_file_leaves_the_target_and_no_temp` and `replacing_from_a_file_refuses_a_read_only_target`. In `pipeline.rs` tests, add:
  ```rust
  #[test] fn a_file_content_is_copied_not_read_into_memory() {       // Review Focus 5
      let env = Env::new();
      let src = env.file("big.bin", &vec![7u8; 3_000_000]);
      let target = env.file("t.bin", b"old");
      let job = Job::file(&env.file("a.png", &testutil::png(8, 8)), &Settings::default(), true).unwrap();
      job.write_file(&target, Content::File(&src), &target, &options()).unwrap();
      assert_eq!(fs::read(&target).unwrap().len(), 3_000_000);
      assert!(!target.with_file_name("t.bin.ferry-tmp").exists());
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core fsutil`. Expected: FAIL.
- [ ] **Step 3: Implement.** Refactor `atomic_replace` so that both functions share one private `replace_with(target, write: impl FnOnce(&Path) -> io::Result<()>)`: symlink follow, read-only refusal, temp path, permission copy, rename, cleanup. `atomic_replace_from` writes with `fs::copy(from, tmp)` and then `File::open(tmp)?.sync_all()`. **Merge note for stream C:** C's `selfwrite::record` call belongs in `replace_with`, so both functions record. Then thread `Content` through the four `Job` methods.
- [ ] **Step 4: Run** `cargo test -p ferry-core fsutil`, then `cargo test -p ferry-core pipeline`. Expected: PASS. All existing pipeline tests pass unchanged.
- [ ] **Step 5: Commit** `refactor(core): write results and originals from a file as well as bytes`.

---

### Task 10: Audio jobs: creation, card fields, detail line, format guard

**Files:** Create `crates/ferry-core/src/pipeline/audio.rs` (declared with `mod audio;` and `pub use audio::AudioRunOptions;` in `pipeline.rs` under `// ---- stream B: audio ----`). Modify `crates/ferry-core/src/pipeline.rs` at these anchors:
- the `Job` field;
- `RunOptions` field and `new`;
- `Job::file` dispatch;
- `Job::clipboard`/`Job::file`/`fork` field init;
- `update()`: `thumb_path`, `audio`;
- `detail()`;
- `edit()`.

**Interfaces:**
- Consumes: `sniff::HEAD`, `Format::is_audio`, `audio::{AudioCard, TARGETS, is_lossless_format}`, `audio::tool::{installed, aac_encoder, AacEncoder, AudioTool}`, `audio::settings::Audio`, `probe::AudioInfo`.
- Produces:
  ```rust
  // pipeline/audio.rs
  #[derive(Debug, Clone)] pub struct AudioRunOptions { pub settings: Audio, pub tool: Option<Arc<dyn AudioTool>>, pub aac: AacEncoder }
  #[derive(Debug, Clone, Default)] pub(crate) struct AudioJob {
      pub(crate) info: Option<AudioInfo>,
      pub(crate) cover_file: Option<PathBuf>,  // the original's cover, stored beside the backup
      pub(crate) bitrate_after: Option<u16>,
      pub(crate) cover_removed: bool,
      pub(crate) lufs_after: Option<f32>,
  }
  impl AudioJob { pub(crate) fn reset_result(&mut self); }   // bitrate_after, cover_removed, lufs_after back to empty
  impl Job {
      pub(super) fn audio_file(path: &Path, file_name: &str, stem: String, ext: String, len: u64, format: Format, settings: &Settings) -> Result<Job, String>;
      pub fn is_audio(&self) -> bool;
      pub(super) fn audio_card(&self) -> AudioCard;
      pub(super) fn audio_detail(&self) -> String;
  }
  // pipeline.rs
  pub struct RunOptions { …, pub audio: AudioRunOptions }   // new(): AudioRunOptions { settings: settings.audio.clone(), tool: audio::tool::installed(), aac: audio::tool::aac_encoder() }
  pub struct Job { …, audio: Option<AudioJob> }            // Some for audio jobs
  ```
  Behaviour:
  - **`Job::file`** reads the first `sniff::HEAD` bytes. An unreadable file gives the same `error.read` text. If `sniff(head)` is an audio format, it returns `Job::audio_file(...)`. Otherwise the existing image path runs unchanged. If A's Task 10 already added a video branch (`Job::video_file`) to `Job::file`, put the audio branch beside it, comment `// stream B: audio`. Both branches only look at the head, so their order does not matter.
  - **`audio_file`** applies `settings.audio.ignored_extensions` (via `is_ignored_extension`) and then `settings.audio.max_size_mb` (`Skip::TooLarge`). It never reads the whole file.
  - **`update()`:** `thumb_path` is `audio.cover_file` for audio jobs (backup otherwise). `audio` is `Some(self.audio_card())` for audio jobs.
  - **`audio_card`:**
    - `bitrate_before` is `info.kbps` when the source is lossy, else `None`.
    - `bitrate_after` is the field.
    - `lossless` is `info.lossless()` when the format equals the source format and info is known, else `is_lossless_format(self.format)`.
    - `has_cover` is `cover_file.is_some()`.
  - **`detail()`** returns `audio_detail()` for audio jobs.
  - **`edit()`:** an audio job keeps `Some(source)`, turns a format not in `TARGETS` into `None`, and ignores the image rule. An image job also turns audio formats into `None`.
  - **`audio_detail`** joins with `" · "`:
    - bitrate: `kbps_change` when before and after are known and differ, else `kbps` after. Omitted for lossless results.
    - cover: `cover_removed` → `cover_removed`; `dims_after` set → `cover_change`; `dims` set → `cover`.
    - `self.format.label()`.
    - `lufs` with `lufs_after` formatted as in `loudness::label`.
    - `card.detail.aggressive` when Aggressive.

- [ ] **Step 1: Write the failing tests** (in `pipeline/audio.rs`'s own `tests` module, with its own `Env`/`trash` helpers like those in `pipeline.rs` tests):
  ```rust
  #[test] fn an_audio_file_becomes_an_audio_job() {
      let env = Env::new();
      let path = env.file("my tone.wav", &testutil::wav_tone(&[(1.0, 0.5)], 44100, 2));
      let job = Job::file(&path, &Settings::default(), false).unwrap();     // no gifsicle needed
      assert!(job.is_audio());
      assert_eq!((job.format, job.ext.as_str(), job.name.as_str()), (Format::Wav, "wav", "my tone"));
      let card = job.update(JobState::Queued, None);
      assert_eq!(card.audio, Some(AudioCard { bitrate_before: None, bitrate_after: None, lossless: true, has_cover: false }));
      assert_eq!((card.thumb_path, card.can_edit), (None, false));
  }
  #[test] fn audio_files_follow_the_audio_skip_rules() {
      let env = Env::new();
      let mut s = Settings::default();
      s.audio.ignored_extensions = vec!["aif".into()];
      let aiff = env.file("x.aif", b"FORM\0\0\0\x10AIFFCOMM");
      assert_eq!(Job::file(&aiff, &s, true).unwrap_err(), ".aiff files are ignored in Settings");
      s.images.ignored_extensions = vec!["wav".into()];
      s.audio.max_size_mb = 1;
      let big = env.file("big.wav", &testutil::wav_tone(&[(6.0, 0.5)], 44100, 2));   // > 1 MB
      assert_eq!(Job::file(&big, &s, true).unwrap_err(), "Larger than 1 MB — change the limit in Settings");
  }
  #[test] fn an_mp4_video_is_not_taken_for_audio() {                     // Review Focus 4
      let env = Env::new();
      let mut mp4 = b"\0\0\0\x18ftypisom\0\0\x02\0isomiso2".to_vec(); mp4.extend_from_slice(&[0; 64]);
      let path = env.file("clip.mp4", &mp4);
      assert!(Job::file(&path, &Settings::default(), true).map_or(true, |j| !j.is_audio()));
  }
  #[test] fn an_audio_edit_keeps_its_format_choice_and_refuses_image_formats() {
      let env = Env::new();
      let mut job = Job::file(&env.file("a.mp3", &[0xFF, 0xFB, 0x90, 0x64, 0, 0, 0, 0]), &Settings::default(), true).unwrap();
      job.edit(EditsPatch { format: Some(Some(Format::Mp3)), ..Default::default() });
      assert_eq!(job.edits.format, Some(Format::Mp3));
      job.edit(EditsPatch { format: Some(Some(Format::Png)), ..Default::default() });
      assert_eq!(job.edits.format, None);
      let mut image = Job::file(&env.file("s.png", &testutil::png(8, 8)), &Settings::default(), true).unwrap();
      image.edit(EditsPatch { format: Some(Some(Format::Mp3)), ..Default::default() });
      assert_eq!(image.edits.format, None);
      assert_eq!(image.update(JobState::Queued, None).audio, None);
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core pipeline::audio`. Expected: FAIL.
- [ ] **Step 3: Implement** as specified. `audio_detail` gets its own tests in Task 11, because it needs a run.
- [ ] **Step 4: Run** `cargo test -p ferry-core pipeline`. Expected: PASS, with every existing test unchanged.
- [ ] **Step 5: Commit** `feat(core): audio files become audio jobs`.

---

### Task 11: Running an audio job (`run_audio`), restore and extract

**Files:** Modify `crates/ferry-core/src/pipeline/audio.rs`, and `crates/ferry-core/src/pipeline.rs` at these anchors:
- `run_recipe`, first lines after the cancel check: `if self.audio.is_some() { return self.run_audio(level, o, backups, emit); }`;
- `restore()`, first line: `if self.audio.is_some() { return self.restore_audio(o); }` (comment `// stream B: audio`; A adds the same line for video).

**Interfaces:**
- Consumes: Tasks 3, 4, 7, 8, 9 and 10 as named there; `Backups::store_bytes`; `output::unique`; `bytes_label` (make it `pub(crate)`).
- Produces:
  ```rust
  impl Job {
      pub(super) fn run_audio(&mut self, level: Level, o: &RunOptions, backups: &Backups, emit: &mut dyn FnMut(JobUpdate)) -> Option<Finished>;
      pub(super) fn restore_audio(&mut self, o: &RunOptions) -> Finished;   // put_back(Content::File(backup), true, audio_o, no deliver); clear edits, dims_after, note; audio.reset_result(); Restored. A missing backup gives the same Error as restore()
      pub fn extract_cover(&self) -> Result<PathBuf, String>;   // copies cover_file to "<output dir>/<name> cover.<ext>" (unique); Err(t("audio.cover.none"))
  }
  ```

The algorithm is the order of side effects that tests cannot fully pin. Follow it exactly:

```text
1  backup = self.backup_path(backups)            ─ Err → fail(tf("error.backup", why))
2  tool = o.audio.tool.clone()                    ─ None → fail(t("audio.error.ffmpeg_missing"), "ffmpeg not found")
3  work = o.work_dir/audio/<job id>; create_dir_all ─ Err → fail(tf("error.write", dir))
4  if audio.info is None:
     emit Optimising{0.0, t("audio.step.probe")}
     log = tool.run(probe::args(&backup), cancel, no progress)   ─ Cancelled → cleanup, None; Failed/parse Err → fail(tf("error.decode", source label))
     audio.info = Some(info)
     if info.cover and cover::file_ext(codec) = Some(ext):
        tool.run(cover::extract_args(&backup, work/cover-original.<ext>)) ─ Cancelled → None; Failed → no cover (log at debug)
        bytes = read it; audio.cover_file = backups.store_bytes(id, ".ferry-cover.<ext>", bytes)
        self.dims = raster::display_dims(bytes, sniffed) or info.cover.dims
5  plan = plan::resolve_with(Inputs{info, source_format, &edits, level, &o.audio.settings}, o.audio.aac)
6  audio_o = RunOptions { output: o.audio.settings.output.clone(), ..o.clone() }; no_deliver = |_,_,_| Ok(())
7  if plan.nothing_to_do(source): reset dims_after/note/audio result → put_back(File(backup), self.changed, &audio_o) → AlreadyOptimal
8  cover input: Copy → Source; Optimise/Resize/Scale → cover::process(read cover_file) → write work/cover.<ext> → File(path),
     dims_after = Some(p.dims) if != dims; process Err → note cover_unreadable, cover_removed = true, None;
     Remove → cover_removed = true; Drop(Container) → note tf(cover_dropped, target label), cover_removed = true;
     Drop(Unreadable) → note cover_unreadable, cover_removed = true; Absent → None
9  filter = None; if plan.loudness = Some(l): spec = loudness::spec(l, info.channels)
     report steps: measure "audio.step.measure_of" over [0, MEASURE_SHARE], encode "audio.step.encode_of" over [MEASURE_SHARE, 1]
     log = tool.run(measure_args(&backup, &spec), mapped progress)  ─ Cancelled → cleanup, None; Failed → fail(t("audio.error.measure"))
     m = parse_measurement(log) ─ Err → fail(t("audio.error.measure"))
     if m.is_measurable(): filter = Some(loudness::filter(&spec, &m)) else note too_quiet
   else encode step "audio.step.encode" over [0, 1]
10 out = work/out.<target ext>; log = tool.run(encode_args(..), mapped progress)
     ─ Cancelled → cleanup, None; Failed → cleanup, fail(tf("error.encode", target label))
   if filter.is_some(): lufs_after = Some(spec.i); if loudness::was_limited(log): note tf(limited, loudness::label(&spec))
11 fit: if plan.fit_under = Some(limit): up to 2 retries while size(out) > limit and kbps is Some:
     retry_kbps(..) → None: stop; Some(k) → re-run step 10 with plan.kbps = Some(plan::encoder_kbps(plan.target, plan.aac, k))
   if size(out) > limit: note = tf("card.note.fit_missed", bytes_label(limit))   (overrides any other note)
12 if o.cancel.is_cancelled(): cleanup, None                                     (last moment a cancel counts)
13 bitrate_after = plan.kbps (copy_audio: info.kbps; lossless: None)
14 if plan.explicit or grade(before, size(out)) == Smaller: put(File(out), plan.target, File(backup), &audio_o) → Done(after = size)
   else: reset dims_after/note/audio result → put_back(File(backup), self.changed, &audio_o) → AlreadyOptimal (Same) / Larger
15 cleanup: remove the whole work/audio/<job id> folder (best effort); return Finished
```

"Note" means `self.note = self.note.take().or(Some(text))`, so the first note wins, except where step 11 says it overrides. Progress updates are built from a snapshot `let base = self.update(JobState::Queued, None);`. Each update clones `base` and sets `state`, so the callback never borrows `self`. Updates are emitted only when progress has moved by at least 0.01 since the last one, or reaches 1.0. A step with progress above 0 is shown as `tf("audio.step.progress", step, n = round(progress × 100))`, for example "Encoding MP3 · 50 %". This matches A's `video.step.progress`, so `Card.svelte` needs no percentage of its own.

- [ ] **Step 1: Write the failing tests** with `FakeTool` (`o.audio.tool = Some(fake.clone())`; `options()` uses `test_trash` and a temp work dir, and sets `o.audio.aac = AacEncoder::Native`, so the expected arguments are the same on every OS whatever the real ffmpeg offers). `wav(env)` writes `tone.wav` with `wav_tone(&[(1.0, 0.5)], 44100, 2)`, and the fake writes 1000 bytes as the MP3.
  ```rust
  #[test] fn a_wav_becomes_an_mp3_beside_it_and_the_wav_goes_to_the_trash() {
      let fake = FakeTool::new(vec![FakeStep::Log(samples::WAV_STEREO.into()), FakeStep::Write { log: String::new(), bytes: vec![1; 1000] }]);
      let (mut job, env, o) = audio_job("tone.wav", &wav_bytes(), &fake);
      let (f, seen) = run(&mut job, Level::Normal, &o, &env);
      let u = f.unwrap().update;
      assert_eq!((u.state, u.after, u.format, u.converted), (JobState::Done, Some(1000), Some(Format::Mp3), true));
      assert_eq!(u.output.unwrap(), env.dir.path().join("tone.mp3"));
      assert!(!env.dir.path().join("tone.wav").exists() && env.dir.path().join(".trash/tone.wav").exists());
      assert_eq!(u.detail, "192 kbps · MP3");
      assert_eq!(u.audio.unwrap().bitrate_after, Some(192));
      assert!(seen.iter().any(|s| matches!(&s.state, JobState::Optimising { progress, step } if *progress == 0.5 && step == "Encoding MP3 · 50 %")));
      assert!(!o.work_dir.join("audio").join(job.id.as_str()).exists());
  }
  #[test] fn loudness_runs_two_passes_and_reports_progress_across_both() {
      // script: probe log, measure log (loudness::samples::MEASURED), encode Write with log PASS2_DYNAMIC
      // edits.loudness = Some(Streaming)
      // assert: 3 calls; call 2 contains "-af" with "print_format=json" and "-f","null"; call 3 contains "measured_I=-27.61";
      // Optimising progress 0.15 with "Step 1 of 2 · Measuring loudness · 15 %" and 0.65 with "Step 2 of 2 · Encoding MP3 · 65 %";
      // u.note == Some("Peaks limited to reach −14 LUFS"); u.detail == "192 kbps · MP3 · −14 LUFS"
  }
  #[test] fn an_unmeasurable_file_keeps_its_loudness_and_says_so() {   // Review Focus 1
      // script: probe, measure log loudness::samples::SILENT, encode Write
      // assert: Done; encode args contain no "loudnorm"; note == "Too quiet to measure — loudness left as it was"; detail has no "LUFS"
  }
  #[test] fn a_cancel_during_the_encode_changes_nothing_and_leaves_no_temp_file() {  // Review Focus 2
      // fake.cancel_on_call = Some(1) (the encode call); source bytes before == after; run returns None;
      // no "tone.mp3", no ".trash", no "*.ferry-tmp"; work/audio/<id> gone; job.edits back to the prior recipe
  }
  #[test] fn an_encoder_failure_is_an_error_with_retry_and_the_source_is_untouched() {  // Review Focus 2
      // script: probe, Fail("Unknown encoder 'libmp3lame'"); assert state Error{reason: "Encoder failed (MP3)"}, can_retry, cause contains "libmp3lame",
      // source bytes unchanged, work dir gone
  }
  #[test] fn a_low_bitrate_mp3_is_already_optimal_without_encoding() {   // Review Focus 3
      // source: MP3 frame bytes; script: probe log = MP3_COVER with "320 kb/s" replaced by "128 kb/s" and the Video line removed
      // assert: state AlreadyOptimal, fake.calls().len() == 1, source untouched
  }
  #[test] fn a_missing_ffmpeg_is_an_error_card() {
      // o.audio.tool = None → Error{reason: "ffmpeg missing — reinstall Ferry"}, source untouched
  }
  #[test] fn restore_brings_the_wav_back_and_trashes_the_mp3() {
      // after the first test's run: job.restore(&o, &mut no_deliver) → Restored (through restore_audio: the backup is never read into memory); tone.wav back with original bytes; tone.mp3 in .trash;
      // update.audio.bitrate_after == None
  }
  #[test] fn fit_under_retries_with_a_lower_bitrate_then_notes_a_miss() {
      // probe WAV_STEREO but Duration 00:01:00.00; edits.fit_under = Some(1_000_000); script: probe, Write 1_200_000 bytes, Write 1_100_000, Write 1_050_000
      // assert: 4 calls; the -b:a values of calls 2..4 are strictly decreasing and the first is "128k";
      // note == "Couldn't reach 1 MB — try downscaling"; state Done (explicit edit)
  }
  #[test] fn a_source_named_like_an_option_is_passed_safely() {         // Review Focus 5
      // source "-intro ü.wav"; assert every call's arg after "-i" is absolute and ends with "-intro ü.wav" (probe, encode),
      // and every call's last arg is absolute or "-"
  }
  #[test] fn the_card_shows_the_cover_and_its_new_size() {
      // source MP3 (frame bytes); probe MP3_COVER (320 kbps, mjpeg 3000x3000 → use a real testutil::jpeg(300, 300, 95) as the extracted bytes:
      // script: probe Log(MP3_COVER with 3000x3000 → 300x300), Write{jpeg 300x300} (extract), Write{mp3 bytes} (encode)
      // edits.size = Exact{150,150}; assert: thumb_path is inside env backups dir and ends ".ferry-cover.jpg";
      // dims_before (300,300), dims_after (150,150); detail "320 → 192 kbps · Cover 300×300 → 150×150 · MP3";
      // encode args contain "-map","1:v:0" and "attached_pic"; audio.has_cover
  }
  #[test] fn an_opus_target_drops_the_cover_with_a_note() {
      // edits.format = Some(Opus) on the MP3 with cover; assert note "Opus can't hold cover art — cover removed",
      // detail contains "Cover removed", encode args have no "-map" "1:v:0" nor "0:v:0", output ends ".opus"
  }
  #[test] fn extract_cover_saves_beside_the_result() {
      // after the cover run: job.extract_cover() == Ok(<dir>/a cover.jpg), and a second call gives "a cover (2).jpg"
  }
  #[test] fn an_m4a_on_audiotoolbox_reports_the_snapped_bitrate() {     // Review Focus 6
      // tone.wav; o.audio.aac = AacEncoder::AudioToolbox; edits.format = Some(M4a), edits.bitrate = Some(150)
      // script: probe WAV_STEREO, Write 1000 bytes (encode)
      // assert: Done, output ends ".m4a"; call 2 contains "-c:a","aac_at" and "-b:a","128k" and "-aac_at_mode","cvbr";
      // u.audio.bitrate_after == Some(128); u.detail == "128 kbps · M4A"
  }
  ```
  The bodies given as comments are fixed by their asserts. Write them in the style of the first test.
- [ ] **Step 2: Run** `cargo test -p ferry-core pipeline::audio`. Expected: FAIL.
- [ ] **Step 3: Implement `run_audio`, the two `pipeline.rs` anchors and `extract_cover`.**
- [ ] **Step 4: Run** `cargo test -p ferry-core pipeline`, then `cargo clippy -p ferry-core --all-targets -- -D warnings`. Expected: PASS and clean.
- [ ] **Step 5: Commit** `feat(core): optimise, convert and normalise audio jobs`.

---

### Task 12: End-to-end tests against the real ffmpeg

**Files:** Create `crates/ferry-core/src/pipeline/audio/ffmpeg_tests.rs` (`#[cfg(test)] mod ffmpeg_tests;` in `pipeline/audio.rs`).

**Interfaces:** Consumes the public `Job::file`/`run`/`restore`, `RunOptions::new` (real `tool::installed()` and `tool::aac_encoder()`), `probe`, `loudness`. On a Mac `RunOptions::new` picks `aac_at`, so the M4A tests there run Apple's encoder; CI's macOS `core` job runs them too. Each test starts with `let Some(tool) = tool::installed() else { eprintln!("skipped: no ffmpeg"); return; };`.

- [ ] **Step 1: Write the tests.** Fixtures are built with ffmpeg from `wav_tone`: FLAC and MP3 via `-c:a flac` / `-c:a libmp3lame -b:a 320k`, and a cover via `-i cover.jpg -map 0:a -map 1:v -c:v copy -disposition:v:0 attached_pic`.
  ```rust
  #[test] fn ci_always_has_ffmpeg() {
      if std::env::var_os("CI").is_some() { assert!(tool::installed().is_some(), "ffmpeg must be available to ferry-core tests in CI"); }
  }
  #[test] fn a_real_wav_becomes_a_192_kbps_mp3() {
      // 3 s stereo 44.1 kHz tone → Done; output sniffs Mp3; probe::parse of the output's log: codec "mp3", 44100 Hz, kbps Some(192)
  }
  #[test] fn a_real_flac_with_cover_becomes_an_m4a_that_keeps_a_downscaled_cover() {
      // FLAC + 1200×1200 JPEG cover; edits.size = Exact{600,600} → .m4a; probe of output: codec "aac", cover Some(mjpeg, (600,600))
      // If this fails because the `ipod` muxer drops the picture, switch plan::muxer(M4a) to "mp4" with "-brand", "M4A " and re-run; note it in the commit.
  }
  #[test] fn a_real_m4a_comes_out_of_each_aac_encoder_this_system_offers() {
      // 3 s stereo 44.1 kHz tone, edits.format = Some(M4a); for aac in [AacEncoder::Native, tool::aac_encoder()] (deduplicated):
      //   o = RunOptions::new(..) with o.audio.aac = aac → Done; output sniffs M4a; probe of the output: codec "aac", 44100 Hz, 2 channels
      // if cfg!(target_os = "macos") { assert_eq!(tool::aac_encoder(), AacEncoder::AudioToolbox) }
      // No bitrate assertion: cvbr on a pure tone can come out well under 192 kbps.
  }
  #[test] fn real_streaming_normalisation_lands_on_target() {
      // 12 s tone: 6 s at amplitude 0.05 then 6 s at 0.02 (LRA > 0, about −30 LUFS); edits.loudness = Streaming → WAV target (edits.format = Wav)
      // measure the result with loudness::measure_args + parse_measurement: |i − (−14)| ≤ 1.0 and tp ≤ −0.5
  }
  #[test] fn a_real_opus_result_has_no_cover_and_says_so() {
      // MP3 with cover, edits.format = Opus → output sniffs Opus; probe cover None; note "Opus can't hold cover art — cover removed"
  }
  #[test] fn a_real_low_bitrate_mp3_is_left_alone() {
      // 128 kbps MP3 without a cover → AlreadyOptimal; bytes unchanged
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry-core ffmpeg_tests` with `FERRY_FFMPEG` set. Expected: PASS. Fix the core code, not the tests, if the real ffmpeg disagrees with the plan's arguments. Report any argument change to the controller. If `aac_at` rejects an argument (for example `-aac_at_mode cvbr` at some rate or channel count), report it; do not switch the Mac back to native `aac` on your own.
- [ ] **Step 3: Run** without the variable: `env -u FERRY_FFMPEG cargo test -p ferry-core ffmpeg_tests`. Expected: PASS, with every test skipped except `ci_always_has_ffmpeg` (CI unset locally). If A's CI `core` job does not provide ffmpeg, tell the controller. Do not delete `ci_always_has_ffmpeg`.
- [ ] **Step 4: Commit** `test(core): audio end to end against the bundled ffmpeg`.

---

### Task 13: App: context menu, Extract cover, no compare for audio, `aac_at` warm-up

**Files:**
- Create: `src-tauri/src/audio.rs`
- Modify: `src-tauri/src/lib.rs` (anchors, including the `setup()` line), `src-tauri/src/menu.rs` (anchors), `src-tauri/src/compare.rs` (first lines of `view()`, comment `// stream B: audio`)

**Interfaces:**
- Consumes: `Job::extract_cover`, `audio::{Loudness, CoverPolicy, BAR}`, `audio::tool::aac_encoder`, `JobUpdate.audio`, `jobs::edit`, `crate::state::{AppState, relock}`.
- Produces:
  ```rust
  // audio.rs
  pub fn extract_cover(app: &AppHandle, id: &JobId) -> Result<PathBuf, String>;    // idle entry → job.extract_cover() → reveal_item_in_dir
  #[tauri::command] pub async fn card_extract_cover(app: AppHandle, id: JobId) -> Result<(), String>;  // spawn_blocking(extract_cover)
  pub fn startup();   // warms the cached AAC choice off the main thread and logs it
  // menu.rs
  pub enum Action { …, Loudness(Loudness), Cover(CoverPolicy), CoverSquare(u32), ExtractCover }
  const COVER_SQUARES: [u32; 4] = [1400, 1000, 600, 300];
  // ids: "loudness-off|streaming|apple_music|podcast|broadcast", "cover-keep|optimise|remove", "cover_square-<n>", "extract_cover"
  ```
  Behaviour:
  - `FORMATS` gains `Mp3, M4a, Opus, Wav`, appended. `convert_formats` returns `BAR` for audio cards.
  - `enabled`:
    - `Compare` also needs `card.audio.is_none()`.
    - `Loudness(l)` needs `can_edit && card.edits.loudness != Some(l)`.
    - `Cover(_)` and `CoverSquare(_)` need `can_edit && has_cover`.
    - `ExtractCover` needs an idle card with `has_cover`.
  - `build` delegates audio cards to `build_audio`. Its layout:
    - Open · Show in Finder/Explorer
    - ─ Copy · Copy path · Rename…
    - ─ Preview
    - ─ Re-optimise · Optimise aggressively · Fit under size…
    - ─ Convert to ▸ (MP3, M4A, Opus, WAV) · Convert and keep both ▸
    - ─ Normalise loudness ▸ (the five `audio.loudness.*` labels)
    - · Cover art ▸ (Keep as is, Optimise, Downscale ▸ (Square 1400/1000/600/300 px), Remove, ─, Extract)
    - ─ Restore original ─ Close.
  - `run` routes:
    - `Loudness` → `jobs::edit(EditsPatch{loudness: Some(Some(l))})`;
    - `Cover(c)` → `jobs::edit(EditsPatch{cover: Some(Some(c))})`;
    - `CoverSquare(n)` → `jobs::edit(EditsPatch{size: Some(Some(Size::Exact{width:n,height:n})), framing: Some(Framing::Centre)})`;
    - `ExtractCover` → `crate::audio::extract_cover`.
  - **`audio::startup`:** `std::thread::Builder::new().name("ferry-audio-probe".into()).spawn(|| { let aac = ferry_core::audio::tool::aac_encoder(); tracing::info!(?aac, "audio AAC encoder"); })`. A spawn error is logged with `tracing::warn!` and ignored; jobs then run the check themselves on first use. `setup()` calls it as its last line before `Ok(())` (comment `// stream B: audio`). The log line reads `aac=AudioToolbox` on a Mac with `aac_at`, `aac=Native` otherwise. No test: the choice itself is tested in Task 1, and Task 16 checks the log line.
  - **`compare::view`:** `if card.audio.is_some() && (!single || !cfg!(target_os = "macos")) { return Err(t("audio.compare_unavailable")); }`. Quick Look on macOS still previews and plays the audio result. On Windows, Space on an audio card does nothing beyond a log line (see User decisions).

- [ ] **Step 1: Write the failing tests** in `menu.rs` tests, with `fn done_audio() -> JobUpdate` (a Done MP3 card: `format: Some(Mp3)`, `source_format: Some(Wav)`, `audio: Some(AudioCard{…, has_cover: true})`):
  ```rust
  #[test] fn audio_item_ids_read_back() {
      let card = done_audio();
      for a in [Action::Loudness(Loudness::AppleMusic), Action::Cover(CoverPolicy::Remove), Action::CoverSquare(600),
                Action::ExtractCover, Action::Convert(Format::Opus), Action::KeepBoth(Format::Wav)] {
          assert_eq!(parse_id(&item_id(&card.id, a)), Some((card.id.clone(), a)));
      }
      assert_eq!(parse_id("card:18f-1:loudness-loud"), None);
  }
  #[test] fn an_audio_card_offers_audio_tools_and_no_compare() {
      let mut card = done_audio();
      assert!(!enabled(&card, Action::Compare));
      assert!(enabled(&card, Action::Loudness(Loudness::Streaming)) && enabled(&card, Action::ExtractCover));
      assert_eq!(convert_formats(&card), [Format::Mp3, Format::M4a, Format::Opus, Format::Wav]);
      assert!(!enabled(&card, Action::Convert(Format::Mp3)));
      card.edits.loudness = Some(Loudness::Streaming);
      assert!(!enabled(&card, Action::Loudness(Loudness::Streaming)));
      card.audio = Some(AudioCard { has_cover: false, ..card.audio.clone().unwrap() });
      assert!(!enabled(&card, Action::Cover(CoverPolicy::Optimise)) && !enabled(&card, Action::CoverSquare(600)) && !enabled(&card, Action::ExtractCover));
  }
  ```
  In `compare.rs` tests:
  ```rust
  #[test] fn an_audio_card_has_no_compare_window() {
      let dir = tempfile::tempdir().unwrap();
      let card = JobUpdate { audio: Some(AudioCard { bitrate_before: None, bitrate_after: Some(192), lossless: false, has_cover: false }), ..converted(dir.path()) };
      assert_eq!(view(&card, false).unwrap_err(), "Audio compare isn't available yet");
      assert_eq!(view(&card, true).is_ok(), cfg!(target_os = "macos"));
  }
  ```
- [ ] **Step 2: Run** `cargo test -p ferry menu`, then `cargo test -p ferry compare`. Expected: FAIL. These app tests run locally; CI runs clippy on the app.
- [ ] **Step 3: Implement.** Register `audio::card_extract_cover` in `generate_handler!` under the anchor, and add `audio::startup();` to `setup()`.
- [ ] **Step 4: Run** `cargo test -p ferry menu`, `cargo test -p ferry compare`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS and clean.
- [ ] **Step 5: Commit** `feat(app): audio context menu, cover extraction and the aac_at warm-up`.

---

### Task 14: The card UI for audio

**Files:**
- Modify: `src/lib/ipc.ts`, `src/lib/edits.ts`, `src/lib/cards.ts`, `src/lib/card.fixture.ts`, `src/stack/Card.svelte`
- Test: `src/lib/edits.test.ts`, `src/lib/cards.test.ts`

**Interfaces:**
- Consumes: `JobUpdate.audio`, `Edits.{bitrate,loudness,cover}` (Rust serde names), `audio.*` strings.
- Produces:
  ```ts
  // ipc.ts (anchor)
  export type Format = 'png' | 'jpeg' | 'gif' | 'webp' | 'mp3' | 'm4a' | 'flac' | 'wav' | 'aiff' | 'ogg' | 'opus';
  export type Loudness = 'off' | 'streaming' | 'apple_music' | 'podcast' | 'broadcast';
  export type CoverPolicy = 'keep' | 'optimise' | 'downscale' | 'remove';
  export interface AudioCard { bitrate_before: number | null; bitrate_after: number | null; lossless: boolean; has_cover: boolean }
  // Edits gains: bitrate: number | null; loudness: Loudness | null; cover: CoverPolicy | null;   JobUpdate gains: audio: AudioCard | null;
  // edits.ts
  export const BITRATE_STOPS: number[];                       // [320, 256, 192, 160, 128, 96, 64]
  export function currentBitrate(card: JobUpdate): number;    // edits.bitrate ?? audio.bitrate_after ?? audio.bitrate_before ?? 192, as the nearest stop at or below
  export function nextBitrate(kbps: number): number;          // the next lower stop; 64 stays 64
  export function bitratePatch(kbps: number): EditsPatch;     // { bitrate: kbps, level: 'normal' }
  export function bitrateLabel(kbps: number): string;         // "192 kbps"
  /** What the Compression button drives: encoder quality on images, bitrate on audio. */
  export function compressionStops(card: JobUpdate): Quality[];
  export function compressionStart(card: JobUpdate, fallback: number): Quality;
  export function compressionLabel(card: JobUpdate, stop: Quality): string;
  export function compressionPatch(card: JobUpdate, stop: Quality): EditsPatch;
  export function nextCompression(card: JobUpdate, current: Quality): Quality;
  export function compressionRound(card: JobUpdate): (stop: Quality) => boolean;  // images: multiple of 25; audio: multiple of 64
  // generalised, same results for the existing stops:
  export function stopY(stops: Quality[], i: number, track: number): number;      // value axis runs from the highest stop (top) to the lowest
  export function labelledStops(stops: Quality[], track: number, round?: (stop: Quality) => boolean): number[];
  ```
  - `formatChoices` returns `BAR` order (`mp3, m4a, opus, wav`) for cards with `audio`. Labels: `FORMAT_LABELS` covers every `Format` (`MP3 M4A FLAC WAV AIFF Ogg Opus`). `firstFrame` is false.
  - `railActions` for audio cards:
    - `downscale` only if `audio.has_cover && dims_before`;
    - `compression` only if `!compact && !audio.lossless`;
    - `restore` as today;
    - `copy`, `more`;
    - never `crop`/`compare`.
  - `needsOriginal` is unchanged.

- [ ] **Step 1: Write the failing tests:**
  ```ts
  // edits.test.ts
  const audio = (extra: Partial<JobUpdate> = {}) => card({ kind: 'done' }, { format: 'mp3', source_format: 'wav', ext: 'mp3',
    audio: { bitrate_before: null, bitrate_after: 192, lossless: false, has_cover: true }, ...extra });
  it('steps the bitrate down the stops', () => {
    expect(currentBitrate(audio())).toBe(192);
    expect(currentBitrate(audio({ edits: { ...audio().edits, bitrate: 150 } }))).toBe(128);
    expect([nextBitrate(320), nextBitrate(96), nextBitrate(64)]).toEqual([256, 64, 64]);
    expect(bitratePatch(128)).toEqual({ bitrate: 128, level: 'normal' });
    expect(bitrateLabel(96)).toBe('96 kbps');
  });
  it('drives bitrate on audio cards and quality on images', () => {
    expect(compressionStops(audio())).toEqual([320, 256, 192, 160, 128, 96, 64]);
    expect(compressionLabel(audio(), 128)).toBe('128 kbps');
    expect(compressionPatch(audio(), 128)).toEqual({ bitrate: 128, level: 'normal' });
    expect(nextCompression(audio(), 192)).toBe(160);
    expect(compressionStops(card({ kind: 'done' }))).toEqual(QUALITY_STOPS);
    expect(compressionLabel(card({ kind: 'done' }), 60)).toBe('q 60');
  });
  it('lays bitrate stops on the track by value and labels the round ones', () => {
    expect(stopY(BITRATE_STOPS, 0, 128)).toBe(0);
    expect(stopY(BITRATE_STOPS, 6, 128)).toBe(128);
    expect(stopY(BITRATE_STOPS, 2, 128)).toBe(64);                       // 192 is halfway between 320 and 64
    expect(labelledStops(BITRATE_STOPS, 200, compressionRound(audio())).map((i) => BITRATE_STOPS[i])).toEqual([320, 256, 192, 128, 64]);
  });
  it('offers MP3, M4A, Opus and WAV on audio cards', () => {
    expect(formatChoices(audio()).map((c) => [c.label, c.current])).toEqual([['MP3', true], ['M4A', false], ['Opus', false], ['WAV', false]]);
  });
  // cards.test.ts
  it('gives audio cards cover downscale, bitrate, restore, copy and more', () => {
    const a = card({ kind: 'done' }, { can_restore: true, format: 'mp3', audio: { bitrate_before: 320, bitrate_after: 192, lossless: false, has_cover: true } });
    expect(railActions(a)).toEqual(['downscale', 'compression', 'restore', 'copy', 'more']);
    expect(railActions({ ...a, audio: { ...a.audio!, has_cover: false, lossless: true } })).toEqual(['restore', 'copy', 'more']);
    expect(railActions(a, true)).toEqual(['downscale', 'restore', 'copy', 'more']);
  });
  ```
  Every existing `edits.test.ts` expectation must still pass. `stopY`/`labelledStops` keep their results for `SCALE_STOPS`/`QUALITY_STOPS`, whose top is 100. Add `audio: null` and the three `Edits` fields (`null`) to `card.fixture.ts`.
- [ ] **Step 2: Run** `pnpm test`. Expected: FAIL.
- [ ] **Step 3: Implement** the helpers. In `Card.svelte`, at `// stream B: audio` anchors:
  - (a) the `quality`, `slideLabel`, `slideStops`, `slideYs`, `slideLabelled`, `press` start index, `release` patch and `step` use the `compression*` helpers instead of `QUALITY_STOPS`/`qualityLabel`/`qualityPatch`/`nextQuality`/`currentQuality`. They pass `compressionRound(card)` to `labelledStops`.
  - (b) `tipText`: `downscale` → `t('audio.card.cover_downscale')` and `compression` → `t('audio.card.bitrate')` when `card.audio`.
  - (c) the placeholder: when `card.audio` and no `thumb_path`, show the Lucide `audio-waveform` icon (`<path d="M2 13a2 2 0 0 0 2-2V7a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0V4a2 2 0 0 1 4 0v13a2 2 0 0 0 4 0v-4a2 2 0 0 1 2-2" />`) instead of the image icon.
- [ ] **Step 4: Run** `pnpm test`, then `pnpm check`. Expected: PASS, with 0 svelte-check errors.
- [ ] **Step 5: Commit** `feat(ui): audio cards with bitrate slider, cover downscale and waveform placeholder`.

---

### Task 15: Settings UI: the Audio pane and the Audio file-handling rows

**Files:**
- Modify: `crates/ferry-core/src/output.rs` (`example_audio`)
- Modify: `src-tauri/src/audio.rs` (`audio_output_preview`), `src-tauri/src/lib.rs` (handler anchor)
- Modify: `src/lib/schema.ts`, `src/lib/settings.ts`, `src/lib/settings.fixture.ts`, `src/settings/Row.svelte`, `src/settings/controls/OutputRule.svelte`
- Test: `crates/ferry-core/src/output.rs` tests, `src/lib/schema.test.ts`

**Interfaces:**
- Produces:
  ```rust
  // output.rs: same as `example`, for "~/Music/track.wav" (Optimised keeps "wav", Converted "mp3", %z "0×0")
  pub fn example_audio(kind: OutputKind, rule: &OutputRule, home: &Path, work_dir: &Path, today: NaiveDate) -> Result<String, Vec<String>>;
  // audio.rs
  #[tauri::command] pub fn audio_output_preview(app: AppHandle, rule: OutputRule, converted: bool) -> Result<String, Vec<String>>;
  ```
  ```ts
  // schema.ts (anchor): PaneId gains 'audio'; panes gains { id: 'audio', label: 'audio.pane' } right after 'images';
  // Control 'output' gains `media?: 'audio'`.
  ```
  Rows (`pane: 'audio'` unless noted; ids and paths are exact):

  | id | group | label / help | path | control |
  | --- | --- | --- | --- | --- |
  | `audio_out_wav` … `audio_out_opus` (7 rows: wav, aiff, flac, mp3, m4a, ogg, opus) | `audio.group.formats` | `audio.outputs.<ext>` / `audio.outputs.help` on the first row only | `audio.outputs.<ext>` | select of `mp3 m4a opus wav flac` labelled `audio.format.*` |
  | `audio_bitrate` | — | `audio.bitrate` / `.help` | `audio.bitrate_kbps` | number 32–320 `audio.unit.kbps` |
  | `audio_opus_bitrate` | — | `audio.opus_bitrate` / `.help` | `audio.opus_bitrate_kbps` | number 16–256 `audio.unit.kbps` |
  | `audio_loudness` | — | `audio.loudness` / `.help` | `audio.loudness` | select of the five `audio.loudness.*` |
  | `audio_cover` | — | `audio.cover` / `.help` | `audio.cover` | select `keep optimise downscale remove` → `audio.cover.*` |
  | `audio_cover_shape` | — | `audio.cover_shape` / `.help` | `audio.cover_size.shape` | select `square long_edge` → `audio.cover_shape.*` |
  | `audio_cover_side` | — | `audio.cover_side` / `.help` | `audio.cover_size.side` | number 100–3000 `audio.unit.px` |
  | `audio_ignored_extensions` | — | `audio.ignored_extensions` / `.help` | `audio.ignored_extensions` | extensions |
  | `audio_max_size` | — | `audio.max_size` / `.help` | `audio.max_size_mb` | number 1–100000 `unit.mb` |
  | `audio_output_optimised` (pane `files`) | `audio.group.files` | `output.optimised` / `output.optimised.help` | `audio.output.optimised` | output, `converted: false, media: 'audio'` |
  | `audio_output_converted` (pane `files`) | `audio.group.files` | `output.converted` / `output.converted.help` | `audio.output.converted` | output, `converted: true, media: 'audio'` |

  `Row.svelte` passes `media` to `OutputRule.svelte`. That control calls `invoke('audio_output_preview', { rule, converted })` when `media === 'audio'`, and `output_preview` otherwise. `settings.ts` `Settings` gains the `audio` interface mirroring Task 5. `settings.fixture.ts` gains its defaults.

- [ ] **Step 1: Write the failing tests.** In `output.rs`:
  ```rust
  #[test] fn the_audio_example_is_a_track_in_music() {
      let home = Path::new("/Users/me"); let rule = OutputRule { location: Location::SameFolder, template: "%f_optimised.%e".into(), ..OutputRule::default() };
      assert_eq!(example_audio(OutputKind::Optimised, &rule, home, Path::new("/w"), date()).unwrap(), "~/Music/track_optimised.wav");
      let conv = OutputRule { location: Location::InPlace, ..OutputRule::default() };
      assert_eq!(example_audio(OutputKind::Converted, &conv, home, Path::new("/w"), date()).unwrap(), "~/Music/track.mp3");
  }
  ```
  Use the existing tests' date helper, or `NaiveDate::from_ymd_opt(2026, 10, 23)`. In `schema.test.ts`, change the File-handling expectation to `['images.output.optimised', 'images.output.converted', 'audio.output.optimised', 'audio.output.converted']`, and add:
  ```ts
  it('has the Audio pane with every audio setting', () => {
    expect(panes.map((p) => p.id)).toContain('audio');
    expect(all.filter((s) => s.pane === 'audio').map((s) => s.path)).toEqual([
      'audio.outputs.wav', 'audio.outputs.aiff', 'audio.outputs.flac', 'audio.outputs.mp3', 'audio.outputs.m4a', 'audio.outputs.ogg', 'audio.outputs.opus',
      'audio.bitrate_kbps', 'audio.opus_bitrate_kbps', 'audio.loudness', 'audio.cover', 'audio.cover_size.shape', 'audio.cover_size.side',
      'audio.ignored_extensions', 'audio.max_size_mb',
    ]);
  });
  ```
  The existing tests "points every value setting at a field that exists" and "has real strings" then cover the fixture and the strings.
- [ ] **Step 2: Run** `cargo test -p ferry-core output`, then `pnpm test`. Expected: FAIL.
- [ ] **Step 3: Implement.** Refactor `example` into a private `example_for(kind, rule, home, work_dir, today, sample)`, where the sample gives the folder (`Desktop`/`Music`), file (`shot.png`/`track.wav`), extensions and dims. `example` and `example_audio` call it. `example`'s existing tests stay unchanged.
- [ ] **Step 4: Run** `cargo test -p ferry-core output`, `pnpm test`, `pnpm check`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS.
- [ ] **Step 5: Commit** `feat(ui): Audio settings pane and audio file handling`.

---

### Task 16: Manual check on the Mac (cannot be automated)

**Manual check.** Ask the user before starting `pnpm tauri dev`. Use a TextEdit window as the focus target, and never click into other apps. Prepare `~/Desktop/ferry-audio/` with:
- `tone.wav`, a 30 s stereo file;
- `album.flac` with a 3000×3000 JPEG cover;
- `song.mp3` at 320 kbps with a cover;
- `low.mp3` at 128 kbps without a cover;
- `quiet.wav`, about −30 LUFS.

Make them with the bundled ffmpeg; the commands from Task 12's fixtures work. Record each result in the PR description.

- [ ] **1. WAV → MP3.** Select `tone.wav` in Finder, press ⌘C, then ⌃⇧C.
  - The card shows a waveform icon and "Reading audio", then "Encoding MP3" with a rising "NN %".
  - It ends Done with `tone.mp3`, a green savings pill, and the detail "192 kbps · MP3".
  - `tone.wav` is in the Trash.
- [ ] **2. Cover thumbnail.** Do the same with `album.flac`.
  - The thumbnail is the album art, and the result is `album.m4a`.
  - Finder Get Info, or Music.app (drag the file in), shows the artwork.
- [ ] **3. Bitrate slider.** On the `song.mp3` card, press and drag the Compression button.
  - The stops read 320 … 64 kbps.
  - Release at 128: the card re-runs and the detail reads "320 → 128 kbps · Cover … · MP3".
- [ ] **4. Cover downscale.** Use ⋯ → Cover art ▸ Downscale ▸ Square 600 px.
  - The detail shows "Cover 3000×3000 → 600×600".
  - Press `5` while hovering the card: the cover goes to 50 % (1500×1500).
- [ ] **5. Loudness.** On `quiet.wav`, use ⋯ → Normalise loudness ▸ Streaming.
  - The step text goes "Step 1 of 2 · Measuring loudness" (progress up to about 30 %), then "Step 2 of 2 · Encoding MP3".
  - The detail ends "· −14 LUFS".
  - Verify with `"$FERRY_FFMPEG" -hide_banner -i ~/Desktop/ferry-audio/quiet.mp3 -af loudnorm=print_format=json -f null - 2>&1 | grep input_i`: the value is between −15.0 and −13.0.
- [ ] **6. Remove and Extract.** Use Cover art ▸ Remove: the detail shows "Cover removed". Then use Cover art ▸ Extract: Finder reveals "song cover.jpg".
- [ ] **7. Never worse.** Optimise `low.mp3`: the amber "Already optimal" pill appears, and the file's modification date does not change.
- [ ] **8. Opus.** On the `song.mp3` card, click the Opus chip in the format bar: the result is `song.opus` with the amber note "Opus can't hold cover art — cover removed".
- [ ] **9. Restore.** On the `tone.mp3` card, click Restore: `tone.wav` is back and `tone.mp3` is in the Trash.
- [ ] **10. Settings.** In Settings → Audio, set "WAV becomes" to M4A (AAC) and optimise another WAV: it becomes `.m4a`. In File handling → Audio → Optimised → Same folder, the example reads `~/Music/track_optimised.wav`.
- [ ] **11. Compare is off.** ⌘D on an audio card does nothing visible (the log has "Audio compare isn't available yet"). Space opens Quick Look, which plays the result.
- [ ] **12. Apple AAC encoder (`aac_at`).**
  - Within a few seconds of launch, `~/Library/Logs/io.ferry.desktop/ferry.log` has the line `audio AAC encoder` with `aac=AudioToolbox`. `aac=Native` on this Mac is a failure: report it (A's build lacks `--enable-audiotoolbox`, see Task 1 Step 1).
  - `album.m4a` from item 2 plays in Music.app without clicks or a gap at the start, and Finder Get Info → More Info → Codecs shows AAC.
  - `"$FERRY_FFMPEG" -hide_banner -i ~/Desktop/ferry-audio/album.m4a 2>&1 | grep Audio` shows `aac (LC)`, 44100 or 48000 Hz, and a bitrate close to 192 kb/s (constrained VBR stays near the target; well over 200 kb/s is a failure).
  - Listen once in Music.app to `tone.wav` converted with "WAV becomes" = M4A (from item 10) next to the original: no audible difference.
- [ ] **13. Windows:** not verified. It is built in CI only, as in Cycles 1–2. Native `aac` is used there; CI's Windows `core` job runs Task 1's `other_systems_use_native_aac_without_asking_ffmpeg`. Say so in the PR.

---

## Later: stream F (pipeline steps)

B ships no pipeline code. When F's `StepKind` registry exists (`crates/ferry-core/src/pipeline_steps.rs`), F's second step registers the audio steps as producers of the `EditsPatch` that B defined. Each step maps one-to-one to a recipe field:
- `normaliseLoudness(target)` → `EditsPatch { loudness }`;
- `coverArt(policy)` → `EditsPatch { cover }`;
- `coverSize(Size)` → `EditsPatch { size }`;
- `audioBitrate(kbps)` → `EditsPatch { bitrate }`;
- `convert(to: mp3|m4a|opus|wav)` → `EditsPatch { format }`.

The job then runs once through `Job::run` → `run_audio`, so a file is encoded once however many audio steps a pipeline has.

## Cross-stream notes (for merges into `feat/c3-all`)

- **A (video):**
  - B's `sniff` M4A check must sit above A's `ftyp` branch. A gives `mp42` with a compatible `M4A ` brand to `Mp4`; B claims it as `M4a` first.
  - If A also added a file-backed write path, keep one (Task 9 skip rule).
  - B's tool calls A's `ffmpeg::exec` and passes none of the global flags `exec` prepends. A's Task 9 and B's Task 9 are the same change (`Content`, `backup_path`, `atomic_replace_from`), so whichever lands second is a no-op.
  - `restore()` gets one dispatch line from each stream (`restore_video`, `restore_audio`); keep both.
  - A's ffmpeg build must contain the components in Global Constraints.
  - **AudioToolbox on macOS:** B prefers `aac_at`, which exists only if A's macOS `./configure` passes `--enable-audiotoolbox`. A's plan (Task 2, "macOS adds `--enable-videotoolbox --enable-audiotoolbox`") already has it, so A needs no change. Because A also passes `--disable-autodetect`, dropping that flag would silently remove `aac_at`; Ferry would fall back to native `aac`, and B's Task 1 test `the_macos_sidecar_offers_audiotoolbox_aac` (run in the macOS `core` CI job) would fail. Keep the flag whenever the build script changes.
- **C (watchers):**
  - In `src-tauri/src/watch/hooks.rs`, set `pub const AUDIO: bool = true;` and make `audio_job` return `Some(Job::file(path, settings, state.gifsicle.is_some()))`. `Job::file` dispatches audio itself.
  - In C's `watch::media::classify`, `sniff(head).is_some()` must become `sniff(head).is_some_and(|f| !f.is_audio())` → `Image`. Add `sniff(head).is_some_and(Format::is_audio)` → `Audio` before the extension rules.
  - C's `selfwrite::record` goes in `fsutil::replace_with` (Task 9), so `atomic_replace_from` records too.
- **D (drop/batch):** dropped audio files go through `Job::file` and need nothing else. The batch Adjust panel's "audio bitrate" and "cover art size" map to `EditsPatch { bitrate }` and `EditsPatch { size }`.
- **I (clipboard-fixes):** no overlap.

## User decisions (2026-10-23)

The user answered the plan's open questions on 2026-10-23 (`docs/superpowers/handover/2026-10-23-cycle3-open-questions.md`, "B audio" and "Decisions"). The plan above already follows every answer; nothing here is still open.

1. **Order of merging A and B:** start B only after A's Tasks 1–4 and 9 are merged (see Start condition). B's Task 9 is then skipped.
2. **Ogg Vorbis inputs** default to Opus (`.ogg` becomes `.opus`). No libvorbis is added.
3. **Broadcast target:** EBU R 128 (−23 LUFS, −1 dBTP) only. No ATSC A/85 option this cycle.
4. **Audio A/B compare:** not in B. ⌘D stays disabled on audio cards; compare is a Cycle 4 item.
5. **Windows Space preview** on audio cards does nothing beyond a log line.
6. **AAC encoder:** on macOS use Apple's `aac_at` when the sidecar offers it, checked once at start-up; native `aac` otherwise and on Windows. Specified in Decisions ("AAC encoder") and built in Tasks 1, 7, 10, 11, 12, 13 and 16.
7. **Defaults** as listed: loudness Off; cover policy Optimise (recompress, no resize); 192 kbps MP3/AAC and 128 kbps Opus; audio size limit 2000 MB.
