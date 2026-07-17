//! iOS audio-session bootstrap.
//!
//! cpal (CoreAudio/AudioUnit) inherits whatever `AVAudioSession` the app
//! configured, so before the rig opens audio we set:
//!
//! - category `playAndRecord` — duplex: interface/mic in, output out
//! - `allowBluetoothA2DP` + `defaultToSpeaker` — without the override, a
//!   plugged USB interface makes iOS route BOTH directions to it; with it,
//!   the input follows the interface while the output falls back to the
//!   speaker when no headphone/interface output is preferred. The user can
//!   still pick routes from Control Center; the rig's own device prefs
//!   choose among cpal devices on top of this session.
//! - preferred sample rate 48 kHz and a small IO buffer for live playing.
//!
//! Uses dynamic `objc2` messaging — no AVFAudio bindings crate needed for
//! four calls.

use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use objc2::{class, msg_send, msg_send_id};

/// Configure and activate the shared AVAudioSession. Call once at startup,
/// before any cpal stream opens. Logs and continues on failure — the rig
/// then runs on whatever default session iOS gave the app.
pub fn configure() {
    unsafe {
        let session: Retained<AnyObject> =
            msg_send_id![class!(AVAudioSession), sharedInstance];

        // NSString category constant: AVAudioSessionCategoryPlayAndRecord.
        let category: Retained<AnyObject> = msg_send_id![
            class!(NSString),
            stringWithUTF8String: c"AVAudioSessionCategoryPlayAndRecord".as_ptr()
        ];
        // Options: DefaultToSpeaker (0x8) | AllowBluetoothA2DP (0x20).
        let options: usize = 0x8 | 0x20;
        let mut err: *mut AnyObject = std::ptr::null_mut();
        let ok: bool = msg_send![
            &*session,
            setCategory: &*category,
            withOptions: options,
            error: &mut err
        ];
        if !ok {
            tracing::warn!("AVAudioSession setCategory failed");
        }

        let _: bool = msg_send![&*session, setPreferredSampleRate: 48_000.0f64, error: &mut err];
        // 128 frames @ 48 kHz ≈ 2.7 ms.
        let _: bool =
            msg_send![&*session, setPreferredIOBufferDuration: (128.0f64 / 48_000.0), error: &mut err];

        let active: bool = msg_send![&*session, setActive: true, error: &mut err];
        if active {
            tracing::info!("AVAudioSession active (playAndRecord, speaker default)");
        } else {
            tracing::warn!("AVAudioSession activation failed");
        }
    }
}
