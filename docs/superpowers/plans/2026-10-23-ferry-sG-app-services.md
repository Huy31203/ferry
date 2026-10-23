# Stream G: app services Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ferry updates itself from signed GitHub Releases, keeps its settings in step across computers through a folder the user already syncs, lets the user rebind and turn off every hotkey from Settings, and lets the user choose which tools a card's rail shows and in which order, through a "+" button on the card and an editor in Settings.

**Architecture:** One settings section `[svc]` (`ferry_core::svc_settings::Svc`) holds everything this stream stores. Pure logic lives in `ferry-core` (the sync engine `settings_sync`) or in unit-tested pure functions in the app crate (`hotkeys`, `updater`), so it is tested on macOS and Windows CI. The updater is `tauri-plugin-updater` driven from Rust only, with no JS plugin package: a state machine in `updater.rs` emits `svc:update` to Settings, the stack pill and the tray. Settings sync is a worker thread that writes one file per device into `<chosen folder>/Ferry Settings/` and merges the other devices' files key by key (last writer wins), so cloud clients never see two computers write the same file. Hotkeys become a table of bindings with one conflict check, re-registered on the main thread whenever their settings change. The rail reads an ordered list of tool ids; ids the build does not know are kept in settings but not drawn.

**Tech Stack:** Rust 2024 (stable), Tauri 2.12.4, `tauri-plugin-updater` 2.14.0 (new), `tauri-plugin-global-shortcut` 2.4.1 / `global-hotkey` 0.8.0, `tauri-plugin-dialog` 2.8.3, `tauri-plugin-opener` 2.7.2, `trash` 5.2, Svelte 5 (runes) + TypeScript 5.9, vitest 5, svelte-check 4, GitHub Actions with `tauri-apps/tauri-action@v1`.

**Spec:** [Cycle 3 stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding; it wins over the requirements), [requirements](../../requirements/ferry-optimize-requirements.md) R§2.9 (updates, optional sync), R§2.10 (hotkeys), R§4.2 tray menu ("Check for updates…"), R§4.3 side action rail ("+" slot), R§4.4 "Update available" pill, R§4.12 panes General (optional sync), Floating results (side-rail editor, update-available pill), Hotkeys, About and updates, R§4.15 update prompt, R§5 "Launch at login, single instance, updates", [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md) §4.3, §5.6, §5.7, §6 (hotkey conflicts), §9, [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) §4.5 (hover hotkeys), §5.1 (rail A).

## Decisions

| Topic | Decision | Why |
| --- | --- | --- |
| Updater plugin | `tauri-plugin-updater = "2.14.0"` (Apache-2.0 OR MIT), Rust API only | Latest 2.x; requires `tauri ^2.12` (we lock 2.12.4) and Rust 1.90 (we run stable). No `@tauri-apps/plugin-updater` npm package and no capability permission are needed, because the page never calls the plugin directly |
| Manifest hosting | `https://github.com/Huy31203/ferry/releases/latest/download/latest.json`, written by `tauri-action` (`uploadUpdaterJson: true`) | No server to run. `latest` skips drafts and pre-releases |
| Updater artifacts | Built only in `release.yml`, through `--config src-tauri/tauri.updater.conf.json` (`bundle.createUpdaterArtifacts: true`) | CI, PR builds and local `pnpm tauri build` need no signing key |
| macOS updater bundle | `--bundles app,dmg` in the release matrix; the universal build yields one `Ferry.app.tar.gz` + `.sig`, which `tauri-action` lists under `darwin-aarch64` and `darwin-x86_64` (and `-app` variants) | Verified in `tauri-action@v1` `src/upload-version-json.ts` |
| Windows updater bundle | NSIS `Ferry_<v>_x64-setup.exe` + `.sig`; `plugins.updater.windows.installMode = "passive"` | Per-user install (C1 §2) never asks for admin; passive shows a progress bar with no clicks |
| Manifest race | Both matrix jobs merge into one `latest.json`; a new `updater-manifest` job checks that all three platforms are present and signed | `tauri-action` re-reads and rewrites `latest.json` per job, so two jobs finishing together could drop one platform |
| Update flow | Check quietly at 60 s after start and every 6 h (release builds, `svc.updates.auto_check`). "Install and restart" downloads with progress, installs at once if no job is busy, otherwise shows "Ready to install" with the busy count | R§4.15 states; a restart never kills a running job without telling the user |
| Update prompt | Lives in Settings ▸ About and updates, not in its own window. The stack pill and the tray item open that pane | R§4.15's elements and states fit the pane; no new window, page or capability |
| Restart | `app.request_restart()` after `install` on macOS. Windows' `install` launches the installer and exits the process itself | Tauri 2.12 `request_restart` exits with `RESTART_EXIT_CODE`, which `prevent_exit` ignores, so the existing `ExitRequested { code: None }` guard does not block it |
| Sync medium | A folder the user picks; Ferry writes into its `Ferry Settings` subfolder (or the folder itself if it is already named that) | R§2.9 "optional sync"; works with iCloud Drive, Dropbox, OneDrive, Google Drive and SMB |
| Sync file layout | One file per device, `<device id>.json`, written only by that device | Two computers never write the same file, so cloud "conflicted copy" files only appear from clock or copy accidents; those are merged and moved to the Trash |
| Sync conflicts | Per key, last writer wins on `(modified ms, device id)` | Two computers changing different settings both keep their change |
| Watching for remote changes | Poll the sync folder every 15 s (file length + mtime; only changed files are re-read) plus "Sync now" | FSEvents/ReadDirectoryChangesW miss many cloud-provider and SMB updates; polling ≤ 20 small files costs nothing and adds no dependency (stream C owns `notify`) |
| Atomic writes | `fsutil::atomic_replace` (temp `<name>.ferry-tmp` + fsync + rename); readers skip `.ferry-tmp`, dot-files and anything over 1 MiB | A reader never sees half of our own write; a half-synced remote file fails to parse and is skipped until the next poll |
| Hotkey storage | Keep C1's `hotkeys.{optimise,aggressive,cycle_state}`; add `svc.shortcuts.{preview,compare,step_down,disabled}`. Digits `1`–`9` stay fixed, with an on/off toggle | No rename, so no settings version bump; digits are a fixed preset set (C2 §4.5) |
| Hotkey conflicts | Blocked: unparseable, a global key without Ctrl/Alt/⌘, an OS shortcut on the built-in list, Esc/Tab/Enter/Backspace as a hover key, a chord another enabled Ferry hotkey uses. Warned: Windows browser dev-tools chords. Reported after registering: "another app may be using it" | R§2.10 "conflict warning"; C1 §6 keeps the app running when registration fails |
| Recording | While a recorder field is active, Ferry unregisters its own global and hover keys (`svc_hotkeys_suspend`) | Otherwise pressing the current chord would trigger it instead of reaching the field |
| Rail order | The rail keeps its existing `{#if}` blocks; a Svelte `$effect` sets each button's CSS `order` from the user's list | Avoids moving Card.svelte's existing lines (brief merge rule). Trade-off: Tab order follows the built-in order, not the visual one (open question 5) |
| Rail extension | `src/lib/rail.ts` `RAIL_TOOLS` catalog. Another stream adds a tool by adding one catalog entry and one button block in `Card.svelte` | Unknown ids (from a newer Ferry through sync, or a stream not merged yet) are kept and shown in the editor as "Not in this version" |
| Tray "update available" dot | Not drawn this stream. The tray menu item reads "Update to Ferry 0.3.1…" and the stack shows the pill | The tray dot already carries four states (C1 §5.6); see open question 6 |
| Import/export settings | Not in this plan | Not in the brief's stream G scope |

**Unsigned macOS updates (signing and notarisation are out of scope):** the plugin downloads with its own HTTP client, so the new bundle carries no `com.apple.quarantine` attribute and Gatekeeper does not show the first-launch prompt again after an update. The bundle is replaced in place; if the user cannot write to `/Applications`, macOS asks for an administrator password. Privacy grants that macOS ties to the code signature (Accessibility, Input Monitoring, Files and Folders, Automation) may be asked for again after each update, because an ad-hoc signature changes with every build. The login item keeps working because it points at the app's path. On Windows the installer downloaded by Ferry has no Mark-of-the-Web, so SmartScreen does not prompt. Task 15's manual check confirms the macOS behaviour.

**First updatable version:** the first release built from this branch is the first one that can update itself. Users on 0.2.0 must download that release by hand once.

## Global Constraints

- **Out of scope:** secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP automation, settings import/export, pipeline buttons on the rail (stream F), the tray's update dot.
- **New dependency (the only one):** `tauri-plugin-updater = "2.14.0"` in `src-tauri/Cargo.toml` (Apache-2.0 OR MIT). It adds these crates to `Cargo.lock` (resolved on 2026-10-23; let `cargo` write the lock, never edit it by hand): `minisign-verify 0.3.0` (MIT), `ring 0.17.14` (Apache-2.0 AND ISC), `rustls 0.23.46` and `hyper-rustls 0.27.10` (Apache-2.0 OR ISC OR MIT), `rustls-webpki 0.103.16` and `untrusted 0.9.0` (ISC), `rustls-pki-types 1.15.1`, `rustls-platform-verifier 0.7.1`, `tokio-rustls 0.26.6`, `tar 0.4.46`, `osakit 0.3.1`, `xattr 1.6.1`, `filetime 0.2.29`, `security-framework 3.7.0`, `system-configuration 0.7.0` (all MIT OR Apache-2.0), `objc2-osa-kit 0.3.2` (Zlib OR Apache-2.0 OR MIT), `subtle 2.6.1` (BSD-3-Clause), `zeroize 1.9.1` (Apache-2.0 OR MIT), `zip 4.6.1` (MIT, Windows only); locked but not built for macOS or Windows: `webpki-root-certs 1.0.11` (CDLA-Permissive-2.0), `rustls-native-certs`, `openssl-probe`, `schannel`, `jni`, `simdutf8`, `simd_cesu8`, `arbitrary`. **None is GPL, LGPL or AGPL, and none is patent-encumbered.** After adding the dependency, run the licence check in Task 10 Step 1 and stop if anything new is GPL/LGPL/AGPL.
- **No new npm package and no new binary.** `package.json` and `pnpm-lock.yaml` must not change.
- **Settings:** one new top-level section `svc` (`struct Svc` and its sub-structs, every one `#[serde(default)]` with a `#[serde(flatten)] extra: Map<String, Value>` so unknown fields survive). `settings::VERSION` stays `2`. Every new key is listed in "New settings keys" below.
- **i18n:** every new string goes under `svc.*` in `src/lib/i18n/en.json` (a flat map Rust also reads). The keys are in "Strings" below, with their exact English copy. They go in one block inserted directly after the line `"app.name": "Ferry",`; the first task that adds a key creates the block, later tasks append to its end. Existing keys are reused where the copy is identical (`tray.optimise`, `tray.aggressive`).
- **Shared files:** additions only at the anchors in "Shared-file edits" below, each marked `svc` in a comment where the language allows one. Never reorder or reformat existing lines.
- **Main thread:** global shortcuts are registered and unregistered on the main thread only. Sync commands (`#[tauri::command]` on a plain `fn`) run there; anything else reaches the main thread with `app.run_on_main_thread`. File and network work never runs on the main thread: settings saves from commands use `#[tauri::command(async)]`, sync IO runs on its worker thread, updater network calls on the async runtime.
- **Network:** the only new network use is the updater's request to GitHub Releases. No telemetry, no other host.
- **Debug builds** never check for updates on their own (manual "Check now" still works), as debug builds never touch the login item.
- **Paths in settings** are stored with `~` (`settings::contract_home`) and expanded with `settings::expand_home`.
- **Never hard-delete a user-visible file:** sync conflict copies go to the Trash with the `trash` crate.
- **Windows-only code:** exactly one `#[cfg(windows)]` branch, `sync_worker::device_name()` reading `COMPUTERNAME` (Task 13). It **cannot be compiled on this Mac**; CI's `bundle (windows)` job checks it with `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test -p ferry`. Say so in that commit's message. Windows hotkey defaults and the Windows reserved-shortcut list are plain data chosen at run time with a `mac: bool` argument, compiled and unit-tested on the Mac. The updater's `on_before_exit` hook and `windows.installMode` only take effect on Windows inside the plugin.
- **JavaScript/TypeScript:** never use `var`. Svelte 5 runes (`$state`, `$derived`, `$effect`, `$props`) as in the existing components.
- **Accessibility:** every new control has an accessible name and is reachable by keyboard; drag-to-reorder always has Move up / Move down buttons beside it.
- **Error text:** one line that says what to do (C1 §6).
- **Live app:** ask the user before running `pnpm tauri dev` or a built app; it takes over the real clipboard. Use a TextEdit window as the focus target. Every "manual check" step is run by, or with the permission of, the user. A manual check that cannot run yet goes into the commit body for the integrator.
- **Git:** never push, push tags, merge, or delete branches or worktrees without asking. End each commit message with the trailer your session's instructions give (the handover rule is `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>`).
- **Environment**, run from the worktree root before any command:
  ```bash
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  ```
  A new worktree needs the git-ignored `src-tauri/binaries/` copied from the main checkout (or `pnpm sidecar`). The app crate's tests need a built frontend: run `pnpm build` once before `cargo test -p ferry` or `cargo clippy --workspace`. `cargo test` takes one name filter per command.
- **Every task ends with:** `cargo fmt --all`, `cargo clippy --workspace --all-targets -- -D warnings`, the task's tests, `pnpm check`, `pnpm test`, then a commit. Every new Rust module opens with a `//!` doc comment; tests go in `#[cfg(test)] mod tests` at the end of the file.

### Setup (once, before Task 1)

```bash
cd /Users/gos/code/rust/ferry
git branch --list feat/c3-all
```
If `feat/c3-all` exists: `git worktree add .worktrees/sG -b feat/c3-sG-app-services feat/c3-all`. If it does not, ask the user whether to cut from `main` instead. Then:
```bash
cd .worktrees/sG
cp -R ../../src-tauri/binaries src-tauri/ 2>/dev/null || pnpm sidecar
pnpm install --frozen-lockfile && pnpm build
cargo test --workspace 2>&1 | grep "test result"
```
Expected: every `test result` line says `ok`. Keep a ledger at `.superpowers-ledger-sG.md` in the worktree (uncommitted): one line per task, plus a `Ruling:` line for every deviation from this plan.

### Shared-file edits (all of them)

