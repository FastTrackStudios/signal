//! The device's own audio, as its platform reports it: where the sound is
//! going and coming from, at what rate and buffer, and the switches for it.
//! The Settings page shows it (`crate::setup`) when the host provides a
//! [`DeviceAudio`] — an iPhone's or an iPad's audio session does; a desktop
//! or a page has none to show.

use dioxus::prelude::*;

/// Where the device's audio is, now.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct Report {
    /// The outputs the sound goes to, by name (`Speaker`, `AirPods Pro`).
    pub output: String,
    /// The input it comes from (`iPhone Microphone`), empty with none.
    pub input: String,
    /// The hardware's rate, in hertz.
    pub sample_rate: f64,
    /// The buffer, and the output's latency, in milliseconds.
    pub buffer_ms: f64,
    pub latency_ms: f64,
    /// Whether the session takes the microphone (play-and-record) or only
    /// plays (best for Bluetooth headphones).
    pub microphone: bool,
    /// Whether the output is forced to the device's own loudspeaker.
    pub speaker: bool,
}

/// A switch changed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Change {
    /// Take the microphone (from the next launch) or only play.
    Microphone(bool),
    /// Force the loudspeaker, or let the route decide.
    Speaker(bool),
}

/// The host's hold on the device's audio: reading it, and changing it.
#[derive(Clone, Copy, PartialEq)]
pub struct DeviceAudio {
    pub read: Callback<(), Report>,
    pub change: Callback<Change>,
}
