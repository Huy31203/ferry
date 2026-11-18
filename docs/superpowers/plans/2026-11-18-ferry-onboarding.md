# Onboarding Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A one-page first-run window. It sets what happens to the original file, the video quality and encoder (each encoder with a time measured on this Mac), and the shelf archive result. It shows once to new and existing users.

**Architecture:**
- `General.onboarded` decides whether the window opens at the end of setup.
- `ferry_core::video::bench` times each encoder on a generated 3 s 1080p sample through the real `plan::plan`.
- `src-tauri/src/onboarding.rs` owns the window and the measuring thread, which emits `onboarding:measured` events.
- The Svelte page uses pure helpers in `src/onboarding/choices.ts` to read the settings and write the choices back through `settings_set`.

**Tech Stack:** Rust (ferry-core, Tauri 2), Svelte 5 runes, vitest.

**Spec:** `docs/superpowers/specs/2026-11-18-ferry-onboarding-design.md`. Read it first; it's the authority.

## Global Constraints

- Read AGENTS.md and CONSTRAINTS.md at the repo root if present, and follow both. Neither exists today.
- Shell prefix for every command:
  ```
  export PATH=/usr/local/bin:$HOME/.nvm/versions/node/v22.23.4/bin:$HOME/.cargo/bin:$PATH
  ```
- ferry-core tests need `FERRY_GIFSICLE=$PWD/src-tauri/binaries/gifsicle-x86_64-apple-darwin FERRY_FFMPEG=$PWD/src-tauri/binaries/ffmpeg-x86_64-apple-darwin`.
- Never use `var` in JS or TS. Keep code minimal and reuse the nearest existing pattern.
- No manual end-to-end runs and no screenshots. Don't stop or relaunch the running dev app or the installed Ferry.
- Every commit message ends with:
  ```

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  ```
- Final gate, from the repo root, with the FERRY env vars:
  ```
  cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npx svelte-check && npx vitest run && npx vite build
  ```

## Review Focus

1. **A user whose location is `folder` or `temporary`, who leaves the radio alone.** Their location is kept. Pinned by `an_untouched_radio_keeps_a_custom_location` (Task 3).
2. **An existing settings file with no `onboarded` field.** It loads as `false`, and everything else stays as stored, including a stored `video.compression` of 80. Pinned by `an_old_file_is_not_onboarded_and_keeps_its_video_quality` (Task 1).
3. **An encoder that fails** (for example, no hardware encoder). Its result is `None` and the other encoders still run. Pinned by `a_failed_encoder_is_none_and_the_rest_still_run` (Task 2), which uses a cancelled token for one encoder.
4. **Closing mid-measure.** The thread stops, and the temporary folder is removed. Pinned by `the_sample_folder_is_removed` (Task 2). The cancel itself is a manual check.
5. **On Windows**, the shelf section is hidden and its setting untouched. Pinned by the `includeShelf: false` case in `applyChoices` (Task 3).

---

### Task 1: Settings (`onboarded` and video quality 95)

**Files:**
- Modify:
  - `crates/ferry-core/src/settings.rs`
  - `src/lib/settings.ts`
  - `src/lib/settings.fixture.ts`

**Interfaces:**
- Produces:
  - `General.onboarded: bool` (default `false`)
  - `Video::default().compression == 95`
  - TS: `general.onboarded: boolean`

- [ ] **Step 1: Failing tests** in `settings.rs`'s test module:

