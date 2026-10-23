# Handover: Cycle 3, resume on 2026-10-24

Stopped at the user's request at the end of 2026-10-23. Work lives on branch **`claude/clever-cori-t3h3y2`** (pushed; it contains `feat/cycle2-m3`, which is `main` + the stream brief + the first handover). Nothing from Cycle 3 is implemented. `main` on origin is still `37955bf` (v0.2.0).

Read first: [stream brief](../specs/2026-10-23-ferry-cycle3-streams.md), [open questions and decisions](2026-10-23-cycle3-open-questions.md) (every stream's questions, the user's answers, and the integration-task list), and the user rules in [the first handover](2026-10-23-cycle3-plans.md#user-rules-to-keep).

## State of the nine plans (`docs/superpowers/plans/2026-10-23-ferry-s<X>-<name>.md`)

| Stream | Tasks | State |
| --- | --- | --- |
| A video | 15 | Written. **Amendment unfinished** (stopped mid-edit; see below). |
| B audio | 16 | Written. Amendment (`aac_at`) applied; **self-review of the edited sections not done**. |
| C watchers | 11 | Final, amended (captures off by default, no `.avi`). |
| D drop-batch | 11 | Final, amended (Windows tap-only default, click before list keys). |
| E pdf-formats | 20 | Final; decisions match the plan, no amendment needed. |
| F pipelines | 18 | Final. |
| G app-services | 15 | Final. |
| H integrations | 13 | Final. |
| I clipboard-fixes | 14 | Final. |

All user decisions are taken: the user accepted every **Recommend** in the open-questions file, for all nine streams.

## Next steps
1. **Finish A's amendment** (one Opus agent, edits only `sA-video.md`, no commit). Check all three, since the stopped agent's edits are partial and some `x265` mentions remain:
   - x265 removed from the ffmpeg build, configure flags, licence/notices, dependency table, planner, hardware detection, settings, tests, manual checks, Review Focus; HEVC only via hardware encoders (VideoToolbox, NVENC, Quick Sync, AMF); state what happens with no hardware HEVC encoder (not offered, or fall back to H.264).
   - Reuse F's `ferry_core::optimise::Encoder` and `Edits.encoder` exactly as F's plan defines them; if F's definition is not on the branch yet, A adds it verbatim so whichever lands second changes nothing. No conflicting A-side definition.
   - No AVI reference relying on stream C.
   - Also check: B's amendment says whether A's macOS ffmpeg configure needs `--enable-audiotoolbox` for `aac_at`; if so, add it to A's build task.
2. **Self-review B's amended sections** (Opus, edits only `sB-audio.md`): `aac_at` on macOS when present (detected once at start-up through A's `ffmpeg::exec`/`capture`), native `aac` otherwise; argument-builder tests with and without `aac_at`; manual check; cross-stream note to A on `--enable-audiotoolbox`.
3. Commit and push both. Then ask the user to **approve the nine plans**.
4. Outstanding user question (H): which of Dropshare, Yoink, Dropover, Dockside are installed (needed for H's manual check only).
5. **User actions before G's release work runs** (from the G section of the open-questions file): generate the updater signing key, put the public key in `tauri.conf.json`, add the two GitHub secrets.
6. After approval, implementation per the first handover's step 3: one **Sonnet** implementer per plan (`model: "sonnet"`), each in its own worktree from `main`, merged into `feat/c3-all`.
   - Order: A's Tasks 1-4 and 9 before B starts. F image steps first; F's video/audio/PDF registration after A, B, E merge.
   - Then the integration tasks listed under "Decisions" in the open-questions file, plus FR-T3 for copied video and PDF files (needs A, E, I).
   - Then one Opus whole-branch review, fix Critical/Important, then one manual check pass (ask the user before running the live app).
7. Cost note from the user: research-heavy re-dispatches may split research (Sonnet) from plan writing (Opus). Plans and reviews stay on Opus, implementation on Sonnet, never Fable.

## Environment notes
- This cloud session ran on Linux; plans target the user's Intel Mac and Windows CI. The Mac needs `brew install nasm pkgconf meson ninja cmake` for A/E; the first local ffmpeg build takes 30-45 min.
- A session limit stopped six writers once today; resuming each via SendMessage from its transcript worked. Commit and push each plan as soon as it lands.
