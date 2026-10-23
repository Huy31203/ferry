# Ferry Cycle 3, Stream H (integrations): Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Hand a card's file to the rest of the system and take files in from it: Share (system share sheet, AirDrop, Nearby Share), Open With ▸ and Edit with ⟨app⟩ (⌘E / Ctrl+E), Upload with Dropshare (⌘U / Ctrl+U), Add to Shelf ▸, plus "Optimise with Ferry" in Finder / Explorer and Ferry as an "Open with" target.

**Architecture:** Pure logic (file-type table, launch-argument parsing, app ordering, known-app detection, `open` arguments, anchor maths) lives in a new `ferry_core::integrations` module and a new `share` settings section, tested on both CI OSes. The app crate gets one new module tree, `src-tauri/src/integrations/` (launch routing, card-menu items, actions, commands), and per-OS native modules under `src-tauri/src/platform/` that follow the existing `#[path = "macos_*.rs"]` / `#[path = "windows_*.rs"]` pattern. Files reach the running app through `RunEvent::Opened` (macOS Open With), an NSServices provider (macOS Finder Services), and `tauri-plugin-single-instance` args (Windows Explorer verb and Open with, forwarded from a second process).

**Tech Stack:** Rust 2024, Tauri 2.12.4, objc2 0.6.5 / objc2-app-kit 0.3.2 / objc2-foundation 0.3.2 (macOS), windows 0.62.2 + windows-collections 0.3.2 (Windows, new direct deps, already in `Cargo.lock`), NSIS installer hooks, Svelte 5 + TypeScript, vitest.

**Spec:** [Cycle 3 stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding; wins over the requirements), [requirements](../../requirements/ferry-optimize-requirements.md) R§2.5 "Hand off", R§2.8, R§2.10, R§4.6, R§4.12 Sharing pane, R§5 rows "Open With and context menu" and "Share and nearby send", FR-T9. Architecture background: [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md) §4.3–4.4, [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md) §4.5.

## Decisions (and why)

| Topic | Decision | Why |
| --- | --- | --- |
| macOS share sheet | `NSSharingServicePicker::initWithItems` + `showRelativeToRect_ofView_preferredEdge` on the stack window's content view (`WebviewWindow::ns_view()`), anchored to the card's `getBoundingClientRect()` | Brief. The card menu sends `card:action` `share` to the page (existing pattern for rename/crop), the page measures the card and calls `card_share` |
| AirDrop | `NSSharingService::sharingServiceNamed(NSSharingServiceNameSendViaAirDrop)`, `canPerformWithItems` then `performWithItems` | Named service, no picker needed |
| Focus around share UI | Activate Ferry first (as Quick Look does, `platform::preview::remember_front` + `activateIgnoringOtherApps`), hand focus back from the picker/service delegate. A `const ACTIVATE_FOR_SHARE: bool = true` fallback switch, like the existing `ACTIVATE_FOR_KEYS` | Ferry is an accessory app behind a non-activating panel; service windows (AirDrop, Mail) would open behind the frontmost app otherwise |
| Windows share | `windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()`, `GetForWindow(stack hwnd)`, `DataRequested` sets title + `SetStorageItemsReadOnly`, then `ShowShareUIForWindow`. Nearby Share is a row inside that UI | Brief; Win32 desktop apps reach the share UI only through the interop interface |
| Listing "Open With" apps | macOS `NSWorkspace::URLsForApplicationsToOpenURL` + `URLForApplicationToOpenURL` (default); Windows `SHAssocEnumHandlers(".ext", ASSOC_FILTER_RECOMMENDED)` | Brief; both are the OS's own "Open With" lists |
| Launching an app with a file | macOS `/usr/bin/open -b <bundle id> <file>` (or `-a <path>` when the app has no bundle id), args passed as separate `Command` args, never a shell. Windows: an `.exe` path is spawned with the file as its one argument; any other id is matched against `IAssocHandler::GetName` and `Invoke`d | `open` already handles activation, running instances and documents; no `block2` completion handlers needed. `IAssocHandler::Invoke` also covers packaged apps (Photos) that have no exe path |
| Icons | macOS `NSWorkspace::iconForFile` drawn into a 32×32 `NSImage` (`lockFocus`), PNG via `NSBitmapImageRep`, `tauri::image::Image::from_bytes`. Windows `SHDefExtractIconW(exe, 0, …, 32)` → `GetIconInfo` → `GetDIBits` (32 bpp top-down) → BGRA→RGBA. Cached per app id for the session | `IconMenuItem` takes a `tauri::image::Image` |
| Finder integration | **NSServices** "Optimise with Ferry" + `CFBundleDocumentTypes` (role `Viewer`, `LSHandlerRank` `Alternate`) in a new `src-tauri/Info.plist`, which the Tauri bundler merges automatically. No Finder Sync or Action extension | App extensions need a separately built, signed `.appex`, which the Tauri bundler does not produce and signing is out of scope. Services and document types work unsigned. `Alternate` rank means Ferry is never made the default app |
| Explorer integration | NSIS `installerHooks` writing **HKCU** keys: a static verb under `SystemFileAssociations\.<ext>\shell\Ferry.Optimise`, ProgID `Ferry.File`, and `.<ext>\OpenWithProgids`. No `bundle.fileAssociations` | Tauri's own `fileAssociations` NSIS macro (`APP_ASSOCIATE`) writes the default value of `HKCU\Software\Classes\.<ext>`, which would make Ferry the default app for PNG. `IExplorerCommand` / the Windows 11 top-level menu needs a sparse package with identity, which needs signing (out of scope), so the verb appears under "Show more options" on Windows 11 |
| How the invocation reaches the app | macOS: `RunEvent::Opened { urls }` (Open With, Dock drops) and the Services provider. Windows: the command `"…\Ferry.exe" --optimise "%1"`. A cold start reads its own args in `setup`, and a warm start's second process is forwarded by the existing `tauri-plugin-single-instance` callback | Single-instance is already the first plugin; its callback gets `args` and `cwd` |
| What an incoming file does | Starts a normal file job (`Job::file` + `jobs::start`, level Normal), whatever the run state | Same path as the optimise hotkey's Finder-copied files; an explicit user action, like hotkeys (C1§5.1) |
| Dropshare | "Upload with ⟨app⟩" = open the file with the uploader app. Uploader = `share.uploader` setting, else Dropshare when found on macOS. Hidden when there is none | Dropshare uploads documents opened with it (Dock drop / Open With). No undocumented URL scheme to depend on. On Windows the user may choose any uploader (for example ShareX) in Settings |
| Shelf apps | (1) Drag-out already works: `tauri-plugin-drag` starts a real file-URL drag session, which Yoink, Dropover and Dockside watch for. No code needed, manual check only. (2) "Add to Shelf ▸" = open the file with that shelf app. Known apps (Yoink, Dropover, Dockside) are found by bundle id or by bundle name in `/Applications`, `/Applications/Setapp`, `~/Applications`, plus the ones listed in `share.shelf_apps` | File promises are not needed: every card already has a real file on disk. `spike/drag` is unmerged and is not used |
| Edit with | Per media kind (`image`, `video`, `audio`, `pdf`) in `share.edit_with`. When set: menu item "Edit with ⟨app⟩ ⌘E". When unset: an "Edit With ▸" submenu of the Open With apps; picking one saves it as that kind's editor and opens the file. ⌘E with no editor set opens the file in the default app | No file dialog from the non-activating stack; one click both chooses and opens |
| Menu placement | Items are inserted into the built card menu by item id (after Open; just before Restore), from `integrations::menu::extend`, so `menu.rs` only gains a few named lines | Merge rule: many streams add card menu items |

## Global Constraints

- Every task's requirements include this section.
- Out of scope: secure send ("Send securely…"), onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP. Do not add a Finder Sync / Action extension, an `IExplorerCommand` handler, an MSIX/sparse package or entitlements.
- Do not depend on branch `spike/drag`. Do not edit `src/stack/DragAllBar.svelte`, `src-tauri/src/clipboard.rs`, `src-tauri/src/platform/macos_clipboard.rs` or `src-tauri/src/platform/windows_clipboard.rs` (stream I owns the clipboard this cycle). Where this plan needs a helper that exists there, write a local one.
- Shared files: additions only, at the anchors in the table below; never reorder or reformat existing lines (`cargo fmt` of your own new code is fine).
- Settings: one new section `share` (struct `Share`, `#[serde(default)]`), no version bump: `settings::VERSION` stays `2`.
- i18n: every new string is under `share.*` in `src/lib/i18n/en.json`, inserted as one block directly after the line `"menu.keep_both": "Convert and keep both",`. The strings in `Info.plist` and `hooks.nsh` are fixed English literals (OS files, not loaded through i18n).
- JavaScript/TypeScript: never use `var`. No new npm packages.
- Rust: `cargo clippy --workspace --all-targets -- -D warnings` clean; `cargo fmt --all --check` clean.
- Main thread: menu events and sync commands run on the main thread. File IO, `Job::file` and spawning `open` run on `tauri::async_runtime::spawn_blocking`. AppKit share UI and AirDrop run on the main thread (`MainThreadMarker::new()`), reached with `app.run_on_main_thread` when needed.
- Windows COM: every public function in `platform/windows_apps.rs` creates a `ComGuard` first (`CoInitializeEx(None, COINIT_APARTMENTTHREADED)`; `CoUninitialize` on drop only when that call returned `S_OK` or `S_FALSE`; `RPC_E_CHANGED_MODE` is fine and is not undone).
- **Windows code is not compilable on this Mac.** It lives only in `src-tauri/src/platform/windows_apps.rs`, `src-tauri/src/platform/windows_share.rs`, `src-tauri/installer/hooks.nsh` and `#[cfg(windows)]` blocks. Locally run `rustfmt --edition 2024 --check <file>` on each Windows `.rs` file to catch syntax errors. It is compiled, linted and tested only in CI (`core (windows-latest)`, `bundle (windows)`). Do not push without asking the user; when a Windows task is done, ask the user to push the branch and report the CI result. Windows desktop behaviour stays "built, not verified on a real desktop" (C1§1).
- Never pass paths through a shell. Paths go to `Command::arg(s)` one by one.
- Running the live app (`pnpm tauri dev`) takes over the real clipboard: ask the user first. When driving the UI, use a TextEdit window as the focus target and do not click into other apps. Restart `pnpm tauri dev` before judging UI after several quick edits.
- Copying a built `Ferry.app` into `/Applications` replaces the user's installed Ferry: ask first.
- Environment (from the worktree root):
  ```
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  ```
  A new worktree needs the git-ignored `src-tauri/binaries/` copied from the main checkout. Run `pnpm build` once before `cargo test -p ferry` (the app crate embeds `dist/`). `cargo test` takes one name filter per command.
- Commits: one per task, message `feat(share): …` / `test(share): …` / `ci: …`, with the commit trailer your session's instructions give.

### New dependencies

| Crate | Version | Where | Licence | Note |
| --- | --- | --- | --- | --- |
| `windows` | `0.62.2` | `src-tauri`, `[target.'cfg(windows)'.dependencies]` | MIT OR Apache-2.0 | Already in `Cargo.lock` (via tauri, tao, wry, trash). Features: `ApplicationModel_DataTransfer`, `Foundation`, `Foundation_Collections`, `Storage`, `Win32_Foundation`, `Win32_Graphics_Gdi`, `Win32_System_Com`, `Win32_UI_Shell`, `Win32_UI_Shell_Common`, `Win32_UI_WindowsAndMessaging` |
| `windows-collections` | `0.3.2` | same table | MIT OR Apache-2.0 | Already in `Cargo.lock`. Gives `IIterable<T>: From<Vec<Option<T>>>` for `SetStorageItemsReadOnly` |
| objc2-app-kit features | (0.3.2, existing) | macOS table | MIT OR Zlib OR Apache-2.0 | Add `NSApplication`, `NSBitmapImageRep`, `NSImage`, `NSImageRep`, `NSResponder`, `NSSharingService`, `NSView` |
| objc2-foundation features | (0.3.2, existing) | macOS table | MIT OR Zlib OR Apache-2.0 | Add `NSDictionary`, `NSError`, `NSGeometry` |

No GPL/LGPL/AGPL or patent-encumbered additions. No npm packages, no binaries.

### Shared-file edits