```rust
    #[test]
    fn fresh_settings_are_not_onboarded_and_videos_default_to_95() {
        let s = Settings::default();
        assert!(!s.general.onboarded);
        assert_eq!(s.video.compression, 95);
    }

    #[test]
    fn an_old_file_is_not_onboarded_and_keeps_its_video_quality() {
        let s: Settings =
            serde_json::from_str(r#"{"general":{"language":"en"},"video":{"compression":80}}"#).unwrap();
        assert!(!s.general.onboarded);
        assert_eq!(s.video.compression, 80);
    }
```

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p ferry-core onboarded` with the FERRY env vars. Expected: a compile error, because there is no `onboarded` field.

- [ ] **Step 3: Implement.**
- `General`: after `language`, add:

  ```rust
      /// The first-run screen was finished; it shows until then (onboarding spec §2).
      pub onboarded: bool,
  ```

  Add `onboarded: false,` to `impl Default for General`.
- `impl Default for Video`: `compression: 80` becomes `compression: 95`.
- `src/lib/settings.ts`: add `onboarded: boolean;` after `language: string;` in `general`.
- `src/lib/settings.fixture.ts`: add `onboarded: false,` after `language: 'en',`.
- In the fixture, change the video `compression: 80` (line ~86, inside `video`) to `95`. Leave the images one at 80.

- [ ] **Step 4: Run.**
  - Run `cargo test -p ferry-core settings` with the env vars, then `npx vitest run src/lib`.
  - Expected: everything passes.
  - If an existing test asserted video compression 80 as a default, update it to 95 and record a Ruling.
  - If a TS test compares the fixture against Rust's serialised defaults (look for a test reading `Settings::default()` JSON), it should now pass. Fix whatever else it flags.

- [ ] **Step 5: Commit** `feat(settings): onboarded flag; videos default to quality 95`.

---

### Task 2: Measure the encoders (`ferry_core::video::bench`)

**Files:**
- Create: `crates/ferry-core/src/video/bench.rs`
- Modify: `crates/ferry-core/src/video/mod.rs` (`pub mod bench;`)

**Interfaces:**
- Consumes:
  - `video::probe::probe(path, cancel) -> Result<Probe, FerryError>`
  - `video::plan::{plan, plain_target, PlanInput}`
  - `video::hardware::hardware()`
  - `ffmpeg::run(args, cancel, on_progress)`
  - `optimise::Encoder`
  - `edit::Edits`
  - `settings::Video`
- Produces:
  - `pub const SAMPLE_SECS: u32 = 3;`
  - `pub fn make_sample(dir: &Path, cancel: &CancelToken) -> Result<PathBuf, FerryError>`
  - `pub fn time(sample: &Path, dir: &Path, encoder: Encoder, video: &settings::Video, cancel: &CancelToken) -> Option<f64>`, giving seconds per minute of video
  - `pub fn measure_all(video: &settings::Video, cancel: &CancelToken, each: &mut dyn FnMut(Encoder, Option<f64>))`, which makes its own temporary folder and removes it
  - `pub fn label(secs_per_min: Option<f64>) -> String`

- [ ] **Step 1: Failing tests** at the bottom of `bench.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn labels_read_like_the_spec() {
        assert_eq!(label(None), "—");
        assert_eq!(label(Some(3.0)), "< 5 s per min of 1080p");
        assert_eq!(label(Some(20.4)), "~20 s per min of 1080p");
        assert_eq!(label(Some(150.0)), "~3 min per min of 1080p");
    }

    #[test]
    fn every_encoder_gets_a_time() {
        if crate::testutil::ffmpeg().is_none() {
            return;
        }
        let mut got = Vec::new();
        measure_all(&settings::Video::default(), &CancelToken::new(), &mut |e, t| got.push((e, t)));
        let encoders: Vec<_> = got.iter().map(|(e, _)| *e).collect();
        assert_eq!(encoders, [Encoder::Fast, Encoder::Small, Encoder::VisuallyLossless]);
        assert!(got.iter().all(|(_, t)| t.is_some_and(|s| s > 0.0)), "{got:?}");
    }

    #[test]
    fn a_failed_encoder_is_none_and_the_rest_still_run() {
        if crate::testutil::ffmpeg().is_none() {
            return;
        }
        let dir = tempfile::tempdir().unwrap();
        let sample = make_sample(dir.path(), &CancelToken::new()).unwrap();
        let cancelled = CancelToken::new();
        cancelled.cancel();
        let video = settings::Video::default();
        assert_eq!(time(&sample, dir.path(), Encoder::Small, &video, &cancelled), None);
        assert!(time(&sample, dir.path(), Encoder::Fast, &video, &CancelToken::new()).is_some());
    }

    #[test]
    fn the_sample_folder_is_removed() {
        if crate::testutil::ffmpeg().is_none() {
            return;
        }
        let before = bench_dirs();
        measure_all(&settings::Video::default(), &CancelToken::new(), &mut |_, _| {});
        assert_eq!(bench_dirs(), before);
    }

    fn bench_dirs() -> usize {
        std::fs::read_dir(std::env::temp_dir())
            .unwrap()
            .filter_map(Result::ok)
            .filter(|e| e.file_name().to_string_lossy().starts_with(&format!("ferry-bench-{}", std::process::id())))
            .count()
    }
}
```

`label` returns English. Check whether `ferry_core::i18n::tf` resolves `en.json` keys in core tests; other core modules use `t()` in tests. If it does, use the i18n keys from Task 3 Step 6 (`onboarding.measure.*`) and add them to `en.json` in this task. Otherwise return the literal strings above, and record a Ruling.

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p ferry-core video::bench` with the env vars. Expected: a compile error.

