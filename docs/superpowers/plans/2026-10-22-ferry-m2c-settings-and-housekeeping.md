# Ferry M2c — Settings and Housekeeping Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give Ferry its Settings window (General, Clipboard, Images and Floating results panes, with search and live-applied changes) and the housekeeping around it: launch at login, the Dock icon, the ignored-apps picker, startup backup pruning, "Force clean working directory", the three tray icon styles and the macOS accent colour.

**Architecture:**
- **Decisions live in `ferry-core`** and are tested on three OSes: the accent-colour contrast fix (`color`), the clipboard file-name preview (`template`), settings clamping (`settings`), the working-directory clean (`workdir`) and the tray icon styles (`glyph`).
- **The app crate is thin glue:** `prefs` saves a change and applies what it touches (`Effects::between` decides), `commands` serves the Settings page, `tray` and `jobs` gain Force clean.
- **The Settings page is data-driven.** `schema.ts` lists every setting once (pane, label, help, control, dependency). One `Row` component renders any of them, and the same list feeds search and "Turn on … first".

This is milestone **M2c**, the last of three. M2a (engine) and M2b (core loop) are merged.

**Tech Stack:**
- Tauri 2.12 with the existing plugins, plus `tauri-plugin-autostart` 2.7.0 and `tauri-plugin-dialog` 2.8.3.
- macOS: `objc2-app-kit` 0.3.2 (`NSColor`, `NSColorSpace`) and `objc2-foundation` 0.3.2 (`NSBundle`).
- Frontend: Svelte 5 (runes, `.svelte.ts` stores) and vitest. No new npm packages.

**Spec:** `docs/superpowers/specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md` (approved). Read §3, §4.3, §5.5 to §5.7, §6 and §7 before starting. Requirements: `docs/requirements/ferry-optimize-requirements.md`, §4.12 (Settings window) and §4.1 (tray icon).

## Global Constraints

Every task's requirements include these. Values are copied from the spec.

**Settings window (§3, §4.3, §7)**
- "Settings window with the General, Clipboard, Images and Floating results panes, plus search."
- `settings` is "decorated, 820×600, resizable with a minimum of 720×480, and created on demand" (already true since M1).
- Changes apply instantly, with no Save button (R§4.12).
- Search "matches each whitespace-separated word against the label, help text and pane name. Picking a result jumps to it and highlights the row for 2 s."
- Empty search state: "No settings match".
- "Settings controls are real elements: `role="switch"` buttons, range inputs and labelled fields."
- Template field "shows its live example, or the red text "Unknown token %q" with `aria-invalid` set."
- "Dependent settings: when the setting they depend on is off, they grey out and show "Turn on '…' first.""
- `adaptive` and `switch_png_jpeg` "are stored and shown, but this cycle they make no difference to the output; their help text says "Coming soon"."
- UI text comes from `src/lib/i18n/en.json`; English only.

**Housekeeping (§3, §5.6, §5.7)**
- Launch at login uses the `autostart` plugin; the default is `launch_at_login: true`.
- Backups: "pruning by age"; the retention is `backups.retention_days` (default 7).
- "Force clean working directory" is a tray item (§5.6) and does not touch backups.
- Tray icon: "three styles (default, classic, flat)". Windows keeps its colour icon (Decision 6 of M2b).
- "Unknown fields are kept when the file is saved; missing fields take their defaults."
- Settings paths: `app_config_dir()/settings.json`; working directory `app_cache_dir()/work`; backups `app_data_dir()/backups`.

**Accent colour (§7)**
- "On macOS, Rust reads `NSColor.controlAccentColor` at startup and passes it to the UI as `--accent` (R§3.5). Windows uses `#0A64D8` this cycle."
- `--accent` is for "fills, selection, white text on top".

**Windows**
- Windows desktop behaviour is "built but not verified on a real desktop". Every status report says so.

**Project conventions**
- No `AGENTS.md` or `CONSTRAINTS.md` exists at the repo root (checked 2026-10-22).
- Reuse existing patterns:
  - each Rust module opens with a doc comment and ends with `#[cfg(test)] mod tests`;
  - `cargo fmt`, and clippy with `-D warnings`;
  - UI text from `src/lib/i18n/en.json`;
  - no `var` in JavaScript or TypeScript.

## Decisions made while planning

These are mine, not the spec's. Each says what it costs to change.

1. **The Settings page is data-driven** (`src/lib/schema.ts`). One list of settings drives the panes, search and dependents, so a setting cannot appear in one and be missing from another. *If wrong:* hand-written panes (the spec's `panes/*.svelte`) and a separate search index.
2. **Storage controls live in General.** The retention days, "Open backups folder", "Open working directory" and "Force clean" sit in a "Storage" group at the bottom of General, because the spec names no pane for them (R§4.12 puts them in a File handling pane that is out of scope). *If wrong:* move four schema entries.
3. **Changes are sent whole and applied under one lock.** The page sends the full `Settings` object (unknown fields included) to `settings_set`; Rust sanitises, saves atomically, applies side effects and answers with the saved settings. A `settings:changed` event keeps every window, and the tray's own toggle, in step. *If wrong:* send patches.
4. **Launch at login is never touched in debug builds.** The default is on, so a `pnpm tauri dev` run would otherwise register a login item pointing into `target/debug`. *If wrong:* remove one `if cfg!(debug_assertions)`.
5. **The tray styles are made from the shipped logo, not new art.** The logo is "F." with a dot, not the spec's ferry hull, so:
   - *Default* is the logo as drawn;
   - *Classic* keeps only the rim of each shape (an outline);
   - *Flat* is a solid rounded badge with the logo cut out.
   - The state glyphs (Paused, Working, Error, Stopped) are drawn first and styled after, so every state exists in every style. The styles apply on macOS only; the picker is hidden on Windows. *If wrong:* replace three code paths in `glyph.rs` with art.
6. **The accent colour is darkened until white text on it reaches 3:1 contrast** (WCAG's bar for UI components). A user with a yellow accent would otherwise get white-on-yellow buttons. Dark mode lightens the accent text with `color-mix`. *If wrong:* delete `color.rs`.
7. **The ignored-apps picker takes an application, not a running-app list.** A native file dialog picks `Foo.app` (macOS) or `foo.exe` (Windows); Rust reads the bundle identifier (macOS) or file name (Windows), the same strings `Snapshot::source_app` carries. "Add from running apps" (R§4.12) needs a per-OS list and is left out. *If wrong:* one more platform function.
8. **Force clean keeps what is still in use.** It deletes everything under the working directory except the files of queued or running jobs, forgets the finished jobs (their cards and Recent results), and clears the stack. Backups are untouched. *If wrong:* keep the idle jobs' files too.
9. **Hotkeys, import/export, sync, language choice and the log viewer are not in Settings.** The spec puts the hotkey editor out of scope; the others are R§4.12 items outside the four panes. Language shows English only. *If wrong:* later milestones.
10. **No live preview strip for Floating results.** R§4.12 asks for one; the approved spec's §3 lists only the panes and search. *If wrong:* a later task.
11. **Startup prune runs on its own thread** right after settings load, so a large backups folder cannot delay the tray. *If wrong:* run it inline.

## Review Focus

Failure modes the spec implies but no spec test names, most likely first. Each has a test in the task that owns the code.

1. **A value outside its range reaches the file** (compression 0, retention 0, an hour-long auto-hide typed as 99999999).
   - Task 2: `out_of_range_values_are_clamped_and_blank_list_entries_dropped`.
2. **Force clean deletes something it should not:** a running job's file, a folder outside the working directory, or the target of a symlink.
   - Task 3: `a_running_jobs_file_and_its_folder_stay`, `a_missing_working_directory_is_already_clean`, `a_symlink_is_removed_not_followed`.
3. **An accent colour white text cannot sit on** (yellow, light green).
   - Task 1: `a_light_accent_is_darkened_until_white_text_is_readable`.
4. **A setting changes and its side effect does not** (screenshot protection, Dock icon, login item, tray icon), or the reverse: an unrelated change re-registers the login item.
   - Task 6: `each_change_touches_only_its_own_effects`.
5. **Unknown settings fields are lost when the page saves.**
   - Task 5: `changing_one_path_keeps_every_other_field_and_unknown_ones`.

## File Map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/color.rs` | `accent_hex` | 1 |
| `crates/ferry-core/src/template.rs`, `pipeline.rs` (modify) | `clipboard_name`, `preview` | 2 |
| `crates/ferry-core/src/settings.rs` (modify) | `sanitised` | 2 |
| `crates/ferry-core/src/workdir.rs` | `clean`, `CleanReport` | 3 |
| `crates/ferry-core/src/glyph.rs` (modify) | `render_styled`, `to_png` | 4 |
| `crates/ferry-core/src/lib.rs` | Module list | 1, 3 |
| `src/lib/settings.ts`, `fuzzy.ts`, `schema.ts` (+ tests), `i18n/en.json` | Settings types, search, the list of settings, strings | 5 |
| `src-tauri/Cargo.toml`, `capabilities/default.json` | Plugins, features | 6 |
| `src-tauri/src/platform/{mod,macos,windows}.rs` | `accent_rgb`, `app_identifier` | 6 |
| `src-tauri/src/prefs.rs` | `Effects`, `update`, `sync_autostart`, `set_dock` | 6 |
| `src-tauri/src/lib.rs` | Plugins, housekeeping at startup, command list | 6, 7 |
| `src-tauri/src/commands.rs`, `jobs.rs`, `tray.rs` | Settings commands, Force clean, tray styles | 7 |
| `src/settings/{store.svelte.ts,Row.svelte,Settings.svelte,main.ts}`, `src/settings/controls/*.svelte` | The Settings page | 8, 9 |
| `src/lib/{ipc.ts,theme.css}`, `src/stack/Stack.svelte` | Accent, live floating settings, clear event | 9 |

## How to run things

Every Bash call needs the Rust environment and Node 22:

```bash
source "$HOME/.cargo/env"
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
```

Run development builds under their own identifier, because the installed `/Applications/Ferry.app` may hold the single-instance lock:

```bash
pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'
```

Tasks 1 to 4 add core modules that nothing calls until Tasks 6 to 9. `ferry-core` is a library, so unused public functions do not warn.

Steps that say "add this test module" create the file with only that module; the next step adds the code **above** it.

---

### Task 1: The accent colour

**Files:**
- Create: `crates/ferry-core/src/color.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Produces: `ferry_core::color::accent_hex(rgb: (u8, u8, u8)) -> String`, a `#rrggbb` string whose contrast against white is at least 3:1.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/color.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn parse(hex: &str) -> (u8, u8, u8) {
        let n = |i| u8::from_str_radix(&hex[i..i + 2], 16).unwrap();
        (n(1), n(3), n(5))
    }

    #[test]
    fn a_dark_enough_accent_is_kept() {
        assert_eq!(accent_hex((10, 100, 216)), "#0a64d8");
        assert_eq!(accent_hex((0, 0, 0)), "#000000");
    }

    #[test]
    fn a_light_accent_is_darkened_until_white_text_is_readable() {
        for rgb in [(255, 204, 0), (152, 255, 152), (255, 255, 255)] {
            let fixed = parse(&accent_hex(rgb));
            assert!(
                contrast_with_white(fixed) >= 3.0,
                "{rgb:?} became {fixed:?}"
            );
        }
    }

    #[test]
    fn darkening_keeps_the_hue() {
        let (r, g, b) = parse(&accent_hex((255, 204, 0)));
        assert!(r > g && g > b, "yellow stays yellow: {r} {g} {b}");
    }
}
```

Add `pub mod color;` to `crates/ferry-core/src/lib.rs` after `pub mod cancel;`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core color`
Expected: compile error, "cannot find function `accent_hex`".

- [ ] **Step 3: Implement**

Add above the test module:

```rust
//! The macOS accent colour, made safe for white text: the UI puts white text
//! on `--accent` fills (spec §7), and a yellow or light green accent would
//! make it unreadable.

/// WCAG's minimum contrast for user-interface components.
const MIN_CONTRAST: f64 = 3.0;

fn linear(channel: u8) -> f64 {
    let c = f64::from(channel) / 255.0;
    if c <= 0.03928 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn contrast_with_white((r, g, b): (u8, u8, u8)) -> f64 {
    let luminance = 0.2126 * linear(r) + 0.7152 * linear(g) + 0.0722 * linear(b);
    1.05 / (luminance + 0.05)
}

/// `rgb` as `#rrggbb`, darkened in 10% steps until white text on it has at
/// least 3:1 contrast. Black always passes, so this ends.
pub fn accent_hex(mut rgb: (u8, u8, u8)) -> String {
    while contrast_with_white(rgb) < MIN_CONTRAST {
        let darker = |c: u8| (u16::from(c) * 9 / 10) as u8;
        rgb = (darker(rgb.0), darker(rgb.1), darker(rgb.2));
    }
    format!("#{:02x}{:02x}{:02x}", rgb.0, rgb.1, rgb.2)
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core color`
Expected: PASS, 3 tests.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): accent colour that white text can sit on"
```

---

### Task 2: File-name preview and settings clamping

**Files:**
- Modify: `crates/ferry-core/src/template.rs`, `crates/ferry-core/src/pipeline.rs`, `crates/ferry-core/src/settings.rs`

**Interfaces:**
- Consumes: `i18n::{t, tf}` (Task 1 of M2b).
- Produces:
  - `template::clipboard_name(tpl: &str, ext: &str, dims: (u32, u32), now: NaiveDateTime) -> String`: the file name without extension that a clipboard image gets; `Job::clipboard` now calls it.
  - `template::preview(tpl: &str) -> Result<String, Vec<String>>`: the Settings window's live example, with the extension; `Err` lists the unknown tokens.
  - `Settings::sanitised(self) -> Settings` (was the private `clamped`).

- [ ] **Step 1: Write the failing tests**

Add these to `mod tests` in `crates/ferry-core/src/template.rs`, after `use super::*;`:

```rust
    fn at() -> chrono::NaiveDateTime {
        NaiveDate::from_ymd_opt(2026, 10, 22)
            .unwrap()
            .and_hms_opt(14, 23, 11)
            .unwrap()
    }

    #[test]
    fn a_clipboard_image_is_named_like_a_macos_screenshot() {
        assert_eq!(
            clipboard_name("%f", "png", (1920, 1080), at()),
            "Clipboard 2026-10-22 at 14.23.11"
        );
        assert_eq!(
            clipboard_name("%P_%z", "png", (1920, 1080), at()),
            "Clipboard_1920x1080"
        );
    }

    #[test]
    fn a_clipboard_name_is_safe_as_a_file_name() {
        assert_eq!(clipboard_name("a/b", "png", (1, 1), at()), "a_b");
    }

    #[test]
    fn the_preview_adds_the_extension_and_refuses_unknown_tokens() {
        let example = preview("%f_%z").unwrap();
        assert!(example.starts_with("Clipboard "), "{example}");
        assert!(example.ends_with("_1920x1080.png"), "{example}");
        assert_eq!(preview("%f_%q"), Err(vec!["%q".to_string()]));
    }
```

Add this to `mod tests` in `crates/ferry-core/src/settings.rs`, at the end of the module:

```rust
    #[test]
    fn out_of_range_values_are_clamped_and_blank_list_entries_dropped() {
        let mut s = Settings::default();
        s.images.compression = 0;
        s.images.max_size_mb = 0;
        s.clipboard.max_size_mb = 0;
        s.floating.auto_hide_secs = 99_999_999;
        s.backups.retention_days = 0;
        s.images.ignored_extensions = vec![" png ".into(), "".into(), "  ".into()];
        s.clipboard.ignored_apps = vec!["".into(), "com.example.App".into()];
        let s = s.sanitised();
        assert_eq!(s.images.compression, 5);
        assert_eq!((s.images.max_size_mb, s.clipboard.max_size_mb), (1, 1));
        assert_eq!(s.floating.auto_hide_secs, 3600);
        assert_eq!(s.backups.retention_days, 1);
        assert_eq!(s.images.ignored_extensions, vec!["png".to_string()]);
        assert_eq!(s.clipboard.ignored_apps, vec!["com.example.App".to_string()]);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core template settings`
Expected: compile errors, "cannot find function `clipboard_name`" and "no method named `sanitised`".

- [ ] **Step 3: Implement**

In `crates/ferry-core/src/template.rs`, change `use chrono::{Datelike, NaiveDate};` to:

```rust
use chrono::{Datelike, NaiveDate, NaiveDateTime};

use crate::i18n::{t, tf};
```

Add this above `#[cfg(test)]`:

```rust
/// The file name, without extension, that a clipboard image gets: `tpl` with
/// the "Clipboard <date> at <time>" stem, made safe for the file system.
pub fn clipboard_name(
    tpl: &str,
    ext: &str,
    (width, height): (u32, u32),
    now: NaiveDateTime,
) -> String {
    let date = now.format("%Y-%m-%d").to_string();
    let time = now.format("%H.%M.%S").to_string();
    let stem = tf("clipboard.name", &[("date", &date), ("time", &time)]);
    let ctx = Ctx {
        stem: &stem,
        ext,
        width,
        height,
        parent: &t("clipboard.folder"),
        date: now.date(),
    };
    safe_file_name(&render(tpl, &ctx))
}

/// The Settings window's live example: `tpl` for a 1920×1080 PNG copied now.
/// `Err` lists the unknown tokens, so the field can say what is wrong.
pub fn preview(tpl: &str) -> Result<String, Vec<String>> {
    validate(tpl)?;
    let now = chrono::Local::now().naive_local();
    Ok(format!(
        "{}.png",
        clipboard_name(tpl, "png", (1920, 1080), now)
    ))
}
```

In `crates/ferry-core/src/pipeline.rs`, replace this block in `Job::clipboard`:

```rust
        let date = now.format("%Y-%m-%d").to_string();
        let time = now.format("%H.%M.%S").to_string();
        let stem = tf("clipboard.name", &[("date", &date), ("time", &time)]);
        let (width, height) = dims.unwrap_or_default();
        let ctx = template::Ctx {
            stem: &stem,
            ext,
            width,
            height,
            parent: &t("clipboard.folder"),
            date: now.date(),
        };
        let name = template::safe_file_name(&template::render(file_template, &ctx));
```

with:

```rust
        let name = template::clipboard_name(file_template, ext, dims.unwrap_or_default(), now);
```

If the compiler then reports an unused `t` or `tf` import in `pipeline.rs`, remove just that name from its `use ferry_core::i18n` line.

In `crates/ferry-core/src/settings.rs`, replace the `clamped` function (and its doc comment) with:

```rust
    /// A hand-edited file, or a typo in the Settings window, must not turn the
    /// encoders, the size limits or the housekeeping into nonsense.
    pub fn sanitised(mut self) -> Self {
        self.images.compression = self.images.compression.clamp(5, 100);
        self.images.max_size_mb = self.images.max_size_mb.max(1);
        self.clipboard.max_size_mb = self.clipboard.max_size_mb.max(1);
        self.floating.auto_hide_secs = self.floating.auto_hide_secs.min(3600);
        self.backups.retention_days = self.backups.retention_days.clamp(1, 3650);
        for list in [
            &mut self.images.ignored_extensions,
            &mut self.clipboard.ignored_apps,
        ] {
            *list = list
                .iter()
                .map(|item| item.trim().to_string())
                .filter(|item| !item.is_empty())
                .collect();
        }
        self
    }
```

Then change its one caller in `Settings::load`: `Ok(settings) => (settings.clamped(), LoadStatus::Loaded),` becomes `Ok(settings) => (settings.sanitised(), LoadStatus::Loaded),`.

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: all pass (the existing pipeline and settings tests still pass), and no warnings. `cargo test -p ferry-core template` shows 3 new tests.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): clipboard file-name preview and stricter settings clamping"
```

---

### Task 3: Cleaning the working directory

**Files:**
- Create: `crates/ferry-core/src/workdir.rs`
- Modify: `crates/ferry-core/src/lib.rs`

**Interfaces:**
- Produces, in `ferry_core::workdir`:
  - `CleanReport { files: u32, bytes: u64 }` (`Debug, Default, PartialEq, Eq`);
  - `clean(root: &Path, keep: &[PathBuf]) -> io::Result<CleanReport>`: deletes everything under `root` except the paths in `keep` and the folders that lead to them. It never deletes `root` itself, and never follows a symlink.

- [ ] **Step 1: Write the failing tests**

Create `crates/ferry-core/src/workdir.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn file(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![0; bytes]).unwrap();
    }

    #[test]
    fn finished_jobs_files_and_folders_go() {
        let dir = tempfile::tempdir().unwrap();
        file(&dir.path().join("clipboard/job-1/a.png"), 10);
        file(&dir.path().join("clipboard/job-2/b.png"), 5);
        file(&dir.path().join("loose.txt"), 1);
        let report = clean(dir.path(), &[]).unwrap();
        assert_eq!(report, CleanReport { files: 3, bytes: 16 });
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 0);
        assert!(dir.path().exists(), "the working directory itself stays");
    }

    #[test]
    fn a_running_jobs_file_and_its_folder_stay() {
        let dir = tempfile::tempdir().unwrap();
        let busy = dir.path().join("clipboard/job-1/a.png");
        file(&busy, 10);
        file(&dir.path().join("clipboard/job-1/stale.tmp"), 2);
        file(&dir.path().join("clipboard/job-2/b.png"), 5);
        let report = clean(dir.path(), std::slice::from_ref(&busy)).unwrap();
        assert!(busy.exists());
        assert!(!dir.path().join("clipboard/job-1/stale.tmp").exists());
        assert!(!dir.path().join("clipboard/job-2").exists());
        assert_eq!(report, CleanReport { files: 2, bytes: 7 });
    }

    #[test]
    fn a_missing_working_directory_is_already_clean() {
        let dir = tempfile::tempdir().unwrap();
        let report = clean(&dir.path().join("never-created"), &[]).unwrap();
        assert_eq!(report, CleanReport::default());
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_removed_not_followed() {
        let outside = tempfile::tempdir().unwrap();
        file(&outside.path().join("precious.txt"), 3);
        let dir = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), dir.path().join("link")).unwrap();
        clean(dir.path(), &[]).unwrap();
        assert!(!dir.path().join("link").exists());
        assert!(outside.path().join("precious.txt").exists());
    }
}
```

Add `pub mod workdir;` to `crates/ferry-core/src/lib.rs` after `pub mod template;` and before `pub mod trigger;`.

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core workdir`
Expected: compile error, "cannot find type `CleanReport`".

- [ ] **Step 3: Implement**

Add above the test module:

```rust
//! "Force clean working directory" (spec §3, §5.6): finished jobs' working
//! files go; files that queued or running jobs still use stay.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// What a clean removed.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct CleanReport {
    pub files: u32,
    pub bytes: u64,
}

/// Delete everything under `root` except the paths in `keep` and the folders
/// that lead to them. `root` itself stays, and a missing `root` is already
/// clean. A symlink is removed as a link; what it points to is never touched.
pub fn clean(root: &Path, keep: &[PathBuf]) -> io::Result<CleanReport> {
    let mut report = CleanReport::default();
    match clean_dir(root, keep, &mut report) {
        Ok(()) => Ok(report),
        Err(e) if e.kind() == io::ErrorKind::NotFound && !root.exists() => Ok(report),
        Err(e) => Err(e),
    }
}

fn clean_dir(dir: &Path, keep: &[PathBuf], report: &mut CleanReport) -> io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if keep.iter().any(|k| *k == path) {
            continue;
        }
        if entry.file_type()?.is_dir() {
            if keep.iter().any(|k| k.starts_with(&path)) {
                clean_dir(&path, keep, report)?;
            } else {
                tally(&path, report);
                fs::remove_dir_all(&path)?;
            }
        } else {
            report.files += 1;
            report.bytes += entry.metadata().map_or(0, |m| m.len());
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// Count what `remove_dir_all` is about to delete.
fn tally(dir: &Path, report: &mut CleanReport) {
    let Ok(entries) = fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        match entry.file_type() {
            Ok(kind) if kind.is_dir() => tally(&entry.path(), report),
            _ => {
                report.files += 1;
                report.bytes += entry.metadata().map_or(0, |m| m.len());
            }
        }
    }
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core workdir && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: PASS, 4 tests on macOS (3 on Windows, where the symlink test is skipped), and no warnings.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): clean the working directory without touching jobs still running"
```

---

### Task 4: Tray icon styles

**Files:**
- Modify: `crates/ferry-core/src/glyph.rs`

**Interfaces:**
- Consumes: `settings::TrayStyle { Default, Classic, Flat }` (M2a).
- Produces, in `ferry_core::glyph`:
  - `render_styled(base: &[u8], size: u32, glyph: TrayGlyph, style: TrayStyle) -> Vec<u8>`: draws the state glyph, then the style, then (for Stopped) the dimming. `render(base, size, glyph)` stays and means `Default`.
  - `to_png(rgba: &[u8], size: u32) -> Vec<u8>`: a square RGBA image as PNG, for the Settings thumbnails.

- [ ] **Step 1: Write the failing tests**

Add these to `mod tests` in `crates/ferry-core/src/glyph.rs`, at the end of the module (the `logo` and `alpha` helpers are already there):

