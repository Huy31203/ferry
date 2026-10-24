//! Windows sharing: the system share UI (which includes Nearby Share) through
//! `DataTransferManager`. Not compilable off Windows; CI builds it.

use std::path::PathBuf;
use std::sync::Mutex;

use ferry_core::integrations::Anchor;
use tauri::WebviewWindow;
use windows::ApplicationModel::DataTransfer::{DataRequestedEventArgs, DataTransferManager};
use windows::Foundation::TypedEventHandler;
use windows::Storage::{IStorageItem, StorageFile};
use windows::Win32::Foundation::HWND;
use windows::Win32::UI::Shell::IDataTransferManagerInterop;
use windows::core::{HSTRING, Interface};

/// The `DataRequested` subscription of the last share, replaced by the next.
static TOKEN: Mutex<Option<i64>> = Mutex::new(None);

/// Open the Windows share UI for `paths`. The UI places itself, so `_anchor` is
/// unused. Main thread only (the command is sync). The files are opened when
/// the share UI asks for them, on the UI thread: `StorageFile` cannot cross threads.
pub fn share(window: &WebviewWindow, paths: Vec<PathBuf>, _anchor: &Anchor) -> Result<(), String> {
    let title = paths
        .first()
        .and_then(|path| path.file_name())
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    let hwnd = HWND(window.hwnd().map_err(|e| e.to_string())?.0);
    show(hwnd, paths, title).map_err(|e| e.to_string())
}

fn show(hwnd: HWND, paths: Vec<PathBuf>, title: String) -> windows::core::Result<()> {
    let interop = windows::core::factory::<DataTransferManager, IDataTransferManagerInterop>()?;
    // SAFETY: `hwnd` is the stack window's handle, valid on the main thread.
    let manager: DataTransferManager = unsafe { interop.GetForWindow(hwnd)? };
    if let Some(old) = TOKEN.lock().unwrap().take() {
        manager.RemoveDataRequested(old)?;
    }
    let token = manager.DataRequested(&TypedEventHandler::<
        DataTransferManager,
        DataRequestedEventArgs,
    >::new(move |_, args| {
        let data = args.ok()?.Request()?.Data()?;
        data.Properties()?.SetTitle(&HSTRING::from(&title))?;
        let mut items: Vec<Option<IStorageItem>> = Vec::new();
        for path in &paths {
            let file =
                StorageFile::GetFileFromPathAsync(&HSTRING::from(path.as_os_str()))?.join()?;
            items.push(file.cast().ok());
        }
        data.SetStorageItemsReadOnly(&windows_collections::IIterable::<IStorageItem>::from(items))
    }))?;
    *TOKEN.lock().unwrap() = Some(token);
    // SAFETY: as above.
    unsafe { interop.ShowShareUIForWindow(hwnd) }
}