- [ ] **Step 3: Implement** above the tests:

```rust
//! Time each video encoder on a generated 3 s 1080p clip, through the same plan
//! real videos use (onboarding spec §5).

use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Instant;

use crate::cancel::CancelToken;
use crate::edit::Edits;
use crate::error::FerryError;
use crate::ffmpeg;
use crate::optimise::Encoder;
use crate::settings;
use crate::sniff::Format;
use crate::video::{hardware, plan, probe};

pub const SAMPLE_SECS: u32 = 3;

pub fn make_sample(dir: &Path, cancel: &CancelToken) -> Result<PathBuf, FerryError> {
    let out = dir.join("sample.mp4");
    let args: Vec<OsString> = [
        "-f", "lavfi", "-i",
        &format!("testsrc2=size=1920x1080:rate=30:duration={SAMPLE_SECS}"),
        "-c:v", "libx264", "-preset", "ultrafast", "-crf", "10", "-pix_fmt", "yuv420p", "-an",
    ]
    .iter()
    .map(OsString::from)
    .chain([out.clone().into_os_string()])
    .collect();
    ffmpeg::run(&args, cancel, &mut |_| {})?;
    Ok(out)
}

/// Seconds per minute of video, or `None` when the encode failed or was cancelled.
pub fn time(sample: &Path, dir: &Path, encoder: Encoder, video: &settings::Video, cancel: &CancelToken) -> Option<f64> {
    if cancel.is_cancelled() {
        return None;
    }
    let probe = probe::probe(sample, cancel).ok()?;
    let settings = settings::Video { encoder, ..video.clone() };
    let edits = Edits { encoder: Some(encoder), ..Edits::default() };
    let target = plan::plain_target(Format::Mp4, &probe, &settings);
    let output = dir.join(format!("{encoder:?}.mp4"));
    let planned = plan::plan(&plan::PlanInput {
        input: sample,
        output: &output,
        probe: &probe,
        source: Format::Mp4,
        target,
        edits: &edits,
        settings: &settings,
        hardware: hardware::hardware(),
        crop: None,
        out: (probe.video.width, probe.video.height),
        software: false,
        video_bitrate: None,
        same_container: target == Format::Mp4,
    });
    let began = Instant::now();
    ffmpeg::run(&planned.args, cancel, &mut |_| {}).ok()?;
    let _ = std::fs::remove_file(&output);
    Some(began.elapsed().as_secs_f64() * 60.0 / f64::from(SAMPLE_SECS))
}

pub fn measure_all(video: &settings::Video, cancel: &CancelToken, each: &mut dyn FnMut(Encoder, Option<f64>)) {
    let dir = std::env::temp_dir().join(format!("ferry-bench-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let sample = std::fs::create_dir_all(&dir).ok().and_then(|()| make_sample(&dir, cancel).ok());
    for encoder in [Encoder::Fast, Encoder::Small, Encoder::VisuallyLossless] {
        let secs = sample.as_deref().and_then(|s| time(s, &dir, encoder, video, cancel));
        each(encoder, secs);
    }
    let _ = std::fs::remove_dir_all(&dir);
}

pub fn label(secs_per_min: Option<f64>) -> String {
    match secs_per_min {
        None => "—".into(),
        Some(s) if s < 5.0 => "< 5 s per min of 1080p".into(),
        Some(s) if s < 90.0 => format!("~{} s per min of 1080p", s.round() as u64),
        Some(s) => format!("~{} min per min of 1080p", (s / 60.0).round() as u64),
    }
}
```

Adapt these to the real code, and record each change as a Ruling:
- `Edits` may not implement a struct update this way, or `encoder` may live elsewhere. Read `crates/ferry-core/src/edit/mod.rs:97-125`.
- `Encoder` needs `Debug` for the file name. If it lacks it, use a `match`.
- The time `plan::plan` expects for 3 s at 30 fps comes from the probe, so no extra work is needed there.
- `hardware::hardware()` returns the default (no hardware) until `probe_hardware` has run at app start. In tests, Fast then uses software x264, which is fine.

- [ ] **Step 4: Run.** Run `cargo test -p ferry-core video::bench` with the env vars. Expected: 4 passed. The ffmpeg tests take a few seconds.

