//! iOS: the devices are the audio session's ports.
//!
//! cpal's iOS host has one device each way, "Default Device" — the route
//! the shared `AVAudioSession` has chosen. What a player chooses between
//! (a USB interface, the built-in mic, a headset) are the session's input
//! *ports*, so on iOS those are the input devices: listing them reads
//! `availableInputs`, and opening one by name makes it the session's
//! preferred input before the default device opens — which then plays
//! through it. Outputs cannot be chosen by an app (the system routes them,
//! to a connected interface on its own), so the output devices are the
//! ports the current route plays through.
//!
//! `availableInputs` is empty unless the session's category records
//! (`playAndRecord`); the app configures the session before it lists.
//! Dynamic `objc2` messaging, as the app's own session code does — no
//! AVFAudio bindings crate.

use objc2::runtime::{AnyObject, Bool};
use objc2::{class, msg_send};

/// A port's name.
unsafe fn port_name(port: *mut AnyObject) -> String {
    let name: *mut AnyObject = msg_send![port, portName];
    if name.is_null() {
        return String::new();
    }
    let c: *const std::os::raw::c_char = msg_send![name, UTF8String];
    if c.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned()
}

/// How many channels a port carries (0 when it does not say).
unsafe fn port_channels(port: *mut AnyObject) -> u16 {
    let channels: *mut AnyObject = msg_send![port, channels];
    if channels.is_null() {
        return 0;
    }
    let n: usize = msg_send![channels, count];
    n as u16
}

unsafe fn session() -> *mut AnyObject {
    msg_send![class!(AVAudioSession), sharedInstance]
}

/// Each port of an `NSArray` of `AVAudioSessionPortDescription`s.
unsafe fn ports(list: *mut AnyObject) -> Vec<*mut AnyObject> {
    if list.is_null() {
        return Vec::new();
    }
    let count: usize = msg_send![list, count];
    (0..count).map(|i| msg_send![list, objectAtIndex: i]).collect()
}

/// The session's rate now (what the route plays at).
pub(crate) fn sample_rate() -> u32 {
    let rate: f64 = unsafe { msg_send![session(), sampleRate] };
    rate.round() as u32
}

/// A port's type (`USBAudio`, `MicrophoneBuiltIn`, …).
unsafe fn port_type(port: *mut AnyObject) -> String {
    let t: *mut AnyObject = msg_send![port, portType];
    if t.is_null() {
        return String::new();
    }
    let c: *const std::os::raw::c_char = msg_send![t, UTF8String];
    if c.is_null() {
        return String::new();
    }
    std::ffi::CStr::from_ptr(c).to_string_lossy().into_owned()
}

/// The inputs the current route records from (the ones in use).
unsafe fn route_inputs() -> Vec<*mut AnyObject> {
    let route: *mut AnyObject = msg_send![session(), currentRoute];
    if route.is_null() {
        return Vec::new();
    }
    ports(msg_send![route, inputs])
}

/// The inputs the session can record from: name and channel count — the
/// one in use first (what "Automatic" plays from). A port not in the route
/// may not say how many channels it has until it is; the one in use is
/// counted by the session (every channel the interface has, once the app
/// asked for them all).
pub(crate) fn inputs() -> Vec<(String, u16)> {
    unsafe {
        let in_use: Vec<String> = route_inputs().into_iter().map(|p| port_name(p)).collect();
        let route_channels: isize = msg_send![session(), inputNumberOfChannels];
        let list: *mut AnyObject = msg_send![session(), availableInputs];
        let mut out: Vec<(String, u16)> = ports(list)
            .into_iter()
            .map(|p| {
                let name = port_name(p);
                let mut channels = port_channels(p);
                if in_use.contains(&name) {
                    channels = channels.max(route_channels.max(0) as u16);
                }
                (name, channels)
            })
            .collect();
        out.sort_by_key(|(name, _)| !in_use.contains(name));
        out
    }
}

