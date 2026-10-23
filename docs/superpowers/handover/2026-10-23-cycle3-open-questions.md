# Cycle 3 plan writers: open questions for the user

One section per stream, added as each plan lands. Answer before implementation starts.

## I clipboard-fixes (`plans/2026-10-23-ferry-sI-clipboard-fixes.md`, 14 tasks, no new dependencies)
1. Exact crop then downscale: plan applies the scale to the Exact size (1200x628 at 50% gives 600x314), changing the Cycle 2 rule that size and scale replace each other. Alternative keeps the rule but scales the cropped original (can come out larger, 1440x754).
2. "Copy file paths" becomes "instead of the image": apps that only paste image data get no pixels while it is on. OK?
3. Auto-copy: one toggle for drops and watched folders, default off, first run only.
4. Cross-stream: C and D must call `jobs::start_auto_copied` instead of `jobs::start`; tell them or swap at merge.
5. Accumulated collection lifetime unchanged: since launch or last Clear all; closing a card does not remove it.
- Dropped minor: "Copy/Copy path on a twin before first run" is unreachable from the UI (`pipeline.rs:352`, `cards.ts:25`, `menu.rs:122-129`, `jobs.rs:227-230`); Task 3 adds an existence check anyway.
- **Decided (user accepted recommendations):** 1 scale applies to the Exact size; 2 yes, paths instead of image; 3 as planned; 4 integrator swaps C/D's `jobs::start` calls to `jobs::start_auto_copied` when merging into `feat/c3-all` (I's function does not exist on `main`, so C/D cannot call it in their worktrees); 5 as planned.

## C watchers (`plans/2026-10-23-ferry-sC-watchers.md`, 11 tasks; new dep `notify` 8.2.0 CC0-1.0, transitive all permissive)
1. Screenshot/recording watching defaults on: on upgrade Ferry starts optimising Desktop screenshots in place and macOS asks for Desktop access. Keep, or default off until onboarding exists? **Recommend: default off.**
2. macOS `.mov` counts as a recording by screenshot marker or name "Screen Recording" (English only). Drop the name fallback if recordings carry the marker? **Recommend: keep the fallback until the manual check answers it.**
3. Watched folders are not recursive. Per-folder option? **Recommend: not this cycle.**
4. Windows uses default `Videos\Captures`; follow a moved folder via registry? **Recommend: not this cycle (cannot test on the Mac).**
5. A watched USB folder on Windows blocks "Safely remove" until paused. Acceptable? **Recommend: yes for now.**
- Cross-stream: `src-tauri/src/watch/hooks.rs` has video/audio/PDF hooks (`VIDEO: bool = false`, `video_job(...) -> None`) for A/B/E to fill at merge.
- **Decided (user accepted recommendations):** 1 default off; 2 keep name fallback until the manual check; 3 no; 4 no; 5 yes. The implementer must change the plan's on-by-default screenshot/recording watching to off.