- [ ] **Step 5: Commit** `feat(video): time each encoder on a generated 1080p sample`.

---

### Task 3: The onboarding window and page

**Files:**
- Create:
  - `src-tauri/src/onboarding.rs`
  - `src/onboarding.html`, `src/onboarding/main.ts`, `src/onboarding/Onboarding.svelte`
  - `src/onboarding/choices.ts`, `src/onboarding/choices.test.ts`
- Modify:
  - `src-tauri/src/lib.rs` (`mod onboarding;`, register the commands, open the window at the end of setup)
  - `src-tauri/capabilities/default.json` (add `"onboarding"` to `windows`)
  - `vite.config.ts` (`onboarding: page('onboarding.html'),`)
  - `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `bench::{measure_all, label}`, `General.onboarded`, the `settings_get` and `settings_set` commands.
- Produces:
  - `onboarding::{ONBOARDING, open_if_needed(app)}`
  - commands `onboarding_measure()` and `onboarding_closed()`
  - event `onboarding:measured { encoder: 'fast'|'small'|'visually_lossless', label: string }`
  - TS:
    - `Choices = { keepOriginal: boolean; quality: number; encoder: Encoder; archive: 'replace'|'add' }`
    - `fromSettings(s): Choices`
    - `applyChoices(s, initial, c, includeShelf): Settings`

- [ ] **Step 1: Failing TS tests** `src/onboarding/choices.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { defaults } from '../lib/settings.fixture';
import { applyChoices, fromSettings } from './choices';

const base = structuredClone(defaults);

describe('onboarding choices', () => {
  it('start from the settings', () => {
    const c = fromSettings(base);
    expect(c).toEqual({ keepOriginal: false, quality: 95, encoder: base.video.encoder, archive: 'replace' });
  });

  it('keep the original sets both optimised locations to same_folder and marks onboarded', () => {
    const initial = fromSettings(base);
    const next = applyChoices(base, initial, { ...initial, keepOriginal: true, quality: 70, encoder: 'small', archive: 'add' }, true);
    expect(next.images.output.optimised.location).toBe('same_folder');
    expect(next.video.output.optimised.location).toBe('same_folder');
    expect(next.video.compression).toBe(70);
    expect(next.video.encoder).toBe('small');
    expect(next.floating.shelf_archive_result).toBe('add');
    expect(next.general.onboarded).toBe(true);
    expect(base.general.onboarded).toBe(false);
  });

  it('an_untouched_radio_keeps_a_custom_location', () => {
    const custom = structuredClone(base);
    custom.images.output.optimised.location = 'folder';
    const initial = fromSettings(custom);
    expect(initial.keepOriginal).toBe(false);
    const next = applyChoices(custom, initial, initial, true);
    expect(next.images.output.optimised.location).toBe('folder');
  });

  it('leaves the shelf setting alone without a shelf', () => {
    const initial = fromSettings(base);
    const next = applyChoices(base, initial, { ...initial, archive: 'add' }, false);
    expect(next.floating.shelf_archive_result).toBe('replace');
  });
});
```

Use the real export name of the fixture in `src/lib/settings.fixture.ts` (it may not be `defaults`), and the real `Encoder` and location type names in `src/lib/settings.ts`.

- [ ] **Step 2: Run it to see it fail.** Run `npx vitest run src/onboarding`. Expected: FAIL, because the module is missing.

- [ ] **Step 3: `src/onboarding/choices.ts`:**

```ts
import type { Settings } from '../lib/settings';

export type Encoder = Settings['video']['encoder'];
export type Choices = { keepOriginal: boolean; quality: number; encoder: Encoder; archive: 'replace' | 'add' };

export function fromSettings(s: Settings): Choices {
  return {
    keepOriginal: [s.images.output.optimised.location, s.video.output.optimised.location].includes('same_folder'),
    quality: s.video.compression,
    encoder: s.video.encoder,
    archive: s.floating.shelf_archive_result,
  };
}

/** The settings to save on Done. The location is only written when the radio was changed (spec §4). */
export function applyChoices(s: Settings, initial: Choices, c: Choices, includeShelf: boolean): Settings {
  const next = structuredClone(s);
  if (c.keepOriginal !== initial.keepOriginal) {
    const location = c.keepOriginal ? 'same_folder' : 'in_place';
    next.images.output.optimised.location = location;
    next.video.output.optimised.location = location;
  }
  next.video.compression = c.quality;
  next.video.encoder = c.encoder;
  if (includeShelf) next.floating.shelf_archive_result = c.archive;
  next.general.onboarded = true;
  return next;
}
```

Run `npx vitest run src/onboarding`. Expected: 4 passed.

- [ ] **Step 4: `src-tauri/src/onboarding.rs`.** Follow `windows::open_settings` and `shelf/archive_dialog.rs`:

```rust
//! The first-run screen (onboarding spec): opens until `general.onboarded`.