```rust
    #[test]
    fn the_default_style_is_render_itself() {
        for glyph in [TrayGlyph::Running, TrayGlyph::Paused, TrayGlyph::Stopped] {
            assert_eq!(
                render_styled(&logo(64), 64, glyph, TrayStyle::Default),
                render(&logo(64), 64, glyph)
            );
        }
    }

    #[test]
    fn classic_keeps_only_the_rim_of_a_shape() {
        let px = render_styled(&logo(64), 64, TrayGlyph::Running, TrayStyle::Classic);
        assert_eq!(alpha(&px, 64, 33, 45), 255, "the rim");
        assert_eq!(alpha(&px, 64, 40, 52), 0, "the hollow middle");
        assert_eq!(alpha(&px, 64, 10, 10), 0, "outside the shape");
    }

    #[test]
    fn flat_cuts_the_logo_out_of_a_solid_badge() {
        let px = render_styled(&logo(64), 64, TrayGlyph::Running, TrayStyle::Flat);
        assert_eq!(alpha(&px, 64, 40, 52), 0, "the dot is cut out");
        assert_eq!(alpha(&px, 64, 10, 10), 255, "the badge");
        assert_eq!(alpha(&px, 64, 0, 0), 0, "outside the rounded corner");
    }

    #[test]
    fn a_state_glyph_shows_in_every_style() {
        // Paused draws a gap between two bars; each style must keep it visible.
        let plain = render_styled(&logo(64), 64, TrayGlyph::Paused, TrayStyle::Default);
        for style in [TrayStyle::Classic, TrayStyle::Flat] {
            let styled = render_styled(&logo(64), 64, TrayGlyph::Paused, style);
            assert_ne!(styled, render_styled(&logo(64), 64, TrayGlyph::Running, style));
            assert_ne!(styled, plain);
        }
    }

    #[test]
    fn stopped_dims_a_styled_icon_too() {
        let px = render_styled(&logo(64), 64, TrayGlyph::Stopped, TrayStyle::Flat);
        assert!(px.as_chunks::<4>().0.iter().all(|p| p[3] <= 102));
        assert_eq!(alpha(&px, 64, 10, 10), 102);
    }

    #[test]
    fn a_png_round_trips() {
        let png = to_png(&logo(16), 16);
        let decoder = png::Decoder::new(std::io::Cursor::new(png));
        let mut reader = decoder.read_info().unwrap();
        let mut buf = vec![0; reader.output_buffer_size().unwrap()];
        let info = reader.next_frame(&mut buf).unwrap();
        assert_eq!((info.width, info.height), (16, 16));
        assert_eq!(&buf[..info.buffer_size()], &logo(16)[..]);
    }
```

- [ ] **Step 2: Run them to see them fail**

Run: `cargo test -p ferry-core glyph`
Expected: compile errors, "cannot find function `render_styled`" and "cannot find function `to_png`".

- [ ] **Step 3: Implement**

In `crates/ferry-core/src/glyph.rs`, add this after the module's doc comment:

```rust
use crate::settings::TrayStyle;
```

Replace the whole `render` function (from its doc comment to its closing brace) with:

```rust
/// Draw `glyph` over `base`, a square RGBA image `size` pixels wide, in the
/// default style.
pub fn render(base: &[u8], size: u32, glyph: TrayGlyph) -> Vec<u8> {
    render_styled(base, size, glyph, TrayStyle::Default)
}

/// Draw `glyph` over `base`, then give it the look of `style`. The state is
/// drawn first so every state exists in every style; dimming comes last so a
/// Stopped icon is faded whatever its style.
pub fn render_styled(base: &[u8], size: u32, glyph: TrayGlyph, style: TrayStyle) -> Vec<u8> {
    let mut out = base.to_vec();
    let scale = size as f32 / 64.0;
    let mut fill = |x0: f32, y0: f32, x1: f32, y1: f32, alpha: u8| {
        let (x0, y0) = ((x0 * scale).round() as u32, (y0 * scale).round() as u32);
        let (x1, y1) = ((x1 * scale).round() as u32, (y1 * scale).round() as u32);
        for y in y0..y1.min(size) {
            for x in x0..x1.min(size) {
                let i = ((y * size + x) * 4) as usize;
                out[i..i + 4].copy_from_slice(&[0, 0, 0, alpha]);
            }
        }
    };
    let (x0, y0, x1, y1) = DOT;
    match glyph {
        TrayGlyph::Running | TrayGlyph::Stopped => {}
        TrayGlyph::Paused => {
            fill(x0, y0, x1, y1, 0);
            fill(x0, y0, x0 + 6.0, y1, 255);
            fill(x1 - 6.0, y0, x1, y1, 255);
        }
        TrayGlyph::Working => {
            fill(x0, y0, x1, y1, 255);
            fill(x0 + 4.0, y0 + 4.0, x1 - 4.0, y1 - 4.0, 0);
        }
        TrayGlyph::Error => {
            fill(x0, y0, x1, y1, 0);
            fill(x0 + 5.0, y0, x1 - 5.0, y1 - 6.0, 255);
            fill(x0 + 5.0, y1 - 4.0, x1 - 5.0, y1, 255);
        }
    }
    let mut out = match style {
        TrayStyle::Default => out,
        TrayStyle::Classic => outline(&out, size),
        TrayStyle::Flat => badge(&out, size),
    };
    if glyph == TrayGlyph::Stopped {
        for pixel in out.as_chunks_mut::<4>().0 {
            pixel[3] = (f32::from(pixel[3]) * STOPPED_ALPHA).round() as u8;
        }
    }
    out
}

fn is_solid(px: &[u8], size: u32, x: i64, y: i64) -> bool {
    let size = i64::from(size);
    if x < 0 || y < 0 || x >= size || y >= size {
        return false;
    }
    px[((y * size + x) * 4 + 3) as usize] >= 128
}

/// Classic: only the rim of each shape, 1/32 of the icon wide (one pixel at least).
fn outline(px: &[u8], size: u32) -> Vec<u8> {
    let rim = i64::from((size / 32).max(1));
    let mut out = vec![0; px.len()];
    for y in 0..i64::from(size) {
        for x in 0..i64::from(size) {
            if !is_solid(px, size, x, y) {
                continue;
            }
            let on_rim = (-rim..=rim)
                .any(|dy| (-rim..=rim).any(|dx| !is_solid(px, size, x + dx, y + dy)));
            if on_rim {
                out[((y * i64::from(size) + x) * 4 + 3) as usize] = 255;
            }
        }
    }
    out
}

/// Flat: a solid rounded badge with the shapes cut out of it.
fn badge(px: &[u8], size: u32) -> Vec<u8> {
    let scale = size as f32 / 64.0;
    let (margin, radius) = (2.0 * scale, 10.0 * scale);
    let (low, high) = (margin + radius, size as f32 - margin - radius);
    let mut out = vec![0; px.len()];
    for y in 0..size {
        for x in 0..size {
            let (cx, cy) = (x as f32 + 0.5, y as f32 + 0.5);
            let (nx, ny) = (cx.clamp(low, high), cy.clamp(low, high));
            if (cx - nx).hypot(cy - ny) <= radius {
                let i = ((y * size + x) * 4) as usize;
                out[i + 3] = 255 - px[i + 3];
            }
        }
    }
    out
}

/// Encode a square RGBA image as PNG (the Settings window's style thumbnails).
pub fn to_png(rgba: &[u8], size: u32) -> Vec<u8> {
    let mut out = Vec::new();
    let mut encoder = png::Encoder::new(&mut out, size, size);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_depth(png::BitDepth::Eight);
    encoder
        .write_header()
        .and_then(|mut writer| writer.write_image_data(rgba))
        .expect("writing a PNG to memory cannot fail");
    out
}
```

- [ ] **Step 4: Run them to see them pass**

Run: `cargo test -p ferry-core glyph && cargo clippy -p ferry-core --all-targets -- -D warnings`
Expected: PASS, 12 tests (the 6 from M2b plus 6), and no warnings. The existing M2b glyph tests prove `render` still behaves as before.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src
git commit -m "feat(core): classic and flat tray icon styles"
```

---

### Task 5: Settings types, search and the list of settings

**Files:**
- Create: `src/lib/settings.ts`, `src/lib/settings.test.ts`, `src/lib/fuzzy.ts`, `src/lib/fuzzy.test.ts`, `src/lib/schema.ts`, `src/lib/schema.test.ts`
- Modify: `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `t` and `StringKey` from `src/lib/i18n.ts`.
- Produces:
  - `settings.ts`: `Settings` (mirrors `ferry_core::settings::Settings`), `getPath(settings, path)`, `withPath(settings, path, value)` (immutable).
  - `fuzzy.ts`: `matches(query: string, ...fields: string[]): boolean`.
  - `schema.ts`: `PaneId`, `Control`, `Setting`, `panes`, `settingsFor(isMac: boolean): Setting[]`.
  - Every Settings string in `en.json`.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/settings.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { getPath, withPath, type Settings } from './settings';

export const sample: Settings = {
  version: 1,
  general: {
    launch_at_login: true,
    start_state: 'running',
    show_dock_icon: false,
    show_in_screenshots: false,
    tray_style: 'default',
    language: 'en',
  },
  clipboard: {
    enabled: true,
    skip_multiple_files: true,
    max_size_mb: 50,
    ignored_apps: [],
    accumulate: false,
    copy_paths: false,
    file_template: '%f',
  },
  images: {
    compression: 80,
    adaptive: false,
    switch_png_jpeg: false,
    strip_metadata: true,
    preserve_dates: true,
    ignored_extensions: [],
    max_size_mb: 50,
  },
  floating: {
    enabled: true,
    corner: 'bottom_right',
    card_size: 'default',
    auto_hide_secs: 6,
    follow_cursor: true,
  },
  backups: { retention_days: 7 },
  hotkeys: { optimise: 'Ctrl+Shift+C', aggressive: 'Ctrl+Shift+A', cycle_state: 'Ctrl+Shift+P' },
};

describe('settings paths', () => {
  it('reads a nested value', () => {
    expect(getPath(sample, 'images.compression')).toBe(80);
    expect(getPath(sample, 'nope.nothing')).toBeUndefined();
  });

  it('changing one path keeps every other field and unknown ones', () => {
    const withUnknown = { ...sample, future: { flag: true } } as Settings;
    const next = withPath(withUnknown, 'images.compression', 55);
    expect(getPath(next, 'images.compression')).toBe(55);
    expect({ ...next, images: { ...next.images, compression: 80 } }).toEqual(withUnknown);
    expect(getPath(sample, 'images.compression')).toBe(80);
  });
});
```

Create `src/lib/fuzzy.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { matches } from './fuzzy';

describe('matches', () => {
  it('needs every word somewhere in the fields, in any case', () => {
    expect(matches('LAUNCH login', 'Launch at login', 'Start Ferry when you sign in.', 'General')).toBe(true);
    expect(matches('launch dock', 'Launch at login', 'Start Ferry', 'General')).toBe(false);
  });

  it('looks in the help text and the pane name too', () => {
    expect(matches('sign', 'Launch at login', 'Start Ferry when you sign in.', 'General')).toBe(true);
    expect(matches('general', 'Launch at login', '', 'General')).toBe(true);
  });

  it('matches everything for a blank query', () => {
    expect(matches('   ', 'Anything')).toBe(true);
  });
});
```

Create `src/lib/schema.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { t } from './i18n';
import { panes, settingsFor } from './schema';
import { getPath } from './settings';
import { sample } from './settings.test';

