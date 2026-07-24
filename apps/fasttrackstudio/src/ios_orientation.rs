//! Per-screen orientation on iOS.
//!
//! The app is portrait by default (home page); the guitar-rig surface is a
//! wide control panel that wants landscape. Info.plist lists BOTH portrait
//! and landscape (the superset); at runtime we ask the window scene to
//! rotate with `requestGeometryUpdate` (iOS 16+) as the user moves between
//! screens.
//!
//! `UIInterfaceOrientationMask` values (bit = 1 << UIInterfaceOrientation):
//! portrait = 2, landscapeLeft = 8, landscapeRight = 16, landscape = 24.

use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

const MASK_PORTRAIT: usize = 1 << 1; // 2
const MASK_LANDSCAPE: usize = (1 << 3) | (1 << 4); // 24 (left|right)

/// Rotate to landscape (the guitar rig).
pub fn landscape() {
    request(MASK_LANDSCAPE);
}

/// Rotate to portrait (the home page and everything else).
pub fn portrait() {
    request(MASK_PORTRAIT);
}

/// Force the Local Network permission prompt. iOS only asks when it
/// notices "local network" API use, and iroh's raw UDP unicast gets
/// silently filtered instead of prompting on recent iOS — so we kick a
/// Bonjour browse (the canonical trigger; `NSBonjourServices` lists the
/// type in Info.plist). The browser object is intentionally leaked so
/// the search — and the prompt — survive this call. Call once from the
/// keys surface, on the main thread.
pub fn request_local_network() {
    use std::sync::Once;
    static ONCE: Once = Once::new();
    ONCE.call_once(|| unsafe {
        let browser: *mut AnyObject = msg_send![class!(NSNetServiceBrowser), new];
        if browser.is_null() {
            return;
        }
        let service_type = nsstring("_fts._tcp.");
        let domain = nsstring("local.");
        let _: () = msg_send![browser, searchForServicesOfType: service_type, inDomain: domain];
    });
}

/// A retained `NSString` from a Rust str (leaked to the objc runtime).
unsafe fn nsstring(s: &str) -> *mut AnyObject {
    let c = std::ffi::CString::new(s).unwrap();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

/// Keep the screen awake (`UIApplication.idleTimerDisabled`) — on while a
/// pack download runs, since iOS suspends the app (and its sockets) when
/// the phone locks.
pub fn set_idle_timer_disabled(disabled: bool) {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let _: () = msg_send![app, setIdleTimerDisabled: disabled];
    }
}

/// Ask every window scene to adopt `mask`. No-op if the scene isn't up yet.
/// Manual retain/release (no ARC) — raw pointers are used immediately and
/// the one owned object (`prefs`) is released after use.
fn request(mask: usize) {
    unsafe {
        let app: *mut AnyObject = msg_send![class!(UIApplication), sharedApplication];
        if app.is_null() {
            return;
        }
        let scenes: *mut AnyObject = msg_send![app, connectedScenes];
        let all: *mut AnyObject = msg_send![scenes, allObjects];
        let count: usize = msg_send![all, count];
        let window_scene_cls = class!(UIWindowScene);
        for i in 0..count {
            let scene: *mut AnyObject = msg_send![all, objectAtIndex: i];
            let is_window_scene: bool = msg_send![scene, isKindOfClass: window_scene_cls];
            if !is_window_scene {
                continue;
            }
            let prefs: *mut AnyObject =
                msg_send![class!(UIWindowSceneGeometryPreferencesIOS), alloc];
            let prefs: *mut AnyObject = msg_send![prefs, initWithInterfaceOrientations: mask];
            let _: () = msg_send![
                scene,
                requestGeometryUpdateWithPreferences: prefs,
                errorHandler: std::ptr::null_mut::<AnyObject>()
            ];
            let _: () = msg_send![prefs, release];
            // Nudge the root VC to re-evaluate (paired with a supported-
            // orientations override if we add a hard lock later).
            let key_window: *mut AnyObject = msg_send![scene, keyWindow];
            if !key_window.is_null() {
                let root_vc: *mut AnyObject = msg_send![key_window, rootViewController];
                if !root_vc.is_null() {
                    let _: () = msg_send![root_vc, setNeedsUpdateOfSupportedInterfaceOrientations];
                }
            }
        }
    }
}
