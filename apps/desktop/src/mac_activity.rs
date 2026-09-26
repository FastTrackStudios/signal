//! Keep the rig at full priority when it is not the frontmost app.
//!
//! macOS puts a background app into App Nap: its timers are coalesced and
//! its threads throttled. The audio callback itself runs on CoreAudio's
//! real-time thread, but it is fed by the rest of the process — chain
//! rebuilds, parameter writes, the locks those hold — and a throttled thread
//! holding one past a buffer's deadline is an xrun. Played from another
//! app's window, the rig dropped out.
//!
//! A process declares it must not be throttled with an `NSProcessInfo`
//! activity: `UserInitiated` (no App Nap, no idle sleep while it runs — a
//! live instrument) plus `LatencyCritical` (no timer coalescing). Held for
//! the life of the process.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send};
use objc2_foundation::NSString;

/// `NSActivityUserInitiated`.
const USER_INITIATED: u64 = 0x00FF_FFFF;
/// `NSActivityLatencyCritical`.
const LATENCY_CRITICAL: u64 = 0xFF_0000_0000;

/// Begin the activity and hold it until the process exits.
pub fn hold_realtime_activity() {
    let reason = NSString::from_str("Live audio: the guitar rig plays in real time");
    // SAFETY: `+[NSProcessInfo processInfo]` and
    // `-beginActivityWithOptions:reason:` are plain Foundation calls with the
    // declared argument and return types; the returned activity token is an
    // object we retain.
    let activity: Option<Retained<AnyObject>> = unsafe {
        let info: Retained<AnyObject> = msg_send![class!(NSProcessInfo), processInfo];
        msg_send![&*info, beginActivityWithOptions: USER_INITIATED | LATENCY_CRITICAL, reason: &*reason]
    };
    match activity {
        // The token must outlive the process's audio: never ended.
        Some(token) => std::mem::forget(token),
        None => tracing::warn!("macOS activity not granted: the rig may be throttled in the background"),
    }
}