describe('the list of settings', () => {
  const all = settingsFor(true);

  it('has unique ids', () => {
    const ids = all.map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('points every value setting at a field that exists', () => {
    for (const s of all) {
      if (s.path) expect(getPath(sample, s.path), s.id).not.toBeUndefined();
    }
  });

  it('only depends on switches that exist', () => {
    for (const s of all) {
      if (!s.depends_on) continue;
      const parent = all.find((p) => p.path === s.depends_on);
      expect(parent?.control.kind, `${s.id} depends on ${s.depends_on}`).toBe('switch');
    }
  });

  it('has real strings for every label and help text', () => {
    for (const s of all) {
      expect(t(s.label), s.id).not.toBe('');
      if (s.help) expect(t(s.help), s.id).not.toBe('');
    }
    for (const p of panes) expect(t(p.label)).not.toBe('');
  });

  it('hides the macOS-only settings on Windows', () => {
    expect(settingsFor(false).some((s) => s.mac_only)).toBe(false);
    expect(all.some((s) => s.mac_only)).toBe(true);
  });

  it('puts every setting in a pane that exists', () => {
    const ids = panes.map((p) => p.id);
    for (const s of all) expect(ids).toContain(s.pane);
  });
});
```

- [ ] **Step 2: Run them to see them fail**

Run: `pnpm test`
Expected: FAIL, with "Failed to resolve import './settings'" (and the same for `./fuzzy` and `./schema`).

- [ ] **Step 3: Implement the types, search and list**

Create `src/lib/settings.ts`:

```ts
/** Mirrors ferry_core::settings::Settings (serde snake_case). Unknown fields from a newer file ride along untouched. */
export interface Settings {
  version: number;
  general: {
    launch_at_login: boolean;
    start_state: 'running' | 'paused' | 'stopped';
    show_dock_icon: boolean;
    show_in_screenshots: boolean;
    tray_style: 'default' | 'classic' | 'flat';
    language: string;
  };
  clipboard: {
    enabled: boolean;
    skip_multiple_files: boolean;
    max_size_mb: number;
    ignored_apps: string[];
    accumulate: boolean;
    copy_paths: boolean;
    file_template: string;
  };
  images: {
    compression: number;
    adaptive: boolean;
    switch_png_jpeg: boolean;
    strip_metadata: boolean;
    preserve_dates: boolean;
    ignored_extensions: string[];
    max_size_mb: number;
  };
  floating: {
    enabled: boolean;
    corner: 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';
    card_size: 'default' | 'compact';
    auto_hide_secs: number;
    follow_cursor: boolean;
  };
  backups: { retention_days: number };
  hotkeys: { optimise: string; aggressive: string; cycle_state: string };
}

/** The value at a dotted path such as `images.compression`, or undefined. */
export function getPath(settings: Settings, path: string): unknown {
  let node: unknown = settings;
  for (const key of path.split('.')) {
    if (typeof node !== 'object' || node === null) return undefined;
    node = (node as Record<string, unknown>)[key];
  }
  return node;
}

/** A copy of `settings` with the value at `path` replaced; everything else, unknown fields included, is kept. */
export function withPath(settings: Settings, path: string, value: unknown): Settings {
  const next = structuredClone(settings) as unknown as Record<string, unknown>;
  const keys = path.split('.');
  const last = keys.pop() as string;
  let node = next;
  for (const key of keys) node = node[key] as Record<string, unknown>;
  node[last] = value;
  return next as unknown as Settings;
}
```

Create `src/lib/fuzzy.ts`:

```ts
/** Settings search (spec §7): every whitespace-separated word of `query` must appear in one of `fields`, in any case. */
export function matches(query: string, ...fields: string[]): boolean {
  const words = query.toLowerCase().split(/\s+/).filter(Boolean);
  const text = fields.join(' ').toLowerCase();
  return words.every((word) => text.includes(word));
}
```

Create `src/lib/schema.ts`:

```ts
import type { StringKey } from './i18n';

export type PaneId = 'general' | 'clipboard' | 'images' | 'floating';

export const panes: { id: PaneId; label: StringKey }[] = [
  { id: 'general', label: 'pane.general' },
  { id: 'clipboard', label: 'pane.clipboard' },
  { id: 'images', label: 'pane.images' },
  { id: 'floating', label: 'pane.floating' },
];

export type Control =
  | { kind: 'switch' }
  | { kind: 'slider'; min: number; max: number }
  | { kind: 'number'; min: number; max: number; unit: StringKey }
  | { kind: 'select'; options: { value: string; label: StringKey }[] }
  | { kind: 'template' }
  | { kind: 'extensions' }
  | { kind: 'apps' }
  | { kind: 'tray_style' }
  | { kind: 'action'; action: 'open_backups' | 'open_work' | 'force_clean'; button: StringKey };

export interface Setting {
  id: string;
  pane: PaneId;
  /** A heading shown above the first row of a run of rows with the same group. */
  group?: StringKey;
  label: StringKey;
  help?: StringKey;
  /** Dotted path into Settings; actions have none. */
  path?: string;
  control: Control;
  /** Path of the switch that must be on for this row to work. */
  depends_on?: string;
  mac_only?: boolean;
}

const SIZE_UNIT: StringKey = 'unit.mb';

const all: Setting[] = [
  {
    id: 'launch_at_login',
    pane: 'general',
    label: 'general.launch_at_login',
    help: 'general.launch_at_login.help',
    path: 'general.launch_at_login',
    control: { kind: 'switch' },
  },
  {
    id: 'start_state',
    pane: 'general',
    label: 'general.start_state',
    help: 'general.start_state.help',
    path: 'general.start_state',
    control: {
      kind: 'select',
      options: [
        { value: 'running', label: 'tray.state.running' },
        { value: 'paused', label: 'tray.state.paused' },
        { value: 'stopped', label: 'tray.state.stopped' },
      ],
    },
  },
  {
    id: 'show_dock_icon',
    pane: 'general',
    label: 'general.show_dock_icon',
    help: 'general.show_dock_icon.help',
    path: 'general.show_dock_icon',
    control: { kind: 'switch' },
    mac_only: true,
  },
  {
    id: 'show_in_screenshots',
    pane: 'general',
    label: 'general.show_in_screenshots',
    help: 'general.show_in_screenshots.help',
    path: 'general.show_in_screenshots',
    control: { kind: 'switch' },
  },
  {
    id: 'tray_style',
    pane: 'general',
    label: 'general.tray_style',
    help: 'general.tray_style.help',
    path: 'general.tray_style',
    control: { kind: 'tray_style' },
    mac_only: true,
  },
  {
    id: 'language',
    pane: 'general',
    label: 'general.language',
    help: 'general.language.help',
    path: 'general.language',
    control: { kind: 'select', options: [{ value: 'en', label: 'language.en' }] },
  },
  {
    id: 'retention_days',
    pane: 'general',
    group: 'group.storage',
    label: 'general.retention',
    help: 'general.retention.help',
    path: 'backups.retention_days',
    control: { kind: 'number', min: 1, max: 3650, unit: 'unit.days' },
  },
  {
    id: 'open_backups',
    pane: 'general',
    group: 'group.storage',
    label: 'general.open_backups',
    control: { kind: 'action', action: 'open_backups', button: 'action.open' },
  },
  {
    id: 'open_work',
    pane: 'general',
    group: 'group.storage',
    label: 'general.open_work',
    control: { kind: 'action', action: 'open_work', button: 'action.open' },
  },
  {
    id: 'force_clean',
    pane: 'general',
    group: 'group.storage',
    label: 'general.force_clean',
    help: 'general.force_clean.help',
    control: { kind: 'action', action: 'force_clean', button: 'action.clean' },
  },

  {
    id: 'clipboard_enabled',
    pane: 'clipboard',
    label: 'clipboard.enabled',
    help: 'clipboard.enabled.help',
    path: 'clipboard.enabled',
    control: { kind: 'switch' },
  },
  {
    id: 'skip_multiple_files',
    pane: 'clipboard',
    label: 'clipboard.skip_multiple_files',
    help: 'clipboard.skip_multiple_files.help',
    path: 'clipboard.skip_multiple_files',
    control: { kind: 'switch' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'clipboard_max_size',
    pane: 'clipboard',
    label: 'clipboard.max_size',
    help: 'clipboard.max_size.help',
    path: 'clipboard.max_size_mb',
    control: { kind: 'number', min: 1, max: 4096, unit: SIZE_UNIT },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'ignored_apps',
    pane: 'clipboard',
    label: 'clipboard.ignored_apps',
    help: 'clipboard.ignored_apps.help',
    path: 'clipboard.ignored_apps',
    control: { kind: 'apps' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'accumulate',
    pane: 'clipboard',
    label: 'clipboard.accumulate',
    help: 'clipboard.accumulate.help',
    path: 'clipboard.accumulate',
    control: { kind: 'switch' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'copy_paths',
    pane: 'clipboard',
    label: 'clipboard.copy_paths',
    help: 'clipboard.copy_paths.help',
    path: 'clipboard.copy_paths',
    control: { kind: 'switch' },
    depends_on: 'clipboard.enabled',
  },
  {
    id: 'file_template',
    pane: 'clipboard',
    label: 'clipboard.file_template',
    help: 'clipboard.file_template.help',
    path: 'clipboard.file_template',
    control: { kind: 'template' },
    depends_on: 'clipboard.enabled',
  },

  {
    id: 'compression',
    pane: 'images',
    label: 'images.compression',
    help: 'images.compression.help',
    path: 'images.compression',
    control: { kind: 'slider', min: 5, max: 100 },
  },
  {
    id: 'adaptive',
    pane: 'images',
    label: 'images.adaptive',
    help: 'images.adaptive.help',
    path: 'images.adaptive',
    control: { kind: 'switch' },
  },
  {
    id: 'switch_png_jpeg',
    pane: 'images',
    label: 'images.switch_png_jpeg',
    help: 'images.switch_png_jpeg.help',
    path: 'images.switch_png_jpeg',
    control: { kind: 'switch' },
  },
  {
    id: 'strip_metadata',
    pane: 'images',
    label: 'images.strip_metadata',
    help: 'images.strip_metadata.help',
    path: 'images.strip_metadata',
    control: { kind: 'switch' },
  },
  {
    id: 'preserve_dates',
    pane: 'images',
    label: 'images.preserve_dates',
    help: 'images.preserve_dates.help',
    path: 'images.preserve_dates',
    control: { kind: 'switch' },
  },
  {
    id: 'ignored_extensions',
    pane: 'images',
    label: 'images.ignored_extensions',
    help: 'images.ignored_extensions.help',
    path: 'images.ignored_extensions',
    control: { kind: 'extensions' },
  },
  {
    id: 'images_max_size',
    pane: 'images',
    label: 'images.max_size',
    help: 'images.max_size.help',
    path: 'images.max_size_mb',
    control: { kind: 'number', min: 1, max: 4096, unit: SIZE_UNIT },
  },

  {
    id: 'floating_enabled',
    pane: 'floating',
    label: 'floating.enabled',
    help: 'floating.enabled.help',
    path: 'floating.enabled',
    control: { kind: 'switch' },
  },
  {
    id: 'corner',
    pane: 'floating',
    label: 'floating.corner',
    help: 'floating.corner.help',
    path: 'floating.corner',
    control: {
      kind: 'select',
      options: [
        { value: 'top_left', label: 'corner.top_left' },
        { value: 'top_right', label: 'corner.top_right' },
        { value: 'bottom_left', label: 'corner.bottom_left' },
        { value: 'bottom_right', label: 'corner.bottom_right' },
      ],
    },
    depends_on: 'floating.enabled',
  },
  {
    id: 'card_size',
    pane: 'floating',
    label: 'floating.card_size',
    help: 'floating.card_size.help',
    path: 'floating.card_size',
    control: {
      kind: 'select',
      options: [
        { value: 'default', label: 'size.default' },
        { value: 'compact', label: 'size.compact' },
      ],
    },
    depends_on: 'floating.enabled',
  },
  {
    id: 'auto_hide',
    pane: 'floating',
    label: 'floating.auto_hide',
    help: 'floating.auto_hide.help',
    path: 'floating.auto_hide_secs',
    control: { kind: 'number', min: 0, max: 3600, unit: 'unit.seconds' },
    depends_on: 'floating.enabled',
  },
  {
    id: 'follow_cursor',
    pane: 'floating',
    label: 'floating.follow_cursor',
    help: 'floating.follow_cursor.help',
    path: 'floating.follow_cursor',
    control: { kind: 'switch' },
    depends_on: 'floating.enabled',
  },
];

/** The settings to show on this OS. */
export function settingsFor(isMac: boolean): Setting[] {
  return all.filter((s) => isMac || !s.mac_only);
}
```

Add the strings. Run this to insert them before the closing brace of `src/lib/i18n/en.json`:

```bash
python3 - <<'EOF'
import json
path = 'src/lib/i18n/en.json'
strings = json.load(open(path))
strings.update({
  "pane.general": "General",
  "pane.clipboard": "Clipboard",
  "pane.images": "Images",
  "pane.floating": "Floating results",
  "settings.search": "Search settings",
  "settings.no_match": "No settings match",
  "settings.turn_on_first": "Turn on '{name}' first.",
  "settings.unknown_token": "Unknown token {token}",
  "settings.template_example": "Example: {example}",
  "settings.add_app": "Add app…",
  "settings.add_extension": "Add extension",
  "settings.extension_placeholder": "png",
  "settings.remove": "Remove {name}",
  "settings.cleaned": "Removed {files} files ({size})",
  "settings.failed": "That didn't work: {why}",
  "group.storage": "Storage",
  "unit.mb": "MB",
  "unit.days": "days",
  "unit.seconds": "seconds",
  "action.open": "Open",
  "action.clean": "Clean now",
  "language.en": "English",
  "corner.top_left": "Top left",
  "corner.top_right": "Top right",
  "corner.bottom_left": "Bottom left",
  "corner.bottom_right": "Bottom right",
  "size.default": "Default",
  "size.compact": "Compact",
  "tray_style.default": "Default",
  "tray_style.classic": "Classic",
  "tray_style.flat": "Flat",
  "general.launch_at_login": "Launch at login",
  "general.launch_at_login.help": "Start Ferry when you sign in.",
  "general.start_state": "Start in",
  "general.start_state.help": "The state Ferry starts in. Applies the next time Ferry starts.",
  "general.show_dock_icon": "Show Dock icon",
  "general.show_dock_icon.help": "Also list Ferry in the Dock.",
  "general.show_in_screenshots": "Show app UI in screenshots",
  "general.show_in_screenshots.help": "Result cards appear in screenshots and screen sharing.",
  "general.tray_style": "Tray icon style",
  "general.tray_style.help": "How Ferry's menu bar icon looks.",
  "general.language": "Language",
  "general.language.help": "More languages are coming.",
  "general.retention": "Keep backups for",
  "general.retention.help": "Originals older than this are deleted when Ferry starts.",
  "general.open_backups": "Backups folder",
  "general.open_work": "Working directory",
  "general.force_clean": "Clean working directory",
  "general.force_clean.help": "Deletes the working files of finished jobs. Backups are kept.",
  "clipboard.enabled": "Optimise copied images",
  "clipboard.enabled.help": "Shrink images as you copy them.",
  "clipboard.skip_multiple_files": "Skip when several files are copied",
  "clipboard.skip_multiple_files.help": "The optimise hotkey then shows a notice instead.",
  "clipboard.max_size": "Maximum size",
  "clipboard.max_size.help": "Larger copied images are left alone.",
  "clipboard.ignored_apps": "Ignored apps",
  "clipboard.ignored_apps.help": "Copies from these apps are never optimised.",
  "clipboard.accumulate": "Accumulate results",
  "clipboard.accumulate.help": "Put every result since launch on the clipboard as files.",
  "clipboard.copy_paths": "Copy file paths",
  "clipboard.copy_paths.help": "Also put the working file on the clipboard, for apps that paste files.",
  "clipboard.file_template": "File name template",
  "clipboard.file_template.help": "%f name, %e extension, %z size, %y %m %d date, %P folder.",
  "clipboard.pick_app": "Choose an app to ignore",
  "images.compression": "Compression quality",
  "images.compression.help": "Higher keeps more quality and saves less.",
  "images.adaptive": "Adaptive PNG/JPEG switching",
  "images.adaptive.help": "Coming soon.",
  "images.switch_png_jpeg": "Switch between PNG and JPEG",
  "images.switch_png_jpeg.help": "Coming soon.",
  "images.strip_metadata": "Strip metadata",
  "images.strip_metadata.help": "Removes EXIF and similar data. The colour profile and orientation stay.",
  "images.preserve_dates": "Preserve file dates",
  "images.preserve_dates.help": "Optimised files keep their modified and created dates.",
  "images.ignored_extensions": "Ignored extensions",
  "images.ignored_extensions.help": "Files with these extensions are left alone.",
  "images.max_size": "Maximum size",
  "images.max_size.help": "Larger image files are left alone.",
  "floating.enabled": "Show result cards",
  "floating.enabled.help": "Show a card for every optimised image.",
  "floating.corner": "Corner",
  "floating.corner.help": "Where the cards stack.",
  "floating.card_size": "Card size",
  "floating.card_size.help": "Compact cards are smaller and show less.",
  "floating.auto_hide": "Hide after",
  "floating.auto_hide.help": "0 keeps cards until you close them. Cards that failed or are working stay.",
  "floating.follow_cursor": "Follow the cursor across screens",
  "floating.follow_cursor.help": "Show the cards on the screen the pointer is on.",
  "tray.force_clean": "Force clean working directory",
  "error.pick_app": "Couldn't read that app's identifier",
})
json.dump(strings, open(path, 'w'), indent=2, ensure_ascii=False)
open(path, 'a').write('\n')
EOF
```

- [ ] **Step 4: Run them to see them pass**

Run: `pnpm check && pnpm test`
Expected: 0 svelte-check errors; every vitest passes (25 from before plus 2 + 3 + 6 = 36). `cargo test -p ferry-core i18n` still passes: the Rust side reads the same JSON.

- [ ] **Step 5: Commit**

```bash
git add src/lib
git commit -m "feat(ui): settings types, search matching and the list of settings"
```

---

### Task 6: Plugins, platform helpers and `prefs`

**Files:**
- Modify: `src-tauri/Cargo.toml`, `src-tauri/capabilities/default.json`, `src-tauri/src/platform/mod.rs`, `src-tauri/src/platform/macos.rs`, `src-tauri/src/platform/windows.rs`, `src-tauri/src/lib.rs`
- Create: `src-tauri/src/prefs.rs`

**Interfaces:**
- Consumes: `Settings::sanitised` (Task 2), `windows::set_content_protected`, `tray::rebuild`.
- Produces:
  - `platform::accent_rgb() -> Option<(u8, u8, u8)>` (macOS reads `controlAccentColor`; Windows returns `None`);
  - `platform::app_identifier(path: &Path) -> Option<String>` (macOS: the bundle identifier; Windows: the file name);
  - `prefs::Effects { screenshots, autostart, dock, tray: bool }` with `Effects::between(&Settings, &Settings)`;
  - `prefs::update(app, impl FnOnce(&mut Settings)) -> Result<Settings, String>`: sanitise, save, apply, emit `settings:changed`;
  - `prefs::sync_autostart(app, want: bool)`, `prefs::set_dock(app, show: bool)`;
  - startup housekeeping: backups pruned, login item and Dock policy synced.

- [ ] **Step 1: Add the dependencies**

In `src-tauri/Cargo.toml`, add these to `[dependencies]` (keeping the list sorted):

```toml
tauri-plugin-autostart = "2.7.0"
tauri-plugin-dialog = "2.8.3"
```

In the macOS dependency block, replace the `objc2-app-kit` and `objc2-foundation` lines with:

```toml
objc2-app-kit = { version = "0.3.2", default-features = false, features = ["std", "NSPasteboard", "NSPasteboardItem", "NSWorkspace", "NSRunningApplication", "NSColor", "NSColorSpace"] }
objc2-foundation = { version = "0.3.2", default-features = false, features = ["std", "NSArray", "NSData", "NSString", "NSURL", "NSBundle"] }
```

Run: `cargo fetch`

In `src-tauri/capabilities/default.json`, change the permissions to `["core:default", "drag:default", "dialog:default"]` only if Step 6's smoke run shows the Settings page cannot reach the picker; the picker is called from Rust (`pick_app`), so the page itself needs no dialog permission and the file stays as it is.

- [ ] **Step 2: Write the failing tests**

Create `src-tauri/src/prefs.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_changes_nothing_happens() {
        let s = Settings::default();
        assert_eq!(Effects::between(&s, &s), Effects::default());
    }

    #[test]
    fn each_change_touches_only_its_own_effects() {
        let base = Settings::default();

        let mut s = base.clone();
        s.general.show_in_screenshots = !s.general.show_in_screenshots;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                screenshots: true,
                tray: true,
                ..Effects::default()
            }
        );

        let mut s = base.clone();
        s.general.launch_at_login = !s.general.launch_at_login;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                autostart: true,
                ..Effects::default()
            }
        );

        let mut s = base.clone();
        s.general.show_dock_icon = !s.general.show_dock_icon;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                dock: true,
                ..Effects::default()
            }
        );

        let mut s = base.clone();
        s.general.tray_style = TrayStyle::Flat;
        assert_eq!(
            Effects::between(&base, &s),
            Effects {
                tray: true,
                ..Effects::default()
            }
        );
    }

    #[test]
    fn settings_read_on_every_job_need_no_effect() {
        let base = Settings::default();
        let mut s = base.clone();
        s.images.compression = 40;
        s.clipboard.ignored_apps.push("com.example.App".into());
        s.floating.corner = ferry_core::settings::Corner::TopLeft;
        s.backups.retention_days = 30;
        assert_eq!(Effects::between(&base, &s), Effects::default());
    }
}
```

Add `mod prefs;` to `src-tauri/src/lib.rs`, after `mod platform;`.

- [ ] **Step 3: Run them to see them fail**

Run: `cargo test -p ferry prefs`
Expected: compile errors, "cannot find type `Effects`" (and unresolved `Settings`, `TrayStyle`).

- [ ] **Step 4: Implement `prefs`**

Add above the test module in `src-tauri/src/prefs.rs`:

```rust
//! Changing settings while the app runs (spec §3, §5.7): sanitise, save,
//! then apply what the change touches. Everything else, such as compression or
//! the ignored apps, is read from `AppState` by the next job.

