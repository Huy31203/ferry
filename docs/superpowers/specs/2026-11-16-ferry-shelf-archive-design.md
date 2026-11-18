# Shelf archive: design

Date: 2026-11-16. Branch: `feat/shelf`. Builds on `2026-11-16-ferry-shelf-core-design.md`.

## 1. Outcome

The shelf can pack what it holds into a `.zip`. One click for the common case, plus an "Advanced…" dialog modelled on 7-Zip's Add to Archive for name, level, threads, split volumes and a password.

**User decisions**
- Zip only: no `.7z` or `.tar.*`.
- 7-Zip's expert fields are dropped: dictionary, word size, solid block, memory, parameters, update mode, path mode, SFX and shared files.
- Ferry never deletes the originals.
- By default the result replaces the shelf's items; a setting can add it alongside them instead.

**Success**
- Each menu choice shows a time estimate.
- The archive opens in Finder (except with AES-256 or split volumes, and the dialog says so).
- A cancel or failure leaves no file behind.
- An existing file is never overwritten.

## 2. Menu

The shelf menu gains "Add to archive ▸" after "Optimise all". Its submenu:

| Item | Level | Threads |
|---|---|---|
| Fast · ~3 s | 1 | all cores |
| Balanced · ~8 s | 5 | all cores |
| Smallest · ~20 s | 9 | all cores |
| — | | |
| Advanced… | dialog | |

The quick items use no password and no split. They name the archive the way Finder's Compress does:
- one item: `<item name>.zip`;
- more than one: `Archive.zip`.

The archive goes in the first item's folder. If that name is taken, the next free `Archive 2.zip`, `Archive 3.zip` is used; a name counts as taken if the file or its `.001` volume exists.

While an archive is being made, "Add to archive ▸" becomes "Stop archiving". Only one archive runs at a time.

## 3. Advanced dialog

A normal, focusable window. The shelf panel is non-key and can't take a password. It is laid out like 7-Zip.

**Left column**
- Archive: the folder path, a name field, and a "…" button that opens a save panel.
- Compression level: 0 Store, 1 Fastest, 3 Fast, 5 Normal, 7 Maximum, 9 Ultra. There is no method field: level 0 is Store, and every other level is Deflate.
- CPU threads: 1 to the number of cores, shown as "/ N".
- Split to volumes: free text with presets 10M, 100M, 650M, 700M, 4092M and 4480M. k, M and G are binary multiples, and the minimum is 64k. The help text says Finder can't open `.zip.001`, but 7-Zip, Keka and The Unarchiver can.
- "Estimated time: ~12 s", updated live.

**Right column, Encryption**
- Password and Reenter password fields, with a Show password option.
- Encryption method:
  - AES-256 (default). Help text: "Finder can't open AES-256 zips. Use Keka or The Unarchiver."
  - ZipCrypto. Help text: "Weak protection, but Finder can open it."
- **Open question for the user's manual check:** does Archive Utility really open a ZipCrypto zip? If it doesn't, drop ZipCrypto.
- If the passwords differ, the dialog shows "The passwords don't match" and OK is disabled. An empty password means no encryption.

**Footer:** Cancel and OK. Esc cancels and Enter means OK.

**Errors shown inline:**
- an empty name;
- a bad volume size;
- "<name> already exists there". The dialog never overwrites, even when the save panel offered Replace.

The dialog remembers the level and threads for the session. It never remembers the password.

## 4. Engine (`ferry_core::archive`)

**Collect**
- Each shelf item becomes a top-level entry under its own name. Clashing names get " 2" before the extension (`photo 2.png`).
- Folders are walked in sorted order, and each folder is also stored as a directory entry.
- Symlinks inside folders and `.DS_Store` are skipped.
- Collecting happens before anything is written, so an archive that lands inside an archived folder never contains itself.

**Write**
- Files are split into `threads` groups by size, largest first, each to the least-loaded group.
- Each group is compressed on its own rayon thread into a part zip, in a hidden work folder `.<name>.ferry/` next to the output.
- The parts are then merged with `ZipWriter::merge_archive`, a raw copy, so encrypted entries stay encrypted, into `out.zip` in the work folder. Directory entries come first.
- Each entry carries:
  - its modification time (UTC);
  - its Unix permissions;
  - `large_file` (zip64) when the file is 4 GiB or more.

**Finish**
- Check cancel, check again that the target is free, then rename into place.
- With a split size smaller than the archive, the file is cut into `<name>.zip.001`, `.002` and so on, in place of the rename.
- The work folder is removed on every path.
- Cancel is Ferry's `CancelToken`, checked between 1 MiB chunks. A cancel returns `ErrorKind::Interrupted`; a taken target returns `AlreadyExists`.

**Progress:** a callback with the bytes done so far, against the sum of the file sizes.

**Crate:** `zip = { version = "4.6", default-features = false, features = ["deflate-flate2", "aes-crypto", "time"] }`, plus `time = "0.3"`. Not `deflate-flate2-zlib-rs`, which would switch flate2's backend for the PDF code too.

## 5. Time estimate

`Rates` holds MB/s per level, per worker. The seeds are conservative single-thread Deflate figures:
- level 0: 800
- levels 1 to 9: 120, 110, 90, 70, 45, 35, 25, 18, 15

The estimate is `bytes / (rate × workers)`, where workers is `min(threads, file count)`.

After each archive of 8 MB or more, the observed per-worker rate is folded in: `rate = 0.7·rate + 0.3·observed`. The rates are kept in `archive_rates.json` in the app data folder; a missing or corrupt file means the seeds.

Labels read "< 1 s", "~N s" (under 90 s) or "~N min".

## 6. Progress, result and errors

**Progress**
- While running, the shelf's pill reads "Archiving 42%".
- The page gets `shelf:archive { progress }` events at most 5 times a second, and one with `progress: null` at the end.

**Result** (setting `floating.shelf_archive_result`)
- `replace` (default): the archived items leave the shelf, never the disk, and so do their shelved-card links. The archive (or its volumes) is added.
- `add`: the archive is added alongside the items.
- Either way, the shelf is summoned so the result is seen.
- After a replace there is no Undo: the originals are untouched on disk.

**Errors:** a failure shows a notice card, "Couldn't make the archive: <reason>". A cancel shows nothing.

**Setting:** `Floating.shelf_archive_result: ArchiveResult { Replace, Add }`, shown as a select in the Floating pane under Shelf opacity. Onboarding will ask it later.

## 7. Out of scope

- `.7z`, `.tar.*`, Zstd and other methods.
- Encrypted file names, which zip can't do.
- Deleting the originals.
- Undo of a replace.
- Archiving from anywhere but the shelf.
