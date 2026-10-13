//! iOS audio-session bootstrap.
//!
//! **The built-in mic is never used as rig input** — mic → NAM → speaker is
//! instant feedback. Feedback is prevented not by muting the hardware but
//! by the rig only ever *reading* input when a real external interface is
//! present (see `rig_engine`): the built-in mic is never opened as a cpal
//! input stream.
//!
//! The session itself stays `playAndRecord` + `defaultToSpeaker`, because
//! `AVAudioSession.availableInputs` returns nil under `playback` — we could
//! never *see* a connected interface from an output-only category. With an
//! interface present we pin it as the preferred input; otherwise we simply
//! don't open the rig input. Route changes (plug/unplug) are handled by the
//! watcher in `rig_engine`, which re-runs [`configure`] and (re)opens the
//! rig.
//!
//! cpal (CoreAudio/AudioUnit) inherits whatever session we set here.
//!
//! Uses dynamic `objc2` messaging — no AVFAudio bindings crate needed.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::runtime::NSObject;
use objc2::{class, msg_send};

/// Port types that count as a real guitar/line interface (allowlist — the
/// built-in mic, wired-headset mic, and Bluetooth are deliberately absent).
const EXTERNAL_INPUT_PORTS: &[&str] = &[
    "USBAudio",
    "LineIn",
    "HDMI",
    "Thunderbolt",
    "CarAudio",
    "PCI",
    "DisplayPort",
    "AirPlay",
    "Virtual",
];

/// An `NSString` from a Rust `&str` (autoreleased).
unsafe fn nsstring(s: &str) -> *mut AnyObject {
    let c = std::ffi::CString::new(s).unwrap();
    msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()]
}

/// The first connected external audio-interface input port
/// (`AVAudioSessionPortDescription`), or null if the only inputs are the
/// built-in mic / headset / Bluetooth.
unsafe fn external_input_port() -> *mut AnyObject {
    let session: *mut AnyObject = msg_send![class!(AVAudioSession), sharedInstance];
    let inputs: *mut AnyObject = msg_send![session, availableInputs];
    if inputs.is_null() {
        return std::ptr::null_mut();
    }
    let count: usize = msg_send![inputs, count];
    for i in 0..count {
        let port: *mut AnyObject = msg_send![inputs, objectAtIndex: i];
        let ptype: *mut AnyObject = msg_send![port, portType];
        for name in EXTERNAL_INPUT_PORTS {
            let candidate = nsstring(name);
            let eq: bool = msg_send![ptype, isEqualToString: candidate];
            if eq {
                return port;
            }
        }
    }
    std::ptr::null_mut()
}