use ferry_core::settings::{Settings, TrayStyle};
use tauri::{AppHandle, Emitter, Manager};
use tracing::warn;

use crate::state::AppState;
use crate::{tray, windows};

/// What a change to the settings must do beyond being saved.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Effects {
    /// Hide the stack from screenshots, or stop hiding it.
    pub screenshots: bool,
    /// Register or remove the login item.
    pub autostart: bool,
    /// Show or hide the Dock icon (macOS).
    pub dock: bool,
    /// Rebuild the tray menu and icon.
    pub tray: bool,
}

impl Effects {
    pub fn between(old: &Settings, new: &Settings) -> Self {
        let (a, b) = (&old.general, &new.general);
        let screenshots = a.show_in_screenshots != b.show_in_screenshots;
        let style: fn(&Settings) -> TrayStyle = |s| s.general.tray_style;
        Self {
            screenshots,
            autostart: a.launch_at_login != b.launch_at_login,
            dock: a.show_dock_icon != b.show_dock_icon,
            // The menu's "Show app UI in screenshots" tick follows the setting.
            tray: screenshots || style(old) != style(new),
        }
    }
}

/// Change the settings: `change` edits a copy, which is sanitised, saved and
/// applied. Every window learns of it through `settings:changed`. The write
/// lock is held across the save so two changes cannot interleave.
pub fn update(
    app: &AppHandle,
    change: impl FnOnce(&mut Settings),
) -> Result<Settings, String> {
    let state = app.state::<AppState>();
    let (old, new) = {
        let mut current = state.settings.write().unwrap();
        let old = current.clone();
        let mut next = old.clone();
        change(&mut next);
        let next = next.sanitised();
        next.save(&state.settings_path).map_err(|e| e.to_string())?;
        *current = next.clone();
        (old, next)
    };
    apply(app, &old, &new);
    let _ = app.emit("settings:changed", &new);
    Ok(new)
}

fn apply(app: &AppHandle, old: &Settings, new: &Settings) {
    let effects = Effects::between(old, new);
    if effects.screenshots {
        windows::set_content_protected(app, !new.general.show_in_screenshots);
    }
    if effects.autostart {
        sync_autostart(app, new.general.launch_at_login);
    }
    if effects.dock {
        set_dock(app, new.general.show_dock_icon);
    }
    if effects.tray {
        tray::rebuild(app);
    }
}

/// Make the login item match `want`. Debug builds never touch it: the default
/// is on, and a development run must not register `target/debug/ferry`.
pub fn sync_autostart(app: &AppHandle, want: bool) {
    if cfg!(debug_assertions) {
        return;
    }
    use tauri_plugin_autostart::ManagerExt;
    let launcher = app.autolaunch();
    let result = match launcher.is_enabled() {
        Ok(on) if on != want => {
            if want {
                launcher.enable()
            } else {
                launcher.disable()
            }
        }
        Ok(_) => Ok(()),
        Err(e) => Err(e),
    };
    if let Err(e) = result {
        warn!("could not update the login item: {e}");
    }
}

