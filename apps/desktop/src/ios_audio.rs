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
use objc2::{class, msg_send, msg_send_id};

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
        let session: Retained<AnyObject> = msg_send_id![class!(AVAudioSession), sharedInstance];
        let mut err: *mut AnyObject = std::ptr::null_mut();

        let category = nsstring("AVAudioSessionCategoryPlayAndRecord");
        // DefaultToSpeaker | AllowBluetoothA2DP.
        let options: usize = 0x8 | 0x20;
        let ok: bool = msg_send![
            &*session, setCategory: category, withOptions: options, error: &mut err
        ];
        if !ok {
            tracing::warn!("AVAudioSession setCategory(playAndRecord) failed");
        }
        let _: bool = msg_send![&*session, setPreferredSampleRate: 48_000.0f64, error: &mut err];
        let _: bool = msg_send![
            &*session, setPreferredIOBufferDuration: (128.0f64 / 48_000.0), error: &mut err
        ];

        let ext = external_input_port();
        if !ext.is_null() {
            let set_in: bool = msg_send![&*session, setPreferredInput: ext, error: &mut err];
            if !set_in {
                tracing::warn!("AVAudioSession setPreferredInput failed");
            }
            tracing::info!("AVAudioSession: external interface present — input pinned");
        } else {
            // Clear any preferred input; the rig won't read the built-in mic.
            let _: bool = msg_send![
                &*session, setPreferredInput: std::ptr::null_mut::<AnyObject>(), error: &mut err
            ];
            tracing::info!("AVAudioSession: no interface — rig input stays closed (no mic)");
        }

        let active: bool = msg_send![&*session, setActive: true, error: &mut err];
        if !active {
            tracing::warn!("AVAudioSession activation failed");
        }
        // After activation, availableInputs is populated — log what's there.
        log_available_inputs();
    }
}
