//! macOS Services: "Optimise with Ferry" in Finder's Services / Quick Actions
//! menu (declared in `Info.plist`). AppKit calls the provider's `optimiseFiles`
//! on the main thread with a pasteboard holding the selected files.

use std::cell::OnceCell;
use std::path::PathBuf;
use std::sync::OnceLock;

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{NSApplication, NSPasteboard, NSUpdateDynamicServices};
use objc2_foundation::{NSObject, NSObjectProtocol, NSString, NSURL};
use tauri::AppHandle;

use crate::integrations::launch;

const FILE_URL: &str = "public.file-url";

static APP: OnceLock<AppHandle> = OnceLock::new();

thread_local! {
    /// NSApplication does not retain its services provider.
    static PROVIDER: OnceCell<Retained<ServiceProvider>> = const { OnceCell::new() };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct ServiceProvider;

    unsafe impl NSObjectProtocol for ServiceProvider {}

    impl ServiceProvider {
        #[unsafe(method(optimiseFiles:userData:error:))]
        fn optimise_files(
            &self,
            pboard: &NSPasteboard,
            _user_data: Option<&NSString>,
            _error: *mut *mut NSString,
        ) {
            if let Some(app) = APP.get() {
                launch::optimise_paths(app, file_paths(pboard));
            }
        }
    }
);

impl ServiceProvider {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// Offer the Services provider. Runs on the main thread (`setup`).
pub fn register(app: &AppHandle) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let _ = APP.set(app.clone());
    let provider = PROVIDER.with(|cell| cell.get_or_init(|| ServiceProvider::new(mtm)).clone());
    let provider: &AnyObject = &provider;
    // SAFETY: the provider implements `optimiseFiles:userData:error:` and lives in PROVIDER.
    unsafe { NSApplication::sharedApplication(mtm).setServicesProvider(Some(provider)) };
    NSUpdateDynamicServices();
}

/// The files on a pasteboard: each item's `public.file-url`.
pub(crate) fn file_paths(pb: &NSPasteboard) -> Vec<PathBuf> {
    pb.pasteboardItems()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.stringForType(&NSString::from_str(FILE_URL)))
                .filter_map(|url| NSURL::URLWithString(&url)?.to_file_path())
                .collect()
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::runtime::ProtocolObject;
    use objc2_app_kit::NSPasteboardWriting;
    use objc2_foundation::NSArray;

    /// A private pasteboard, so tests never touch the user's clipboard.
    struct Private(Retained<NSPasteboard>);
    impl Private {
        fn new() -> Self {
            Self(NSPasteboard::pasteboardWithUniqueName())
        }
    }
    impl Drop for Private {
        fn drop(&mut self) {
            // Not in objc2-app-kit's bindings; frees the named pasteboard server-side.
            let _: () = unsafe { objc2::msg_send![&*self.0, releaseGlobally] };
        }
    }

    fn write_urls(pb: &NSPasteboard, paths: &[PathBuf]) {
        let urls: Vec<Retained<ProtocolObject<dyn NSPasteboardWriting>>> = paths
            .iter()
            .map(|p| NSURL::from_file_path(p).map(ProtocolObject::from_retained))
            .collect::<Option<_>>()
            .unwrap();
        pb.clearContents();
        assert!(pb.writeObjects(&NSArray::from_retained_slice(&urls)));
    }

    #[test]
    fn a_services_pasteboard_gives_its_files() {
        let pb = Private::new();
        let dir = tempfile::tempdir().unwrap();
        let a = dir.path().join("shot a.png");
        let b = dir.path().join("b.jpg");
        write_urls(&pb.0, &[a.clone(), b.clone()]);
        assert_eq!(file_paths(&pb.0), vec![a, b]);
    }
}