/// Show or hide the Dock icon. Ferry is a tray app, so it starts hidden.
pub fn set_dock(app: &AppHandle, show: bool) {
    #[cfg(target_os = "macos")]
    {
        let policy = if show {
            tauri::ActivationPolicy::Regular
        } else {
            tauri::ActivationPolicy::Accessory
        };
        if let Err(e) = app.set_activation_policy(policy) {
            warn!("could not change the Dock icon: {e}");
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = (app, show);
}
```

- [ ] **Step 5: Implement the platform helpers**

In `src-tauri/src/platform/mod.rs`, replace the two `pub use` lines so they also export the new functions:

```rust
#[cfg(target_os = "macos")]
pub use macos::{accent_rgb, app_identifier, hide, make_non_activating, show_without_focus};
```

```rust
#[cfg(windows)]
pub use self::windows::{accent_rgb, app_identifier, hide, make_non_activating, show_without_focus};
```

Append to `src-tauri/src/platform/macos.rs`, above any `#[cfg(test)]` module (add `use std::path::Path;` to its imports):

```rust
/// The user's accent colour (System Settings → Appearance), in sRGB.
pub fn accent_rgb() -> Option<(u8, u8, u8)> {
    use objc2_app_kit::{NSColor, NSColorSpace};
    let color = NSColor::controlAccentColor();
    let rgb = color.colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())?;
    let byte = |v: f64| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    Some((
        byte(rgb.redComponent()),
        byte(rgb.greenComponent()),
        byte(rgb.blueComponent()),
    ))
}

/// The bundle identifier of the `.app` at `path`, which is what the clipboard
/// snapshot reports as its source app.
pub fn app_identifier(path: &Path) -> Option<String> {
    use objc2_foundation::{NSBundle, NSString};
    let bundle = NSBundle::bundleWithPath(&NSString::from_str(path.to_str()?))?;
    bundle.bundleIdentifier().map(|id| id.to_string())
}
```

Append to `src-tauri/src/platform/windows.rs` (add `use std::path::Path;` to its imports):

```rust
/// Windows uses the default accent this cycle (spec §7).
pub fn accent_rgb() -> Option<(u8, u8, u8)> {
    None
}

/// The executable's file name, which is what the clipboard snapshot reports as
/// its source app.
pub fn app_identifier(path: &Path) -> Option<String> {
    path.file_name().map(|name| name.to_string_lossy().into_owned())
}
```

If `NSColor`, `NSBundle` or the component getters are `unsafe` or named differently in `objc2-app-kit` 0.3.2, fix the calls in this step (wrap them in `unsafe` with a `// SAFETY:` comment where the binding requires it); the function signatures above are what later tasks rely on.

- [ ] **Step 6: Housekeeping at startup**

In `src-tauri/src/lib.rs`:

Add the plugins to the builder, after `.plugin(tauri_plugin_drag::init())`:

```rust
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_autostart::init(
            #[cfg(target_os = "macos")]
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            #[cfg(not(target_os = "macos"))]
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
```

(`MacosLauncher` is an argument on every OS; the plugin ignores it off macOS. If the compiler accepts a single un-`cfg`'d `tauri_plugin_autostart::MacosLauncher::LaunchAgent` argument, use that and drop the two `cfg` lines.)

In `setup`, after `tray::build(app)?;` and `clipboard::start(app);`, add:

```rust
    housekeeping(app);
```

Add this function at the end of the file, before `gifsicle()`:

```rust
/// Work that follows the settings at every start: prune old backups off the
/// main path, and make the login item and the Dock icon match.
fn housekeeping(app: &AppHandle) {
    let (retention, login, dock) = {
        let settings = app.state::<AppState>().settings.read().unwrap().clone();
        (
            settings.backups.retention_days,
            settings.general.launch_at_login,
            settings.general.show_dock_icon,
        )
    };
    prefs::sync_autostart(app, login);
    if dock {
        prefs::set_dock(app, true);
    }
    let app = app.clone();
    std::thread::spawn(move || {
        match app.state::<AppState>().backups.prune(retention) {
            Ok(report) => info!(removed = report.removed.len(), retention, "pruned old backups"),
            Err(e) => tracing::warn!("could not prune old backups: {e}"),
        }
    });
}
```

- [ ] **Step 7: Run them to see them pass**

Run: `cargo fmt --all && cargo test -p ferry && cargo clippy --workspace --all-targets -- -D warnings`
Expected: PASS, 21 app tests (18 before plus 3), and no warnings. `prefs::update`, `accent_rgb` and `app_identifier` are called from Task 7; if clippy reports them as dead code now, add `#[allow(dead_code)] // used from Task 7` on just those items, and remove the attributes in Task 7.

- [ ] **Step 8: Commit**

```bash
git add Cargo.lock src-tauri
git commit -m "feat(app): prefs, login item, Dock icon, startup prune and platform helpers"
```

---

### Task 7: Commands, Force clean and the tray

**Files:**
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/jobs.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/lib.rs`

**Interfaces:**
- Consumes: Tasks 2 to 4 and 6.
- Produces these commands for the Settings page (Task 8):
  - `settings_get() -> Settings`;
  - `settings_set(settings: Settings) -> Result<Settings, String>`;
  - `template_preview(template: String) -> Result<String, Vec<String>>`;
  - `tray_style_previews() -> Vec<TrayPreview { style: TrayStyle, png: Vec<u8> }>`;
  - `accent_color() -> Option<String>`;
  - `pick_app() -> Result<Option<String>, String>`;
  - `open_backups()`, `open_work()`;
  - `force_clean() -> Result<CleanDone { files: u32, bytes: u64 }, String>`.
- Produces the event `stack:clear` (the stack empties itself) and the tray item "Force clean working directory".

This task is glue over code tested in Tasks 2 to 6, so it has no new unit tests; Step 6 checks it by running the app.

- [ ] **Step 1: Force clean in `jobs`**

Add to `src-tauri/src/jobs.rs`, after `output`:

```rust
/// "Force clean working directory" (§5.6): delete the working files of every
/// finished job and forget those jobs (their cards and Recent results), keeping
/// what queued or running jobs still use. Backups are untouched.
pub fn force_clean(app: &AppHandle) -> Result<ferry_core::workdir::CleanReport, String> {
    let state = app.state::<AppState>();
    let (keep, busy) = {
        let mut jobs = state.jobs.lock().unwrap();
        jobs.retain(|_, entry| entry.is_busy());
        let keep: Vec<std::path::PathBuf> = jobs.values().map(|e| e.output.clone()).collect();
        let busy: Vec<JobUpdate> = jobs.values().map(|e| e.last.lock().unwrap().clone()).collect();
        (keep, busy)
    };
    state.recent.lock().unwrap().clear();
    let report = ferry_core::workdir::clean(&state.work_dir, &keep).map_err(|e| e.to_string())?;
    info!(files = report.files, bytes = report.bytes, "working directory cleaned");
    // Empty the stack, then put back the cards of the jobs that carry on.
    if let Err(e) = app.emit_to(STACK, "stack:clear", ()) {
        warn!("could not clear the stack: {e}");
    }
    for card in busy {
        show(app, card);
    }
    tray::rebuild(app);
    Ok(report)
}
```

- [ ] **Step 2: Commands**

Replace the imports at the top of `src-tauri/src/commands.rs` with whatever it needs after Step 2's additions (at least `ferry_core::settings::{Settings, TrayStyle}`, `ferry_core::{color, glyph, i18n::t, template}`), then add at the end of the file:

```rust
#[tauri::command]
pub fn settings_get(app: AppHandle) -> Settings {
    app.state::<AppState>().settings.read().unwrap().clone()
}

/// Apply a change from the Settings page. The answer is what was saved, which
/// may differ from what was sent (values are clamped).
#[tauri::command]
pub fn settings_set(app: AppHandle, settings: Settings) -> Result<Settings, String> {
    prefs::update(&app, |current| *current = settings)
}

/// The live example under the file-name template field, or its unknown tokens.
#[tauri::command]
pub fn template_preview(template: String) -> Result<String, Vec<String>> {
    template::preview(&template)
}

#[derive(Serialize)]
pub struct TrayPreview {
    style: TrayStyle,
    png: Vec<u8>,
}

/// The three tray icon styles as PNGs, for the picker. Black on transparent,
/// like the template image the menu bar tints; the page inverts it in dark mode.
#[tauri::command]
pub fn tray_style_previews() -> Vec<TrayPreview> {
    let Ok(base) = tauri::image::Image::from_bytes(crate::tray::BASE_ICON) else {
        return Vec::new();
    };
    [TrayStyle::Default, TrayStyle::Classic, TrayStyle::Flat]
        .into_iter()
        .map(|style| {
            let rgba = glyph::render_styled(base.rgba(), base.width(), glyph::TrayGlyph::Running, style);
            TrayPreview {
                style,
                png: glyph::to_png(&rgba, base.width()),
            }
        })
        .collect()
}

/// The macOS accent colour as `#rrggbb`, fit for white text; `None` elsewhere.
#[tauri::command]
pub fn accent_color() -> Option<String> {
    platform::accent_rgb().map(color::accent_hex)
}

/// Let the user pick an application and answer with the identifier the
/// clipboard reports for it (bundle id on macOS, file name on Windows);
/// `None` when the dialog is cancelled.
#[tauri::command]
pub async fn pick_app(app: AppHandle) -> Result<Option<String>, String> {
    blocking(app, |app| {
        use tauri_plugin_dialog::DialogExt;
        let dialog = app.dialog().file().set_title(t("clipboard.pick_app"));
        #[cfg(target_os = "macos")]
        let dialog = dialog
            .add_filter("Applications", &["app"])
            .set_directory("/Applications");
        #[cfg(windows)]
        let dialog = dialog.add_filter("Programs", &["exe"]);
        let Some(path) = dialog.blocking_pick_file() else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        platform::app_identifier(&path)
            .map(Some)
            .ok_or_else(|| t("error.pick_app"))
    })
    .await
}

#[tauri::command]
pub fn open_backups(app: AppHandle) {
    crate::tray::open_folder(&app.state::<AppState>().backups_dir);
}

#[tauri::command]
pub fn open_work(app: AppHandle) {
    crate::tray::open_folder(&app.state::<AppState>().work_dir);
}

#[derive(Serialize)]
pub struct CleanDone {
    files: u32,
    bytes: u64,
}

#[tauri::command]
pub async fn force_clean(app: AppHandle) -> Result<CleanDone, String> {
    blocking(app, |app| {
        jobs::force_clean(app).map(|r| CleanDone {
            files: r.files,
            bytes: r.bytes,
        })
    })
    .await
}
```

Make sure the file imports `crate::{platform, prefs}` and `serde::Serialize` (it already has the latter).

In `src-tauri/src/lib.rs`, add the new commands to `tauri::generate_handler![...]`:

```rust
            commands::settings_get,
            commands::settings_set,
            commands::template_preview,
            commands::tray_style_previews,
            commands::accent_color,
            commands::pick_app,
            commands::open_backups,
            commands::open_work,
            commands::force_clean,
```

- [ ] **Step 3: Tray**

In `src-tauri/src/tray.rs`:

1. Make the base icon and the folder opener public to the crate. Change `const BASE_ICON: &[u8] = include_bytes!("../icons/tray/64x64.png");` (it is `#[cfg(target_os = "macos")]`) to `pub(crate) const BASE_ICON`, and drop its `cfg` line so the picker works on Windows too (the constant is 2 KB). Change `fn open_folder` to `pub(crate) fn open_folder`.
2. In `refresh`, draw the chosen style. Replace `let rgba = glyph::render(base.rgba(), base.width(), glyph);` with:

```rust
        let style = state.settings.read().unwrap().general.tray_style;
        let rgba = glyph::render_styled(base.rgba(), base.width(), glyph, style);
```

3. Add the menu item. After the `work` item in `menu`:

```rust
    let force_clean = MenuItem::with_id(
        app,
        "force_clean",
        t("tray.force_clean"),
        true,
        None::<&str>,
    )?;
```

and list `&force_clean,` after `&work,` in the `Menu::with_items` array.

4. In `on_menu`, handle it, and route the screenshots toggle through `prefs` so the Settings window and the saved file follow. Replace the `"screenshots" => { ... }` arm with:

```rust
        "screenshots" => {
            if let Err(e) = crate::prefs::update(app, |s| {
                s.general.show_in_screenshots = !s.general.show_in_screenshots;
            }) {
                warn!("could not save settings: {e}");
            }
        }
        "force_clean" => {
            let app = app.clone();
            std::thread::spawn(move || {
                if let Err(e) = jobs::force_clean(&app) {
                    warn!("could not clean the working directory: {e}");
                }
            });
        }
```

`prefs::update` already sets content protection and rebuilds the tray, so the old arm's calls to `windows::set_content_protected` and `rebuild` go away. If `windows` is now unused in `tray.rs` except for `open_settings`, leave its import.

- [ ] **Step 4: Remove the temporary `allow(dead_code)` attributes** from Task 6, if you added any.

- [ ] **Step 5: Lint and test**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
Expected: no warnings, and every test passes. Fix API mismatches (the `objc2` calls from Task 6, `set_activation_policy`, the dialog's `into_path`) here.

- [ ] **Step 6: Smoke-run**

Delete the dev settings so the defaults apply, then start the dev build:

```bash
rm -f "$HOME/Library/Application Support/io.ferry.desktop.dev/settings.json"
pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'
```

Expected: `~/Library/Logs/io.ferry.desktop.dev/ferry.<today>.log` has `Ferry started`, three `hotkey registered` lines, and `pruned old backups`; it has no WARN or ERROR line; `ls ~/Library/LaunchAgents | grep -i ferry` prints nothing (debug builds never touch the login item). Open the tray menu: "Force clean working directory" is listed. Stop the app.

- [ ] **Step 7: Commit**

```bash
git add src-tauri
git commit -m "feat(app): settings commands, force clean, tray icon styles and the picker"
```

---

### Task 8: The settings store and its controls

**Files:**
- Create: `src/settings/store.svelte.ts`, `src/settings/controls/{Switch,Slider,NumberField,Choice,ListEditor,TemplateField,TrayStyle}.svelte`

**Interfaces:**
- Consumes: the Task 7 commands; `getPath`, `withPath`, `Settings` (Task 5).
- Produces:
  - `store.svelte.ts`: `store.value: Settings | null`, `load(): Promise<void>`, `change(path: string, value: unknown): void`;
  - controls, each taking `labelledby` (the id of the row label) and reporting changes through `onchange`.

The page has no automated UI tests this cycle (spec §8); `svelte-check` types it, and Task 10 checks it by running it.

- [ ] **Step 1: The store**

Create `src/settings/store.svelte.ts`:

```ts
import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { withPath, type Settings } from '../lib/settings';

/** The settings the page shows. Null until `load` has answered. */
export const store = $state<{ value: Settings | null; error: string | null }>({
  value: null,
  error: null,
});

/** Saves in flight; while any are, answers and events must not overwrite what the user just changed. */
let pending = 0;
let queue: Promise<void> = Promise.resolve();

export async function load(): Promise<void> {
  store.value = await invoke<Settings>('settings_get');
  // The tray's own toggle and other windows change settings too.
  await listen<Settings>('settings:changed', (event) => {
    if (pending === 0) store.value = event.payload;
  });
}

/** Show a change at once, then save it. Rust answers with what it saved (values are clamped). */
export function change(path: string, value: unknown): void {
  if (!store.value) return;
  const next = withPath($state.snapshot(store.value) as Settings, path, value);
  store.value = next;
  store.error = null;
  pending += 1;
  queue = queue
    .then(() => invoke<Settings>('settings_set', { settings: next }))
    .then((saved) => {
      if (pending === 1) store.value = saved;
    })
    .catch(async (e) => {
      store.error = String(e);
      store.value = await invoke<Settings>('settings_get');
    })
    .finally(() => {
      pending -= 1;
    });
}
```

- [ ] **Step 2: Switch, Slider, NumberField and Choice**

Create `src/settings/controls/Switch.svelte`:

```svelte
<script lang="ts">
  let {
    checked,
    onchange,
    labelledby,
    disabled = false,
  }: {
    checked: boolean;
    onchange: (value: boolean) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();
</script>

<button
  type="button"
  role="switch"
  class="switch"
  class:on={checked}
  aria-checked={checked}
  aria-labelledby={labelledby}
  {disabled}
  onclick={() => onchange(!checked)}
>
  <span class="knob"></span>
</button>

<style>
  .switch {
    flex-shrink: 0;
    width: 38px;
    height: 22px;
    padding: 2px;
    border: 0;
    border-radius: 11px;
    background: var(--track-off);
    cursor: pointer;
    transition: background 120ms;
  }
  .switch.on {
    background: var(--accent);
  }
  .switch:disabled {
    opacity: 0.45;
    cursor: default;
  }
  .switch:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  .knob {
    display: block;
    width: 18px;
    height: 18px;
    border-radius: 50%;
    background: #fff;
    transition: transform 120ms;
  }
  .on .knob {
    transform: translateX(16px);
  }
  @media (prefers-reduced-motion: reduce) {
    .switch,
    .knob {
      transition: none;
    }
  }
</style>
```

Create `src/settings/controls/Slider.svelte`:

```svelte
<script lang="ts">
  let {
    value,
    min,
    max,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: number;
    min: number;
    max: number;
    onchange: (value: number) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  // The thumb moves freely; the value is saved when it is let go.
  let live = $state(0);
  $effect(() => {
    live = value;
  });
</script>

<div class="slider">
  <input
    type="range"
    {min}
    {max}
    {disabled}
    bind:value={live}
    aria-labelledby={labelledby}
    onchange={() => onchange(live)}
  />
  <output>{live}</output>
</div>

<style>
  .slider {
    display: flex;
    align-items: center;
    gap: 10px;
  }
  input {
    width: 160px;
    accent-color: var(--accent);
  }
  output {
    min-width: 2.5em;
    text-align: right;
    font-variant-numeric: tabular-nums;
  }
</style>
```

Create `src/settings/controls/NumberField.svelte`:

```svelte
<script lang="ts">
  let {
    value,
    min,
    max,
    unit,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: number;
    min: number;
    max: number;
    unit: string;
    onchange: (value: number) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  function commit(input: HTMLInputElement) {
    const typed = Number(input.value);
    if (!Number.isFinite(typed)) {
      input.value = String(value);
      return;
    }
    const clamped = Math.min(max, Math.max(min, Math.round(typed)));
    input.value = String(clamped);
    if (clamped !== value) onchange(clamped);
  }
</script>

<span class="field">
  <input
    type="number"
    {min}
    {max}
    {value}
    {disabled}
    aria-labelledby={labelledby}
    onchange={(event) => commit(event.currentTarget)}
  />
  <span class="unit">{unit}</span>
</span>

<style>
  .field {
    display: inline-flex;
    align-items: center;
    gap: 6px;
  }
  input {
    width: 72px;
    padding: 4px 6px;
    font: inherit;
    font-variant-numeric: tabular-nums;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  .unit {
    color: var(--fg-2);
  }
</style>
```

Create `src/settings/controls/Choice.svelte`:

```svelte
<script lang="ts">
  let {
    value,
    options,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: string;
    options: { value: string; label: string }[];
    onchange: (value: string) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();
</script>

<select
  {disabled}
  aria-labelledby={labelledby}
  onchange={(event) => onchange(event.currentTarget.value)}
>
  {#each options as option (option.value)}
    <option value={option.value} selected={option.value === value}>{option.label}</option>
  {/each}
</select>

<style>
  select {
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  select:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
</style>
```

- [ ] **Step 3: The list editor, the template field and the tray style picker**

Create `src/settings/controls/ListEditor.svelte`:

```svelte
<script lang="ts">
  import { t } from '../../lib/i18n';

  let {
    items,
    mode,
    onchange,
    onpick,
    labelledby,
    disabled = false,
  }: {
    items: string[];
    /** `text` types an extension; `picker` asks `onpick` for an app. */
    mode: 'text' | 'picker';
    onchange: (items: string[]) => void;
    onpick?: () => Promise<string | null>;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');

  function add(item: string) {
    const clean = item.trim().replace(/^\./, '').toLowerCase();
    if (clean && !items.includes(clean)) onchange([...items, clean]);
    draft = '';
  }

  async function pick() {
    const id = await onpick?.();
    if (id && !items.includes(id)) onchange([...items, id]);
  }
</script>

<div class="list">
  <ul aria-labelledby={labelledby}>
    {#each items as item (item)}
      <li>
        <span>{item}</span>
        <button
          type="button"
          {disabled}
          aria-label={t('settings.remove', { name: item })}
          onclick={() => onchange(items.filter((i) => i !== item))}>×</button
        >
      </li>
    {/each}
  </ul>
  {#if mode === 'text'}
    <input
      type="text"
      {disabled}
      bind:value={draft}
      placeholder={t('settings.extension_placeholder')}
      aria-label={t('settings.add_extension')}
      onkeydown={(event) => {
        if (event.key === 'Enter') add(draft);
      }}
      onblur={() => add(draft)}
    />
  {:else}
    <button type="button" class="add" {disabled} onclick={pick}>{t('settings.add_app')}</button>
  {/if}
</div>

<style>
  .list {
    display: flex;
    flex-direction: column;
    align-items: flex-end;
    gap: 6px;
    max-width: 320px;
  }
  ul {
    display: flex;
    flex-wrap: wrap;
    justify-content: flex-end;
    gap: 4px;
    margin: 0;
    padding: 0;
    list-style: none;
  }
  li {
    display: inline-flex;
    align-items: center;
    gap: 4px;
    padding: 2px 4px 2px 10px;
    border-radius: 999px;
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
  }
  li button {
    border: 0;
    background: none;
    color: var(--fg-2);
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
  }
  input {
    width: 100px;
    padding: 4px 6px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  .add {
    padding: 4px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  button:focus-visible,
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
```

Create `src/settings/controls/TemplateField.svelte`:

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t } from '../../lib/i18n';

  let {
    value,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: string;
    onchange: (value: string) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');
  let example = $state('');
  let unknown = $state<string[]>([]);

  $effect(() => {
    draft = value;
  });

  // Rust renders the example, so there is one template implementation.
  $effect(() => {
    const text = draft;
    let current = true;
    invoke<string>('template_preview', { template: text })
      .then((ok) => {
        if (!current) return;
        example = ok;
        unknown = [];
      })
      .catch((bad) => {
        if (current) unknown = bad as string[];
      });
    return () => {
      current = false;
    };
  });

  /** Only a valid template is saved; an invalid one stays in the field with its error. */
  function commit() {
    if (unknown.length === 0 && draft !== value) onchange(draft);
  }
</script>

<div class="template">
  <input
    type="text"
    {disabled}
    bind:value={draft}
    aria-labelledby={labelledby}
    aria-invalid={unknown.length > 0}
    onchange={commit}
    onkeydown={(event) => {
      if (event.key === 'Enter') commit();
    }}
  />
  {#if unknown.length > 0}
    <p class="bad" role="alert">{t('settings.unknown_token', { token: unknown[0] })}</p>
  {:else}
    <p class="example">{t('settings.template_example', { example })}</p>
  {/if}
</div>

<style>
  .template {
    width: 300px;
  }
  input {
    box-sizing: border-box;
    width: 100%;
    padding: 4px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  input[aria-invalid='true'] {
    border-color: var(--error);
  }
  input:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  p {
    margin: 4px 0 0;
    font-size: 11px;
    overflow-wrap: anywhere;
  }
  .example {
    color: var(--fg-2);
  }
  .bad {
    color: var(--error);
  }
</style>
```

Create `src/settings/controls/TrayStyle.svelte`:

```svelte
<script lang="ts">
  import { onMount } from 'svelte';
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../../lib/i18n';

  let {
    value,
    onchange,
    labelledby,
  }: { value: string; onchange: (value: string) => void; labelledby: string } = $props();

  interface Preview {
    style: 'default' | 'classic' | 'flat';
    png: number[];
  }

  let previews = $state<{ style: Preview['style']; url: string }[]>([]);

  onMount(() => {
    let urls: string[] = [];
    invoke<Preview[]>('tray_style_previews').then((list) => {
      previews = list.map((p) => {
        const url = URL.createObjectURL(new Blob([new Uint8Array(p.png)], { type: 'image/png' }));
        urls.push(url);
        return { style: p.style, url };
      });
    });
    return () => urls.forEach((url) => URL.revokeObjectURL(url));
  });
</script>

<div class="styles" role="radiogroup" aria-labelledby={labelledby}>
  {#each previews as preview (preview.style)}
    <button
      type="button"
      role="radio"
      aria-checked={value === preview.style}
      class:selected={value === preview.style}
      onclick={() => onchange(preview.style)}
    >
      <img src={preview.url} alt="" width="32" height="32" />
      <span>{t(`tray_style.${preview.style}` as StringKey)}</span>
    </button>
  {/each}
</div>

<style>
  .styles {
    display: flex;
    gap: 8px;
  }
  button {
    display: flex;
    flex-direction: column;
    align-items: center;
    gap: 4px;
    padding: 8px 10px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 2px solid transparent;
    border-radius: 8px;
    cursor: pointer;
  }
  button.selected {
    border-color: var(--accent);
  }
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 2px;
  }
  /* The images are black template art; the menu bar tints them, and so do we. */
  img {
    filter: none;
  }
  @media (prefers-color-scheme: dark) {
    img {
      filter: invert(1);
    }
  }
</style>
```

- [ ] **Step 4: Check**

Run: `pnpm check`
Expected: 0 errors (the controls are not mounted yet; Task 9 does that). Warnings about unused CSS are fixed here.

- [ ] **Step 5: Commit**

```bash
git add src/settings
git commit -m "feat(ui): settings store and controls"
```

---

### Task 9: The Settings page, search, accent and live floating settings

**Files:**
- Create: `src/settings/Row.svelte`
- Replace: `src/settings/Settings.svelte`
- Modify: `src/settings/main.ts`, `src/stack/main.ts`, `src/lib/ipc.ts`, `src/lib/theme.css`, `src/stack/Stack.svelte`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 5's `schema`, `fuzzy` and `settings`; Task 8's store and controls; the Task 7 commands and events.
- Produces: the working Settings window; `applyAccent()` in `ipc.ts`; a stack that follows floating settings and `stack:clear`.

- [ ] **Step 1: The accent**

Append to `src/lib/ipc.ts`:

```ts
/** On macOS, Rust answers with the user's accent colour, already adjusted for white text on top; Windows keeps the default (spec §7). */
export async function applyAccent(): Promise<void> {
  const { invoke } = await import('@tauri-apps/api/core');
  const hex = await invoke<string | null>('accent_color');
  if (!hex) return;
  document.documentElement.style.setProperty('--accent', hex);
  document.documentElement.dataset.accent = 'custom';
}
```

Append to `src/lib/theme.css`, before the `prefers-reduced-motion` block:

```css
/* A custom accent (macOS) also colours accent text; in dark mode it is lightened so it reads on dark. */
:root[data-accent] {
  --accent-text: var(--accent);
}

@media (prefers-color-scheme: dark) {
  :root[data-accent] {
    --accent-text: color-mix(in srgb, var(--accent) 55%, white);
  }
}
```

In `src/settings/main.ts` and `src/stack/main.ts`, change the `import { applyOsClass } from '../lib/ipc';` line to `import { applyAccent, applyOsClass } from '../lib/ipc';` and call `void applyAccent();` on the line after `applyOsClass();`.

- [ ] **Step 2: One row**

Create `src/settings/Row.svelte`:

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { formatBytes } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import type { Setting } from '../lib/schema';
  import Choice from './controls/Choice.svelte';
  import ListEditor from './controls/ListEditor.svelte';
  import NumberField from './controls/NumberField.svelte';
  import Slider from './controls/Slider.svelte';
  import Switch from './controls/Switch.svelte';
  import TemplateField from './controls/TemplateField.svelte';
  import TrayStyle from './controls/TrayStyle.svelte';

  let {
    setting,
    value,
    disabled,
    hint,
    highlight,
    onchange,
  }: {
    setting: Setting;
    value: unknown;
    disabled: boolean;
    /** "Turn on '…' first." when a switch this row depends on is off. */
    hint: string | null;
    highlight: boolean;
    onchange: (value: unknown) => void;
  } = $props();

  const labelId = $derived(`label-${setting.id}`);
  const control = $derived(setting.control);
  let outcome = $state<string | null>(null);

  async function run(action: 'open_backups' | 'open_work' | 'force_clean') {
    outcome = null;
    try {
      if (action === 'force_clean') {
        const done = await invoke<{ files: number; bytes: number }>('force_clean');
        outcome = t('settings.cleaned', { files: done.files, size: formatBytes(done.bytes) });
      } else {
        await invoke(action);
      }
    } catch (e) {
      outcome = t('settings.failed', { why: String(e) });
    }
  }

  async function pickApp(): Promise<string | null> {
    try {
      return await invoke<string | null>('pick_app');
    } catch (e) {
      outcome = t('settings.failed', { why: String(e) });
      return null;
    }
  }
</script>

<div class="row" class:highlight id={`row-${setting.id}`}>
  <div class="text">
    <div class="label" id={labelId}>{t(setting.label)}</div>
    {#if hint}
      <div class="help">{hint}</div>
    {:else if setting.help}
      <div class="help">{t(setting.help)}</div>
    {/if}
    {#if outcome}
      <div class="help" role="status">{outcome}</div>
    {/if}
  </div>
  <div class="control">
    {#if control.kind === 'switch'}
      <Switch checked={value as boolean} {disabled} labelledby={labelId} {onchange} />
    {:else if control.kind === 'slider'}
      <Slider
        value={value as number}
        min={control.min}
        max={control.max}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'number'}
      <NumberField
        value={value as number}
        min={control.min}
        max={control.max}
        unit={t(control.unit)}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'select'}
      <Choice
        value={value as string}
        options={control.options.map((o) => ({ value: o.value, label: t(o.label) }))}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'template'}
      <TemplateField value={value as string} {disabled} labelledby={labelId} {onchange} />
    {:else if control.kind === 'extensions'}
      <ListEditor
        items={value as string[]}
        mode="text"
        {disabled}
        labelledby={labelId}
        onchange={onchange}
      />
    {:else if control.kind === 'apps'}
      <ListEditor
        items={value as string[]}
        mode="picker"
        onpick={pickApp}
        {disabled}
        labelledby={labelId}
        onchange={onchange}
      />
    {:else if control.kind === 'tray_style'}
      <TrayStyle value={value as string} labelledby={labelId} {onchange} />
    {:else if control.kind === 'action'}
      <button type="button" class="action" aria-labelledby={labelId} onclick={() => run(control.action)}>
        {t(control.button as StringKey)}
      </button>
    {/if}
  </div>
</div>

<style>
  .row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 16px;
    padding: 10px 14px;
    border-bottom: 1px solid var(--rail-line);
    transition: background 300ms;
  }
  .row:last-child {
    border-bottom: 0;
  }
  .row.highlight {
    background: color-mix(in srgb, var(--accent) 22%, transparent);
  }
  .text {
    min-width: 0;
  }
  .label {
    font-size: 13px;
  }
  .help {
    margin-top: 2px;
    font-size: 11px;
    color: var(--fg-2);
  }
  .control {
    flex-shrink: 0;
  }
  .action {
    padding: 4px 12px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  .action:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  @media (prefers-reduced-motion: reduce) {
    .row {
      transition: none;
    }
  }
</style>
```

- [ ] **Step 3: The window**

Replace `src/settings/Settings.svelte`:

```svelte
<script lang="ts">
  import { onMount, tick } from 'svelte';
  import { matches } from '../lib/fuzzy';
  import { t } from '../lib/i18n';
  import { isMac } from '../lib/ipc';
  import { panes, settingsFor, type PaneId, type Setting } from '../lib/schema';
  import { getPath } from '../lib/settings';
  import Row from './Row.svelte';
  import { change, load, store } from './store.svelte';

  const all = settingsFor(isMac);
  const paneLabel = (id: PaneId) => t(panes.find((p) => p.id === id)!.label);

  let pane = $state<PaneId>('general');
  let query = $state('');
  let highlighted = $state<string | null>(null);

  const searching = $derived(query.trim() !== '');
  const results = $derived(
    all.filter((s) =>
      matches(query, t(s.label), s.help ? t(s.help) : '', paneLabel(s.pane)),
    ),
  );
  const rows = $derived(all.filter((s) => s.pane === pane));

  onMount(() => {
    void load();
  });

  /** The switch a row depends on, when it is off. */
  function blocker(setting: Setting): Setting | null {
    if (!setting.depends_on || !store.value) return null;
    if (getPath(store.value, setting.depends_on)) return null;
    return all.find((s) => s.path === setting.depends_on) ?? null;
  }

  async function jump(setting: Setting) {
    pane = setting.pane;
    query = '';
    await tick();
    document.getElementById(`row-${setting.id}`)?.scrollIntoView({ block: 'center' });
    highlighted = setting.id;
    setTimeout(() => {
      if (highlighted === setting.id) highlighted = null;
    }, 2000);
  }
</script>

<div class="window">
  <nav class="sidebar" aria-label={t('settings.window_title')}>
    <input
      type="search"
      class="search"
      placeholder={t('settings.search')}
      aria-label={t('settings.search')}
      bind:value={query}
    />
    {#if searching}
      {#if results.length === 0}
        <p class="empty">{t('settings.no_match')}</p>
      {:else}
        <ul class="results">
          {#each results as result (result.id)}
            <li>
              <button type="button" onclick={() => jump(result)}>
                <span>{t(result.label)}</span>
                <small>{paneLabel(result.pane)}</small>
              </button>
            </li>
          {/each}
        </ul>
      {/if}
    {:else}
      <ul class="panes">
        {#each panes as p (p.id)}
          <li>
            <button
              type="button"
              class:selected={pane === p.id}
              aria-current={pane === p.id ? 'page' : undefined}
              onclick={() => (pane = p.id)}>{t(p.label)}</button
            >
          </li>
        {/each}
      </ul>
    {/if}
  </nav>

  <main class="content">
    <h1>{paneLabel(pane)}</h1>
    {#if store.error}
      <p class="error" role="alert">{t('settings.failed', { why: store.error })}</p>
    {/if}
    {#if store.value}
      <div class="group">
        {#each rows as setting, i (setting.id)}
          {#if setting.group && setting.group !== rows[i - 1]?.group}
            </div>
            <h2>{t(setting.group)}</h2>
            <div class="group">
          {/if}
          {@const lock = blocker(setting)}
          <Row
            {setting}
            value={setting.path ? getPath(store.value, setting.path) : undefined}
            disabled={lock !== null}
            hint={lock ? t('settings.turn_on_first', { name: t(lock.label) }) : null}
            highlight={highlighted === setting.id}
            onchange={(value) => setting.path && change(setting.path, value)}
          />
        {/each}
      </div>
    {/if}
  </main>
</div>

<style>
  .window {
    display: flex;
    height: 100vh;
  }
  .sidebar {
    flex: 0 0 210px;
    padding: 14px 10px;
    overflow-y: auto;
    background: var(--bg-sidebar);
  }
  .search {
    box-sizing: border-box;
    width: 100%;
    margin-bottom: 10px;
    padding: 5px 8px;
    font: inherit;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
  }
  ul {
    margin: 0;
    padding: 0;
    list-style: none;
  }
  .sidebar button {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    width: 100%;
    padding: 6px 10px;
    font: inherit;
    color: var(--fg);
    text-align: left;
    background: none;
    border: 0;
    border-radius: 6px;
    cursor: pointer;
  }
  .sidebar button.selected {
    background: var(--accent);
    color: #fff;
  }
  .sidebar button:focus-visible,
  .search:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  small {
    color: var(--fg-2);
    font-size: 11px;
  }
  .empty {
    padding: 0 10px;
    color: var(--fg-2);
  }
  .content {
    flex: 1;
    padding: 20px 24px;
    overflow-y: auto;
  }
  h1 {
    margin: 0 0 14px;
    font-size: 20px;
    font-weight: 600;
  }
  h2 {
    margin: 22px 0 8px;
    font-size: 11px;
    font-weight: 600;
    text-transform: uppercase;
    letter-spacing: 0.04em;
    color: var(--fg-2);
  }
  .group {
    background: var(--bg-group);
    border-radius: 10px;
  }
  .error {
    color: var(--error);
  }
</style>
```

Svelte does not allow an unbalanced `</div>` inside an `{#if}`. Replace the `{#each rows ...}` block above with one that groups first. In the `<script>`, add:

```ts
  /** The rows of the open pane, split into runs that share a group heading. */
  const groups = $derived.by(() => {
    const out: { heading: Setting['group']; rows: Setting[] }[] = [];
    for (const setting of rows) {
      const last = out[out.length - 1];
      if (last && last.heading === setting.group) last.rows.push(setting);
      else out.push({ heading: setting.group, rows: [setting] });
    }
    return out;
  });
```

and replace everything from `<div class="group">` to its closing `</div>` inside `{#if store.value}` with:

```svelte
      {#each groups as g (g.heading ?? 'main')}
        {#if g.heading}<h2>{t(g.heading)}</h2>{/if}
        <div class="group">
          {#each g.rows as setting (setting.id)}
            {@const lock = blocker(setting)}
            <Row
              {setting}
              value={setting.path ? getPath(store.value, setting.path) : undefined}
              disabled={lock !== null}
              hint={lock ? t('settings.turn_on_first', { name: t(lock.label) }) : null}
              highlight={highlighted === setting.id}
              onchange={(value) => setting.path && change(setting.path, value)}
            />
          {/each}
        </div>
      {/each}
```

Add `'settings.window_title'` use is already in `en.json`. Delete the now-unused `settings.placeholder_title` and `settings.placeholder_body` entries from `src/lib/i18n/en.json`.

- [ ] **Step 4: The stack follows the settings**

In `src/stack/Stack.svelte`, add these listeners to the `Promise.all([...])` array in `onMount`:

```ts
      listen<{ floating: StackConfig }>('settings:changed', (event) => {
        config = {
          corner: event.payload.floating.corner,
          card_size: event.payload.floating.card_size,
          auto_hide_secs: event.payload.floating.auto_hide_secs,
        };
      }),
      listen('stack:clear', () => {
        cards = [];
      }),
```

and, after the existing `$effect` that reports on `cards.length`, add:

```ts
  // A new corner or card size changes where the window sits and how big it is.
  $effect(() => {
    void config.corner;
    void config.card_size;
    report();
  });
```

- [ ] **Step 5: Check**

Run: `pnpm check && pnpm test && cargo clippy --workspace --all-targets -- -D warnings`
Expected: 0 svelte-check errors, all vitest tests pass, and no clippy warnings.

- [ ] **Step 6: Commit**

```bash
git add src
git commit -m "feat(ui): settings window with four panes, search, accent colour and live floating settings"
```

---

### Task 10: Run it, then CI

**Files:** none, unless a check fails. A failure goes through superpowers:systematic-debugging, and its fix gets a test where the code is testable.

Checks that put things on the user's clipboard or click through the window are the user's to run; run the rest here and list what is left.

- [ ] **Step 1: Everything green locally**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && pnpm check && pnpm test`
Expected: all pass. Report the test counts.

- [ ] **Step 2: Smoke-run the dev build**

Run `pnpm tauri dev --config '{"identifier":"io.ferry.desktop.dev"}'` with the dev settings file deleted. Expected in `~/Library/Logs/io.ferry.desktop.dev/ferry.<today>.log`: `Ferry started`, three `hotkey registered` lines, `pruned old backups`, and no WARN or ERROR line. `ls ~/Library/LaunchAgents | grep -i ferry` prints nothing. Stop the app.

- [ ] **Step 3: The manual checklist for the user**

Write the results table into the PR description or the merge commit, with each row marked "not run" until someone runs it:

| # | Check | How |
| --- | --- | --- |
| 1 | Settings opens from the tray and shows four panes | Tray → Settings… |
| 2 | A change applies at once and survives a restart | Turn off "Show result cards", quit, reopen: still off; `settings.json` has `"enabled": false` under `floating` |
| 3 | Search finds a setting by a word of its help text and highlights it for 2 s | Type "sign in", pick the result |
| 4 | "No settings match" shows for nonsense | Type "zzzz" |
| 5 | Dependent rows grey out with "Turn on 'Optimise copied images' first." | Turn that switch off in Clipboard |
| 6 | The template field shows an example and the red "Unknown token %q" | Type `%f_%q` in Clipboard → File name template; the invalid text is not saved |
| 7 | Compression slider and number fields clamp | Type 0 into "Keep backups for": it becomes 1 |
| 8 | "Show app UI in screenshots" in Settings and in the tray stay in step | Toggle each, watch the other |
| 9 | The Dock icon appears and disappears (macOS) | Toggle "Show Dock icon" |
| 10 | The tray style changes the menu bar icon (macOS) | Pick Classic, then Flat; try Paused and Stopped from the tray menu in each |
| 11 | The ignored-apps picker adds an app by its identifier | Clipboard → Ignored apps → Add app…, pick Safari: `com.apple.Safari` is listed; copy an image in Safari with Running: no card |
| 12 | Cards follow floating settings at once | Change the corner and card size with a card showing |
| 13 | Force clean removes finished jobs' files and keeps running ones | Tray → Force clean working directory after a few copies: `~/Library/Caches/<id>/work` is empty, Recent results is empty, the stack clears; copy a large JPEG and clean while it runs: its card stays |
| 14 | An accent colour other than blue is used (macOS) | System Settings → Appearance → Accent colour → Orange; reopen Settings: switches and the selected pane are orange with white text readable. Try Yellow |
| 15 | Launch at login (release build only) | Install the CI `.dmg`; with the switch on, Ferry is in System Settings → General → Login Items; turn it off and it is gone |

Windows: built but not verified on a real desktop.

- [ ] **Step 4: Push and confirm CI**

Ask first if pushing this branch has not been approved in this session. Then:

```bash
git push -u origin m2c-settings-and-housekeeping
~/.local/bin/gh run watch
```

Expected: the core jobs (Ubuntu, macOS, Windows), `web` and the macOS bundle job pass. Check the Windows bundle job too, without blocking on it (its clippy compiles `platform/windows.rs` and the Windows branches of `prefs.rs` and `commands.rs`; fix any failure there with a ruling in the ledger).
