//! Windows clipboard (spec §4.4): registered image formats first, then a
//! bitmap kept as a lossless PNG; copied files from `CF_HDROP`. Changes come
//! from a clipboard listener on its own thread. Only CI has run this so far;
//! no real Windows desktop has (spec §1).

use std::path::PathBuf;
use std::sync::mpsc::Sender;
use std::thread;

use clipboard_win::{Clipboard, Monitor, formats, options, raw};
use ferry_core::bitmap;
use ferry_core::sniff::{Format, sniff};
use ferry_core::trigger::Snapshot;
use windows_sys::Win32::Foundation::{CloseHandle, HWND};
use windows_sys::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows_sys::Win32::UI::WindowsAndMessaging::GetWindowThreadProcessId;

/// Another app may hold the clipboard open for a moment.
const ATTEMPTS: usize = 10;

/// Registered formats in read order; Windows apps name JPEG "JFIF".
const IMAGE_FORMATS: [(&str, Format); 5] = [
    ("PNG", Format::Png),
    ("JFIF", Format::Jpeg),
    ("image/jpeg", Format::Jpeg),
    ("GIF", Format::Gif),
    ("image/webp", Format::WebP),
];

/// Formats password managers and clipboard tools set on secret content.
const EXCLUDE: &str = "ExcludeClipboardContentFromMonitorProcessing";
const HISTORY: &str = "CanIncludeInClipboardHistory";
const VIEWER_IGNORE: &str = "Clipboard Viewer Ignore";

fn registered(name: &str) -> Option<u32> {
    raw::register_format(name).map(|id| id.get())
}

fn format_name(format: Format) -> &'static str {
    match format {
        Format::Png => "PNG",
        Format::Jpeg => "JFIF",
        Format::Gif => "GIF",
        Format::WebP => "image/webp",
        // sE pdf-formats
        Format::Tiff => "image/tiff",
        Format::Heic => "image/heic",
        Format::Avif => "image/avif",
        Format::Pdf => "application/pdf",
        // stream A: video. A video never goes on the clipboard as data.
        Format::Mp4 | Format::Hevc | Format::Av1 | Format::WebM | Format::Mov | Format::Mkv => "",
        // stream B: audio never goes on the clipboard as data either.
        Format::Mp3
        | Format::M4a
        | Format::Flac
        | Format::Wav
        | Format::Aiff
        | Format::Ogg
        | Format::Opus => "",
    }
}

pub fn read() -> Result<Snapshot, String> {
    let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
    let seq = seq();
    let source_app = owner_exe();
    if is_concealed() {
        return Ok(Snapshot {
            seq,
            concealed: true,
            source_app,
            ..Snapshot::default()
        });
    }
    let mut snap = Snapshot {
        seq,
        source_app,
        ..Snapshot::default()
    };
    if raw::is_format_avail(formats::CF_HDROP) {
        let _ = raw::get_file_list_path(&mut snap.files);
    }
    for (name, format) in IMAGE_FORMATS {
        let Some(id) = registered(name).filter(|&id| raw::is_format_avail(id)) else {
            continue;
        };
        let mut bytes = Vec::new();
        // Trust the content, not the format name (R§2.4).
        if raw::get_vec(id, &mut bytes).is_ok() && sniff(&bytes) == Some(format) {
            snap.image = Some((format, bytes));
            return Ok(snap);
        }
    }
    // Copy the bitmaps out and let go of the clipboard before the slow PNG
    // encode, so a paste in another app is not blocked meanwhile.
    let mut dibs = Vec::new();
    for id in [formats::CF_DIBV5, formats::CF_DIB] {
        let mut dib = Vec::new();
        if raw::is_format_avail(id) && raw::get_vec(id, &mut dib).is_ok() {
            dibs.push(dib);
        }
    }
    drop(_open);
    if let Some(png) = dibs.iter().find_map(|dib| bitmap::dib_to_png(dib).ok()) {
        snap.image = Some((Format::Png, png));
        snap.from_bitmap = true;
    }
    Ok(snap)
}

