//! Windows: the apps that open a file (the "Open with" list), their icons, and
//! opening a file with one. Not compilable off Windows; CI builds and tests it.

use std::path::{Path, PathBuf};
use std::process::Command;

use ferry_core::integrations::{KnownApp, ListedApp};
use ferry_core::settings::AppRef;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, DIB_RGB_COLORS, DeleteObject, GetDC, GetDIBits, ReleaseDC,
};
use windows::Win32::System::Com::{
    COINIT_APARTMENTTHREADED, CoInitializeEx, CoTaskMemFree, CoUninitialize, IBindCtx, IDataObject,
};
use windows::Win32::UI::Shell::{
    ASSOC_FILTER, ASSOC_FILTER_NONE, ASSOC_FILTER_RECOMMENDED, BHID_DataObject, IAssocHandler,
    IShellItem, SHAssocEnumHandlers, SHCreateItemFromParsingName, SHDefExtractIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{DestroyIcon, GetIconInfo, HICON, ICONINFO};
use windows::core::{HSTRING, PWSTR};

/// COM for the calling thread. `RPC_E_CHANGED_MODE` (already initialised the
/// other way) is fine and is not undone.
struct ComGuard(bool);

impl ComGuard {
    fn new() -> Self {
        // SAFETY: plain COM initialisation, undone in `drop` when it succeeded.
        Self(unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.is_ok())
    }
}

impl Drop for ComGuard {
    fn drop(&mut self) {
        if self.0 {
            // SAFETY: balances the successful `CoInitializeEx` in `new`.
            unsafe { CoUninitialize() };
        }
    }
}

/// A COM-allocated string as an owned one.
fn take(text: PWSTR) -> String {
    // SAFETY: `text` came from a COM call that allocates with CoTaskMemAlloc.
    let owned = unsafe { text.to_string() }.unwrap_or_default();
    unsafe { CoTaskMemFree(Some(text.0 as _)) };
    owned
}

fn handlers(ext: &str, filter: ASSOC_FILTER) -> Vec<IAssocHandler> {
    // SAFETY: the string outlives the call.
    let Ok(all) = (unsafe { SHAssocEnumHandlers(&HSTRING::from(format!(".{ext}")), filter) })
    else {
        return Vec::new();
    };
    let mut found = Vec::new();
    loop {
        let mut next = [None];
        let mut fetched = 0u32;
        // SAFETY: `next` and `fetched` are valid for the call.
        if unsafe { all.Next(&mut next, Some(&mut fetched)) }.is_err() || fetched == 0 {
            return found;
        }
        found.extend(next[0].take());
    }
}

fn extension(path: &Path) -> Option<String> {
    path.extension()
        .map(|ext| ext.to_string_lossy().into_owned())
}

/// The apps Windows recommends for `file`'s extension. Unordered.
pub fn list(file: &Path) -> Vec<ListedApp> {
    let _com = ComGuard::new();
    let Some(ext) = extension(file) else {
        return Vec::new();
    };
    handlers(&ext, ASSOC_FILTER_RECOMMENDED)
        .iter()
        .filter_map(|handler| {
            // SAFETY: both strings are freed by `take`.
            let (id, name) = unsafe { (handler.GetName().ok()?, handler.GetUIName().ok()?) };
            Some(ListedApp {
                app: AppRef {
                    id: take(id),
                    name: take(name),
                },
                default: false,
            })
        })
        .collect()
}

/// A picked program as a setting: its full path and its file name without `.exe`.
pub fn app_ref(path: &Path) -> Option<AppRef> {
    Some(AppRef {
        id: path.to_string_lossy().into_owned(),
        name: path.file_stem()?.to_string_lossy().into_owned(),
    })
}

/// The program's icon at 32 px, for a menu item. Handlers without an exe path have none.
pub fn icon(app: &AppRef) -> Option<tauri::image::Image<'static>> {
    if !Path::new(&app.id).is_file() {
        return None;
    }
    let _com = ComGuard::new();
    let mut large = HICON::default();
    let mut rgba = vec![0u8; 32 * 32 * 4];
    // SAFETY: `large` is valid for the call; the icon and bitmaps are released below.
    unsafe {
        SHDefExtractIconW(
            &HSTRING::from(app.id.as_str()),
            0,
            0,
            Some(&mut large),
            None,
            32,
        )
        .ok()
        .ok()?;
        let mut info = ICONINFO::default();
        let got = GetIconInfo(large, &mut info);
        let read = got.is_ok() && {
            let mut header = BITMAPINFO {
                bmiHeader: BITMAPINFOHEADER {
                    biSize: size_of::<BITMAPINFOHEADER>() as u32,
                    biWidth: 32,
                    biHeight: -32,
                    biPlanes: 1,
                    biBitCount: 32,
                    biCompression: BI_RGB.0,
                    ..Default::default()
                },
                ..Default::default()
            };
            let dc = GetDC(None);
            let lines = GetDIBits(
                dc,
                info.hbmColor,
                0,
                32,
                Some(rgba.as_mut_ptr().cast()),
                &mut header,
                DIB_RGB_COLORS,
            );
            ReleaseDC(None, dc);
            lines == 32
        };
        let _ = DeleteObject(info.hbmColor.into());
        let _ = DeleteObject(info.hbmMask.into());
        let _ = DestroyIcon(large);
        if !read {
            return None;
        }
    }
    for px in rgba.chunks_mut(4) {
        px.swap(0, 2); // BGRA to RGBA
    }
    if rgba.chunks(4).all(|px| px[3] == 0) {
        rgba.chunks_mut(4).for_each(|px| px[3] = 255);
    }
    Some(tauri::image::Image::new_owned(rgba, 32, 32))
}

