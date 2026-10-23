# Handover: Cycle 3 planning, 2026-10-23

Stopped at the user's request. Nothing from Cycle 3 is built and **no plan was written**: the nine Opus plan writers were stopped mid-research, before saving any file.

## State
- `main` at `cf9d5c9` (stream brief commit). `origin/main` is at `37955bf`, so `main` is 1 commit ahead (the brief, not pushed).
- `v0.2.0` is pushed. The tag should have started `release.yml` (Windows NSIS + macOS dmg). Not checked; look at the GitHub Actions run and the release page.
- The M3 worktrees and branches are deleted. Their ledgers are in the session scratchpad `.../scratchpad/ledgers/` and may be gone; the rulings are summarised in the memory file `project-cycle2-m3-handover.md`.
- Still present: branch `spike/drag` (unmerged prototype commits, worktree in the old scratchpad; delete only with the user's OK) and merged branch `fix/card-ui-feedback`.

## What to do next
1. Re-dispatch the nine plan writers (Agent, `model: "opus"`, one message). The prompts follow one template. Brief, requirements and specs to read: [stream brief](../specs/2026-10-23-ferry-cycle3-streams.md) (binding), [requirements](../../requirements/ferry-optimize-requirements.md), [Cycle 1 spec](../specs/2026-10-21-ferry-cycle1-foundation-phase1-design.md), [Cycle 2 spec](../specs/2026-10-22-ferry-cycle2-card-tools-design.md).
   - Start each prompt with "Read AGENTS.md and CONSTRAINTS.md at repo root if present. Follow both." (neither file exists).
   - Tell each writer to use `superpowers:writing-plans`, write ONLY the plan file, no dry-run code, no `git add` or commit, and to save to `docs/superpowers/plans/2026-10-23-ferry-s<X>-<name>.md`.
   - Per stream, name the source files to study and the decisions to justify:
     - **A video** (`sA-video`): ffmpeg sidecar bundling like gifsicle in CI and `tauri.conf.json`; ffmpeg LGPL/GPL build and its licence; owns the `ffmpeg.rs` `locate`/`run` contract.
     - **B audio** (`sB-audio`): consumes A's ffmpeg contract; loudness two-pass arguments; cover art.
     - **C watchers** (`sC-watchers`): notify crate; screenshot and recording folders per OS; partial writes, no feedback loops; a named hook for video until A lands.
     - **D drop-batch** (`sD-drop-batch`): drop zone, batch window, compact list over the existing card model.
     - **E pdf-formats** (`sE-pdf-formats`): PDF engine, HEIC (patents/libheif or OS decoders), AVIF, TIFF, animated WebP, with licences; each format its own task group.
     - **F pipelines** (`sF-pipelines`): compose the Cycle 2 recipe model, StepKind registry, adaptive compression defined precisely, image steps only.
     - **G app-services** (`sG-app-services`): tauri-plugin-updater (updater signing key and secrets listed as user actions), settings sync via a user-chosen folder, hotkey editor, rail editor.
     - **H integrations** (`sH-integrations`): NSSharingServicePicker, Windows share, Open with, Finder/Explorer context menus, Dropshare/shelf apps. Windows code behind `cfg(windows)`.
     - **I clipboard-fixes** (`sI-clipboard-fixes`): accumulate, paths-not-images, auto-copy, plus the deferred minors, each verified real at HEAD first.
   - Each writer's final message: plan path, task count, new dependencies and licences, open questions.
2. Commit the plans to `main` locally. Bring the open questions to the user: ffmpeg/HEIC licensing, updater key and secrets, manual checks, anything needing hardware or accounts.
3. After the user approves the plans: one Sonnet implementer per plan (`model: "sonnet"`), each in its own worktree from `main`, stream F in two steps (image steps first, video/audio/PDF steps after A, B, E merge), B after A's ffmpeg contract exists. Merge into `feat/c3-all`. Then one Opus whole-branch review, fix Critical/Important, then one manual check pass (ask the user before running the live app).

## User rules to keep
- Out of scope: secure send, onboarding, Vietnamese, code signing and notarisation, Windows ARM64, CLI/SDK/MCP.
- Don't push, push tags, merge into other branches, or delete branches/worktrees without asking.
- Models: plans and reviews on `opus`, implementation on `sonnet`, never `fable`. Plan writers write only the plan.
- Never use `var` in JavaScript. Keep implementation and responses minimal.
- Ask before running the live app (`pnpm tauri dev` takes over the real clipboard). Don't click into other apps when driving the UI; use a TextEdit window as focus target.
- Commit trailer: `Co-Authored-By: Claude Sonnet 5.5 <noreply@anthropic.com>` (the user's older note said Opus 5.5; the current system instruction says Sonnet 5.5).
- Environment, from the worktree root:
  ```
  source "$HOME/.cargo/env"
  export PATH="$HOME/.nvm/versions/node/v22.23.3/bin:$PATH"
  export FERRY_GIFSICLE="$PWD/src-tauri/binaries/gifsicle-$(rustc -vV | sed -n 's/^host: //p')"
  ```
  A new worktree needs the git-ignored `src-tauri/binaries/` copied from the main checkout. `cargo test` takes one name filter per command.
- After a few rapid UI edits the Tauri webview can show stale modules; restart `pnpm tauri dev` before judging the UI.
