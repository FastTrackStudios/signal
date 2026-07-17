//! iOS audio-session bootstrap.
//!
//! **The built-in mic is never used as rig input** — mic → NAM → speaker is
//! instant feedback. Input is engaged ONLY when a real external audio
//! interface is connected (USB, line-in, HDMI, Thunderbolt, …). With no
//! interface we set the session to `playback` (output only, no input
//! hardware), so the built-in mic can't feed back. With an interface we use
//! `playAndRecord` + `defaultToSpeaker` and pin the preferred input to that
//! interface. Route changes (plug/unplug) are handled by the watcher in
//! `rig_engine`, which re-runs [`configure`] and reopens the rig.
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

/// Configure + activate the shared AVAudioSession for the CURRENT hardware:
/// record only when an external interface is present, else output-only so
/// the built-in mic never feeds back. Safe to call repeatedly (on launch
/// and on every route change).
pub fn configure() {
    unsafe {
        let session: Retained<AnyObject> = msg_send_id![class!(AVAudioSession), sharedInstance];
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let ext = external_input_port();

        if !ext.is_null() {
            // Interface present → duplex, speaker out, this interface in.
            let category = nsstring("AVAudioSessionCategoryPlayAndRecord");
            let options: usize = 0x8 | 0x20; // DefaultToSpeaker | AllowBluetoothA2DP
            let ok: bool = msg_send![
                &*session, setCategory: category, withOptions: options, error: &mut err
            ];
            if !ok {
                tracing::warn!("AVAudioSession setCategory(playAndRecord) failed");
            }
            let set_in: bool = msg_send![&*session, setPreferredInput: ext, error: &mut err];
            if !set_in {
                tracing::warn!("AVAudioSession setPreferredInput failed");
            }
            let _: bool = msg_send![&*session, setPreferredSampleRate: 48_000.0f64, error: &mut err];
            let _: bool = msg_send![
                &*session, setPreferredIOBufferDuration: (128.0f64 / 48_000.0), error: &mut err
            ];
            tracing::info!("AVAudioSession: external interface input engaged");
        } else {
            // No interface → OUTPUT ONLY. The built-in mic is not engaged,
            // so there is no feedback path.
            let category = nsstring("AVAudioSessionCategoryPlayback");
            let ok: bool = msg_send![&*session, setCategory: category, error: &mut err];
            if !ok {
                tracing::warn!("AVAudioSession setCategory(playback) failed");
            }
            tracing::info!("AVAudioSession: no interface — output only (mic disabled)");
        }

        let active: bool = msg_send![&*session, setActive: true, error: &mut err];
        if !active {
            tracing::warn!("AVAudioSession activation failed");
        }
    }
}
