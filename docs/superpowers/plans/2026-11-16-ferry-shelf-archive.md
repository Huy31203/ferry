# Shelf Archive Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task by task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** The shelf packs its items into a `.zip`. "Add to archive ▸" offers Fast, Balanced and Smallest, each with a time estimate, plus a 7-Zip-style Advanced dialog with name, level, threads, split volumes and a password.

**Architecture:**
- A pure engine in `ferry_core::archive` covers collecting entries, writing in parallel, splitting and estimating.
- A runner in `src-tauri/src/shelf/archive.rs` handles one archive at a time, with progress events, cancel and the result swap.
- A submenu goes in `shelf/menu.rs`.
- The dialog is a new window: `src-tauri/src/shelf/archive_dialog.rs` and `src/archive/Archive.svelte`.

**Tech Stack:**
- Rust: the `zip` 4.6 crate (Deflate, AES, time) and rayon.
- Tauri 2, Svelte 5 runes, vitest.

**Spec:** `docs/superpowers/specs/2026-11-16-ferry-shelf-archive-design.md`. Read it first; it's the authority.

## Global Constraints

- Read AGENTS.md and CONSTRAINTS.md at the repo root if present, and follow both. Today neither exists.
- Shell prefix for every command:
  ```
  export PATH=/usr/local/bin:$HOME/.nvm/versions/node/v22.23.4/bin:$HOME/.cargo/bin:$PATH
  ```
- Core tests need `FERRY_GIFSICLE=$PWD/src-tauri/binaries/gifsicle-x86_64-apple-darwin FERRY_FFMPEG=$PWD/src-tauri/binaries/ffmpeg-x86_64-apple-darwin`.
- Never use `var` in JS or TS. Keep code minimal and reuse the nearest existing pattern.
- No manual end-to-end runs or screenshots: the user tests by hand at the end. Do not stop or relaunch the running dev app or the installed Ferry.
- `zip` uses `default-features = false` with features `["deflate-flate2", "aes-crypto", "time"]`. Never `deflate-flate2-zlib-rs`, which switches flate2's backend for the PDF code.
- Ferry never overwrites an existing file and never deletes an original.
- Every commit message ends with:
  ```

  Co-Authored-By: Claude Opus 5.5 <noreply@anthropic.com>
  ```
- Final gate, from the repo root:
  ```
  cargo fmt --all --check && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace && npx svelte-check && npx vitest run
  ```

## Review Focus

1. **A 4 GiB+ file.** The entry is written as zip64 (`large_file`). Pinned by `zip64_from_4_gib` in Task 2.
2. **An encrypted archive with threads > 1.** The merged entries stay encrypted and decrypt with the password. Pinned by `aes_round_trip_across_threads` in Task 2.
3. **A target that exists, including as `.001`.** It is refused and the existing file is untouched. Pinned by `refuses_an_existing_target` (Task 2) and `free_path_skips_taken_names_and_volumes` (Task 1).
4. **Cancel mid-way.** No output and no `.name.ferry` folder are left. Pinned by `cancel_leaves_nothing` in Task 2.
5. **The output lands inside an archived folder.** The archive doesn't contain itself or its work folder. Pinned by `archive_inside_an_archived_folder_skips_itself` in Task 2.
6. **Not pinned, a manual note:** a shelf of huge folders makes the menu walk the folders when it opens. This is accepted for v1.

---

### Task 1: Collect entries and pick names (`ferry_core::archive`)

**Files:**
- Create:
  - `crates/ferry-core/src/archive/mod.rs`
  - `crates/ferry-core/src/archive/entries.rs`
- Modify:
  - `crates/ferry-core/src/lib.rs` (add `pub mod archive;` after `pub mod audio;`)
  - `crates/ferry-core/Cargo.toml`

**Interfaces:**
- Produces:
  - `archive::Entry { source: PathBuf, name: String, size: u64 }` with `is_dir()`
  - `archive::collect(items: &[PathBuf]) -> io::Result<Vec<Entry>>`
  - `archive::default_name(items: &[PathBuf]) -> String`
  - `archive::free_path(folder: &Path, name: &str) -> PathBuf`
  - `archive::taken(path: &Path) -> bool`

- [ ] **Step 1: Dependencies.** In `crates/ferry-core/Cargo.toml` `[dependencies]`, add:

```toml
# shelf archive: zip only, Deflate + AES, no zlib-rs (would switch the PDF code's flate2 backend).
zip = { version = "4.6", default-features = false, features = ["deflate-flate2", "aes-crypto", "time"] }
time = "0.3"
```

Run `cargo fetch`. It pulls `aes 0.8` and friends from the network. Expected: success.

- [ ] **Step 2: Failing tests.** Write `archive/mod.rs`:

```rust
//! The shelf's "Add to archive" (shelf archive spec): zip only.

mod entries;

pub use entries::{Entry, collect, default_name, free_path, taken};
```

Write `archive/entries.rs` with only the tests below and `use super::*` stubs failing to compile:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn clashing_item_names_get_a_number_before_the_extension() {
        let dir = tempfile::tempdir().unwrap();
        let (a, b) = (dir.path().join("a"), dir.path().join("b"));
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        fs::write(a.join("photo.png"), b"1").unwrap();
        fs::write(b.join("photo.png"), b"22").unwrap();
        let got = collect(&[a.join("photo.png"), b.join("photo.png")]).unwrap();
        let names: Vec<_> = got.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names, ["photo.png", "photo 2.png"]);
        assert_eq!(got[1].size, 2);
    }

    #[test]
    fn folders_are_walked_sorted_without_ds_store_or_symlinks() {
        let dir = tempfile::tempdir().unwrap();
        let top = dir.path().join("pics");
        fs::create_dir_all(top.join("sub")).unwrap();
        fs::write(top.join("b.txt"), b"b").unwrap();
        fs::write(top.join("a.txt"), b"a").unwrap();
        fs::write(top.join(".DS_Store"), b"x").unwrap();
        fs::write(top.join("sub/c.txt"), b"c").unwrap();
        #[cfg(unix)]
        std::os::unix::fs::symlink(top.join("a.txt"), top.join("link")).unwrap();
        let names: Vec<_> = collect(&[top]).unwrap().into_iter().map(|e| e.name).collect();
        assert_eq!(names, ["pics/", "pics/a.txt", "pics/b.txt", "pics/sub/", "pics/sub/c.txt"]);
    }

    #[test]
    fn default_name_follows_finder() {
        assert_eq!(default_name(&["/x/a.png".into()]), "a.png.zip");
        assert_eq!(default_name(&["/x/folder".into()]), "folder.zip");
        assert_eq!(default_name(&["/x/a".into(), "/x/b".into()]), "Archive.zip");
    }

    #[test]
    fn free_path_skips_taken_names_and_volumes() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(free_path(dir.path(), "Archive.zip"), dir.path().join("Archive.zip"));
        fs::write(dir.path().join("Archive.zip"), b"").unwrap();
        fs::write(dir.path().join("Archive 2.zip.001"), b"").unwrap();
        assert_eq!(free_path(dir.path(), "Archive.zip"), dir.path().join("Archive 3.zip"));
    }
}
```

- [ ] **Step 3: Run it to see it fail.** Run `cargo test -p ferry-core archive::`. Expected: a compile error, because the functions are missing.

- [ ] **Step 4: Implement** above the tests in `entries.rs`:

```rust
//! What goes into the archive: each shelf item under its own name, folders walked.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq)]
pub struct Entry {
    pub source: PathBuf,
    /// The path inside the zip, `/`-separated; a folder ends in `/`.
    pub name: String,
    /// Bytes for a file, 0 for a folder.
    pub size: u64,
}