use std::sync::Mutex;

use ferry_core::cancel::CancelToken;
use ferry_core::i18n::t;
use ferry_core::optimise::Encoder;
use ferry_core::video::bench;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, WebviewUrl, WebviewWindowBuilder, WindowEvent};

use crate::state::AppState;

pub const ONBOARDING: &str = "onboarding";

static MEASURING: Mutex<Option<CancelToken>> = Mutex::new(None);

#[derive(Clone, Serialize)]
struct Measured {
    encoder: &'static str,
    label: String,
}

pub fn open_if_needed(app: &AppHandle) {
    let done = app.state::<AppState>().settings.read().unwrap().general.onboarded;
    if done || app.get_webview_window(ONBOARDING).is_some() {
        return;
    }
    let built = WebviewWindowBuilder::new(app, ONBOARDING, WebviewUrl::App("onboarding.html".into()))
        .title(t("onboarding.title"))
        .inner_size(520.0, 560.0)
        .resizable(false)
        .center()
        .build();
    match built {
        Ok(window) => {
            let handle = app.clone();
            window.on_window_event(move |event| {
                // The close button saves like Done: the page saves, then destroys the window.
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    stop();
                    let _ = handle.emit_to(ONBOARDING, "onboarding:closing", ());
                }
            });
            let _ = window.set_focus();
        }
        Err(e) => tracing::warn!("could not open onboarding: {e}"),
    }
}

fn stop() {
    if let Some(token) = MEASURING.lock().unwrap().take() {
        token.cancel();
    }
}

fn id(encoder: Encoder) -> &'static str {
    match encoder {
        Encoder::Fast => "fast",
        Encoder::Small => "small",
        Encoder::VisuallyLossless => "visually_lossless",
    }
}

/// Time each encoder off the main thread; each result is an `onboarding:measured` event.
#[tauri::command]
pub fn onboarding_measure(app: AppHandle) {
    stop();
    let token = CancelToken::new();
    *MEASURING.lock().unwrap() = Some(token.clone());
    let video = app.state::<AppState>().settings.read().unwrap().video.clone();
    std::thread::spawn(move || {
        bench::measure_all(&video, &token, &mut |encoder, secs| {
            let _ = app.emit_to(ONBOARDING, "onboarding:measured", Measured { encoder: id(encoder), label: bench::label(secs) });
        });
    });
}

/// Done was pressed and the settings are saved: stop measuring and close.
#[tauri::command]
pub fn onboarding_closed(app: AppHandle) {
    stop();
    if let Some(w) = app.get_webview_window(ONBOARDING) {
        let _ = w.destroy();
    }
}
```

**Closing the window:** the close button saves like Done (spec §4).
- The listener prevents the close and emits `onboarding:closing`.
- The page runs the same `done()` as the button. It saves, then calls `onboarding_closed`, which destroys the window.
- `done()` calls `onboarding_closed` in a `finally`, so a failed save can't leave the window stuck open.

**`lib.rs`:**
- Add `mod onboarding;`.
- Register `onboarding::onboarding_measure` and `onboarding::onboarding_closed`.
- At the end of setup, after `audio::startup();`, call `onboarding::open_if_needed(app);`.
- Check that `tracing` and `serde` are already dependencies of `src-tauri`; they are used in `lib.rs` and `shelf`.
- If `ferry_core::optimise::Encoder` is the settings encoder type, use it; otherwise use the type `settings::Video.encoder` really has.

- [ ] **Step 5: The page.**

`src/onboarding.html`: copy `src/settings.html`, with title `Welcome to Ferry` and script `/onboarding/main.ts`.

`src/onboarding/main.ts`: copy `src/settings/main.ts`, mounting `Onboarding.svelte`.

`Onboarding.svelte` (runes, `theme.css` variables, no `var`):

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { listen } from '@tauri-apps/api/event';
  import { onMount } from 'svelte';
  import { t } from '../lib/i18n';
  import { isMac } from '../lib/ipc';
  import type { Settings } from '../lib/settings';
  import { applyChoices, fromSettings, type Choices, type Encoder } from './choices';

  const ENCODERS: Encoder[] = ['fast', 'small', 'visually_lossless'];
  let settings = $state<Settings | null>(null);
  let initial: Choices | null = null;
  let c = $state<Choices | null>(null);
  let times = $state<Record<string, string>>({});
  let saving = false;

  onMount(() => {
    void invoke<Settings>('settings_get').then((s) => {
      settings = s;
      initial = fromSettings(s);
      c = { ...initial };
      void invoke('onboarding_measure');
    });
    const offs = [
      listen<{ encoder: string; label: string }>('onboarding:measured', (e) => (times[e.payload.encoder] = e.payload.label)),
      listen('onboarding:closing', () => void done()),
    ];
    return () => offs.forEach((p) => void p.then((fn) => fn()));
  });

  async function done() {
    if (saving || !settings || !initial || !c) return;
    saving = true;
    try {
      await invoke('settings_set', { settings: applyChoices(settings, initial, c, isMac) });
    } finally {
      await invoke('onboarding_closed');
    }
  }
</script>

<svelte:window onkeydown={(e) => e.key === 'Enter' && void done()} />
```