fn is_concealed() -> bool {
    let present = |name| registered(name).is_some_and(raw::is_format_avail);
    if present(EXCLUDE) || present(VIEWER_IGNORE) {
        return true;
    }
    // A DWORD of 0 asks clipboard history (and so Ferry) to leave the content alone.
    let mut value = Vec::new();
    registered(HISTORY).is_some_and(|id| {
        raw::is_format_avail(id)
            && raw::get_vec(id, &mut value).is_ok()
            && value.get(..4) == Some(&[0, 0, 0, 0])
    })
}

/// The executable name of the app that owns the clipboard, such as `KeePass.exe`.
fn owner_exe() -> Option<String> {
    let owner = raw::get_owner()?;
    let mut pid = 0_u32;
    // SAFETY: `owner` is a window handle from GetClipboardOwner; a stale handle
    // makes the call return 0, which is checked.
    unsafe { GetWindowThreadProcessId(owner.as_ptr() as HWND, &mut pid) };
    if pid == 0 {
        return None;
    }
    // SAFETY: plain Win32 calls; the handle is closed before returning.
    unsafe {
        let process = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if process.is_null() {
            return None;
        }
        let mut buf = [0_u16; 1024];
        let mut len = buf.len() as u32;
        let ok =
            QueryFullProcessImageNameW(process, PROCESS_NAME_WIN32, buf.as_mut_ptr(), &mut len);
        CloseHandle(process);
        if ok == 0 {
            return None;
        }
        let path = PathBuf::from(String::from_utf16_lossy(&buf[..len as usize]));
        Some(path.file_name()?.to_string_lossy().into_owned())
    }
}

/// Windows browsers render the image when the clipboard is read.
pub fn may_gain_image() -> bool {
    false
}

/// The sequence number now, to tell whether the user has copied since a snapshot.
pub fn seq() -> u64 {
    raw::seq_num().map_or(0, |n| n.get().into())
}

/// Put an image on the clipboard in its own format, plus a `CF_DIBV5` for apps
/// that paste only bitmaps (not for GIF). Returns the new sequence number.
pub fn write_image(format: Format, bytes: &[u8]) -> Result<u64, String> {
    // Decode before opening the clipboard, so it is held as briefly as possible.
    let dib = bitmap::decode_rgba(bytes, format)
        .ok()
        .map(|(w, h, rgba)| bitmap::rgba_to_dibv5(w, h, &rgba));
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::empty().map_err(|e| e.to_string())?;
        let id = registered(format_name(format)).ok_or("could not register the format")?;
        raw::set_without_clear(id, bytes).map_err(|e| e.to_string())?;
        if let Some(dib) = dib {
            raw::set_without_clear(formats::CF_DIBV5, &dib).map_err(|e| e.to_string())?;
        }
    } // closing the clipboard publishes the change
    Ok(seq())
}

/// Put files on the clipboard, as Explorer's Copy does.
pub fn write_files(paths: &[PathBuf]) -> Result<u64, String> {
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::empty().map_err(|e| e.to_string())?;
        raw::set_file_list(&names).map_err(|e| e.to_string())?;
    }
    Ok(seq())
}

/// Put files on the clipboard with their full paths as text, one per line
/// ("Copy file paths"). Returns the new sequence number.
pub fn write_paths(paths: &[PathBuf]) -> Result<u64, String> {
    let names: Vec<String> = paths
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    let text = names.join("\r\n");
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::empty().map_err(|e| e.to_string())?;
        raw::set_file_list(&names).map_err(|e| e.to_string())?; // does not empty the clipboard
        raw::set_string_with(&text, options::NoClear).map_err(|e| e.to_string())?;
    } // closing the clipboard publishes the change
    Ok(seq())
}

