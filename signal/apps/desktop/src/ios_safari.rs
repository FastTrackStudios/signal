//! The sign-in page in an in-app Safari sheet.
//!
//! The FastTrackStudio account (and through it TONE3000) signs in through a
//! web page that redirects to `localhost:4040`, where the embedded engine's
//! callback listener waits (`rig_engine.rs`). Sent out to Safari, the app
//! would go to the background and could be suspended before the redirect
//! arrives; an `SFSafariViewController` keeps it in front, so the listener
//! answers and the sheet can be closed.

use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{MainThreadMarker, msg_send};
use objc2_foundation::{NSObjectProtocol, NSString, NSURL};
use objc2_ui_kit::{UIApplication, UIViewController, UIWindowScene};

// SFSafariViewController lives in SafariServices, which nothing else links.
#[link(name = "SafariServices", kind = "framework")]
unsafe extern "C" {}

fn safari_class() -> Option<&'static AnyClass> {
    AnyClass::get(c"SFSafariViewController")
}

/// The sheet up now, so a finished sign-in can close it.
static OPEN: std::sync::Mutex<bool> = std::sync::Mutex::new(false);

/// The view controller at the top of the window: what presents the sheet.
fn top_controller(mtm: MainThreadMarker) -> Option<Retained<UIViewController>> {
    let scenes = UIApplication::sharedApplication(mtm).connectedScenes();
    let scene = scenes.iter().find_map(|s| s.downcast::<UIWindowScene>().ok())?;
    let windows = scene.windows();
    let window = windows.iter().next()?;
    let mut top = window.rootViewController()?;
    while let Some(next) = top.presentedViewController() {
        top = next;
    }
    Some(top)
}

/// Open `url` in an in-app Safari sheet (main thread only; elsewhere it
/// does nothing and says so).
pub fn open(url: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        tracing::warn!("sign-in sheet: not on the main thread");
        return;
    };
    let Some(ns_url) = NSURL::URLWithString(&NSString::from_str(url)) else {
        tracing::warn!("sign-in sheet: not a URL");
        return;
    };
    let Some(top) = top_controller(mtm) else {
        tracing::warn!("sign-in sheet: no window to present from");
        return;
    };
    let Some(class) = safari_class() else {
        tracing::warn!("sign-in sheet: SafariServices missing");
        return;
    };
    // SAFETY: `SFSafariViewController` alloc + `initWithURL:`, a
    // UIViewController subclass, on the main thread.
    let sheet: Option<Retained<UIViewController>> = unsafe {
        let alloc: *mut AnyObject = msg_send![class, alloc];
        Retained::from_raw(msg_send![alloc, initWithURL: &*ns_url])
    };
    let Some(sheet) = sheet else { return };
    top.presentViewController_animated_completion(&sheet, true, None);
    *OPEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = true;
}

/// Close the sheet, if one is up (the sign-in finished).
pub fn close() {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let mut open = OPEN.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    if !*open {
        return;
    }
    *open = false;
    if let (Some(top), Some(class)) = (top_controller(mtm), safari_class())
        && top.isKindOfClass(class)
    {
        top.dismissViewControllerAnimated_completion(true, None);
    }
}
