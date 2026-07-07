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
}

impl Default for RigAudioPrefs {
    fn default() -> Self {
        Self {
            input_device: String::new(),
            input_channel: 0,
            output_device: String::new(),
            sample_rate: 48_000,
            buffer_size: 256,
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

/// Convert the rig's device prefs into daw's engine-global [`AudioIoPrefs`].
///
/// The rig is always duplex (live monitoring needs the input), so `want_input`
/// is forced on. The per-track input *channel* lives on [`RigAudioPrefs`] but
/// not on [`AudioIoPrefs`] (which is engine-global) — it's applied to the rig
/// track's `RecordInput` separately, in `GuitarRig::open`.
impl From<&RigAudioPrefs> for daw_audio_io::AudioIoPrefs {
    fn from(p: &RigAudioPrefs) -> Self {
        daw_audio_io::AudioIoPrefs {
            input_device: p.input_device.clone(),
            output_device: p.output_device.clone(),
            sample_rate: p.sample_rate,
            buffer_size: p.buffer_size,
            want_input: true,
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