/// Everything the session says about input, on one line — the wide
/// event's `audio.session` field: why a guitar is or is not heard.
pub(crate) fn report() -> String {
    unsafe {
        let s = session();
        let describe = |list: Vec<*mut AnyObject>| -> String {
            list.into_iter()
                .map(|p| format!("{} [{}, {} ch]", port_name(p), port_type(p), port_channels(p)))
                .collect::<Vec<_>>()
                .join("; ")
        };
        let available = describe(ports(msg_send![s, availableInputs]));
        let route_in = describe(route_inputs());
        let route: *mut AnyObject = msg_send![s, currentRoute];
        let route_out = if route.is_null() { String::new() } else { describe(ports(msg_send![route, outputs])) };
        let preferred: *mut AnyObject = msg_send![s, preferredInput];
        let preferred = if preferred.is_null() { "none".to_string() } else { port_name(preferred) };
        let rate: f64 = msg_send![s, sampleRate];
        let io: f64 = msg_send![s, IOBufferDuration];
        let in_ch: isize = msg_send![s, inputNumberOfChannels];
        let max_in: isize = msg_send![s, maximumInputNumberOfChannels];
        let out_ch: isize = msg_send![s, outputNumberOfChannels];
        let available_flag: objc2::runtime::Bool = msg_send![s, isInputAvailable];
        let gain: f32 = msg_send![s, inputGain];
        format!(
            "access={:?} input_available={} route_in=[{route_in}] route_out=[{route_out}] available=[{available}] preferred={preferred} rate={rate} io_ms={:.2} in_ch={in_ch}/{max_in} out_ch={out_ch} input_gain={gain:.2}",
            input_access(),
            available_flag.as_bool(),
            io * 1000.0,
        )
    }
}

/// The outputs the current route plays through.
pub(crate) fn outputs() -> Vec<(String, u16)> {
    unsafe {
        let route: *mut AnyObject = msg_send![session(), currentRoute];
        if route.is_null() {
            return Vec::new();
        }
        let list: *mut AnyObject = msg_send![route, outputs];
        ports(list)
            .into_iter()
            .map(|p| (port_name(p), port_channels(p)))
            .collect()
    }
}

/// Make the input whose name contains `name` the session's preferred input.
/// Whether one did.
pub(crate) fn prefer_input(name: &str) -> bool {
    unsafe {
        let list: *mut AnyObject = msg_send![session(), availableInputs];
        let Some(port) = ports(list).into_iter().find(|p| port_name(*p).contains(name)) else {
            return false;
        };
        let set: Bool = msg_send![
            session(),
            setPreferredInput: port,
            error: std::ptr::null_mut::<*mut AnyObject>()
        ];
        set.as_bool()
    }
}

/// Whether the player has let the app record — on iOS an interface's input
/// is a recording, and without this it is silent (and the session may list
/// no inputs at all). `AVAudioApplication` from iOS 17 (the session's own
/// `recordPermission` is deprecated there), the session's before.
pub(crate) fn input_access() -> crate::device::InputAccess {
    use crate::device::InputAccess;
    // The four-char codes both classes answer with.
    const GRANTED: usize = 0x6772_6e74; // 'grnt'
    const DENIED: usize = 0x6465_6e79; // 'deny'
    let code: usize = unsafe {
        match objc2::runtime::AnyClass::get(c"AVAudioApplication") {
            Some(app) => {
                let shared: *mut AnyObject = msg_send![app, sharedInstance];
                msg_send![shared, recordPermission]
            }
            None => msg_send![session(), recordPermission],
        }
    };
    match code {
        GRANTED => InputAccess::Granted,
        DENIED => InputAccess::Denied,
        _ => InputAccess::Undetermined,
    }
}

/// Ask the session for `rate` (it may not grant it: an interface that only
/// runs at 44.1 kHz stays there), then say what the session runs at.
pub(crate) fn negotiate_rate(rate: Option<u32>) -> u32 {
    unsafe {
        if let Some(r) = rate {
            let _: objc2::runtime::Bool = msg_send![
                session(),
                setPreferredSampleRate: f64::from(r),
                error: std::ptr::null_mut::<*mut AnyObject>()
            ];
        }
    }
    sample_rate()
}
