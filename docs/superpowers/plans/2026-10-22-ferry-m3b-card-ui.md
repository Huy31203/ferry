# M3b Card UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. One Sonnet implementer runs every task in order with no per-task review; one Opus reviewer checks the whole branch at the end. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Put M3a's edit engine on the result cards: rail A with press-and-drag sliders, crop and fit popovers beside the card, a format bar, inline rename, a native context menu, hover hotkeys and the File handling settings, all backed by Tauri commands around `Job::edit`, `Job::fork` and `Job::rename`.

**Architecture:** The card UI stays in the stack page. `Card.svelte` gets rail A, a vertical slider, the format bar and an inline rename field; the crop and fit popovers render in the stack beside their card, so the window (pinned to its corner) grows toward the screen centre. Pure logic lives in `src/lib/edits.ts` and is tested with vitest. Rust adds thin commands over `Job::edit` / `fork` / `rename` in `jobs.rs`, a native context menu in `menu.rs`, hover hotkeys in `hover_keys.rs`, and key-on-demand for the stack panel in `platform`. Whatever ends in the page's own UI (rename, crop, fit) or in M3c's commands (compare, preview) travels from Rust to the page as one `card:action` event, and the page calls M3c's `open_compare` / `quick_preview` by name, so this branch compiles and runs without M3c.

**Tech Stack:** Tauri 2.12 (`tauri::menu`, `popup_menu_at`, `tauri-plugin-global-shortcut` 2.4.1 / `global-hotkey` 0.8, `tauri-plugin-dialog` 2.8), `tauri-nspanel` 2.1.1 (macOS), `windows-sys` 0.61, `clipboard-win` 5.4; Svelte 5 + TypeScript, vitest 5, svelte-check 4; Rust 2024. No new dependencies.

**Spec:** [docs/superpowers/specs/2026-10-22-ferry-cycle2-card-tools-design.md](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) (M3b row of §8; §1 criteria 1–4 and 6; §2 "Text input in the stack", "Hover hotkeys", "Context menu", "Compression slider"; §4.1, §4.3, §4.5; §5.1–§5.6; §6; §7 vitest list; §9 risks 1, 3, 4, 5). Approved mockups (local, git-ignored): `.superpowers/brainstorm/78228-1792661273/content/card-layout.html` (layout A) and `crop-popover.html` (sliders, popover A).

**Depends on:** M3a (`docs/superpowers/plans/2026-10-22-ferry-m3a-edit-engine.md`, branch `feat/m3a-edit-engine`). M3c (`docs/superpowers/plans/2026-10-22-ferry-m3c-compare-preview.md`) is built in parallel on its own branch; this plan only calls its two commands by name.

## Global Constraints

- Implementers run on model `sonnet`; plan writing and reviews run on `opus` (never `fable`).
- Never use `var` in JS/TS.
- Commit trailer: `Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>`.
- Each task ends with `cargo fmt`, `cargo clippy --workspace --all-targets -- -D warnings`, relevant tests, `pnpm check`, then a commit.
- Environment (run from worktree root): `source "$HOME/.cargo/env"; export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"; export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"`.
- Build location: worktree `.worktrees/m3b`, branch `feat/m3b-card-ui` cut from `feat/m3a-edit-engine` with `main` merged in.
- Manual app checks (`pnpm tauri dev`) go in a final task, gated on asking the user first (the live app takes over the real clipboard).
- `cargo test` takes one test-name filter per command. To run two filters, run two commands.
- Every Rust module opens with a `//!` doc comment; tests live in `#[cfg(test)] mod tests` at the end of the file. Match the surrounding comment density and naming.
- UI text lives in `src/lib/i18n/en.json` (a flat map shared with Rust through `ferry_core::i18n::{t, tf}`). Never hard-code English UI text in Rust or Svelte. `t()` in TS is typed on the keys, so a missing key fails `pnpm check`.
- Do not add a test stack. The repo has no jsdom or testing-library and Tauri's `test` feature is off: component behaviour is tested through pure helpers (`src/lib/edits.ts`, `src/lib/cards.ts`, pure Rust functions) and the manual checks in Task 11.
- Do not reference M3c's Rust code (`compare::*`, `preview::*`, `platform::preview`); it is not on this branch. Call `open_compare` / `quick_preview` from the page with `invoke` by name. Until M3c is merged they reject with "command not found", which the page logs and ignores.
- Exact copy from the spec: "Original no longer kept", "First frame only", "Couldn't reach 1 MB — try downscaling" (M3a), "Name taken", "Name can't be empty", "File no longer there", "Larger than the original", "Extreme", "converted". Slider badges: "50 % · 1440×900", "q 60", "Extreme".
- Steps are fixed by the spec: Downscale click −10 points (100 → 90 → … → 10), slider 90–10 % in steps of 5; Compression click −10 quality (then Extreme), slider 100–5 in steps of 5 then Extreme (= `level: "aggressive"` with `compression: 5`). Hover `1`–`9` set 10 %–90 %, `-` steps 10 points down.
- Tauri command arguments are camelCase on the JS side (`keep_both` → `keepBoth`).
- Windows behaviour is built but not verified on a real desktop (spec §1). No Windows Rust target is installed here; Windows-only code is checked by reading it against the `windows-sys` 0.61 signatures quoted in the task.

## Review Focus

1. **Hover hotkeys left registered after the pointer is gone**: the stack hides through `stack_resize(0, 0)` (no mouse-exit arrives), the hovered card is closed under a still pointer, "Clear all", the card turns busy, or the setting is turned off. Expected: Space, ⌘D, `-` and `1`–`9` go back to the frontmost app at once. Pinned in Task 9 (`keys_are_held_only_over_a_card_with_the_setting_on`; `stack_resize`'s zero-size branch calls `hover_keys::set(&app, None)`; vitest "is nothing once the card is gone, busy or has no file") and Task 11 check 10.
2. **Typing into the rename, crop or fit field while the pointer rests on the card**: digits and Space must land in the field, never trigger a downscale or Quick Look. Expected: every `stack_focus(true)` drops the hover hotkeys and the page sends no hover target while a tool is open. Pinned in Task 9 (`a_key_window_never_keeps_the_hover_keys`; vitest "is nothing while a tool has the keyboard or the setting is off").
3. **"Convert and keep both" on a file job with the default In place location**: the twin must not trash or overwrite `shot.png`, the parent card's result, and the twin's Restore must not rewrite it either. Pinned in Task 1 (`a_keep_both_twin_never_touches_its_parents_file`, `a_twin_that_keeps_its_format_writes_beside_the_source`) and Task 11 check 6.
4. **Reveal, drag, copy and the busy check after a rename or conversion**: they must act on the new file, not the old path. Pinned in Task 2 (`a_moved_result_moves_the_busy_check_with_it`; `jobs::rename` re-sends the card with the new `output`) and Task 11 check 2 (Show in Finder after a rename).
5. **The stack left key (keyboard taken from the frontmost app)** when a popover or rename field's card disappears (auto-hide, "Clear all", force clean, closed from its menu), turns busy, or the user switches apps. Expected: the tool closes and `stack_focus(false)` follows. Pinned in Task 4 (vitest "closes when its card is gone, busy, or has no file"; the stack's focus effect keys only off `tool !== null`) and Task 7 (`windowBlur` closes a popover or rename field).

## Setup (once, before Task 1)

```bash
cd /Users/gos/code/rust/ferry
git worktree add .worktrees/m3b -b feat/m3b-card-ui feat/m3a-edit-engine
cd .worktrees/m3b
git merge --no-edit main
source "$HOME/.cargo/env"
export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
pnpm install --frozen-lockfile
ls "$FERRY_GIFSICLE" || pnpm sidecar
grep -n "pub fn fork" crates/ferry-core/src/pipeline.rs
cargo test --workspace 2>&1 | grep "test result" | head -3
```