impl Entry {
    pub fn is_dir(&self) -> bool {
        self.name.ends_with('/')
    }
}

/// Every item as a top-level entry; clashing names get " 2", " 3" before the
/// extension. Folders are walked sorted; symlinks inside and `.DS_Store` are skipped.
/// Runs before anything is written, so the archive never contains itself.
pub fn collect(items: &[PathBuf]) -> io::Result<Vec<Entry>> {
    let mut out = Vec::new();
    let mut taken_names: Vec<String> = Vec::new();
    for item in items {
        let name = unique(&file_name(item), &taken_names);
        taken_names.push(name.clone());
        let meta = fs::metadata(item)?;
        if meta.is_dir() {
            walk(item, &name, &mut out)?;
        } else if meta.is_file() {
            out.push(Entry { source: item.clone(), name, size: meta.len() });
        }
    }
    Ok(out)
}

fn walk(dir: &Path, name: &str, out: &mut Vec<Entry>) -> io::Result<()> {
    out.push(Entry { source: dir.to_path_buf(), name: format!("{name}/"), size: 0 });
    let mut children = fs::read_dir(dir)?.collect::<Result<Vec<_>, _>>()?;
    children.sort_by_key(|c| c.file_name());
    for child in children {
        let file = child.file_name().to_string_lossy().into_owned();
        if file == ".DS_Store" {
            continue;
        }
        let (path, inner) = (child.path(), format!("{name}/{file}"));
        let meta = fs::symlink_metadata(&path)?;
        if meta.is_dir() {
            walk(&path, &inner, out)?;
        } else if meta.is_file() {
            out.push(Entry { source: path, name: inner, size: meta.len() });
        }
    }
    Ok(())
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned())
}

fn unique(base: &str, taken_names: &[String]) -> String {
    if !taken_names.iter().any(|t| t == base) {
        return base.to_string();
    }
    let (stem, ext) = match base.rfind('.') {
        Some(i) if i > 0 => base.split_at(i),
        _ => (base, ""),
    };
    (2..)
        .map(|n| format!("{stem} {n}{ext}"))
        .find(|c| !taken_names.contains(c))
        .expect("an unused number exists")
}

/// As Finder's Compress: one item is `<name>.zip`, more are `Archive.zip`.
pub fn default_name(items: &[PathBuf]) -> String {
    match items {
        [one] => format!("{}.zip", file_name(one)),
        _ => "Archive.zip".to_string(),
    }
}

/// A target counts as taken if it or its first volume exists.
pub fn taken(path: &Path) -> bool {
    path.exists() || PathBuf::from(format!("{}.001", path.display())).exists()
}

/// `name` in `folder`, or the first free `<stem> 2.zip`, `<stem> 3.zip`…
pub fn free_path(folder: &Path, name: &str) -> PathBuf {
    let first = folder.join(name);
    if !taken(&first) {
        return first;
    }
    let (stem, ext) = name.strip_suffix(".zip").map_or((name, ""), |s| (s, ".zip"));
    (2..)
        .map(|n| folder.join(format!("{stem} {n}{ext}")))
        .find(|p| !taken(p))
        .expect("an unused number exists")
}
```

- [ ] **Step 5: Run.** Run `cargo test -p ferry-core archive::`. Expected: 4 passed.

- [ ] **Step 6: Commit.**

```bash
git add Cargo.lock crates/ferry-core/Cargo.toml crates/ferry-core/src/lib.rs crates/ferry-core/src/archive
git commit -m "feat(archive): collect shelf items into zip entries with Finder-style names"
```

(Add the trailer from Global Constraints to every commit.)

---

### Task 2: Write the zip (parallel parts, encryption, split, cancel)

**Files:**
- Create: `crates/ferry-core/src/archive/write.rs`
- Modify: `crates/ferry-core/src/archive/mod.rs` (`mod write; pub use write::{Encryption, Options, parse_size, write};`)

**Interfaces:**
- Consumes: `Entry`, `taken` (Task 1), `crate::cancel::CancelToken`.
- Produces:
  - `pub enum Encryption { Aes256, ZipCrypto }`
  - `pub struct Options { pub level: u32, pub threads: usize, pub password: Option<String>, pub encryption: Encryption, pub split: Option<u64> }`
  - `pub fn write(entries: &[Entry], out: &Path, opts: &Options, cancel: &CancelToken, progress: &(dyn Fn(u64) + Sync)) -> io::Result<Vec<PathBuf>>`. It returns the files made: the zip, or its volumes. A cancel gives `ErrorKind::Interrupted`; a taken target gives `ErrorKind::AlreadyExists`.
  - `pub fn parse_size(text: &str) -> Option<u64>`

- [ ] **Step 1: Failing tests** at the bottom of `write.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::collect;
    use std::fs;
    use std::io::Read;

    fn opts(level: u32, threads: usize) -> Options {
        Options { level, threads, password: None, encryption: Encryption::Aes256, split: None }
    }

    fn fixture(dir: &Path) -> Vec<PathBuf> {
        let folder = dir.join("docs");
        fs::create_dir_all(folder.join("deep")).unwrap();
        fs::write(folder.join("a.txt"), "alpha ".repeat(5000)).unwrap();
        fs::write(folder.join("deep/b.txt"), "beta ".repeat(3000)).unwrap();
        fs::write(dir.join("c.bin"), (0..200_000u32).map(|i| (i % 251) as u8).collect::<Vec<_>>()).unwrap();
        vec![folder, dir.join("c.bin")]
    }

    fn read_all(zip: &Path, password: Option<&str>) -> Vec<(String, Vec<u8>)> {
        let mut archive = zip::ZipArchive::new(fs::File::open(zip).unwrap()).unwrap();
        let mut out = Vec::new();
        for i in 0..archive.len() {
            let mut file = match password {
                Some(p) => archive.by_index_decrypt(i, p.as_bytes()).unwrap(),
                None => archive.by_index(i).unwrap(),
            };
            let mut bytes = Vec::new();
            file.read_to_end(&mut bytes).unwrap();
            out.push((file.name().to_string(), bytes));
        }
        out.sort();
        out
    }

    fn expected(dir: &Path) -> Vec<(String, Vec<u8>)> {
        let mut v = vec![
            ("c.bin".to_string(), fs::read(dir.join("c.bin")).unwrap()),
            ("docs/".to_string(), vec![]),
            ("docs/a.txt".to_string(), fs::read(dir.join("docs/a.txt")).unwrap()),
            ("docs/deep/".to_string(), vec![]),
            ("docs/deep/b.txt".to_string(), fs::read(dir.join("docs/deep/b.txt")).unwrap()),
        ];
        v.sort();
        v
    }

    #[test]
    fn round_trips_at_every_level_and_thread_count() {
        for (level, threads) in [(0, 1), (1, 4), (5, 2), (9, 1)] {
            let dir = tempfile::tempdir().unwrap();
            let entries = collect(&fixture(dir.path())).unwrap();
            let out = dir.path().join("out/x.zip");
            fs::create_dir_all(out.parent().unwrap()).unwrap();
            let made = write(&entries, &out, &opts(level, threads), &CancelToken::new(), &|_| {}).unwrap();
            assert_eq!(made, vec![out.clone()]);
            assert_eq!(read_all(&out, None), expected(dir.path()), "level {level} threads {threads}");
            assert!(!dir.path().join("out/.x.zip.ferry").exists());
        }
    }

    #[test]
    fn aes_round_trip_across_threads() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options { password: Some("s3cret".into()), ..opts(5, 4) };
        write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap();
        assert_eq!(read_all(&out, Some("s3cret")), expected(dir.path()));
        let mut archive = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
        let file = archive.by_name("c.bin").map(|_| ());
        assert!(file.is_err(), "an encrypted entry must not open without the password");
    }

    #[test]
    fn zipcrypto_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options { password: Some("pw".into()), encryption: Encryption::ZipCrypto, ..opts(5, 2) };
        write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap();
        assert_eq!(read_all(&out, Some("pw")), expected(dir.path()));
    }

    #[test]
    fn refuses_an_existing_target() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        fs::write(&out, b"keep me").unwrap();
        let err = write(&entries, &out, &opts(1, 1), &CancelToken::new(), &|_| {}).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::AlreadyExists);
        assert_eq!(fs::read(&out).unwrap(), b"keep me");
    }

    #[test]
    fn cancel_leaves_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let cancel = CancelToken::new();
        cancel.cancel();
        let err = write(&entries, &out, &opts(1, 2), &cancel, &|_| {}).unwrap_err();
        assert_eq!(err.kind(), io::ErrorKind::Interrupted);
        assert!(!out.exists());
        assert!(!dir.path().join(".x.zip.ferry").exists());
    }

    #[test]
    fn archive_inside_an_archived_folder_skips_itself() {
        let dir = tempfile::tempdir().unwrap();
        let items = fixture(dir.path());
        let entries = collect(&items).unwrap();
        let out = dir.path().join("docs/x.zip");
        write(&entries, &out, &opts(1, 2), &CancelToken::new(), &|_| {}).unwrap();
        let names: Vec<_> = read_all(&out, None).into_iter().map(|(n, _)| n).collect();
        assert!(names.iter().all(|n| !n.contains("x.zip")), "{names:?}");
    }

    #[test]
    fn progress_reaches_the_total() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let total: u64 = entries.iter().map(|e| e.size).sum();
        let last = std::sync::atomic::AtomicU64::new(0);
        let out = dir.path().join("x.zip");
        write(&entries, &out, &opts(1, 3), &CancelToken::new(), &|n| {
            last.fetch_max(n, std::sync::atomic::Ordering::SeqCst);
        })
        .unwrap();
        assert_eq!(last.into_inner(), total);
    }

    #[test]
    fn splits_into_numbered_volumes_that_join_back() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let whole = dir.path().join("whole.zip");
        write(&entries, &whole, &opts(0, 1), &CancelToken::new(), &|_| {}).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options { split: Some(64 * 1024), ..opts(0, 1) };
        let made = write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap();
        assert!(made.len() >= 3, "{made:?}");
        assert_eq!(made[0], dir.path().join("x.zip.001"));
        assert!(!out.exists());
        let joined: Vec<u8> = made.iter().flat_map(|p| fs::read(p).unwrap()).collect();
        fs::write(dir.path().join("joined.zip"), &joined).unwrap();
        assert_eq!(read_all(&dir.path().join("joined.zip"), None), expected(dir.path()));
    }

    #[test]
    fn a_split_bigger_than_the_archive_keeps_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options { split: Some(1 << 30), ..opts(1, 1) };
        assert_eq!(write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap(), vec![out]);
    }

    #[test]
    fn zip64_from_4_gib() {
        assert!(!needs_zip64(u64::from(u32::MAX) - 1));
        assert!(needs_zip64(u64::from(u32::MAX)));
    }

    #[test]
    fn parse_size_reads_7zip_style_volume_sizes() {
        assert_eq!(parse_size("100M"), Some(100 << 20));
        assert_eq!(parse_size(" 650m "), Some(650 << 20));
        assert_eq!(parse_size("4g"), Some(4 << 30));
        assert_eq!(parse_size("64k"), Some(64 << 10));
        assert_eq!(parse_size("70000"), Some(70000));
        assert_eq!(parse_size("1k"), None, "under 64k");
        assert_eq!(parse_size(""), None);
        assert_eq!(parse_size("12x"), None);
    }
}
```

`progress_reaches_the_total` expects the callback to get the **cumulative** byte count.

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p ferry-core archive::write`. Expected: a compile error.

