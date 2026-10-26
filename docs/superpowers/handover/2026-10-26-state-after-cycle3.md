# State after Cycle 3 (2026-10-26)

Supersedes `2026-10-23-cycle3-plans.md` and `2026-10-24-cycle3-resume.md`: all nine streams are built and merged.

## Where things are

- Integration branch `claude/clever-cori-t3h3y2` (local only, unpushed). Latest merge `6b9a55b`. fmt, pnpm check, pnpm test, clippy `-D warnings`, `cargo test -p ferry-core --lib`, `-p ferry` and `ffmpeg_real` were green at that commit.
- Origin has `main` at `37955bf` and tag `v0.2.0`. The app version is still 0.2.0.
- The Opus whole-branch review's Critical and Important findings are fixed and merged.
- Real GPL ffmpeg sidecar is built and installed locally (no x265).

## In flight

- `fix/dir` (worktree `.worktrees/fix-dir`): a folder reaching a file job showed "Can't read batch: Is a directory". Adds `Skip::Folder` and i18n key `error.folder`. Not merged; rerun the full checks after merging.

## Open decisions for the user

- Bump to 1.0.0, tag, push the integration branch and open the release?
- Deck "Planned next" line (secure send, onboarding, Vietnamese, signing, ARM64) and the architecture slide numbers (about 1,267 tests: 865 core, 153 app, 249 vitest).
- Delete `spike/drag` (unmerged prototype commits) only with the user's OK.
- Remove the `.worktrees/fix-*` worktrees only with the user's OK.

## Not verified

- Windows code paths (never compiled or run here), the release workflow (no Actions quota), visual check of the PDF card border, tooltip wrap and select chevrons.
- Hover hotkeys sometimes stop working (Medium, not isolated); share picker stays open; "4 pages" badge hidden under the hovered rail.
- Deferred Minor review findings: see the review list in the session transcript (probe panic on negative duration, PDF greyscale never JPEG, signature check ignores /Perms, `compare_page` blocks the main thread, and others).

## Deck

"Optimizer" (Slides artifact, https://claude.ai/artifact/2MoKeZ24jYbDH22425hjPM): presents Ferry as released v1.0.0. Real screenshots are placed on the hero slides, with detail slides for compare, video, PDF and batch. Missing shots: crop popover, crop + convert result, PDF page extraction, compact list selection bar, pipeline editor, Run pipeline menu; the share-menu shot still shows the old colour-bar thumbnail.
Media credits: whale photo Giles Laurent, CC BY-SA 4.0; Big Buck Bunny, Blender Foundation, CC BY 3.0; Qajar tiles public domain.
Leftovers to delete: scratchpad `demo-media`, `~/Desktop/ferry-c3-check`.
