//! The real file-system probe behind the settle state machine. Everything runs
//! inside `fsutil::with_timeout`, so a sleeping or network drive cannot stall
//! the watcher thread (NFR-P4).

use std::fs;
use std::io::{self, Read};
use std::path::Path;
use std::time::Duration;

use super::settle::Probe;
use crate::fsutil::with_timeout;
use crate::selfwrite::Stamp;

pub const PROBE_TIMEOUT: Duration = Duration::from_secs(5);

/// macOS `SF_DATALESS`: the content lives in iCloud and is not on this disk.
pub fn is_dataless(st_flags: u32) -> bool {
    st_flags & 0x4000_0000 != 0
}

/// Windows `OFFLINE`, `RECALL_ON_OPEN` and `RECALL_ON_DATA_ACCESS`: reading the
/// file would download it.
pub fn is_placeholder_attributes(attrs: u32) -> bool {
    attrs & (0x1000 | 0x4_0000 | 0x40_0000) != 0
}

/// What `path` is right now. A timeout counts as `Busy`.
pub fn probe(path: &Path) -> Probe {
    let path = path.to_owned();
    with_timeout(PROBE_TIMEOUT, move || Ok(probe_now(&path))).unwrap_or(Probe::Busy)
}

fn probe_now(path: &Path) -> Probe {
    let meta = match fs::symlink_metadata(path) {
        Ok(meta) => meta,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Probe::Gone,
        Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Probe::Denied,
        Err(_) => return Probe::Busy,
    };
    if meta.file_type().is_symlink() || !meta.is_file() {
        return Probe::NotAFile;
    }
    #[cfg(target_os = "macos")]
    {
        use std::os::macos::fs::MetadataExt;
        if is_dataless(meta.st_flags()) {
            return Probe::Placeholder;
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::{MetadataExt, OpenOptionsExt};
        if is_placeholder_attributes(meta.file_attributes()) {
            return Probe::Placeholder;
        }
        // A writer that still holds the file open without sharing it.
        match fs::OpenOptions::new().read(true).share_mode(0).open(path) {
            Ok(_) => {}
            Err(e) if matches!(e.raw_os_error(), Some(32 | 33)) => return Probe::Busy,
            Err(e) if e.kind() == io::ErrorKind::PermissionDenied => return Probe::Denied,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return Probe::Gone,
            Err(_) => return Probe::Busy,
        }
    }
    match meta.modified() {
        Ok(modified) => Probe::File(Stamp {
            len: meta.len(),
            modified,
        }),
        Err(_) => Probe::Busy,
    }
}

/// At most the first `n` bytes of `path`.
pub fn head(path: &Path, n: usize) -> Option<Vec<u8>> {
    let path = path.to_owned();
    with_timeout(PROBE_TIMEOUT, move || {
        let mut bytes = Vec::with_capacity(n);
        fs::File::open(path)?
            .take(n as u64)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    })
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_plain_file_probes_with_its_size() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, b"12345").unwrap();
        assert!(matches!(probe(&p), Probe::File(Stamp { len: 5, .. })));
    }

    #[test]
    fn a_missing_file_is_gone_and_a_folder_is_not_a_file() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(probe(&dir.path().join("nope")), Probe::Gone);
        assert_eq!(probe(dir.path()), Probe::NotAFile);
    }

    #[cfg(unix)]
    #[test]
    fn a_symlink_is_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let (real, link) = (dir.path().join("real"), dir.path().join("link"));
        fs::write(&real, b"x").unwrap();
        std::os::unix::fs::symlink(&real, &link).unwrap();
        assert_eq!(probe(&link), Probe::NotAFile);
    }

    #[test]
    fn placeholder_bits_are_recognised() {
        assert!(is_dataless(0x4000_0000));
        assert!(!is_dataless(0x20));
        for a in [0x1000, 0x4_0000, 0x40_0000] {
            assert!(is_placeholder_attributes(a | 0x20));
        }
        assert!(!is_placeholder_attributes(0x20 | 0x80));
    }

    #[test]
    fn head_reads_at_most_n_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, b"0123456789").unwrap();
        assert_eq!(head(&p, 4).unwrap(), b"0123");
        assert_eq!(head(&p, 64).unwrap(), b"0123456789");
        assert!(head(&dir.path().join("nope"), 4).is_none());
    }

    #[cfg(windows)]
    #[test]
    fn a_file_held_open_without_sharing_is_busy() {
        use std::os::windows::fs::OpenOptionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.bin");
        fs::write(&p, b"x").unwrap();
        let held = fs::OpenOptions::new()
            .write(true)
            .share_mode(0)
            .open(&p)
            .unwrap();
        assert_eq!(probe(&p), Probe::Busy);
        drop(held);
        assert!(matches!(probe(&p), Probe::File(_)));
    }
}