- [ ] **Step 3: Implement** `write.rs` above the tests:

```rust
//! Write the zip: files split into `threads` groups, each group compressed into a
//! part zip on its own thread, then the parts merged raw (still encrypted) into the
//! final archive. Everything happens in a hidden work folder next to the output;
//! only a finished archive is moved into place.

use std::fs::{self, File};
use std::io::{self, BufWriter, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use rayon::prelude::*;
use zip::unstable::write::FileOptionsExt;
use zip::write::FileOptions;
use zip::{AesMode, CompressionMethod, ZipArchive, ZipWriter};

use super::{Entry, taken};
use crate::cancel::CancelToken;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encryption {
    Aes256,
    ZipCrypto,
}

#[derive(Debug, Clone)]
pub struct Options {
    /// 0 stores; 1–9 deflate at that level.
    pub level: u32,
    pub threads: usize,
    /// `None` (or empty) writes no encryption.
    pub password: Option<String>,
    pub encryption: Encryption,
    /// Volume size in bytes; `None` writes one file.
    pub split: Option<u64>,
}

const CHUNK: usize = 1 << 20;

pub fn write(
    entries: &[Entry],
    out: &Path,
    opts: &Options,
    cancel: &CancelToken,
    progress: &(dyn Fn(u64) + Sync),
) -> io::Result<Vec<PathBuf>> {
    if taken(out) {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    let work = work_dir(out);
    let _ = fs::remove_dir_all(&work);
    fs::create_dir_all(&work)?;
    let _cleanup = RemoveOnDrop(work.clone());

    let groups = groups(entries, opts.threads.max(1));
    let done = AtomicU64::new(0);
    let tick = |n: u64| progress(done.fetch_add(n, Ordering::SeqCst) + n);
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(groups.len().max(1))
        .build()
        .map_err(io::Error::other)?;
    let parts: Vec<PathBuf> = pool.install(|| {
        groups
            .par_iter()
            .enumerate()
            .map(|(i, group)| part(&work.join(format!("{i}.zip")), group, opts, cancel, &tick))
            .collect::<io::Result<_>>()
    })?;

    let tmp = work.join("out.zip");
    let mut zip = ZipWriter::new(BufWriter::new(File::create(&tmp)?));
    for dir in entries.iter().filter(|e| e.is_dir()) {
        zip.add_directory(dir.name.trim_end_matches('/'), base_options(dir))
            .map_err(io::Error::other)?;
    }
    for p in &parts {
        zip.merge_archive(ZipArchive::new(File::open(p)?).map_err(io::Error::other)?)
            .map_err(io::Error::other)?;
    }
    zip.finish().map_err(io::Error::other)?.flush()?;

    if cancel.is_cancelled() {
        return Err(io::ErrorKind::Interrupted.into());
    }
    if taken(out) {
        return Err(io::ErrorKind::AlreadyExists.into());
    }
    match opts.split {
        Some(size) if fs::metadata(&tmp)?.len() > size => split(&tmp, out, size),
        _ => {
            fs::rename(&tmp, out)?;
            Ok(vec![out.to_path_buf()])
        }
    }
}

fn work_dir(out: &Path) -> PathBuf {
    let name = out.file_name().map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    out.with_file_name(format!(".{name}.ferry"))
}

struct RemoveOnDrop(PathBuf);

impl Drop for RemoveOnDrop {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

/// Files only, largest first, each to the least-loaded group. Empty groups are dropped.
fn groups(entries: &[Entry], threads: usize) -> Vec<Vec<&Entry>> {
    let mut files: Vec<&Entry> = entries.iter().filter(|e| !e.is_dir()).collect();
    files.sort_by_key(|e| std::cmp::Reverse(e.size));
    let mut groups: Vec<(u64, Vec<&Entry>)> = (0..threads).map(|_| (0, Vec::new())).collect();
    for file in files {
        let lightest = groups.iter_mut().min_by_key(|(load, _)| *load).expect("threads >= 1");
        lightest.0 += file.size;
        lightest.1.push(file);
    }
    groups.into_iter().map(|(_, g)| g).filter(|g| !g.is_empty()).collect()
}

fn part(
    path: &Path,
    group: &[&Entry],
    opts: &Options,
    cancel: &CancelToken,
    tick: &(dyn Fn(u64) + Sync),
) -> io::Result<PathBuf> {
    let mut zip = ZipWriter::new(BufWriter::new(File::create(path)?));
    let mut buf = vec![0u8; CHUNK];
    for entry in group {
        let options = encrypted(file_options(entry, opts.level), opts);
        zip.start_file(entry.name.as_str(), options).map_err(io::Error::other)?;
        let mut src = File::open(&entry.source)?;
        loop {
            if cancel.is_cancelled() {
                return Err(io::ErrorKind::Interrupted.into());
            }
            let n = src.read(&mut buf)?;
            if n == 0 {
                break;
            }
            zip.write_all(&buf[..n])?;
            tick(n as u64);
        }
    }
    zip.finish().map_err(io::Error::other)?.flush()?;
    Ok(path.to_path_buf())
}

fn needs_zip64(size: u64) -> bool {
    size >= u64::from(u32::MAX)
}

/// Modification time (UTC) and Unix permissions, as the source has them.
fn base_options(entry: &Entry) -> FileOptions<'static, ()> {
    let mut o = FileOptions::<()>::default();
    if let Ok(meta) = fs::metadata(&entry.source) {
        if let Some(t) = meta
            .modified()
            .ok()
            .and_then(|m| zip::DateTime::try_from(time::OffsetDateTime::from(m)).ok())
        {
            o = o.last_modified_time(t);
        }
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            o = o.unix_permissions(meta.permissions().mode());
        }
    }
    o
}

fn file_options(entry: &Entry, level: u32) -> FileOptions<'static, ()> {
    let (method, level) = if level == 0 {
        (CompressionMethod::Stored, None)
    } else {
        (CompressionMethod::Deflated, Some(i64::from(level.min(9))))
    };
    base_options(entry)
        .compression_method(method)
        .compression_level(level)
        .large_file(needs_zip64(entry.size))
}

fn encrypted<'a>(o: FileOptions<'static, ()>, opts: &'a Options) -> FileOptions<'a, ()> {
    match (opts.password.as_deref().filter(|p| !p.is_empty()), opts.encryption) {
        (Some(p), Encryption::Aes256) => o.with_aes_encryption(AesMode::Aes256, p),
        (Some(p), Encryption::ZipCrypto) => o.with_deprecated_encryption(p.as_bytes()),
        (None, _) => o,
    }
}

/// Cut `tmp` into `<out>.001`, `.002`…; on failure remove the volumes already made.
fn split(tmp: &Path, out: &Path, size: u64) -> io::Result<Vec<PathBuf>> {
    let mut made = Vec::new();
    let result = (|| {
        let mut src = File::open(tmp)?;
        for n in 1.. {
            let path = PathBuf::from(format!("{}.{n:03}", out.display()));
            let mut dst = File::create_new(&path)?;
            made.push(path);
            if io::copy(&mut (&mut src).take(size), &mut dst)? < size {
                break;
            }
        }
        Ok(())
    })();
    match result {
        Ok(()) => {
            // An exact multiple leaves an empty last volume; drop it.
            if made.last().is_some_and(|p| fs::metadata(p).is_ok_and(|m| m.len() == 0)) {
                let _ = fs::remove_file(made.pop().expect("checked"));
            }
            Ok(made)
        }
        Err(e) => {
            for p in &made {
                let _ = fs::remove_file(p);
            }
            Err(e)
        }
    }
}

/// "100M", "650m", "4g", "64k" or plain bytes; binary multiples; at least 64 KiB.
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let (digits, shift) = match text.char_indices().last()? {
        (i, 'k' | 'K') => (&text[..i], 10),
        (i, 'm' | 'M') => (&text[..i], 20),
        (i, 'g' | 'G') => (&text[..i], 30),
        _ => (text, 0),
    };
    let n: u64 = digits.trim().parse().ok()?;
    n.checked_shl(shift).filter(|&b| b >= 64 << 10 && b >> shift == n)
}
```