/// Open `files` with the app: spawn a program path, or invoke the named handler
/// (which also covers packaged apps that have no exe path).
pub fn open_with(app: &AppRef, files: &[PathBuf]) -> Result<(), String> {
    if Path::new(&app.id).is_file() {
        return Command::new(&app.id)
            .args(files)
            .spawn()
            .map(drop)
            .map_err(|e| e.to_string());
    }
    let _com = ComGuard::new();
    let file = files.first().ok_or("no file")?;
    let ext = extension(file).ok_or("app not found")?;
    let handler = handlers(&ext, ASSOC_FILTER_NONE)
        .into_iter()
        // SAFETY: the name is freed by `take`.
        .find(|h| unsafe { h.GetName() }.is_ok_and(|name| take(name).eq_ignore_ascii_case(&app.id)))
        .ok_or("app not found")?;
    // SAFETY: the path string outlives the call.
    unsafe {
        let item: IShellItem =
            SHCreateItemFromParsingName(&HSTRING::from(file.as_os_str()), None::<&IBindCtx>)
                .map_err(|e| e.to_string())?;
        let data: IDataObject = item
            .BindToHandler(None::<&IBindCtx>, &BHID_DataObject)
            .map_err(|e| e.to_string())?;
        handler.Invoke(&data).map_err(|e| e.to_string())
    }
}

/// The known uploader and shelf apps are macOS apps.
pub fn find_known(_known: &KnownApp) -> Option<AppRef> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_png_handlers_never_fails() {
        let dir = tempfile::tempdir().unwrap();
        let png = dir.path().join("x.png");
        std::fs::write(&png, b"").unwrap();
        let _ = list(&png); // a server runner may have none; it must not panic
        assert!(list(&dir.path().join("noext")).is_empty());
    }

    #[test]
    fn opening_with_a_missing_exe_fails() {
        let nope = AppRef {
            id: r"C:\no\such\app.exe".into(),
            name: "Nope".into(),
        };
        assert!(open_with(&nope, &[PathBuf::from(r"C:\x.png")]).is_err());
    }

    #[test]
    fn an_exe_has_a_32_px_icon() {
        let explorer = AppRef {
            id: r"C:\Windows\explorer.exe".into(),
            name: "Explorer".into(),
        };
        let icon = icon(&explorer).unwrap();
        assert_eq!((icon.width(), icon.height()), (32, 32));
        assert!(icon.rgba().chunks(4).any(|px| px[3] > 0));
    }

    #[test]
    fn a_picked_exe_keeps_its_full_path() {
        assert_eq!(
            app_ref(Path::new(r"C:\Program Files\ShareX\ShareX.exe")),
            Some(AppRef {
                id: r"C:\Program Files\ShareX\ShareX.exe".into(),
                name: "ShareX".into()
            })
        );
    }
}