/// Put plain text on the clipboard ("Copy path"). `set_string` empties it first.
pub fn write_text(text: &str) -> Result<u64, String> {
    {
        let _open = Clipboard::new_attempts(ATTEMPTS).map_err(|e| e.to_string())?;
        raw::set_string(text).map_err(|e| e.to_string())?;
    }
    Ok(seq())
}

/// Send the sequence number on `tx` after every clipboard change. The listener
/// window must live on the thread that waits on it.
pub fn watch(tx: Sender<u64>) {
    thread::Builder::new()
        .name("ferry-clipboard".into())
        .spawn(move || {
            let mut monitor = match Monitor::new() {
                Ok(monitor) => monitor,
                Err(e) => {
                    tracing::warn!("clipboard listener failed: {e}");
                    return;
                }
            };
            while let Ok(true) = monitor.recv() {
                if tx.send(seq()).is_err() {
                    return;
                }
            }
        })
        .expect("spawn the clipboard thread");
}

#[cfg(test)]
mod tests {
    //! These use the real clipboard (Windows has no private one), so they run
    //! one at a time and only in CI's Windows bundle job.
    use super::*;
    use std::sync::Mutex;

    static CLIPBOARD: Mutex<()> = Mutex::new(());

    const PNG: &[u8] = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR";

    #[test]
    fn an_image_round_trips_and_the_sequence_moves() {
        let _one = CLIPBOARD.lock().unwrap();
        let before = seq();
        let written = write_image(Format::Png, PNG).unwrap();
        assert!(written > before);
        let snap = read().unwrap();
        assert_eq!(snap.seq, written);
        assert_eq!(snap.image, Some((Format::Png, PNG.to_vec())));
        assert!(!snap.from_bitmap && !snap.concealed);
    }

    #[test]
    fn files_round_trip() {
        let _one = CLIPBOARD.lock().unwrap();
        let dir = std::env::temp_dir();
        let paths = vec![dir.join("ferry a.png"), dir.join("b.jpg")];
        write_files(&paths).unwrap();
        assert_eq!(read().unwrap().files, paths);
    }

    #[test]
    fn paths_go_on_as_files_and_as_text() {
        let _one = CLIPBOARD.lock().unwrap();
        let dir = std::env::temp_dir();
        let paths = vec![dir.join("ferry a.png"), dir.join("b.jpg")];
        write_paths(&paths).unwrap();
        assert_eq!(read().unwrap().files, paths);
        let want = paths
            .iter()
            .map(|p| p.to_string_lossy().into_owned())
            .collect::<Vec<_>>()
            .join("\r\n");
        assert_eq!(clipboard_win::get_clipboard_string().unwrap(), want);
    }

    #[test]
    fn a_bitmap_only_copy_becomes_a_png_from_bitmap() {
        let _one = CLIPBOARD.lock().unwrap();
        let rgba = vec![200_u8; 4 * 4 * 4];
        let dib = bitmap::rgba_to_dibv5(4, 4, &rgba);
        {
            let _open = Clipboard::new_attempts(ATTEMPTS).unwrap();
            raw::empty().unwrap();
            raw::set_without_clear(formats::CF_DIBV5, &dib).unwrap();
        }
        let snap = read().unwrap();
        assert!(snap.from_bitmap);
        assert_eq!(snap.image.map(|(f, _)| f), Some(Format::Png));
    }

    #[test]
    fn content_marked_for_exclusion_is_concealed() {
        let _one = CLIPBOARD.lock().unwrap();
        {
            let _open = Clipboard::new_attempts(ATTEMPTS).unwrap();
            raw::empty().unwrap();
            raw::set_without_clear(registered("PNG").unwrap(), PNG).unwrap();
            raw::set_without_clear(registered(EXCLUDE).unwrap(), &[0]).unwrap();
        }
        let snap = read().unwrap();
        assert!(snap.concealed);
        assert_eq!(snap.image, None);
    }
}