The markup has three sections and a footer, as spec §4 describes:

1. **`onboarding.original`**: two radio cards bound to `c.keepOriginal`.
   - Replace: `onboarding.replace`, with the note `t('onboarding.replace.note', { days: String(settings.backups.retention_days) })`.
   - Keep: `onboarding.keep`, with the note `onboarding.keep.note`.
2. **`onboarding.videos`**:
   - An `<input type="range" min="5" max="100">` bound to `c.quality`, with its value shown, and the help text `onboarding.quality.help`.
   - Then one radio row per entry in `ENCODERS`, bound to `c.encoder`. Each row shows `t('video.encoder.' + e)` and `t('video.encoder.' + e + '.help')` on the left, and on the right `times[e] ?? t('onboarding.measuring')`.
3. **`{#if isMac}` `onboarding.archive`**: two radios bound to `c.archive`, using the existing keys `floating.shelf_archive_result.replace` and `.add`.

**Footer:** a primary button, `onboarding.done`, that runs `done()`.

Render nothing until `settings && c`. If `t()` needs the key typed as `StringKey`, cast dynamic keys the same way `Archive.svelte` does for `archive.level.*`.

- [ ] **Step 6: Strings** in `en.json`:

```json
  "onboarding.title": "Welcome to Ferry",
  "onboarding.intro": "A few choices before you start. You can change them later in Settings.",
  "onboarding.original": "When Ferry optimises a file",
  "onboarding.replace": "Replace the original",
  "onboarding.replace.note": "A backup is kept for {days} days.",
  "onboarding.keep": "Keep the original",
  "onboarding.keep.note": "Saved next to it as photo_optimised.jpg.",
  "onboarding.videos": "Videos",
  "onboarding.quality": "Quality",
  "onboarding.quality.help": "Lower is smaller.",
  "onboarding.encoder": "Encoder",
  "onboarding.measuring": "Measuring…",
  "onboarding.archive": "Shelf archives",
  "onboarding.done": "Done",
```

Show `onboarding.intro` under the page heading (`onboarding.title`).

**Config:**
- `vite.config.ts`: add `onboarding: page('onboarding.html'),` to the inputs.
- `capabilities/default.json`: add `"onboarding"` to `windows`.

- [ ] **Step 7: Run.**
  - Run `npx vitest run src/onboarding && npx svelte-check`, then `cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry`.
  - Expected: everything clean and passing.
  - If a schema or i18n test checks that every `en.json` key is used, or that keys are sorted, follow it.

- [ ] **Step 8: Final gate** (Global Constraints), with the env vars. Expected: fmt, clippy, tests, svelte-check, vitest and vite build all clean.

- [ ] **Step 9: Commit** `feat: first-run onboarding with measured encoder times`.

---

## Manual checks for the user (not the implementer)

1. Launch the dev app. Because your settings file has no `onboarded` field, the window appears in front.
2. The three encoder times change from "Measuring…" to real times within about 10 s.
3. Choose "Keep the original", then Done. Settings ▸ File handling shows Same folder for images and videos, and the window doesn't come back on the next launch.
4. To see the window again, set `"onboarded": false` in `~/Library/Application Support/io.ferry.desktop/settings.json` and relaunch. Close it with the red button: the choices are saved the same way as Done.