/// Whether the current route's input is an external interface — the one
/// `setPreferredInput` asked for, once the route has switched to it. Polls
/// until `within` has passed.
fn wait_for_route_input(within: std::time::Duration) -> bool {
    let deadline = std::time::Instant::now() + within;
    loop {
        let routed = unsafe {
            let session: *mut AnyObject = msg_send![class!(AVAudioSession), sharedInstance];
            let route: *mut AnyObject = msg_send![session, currentRoute];
            let inputs: *mut AnyObject = if route.is_null() { std::ptr::null_mut() } else { msg_send![route, inputs] };
            let count: usize = if inputs.is_null() { 0 } else { msg_send![inputs, count] };
            (0..count).any(|i| {
                let port: *mut AnyObject = msg_send![inputs, objectAtIndex: i];
                let ptype: *mut AnyObject = msg_send![port, portType];
                EXTERNAL_INPUT_PORTS.iter().any(|name| {
                    let candidate = nsstring(name);
                    let eq: bool = msg_send![ptype, isEqualToString: candidate];
                    eq
                })
            })
        };
        if routed || std::time::Instant::now() >= deadline {
            return routed;
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
}

/// Whether a real external audio interface is currently connected.
pub fn has_external_input() -> bool {
    unsafe { !external_input_port().is_null() }
}

/// Log every available input port (type + name) — diagnostics for "why
/// isn't my interface detected".
pub fn log_available_inputs() {
    unsafe {
        let session: *mut AnyObject = msg_send![class!(AVAudioSession), sharedInstance];
        let inputs: *mut AnyObject = msg_send![session, availableInputs];
        if inputs.is_null() {
            tracing::info!("AVAudioSession availableInputs: nil");
            return;
        }
        let count: usize = msg_send![inputs, count];
        for i in 0..count {
            let port: *mut AnyObject = msg_send![inputs, objectAtIndex: i];
            let ptype: *mut AnyObject = msg_send![port, portType];
            let pname: *mut AnyObject = msg_send![port, portName];
            let tc: *const std::os::raw::c_char = msg_send![ptype, UTF8String];
            let nc: *const std::os::raw::c_char = msg_send![pname, UTF8String];
            let ts = std::ffi::CStr::from_ptr(tc).to_string_lossy();
            let ns = std::ffi::CStr::from_ptr(nc).to_string_lossy();
            tracing::info!("available input: type={ts} name={ns}");
        }
    }
}

/// Configure + activate the shared AVAudioSession. Always `playAndRecord`
/// (so `availableInputs` can see a connected interface) + `defaultToSpeaker`;
/// when an external interface is present it's pinned as the preferred input.
/// Whether the rig actually *reads* input is decided by `rig_engine` (it
/// opens the input only for a real interface). Safe to call repeatedly.
pub fn configure() {
    unsafe {
        let session: Retained<AnyObject> = msg_send![class!(AVAudioSession), sharedInstance];

        // The order Apple's docs require and JUCE (juce_Audio_ios.cpp) and
        // aurioTouch follow: category and mode, the preferred rate and
        // buffer, activate — and only then the preferred input and its
        // channel count ("set a preferred input port only after setting the
        // audio session's category and mode and activating the session").
        // Picking the interface before activating left the route to switch
        // to it later, after the rig's streams were built on the old one.
        //
        // Mode Measurement: the least system processing on the input (no
        // voice AGC / EQ) — an instrument, not a call.
        let category = nsstring("AVAudioSessionCategoryPlayAndRecord");
        let mode = nsstring("AVAudioSessionModeMeasurement");
        // DefaultToSpeaker | AllowBluetoothA2DP.
        let options: usize = 0x8 | 0x20;
        let set: Result<(), Retained<NSObject>> =
            msg_send![&*session, setCategory: category, mode: mode, options: options, error: _];
        if set.is_err() {
            tracing::warn!("AVAudioSession setCategory(playAndRecord, measurement) failed");
        }
        // No preferred rate here: the engine asks for the player's (Audio ›
        // Interface › Sample rate) when it opens, and builds for what the
        // device grants — an interface that only runs at 44.1 kHz included.
        let _: Result<(), Retained<NSObject>> = msg_send![
            &*session, setPreferredIOBufferDuration: (128.0f64 / 48_000.0), error: _
        ];

        let active: Result<(), Retained<NSObject>> = msg_send![&*session, setActive: true, error: _];
        if active.is_err() {
            tracing::warn!("AVAudioSession activation failed");
        }

        let ext = external_input_port();
        if !ext.is_null() {
            let set_in: Result<(), Retained<NSObject>> =
                msg_send![&*session, setPreferredInput: ext, error: _];
            if set_in.is_err() {
                tracing::warn!("AVAudioSession setPreferredInput failed");
            }
            // The route switches to it asynchronously: wait (up to a second)
            // until it is the route's input, so what is built next — the
            // channel count, the rig's streams — is built on it.
            let routed = wait_for_route_input(std::time::Duration::from_secs(1));
            tracing::info!(audio.routed = routed, "AVAudioSession: external interface present — input pinned");
        } else {
            // Clear any preferred input; the rig won't read the built-in mic.
            let _: Result<(), Retained<NSObject>> = msg_send![
                &*session, setPreferredInput: std::ptr::null_mut::<AnyObject>(), error: _
            ];
            tracing::info!("AVAudioSession: no interface — rig input stays closed (no mic)");
        }
        // Every channel the route has, not the two a session gets by
        // default: a four-in interface's inputs 3-4 (and outputs 3-4) are
        // otherwise not there to pick. Only meaningful once active.
        let max_in: isize = msg_send![&*session, maximumInputNumberOfChannels];
        if max_in > 0 {
            let _: Result<(), Retained<NSObject>> =
                msg_send![&*session, setPreferredInputNumberOfChannels: max_in, error: _];
        }
        let max_out: isize = msg_send![&*session, maximumOutputNumberOfChannels];
        if max_out > 0 {
            let _: Result<(), Retained<NSObject>> =
                msg_send![&*session, setPreferredOutputNumberOfChannels: max_out, error: _];
        }
        // After activation, availableInputs is populated — log what's there.
        log_available_inputs();
    }
}

/// `AVAudioSessionRecordPermission` values (four-char codes).
const RECORD_GRANTED: usize = 0x6772_6e74; // 'grnt'
const RECORD_UNDETERMINED: usize = 0x756e_6474; // 'undt'

/// The record permission's state. iOS 17 moved it to `AVAudioApplication`
/// (the session's `recordPermission` is deprecated there); the session's
/// before it. Both answer with the same four-char codes.
fn record_permission() -> usize {
    unsafe {
        match objc2::runtime::AnyClass::get(c"AVAudioApplication") {
            Some(app) => {
                let shared: *mut AnyObject = msg_send![app, sharedInstance];
                msg_send![shared, recordPermission]
            }
            None => {
                let session: *mut AnyObject = msg_send![class!(AVAudioSession), sharedInstance];
                msg_send![session, recordPermission]
            }
        }
    }
}

/// Whether the player has let the app record (an interface's input is a
/// recording, as far as iOS is concerned).
pub fn record_permission_granted() -> bool {
    record_permission() == RECORD_GRANTED
}

/// Ask for the record permission if it has never been asked: without it an
/// interface's input is silent. The answer arrives later; the route watcher
/// (`rig_engine`) restarts the rig when it turns to granted.
pub fn request_record_permission() {
    let p = record_permission();
    tracing::info!(state = format!("{:#x}", p), "ios: record permission");
    if p != RECORD_UNDETERMINED {
        return;
    }
    unsafe {
        let done = block2::RcBlock::new(|granted: objc2::runtime::Bool| {
            tracing::info!(granted = granted.as_bool(), "ios: record permission answered");
        });
        match objc2::runtime::AnyClass::get(c"AVAudioApplication") {
            // A class method from iOS 17.
            Some(app) => {
                let _: () = msg_send![app, requestRecordPermissionWithCompletionHandler: &*done];
            }
            None => {
                let session: *mut AnyObject = msg_send![class!(AVAudioSession), sharedInstance];
                let _: () = msg_send![session, requestRecordPermission: &*done];
            }
        }
    }
}