If a `zip` 4.6 API name differs (for example `add_directory`'s argument types, or `with_aes_encryption` returning a different lifetime), read the signature in `~/.cargo/registry/src/*/zip-4.6.1/src/write.rs` and adapt. Ledger any adaptation as a Ruling.

- [ ] **Step 4: Run.** Run `cargo test -p ferry-core archive::`. Expected: all archive tests pass (15).

- [ ] **Step 5: The PDF code is unchanged.** Run `FERRY_GIFSICLE=… FERRY_FFMPEG=… cargo test -p ferry-core pdf`. Expected: passes; it proves the flate2 backend didn't change.

- [ ] **Step 6: Commit** `feat(archive): parallel zip writer with AES/ZipCrypto, volumes and clean cancel`.

---

### Task 3: Time estimates and the result setting

**Files:**
- Create: `crates/ferry-core/src/archive/rates.rs`
- Modify:
  - `crates/ferry-core/src/archive/mod.rs` (`mod rates; pub use rates::Rates;`)
  - `crates/ferry-core/src/settings.rs`
  - `src/lib/settings.ts`, `src/lib/settings.fixture.ts`, `src/lib/schema.ts`, `src/lib/i18n/en.json`

**Interfaces:**
- Produces:
  - `Rates { pub mb_per_sec: [f64; 10] }` with `estimate(&self, bytes: u64, level: u32, workers: usize) -> f64` (seconds), `record(&mut self, bytes: u64, level: u32, workers: usize, secs: f64)`, `load(path: &Path) -> Rates` and `save(&self, path: &Path) -> io::Result<()>`
  - `settings::ArchiveResult { Replace, Add }` (serde `snake_case`, default `Replace`)
  - `Floating.shelf_archive_result: ArchiveResult`

- [ ] **Step 1: Failing tests** in `rates.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn estimate_divides_by_rate_and_workers() {
        let r = Rates::default();
        assert!((r.estimate(120_000_000, 1, 1) - 1.0).abs() < 1e-9);
        assert!((r.estimate(120_000_000, 1, 4) - 0.25).abs() < 1e-9);
        assert!((r.estimate(15_000_000, 9, 0) - 1.0).abs() < 1e-9, "0 workers counts as 1");
    }

    #[test]
    fn record_moves_toward_what_was_seen() {
        let mut r = Rates::default();
        r.record(100_000_000, 5, 2, 1.0); // 50 MB/s per worker
        assert!((r.mb_per_sec[5] - (0.7 * 45.0 + 0.3 * 50.0)).abs() < 1e-9);
    }

    #[test]
    fn small_or_instant_runs_teach_nothing() {
        let mut r = Rates::default();
        r.record(1_000_000, 5, 1, 0.5);
        r.record(100_000_000, 5, 1, 0.0);
        assert_eq!(r, Rates::default());
    }

    #[test]
    fn a_missing_or_corrupt_file_means_the_seeds() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("archive_rates.json");
        assert_eq!(Rates::load(&path), Rates::default());
        std::fs::write(&path, b"{nope").unwrap();
        assert_eq!(Rates::load(&path), Rates::default());
        let mut r = Rates::default();
        r.mb_per_sec[3] = 77.0;
        r.save(&path).unwrap();
        assert_eq!(Rates::load(&path), r);
    }
}
```

In `settings.rs`'s test module, add:

```rust
    #[test]
    fn shelf_archive_result_defaults_to_replace_and_reads_add() {
        assert_eq!(Settings::default().floating.shelf_archive_result, ArchiveResult::Replace);
        let s: Floating = serde_json::from_str(r#"{"shelf_archive_result":"add"}"#).unwrap();
        assert_eq!(s.shelf_archive_result, ArchiveResult::Add);
    }
```

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p ferry-core rates` and `cargo test -p ferry-core shelf_archive_result`. Expected: compile errors.

- [ ] **Step 3: Implement** `rates.rs`:

```rust
//! Time estimates for "Add to archive" from measured throughput (spec §5).

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// MB/s per worker for levels 0–9, learned from finished archives.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rates {
    pub mb_per_sec: [f64; 10],
}

