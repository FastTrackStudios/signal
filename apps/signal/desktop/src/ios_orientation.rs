//! Small UIKit calls the iPhone shell makes: the Local Network prompt, the
//! pasteboard, the build number, the idle timer. (The app is landscape
//! only — Info.plist's orientations, written by ios/app-plist.sh — so there
//! is no per-screen rotation here any more.)

use objc2::runtime::AnyObject;
use objc2::{class, msg_send};

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

/// Copy text to the system pasteboard (the Logs tab's Copy button).
pub fn set_clipboard(text: &str) {
    unsafe {
        let pb: *mut AnyObject = msg_send![class!(UIPasteboard), generalPasteboard];
        if pb.is_null() {
            return;
        }
        let s = nsstring(text);
        let _: () = msg_send![pb, setString: s];
    }
}

/// The bundle's CFBundleVersion (the TestFlight build number) — shown in
/// the keys UI so "which build are you on" is never a guessing game.
pub fn build_number() -> String {
    unsafe {
        let bundle: *mut AnyObject = msg_send![class!(NSBundle), mainBundle];
        if bundle.is_null() {
            return String::new();
        }
        let key = nsstring("CFBundleVersion");
        let value: *mut AnyObject = msg_send![bundle, objectForInfoDictionaryKey: key];
        if value.is_null() {
            return String::new();
        }
        let utf8: *const std::ffi::c_char = msg_send![value, UTF8String];
        if utf8.is_null() {
            return String::new();
        }
        std::ffi::CStr::from_ptr(utf8)
            .to_string_lossy()
            .into_owned()
    }
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

