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
