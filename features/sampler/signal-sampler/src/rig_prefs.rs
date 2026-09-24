//! Audio I/O preferences for the guitar rig — a data sub-struct of
//! [`RigManager`](crate::rig_manager::RigManager). Persistence lives on the
//! manager; this is just the device / channel / rate config.

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
    /// Play the monitor mix from the separate headphone-mixer process
    /// (`signal-phones`) instead of this engine, so it keeps playing when
    /// the rig overruns, stops or crashes. Needs `phones_routing`.
    #[facet(default)]
    pub phones_mixer: bool,
    /// Let the rig open a built-in microphone as its input. Off by default —
    /// the laptop mic through the laptop speakers feeds back the moment the
    /// rig opens (see `daw_audio_io::input_guard`). `allow_builtin_mic true`
    /// in the rig's styx turns it on.
    #[facet(default)]
    pub allow_builtin_mic: bool,
    /// NAM level calibration: the interface's full-scale input level, dBu.
    /// `0` means the MiniFuse instrument input's +11.5 dBu (at minimum
    /// gain — turning the gain up lowers it by as much).
    #[facet(default)]
    pub input_calibration_dbu: f32,
    /// Turn NAM level calibration off (models fed and heard as-is).
    #[facet(default)]
    pub nam_calibration_off: bool,
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
            phones_mixer: false,
            allow_builtin_mic: false,
            input_calibration_dbu: 0.0,
            nam_calibration_off: false,
        }
    }
}

impl RigAudioPrefs {
    /// The MiniFuse instrument input's full scale at minimum gain.
    pub const DEFAULT_INPUT_CALIBRATION_DBU: f32 = 11.5;

    /// The interface calibration NAM blocks are levelled against, or `None`
    /// when calibration is off.
    #[must_use]
    pub fn nam_calibration(&self) -> Option<f32> {
        if self.nam_calibration_off {
            return None;
        }
        Some(if self.input_calibration_dbu == 0.0 {
            Self::DEFAULT_INPUT_CALIBRATION_DBU
        } else {
            self.input_calibration_dbu
        })
    }
}

impl RigAudioPrefs {
    /// The routing, zeros resolved to the live-rig conventions: `(main,
    /// phones, monitor-mix in)` pairs, 0-based — main on 3-4, phones on
    /// 1-2, the mix in on 3-4. Meaningful with `phones_routing` on.
    #[must_use]
    pub fn resolved_routing(&self) -> ((usize, usize), (usize, usize), (usize, usize)) {
        let pair = |l: usize, r: usize, d: (usize, usize)| if l == 0 && r == 0 { d } else { (l, r) };
        (
            pair(self.main_out_l, self.main_out_r, (2, 3)),
            pair(self.phones_out_l, self.phones_out_r, (0, 1)),
            pair(self.phones_mix_in_l, self.phones_mix_in_r, (2, 3)),
        )
    }

    /// Input device substring, or `None` if unset (use default).
    #[must_use]
    pub fn input_name(&self) -> Option<&str> {
        Some(self.input_device.as_str()).filter(|s| !s.is_empty())
    }
    /// Output device substring, or `None` if unset.
    #[must_use]
    pub fn output_name(&self) -> Option<&str> {
        Some(self.output_device.as_str()).filter(|s| !s.is_empty())
    }
    /// Requested sample rate, or `None` (device native) if `0`.
    #[must_use]
    pub fn sample_rate_opt(&self) -> Option<u32> {
        (self.sample_rate != 0).then_some(self.sample_rate)
    }
    /// Requested buffer size, or `None` (backend default) if `0`.
    #[must_use]
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
        Self {
            input_device: p.input_device.clone(),
            output_device: p.output_device.clone(),
            sample_rate: p.sample_rate,
            buffer_size: p.buffer_size,
            want_input: true,
            allow_builtin_mic: p.allow_builtin_mic,
            // The guitar rig keeps the engine's default node name; the caller
            // overrides it where a process runs more than one engine.
            node_name: String::new(),
            phones_routing: p.phones_routing,
            // Zeroed pairs fall back to the live-rig conventions:
            // main → 3-4, phones → 1-2, monitor mix in → 3-4.
            main_out_l: if p.phones_routing && p.main_out_l == 0 && p.main_out_r == 0 {
                2
            } else {
                p.main_out_l
            },
            main_out_r: if p.phones_routing && p.main_out_l == 0 && p.main_out_r == 0 {
                3
            } else {
                p.main_out_r
            },
            phones_out_l: p.phones_out_l,
            phones_out_r: if p.phones_routing && p.phones_out_l == 0 && p.phones_out_r == 0 {
                1
            } else {
                p.phones_out_r
            },
            phones_mix_in_l: if p.phones_routing && p.phones_mix_in_l == 0 && p.phones_mix_in_r == 0
            {
                2
            } else {
                p.phones_mix_in_l
            },
            phones_mix_in_r: if p.phones_routing && p.phones_mix_in_l == 0 && p.phones_mix_in_r == 0
            {
                3
            } else {
                p.phones_mix_in_r
            },
        }
    }
}

/// Signal's user config directory — now provided by the shared rig host;
/// kept re-exported here for the existing `rig_prefs::signal_config_dir` path.
pub use signal_rig_host::store::signal_config_dir;
