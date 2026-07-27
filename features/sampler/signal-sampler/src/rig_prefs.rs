//! Audio I/O preferences for the guitar rig — a data sub-struct of
//! [`RigManager`](crate::rig_manager::RigManager). Persistence lives on the
//! manager; this is just the device / channel / rate config.

use std::path::PathBuf;

use facet::Facet;

/// Audio I/O preferences for [`GuitarRig`](crate::rig::GuitarRig).
///
/// Empty-string / `0` mean "unset" (use the system default) rather than
/// `Option`, because the styx config is both written and re-read and the
/// serializer can't round-trip a serialized `None`.
#[derive(Clone, Debug, PartialEq, Facet)]
pub struct RigAudioPrefs {
    /// Input device substring (matched against device names). Empty = system
    /// default input.
    #[facet(default)]
    pub input_device: String,
    /// 0-based input channel to use as the mono guitar DI. The Yamaha TF1's
    /// 4th input is index `3`.
    #[facet(default)]
    pub input_channel: usize,
    /// Output device substring. Empty = system default output.
    #[facet(default)]
    pub output_device: String,
    /// Requested sample rate in Hz. `0` = device native; the rig prefers
    /// 48 kHz (NAM's training rate).
    #[facet(default)]
    pub sample_rate: u32,
    /// Requested buffer size in frames. `0` = backend default.
    #[facet(default)]
    pub buffer_size: u32,
    /// Interface output routing. Off = legacy stereo out on channels 1-2.
    /// On (the live-rig interface convention): main out lands on 3-4, the
    /// headphone bus on 1-2, and interface inputs 3-4 (an external monitor
    /// mix) blend into the phones. Channel fields are 0-based; zeros mean
    /// "use those conventions".
    #[facet(default)]
    pub phones_routing: bool,
    #[facet(default)]
    pub main_out_l: usize,
    #[facet(default)]
    pub main_out_r: usize,
    #[facet(default)]
    pub phones_out_l: usize,
    #[facet(default)]
    pub phones_out_r: usize,
    #[facet(default)]
    pub phones_mix_in_l: usize,
    #[facet(default)]
    pub phones_mix_in_r: usize,
}

impl Default for RigAudioPrefs {
    fn default() -> Self {
        Self {
            input_device: String::new(),
            input_channel: 0,
            output_device: String::new(),
            sample_rate: 48_000,
            buffer_size: 256,
            phones_routing: false,
            main_out_l: 0,
            main_out_r: 0,
            phones_out_l: 0,
            phones_out_r: 0,
            phones_mix_in_l: 0,
            phones_mix_in_r: 0,
        }
    }
}

impl RigAudioPrefs {
    /// Input device substring, or `None` if unset (use default).
    pub fn input_name(&self) -> Option<&str> {
        Some(self.input_device.as_str()).filter(|s| !s.is_empty())
    }
    /// Output device substring, or `None` if unset.
    pub fn output_name(&self) -> Option<&str> {
        Some(self.output_device.as_str()).filter(|s| !s.is_empty())
    }
    /// Requested sample rate, or `None` (device native) if `0`.
    pub fn sample_rate_opt(&self) -> Option<u32> {
        (self.sample_rate != 0).then_some(self.sample_rate)
    }
    /// Requested buffer size, or `None` (backend default) if `0`.
    pub fn buffer_size_opt(&self) -> Option<u32> {
        (self.buffer_size != 0).then_some(self.buffer_size)
    }
}

/// Convert the rig's device prefs into daw's engine-global `AudioIoPrefs`.
///
/// The rig is always duplex (live monitoring needs the input), so `want_input`
/// is forced on. The per-track input *channel* lives on [`RigAudioPrefs`] but
/// not on `AudioIoPrefs` (which is engine-global) — it's applied to the rig
/// track's `RecordInput` separately, in `GuitarRig::open`.
impl From<&RigAudioPrefs> for daw_audio_io::AudioIoPrefs {
    fn from(p: &RigAudioPrefs) -> Self {
        daw_audio_io::AudioIoPrefs {
            input_device: p.input_device.clone(),
            output_device: p.output_device.clone(),
            sample_rate: p.sample_rate,
            buffer_size: p.buffer_size,
            want_input: true,
            phones_routing: p.phones_routing,
            // Zeroed pairs fall back to the live-rig conventions:
            // main → 3-4, phones → 1-2, monitor mix in → 3-4.
            main_out_l: if p.phones_routing && p.main_out_l == 0 && p.main_out_r == 0 { 2 } else { p.main_out_l },
            main_out_r: if p.phones_routing && p.main_out_l == 0 && p.main_out_r == 0 { 3 } else { p.main_out_r },
            phones_out_l: p.phones_out_l,
            phones_out_r: if p.phones_routing && p.phones_out_l == 0 && p.phones_out_r == 0 { 1 } else { p.phones_out_r },
            phones_mix_in_l: if p.phones_routing && p.phones_mix_in_l == 0 && p.phones_mix_in_r == 0 { 2 } else { p.phones_mix_in_l },
            phones_mix_in_r: if p.phones_routing && p.phones_mix_in_l == 0 && p.phones_mix_in_r == 0 { 3 } else { p.phones_mix_in_r },
        }
    }
}

/// Signal's user config directory: `$XDG_CONFIG_HOME/signal`, falling back to
/// `$HOME/.config/signal`, then `./signal`.
pub fn signal_config_dir() -> PathBuf {
    let base = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
        .unwrap_or_else(|| PathBuf::from("."));
    base.join("signal")
}