Expected: the `grep` prints one line; the three `test result` lines say `ok` (M3a's 248 core tests, 22 app tests, 7 sidecar tests, give or take). If `pub fn fork` is missing, the branch is not M3a: stop and report. Keep a ledger at `.superpowers-ledger-m3b.md` in the worktree (uncommitted), one line per task as M3a's ledger does, plus a `Ruling:` line for every deviation from this plan.

## Shared files (M3c runs in parallel)

M3c edits some of the same files. Every M3b edit below sits at a different anchor, so merges stay small; if git still conflicts, keep both sides.

| File | M3b edit | Anchor | M3c's anchor |
| --- | --- | --- | --- |
| `src-tauri/src/lib.rs` | `mod hover_keys;`, `mod menu;`; commands; `menu::init` | `mod hover_keys;` after `mod hotkeys;`, `mod menu;` after `mod logging;` (rustfmt keeps `mod` lines sorted); commands appended at the **end** of `generate_handler!` after `commands::force_clean,`; `menu::init(app)` after `tray::build(app)?;` in `setup` | `mod compare;` after `mod commands;`, `mod preview;` after `mod prefs;`, commands at the **top** of the list |
| `src/lib/i18n/en.json` | `card.no_original` (Task 1) and every other M3b key | the line right after `"rename.gone": "File no longer there",` | right after `"stack.window_title"` |
| `src-tauri/src/platform/mod.rs` | two **new** `pub use` lines per OS (`set_stack_key`, `app_is_active`), each with its `#[cfg]`; the `//!` doc names `write_text` | above each OS's existing `pub use …::{…};` line (rustfmt sorts them there); the existing lines are not edited | `pub mod preview;` per OS |
| `src-tauri/src/windows.rs` | `stack_focus` command (its first line calls `hover_keys::set_keyed`); `hover_keys::set` call in `stack_resize`; `use crate::{hover_keys, platform};` | `stack_focus` between the end of `stack_resize` and `/// Show the Settings window`; the call is the first line of `stack_resize`'s zero-size branch | `set_content_protected` and the end of the file |
| `src-tauri/src/platform/macos.rs`, `windows.rs` | `can_become_key_window`, `set_stack_key`, `app_is_active` | inside the `tauri_panel!` config; new functions appended at the end | not edited by M3c |

## M3c interfaces this plan calls (by name only)

- `invoke('open_compare', { id })`: toggles the compare window for card `id`. Rejects with a plain string such as "Original no longer kept".
- `invoke('quick_preview', { id })`: toggles Quick Look (macOS) or the single-image compare window (Windows).
- M3c's compare window has the Tauri label `"compare"`. This plan never looks it up: Task 9 asks only whether Ferry is the active app (`platform::app_is_active`).
- Until M3c is merged both commands reject with "command … not found"; the card logs it with `console.warn` and nothing else happens. §1 criterion 5 belongs to M3c.

## File map

| File | Responsibility | Task |
| --- | --- | --- |
| `crates/ferry-core/src/job.rs` (modify) | `JobUpdate.source_format`, `edit_hint` | 1 |
| `crates/ferry-core/src/pipeline.rs` (modify) | a keep-both twin never touches its parent's file; `edit_hint` | 1 |
| `crates/ferry-core/src/output.rs` (modify) | `example`: the settings pane's live example path | 10 |
| `src-tauri/src/state.rs` (modify) | `Entry::output` follows runs and renames | 2 |
| `src-tauri/src/jobs.rs` (modify) | `edit`, `convert`, `rename`, `requeue`, `copy_path` | 2, 8 |
| `src-tauri/src/commands.rs` (modify) | `card_edit`, `card_convert`, `card_rename`, `crop_sizes_reset`, `output_preview`, `pick_folder`; `StackConfig` extras | 2, 7, 10 |
| `src-tauri/src/windows.rs` (modify) | `stack_focus`; hover keys off when the stack hides | 4, 9 |
| `src-tauri/src/platform/{mod,macos,windows}.rs` (modify) | `set_stack_key`, `app_is_active` | 4, 9 |
| `src-tauri/src/platform/{macos,windows}_clipboard.rs` (modify) | `write_text` for "Copy path" | 8 |
| `src-tauri/src/menu.rs` (create) | the card's context menu: model, build, routing | 8 |
| `src-tauri/src/hover_keys.rs` (create) | hover hotkeys: register / unregister / route | 9 |
| `src-tauri/src/lib.rs` (modify) | modules, commands, menu and focus hooks | 2, 4, 7, 8, 9, 10 |
| `src/lib/ipc.ts`, `src/lib/settings.ts`, `src/lib/settings.fixture.ts` (modify), `src/lib/card.fixture.ts` (create) | v2 types; `CardAction` (8) | 3, 8 |
| `src/lib/edits.ts` (create), `src/lib/edits.test.ts` (create) | slider, step, format bar, crop, fit, size helpers | 3, 5, 6, 7, 10 |
| `src/lib/cards.ts`, `src/lib/cards.test.ts` (modify) | open tool, rail A, hover target | 4, 5, 9 |
| `src/stack/Card.svelte` (modify) | rail A, rename field, format bar slot, crop frame, note, right-click | 4, 5, 6, 7, 8 |
| `src/stack/Slider.svelte`, `FormatBar.svelte`, `CropPopover.svelte`, `FitPopover.svelte` (create) | the card tools | 5, 6, 7 |
| `src/stack/Stack.svelte` (modify) | open tool, focus, popover rows, `card:action`, hover target | 4, 7, 8, 9 |
| `src/lib/schema.ts`, `src/lib/schema.test.ts` (modify) | File handling pane, hover toggle, crop sizes | 10 |
| `src/settings/Row.svelte` (modify), `src/settings/controls/OutputRule.svelte`, `src/settings/controls/SizeList.svelte` (create) | settings controls | 10 |
| `src/lib/i18n/en.json` (modify) | M3b strings | 1, 3–10 |

---

### Task 1: Core: card fields for the tools, and keep-both keeps the source

**Files:**
- Modify: `crates/ferry-core/src/job.rs` (`JobUpdate`, its round-trip test)
- Modify: `crates/ferry-core/src/pipeline.rs` (`Job` struct, constructors, `update`, `fork`, `put`, `notice`, tests)
- Modify: `src/lib/i18n/en.json` (after `"rename.gone"`)

**Interfaces:**
- Consumes: M3a's `Job::fork(&self) -> Job`, `Job::edit`, `Job::update`, `output::unique(&Path, Option<&Path>) -> PathBuf`, test helpers `Env`, `done_file`, `run`, `convert`, `scale`, `options` in `pipeline::tests`.
- Produces:
  - `JobUpdate.source_format: Option<Format>` (the original's format; `None` on a notice). JSON `"png" | "jpeg" | "gif" | "webp" | null`.
  - `JobUpdate.edit_hint: Option<String>`: why the edit tools are off ("Original no longer kept" or "Animated WebP can't be edited yet"); `None` when `can_edit` or while busy.
  - A forked job (keep-both twin) never writes over, trashes or restores its source file; a twin whose result would land on the source gets `output::unique`'s ` (2)` name instead.
  - i18n key `card.no_original` = "Original no longer kept".

Why: under the default Converted = In place, `fork()` gives the twin the parent's `source_path`, and the twin's first conversion then trashes `shot.png`, which is the parent card's in-place result (spec §4.3 "Convert keep-both … it gets its own card"; R§ safety: never destroy what the user still sees).

- [ ] **Step 1: Write the failing tests**

In `crates/ferry-core/src/pipeline.rs`, inside `mod tests`, insert right before `#[test] fn sizes_read_like_the_cards()`:

```rust
    #[test]
    fn a_keep_both_twin_never_touches_its_parents_file() {
        let env = Env::new();
        let (job, path) = done_file(&env, "shot.png");
        let parent = fs::read(&path).unwrap();
        let o = options();
        let mut twin = job.fork();
        twin.edit(convert(Format::WebP));
        let update = run(&mut twin, Level::Normal, &o, &env).0.unwrap().update;

        let webp = env.dir.path().join("shot.webp");
        assert_eq!(update.output, Some(webp.clone()));
        assert_eq!(fs::read(&path).unwrap(), parent, "the parent's file stays");

        let restored = twin.restore(&o, &mut |_, _, _| Ok(())).update;
        assert_eq!(restored.state, JobState::Restored);
        assert!(!webp.exists(), "the twin's own file went to the Trash");
        assert_eq!(
            fs::read(&path).unwrap(),
            parent,
            "and the parent's still stays"
        );
    }

    #[test]
    fn a_twin_that_keeps_its_format_writes_beside_the_source() {
        let env = Env::new();
        let (job, path) = done_file(&env, "shot.png");
        let parent = fs::read(&path).unwrap();
        let mut twin = job.fork();
        twin.edit(scale(50));
        let update = run(&mut twin, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(update.output, Some(env.dir.path().join("shot (2).png")));
        assert_eq!(fs::read(&path).unwrap(), parent);
    }

    #[test]
    fn a_card_says_why_its_tools_are_off() {
        let env = Env::new();
        let mut animated = Job::clipboard(
            Format::WebP,
            testutil::animated_webp(),
            false,
            "%f",
            &env.work(),
            now(),
        );
        let update = run(&mut animated, Level::Normal, &options(), &env)
            .0
            .unwrap()
            .update;
        assert_eq!(
            update.edit_hint.as_deref(),
            Some("Animated WebP can't be edited yet")
        );
        assert_eq!(update.source_format, Some(Format::WebP));

        let (job, _) = done_file(&env, "shot.png");
        let done = job.update(JobState::Done, None);
        assert!(done.can_edit);
        assert_eq!(done.edit_hint, None);
        fs::remove_file(done.thumb_path.unwrap()).unwrap();
        let gone = job.update(JobState::Done, None);
        assert!(!gone.can_edit);
        assert_eq!(gone.edit_hint.as_deref(), Some("Original no longer kept"));
        assert_eq!(job.update(JobState::Queued, None).edit_hint, None);
    }

```

In `crates/ferry-core/src/job.rs`, in the test `an_update_round_trips_through_json`, change the end of the `JobUpdate` literal from

```rust
            note: Some("Couldn't reach 1 MB".into()),
        };
```

to

```rust
            note: Some("Couldn't reach 1 MB".into()),
            source_format: Some(Format::Png),
            edit_hint: None,
        };
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core twin`
Expected: FAIL to compile — `no field source_format on type JobUpdate` (and `edit_hint`).

- [ ] **Step 3: Add the two `JobUpdate` fields**

In `crates/ferry-core/src/job.rs`, change the end of `pub struct JobUpdate` from

```rust
    /// An amber note, such as "Couldn't reach 1 MB — try downscaling".
    pub note: Option<String>,
}
```

to

```rust
    /// An amber note, such as "Couldn't reach 1 MB — try downscaling".
    pub note: Option<String>,
    /// The original's format: the format bar's GIF chip and "First frame only".
    pub source_format: Option<Format>,
    /// Why the edit tools are off, for their tooltip; `None` when they are on
    /// or the job is busy.
    pub edit_hint: Option<String>,
}
```

In `src/lib/i18n/en.json`, right after the line `  "rename.gone": "File no longer there",` insert:

```json
  "card.no_original": "Original no longer kept",
```

- [ ] **Step 4: Fill them in and make the twin leave the source alone**

In `crates/ferry-core/src/pipeline.rs`:

4a. In `pub struct Job`, after the `changed` field:

```rust
    /// `output` (and the clipboard) hold a result, not the original.
    changed: bool,
    /// A "Convert and keep both" twin: the source belongs to the parent's
    /// card, so the twin never overwrites, trashes or restores it.
    twin: bool,
}
```

4b. In `Job::clipboard` and `Job::file`, add `twin: false,` right after `changed: false,` in the struct literal. In `Job::fork`, add `twin: true,` right after `changed: false,`.

4c. In `Job::update`, replace

```rust
        let editable = self.source_format == Format::Gif || !self.animated;
        JobUpdate {
```

with

```rust
        let editable = self.source_format == Format::Gif || !self.animated;
        let backed_up = self.backup.as_ref().is_some_and(|b| b.exists());
        let edit_hint = if busy || (editable && backed_up) {
            None
        } else if !editable {
            Some(tf(
                "error.animated_edit",
                &[("format", self.source_format.label())],
            ))
        } else {
            Some(t("card.no_original"))
        };
        JobUpdate {
```

and in the same literal replace

```rust
            can_edit: !busy && editable && self.backup.as_ref().is_some_and(|b| b.exists()),
            edits: self.edits.clone(),
            note: if done { self.note.clone() } else { None },
            state,
```

with

```rust
            can_edit: !busy && editable && backed_up,
            edits: self.edits.clone(),
            note: if done { self.note.clone() } else { None },
            source_format: Some(self.source_format),
            edit_hint,
            state,
```

4d. In `Job::put`, `Source::File` arm, replace

```rust
                let path = if wanted == source {
```

with

```rust
                let path = if wanted == source && !self.twin {
```

and replace

```rust
                let trash_source = kind == OutputKind::Converted
                    && o.output.converted.location == Location::InPlace
                    && path != source;
```

with

```rust
                let trash_source = kind == OutputKind::Converted
                    && o.output.converted.location == Location::InPlace
                    && path != source
                    && !self.twin;
```

(With both changes a twin's `Written.source_changed` is always false, so its `undo` and `restore` never rewrite the source.)

4e. In `pub fn notice`, after `note: None,` add:

```rust
        source_format: None,
        edit_hint: None,
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -p ferry-core twin`
Expected: PASS, 2 tests (`a_keep_both_twin_never_touches_its_parents_file`, `a_twin_that_keeps_its_format_writes_beside_the_source`).

Run: `cargo test -p ferry-core a_card_says_why_its_tools_are_off`
Expected: PASS, 1 test.

Run: `cargo test --workspace 2>&1 | grep "test result"`
Expected: every line `ok`, 0 failed (core goes up by 3).

- [ ] **Step 6: Format, lint, check, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
pnpm check
git add crates/ferry-core/src/job.rs crates/ferry-core/src/pipeline.rs src/lib/i18n/en.json
git commit -m "fix(core): a keep-both twin never touches its parent's file; cards say why tools are off

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clippy prints no warnings; `pnpm check` reports `0 errors and 0 warnings` (the TS types do not read the new fields yet).

---

### Task 2: App: fresh card paths and the edit, convert and rename commands

**Files:**
- Modify: `src-tauri/src/state.rs` (`Entry`, `output_busy`, tests)
- Modify: `src-tauri/src/jobs.rs` (`rerun`, new `edit` / `convert` / `rename` / `requeue` / `editable_entry`, `restore`, `run`, `output`, `force_clean`, `entry_name`)
- Modify: `src-tauri/src/commands.rs` (`StackConfig`, `stack_ready`, three commands)
- Modify: `src-tauri/src/lib.rs` (`generate_handler!`, at the end)

**Interfaces:**
- Consumes: Task 1's `JobUpdate.edit_hint`; M3a's `Job::edit(&mut self, EditsPatch)`, `Job::fork(&self) -> Job`, `Job::rename(&mut self, &str) -> Result<(), RenameError>`, `RenameError::message(&self) -> String`, `EditsPatch` (`Deserialize`, every field `Option<Option<T>>` except `framing` / `level`).
- Produces:
  - `Entry::output(&self) -> PathBuf`, `Entry::set_output(&self, PathBuf)`; the field is private. Called after every run, restore and rename.
  - `jobs::edit(app, &JobId, EditsPatch) -> Result<(), String>`; `jobs::convert(app, &JobId, Format, keep_both: bool) -> Result<(), String>`; `jobs::rename(app, &JobId, &str) -> Result<(), String>` (`Err` is the inline message).
  - Commands (JS names and args): `card_edit({ id, edits })`, `card_convert({ id, format, keepBoth })`, `card_rename({ id, stem })`; all reject with a display string.
  - `stack_ready` and the page's `StackConfig` gain `compression: number`, `crop_sizes: [number, number][]`, `hover_hotkeys: boolean`.

Why: `Entry::output` is captured once at creation, so after a conversion (`shot.webp`) or a rename, Show in Finder, the force-clean keep list and the "two jobs write one file" check use the old path (M3a handover, "Open for M3b").

- [ ] **Step 1: Write the failing test**

In `src-tauri/src/state.rs`, inside `mod tests`, insert right before `#[test] fn two_jobs_never_write_the_same_file()`:

```rust
    #[test]
    fn a_moved_result_moves_the_busy_check_with_it() {
        let mut jobs = HashMap::new();
        let busy = entry("/w/b.webp", JobState::Queued);
        let idle = entry("/w/a.png", JobState::Done);
        for e in [&busy, &idle] {
            let id = e.job.lock().unwrap().id.clone();
            jobs.insert(id, e.clone());
        }
        assert!(!output_busy(&jobs, &idle));
        // The idle card was converted to b.webp, which the busy job also writes.
        idle.set_output(PathBuf::from("/w/b.webp"));
        assert_eq!(idle.output(), PathBuf::from("/w/b.webp"));
        assert!(output_busy(&jobs, &idle));
    }

```

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p ferry a_moved_result`
Expected: FAIL to compile — `no method named set_output found for struct Arc<Entry>`.

- [ ] **Step 3: Make `Entry::output` follow the job**

In `src-tauri/src/state.rs`, replace

```rust
/// A job, its cancel switch and the card it last showed. `job` stays locked
/// while the job runs, so `output` is kept outside it.
pub struct Entry {
    pub job: Mutex<Job>,
    pub output: PathBuf,
```

with

```rust
/// A job, its cancel switch and the card it last showed. `job` stays locked
/// while the job runs, so its current file is kept outside it.
pub struct Entry {
    pub job: Mutex<Job>,
    /// The job's current file: follows every run, conversion and rename.
    /// Never held while taking another lock.
    output: Mutex<PathBuf>,
```

In `Entry::new`, replace `output: job.output.clone(),` with `output: Mutex::new(job.output.clone()),`. Add these methods at the top of `impl Entry`, right before `pub fn is_busy`:

```rust
    pub fn output(&self) -> PathBuf {
        self.output.lock().unwrap().clone()
    }

    /// Call after anything that may move the job's file: a run, a restore, a rename.
    pub fn set_output(&self, path: PathBuf) {
        *self.output.lock().unwrap() = path;
    }

```

Replace the body of `output_busy` with:

```rust
    let output = entry.output();
    jobs.values()
        .any(|e| !Arc::ptr_eq(e, entry) && e.is_busy() && e.output() == output)
```

- [ ] **Step 4: Keep it fresh in `jobs.rs` and add the three entry points**

In `src-tauri/src/jobs.rs`:

4a. Imports: add `use ferry_core::edit::EditsPatch;` after `use ferry_core::cancel::CancelToken;`.

4b. Replace the whole `pub fn rerun` with:

```rust
/// Run the job again from its original: retry and re-optimise keep its level,
/// "Try aggressive" asks for Aggressive.
pub fn rerun(app: &AppHandle, id: &JobId, aggressive: bool) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let level = if aggressive {
        Level::Aggressive
    } else {
        relock(&entry.job).level
    };
    requeue(app, entry, level);
    Ok(())
}

/// Merge what the card asked for into its recipe and run it on the original
/// again (spec §4.5 `card_edit`).
pub fn edit(app: &AppHandle, id: &JobId, patch: EditsPatch) -> Result<(), String> {
    let entry = editable_entry(app, id)?;
    let level = {
        let mut job = relock(&entry.job);
        job.edit(patch);
        job.level
    };
    requeue(app, entry, level);
    Ok(())
}

/// Convert the card's result (replacing its last conversion), or with
/// `keep_both` start a twin job on the same original with its own card.
pub fn convert(app: &AppHandle, id: &JobId, format: Format, keep_both: bool) -> Result<(), String> {
    let patch = EditsPatch {
        format: Some(Some(format)),
        ..EditsPatch::default()
    };
    if !keep_both {
        return edit(app, id, patch);
    }
    let entry = editable_entry(app, id)?;
    let (twin, level) = {
        let mut twin = relock(&entry.job).fork();
        twin.edit(patch);
        let level = twin.level;
        (twin, level)
    };
    if start(app, twin, level, None) {
        Ok(())
    } else {
        Err(tf("error.busy", &[("name", &entry_name(&entry))]))
    }
}

/// Rename the card's file in its folder; the card keeps its state and sizes.
/// `Err` is the inline message ("Name taken", …).
pub fn rename(app: &AppHandle, id: &JobId, stem: &str) -> Result<(), String> {
    let entry = idle_entry(app, id)?;
    let card = {
        let mut job = relock(&entry.job);
        job.rename(stem).map_err(|e| e.message())?;
        entry.set_output(job.output.clone());
        let last = entry.last.lock().unwrap().clone();
        job.update(last.state, last.after)
    };
    *entry.last.lock().unwrap() = card.clone();
    app.state::<AppState>().remember(&card);
    show(app, card);
    tray::rebuild(app);
    Ok(())
}

/// Show the job's Queued card and run it again at `level`.
fn requeue(app: &AppHandle, entry: Arc<Entry>, level: Level) {
    let queued = relock(&entry.job).update(JobState::Queued, None);
    *entry.cancel.lock().unwrap() = CancelToken::new();
    entry.begin_rerun(queued.clone());
    show(app, queued);
    tray::refresh(app);
    spawn(app, entry, level);
}
```

4c. In `pub fn restore`, replace

```rust
    let finished = relock(&entry.job).restore(&options, &mut |format, bytes, file| {
        deliver(app, format, bytes, file, None)
    });
```

with

```rust
    let finished = {
        let mut job = relock(&entry.job);
        let finished = job.restore(&options, &mut |format, bytes, file| {
            deliver(app, format, bytes, file, None)
        });
        entry.set_output(job.output.clone());
        finished
    };
```

4d. In `fn run`, right after the `job.run(...)` call's closing `);` and before `drop(job);`, add `entry.set_output(job.output.clone());`.

4e. In `pub fn output`, replace `.output.clone();` with `.output();` (the line becomes `let path = state.jobs.lock().unwrap().get(id)?.output();`). In `force_clean`, replace `jobs.values().map(|e| e.output.clone()).collect();` with `jobs.values().map(|e| e.output()).collect();`. In `fn entry_name`, replace `.output` with `.output()`.

4f. Right before `fn run_options`, add:

```rust
/// An idle card whose edit tools apply; `Err` is why not ("Original no longer kept").
fn editable_entry(app: &AppHandle, id: &JobId) -> Result<Arc<Entry>, String> {
    let entry = idle_entry(app, id)?;
    let last = entry.last.lock().unwrap().clone();
    if !last.can_edit {
        return Err(last.edit_hint.unwrap_or_else(|| t("card.no_original")));
    }
    Ok(entry)
}

```

Lock order stays `job` → `last` (as in `run`'s emit closure); `output` is a leaf lock.

- [ ] **Step 5: Commands and the stack's extra settings**

In `src-tauri/src/commands.rs`:

5a. Imports: add `use ferry_core::edit::EditsPatch;` before `use ferry_core::i18n::t;`, and `use ferry_core::sniff::Format;` after the `ferry_core::settings` line.

5b. Replace `pub struct StackConfig` and `pub fn stack_ready` with:

```rust
#[derive(Serialize)]
pub struct StackConfig {
    corner: Corner,
    card_size: CardSize,
    auto_hide_secs: u32,
    /// Where the Compression slider starts when a card has no quality of its own.
    compression: u8,
    crop_sizes: Vec<(u32, u32)>,
    hover_hotkeys: bool,
}

/// The stack page is listening: hand it its settings and any cards sent before.
#[tauri::command]
pub fn stack_ready(app: AppHandle) -> (StackConfig, Vec<JobUpdate>) {
    let state = app.state::<AppState>();
    let settings = state.settings.read().unwrap().clone();
    let mut inbox = state.stack.lock().unwrap();
    inbox.ready = true;
    let config = StackConfig {
        corner: settings.floating.corner,
        card_size: settings.floating.card_size,
        auto_hide_secs: settings.floating.auto_hide_secs,
        compression: settings.images.compression,
        crop_sizes: settings.images.crop_sizes,
        hover_hotkeys: settings.floating.hover_hotkeys,
    };
    (config, std::mem::take(&mut inbox.pending))
}
```

5c. Right before `#[tauri::command] pub fn card_cancel`, add:

```rust
/// Merge `edits` into the card's recipe and run it again (spec §4.5).
#[tauri::command]
pub async fn card_edit(app: AppHandle, id: JobId, edits: EditsPatch) -> Result<(), String> {
    blocking(app, move |app| jobs::edit(app, &id, edits)).await
}

/// The format bar: convert, or with `keep_both` (JS `keepBoth`) add a twin card.
#[tauri::command]
pub async fn card_convert(
    app: AppHandle,
    id: JobId,
    format: Format,
    keep_both: bool,
) -> Result<(), String> {
    blocking(app, move |app| jobs::convert(app, &id, format, keep_both)).await
}

/// `Err` is the inline message under the name field.
#[tauri::command]
pub async fn card_rename(app: AppHandle, id: JobId, stem: String) -> Result<(), String> {
    blocking(app, move |app| jobs::rename(app, &id, &stem)).await
}

```

5d. In `src-tauri/src/lib.rs`, at the end of `generate_handler!`, after `commands::force_clean,` add:

```rust
            commands::card_edit,
            commands::card_convert,
            commands::card_rename,
```

- [ ] **Step 6: Run the tests to verify they pass**

Run: `cargo test -p ferry a_moved_result`
Expected: PASS, 1 test.

Run: `cargo test -p ferry`
Expected: PASS, 0 failed (23 tests).

- [ ] **Step 7: Format, lint, check, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
pnpm check
git add src-tauri/src/state.rs src-tauri/src/jobs.rs src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(app): card_edit, card_convert and card_rename; card paths follow the job

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clippy clean; `pnpm check` 0 errors.

---

### Task 3: Frontend: v2 types and the step helpers

**Files:**
- Modify: `src/lib/ipc.ts` (`JobUpdate`, `StackConfig`; new `Format`, `Size`, `Edits`, `EditsPatch`)
- Modify: `src/lib/settings.ts` (`OutputLocation`, `OutputRule`, `images.output`, `images.crop_sizes`, `floating.hover_hotkeys`)
- Modify: `src/lib/settings.fixture.ts` (version 2 defaults)
- Create: `src/lib/card.fixture.ts` (the test card, moved out of `cards.test.ts`)
- Modify: `src/lib/cards.test.ts` (import the fixture)
- Create: `src/lib/edits.ts`, `src/lib/edits.test.ts`
- Modify: `src/stack/Stack.svelte` (default config, `settings:changed` listener)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: Task 1/2 JSON shapes: `JobUpdate` (M3a fields plus `source_format`, `edit_hint`), `StackConfig` (`compression`, `crop_sizes`, `hover_hotkeys`); M3a's serde shapes: `Format` lowercase (`"png" | "jpeg" | "gif" | "webp"`), `Size` tagged `kind` snake_case (`{kind:'exact',width,height}`, `{kind:'width',width}`, `{kind:'height',height}`, `{kind:'long_edge',edge}`), `Level` `"normal" | "aggressive"`, `Framing` `"centre" | "smart"`, tuples as arrays.
- Produces (later tasks import these exact names):
  - `src/lib/ipc.ts`: `type Format`, `type Size`, `interface Edits`, `type EditsPatch = Partial<Edits>`, the full `JobUpdate`, `StackConfig`.
  - `src/lib/settings.ts`: `type OutputLocation = 'in_place' | 'same_folder' | 'folder' | 'temporary'`, `interface OutputRule { location; folder; template }`.
  - `src/lib/card.fixture.ts`: `card(state: JobState, extra?: Partial<JobUpdate>): JobUpdate` (a done-able 2880×1800 PNG clipboard card, `can_edit: true`, default `edits`).
  - `src/lib/edits.ts`: `type Quality = number | 'extreme'`; `SCALE_STOPS: number[]` (100, 90 … 10); `QUALITY_STOPS: Quality[]` (100 … 5, `'extreme'`); `nextScale(scale: number | null): number`; `currentQuality(edits: Edits, fallback: number): Quality`; `nextQuality(q: Quality): Quality`; `scalePatch(scale: number): EditsPatch`; `qualityPatch(q: Quality): EditsPatch`; `scaleLabel(scale: number, dims: [number, number] | null, edits: Edits): string`; `qualityLabel(q: Quality): string`; `sliderIndex(start: number, dy: number, count: number, track: number): number`; `stopIndex<T extends number | string>(stops: T[], value: T): number`.

- [ ] **Step 1: Write the failing tests**

Create `src/lib/edits.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { card } from './card.fixture';
import {
  QUALITY_STOPS,
  SCALE_STOPS,
  currentQuality,
  nextQuality,
  nextScale,
  qualityLabel,
  qualityPatch,
  scaleLabel,
  scalePatch,
  sliderIndex,
  stopIndex,
} from './edits';

const edits = card({ kind: 'done' }).edits;

describe('downscale steps', () => {
  it('has 100 on top, then 90 to 10 in fives', () => {
    expect(SCALE_STOPS[0]).toBe(100);
    expect(SCALE_STOPS[1]).toBe(90);
    expect(SCALE_STOPS.at(-1)).toBe(10);
    expect(SCALE_STOPS).toHaveLength(18);
  });

  it('clicks down ten points and stops at 10', () => {
    expect(nextScale(null)).toBe(90);
    expect(nextScale(90)).toBe(80);
    expect(nextScale(15)).toBe(10);
    expect(nextScale(10)).toBe(10);
  });

  it('sends a percentage, or clears it at 100', () => {
    expect(scalePatch(50)).toEqual({ scale: 50 });
    expect(scalePatch(100)).toEqual({ scale: null });
  });

  it('labels the live target with the result size', () => {
    expect(scaleLabel(50, [2880, 1800], edits)).toBe('50 % · 1440×900');
    expect(scaleLabel(10, [3, 3], edits)).toBe('10 % · 1×1');
    expect(scaleLabel(50, null, edits)).toBe('50 %');
    expect(scaleLabel(50, [2880, 1800], { ...edits, aspect: [16, 9] })).toBe('50 %');
  });
});

describe('compression steps', () => {
  it('runs from 100 to 5 in fives, then Extreme', () => {
    expect(QUALITY_STOPS[0]).toBe(100);
    expect(QUALITY_STOPS.at(-2)).toBe(5);
    expect(QUALITY_STOPS.at(-1)).toBe('extreme');
    expect(QUALITY_STOPS).toHaveLength(21);
  });

  it('starts from the card, else from the settings', () => {
    expect(currentQuality(edits, 80)).toBe(80);
    expect(currentQuality({ ...edits, compression: 60 }, 80)).toBe(60);
    expect(currentQuality({ ...edits, compression: 5, level: 'aggressive' }, 80)).toBe('extreme');
  });

  it('clicks down ten, then Extreme, and stays there', () => {
    expect(nextQuality(80)).toBe(70);
    expect(nextQuality(15)).toBe(5);
    expect(nextQuality(10)).toBe('extreme');
    expect(nextQuality('extreme')).toBe('extreme');
  });

  it('sends Extreme as aggressive at quality 5', () => {
    expect(qualityPatch('extreme')).toEqual({ compression: 5, level: 'aggressive' });
    expect(qualityPatch(60)).toEqual({ compression: 60, level: 'normal' });
  });

  it('labels quality and Extreme', () => {
    expect(qualityLabel(60)).toBe('q 60');
    expect(qualityLabel('extreme')).toBe('Extreme');
  });
});

describe('slider drag', () => {
  it('moves one stop per step of the track and stays in range', () => {
    expect(sliderIndex(0, 0, 18, 136)).toBe(0);
    expect(sliderIndex(0, 8, 18, 136)).toBe(1);
    expect(sliderIndex(5, -400, 18, 136)).toBe(0);
    expect(sliderIndex(5, 400, 18, 136)).toBe(17);
  });

  it('finds where a value sits, or the stop just below it', () => {
    expect(stopIndex(SCALE_STOPS, 50)).toBe(9);
    expect(stopIndex(SCALE_STOPS, 100)).toBe(0);
    expect(stopIndex(QUALITY_STOPS, 'extreme')).toBe(20);
    expect(stopIndex(QUALITY_STOPS, 82)).toBe(4);
  });
});
```

Create `src/lib/card.fixture.ts`:

```ts
import type { JobState, JobUpdate } from './ipc';

/** A 2880×1800 PNG clipboard card in `state`, for tests; `extra` overrides any field. */
export function card(state: JobState, extra: Partial<JobUpdate> = {}): JobUpdate {
  return {
    id: 'a',
    state,
    name: 'shot',
    ext: 'png',
    before: 2_000_000,
    after: null,
    detail: '',
    thumb_path: '/b/shot.png',
    source: 'clipboard',
    can_restore: false,
    can_retry: false,
    output: '/w/shot.png',
    dims_before: [2880, 1800],
    dims_after: null,
    format: 'png',
    converted: false,
    can_edit: true,
    edits: {
      aspect: null,
      framing: 'centre',
      size: null,
      scale: null,
      format: null,
      compression: null,
      fit_under: null,
      level: 'normal',
    },
    note: null,
    source_format: 'png',
    edit_hint: null,
    ...extra,
  };
}
```

In `src/lib/cards.test.ts`, delete the local `function card(...) { ... }` and the line `import type { JobState, JobUpdate } from './ipc';`, and add `import { card } from './card.fixture';` after the `./cards` import.

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm test`
Expected: FAIL — `src/lib/edits.test.ts` cannot resolve `./edits`; `pnpm check` (run it too) reports `card.fixture.ts` fields that `JobUpdate` does not have (`dims_before` …).

- [ ] **Step 3: The v2 types**

In `src/lib/ipc.ts`, replace everything from `export interface JobUpdate {` through the end of `export interface StackConfig { … }` with:

```ts
export type Format = 'png' | 'jpeg' | 'gif' | 'webp';

/** Mirrors ferry_core::edit::Size (serde tag `kind`). */
export type Size =
  | { kind: 'exact'; width: number; height: number }
  | { kind: 'width'; width: number }
  | { kind: 'height'; height: number }
  | { kind: 'long_edge'; edge: number };

/** Mirrors ferry_core::edit::Edits: the recipe every run applies to the original. */
export interface Edits {
  aspect: [number, number] | null;
  framing: 'centre' | 'smart';
  size: Size | null;
  scale: number | null;
  format: Format | null;
  compression: number | null;
  fit_under: number | null;
  level: 'normal' | 'aggressive';
}

/** What `card_edit` sends (ferry_core::edit::EditsPatch): an absent field is left alone, `null` clears it. */
export type EditsPatch = Partial<Edits>;

export interface JobUpdate {
  id: string;
  state: JobState;
  name: string;
  ext: string;
  before: number;
  after: number | null;
  detail: string;
  thumb_path: string | null;
  source: 'clipboard' | 'file';
  can_restore: boolean;
  can_retry: boolean;
  output: string | null;
  dims_before: [number, number] | null;
  dims_after: [number, number] | null;
  format: Format | null;
  converted: boolean;
  can_edit: boolean;
  edits: Edits;
  note: string | null;
  source_format: Format | null;
  edit_hint: string | null;
}

export type Corner = 'top_left' | 'top_right' | 'bottom_left' | 'bottom_right';

/** The settings the stack page needs (commands::stack_ready). */
export interface StackConfig {
  corner: Corner;
  card_size: 'default' | 'compact';
  auto_hide_secs: number;
  /** Where the Compression slider starts on a card without its own quality. */
  compression: number;
  crop_sizes: [number, number][];
  hover_hotkeys: boolean;
}
```

In `src/lib/settings.ts`, insert above `/** Mirrors ferry_core::settings::Settings …`:

```ts
export type OutputLocation = 'in_place' | 'same_folder' | 'folder' | 'temporary';

/** Mirrors ferry_core::settings::OutputRule. `folder` keeps a leading `~`. */
export interface OutputRule {
  location: OutputLocation;
  folder: string;
  template: string;
}

```

and in `interface Settings` add `output: { optimised: OutputRule; converted: OutputRule };` and `crop_sizes: [number, number][];` after `max_size_mb: number;` in `images`, and `hover_hotkeys: boolean;` after `follow_cursor: boolean;` in `floating`.

In `src/lib/settings.fixture.ts`: change `version: 1,` to `version: 2,`; after `max_size_mb: 50,` in `images` add

```ts
    output: {
      optimised: { location: 'in_place', folder: '~/Pictures/Ferry', template: '%f_optimised.%e' },
      converted: { location: 'in_place', folder: '~/Pictures/Ferry', template: '%f.%e' },
    },
    crop_sizes: [
      [1200, 628],
      [1080, 1080],
      [1920, 1080],
      [1280, 720],
    ],
```

and after `follow_cursor: true,` in `floating` add `hover_hotkeys: true,`.

In `src/stack/Stack.svelte`:
- add `import type { Settings } from '../lib/settings';` after the `../lib/ipc` import;
- replace the `let config = $state<StackConfig>({ … });` line with

```ts
  let config = $state<StackConfig>({
    corner: 'bottom_right',
    card_size: 'default',
    auto_hide_secs: 6,
    compression: 80,
    crop_sizes: [],
    hover_hotkeys: true,
  });
```

- replace the `listen<{ floating: StackConfig }>('settings:changed', …)` entry with

```ts
      listen<Settings>('settings:changed', (event) => {
        const { floating, images } = event.payload;
        config = {
          corner: floating.corner,
          card_size: floating.card_size,
          auto_hide_secs: floating.auto_hide_secs,
          compression: images.compression,
          crop_sizes: images.crop_sizes,
          hover_hotkeys: floating.hover_hotkeys,
        };
      }),
```

- [ ] **Step 4: The step helpers**

Create `src/lib/edits.ts`:

```ts
import { t } from './i18n';
import type { Edits, EditsPatch } from './ipc';

/** A Compression stop: an encoder quality 5–100, or Extreme (aggressive at quality 5). */
export type Quality = number | 'extreme';

/** Downscale slider stops, top to bottom: 100 % (no downscale), then 90 % … 10 % in steps of 5. */
export const SCALE_STOPS: number[] = [100, ...Array.from({ length: 17 }, (_, i) => 90 - i * 5)];

/** Compression slider stops, top to bottom: quality 100 … 5 in steps of 5, then Extreme. */
export const QUALITY_STOPS: Quality[] = [
  ...Array.from({ length: 20 }, (_, i) => 100 - i * 5),
  'extreme',
];

/** One click on Downscale: 10 points below the current scale (100 → 90 → … → 10). */
export function nextScale(scale: number | null): number {
  return Math.max(10, (scale ?? 100) - 10);
}

/** The card's quality: its own, else the settings' `fallback`. */
export function currentQuality(edits: Edits, fallback: number): Quality {
  if (edits.level === 'aggressive' && edits.compression === 5) return 'extreme';
  return edits.compression ?? fallback;
}

/** One click on Compression: 10 quality lower, then Extreme. */
export function nextQuality(quality: Quality): Quality {
  if (quality === 'extreme' || quality - 10 < 5) return 'extreme';
  return quality - 10;
}

export function scalePatch(scale: number): EditsPatch {
  return { scale: scale >= 100 ? null : scale };
}

export function qualityPatch(quality: Quality): EditsPatch {
  return quality === 'extreme'
    ? { compression: 5, level: 'aggressive' }
    : { compression: quality, level: 'normal' };
}

/** `side × percent`, rounded like ferry_core's `scaled`, at least 1. */
function scaled(side: number, percent: number): number {
  return Math.max(1, Math.round((side * percent) / 100));
}

/** "50 % · 1440×900"; just "50 %" when a crop changes the size first or the size is unknown. */
export function scaleLabel(scale: number, dims: [number, number] | null, edits: Edits): string {
  if (!dims || edits.aspect !== null) return t('slider.scale_only', { n: scale });
  return t('slider.scale', { n: scale, w: scaled(dims[0], scale), h: scaled(dims[1], scale) });
}

/** "q 60" or "Extreme". */
export function qualityLabel(quality: Quality): string {
  return quality === 'extreme' ? t('slider.extreme') : t('slider.quality', { q: quality });
}

/** The stop a press-and-drag lands on: `start` moved by the drag, one stop per `track / (count - 1)` px, kept in range. */
export function sliderIndex(start: number, dy: number, count: number, track: number): number {
  const step = track / Math.max(1, count - 1);
  return Math.min(count - 1, Math.max(0, start + Math.round(dy / step)));
}

/** Where a stop list holds `value`; the nearest stop below it when it falls between stops. */
export function stopIndex<T extends number | string>(stops: T[], value: T): number {
  const exact = stops.indexOf(value);
  if (exact !== -1 || typeof value !== 'number') return Math.max(0, exact);
  const below = stops.findIndex((stop) => typeof stop === 'number' && stop <= value);
  return below === -1 ? stops.length - 1 : below;
}
```

In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "slider.scale": "{n} % · {w}×{h}",
  "slider.scale_only": "{n} %",
  "slider.quality": "q {q}",
  "slider.extreme": "Extreme",
```

- [ ] **Step 5: Run the tests to verify they pass**

Run: `pnpm test`
Expected: PASS — 8 files, 50 tests, 0 failed.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 6: Format, lint, check, commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ferry-core i18n
git add src/lib src/stack/Stack.svelte
git commit -m "feat(ui): settings v2 and card types, slider and step helpers

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: fmt and clippy clean (no Rust changed); the i18n tests pass (en.json still a flat map that Rust parses).

---

### Task 4: Key-on-demand and inline rename (the spike)

**Files:**
- Modify: `src-tauri/src/platform/macos.rs` (imports, `tauri_panel!` config, new functions at the end)
- Modify: `src-tauri/src/platform/windows.rs` (imports, statics, `apply_tool_window_style`, new function at the end)
- Modify: `src-tauri/src/platform/mod.rs` (one new `pub use` per OS)
- Modify: `src-tauri/src/windows.rs` (`stack_focus`, between `stack_resize` and `open_settings`)
- Modify: `src-tauri/src/lib.rs` (`generate_handler!`, at the end)
- Modify: `src/lib/cards.ts`, `src/lib/cards.test.ts` (`Tool`, `OpenTool`, `openTool`)
- Modify: `src/stack/Card.svelte` (name chip → rename field; auto-hide holds while a tool is open)
- Modify: `src/stack/Stack.svelte` (`tool` state, focus effect)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: Task 2's `card_rename({ id, stem })` (rejects with "Name taken", "Name can't be empty", "Can't use / \ : or a reserved name", "File no longer there"); Task 3's `card.fixture.ts`; `tauri_nspanel` 2.1.1 (`panel!` `config:` values are expressions pasted into the generated `canBecomeKeyWindow` method, so they are read on every call; `Panel::make_key_window`, `Panel::resign_key_window`); `WebviewWindow::set_focusable` (tauri 2.12); `windows-sys` 0.61 `GetForegroundWindow() -> HWND`, `SetForegroundWindow(HWND) -> BOOL` (both in `Win32::UI::WindowsAndMessaging`, a feature already enabled; `HWND` is `*mut c_void`).
- Produces:
  - `platform::set_stack_key(window: &WebviewWindow, on: bool) -> tauri::Result<()>` on both OSes.
  - Command `stack_focus({ on })`: sync (runs on the main thread, like `stack_resize`).
  - `src/lib/cards.ts`: `type Tool = 'rename' | 'crop' | 'fit' | 'slider'`; `interface OpenTool { id: string; kind: Tool }`; `openTool(tool: OpenTool | null, cards: JobUpdate[]): OpenTool | null`.
  - `Card.svelte` props: `tool?: Tool | null` (this card's open tool) and `ontool: (kind: Tool | null) => void`.
  - `Stack.svelte`: `let tool = $state<OpenTool | null>(null)` and `setTool(id: string, kind: Tool | null)`; `stack_focus` follows `tool !== null`. Later tasks open tools only through `setTool` / `ontool`.

This is spec §9's first risk: text input in a non-activating panel. It comes before the popovers so the manual check in Task 11 can try it first. `ACTIVATE_FOR_KEYS` is the named fallback switch: if typing never reaches the field, set it to `true` (Ferry activates while a field is open and gives focus back after).

- [ ] **Step 1: Write the failing test**

In `src/lib/cards.test.ts`, change the `./cards` import to

```ts
import { autoHides, canDrag, dragAllFiles, isBusy, openTool, railActions, upsert } from './cards';
```

and append at the end of the file:

```ts
describe('the open tool', () => {
  const tool = { id: 'a', kind: 'rename' as const };

  it('stays open while its card is finished and idle', () => {
    expect(openTool(tool, [card({ kind: 'done' })])).toEqual(tool);
    expect(openTool(null, [card({ kind: 'done' })])).toBeNull();
  });

  it('closes when its card is gone, busy, or has no file', () => {
    expect(openTool(tool, [])).toBeNull();
    expect(openTool(tool, [card({ kind: 'queued' })])).toBeNull();
    expect(openTool(tool, [card({ kind: 'done' }, { output: null })])).toBeNull();
    expect(openTool(tool, [card({ kind: 'done' }, { id: 'b' })])).toBeNull();
  });
});
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm test`
Expected: FAIL — `openTool is not a function` (or `does not provide an export named 'openTool'`).

- [ ] **Step 3: `openTool`**

Append to `src/lib/cards.ts`:

```ts

/** A card tool that needs the keyboard (or, for the slider, Esc). */
export type Tool = 'rename' | 'crop' | 'fit' | 'slider';

/** The one tool open in the stack, and the card it belongs to. */
export interface OpenTool {
  id: string;
  kind: Tool;
}

/** `tool`, or null once its card is gone or busy: the stack must not stay key for a card that left. */
export function openTool(tool: OpenTool | null, cards: JobUpdate[]): OpenTool | null {
  if (tool === null) return null;
  const owner = cards.find((c) => c.id === tool.id);
  return owner && canDrag(owner) ? tool : null;
}
```

Run: `pnpm test`
Expected: PASS — 52 tests.

- [ ] **Step 4: Key-on-demand, macOS**

In `src-tauri/src/platform/macos.rs`:

4a. Replace the imports block

```rust
use std::path::Path;

use tauri::{Emitter, Manager, WebviewWindow};
```

with

```rust
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};

use objc2_app_kit::{NSApplicationActivationOptions, NSRunningApplication, NSWorkspace};
use tauri::{Emitter, Manager, WebviewWindow};
```

(Do not `use objc2::rc::Retained`: `tauri_panel!` already imports it into this module and a second import fails with E0252.)

4b. In the `tauri_panel!` config, replace `can_become_key_window: false,` with `can_become_key_window: key_wanted(),`.

4c. Append at the end of the file:

```rust

/// Whether the stack may take the keyboard: only while a text field or a
/// slider drag is open (spec §2 "Text input in the stack").
static KEY_WANTED: AtomicBool = AtomicBool::new(false);

/// The app that had the keyboard before the stack took it.
// `tauri_panel!` already imports `Retained` into this module.
static FRONT: Mutex<Option<objc2::rc::Retained<NSRunningApplication>>> = Mutex::new(None);

/// Fallback switch (Task 11): `true` activates Ferry while a field is open,
/// for when a non-activating key panel does not get typing.
const ACTIVATE_FOR_KEYS: bool = false;

/// The panel's `canBecomeKeyWindow`, asked by AppKit on every call.
fn key_wanted() -> bool {
    KEY_WANTED.load(Ordering::Relaxed)
}

/// Make the stack the key window without activating Ferry (the panel is
/// non-activating), or give the keyboard back to the app that had it.
pub fn set_stack_key(window: &WebviewWindow, on: bool) -> tauri::Result<()> {
    KEY_WANTED.store(on, Ordering::Relaxed);
    let Ok(panel) = window.app_handle().get_webview_panel(window.label()) else {
        return Ok(());
    };
    if on {
        let me = NSRunningApplication::currentApplication();
        let front = NSWorkspace::sharedWorkspace()
            .frontmostApplication()
            .filter(|app| app.processIdentifier() != me.processIdentifier());
        *FRONT.lock().unwrap() = front;
        if ACTIVATE_FOR_KEYS {
            #[allow(deprecated)]
            me.activateWithOptions(NSApplicationActivationOptions::ActivateIgnoringOtherApps);
        }
        panel.make_key_window();
    } else {
        panel.resign_key_window();
        // That app stayed active while the panel was key (unless the fallback
        // is on); activating it again gives its own window the keyboard back.
        // Not when the user has since clicked another app (that click is what
        // closed the field): the keyboard stays where they put it.
        if let Some(front) = FRONT.lock().unwrap().take() {
            let me = NSRunningApplication::currentApplication().processIdentifier();
            let still_ours = NSWorkspace::sharedWorkspace()
                .frontmostApplication()
                .is_none_or(|now| {
                    let pid = now.processIdentifier();
                    pid == front.processIdentifier() || pid == me
                });
            if still_ours {
                front.activateWithOptions(NSApplicationActivationOptions::empty());
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 5: Key-on-demand, Windows**

In `src-tauri/src/platform/windows.rs`, replace everything from `use std::path::Path;` through the end of `fn apply_tool_window_style` with:

```rust
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicIsize, Ordering};

use tauri::WebviewWindow;
use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GWL_EXSTYLE, GetForegroundWindow, GetWindowLongPtrW, HWND_TOPMOST, SW_HIDE, SW_SHOWNOACTIVATE,
    SWP_FRAMECHANGED, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetForegroundWindow,
    SetWindowLongPtrW, SetWindowPos, ShowWindow, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW,
};

/// Whether the stack may take the keyboard: only while a text field or a
/// slider drag is open (spec §2 "Text input in the stack").
static KEY_WANTED: AtomicBool = AtomicBool::new(false);
/// The window that had the keyboard before the stack took it.
static PREVIOUS: AtomicIsize = AtomicIsize::new(0);

fn apply_tool_window_style(hwnd: HWND) {
    let no_activate = if KEY_WANTED.load(Ordering::Relaxed) {
        0
    } else {
        WS_EX_NOACTIVATE
    };
    // SAFETY: `hwnd` is a live top-level window owned by this process.
    unsafe {
        let style = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) & !(WS_EX_NOACTIVATE as isize);
        SetWindowLongPtrW(
            hwnd,
            GWL_EXSTYLE,
            style | (no_activate | WS_EX_TOOLWINDOW) as isize,
        );
    }
}
```

(`show_without_focus` re-applies the style on every show, so a stack that grows while a popover is open stays activatable.) Append at the end of the file:

```rust

/// Let the stack take the keyboard (dropping WS_EX_NOACTIVATE) and remember
/// the window that had it; give it back when `on` is false.
pub fn set_stack_key(window: &WebviewWindow, on: bool) -> tauri::Result<()> {
    KEY_WANTED.store(on, Ordering::Relaxed);
    // tao tracks focusability itself and would put WS_EX_NOACTIVATE back.
    window.set_focusable(on)?;
    let hwnd = window.hwnd()?.0;
    apply_tool_window_style(hwnd);
    // SAFETY: both handles are top-level windows; a stale one only makes the call fail.
    unsafe {
        if on {
            PREVIOUS.store(GetForegroundWindow() as isize, Ordering::Relaxed);
            SetForegroundWindow(hwnd);
        } else {
            let previous = PREVIOUS.swap(0, Ordering::Relaxed) as HWND;
            // Only while the stack still has the foreground: a click on another
            // window (which closed the field) already put the keyboard there.
            if !previous.is_null() && GetForegroundWindow() == hwnd {
                SetForegroundWindow(previous);
            }
        }
    }
    Ok(())
}
```

- [ ] **Step 6: Export it and add `stack_focus`**

In `src-tauri/src/platform/mod.rs`, add a new line per OS (do not edit the existing `pub use` lines; rustfmt sorts the new one above them):

```rust
#[cfg(target_os = "macos")]
pub use macos::set_stack_key;
```

and

```rust
#[cfg(windows)]
pub use self::windows::set_stack_key;
```

In `src-tauri/src/windows.rs`, insert right before `/// Show the Settings window, creating it the first time.`:

```rust
/// Let the stack take the keyboard while a text field (rename, crop, fit) or
/// a slider drag is open, and hand it back after (spec §2, §5.3).
#[tauri::command]
pub fn stack_focus(app: AppHandle, on: bool) -> Result<(), String> {
    let Some(stack) = app.get_webview_window(STACK) else {
        return Ok(());
    };
    platform::set_stack_key(&stack, on).map_err(|e| e.to_string())
}

```

In `src-tauri/src/lib.rs`, after `commands::card_rename,` at the end of `generate_handler!`, add `windows::stack_focus,`.

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean. (If it reports E0252 for `Retained`, a `use objc2::rc::Retained;` slipped in; remove it.)

- [ ] **Step 7: The stack keeps one open tool**

In `src/stack/Stack.svelte`:

7a. Change `import { draggable, upsert } from '../lib/cards';` to

```ts
  import { draggable, openTool, upsert, type OpenTool, type Tool } from '../lib/cards';
```

7b. Right after `let root = $state<HTMLDivElement>();` insert:

```ts
  /** The one card tool that has the keyboard (rename, crop, fit, a slider drag). */
  let tool = $state<OpenTool | null>(null);

  // A tool whose card left (closed, faded, cleared, now busy) closes with it,
  // so the stack never keeps the keyboard for a card that is gone.
  $effect(() => {
    if (tool !== null && openTool(tool, cards) === null) tool = null;
  });

  // The stack takes the keyboard only while a tool is open (spec §2).
  let keyed = false;
  $effect(() => {
    const on = tool !== null;
    if (on === keyed) return;
    keyed = on;
    invoke('stack_focus', { on }).catch((e) => console.warn('stack_focus', e));
  });

  function setTool(id: string, kind: Tool | null) {
    if (kind !== null) tool = { id, kind };
    else if (tool?.id === id) tool = null;
  }
```

7c. In the `<Card … />` element add the two props:

```svelte
      tool={tool?.id === card.id ? tool.kind : null}
      ontool={(kind) => setTool(card.id, kind)}
```

(`tool` goes after `autoHideSecs={config.auto_hide_secs}`, `ontool` after `ondrag={dragState}`.)

- [ ] **Step 8: The rename field**

In `src/stack/Card.svelte`:

8a. Change the cards import to `import { autoHides, canDrag, isBusy, railActions, type Tool } from '../lib/cards';`.

8b. Props: after `autoHideSecs,` add `tool = null,`; after `ondrag,` add `ontool,`. In the props type, after `autoHideSecs: number;` add

```ts
    /** This card's open tool, if any. */
    tool?: Tool | null;
```

and after `ondrag: (active: boolean) => void;` add

```ts
    /** Open a tool on this card, or close it with null. */
    ontool: (kind: Tool | null) => void;
```

8c. After `let tip = $state<…>(null);` add `let renameError = $state<string | null>(null);`. After `const rail = $derived(railActions(card));` add `const canRename = $derived(canDrag(card));`.

8d. In the auto-hide effect, change the guard line to

```ts
    if (!autoHides(card.state) || autoHideSecs === 0 || hovered || focused || held || tool !== null) return;
```

8e. Right before `function tipText(action: string): string {` insert:

```ts
  $effect(() => {
    if (tool !== 'rename') renameError = null;
  });

  function focusSelect(input: HTMLInputElement) {
    input.focus();
    input.select();
  }

  async function rename(stem: string) {
    if (stem === card.name) {
      ontool(null);
      return;
    }
    try {
      await invoke('card_rename', { id: card.id, stem });
      ontool(null);
    } catch (e) {
      renameError = String(e);
    }
  }

  function renameKey(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      void rename((event.currentTarget as HTMLInputElement).value);
    } else if (event.key === 'Escape') {
      event.preventDefault();
      ontool(null);
    }
  }

```

and inside `tipText`'s `switch`, as the first case:

```ts
      case 'rename':
        return t('card.rename');
```

8f. Replace the chips block

```svelte
      <div class="chips">
        <span class="name">{card.name}</span>
        {#if card.ext}<span class="ext">{card.ext}</span>{/if}
      </div>
```

with

```svelte
      <div class="chips">
        {#if tool === 'rename'}
          <input
            class="name name-field"
            type="text"
            value={card.name}
            aria-label={t('card.rename_named', { name: fileName })}
            aria-invalid={renameError !== null}
            use:focusSelect
            onkeydown={renameKey}
            onblur={() => ontool(null)}
          />
        {:else if canRename}
          <button
            type="button"
            class="name"
            data-action="rename"
            class:hot={hotAction === 'rename'}
            aria-label={t('card.rename_named', { name: fileName })}
            onclick={() => ontool('rename')}>{card.name}</button
          >
        {:else}
          <span class="name">{card.name}</span>
        {/if}
        {#if card.ext}<span class="ext">{card.ext}</span>{/if}
      </div>
      {#if renameError}
        <span class="rename-error" role="alert">{renameError}</span>
      {/if}
```

(The chip is a `<button>`, so `drag()` ignores a press on it and the card is not dragged out.)

8g. In `<style>`, right before the `.ext {` rule, add:

```css
  button.name {
    border: 0;
    color: inherit;
    font: inherit;
    font-size: 12px;
    cursor: text;
  }
  button.name:hover,
  button.name.hot {
    background: rgba(255, 255, 255, 0.28);
  }
  .name-field {
    width: 180px;
    border: 1px solid rgba(255, 255, 255, 0.6);
    color: #fff;
    font: inherit;
    font-size: 12px;
    outline: none;
  }
  .name-field[aria-invalid='true'] {
    border-color: #ff453a;
  }
  .rename-error {
    font-size: 11px;
    color: #ffb3ae;
  }
```

8h. In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "card.rename": "Rename",
  "card.rename_named": "Rename {name}",
```

- [ ] **Step 9: Run everything**

Run: `pnpm test`
Expected: PASS — 52 tests, 0 failed.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

Run: `cargo test -p ferry`
Expected: PASS, 0 failed.

- [ ] **Step 10: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
pnpm check
git add src-tauri/src/platform src-tauri/src/windows.rs src-tauri/src/lib.rs src/lib/cards.ts src/lib/cards.test.ts src/lib/i18n/en.json src/stack/Card.svelte src/stack/Stack.svelte
git commit -m "feat(app): the stack takes the keyboard on demand; rename from the name chip

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clean.

---

### Task 5: Rail A and the sliders

**Files:**
- Modify: `src/lib/cards.ts`, `src/lib/cards.test.ts` (`RailAction`, `railActions`, `needsOriginal`)
- Create: `src/stack/Slider.svelte`
- Modify: `src/stack/Card.svelte` (imports, `compression` prop, slider logic, `tipText`, the rail markup, `openMenu`, Esc)
- Modify: `src/stack/Stack.svelte` (pass `compression`)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: Task 3's `SCALE_STOPS`, `QUALITY_STOPS`, `nextScale`, `currentQuality`, `nextQuality`, `scalePatch`, `qualityPatch`, `scaleLabel`, `qualityLabel`, `sliderIndex`, `stopIndex`, `EditsPatch`; Task 2's `card_edit({ id, edits })`, `StackConfig.compression`; Task 4's `Tool`, `ontool`, `tool`; M3c's `open_compare({ id })` (by name; rejects until M3c is merged).
- Produces:
  - `type RailAction = 'downscale' | 'compression' | 'crop' | 'restore' | 'compare' | 'copy' | 'more'`; `railActions(card: JobUpdate, compact?: boolean): RailAction[]`; `needsOriginal(action: RailAction): boolean`.
  - `Card.svelte` prop `compression: number`; the Crop button calls `ontool('crop')` (Task 7 draws the popover); the ⋯ button calls `card_menu({ id, x, y })` with the button's bottom-left in page pixels (Task 8 builds the command; until then the call is logged and ignored).
  - `Slider.svelte` props `{ index: number; count: number; label: string; left: number; track: number }`.

Spec §5.1: Downscale · Compression · Crop · Restore · Compare · Copy · ⋯; compact cards hide Compression and Compare; Show in folder and Optimise aggressively move to the ⋯ menu (Task 8). Spec §5.2: press-and-drag drops a slider with a live badge; release applies; Esc or releasing at the start value cancels; a plain click is one step. The slider makes the stack key (`ontool('slider')`) only while dragging, so Esc reaches the page. `PRESS_AND_DRAG` is the named fallback switch for spec §9's "Press-and-drag in the panel" risk.

- [ ] **Step 1: Write the failing tests**

In `src/lib/cards.test.ts`, replace the `./cards` import with

```ts
import {
  autoHides,
  canDrag,
  dragAllFiles,
  isBusy,
  needsOriginal,
  openTool,
  railActions,
  upsert,
  type RailAction,
} from './cards';
```

and replace the whole `describe('railActions', () => { … });` block with:

```ts
describe('railActions', () => {
  it('lays out rail A, with restore only on a restorable done card', () => {
    expect(railActions(card({ kind: 'done' }, { can_restore: true }))).toEqual([
      'downscale',
      'compression',
      'crop',
      'restore',
      'compare',
      'copy',
      'more',
    ]);
    expect(railActions(card({ kind: 'larger' }))).toEqual([
      'downscale',
      'compression',
      'crop',
      'compare',
      'copy',
      'more',
    ]);
  });

  it('leaves Compression and Compare off a compact card', () => {
    expect(railActions(card({ kind: 'done' }, { can_restore: true }), true)).toEqual([
      'downscale',
      'crop',
      'restore',
      'copy',
      'more',
    ]);
  });

  it('keeps the tools on a restored card, which offers re-optimise instead of restore', () => {
    expect(railActions(card({ kind: 'restored' }, { can_restore: true }))).not.toContain('restore');
    expect(railActions(card({ kind: 'restored' }))).toContain('downscale');
  });

  it('has no rail while busy, on errors, or without an output file', () => {
    expect(railActions(card({ kind: 'queued' }))).toEqual([]);
    expect(railActions(card({ kind: 'error', reason: 'x' }))).toEqual([]);
    expect(railActions(card({ kind: 'done' }, { output: null }))).toEqual([]);
  });

  it('turns off only the tools that need the original', () => {
    expect(['downscale', 'compression', 'crop', 'compare'].every((a) => needsOriginal(a as RailAction))).toBe(true);
    expect(['restore', 'copy', 'more'].some((a) => needsOriginal(a as RailAction))).toBe(false);
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm test`
Expected: FAIL — `needsOriginal` is not exported, and the rail A expectations differ (`['copy', 'reveal', …]`).

- [ ] **Step 3: Rail A in `cards.ts`**

In `src/lib/cards.ts`, replace `export type RailAction = 'copy' | 'reveal' | 'restore' | 'aggressive';` with

```ts
/** Rail A (spec §5.1). Show in folder, Aggressive, Rename, Preview and Fit live in the ⋯ menu. */
export type RailAction =
  | 'downscale'
  | 'compression'
  | 'crop'
  | 'restore'
  | 'compare'
  | 'copy'
  | 'more';
```

and replace the whole `railActions` function (with its doc comment) with

```ts
/** The rail's buttons for a finished card, in order; compact cards leave out Compression and Compare. */
export function railActions(card: JobUpdate, compact = false): RailAction[] {
  if (!autoHides(card.state) || card.output === null) return [];
  const actions: RailAction[] = ['downscale'];
  if (!compact) actions.push('compression');
  actions.push('crop');
  if (card.state.kind === 'done' && card.can_restore) actions.push('restore');
  if (!compact) actions.push('compare');
  actions.push('copy', 'more');
  return actions;
}

/** Rail buttons that work on the original, so `can_edit` turns them off. */
export function needsOriginal(action: RailAction): boolean {
  return ['downscale', 'compression', 'crop', 'compare'].includes(action);
}
```

Run: `pnpm test`
Expected: PASS — 54 tests.

- [ ] **Step 4: The slider**

Create `src/stack/Slider.svelte`:

```svelte
<script lang="ts">
  /** The vertical slider a press-and-drag on Downscale or Compression drops over the thumbnail (spec §5.2). */
  let {
    index,
    count,
    label,
    left,
    track,
  }: {
    /** The stop under the pointer, 0 at the top. */
    index: number;
    count: number;
    /** The live target, such as "50 % · 1440×900" or "q 60". */
    label: string;
    /** The centre of the button it drops from, in card pixels. */
    left: number;
    /** The track's length in px. */
    track: number;
  } = $props();

  const knob = $derived((index / Math.max(1, count - 1)) * track);
</script>

<div class="slider" style="left: {left}px; height: {track}px" aria-hidden="true">
  <div class="fill" style="height: {knob}px"></div>
  <div class="knob" style="top: {knob}px"></div>
  <div class="badge" style="top: {knob}px">{label}</div>
</div>

<style>
  .slider {
    position: absolute;
    top: 44px;
    width: 6px;
    margin-left: -3px;
    border-radius: 3px;
    background: rgba(255, 255, 255, 0.25);
    z-index: 4;
    pointer-events: none;
  }
  .fill {
    width: 100%;
    border-radius: 3px;
    background: var(--accent);
  }
  .knob {
    position: absolute;
    left: -5px;
    width: 16px;
    height: 16px;
    margin-top: -8px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 4px rgba(0, 0, 0, 0.4);
  }
  .badge {
    position: absolute;
    left: 18px;
    margin-top: -11px;
    padding: 3px 8px;
    border-radius: 7px;
    background: rgba(30, 30, 32, 0.9);
    color: #fff;
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
  }
</style>
```

- [ ] **Step 5: The rail and the slider on the card**

In `src/stack/Card.svelte`:

5a. Replace the imports of `../lib/cards` and `../lib/ipc` so the import block reads:

```ts
  import { convertFileSrc, invoke } from '@tauri-apps/api/core';
  import { startDrag } from '@crabnebula/tauri-plugin-drag';
  import { fade } from 'svelte/transition';
  import { autoHides, canDrag, isBusy, needsOriginal, railActions, type RailAction, type Tool } from '../lib/cards';
  import {
    QUALITY_STOPS,
    SCALE_STOPS,
    currentQuality,
    nextQuality,
    nextScale,
    qualityLabel,
    qualityPatch,
    scaleLabel,
    scalePatch,
    sliderIndex,
    stopIndex,
  } from '../lib/edits';
  import { formatBytes, savingPercent } from '../lib/format';
  import { t, type StringKey } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';
  import Slider from './Slider.svelte';
```

(`isMac` is no longer used here: Show in folder moves to the menu.)

5b. Props: after `autoHideSecs,` add `compression,`; in the props type after `autoHideSecs: number;` add

```ts
    /** The settings' compression quality: where the Compression slider starts on a card without its own. */
    compression: number;
```

5c. Change `const rail = $derived(railActions(card));` to `const rail = $derived(railActions(card, compact));`.

5d. Right before `function tipText(action: string): string {` insert:

```ts
  /** The slider's track length in px; the thumbnail below the rail. */
  const TRACK = 136;
  /** Fallback switch (Task 11): false if drags never reach the panel; the buttons then only step. */
  const PRESS_AND_DRAG = true;
  type SliderKind = 'downscale' | 'compression';
  let slide = $state<{
    kind: SliderKind;
    startY: number;
    start: number;
    index: number;
    moved: boolean;
    left: number;
  } | null>(null);
  const quality = $derived(currentQuality(card.edits, compression));
  const slideLabel = $derived.by(() => {
    if (!slide) return '';
    return slide.kind === 'downscale'
      ? scaleLabel(SCALE_STOPS[slide.index], card.dims_before, card.edits)
      : qualityLabel(QUALITY_STOPS[slide.index]);
  });

  function edit(edits: EditsPatch) {
    void act('card_edit', { edits });
  }

  function step(kind: SliderKind) {
    edit(kind === 'downscale' ? scalePatch(nextScale(card.edits.scale)) : qualityPatch(nextQuality(quality)));
  }

  // Press and hold: a drag of a few px turns the press into a slider; a plain click is one step.
  function press(kind: SliderKind, event: PointerEvent) {
    if (!PRESS_AND_DRAG || event.button !== 0 || !card.can_edit) return;
    const button = event.currentTarget as HTMLElement;
    button.setPointerCapture(event.pointerId);
    const box = cardEl?.getBoundingClientRect();
    const rect = button.getBoundingClientRect();
    const start =
      kind === 'downscale' ? stopIndex(SCALE_STOPS, card.edits.scale ?? 100) : stopIndex(QUALITY_STOPS, quality);
    slide = {
      kind,
      startY: event.clientY,
      start,
      index: start,
      moved: false,
      left: rect.left + rect.width / 2 - (box?.left ?? 0),
    };
  }

  function slideMove(event: PointerEvent) {
    if (!slide) return;
    const dy = event.clientY - slide.startY;
    if (!slide.moved && Math.abs(dy) < 4) return;
    if (!slide.moved) ontool('slider');
    const count = slide.kind === 'downscale' ? SCALE_STOPS.length : QUALITY_STOPS.length;
    slide = { ...slide, moved: true, index: sliderIndex(slide.start, dy, count, TRACK) };
  }

  // Release applies; releasing back at the start value changes nothing.
  function release() {
    const done = slide;
    slide = null;
    if (!done) return;
    if (!done.moved) {
      step(done.kind);
      return;
    }
    ontool(null);
    if (done.index === done.start) return;
    edit(
      done.kind === 'downscale'
        ? scalePatch(SCALE_STOPS[done.index])
        : qualityPatch(QUALITY_STOPS[done.index]),
    );
  }

  function cancelSlide() {
    if (slide?.moved) ontool(null);
    slide = null;
  }

  // Enter or Space on a focused button: one step (a pointer press is handled by `release`).
  function keyStep(kind: SliderKind, event: MouseEvent) {
    if ((event.detail === 0 || !PRESS_AND_DRAG) && card.can_edit) step(kind);
  }

```

5e. Replace the start of `tipText` and its `reveal` / `restore` / `aggressive` / `more` cases so the function begins:

```ts
  function tipText(action: string): string {
    if (needsOriginal(action as RailAction) && !card.can_edit) return card.edit_hint ?? t('card.no_original');
    switch (action) {
      case 'downscale':
        return t('card.downscale');
      case 'compression':
        return t('card.compression');
      case 'crop':
        return t('card.crop');
      case 'compare':
        return t('card.compare');
      case 'rename':
        return t('card.rename');
      case 'copy':
        return copied === 'ok' ? t('card.copied') : copied === 'failed' ? t('card.copy_failed') : t('card.copy');
      case 'restore':
        return t('card.restore');
      case 'more':
        return t('card.menu');
```

(the `close`, `retry`, `details`, `reoptimise` cases and `default` stay as they are; the `reveal` and `aggressive` cases are gone).

5f. Right before `function close() {` insert:

```ts
  // The ⋯ button opens the native menu just under itself (Task 8 builds it).
  function openMenu(event: MouseEvent) {
    const rect = (event.currentTarget as HTMLElement).getBoundingClientRect();
    void act('card_menu', { x: rect.left, y: rect.bottom + 4 });
  }

```

5g. Replace the whole rail block, from `  {#if rail.length > 0}` through the `{/if}` that closes it (right before `<button class="close"`), with:

```svelte
  {#if rail.length > 0}
    <div class="rail">
      {#if rail.includes('downscale')}
        <button
          type="button"
          data-action="downscale"
          class:hot={hotAction === 'downscale'}
          aria-label={tipText('downscale')}
          disabled={!card.can_edit}
          onpointerdown={(event) => press('downscale', event)}
          onpointermove={slideMove}
          onpointerup={release}
          onpointercancel={cancelSlide}
          onclick={(event) => keyStep('downscale', event)}
        >
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M4 14h6v6" /><path d="M20 10h-6V4" /><path d="m14 10 7-7" /><path d="m3 21 7-7" /></svg>
        </button>
      {/if}
      {#if rail.includes('compression')}
        <button
          type="button"
          data-action="compression"
          class:hot={hotAction === 'compression'}
          aria-label={tipText('compression')}
          disabled={!card.can_edit}
          onpointerdown={(event) => press('compression', event)}
          onpointermove={slideMove}
          onpointerup={release}
          onpointercancel={cancelSlide}
          onclick={(event) => keyStep('compression', event)}
        >
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2"><circle cx="12" cy="12" r="9" /><path d="M12 3a9 9 0 0 1 0 18z" fill="currentColor" /></svg>
        </button>
      {/if}
      {#if rail.includes('crop')}
        <button type="button" data-action="crop" class:hot={hotAction === 'crop'} aria-label={tipText('crop')} disabled={!card.can_edit} onclick={() => ontool(tool === 'crop' ? null : 'crop')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M6 2v14a2 2 0 0 0 2 2h14" /><path d="M18 22V8a2 2 0 0 0-2-2H2" /></svg>
        </button>
      {/if}
      {#if rail.includes('restore')}
        <button type="button" data-action="restore" class:hot={hotAction === 'restore'} aria-label={tipText('restore')} onclick={() => act('card_restore')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><path d="M3 12a9 9 0 1 0 3-6.7L3 8" /><path d="M3 3v5h5" /></svg>
        </button>
      {/if}
      {#if rail.includes('compare')}
        <button type="button" data-action="compare" class:hot={hotAction === 'compare'} aria-label={tipText('compare')} disabled={!card.can_edit} onclick={() => act('open_compare')}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="3" y="3" width="18" height="18" rx="2" /><path d="M12 3v18" /></svg>
        </button>
      {/if}
      {#if rail.includes('copy')}
        <button
          type="button"
          data-action="copy"
          class:hot={hotAction === 'copy'}
          class:ok={copied === 'ok'}
          class:failed={copied === 'failed'}
          aria-label={tipText('copy')}
          onclick={copy}
        >
          {#if copied === 'ok'}
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2.5" stroke-linecap="round" stroke-linejoin="round"><path d="M20 6 9 17l-5-5" /></svg>
          {:else}
            <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><rect x="9" y="9" width="13" height="13" rx="2" /><path d="M5 15H4a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2h9a2 2 0 0 1 2 2v1" /></svg>
          {/if}
        </button>
      {/if}
      {#if rail.includes('more')}
        <button type="button" data-action="more" class:hot={hotAction === 'more'} aria-label={tipText('more')} onclick={(event) => openMenu(event)}>
          <svg aria-hidden="true" width="16" height="16" viewBox="0 0 24 24" fill="currentColor"><circle cx="5" cy="12" r="1.8" /><circle cx="12" cy="12" r="1.8" /><circle cx="19" cy="12" r="1.8" /></svg>
        </button>
      {/if}
    </div>
  {/if}
  {#if slide?.moved}
    <Slider
      index={slide.index}
      count={slide.kind === 'downscale' ? SCALE_STOPS.length : QUALITY_STOPS.length}
      label={slideLabel}
      left={slide.left}
      track={TRACK}
    />
  {/if}

```

5h. Right before `<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->` (above `<article`), add:

```svelte
<svelte:window
  onkeydown={(event) => {
    if (event.key === 'Escape' && slide) cancelSlide();
  }}
/>

```

5i. In `src/stack/Stack.svelte`, in the `<Card … />` element, after `autoHideSecs={config.auto_hide_secs}` add `compression={config.compression}`.

5j. In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "card.downscale": "Downscale — click for −10 %, drag for a slider",
  "card.compression": "Compression — click for −10, drag for a slider",
  "card.crop": "Crop…",
  "card.compare": "Compare with original",
  "card.menu": "More",
```

- [ ] **Step 6: Run everything**

Run: `pnpm test`
Expected: PASS — 54 tests.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

Run: `pnpm build`
Expected: ends with `✓ built` (the existing `INEFFECTIVE_DYNAMIC_IMPORT` notice is not new).

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
git add src/lib/cards.ts src/lib/cards.test.ts src/lib/i18n/en.json src/stack/Slider.svelte src/stack/Card.svelte src/stack/Stack.svelte
git commit -m "feat(ui): rail A with press-and-drag Downscale and Compression sliders

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clean.

---

### Task 6: Format bar and convert

**Files:**
- Modify: `src/lib/edits.ts`, `src/lib/edits.test.ts` (`FormatChoice`, `formatChoices`, `convertRequest`)
- Create: `src/stack/FormatBar.svelte`
- Modify: `src/stack/Card.svelte` (format bar in place of the detail line on hover, "converted" label, tooltips above the bottom row)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: Task 1's `JobUpdate.source_format`, `edit_hint`; Task 2's `card_convert({ id, format, keepBoth })`; Task 3's `Format`, `JobUpdate`, `card.fixture.ts`; Task 5's `tipText` / `hotAction` tooltip mechanism (`data-action` attributes).
- Produces: `interface FormatChoice { format: Format; label: string; current: boolean; firstFrame: boolean }`; `formatChoices(card: JobUpdate): FormatChoice[]`; `convertRequest(card: JobUpdate, format: Format, keepBoth: boolean): { format: Format; keepBoth: boolean } | null`; `FormatBar.svelte` props `{ card: JobUpdate; hotAction?: string | null; onconvert: (format: Format, keepBoth: boolean) => void }`; chip `data-action` values `format-png`, `format-jpeg`, `format-webp`, `format-gif`.

Spec §5.4: on hover the detail line becomes PNG · JPEG · WebP with the current format selected; GIF cards add a selected GIF chip and the others say "First frame only"; click converts (replacing the last conversion), ⌥/Alt-click keeps both; the extension chip shows the new format with a small "converted" label. Clicking the source format's chip clears the conversion (M3a's `Job::edit` turns `format == source` into `None`).

- [ ] **Step 1: Write the failing tests**

In `src/lib/edits.test.ts`, add `convertRequest,` and `formatChoices,` to the `./edits` import list (right after `QUALITY_STOPS,`), and append:

```ts
describe('format bar', () => {
  const pick = (choices: ReturnType<typeof formatChoices>) =>
    choices.map((c) => [c.label, c.current, c.firstFrame]);

  it('offers PNG, JPEG and WebP with the current one selected', () => {
    expect(pick(formatChoices(card({ kind: 'done' })))).toEqual([
      ['PNG', true, false],
      ['JPEG', false, false],
      ['WebP', false, false],
    ]);
  });

  it('puts a GIF first and marks the others first frame only', () => {
    const gif = card({ kind: 'done' }, { format: 'gif', source_format: 'gif', ext: 'gif' });
    expect(pick(formatChoices(gif))).toEqual([
      ['GIF', true, false],
      ['PNG', false, true],
      ['JPEG', false, true],
      ['WebP', false, true],
    ]);
  });

  it('keeps the GIF chip after a GIF was converted, so it can go back', () => {
    const converted = card({ kind: 'done' }, { format: 'webp', source_format: 'gif', converted: true });
    expect(formatChoices(converted).map((c) => c.format)).toEqual(['gif', 'png', 'jpeg', 'webp']);
    expect(formatChoices(converted).find((c) => c.current)?.format).toBe('webp');
  });

  it('ignores a plain click on the current format but keeps both on ⌥-click', () => {
    const png = card({ kind: 'done' });
    expect(convertRequest(png, 'png', false)).toBeNull();
    expect(convertRequest(png, 'webp', false)).toEqual({ format: 'webp', keepBoth: false });
    expect(convertRequest(png, 'png', true)).toEqual({ format: 'png', keepBoth: true });
  });
});
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm test`
Expected: FAIL — `formatChoices` / `convertRequest` are not exported from `./edits`.

- [ ] **Step 3: The helpers**

In `src/lib/edits.ts`, change the `./ipc` import to `import type { Edits, EditsPatch, Format, JobUpdate } from './ipc';` and append:

```ts

/** A chip in the format bar (spec §5.4). */
export interface FormatChoice {
  format: Format;
  /** The format's own name, as `ferry_core::sniff::Format::label` writes it. */
  label: string;
  current: boolean;
  /** Converting a GIF keeps only its first frame. */
  firstFrame: boolean;
}

const FORMAT_LABELS: Record<Format, string> = { png: 'PNG', jpeg: 'JPEG', gif: 'GIF', webp: 'WebP' };

/** PNG · JPEG · WebP, with GIF first on a card that started as a GIF. */
export function formatChoices(card: JobUpdate): FormatChoice[] {
  const source = card.source_format ?? card.format;
  const formats: Format[] = source === 'gif' ? ['gif', 'png', 'jpeg', 'webp'] : ['png', 'jpeg', 'webp'];
  return formats.map((format) => ({
    format,
    label: FORMAT_LABELS[format],
    current: card.format === format,
    firstFrame: source === 'gif' && format !== 'gif',
  }));
}

/** What a chip click asks `card_convert` for; null for a plain click on the current format. */
export function convertRequest(
  card: JobUpdate,
  format: Format,
  keepBoth: boolean,
): { format: Format; keepBoth: boolean } | null {
  if (card.format === format && !keepBoth) return null;
  return { format, keepBoth };
}
```

Run: `pnpm test`
Expected: PASS — 58 tests.

- [ ] **Step 4: The bar**

Create `src/stack/FormatBar.svelte`:

```svelte
<script lang="ts">
  import { convertRequest, formatChoices } from '../lib/edits';
  import type { Format, JobUpdate } from '../lib/ipc';

  /** PNG · JPEG · WebP chips in place of the detail line while the card is hovered (spec §5.4). */
  let {
    card,
    hotAction = null,
    onconvert,
  }: {
    card: JobUpdate;
    hotAction?: string | null;
    onconvert: (format: Format, keepBoth: boolean) => void;
  } = $props();

  const choices = $derived(formatChoices(card));

  // ⌥-click (Alt-click on Windows) keeps both: a new card with the other format.
  function click(format: Format, event: MouseEvent) {
    const request = convertRequest(card, format, event.altKey);
    if (request) onconvert(request.format, request.keepBoth);
  }
</script>

<div class="bar">
  {#each choices as choice (choice.format)}
    <button
      type="button"
      data-action={`format-${choice.format}`}
      class:current={choice.current}
      class:hot={hotAction === `format-${choice.format}`}
      aria-pressed={choice.current}
      disabled={!card.can_edit}
      onclick={(event) => click(choice.format, event)}>{choice.label}</button
    >
  {/each}
</div>

<style>
  .bar {
    display: flex;
    gap: 4px;
  }
  button {
    border: 0;
    border-radius: 9px;
    padding: 1px 8px;
    font: inherit;
    font-size: 11px;
    font-weight: 600;
    color: #fff;
    background: rgba(255, 255, 255, 0.14);
    cursor: pointer;
  }
  button:hover:not(:disabled),
  button.hot:not(:disabled) {
    background: rgba(255, 255, 255, 0.3);
  }
  button.current {
    background: rgba(255, 255, 255, 0.9);
    color: #111;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
```

- [ ] **Step 5: Put it on the card**

In `src/stack/Card.svelte`:

5a. Add `formatChoices,` to the `../lib/edits` import list (after `SCALE_STOPS,`). Replace `import type { EditsPatch, JobUpdate } from '../lib/ipc';` with

```ts
  import { isMac, type EditsPatch, type Format, type JobUpdate } from '../lib/ipc';
  import FormatBar from './FormatBar.svelte';
```

5b. After `const canRename = $derived(canDrag(card));` add:

```ts
  // The format bar replaces the detail line while the card is hovered (spec §5.4).
  const showFormats = $derived(hovered && rail.length > 0 && card.state.kind !== 'restored');
```

5c. Replace

```ts
  function tipText(action: string): string {
    if (needsOriginal(action as RailAction) && !card.can_edit) return card.edit_hint ?? t('card.no_original');
```

with

```ts
  function convert(format: Format, keepBoth: boolean) {
    void act('card_convert', { format, keepBoth });
  }

  function formatTip(format: string): string {
    if (!card.can_edit) return card.edit_hint ?? t('card.no_original');
    const choice = formatChoices(card).find((c) => c.format === format);
    if (!choice) return '';
    if (choice.firstFrame) return t('card.first_frame');
    return t(isMac ? 'format.tip.mac' : 'format.tip.windows', { format: choice.label });
  }

  function tipText(action: string): string {
    if (action.startsWith('format-')) return formatTip(action.slice('format-'.length));
    if (needsOriginal(action as RailAction) && !card.can_edit) return card.edit_hint ?? t('card.no_original');
```

5d. In the tooltip effect, replace

```ts
    tip = {
      text: tipText(action),
      left: Math.max(8, rect.left - box.left),
      top: rect.bottom - box.top + 6,
    };
```

with

```ts
    // Under the button, or above it near the bottom edge (the format bar) so the card does not clip it.
    const below = rect.bottom - box.top + 6;
    tip = {
      text: tipText(action),
      left: Math.max(8, rect.left - box.left),
      top: below + 24 > box.height ? rect.top - box.top - 28 : below,
    };
```

5e. After `{#if card.ext}<span class="ext">{card.ext}</span>{/if}` add

```svelte
        {#if card.converted}<span class="converted">{t('card.converted')}</span>{/if}
```

5f. Replace

```svelte
    {:else if card.state.kind === 'done' && card.detail && !compact}
      <span class="detail">{card.detail}</span>
    {/if}
```

with

```svelte
    {:else if showFormats}
      <FormatBar {card} {hotAction} onconvert={convert} />
    {:else if card.state.kind === 'done' && card.detail && !compact}
      <span class="detail">{card.detail}</span>
    {/if}
```

5g. In `<style>`, before `.rename-error {` add:

```css
  .converted {
    font-size: 11px;
    color: rgba(255, 255, 255, 0.82);
  }
```

5h. In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "card.converted": "converted",
  "card.first_frame": "First frame only",
  "format.tip.mac": "Convert to {format} · ⌥-click keeps both",
  "format.tip.windows": "Convert to {format} · Alt-click keeps both",
```

- [ ] **Step 6: Run everything**

Run: `pnpm test`
Expected: PASS — 58 tests.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt --all --check
cargo clippy --workspace --all-targets -- -D warnings
git add src/lib/edits.ts src/lib/edits.test.ts src/lib/i18n/en.json src/stack/FormatBar.svelte src/stack/Card.svelte
git commit -m "feat(ui): format bar converts on click and keeps both on ⌥-click

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clean.

---

### Task 7: Crop and Fit popovers

**Files:**
- Modify: `src/lib/edits.ts`, `src/lib/edits.test.ts` (crop form, checks, frame; fit sizes)
- Create: `src/stack/CropPopover.svelte`, `src/stack/FitPopover.svelte`
- Modify: `src/stack/Stack.svelte` (a row per card with its popover; apply; click-away; window blur)
- Modify: `src/stack/Card.svelte` (crop frame over the thumbnail; the amber `note` line)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (`crop_sizes_reset`)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: Task 2's `card_edit({ id, edits })`; Task 3's `Edits`, `EditsPatch`, `Size`, `StackConfig.crop_sizes: [number, number][]`; Task 4's `Tool`, `setTool(id, kind)`, the `tool` state and `stack_focus` effect in `Stack.svelte` (a tool open = the stack is key); Task 5's crop rail button (`data-action="crop"`, `ontool('crop')`). `formatBytes` from `src/lib/format.ts` (existing).
- Produces: `MAX_SIDE`, `ASPECTS`, `interface CropForm`, `cropForm(edits)`, `type CropField`, `type CropCheck`, `checkCrop(form, dims)`, `scaleForm(form, factor)`, `fitRatio(dims, ratio)`, `interface CropFrame`, `cropFrame(dims, form)`, `frameBox(frame, dims, box)`, `FIT_PRESETS_MB`, `fitBytes(text, unit)`, `fitForm(fitUnder)` in `src/lib/edits.ts`; `CropPopover.svelte` props `{ card; sizes; onapply(edits); onclose(); onframe(frame | null) }`; `FitPopover.svelte` props `{ card; onapply(edits); onclose() }`; both roots carry `data-popover` and `data-card-id`. Rust: `#[tauri::command(async)] pub fn crop_sizes_reset(app: AppHandle) -> Result<Settings, String>`. Task 8 opens the Fit popover by calling `setTool(id, 'fit')` (there is no rail button for Fit).

Spec §5.3: the crop popover opens beside the card toward the screen centre; Free · 16:9 · 4:3 · 1:1 · 1.91:1 · 9:16; W × H with "0 keeps ratio", or Long edge; Smart / Centre; ×0.5 / ×2; saved sizes from settings with "Restore defaults"; Enter applies, Esc or click-away closes; a live frame on the thumbnail. Validation: whole numbers 1–30000 (empty is 0), a Free crop needs a size, and "Larger than the original" is a hint, not an error (M3a's engine never upscales a crop; it clamps). Fit under size: a field with KB/MB, presets 1 · 5 · 10 · 25 MB, decimal units like the card's sizes. The popover is not a new window: it renders in the stack page, and the stack window (pinned to its corner by `stack_resize`) grows toward the centre to fit it.

- [ ] **Step 1: Write the failing tests**

In `src/lib/edits.test.ts`, extend the `./edits` import so it reads:

```ts
import {
  QUALITY_STOPS,
  checkCrop,
  convertRequest,
  cropForm,
  cropFrame,
  fitBytes,
  fitForm,
  fitRatio,
  formatChoices,
  frameBox,
  scaleForm,
  type CropForm,
```

(the remaining names in that import stay as they are), and append:

```ts
describe('crop popover', () => {
  const form = (extra: Partial<CropForm> = {}): CropForm => ({ ...cropForm(edits), ...extra });
  const dims: [number, number] = [2880, 1800];

  it('starts from the card’s current crop', () => {
    expect(cropForm(edits)).toEqual({
      aspect: null,
      mode: 'size',
      width: '0',
      height: '0',
      edge: '0',
      framing: 'centre',
    });
    const cropped = { ...edits, aspect: [16, 9] as [number, number], size: { kind: 'long_edge' as const, edge: 1200 }, framing: 'smart' as const };
    expect(cropForm(cropped)).toMatchObject({ aspect: [16, 9], mode: 'long_edge', edge: '1200', framing: 'smart' });
  });

  it('turns the fields into a size: 0 keeps the ratio', () => {
    expect(checkCrop(form({ width: '1200', height: '628' }), dims).patch).toEqual({
      aspect: null,
      framing: 'centre',
      size: { kind: 'exact', width: 1200, height: 628 },
    });
    expect(checkCrop(form({ width: '1440' }), dims).patch?.size).toEqual({ kind: 'width', width: 1440 });
    expect(checkCrop(form({ height: '900' }), dims).patch?.size).toEqual({ kind: 'height', height: 900 });
    expect(checkCrop(form({ mode: 'long_edge', edge: '1000' }), dims).patch?.size).toEqual({
      kind: 'long_edge',
      edge: 1000,
    });
  });

  it('crops to a ratio alone, with no size', () => {
    expect(checkCrop(form({ aspect: [16, 9] }), dims).patch).toEqual({
      aspect: [16, 9],
      framing: 'centre',
      size: null,
    });
  });

  it('refuses words, huge sides and an empty Free crop', () => {
    expect(checkCrop(form({ width: '12a' }), dims)).toMatchObject({ error: 'Whole numbers only', field: 'width' });
    expect(checkCrop(form({ height: '-5' }), dims)).toMatchObject({ error: 'Whole numbers only', field: 'height' });
    expect(checkCrop(form({ width: '30001' }), dims)).toMatchObject({ error: 'At most 30000 px', field: 'width' });
    expect(checkCrop(form(), dims)).toMatchObject({ error: 'Choose a ratio or a size', field: null });
  });

  it('hints when the size is larger than the original', () => {
    expect(checkCrop(form({ width: '4000' }), dims).larger).toBe(true);
    expect(checkCrop(form({ width: '1440' }), dims).larger).toBe(false);
    expect(checkCrop(form({ width: '4000' }), null).larger).toBe(false);
  });

  it('halves and doubles the sizes, within 1 to 30000', () => {
    expect(scaleForm(form({ width: '1440', height: '0' }), 0.5)).toMatchObject({ width: '720', height: '0' });
    expect(scaleForm(form({ width: '20000', height: '1' }), 2)).toMatchObject({ width: '30000', height: '2' });
    expect(scaleForm(form({ width: '1', edge: 'x' }), 0.5)).toMatchObject({ width: '1', edge: 'x' });
  });

  it('fits a ratio like ferry_core', () => {
    expect(fitRatio([2880, 1800], [16, 9])).toEqual([2880, 1620]);
    expect(fitRatio([2880, 1800], [1, 1])).toEqual([1800, 1800]);
    expect(fitRatio([1800, 2880], [16, 9])).toEqual([1800, 1013]);
    expect(fitRatio([3, 1], [1, 100])).toEqual([1, 1]);
  });

  it('frames the crop on the thumbnail', () => {
    expect(cropFrame(dims, form())).toBeNull();
    expect(cropFrame(null, form({ aspect: [1, 1] }))).toBeNull();
    const square = cropFrame(dims, form({ aspect: [1, 1] }));
    expect(square).toEqual({ x: 540 / 2880, y: 0, w: 1800 / 2880, h: 1 });
    // A 2880×1800 picture in a 300×200 card is 300×187.5, 6.25 px from the top.
    expect(frameBox(square!, dims, [300, 200])).toEqual({ left: 56.25, top: 6.25, width: 187.5, height: 187.5 });
  });
});

describe('fit popover', () => {
  it('reads KB and MB in decimal units', () => {
    expect(fitBytes('1', 'mb')).toBe(1_000_000);
    expect(fitBytes('1.5', 'mb')).toBe(1_500_000);
    expect(fitBytes('250', 'kb')).toBe(250_000);
  });

  it('refuses zero, negatives and words', () => {
    expect(fitBytes('0', 'mb')).toBeNull();
    expect(fitBytes('-1', 'mb')).toBeNull();
    expect(fitBytes('', 'kb')).toBeNull();
    expect(fitBytes('big', 'mb')).toBeNull();
  });

  it('starts from the card’s target', () => {
    expect(fitForm(null)).toEqual({ text: '', unit: 'mb' });
    expect(fitForm(5_000_000)).toEqual({ text: '5', unit: 'mb' });
    expect(fitForm(250_000)).toEqual({ text: '250', unit: 'kb' });
  });
});
```

`edits` is the file's existing `const edits = card({ kind: 'done' }).edits;` from Task 3 (no crop, `framing: 'centre'`).

- [ ] **Step 2: Run the tests to verify they fail**

Run: `pnpm test`
Expected: FAIL — `checkCrop`, `cropForm` and the others are not exported from `./edits`.

- [ ] **Step 3: The helpers**

In `src/lib/edits.ts`, change the `./ipc` import to `import type { Edits, EditsPatch, Format, JobUpdate, Size } from './ipc';` and append:

```ts

/** The largest width, height or long edge a crop may ask for (`ferry_core::edit::MAX_SIDE`). */
export const MAX_SIDE = 30_000;

/** The crop popover's ratio chips; `null` is "Free" (no ratio crop, only the size fields). */
export const ASPECTS: { label: string; ratio: [number, number] | null }[] = [
  { label: '', ratio: null },
  { label: '16:9', ratio: [16, 9] },
  { label: '4:3', ratio: [4, 3] },
  { label: '1:1', ratio: [1, 1] },
  { label: '1.91:1', ratio: [191, 100] },
  { label: '9:16', ratio: [9, 16] },
];

/** What the crop popover's fields hold, as typed. */
export interface CropForm {
  aspect: [number, number] | null;
  mode: 'size' | 'long_edge';
  width: string;
  height: string;
  edge: string;
  framing: 'centre' | 'smart';
}

/** The popover's starting point: the card's current crop, if any. */
export function cropForm(edits: Edits): CropForm {
  const size = edits.size;
  return {
    aspect: edits.aspect,
    mode: size?.kind === 'long_edge' ? 'long_edge' : 'size',
    width: size?.kind === 'exact' || size?.kind === 'width' ? String(size.width) : '0',
    height: size?.kind === 'exact' || size?.kind === 'height' ? String(size.height) : '0',
    edge: size?.kind === 'long_edge' ? String(size.edge) : '0',
    framing: edits.framing,
  };
}

export type CropField = 'width' | 'height' | 'edge';

export type CropCheck =
  | { patch: EditsPatch; larger: boolean; error: null; field: null }
  | { patch: null; larger: false; error: string; field: CropField | null };

/** A field as a side: '' is 0; anything but digits, or more than MAX_SIDE, is an error. */
function side(text: string): number | 'not_number' | 'too_large' {
  const trimmed = text.trim();
  if (trimmed === '') return 0;
  if (!/^\d+$/.test(trimmed)) return 'not_number';
  const value = Number(trimmed);
  return value > MAX_SIDE ? 'too_large' : value;
}

/** Check the popover (spec §5.3) and build the `card_edit` patch: aspect, framing and size. */
export function checkCrop(form: CropForm, dims: [number, number] | null): CropCheck {
  const fields: CropField[] = form.mode === 'size' ? ['width', 'height'] : ['edge'];
  const values: Record<CropField, number> = { width: 0, height: 0, edge: 0 };
  for (const field of fields) {
    const value = side(form[field]);
    if (typeof value === 'string') {
      const error = t(value === 'not_number' ? 'crop.not_number' : 'crop.too_large');
      return { patch: null, larger: false, error, field };
    }
    values[field] = value;
  }
  const { width, height, edge } = values;
  let size: Size | null = null;
  if (form.mode === 'long_edge') {
    if (edge > 0) size = { kind: 'long_edge', edge };
  } else if (width > 0 && height > 0) {
    size = { kind: 'exact', width, height };
  } else if (width > 0) {
    size = { kind: 'width', width };
  } else if (height > 0) {
    size = { kind: 'height', height };
  }
  if (size === null && form.aspect === null) {
    return { patch: null, larger: false, error: t('crop.nothing'), field: null };
  }
  const larger =
    dims !== null &&
    (width > dims[0] || height > dims[1] || edge > Math.max(dims[0], dims[1]));
  return {
    patch: { aspect: form.aspect, framing: form.framing, size },
    larger,
    error: null,
    field: null,
  };
}

/** ×0.5 and ×2: every non-zero size field, rounded, kept within 1..MAX_SIDE; fields that are not numbers stay. */
export function scaleForm(form: CropForm, factor: number): CropForm {
  const scale = (text: string) => {
    const value = side(text);
    if (typeof value === 'string' || value === 0) return text;
    return String(Math.min(MAX_SIDE, Math.max(1, Math.round(value * factor))));
  };
  return { ...form, width: scale(form.width), height: scale(form.height), edge: scale(form.edge) };
}

/** The largest window of `ratio` inside `dims`, rounded, at least 1×1 (mirrors `ferry_core::edit::geometry::fit_ratio`). */
export function fitRatio([w, h]: [number, number], [rw, rh]: [number, number]): [number, number] {
  if (w * rh > h * rw) {
    return [Math.min(w, Math.max(1, Math.floor((h * rw + Math.floor(rh / 2)) / rh))), h];
  }
  return [w, Math.min(h, Math.max(1, Math.floor((w * rh + Math.floor(rw / 2)) / rw)))];
}

/** Where the crop cuts, as fractions of the original: centred (Smart framing is decided by Rust and not previewed). */
export interface CropFrame {
  x: number;
  y: number;
  w: number;
  h: number;
}

export function cropFrame(dims: [number, number] | null, form: CropForm): CropFrame | null {
  if (dims === null) return null;
  const check = checkCrop(form, dims);
  const size = check.patch?.size;
  const ratio: [number, number] | null =
    size?.kind === 'exact' ? [size.width, size.height] : (check.patch?.aspect ?? null);
  if (ratio === null) return null;
  const [fw, fh] = fitRatio(dims, ratio);
  return {
    x: (dims[0] - fw) / 2 / dims[0],
    y: (dims[1] - fh) / 2 / dims[1],
    w: fw / dims[0],
    h: fh / dims[1],
  };
}

/** `frame` in card pixels, over a thumbnail drawn with `object-fit: contain` in a `box`-sized card. */
export function frameBox(
  frame: CropFrame,
  dims: [number, number],
  box: [number, number],
): { left: number; top: number; width: number; height: number } {
  const scale = Math.min(box[0] / dims[0], box[1] / dims[1]);
  const [iw, ih] = [dims[0] * scale, dims[1] * scale];
  const [ix, iy] = [(box[0] - iw) / 2, (box[1] - ih) / 2];
  return { left: ix + frame.x * iw, top: iy + frame.y * ih, width: frame.w * iw, height: frame.h * ih };
}

/** Fit under size chips, in MB. */
export const FIT_PRESETS_MB = [1, 5, 10, 25];

/** The fit field in bytes (decimal units, like the cards), or null when it is not a size above 0. */
export function fitBytes(text: string, unit: 'kb' | 'mb'): number | null {
  const trimmed = text.trim();
  if (!/^\d+(\.\d+)?$/.test(trimmed)) return null;
  const bytes = Math.round(Number(trimmed) * (unit === 'mb' ? 1_000_000 : 1_000));
  return bytes > 0 ? bytes : null;
}

/** The fit field's starting text and unit: the card's current target, or empty in MB. */
export function fitForm(fitUnder: number | null): { text: string; unit: 'kb' | 'mb' } {
  if (fitUnder === null) return { text: '', unit: 'mb' };
  return fitUnder % 1_000_000 === 0
    ? { text: String(fitUnder / 1_000_000), unit: 'mb' }
    : { text: String(Math.round(fitUnder / 1_000)), unit: 'kb' };
}
```

`fitRatio` mirrors `ferry_core::edit::geometry::fit_ratio` (open it and compare before moving on; if the Rust rounding differs, copy the Rust). The frame previews a centred crop; Smart framing is decided by Rust after Apply and is not previewed.

- [ ] **Step 4: The strings**

In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "crop.title": "Crop",
  "crop.free": "Free",
  "crop.width": "Width",
  "crop.height": "Height",
  "crop.keeps_ratio": "0 keeps ratio",
  "crop.size": "Size",
  "crop.long_edge": "Long edge",
  "crop.smart": "Smart",
  "crop.centre": "Centre",
  "crop.half": "×0.5",
  "crop.double": "×2",
  "crop.saved": "Saved",
  "crop.restore_defaults": "Restore defaults",
  "crop.apply": "Crop",
  "crop.not_number": "Whole numbers only",
  "crop.too_large": "At most 30000 px",
  "crop.nothing": "Choose a ratio or a size",
  "crop.larger": "Larger than the original",
  "fit.title": "Fit under size",
  "fit.size": "Size",
  "fit.apply": "Fit",
  "fit.invalid": "Enter a size above 0",
  "unit.kb": "KB",
```

(`unit.mb` already exists.)

- [ ] **Step 5: Run the tests**

Run: `pnpm test`
Expected: PASS — 69 tests.

- [ ] **Step 6: The popovers**

Create `src/stack/CropPopover.svelte`:

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { untrack } from 'svelte';
  import { ASPECTS, checkCrop, cropForm, cropFrame, scaleForm, type CropForm, type CropFrame } from '../lib/edits';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';

  /** Crop beside the card (spec §5.3, mockup "crop popover A"). */
  let {
    card,
    sizes,
    onapply,
    onclose,
    onframe,
  }: {
    card: JobUpdate;
    /** The saved sizes from settings (`images.crop_sizes`). */
    sizes: [number, number][];
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
    /** The live crop frame for the card's thumbnail. */
    onframe: (frame: CropFrame | null) => void;
  } = $props();

  // The form starts from the card's edits once; later job updates don't reset what is typed.
  let form = $state<CropForm>(untrack(() => cropForm(card.edits)));
  const check = $derived(checkCrop(form, card.dims_before));

  $effect(() => {
    onframe(cropFrame(card.dims_before, form));
    return () => onframe(null);
  });

  function apply() {
    if (check.patch) onapply(check.patch);
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      apply();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }

  function same(a: [number, number] | null, b: [number, number] | null): boolean {
    return a?.[0] === b?.[0] && a?.[1] === b?.[1];
  }

  function focusFirst(node: HTMLElement) {
    node.querySelector<HTMLInputElement>('input')?.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="popover"
  role="dialog"
  aria-label={t('crop.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="seg">
    {#each ASPECTS as aspect (aspect.label)}
      <button
        type="button"
        class:on={same(form.aspect, aspect.ratio)}
        aria-pressed={same(form.aspect, aspect.ratio)}
        onclick={() => (form.aspect = aspect.ratio)}>{aspect.ratio ? aspect.label : t('crop.free')}</button
      >
    {/each}
  </div>

  <div class="fields">
    {#if form.mode === 'size'}
      <input
        type="text"
        inputmode="numeric"
        aria-label={t('crop.width')}
        aria-invalid={check.field === 'width'}
        bind:value={form.width}
      />
      <span>×</span>
      <input
        type="text"
        inputmode="numeric"
        aria-label={t('crop.height')}
        aria-invalid={check.field === 'height'}
        bind:value={form.height}
      />
      <span class="muted">{t('crop.keeps_ratio')}</span>
    {:else}
      <input
        type="text"
        inputmode="numeric"
        aria-label={t('crop.long_edge')}
        aria-invalid={check.field === 'edge'}
        bind:value={form.edge}
      />
    {/if}
  </div>

  <div class="seg">
    <button type="button" class:on={form.mode === 'size'} aria-pressed={form.mode === 'size'} onclick={() => (form.mode = 'size')}>{t('crop.size')}</button>
    <button type="button" class:on={form.mode === 'long_edge'} aria-pressed={form.mode === 'long_edge'} onclick={() => (form.mode = 'long_edge')}>{t('crop.long_edge')}</button>
    <span class="gap"></span>
    <button type="button" class:on={form.framing === 'smart'} aria-pressed={form.framing === 'smart'} onclick={() => (form.framing = 'smart')}>{t('crop.smart')}</button>
    <button type="button" class:on={form.framing === 'centre'} aria-pressed={form.framing === 'centre'} onclick={() => (form.framing = 'centre')}>{t('crop.centre')}</button>
    <span class="gap"></span>
    <button type="button" onclick={() => (form = scaleForm(form, 0.5))}>{t('crop.half')}</button>
    <button type="button" onclick={() => (form = scaleForm(form, 2))}>{t('crop.double')}</button>
  </div>

  <div class="saved">
    <span class="muted">{t('crop.saved')}:</span>
    {#each sizes as [w, h] (`${w}x${h}`)}
      <button
        type="button"
        class="link"
        onclick={() => (form = { ...form, mode: 'size', width: String(w), height: String(h) })}>{w}×{h}</button
      >
    {/each}
    <button type="button" class="link" onclick={() => invoke('crop_sizes_reset').catch((e) => console.warn('crop_sizes_reset', e))}>{t('crop.restore_defaults')}</button>
  </div>

  {#if check.error}
    <p class="error" role="alert">{check.error}</p>
  {:else if check.larger}
    <p class="muted">{t('crop.larger')}</p>
  {/if}

  <button type="button" class="apply" disabled={check.patch === null} onclick={apply}>{t('crop.apply')}</button>
</div>

<style>
  .popover {
    width: 300px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 14px;
    background: #2c2c2e;
    color: #fff;
    font-size: 12px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.25);
    outline: none;
  }
  .seg,
  .saved {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .gap {
    width: 6px;
  }
  button {
    border: 0;
    border-radius: 7px;
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    background: rgba(255, 255, 255, 0.12);
    cursor: pointer;
  }
  button.on {
    background: #0a84ff;
  }
  button.link {
    background: none;
    padding: 2px 4px;
    text-decoration: underline;
  }
  .fields {
    display: flex;
    gap: 6px;
    align-items: center;
  }
  input {
    width: 60px;
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 6px;
  }
  input[aria-invalid='true'] {
    border-color: #ff453a;
  }
  .muted {
    margin: 0;
    opacity: 0.65;
  }
  .error {
    margin: 0;
    color: #ffb3ae;
  }
  .apply {
    align-self: flex-end;
    padding: 3px 12px;
    font-weight: 600;
    background: #0a84ff;
  }
  .apply:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
```

Create `src/stack/FitPopover.svelte`:

```svelte
<script lang="ts">
  import { untrack } from 'svelte';
  import { FIT_PRESETS_MB, fitBytes, fitForm } from '../lib/edits';
  import { formatBytes } from '../lib/format';
  import { t } from '../lib/i18n';
  import type { EditsPatch, JobUpdate } from '../lib/ipc';

  /** Fit under size beside the card (spec §5.3). */
  let {
    card,
    onapply,
    onclose,
  }: {
    card: JobUpdate;
    onapply: (edits: EditsPatch) => void;
    onclose: () => void;
  } = $props();

  const start = untrack(() => fitForm(card.edits.fit_under));
  let text = $state(start.text);
  let unit = $state<'kb' | 'mb'>(start.unit);
  const bytes = $derived(fitBytes(text, unit));

  function apply(target: number | null = bytes) {
    if (target !== null) onapply({ fit_under: target });
  }

  function keydown(event: KeyboardEvent) {
    if (event.key === 'Enter') {
      event.preventDefault();
      apply();
    } else if (event.key === 'Escape') {
      event.preventDefault();
      onclose();
    }
  }

  function focusFirst(node: HTMLElement) {
    node.querySelector<HTMLInputElement>('input')?.focus();
  }
</script>

<!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
<div
  class="popover"
  role="dialog"
  aria-label={t('fit.title')}
  data-popover
  data-card-id={card.id}
  tabindex="-1"
  use:focusFirst
  onkeydown={keydown}
>
  <div class="row">
    <input type="text" inputmode="decimal" aria-label={t('fit.size')} aria-invalid={text !== '' && bytes === null} bind:value={text} />
    <button type="button" class:on={unit === 'kb'} aria-pressed={unit === 'kb'} onclick={() => (unit = 'kb')}>{t('unit.kb')}</button>
    <button type="button" class:on={unit === 'mb'} aria-pressed={unit === 'mb'} onclick={() => (unit = 'mb')}>{t('unit.mb')}</button>
  </div>
  <div class="row">
    {#each FIT_PRESETS_MB as mb (mb)}
      <button type="button" onclick={() => apply(mb * 1_000_000)}>{formatBytes(mb * 1_000_000)}</button>
    {/each}
  </div>
  {#if text !== '' && bytes === null}
    <p class="error" role="alert">{t('fit.invalid')}</p>
  {/if}
  <button type="button" class="apply" disabled={bytes === null} onclick={() => apply()}>{t('fit.apply')}</button>
</div>

<style>
  .popover {
    width: 220px;
    box-sizing: border-box;
    display: flex;
    flex-direction: column;
    gap: 8px;
    padding: 12px;
    border-radius: 14px;
    background: #2c2c2e;
    color: #fff;
    font-size: 12px;
    box-shadow: 0 10px 28px rgba(0, 0, 0, 0.25);
    outline: none;
  }
  .row {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  button {
    border: 0;
    border-radius: 7px;
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    background: rgba(255, 255, 255, 0.12);
    cursor: pointer;
  }
  button.on {
    background: #0a84ff;
  }
  input {
    width: 70px;
    padding: 2px 8px;
    font: inherit;
    color: #fff;
    background: rgba(255, 255, 255, 0.1);
    border: 1px solid rgba(255, 255, 255, 0.25);
    border-radius: 6px;
  }
  input[aria-invalid='true'] {
    border-color: #ff453a;
  }
  .error {
    margin: 0;
    color: #ffb3ae;
  }
  .apply {
    align-self: flex-end;
    padding: 3px 12px;
    font-weight: 600;
    background: #0a84ff;
  }
  .apply:disabled {
    opacity: 0.45;
    cursor: default;
  }
</style>
```

`untrack` keeps the form from being reset by later job updates for the same card while the user types (and keeps `svelte-check` free of the `state_referenced_locally` warning).

- [ ] **Step 7: The stack renders the popover beside its card**

7a. In `src/stack/Stack.svelte`, replace `  import type { JobUpdate, StackConfig } from '../lib/ipc';` with

```ts
  import type { CropFrame } from '../lib/edits';
  import type { EditsPatch, JobUpdate, StackConfig } from '../lib/ipc';
```

and after `  import Card from './Card.svelte';` add

```ts
  import CropPopover from './CropPopover.svelte';
  import FitPopover from './FitPopover.svelte';
```

7b. After the `setTool` function (Task 4) add:

```ts

  /** The crop popover's live frame, drawn on its card's thumbnail. */
  let frame = $state<CropFrame | null>(null);

  // Crop or Fit: close the popover (handing the keyboard back), then run the edit.
  function apply(id: string, edits: EditsPatch) {
    setTool(id, null);
    invoke('card_edit', { id, edits }).catch((e) => console.warn('card_edit', e));
  }

  // A click elsewhere in the stack closes a popover (spec §5.3 "click-away").
  function clickAway(event: PointerEvent) {
    if (tool === null || (tool.kind !== 'crop' && tool.kind !== 'fit')) return;
    const target = event.target as Element;
    if (target.closest('[data-popover]') || target.closest(`[data-action="${tool.kind}"]`)) return;
    tool = null;
  }

  // Another app took the keyboard: a popover or a rename field closes.
  function windowBlur() {
    if (tool !== null && tool.kind !== 'slider') tool = null;
  }
```

The crop button is skipped by `clickAway` so its own `onclick` (Task 5, a toggle) decides; otherwise pointerdown would close and the click would reopen.

7c. Replace the stack's opening tag

```svelte
<!-- Native pointer events cover Windows; on macOS the panel never gets them and Rust forwards the pointer instead. -->
<div
  class="stack"
  class:top
  bind:this={root}
  role="presentation"
  onpointermove={(event) => track(event.target as Element)}
  onpointerleave={leave}
>
```

with

```svelte
<svelte:window onblur={windowBlur} />

<!-- Native pointer events cover Windows; on macOS the panel never gets them and Rust forwards the pointer instead. -->
<div
  class="stack"
  class:top
  class:left={railRight}
  bind:this={root}
  role="presentation"
  onpointermove={(event) => track(event.target as Element)}
  onpointerleave={leave}
  onpointerdown={clickAway}
>
```

7d. Replace the whole `{#each cards as card (card.id)} … {/each}` block with:

```svelte
  {#each cards as card (card.id)}
    {@const open = tool?.id === card.id ? tool.kind : null}
    <!-- A popover opens beside its card, toward the screen centre; the window grows to fit (spec §5.3). -->
    <div class="row">
      <Card
        {card}
        hovered={card.id === hovered}
        {held}
        hotAction={hot?.card === card.id ? hot.action : null}
        compact={config.card_size === 'compact'}
        {railRight}
        autoHideSecs={config.auto_hide_secs}
        compression={config.compression}
        tool={open}
        onclose={() => close(card.id)}
        ondrag={dragState}
        ontool={(kind) => setTool(card.id, kind)}
        frame={open === 'crop' ? frame : null}
      />
      {#if open === 'crop'}
        <CropPopover
          {card}
          sizes={config.crop_sizes}
          onapply={(edits) => apply(card.id, edits)}
          onclose={() => setTool(card.id, null)}
          onframe={(next) => (frame = next)}
        />
      {:else if open === 'fit'}
        <FitPopover {card} onapply={(edits) => apply(card.id, edits)} onclose={() => setTool(card.id, null)} />
      {/if}
    </div>
  {/each}
```

7e. In `<style>`, after the `.stack { … }` rule add:

```css
  /* Cards keep to the screen edge; a popover sits on their screen-centre side. */
  .row {
    display: flex;
    flex-direction: row-reverse;
    align-items: flex-start;
    gap: 8px;
  }
  .stack.left .row {
    flex-direction: row;
  }
```

(`railRight` is true for the left corners, so `.left` means the stack hugs the left edge.)

- [ ] **Step 8: The card draws the frame and the note**

8a. In `src/stack/Card.svelte`, add `frameBox,` and `type CropFrame,` to the `../lib/edits` import (after `formatChoices,`).

8b. In the props destructuring add `frame = null,` after `tool = null,`, and in the props type, after the `tool?: Tool | null;` line:

```ts
    /** The crop popover's live frame, drawn over the thumbnail. */
    frame?: CropFrame | null;
```

8c. After `  const canRename = $derived(canDrag(card));` add:

```ts
  const frameRect = $derived(
    frame && card.dims_before
      ? frameBox(frame, card.dims_before, compact ? [220, 150] : [300, 200])
      : null,
  );
```

(300×200 and 220×150 are the card sizes in `.card` / `.card.compact`; if those rules say otherwise, use their numbers.)

8d. After `  <div class="scrim" aria-hidden="true"></div>` add:

```svelte
  {#if frameRect}
    <div
      class="frame"
      aria-hidden="true"
      style="left: {frameRect.left}px; top: {frameRect.top}px; width: {frameRect.width}px; height: {frameRect.height}px"
    ></div>
  {/if}
```

8e. M3a's fit-under shortfall ("Couldn't reach 1 MB — try downscaling") arrives as `card.note`. Right before `    {#if card.state.kind === 'error'}` (the buttons row) add:

```svelte
    {#if card.note}
      <span class="note" role="status">{card.note}</span>
    {/if}
```

8f. In `<style>`, before `.converted {` add:

```css
  .frame {
    position: absolute;
    box-sizing: border-box;
    border: 2px dashed #fff;
    box-shadow: 0 0 0 999px rgba(0, 0, 0, 0.45);
    pointer-events: none;
  }
  .note {
    font-size: 11px;
    font-weight: 600;
    color: #ffd27a;
  }
```

- [ ] **Step 9: "Restore defaults" in Rust**

In `src-tauri/src/commands.rs`, add `Images` to the settings import (`use ferry_core::settings::{CardSize, Corner, Images, Settings, TrayStyle};`) and after `settings_set` add:

```rust

/// The crop popover's "Restore defaults": put the saved crop sizes back.
#[tauri::command(async)]
pub fn crop_sizes_reset(app: AppHandle) -> Result<Settings, String> {
    prefs::update(&app, |s| s.images.crop_sizes = Images::default().crop_sizes)
}
```

`prefs::update` saves and emits `settings:changed`, which Task 3's listener in `Stack.svelte` turns into a new `config.crop_sizes`. In `src-tauri/src/lib.rs`, append `commands::crop_sizes_reset,` at the end of `generate_handler!` (after `windows::stack_focus,`).

- [ ] **Step 10: Run everything**

Run: `pnpm test`
Expected: PASS — 69 tests.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

Run: `pnpm build`
Expected: `✓ built`.

- [ ] **Step 11: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
cargo test -p ferry 2>&1 | grep "test result"
git add src/lib/edits.ts src/lib/edits.test.ts src/lib/i18n/en.json src/stack/CropPopover.svelte src/stack/FitPopover.svelte src/stack/Stack.svelte src/stack/Card.svelte src-tauri/src/commands.rs src-tauri/src/lib.rs
git commit -m "feat(ui): crop and fit popovers beside the card

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clippy clean; `test result: ok`.

---

### Task 8: Native context menu

**Files:**
- Create: `src-tauri/src/menu.rs` (the model, the menu, routing; cargo tests)
- Modify: `src-tauri/src/platform/macos_clipboard.rs`, `src-tauri/src/platform/windows_clipboard.rs`, `src-tauri/src/platform/mod.rs` (doc line) (`write_text`)
- Modify: `src-tauri/src/jobs.rs` (`copy_path`)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (`card_menu`, `mod menu;`, `menu::init`)
- Modify: `src/lib/ipc.ts` (`CardAction`), `src/stack/Stack.svelte` (`card:action` listener), `src/stack/Card.svelte` (right-click)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: Task 1's `JobUpdate.source_format`; Task 2's `jobs::edit`, `jobs::convert`, `jobs::rerun`, `jobs::restore`, `jobs::copy`, `jobs::output`, `Entry.last`; Task 4's `setTool(id, kind)`; Task 5's `openMenu` (`act('card_menu', { x, y })` from the ⋯ button); existing `windows::STACK`, `isBusy`, `close(id)` in `Stack.svelte`, keys `card.reveal.mac`/`.windows`, `card.compare`, `card.reoptimise`, `card.aggressive`, `card.crop`, `card.restore`, `card.close`.
- Produces: `pub enum menu::Action { Open, Reveal, Copy, CopyPath, Rename, Compare, Preview, Rerun, Aggressive, Fit, Scale(u8), Crop, Convert(Format), KeepBoth(Format), Restore, Close }` with `pub fn page(self) -> Option<&'static str>`; `menu::item_id`, `menu::parse_id`, `menu::enabled(card, action) -> bool`, `menu::convert_formats(card)`, `menu::popup(app, window, id, x, y)`, `menu::init(app)`, `menu::send(app: &AppHandle, id: JobId, action: &'static str)` (emits `card:action`); the event `card:action` with payload `{ id: JobId, action: "rename" | "crop" | "fit" | "compare" | "preview" | "close" }` sent to the stack (Task 9 sends `compare` and `preview` the same way); TS `interface CardAction`; `platform::clipboard::write_text(text: &str) -> Result<u64, String>`; `jobs::copy_path(app, id)`; command `card_menu({ id, x, y })`.

Spec §4.5: Open · Show in Finder/Explorer ─ Copy · Copy path · Rename… ─ Compare with original ⌘D · Preview Space ─ Re-optimise · Optimise aggressively · Fit under size… ─ Downscale ▸ (90 % … 10 %) · Crop… · Convert to ▸ · Convert and keep both ▸ ─ Restore original ─ Close. Items that need the original follow `can_edit`; Restore needs a backup. The menu is popped from Rust (`popup_menu_at`, a sync command so it runs on the main thread). Tauri's `on_menu_event` is global: the tray's handler already ignores ids it does not know, and card ids start with `card:` so neither handler takes the other's items.

- [ ] **Step 1: "Copy path" on the clipboard (test first)**

In `src-tauri/src/platform/macos_clipboard.rs`, in `mod tests`, before `fn an_image_can_carry_its_working_file` add:

```rust
    #[test]
    fn text_round_trips_and_the_change_count_moves() {
        let pb = Private::new();
        let before = pb.0.changeCount() as u64;
        let seq = write_text_to(&pb.0, "/Users/me/shot 1.png").unwrap();
        assert!(seq > before);
        let text = pb.0.stringForType(&NSString::from_str(TEXT)).unwrap();
        assert_eq!(text.to_string(), "/Users/me/shot 1.png");
        assert_eq!(read_from(&pb.0).image, None);
    }
```

Run: `cargo test -p ferry text_round_trips`
Expected: FAIL to compile — `write_text_to` and `TEXT` not found.

Then, after `const FILE_URL: &str = "public.file-url";` add `const TEXT: &str = "public.utf8-plain-text";`, and right above the doc comment of `pub fn watch` add:

```rust
/// Put plain text on the clipboard ("Copy path").
pub fn write_text(text: &str) -> Result<u64, String> {
    write_text_to(&NSPasteboard::generalPasteboard(), text)
}

fn write_text_to(pb: &NSPasteboard, text: &str) -> Result<u64, String> {
    let seq = pb.clearContents();
    if !pb.setString_forType(&NSString::from_str(text), &NSString::from_str(TEXT)) {
        return Err("the clipboard refused the text".into());
    }
    Ok(seq as u64)
}

```

In `src-tauri/src/platform/windows_clipboard.rs`, right above the doc comment of `pub fn watch` add (clipboard-win 5.4.1 `raw::set_string(data: &str) -> SysResult<()>` empties the clipboard itself):

```rust
/// Put plain text on the clipboard ("Copy path"). `set_string` empties it first.
pub fn write_text(text: &str) -> Result<u64, String> {
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::set_string(text).map_err(|e| e.to_string())?;
    }
    Ok(seq())
}

```

In `src-tauri/src/platform/mod.rs`, change the doc's second line to name it (only the `//!` lines change):

```rust
//! Per-OS native glue. Each OS module exposes the same functions, and each
//! `clipboard` module the same `read`, `write_image`, `write_files`, `write_text`
//! and `watch`.
```

In `src-tauri/src/jobs.rs`, before `/// Stop a queued or running job and leave its file as it was.` add:

```rust
/// "Copy path": the result's full path as text.
pub fn copy_path(app: &AppHandle, id: &JobId) -> Result<(), String> {
    let path = output(app, id).ok_or("the file is gone")?;
    let state = app.state::<AppState>();
    let mut log = state.clipboard.lock().unwrap();
    let seq = platform::clipboard::write_text(&path.to_string_lossy())?;
    log.record(seq, None);
    Ok(())
}

```

(Recording the sequence number, as `copy` does, keeps the clipboard watcher from treating Ferry's own write as a new copy.)

Run: `cargo test -p ferry text_round_trips`
Expected: PASS — 1 test. (`copy_path` is unused until Step 2; clippy is run after Step 2.)

- [ ] **Step 2: The menu module**

Create `src-tauri/src/menu.rs`:

```rust
//! The card's context menu (spec §4.5): built from the card's last update,
//! popped up under the ⋯ button or the pointer, and routed back by item id.
//! What ends in the page (rename, crop, fit, compare, preview, close) goes to
//! the stack as a `card:action` event; the rest runs here.

use ferry_core::i18n::{t, tf};
use ferry_core::job::{JobId, JobState, JobUpdate};
use ferry_core::sniff::Format;
use serde::Serialize;
use tauri::menu::{IsMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::{AppHandle, Emitter, LogicalPosition, Manager, WebviewWindow, Wry};
use tracing::warn;

use crate::jobs;
use crate::state::AppState;
use crate::windows::STACK;

const PREFIX: &str = "card:";

/// What a card menu item does.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    Open,
    Reveal,
    Copy,
    CopyPath,
    Rename,
    Compare,
    Preview,
    Rerun,
    Aggressive,
    Fit,
    /// Downscale to this percent.
    Scale(u8),
    Crop,
    Convert(Format),
    KeepBoth(Format),
    Restore,
    Close,
}

const FORMATS: [Format; 4] = [Format::Png, Format::Jpeg, Format::Gif, Format::WebP];

/// The actions without a value, by their id part.
const NAMED: [(&str, Action); 13] = [
    ("open", Action::Open),
    ("reveal", Action::Reveal),
    ("copy", Action::Copy),
    ("copy_path", Action::CopyPath),
    ("rename", Action::Rename),
    ("compare", Action::Compare),
    ("preview", Action::Preview),
    ("rerun", Action::Rerun),
    ("aggressive", Action::Aggressive),
    ("fit", Action::Fit),
    ("crop", Action::Crop),
    ("restore", Action::Restore),
    ("close", Action::Close),
];

impl Action {
    /// The item id's last part: "open", "scale-50", "convert-webp", "keep_both-jpg".
    fn key(self) -> String {
        match self {
            Action::Scale(percent) => format!("scale-{percent}"),
            Action::Convert(format) => format!("convert-{}", format.extension()),
            Action::KeepBoth(format) => format!("keep_both-{}", format.extension()),
            named => NAMED
                .iter()
                .find(|(_, a)| *a == named)
                .map(|(key, _)| key.to_string())
                .expect("every action without a value is in NAMED"),
        }
    }

    fn parse(key: &str) -> Option<Self> {
        let format = |ext: &str| FORMATS.into_iter().find(|f| f.extension() == ext);
        match key.split_once('-') {
            Some(("scale", value)) => value.parse().ok().map(Action::Scale),
            Some(("convert", value)) => format(value).map(Action::Convert),
            Some(("keep_both", value)) => format(value).map(Action::KeepBoth),
            Some(_) => None,
            None => NAMED.iter().find(|(k, _)| *k == key).map(|(_, a)| *a),
        }
    }

    /// The `card:action` the page handles, for actions that end in the page.
    pub fn page(self) -> Option<&'static str> {
        Some(match self {
            Action::Rename => "rename",
            Action::Crop => "crop",
            Action::Fit => "fit",
            Action::Compare => "compare",
            Action::Preview => "preview",
            Action::Close => "close",
            _ => return None,
        })
    }
}

/// The menu item id: `card:<job id>:<action>`. Job ids have no `:`.
pub fn item_id(id: &JobId, action: Action) -> String {
    format!("{PREFIX}{}:{}", id.as_str(), action.key())
}

/// The card and action behind a menu item id; `None` for other menus' items.
pub fn parse_id(item: &str) -> Option<(JobId, Action)> {
    let (id, key) = item.strip_prefix(PREFIX)?.split_once(':')?;
    Some((job_id(id)?, Action::parse(key)?))
}

/// A `JobId` from its text (the type has no public constructor).
fn job_id(text: &str) -> Option<JobId> {
    serde_json::from_value(serde_json::Value::String(text.into())).ok()
}

/// Whether the item applies to the card now (spec §4.5): items that need the
/// original follow `can_edit`, Restore needs a backup, and a busy card offers
/// only Close.
pub fn enabled(card: &JobUpdate, action: Action) -> bool {
    let idle = !matches!(card.state, JobState::Queued | JobState::Optimising { .. });
    let file = idle && card.output.is_some();
    match action {
        Action::Close => true,
        Action::Open
        | Action::Reveal
        | Action::Copy
        | Action::CopyPath
        | Action::Rename
        | Action::Preview => file,
        Action::Restore => idle && card.can_restore,
        Action::Convert(format) => card.can_edit && card.format != Some(format),
        Action::Scale(percent) => card.can_edit && card.edits.scale.unwrap_or(100) != percent,
        Action::Compare
        | Action::Rerun
        | Action::Aggressive
        | Action::Fit
        | Action::Crop
        | Action::KeepBoth(_) => card.can_edit,
    }
}

/// "Convert to ▸": PNG, JPEG, WebP, and GIF first on a card that started as a
/// GIF (like the format bar).
pub fn convert_formats(card: &JobUpdate) -> Vec<Format> {
    let gif = (card.source_format == Some(Format::Gif)).then_some(Format::Gif);
    gif.into_iter()
        .chain([Format::Png, Format::Jpeg, Format::WebP])
        .collect()
}

/// Pop the card's menu at `(x, y)`, in the stack page's CSS pixels.
pub fn popup(
    app: &AppHandle,
    window: &WebviewWindow,
    id: &JobId,
    x: f64,
    y: f64,
) -> Result<(), String> {
    // Copy the card first: building items waits on nothing, but keep no lock across the popup.
    let card = app
        .state::<AppState>()
        .jobs
        .lock()
        .unwrap()
        .get(id)
        .map(|entry| entry.last.lock().unwrap().clone())
        .ok_or("this card's job is gone")?;
    let menu = build(app, &card).map_err(|e| e.to_string())?;
    window
        .popup_menu_at(&menu, LogicalPosition::new(x, y))
        .map_err(|e| e.to_string())
}

fn build(app: &AppHandle, card: &JobUpdate) -> tauri::Result<Menu<Wry>> {
    let item = |action: Action, label: String, accelerator: Option<&str>| {
        MenuItem::with_id(
            app,
            item_id(&card.id, action),
            label,
            enabled(card, action),
            accelerator,
        )
    };
    let submenu = |label: String, items: &[MenuItem<Wry>]| {
        let refs: Vec<&dyn IsMenuItem<Wry>> =
            items.iter().map(|i| i as &dyn IsMenuItem<Wry>).collect();
        Submenu::with_items(app, label, card.can_edit, &refs)
    };
    let reveal = if cfg!(target_os = "macos") {
        "card.reveal.mac"
    } else {
        "card.reveal.windows"
    };
    let scales = (1..=9)
        .rev()
        .map(|n| {
            let percent = n * 10;
            let label = tf("menu.percent", &[("n", &percent.to_string())]);
            item(Action::Scale(percent), label, None)
        })
        .collect::<tauri::Result<Vec<_>>>()?;
    let formats = convert_formats(card);
    let converts = formats
        .iter()
        .map(|&f| item(Action::Convert(f), f.label().into(), None))
        .collect::<tauri::Result<Vec<_>>>()?;
    let twins = formats
        .iter()
        .map(|&f| item(Action::KeepBoth(f), f.label().into(), None))
        .collect::<tauri::Result<Vec<_>>>()?;

    Menu::with_items(
        app,
        &[
            &item(Action::Open, t("menu.open"), None)?,
            &item(Action::Reveal, t(reveal), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Copy, t("menu.copy"), None)?,
            &item(Action::CopyPath, t("menu.copy_path"), None)?,
            &item(Action::Rename, t("menu.rename"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Compare, t("card.compare"), Some("CmdOrCtrl+D"))?,
            &item(Action::Preview, t("menu.preview"), Some("Space"))?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Rerun, t("card.reoptimise"), None)?,
            &item(Action::Aggressive, t("card.aggressive"), None)?,
            &item(Action::Fit, t("menu.fit"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &submenu(t("menu.downscale"), &scales)?,
            &item(Action::Crop, t("card.crop"), None)?,
            &submenu(t("menu.convert"), &converts)?,
            &submenu(t("menu.keep_both"), &twins)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Restore, t("card.restore"), None)?,
            &PredefinedMenuItem::separator(app)?,
            &item(Action::Close, t("card.close"), None)?,
        ],
    )
}

/// Route card menu items. Other menus' items (the tray's) are not ours.
pub fn init(app: &AppHandle) {
    app.on_menu_event(|app, event| {
        if let Some((id, action)) = parse_id(event.id().as_ref()) {
            run(app, id, action);
        }
    });
}

#[derive(Clone, Serialize)]
struct CardAction {
    id: JobId,
    action: &'static str,
}

/// Hand an action that ends in the page to the stack (`card:action`).
pub fn send(app: &AppHandle, id: JobId, action: &'static str) {
    let _ = app.emit_to(STACK, "card:action", CardAction { id, action });
}

fn run(app: &AppHandle, id: JobId, action: Action) {
    if let Some(page) = action.page() {
        send(app, id, page);
        return;
    }
    // Menu events arrive on the main thread; file and clipboard work does not belong there.
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let done = match action {
            Action::Open | Action::Reveal => jobs::output(&app, &id)
                .ok_or_else(|| "the file is gone".to_string())
                .and_then(|path| {
                    if action == Action::Open {
                        tauri_plugin_opener::open_path(path, None::<&str>)
                    } else {
                        tauri_plugin_opener::reveal_item_in_dir(path)
                    }
                    .map_err(|e| e.to_string())
                }),
            Action::Copy => jobs::copy(&app, &id),
            Action::CopyPath => jobs::copy_path(&app, &id),
            Action::Rerun => jobs::rerun(&app, &id, false),
            Action::Aggressive => jobs::rerun(&app, &id, true),
            Action::Scale(percent) => jobs::edit(
                &app,
                &id,
                ferry_core::edit::EditsPatch {
                    scale: Some(Some(percent)),
                    ..Default::default()
                },
            ),
            Action::Convert(format) => jobs::convert(&app, &id, format, false),
            Action::KeepBoth(format) => jobs::convert(&app, &id, format, true),
            Action::Restore => jobs::restore(&app, &id),
            _ => Ok(()),
        };
        if let Err(e) = done {
            warn!(?action, "card menu: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use ferry_core::pipeline;

    /// A finished PNG result with its original backed up.
    fn done() -> JobUpdate {
        let mut card = pipeline::notice("x".into());
        card.id = job_id("18f-1").unwrap();
        card.state = JobState::Done;
        card.output = Some("/tmp/shot.png".into());
        card.format = Some(Format::Png);
        card.source_format = Some(Format::Png);
        card.can_edit = true;
        card.can_restore = true;
        card
    }

    const ALL: [Action; 16] = [
        Action::Open,
        Action::Reveal,
        Action::Copy,
        Action::CopyPath,
        Action::Rename,
        Action::Compare,
        Action::Preview,
        Action::Rerun,
        Action::Aggressive,
        Action::Fit,
        Action::Scale(50),
        Action::Crop,
        Action::Convert(Format::WebP),
        Action::KeepBoth(Format::Jpeg),
        Action::Restore,
        Action::Close,
    ];

    #[test]
    fn every_item_id_reads_back_as_its_card_and_action() {
        let card = done();
        for action in ALL {
            assert_eq!(
                parse_id(&item_id(&card.id, action)),
                Some((card.id.clone(), action))
            );
        }
        assert_eq!(parse_id("recent:18f-1"), None);
        assert_eq!(parse_id("card:18f-1:explode"), None);
        assert_eq!(parse_id("card:18f-1:scale-x"), None);
    }

    #[test]
    fn a_finished_card_offers_everything_but_its_own_format_and_scale() {
        let card = done();
        for action in ALL {
            assert!(enabled(&card, action), "{action:?}");
        }
        assert!(!enabled(&card, Action::Convert(Format::Png)));
        assert!(enabled(&card, Action::KeepBoth(Format::Png)));
        assert!(!enabled(&card, Action::Scale(100)));
    }

    #[test]
    fn without_the_original_only_file_actions_stay() {
        let mut card = done();
        card.can_edit = false;
        card.can_restore = false;
        let on: Vec<Action> = ALL.into_iter().filter(|&a| enabled(&card, a)).collect();
        assert_eq!(
            on,
            [
                Action::Open,
                Action::Reveal,
                Action::Copy,
                Action::CopyPath,
                Action::Rename,
                Action::Preview,
                Action::Close
            ]
        );
    }

    #[test]
    fn a_busy_card_offers_only_close() {
        let mut card = done();
        card.state = JobState::Queued;
        card.can_edit = false;
        let on: Vec<Action> = ALL.into_iter().filter(|&a| enabled(&card, a)).collect();
        assert_eq!(on, [Action::Close]);
    }

    #[test]
    fn a_gif_card_can_convert_back_to_gif() {
        let mut card = done();
        assert_eq!(
            convert_formats(&card),
            [Format::Png, Format::Jpeg, Format::WebP]
        );
        card.source_format = Some(Format::Gif);
        card.format = Some(Format::WebP);
        assert_eq!(
            convert_formats(&card),
            [Format::Gif, Format::Png, Format::Jpeg, Format::WebP]
        );
        assert!(enabled(&card, Action::Convert(Format::Gif)));
    }
}
```

`Format::extension` gives `jpg` for JPEG, so the ids read `convert-jpg`; only `parse` reads them. In `src-tauri/src/lib.rs` add `mod menu;` after `mod logging;`, and after `    tray::build(app)?;` in `setup` add `    menu::init(app);`.

In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "menu.open": "Open",
  "menu.copy": "Copy",
  "menu.copy_path": "Copy path",
  "menu.rename": "Rename…",
  "menu.preview": "Preview",
  "menu.fit": "Fit under size…",
  "menu.downscale": "Downscale",
  "menu.percent": "{n} %",
  "menu.convert": "Convert to",
  "menu.keep_both": "Convert and keep both",
```

- [ ] **Step 3: The command**

In `src-tauri/src/commands.rs`, change `use crate::{jobs, logging, platform, prefs};` to `use crate::{jobs, logging, menu, platform, prefs};` and right above the `#[tauri::command]` line of `pub fn card_reveal` add:

```rust
/// The card's ⋯ button or right-click: the native context menu at `(x, y)`
/// in the stack page. Sync, so it runs on the main thread, as menus must.
#[tauri::command]
pub fn card_menu(
    app: AppHandle,
    window: tauri::WebviewWindow,
    id: JobId,
    x: f64,
    y: f64,
) -> Result<(), String> {
    menu::popup(&app, &window, &id, x, y)
}

```

In `src-tauri/src/lib.rs`, append `commands::card_menu,` at the end of `generate_handler!` (after `commands::crop_sizes_reset,`).

- [ ] **Step 4: Run the Rust tests**

Run: `cargo test -p ferry menu`
Expected: PASS — 5 tests (`every_item_id_reads_back_as_its_card_and_action`, `a_finished_card_offers_everything_but_its_own_format_and_scale`, `without_the_original_only_file_actions_stay`, `a_busy_card_offers_only_close`, `a_gif_card_can_convert_back_to_gif`).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 5: The page side**

5a. In `src/lib/ipc.ts`, before `export type Corner =` add:

```ts
/** Rust → stack: a menu item or hover key that ends in the page (`menu.rs`, `hover_keys.rs`). */
export interface CardAction {
  id: string;
  action: 'rename' | 'crop' | 'fit' | 'compare' | 'preview' | 'close';
}

```

5b. In `src/stack/Stack.svelte`, import `isBusy` from `../lib/cards` and `CardAction` from `../lib/ipc` (add them to the existing import lists). Before `  const top = $derived(config.corner.startsWith('top'));` add:

```ts
  // A menu item or hover key that ends here. M3c's open_compare and
  // quick_preview reject until M3c is merged; the rejection is only logged.
  function cardAction({ id, action }: CardAction) {
    const card = cards.find((c) => c.id === id);
    if (!card) return;
    if (action === 'compare' || action === 'preview') {
      invoke(action === 'compare' ? 'open_compare' : 'quick_preview', { id }).catch((e) =>
        console.warn(action, e),
      );
    } else if (action === 'close') {
      if (isBusy(card.state)) invoke('card_cancel', { id });
      close(id);
    } else {
      setTool(id, action);
    }
  }

```

and in `onMount`'s `Promise.all([...])`, after `      listen('stack:pointer-leave', leave),` add:

```ts
      listen<CardAction>('card:action', (event) => cardAction(event.payload)),
```

`setTool(id, 'rename' | 'crop' | 'fit')` opens that card's field or popover exactly as its button would (Task 4's `openTool` effect closes it again if the card cannot take it). Fit has no rail button: the menu is its only way in.

5c. In `src/stack/Card.svelte`, change the comment above `openMenu` to `  // The ⋯ button opens the native menu just under itself; right-click opens it at the pointer.` and on the `<article class="card" …>` element, after `  onmousedown={drag}` add:

```svelte
  oncontextmenu={(event) => {
    event.preventDefault();
    void act('card_menu', { x: event.clientX, y: event.clientY });
  }}
```

(`drag` already ignores every button but the left one.)

- [ ] **Step 6: Run everything**

Run: `pnpm test`
Expected: PASS — 69 tests (no new vitest; the menu model is tested in Rust, see Rulings).

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

Run: `cargo test -p ferry 2>&1 | grep "test result"`
Expected: `test result: ok.` with 29 passed on the app crate's unit tests.

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add src-tauri/src/menu.rs src-tauri/src/platform/macos_clipboard.rs src-tauri/src/platform/windows_clipboard.rs src-tauri/src/platform/mod.rs src-tauri/src/jobs.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/ipc.ts src/stack/Stack.svelte src/stack/Card.svelte src/lib/i18n/en.json
git commit -m "feat(app): card context menu

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clean.

---

### Task 9: Hover hotkeys

**Files:**
- Create: `src-tauri/src/hover_keys.rs` (register / unregister / route; cargo tests)
- Modify: `src-tauri/src/platform/macos.rs`, `src-tauri/src/platform/windows.rs`, `src-tauri/src/platform/mod.rs` (`app_is_active`)
- Modify: `src-tauri/src/windows.rs` (`stack_resize` zero size and `stack_focus` drop the keys)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (`card_hover`, `mod hover_keys;`)
- Modify: `src/lib/cards.ts`, `src/lib/cards.test.ts` (`hoverTarget`), `src/stack/Stack.svelte` (send the target)

**Interfaces:**
- Consumes: Task 2's `jobs::edit`, `Entry.last`; Task 3's `StackConfig.hover_hotkeys`; Task 4's `stack_focus`, `OpenTool`, `canDrag`; Task 8's `menu::send(app, id, action)` and the page's `card:action` handler (`compare` → `open_compare`, `preview` → `quick_preview`); the existing `tauri-plugin-global-shortcut` 2.4.1 (`GlobalShortcutExt::on_shortcut`, `unregister`, `Shortcut: FromStr`) as `hotkeys.rs` uses it.
- Produces: `hover_keys::set(app: &AppHandle, card: Option<JobId>)`, `hover_keys::set_keyed(app: &AppHandle, keyed: bool)`; `platform::app_is_active() -> bool`; command `card_hover({ id: string | null })`; TS `hoverTarget(hovered: string | null, cards: JobUpdate[], tool: OpenTool | null, enabled: boolean): string | null`.

Spec §2 / §4.5: the keys are registered globally only while the pointer is over an idle card; on leave, when the card becomes busy, while the stack is key, or with `floating.hover_hotkeys` off, they are unregistered. Space previews, ⌘/Ctrl+D compares, `1`–`9` set 10 %–90 %, `-` steps 10 points down. A registration failure is logged once and the card still works by click (§6). The brief adds: `1`–`9` and `-` are suspended while the compare window or Quick Look has focus, which here means "while Ferry is the active app" (`platform::app_is_active`), so no `on_window_event` hook and no reference to M3c's windows is needed.

The page decides *which* card (`hoverTarget`, from the hover state it already has); Rust decides *whether* the keys are held (`plan`). Every call reaches `hover_keys` on the main thread (`card_hover`, `stack_focus` and `stack_resize` are sync commands, and shortcut handlers run there), which is where the plugin registers keys. Only the accelerators this module registered are unregistered; `unregister_all` would also drop the three main hotkeys.

- [ ] **Step 1: Write the failing vitest**

In `src/lib/cards.test.ts`, add `hoverTarget,` to the `./cards` import (after `dragAllFiles,`) and append:

```ts
describe('the hover hotkeys target', () => {
  const cards = [card({ kind: 'done' })];

  it('is the idle card under the pointer', () => {
    expect(hoverTarget('a', cards, null, true)).toBe('a');
    expect(hoverTarget(null, cards, null, true)).toBeNull();
  });

  it('is nothing while a tool has the keyboard or the setting is off', () => {
    expect(hoverTarget('a', cards, { id: 'a', kind: 'rename' }, true)).toBeNull();
    expect(hoverTarget('a', cards, { id: 'b', kind: 'crop' }, true)).toBeNull();
    expect(hoverTarget('a', cards, null, false)).toBeNull();
  });

  it('is nothing once the card is gone, busy or has no file', () => {
    expect(hoverTarget('a', [], null, true)).toBeNull();
    expect(hoverTarget('a', [card({ kind: 'queued' })], null, true)).toBeNull();
    expect(hoverTarget('a', [card({ kind: 'done' }, { output: null })], null, true)).toBeNull();
  });
});
```

Run: `pnpm test`
Expected: FAIL — `hoverTarget` is not exported from `./cards`.

- [ ] **Step 2: `hoverTarget`**

Append to `src/lib/cards.ts`:

```ts

/** The card the hover hotkeys act on (spec §4.5): the idle card under the pointer,
 *  unless a tool has the keyboard or the setting is off. */
export function hoverTarget(
  hovered: string | null,
  cards: JobUpdate[],
  tool: OpenTool | null,
  enabled: boolean,
): string | null {
  if (!enabled || tool !== null || hovered === null) return null;
  const card = cards.find((c) => c.id === hovered);
  return card && canDrag(card) ? card.id : null;
}
```

Run: `pnpm test`
Expected: PASS — 72 tests.

- [ ] **Step 3: `app_is_active` per OS**

Append to `src-tauri/src/platform/macos.rs` (`NSRunningApplication` is already imported):

```rust

/// Whether Ferry is the active app, i.e. one of its own windows (compare,
/// Quick Look, Settings) has the keyboard. The non-activating stack never makes it so.
pub fn app_is_active() -> bool {
    NSRunningApplication::currentApplication().isActive()
}
```

Append to `src-tauri/src/platform/windows.rs`, and add `use windows_sys::Win32::System::Threading::GetCurrentProcessId;` after the `Foundation::HWND` import and `GetWindowThreadProcessId` to the `WindowsAndMessaging` import list (windows-sys 0.61: `GetWindowThreadProcessId(hwnd: HWND, lpdwprocessid: *mut u32) -> u32`, `GetCurrentProcessId() -> u32`; the `Win32_System_Threading` feature is already on for `windows_clipboard.rs`):

```rust

/// Whether one of Ferry's own windows is the foreground window.
pub fn app_is_active() -> bool {
    let mut pid = 0u32;
    // SAFETY: plain queries; a null foreground window leaves `pid` at 0.
    unsafe {
        GetWindowThreadProcessId(GetForegroundWindow(), &mut pid);
        pid == GetCurrentProcessId()
    }
}
```

In `src-tauri/src/platform/mod.rs`, add right above the `#[cfg(target_os = "macos")]` line that precedes Task 4's `pub use macos::set_stack_key;`:

```rust
#[cfg(target_os = "macos")]
pub use macos::app_is_active;
```

and right above the `#[cfg(windows)]` line that precedes `pub use self::windows::set_stack_key;`:

```rust
#[cfg(windows)]
pub use self::windows::app_is_active;
```

(rustfmt sorts these `pub use` lines; that is where it keeps them.)

- [ ] **Step 4: The module, tests first**

Create `src-tauri/src/hover_keys.rs` with the whole file below, and add `mod hover_keys;` after `mod hotkeys;` in `src-tauri/src/lib.rs`:

```rust
//! Hover hotkeys (spec §2, §4.5): while the pointer rests on an idle card,
//! Space previews it, ⌘/Ctrl+D compares it, `1`–`9` downscale it to 10–90 %
//! and `-` steps it down 10 points. The stack never has the keyboard, so the
//! keys are global, and they are held only while there is a hovered card, the
//! stack is not key and `floating.hover_hotkeys` is on.
//!
//! Every caller (`card_hover`, `stack_focus`, `stack_resize`, the key handler)
//! runs on the main thread, where the global-shortcut plugin registers keys.

use std::sync::Mutex;

use ferry_core::edit::EditsPatch;
use ferry_core::job::JobId;
use tauri::{AppHandle, Manager};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, ShortcutState};
use tracing::{debug, warn};

use crate::state::AppState;
use crate::{jobs, menu, platform};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Key {
    Preview,
    Compare,
    StepDown,
    /// Downscale to this percent.
    Scale(u8),
}

const KEYS: [(&str, Key); 12] = [
    ("Space", Key::Preview),
    ("CmdOrCtrl+D", Key::Compare),
    ("-", Key::StepDown),
    ("1", Key::Scale(10)),
    ("2", Key::Scale(20)),
    ("3", Key::Scale(30)),
    ("4", Key::Scale(40)),
    ("5", Key::Scale(50)),
    ("6", Key::Scale(60)),
    ("7", Key::Scale(70)),
    ("8", Key::Scale(80)),
    ("9", Key::Scale(90)),
];

struct Keys {
    /// The idle card under the pointer, as the page reports it.
    card: Option<JobId>,
    /// The stack has the keyboard (a text field is open).
    keyed: bool,
    /// The accelerators this module registered, to unregister exactly those.
    held: Vec<&'static str>,
    /// A registration failure was logged once already (spec §6).
    warned: bool,
}

static STATE: Mutex<Keys> = Mutex::new(Keys {
    card: None,
    keyed: false,
    held: Vec::new(),
    warned: false,
});

/// Whether to register (`Some(true)`) or unregister (`Some(false)`) now, or
/// leave the keys as they are (`None`).
fn plan(held: bool, card: bool, enabled: bool, keyed: bool) -> Option<bool> {
    let want = card && enabled && !keyed;
    (want != held).then_some(want)
}

/// The scale `-` steps to: 10 points below the current one (none is 100 %),
/// never under 10 %; `None` when it is already 10 %.
fn step_down(scale: Option<u8>) -> Option<u8> {
    let next = scale.unwrap_or(100).saturating_sub(10).max(10);
    (Some(next) != scale).then_some(next)
}

/// The page's hovered idle card, or `None` when the pointer left it.
pub fn set(app: &AppHandle, card: Option<JobId>) {
    STATE.lock().unwrap().card = card;
    apply(app);
}

/// The stack took or gave back the keyboard: typing must reach the field.
pub fn set_keyed(app: &AppHandle, keyed: bool) {
    STATE.lock().unwrap().keyed = keyed;
    apply(app);
}

fn apply(app: &AppHandle) {
    let enabled = app
        .state::<AppState>()
        .settings
        .read()
        .unwrap()
        .floating
        .hover_hotkeys;
    // Not held across (un)registering: the key handler takes the same lock.
    let (change, held) = {
        let state = STATE.lock().unwrap();
        let change = plan(
            !state.held.is_empty(),
            state.card.is_some(),
            enabled,
            state.keyed,
        );
        (change, state.held.clone())
    };
    let shortcuts = app.global_shortcut();
    match change {
        Some(true) => {
            let mut now = Vec::new();
            let mut failed = Vec::new();
            for (accelerator, key) in KEYS {
                let result = shortcuts.on_shortcut(accelerator, move |app, _, event| {
                    if event.state == ShortcutState::Pressed {
                        pressed(app, key);
                    }
                });
                match result {
                    Ok(()) => now.push(accelerator),
                    Err(e) => failed.push(format!("{accelerator}: {e}")),
                }
            }
            let mut state = STATE.lock().unwrap();
            if !failed.is_empty() && !state.warned {
                state.warned = true;
                warn!(?failed, "hover hotkeys not registered");
            }
            state.held = now;
        }
        Some(false) => {
            for accelerator in held {
                if let Err(e) = shortcuts.unregister(accelerator) {
                    debug!(accelerator, "hover hotkey not unregistered: {e}");
                }
            }
            STATE.lock().unwrap().held.clear();
        }
        None => {}
    }
}

fn pressed(app: &AppHandle, key: Key) {
    let Some(id) = STATE.lock().unwrap().card.clone() else {
        return;
    };
    let scale = match key {
        Key::Preview | Key::Compare => {
            let action = if key == Key::Preview {
                "preview"
            } else {
                "compare"
            };
            menu::send(app, id, action);
            return;
        }
        // Ferry has the keyboard (the compare window or Quick Look): digits
        // and `-` are not ours to take (spec §1).
        _ if platform::app_is_active() => return,
        Key::Scale(percent) => Some(percent),
        Key::StepDown => {
            let current = app
                .state::<AppState>()
                .jobs
                .lock()
                .unwrap()
                .get(&id)
                .map(|entry| entry.last.lock().unwrap().edits.scale);
            current.and_then(step_down)
        }
    };
    let Some(scale) = scale else {
        return;
    };
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let patch = EditsPatch {
            scale: Some(Some(scale)),
            ..EditsPatch::default()
        };
        if let Err(e) = jobs::edit(&app, &id, patch) {
            debug!("hover downscale: {e}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use tauri_plugin_global_shortcut::Shortcut;

    #[test]
    fn keys_are_held_only_over_a_card_with_the_setting_on() {
        assert_eq!(plan(false, true, true, false), Some(true));
        assert_eq!(plan(true, false, true, false), Some(false));
        assert_eq!(plan(true, true, false, false), Some(false));
        assert_eq!(plan(false, true, false, false), None);
        assert_eq!(plan(false, false, true, false), None);
    }

    #[test]
    fn moving_between_cards_keeps_the_keys() {
        assert_eq!(plan(true, true, true, false), None);
    }

    #[test]
    fn a_key_window_never_keeps_the_hover_keys() {
        assert_eq!(plan(true, true, true, true), Some(false));
        assert_eq!(plan(false, true, true, true), None);
    }

    #[test]
    fn minus_steps_ten_points_down_to_ten() {
        assert_eq!(step_down(None), Some(90));
        assert_eq!(step_down(Some(55)), Some(45));
        assert_eq!(step_down(Some(15)), Some(10));
        assert_eq!(step_down(Some(10)), None);
    }

    #[test]
    fn every_accelerator_parses() {
        for (accelerator, _) in KEYS {
            assert!(accelerator.parse::<Shortcut>().is_ok(), "{accelerator}");
        }
    }
}
```

Run: `cargo test -p ferry hover_keys`
Expected: PASS — 5 tests (`keys_are_held_only_over_a_card_with_the_setting_on`, `moving_between_cards_keeps_the_keys`, `a_key_window_never_keeps_the_hover_keys`, `minus_steps_ten_points_down_to_ten`, `every_accelerator_parses`). If `every_accelerator_parses` fails for `-`, use `"Minus"` and for the digits `"Digit1"` … `"Digit9"` (global-hotkey 0.8 accepts both spellings) and re-run.

(Clippy warns that `set` and `set_keyed` are unused until Step 5; run it after Step 5.)

- [ ] **Step 5: Wire it in**

5a. In `src-tauri/src/commands.rs`, change the crate import to `use crate::{hover_keys, jobs, logging, menu, platform, prefs};` and right above the `#[tauri::command]` line of `pub fn card_reveal` add:

```rust
/// The idle card under the pointer, or none: the hover hotkeys follow it.
/// Sync, so it runs on the main thread, where shortcuts are registered.
#[tauri::command]
pub fn card_hover(app: AppHandle, id: Option<JobId>) {
    hover_keys::set(&app, id);
}

```

and append `commands::card_hover,` at the end of `generate_handler!` in `src-tauri/src/lib.rs` (after `commands::card_menu,`).

5b. In `src-tauri/src/windows.rs`, change `use crate::platform;` to `use crate::{hover_keys, platform};`. In `stack_resize`, make the zero-size branch:

```rust
    if width < 1.0 || height < 1.0 {
        // No pointer-leave arrives from a hidden window: drop the hover keys here.
        hover_keys::set(&app, None);
        return platform::hide(&stack).map_err(|e| e.to_string());
    }
```

and in `stack_focus` (Task 4), make the first line of the body:

```rust
    // Typing into the field must never trigger a hover key.
    hover_keys::set_keyed(&app, on);
```

5c. In `src/stack/Stack.svelte`, add `hoverTarget` to the `../lib/cards` import, and right above `  function setTool(id: string, kind: Tool | null) {` add:

```ts
  // Hover hotkeys follow the idle card under the pointer (Rust registers them).
  let hoverSent: string | null = null;
  $effect(() => {
    const id = hoverTarget(hovered, cards, tool, config.hover_hotkeys);
    if (id === hoverSent) return;
    hoverSent = id;
    invoke('card_hover', { id }).catch((e) => console.warn('card_hover', e));
  });

```

The effect re-runs when the pointer moves to another card, the card list changes (closed, auto-hidden, cleared, now busy), a tool opens or the setting changes, so each of those sends the new target or `null`. Rust's `stack_focus(true)` and `stack_resize(0, 0)` drop the keys without waiting for the page.

- [ ] **Step 6: Run everything**

Run: `cargo test -p ferry 2>&1 | grep "test result"`
Expected: `test result: ok.` with 34 passed on the app crate's unit tests.

Run: `pnpm test`
Expected: PASS — 72 tests.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add src-tauri/src/hover_keys.rs src-tauri/src/platform/macos.rs src-tauri/src/platform/windows.rs src-tauri/src/platform/mod.rs src-tauri/src/windows.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/cards.ts src/lib/cards.test.ts src/stack/Stack.svelte
git commit -m "feat(app): hover hotkeys over an idle card

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clean.

---

### Task 10: Settings: File handling, hover hotkeys, crop sizes

**Files:**
- Modify: `crates/ferry-core/src/output.rs` (`example`, tests)
- Modify: `src-tauri/src/commands.rs`, `src-tauri/src/lib.rs` (`output_preview`, `pick_folder`)
- Modify: `src/lib/edits.ts`, `src/lib/edits.test.ts` (`parseSize`, `sizeLabel`, `addSize`)
- Modify: `src/lib/schema.ts`, `src/lib/schema.test.ts` (pane `files`, three rows, two control kinds)
- Create: `src/settings/controls/OutputRule.svelte`, `src/settings/controls/SizeList.svelte`
- Modify: `src/settings/Row.svelte` (the two controls)
- Modify: `src/lib/i18n/en.json` (after `"card.no_original"`)

**Interfaces:**
- Consumes: M3a's `ferry_core::output::{resolve, Target, OutputKind}`, `settings::{OutputRule, Location, contract_home}`, `template::validate`; Task 3's TS `OutputRule`, `OutputLocation`, `Settings.images.output`, `images.crop_sizes`, `floating.hover_hotkeys`, `settings.fixture.ts` at version 2; Task 7's `MAX_SIDE` and `crop_sizes_reset`; the existing settings store (`settings:changed` listener), `Choice.svelte`, keys `settings.unknown_token`, `settings.template_example`, `settings.remove`, `settings.failed`, `crop.restore_defaults`.
- Produces: `pub fn output::example(kind: OutputKind, rule: &OutputRule, home: &Path, work_dir: &Path, today: NaiveDate) -> Result<String, Vec<String>>`; commands `output_preview({ rule, converted }) -> Result<String, string[]>` and `pick_folder() -> Result<Option<String>, String>`; TS `parseSize(text): [number, number] | null`, `sizeLabel(size): string`, `addSize(sizes, size)`; `PaneId` gains `'files'`; `Control` gains `{ kind: 'output'; converted: boolean }` and `{ kind: 'sizes' }`.

Spec §5.6: a **File handling** pane (Images row: Optimised and Converted, each a location picker, a folder picker when Specific, a template field with token chips and a live example path), a **Hover hotkeys** toggle under Floating results, and the crop sizes list under Images. The Settings page is schema-driven (`schema.ts` → `Row.svelte`), so each item is a schema row plus, where needed, a control. Rust renders the example (one output implementation), as `template_preview` already does for the clipboard template.

- [ ] **Step 1: The example path (test first)**

At the end of `mod tests` in `crates/ferry-core/src/output.rs` (before its closing `}`), add:

```rust
    fn rule(location: Location, template: &str) -> OutputRule {
        OutputRule {
            location,
            template: template.into(),
            ..OutputRule::default()
        }
    }

    fn example_of(kind: OutputKind, rule: &OutputRule) -> Result<String, Vec<String>> {
        example(kind, rule, Path::new("/h"), Path::new("/w"), today())
    }

    #[test]
    fn the_example_shows_where_a_desktop_screenshot_goes() {
        let in_place = rule(Location::InPlace, "%f.%e");
        assert_eq!(
            example_of(OutputKind::Optimised, &in_place).unwrap(),
            "~/Desktop/shot.png"
        );
        assert_eq!(
            example_of(OutputKind::Converted, &in_place).unwrap(),
            "~/Desktop/shot.webp"
        );
        assert_eq!(
            example_of(
                OutputKind::Optimised,
                &rule(Location::SameFolder, "%f_optimised.%e")
            )
            .unwrap(),
            "~/Desktop/shot_optimised.png"
        );
        let folder = OutputRule {
            folder: "~/Pictures/Ferry".into(),
            ..rule(Location::Folder, "%f_%z.%e")
        };
        assert_eq!(
            example_of(OutputKind::Converted, &folder).unwrap(),
            "~/Pictures/Ferry/shot_1920x1080.webp"
        );
    }

    #[test]
    fn the_example_names_unknown_tokens_only_where_the_template_is_used() {
        assert_eq!(
            example_of(
                OutputKind::Optimised,
                &rule(Location::SameFolder, "%f%q.%e")
            ),
            Err(vec!["%q".to_string()])
        );
        assert!(example_of(OutputKind::Optimised, &rule(Location::InPlace, "%q")).is_ok());
    }
```

Run: `cargo test -p ferry-core the_example`
Expected: FAIL to compile — `example` not found, `OutputRule` not imported.

Then change `use crate::settings::{self, Location, Output};` to `use crate::settings::{self, Location, Output, OutputRule};` and right above the doc comment of `pub fn unique` add:

```rust
/// The File handling pane's live example: where `~/Desktop/shot.png`
/// (1920×1080) goes under `rule`, as a WebP when `kind` is Converted, with
/// `~` for the home folder. `Err` lists the template's unknown tokens, when
/// the location uses the template.
pub fn example(
    kind: OutputKind,
    rule: &OutputRule,
    home: &Path,
    work_dir: &Path,
    today: NaiveDate,
) -> Result<String, Vec<String>> {
    if matches!(rule.location, Location::SameFolder | Location::Folder) {
        template::validate(&rule.template)?;
    }
    let rules = Output {
        optimised: rule.clone(),
        converted: rule.clone(),
        ..Output::default()
    };
    let source = home.join("Desktop").join("shot.png");
    let target = Target {
        source: &source,
        job: "example",
        ext: match kind {
            OutputKind::Optimised => "png",
            OutputKind::Converted => "webp",
        },
        dims: (1920, 1080),
        kind,
    };
    let path = resolve(&target, &rules, work_dir, home, today);
    Ok(settings::contract_home(&path, home))
}
```

Run: `cargo test -p ferry-core the_example`
Expected: PASS — 2 tests (`the_example_shows_where_a_desktop_screenshot_goes`, `the_example_names_unknown_tokens_only_where_the_template_is_used`).

- [ ] **Step 2: The commands**

In `src-tauri/src/commands.rs`, replace the settings import with

```rust
use ferry_core::output::{self, OutputKind};
use ferry_core::settings::{CardSize, Corner, Images, OutputRule, Settings, TrayStyle, contract_home};
```

and right above the `#[tauri::command]` line of `pub fn open_backups` add:

```rust
/// The File handling pane's live example path for an output rule, or the
/// template's unknown tokens.
#[tauri::command]
pub fn output_preview(
    app: AppHandle,
    rule: OutputRule,
    converted: bool,
) -> Result<String, Vec<String>> {
    let kind = if converted {
        OutputKind::Converted
    } else {
        OutputKind::Optimised
    };
    let home = app.path().home_dir().unwrap_or_default();
    let work_dir = app.state::<AppState>().work_dir.clone();
    let today = chrono::Local::now().date_naive();
    output::example(kind, &rule, &home, &work_dir, today)
}

/// Let the user pick the Specific folder; answered with `~` for the home
/// folder, as settings store it. `None` when the dialog is cancelled.
#[tauri::command]
pub async fn pick_folder(app: AppHandle) -> Result<Option<String>, String> {
    blocking(app, |app| {
        use tauri_plugin_dialog::DialogExt;
        let Some(path) = app
            .dialog()
            .file()
            .set_title(t("output.pick_folder"))
            .blocking_pick_folder()
        else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        let home = app.path().home_dir().map_err(|e| e.to_string())?;
        Ok(Some(contract_home(&path, &home)))
    })
    .await
}
```

(`chrono` with `clock` is already a dependency of the app crate.) Append `commands::output_preview,` and `commands::pick_folder,` at the end of `generate_handler!` in `src-tauri/src/lib.rs` (after `commands::card_hover,`).

Run: `cargo clippy --workspace --all-targets -- -D warnings`
Expected: clean.

- [ ] **Step 3: Size helpers and the schema test (test first)**

In `src/lib/edits.test.ts`, add `addSize,`, `parseSize,` and `sizeLabel,` to the `./edits` import (after `QUALITY_STOPS,`) and append:

```ts
describe('saved crop sizes', () => {
  it('reads width×height with x, × or spaces', () => {
    expect(parseSize('1200×628')).toEqual([1200, 628]);
    expect(parseSize(' 1080 x 1080 ')).toEqual([1080, 1080]);
    expect(parseSize('640X480')).toEqual([640, 480]);
  });

  it('refuses zero, huge sides and words', () => {
    expect(parseSize('0×100')).toBeNull();
    expect(parseSize('30001×10')).toBeNull();
    expect(parseSize('wide')).toBeNull();
    expect(parseSize('1200')).toBeNull();
  });

  it('puts a new size first and never twice', () => {
    const sizes: [number, number][] = [
      [1200, 628],
      [1080, 1080],
    ];
    expect(addSize(sizes, [640, 480])).toEqual([[640, 480], [1200, 628], [1080, 1080]]);
    expect(addSize(sizes, [1080, 1080])).toEqual([[1080, 1080], [1200, 628]]);
    expect(sizeLabel([1200, 628])).toBe('1200×628');
  });
});
```

In `src/lib/schema.test.ts`, inside `describe('the list of settings', …)`, after the last `it(…)`, add:

```ts
  it('has the File handling pane with both output rules, the hover toggle and crop sizes', () => {
    const at = (id: string) => all.find((s) => s.id === id);
    expect(all.filter((s) => s.pane === 'files').map((s) => s.path)).toEqual([
      'images.output.optimised',
      'images.output.converted',
    ]);
    expect(at('hover_hotkeys')?.path).toBe('floating.hover_hotkeys');
    expect(at('crop_sizes')?.control.kind).toBe('sizes');
  });
```

Run: `pnpm test`
Expected: FAIL — `parseSize` is not exported, and no row has pane `files`.

- [ ] **Step 4: The helpers, the schema rows and the strings**

4a. Append to `src/lib/edits.ts`:

```ts

/** A saved crop size as typed in Settings: "1200×628", "1200x628" or "1200 x 628"; each side 1..MAX_SIDE. */
export function parseSize(text: string): [number, number] | null {
  const match = /^\s*(\d+)\s*[x×X]\s*(\d+)\s*$/.exec(text);
  if (!match) return null;
  const [w, h] = [Number(match[1]), Number(match[2])];
  return w >= 1 && h >= 1 && w <= MAX_SIDE && h <= MAX_SIDE ? [w, h] : null;
}

/** "1200×628". */
export function sizeLabel([w, h]: [number, number]): string {
  return `${w}×${h}`;
}

/** The saved sizes with `size` first (newest first), without a duplicate. */
export function addSize(sizes: [number, number][], size: [number, number]): [number, number][] {
  return [size, ...sizes.filter(([w, h]) => w !== size[0] || h !== size[1])];
}
```

4b. In `src/lib/schema.ts`:
- `export type PaneId = 'general' | 'clipboard' | 'images' | 'files' | 'floating';`
- in `panes`, after `  { id: 'images', label: 'pane.images' },` add `  { id: 'files', label: 'pane.files' },`
- in `Control`, after `  | { kind: 'tray_style' }` add

```ts
  | { kind: 'output'; converted: boolean }
  | { kind: 'sizes' }
```

- after the `images_max_size` row (the last `images` row) add

```ts
  {
    id: 'crop_sizes',
    pane: 'images',
    label: 'images.crop_sizes',
    help: 'images.crop_sizes.help',
    path: 'images.crop_sizes',
    control: { kind: 'sizes' },
  },

  {
    id: 'output_optimised',
    pane: 'files',
    group: 'group.images',
    label: 'output.optimised',
    help: 'output.optimised.help',
    path: 'images.output.optimised',
    control: { kind: 'output', converted: false },
  },
  {
    id: 'output_converted',
    pane: 'files',
    group: 'group.images',
    label: 'output.converted',
    help: 'output.converted.help',
    path: 'images.output.converted',
    control: { kind: 'output', converted: true },
  },
```

- after the `follow_cursor` row (the last row) add

```ts
  {
    id: 'hover_hotkeys',
    pane: 'floating',
    label: 'floating.hover_hotkeys',
    help: 'floating.hover_hotkeys.help',
    path: 'floating.hover_hotkeys',
    control: { kind: 'switch' },
    depends_on: 'floating.enabled',
  },
```

The group heading "Images" leaves room for the Video and PDF rows of later cycles (spec §2 "Output location").

4c. In `src/lib/i18n/en.json`, right after `  "card.no_original": "Original no longer kept",` insert:

```json
  "pane.files": "File handling",
  "group.images": "Images",
  "output.optimised": "Optimised",
  "output.optimised.help": "Where optimised, downscaled and cropped files go.",
  "output.converted": "Converted",
  "output.converted.help": "Where files converted to another format go.",
  "output.in_place": "In place",
  "output.same_folder": "Same folder",
  "output.folder": "Specific folder",
  "output.temporary": "Temporary",
  "output.choose": "Choose…",
  "output.pick_folder": "Choose a folder for results",
  "output.insert": "Insert {token}",
  "floating.hover_hotkeys": "Hover hotkeys",
  "floating.hover_hotkeys.help": "With the pointer on a card: Space previews, ⌘/Ctrl+D compares, 1–9 and − downscale.",
  "images.crop_sizes": "Crop sizes",
  "images.crop_sizes.help": "The sizes the crop popover offers.",
  "sizes.add": "Add size",
  "sizes.invalid": "Use width×height, each 1 to 30000",
```

Run: `pnpm test`
Expected: PASS — 76 tests.

- [ ] **Step 5: The controls**

Create `src/settings/controls/OutputRule.svelte`:

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { t, type StringKey } from '../../lib/i18n';
  import type { OutputLocation, OutputRule } from '../../lib/settings';
  import Choice from './Choice.svelte';

  /** One column of the File handling pane's Images row (spec §5.6). */
  let {
    value,
    converted,
    onchange,
    labelledby,
    disabled = false,
  }: {
    value: OutputRule;
    /** The Converted rule (its example is a WebP) rather than Optimised. */
    converted: boolean;
    onchange: (value: OutputRule) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  const LOCATIONS: { value: OutputLocation; label: StringKey }[] = [
    { value: 'in_place', label: 'output.in_place' },
    { value: 'same_folder', label: 'output.same_folder' },
    { value: 'folder', label: 'output.folder' },
    { value: 'temporary', label: 'output.temporary' },
  ];
  const TOKENS = ['%f', '%e', '%z', '%y', '%m', '%d', '%P'];

  let draft = $state('');
  let example = $state('');
  let unknown = $state<string[]>([]);
  let failed = $state<string | null>(null);
  const named = $derived(value.location === 'same_folder' || value.location === 'folder');

  $effect(() => {
    draft = value.template;
  });

  // Rust resolves the example, so there is one output implementation.
  $effect(() => {
    const rule = { ...value, template: draft };
    let current = true;
    invoke<string>('output_preview', { rule, converted })
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

  // Spread keeps fields a newer Ferry wrote.
  function set(patch: Partial<OutputRule>) {
    onchange({ ...value, ...patch });
  }

  /** Only a valid template is saved; an invalid one stays in the field with its error. */
  function commit() {
    if (unknown.length === 0 && draft !== value.template) set({ template: draft });
  }

  async function pick() {
    failed = null;
    try {
      const folder = await invoke<string | null>('pick_folder');
      if (folder) set({ folder });
    } catch (e) {
      failed = String(e);
    }
  }
</script>

<div class="rule">
  <Choice
    value={value.location}
    options={LOCATIONS.map((o) => ({ value: o.value, label: t(o.label) }))}
    {disabled}
    {labelledby}
    onchange={(location) => set({ location: location as OutputLocation })}
  />
  {#if value.location === 'folder'}
    <div class="folder">
      <span class="path">{value.folder}</span>
      <button type="button" {disabled} onclick={pick}>{t('output.choose')}</button>
    </div>
  {/if}
  {#if named}
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
    <div class="tokens">
      {#each TOKENS as token (token)}
        <button
          type="button"
          {disabled}
          aria-label={t('output.insert', { token })}
          onclick={() => {
            draft += token;
            commit();
          }}>{token}</button
        >
      {/each}
    </div>
  {/if}
  {#if unknown.length > 0}
    <p class="bad" role="alert">{t('settings.unknown_token', { token: unknown[0] })}</p>
  {:else}
    <p class="example">{t('settings.template_example', { example })}</p>
  {/if}
  {#if failed}
    <p class="bad" role="alert">{t('settings.failed', { why: failed })}</p>
  {/if}
</div>

<style>
  .rule {
    display: flex;
    flex-direction: column;
    gap: 6px;
    width: 300px;
  }
  .folder,
  .tokens {
    display: flex;
    flex-wrap: wrap;
    gap: 4px;
    align-items: center;
  }
  .path {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
    font-size: 12px;
    color: var(--fg-2);
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
  button {
    padding: 2px 8px;
    font: inherit;
    font-size: 12px;
    color: var(--fg);
    background: var(--bg-content);
    border: 1px solid var(--rail-line);
    border-radius: 6px;
    cursor: pointer;
  }
  button:disabled {
    opacity: 0.45;
    cursor: default;
  }
  input:focus-visible,
  button:focus-visible {
    outline: 2px solid var(--accent-text);
    outline-offset: 1px;
  }
  p {
    margin: 0;
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

Create `src/settings/controls/SizeList.svelte` (the layout follows `ListEditor.svelte`):

```svelte
<script lang="ts">
  import { invoke } from '@tauri-apps/api/core';
  import { addSize, parseSize, sizeLabel } from '../../lib/edits';
  import { t } from '../../lib/i18n';

  /** The crop popover's saved sizes (`images.crop_sizes`), newest first. */
  let {
    sizes,
    onchange,
    labelledby,
    disabled = false,
  }: {
    sizes: [number, number][];
    onchange: (sizes: [number, number][]) => void;
    labelledby: string;
    disabled?: boolean;
  } = $props();

  let draft = $state('');
  let invalid = $state(false);

  function add() {
    if (draft.trim() === '') return;
    const size = parseSize(draft);
    invalid = size === null;
    if (size === null) return;
    onchange(addSize(sizes, size));
    draft = '';
  }

  // Rust writes the defaults and announces them with `settings:changed`, which the store follows.
  function reset() {
    invoke('crop_sizes_reset').catch((e) => console.warn('crop_sizes_reset', e));
  }
</script>

<div class="list">
  <ul aria-labelledby={labelledby}>
    {#each sizes as size (sizeLabel(size))}
      <li>
        <span>{sizeLabel(size)}</span>
        <button
          type="button"
          {disabled}
          aria-label={t('settings.remove', { name: sizeLabel(size) })}
          onclick={() => onchange(sizes.filter((s) => s !== size))}>×</button
        >
      </li>
    {/each}
  </ul>
  <div class="add">
    <input
      type="text"
      {disabled}
      bind:value={draft}
      placeholder="1200×628"
      aria-label={t('sizes.add')}
      aria-invalid={invalid}
      oninput={() => (invalid = false)}
      onkeydown={(event) => {
        if (event.key === 'Enter') add();
      }}
      onblur={add}
    />
    <button type="button" {disabled} onclick={reset}>{t('crop.restore_defaults')}</button>
  </div>
  {#if invalid}
    <p class="bad" role="alert">{t('sizes.invalid')}</p>
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
  .add {
    display: flex;
    gap: 6px;
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
  input[aria-invalid='true'] {
    border-color: var(--error);
  }
  .add button {
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
  .bad {
    margin: 0;
    font-size: 11px;
    color: var(--error);
  }
</style>
```

In `src/settings/Row.svelte`:
- add `  import OutputRule from './controls/OutputRule.svelte';` after the `NumberField` import, `  import SizeList from './controls/SizeList.svelte';` before the `Slider` import, and `  import type { OutputRule as Rule } from '../lib/settings';` after the `Setting` import;
- before `    {:else if control.kind === 'tray_style'}` add

```svelte
    {:else if control.kind === 'output'}
      <OutputRule
        value={value as Rule}
        converted={control.converted}
        {disabled}
        labelledby={labelId}
        {onchange}
      />
    {:else if control.kind === 'sizes'}
      <SizeList sizes={value as [number, number][]} {disabled} labelledby={labelId} {onchange} />
```

- [ ] **Step 6: Run everything**

Run: `pnpm test`
Expected: PASS — 76 tests.

Run: `pnpm check`
Expected: `0 ERRORS 0 WARNINGS`.

Run: `pnpm build`
Expected: `✓ built`.

Run: `cargo test --workspace 2>&1 | grep "test result"`
Expected: every line `ok` (core 253 passed and 1 ignored, app 34, sidecar 7, give or take M3a's count).

- [ ] **Step 7: Format, lint, commit**

```bash
cargo fmt --all
cargo clippy --workspace --all-targets -- -D warnings
git add crates/ferry-core/src/output.rs src-tauri/src/commands.rs src-tauri/src/lib.rs src/lib/edits.ts src/lib/edits.test.ts src/lib/schema.ts src/lib/schema.test.ts src/settings/controls/OutputRule.svelte src/settings/controls/SizeList.svelte src/settings/Row.svelte src/lib/i18n/en.json
git commit -m "feat(settings): File handling pane, hover hotkeys toggle, crop sizes

Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>"
```

Expected: clean.

---

### Task 11: Verify on this Mac (manual, ask first)

**Files:** none changed unless a check fails; a fix goes in its own commit with a `Ruling:` line in the ledger.

**Interfaces:**
- Consumes: everything above. The fallback switches `ACTIVATE_FOR_KEYS` (Task 4, `src-tauri/src/platform/macos.rs`) and `PRESS_AND_DRAG` (Task 5, `src/stack/Card.svelte`).
- Produces: a checklist result in the ledger, one line per check (`pass`, `fail: …`, or `fallback on: …`).

The live app watches and writes the real clipboard and moves files to the Trash, so this task starts only after the user says yes. Compare and Preview call M3c's `open_compare` / `quick_preview`, which are not on this branch: until M3c is merged, Space, ⌘D, the Compare button and the menu's Compare/Preview log "command … not found" in the stack's devtools console and do nothing else. That is expected here; §1 criterion 5's Quick Look and compare window belong to M3c.

- [ ] **Step 1: Ask the user**

Ask: "M3b is built and all automated checks pass. May I run `pnpm tauri dev` for the manual checks? The app will watch and change your clipboard, and Finder-copied test files will be converted and trashed. I'll use files in a scratch folder." Wait for an explicit yes. If the answer is no, record `manual checks: not run (user declined)` in the ledger and stop here.

- [ ] **Step 2: Start the app**

```bash
mkdir -p "$HOME/Desktop/ferry-m3b-check"
screencapture -x "$HOME/Desktop/ferry-m3b-check/shot.png"
sips -g pixelWidth -g pixelHeight "$HOME/Desktop/ferry-m3b-check/shot.png"
pnpm tauri dev
```

Expected: the app starts with its tray icon; `sips` prints the screen's pixel size. Checks 3 and 7 use those numbers (the spec's example is 2880×1800).

- [ ] **Step 3: The checklist**

Do each check and write one ledger line for it.

1. **Key-on-demand (spec §9 risk 1; do it first).** Focus TextEdit and type a word. Copy a screenshot (⌘⇧⌃4). Click the card's name chip, type `renamed`, press Enter. Expected: the letters go into the field (not into TextEdit), the card's name becomes `renamed`, and the next keystroke goes to TextEdit again without clicking it. If the letters never reach the field: set `ACTIVATE_FOR_KEYS` to `true`, restart, retry, and record `fallback on: ACTIVATE_FOR_KEYS`. Then open the rename field again and click a Safari window: Safari keeps the keyboard (TextEdit does not jump back to the front) and the field closes.
2. **Rename errors (§1 criterion 6).** In Finder copy `shot.png` from the scratch folder; on its card rename to `shot` while a `shot.png` already exists next to it (make a copy first). Expected: "Name taken" under the field; Esc closes it and the name is unchanged. Rename to `shot-1`: the file on disk is now `shot-1.png` and **Show in Finder** from the ⋯ menu selects it.
3. **Press-and-drag slider (§1 criterion 1, spec §9 risk 5).** Copy a full-screen screenshot. Press on the card's Downscale button and drag down to the 50 % stop, release. Expected: the badge reads "50 % · 1440×900" while dragging; after release the card reads "2880×1800 → 1440×900" (half of your numbers) and pasting into Preview (⌘N) gives that size. A plain click steps 100 → 90. Esc during a drag cancels. If drags never move the slider: set `PRESS_AND_DRAG` to `false` and record `fallback on: PRESS_AND_DRAG`.
4. **Compression.** Click Compression once: badge "q 70" (from the default 80), the size changes. Drag to the bottom: "Extreme".
5. **Convert in place (§1 criterion 2).** Copy `shot.png` in Finder; on its card hover and click **WebP**. Expected: the folder has `shot.webp` and no `shot.png` (it is in the Trash); the card shows "WebP" and "converted". Click **Restore**: `shot.png` is back and `shot.webp` is in the Trash.
6. **Keep both.** ⌥-click **JPEG** on a file card. Expected: a second card for `shot.jpg`; `shot.png` and the first card's result are untouched (Review Focus 3).
7. **Crop, then downscale, then convert (§1 criterion 3).** Crop 16:9 + Smart, Apply; downscale to 50 %; convert to WebP. Expected: each step re-runs from the original (the card's "before" size never changes) and the final dimensions are exactly half of the 16:9 window. The crop popover opens beside the card toward the screen centre, shows the dashed frame on the thumbnail, Enter applies, Esc and a click on another card close it, and the keyboard goes back to the frontmost app after each.
8. **Fit under size (§1 criterion 4).** Copy a ~4 MB JPEG file in Finder, open ⋯ → Fit under size…, click "1 MB". Expected: the result is ≤ 1 MB, or the amber "Couldn't reach 1 MB — try downscaling" note.
9. **Context menu.** Right-click a card, then use its ⋯ button. Expected: the menu opens at the pointer, and under the button; the current format under Convert to ▸ and the current scale under Downscale ▸ are greyed; Copy path puts the full path on the clipboard (paste into TextEdit); Rename…, Crop… and Fit under size… open the card's own field or popover; Close closes the card; Compare and Preview only log (M3c).
10. **Hover hotkeys (spec §9 risk 3).** Hover a finished card and press `5`. Expected: it downscales to 50 %. Press `-`: 40 %. Move the pointer off the stack and press Space and `5` in TextEdit: they type normally. Close the hovered card with its × and, without moving the pointer, type `5` into TextEdit: it types. Turn **Hover hotkeys** off in Settings → Floating results: `5` over a card types into the frontmost app. If the newest log file (an Error card's Details opens the log folder) says "hover hotkeys not registered", record which keys.
11. **Settings → File handling.** Choose Same folder for Optimised: the example reads `~/Desktop/shot_optimised.png`; type `%q` into the template: "Unknown token %q" and nothing is saved; choose Specific folder → Choose…: the picked folder shows with `~`. Settings → Images → Crop sizes: add `640x480` (it appears first and in the crop popover's saved sizes), "Restore defaults" brings back the four defaults.

- [ ] **Step 4: Clean up and report**

```bash
rm -rf "$HOME/Desktop/ferry-m3b-check"
```

Stop the app (Ctrl+C in the terminal). Commit any fix made during the checks (one commit per fix, the usual trailer). Report the ledger lines to the controller.

---

## Rulings made while planning

Each is a choice the spec left open or a place this plan departs from it. Copy them into the ledger as `Ruling:` lines when the task that owns them lands.

1. **Keep-both twin (Task 1).** With the default In place rule, a forked job resolved its output to the source path and then trashed or overwrote the parent's file. The spec (§4.3) only says the twin "shares the same backup path". Ruled: a twin (`Job.twin`, set by `fork`) never writes to or trashes its source path; it writes beside it (`output::unique`). Spec gap, reported.
2. **`card_rename` returns `Result<(), String>` (Task 2)**, the inline message from `RenameError::message()`, not `RenameError` itself (spec §4.5). The page only shows the text; no TS mirror of the enum is needed.
3. **Two extra `JobUpdate` fields (Task 1):** `source_format` (the format bar's GIF chip and "First frame only") and `edit_hint` (why the tools are off, e.g. "Original no longer kept" or the animated-WebP message). Spec §4.3 lists neither; the UI needs both.
4. **The menu's enabled model is tested in Rust, not vitest (Task 8).** Spec §7 lists "menu enabled model per card state" under vitest, but the menu is built in Rust from `Entry.last`, so the model is a pure Rust function (`menu::enabled`) with cargo tests. A TS copy would be dead code.
5. **One `card:action` event instead of `card:open` (Tasks 8, 9).** Spec §4.5 names `card:open` for Rename…, Crop… and Fit…; the same event also carries Compare, Preview and Close from the menu and Space / ⌘D from the hover keys, so it is called `card:action`.
6. **Command names follow M3c:** the page calls `open_compare` / `quick_preview` (M3c's plan) rather than spec §4.5's `card_compare` / `card_preview`. M3b does not define either.
7. **Crop preview (Task 7):** `fitRatio` is mirrored in TS for the live frame; the frame is always centred. Smart framing is decided in Rust after Apply and is not previewed.
8. **Crop validation (Task 7):** both sides 0 is an error only with Free ("Choose a ratio or a size"); with a ratio chip it means "ratio only". "Larger than the original" is a hint, not an error (the engine clamps). The popover does not save a typed size to the saved list; Settings → Images → Crop sizes does.
9. **The slider drag is a tool (Task 4/5):** a press-and-drag makes the stack key, so Esc cancels the drag; it closes on release.
10. **Hover-key suspension (Task 9):** "while the compare window or Quick Look has focus" is implemented as "while Ferry is the active app"; it also covers the Settings window. If M3c's Quick Look falls back to `qlmanage -p` (another process), digits are not suspended then.
11. **Hover `-` and digits run in Rust (Task 9)** through `jobs::edit`; Space and ⌘D go to the page (`card:action`) so the page alone calls M3c's commands.
12. **Fallback switches:** `ACTIVATE_FOR_KEYS` (macOS, Task 4) and `PRESS_AND_DRAG` (Task 5) are constants, off and on respectively; Task 11 flips them only if a check fails.
13. **Context menu while busy:** only Close is enabled; a notice card (not a job) opens no menu.
14. **"Restore defaults" for crop sizes** is a Rust command (`crop_sizes_reset`) so the defaults live only in `ferry_core::settings`.
15. **Focus goes back only if nobody took it (Task 4):** `set_stack_key(false)` re-activates the remembered app (macOS) or window (Windows) only while Ferry or that app is still in front. Task 7's `windowBlur` closes a field when the user clicks another app, and without this check the old app would grab the keyboard back from the one just clicked.

## Self-review

- **Spec coverage (M3b row of §8):** rail A and sliders (Task 5), format bar and convert (6), rename (4), crop and fit popovers (7), key-on-demand (4), context menu (8), hover hotkeys (9), File handling pane, hover toggle and crop sizes (10), commands around `Job::edit` / `fork` / `rename` (2), M3a handoff: `Entry::output` (2) and TS v2 types and fixture (3). §6: "Original no longer kept" tooltips (5, 6), amber note (7), rename errors (4), hotkey failure logged once (9). §7 vitest list: slider ↔ label (3), crop validation (7), format-bar choices (6); menu model in Rust (Ruling 4). §1 criteria 1–4 and 6 by hand (11); criterion 5 belongs to M3c.
- **Not covered here:** Windows behaviour is written but never compiled or run (no Windows target here); Quick Look and the compare window (M3c).
- **Placeholders:** none; every code step carries the code, and every run step the command and the expected result.
- **Names across tasks:** `Tool`, `OpenTool`, `openTool`, `setTool` (4) are used by 7, 8, 9; `menu::send` (8) by 9; `crop_sizes_reset` (7) by 10; `MAX_SIDE` (7) by 10; `card:action` payload `{ id, action }` matches `CardAction` in Rust (`menu.rs`) and TS (`ipc.ts`). Command arguments: `card_edit { id, edits }`, `card_convert { id, format, keepBoth }`, `card_rename { id, stem }`, `card_menu { id, x, y }`, `card_hover { id }`, `stack_focus { on }`, `output_preview { rule, converted }`.
- **Review Focus pins** name the tests as written in Tasks 1, 2, 4, 7 and 9.
- **Test counts** were taken from a full dry run of this plan on a copy of `feat/m3a-edit-engine`: vitest 50 → 52 → 54 → 58 → 69 → 69 → 72 → 76 after Tasks 3–10; app crate 23 after Task 2, 29 after Task 8 (five menu tests and one clipboard test), 34 after Task 9; core 251 → 253 (+1 ignored) after Tasks 1 and 10. Small differences from M3a's own count are fine; a failure is not.