| File | Anchor | Edit | Task |
| --- | --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | after `pub mod settings;` / after `pub mod sniff;` | `pub mod settings_sync;` / `pub mod svc_settings;` | 2 / 1 |
| `crates/ferry-core/src/settings.rs` | after `pub general: General,` in `struct Settings`; after `general: General::default(),` in `impl Default for Settings`; immediately before `self.version = self.version.max(VERSION);` in `sanitised` | `pub svc: Svc,` / `svc: Svc::default(),` / `self.svc = self.svc.sanitised();` | 1 |
| `src-tauri/Cargo.toml` | after `tauri-plugin-single-instance = "2.5"` | `tauri-plugin-updater = "2.14.0"` | 10 |
| `Cargo.lock` | written by cargo | — | 10 |
| `src-tauri/tauri.conf.json` | new top-level key `"plugins"` after the `"bundle"` object | `plugins.updater` (Task 10) | 10 |
| `src-tauri/src/lib.rs` | after `mod windows;`: a `// svc (stream G)` comment and `mod rail_menu; mod svc_commands; mod sync_worker; mod updater;`. Immediately before `.setup(\|app\| {`: `.plugin(tauri_plugin_updater::Builder::new().build())` and one `.on_window_event(…)`. In `setup`: the two lines `let registered = hotkeys::register(app);` / `app.manage(registered);` become `hotkeys::reregister(app);`; after `housekeeping(app);`: `svc_commands::setup(app);`. In `generate_handler!`: the `svc_*` commands appended after `commands::pick_folder,` | 4, 6, 8, 10, 13 |
| `src-tauri/src/commands.rs` | `struct StackConfig`: new last field `rail: Vec<String>`; `stack_ready`: `rail: settings.svc.rail.tools,` as the last field of the literal | 8 |
| `src-tauri/src/tray.rs` | `use crate::hotkeys::{self, Registered};` → `use crate::hotkeys;`; the `let keys = app.try_state::<Registered>()…unwrap_or_default();` statement → `let keys = hotkeys::current();`; after the `let settings = MenuItem::with_id(…)?;` statement: `let update = crate::updater::tray_item(app)?; // svc`; in `Menu::with_items`, `&update,` between `&settings,` and `&about,`; in `on_menu`, arm `"svc:update" => crate::updater::tray_clicked(app),` before `"quit"` | 4, 10 |
| `src-tauri/src/menu.rs` | in `build`: `Some("CmdOrCtrl+D")` → `crate::hotkeys::hover_accelerator(app, "compare").as_deref()`, `Some("Space")` → `crate::hotkeys::hover_accelerator(app, "preview").as_deref()` | 4 |
| `src-tauri/src/prefs.rs` | `struct Effects`: new fields `hotkeys`, `updates`, `sync` after `tray`; matching lines at the end of `between`'s literal and of `apply`; in `update`, after `let _ = app.emit("settings:changed", &new);`: `crate::sync_worker::local_changed(app);`; tests appended at the end of `mod tests` | 4, 10, 13 |
| `src/lib/i18n/en.json` | one `svc.*` block after `"app.name": "Ferry",` (created in Task 5) | 5, 6, 7, 8, 9, 10, 11, 14 |
| `src/stack/Card.svelte` | see Task 8 (props, `railActions` call, one `$effect`, one `<!-- svc -->` button block after the `copy` block, the "+" button after the `more` block, `tipText` cases before `default:`) | 8 |
| `src/stack/Stack.svelte` | config default, `settings:changed` listener, `railTools` prop on `<Card>`, the pill (Task 11) | 8, 11 |
| `src/lib/types.ts` | **not created.** It does not exist at HEAD; this stream's types go in the new `src/lib/svc.ts` | — |
| `src-tauri/capabilities/*`, `src-tauri/src/state.rs`, `src-tauri/src/windows.rs`, `src-tauri/src/platform/mod.rs`, `src/lib/ipc.ts` other than `StackConfig` | **not touched** | — |

Files not on the brief's list that other streams may also touch, edited at named anchors: `src/lib/ipc.ts` (`StackConfig`: new last field `rail: string[];`), `src/lib/settings.ts` and `src/lib/settings.fixture.ts` (`svc` block immediately before the line `  clipboard: {`), `src/lib/schema.ts` (`PaneId` union and `panes` array: appended entries; `Control` union: appended variants; `Setting`: new optional `wide`; rows: the General sync row right after the `force_clean` row, every other new row right after the `hover_hotkeys` row), `src/lib/cards.ts` (`RailAction` union, `railActions`, `needsOriginal`), `src/settings/Row.svelte` (new branches immediately before `{:else if control.kind === 'action'}`, a `wide` class), `src/settings/Settings.svelte` (pane requests in `onMount`), `src-tauri/src/hotkeys.rs` and `src-tauri/src/hover_keys.rs` (owned by this stream; stream I's Task 10 also edits `hover_keys.rs`'s `plan` and `Keys`, so keep `plan`'s signature unchanged here), `.github/workflows/release.yml`.

### Cross-stream hooks (produced by this stream)

