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
    let name = out
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
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
        let lightest = groups
            .iter_mut()
            .min_by_key(|(load, _)| *load)
            .expect("threads >= 1");
        lightest.0 += file.size;
        lightest.1.push(file);
    }
    groups
        .into_iter()
        .map(|(_, g)| g)
        .filter(|g| !g.is_empty())
        .collect()
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
        zip.start_file(entry.name.as_str(), options)
            .map_err(io::Error::other)?;
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
    match (
        opts.password.as_deref().filter(|p| !p.is_empty()),
        opts.encryption,
    ) {
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
            if let Some(empty) = made.pop_if(|p| fs::metadata(p).is_ok_and(|m| m.len() == 0)) {
                let _ = fs::remove_file(empty);
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
    n.checked_shl(shift)
        .filter(|&b| b >= 64 << 10 && b >> shift == n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::archive::collect;
    use std::fs;
    use std::io::Read;

    fn opts(level: u32, threads: usize) -> Options {
        Options {
            level,
            threads,
            password: None,
            encryption: Encryption::Aes256,
            split: None,
        }
    }

    fn fixture(dir: &Path) -> Vec<PathBuf> {
        let folder = dir.join("docs");
        fs::create_dir_all(folder.join("deep")).unwrap();
        fs::write(folder.join("a.txt"), "alpha ".repeat(5000)).unwrap();
        fs::write(folder.join("deep/b.txt"), "beta ".repeat(3000)).unwrap();
        fs::write(
            dir.join("c.bin"),
            (0..200_000u32).map(|i| (i % 251) as u8).collect::<Vec<_>>(),
        )
        .unwrap();
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
            (
                "docs/a.txt".to_string(),
                fs::read(dir.join("docs/a.txt")).unwrap(),
            ),
            ("docs/deep/".to_string(), vec![]),
            (
                "docs/deep/b.txt".to_string(),
                fs::read(dir.join("docs/deep/b.txt")).unwrap(),
            ),
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
            let made = write(
                &entries,
                &out,
                &opts(level, threads),
                &CancelToken::new(),
                &|_| {},
            )
            .unwrap();
            assert_eq!(made, vec![out.clone()]);
            assert_eq!(
                read_all(&out, None),
                expected(dir.path()),
                "level {level} threads {threads}"
            );
            assert!(!dir.path().join("out/.x.zip.ferry").exists());
        }
    }

    #[test]
    fn aes_round_trip_across_threads() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options {
            password: Some("s3cret".into()),
            ..opts(5, 4)
        };
        write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap();
        assert_eq!(read_all(&out, Some("s3cret")), expected(dir.path()));
        let mut archive = zip::ZipArchive::new(fs::File::open(&out).unwrap()).unwrap();
        let file = archive.by_name("c.bin").map(|_| ());
        assert!(
            file.is_err(),
            "an encrypted entry must not open without the password"
        );
    }

    #[test]
    fn zipcrypto_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options {
            password: Some("pw".into()),
            encryption: Encryption::ZipCrypto,
            ..opts(5, 2)
        };
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
        let o = Options {
            split: Some(64 * 1024),
            ..opts(0, 1)
        };
        let made = write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap();
        assert!(made.len() >= 3, "{made:?}");
        assert_eq!(made[0], dir.path().join("x.zip.001"));
        assert!(!out.exists());
        let joined: Vec<u8> = made.iter().flat_map(|p| fs::read(p).unwrap()).collect();
        fs::write(dir.path().join("joined.zip"), &joined).unwrap();
        assert_eq!(
            read_all(&dir.path().join("joined.zip"), None),
            expected(dir.path())
        );
    }

    #[test]
    fn a_split_bigger_than_the_archive_keeps_one_file() {
        let dir = tempfile::tempdir().unwrap();
        let entries = collect(&fixture(dir.path())).unwrap();
        let out = dir.path().join("x.zip");
        let o = Options {
            split: Some(1 << 30),
            ..opts(1, 1)
        };
        assert_eq!(
            write(&entries, &out, &o, &CancelToken::new(), &|_| {}).unwrap(),
            vec![out]
        );
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
