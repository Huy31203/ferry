//! macOS: the apps that open a file (the "Open With" list), their icons, and
//! opening a file with one. LaunchServices and `open` do the work.

use std::path::{Path, PathBuf};
use std::process::Command;

use ferry_core::integrations::{KnownApp, ListedApp, find_in_dirs, mac_open_args};
use ferry_core::settings::AppRef;
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage, NSWorkspace};
use objc2_foundation::{NSDictionary, NSPoint, NSRect, NSSize, NSString, NSURL};

fn file_url(path: &Path) -> Retained<NSURL> {
    NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()))
}

/// The apps LaunchServices offers for `file`, the default one marked. Unordered.
pub fn list(file: &Path) -> Vec<ListedApp> {
    let workspace = NSWorkspace::sharedWorkspace();
    let url = file_url(file);
    let default = workspace
        .URLForApplicationToOpenURL(&url)
        .and_then(|app| app.to_file_path());
    workspace
        .URLsForApplicationsToOpenURL(&url)
        .iter()
        .filter_map(|app| app.to_file_path())
        .filter_map(|path| {
            Some(ListedApp {
                default: default.as_ref() == Some(&path),
                app: app_ref(&path)?,
            })
        })
        .collect()
}

/// An app bundle as a setting: its bundle identifier (its path when it has
/// none) and its file name without `.app`.
pub fn app_ref(path: &Path) -> Option<AppRef> {
    let id = super::macos::app_identifier(path).unwrap_or_else(|| path.to_string_lossy().into());
    let name = path.file_stem()?.to_string_lossy().into_owned();
    Some(AppRef { id, name })
}

fn app_path(app: &AppRef) -> Option<PathBuf> {
    if app.id.starts_with('/') {
        return Some(PathBuf::from(&app.id));
    }
    NSWorkspace::sharedWorkspace()
        .URLForApplicationWithBundleIdentifier(&NSString::from_str(&app.id))?
        .to_file_path()
}

/// The app's icon at 32 px, for a menu item.
pub fn icon(app: &AppRef) -> Option<tauri::image::Image<'static>> {
    let path = app_path(app)?;
    let source =
        NSWorkspace::sharedWorkspace().iconForFile(&NSString::from_str(&path.to_string_lossy()));
    let size = NSSize::new(32.0, 32.0);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    #[allow(deprecated)]
    image.lockFocus();
    source.drawInRect(NSRect::new(NSPoint::new(0.0, 0.0), size));
    #[allow(deprecated)]
    image.unlockFocus();
    let tiff = image.TIFFRepresentation()?;
    let rep = NSBitmapImageRep::imageRepWithData(&tiff)?;
    // SAFETY: an empty dictionary is a valid properties argument.
    let png = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(
        tauri::image::Image::from_bytes(&png.to_vec())
            .ok()?
            .to_owned(),
    )
}

/// Open `files` with the app: `open -b <bundle id>` or `-a <path>`.
pub fn open_with(app: &AppRef, files: &[PathBuf]) -> Result<(), String> {
    let output = Command::new("/usr/bin/open")
        .args(mac_open_args(&app.id, files))
        .output()
        .map_err(|e| e.to_string())?;
    if output.status.success() {
        Ok(())
    } else {
        Err(String::from_utf8_lossy(&output.stderr).trim().to_string())
    }
}

/// A well-known app: by bundle id, else by name in the Applications folders.
pub fn find_known(known: &KnownApp) -> Option<AppRef> {
    let workspace = NSWorkspace::sharedWorkspace();
    let by_id = known.bundle_ids.iter().find_map(|id| {
        workspace
            .URLForApplicationWithBundleIdentifier(&NSString::from_str(id))?
            .to_file_path()
    });
    let path = by_id.or_else(|| {
        let mut dirs = vec![
            PathBuf::from("/Applications"),
            PathBuf::from("/Applications/Setapp"),
        ];
        dirs.extend(std::env::var_os("HOME").map(|home| PathBuf::from(home).join("Applications")));
        find_in_dirs(known, &dirs)
    })?;
    Some(AppRef {
        name: known.name.into(),
        ..app_ref(&path)?
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn png() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("x.png");
        std::fs::write(&path, b"").unwrap(); // LaunchServices goes by the extension
        (dir, path)
    }

    #[test]
    fn preview_opens_png_files() {
        let (_dir, png) = png();
        let ids: Vec<String> = list(&png).into_iter().map(|l| l.app.id).collect();
        assert!(ids.iter().any(|id| id == "com.apple.Preview"), "{ids:?}");
    }

    #[test]
    fn a_picked_app_bundle_becomes_its_identifier() {
        assert_eq!(
            app_ref(Path::new("/System/Applications/Preview.app")),
            Some(AppRef {
                id: "com.apple.Preview".into(),
                name: "Preview".into()
            })
        );
    }

    #[test]
    fn a_known_app_is_found_by_bundle_id_and_has_an_icon() {
        const PREVIEW: KnownApp = KnownApp {
            name: "Preview",
            bundle_ids: &["com.apple.Preview"],
            bundle_names: &[],
        };
        let found = find_known(&PREVIEW).unwrap();
        assert_eq!(found.id, "com.apple.Preview");
        let icon = icon(&found).unwrap();
        assert!(icon.width() >= 32 && icon.width() == icon.height());
    }

    #[test]
    fn opening_with_a_missing_app_fails() {
        let (_dir, png) = png();
        let nope = AppRef {
            id: "io.ferry.no-such-app".into(),
            name: "Nope".into(),
        };
        assert!(open_with(&nope, &[png]).is_err());
    }
}