| File | Anchor | Change |
| --- | --- | --- |
| `crates/ferry-core/src/lib.rs` | after `pub mod glyph;` | `pub mod integrations;` (Task 1) |
| `crates/ferry-core/src/settings.rs` | `Settings` struct after `pub hotkeys: Hotkeys,`; `impl Default for Settings` after `hotkeys: Hotkeys::default(),`; end of `sanitised` before `self.version = …`; new structs after `impl Default for Backups` block | `pub share: Share,` / `share: Share::default(),` / `self.share.sanitise();` / `// Stream H: integrations` block with `AppRef`, `EditWith`, `Share` (Task 2) |
| `src-tauri/Cargo.toml` | macOS and Windows target tables | features and deps above (Tasks 6, 7, 8, 10, 11) |
| `src-tauri/src/lib.rs` | `mod hover_keys;` / `mod jobs;` | insert `mod integrations;` between them (Task 4) |
| `src-tauri/src/lib.rs` | single-instance closure | body becomes `integrations::launch::second_instance(app, args, cwd)` (Task 4) |
| `src-tauri/src/lib.rs` | `generate_handler![…]` after `commands::pick_folder,` | `// Stream H: integrations` then `integrations::commands::card_share,` (Task 10), `integrations::commands::share_pick_app,` (Task 13) |
| `src-tauri/src/lib.rs` | `.run(|_app, event| match event {` before `_ => {}` | `#[cfg(target_os = "macos")] tauri::RunEvent::Opened { urls } => integrations::launch::opened(_app, urls),` (Task 4) |
| `src-tauri/src/lib.rs` | `setup` after `clipboard::start(app);` | `integrations::init(app);` (Task 4) |
| `src-tauri/src/platform/mod.rs` | end of file | `// Stream H: integrations` block of `#[cfg] #[path] pub mod` lines (Tasks 6, 7, 8, 10, 11) |
| `src-tauri/src/menu.rs` | `Action` enum, `key`, `parse`, `page`, `enabled`, `build`, `run` | one variant `Integration(crate::integrations::menu::Item)` and one arm in each match; `build` binds the menu and calls `extend` (Task 9) |
| `src-tauri/src/hover_keys.rs` | `Key` enum, after `const KEYS`, the `for` loop in `apply`, `pressed` | two variants, `INTEGRATION_KEYS`, `.chain(INTEGRATION_KEYS)`, one arm (Task 12) |
| `src-tauri/tauri.conf.json` | `bundle.windows.nsis` | add `"installerHooks": "installer/hooks.nsh"` (Task 5) |
| `.github/workflows/ci.yml` | after step `Smoke test installer` | new step `Smoke test Explorer integration` (Task 5) |
| `src/lib/i18n/en.json` | after `"menu.keep_both"` line | the `share.*` block (Tasks 9–13 each add their keys there) |
| `src/lib/ipc.ts` (the brief's "types.ts") | `CardAction.action` union | add `'share'` (Task 10) |
| `src/stack/Stack.svelte` | `cardAction`, before the final `else` | one `else if (action === 'share')` branch (Task 10) |
| `src/lib/settings.ts`, `src/lib/settings.fixture.ts` | `Settings` interface / `sample` after `hotkeys` | `share` field (Task 2) |
| `src/lib/schema.ts` | `PaneId`, `panes`, `Control`, end of `all` | `'sharing'`, pane entry, two control kinds, `...shareSettings,` (Task 13) |
| `src/settings/Row.svelte` | before `{:else if control.kind === 'action'}` | two branches (Task 13) |

`src/stack/Card.svelte`, `commands.rs`, `state.rs`, `tray.rs`, `windows.rs` and `capabilities/*` are not touched: app commands need no capability entries, and the card reaches share through the existing `card:action` route.

### New settings keys (`settings.json`)

```jsonc
"share": {
  "edit_with": { "image": null, "video": null, "audio": null, "pdf": null }, // AppRef | null each
  "uploader": null,      // AppRef | null; null = Dropshare when found (macOS)
  "shelf_apps": []       // AppRef[], added to the detected Yoink / Dropover / Dockside
}
// AppRef = { "id": "<bundle id, or .app path> | <Windows: .exe path or handler name>", "name": "Pixelmator Pro" }
```

### New strings (`share.*`, exact copy)

| Key | English |
| --- | --- |
| `share.menu.open_with` | `Open With` |
| `share.menu.default_app` | `{app} (default)` |
| `share.menu.no_apps` | `No apps found` |
| `share.menu.edit_with` | `Edit with {app}` |
| `share.menu.edit_with_choose` | `Edit With` |
| `share.menu.share` | `Share` |
| `share.menu.airdrop` | `AirDrop` |
| `share.menu.more` | `More…` |
| `share.menu.share_windows` | `Share or Nearby Share…` |
| `share.menu.upload` | `Upload with {app}` |
| `share.menu.shelf` | `Add to Shelf` |
| `share.error.open` | `Couldn't open {name} in {app}` |
| `share.error.share` | `Couldn't share {name}` |
| `share.error.airdrop` | `AirDrop isn't available. Turn on Wi-Fi and Bluetooth.` |
| `share.error.gone` | `The file is no longer there` |
| `share.pane` | `Sharing` |
| `share.group.edit_with` | `Edit with` |
| `share.edit_with.image` | `Images` |
| `share.edit_with.image.help` | `⌘E or Ctrl+E on a card opens it in this app. With none chosen, the card menu offers your apps.` |
| `share.edit_with.video` | `Videos` |
| `share.edit_with.audio` | `Audio` |
| `share.edit_with.pdf` | `PDFs` |
| `share.group.upload` | `Upload and shelve` |
| `share.uploader` | `Upload with` |
| `share.uploader.help` | `⌘U or Ctrl+U on a card. With none chosen, Dropshare is used when it is installed.` |
| `share.shelf_apps` | `Shelf apps` |
| `share.shelf_apps.help` | `Yoink, Dropover and Dockside are found on their own. Add any other shelf app here.` |
| `share.app.none` | `None` |
| `share.app.choose` | `Choose…` |
| `share.app.clear` | `Clear` |
| `share.app.add` | `Add…` |
| `share.app.remove` | `Remove {name}` |
| `share.pick_app` | `Choose an app` |

## Review Focus

1. **Cold start through Finder "Open With" on macOS:** AppKit can deliver `application:openURLs:` before Tauri's `setup` has managed `AppState`. Expected: the files are held and optimised once setup finishes, with no panic. Pinned by `early_paths_wait_for_setup` (Task 4).
2. **Ferry listing itself in Open With ▸:** Ferry registers for PNG etc., so the OS lists Ferry among the PNG apps. Expected: Ferry never appears in its own menu, and the comparison ignores case (Windows paths). Pinned by `ferry_never_lists_itself_and_the_default_comes_first` and `windows_paths_compare_without_case` (Task 3).
3. **Paths with spaces, and files named like flags:** Explorer passes `"%1"`, and `open` gets a path argument. Expected: `C:\…\ferry smoke\cold shot.png` and `/tmp/shot a.png` arrive whole, and nothing goes through a shell. Pinned by `launch_paths_skip_the_program_and_flags`, `open_args_pass_each_file_whole` (Tasks 1, 3) and the CI smoke files with spaces (Task 5).
4. **A menu item clicked after the menu's data went stale:** the Open With list is indexed per menu. Expected: an index from another card's menu, or one past the end, does nothing. It never opens the wrong app or file. Pinned by `a_stale_index_opens_nothing` (Task 9).
5. **The output file vanished, or the card is busy:** Expected: busy cards offer no integration items, and a missing file gives a "The file is no longer there" notice instead of a silent failure. Pinned by `a_busy_card_offers_no_integration_items` (Task 9) and `a_missing_output_is_a_notice` (Task 9).

---

## File structure

```
crates/ferry-core/src/
├─ integrations.rs        # NEW: FILE_TYPES, MediaKind, launch_paths, ListedApp/order_apps, KnownApp/DROPSHARE/SHELVES/find_in_dirs, merge_apps, mac_open_args, Anchor/Toward/ViewRect/view_rect
└─ settings.rs            # + AppRef, EditWith, Share (section "share")
src-tauri/
├─ Info.plist             # NEW: CFBundleDocumentTypes + NSServices (merged by the bundler)
├─ installer/hooks.nsh    # NEW: HKCU verb, ProgID, OpenWithProgids (Windows)
└─ src/
   ├─ integrations/mod.rs       # NEW: init(app), consistency tests for Info.plist / hooks.nsh
   ├─ integrations/launch.rs    # NEW: optimise_paths, second_instance, opened, Early queue
   ├─ integrations/actions.rs   # NEW: open_with, edit_with, choose_editor, upload, shelve, share, airdrop, uploader(), shelves(), own_id()
   ├─ integrations/menu.rs      # NEW: Item, extend(), Listed, run()
   ├─ integrations/commands.rs  # NEW: card_share, share_pick_app
   └─ platform/
      ├─ macos_services.rs      # NEW: Services provider (optimiseFiles:userData:error:)
      ├─ macos_apps.rs          # NEW: list, icon, open_with, find_known, app_ref
      ├─ macos_share.rs         # NEW: share (picker), airdrop, delegate
      ├─ windows_apps.rs        # NEW (cfg windows): list, icon, open_with, find_known, app_ref
      └─ windows_share.rs       # NEW (cfg windows): share (DataTransferManager)
src/
├─ lib/share.ts, lib/share.test.ts     # NEW: shareAnchor
├─ lib/schema.share.ts                 # NEW: shareSettings rows
└─ settings/controls/{AppPicker,AppList}.svelte  # NEW
```

---

### Task 1: Core file types, media kinds and launch arguments

**Files:**
- Create: `crates/ferry-core/src/integrations.rs`
- Modify: `crates/ferry-core/src/lib.rs` (anchor: after `pub mod glyph;`)

**Interfaces:**
- Produces:
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)] #[serde(rename_all = "snake_case")] pub enum MediaKind { Image, Video, Audio, Pdf }`
  - `pub fn media_kind(ext: &str) -> Option<MediaKind>`: case-insensitive, no leading dot. Image: png jpg jpeg gif webp heic heif tif tiff avif bmp. Video: mp4 mov m4v webm mkv avi. Audio: mp3 m4a aac wav aif aiff flac ogg opus. Pdf: pdf.
  - `pub struct FileType { pub uti: &'static str, pub exts: &'static [&'static str] }`
  - `pub const FILE_TYPES: [FileType; 12]`: the types Ferry registers for Open With / Optimise with Ferry, in this order: `public.png` [png]; `public.jpeg` [jpg, jpeg]; `com.compuserve.gif` [gif]; `org.webmproject.webp` [webp]; `public.heic` [heic]; `public.heif` [heif]; `public.tiff` [tif, tiff]; `public.avif` [avif]; `public.mpeg-4` [mp4]; `com.apple.quicktime-movie` [mov]; `com.apple.m4v-video` [m4v]; `com.adobe.pdf` [pdf]. (Video/HEIC/PDF jobs succeed once streams A and E land; until then `Job::file` shows its existing "isn't an image" notice.)
  - `pub fn launch_paths(args: &[String], cwd: &Path) -> Vec<PathBuf>`: drops `args[0]` (the program), drops every arg starting with `-` (`--optimise`, macOS `-psn_…`), joins relative paths onto `cwd`, keeps the first of duplicates, keeps order.

- [ ] **Step 1: Write the failing tests** (in `integrations.rs`, `#[cfg(test)] mod tests`)

```rust
use super::*;

#[test]
fn media_kind_follows_the_extension_in_any_case() {
    assert_eq!(media_kind("PNG"), Some(MediaKind::Image));
    assert_eq!(media_kind("jpeg"), Some(MediaKind::Image));
    assert_eq!(media_kind("mov"), Some(MediaKind::Video));
    assert_eq!(media_kind("m4a"), Some(MediaKind::Audio));
    assert_eq!(media_kind("pdf"), Some(MediaKind::Pdf));
    assert_eq!(media_kind("zip"), None);
    assert_eq!(media_kind(""), None);
}

#[test]
fn every_registered_extension_has_a_kind_and_appears_once() {
    let exts: Vec<&str> = FILE_TYPES.iter().flat_map(|t| t.exts.iter().copied()).collect();
    assert_eq!(exts.len(), 14);
    for ext in &exts {
        assert!(media_kind(ext).is_some(), "{ext}");
        assert_eq!(exts.iter().filter(|e| *e == ext).count(), 1, "{ext}");
        assert_eq!(*ext, ext.to_lowercase());
    }
}

#[test]
fn launch_paths_skip_the_program_and_flags() {
    let cwd = std::env::temp_dir();
    let shot = cwd.join("shot a.png");
    let args = vec![
        "Ferry".to_string(),
        "--optimise".into(),
        shot.to_string_lossy().into_owned(),
        "-psn_0_1234".into(),
    ];
    assert_eq!(launch_paths(&args, &cwd), vec![shot]);
}

#[test]
fn relative_launch_paths_resolve_against_the_working_folder() {
    let cwd = std::env::temp_dir();
    let args = vec!["Ferry".to_string(), "b.png".into()];
    assert_eq!(launch_paths(&args, &cwd), vec![cwd.join("b.png")]);
}

#[test]
fn a_bare_launch_has_no_paths() {
    let cwd = std::env::temp_dir();
    assert!(launch_paths(&["Ferry".to_string()], &cwd).is_empty());
    assert!(launch_paths(&[], &cwd).is_empty());
}

#[test]
fn the_same_file_twice_is_optimised_once() {
    let cwd = std::env::temp_dir();
    let a = cwd.join("a.png").to_string_lossy().into_owned();
    let args = vec!["Ferry".to_string(), a.clone(), "--optimise".into(), a];
    assert_eq!(launch_paths(&args, &cwd).len(), 1);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core integrations`