impl Default for Rates {
    /// Conservative single-thread Deflate figures.
    fn default() -> Self {
        Self { mb_per_sec: [800.0, 120.0, 110.0, 90.0, 70.0, 45.0, 35.0, 25.0, 18.0, 15.0] }
    }
}

const MIN_BYTES: u64 = 8_000_000;

impl Rates {
    pub fn estimate(&self, bytes: u64, level: u32, workers: usize) -> f64 {
        bytes as f64 / 1e6 / (self.rate(level) * workers.max(1) as f64)
    }

    pub fn record(&mut self, bytes: u64, level: u32, workers: usize, secs: f64) {
        if bytes < MIN_BYTES || secs <= 0.0 {
            return;
        }
        let seen = bytes as f64 / 1e6 / secs / workers.max(1) as f64;
        let i = level.min(9) as usize;
        self.mb_per_sec[i] = 0.7 * self.mb_per_sec[i] + 0.3 * seen;
    }

    fn rate(&self, level: u32) -> f64 {
        self.mb_per_sec[level.min(9) as usize].max(0.1)
    }

    pub fn load(path: &Path) -> Self {
        std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default()
    }

    pub fn save(&self, path: &Path) -> io::Result<()> {
        std::fs::write(path, serde_json::to_vec(self).map_err(io::Error::other)?)
    }
}
```

In `settings.rs`, next to `CardSize`:

```rust
/// What the shelf holds after "Add to archive" (shelf archive spec §6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArchiveResult {
    #[default]
    Replace,
    Add,
}
```

Then add these, matching how `shelf_opacity` is declared and defaulted:
- in `Floating`, after `shelf_opacity`: `pub shelf_archive_result: ArchiveResult,` with a doc comment;
- in `Floating`'s `Default`: `shelf_archive_result: ArchiveResult::Replace,`.

Frontend:
- `src/lib/settings.ts`, after `shelf_opacity: number;`: `shelf_archive_result: 'replace' | 'add';`
- `src/lib/settings.fixture.ts`, after `shelf_opacity: 95,`: `shelf_archive_result: 'replace',`
- `src/lib/schema.ts`, right after the `shelf_opacity` row:

```ts
  {
    id: 'shelf_archive_result',
    pane: 'floating',
    label: 'floating.shelf_archive_result',
    help: 'floating.shelf_archive_result.help',
    path: 'floating.shelf_archive_result',
    control: {
      kind: 'select',
      options: [
        { value: 'replace', label: 'floating.shelf_archive_result.replace' },
        { value: 'add', label: 'floating.shelf_archive_result.add' },
      ],
    },
  },
```

- `en.json`, after `floating.shelf_opacity.help`:

```json
  "floating.shelf_archive_result": "After making an archive",
  "floating.shelf_archive_result.help": "What the shelf holds once an archive is made. The original files always stay on disk.",
  "floating.shelf_archive_result.replace": "The archive replaces the items",
  "floating.shelf_archive_result.add": "Add the archive beside them",
```

- [ ] **Step 4: Run.**
  - Run `cargo test -p ferry-core archive:: settings` with the FERRY env vars.
  - Run `npx vitest run src/lib`.
  - Expected: everything passes. If a schema test lists every setting path or counts rows, update it to include `floating.shelf_archive_result`, and ledger the change.

- [ ] **Step 5: Commit** `feat(archive): learned time estimates and the shelf archive result setting`.

---

### Task 4: The runner, the submenu and the shelf's progress

**Files:**
- Create: `src-tauri/src/shelf/archive.rs`
- Modify:
  - `src-tauri/src/shelf/mod.rs` (`pub mod archive;`)
  - `src-tauri/src/shelf/menu.rs`
  - `src/shelf/Shelf.svelte`
  - `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: `ferry_core::archive::{collect, default_name, free_path, write, Options, Encryption, Rates}`, `settings::ArchiveResult`, `shelf::{state, mutate, summon}` and `jobs::notice`.
- Produces:
  - `shelf::archive::Preset { Fast, Balanced, Smallest }` with `level()`
  - `shelf::archive::cores() -> usize`
  - `shelf::archive::busy() -> bool`
  - `shelf::archive::cancel()`
  - `shelf::archive::eta_label(secs: f64) -> String`
  - `shelf::archive::estimate(app, items: &[PathBuf], level: u32, threads: usize) -> f64`
  - `shelf::archive::quick(app, preset)`
  - `shelf::archive::start(app, items: Vec<PathBuf>, out: PathBuf, opts: Options) -> Result<(), String>`
  - Menu ids `shelf:archive:{fast,balanced,smallest,advanced,stop}`
  - `menu::entries(count, size, busy)` with the new `Entry::Archive`, `Entry::StopArchive`
  - Event `shelf:archive` with payload `{ progress: number | null }`

- [ ] **Step 1: Failing tests.**

In `menu.rs` tests:
- Change `entries(8, 214_000_000)` to `entries(8, 214_000_000, false)`.
- Expect `Entry::Archive` right after `Entry::OptimiseAll`.
- In the empty-shelf test, use `entries(0, 0, false)`.
- Add:

```rust
    #[test]
    fn while_archiving_the_archive_entry_becomes_stop() {
        let e = entries(2, 10, true);
        assert!(e.contains(&Entry::StopArchive));
        assert!(!e.contains(&Entry::Archive));
    }
```

In `archive.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn eta_labels_read_like_the_spec() {
        assert_eq!(eta_label(0.4), "< 1 s");
        assert_eq!(eta_label(3.2), "~3 s");
        assert_eq!(eta_label(89.0), "~89 s");
        assert_eq!(eta_label(150.0), "~3 min");
    }

    #[test]
    fn presets_map_to_levels() {
        assert_eq!([Preset::Fast, Preset::Balanced, Preset::Smallest].map(Preset::level), [1, 5, 9]);
    }
}
```

- [ ] **Step 2: Run them to see them fail.** Run `cargo test -p ferry shelf::`. Expected: compile errors.

- [ ] **Step 3: Implement `shelf/archive.rs`:**

