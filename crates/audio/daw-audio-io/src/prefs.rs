//! Audio I/O preferences — two levels, matching the DAW routing model.
//!
//! - [`AudioIoPrefs`] is **engine-global**: which input device, which output
//!   device, the sample rate, and the buffer size. No channel lives here.
//! - [`TrackIo`] is **per-track**: which input channel a track records from and
//!   which output-device channel pair it sums into.

use facet::Facet;

/// Engine-global audio device selection. Persisted (styx/toml) via Facet.
///
/// Empty-string / `0` mean "unset" (use the system default / device-native)
/// rather than `Option`, so the config round-trips through a serializer that
/// can't represent a serialized `None`. The all-default value reproduces the
/// classic "default output device, native rate, backend-default buffer,
/// output-only" behavior.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct AudioIoPrefs {
    /// Input device name substring. Empty = system default input.
    #[facet(default)]
    pub input_device: String,
    /// Output device name substring. Empty = system default output.
    #[facet(default)]
    pub output_device: String,
    /// Requested sample rate (Hz). `0` = device native.
    #[facet(default)]
    pub sample_rate: u32,
    /// Requested buffer size (frames). `0` = backend default.
    #[facet(default)]
    pub buffer_size: u32,
    /// Whether to open an input stream at all. `false` = output-only (plain
    /// DAW playback); `true` = duplex (live monitoring needs the input).
    #[facet(default)]
    pub want_input: bool,
    /// Graph node name for the native (PipeWire) backend. Empty = the engine's
    /// default.
    ///
    /// This has to be settable because a process can run more than one engine
    /// — the guitar rig and the keys rig both open one — and the linker finds
    /// its ports BY NAME. Two engines sharing a name means the second node
    /// never gets linked to the device and plays to nothing: audible as "the
    /// UI is fine, the meters are dead".
    #[facet(default)]
    pub node_name: String,
    /// Output routing (0-based channel indices). `main_out` is where the
    /// stereo master lands (default 0/1). When `phones_out` differs from
    /// `main_out`, a second stereo bus is written there: master (scaled by
    /// a live self-mix level) plus, when `phones_mix_in` is set, an external
    /// monitor-mix input pair blended in. All default 0/0 = legacy 2-out.
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
    /// Enables the phones bus + mix-in routing above (so 0 stays a valid
    /// channel index without being ambiguous with "unset").
    #[facet(default)]
    pub phones_routing: bool,
    /// Let a built-in microphone be opened as the live input. Off by default:
    /// with nothing plugged in the default input is the laptop's own mic, and
    /// monitoring it through the laptop's own speakers feeds back instantly.
    /// See [`input_guard`](crate::input_guard).
    #[facet(default)]
    pub allow_builtin_mic: bool,
}

impl AudioIoPrefs {
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

/// Per-track audio routing: which input channel a track records from and which
/// output-device channel pair it sums into. A plain runtime value — persistence
/// of track routing lives with the track model, not this crate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrackIo {
    /// 0-based input channel on the engine's input device (the track's DI).
    pub input_channel: usize,
    /// Left output channel on the engine's output device.
    pub output_left: usize,
    /// Right output channel on the engine's output device.
    pub output_right: usize,
}

impl Default for TrackIo {
    /// Input channel 0, output to the main L/R pair (0, 1).
    fn default() -> Self {
        Self {
            input_channel: 0,
            output_left: 0,
            output_right: 1,
        }
    }
}

impl TrackIo {
    /// A track that records from `channel` and outputs to the main L/R pair.
    pub fn mono_in(channel: usize) -> Self {
        Self {
            input_channel: channel,
            ..Self::default()
        }
    }

    /// The output channel pair `(left, right)`.
    pub fn output_channels(&self) -> (usize, usize) {
        (self.output_left, self.output_right)
    }
}