Expected: compile error, `integrations` module not found.

- [ ] **Step 3: Implement** the items in Interfaces, and add `pub mod integrations;` to `crates/ferry-core/src/lib.rs` after `pub mod glyph;`. Module doc comment: "Stream H: what Ferry registers with Finder / Explorer and the pure parts of sharing and opening files with other apps."

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core integrations`
Expected: 6 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/integrations.rs crates/ferry-core/src/lib.rs
git commit -m "feat(share): file types, media kinds and launch arguments in core"
```

---

### Task 2: The `share` settings section

**Files:**
- Modify: `crates/ferry-core/src/settings.rs` (anchors in the shared-file table)
- Modify: `src/lib/settings.ts` (after the `hotkeys` field), `src/lib/settings.fixture.ts` (after `hotkeys`)

**Interfaces:**
- Consumes: `crate::integrations::MediaKind` (Task 1).
- Produces (Rust, in a `// Stream H: integrations` block placed after the `impl Default for Backups` block):
  - `#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)] pub struct AppRef { pub id: String, pub name: String }`
  - `#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)] #[serde(default)] pub struct EditWith { pub image: Option<AppRef>, pub video: Option<AppRef>, pub audio: Option<AppRef>, pub pdf: Option<AppRef>, #[serde(flatten)] pub extra: Map<String, Value> }` with `pub fn get(&self, kind: MediaKind) -> Option<&AppRef>` and `pub fn set(&mut self, kind: MediaKind, app: Option<AppRef>)`.
  - `#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)] #[serde(default)] pub struct Share { pub edit_with: EditWith, pub uploader: Option<AppRef>, pub shelf_apps: Vec<AppRef>, #[serde(flatten)] pub extra: Map<String, Value> }` with `pub fn sanitise(&mut self)`. It trims `id` and `name`, turns an app with an empty id into `None` (or drops it from `shelf_apps`), uses the id's file stem as the name when the name is empty, and de-duplicates `shelf_apps` by id ignoring case (first wins).
  - `Settings` gains `pub share: Share`. `sanitised()` calls `self.share.sanitise();` just before `self.version = self.version.max(VERSION);`.
- Produces (TS, `src/lib/settings.ts`): `export interface AppRef { id: string; name: string }` and on `Settings`: `share: { edit_with: { image: AppRef | null; video: AppRef | null; audio: AppRef | null; pdf: AppRef | null }; uploader: AppRef | null; shelf_apps: AppRef[] };`. Fixture: `share: { edit_with: { image: null, video: null, audio: null, pdf: null }, uploader: null, shelf_apps: [] },`.

- [ ] **Step 1: Write the failing tests** (in `settings.rs` `mod tests`, which already has `read_json`, `json!` and `tempfile`)

```rust
use crate::integrations::MediaKind;

fn app_ref(id: &str, name: &str) -> AppRef {
    AppRef { id: id.into(), name: name.into() }
}

#[test]
fn share_defaults_are_empty_and_a_version_2_file_loads_them() {
    let s = Settings::default();
    assert_eq!(s.share, Share::default());
    assert_eq!(s.share.edit_with.get(MediaKind::Image), None);
    assert!(s.share.uploader.is_none() && s.share.shelf_apps.is_empty());
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(&path, r#"{"version": 2, "images": {"compression": 70}}"#).unwrap();
    let (loaded, _) = Settings::load(&path);
    assert_eq!(loaded.share, Share::default());
    assert_eq!(loaded.version, 2);
}

#[test]
fn share_round_trips_and_keeps_unknown_fields() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.json");
    fs::write(
        &path,
        r#"{"version": 2, "share": {"future": 1, "edit_with": {"image": {"id": "com.pixelmatorteam.pixelmator.x", "name": "Pixelmator Pro"}, "later": true}}}"#,
    )
    .unwrap();
    let (loaded, _) = Settings::load(&path);
    assert_eq!(
        loaded.share.edit_with.get(MediaKind::Image),
        Some(&app_ref("com.pixelmatorteam.pixelmator.x", "Pixelmator Pro"))
    );
    loaded.save(&path).unwrap();
    let json = read_json(&path);
    assert_eq!(json["share"]["future"], json!(1));
    assert_eq!(json["share"]["edit_with"]["later"], json!(true));
    assert_eq!(json["share"]["uploader"], json!(null));
}

#[test]
fn sanitising_drops_blank_apps_and_twin_shelves() {
    let mut s = Settings::default();
    s.share.uploader = Some(app_ref("  ", "Nothing"));
    s.share.edit_with.set(MediaKind::Pdf, Some(app_ref(" /Applications/PDF Expert.app ", "")));
    s.share.shelf_apps = vec![app_ref("com.x", "X"), app_ref("COM.X", "X again"), app_ref("", "Blank")];
    let s = s.sanitised();
    assert_eq!(s.share.uploader, None);
    assert_eq!(
        s.share.edit_with.get(MediaKind::Pdf),
        Some(&app_ref("/Applications/PDF Expert.app", "PDF Expert"))
    );
    assert_eq!(s.share.shelf_apps, [app_ref("com.x", "X")]);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core share_`
Expected: compile errors (`Share`, `AppRef` missing).

- [ ] **Step 3: Implement** the Rust items and the TS interface and fixture lines.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core share_` then `cargo test -p ferry-core sanitising_drops_blank_apps` then `cargo test -p ferry-core settings` (all existing settings tests still pass), then `pnpm check` and `pnpm test`.
Expected: all pass; svelte-check 0 errors.

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/settings.rs src/lib/settings.ts src/lib/settings.fixture.ts
git commit -m "feat(share): share settings section (editors, uploader, shelf apps)"
```

---

### Task 3: Core app ordering, known apps, `open` arguments and anchor maths

**Files:**
- Modify: `crates/ferry-core/src/integrations.rs`

**Interfaces:**
- Consumes: `crate::settings::AppRef` (Task 2).
- Produces:
  - `pub const MENU_APPS: usize = 15;`
  - `#[derive(Debug, Clone, PartialEq, Eq)] pub struct ListedApp { pub app: AppRef, pub default: bool }`
  - `pub fn order_apps(apps: Vec<ListedApp>, own_id: &str, limit: usize) -> Vec<ListedApp>`: drop empty ids and ids equal to `own_id` ignoring ASCII case; de-duplicate by id ignoring case (the first entry wins, and it is `default` if any duplicate was); the default first; the rest by `name` ignoring case; then truncate to `limit`.
  - `pub struct KnownApp { pub name: &'static str, pub bundle_ids: &'static [&'static str], pub bundle_names: &'static [&'static str] }`
  - `pub const DROPSHARE: KnownApp = KnownApp { name: "Dropshare", bundle_ids: &["net.mkswap.Dropshare5", "net.mkswap.Dropshare-setapp"], bundle_names: &["Dropshare 5.app", "Dropshare.app"] };`
  - `pub const SHELVES: [KnownApp; 3]` = Yoink `{ bundle_ids: &["at.EternalStorms.Yoink", "at.EternalStorms.Yoink-setapp"], bundle_names: &["Yoink.app"] }`, Dropover `{ bundle_ids: &["me.damir.dropover-mac"], bundle_names: &["Dropover.app"] }`, Dockside `{ bundle_ids: &[], bundle_names: &["Dockside.app"] }`. (The bundle ids are unverified, so the name search is the fallback. See Open questions.)
  - `pub fn find_in_dirs(known: &KnownApp, dirs: &[PathBuf]) -> Option<PathBuf>`: for each dir in order, for each `bundle_names` entry, the first `dir.join(name)` that `is_dir()`.
  - `pub fn merge_apps(detected: Vec<AppRef>, configured: &[AppRef]) -> Vec<AppRef>`: detected then configured, de-duplicated by id ignoring case.
  - `pub fn mac_open_args(app_id: &str, files: &[PathBuf]) -> Vec<OsString>`: `["-a", id, files…]` when `app_id` starts with `/`, else `["-b", id, files…]`.
  - `#[derive(Debug, Clone, Copy, PartialEq, Deserialize)] pub struct Anchor { pub x: f64, pub y: f64, pub width: f64, pub height: f64, pub toward: Toward }`: the card's box in the stack page's CSS px (top-left origin).
  - `#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)] #[serde(rename_all = "lowercase")] pub enum Toward { Left, Right }`
  - `#[derive(Debug, Clone, Copy, PartialEq)] pub struct ViewRect { pub x: f64, pub y: f64, pub width: f64, pub height: f64 }`
  - `pub fn view_rect(anchor: &Anchor, view_height: f64, flipped: bool) -> ViewRect`: unchanged when `flipped`; otherwise `y = (view_height - anchor.y - anchor.height).max(0.0)`.

- [ ] **Step 1: Write the failing tests** (append to `mod tests`; add `use crate::settings::AppRef; use std::ffi::OsString; use std::fs;`)