```rust
//! "Add to archive" (shelf archive spec): one archive at a time, progress to the
//! shelf page, the result swapped into the shelf per `floating.shelf_archive_result`.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, Instant};

use ferry_core::archive::{self, Encryption, Options, Rates};
use ferry_core::cancel::CancelToken;
use ferry_core::i18n::{t, tf};
use ferry_core::settings::ArchiveResult;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager};

use super::{SHELF, mutate, state};
use crate::jobs;
use crate::state::AppState;

static RUNNING: Mutex<Option<CancelToken>> = Mutex::new(None);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Fast,
    Balanced,
    Smallest,
}

impl Preset {
    pub fn level(self) -> u32 {
        match self {
            Preset::Fast => 1,
            Preset::Balanced => 5,
            Preset::Smallest => 9,
        }
    }
}

#[derive(Clone, Serialize)]
struct Progress {
    progress: Option<f32>,
}

pub fn cores() -> usize {
    std::thread::available_parallelism().map_or(2, |n| n.get())
}

pub fn busy() -> bool {
    RUNNING.lock().unwrap().is_some()
}

pub fn cancel() {
    if let Some(token) = RUNNING.lock().unwrap().as_ref() {
        token.cancel();
    }
}

pub fn eta_label(secs: f64) -> String {
    if secs < 1.0 {
        t("shelf.archive.eta_now")
    } else if secs < 90.0 {
        tf("shelf.archive.eta_secs", &[("n", &(secs.round() as u64).to_string())])
    } else {
        tf("shelf.archive.eta_mins", &[("n", &((secs / 60.0).round() as u64).to_string())])
    }
}

fn rates_path(app: &AppHandle) -> Option<PathBuf> {
    app.path().app_data_dir().ok().map(|d| d.join("archive_rates.json"))
}

/// Seconds to archive `items` at `level` on `threads` (spec §5).
pub fn estimate(app: &AppHandle, items: &[PathBuf], level: u32, threads: usize) -> f64 {
    let entries = archive::collect(items).unwrap_or_default();
    let files = entries.iter().filter(|e| !e.is_dir()).count();
    let bytes = entries.iter().map(|e| e.size).sum();
    let rates = rates_path(app).map(|p| Rates::load(&p)).unwrap_or_default();
    rates.estimate(bytes, level, threads.min(files).max(1))
}

fn items(app: &AppHandle) -> Vec<PathBuf> {
    state(app).map(|s| s.list.lock().unwrap().paths()).unwrap_or_default()
}

/// The quick menu items: Finder-style name in the first item's folder (spec §2).
pub fn quick(app: &AppHandle, preset: Preset) {
    let items = items(app);
    let Some(folder) = items.first().and_then(|p| p.parent()).map(Path::to_path_buf) else {
        return;
    };
    let out = archive::free_path(&folder, &archive::default_name(&items));
    let opts = Options {
        level: preset.level(),
        threads: cores(),
        password: None,
        encryption: Encryption::Aes256,
        split: None,
    };
    if let Err(e) = start(app, items, out, opts) {
        jobs::notice(app, e);
    }
}

pub fn start(app: &AppHandle, items: Vec<PathBuf>, out: PathBuf, opts: Options) -> Result<(), String> {
    let token = {
        let mut running = RUNNING.lock().unwrap();
        if running.is_some() {
            return Err(t("shelf.archive.busy"));
        }
        let token = CancelToken::new();
        *running = Some(token.clone());
        token
    };
    let app = app.clone();
    std::thread::spawn(move || {
        let began = Instant::now();
        let result = run(&app, &items, &out, &opts, &token);
        *RUNNING.lock().unwrap() = None;
        let _ = app.emit_to(SHELF, "shelf:archive", Progress { progress: None });
        match result {
            Ok((made, bytes, workers)) => {
                if let Some(path) = rates_path(&app) {
                    let mut rates = Rates::load(&path);
                    rates.record(bytes, opts.level, workers, began.elapsed().as_secs_f64());
                    let _ = rates.save(&path);
                }
                finish(&app, &items, made);
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => jobs::notice(&app, tf("shelf.archive.failed", &[("reason", &e.to_string())])),
        }
    });
    Ok(())
}

fn run(
    app: &AppHandle,
    items: &[PathBuf],
    out: &Path,
    opts: &Options,
    token: &CancelToken,
) -> std::io::Result<(Vec<PathBuf>, u64, usize)> {
    let entries = archive::collect(items)?;
    let total: u64 = entries.iter().map(|e| e.size).sum::<u64>().max(1);
    let files = entries.iter().filter(|e| !e.is_dir()).count();
    let last = Mutex::new(Instant::now() - Duration::from_secs(1));
    let progress = |done: u64| {
        let mut last = last.lock().unwrap();
        if last.elapsed() >= Duration::from_millis(200) {
            *last = Instant::now();
            let p = (done as f64 / total as f64).min(1.0) as f32;
            let _ = app.emit_to(SHELF, "shelf:archive", Progress { progress: Some(p) });
        }
    };
    let made = archive::write(&entries, out, opts, token, &progress)?;
    Ok((made, total, opts.threads.min(files).max(1)))
}

/// Spec §6: replace takes the items off the shelf (never off the disk), add keeps them.
fn finish(app: &AppHandle, items: &[PathBuf], made: Vec<PathBuf>) {
    let mode = app.state::<AppState>().settings.read().unwrap().floating.shelf_archive_result;
    if mode == ArchiveResult::Replace {
        if let Some(s) = state(app) {
            let mut cards = s.cards.lock().unwrap();
            for item in items {
                cards.remove(item);
            }
        }
    }
    mutate(app, |list| {
        if mode == ArchiveResult::Replace {
            list.remove(items);
        }
        list.add(&made, super::now());
    });
    super::summon(app);
}
```

`super::now` is private in `shelf/mod.rs`. Make it `pub(crate) fn now()`. Check that `summon(app: &AppHandle)` exists with that signature, and adapt if not.

The tests compare `eta_label` with English strings. `ferry_core::i18n::t` reads `en.json`, so add the keys in Step 5 before running them.

- [ ] **Step 4: The submenu (`menu.rs`).**
- Add `Archive` and `StopArchive` to `Entry`.
- `entries(count, size, busy: bool)`: right after `Entry::OptimiseAll`, push `if busy { Entry::StopArchive } else { Entry::Archive }`.
- `id`: `Archive => "shelf:archive"`, `StopArchive => "shelf:archive:stop"`.
- `label`: `Archive => t("shelf.menu.archive")`, `StopArchive => t("shelf.menu.archive_stop")`.
- `popup` calls `entries(view.items.len(), view.size, super::archive::busy())` and builds `Entry::Archive` as a `Submenu` (same imports as `tray.rs:181`):

```rust
            Entry::Archive => Box::new(archive_submenu(app)?),
```

```rust
fn archive_submenu(app: &AppHandle) -> Result<Submenu<Wry>, String> {
    use super::archive::{Preset, cores, estimate, eta_label};
    let items = state(app).map(|s| s.list.lock().unwrap().paths()).unwrap_or_default();
    let quick = |preset: Preset, id: &str, key: &str| {
        let eta = eta_label(estimate(app, &items, preset.level(), cores()));
        MenuItem::with_id(app, id, tf(key, &[("eta", &eta)]), true, None::<&str>)
    };
    let fast = quick(Preset::Fast, "shelf:archive:fast", "shelf.archive.fast").map_err(|e| e.to_string())?;
    let balanced = quick(Preset::Balanced, "shelf:archive:balanced", "shelf.archive.balanced").map_err(|e| e.to_string())?;
    let smallest = quick(Preset::Smallest, "shelf:archive:smallest", "shelf.archive.smallest").map_err(|e| e.to_string())?;
    let sep = PredefinedMenuItem::separator(app).map_err(|e| e.to_string())?;
    let advanced = MenuItem::with_id(app, "shelf:archive:advanced", t("shelf.archive.advanced"), true, None::<&str>)
        .map_err(|e| e.to_string())?;
    Submenu::with_id_and_items(app, "shelf:archive", t("shelf.menu.archive"), true, &[&fast, &balanced, &smallest, &sep, &advanced])
        .map_err(|e| e.to_string())
}
```

- In `run`, add these arms:

```rust
        "shelf:archive:fast" => super::archive::quick(app, super::archive::Preset::Fast),
        "shelf:archive:balanced" => super::archive::quick(app, super::archive::Preset::Balanced),
        "shelf:archive:smallest" => super::archive::quick(app, super::archive::Preset::Smallest),
        "shelf:archive:stop" => super::archive::cancel(),
        "shelf:archive:advanced" => super::archive_dialog::open(app),
```

The `advanced` arm needs Task 5. Until then, use `"shelf:archive:advanced" => {}` and fill it in Task 5.

- [ ] **Step 5: Strings** in `en.json`, after `shelf.menu.optimise_all`:

```json
  "shelf.menu.archive": "Add to archive",
  "shelf.menu.archive_stop": "Stop archiving",
  "shelf.archive.fast": "Fast · {eta}",
  "shelf.archive.balanced": "Balanced · {eta}",
  "shelf.archive.smallest": "Smallest · {eta}",
  "shelf.archive.advanced": "Advanced…",
  "shelf.archive.progress": "Archiving {percent}%",
  "shelf.archive.failed": "Couldn't make the archive: {reason}",
  "shelf.archive.busy": "An archive is already being made",
  "shelf.archive.eta_now": "< 1 s",
  "shelf.archive.eta_secs": "~{n} s",
  "shelf.archive.eta_mins": "~{n} min",
```

- [ ] **Step 6: The shelf page shows progress.** In `Shelf.svelte`:
- Add `let archiving = $state<number | null>(null);`.
- Add to the listeners: `listen<{ progress: number | null }>('shelf:archive', (e) => (archiving = e.payload.progress)),`.
- In both pill buttons (lines near 238 and 247), replace `{countLabel(view.items.length)}` with:

```svelte
{archiving === null ? countLabel(view.items.length) : t('shelf.archive.progress', { percent: String(Math.round(archiving * 100)) })}
```

Use the page's existing i18n helper: check how `countLabel` builds its text in `src/shelf/strip.ts` and call the same function the same way.

- [ ] **Step 7: Run.** Run `cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry shelf::`, then `npx svelte-check` and `npx vitest run`. Expected: clean, all passing.

- [ ] **Step 8: Commit** `feat(shelf): add to archive submenu with estimates, progress and result swap`.

---

### Task 5: The Advanced dialog

**Files:**
- Create:
  - `src-tauri/src/shelf/archive_dialog.rs`
  - `src/archive.html`, `src/archive/main.ts`, `src/archive/Archive.svelte`
  - `src/archive/form.ts` and `src/archive/form.test.ts`
- Modify:
  - `src-tauri/src/shelf/mod.rs` (`pub mod archive_dialog;`) and `src-tauri/src/shelf/menu.rs` (the advanced arm)
  - `src-tauri/src/lib.rs` (register the commands)
  - `src-tauri/src/windows.rs` (`follows_screenshot_setting` adds `ARCHIVE`)
  - `src-tauri/capabilities/default.json` (add `"archive"` to `windows`)
  - `vite.config.ts` (`archive: page('archive.html'),`)
  - `src/lib/i18n/en.json`

**Interfaces:**
- Consumes: everything from Task 4, and `archive::{default_name, taken, parse_size}`.
- Produces:
  - `archive_dialog::{ARCHIVE, open(app)}`
  - commands `archive_view() -> DialogView`, `archive_estimate(level: u32, threads: usize) -> String`, `archive_pick(folder: String, name: String) -> Option<(String, String)>`, `archive_start(req: StartRequest) -> Result<(), String>`, `archive_close()`
  - pure TS `checkForm(f: Form): StringKey | null` and `LEVELS`, `VOLUMES`

- [ ] **Step 1: Failing TS test** `src/archive/form.test.ts`:

```ts
import { describe, expect, it } from 'vitest';
import { checkForm, LEVELS } from './form';

const ok = { name: 'Archive.zip', password: '', again: '', split: '' };

describe('checkForm', () => {
  it('accepts a plain form', () => expect(checkForm(ok)).toBeNull());
  it('needs a name', () => expect(checkForm({ ...ok, name: '  ' })).toBe('archive.error.name'));
  it('rejects a slash in the name', () => expect(checkForm({ ...ok, name: 'a/b.zip' })).toBe('archive.error.name'));
  it('needs matching passwords', () => expect(checkForm({ ...ok, password: 'a', again: 'b' })).toBe('archive.mismatch'));
  it('levels follow 7-Zip', () => expect(LEVELS).toEqual([0, 1, 3, 5, 7, 9]));
});
```

Run `npx vitest run src/archive`. Expected: FAIL, because the module is missing.

- [ ] **Step 2: `src/archive/form.ts`:**

```ts
import type { StringKey } from '../lib/i18n';

export const LEVELS = [0, 1, 3, 5, 7, 9] as const;
export const VOLUMES = ['10M', '100M', '650M', '700M', '4092M', '4480M'] as const;

export type Form = { name: string; password: string; again: string; split: string };

/** The first problem that keeps OK disabled; the volume size is checked by Rust. */
export function checkForm(f: Form): StringKey | null {
  const name = f.name.trim();
  if (!name || name.includes('/')) return 'archive.error.name';
  if (f.password !== f.again) return 'archive.mismatch';
  return null;
}
```

Use the real `StringKey` import path from how `src/lib/schema.ts` imports it.

Run `npx vitest run src/archive`. Expected: PASS once the Step 4 strings are in `en.json`, if `StringKey` is checked against `en.json`.

- [ ] **Step 3: `src-tauri/src/shelf/archive_dialog.rs`.** Build the window like `compare.rs::build`:
- `WebviewWindowBuilder::new(app, ARCHIVE, WebviewUrl::App("archive.html".into()))`
- `.title(t("archive.title"))`, `.inner_size(560.0, 400.0)`, `.resizable(false)`, `.content_protected(!show_in_screenshots)`, `.visible(false)`
- on `CloseRequested`, `prevent_close()` and hide.

`open(app)` builds the window if missing, then `center()`, `show()` and `set_focus()`, and emits `archive:open` to it. Menu events run off the main thread, so wrap the window calls in `app.run_on_main_thread`.

