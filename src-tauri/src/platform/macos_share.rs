//! macOS sharing: the system share picker anchored to a card, and AirDrop. The
//! stack is a non-activating panel, so Ferry activates around the share UI (as
//! Quick Look does) and hands focus back when it ends.

use std::cell::{OnceCell, RefCell};
use std::path::PathBuf;

use ferry_core::i18n::t;
use ferry_core::integrations::{Anchor, Toward, view_rect};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, Message, define_class};
use objc2_app_kit::{
    NSApplication, NSSharingService, NSSharingServiceDelegate, NSSharingServiceNameSendViaAirDrop,
    NSSharingServicePicker, NSSharingServicePickerDelegate, NSView,
};
use objc2_foundation::{
    NSArray, NSError, NSObject, NSObjectProtocol, NSPoint, NSRect, NSRectEdge, NSSize, NSURL,
};
use tauri::WebviewWindow;
use tracing::warn;

use super::preview::{give_back_focus, remember_front};

/// `false` skips activating Ferry around the share UI. Switch it when the
/// picker does not take clicks or service windows open behind the frontmost app.
const ACTIVATE_FOR_SHARE: bool = true;

/// What is on screen, kept alive until it ends.
#[allow(dead_code)]
enum Held {
    Picker(Retained<NSSharingServicePicker>),
    Service(Retained<NSSharingService>),
}

thread_local! {
    static HELD: RefCell<Option<Held>> = const { RefCell::new(None) };
    static DELEGATE: OnceCell<Retained<ShareDelegate>> = const { OnceCell::new() };
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    struct ShareDelegate;

    unsafe impl NSObjectProtocol for ShareDelegate {}

    unsafe impl NSSharingServicePickerDelegate for ShareDelegate {
        #[unsafe(method(sharingServicePicker:didChooseSharingService:))]
        fn did_choose(&self, _picker: &NSSharingServicePicker, service: Option<&NSSharingService>) {
            // No service: the picker was dismissed.
            if service.is_none() {
                finish();
            }
        }

        #[unsafe(method_id(sharingServicePicker:delegateForSharingService:))]
        fn delegate_for(
            &self,
            _picker: &NSSharingServicePicker,
            _service: &NSSharingService,
        ) -> Option<Retained<ProtocolObject<dyn NSSharingServiceDelegate>>> {
            Some(ProtocolObject::from_retained(self.retain()))
        }
    }

    unsafe impl NSSharingServiceDelegate for ShareDelegate {
        #[unsafe(method(sharingService:didShareItems:))]
        fn did_share(&self, _service: &NSSharingService, _items: &NSArray) {
            finish();
        }

        #[unsafe(method(sharingService:didFailToShareItems:error:))]
        fn did_fail(&self, _service: &NSSharingService, _items: &NSArray, error: &NSError) {
            warn!("sharing failed: {error:?}");
            finish();
        }
    }
);

impl ShareDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(());
        // SAFETY: NSObject's designated initialiser.
        unsafe { objc2::msg_send![super(this), init] }
    }
}

fn delegate(mtm: MainThreadMarker) -> Retained<ShareDelegate> {
    DELEGATE.with(|cell| cell.get_or_init(|| ShareDelegate::new(mtm)).clone())
}

/// The share UI ended: let go of it and give the keyboard back.
fn finish() {
    HELD.with(|held| held.borrow_mut().take());
    give_back_focus();
}

fn activate(mtm: MainThreadMarker) {
    if ACTIVATE_FOR_SHARE {
        remember_front();
        // What Quick Look does; NSApplication.activate() needs macOS 14 and Ferry supports 13.
        #[allow(deprecated)]
        NSApplication::sharedApplication(mtm).activateIgnoringOtherApps(true);
    }
}

fn items(paths: &[PathBuf]) -> Result<Retained<NSArray>, String> {
    let urls: Vec<Retained<NSURL>> = paths
        .iter()
        .map(NSURL::from_file_path)
        .collect::<Option<_>>()
        .ok_or("not an absolute path")?;
    // SAFETY: an array of NSURL is an array of objects.
    Ok(unsafe { Retained::cast_unchecked(NSArray::from_retained_slice(&urls)) })
}

/// Open the share picker next to the card. Main thread only.
pub fn share(window: &WebviewWindow, paths: Vec<PathBuf>, anchor: &Anchor) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    let view = window.ns_view().map_err(|e| e.to_string())?;
    // SAFETY: the window's content view, borrowed for this call on the main thread.
    let view: &NSView = unsafe { &*view.cast::<NSView>() };
    let at = view_rect(anchor, view.bounds().size.height, view.isFlipped());
    let rect = NSRect::new(NSPoint::new(at.x, at.y), NSSize::new(at.width, at.height));
    let edge = match anchor.toward {
        Toward::Left => NSRectEdge::MinX,
        Toward::Right => NSRectEdge::MaxX,
    };
    let items = items(&paths)?;
    // SAFETY: `items` holds NSURLs, which the picker accepts.
    let picker =
        unsafe { NSSharingServicePicker::initWithItems(NSSharingServicePicker::alloc(), &items) };
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate(mtm))));
    activate(mtm);
    picker.showRelativeToRect_ofView_preferredEdge(rect, view, edge);
    HELD.with(|held| *held.borrow_mut() = Some(Held::Picker(picker)));
    Ok(())
}

/// Send the files with AirDrop. Main thread only.
pub fn airdrop(paths: &[PathBuf]) -> Result<(), String> {
    let mtm = MainThreadMarker::new().ok_or("not on the main thread")?;
    // SAFETY: a constant exported by AppKit.
    let name = unsafe { NSSharingServiceNameSendViaAirDrop };
    let unavailable = || t("share.error.airdrop");
    let service = NSSharingService::sharingServiceNamed(name).ok_or_else(unavailable)?;
    let items = items(paths)?;
    // SAFETY: `items` holds NSURLs.
    if !unsafe { service.canPerformWithItems(Some(&items)) } {
        return Err(unavailable());
    }
    service.setDelegate(Some(ProtocolObject::from_ref(&*delegate(mtm))));
    activate(mtm);
    // SAFETY: as above.
    unsafe { service.performWithItems(&items) };
    HELD.with(|held| *held.borrow_mut() = Some(Held::Service(service)));
    Ok(())
}