```rust
fn app(id: &str, name: &str) -> AppRef {
    AppRef { id: id.into(), name: name.into() }
}
fn listed(id: &str, name: &str, default: bool) -> ListedApp {
    ListedApp { app: app(id, name), default }
}

#[test]
fn ferry_never_lists_itself_and_the_default_comes_first() {
    let apps = vec![
        listed("com.b", "beta", false),
        listed("io.ferry.desktop", "Ferry", false),
        listed("com.a", "Alpha", false),
        listed("com.p", "Preview", true),
        listed("com.a", "Alpha", false),
        listed("", "Nameless", false),
    ];
    let names: Vec<String> = order_apps(apps, "IO.FERRY.DESKTOP", MENU_APPS)
        .into_iter()
        .map(|l| l.app.name)
        .collect();
    assert_eq!(names, ["Preview", "Alpha", "beta"]);
}

#[test]
fn windows_paths_compare_without_case() {
    let apps = vec![listed(r"c:\users\x\appdata\local\ferry\ferry.exe", "Ferry", false)];
    assert!(order_apps(apps, r"C:\Users\x\AppData\Local\Ferry\Ferry.exe", MENU_APPS).is_empty());
}

#[test]
fn the_list_stops_at_the_limit() {
    let apps = (0..20).map(|n| listed(&format!("com.{n}"), &format!("App {n:02}"), false)).collect();
    assert_eq!(order_apps(apps, "io.ferry.desktop", MENU_APPS).len(), 15);
}

#[test]
fn a_known_app_is_found_by_bundle_name_in_the_first_folder_that_has_it() {
    let root = tempfile::tempdir().unwrap();
    let apps = root.path().join("Applications");
    let setapp = root.path().join("Setapp");
    fs::create_dir_all(&apps).unwrap();
    fs::create_dir_all(setapp.join("Dropover.app")).unwrap();
    fs::write(apps.join("Dropover.app"), b"").unwrap(); // a file, not a bundle
    let dropover = SHELVES.iter().find(|k| k.name == "Dropover").unwrap();
    let dirs = [apps, setapp.clone()];
    assert_eq!(find_in_dirs(dropover, &dirs), Some(setapp.join("Dropover.app")));
    assert_eq!(find_in_dirs(&DROPSHARE, &dirs), None);
}

#[test]
fn configured_apps_follow_the_detected_ones_without_twins() {
    let merged = merge_apps(
        vec![app("at.EternalStorms.Yoink", "Yoink")],
        &[app("AT.ETERNALSTORMS.YOINK", "Yoink"), app("com.x.shelf", "Shelf X")],
    );
    assert_eq!(merged, [app("at.EternalStorms.Yoink", "Yoink"), app("com.x.shelf", "Shelf X")]);
}

#[test]
fn open_args_pass_each_file_whole() {
    let os = |s: &str| OsString::from(s);
    let files = [PathBuf::from("/tmp/shot a.png"), PathBuf::from("/tmp/-b.png")];
    assert_eq!(
        mac_open_args("com.apple.Preview", &files),
        [os("-b"), os("com.apple.Preview"), os("/tmp/shot a.png"), os("/tmp/-b.png")]
    );
    assert_eq!(
        mac_open_args("/Applications/Tool.app", &files[..1]),
        [os("-a"), os("/Applications/Tool.app"), os("/tmp/shot a.png")]
    );
}

#[test]
fn the_anchor_flips_into_an_unflipped_view() {
    let anchor = Anchor { x: 12.0, y: 40.0, width: 300.0, height: 200.0, toward: Toward::Left };
    assert_eq!(view_rect(&anchor, 500.0, false), ViewRect { x: 12.0, y: 260.0, width: 300.0, height: 200.0 });
    assert_eq!(view_rect(&anchor, 500.0, true), ViewRect { x: 12.0, y: 40.0, width: 300.0, height: 200.0 });
    assert_eq!(view_rect(&anchor, 100.0, false).y, 0.0);
}

#[test]
fn the_page_sends_the_anchor_as_json() {
    let anchor: Anchor =
        serde_json::from_str(r#"{"x":1,"y":2,"width":3,"height":4,"toward":"right"}"#).unwrap();
    assert_eq!(anchor.toward, Toward::Right);
    assert_eq!(anchor.width, 3.0);
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run: `cargo test -p ferry-core integrations`
Expected: compile errors for the missing items.

- [ ] **Step 3: Implement** the items in Interfaces.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry-core integrations`
Expected: 14 passed (with Task 1's 6).

- [ ] **Step 5: Commit**

```bash
git add crates/ferry-core/src/integrations.rs
git commit -m "feat(share): app ordering, known shelf apps, open arguments and anchor maths"
```

---

### Task 4: Files from Finder / Explorer reach the running app

**Files:**
- Create: `src-tauri/src/integrations/mod.rs`, `src-tauri/src/integrations/launch.rs`
- Modify: `src-tauri/src/lib.rs` (anchors: `mod integrations;`, single-instance closure, `RunEvent::Opened` arm, `integrations::init(app);`)

**Interfaces:**
- Consumes: `ferry_core::integrations::launch_paths` (Task 1); existing `ferry_core::pipeline::Job::file(&Path, &Settings, bool) -> Result<Job, String>`, `crate::jobs::{start, notice}`, `crate::windows::open_settings`, `AppState::{settings, gifsicle}`.
- Produces:
  - `integrations/mod.rs`: `pub mod launch;` and `pub fn init(app: &AppHandle)`. It calls `launch::ready(app, ferry_core::integrations::launch_paths(&args, &cwd))` with `args` from `std::env::args_os()` mapped through `to_string_lossy` and `cwd` from `std::env::current_dir()` (fall back to an empty path). Task 6 adds the macOS Services registration here.
  - `launch.rs`:
    - `pub fn optimise_paths(app: &AppHandle, paths: Vec<PathBuf>)`: on `spawn_blocking`, for each path, `Job::file` then `jobs::start(app, job, Level::Normal, None)`. When `start` returns false, `jobs::notice(app, tf("error.busy", &[("name", &file_name)]))`; when `Job::file` fails, `jobs::notice(app, reason)`. Runs whatever the run state is. (This duplicates the 10-line loop in `clipboard.rs` on purpose: stream I owns that file this cycle.)
    - `pub fn second_instance(app: &AppHandle, args: Vec<String>, cwd: String)`: paths from `launch_paths(&args, Path::new(&cwd))`; none → `crate::windows::open_settings(app)` (today's behaviour), else `offer(app, paths)`.
    - `#[cfg(target_os = "macos")] pub fn opened(app: &AppHandle, urls: Vec<tauri::Url>)`: the `file:` URLs through `to_file_path()`, then `offer`.
    - `pub fn ready(app: &AppHandle, initial: Vec<PathBuf>)`: opens the early queue and optimises `initial` followed by whatever was held.
    - `fn offer(app: &AppHandle, paths: Vec<PathBuf>)`: holds the paths in the early queue until `ready`, then optimises.
    - `struct Early { held: Option<Vec<PathBuf>> }` in a `static EARLY: Mutex<Early> = Mutex::new(Early::new());`, with `const fn new() -> Self` (starts holding: `Some(Vec::new())`), `fn offer(&mut self, paths: Vec<PathBuf>) -> Vec<PathBuf>` (returns the paths to run now; empty while holding) and `fn open(&mut self) -> Vec<PathBuf>` (stops holding, returns what was held; a second `open` returns empty).

- [ ] **Step 1: Write the failing test** (in `launch.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn early_paths_wait_for_setup() {
        let mut early = Early::new();
        assert!(early.offer(vec!["/a.png".into()]).is_empty());
        assert!(early.offer(vec!["/b.png".into()]).is_empty());
        assert_eq!(early.open(), vec![PathBuf::from("/a.png"), PathBuf::from("/b.png")]);
        assert_eq!(early.offer(vec!["/c.png".into()]), vec![PathBuf::from("/c.png")]);
        assert!(early.open().is_empty());
    }
}
```

- [ ] **Step 2: Run the test to verify it fails**

Run: `pnpm build && cargo test -p ferry early_paths_wait_for_setup`
Expected: compile error (module missing).

- [ ] **Step 3: Implement** the Interfaces and the four `lib.rs` edits:
  - `mod integrations;` between `mod hover_keys;` and `mod jobs;`.
  - The closure: `tauri_plugin_single_instance::init(|app, args, cwd| { integrations::launch::second_instance(app, args, cwd) })`, and the comment above it becomes "Must be the first plugin: a second launch hands its files to this one (Explorer, Open with) or focuses Settings."
  - In `.run(|_app, event| match event {`, before `_ => {}`:
    ```rust
    // Stream H: Finder "Open With" and files dropped on the Dock icon.
    #[cfg(target_os = "macos")]
    tauri::RunEvent::Opened { urls } => integrations::launch::opened(_app, urls),
    ```
  - In `setup`, after `clipboard::start(app);`: `integrations::init(app);`.

- [ ] **Step 4: Run the test to verify it passes, and lint**

Run: `cargo test -p ferry early_paths_wait_for_setup` then `cargo clippy --workspace --all-targets -- -D warnings`
Expected: 1 passed; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/integrations src-tauri/src/lib.rs
git commit -m "feat(share): files from Open With and a second launch become file jobs"
```

Verification of the full path is in Task 5 (Windows, CI) and Task 6 (macOS, manual).

---

### Task 5: Explorer verb and "Open with" on Windows (installer hooks + CI check)

**Files:**
- Create: `src-tauri/installer/hooks.nsh`
- Modify: `src-tauri/tauri.conf.json` (anchor `bundle.windows.nsis`: `"nsis": { "installMode": "currentUser", "installerHooks": "installer/hooks.nsh" }`)
- Modify: `.github/workflows/ci.yml` (new step after `Smoke test installer`)
- Test: `src-tauri/src/integrations/mod.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `ferry_core::integrations::FILE_TYPES` (Task 1); the `--optimise "%1"` command line parsed by Task 4.

- [ ] **Step 1: Write the failing consistency test** (in `integrations/mod.rs`)

```rust
#[cfg(test)]
mod tests {
    use ferry_core::integrations::FILE_TYPES;

    const HOOKS: &str = include_str!("../../installer/hooks.nsh");

    #[test]
    fn the_installer_registers_every_file_type() {
        for ext in FILE_TYPES.iter().flat_map(|t| t.exts.iter()) {
            assert!(HOOKS.contains(&format!("!insertmacro ${{MACRO}} \"{ext}\"")), "{ext}");
        }
        assert!(HOOKS.contains(r#"--optimise "%1"'"#));
        // Ferry must never become the default app: nothing sets `.<ext>` itself.
        assert!(!HOOKS.contains(r#"Software\Classes\.${EXT}" """#));
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p ferry the_installer_registers_every_file_type`
Expected: compile error, `installer/hooks.nsh` not found.

- [ ] **Step 3: Create `src-tauri/installer/hooks.nsh` with exactly this content**

```nsis
; Stream H: "Optimise with Ferry" in Explorer and Ferry under "Open with", for the
; current user only (installMode currentUser). Ferry never becomes the default app:
; nothing here writes the default value of HKCU\Software\Classes\.<ext>.
; Windows 11 shows the verb under "Show more options"; the top-level menu needs a
; signed sparse package (out of scope).
!define FERRY_PROGID "Ferry.File"
!define FERRY_VERB "Ferry.Optimise"

!macro FERRY_REGISTER_TYPE EXT
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}" "" "Optimise with Ferry"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}" "Icon" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" --optimise "%1"'
  WriteRegStr HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${FERRY_PROGID}" ""
!macroend

!macro FERRY_UNREGISTER_TYPE EXT
  DeleteRegKey HKCU "Software\Classes\SystemFileAssociations\.${EXT}\shell\${FERRY_VERB}"
  DeleteRegValue HKCU "Software\Classes\.${EXT}\OpenWithProgids" "${FERRY_PROGID}"
!macroend

; Keep in step with ferry_core::integrations::FILE_TYPES (a test checks).
!macro FERRY_EACH_TYPE MACRO
  !insertmacro ${MACRO} "png"
  !insertmacro ${MACRO} "jpg"
  !insertmacro ${MACRO} "jpeg"
  !insertmacro ${MACRO} "gif"
  !insertmacro ${MACRO} "webp"
  !insertmacro ${MACRO} "heic"
  !insertmacro ${MACRO} "heif"
  !insertmacro ${MACRO} "tif"
  !insertmacro ${MACRO} "tiff"
  !insertmacro ${MACRO} "avif"
  !insertmacro ${MACRO} "mp4"
  !insertmacro ${MACRO} "mov"
  !insertmacro ${MACRO} "m4v"
  !insertmacro ${MACRO} "pdf"
!macroend

!macro NSIS_HOOK_POSTINSTALL
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}" "" "Ferry"
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}\DefaultIcon" "" "$INSTDIR\${MAINBINARYNAME}.exe,0"
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}\shell\open" "" "Optimise with Ferry"
  WriteRegStr HKCU "Software\Classes\${FERRY_PROGID}\shell\open\command" "" '"$INSTDIR\${MAINBINARYNAME}.exe" --optimise "%1"'
  !insertmacro FERRY_EACH_TYPE FERRY_REGISTER_TYPE
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend

!macro NSIS_HOOK_POSTUNINSTALL
  !insertmacro FERRY_EACH_TYPE FERRY_UNREGISTER_TYPE
  DeleteRegKey HKCU "Software\Classes\${FERRY_PROGID}"
  System::Call 'shell32::SHChangeNotify(i 0x08000000, i 0, p 0, p 0)'
!macroend
```

Then add `"installerHooks": "installer/hooks.nsh"` to `bundle.windows.nsis` in `tauri.conf.json`.

- [ ] **Step 4: Add the CI step** to `.github/workflows/ci.yml` directly after the `Smoke test installer` step (same job, `bundle`):

```yaml
      - name: Smoke test Explorer integration
        if: runner.os == 'Windows'
        shell: pwsh
        run: |
          $exe = Join-Path $env:LOCALAPPDATA 'Ferry\Ferry.exe'
          $verb = 'HKCU:\Software\Classes\SystemFileAssociations\.png\shell\Ferry.Optimise\command'
          $command = (Get-ItemProperty -LiteralPath $verb).'(default)'
          if ($command -ne "`"$exe`" --optimise `"%1`"") { throw "unexpected verb command: $command" }
          $progids = Get-ItemProperty -LiteralPath 'HKCU:\Software\Classes\.png\OpenWithProgids'
          if ($null -eq $progids.'Ferry.File') { throw 'Ferry is not under Open with for .png' }
          $default = (Get-ItemProperty -LiteralPath 'HKCU:\Software\Classes\.png' -ErrorAction SilentlyContinue).'(default)'
          if ($default -eq 'Ferry.File') { throw 'Ferry made itself the default app for .png' }
          Add-Type -AssemblyName System.Drawing
          function New-TestPng([string]$path) {
            $bmp = New-Object System.Drawing.Bitmap 256, 192
            for ($x = 0; $x -lt 256; $x++) { for ($y = 0; $y -lt 192; $y++) {
              $bmp.SetPixel($x, $y, [System.Drawing.Color]::FromArgb(255, $x, $y, ($x * $y) % 256)) } }
            $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
            $bmp.Dispose()
          }
          $dir = Join-Path $env:RUNNER_TEMP 'ferry smoke'
          New-Item -ItemType Directory -Force -Path $dir | Out-Null
          $cold = Join-Path $dir 'cold shot.png'
          $warm = Join-Path $dir 'warm shot.png'
          New-TestPng $cold
          New-TestPng $warm
          $coldBefore = (Get-Item -LiteralPath $cold).Length
          $warmBefore = (Get-Item -LiteralPath $warm).Length
          $proc = Start-Process -FilePath $exe -ArgumentList @('--optimise', "`"$cold`"") -PassThru
          Start-Sleep -Seconds 10
          if ((Get-Item -LiteralPath $cold).Length -ge $coldBefore) { throw 'a cold start did not optimise its file' }
          Start-Process -FilePath $exe -ArgumentList @('--optimise', "`"$warm`"") -Wait
          Start-Sleep -Seconds 8
          if ((Get-Item -LiteralPath $warm).Length -ge $warmBefore) { throw 'a second launch did not hand its file over' }
          if ($proc.HasExited) { throw 'the first Ferry exited' }
          Stop-Process -Id $proc.Id -Force
          Start-Process -FilePath (Join-Path $env:LOCALAPPDATA 'Ferry\uninstall.exe') -ArgumentList '/S' -Wait
          for ($i = 0; $i -lt 20 -and (Test-Path -LiteralPath $verb); $i++) { Start-Sleep -Seconds 1 }
          if (Test-Path -LiteralPath $verb) { throw 'uninstall left the Explorer verb behind' }
          Write-Host 'Explorer verb, Open with and both launch paths work'
```

- [ ] **Step 5: Run the local checks**

Run: `cargo test -p ferry the_installer_registers_every_file_type`
Expected: PASS.
Run: `python3 -c "import json;json.load(open('src-tauri/tauri.conf.json'))"`
Expected: no output (valid JSON).

- [ ] **Step 6: Commit**

```bash
git add src-tauri/installer/hooks.nsh src-tauri/tauri.conf.json .github/workflows/ci.yml src-tauri/src/integrations/mod.rs
git commit -m "feat(share): Optimise with Ferry in Explorer and Ferry under Open with (Windows)"
```

- [ ] **Step 7: CI check (ask the user to push the branch).** In `bundle (windows)`, step `Smoke test Explorer integration` prints `Explorer verb, Open with and both launch paths work`. If NSIS fails to compile the hooks, the `Build installer` step shows the `makensis` error with the hooks line number.

---

### Task 6: macOS "Open With" target and Finder Services (manual check)

**Files:**
- Create: `src-tauri/Info.plist`, `src-tauri/src/platform/macos_services.rs`
- Modify: `src-tauri/src/platform/mod.rs` (end of file, `// Stream H: integrations` block), `src-tauri/src/integrations/mod.rs` (`init` and tests), `src-tauri/Cargo.toml` (objc2-app-kit feature `NSApplication`)

**Interfaces:**
- Consumes: `integrations::launch::optimise_paths` (Task 4).
- Produces:
  - `platform/mod.rs`: `#[cfg(target_os = "macos")] #[path = "macos_services.rs"] pub mod services;`
  - `macos_services.rs`: `pub fn register(app: &AppHandle)` (main thread: stores the handle in `static APP: OnceLock<AppHandle>`, creates the provider once (kept in a `thread_local!` `OnceCell<Retained<ServiceProvider>>`, as `macos_preview.rs` keeps `PREVIEW`), calls `NSApplication::sharedApplication(mtm).setServicesProvider(Some(&provider))` and then `objc2_app_kit::NSUpdateDynamicServices()`), and `pub(crate) fn file_paths(pb: &NSPasteboard) -> Vec<PathBuf>` (each pasteboard item's `public.file-url` string → `NSURL::URLWithString` → `to_file_path`; the same chain as `macos_clipboard.rs` `read_from`, written locally).
  - `ServiceProvider` (`define_class!`, `MainThreadOnly`, superclass `NSObject`) with `#[unsafe(method(optimiseFiles:userData:error:))] fn optimise_files(&self, pboard: &NSPasteboard, _user_data: Option<&NSString>, _error: *mut *mut NSString)`, which calls `launch::optimise_paths(APP.get(), file_paths(pboard))`.
  - `integrations::init` gains `#[cfg(target_os = "macos")] crate::platform::services::register(app);` before the launch call.

- [ ] **Step 1: Write the failing tests**

In `integrations/mod.rs` tests (macOS-agnostic, it reads a file):

```rust
const PLIST: &str = include_str!("../../Info.plist");

#[test]
fn the_bundle_declares_every_file_type_for_open_with_and_services() {
    for t in FILE_TYPES.iter() {
        assert_eq!(PLIST.matches(&format!("<string>{}</string>", t.uti)).count(), 2, "{}", t.uti);
    }
    assert!(PLIST.contains("<string>Alternate</string>"));
    assert!(PLIST.contains("<string>optimiseFiles</string>"));
}
```

In `macos_services.rs` (`#[cfg(test)] mod tests`, private pasteboard as in `macos_clipboard.rs` tests: `NSPasteboard::pasteboardWithUniqueName()`, released with `releaseGlobally()` in `Drop`; write the URLs as `write_files_to` does at `macos_clipboard.rs:136`):

```rust
#[test]
fn a_services_pasteboard_gives_its_files() {
    let pb = Private::new();
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("shot a.png");
    let b = dir.path().join("b.jpg");
    write_urls(&pb.0, &[a.clone(), b.clone()]);
    assert_eq!(file_paths(&pb.0), vec![a, b]);
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry the_bundle_declares_every_file_type` and `cargo test -p ferry a_services_pasteboard_gives_its_files`
Expected: compile errors (no `Info.plist`, no module).

- [ ] **Step 3: Create `src-tauri/Info.plist` with exactly this content** (the bundler merges it into the app's Info.plist)

```xml
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDocumentTypes</key>
  <array>
    <dict>
      <key>CFBundleTypeName</key>
      <string>Images, videos and PDFs</string>
      <key>CFBundleTypeRole</key>
      <string>Viewer</string>
      <key>LSHandlerRank</key>
      <string>Alternate</string>
      <key>LSItemContentTypes</key>
      <array>
        <string>public.png</string>
        <string>public.jpeg</string>
        <string>com.compuserve.gif</string>
        <string>org.webmproject.webp</string>
        <string>public.heic</string>
        <string>public.heif</string>
        <string>public.tiff</string>
        <string>public.avif</string>
        <string>public.mpeg-4</string>
        <string>com.apple.quicktime-movie</string>
        <string>com.apple.m4v-video</string>
        <string>com.adobe.pdf</string>
      </array>
    </dict>
  </array>
  <key>NSServices</key>
  <array>
    <dict>
      <key>NSMenuItem</key>
      <dict>
        <key>default</key>
        <string>Optimise with Ferry</string>
      </dict>
      <key>NSMessage</key>
      <string>optimiseFiles</string>
      <key>NSPortName</key>
      <string>Ferry</string>
      <key>NSRequiredContext</key>
      <dict/>
      <key>NSSendFileTypes</key>
      <array>
        <string>public.png</string>
        <string>public.jpeg</string>
        <string>com.compuserve.gif</string>
        <string>org.webmproject.webp</string>
        <string>public.heic</string>
        <string>public.heif</string>
        <string>public.tiff</string>
        <string>public.avif</string>
        <string>public.mpeg-4</string>
        <string>com.apple.quicktime-movie</string>
        <string>com.apple.m4v-video</string>
        <string>com.adobe.pdf</string>
      </array>
    </dict>
  </array>
</dict>
</plist>
```

- [ ] **Step 4: Implement** `macos_services.rs`, the `platform/mod.rs` line and `init`. Add `"NSApplication"` to the objc2-app-kit features in `src-tauri/Cargo.toml`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `plutil -lint src-tauri/Info.plist` → `src-tauri/Info.plist: OK`
Run: `cargo test -p ferry the_bundle_declares_every_file_type` and `cargo test -p ferry a_services_pasteboard_gives_its_files` → PASS
Run: `cargo clippy --workspace --all-targets -- -D warnings` → clean

- [ ] **Step 6: Build the bundle and check the merge**

Run: `pnpm tauri build --bundles app`
Then: `/usr/libexec/PlistBuddy -c "Print :NSServices:0:NSMessage" target/release/bundle/macos/Ferry.app/Contents/Info.plist` → `optimiseFiles`; `/usr/libexec/PlistBuddy -c "Print :CFBundleDocumentTypes:0:LSHandlerRank" target/release/bundle/macos/Ferry.app/Contents/Info.plist` → `Alternate`.

- [ ] **Step 7: Commit**

```bash
git add src-tauri/Info.plist src-tauri/src/platform/macos_services.rs src-tauri/src/platform/mod.rs src-tauri/src/integrations/mod.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(share): Ferry under Open With and Optimise with Ferry in Finder Services (macOS)"
```

- [ ] **Step 8: Manual check (ask the user before copying to /Applications and before launching).** Quit any running Ferry. `cp -R target/release/bundle/macos/Ferry.app /Applications/`, then `/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister -f /Applications/Ferry.app` and `/System/Library/CoreServices/pbs -update`. Make a test PNG on the Desktop (copy any screenshot to `~/Desktop/ferry test.png`). Look at:
  1. **Cold Open With:** with Ferry not running, Finder → right-click `ferry test.png` → Open With → Ferry. Ferry starts, a card appears with a saving, and the file on disk is smaller (`ls -l`). Finder's default app for PNG is unchanged (Get Info → Open with still shows Preview).
  2. **Warm Open With:** with Ferry running, repeat on a second copy. A card appears, and Settings does not open.
  3. **Services:** right-click a third copy → Quick Actions or Services → "Optimise with Ferry". A card appears. If the item is missing, look in System Settings → Keyboard → Keyboard Shortcuts → Services → Files and Folders, record whether it was off, and turn it on.
  4. **Several files:** select three PNGs → Services → "Optimise with Ferry". Three cards appear.
  5. **Dock:** drag a PNG onto Ferry's icon in Finder's Applications folder. A card appears.
  Record each result in the commit message of the next task, or in a note to the user.

---

### Task 7: macOS app listing, icons and launching

**Files:**
- Create: `src-tauri/src/platform/macos_apps.rs`
- Modify: `src-tauri/src/platform/mod.rs` (Stream H block), `src-tauri/Cargo.toml` (objc2-app-kit features `NSBitmapImageRep`, `NSImage`, `NSImageRep`; objc2-foundation features `NSDictionary`, `NSGeometry`)

**Interfaces:**
- Consumes: `ferry_core::integrations::{ListedApp, KnownApp, find_in_dirs, mac_open_args}`, `ferry_core::settings::AppRef`, existing `platform::macos::app_identifier` (reach it as `super::macos::app_identifier`).
- Produces (`#[cfg(target_os = "macos")] #[path = "macos_apps.rs"] pub mod apps;`). The Windows module (Task 8) has the same signatures:
  - `pub fn list(file: &Path) -> Vec<ListedApp>`: `NSWorkspace::URLsForApplicationsToOpenURL(&NSURL::fileURLWithPath(file))`, each app URL → `app_ref(path)`. `default` is true for the URL equal to `URLForApplicationToOpenURL`. Unordered; the caller orders.
  - `pub fn app_ref(path: &Path) -> Option<AppRef>`: `id` = bundle identifier (via `app_identifier`), or the path itself when there is none; `name` = the file stem (`Preview.app` → `Preview`).
  - `pub fn icon(app: &AppRef) -> Option<tauri::image::Image<'static>>`. The app's path: `id` itself when it starts with `/`, else `URLForApplicationWithBundleIdentifier`. Draw `NSWorkspace::iconForFile(path)` into `NSImage::initWithSize(NSSize::new(32.0, 32.0))` between `lockFocus` / `unlockFocus` (`#[allow(deprecated)]`, using `drawInRect`), then `TIFFRepresentation` → `NSBitmapImageRep::imageRepWithData` → `representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())` → `Image::from_bytes(&png.to_vec()).ok()?.to_owned()`. If `lockFocus` crashes off the main thread in tests, switch to `NSBitmapImageRep` + `NSGraphicsContext::graphicsContextWithBitmapImageRep` (feature `NSGraphicsContext`).
  - `pub fn open_with(app: &AppRef, files: &[PathBuf]) -> Result<(), String>`: `Command::new("/usr/bin/open").args(mac_open_args(&app.id, files)).output()`. Non-zero exit → `Err(stderr trimmed)`.
  - `pub fn find_known(known: &KnownApp) -> Option<AppRef>`: the first `bundle_ids` entry that `URLForApplicationWithBundleIdentifier` resolves, else `find_in_dirs(known, &["/Applications", "/Applications/Setapp", "<home>/Applications"])`. The result goes through `app_ref`, with `name` replaced by `known.name`.

- [ ] **Step 1: Write the failing tests** (`#[cfg(test)] mod tests` in `macos_apps.rs`)

```rust
use super::*;
use ferry_core::integrations::KnownApp;

fn png() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("x.png");
    std::fs::write(&path, b"").unwrap(); // LaunchServices goes by the extension
    (dir, path)
}

#[test]
fn preview_opens_png_files() {
    let (_dir, png) = png();
    let ids: Vec<String> = list(&png).into_iter().map(|l| l.app.id).collect();
    assert!(ids.iter().any(|id| id == "com.apple.Preview"), "{ids:?}");
}

#[test]
fn a_picked_app_bundle_becomes_its_identifier() {
    assert_eq!(
        app_ref(Path::new("/System/Applications/Preview.app")),
        Some(AppRef { id: "com.apple.Preview".into(), name: "Preview".into() })
    );
}

#[test]
fn a_known_app_is_found_by_bundle_id_and_has_an_icon() {
    const PREVIEW: KnownApp = KnownApp { name: "Preview", bundle_ids: &["com.apple.Preview"], bundle_names: &[] };
    let found = find_known(&PREVIEW).unwrap();
    assert_eq!(found.id, "com.apple.Preview");
    let icon = icon(&found).unwrap();
    assert!(icon.width() >= 32 && icon.width() == icon.height());
}

#[test]
fn opening_with_a_missing_app_fails() {
    let (_dir, png) = png();
    let nope = AppRef { id: "io.ferry.no-such-app".into(), name: "Nope".into() };
    assert!(open_with(&nope, &[png]).is_err());
}
```

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry preview_opens_png_files`
Expected: compile error.

- [ ] **Step 3: Implement** the Interfaces and the `platform/mod.rs` line; add the features.

- [ ] **Step 4: Run them to verify they pass**

Run each: `cargo test -p ferry preview_opens_png_files`, `cargo test -p ferry a_picked_app_bundle`, `cargo test -p ferry a_known_app_is_found_by_bundle_id`, `cargo test -p ferry opening_with_a_missing_app`
Expected: PASS each. Then `cargo clippy --workspace --all-targets -- -D warnings`, which is clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/macos_apps.rs src-tauri/src/platform/mod.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(share): list, show and launch the apps that open a file (macOS)"
```

---

### Task 8: Windows app listing, icons and launching (CI only)

**Files:**
- Create: `src-tauri/src/platform/windows_apps.rs` (**`cfg(windows)`, not compilable on the Mac**)
- Modify: `src-tauri/src/platform/mod.rs` (`#[cfg(windows)] #[path = "windows_apps.rs"] pub mod apps;`), `src-tauri/Cargo.toml` (`[target.'cfg(windows)'.dependencies]`: `windows = { version = "0.62.2", features = [the ten features in Global Constraints] }`)

**Interfaces:**
- Produces the same five functions as Task 7:
  - `list(file)`: `ComGuard`; `SHAssocEnumHandlers(&HSTRING::from(format!(".{ext}")), ASSOC_FILTER_RECOMMENDED)`. Loop `Next(&mut [None], Some(&mut fetched))` until `fetched == 0` or `Err`. Each handler: `id` = `GetName()`, `name` = `GetUIName()`, both `PWSTR` read with `.to_string()` and freed with `CoTaskMemFree`. `default: false`. A file with no extension → empty list.
  - `app_ref(path)`: `id` = the full `.exe` path as a string, `name` = the file stem.
  - `icon(app)`: only when `Path::new(&app.id).is_file()`. `SHDefExtractIconW(id, 0, 0, Some(&mut large), None, 32)` → `GetIconInfo` → `GetDIBits` on `hbmColor` into a 32×32 `BITMAPINFOHEADER { biBitCount: 32, biHeight: -32, biCompression: BI_RGB }` buffer. BGRA → RGBA; if every alpha byte is 0, set them all to 255. `DeleteObject` both bitmaps, `DestroyIcon`. `Image::new_owned(rgba, 32, 32)`.
  - `open_with(app, files)`: `ComGuard`. If `Path::new(&app.id).is_file()`: `Command::new(&app.id).args(files).spawn()` (Err → `Err(e.to_string())`). Else, for the first file's extension: enumerate with `ASSOC_FILTER_NONE`, find the handler whose `GetName()` equals `app.id` ignoring case, then `SHCreateItemFromParsingName::<_, _, IShellItem>(file, None)` → `BindToHandler::<_, IDataObject>(None, &BHID_DataObject)` → `handler.Invoke(&data)`. No match → `Err("app not found")`.
  - `find_known(_)`: always `None` (the known shelf apps and Dropshare are macOS apps).
  - `struct ComGuard(bool)` as in Global Constraints.

- [ ] **Step 1: Write the tests** (`#[cfg(test)] mod tests` in `windows_apps.rs`; they run in CI `bundle (windows)` via `cargo test -p ferry`)

```rust
use super::*;

#[test]
fn listing_png_handlers_never_fails() {
    let dir = tempfile::tempdir().unwrap();
    let png = dir.path().join("x.png");
    std::fs::write(&png, b"").unwrap();
    let _ = list(&png); // a server runner may have none; it must not panic
    assert!(list(&dir.path().join("noext")).is_empty());
}

#[test]
fn opening_with_a_missing_exe_fails() {
    let nope = AppRef { id: r"C:\no\such\app.exe".into(), name: "Nope".into() };
    assert!(open_with(&nope, &[PathBuf::from(r"C:\x.png")]).is_err());
}

#[test]
fn an_exe_has_a_32_px_icon() {
    let explorer = AppRef { id: r"C:\Windows\explorer.exe".into(), name: "Explorer".into() };
    let icon = icon(&explorer).unwrap();
    assert_eq!((icon.width(), icon.height()), (32, 32));
    assert!(icon.rgba().chunks(4).any(|px| px[3] > 0));
}

#[test]
fn a_picked_exe_keeps_its_full_path() {
    assert_eq!(
        app_ref(Path::new(r"C:\Program Files\ShareX\ShareX.exe")),
        Some(AppRef { id: r"C:\Program Files\ShareX\ShareX.exe".into(), name: "ShareX".into() })
    );
}
```

- [ ] **Step 2: Implement** the Interfaces.

- [ ] **Step 3: Local checks**

Run: `rustfmt --edition 2024 --check src-tauri/src/platform/windows_apps.rs` → no output.
Run: `cargo check -p ferry` → clean on the Mac (the file is excluded by `cfg`; `Cargo.lock` gains the new direct deps).

- [ ] **Step 4: Commit**

```bash
git add src-tauri/src/platform/windows_apps.rs src-tauri/src/platform/mod.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(share): list, show and launch the apps that open a file (Windows, CI-verified)"
```

- [ ] **Step 5: CI check (ask the user to push).** `bundle (windows)` → `cargo clippy --workspace --all-targets -- -D warnings` is clean, and `cargo test -p ferry` shows the four `windows_apps` tests passing.

---

### Task 9: Card menu: Open With ▸, Edit with, Upload, Add to Shelf ▸

**Files:**
- Create: `src-tauri/src/integrations/menu.rs`, `src-tauri/src/integrations/actions.rs`
- Modify: `src-tauri/src/integrations/mod.rs` (`pub mod actions; pub mod menu;`), `src-tauri/src/menu.rs` (anchors below), `src/lib/i18n/en.json` (keys `share.menu.open_with`, `share.menu.default_app`, `share.menu.no_apps`, `share.menu.edit_with`, `share.menu.edit_with_choose`, `share.menu.upload`, `share.menu.shelf`, `share.error.open`, `share.error.gone`)

**Interfaces:**
- Consumes: `platform::apps::{list, icon, open_with, find_known}` (Tasks 7, 8); `ferry_core::integrations::{order_apps, merge_apps, media_kind, MENU_APPS, DROPSHARE, SHELVES}`; `Settings.share` (Task 2); `crate::jobs::{output, notice}`; `crate::prefs::update`; `tauri_plugin_opener::open_path`.
- Produces:
  - `integrations::menu`:
    - `#[derive(Debug, Clone, Copy, PartialEq, Eq)] pub enum Item { Share, AirDrop, OpenWith(u8), EditWith, ChooseEditor(u8), Upload, Shelf(u8) }`. Share and AirDrop are routed in Task 10, but they are declared now so the ids are stable.
    - `impl Item { pub fn key(self) -> String; pub fn parse(key: &str) -> Option<Self> }` with keys `share`, `airdrop`, `open_with.<n>`, `edit_with`, `choose_editor.<n>`, `upload`, `shelf.<n>`. `n` is a `u8` below `MENU_APPS`; anything else → `None`.
    - `pub struct Listed { job: JobId, open_with: Vec<AppRef>, shelves: Vec<AppRef> }` with `pub fn open_with(&self, job: &JobId, n: u8) -> Option<&AppRef>` and `pub fn shelf(&self, job: &JobId, n: u8) -> Option<&AppRef>` (`None` for another job or past the end), kept in `static LISTED: Mutex<Option<Listed>>`.
    - `pub fn insert_points(ids: &[String], open: &str, restore: &str) -> Option<(usize, usize)>` → `(index of open + 1, index of restore)`.
    - `pub fn extend(app: &AppHandle, menu: &Menu<Wry>, card: &JobUpdate) -> tauri::Result<()>`. Read `menu.items()?` ids, then `insert_points(ids, item_id(card.id, Open), item_id(card.id, Restore))`; when that is `None`, return `Ok(())`. Insert the **share group first** at the restore index: `[Add to Shelf ▸ when shelves are non-empty, Upload with {app} (accelerator "CmdOrCtrl+U") when there is an uploader, separator]` (Task 10 puts Share ▸ at the front of this group), but only when the group has an item before the separator. Then insert the **open group** at the open index: `[Open With ▸, Edit with]`. Open With ▸ is a `Submenu` of `IconMenuItem`s from `order_apps(list(output), own_id, MENU_APPS)`; the default app is labelled `share.menu.default_app`, and an empty list gets one disabled `share.menu.no_apps` item. Edit with is `MenuItem` "Edit with {app}" with "CmdOrCtrl+E" when `share.edit_with` has this kind's app, else an "Edit With ▸" submenu of the same apps as `ChooseEditor(n)` items. Store `Listed` for this card. Icons come from a `static ICONS: Mutex<HashMap<String, Option<Image<'static>>>>` cache filled through `platform::apps::icon`. Every item uses `crate::menu::enabled(card, Action::Integration(item))`.
    - `pub fn run(app: &AppHandle, id: &JobId, item: Item) -> Result<(), String>`: dispatches to `actions`. On `Err(reason)` from an action it shows `jobs::notice(app, reason)` and returns `Ok(())`.
  - `integrations::actions` (every fn returns `Result<(), String>`, where `Err` is a user-facing sentence):
    - `fn output(app, id) -> Result<PathBuf, String>`: `jobs::output` or `Err(t("share.error.gone"))`.
    - `pub fn open_with(app: &AppHandle, id: &JobId, with: &AppRef) -> Result<(), String>`: failure → `tf("share.error.open", [("name", file name), ("app", &with.name)])`.
    - `pub fn edit_with(app: &AppHandle, id: &JobId) -> Result<(), String>`: kind from the output's extension; `share.edit_with.get(kind)`; `Some` → `open_with`, `None` → `tauri_plugin_opener::open_path(path, None::<&str>)`.
    - `pub fn choose_editor(app: &AppHandle, id: &JobId, with: AppRef) -> Result<(), String>`: `prefs::update(app, |s| s.share.edit_with.set(kind, Some(with.clone())))`, then `open_with`.
    - `pub fn uploader(app: &AppHandle) -> Option<AppRef>`: `share.uploader`, else `platform::apps::find_known(&DROPSHARE)`.
    - `pub fn upload(app: &AppHandle, id: &JobId) -> Result<(), String>`: no uploader → `Ok(())` (logged at debug).
    - `pub fn shelves(app: &AppHandle) -> Vec<AppRef>`: `merge_apps(SHELVES.iter().filter_map(find_known).collect(), &share.shelf_apps)`.
    - `pub fn own_id(app: &AppHandle) -> String`: macOS `app.config().identifier.clone()`, Windows `std::env::current_exe()` as a string.
  - `src-tauri/src/menu.rs` anchors (additions only, except the `build` return):
    1. `enum Action`, after `Close,`: `/// Stream H: integrations (share, open with, edit with, upload, shelf).` and `Integration(crate::integrations::menu::Item),`
    2. `key()`, before `named =>`: `Action::Integration(item) => format!("h-{}", item.key()),`
    3. `parse()`, before `Some(_) => None,`: `Some(("h", value)) => crate::integrations::menu::Item::parse(value).map(Action::Integration),`
    4. `enabled()`, before `Action::Restore =>`: `Action::Integration(_) => file,`
    5. `build()`: `Menu::with_items(app, &[…])` becomes `let menu = Menu::with_items(app, &[…])?;` then `crate::integrations::menu::extend(app, &menu, card)?;` and `Ok(menu)`.
    6. `run()`, in the blocking closure's `match`, before `_ => Ok(()),`: `Action::Integration(item) => crate::integrations::menu::run(&app, &id, item),`

- [ ] **Step 1: Write the failing tests** (in `integrations/menu.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::menu::{Action, enabled, item_id, parse_id};
    use ferry_core::job::JobState;
    use ferry_core::pipeline;
    use ferry_core::sniff::Format;

    fn job(text: &str) -> JobId {
        serde_json::from_value(serde_json::Value::String(text.into())).unwrap()
    }

    fn done() -> JobUpdate {
        let mut card = pipeline::notice("x".into());
        card.id = job("18f-1");
        card.state = JobState::Done;
        card.output = Some("/tmp/shot.png".into());
        card.format = Some(Format::Png);
        card
    }

    const ITEMS: [Item; 8] = [
        Item::Share,
        Item::AirDrop,
        Item::OpenWith(0),
        Item::OpenWith(14),
        Item::EditWith,
        Item::ChooseEditor(3),
        Item::Upload,
        Item::Shelf(2),
    ];

    #[test]
    fn every_item_reads_back_through_the_card_menu_ids() {
        let id = job("18f-1");
        for item in ITEMS {
            let action = Action::Integration(item);
            assert_eq!(parse_id(&item_id(&id, action)), Some((id.clone(), action)), "{item:?}");
        }
        assert_eq!(Item::parse("open_with.x"), None);
        assert_eq!(Item::parse("open_with.15"), None);
        assert_eq!(Item::parse("explode"), None);
    }

    #[test]
    fn a_busy_card_offers_no_integration_items() {
        let mut card = done();
        for item in ITEMS {
            assert!(enabled(&card, Action::Integration(item)), "{item:?}");
        }
        card.state = JobState::Queued;
        for item in ITEMS {
            assert!(!enabled(&card, Action::Integration(item)), "{item:?}");
        }
    }

    #[test]
    fn a_stale_index_opens_nothing() {
        let app = |id: &str| AppRef { id: id.into(), name: id.into() };
        let listed = Listed { job: job("18f-1"), open_with: vec![app("com.a")], shelves: vec![] };
        assert_eq!(listed.open_with(&job("18f-1"), 0), Some(&app("com.a")));
        assert_eq!(listed.open_with(&job("18f-1"), 1), None);
        assert_eq!(listed.open_with(&job("18f-2"), 0), None);
        assert_eq!(listed.shelf(&job("18f-1"), 0), None);
    }

    #[test]
    fn integration_items_go_after_open_and_before_restore() {
        let ids: Vec<String> = ["card:1:open", "card:1:reveal", "sep", "card:1:copy", "sep2", "card:1:restore", "sep3", "card:1:close"]
            .iter()
            .map(|s| s.to_string())
            .collect();
        assert_eq!(insert_points(&ids, "card:1:open", "card:1:restore"), Some((1, 5)));
        assert_eq!(insert_points(&ids, "card:1:open", "card:2:restore"), None);
    }
}
```

and in `integrations/actions.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_output_is_a_notice() {
        assert_eq!(gone_reason(), ferry_core::i18n::t("share.error.gone"));
        assert_ne!(gone_reason(), "share.error.gone"); // the key exists in en.json
    }
}
```

(`fn gone_reason() -> String` is the one place `output()` builds its `Err`.)

- [ ] **Step 2: Run them to verify they fail**

Run: `cargo test -p ferry every_item_reads_back_through_the_card_menu_ids`
Expected: compile error.

- [ ] **Step 3: Implement** the Interfaces, the six `menu.rs` anchors and the i18n keys (strings in Global Constraints).

- [ ] **Step 4: Run the tests to verify they pass**

Run each: `cargo test -p ferry every_item_reads_back_through`, `cargo test -p ferry a_busy_card_offers_no_integration`, `cargo test -p ferry a_stale_index_opens_nothing`, `cargo test -p ferry integration_items_go_after_open`, `cargo test -p ferry a_missing_output_is_a_notice`, and `cargo test -p ferry menu` (the existing `menu.rs` tests still pass).
Then: `cargo clippy --workspace --all-targets -- -D warnings`, `pnpm test`.
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/integrations src-tauri/src/menu.rs src/lib/i18n/en.json
git commit -m "feat(share): Open With, Edit with, Upload and Add to Shelf in the card menu"
```

- [ ] **Step 6: Manual check (ask the user before `pnpm tauri dev`; focus a TextEdit window first).** Copy a screenshot so a card appears. Right-click the card:
  1. Under Open there is "Open With ▸" with app icons, Preview marked "(default)" and listed first, and no Ferry entry. Choosing Preview opens the card's file in Preview.
  2. "Edit With ▸" (no editor set) lists the same apps. Choosing one opens the file there. Reopen the menu: the item now reads "Edit with ⟨that app⟩ ⌘E". `~/Library/Application Support/io.ferry.desktop/settings.json` has `share.edit_with.image` set.
  3. If Yoink, Dropover or Dockside is installed, "Add to Shelf ▸" lists it, and choosing it puts the file on that shelf (record whether the app accepted the file or only opened). If Dropshare is installed, "Upload with Dropshare ⌘U" uploads. If neither is installed, those items are absent. Record which apps were present.
  4. The existing items (Copy … Restore, Close) are all still there, in their old order.

---

### Task 10: Share ▸ on macOS (picker anchored to the card, AirDrop) (manual check)

**Files:**
- Create: `src-tauri/src/platform/macos_share.rs`, `src-tauri/src/integrations/commands.rs`, `src/lib/share.ts`, `src/lib/share.test.ts`
- Modify: `src-tauri/src/platform/mod.rs` (`#[cfg(target_os = "macos")] #[path = "macos_share.rs"] pub mod share;`), `src-tauri/src/integrations/{mod.rs, menu.rs, actions.rs}`, `src-tauri/src/menu.rs` (`page()` anchor), `src-tauri/src/lib.rs` (`generate_handler!` anchor), `src/lib/ipc.ts`, `src/stack/Stack.svelte`, `src-tauri/Cargo.toml` (objc2-app-kit `NSResponder`, `NSSharingService`, `NSView`; objc2-foundation `NSError`), `src/lib/i18n/en.json` (`share.menu.share`, `share.menu.airdrop`, `share.menu.more`, `share.menu.share_windows`, `share.error.share`, `share.error.airdrop`)

**Interfaces:**
- Consumes: `ferry_core::integrations::{Anchor, Toward, view_rect}` (Task 3), `platform::preview::{remember_front, give_back_focus}` (existing, both OSes), `menu::send` (existing).
- Produces:
  - `macos_share.rs`:
    - `pub fn share(window: &WebviewWindow, paths: Vec<PathBuf>, anchor: &Anchor) -> Result<(), String>` (main thread). Get `MainThreadMarker`, and the view from `window.ns_view()` as `&NSView` (borrowed for the call). `rect = view_rect(anchor, view.bounds().size.height, view.isFlipped())`. Edge: `NSRectEdge::MinX` for `Toward::Left`, `MaxX` for `Toward::Right`. Items: `NSArray::<NSURL>::from_retained_slice(&urls)` then `unsafe { cast_unchecked::<AnyObject>() }`. Then `NSSharingServicePicker::initWithItems`, `setDelegate(delegate)`, and, when `ACTIVATE_FOR_SHARE`, `remember_front()` + `activateIgnoringOtherApps(true)`, then `showRelativeToRect_ofView_preferredEdge`. Keep the picker in a `thread_local!` `RefCell<Option<Retained<NSSharingServicePicker>>>` until it finishes.
    - `pub fn airdrop(paths: &[PathBuf]) -> Result<(), String>` (main thread). `NSSharingService::sharingServiceNamed(NSSharingServiceNameSendViaAirDrop)`; `None`, or `canPerformWithItems` false → `Err(t("share.error.airdrop"))`. Otherwise `setDelegate`, activate as above, `performWithItems`. Keep the service in the same thread-local until done.
    - `const ACTIVATE_FOR_SHARE: bool = true;`, a fallback switch documented as for the case where the picker does not take clicks or service windows open behind other apps.
    - `ShareDelegate` (`define_class!`, `MainThreadOnly`) implements `NSSharingServicePickerDelegate`. `sharingServicePicker:didChooseSharingService:` with `None` → `give_back_focus()` and drop the picker; `sharingServicePicker:delegateForSharingService:` → itself. It also implements `NSSharingServiceDelegate`: `sharingService:didShareItems:` → `give_back_focus()`; `sharingService:didFailToShareItems:error:` → `warn!` and `give_back_focus()`. Both clear the thread-local. One delegate, kept in a `thread_local!` `OnceCell`.
  - `integrations/commands.rs`: `#[tauri::command] pub fn card_share(app: AppHandle, window: WebviewWindow, id: JobId, anchor: Anchor) -> Result<(), String>` (sync, so it runs on the main thread). Output via `actions::output`, then `platform::share::share(&window, vec![path], &anchor)`. On `Err(e)`, `jobs::notice(app, tf("share.error.share", [("name", file name)]))` and return `Err(e)`. `integrations/mod.rs`: `pub mod commands;`. Registered in `lib.rs` under `// Stream H: integrations` after `commands::pick_folder,`.
  - `integrations::actions::airdrop(app, id)`: `app.run_on_main_thread(move || if let Err(r) = platform::share::airdrop(&[path]) { jobs::notice(&app, r) })` (macOS only, `#[cfg]`).
  - `integrations::menu::extend`: the share group starts with, on macOS, a `Submenu` "Share ▸" of [`AirDrop` → `Item::AirDrop`, `More…` → `Item::Share`], and on Windows a `MenuItem` "Share or Nearby Share…" → `Item::Share`. `run`: `Item::AirDrop` → `actions::airdrop`; `Item::Share` never reaches `run` (it ends in the page).
  - `src-tauri/src/menu.rs` `page()`, before `_ => return None,`: `Action::Integration(crate::integrations::menu::Item::Share) => "share",`
  - `src/lib/share.ts`: `export interface ShareAnchor { x: number; y: number; width: number; height: number; toward: 'left' | 'right' }` and `export function shareAnchor(rect: Pick<DOMRect, 'x' | 'y' | 'width' | 'height'>, railRight: boolean): ShareAnchor` (`toward` is `'right'` when `railRight`, i.e. the stack is in a left corner, else `'left'`).
  - `src/lib/ipc.ts`: `CardAction.action` gains `| 'share'`.
  - `src/stack/Stack.svelte` `cardAction`, before the final `} else {`:
    ```ts
    } else if (action === 'share') {
      const el = root?.querySelector<HTMLElement>(`article[data-card-id="${id}"]`);
      if (el) invoke('card_share', { id, anchor: shareAnchor(el.getBoundingClientRect(), railRight) }).catch((e) => console.warn('share', e));
    ```
    plus `import { shareAnchor } from '../lib/share';`.

- [ ] **Step 1: Write the failing frontend test** (`src/lib/share.test.ts`)

```ts
import { describe, expect, it } from 'vitest';
import { shareAnchor } from './share';

describe('shareAnchor', () => {
  it('copies the card box and points toward the screen centre', () => {
    const rect = { x: 12, y: 40, width: 300, height: 200 };
    expect(shareAnchor(rect, false)).toEqual({ x: 12, y: 40, width: 300, height: 200, toward: 'left' });
    expect(shareAnchor(rect, true).toward).toBe('right');
  });
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm test -- share`
Expected: FAIL, cannot resolve `./share`.

- [ ] **Step 3: Implement** the Interfaces and the strings.

- [ ] **Step 4: Run the checks**

Run: `pnpm test` and `pnpm check` → pass, 0 errors.
Run: `cargo test -p ferry every_item_reads_back_through` (the Share id still routes) and `cargo clippy --workspace --all-targets -- -D warnings` → clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/platform/macos_share.rs src-tauri/src/platform/mod.rs src-tauri/src/integrations src-tauri/src/menu.rs src-tauri/src/lib.rs src-tauri/Cargo.toml Cargo.lock src/lib/share.ts src/lib/share.test.ts src/lib/ipc.ts src/stack/Stack.svelte src/lib/i18n/en.json
git commit -m "feat(share): Share and AirDrop from the card menu (macOS)"
```

- [ ] **Step 6: Manual check (ask before `pnpm tauri dev`; TextEdit frontmost).** Make a card, then right-click it → Share ▸:
  1. **More…**: the system share picker opens next to the card, on the side toward the screen centre, not at the screen corner or over the menu bar. Its entries (AirDrop, Mail, Messages, Notes…) take clicks. Choose Mail: a compose window opens in front with the file attached. Close it: TextEdit is frontmost again.
  2. **Dismiss**: open the picker and press Esc / click away. TextEdit is frontmost again (typing goes to TextEdit).
  3. **AirDrop**: the AirDrop window opens in front with the file. Cancel: TextEdit is frontmost. With Wi-Fi off, an amber/error card says "AirDrop isn't available. Turn on Wi-Fi and Bluetooth."
  4. Move the stack to a top-left corner in Settings; the picker opens on the card's right side.
  If the picker does not take clicks or windows open behind TextEdit, flip `ACTIVATE_FOR_SHARE`, re-test, and record which value works.

---

### Task 11: Share on Windows (DataTransferManager, Nearby Share) (CI compile only)

**Files:**
- Create: `src-tauri/src/platform/windows_share.rs` (**`cfg(windows)`, not compilable on the Mac**)
- Modify: `src-tauri/src/platform/mod.rs` (`#[cfg(windows)] #[path = "windows_share.rs"] pub mod share;`), `src-tauri/Cargo.toml` (`windows-collections = "0.3.2"` in the Windows table)

**Interfaces:**
- Produces: `pub fn share(window: &WebviewWindow, paths: Vec<PathBuf>, _anchor: &Anchor) -> Result<(), String>`, the same signature as macOS. The share UI places itself.
  - Spawn a `std::thread` that resolves `StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.join()?` for each path and builds `title` = the first file's name. Then `app.run_on_main_thread(move || …)` calls `fn show(app: &AppHandle, files: Vec<StorageFile>, title: String) -> windows::core::Result<()>`. Errors from the thread or `show` → `jobs::notice(app, tf("share.error.share", …))`. Return `Ok(())` right away.
  - `show`: `hwnd = HWND(app.get_webview_window(STACK)…hwnd()?.0)`; `interop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?`; `manager: DataTransferManager = interop.GetForWindow(hwnd)?`. If `static TOKEN: Mutex<Option<i64>>` holds a token, `manager.RemoveDataRequested(token)` first. The new token comes from `manager.DataRequested(&TypedEventHandler::<DataTransferManager, DataRequestedEventArgs>::new(move |_, args| { let data = args.ok()?.Request()?.Data()?; data.Properties()?.SetTitle(&HSTRING::from(&title))?; let items: Vec<Option<IStorageItem>> = files.iter().map(|f| f.cast().ok()).collect(); data.SetStorageItemsReadOnly(&windows_collections::IIterable::<IStorageItem>::from(items)) }))?`. Then `interop.ShowShareUIForWindow(hwnd)`.

- [ ] **Step 1: Implement** the Interfaces. There is no automated test: the share UI needs a desktop session. The function is reached through Task 10's `card_share`.

- [ ] **Step 2: Local checks**

Run: `rustfmt --edition 2024 --check src-tauri/src/platform/windows_share.rs` → no output. `cargo check -p ferry` → clean.

- [ ] **Step 3: Commit**

```bash
git add src-tauri/src/platform/windows_share.rs src-tauri/src/platform/mod.rs src-tauri/Cargo.toml Cargo.lock
git commit -m "feat(share): Share and Nearby Share through the Windows share UI (unverified on a desktop)"
```

- [ ] **Step 4: CI check (ask the user to push).** `bundle (windows)` clippy is clean and the installer builds. **Manual check on a real Windows 11 machine (none available; record as unverified):** right-click a card → "Share or Nearby Share…" → the Windows share UI opens with the file name as its title and a Nearby sharing row. Sharing to Mail attaches the file.

---

### Task 12: Hover hotkeys ⌘E / Ctrl+E (Edit with) and ⌘U / Ctrl+U (Upload) (manual check)

**Files:**
- Modify: `src-tauri/src/hover_keys.rs`

**Interfaces:**
- Consumes: `integrations::actions::{edit_with, upload}` (Task 9), `jobs::notice`.
- Produces (additions, anchors in the shared-file table):
  - `enum Key` gains `EditWith,` and `Upload,`.
  - After `const KEYS`: `/// Stream H: integrations.` and `const INTEGRATION_KEYS: [(&str, Key); 2] = [("CmdOrCtrl+E", Key::EditWith), ("CmdOrCtrl+U", Key::Upload)];`
  - In `apply`, the loop `for (accelerator, key) in KEYS {` becomes `for (accelerator, key) in KEYS.into_iter().chain(INTEGRATION_KEYS) {`.
  - In `pressed`, a new first arm of `let scale = match key {`: `Key::EditWith | Key::Upload => { … spawn_blocking: actions::edit_with or actions::upload; Err(reason) → jobs::notice … ; return; }`.

- [ ] **Step 1: Write the failing test** (in `hover_keys.rs` tests)

```rust
#[test]
fn the_integration_accelerators_parse_and_do_not_clash() {
    for (accelerator, _) in INTEGRATION_KEYS {
        assert!(accelerator.parse::<Shortcut>().is_ok(), "{accelerator}");
        assert!(KEYS.iter().all(|(other, _)| *other != accelerator), "{accelerator}");
    }
}
```

- [ ] **Step 2: Run it to verify it fails**

Run: `cargo test -p ferry the_integration_accelerators_parse`
Expected: compile error (`INTEGRATION_KEYS` missing).

- [ ] **Step 3: Implement** the Interfaces.

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -p ferry the_integration_accelerators_parse` and `cargo test -p ferry every_accelerator_parses` → PASS; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src/hover_keys.rs
git commit -m "feat(share): Cmd/Ctrl+E edits and Cmd/Ctrl+U uploads the hovered card"
```

- [ ] **Step 6: Manual check (ask before `pnpm tauri dev`; TextEdit frontmost).**
  1. Hover a card and press ⌘E. With an image editor set (Task 9), the file opens in it; with none set, it opens in Preview.
  2. Move the pointer off the card and press ⌘E / ⌘U in TextEdit: TextEdit's own shortcuts work (⌘U underlines), so Ferry no longer holds the keys.
  3. With Dropshare installed, ⌘U on a hovered card uploads. Without an uploader, nothing happens and there is no error card.

---

### Task 13: Settings → Sharing pane (manual check)

**Files:**
- Create: `src/lib/schema.share.ts`, `src/settings/controls/AppPicker.svelte`, `src/settings/controls/AppList.svelte`
- Modify: `src/lib/schema.ts` (anchors in the table), `src/lib/schema.test.ts`, `src/settings/Row.svelte`, `src-tauri/src/integrations/commands.rs`, `src-tauri/src/lib.rs` (`generate_handler!` Stream H block), `src/lib/i18n/en.json` (`share.pane`, `share.group.*`, `share.edit_with.*`, `share.uploader*`, `share.shelf_apps*`, `share.app.*`, `share.pick_app`)

**Interfaces:**
- Consumes: `platform::apps::app_ref` (Tasks 7, 8), `AppRef` TS type (Task 2).
- Produces:
  - Rust `#[tauri::command] pub async fn share_pick_app(app: AppHandle) -> Result<Option<AppRef>, String>`, modelled on `commands::pick_app`: title `t("share.pick_app")`; macOS filter `Applications` / `app`, directory `/Applications`; Windows filter `Programs` / `exe`. `platform::apps::app_ref(&path)`, or `Err(t("error.pick_app"))`.
  - `schema.ts`: `PaneId` adds `| 'sharing'`; `panes` appends `{ id: 'sharing', label: 'share.pane' }`; `Control` adds `| { kind: 'app' } | { kind: 'app_list' }`; the last element of `all` is followed by `...shareSettings,` with `import { shareSettings } from './schema.share';`.
  - `schema.share.ts`: `export const shareSettings: Setting[]`, in this order. All are `pane: 'sharing'` and none is `mac_only`:

    | id | group | label | help | path | control |
    | --- | --- | --- | --- | --- | --- |
    | `edit_with_image` | `share.group.edit_with` | `share.edit_with.image` | `share.edit_with.image.help` | `share.edit_with.image` | `app` |
    | `edit_with_video` | same | `share.edit_with.video` | – | `share.edit_with.video` | `app` |
    | `edit_with_audio` | same | `share.edit_with.audio` | – | `share.edit_with.audio` | `app` |
    | `edit_with_pdf` | same | `share.edit_with.pdf` | – | `share.edit_with.pdf` | `app` |
    | `uploader` | `share.group.upload` | `share.uploader` | `share.uploader.help` | `share.uploader` | `app` |
    | `shelf_apps` | same | `share.shelf_apps` | `share.shelf_apps.help` | `share.shelf_apps` | `app_list` |

    (`schema.ts` imports from `schema.share.ts` and the reverse import is type-only: `import type { Setting } from './schema';`.)
  - `AppPicker.svelte` props `{ value: AppRef | null; disabled?: boolean; labelledby: string; onchange: (value: AppRef | null) => void }`: shows `value?.name ?? t('share.app.none')`, a `share.app.choose` button (`invoke<AppRef | null>('share_pick_app')`; non-null → `onchange`), and a `share.app.clear` button only when `value` is set (→ `onchange(null)`).
  - `AppList.svelte` props `{ items: AppRef[]; disabled?: boolean; labelledby: string; onchange: (items: AppRef[]) => void }`: a list of names, each with a × button (`aria-label={t('share.app.remove', { name })}`), and a `share.app.add` button that appends a picked app unless its id is already there. Styling follows `ListEditor.svelte`.
  - `Row.svelte`, before `{:else if control.kind === 'action'}`: `{:else if control.kind === 'app'}<AppPicker …/>{:else if control.kind === 'app_list'}<AppList …/>`. Errors from the picker set `outcome` as `pickApp` does.

- [ ] **Step 1: Write the failing test** (append to `src/lib/schema.test.ts` inside `describe`)

```ts
it('has the Sharing pane with editors per type, the uploader and shelf apps', () => {
  expect(all.filter((s) => s.pane === 'sharing').map((s) => s.path)).toEqual([
    'share.edit_with.image',
    'share.edit_with.video',
    'share.edit_with.audio',
    'share.edit_with.pdf',
    'share.uploader',
    'share.shelf_apps',
  ]);
  expect(panes.map((p) => p.id)).toContain('sharing');
});
```

- [ ] **Step 2: Run it to verify it fails**

Run: `pnpm test -- schema`
Expected: FAIL (empty array).

- [ ] **Step 3: Implement** the Interfaces and the strings.

- [ ] **Step 4: Run the checks**

Run: `pnpm test`, `pnpm check` (the existing schema tests also pin every label/help string and that every path exists in `sample`), `cargo clippy --workspace --all-targets -- -D warnings`.
Expected: all pass.

- [ ] **Step 5: Commit**

```bash
git add src/lib/schema.ts src/lib/schema.share.ts src/lib/schema.test.ts src/settings/Row.svelte src/settings/controls/AppPicker.svelte src/settings/controls/AppList.svelte src-tauri/src/integrations/commands.rs src-tauri/src/lib.rs src/lib/i18n/en.json
git commit -m "feat(share): Sharing settings pane (editors, uploader, shelf apps)"
```

- [ ] **Step 6: Manual check (ask before `pnpm tauri dev`).** Tray → Settings… → Sharing:
  1. The pane lists "Edit with" (Images, Videos, Audio, PDFs), "Upload and shelve" (Upload with, Shelf apps), all reading "None" or empty.
  2. Images → Choose… opens an app picker at /Applications. Pick Pixelmator or Preview: the row shows the app's name, and `settings.json` has `share.edit_with.image = { "id": "<bundle id>", "name": "…" }`. Clear empties it.
  3. Shelf apps → Add… twice with the same app: it is listed once. × removes it.
  4. Searching "shelf" in the sidebar finds the Shelf apps row.
  5. After setting an image editor here, a card's menu shows "Edit with ⟨app⟩ ⌘E" (Task 9's item follows the setting).

---

## Risks

- **NSSharingServicePicker from a non-activating panel.** Apple says to call `showRelativeToRect` on mouse-down; here it runs after a native menu closes. Mitigation: `ACTIVATE_FOR_SHARE` and the Task 10 manual check. A further fallback, if needed, is to call it from the page's `mousedown` on a rail button (stream G's "+" slot can host one).
- **Windows share UI from a `WS_EX_NOACTIVATE` tool window** may fail or appear without focus. It is unverified until someone has a Windows desktop. The call failing gives the `share.error.share` notice, not a crash.
- **Explorer multi-select:** static verbs launch one process per file and are hidden for more than 15 selected files (the default `MultiSelectModel`). With Ferry not running, several processes race the single-instance mutex, and one may exit before the first instance listens. Ferry normally runs at login, which makes this rare.
- **Services cache:** macOS may need `pbs -update` or a re-login before "Optimise with Ferry" shows, and the user may have Services turned off.
- **Shelf and Dropshare bundle ids** are unverified. The name search in the Applications folders covers renamed or Setapp builds.

## Open questions for the user

1. Do you have Dropshare, Yoink, Dropover or Dockside installed to verify "Upload" and "Add to Shelf"? Their bundle ids in Task 2 are best guesses, and detection falls back to the app's name in /Applications.
2. Windows 11 shows "Optimise with Ferry" under "Show more options" (the top-level menu needs a signed package). Is that acceptable this cycle?
3. Ferry registers for HEIC, TIFF, AVIF, MP4/MOV/M4V and PDF now. Until streams A and E merge, those show the existing "isn't an image" card. Register them now, or only PNG/JPEG/GIF/WebP until A and E land?
4. For the manual checks the built `Ferry.app` must go into `/Applications`, which replaces your installed 0.2.0. OK?
5. ⌘E with no editor chosen opens the default app (usually Preview), and the menu offers a one-click chooser. Is that the behaviour you want, or should ⌘E open the chooser?