```rust
pub const ARCHIVE: &str = "archive";

/// Remembered for the session: the last level and threads (never the password).
static LAST: Mutex<Option<(u32, usize)>> = Mutex::new(None);

#[derive(Serialize)]
pub struct DialogView {
    pub folder: String,
    pub name: String,
    pub count: usize,
    pub cores: usize,
    pub level: u32,
    pub threads: usize,
}

#[derive(Deserialize)]
pub struct StartRequest {
    pub folder: String,
    pub name: String,
    pub level: u32,
    pub threads: usize,
    pub password: Option<String>,
    /// "aes256" or "zipcrypto".
    pub encryption: String,
    pub split: Option<String>,
}

#[tauri::command]
pub fn archive_view(app: AppHandle) -> DialogView {
    let items = items(&app);
    let (level, threads) = LAST.lock().unwrap().unwrap_or((5, archive::cores()));
    DialogView {
        folder: items.first().and_then(|p| p.parent()).map_or_else(String::new, |p| p.to_string_lossy().into_owned()),
        name: ferry_core::archive::default_name(&items),
        count: items.len(),
        cores: archive::cores(),
        level,
        threads,
    }
}

#[tauri::command]
pub async fn archive_estimate(app: AppHandle, level: u32, threads: usize) -> String {
    tauri::async_runtime::spawn_blocking(move || archive::eta_label(archive::estimate(&app, &items(&app), level, threads)))
        .await
        .unwrap_or_default()
}

#[tauri::command]
pub async fn archive_pick(app: AppHandle, folder: String, name: String) -> Option<(String, String)> {
    use tauri_plugin_dialog::DialogExt;
    let path = app.dialog().file().set_directory(folder).set_file_name(name).add_filter("Zip", &["zip"]).blocking_save_file()?;
    let path = path.into_path().ok()?;
    Some((
        path.parent()?.to_string_lossy().into_owned(),
        path.file_name()?.to_string_lossy().into_owned(),
    ))
}

#[tauri::command]
pub fn archive_start(app: AppHandle, req: StartRequest) -> Result<(), String> {
    let name = req.name.trim();
    if name.is_empty() || name.contains('/') {
        return Err(t("archive.error.name"));
    }
    let name = if name.to_lowercase().ends_with(".zip") { name.to_string() } else { format!("{name}.zip") };
    let split = match req.split.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        Some(s) => Some(ferry_core::archive::parse_size(s).ok_or_else(|| t("archive.error.split"))?),
        None => None,
    };
    let out = PathBuf::from(&req.folder).join(&name);
    if ferry_core::archive::taken(&out) {
        return Err(tf("archive.error.exists", &[("name", &name)]));
    }
    *LAST.lock().unwrap() = Some((req.level, req.threads));
    let opts = Options {
        level: req.level.min(9),
        threads: req.threads.clamp(1, archive::cores()),
        password: req.password.filter(|p| !p.is_empty()),
        encryption: if req.encryption == "zipcrypto" { Encryption::ZipCrypto } else { Encryption::Aes256 },
        split,
    };
    archive::start(&app, items(&app), out, opts)?;
    archive_close(app);
    Ok(())
}

#[tauri::command]
pub fn archive_close(app: AppHandle) {
    if let Some(w) = app.get_webview_window(ARCHIVE) {
        let _ = w.hide();
    }
}
```

- `items(app)` is the same helper as in `archive.rs`. Make that one `pub(super)` and reuse it.
- Check `tauri_plugin_dialog`'s save builder method names against `src-tauri/src/commands.rs:240` and `pipe_commands.rs:147` (`set_directory`, `set_file_name`, `blocking_save_file`, and the `FilePath::into_path` conversion), and adapt.
- Register all five commands in `lib.rs` next to the shelf commands.
- Fill the menu arm: `"shelf:archive:advanced" => super::archive_dialog::open(app),`.
- Add `crate::shelf::archive_dialog::ARCHIVE` to `follows_screenshot_setting`.

- [ ] **Step 4: The page.**

`src/archive.html`: copy `src/shelf.html`, with title `Add to Archive` and script `/archive/main.ts`.

`src/archive/main.ts`: copy `src/shelf/main.ts`, mounting `Archive.svelte`.

`Archive.svelte`, a two-column 7-Zip layout using `theme.css` variables (`--bg-content`, `--bg-group`, `--fg`, `--accent`, `--error`, `--font-ui`). No `var`.

**State** (runes):
- `view` from `invoke('archive_view')`, loaded on mount and on `listen('archive:open')`. Each load also resets `password`, `again` and `error` to `''`.
- `folder`, `name`, `level`, `threads` from the view.
- `split = ''`, `show = false`, `encryption = 'aes256'`.
- `eta` comes from an `$effect` on `[level, threads]` that calls `invoke('archive_estimate', { level, threads })`.
- `problem = $derived(checkForm({ name, password, again, split }))`.

**Left column:**
- Label "Archive". Under it, `folder` in small muted text, then a row with a `name` text input and a "…" button. The button calls `archive_pick` with `{ folder, name }`; when it returns `[f, n]`, set `folder = f; name = n`.
- "Compression level": a `<select>` over `LEVELS` with labels `t('archive.level.' + l)`.
- "CPU threads": a `<select>` over 1..`view.cores`, followed by `/ {view.cores}`.
- "Split to volumes": `<input list="volumes">` with a `<datalist id="volumes">` of `VOLUMES`, and the help text `archive.split.help` under it.
- `t('archive.estimate', { eta })`.

**Right column, a group headed "Encryption":**
- Password and Reenter password inputs, with `type={show ? 'text' : 'password'}`.
- A Show password checkbox.
- An "Encryption method" `<select>` with `aes256` and `zipcrypto`.
- Help text `archive.method.aes.help` or `archive.method.zipcrypto.help` for the chosen method.

**Footer:**
- An error line in `--error` showing `problem ? t(problem) : error`.
- Cancel calls `archive_close`.
- OK (primary, `disabled={!!problem}`) calls:

```ts
invoke('archive_start', { req: { folder, name, level, threads, password: password || null, encryption, split: split || null } })
```

  and catches the rejection into `error`.
- `svelte:window onkeydown`: Escape calls `archive_close`, and Enter (with no problem) runs OK.

Use the page's i18n `t` helper the way other pages do (check `src/settings/Settings.svelte`).

Add these keys to `en.json`:

```json
  "archive.title": "Add to Archive",
  "archive.archive": "Archive",
  "archive.choose": "…",
  "archive.level": "Compression level",
  "archive.level.0": "0 – Store",
  "archive.level.1": "1 – Fastest",
  "archive.level.3": "3 – Fast",
  "archive.level.5": "5 – Normal",
  "archive.level.7": "7 – Maximum",
  "archive.level.9": "9 – Ultra",
  "archive.threads": "CPU threads",
  "archive.split": "Split to volumes",
  "archive.split.help": "Like 100M or 4092M. Volumes are named .zip.001, .zip.002… Finder can't open them; 7-Zip, Keka and The Unarchiver can.",
  "archive.estimate": "Estimated time: {eta}",
  "archive.encryption": "Encryption",
  "archive.password": "Enter password",
  "archive.password_again": "Reenter password",
  "archive.show_password": "Show password",
  "archive.method": "Encryption method",
  "archive.method.aes256": "AES-256",
  "archive.method.zipcrypto": "ZipCrypto",
  "archive.method.aes.help": "Finder can't open AES-256 zips. Use Keka or The Unarchiver.",
  "archive.method.zipcrypto.help": "Weak protection, but Finder can open it.",
  "archive.mismatch": "The passwords don't match",
  "archive.error.name": "Enter a name",
  "archive.error.split": "Volume size: a number with k, M or G, at least 64k",
  "archive.error.exists": "{name} already exists there",
  "archive.ok": "OK",
  "archive.cancel": "Cancel",
```

- [ ] **Step 5: Run.** Run `npx vitest run src/archive`, then `npx svelte-check`, then `cargo clippy -p ferry --all-targets -- -D warnings && cargo test -p ferry`. Expected: all clean and passing.

- [ ] **Step 6: Final gate** (Global Constraints). Run the full command with the FERRY env vars. Expected: fmt, clippy, tests, svelte-check and vitest all clean. If a pre-existing test fails, name it in the report.

- [ ] **Step 7: Commit** `feat(shelf): Advanced archive dialog modelled on 7-Zip's Add to Archive`.

---

## Manual checks for the user (not the implementer)

1. Shelve a folder and two files, then use Add to archive ▸ Fast. The pill shows "Archiving N%". The shelf ends with `Archive.zip`, and the originals stay on disk.
2. Set the setting to Add, then use Balanced. `Archive 2.zip` is added beside the items.
3. In Advanced, set a password with ZipCrypto. Double-click the result in Finder. **Does Archive Utility open it?** If not, drop ZipCrypto (spec §3).
4. In Advanced, split at 10M on a 30 MB shelf: you get `.zip.001`, `.002` and `.003`.
5. Start Smallest on a large folder, then choose Stop archiving. Nothing is left in the folder.