- **Sync scope, for every stream that adds settings:** `ferry_core::settings_sync::{LOCAL, PER_OS}` are prefix lists. A key not on either list syncs to every computer. A stream whose setting holds machine-specific data (a watched folder's absolute path, a device name) adds its path prefix to `LOCAL`; one whose values differ by OS (accelerators, app ids) adds it to `PER_OS`. Streams C (watched folders) and H (per-type "edit with" apps) will need this; the integrator adds their prefixes.
- **Rail tools, for streams A, B, E, F, H:** a new card tool is one entry in `RAIL_TOOLS` (`src/lib/rail.ts`) plus one `<button data-action="<id>">` inside the `<!-- svc: rail tools -->` block of `Card.svelte`. Pipeline buttons should use ids `pipeline:<id>`; `svc_settings::is_tool_id` already accepts them.
- **Hotkeys, for any stream adding one:** add a `(id, Group)` row to `hotkeys::BINDINGS`, its default to `default_accelerator`, and its storage to `accelerator`/`set_accelerator`. The editor, conflict check, tray labels and sync pick it up.
- **Settings pane navigation:** `svc_commands::open_settings_at(app, pane, row)` opens Settings at a pane and highlights a row; any stream may call it.

### New settings keys

| Key | Type | Default | Sync scope |
| --- | --- | --- | --- |
| `svc.updates.auto_check` | bool | `true` | shared |
| `svc.updates.pill` | bool | `true` | shared |
| `svc.updates.skipped_version` | string | `""` | local |
| `svc.sync.enabled` | bool | `false` | local |
| `svc.sync.folder` | string (`~` form) | `""` | local |
| `svc.rail.tools` | string[] | `["downscale","compression","crop","restore","compare","copy"]` | shared |
| `svc.shortcuts.preview` | accelerator | `"Space"` | per OS |
| `svc.shortcuts.compare` | accelerator | `"CmdOrCtrl+D"` | per OS |
| `svc.shortcuts.step_down` | accelerator | `"-"` | per OS |
| `svc.shortcuts.disabled` | string[] (hotkey ids) | `[]` | per OS |

Existing keys whose sync scope this stream defines: `version`, `general.launch_at_login`, `general.show_dock_icon` → local; `hotkeys.*`, `clipboard.ignored_apps` → per OS; everything else → shared.

Hotkey ids (stable strings, used in `svc.shortcuts.disabled`, commands and the UI): `optimise`, `aggressive`, `cycle_state` (global); `preview`, `compare`, `step_down`, `scale` (hover; `scale` is the fixed `1`–`9` set).

### Strings (all new, exact copy)

| Key | English |
| --- | --- |
| `svc.pane.hotkeys` | Hotkeys |
| `svc.pane.about` | About and updates |
| `svc.group.global_hotkeys` | Global hotkeys |
| `svc.group.hover_hotkeys` | Hover hotkeys |
| `svc.group.sync` | Sync |
| `svc.hotkey.cycle_state` | Cycle Running, Paused and Stopped |
| `svc.hotkey.preview` | Quick preview |
| `svc.hotkey.compare` | Compare with original |
| `svc.hotkey.step_down` | Downscale one step |
| `svc.hotkey.scale` | Downscale to 10–90 % |
| `svc.hotkey.scale_keys` | 1 – 9 |
| `svc.hotkey.global.help` | Works in any app. |
| `svc.hotkey.hover.help` | Works while the pointer rests on a card. |
| `svc.hotkey.record` | Change the shortcut for {name} |
| `svc.hotkey.recording` | Type a shortcut… |
| `svc.hotkey.reset` | Restore default |
| `svc.hotkey.enable` | Use {name} |
| `svc.hotkey.none` | None |
| `svc.hotkey.invalid` | Ferry can't use this key. |
| `svc.hotkey.needs_modifier.mac` | Add ⌃, ⌥ or ⌘: a global hotkey needs one. |
| `svc.hotkey.needs_modifier.windows` | Add Ctrl or Alt: a global hotkey needs one. |
| `svc.hotkey.reserved.mac` | macOS uses this shortcut. |
| `svc.hotkey.reserved.windows` | Windows uses this shortcut. |
| `svc.hotkey.card_key` | Cards use this key. |
| `svc.hotkey.duplicate` | Already used by “{name}”. |
| `svc.hotkey.devtools` | Browsers use this to open developer tools. |
| `svc.hotkey.failed` | Couldn't register this shortcut. Another app may be using it. |
| `svc.rail.add` | Add a tool |
| `svc.rail.customise` | Customise rail… |
| `svc.rail.label` | Card tools |
| `svc.rail.help` | The buttons on a card, in order: up to 6. Compact cards show the first 4 that fit. |
| `svc.rail.add_tool` | Add a tool… |
| `svc.rail.full` | A card holds 6 tools. Remove one to add another. |
| `svc.rail.move_up` | Move {name} up |
| `svc.rail.move_down` | Move {name} down |
| `svc.rail.remove` | Remove {name} |
| `svc.rail.drag` | Drag {name} to reorder |
| `svc.rail.unknown` | Not in this version of Ferry ({id}) |
| `svc.rail.reset` | Restore defaults |
| `svc.rail.empty` | No tools. Cards show only the ⋯ menu. |
| `svc.rail.tool.downscale` | Downscale |
| `svc.rail.tool.compression` | Compression |
| `svc.rail.tool.crop` | Crop |
| `svc.rail.tool.restore` | Restore original |
| `svc.rail.tool.compare` | Compare |
| `svc.rail.tool.copy` | Copy |
| `svc.rail.tool.reveal.mac` | Show in Finder |
| `svc.rail.tool.reveal.windows` | Show in Explorer |
| `svc.rail.tool.aggressive` | Optimise aggressively |
| `svc.rail.tool.rename` | Rename |
| `svc.rail.tool.preview` | Preview |
| `svc.rail.tool.fit` | Fit under size |
| `svc.tray.check` | Check for updates… |
| `svc.tray.update_to` | Update to Ferry {version}… |
| `svc.tray.downloading` | Downloading update… |
| `svc.updates.version` | Version |
| `svc.updates.current` | Ferry {version} |
| `svc.updates.auto` | Check for updates automatically |
| `svc.updates.auto.help` | Ferry asks GitHub for a newer version a minute after it starts and every 6 hours. Nothing about your files is sent. |
| `svc.updates.pill` | Show “Update available” under the cards |
| `svc.updates.pill.help` | A small button below the results when a new version is ready. |
| `svc.updates.check_now` | Check now |
| `svc.updates.idle` | Not checked yet. |
| `svc.updates.checking` | Checking for updates… |
| `svc.updates.up_to_date` | Ferry is up to date. |
| `svc.updates.available` | Ferry {version} is available. |
| `svc.updates.skipped` | You skipped this version. |
| `svc.updates.notes` | Release notes |
| `svc.updates.no_notes` | This release has no notes. |
| `svc.updates.view_release` | View on GitHub |
| `svc.updates.install` | Install and restart |
| `svc.updates.later` | Later |
| `svc.updates.skip` | Skip this version |
| `svc.updates.downloading` | Downloading Ferry {version}… {percent} |
| `svc.updates.downloading_bytes` | Downloading Ferry {version}… {size} |
| `svc.updates.ready` | Ferry {version} is ready to install. |
| `svc.updates.ready_busy` | {count} files are still being optimised. Installing now stops them. |
| `svc.updates.installing` | Installing Ferry {version}… |
| `svc.updates.failed.check` | Couldn't check for updates: {why} |
| `svc.updates.failed.download` | Couldn't download the update: {why} |
| `svc.updates.failed.install` | Couldn't install the update: {why} |
| `svc.updates.retry` | Try again |
| `svc.pill.label` | Update available |
| `svc.pill.aria` | Ferry {version} is available. Open About and updates. |
| `svc.sync.enabled` | Sync settings between computers |
| `svc.sync.enabled.help` | Through a folder you already sync, such as iCloud Drive or Dropbox. Hotkeys and ignored apps sync only between computers of the same kind. |
| `svc.sync.no_folder` | No folder chosen |
| `svc.sync.choose` | Choose folder… |
| `svc.sync.now` | Sync now |
| `svc.sync.status.starting` | Syncing… |
| `svc.sync.status.synced` | Last synced {time} |
| `svc.sync.status.synced_with_one` | Last synced {time} · with 1 other computer |
| `svc.sync.status.synced_with` | Last synced {time} · with {count} other computers |
| `svc.sync.status.missing` | Folder not found. Ferry keeps trying. |
| `svc.sync.status.newer` | {device} uses a newer Ferry. Update Ferry to sync with it. |
| `svc.sync.status.failed` | Couldn't sync: {why} |
| `svc.sync.join.title` | This folder already holds Ferry settings |
| `svc.sync.join.from` | From {devices}, last changed {time}. |
| `svc.sync.join.use_folder` | Use the folder's settings |
| `svc.sync.join.use_this` | Use this computer's settings |
| `svc.sync.join.cancel` | Cancel |

### User actions (the implementer cannot do these)

These need the repository owner's secrets and accounts. Ask the user to do them; record the outcome in the ledger.

1. **Generate the updater signing key** on the Mac, once: `pnpm tauri signer generate -w ~/.tauri/ferry-updater.key`. Choose a strong password when asked. Back up `~/.tauri/ferry-updater.key` and the password somewhere safe: if either is lost, no installed copy of Ferry can ever accept an update again, and every user has to reinstall by hand.
2. **Put the public key in the app:** replace `REPLACE_WITH_UPDATER_PUBLIC_KEY` in `src-tauri/tauri.conf.json` (`plugins.updater.pubkey`) with the whole content of `~/.tauri/ferry-updater.key.pub`, and commit it. The public key is not secret.
3. **Add two GitHub Actions secrets** to `Huy31203/ferry` (Settings ▸ Secrets and variables ▸ Actions ▸ New repository secret, or `gh secret set`): `TAURI_SIGNING_PRIVATE_KEY` = the whole content of `~/.tauri/ferry-updater.key` (`gh secret set TAURI_SIGNING_PRIVATE_KEY < ~/.tauri/ferry-updater.key`), and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD` = the password.
4. **Release as before:** bump `version` in `src-tauri/tauri.conf.json`, `package.json` and the workspace `Cargo.toml`, tag `v<version>`, push the tag. Until steps 2 and 3 are done, `release.yml` stops at its "Check updater signing" step on purpose.
5. **Repository visibility:** the updater fetches `latest.json` and the assets without a token, so the repository's Releases must be public.

## Review Focus

1. **A remote sync file that is half-downloaded, a cloud placeholder, hand-edited into junk, or from a newer Ferry.** Expected: local settings stay exactly as they are; the file is skipped until a later poll reads it whole; a newer format shows "{device} uses a newer Ferry…". It must never be read as "no settings" and never trigger a write that overwrites the other computer's values. Pinned in Task 2 (`a_truncated_or_foreign_file_is_rejected_not_read_as_empty`, `a_newer_format_is_named_and_not_merged`) and Task 13 (`a_half_written_file_keeps_the_last_good_copy`).
2. **Two computers of different kinds sharing one sync folder.** Expected: a Mac changing its hotkeys or ignored apps leaves the Windows PC's `Ctrl+Alt+Shift+…` defaults and executable names alone, and the reverse; shared settings (compression, corner, rail) do follow. Pinned in Task 2 (`per_os_keys_never_cross_systems`).
3. **A setting changed in Settings at the moment a remote merge lands.** Expected: the user's change wins and is synced out; the merge never puts the older remote value back. Pinned in Task 2 (`a_local_change_made_during_a_merge_is_not_reverted`).
4. **Recording a shortcut equal to a hotkey Ferry already holds, or closing Settings mid-recording.** Expected: pressing ⌃⇧C while recording records it (no optimise runs) and is refused as a duplicate of "Optimise clipboard" if another action uses it; closing Settings while recording gives every hotkey back. Pinned in Task 3 (`suspended_runtime_registers_nothing`, `a_chord_used_by_another_ferry_hotkey_is_a_duplicate`), Task 4 (the Settings `Destroyed` hook) and Task 6 manual check 3.
5. **Updates before the first signed release, offline, or with a manifest missing this platform.** Expected: the app starts and runs normally, automatic checks fail silently (no card, no tray mark), "Check now" shows "Couldn't check for updates: …" with "Try again", and nothing is downloaded or installed. Pinned in Task 10 (`automatic_failures_stay_quiet`, `one_line_keeps_the_first_line_only`) and Task 11 manual check 1.

Also pinned (lower likelihood): unknown rail ids (Task 7 `skips_unknown_ids_without_dropping_them`, Task 1 `unknown_rail_ids_are_kept_and_junk_dropped`); a skipped version shown again only on a manual check (Task 10 `a_skipped_version_stays_quiet`).

---

### Task 1: The `[svc]` settings section

**Files:**
- Create: `crates/ferry-core/src/svc_settings.rs`
- Modify: `crates/ferry-core/src/lib.rs` (after `pub mod sniff;`), `crates/ferry-core/src/settings.rs` (three anchors in the table)
- Modify: `src/lib/settings.ts`, `src/lib/settings.fixture.ts` (anchor: immediately before `  clipboard: {`)
- Test: `crates/ferry-core/src/svc_settings.rs` `mod tests`; `src/lib/schema.test.ts` (unchanged, must still pass)

**Interfaces:**
- Produces (all `#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]`, `#[serde(default)]`, each with `#[serde(flatten)] pub extra: Map<String, Value>`):
  ```rust
  pub const DEFAULT_RAIL: [&str; 6] = ["downscale", "compression", "crop", "restore", "compare", "copy"];
  pub const MAX_RAIL_ENTRIES: usize = 16;
  pub struct Svc { pub updates: UpdateSettings, pub sync: SyncSettings, pub rail: RailSettings, pub shortcuts: ShortcutSettings, pub extra: Map<String, Value> }
  pub struct UpdateSettings { pub auto_check: bool, pub pill: bool, pub skipped_version: String, pub extra: … }
  pub struct SyncSettings { pub enabled: bool, pub folder: String, pub extra: … }
  pub struct RailSettings { pub tools: Vec<String>, pub extra: … }
  pub struct ShortcutSettings { pub preview: String, pub compare: String, pub step_down: String, pub disabled: Vec<String>, pub extra: … }
  impl Svc { pub fn sanitised(self) -> Self }
  /// `^[a-z][a-z0-9_.:-]{0,63}$`: built-in tools, other streams' tools, `pipeline:<id>`.
  pub fn is_tool_id(id: &str) -> bool
  ```
  `Settings` gains `pub svc: Svc`. TS `Settings` gains
  `svc: { updates: { auto_check: boolean; pill: boolean; skipped_version: string }; sync: { enabled: boolean; folder: string }; rail: { tools: string[] }; shortcuts: { preview: string; compare: string; step_down: string; disabled: string[] } };`
  and the fixture gets the defaults from "New settings keys".

`sanitised` rules: rail tools trimmed, empty and duplicate ids dropped (first wins), ids failing `is_tool_id` dropped, list cut to `MAX_RAIL_ENTRIES`; an empty or whitespace accelerator in `shortcuts` takes its default; `disabled` trimmed, empties and duplicates dropped; `skipped_version` and `folder` trimmed; `sync.enabled` forced to `false` when `folder` is empty.

- [ ] **Step 1: Write the failing tests** in `svc_settings.rs`:
  - `svc_defaults_match_the_plan`: `serde_json::to_value(Svc::default())` equals `json!({"updates": {"auto_check": true, "pill": true, "skipped_version": ""}, "sync": {"enabled": false, "folder": ""}, "rail": {"tools": ["downscale","compression","crop","restore","compare","copy"]}, "shortcuts": {"preview": "Space", "compare": "CmdOrCtrl+D", "step_down": "-", "disabled": []}})`.
  - `a_version_2_file_without_svc_loads_the_defaults`: write `{"version": 2, "images": {"compression": 70}}` to a temp `settings.json`; `Settings::load` gives `svc == Svc::default()`, `version == 2`, compression 70.
  - `unknown_rail_ids_are_kept_and_junk_dropped`: tools `[" crop ", "crop", "", "pipeline:abc", "Bad Id!", "trim"]` sanitise to `["crop", "pipeline:abc", "trim"]`; 20 distinct valid ids are cut to 16.
  - `blank_hover_keys_fall_back_to_their_defaults`: `preview = "  "` → `"Space"`; `disabled = [" scale ", "scale", ""]` → `["scale"]`.
  - `sync_without_a_folder_is_off`: `enabled: true, folder: " "` → `enabled: false, folder: ""`.
  - `unknown_fields_inside_svc_survive`: load `{"svc": {"future": 1, "rail": {"tools": ["copy"], "x": true}}}`, save, re-read the JSON: `svc.future == 1`, `svc.rail.x == true`, `svc.rail.tools == ["copy"]`, and `svc.updates.pill == true` was written out.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p ferry-core svc_settings`. Expected: compile errors (module missing).
- [ ] **Step 3: Implement** `svc_settings.rs`, the three `settings.rs` lines (comment `// svc (stream G)` on each) and the TS type and fixture.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p ferry-core svc_settings`, then `cargo test -p ferry-core settings`, then `pnpm test`, `pnpm check`. Expected: all PASS (the existing settings tests are untouched and still pass).
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/ferry-core/src/svc_settings.rs crates/ferry-core/src/lib.rs crates/ferry-core/src/settings.rs src/lib/settings.ts src/lib/settings.fixture.ts
  git commit -m "feat(core): the svc settings section for updates, sync, rail and hover hotkeys"
  ```

---

### Task 2: The settings sync engine (pure)

**Files:**
- Create: `crates/ferry-core/src/settings_sync.rs`
- Modify: `crates/ferry-core/src/lib.rs` (after `pub mod settings;`)
- Test: `crates/ferry-core/src/settings_sync.rs` `mod tests`

**Interfaces:**
- Consumes: `settings::Settings` (Task 1), `serde_json::{Value, Map}`.
- Produces:
  ```rust
  pub const FORMAT: &str = "ferry-settings-sync";
  pub const FORMAT_VERSION: u32 = 1;
  pub const SUBFOLDER: &str = "Ferry Settings";
  pub const MAX_FILE_BYTES: u64 = 1_048_576;
  /// Path prefixes that never leave this computer. Other streams add theirs here.
  pub const LOCAL: &[&str] = &["version", "general.launch_at_login", "general.show_dock_icon", "svc.sync", "svc.updates.skipped_version"];
  /// Path prefixes that sync only between computers with the same OS.
  pub const PER_OS: &[&str] = &["hotkeys", "svc.shortcuts", "clipboard.ignored_apps"];
  #[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Scope { Shared, PerOs, Local }
  pub fn scope(path: &str) -> Scope;                         // prefix match on a dot boundary
  pub fn sync_key(path: &str, os: &str) -> Option<String>;   // None for Local; "@macos/hotkeys.optimise" for PerOs; the path itself for Shared
  pub fn settings_path_of(key: &str, os: &str) -> Option<String>; // inverse; None for another OS's key
  pub fn flatten(value: &Value) -> BTreeMap<String, Value>;   // dotted leaf paths; arrays, scalars and empty objects are leaves

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct Entry { pub v: Value, pub t: i64, pub d: String } // value, modified (ms since epoch), device id
  pub fn newer(a: &Entry, b: &Entry) -> bool;                 // a.t > b.t, or equal t and a.d > b.d

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct DeviceFile { pub format: String, pub format_version: u32, pub device: String, pub device_name: String,
      pub os: String, pub app_version: String, pub written_at: i64, pub entries: BTreeMap<String, Entry> }
  #[derive(Debug, PartialEq)] pub enum ReadError { TooLarge, Corrupt, NotFerry, NewerFormat { version: u32, device_name: Option<String> } }
  pub fn parse(bytes: &[u8]) -> Result<DeviceFile, ReadError>;

  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")]
  pub enum JoinMode { UseFolder, UseThis }

  #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
  pub struct Ledger { pub device: String, pub host: String, pub entries: BTreeMap<String, Entry> }
  #[derive(Debug, Clone, PartialEq)]
  pub struct Adopted { pub key: String, pub previous: Option<Entry>, pub entry: Entry }
  impl Ledger {
      pub fn join(settings: &Settings, os: &str, mode: JoinMode, device: &str, host: &str, now: i64) -> Self; // UseFolder: every entry t = 0; UseThis: t = now
      pub fn record_local(&mut self, settings: &Settings, os: &str, now: i64) -> bool; // stamps changed keys (t = now, d = self.device); true if any
      pub fn merge(&mut self, remotes: &[DeviceFile], os: &str) -> Vec<Adopted>;      // adopts newer entries whose value differs
      pub fn revert(&mut self, keys: &[String], adopted: &[Adopted]);                 // puts `previous` back (or removes the entry)
      pub fn device_file(&self, device_name: &str, os: &str, app_version: &str, now: i64) -> DeviceFile;
  }
  /// Apply adopted values to `settings`. A key is applied only when the current value at its path still
  /// equals `previous.v` (or `previous` is None); one that fails to deserialize is undone. Returns the
  /// new settings and the keys not applied.
  pub fn apply(settings: &Settings, adopted: &[Adopted], os: &str) -> (Settings, Vec<String>);
  pub fn sync_dir(chosen: &Path) -> PathBuf;                  // chosen/"Ferry Settings", or chosen itself when already named so
  pub fn file_name(device: &str) -> String;                   // "<device>.json"
  pub fn is_conflict_copy(file_name: &str, file: &DeviceFile) -> bool; // file name stem != file.device
  pub fn new_device_id(seed: u64) -> String;                  // "d" + 16 lowercase hex digits
  #[derive(Debug, Clone, PartialEq, Serialize)] pub struct DeviceSummary { pub device: String, pub name: String, pub os: String, pub written_at: i64 }
  #[derive(Debug, Clone, PartialEq, Serialize)] pub struct FolderInfo { pub exists: bool, pub devices: Vec<DeviceSummary>, pub newer_format: bool }
  pub fn summarise(files: &[DeviceFile], own: &str) -> Vec<DeviceSummary>; // one per other device, newest written_at, newest first
  ```
- Rules: `merge` takes every file it is given, including files that carry our own device id (cloud conflict copies of our file); the caller leaves out our canonical `<device>.json` (Task 13). It ignores another OS's `@os/` keys and `Local` keys, and is idempotent. `device_file` writes only shared keys and this OS's `@os/` keys. `record_local` reads keys from `serde_json::to_value(settings)` through `flatten`, so unknown fields (`extra`) sync too.

- [ ] **Step 1: Write the failing tests:**
  - `scope_classifies_keys`: `version`, `general.launch_at_login`, `svc.sync.folder`, `svc.updates.skipped_version` → `Local`; `hotkeys.optimise`, `svc.shortcuts.preview`, `clipboard.ignored_apps` → `PerOs`; `images.compression`, `svc.rail.tools`, `svc.updates.pill`, `watchers.folders`, `svc.sync_extra` → `Shared` (the last checks the dot boundary).
  - `sync_keys_round_trip`: `sync_key("hotkeys.optimise", "macos") == Some("@macos/hotkeys.optimise")`; `settings_path_of("@macos/hotkeys.optimise", "macos") == Some("hotkeys.optimise")`; `settings_path_of("@macos/hotkeys.optimise", "windows") == None`; `sync_key("version", "macos") == None`; `sync_key("images.compression", "windows") == Some("images.compression")`.
  - `flatten_makes_leaves_of_objects_only`: on `to_value(Settings::default())`, keys include `images.output.optimised.template` and `images.crop_sizes` (an array value), and `flatten(&json!({"a": {}}))` has `a == {}`.
  - `a_local_change_is_stamped_and_others_keep_their_time`: join `UseThis` at t=100; change compression to 60; `record_local(.., 200)` returns true; `images.compression` has t=200, `images.strip_metadata` keeps t=100; a second `record_local(.., 300)` returns false.
  - `the_newer_entry_wins_and_ties_go_to_the_larger_device_id`.
  - `per_os_keys_never_cross_systems`: a `windows` remote with `@windows/hotkeys.optimise = "Ctrl+Alt+Shift+X"` at t=999 and `images.compression = 40` at t=999; a `macos` ledger merges only `images.compression`; `device_file(.., "macos", ..)` holds no `@windows/` key.
  - `join_use_folder_lets_every_remote_key_win` and `join_use_this_lets_local_win` (remote t=50 against local t=0 or t=100).
  - `a_truncated_or_foreign_file_is_rejected_not_read_as_empty`: `parse(br#"{"format":"ferry-settings-sync","form"#) == Err(Corrupt)`; `parse(b"{}") == Err(NotFerry)`; `parse(b"[]") == Err(NotFerry)`; 1 MiB + 1 bytes → `Err(TooLarge)`.
  - `a_newer_format_is_named_and_not_merged`: `format_version: 2, device_name: "Huy's PC"` → `Err(NewerFormat { version: 2, device_name: Some("Huy's PC") })`.
  - `a_value_of_the_wrong_type_is_rejected_and_the_rest_applied`: adopted `images.compression = "high"` and `images.strip_metadata = false` → result has `strip_metadata == false`, compression unchanged, rejected `== ["images.compression"]`.
  - `a_local_change_made_during_a_merge_is_not_reverted`: ledger records compression 80; merge adopts a remote 60 (`previous.v == 80`); the settings passed to `apply` now hold 70 → result keeps 70 and `images.compression` is in the returned keys; after `revert` and `record_local(.., now)` the entry is `{v: 70, t: now, d: own}`.
  - `merging_the_same_files_twice_changes_nothing`: second `merge` returns `[]`.
  - `a_conflict_copy_of_our_own_file_is_merged`: a file with `device == ledger.device` holding a newer `images.compression` is adopted like any other.
  - `conflict_copies_are_recognised`: `("dabc.json", device "dabc")` false; `("dabc (Huy's conflicted copy 2026-10-23).json", "dabc")` true; `("dabc 2.json", "dabc")` true.
  - `sync_dir_reuses_a_folder_already_named_ferry_settings`: `/x/iCloud` → `/x/iCloud/Ferry Settings`; `/x/Ferry Settings` → itself.
  - `device_ids_look_alike`: `new_device_id(0)` matches `^d[0-9a-f]{16}$`, two seeds give two ids.
  - `a_device_file_round_trips` through `serde_json` and `parse`.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p ferry-core settings_sync`. Expected: compile errors.
- [ ] **Step 3: Implement** `settings_sync.rs`. `apply` sets values into `serde_json::to_value(settings)` one key at a time (create intermediate objects as needed), checking `serde_json::from_value::<Settings>` after each and undoing a key that fails, then returns the final `Settings` (not yet sanitised; `prefs::update` sanitises).
- [ ] **Step 4: Run the tests.** Run: `cargo test -p ferry-core settings_sync`. Expected: PASS.
- [ ] **Step 5: Commit.**
  ```bash
  git add crates/ferry-core/src/settings_sync.rs crates/ferry-core/src/lib.rs
  git commit -m "feat(core): per-key settings sync through one file per device"
  ```

---

### Task 3: Hotkey bindings and their conflict check (pure)

**Files:**
- Modify: `src-tauri/src/hotkeys.rs` (keeps `Action`, `act`, `optimise_in_background`; adds the items below)
- Test: `src-tauri/src/hotkeys.rs` `mod tests`

**Interfaces:**
- Consumes: `ferry_core::settings::{Settings, Hotkeys}`, `ferry_core::svc_settings::ShortcutSettings` (Task 1), `tauri_plugin_global_shortcut::Shortcut` (parse with `str::parse::<Shortcut>()`; two chords are equal when their `id()` is equal).
- Produces:
  ```rust
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
  pub enum Group { Global, Hover }
  pub const BINDINGS: [(&str, Group); 7] = [("optimise", Group::Global), ("aggressive", Group::Global), ("cycle_state", Group::Global),
      ("preview", Group::Hover), ("compare", Group::Hover), ("step_down", Group::Hover), ("scale", Group::Hover)];
  pub const DIGITS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
  pub fn accelerator(settings: &Settings, id: &str) -> Option<String>;          // None for "scale" and unknown ids
  pub fn set_accelerator(settings: &mut Settings, id: &str, accelerator: &str) -> bool;
  pub fn default_accelerator(id: &str, mac: bool) -> Option<String>;            // Hotkeys::for_platform(mac) / ShortcutSettings::default()
  pub fn is_enabled(settings: &Settings, id: &str) -> bool;                     // id not in svc.shortcuts.disabled
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum Problem { Invalid, NeedsModifier, Reserved, CardKey, Duplicate { with: String } }
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
  pub enum Warning { DevTools }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  pub struct Verdict { pub problem: Option<Problem>, pub warning: Option<Warning> }
  /// Would `accelerator` be fine for binding `id`, given every *other* enabled binding in `settings`?
  pub fn check(settings: &Settings, id: &str, accelerator: &str, mac: bool) -> Verdict;
  /// The chords to register now: enabled bindings of `group` whose own accelerator passes `check`;
  /// "scale" expands to the nine digits. Empty when `suspended`.
  pub fn wanted(settings: &Settings, group: Group, suspended: bool, mac: bool) -> Vec<(&'static str, String)>;
  ```
- Data (in `hotkeys.rs`, as `const` string slices parsed at check time):
  - Reserved on macOS: `Cmd+Space`, `Ctrl+Space`, `Cmd+Tab`, `Cmd+Shift+Tab`, `Cmd+Q`, `Cmd+W`, `Cmd+H`, `Cmd+M`, `Cmd+Alt+H`, `Cmd+Alt+Escape`, `Ctrl+Cmd+Q`, `Ctrl+Cmd+F`, `Ctrl+Cmd+Space`, `Cmd+Shift+3`, `Cmd+Shift+4`, `Cmd+Shift+5`, `Cmd+C`, `Cmd+V`, `Cmd+X`, `Cmd+Z`, `Cmd+A`, `Cmd+S`.
  - Reserved on Windows: `Alt+Tab`, `Alt+F4`, `Alt+Space`, `Ctrl+Escape`, `Ctrl+Shift+Escape`, `Ctrl+Alt+Delete`, `Super+L`, `Super+D`, `Super+E`, `Super+R`, `Super+Tab`, `Ctrl+C`, `Ctrl+V`, `Ctrl+X`, `Ctrl+Z`, `Ctrl+Y`, `Ctrl+A`, `Ctrl+S`.
  - Card keys (refused for hover bindings): `Escape`, `Tab`, `Enter`, `Backspace`.
  - Dev-tools warning (Windows only): `Ctrl+Shift+C`, `Ctrl+Shift+I`, `Ctrl+Shift+J`.
- Order of checks: `Invalid` (empty or does not parse) → `NeedsModifier` (Global only: no Ctrl, Alt or Super) → `CardKey` (Hover only) → `Reserved` → `Duplicate` (against every other enabled binding's parsed chord, and against `DIGITS` when "scale" is enabled; `with` is the other id) → warning.

- [ ] **Step 1: Write the failing tests:**
  - `every_binding_has_a_default_on_both_systems`: for each `BINDINGS` id except `scale`, `default_accelerator(id, true)` and `(id, false)` are `Some` and parse.
  - `defaults_have_no_conflicts_on_either_system`: `Settings::default()`; for each id but `scale`, `check(&s, id, &accelerator(&s, id).unwrap(), cfg!(target_os = "macos")).problem == None`.
  - `a_global_hotkey_needs_ctrl_alt_or_cmd`: `"Shift+C"` and `"C"` → `NeedsModifier` for `optimise`; `"Alt+O"` → none.
  - `a_hover_key_may_be_bare_but_not_a_card_key`: `preview` with `"P"` → none; `"Escape"`, `"Tab"` → `CardKey`.
  - `a_chord_used_by_another_ferry_hotkey_is_a_duplicate`: with `hotkeys.optimise = "Ctrl+Shift+C"`, `check(.., "preview", "Shift+Ctrl+C", ..)` → `Duplicate { with: "optimise" }`.
  - `a_disabled_hotkey_does_not_block_its_chord`: add `"optimise"` to `svc.shortcuts.disabled`; the same check → none.
  - `digits_conflict_only_while_scale_is_on`: `step_down` with `"5"` → `Duplicate { with: "scale" }`; with `"scale"` disabled → none.
  - `system_shortcuts_are_refused`: `check(.., "optimise", "Cmd+Space", .., true)` and `"Cmd+C"` → `Reserved`; `check(.., "optimise", "Ctrl+Alt+Delete", .., false)` → `Reserved`; `"Alt+Tab"` for hover `compare` with `mac = false` → `Reserved`.
  - `windows_warns_about_browser_dev_tools`: `check(.., "optimise", "Ctrl+Shift+I", .., false)` → no problem, `warning == Some(DevTools)`; with `mac = true` → no warning.
  - `junk_is_invalid`: `""`, `"Ctrl+"`, `"Ctrl+Shift+Banana"`, `"Ctrl+C+D"` → `Invalid`.
  - `every_listed_shortcut_parses`: every reserved, card-key and dev-tools entry parses.
  - `suspended_runtime_registers_nothing`: `wanted(&s, Group::Global, true, mac)` is empty; with `false` it holds 3 entries; `wanted(.., Group::Hover, false, ..)` holds 3 + 9 entries, and 3 once `"scale"` is disabled.
  - `set_accelerator_writes_the_right_field`: `"cycle_state"` → `hotkeys.cycle_state`; `"compare"` → `svc.shortcuts.compare`; `"scale"` and `"nope"` → false.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p ferry hotkeys`. Expected: compile errors.
- [ ] **Step 3: Implement** the items above. Tests that use `CmdOrCtrl` pass `mac = cfg!(target_os = "macos")`, because the parser resolves it at compile time.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p ferry hotkeys`. Expected: PASS.
- [ ] **Step 5: Commit.**
  ```bash
  git add src-tauri/src/hotkeys.rs
  git commit -m "feat(app): hotkey bindings with one conflict check"
  ```

---

### Task 4: Re-registering hotkeys at run time

**Files:**
- Modify: `src-tauri/src/hotkeys.rs`, `src-tauri/src/hover_keys.rs`, `src-tauri/src/prefs.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/menu.rs`, `src-tauri/src/lib.rs` (anchors in the table)
- Create: `src-tauri/src/svc_commands.rs` (the hotkey commands; later tasks add more)
- Test: `src-tauri/src/prefs.rs` `mod tests`, `src-tauri/src/hover_keys.rs` `mod tests`

**Interfaces:**
- Consumes: Task 3's `BINDINGS`, `check`, `wanted`, `accelerator`, `set_accelerator`, `default_accelerator`, `is_enabled`.
- Produces in `hotkeys.rs`:
  ```rust
  pub struct Registered { pub optimise: Option<String>, pub aggressive: Option<String> } // unchanged shape
  pub fn reregister(app: &AppHandle);        // main thread: unregister what this module holds, register `wanted(Global)`, record failures, refresh hover keys, rebuild the tray, emit "svc:hotkeys"
  pub fn suspend(app: &AppHandle, on: bool); // main thread: sets the flag, then reregister + hover_keys::set_suspended
  pub fn current() -> Registered;            // registered and enabled accelerators, for the tray
  pub fn hover_accelerator(app: &AppHandle, id: &str) -> Option<String>; // for the card menu; None when off or floating.hover_hotkeys is off
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")]
  pub enum KeyState { Registered, Ready, Off, Failed, Blocked }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  pub struct Status { pub id: &'static str, pub group: Group, pub accelerator: Option<String>, pub enabled: bool, pub state: KeyState, pub problem: Option<Problem> }
  pub fn status(app: &AppHandle) -> Vec<Status>; // BINDINGS order; Ready = a valid hover key (held only on hover); Failed = registration failed
  ```
  A private `static RUNTIME: Mutex<Runtime { held: Vec<(&'static str, String)>, failed: Vec<&'static str>, suspended: bool }>` keeps what is registered. While suspended, `status` returns the statuses computed at the last unsuspended `reregister`.
- In `hover_keys.rs`: `KEYS` is replaced by `fn keys(app: &AppHandle) -> Vec<(String, Key)>` built from `hotkeys::wanted(settings, Group::Hover, false, cfg!(target_os = "macos"))` (`preview` → `Key::Preview`, `compare` → `Key::Compare`, `step_down` → `Key::StepDown`, `scale` digit `n` → `Key::Scale(n * 10)`); `Keys.held` becomes `Vec<String>`; new `suspended: bool` and `failed: Vec<String>` fields; `apply` passes `enabled && !suspended` as `plan`'s `enabled` argument (**`plan`'s signature does not change**). New `pub fn refresh(app: &AppHandle)` (unregister everything held, clear it, then `apply`), `pub fn set_suspended(app: &AppHandle, on: bool)`, `pub fn failed() -> Vec<String>`.
- In `prefs.rs`: `Effects` gains `pub hotkeys: bool` = `old.hotkeys != new.hotkeys || old.svc.shortcuts != new.svc.shortcuts || old.floating.hover_hotkeys != new.floating.hover_hotkeys`; `apply` does `let a = app.clone(); let _ = app.run_on_main_thread(move || crate::hotkeys::reregister(&a));`.
- In `svc_commands.rs` (all registered in `generate_handler!`):
  ```rust
  #[tauri::command] pub fn svc_hotkeys_status(app: AppHandle) -> Vec<hotkeys::Status>;
  #[tauri::command] pub fn svc_hotkey_check(app: AppHandle, id: String, accelerator: String) -> hotkeys::Verdict;
  #[tauri::command(async)] pub fn svc_hotkey_set(app: AppHandle, id: String, accelerator: String) -> Result<hotkeys::Verdict, String>;   // saves only when problem is None
  #[tauri::command(async)] pub fn svc_hotkey_enable(app: AppHandle, id: String, on: bool) -> Result<hotkeys::Verdict, String>;        // turning on re-checks the current accelerator
  #[tauri::command(async)] pub fn svc_hotkey_reset(app: AppHandle, id: String) -> Result<hotkeys::Verdict, String>;                  // default_accelerator, same rules as set
  #[tauri::command] pub fn svc_hotkeys_suspend(app: AppHandle, on: bool);
  pub fn setup(app: &AppHandle); // grows in later tasks; empty body here
  ```
- In `lib.rs`: `.on_window_event(|window, event| if window.label() == windows::SETTINGS && matches!(event, tauri::WindowEvent::Destroyed) { hotkeys::suspend(window.app_handle(), false) }) // svc: closing Settings mid-recording gives the hotkeys back`.

- [ ] **Step 1: Write the failing tests:**
  - in `prefs.rs`: `a_hotkey_change_reregisters`: changing `hotkeys.optimise`, `svc.shortcuts.preview`, `svc.shortcuts.disabled` or `floating.hover_hotkeys` each gives `Effects { hotkeys: true, ..Effects::default() }`; the existing `settings_read_on_every_job_need_no_effect` still passes.
  - in `hover_keys.rs`: replace `every_accelerator_parses` with `every_default_hover_key_parses` (the 12 accelerators from `hotkeys::wanted(&Settings::default(), Group::Hover, false, cfg!(target_os = "macos"))` parse, and map to `Preview`, `Compare`, `StepDown` and `Scale(10)`…`Scale(90)`), through a pure `fn key_for(id: &str, accelerator: &str) -> Option<Key>`.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p ferry prefs`, then `cargo test -p ferry hover_keys`. Expected: compile errors.
- [ ] **Step 3: Implement** the items above and the tray, menu and lib.rs edits from the table. `reregister` replaces `register` (delete the old function). Every `info!`/`warn!` that `register` logged stays (one line per failed hotkey, C1 §6).
- [ ] **Step 4: Run the tests.** Run: `cargo test -p ferry prefs`, `cargo test -p ferry hover_keys`, `cargo test -p ferry hotkeys`, then `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS and clean.
- [ ] **Step 5: Manual check on this Mac** (ask the user first; `pnpm tauri dev`): open the tray menu. Look for: "Optimise clipboard ⌃⇧C" and "Optimise clipboard aggressively ⌃⇧A" exactly as before; copy a screenshot, press ⌃⇧C: a card appears; hover the card and press Space: Quick Look opens; the card's ⋯ menu still shows ⌘D beside Compare and Space beside Preview.
- [ ] **Step 6: Commit.**
  ```bash
  git add src-tauri/src/hotkeys.rs src-tauri/src/hover_keys.rs src-tauri/src/prefs.rs src-tauri/src/tray.rs src-tauri/src/menu.rs src-tauri/src/lib.rs src-tauri/src/svc_commands.rs
  git commit -m "feat(app): hotkeys re-register when their settings change"
  ```

---

### Task 5: Shortcut helpers for the page

**Files:**
- Create: `src/lib/svc.ts` (types), `src/lib/hotkeys.ts`, `src/lib/hotkeys.test.ts`

**Interfaces:**
- Produces in `svc.ts` (mirrors of the Rust serde shapes; extended in Tasks 10 and 13):
  ```ts
  export type HotkeyId = 'optimise' | 'aggressive' | 'cycle_state' | 'preview' | 'compare' | 'step_down' | 'scale';
  export type Problem = { kind: 'invalid' } | { kind: 'needs_modifier' } | { kind: 'reserved' } | { kind: 'card_key' } | { kind: 'duplicate'; with: string };
  export interface Verdict { problem: Problem | null; warning: 'dev_tools' | null }
  export interface HotkeyStatus { id: string; group: 'global' | 'hover'; accelerator: string | null; enabled: boolean;
    state: 'registered' | 'ready' | 'off' | 'failed' | 'blocked'; problem: Problem | null }
  export interface PaneRequest { pane: string; row: string | null }
  ```
- Produces in `hotkeys.ts`:
  ```ts
  /** The accelerator for a keydown, or null for a lone modifier or a key Ferry can't bind. Order: Ctrl+Alt+Shift+Cmd|Super+Key. */
  export function acceleratorFromEvent(e: Pick<KeyboardEvent, 'code' | 'ctrlKey' | 'altKey' | 'shiftKey' | 'metaKey'>, mac: boolean): string | null;
  /** "⌃⇧C" on macOS (⌃⌥⇧⌘ order), "Ctrl+Alt+Shift+C" on Windows; CmdOrCtrl → ⌘ / Ctrl; Super → Win on Windows. */
  export function hotkeyLabel(accelerator: string, mac: boolean): string;
  /** The message key for a verdict or a failed status, or null. */
  export function problemKey(problem: Problem | null, warning: 'dev_tools' | null, mac: boolean): StringKey | null;
  ```
  Key tokens: `KeyA`–`KeyZ` → the letter; `Digit0`–`Digit9` → the digit; these `event.code`s pass through unchanged: `F1`–`F24`, `Space`, `Minus`, `Equal`, `Comma`, `Period`, `Slash`, `Backslash`, `Semicolon`, `Quote`, `BracketLeft`, `BracketRight`, `Backquote`, `ArrowUp`, `ArrowDown`, `ArrowLeft`, `ArrowRight`, `Home`, `End`, `PageUp`, `PageDown`, `Insert`, `Delete`, `Backspace`, `Enter`, `Tab`, `Escape`, `Numpad0`–`Numpad9`. Anything else → null. Labels show `Minus` and `-` as `-`, `Equal` as `=`, arrows as `↑ ↓ ← →`, `Space` as `Space`. `problemKey` maps `needs_modifier` and `reserved` to their `.mac`/`.windows` keys; `duplicate`'s `{name}` is filled by the caller with the other binding's label.

- [ ] **Step 1: Write the failing tests** (`describe('acceleratorFromEvent')`, `describe('hotkeyLabel')`, `describe('problemKey')`):
  - `{code:'KeyC', ctrlKey, shiftKey}` mac → `'Ctrl+Shift+C'`; `{code:'KeyD', metaKey}` mac → `'Cmd+D'`, windows → `'Super+D'`; `{code:'Digit5'}` → `'5'`; `{code:'Minus'}` → `'Minus'`; `{code:'Space'}` → `'Space'`; `{code:'F5'}` → `'F5'`; `{code:'ArrowUp', altKey}` → `'Alt+ArrowUp'`; `{code:'ShiftLeft', shiftKey}`, `{code:'MetaRight', metaKey}`, `{code:'IntlBackslash'}` → `null`.
  - `('Ctrl+Shift+C', mac)` and `('Shift+Ctrl+C', mac)` → `'⌃⇧C'`; `('Cmd+Alt+P', mac)` → `'⌥⌘P'`; `('CmdOrCtrl+D', mac)` → `'⌘D'`, windows → `'Ctrl+D'`; `('Ctrl+Alt+Shift+C', windows)` → `'Ctrl+Alt+Shift+C'`; `('Super+L', windows)` → `'Win+L'`; `('Minus', mac)` and `('-', windows)` → `'-'`; `('Space', mac)` → `'Space'`.
  - `problemKey({kind:'needs_modifier'}, null, true) === 'svc.hotkey.needs_modifier.mac'`; `problemKey(null, 'dev_tools', false) === 'svc.hotkey.devtools'`; `problemKey(null, null, true) === null`.
- [ ] **Step 2: Run them and see them fail.** Run: `pnpm test -- hotkeys`. Expected: FAIL (module missing). `problemKey`'s return type needs the `svc.hotkey.*` strings, so add those rows of "Strings" to `en.json` now (creating the `svc.*` block after `"app.name": "Ferry",`).
- [ ] **Step 3: Implement** `svc.ts` and `hotkeys.ts`.
- [ ] **Step 4: Run the tests.** Run: `pnpm test`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Commit.**
  ```bash
  git add src/lib/svc.ts src/lib/hotkeys.ts src/lib/hotkeys.test.ts src/lib/i18n/en.json
  git commit -m "feat(ui): shortcut recording and labels for the hotkey editor"
  ```

---

### Task 6: The Hotkeys pane and opening Settings at a pane

**Files:**
- Create: `src/settings/controls/HotkeyRecorder.svelte`
- Modify: `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src/settings/Settings.svelte`, `src/lib/i18n/en.json`, `src-tauri/src/svc_commands.rs`, `src-tauri/src/lib.rs` (`generate_handler!`)

**Interfaces:**
- Consumes: Task 4's commands, Task 5's helpers, `store` from `src/settings/store.svelte.ts`.
- Produces in `schema.ts`: `PaneId` gains `'hotkeys' | 'about'`; `panes` appends `{ id: 'hotkeys', label: 'svc.pane.hotkeys' }`, `{ id: 'about', label: 'svc.pane.about' }`; `Control` gains `| { kind: 'hotkey'; action: HotkeyId } | { kind: 'rail' } | { kind: 'sync' } | { kind: 'update' }`; `Setting` gains `wide?: boolean` (Row stacks the control under the text, full width). Rows, appended right after the `hover_hotkeys` row:

  | id | pane | group | label | help | path | control | depends_on |
  | --- | --- | --- | --- | --- | --- | --- | --- |
  | `svc_hk_optimise` | hotkeys | `svc.group.global_hotkeys` | `tray.optimise` | `svc.hotkey.global.help` | `hotkeys.optimise` | hotkey optimise | — |
  | `svc_hk_aggressive` | hotkeys | same | `tray.aggressive` | same | `hotkeys.aggressive` | hotkey aggressive | — |
  | `svc_hk_cycle_state` | hotkeys | same | `svc.hotkey.cycle_state` | same | `hotkeys.cycle_state` | hotkey cycle_state | — |
  | `svc_hk_preview` | hotkeys | `svc.group.hover_hotkeys` | `svc.hotkey.preview` | `svc.hotkey.hover.help` | `svc.shortcuts.preview` | hotkey preview | `floating.hover_hotkeys` |
  | `svc_hk_compare` | hotkeys | same | `svc.hotkey.compare` | same | `svc.shortcuts.compare` | hotkey compare | same |
  | `svc_hk_step_down` | hotkeys | same | `svc.hotkey.step_down` | same | `svc.shortcuts.step_down` | hotkey step_down | same |
  | `svc_hk_scale` | hotkeys | same | `svc.hotkey.scale` | same | — | hotkey scale | same |
- `HotkeyRecorder.svelte` props: `{ action: HotkeyId; value: string | undefined; disabled: boolean; labelledby: string; label: string }`; `Row.svelte` passes `action={control.action}`, `value`, `disabled`, `labelledby={labelId}`, `label={t(setting.label)}`. A `duplicate` problem's `{name}` is the other binding's row label, found in `settingsFor(isMac)` by `control.action === problem.with`. It shows an enable switch (`aria-label` `svc.hotkey.enable`), a button with `hotkeyLabel(value)` (or `svc.hotkey.scale_keys` for `scale`, which has no button), a "Restore default" button, and one status line (`aria-live="polite"`) from the last `Verdict` or this id's `HotkeyStatus` (`failed` → `svc.hotkey.failed`). Recording: click the button → text `svc.hotkey.recording`, `invoke('svc_hotkeys_suspend', { on: true })`; on `keydown` call `preventDefault()`; Escape or blur cancels; a bare Tab ends recording and lets focus move; a lone modifier only updates the live label; any other key → `acceleratorFromEvent` → if null show `svc.hotkey.invalid`, else `invoke('svc_hotkey_set', { id, accelerator })`; in `finally` always `invoke('svc_hotkeys_suspend', { on: false })`. It reads statuses with `svc_hotkeys_status` on mount and on the `svc:hotkeys` event, and `store.value.svc.shortcuts.disabled` for the switch.
- Pane navigation (Rust, `svc_commands.rs`):
  ```rust
  #[derive(Debug, Clone, Serialize)] pub struct PaneRequest { pub pane: String, pub row: Option<String> }
  pub fn open_settings_at(app: &AppHandle, pane: &str, row: Option<&str>); // store the request, windows::open_settings(app), emit "svc:open-pane" to the settings window
  #[tauri::command] pub fn svc_settings_pane() -> Option<PaneRequest>;     // takes (and clears) the stored request
  ```
  `Settings.svelte` `onMount`: `takePane()` once, and again on every `svc:open-pane` event; `takePane` calls `svc_settings_pane`, sets `pane` when the id is in `panes`, and calls the existing `jump(setting)` when `row` names a setting.

- [ ] **Step 1: Write the failing tests** in `schema.test.ts`: `it('has the Hotkeys pane with every binding in order')`: the paths of the `hotkeys` pane's rows are `['hotkeys.optimise', 'hotkeys.aggressive', 'hotkeys.cycle_state', 'svc.shortcuts.preview', 'svc.shortcuts.compare', 'svc.shortcuts.step_down', undefined]`, and `panes.map(p => p.id)` ends with `['hotkeys', 'about']`. The existing tests (unique ids, paths exist in the fixture, depends_on is a switch, real strings) cover the new rows.
- [ ] **Step 2: Run them and see them fail.** Run: `pnpm test -- schema`. Expected: FAIL.
- [ ] **Step 3: Implement** the schema rows, `Row.svelte` branch (`{:else if control.kind === 'hotkey'} <HotkeyRecorder …/>` plus `class:wide`), `HotkeyRecorder.svelte`, the pane navigation in Rust and Svelte, and the `svc.pane.*`/`svc.group.*` strings. Add `svc_settings_pane` and the Task 4 commands to `generate_handler!`.
- [ ] **Step 4: Run the tests.** Run: `pnpm test`, `pnpm check`, `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS and clean.
- [ ] **Step 5: Manual check on this Mac** (ask the user first; `pnpm tauri dev`; TextEdit is the focus target):
  1. Settings ▸ Hotkeys. Look for: three global rows reading ⌃⇧C, ⌃⇧A, ⌃⇧P, three hover rows reading Space, ⌘D, -, and a "1 – 9" row with only a switch.
  2. Click the Optimise clipboard shortcut and press ⌃⌥O. Look for: the row reads ⌃⌥O; the tray menu shows ⌃⌥O beside "Optimise clipboard"; in TextEdit, ⌃⌥O optimises the clipboard and ⌃⇧C no longer does.
  3. Click the Aggressive shortcut and press ⌃⌥O. Look for: "Already used by “Optimise clipboard”." and no optimise ran while recording. Press Esc: the row is back to ⌃⇧A. Click it again and close the Settings window while it says "Type a shortcut…": after reopening Settings, ⌃⌥O still optimises from TextEdit.
  4. Record ⌘Space for Cycle: "macOS uses this shortcut." Record Shift+P: "Add ⌃, ⌥ or ⌘: a global hotkey needs one."
  5. Turn off "Compare with original", hover a card, press ⌘D: nothing opens; the card's ⋯ menu shows Compare with no shortcut.
  6. Click "Restore default" on each changed row. Look for: the defaults are back and work.
  7. Search "preview" in the sidebar: "Quick preview" is offered under Hotkeys, and picking it jumps there.
- [ ] **Step 6: Commit.**
  ```bash
  git add src/settings/controls/HotkeyRecorder.svelte src/lib/schema.ts src/lib/schema.test.ts src/settings/Row.svelte src/settings/Settings.svelte src/lib/i18n/en.json src-tauri/src/svc_commands.rs src-tauri/src/lib.rs
  git commit -m "feat(ui): a Hotkeys pane to record, turn off and restore every hotkey"
  ```

---

### Task 7: The rail tool catalog

**Files:**
- Create: `src/lib/rail.ts`, `src/lib/rail.test.ts`
- Modify: `src/lib/cards.ts`, `src/lib/cards.test.ts`

**Interfaces:**
- Produces in `rail.ts`:
  ```ts
  export interface RailTool { id: string; label: StringKey; labelWindows?: StringKey; needsOriginal: boolean; compact: boolean;
    /** Whether the tool applies to this finished card. Default: always. */ applies?: (card: JobUpdate) => boolean }
  export const RAIL_TOOLS: RailTool[]; // in this order: downscale, compression (compact false), crop, restore (applies: done && can_restore), compare (compact false), copy, reveal, aggressive, rename, preview, fit
  export const DEFAULT_RAIL_TOOLS: string[]; // the Rust DEFAULT_RAIL, same order
  export const MAX_TOOLS = 6; export const MAX_COMPACT_TOOLS = 4;
  export function toolLabel(id: string, mac: boolean): string;  // the catalog label, or t('svc.rail.unknown', { id })
  export function knownCount(tools: string[]): number;
  export function addable(tools: string[]): RailTool[];          // catalog tools not in `tools`, catalog order
  export function editorRows(tools: string[]): { id: string; known: boolean }[];
  export function moveItem<T>(list: T[], from: number, to: number): T[];  // a new array
  export function dropIndex(y: number, midpoints: number[]): number;      // the index the dragged row lands at
  ```
  `needsOriginal` is true for downscale, compression, crop, compare, aggressive and fit. Labels are the `svc.rail.tool.*` strings (`reveal` uses `.mac` / `.windows`).
- In `cards.ts`: `RailAction` becomes `string` (a catalog id or `'more'`); `railActions(card: JobUpdate, compact = false, tools: string[] = DEFAULT_RAIL_TOOLS): RailAction[]` keeps its early return, then walks `tools` in order keeping catalog tools that apply (and `compact: true` ones on a compact card), stops at `MAX_TOOLS` (`MAX_COMPACT_TOOLS` on compact), and appends `'more'`; `needsOriginal` reads the catalog.

- [ ] **Step 1: Write the failing tests** (the existing `railActions` tests in `cards.test.ts` stay as they are and must still pass):
  - `follows the user's order`: `railActions(done, false, ['copy', 'crop', 'downscale'])` → `['copy', 'crop', 'downscale', 'more']`.
  - `skips_unknown_ids_without_dropping_them`: `railActions(done, false, ['trim', 'copy', 'pipeline:x'])` → `['copy', 'more']`; `editorRows(['trim', 'copy'])` → `[{id:'trim', known:false}, {id:'copy', known:true}]`; `knownCount(['trim','copy']) === 1`.
  - `shows at most six tools, compact at most four compact tools`: all 11 catalog ids → 6 tools + `'more'`; compact → `['downscale', 'crop', 'restore', 'copy', 'more']` on a restorable done card.
  - `restore only on a restorable done card even when chosen`.
  - `new tools need the original where they re-encode`: `needsOriginal('aggressive')`, `needsOriginal('fit')` true; `'reveal'`, `'preview'`, `'rename'` false.
  - `addable lists what is not on the rail`: `addable(DEFAULT_RAIL_TOOLS).map(t => t.id)` → `['reveal', 'aggressive', 'rename', 'preview', 'fit']`.
  - `moveItem and dropIndex`: `moveItem(['a','b','c'], 0, 2)` → `['b','c','a']`; `dropIndex(5, [10, 30, 50]) === 0`, `dropIndex(35, [10, 30, 50]) === 2`, `dropIndex(99, [10, 30, 50]) === 3`.
  - `the default rail matches Rust`: `DEFAULT_RAIL_TOOLS` equals `['downscale','compression','crop','restore','compare','copy']`.
- [ ] **Step 2: Run them and see them fail.** Run: `pnpm test -- rail`, then `pnpm test -- cards`. Expected: FAIL. Add the `svc.rail.tool.*` and `svc.rail.unknown` strings now.
- [ ] **Step 3: Implement** `rail.ts` and the `cards.ts` changes.
- [ ] **Step 4: Run the tests.** Run: `pnpm test`, `pnpm check`. Expected: PASS (fix any `RailAction` type narrowing `pnpm check` reports in `Card.svelte` with a cast at the existing `needsOriginal(action as RailAction)` call only).
- [ ] **Step 5: Commit.**
  ```bash
  git add src/lib/rail.ts src/lib/rail.test.ts src/lib/cards.ts src/lib/cards.test.ts src/lib/i18n/en.json
  git commit -m "feat(ui): a catalog of card tools the rail can show, in any order"
  ```

---

### Task 8: The card rail follows the setting, with a "+" button

**Files:**
- Create: `src-tauri/src/rail_menu.rs`
- Modify: `src/stack/Card.svelte`, `src/stack/Stack.svelte`, `src/lib/ipc.ts` (`StackConfig`), `src-tauri/src/commands.rs`, `src-tauri/src/svc_commands.rs`, `src-tauri/src/lib.rs`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 7's catalog; Task 6's `open_settings_at`; `prefs::update`.
- Produces in `rail_menu.rs`:
  ```rust
  #[derive(Debug, Clone, Deserialize)] pub struct RailMenuItem { pub id: String, pub label: String, pub enabled: bool }
  pub fn popup(app: &AppHandle, window: &WebviewWindow, x: f64, y: f64, items: &[RailMenuItem]) -> Result<(), String>;
  pub fn init(app: &AppHandle); // app.on_menu_event for ids "svc:rail:add:<tool id>" and "svc:rail:edit"
  ```
  The menu lists `items` (ids `svc:rail:add:<id>`), a separator, and `svc.rail.customise` (`svc:rail:edit`). Add: on a blocking worker, `prefs::update(app, |s| if !s.svc.rail.tools.contains(&id) { s.svc.rail.tools.push(id) })`. Edit: `svc_commands::open_settings_at(app, "floating", Some("svc_rail"))`. `svc_commands::setup` calls `rail_menu::init`.
  Command: `#[tauri::command] pub fn svc_rail_menu(app: AppHandle, window: tauri::WebviewWindow, x: f64, y: f64, items: Vec<rail_menu::RailMenuItem>) -> Result<(), String>` (sync: menus pop up on the main thread).
- `StackConfig` (Rust and TS) gains `rail` (the `svc.rail.tools` list). `Stack.svelte`: the default config gets `rail: [...DEFAULT_RAIL_TOOLS]`, the `settings:changed` listener sets `rail: event.payload.svc.rail.tools`, `<Card … railTools={config.rail} />`.
- `Card.svelte` edits, nothing else moves:
  1. Props: `railTools = DEFAULT_RAIL_TOOLS` (typed `railTools?: string[]`, with a `/** svc: … */` comment).
  2. `const rail = $derived(railActions(card, compact, railTools));`
  3. One `$effect` (comment `// svc: the rail's visual order follows the setting; the blocks below keep their built-in order in the DOM`) that, after each render, sets `style.order` on every `.rail > [data-action]` to `rail.indexOf(action)` (the "+" button gets `rail.length`).
  4. After the `{#if rail.includes('copy')}…{/if}` block: `<!-- svc: rail tools (stream G) -->` with one `{#if rail.includes(id)}` button per new tool: `reveal` → `act('card_reveal')`; `aggressive` → `act('card_rerun', { aggressive: true })`; `rename` → `ontool('rename')`; `preview` → `invoke('quick_preview', { id: card.id })`; `fit` → `ontool('fit')`. `disabled={!card.can_edit}` where the catalog says `needsOriginal`. Icons: Lucide `folder-open`, `zap`, `pencil`, `eye`, `file-down`, inlined as the existing buttons do.
  5. After the `{#if rail.includes('more')}…{/if}` block: `{#if !compact}` a "+" button (Lucide `plus`, `data-action="add"`, `aria-label` `svc.rail.add`) that calls `invoke('svc_rail_menu', { x, y, items })` with `x`/`y` under the button (as `openMenu` does) and `items = addable(railTools).map(t => ({ id: t.id, label: toolLabel(t.id, isMac), enabled: knownCount(railTools) < MAX_TOOLS }))`.
  6. `tipText`: cases `reveal`, `aggressive`, `preview`, `fit`, `add` before `default:` (the existing `rename` case already answers).

- [ ] **Step 1: Run the existing tests first.** Run: `pnpm test`, `cargo test -p ferry`. Expected: PASS (this task adds no pure logic; Task 7 tested it).
- [ ] **Step 2: Implement** the items above, the `svc.rail.add` and `svc.rail.customise` strings, and register `svc_rail_menu`.
- [ ] **Step 3: Check.** Run: `pnpm check`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm test`. Expected: clean.
- [ ] **Step 4: Manual check on this Mac** (ask the user first; `pnpm tauri dev`; TextEdit is the focus target):
  1. Copy a screenshot. Look for: the rail reads Downscale · Compression · Crop · Restore · Compare · Copy · ⋯ · + exactly as before, plus the "+" at the end.
  2. Click "+". Look for: a native menu with Show in Finder, Optimise aggressively, Rename, Preview, Fit under size, all greyed (the rail is full), a separator, and "Customise rail…".
  3. Quit Ferry, set `svc.rail.tools` to `["copy","reveal","crop","trim"]` in `~/Library/Application Support/io.ferry.desktop/settings.json`, start Ferry again and copy a screenshot. Look for: Copy · Show in Finder · Crop · ⋯ · +; no gap for `trim`. Each new button works: Show in Finder reveals the file; "+" now offers the other tools enabled; picking "Preview" adds a Preview button at the end, which opens Quick Look.
  4. Set Floating results ▸ Card size to Compact. Look for: at most four tools, no "+".
  5. Click "Customise rail…". Look for: Settings opens at Floating results with the "Card tools" row highlighted (row exists after Task 9; before it, the pane opens).
- [ ] **Step 5: Commit.**
  ```bash
  git add src-tauri/src/rail_menu.rs src/stack/Card.svelte src/stack/Stack.svelte src/lib/ipc.ts src-tauri/src/commands.rs src-tauri/src/svc_commands.rs src-tauri/src/lib.rs src/lib/i18n/en.json
  git commit -m "feat(ui): card rails follow the chosen tools and order, with a + to add one"
  ```

---

### Task 9: The rail editor in Settings

**Files:**
- Create: `src/settings/controls/RailEditor.svelte`
- Modify: `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 7's `RAIL_TOOLS`, `DEFAULT_RAIL_TOOLS`, `editorRows`, `addable`, `knownCount`, `moveItem`, `dropIndex`, `toolLabel`, `MAX_TOOLS`.
- Schema rows, right after the `svc_hk_scale` row: `{ id: 'svc_rail', pane: 'floating', label: 'svc.rail.label', help: 'svc.rail.help', path: 'svc.rail.tools', control: { kind: 'rail' }, depends_on: 'floating.enabled', wide: true }` and `{ id: 'svc_update_pill', pane: 'floating', label: 'svc.updates.pill', help: 'svc.updates.pill.help', path: 'svc.updates.pill', control: { kind: 'switch' }, depends_on: 'floating.enabled' }`.
- `RailEditor.svelte` props `{ value: string[]; disabled: boolean; labelledby: string; onchange: (tools: string[]) => void }`. A list (`role="list"`, `aria-labelledby`), one row per `editorRows(value)`: a drag handle (`aria-label` `svc.rail.drag`), the label (unknown ids greyed with `svc.rail.unknown`), Move up / Move down buttons (`svc.rail.move_up`/`_down`, disabled at the ends), Remove (`svc.rail.remove`). Below: a `<select>` "Add a tool…" listing `addable(value)`, disabled with the `svc.rail.full` hint when `knownCount(value) >= MAX_TOOLS`; "Restore defaults" (`svc.rail.reset`) → `onchange([...DEFAULT_RAIL_TOOLS])`; `svc.rail.empty` when the list is empty. Drag uses pointer events on the handle (`setPointerCapture`, row midpoints from `getBoundingClientRect`, `dropIndex`, `moveItem` on release); no HTML5 drag and drop (Tauri's file-drop handling swallows it on Windows).
- `Row.svelte`: `{:else if control.kind === 'rail'} <RailEditor value={value as string[]} {disabled} labelledby={labelId} {onchange} />`.

- [ ] **Step 1: Write the failing test** in `schema.test.ts`: `it('puts the rail editor and the update pill under Floating results')`: rows with ids `svc_rail` and `svc_update_pill` exist in pane `floating` with paths `svc.rail.tools` and `svc.updates.pill`.
- [ ] **Step 2: Run it and see it fail.** Run: `pnpm test -- schema`. Expected: FAIL.
- [ ] **Step 3: Implement** the rows, the strings (`svc.rail.*` not added yet, `svc.updates.pill`, `svc.updates.pill.help`), `RailEditor.svelte` and the Row branch.
- [ ] **Step 4: Run the tests.** Run: `pnpm test`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Manual check on this Mac** (with the user's OK): Settings ▸ Floating results ▸ Card tools.
  1. Drag Copy to the top with the handle, then move Crop down with the keyboard (Tab to its Move down, Space). Look for: the list and a new card's rail show the same order.
  2. Remove Compare; add "Preview" from the select. Look for: the select greys out with "A card holds 6 tools…" when six are on the rail.
  3. With an unknown id in the file (Task 8 check 3), look for "Not in this version of Ferry (trim)" greyed, removable, not counted in the six.
  4. "Restore defaults" brings rail A back. Turn off "Show results" (floating.enabled): the editor greys with "Turn on … first."
- [ ] **Step 6: Commit.**
  ```bash
  git add src/settings/controls/RailEditor.svelte src/lib/schema.ts src/lib/schema.test.ts src/settings/Row.svelte src/lib/i18n/en.json
  git commit -m "feat(ui): a rail editor in Floating results to reorder, add and remove card tools"
  ```

---

### Task 10: The updater

**Files:**
- Create: `src-tauri/src/updater.rs`, `src-tauri/tauri.updater.conf.json`
- Modify: `src-tauri/Cargo.toml`, `Cargo.lock` (by cargo), `src-tauri/tauri.conf.json`, `src-tauri/src/lib.rs`, `src-tauri/src/tray.rs`, `src-tauri/src/prefs.rs`, `src-tauri/src/svc_commands.rs`, `src/lib/svc.ts`, `src/lib/i18n/en.json`
- Test: `src-tauri/src/updater.rs` `mod tests`, `src-tauri/src/prefs.rs` `mod tests`

**Interfaces:**
- Config, `tauri.conf.json` (new top-level key after `"bundle"`):
  ```json
  "plugins": {
    "updater": {
      "pubkey": "REPLACE_WITH_UPDATER_PUBLIC_KEY",
      "endpoints": ["https://github.com/Huy31203/ferry/releases/latest/download/latest.json"],
      "windows": { "installMode": "passive" }
    }
  }
  ```
  `tauri.updater.conf.json`: `{ "bundle": { "createUpdaterArtifacts": true } }`. The plugin reads `plugins.updater` at start-up and fails to start without it, so the key must be present in every build; the placeholder pubkey only makes signature checks fail.
- Produces in `updater.rs`:
  ```rust
  pub const FIRST_CHECK_AFTER: Duration = Duration::from_secs(60);
  pub const CHECK_EVERY: Duration = Duration::from_secs(6 * 60 * 60);
  pub const CHECK_TIMEOUT: Duration = Duration::from_secs(30);
  pub const RELEASES: &str = "https://github.com/Huy31203/ferry/releases/tag/v";
  #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)] #[serde(rename_all = "snake_case")] pub enum Phase { Check, Download, Install }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum UpdateState { Idle, Checking, UpToDate, Available { version: String, notes: String },
      Downloading { version: String, received: u64, total: Option<u64> }, Ready { version: String, busy: usize },
      Installing { version: String }, Failed { phase: Phase, reason: String } }
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)]
  pub struct UpdateView { pub current: String, pub state: UpdateState, pub skipped: bool, pub pill: bool, pub release_url: Option<String> }
  // pure
  pub fn is_skipped(version: &str, skipped_version: &str) -> bool;
  pub fn pill_visible(state: &UpdateState, skipped: bool, dismissed: bool, setting: bool) -> bool; // Available (not skipped), Downloading, Ready
  pub fn check_due(since_start: Duration, since_last: Option<Duration>, auto: bool) -> bool;
  pub fn one_line(reason: &str) -> String;           // first line, trimmed, at most 160 chars ending "…"
  pub fn release_url(version: &str) -> String;
  pub fn tray_label(state: &UpdateState, skipped: bool) -> (String, bool); // text, enabled
  // runtime
  pub fn init(app: &AppHandle);                      // manage the state; release builds start the scheduler thread
  pub fn view(app: &AppHandle) -> UpdateView;
  pub async fn check(app: AppHandle, manual: bool) -> Result<(), String>;
  pub async fn install(app: AppHandle) -> Result<(), String>;
  pub fn later(app: &AppHandle);                     // dismiss the pill for this run
  pub fn skip(app: &AppHandle) -> Result<(), String>; // svc.updates.skipped_version = the offered version
  pub fn tray_item(app: &AppHandle) -> tauri::Result<MenuItem<Wry>>; // id "svc:update"
  pub fn tray_clicked(app: &AppHandle);              // open_settings_at(app, "about", None); a manual check unless busy or an update is offered
  pub fn settings_changed(app: &AppHandle);          // re-emit the view (pill setting, skipped version)
  ```
  Runtime rules: `check` is a no-op while Checking, Downloading or Installing. It builds `app.updater_builder().timeout(CHECK_TIMEOUT).build()` and calls `check()`: `Some(u)` → `Available { version: u.version, notes: u.body.unwrap_or_default() }` (the `Update` is kept); `None` → `UpToDate`; `Err(e)` → `Failed { phase: Check, reason: one_line(&e.to_string()) }`, returned as `Err` only to a manual caller. A failure never shows a card or marks the tray; it is visible only in the About pane. `install`: from Available or a Failed download/install, `Downloading` with progress (emit at most every 250 ms), keep the bytes; then if no job is busy (`AppState.jobs` entries with `is_busy()`, counted here; `state.rs` is not changed) install at once, else `Ready { busy }`; called again from Ready it installs whatever is busy. Installing: `update.install(bytes)`, then `app.request_restart()` (on Windows `install` exits the process itself). Every state change emits `svc:update` with `view(app)` to all windows, and a change of state kind calls `tray::rebuild`. The scheduler thread sleeps `FIRST_CHECK_AFTER`, then every 60 s runs `tauri::async_runtime::block_on(check(app, false))` when `check_due` says so. Log every check, result and failure with `tracing`.
- Commands in `svc_commands.rs`: `svc_update_state() -> UpdateView` (sync); `async svc_update_check() -> Result<(), String>` (manual); `async svc_update_install() -> Result<(), String>`; `svc_update_later()`; `#[tauri::command(async)] svc_update_skip() -> Result<(), String>`; `svc_open_release() -> Result<(), String>` (opener `open_url(release_url(version))`); `svc_open_about()` (`open_settings_at(app, "about", None)`). `svc_commands::setup` calls `updater::init`.
- `prefs.rs`: `Effects.updates = old.svc.updates != new.svc.updates`; `apply` calls `crate::updater::settings_changed(app)`.
- `svc.ts`: `UpdateState`, `UpdateView` mirrors.

- [ ] **Step 1: Add the dependency and check licences.** Add `tauri-plugin-updater = "2.14.0"` to `src-tauri/Cargo.toml` (anchor in the table) and run `cargo build -p ferry`. Then list the new packages' licences:
  ```bash
  git diff Cargo.lock | sed -n 's/^+name = "\(.*\)"/\1/p' | sort -u > /tmp/sg-new.txt
  cargo metadata --format-version 1 | python3 -c "import sys,json;n=set(open('/tmp/sg-new.txt').read().split());[print(p['name'],p['version'],p['license']) for p in json.load(sys.stdin)['packages'] if p['name'] in n]"
  ```
  Expected: the crates and licences in Global Constraints; no `GPL`, `LGPL` or `AGPL`. If anything else appears, stop and report.
- [ ] **Step 2: Write the failing tests** in `updater.rs`:
  - `the_pill_shows_only_for_an_offered_update`: Available + not skipped + not dismissed + setting on → true; skipped, dismissed or setting off → false; Downloading and Ready → true; Idle, Checking, UpToDate, Installing, Failed → false.
  - `a_skipped_version_stays_quiet`: `is_skipped("0.3.0", "0.3.0")` true; `("0.3.1", "0.3.0")` false; `("0.3.0", "")` false; and `tray_label(&Available{"0.3.0"}, true).0 == t("svc.tray.check")`, `tray_label(&Available{"0.3.0"}, false).0 == tf("svc.tray.update_to", &[("version", "0.3.0")])`.
  - `checks_wait_a_minute_then_run_every_six_hours`: `check_due(30s, None, true)` false; `(61s, None, true)` true; `(7h, Some(5h), true)` false; `(7h, Some(6h + 1s), true)` true; anything with `auto = false` false.
  - `automatic_failures_stay_quiet`: for `Failed { phase: Check, .. }`, `tray_label` is `(t("svc.tray.check"), true)` and `pill_visible` is false with every other argument favourable.
  - `one_line_keeps_the_first_line_only`: `"error sending request\ncaused by: dns"` → `"error sending request"`; a 300-char line → 160 chars ending in `…`.
  - `release_urls_point_at_the_tag`: `release_url("0.3.0") == "https://github.com/Huy31203/ferry/releases/tag/v0.3.0"`.
  - `the_tray_item_is_disabled_while_busy`: Downloading and Installing → `(t("svc.tray.downloading"), false)`.
  - in `prefs.rs`: `an_update_setting_change_reaches_the_updater`: `svc.updates.pill` change → `Effects { updates: true, ..Effects::default() }`.
- [ ] **Step 3: Run them and see them fail.** Run: `cargo test -p ferry updater`. Expected: compile errors.
- [ ] **Step 4: Implement** `updater.rs`, both config files, the plugin line in `lib.rs`, the tray edits, the prefs effect, the commands, the `svc.tray.*` strings and the TS mirrors.
- [ ] **Step 5: Run the tests.** Run: `cargo test -p ferry updater`, `cargo test -p ferry prefs`, `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm check`. Expected: PASS and clean.
- [ ] **Step 6: Check that a signed build makes updater artifacts** (no user secrets needed; a throwaway key, deleted after):
  ```bash
  S=$(mktemp -d)
  pnpm tauri signer generate --ci -p "" -w "$S/test.key"
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$S/test.key")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    pnpm tauri build --bundles app --config src-tauri/tauri.updater.conf.json
  ls target/release/bundle/macos/
  pnpm tauri build --bundles app   # without the extra config and with no key in the environment
  rm -rf "$S"
  ```
  Expected: the first build leaves `Ferry.app`, `Ferry.app.tar.gz` and `Ferry.app.tar.gz.sig`; the second succeeds without any signing key.
- [ ] **Step 7: Manual check on this Mac** (ask the user first; `pnpm tauri dev`): the tray menu shows "Check for updates…" between "Settings…" and "About Ferry". Click it. Look for: Settings opens at About and updates (empty until Task 11 except the pane title), and the log (`~/Library/Logs/io.ferry.desktop/ferry.log`) records one manual check ending in a failure (no signed release yet) with a one-line reason. The app keeps running.
- [ ] **Step 8: Commit.**
  ```bash
  git add src-tauri/src/updater.rs src-tauri/tauri.updater.conf.json src-tauri/Cargo.toml Cargo.lock src-tauri/tauri.conf.json src-tauri/src/lib.rs src-tauri/src/tray.rs src-tauri/src/prefs.rs src-tauri/src/svc_commands.rs src/lib/svc.ts src/lib/i18n/en.json
  git commit -m "feat(app): check for, download and install signed updates from GitHub Releases"
  ```

---

### Task 11: The About and updates pane and the stack pill

**Files:**
- Create: `src/lib/updates.ts`, `src/lib/updates.test.ts`, `src/settings/controls/UpdatePanel.svelte`, `src/stack/UpdatePill.svelte`
- Modify: `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src/stack/Stack.svelte`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 10's commands and `svc:update` event; `formatBytes` from `src/lib/format.ts`.
- Produces in `updates.ts`:
  ```ts
  export type UpdateButton = 'check' | 'install' | 'later' | 'skip' | 'retry' | 'release';
  export interface UpdateText { line: StringKey; vars: Record<string, string | number>; buttons: UpdateButton[]; notes: string | null; busy: boolean }
  export function updateText(view: UpdateView): UpdateText;
  ```
  Mapping: Idle → `svc.updates.idle`, [check]; Checking → `svc.updates.checking`, [], busy; UpToDate → `svc.updates.up_to_date`, [check]; Available → `svc.updates.available` {version}, [install, later, skip, release], notes (`svc.updates.no_notes` text when empty); Available + skipped → same line, plus the skipped note shown by the panel, [install, release]; Downloading → `svc.updates.downloading` {version, percent: "42 %"} or `svc.updates.downloading_bytes` {version, size} when the total is unknown, [], busy; Ready → `svc.updates.ready` {version} (and `svc.updates.ready_busy` {count} shown by the panel when busy > 0), [install, later]; Installing → `svc.updates.installing`, [], busy; Failed → `svc.updates.failed.<phase>` {why: reason}, [retry] (retry = check for `check`, install otherwise).
- Schema rows (right after `svc_update_pill`): `{ id: 'svc_update', pane: 'about', label: 'svc.updates.version', control: { kind: 'update' }, wide: true }` and `{ id: 'svc_auto_check', pane: 'about', label: 'svc.updates.auto', help: 'svc.updates.auto.help', path: 'svc.updates.auto_check', control: { kind: 'switch' } }`.
- `UpdatePanel.svelte`: shows `svc.updates.current` {version: view.current}, the `updateText` line (with `role="status"`), the buttons, a progress bar while downloading (`<progress>` with `aria-label`), and the release notes as **plain text** in a scrollable box (`white-space: pre-wrap`, max-height 180 px; never `{@html}`). It loads `svc_update_state` on mount and follows `svc:update`.
- `UpdatePill.svelte` props `{ view: UpdateView; hotAction: string | null }`: a small rounded button, `data-action="update-pill"`, text `svc.pill.label`, `aria-label` `svc.pill.aria` {version}, click → `invoke('svc_open_about')`. `Stack.svelte` keeps `let update = $state<UpdateView | null>(null)`, loads it with `svc_update_state` in `onMount` and follows `svc:update`; it renders the pill when `cards.length > 0 && update?.pill`, at the screen-edge end of the stack, outside the Drag all bar: after the bottom `DragAllBar` block for bottom corners, before the top one for top corners.

- [ ] **Step 1: Write the failing tests** in `updates.test.ts`, one `it` per state in the mapping above (lines, vars and buttons exactly as listed; Downloading with `received: 42, total: 100` gives `percent: '42 %'`; with `total: null` and `received: 1_500_000` gives `size: formatBytes(1_500_000)`), plus `it('never offers a button while busy')`. In `schema.test.ts`: the `about` pane's rows are `['svc_update', 'svc_auto_check']`.
- [ ] **Step 2: Run them and see them fail.** Run: `pnpm test -- updates`. Expected: FAIL. Add the remaining `svc.updates.*` and `svc.pill.*` strings.
- [ ] **Step 3: Implement** `updates.ts`, the two components, the rows, the Row branch (`{:else if control.kind === 'update'} <UpdatePanel />`) and the Stack edits.
- [ ] **Step 4: Run the tests.** Run: `pnpm test`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Manual check on this Mac** (with the user's OK; `pnpm tauri dev`):
  1. Settings ▸ About and updates. Look for: "Ferry 0.2.0", "Not checked yet." and Check now. Click Check now. Look for: "Checking for updates…", then "Couldn't check for updates: …" in one line with "Try again" (there is no signed release yet). Turn the Mac's Wi-Fi off and click Try again: the same kind of line, and the app stays responsive.
  2. Toggle "Check for updates automatically" off and on: no error; the log shows no automatic check (debug build).
  3. The pill and the Available/Downloading states are checked end to end in Task 15.
- [ ] **Step 6: Commit.**
  ```bash
  git add src/lib/updates.ts src/lib/updates.test.ts src/settings/controls/UpdatePanel.svelte src/stack/UpdatePill.svelte src/lib/schema.ts src/lib/schema.test.ts src/settings/Row.svelte src/stack/Stack.svelte src/lib/i18n/en.json
  git commit -m "feat(ui): About and updates pane with the update prompt, and an Update available pill"
  ```

---

### Task 12: Signed updater artifacts from the release workflow

**Files:**
- Modify: `.github/workflows/release.yml`

**Interfaces:**
- Consumes: Task 10's `src-tauri/tauri.updater.conf.json` and the `plugins.updater` block.
- Changes (`ci.yml` does **not** change: its builds never pass the updater config, so they need no key):
  1. Matrix: the macOS entry's `bundles` becomes `app,dmg`.
  2. A step before `tauri-action`, named `Check updater signing`:
     ```yaml
     - name: Check updater signing
       shell: bash
       env:
         TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}
       run: |
         if [ -z "$TAURI_SIGNING_PRIVATE_KEY" ]; then
           echo "::error::Add the TAURI_SIGNING_PRIVATE_KEY and TAURI_SIGNING_PRIVATE_KEY_PASSWORD repository secrets"; exit 1
         fi
         if grep -q 'REPLACE_WITH_UPDATER_PUBLIC_KEY' src-tauri/tauri.conf.json; then
           echo "::error::Put the updater public key in src-tauri/tauri.conf.json (plugins.updater.pubkey)"; exit 1
         fi
     ```
  3. `tauri-action` env gains `TAURI_SIGNING_PRIVATE_KEY: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY }}` and `TAURI_SIGNING_PRIVATE_KEY_PASSWORD: ${{ secrets.TAURI_SIGNING_PRIVATE_KEY_PASSWORD }}`; `with:` gains `uploadUpdaterJson: true` and `uploadUpdaterSignatures: true`; `args` becomes `--target ${{ matrix.target }} --bundles ${{ matrix.bundles }} --config src-tauri/tauri.updater.conf.json`. `releaseBody` stays as it is (it becomes `latest.json`'s `notes`; see open question 3).
  4. A new job:
     ```yaml
     updater-manifest:
       needs: release
       runs-on: ubuntu-latest
       steps:
         - name: Check latest.json lists every platform, signed
           env:
             GH_TOKEN: ${{ secrets.GITHUB_TOKEN }}
           run: |
             gh release download "$GITHUB_REF_NAME" --repo "$GITHUB_REPOSITORY" --pattern latest.json --dir manifest
             jq -e --arg v "${GITHUB_REF_NAME#v}" '. as $m
               | ($m.version | ltrimstr("v")) == $v
               and (["darwin-aarch64", "darwin-x86_64", "windows-x86_64"]
                    | all(. as $p | (($m.platforms[$p].signature // "") != "") and (($m.platforms[$p].url // "") != "")))' \
               manifest/latest.json
     ```
     If it fails because one platform is missing, re-running that platform's `release` job merges it back in.

- [ ] **Step 1: Confirm the inputs** against the action at its pinned tag: `curl -sS https://raw.githubusercontent.com/tauri-apps/tauri-action/v1/action.yml | grep -n -A2 -E 'uploadUpdaterJson|uploadUpdaterSignatures|args:'`. Expected: both inputs exist with default `'true'`.
- [ ] **Step 2: Edit `release.yml`** as above.
- [ ] **Step 3: Lint the workflow.** Run: `python3 -c "import yaml,sys;yaml.safe_load(open('.github/workflows/release.yml'))"` and, if installed, `actionlint .github/workflows/release.yml`. Expected: no error. Run the `jq` expression on a sample file (write `{"version":"0.3.0","platforms":{"darwin-aarch64":{"signature":"s","url":"u"},"darwin-x86_64":{"signature":"s","url":"u"},"windows-x86_64":{"signature":"s","url":"u"}}}` to a temp file): exit 0; drop `windows-x86_64`: exit 1.
- [ ] **Step 4: Manual check (needs the user's secrets, so it waits for the first release after the "User actions"):** after the user pushes a tag, look for: both `release` jobs green, the `updater-manifest` job green, and on the release page `Ferry.app.tar.gz`, `Ferry.app.tar.gz.sig`, `Ferry_<v>_x64-setup.exe`, `Ferry_<v>_x64-setup.exe.sig`, `Ferry_<v>_universal.dmg` and `latest.json`. Write this list into the commit body.
- [ ] **Step 5: Commit.**
  ```bash
  git add .github/workflows/release.yml
  git commit -m "ci: release signed updater artifacts and check latest.json covers every platform"
  ```

---

### Task 13: The sync worker

**Files:**
- Create: `src-tauri/src/sync_worker.rs`
- Modify: `src-tauri/src/prefs.rs`, `src-tauri/src/svc_commands.rs`, `src-tauri/src/lib.rs` (`generate_handler!`), `src/lib/svc.ts`
- Test: `src-tauri/src/sync_worker.rs` `mod tests`, `src-tauri/src/prefs.rs` `mod tests`

**Interfaces:**
- Consumes: Task 2's `settings_sync::*`; `ferry_core::fsutil::atomic_replace`; `settings::{expand_home, contract_home}`; `prefs::update`; `trash::delete`.
- Produces:
  ```rust
  pub const POLL_EVERY: Duration = Duration::from_secs(15);
  pub const SETTLE: Duration = Duration::from_secs(2);   // a burst of local changes is written once
  #[derive(Debug, Clone, PartialEq, Eq, Serialize)] #[serde(tag = "kind", rename_all = "snake_case")]
  pub enum SyncStatus { Off, Starting, Synced { at: i64, devices: usize }, FolderMissing, NewerFormat { device: String }, Failed { reason: String } }
  pub fn start(app: &AppHandle);            // manage SyncWorker { tx: Mutex<Sender<Msg>>, status: Mutex<SyncStatus> }, spawn the thread
  pub fn local_changed(app: &AppHandle);    // cheap signal; no-op before start
  pub fn reconfigure(app: &AppHandle);
  pub fn sync_now(app: &AppHandle);
  pub fn status(app: &AppHandle) -> SyncStatus;
  pub fn inspect(folder: &Path, own_device: Option<&str>) -> FolderInfo;
  pub fn prepare_join(app: &AppHandle, folder: &str, mode: JoinMode) -> Result<(), String>; // writes a fresh ledger (Ledger::join) to disk, keeping the stored device id when its host still matches
  // IO helpers, tested with tempfile
  pub(crate) struct Cache;                  // file name → (len, mtime, DeviceFile)
  pub(crate) struct FolderRead { pub files: Vec<(PathBuf, DeviceFile)>, pub newer: Option<String>, pub unreadable: usize }
  pub(crate) fn read_folder(dir: &Path, cache: &mut Cache) -> FolderRead;
  pub(crate) fn write_own(dir: &Path, file: &DeviceFile) -> io::Result<()>;
  pub(crate) fn conflict_copies(files: &[(PathBuf, DeviceFile)]) -> Vec<PathBuf>;
  pub(crate) fn load_ledger(path: &Path) -> Option<Ledger>;
  pub(crate) fn save_ledger(path: &Path, ledger: &Ledger) -> io::Result<()>;
  fn device_name() -> String; // macOS: `scutil --get ComputerName`, else "Mac"; #[cfg(windows)]: env COMPUTERNAME, else "Windows PC"; other: "Computer"
  ```
  The ledger lives at `settings_path.with_file_name("sync-state.json")`. Device ids come from `new_device_id(RandomState::new().hash_one((SystemTime::now(), std::process::id())))`; if the stored `host` differs from `device_name()` (a copied config on a new computer), a new id is made.
- `read_folder` rules: list `dir`; skip names starting with `.`, names ending `.ferry-tmp`, non-`.json` names, files over `MAX_FILE_BYTES` (count as unreadable); reuse the cached parse when length and mtime are unchanged; on `ReadError::Corrupt` keep the cached copy if there is one, otherwise count as unreadable; `NewerFormat` sets `newer` to the device name (or the file stem) and the file is left out.
- The worker loop, one cycle (on `POLL_EVERY`, `SyncNow`, `Reconfigure`, or `SETTLE` after the last `LocalChanged`):
  1. Read `svc.sync`. Off → status `Off`, stop here. The chosen folder (expanded) missing → `FolderMissing`, stop. Create `sync_dir(chosen)` if missing.
  2. Load the ledger (missing → `Ledger::join(.., JoinMode::UseFolder, ..)` and log it). `record_local(current settings, OS, now)`.
  3. `read_folder`. Remotes = every file except our canonical `<device>.json`.
  4. `merge(remotes)`. If anything was adopted: `prefs::update(app, |s| { let (next, skipped) = apply(s, &adopted, OS); *s = next; keep skipped })`, then `ledger.revert(&skipped, &adopted)` and `record_local` again (catches sanitising).
  5. Write our device file with `write_own` when its entries differ from the last one written or the file is missing. Move every `conflict_copies` path to the Trash (log and carry on if that fails). `save_ledger`.
  6. Status `Synced { at: now, devices }` (distinct other device ids), or `NewerFormat`, or `Failed { reason }` for an IO error (one line). Emit `svc:sync` with the status to the settings window.
  The ledger is owned by the worker thread only; it is never borrowed across `prefs::update`.
- `prefs.rs`: `Effects.sync = old.svc.sync != new.svc.sync`; `apply` calls `crate::sync_worker::reconfigure(app)`; `update` calls `crate::sync_worker::local_changed(app)` after the emit.
- Commands (`svc_commands.rs`): `svc_sync_status() -> SyncStatus`; `async svc_sync_inspect(folder: String) -> Result<FolderInfo, String>` (on a blocking worker); `#[tauri::command(async)] svc_sync_enable(folder: String, join: JoinMode) -> Result<(), String>` (`prepare_join`, then `prefs::update` setting `enabled = true, folder`); `#[tauri::command(async)] svc_sync_disable() -> Result<(), String>`; `svc_sync_now()`. `svc_commands::setup` calls `sync_worker::start`.
- `svc.ts`: `SyncStatus`, `DeviceSummary`, `FolderInfo`, `JoinMode` mirrors.

- [ ] **Step 1: Write the failing tests** in `sync_worker.rs` (temp dirs):
  - `read_folder_skips_hidden_temp_and_oversized_files`: a valid `d1.json`, `.d2.json`, `d3.json.ferry-tmp`, `notes.txt` and a 2 MiB `d4.json` → one file, `unreadable == 1`.
  - `a_half_written_file_keeps_the_last_good_copy`: read a valid `d1.json`, overwrite it with its first 20 bytes (and a later mtime), read again → still one file with the first parse's entries; with an empty cache instead → no file, `unreadable == 1`.
  - `a_newer_format_is_named`: a file with `format_version: 2, device_name: "PC"` → `newer == Some("PC")`, no file.
  - `write_own_replaces_atomically`: two writes leave one `<device>.json` that parses to the second file, and no `.ferry-tmp`.
  - `conflict_copies_are_picked_out`: files `d1.json` (device d1) and `d1 (conflicted copy).json` (device d1) → only the second.
  - `the_ledger_round_trips`: `save_ledger` then `load_ledger` returns it; a corrupt file → `None`.
  - `inspect_lists_other_devices`: two device files plus ours → two summaries, ours left out; a missing folder → `exists: false`.
  - in `prefs.rs`: `a_sync_setting_change_reconfigures_the_worker`: `svc.sync.folder` change → `Effects { sync: true, ..Effects::default() }`.
- [ ] **Step 2: Run them and see them fail.** Run: `cargo test -p ferry sync_worker`. Expected: compile errors.
- [ ] **Step 3: Implement** the worker, the prefs hooks, the commands and the TS mirrors.
- [ ] **Step 4: Run the tests.** Run: `cargo test -p ferry sync_worker`, `cargo test -p ferry prefs`, `cargo clippy --workspace --all-targets -- -D warnings`. Expected: PASS and clean. Note in the commit message that the `#[cfg(windows)]` branch of `device_name` is CI-verified only.
- [ ] **Step 5: Commit.**
  ```bash
  git add src-tauri/src/sync_worker.rs src-tauri/src/prefs.rs src-tauri/src/svc_commands.rs src-tauri/src/lib.rs src/lib/svc.ts
  git commit -m "feat(app): a sync worker that keeps settings in step through a shared folder"
  ```

---

### Task 14: Sync settings in the General pane

**Files:**
- Create: `src/lib/sync.ts`, `src/lib/sync.test.ts`, `src/settings/controls/SyncControl.svelte`
- Modify: `src/lib/schema.ts`, `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: Task 13's commands and `svc:sync` event; the existing `pick_folder` command (answers a `~` path or null).
- Produces in `sync.ts`:
  ```ts
  export function syncLine(status: SyncStatus, now: number): { key: StringKey; vars: Record<string, string | number> } | null; // null for 'off'
  export function needsJoin(info: FolderInfo): boolean;  // other devices present
  export function joinSummary(info: FolderInfo): { devices: string; time: string }; // names joined with ", ", newest written_at as a locale time
  ```
  `syncLine`: starting → `svc.sync.status.starting`; synced with 0 / 1 / n devices → `svc.sync.status.synced` / `.synced_with_one` / `.synced_with` with `time` = `new Date(at).toLocaleTimeString()` (or `toLocaleString()` when not today) and `count`; folder_missing → `svc.sync.status.missing`; newer_format → `svc.sync.status.newer` {device}; failed → `svc.sync.status.failed` {why: reason}.
- Schema row, right after the `force_clean` row: `{ id: 'svc_sync', pane: 'general', group: 'svc.group.sync', label: 'svc.sync.enabled', help: 'svc.sync.enabled.help', path: 'svc.sync.enabled', control: { kind: 'sync' }, wide: true }`.
- `SyncControl.svelte`: a switch (`aria-labelledby`), the folder path (or `svc.sync.no_folder`), "Choose folder…", "Sync now" (only when on), and the `syncLine` status (`role="status"`). Turning on without a folder, or choosing a folder: `pick_folder` → `svc_sync_inspect` → if `needsJoin`, an inline choice titled `svc.sync.join.title` with `svc.sync.join.from` and three buttons (use folder / use this / cancel), else `svc_sync_enable(folder, 'use_this')`. Turning off: `svc_sync_disable`. It loads `svc_sync_status` on mount and follows `svc:sync`. `Row.svelte`: `{:else if control.kind === 'sync'} <SyncControl {disabled} labelledby={labelId} />`.

- [ ] **Step 1: Write the failing tests** in `sync.test.ts`: every `syncLine` case above (with `at` fixed and `now` the same day); `needsJoin({exists: true, devices: [], newer_format: false})` false and with one device true; `joinSummary` joins two names newest first. In `schema.test.ts`: the `general` pane's last row is `svc_sync` in group `svc.group.sync`.
- [ ] **Step 2: Run them and see them fail.** Run: `pnpm test -- sync`. Expected: FAIL. Add the `svc.sync.*` and `svc.group.sync` strings.
- [ ] **Step 3: Implement** `sync.ts`, `SyncControl.svelte`, the row and the Row branch.
- [ ] **Step 4: Run the tests.** Run: `pnpm test`, `pnpm check`. Expected: PASS.
- [ ] **Step 5: Manual check on this Mac** (with the user's OK; two Ferry configs are simulated with a second device file, because only one Mac is available):
  1. Settings ▸ General ▸ Sync: turn on, choose `~/Library/Mobile Documents/com~apple~CloudDocs` (iCloud Drive) or a plain test folder. Look for: "Last synced …" within a few seconds and `<folder>/Ferry Settings/d….json` containing `images.compression` and `@macos/hotkeys.optimise` but no `svc.sync` and no `general.launch_at_login`.
  2. Change Images ▸ Compression to 60. Within ~2 s the device file shows `images.compression` `{"v": 60, …}`.
  3. Copy the device file to `other.json` in the same folder; edit it: `"device": "dzz…"` (16 hex digits), `"device_name": "Test PC"`, `"os": "windows"`, `images.compression` `{"v": 40, "t": <now ms + 60000>, "d": "dzz…"}`, and add `"@windows/hotkeys.optimise": {"v": "Ctrl+Alt+Shift+X", "t": <same>, "d": "dzz…"}`. Within 15 s (or press Sync now), look for: Compression reads 40 in Settings, the status says "with 1 other computer", and Hotkeys ▸ Optimise clipboard still reads ⌃⇧C.
  4. Truncate `other.json` to half its length. Look for: nothing in Settings changes; the status stays "Last synced …".
  5. Set `other.json`'s `format_version` to 2. Look for: "Test PC uses a newer Ferry. Update Ferry to sync with it."
  6. Turn sync off, delete the folder's contents, turn it on choosing the same folder after putting `other.json` (valid, version 1) back. Look for: the "This folder already holds Ferry settings" choice, "From Test PC, last changed …"; "Use this computer's settings" keeps the current values.
  7. Rename the chosen folder in Finder. Look for: "Folder not found. Ferry keeps trying." and no folder re-created. Rename it back: it syncs again.
- [ ] **Step 6: Commit.**
  ```bash
  git add src/lib/sync.ts src/lib/sync.test.ts src/settings/controls/SyncControl.svelte src/lib/schema.ts src/lib/schema.test.ts src/settings/Row.svelte src/lib/i18n/en.json
  git commit -m "feat(ui): sync settings through a chosen folder, from the General pane"
  ```

---

### Task 15: Stream verification and an end-to-end update on this Mac

**Files:** none changed unless a check fails (then fix in the owning task's files and commit).

- [ ] **Step 1: Full checks.** Run: `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test -p ferry-core`, `cargo test -p ferry`, `pnpm check`, `pnpm test`. Expected: all clean and passing. `git diff --stat feat/c3-all` (or the base branch) shows only files listed in this plan, and `package.json` / `pnpm-lock.yaml` unchanged.
- [ ] **Step 2: Manual check, a real update with a throwaway key and a local server** (ask the user first; quit the user's own Ferry, because the test copy shares its settings folder):
  ```bash
  S=$(mktemp -d); pnpm tauri signer generate --ci -p "" -w "$S/k"
  PUB=$(cat "$S/k.pub")
  cat > "$S/old.json" <<EOF
  {"version":"0.0.1","plugins":{"updater":{"pubkey":"$PUB","endpoints":["http://127.0.0.1:8765/latest.json"],"dangerousInsecureTransportProtocol":true}}}
  EOF
  pnpm tauri build --bundles app --config "$S/old.json"
  ditto target/release/bundle/macos/Ferry.app "$HOME/Applications/Ferry.app"
  sed 's/"0.0.1"/"0.0.2"/' "$S/old.json" > "$S/new.json"
  TAURI_SIGNING_PRIVATE_KEY="$(cat "$S/k")" TAURI_SIGNING_PRIVATE_KEY_PASSWORD="" \
    pnpm tauri build --bundles app --config "$S/new.json" --config src-tauri/tauri.updater.conf.json
  mkdir "$S/www" && cp target/release/bundle/macos/Ferry.app.tar.gz "$S/www/"
  ARCH=$( [ "$(uname -m)" = arm64 ] && echo aarch64 || echo x86_64 )
  printf '{"version":"0.0.2","notes":"Test notes\\nSecond line","pub_date":"2026-10-23T00:00:00Z","platforms":{"darwin-%s":{"signature":"%s","url":"http://127.0.0.1:8765/Ferry.app.tar.gz"}}}' \
    "$ARCH" "$(cat target/release/bundle/macos/Ferry.app.tar.gz.sig)" > "$S/www/latest.json"
  (cd "$S/www" && python3 -m http.server 8765)
  ```
  Open `~/Applications/Ferry.app` (0.0.1) and wait 60 s, or use Settings ▸ About and updates ▸ Check now. Look for, in order:
  1. Copy a screenshot: below the card, the "Update available" pill. The tray item reads "Update to Ferry 0.0.2…".
  2. Click the pill: Settings opens at About and updates with "Ferry 0.0.2 is available.", the notes "Test notes / Second line" as two plain lines, and Install and restart, Later, Skip this version, View on GitHub.
  3. Later: the pill goes; the tray item still offers 0.0.2. Skip this version: the tray item is back to "Check for updates…"; Check now shows 0.0.2 again with "You skipped this version."
  4. Install and restart: a progress line, then Ferry quits and comes back by itself. About shows "Ferry 0.0.2". macOS does not ask "Are you sure you want to open it?". The tray icon is there once (not twice).
  5. Stop the server, run `xattr -l ~/Applications/Ferry.app` and note whether `com.apple.quarantine` is absent. Then quit the test copy, remove `~/Applications/Ferry.app` and `$S`, and start the user's own Ferry again: a release build rewrites the login item at every start, and the test copy pointed it at `~/Applications`.
  Record each result in the ledger.
- [ ] **Step 3: Windows.** The Windows half (passive NSIS install and relaunch, the `COMPUTERNAME` device name) runs only in CI and on a real Windows desktop later. Ask the user before pushing the branch so CI's `bundle (windows)` job runs; record its result. State in the hand-off that Windows update install is built but not verified on a desktop (C1 §1).
- [ ] **Step 4: Hand-off note** in the ledger and the final message: the user actions (keys, secrets, pubkey, public Releases), the cross-stream hooks (sync `LOCAL`/`PER_OS` prefixes for C and H, rail catalog, hotkey bindings), and the deferred manual check of Task 12.
