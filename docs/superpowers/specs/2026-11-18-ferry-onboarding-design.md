# Onboarding: design

Date: 2026-11-18. Branch: `feat/shelf`, which already contains the shelf and archive work this screen depends on.

## 1. Outcome

A first-run screen sets three defaults the user should choose for themselves:
- what happens to the original file;
- video quality and encoder, with a measured time for each encoder;
- what the shelf holds after making an archive.

**User decisions**
- It is shown once to new and existing users.
- Its choices start from the current settings.
- Encoder times are measured on this Mac.
- Default video quality is 95.
- It has one page.

**Success**
- On the first launch after the update, the window appears in front.
- Done, or closing the window, saves the choices and the window never shows again.
- The three encoder times appear within a few seconds of opening and match how fast real videos encode.
- Nothing is left in the temp folder afterwards.

## 2. Settings

**New setting:** `General.onboarded: bool`, default `false`.
- Serde fills a missing field with the default, so existing settings files load with `onboarded = false` and those users see the screen once.

**Changed default:** `Video::default().compression` goes from 80 to 95.
- Existing files keep their stored value; only fresh settings get 95.

## 3. When it opens

At the end of setup, if `!settings.general.onboarded`, Ferry opens the onboarding window.

The window:
- is a normal, focusable window titled "Welcome to Ferry";
- is 520×680 and not resizable, tall enough to show Done without scrolling;
- is centred and brought to the front, like `open_settings`.

## 4. The page

One column with three sections and a footer.

**1. When Ferry optimises a file.** Two radio cards:
- "Replace the original": sets `location = in_place`. Its note reads "A backup is kept for {days} days", using `backups.retention_days`.
- "Keep the original": sets `location = same_folder`. Its note reads "Saved next to it as photo_optimised.jpg".

The choice sets `images.output.optimised.location` and `video.output.optimised.location`.
- Its starting value is "Keep" if either of them is `same_folder`, and "Replace" otherwise.
- No other location is offered here. A user with `folder` or `temporary` sees "Replace" preselected. To keep their location, Done writes the location only if the user changed the radio.

**2. Videos.**
- A quality slider, 5–100, starting at `video.compression`. Help text: "Lower is smaller."
- Three encoder radio rows, starting at `video.encoder`. Each row shows:
  - the encoder's name and the existing help text;
  - on the right, the measured time: "Measuring…", then "~20 s per min of 1080p", or "—" if it failed.

**3. Shelf archives** (macOS only, since the shelf is macOS only). Two radios:
- "The archive replaces the items" (`replace`);
- "Add the archive beside them" (`add`).

**Footer:** a single primary "Done" button. Enter also triggers it.

**Closing:** the close button acts like Done, saving what is shown. Esc does nothing, so the screen isn't dismissed by accident.

**Done** sends the changed settings through the existing `settings_set` path, with `general.onboarded = true`, then closes the window. Settings pages already listen for `settings:changed`, so they update.

## 5. Measuring the encoders (`ferry_core::video::bench`)

**When:** the page asks once it opens. The work runs in the background and never blocks the UI.

**Sample**
- A temporary folder `ferry-bench-<pid>` inside the OS temp folder, deleted when done.
- In it, ffmpeg makes `sample.mp4`: lavfi `testsrc2`, 1920×1080, 30 fps, 3 s, encoded with libx264 ultrafast at CRF 10 and yuv420p, with no audio.

**Timing:** for each encoder, in order Fast, Small, Visually lossless:
1. probe the sample;
2. build the real `plan::plan` with that encoder, the user's current `video` settings, `hardware::hardware()`, and target `plain_target(Mp4, …)` (MP4);
3. run ffmpeg and time it.

**Result:** `secs_per_min = elapsed × 60 / 3`. If an encoder fails, the result is `None` and the page shows "—".

**Label:**
- "< 5 s per min of 1080p" under 5 s;
- "~N s per min of 1080p" under 90 s;
- "~N min per min of 1080p" from 90 s up.

**Results:** each one reaches the page as it finishes, as the event `onboarding:measured { encoder, label }`, so Fast shows first.

**Cancel:** closing the window cancels any measurement still running, using the existing `CancelToken`.

## 6. Out of scope

- Several pages or a Back button.
- Reopening the screen from the tray.
- Image quality, hotkeys and other settings.
- Measuring real user files.
- Windows-specific polish beyond hiding section 3.
