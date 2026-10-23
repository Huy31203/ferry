//! macOS quick preview and focus hand-back. Quick Look's shared panel shows a
//! card's result. Ferry is an accessory app that is never frontmost, so it
//! activates to make the panel key (Esc and Space then reach it), and hands
//! focus back to the app the user was in when the panel or the compare window
//! closes.

use std::cell::{OnceCell, RefCell};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::Mutex;

use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSApplication, NSApplicationActivationOptions, NSRunningApplication, NSWindowDelegate,
    NSWindowSharingType, NSWorkspace,
};
use objc2_foundation::{NSInteger, NSNotification, NSObject, NSObjectProtocol, NSString, NSURL};
use objc2_quick_look_ui::{
    QLPreviewItem, QLPreviewPanel, QLPreviewPanelDataSource, QLPreviewPanelDelegate,
};
use tauri::AppHandle;
use tracing::warn;

/// `false` sends every preview to `qlmanage -p`, the plainer fallback for when
/// the panel cannot take keys from an accessory app (spec §9).
const USE_PANEL: bool = true;

/// The app that was frontmost before Ferry took focus.
static FRONT: Mutex<Option<Retained<NSRunningApplication>>> = Mutex::new(None);
/// The `qlmanage -p` fallback while it runs.
static QLMANAGE: Mutex<Option<Child>> = Mutex::new(None);

thread_local! {
    /// The panel's data source and delegate; the panel does not retain them.
    static PREVIEW: OnceCell<Retained<Preview>> = const { OnceCell::new() };
}

#[derive(Default)]
struct Ivars {
    url: RefCell<Option<Retained<NSURL>>>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    struct Preview;

    unsafe impl NSObjectProtocol for Preview {}

    unsafe impl QLPreviewPanelDataSource for Preview {
        #[unsafe(method(numberOfPreviewItemsInPreviewPanel:))]
        fn count(&self, _panel: Option<&QLPreviewPanel>) -> NSInteger {
            NSInteger::from(self.ivars().url.borrow().is_some())
        }

        #[unsafe(method_id(previewPanel:previewItemAtIndex:))]
        fn item(
            &self,
            _panel: Option<&QLPreviewPanel>,
            _index: NSInteger,
        ) -> Option<Retained<ProtocolObject<dyn QLPreviewItem>>> {
            self.ivars()
                .url
                .borrow()
                .clone()
                .map(ProtocolObject::from_retained)
        }
    }

    unsafe impl NSWindowDelegate for Preview {
        #[unsafe(method(windowWillClose:))]
        fn will_close(&self, _notification: &NSNotification) {
            give_back_focus();
        }
    }

    unsafe impl QLPreviewPanelDelegate for Preview {}
);

impl Preview {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(Ivars::default());
        // SAFETY: NSObject's designated initialiser.
        unsafe { msg_send![super(this), init] }
    }
}

/// Note the frontmost app before Ferry takes focus; nothing while Ferry
/// itself is active (Settings in front).
pub fn remember_front() {
    let front = if NSRunningApplication::currentApplication().isActive() {
        None
    } else {
        NSWorkspace::sharedWorkspace().frontmostApplication()
    };
    *FRONT.lock().unwrap() = front;
}

/// Activate the app `remember_front` noted, once.
pub fn give_back_focus() {
    if let Some(app) = FRONT.lock().unwrap().take() {
        app.activateWithOptions(NSApplicationActivationOptions::empty());
    }
}

/// Show `path` in Quick Look, or close Quick Look if it is open. `protected`
/// hides the panel from screenshots and screen sharing, like the stack.
pub fn quick_look(app: &AppHandle, path: PathBuf, protected: bool) -> Result<(), String> {
    app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        if !(USE_PANEL && toggle_panel(mtm, &path, protected)) {
            toggle_qlmanage(&path);
        }
    })
    .map_err(|e| e.to_string())
}

/// False when the shared panel is unavailable.
fn toggle_panel(mtm: MainThreadMarker, path: &Path, protected: bool) -> bool {
    // SAFETY: called on the main thread, as the marker proves.
    let Some(panel) = (unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }) else {
        return false;
    };
    if panel.isVisible() {
        panel.close(); // windowWillClose: hands focus back
        return true;
    }
    let preview = PREVIEW.with(|cell| cell.get_or_init(|| Preview::new(mtm)).clone());
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    *preview.ivars().url.borrow_mut() = Some(url);
    // SAFETY: `preview` lives in PREVIEW for the rest of the app's life.
    unsafe {
        panel.setDataSource(Some(ProtocolObject::from_ref(&*preview)));
        panel.setDelegate(Some(&preview));
    }
    panel.setSharingType(sharing(protected));
    remember_front();
    // What tao's set_focus does for the Settings window; NSApplication.activate()
    // needs macOS 14 and Ferry supports 13.
    #[allow(deprecated)]
    NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    panel.makeKeyAndOrderFront(None);
    // SAFETY: the data source is set.
    unsafe { panel.reloadData() };
    true
}

/// Follow "Show app UI in screenshots" live (`windows::set_content_protected`).
/// `qlmanage` is another process and always shows.
pub fn set_protected(app: &AppHandle, protected: bool) {
    let _ = app.run_on_main_thread(move || {
        let Some(mtm) = MainThreadMarker::new() else {
            return;
        };
        // SAFETY: on the main thread; only an existing panel is touched.
        if unsafe { QLPreviewPanel::sharedPreviewPanelExists(mtm) }
            && let Some(panel) = unsafe { QLPreviewPanel::sharedPreviewPanel(mtm) }
        {
            panel.setSharingType(sharing(protected));
        }
    });
}

fn sharing(protected: bool) -> NSWindowSharingType {
    if protected {
        NSWindowSharingType::None
    } else {
        NSWindowSharingType::ReadOnly
    }
}

/// `qlmanage -p` runs in its own process, takes focus and gives it back when
/// Esc closes it. A second press while it runs closes it.
fn toggle_qlmanage(path: &Path) {
    let mut running = QLMANAGE.lock().unwrap();
    if let Some(mut child) = running.take()
        && matches!(child.try_wait(), Ok(None))
    {
        let _ = child.kill();
        let _ = child.wait();
        return;
    }
    match Command::new("/usr/bin/qlmanage")
        .arg("-p")
        .arg(path)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => *running = Some(child),
        Err(e) => warn!("could not start qlmanage: {e}"),
    }
}
