//! Audio device enumeration + resolution.

use cpal::traits::{DeviceTrait, HostTrait};
use facet::Facet;

/// An enumerated audio device — name, channel count, native sample rate, and
/// whether it is an input or output device. One `Vec<DeviceInfo>` can carry both
/// kinds for a combined picker.
#[derive(Clone, Debug, Facet)]
pub struct DeviceInfo {
    pub name: String,
    pub channels: u16,
    pub default_sample_rate: u32,
    pub is_input: bool,
}

/// Human-readable device name (empty string if the driver won't report one).
pub fn device_name(device: &cpal::Device) -> String {
    device
        .description()
        .map(|d| d.name().to_string())
        .unwrap_or_default()
}

/// Whether the app may read inputs at all. Only iOS asks the player
/// (microphone access); everywhere else it is always granted.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum InputAccess {
    Granted,
    /// Turned off in Settings: every input is silent.
    Denied,
    /// Never asked yet.
    Undetermined,
}

/// The rate the device will run at, where the platform decides it (iOS: the
/// audio session, asked for `requested` first — an interface that only runs
/// at 44.1 kHz stays there). `None` where the caller picks the rate itself.
/// An engine builds its renderer at this, so it never meets a device
/// running at another.
#[must_use]
pub fn negotiate_sample_rate(requested: Option<u32>) -> Option<u32> {
    #[cfg(target_os = "ios")]
    {
        Some(crate::ios_session::negotiate_rate(requested)).filter(|r| *r > 0)
    }
    #[cfg(not(target_os = "ios"))]
    {
        let _ = requested;
        None
    }
}

/// What the audio session says about input, on one line (iOS: its route,
/// ports, channel counts, rate and access — see `ios_session`). `None`
/// where there is no session to ask.
#[must_use]
pub fn session_report() -> Option<String> {
    #[cfg(target_os = "ios")]
    {
        Some(crate::ios_session::report())
    }
    #[cfg(not(target_os = "ios"))]
    {
        None
    }
}

/// Whether the app may read inputs (iOS: the microphone access the player
/// gave, see `ios_session`).
#[must_use]
pub fn input_access() -> InputAccess {
    #[cfg(target_os = "ios")]
    {
        crate::ios_session::input_access()
    }
    #[cfg(not(target_os = "ios"))]
    {
        InputAccess::Granted
    }
}

/// List the host's input devices (name + channel count + native rate). Does not
/// open any stream. On iOS, the audio session's input ports (see
/// `ios_session`).
pub fn input_devices(host: &cpal::Host) -> Vec<DeviceInfo> {
    #[cfg(target_os = "ios")]
    {
        let _ = host;
        let rate = crate::ios_session::sample_rate();
        crate::ios_session::inputs()
            .into_iter()
            .map(|(name, channels)| DeviceInfo { name, channels, default_sample_rate: rate, is_input: true })
            .collect()
    }
    #[cfg(not(target_os = "ios"))]
    {
        let mut out = Vec::new();
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                let (channels, sr) = d
                    .default_input_config()
                    .map(|c| (c.channels(), c.sample_rate()))
                    .unwrap_or((0, 0));
                out.push(DeviceInfo {
                    name: device_name(&d),
                    channels,
                    default_sample_rate: sr,
                    is_input: true,
                });
            }
        }
        out
    }
}

/// List the host's output devices. On iOS, the ports the session's route
/// plays through (an app cannot choose its output).
pub fn output_devices(host: &cpal::Host) -> Vec<DeviceInfo> {
    #[cfg(target_os = "ios")]
    {
        let _ = host;
        let rate = crate::ios_session::sample_rate();
        crate::ios_session::outputs()
            .into_iter()
            .map(|(name, channels)| DeviceInfo { name, channels, default_sample_rate: rate, is_input: false })
            .collect()
    }
    #[cfg(not(target_os = "ios"))]
    {
        let mut out = Vec::new();
        if let Ok(devs) = host.output_devices() {
            for d in devs {
                let (channels, sr) = d
                    .default_output_config()
                    .map(|c| (c.channels(), c.sample_rate()))
                    .unwrap_or((0, 0));
                out.push(DeviceInfo {
                    name: device_name(&d),
                    channels,
                    default_sample_rate: sr,
                    is_input: false,
                });
            }
        }
        out
    }
}

/// Synthesized input-channel names for the input device matching `name_substr`
/// (or the default input device when `None`). cpal exposes no per-channel names,
/// so this returns `(index, "Input N")` up to the device's channel count — the
/// list a track's input picker chooses from.
pub fn input_channels(host: &cpal::Host, name_substr: Option<&str>) -> Vec<(u32, String)> {
    let dev = match pick_device(host, name_substr, true) {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };
    let channels = dev
        .default_input_config()
        .map(|c| c.channels())
        .or_else(|_| {
            dev.supported_input_configs()
                .map(|cfgs| cfgs.map(|c| c.channels()).max().unwrap_or(0))
        })
        .unwrap_or(0);
    (0..channels as u32)
        .map(|i| (i, format!("Input {}", i + 1)))
        .collect()
}

/// Rank a device for the requested direction so [`pick_device`] can choose
/// between same-named nodes. A device that *only* faces the requested direction
/// (a pure capture source for input, a pure sink for output) outranks a Duplex
/// node by a wide margin; within a rank, more channels wins. The channel count
/// comes from the matching default config (0 when the driver won't report one).
fn device_score(device: &cpal::Device, input: bool) -> u32 {
    use cpal::traits::DeviceTrait;
    let dir_bonus = device
        .description()
        .ok()
        .map(|d| match d.direction() {
            cpal::DeviceDirection::Input if input => 1000,
            cpal::DeviceDirection::Output if !input => 1000,
            cpal::DeviceDirection::Duplex => 100,
            _ => 0,
        })
        .unwrap_or(0);
    let channels = if input {
        device
            .default_input_config()
            .map(|c| c.channels())
            .unwrap_or(0)
    } else {
        device
            .default_output_config()
            .map(|c| c.channels())
            .unwrap_or(0)
    };
    dir_bonus + channels as u32
}

/// Pick an input/output device by name substring, falling back to the host
/// default when the name is empty or not found (e.g. under JACK, where the
/// device is the JACK server, not "Yamaha TF").
pub fn pick_device(
    host: &cpal::Host,
    name: Option<&str>,
    input: bool,
) -> Result<cpal::Device, String> {
    // iOS has one device each way, the session's route: a named input is a
    // port, preferred on the session, and the default device then plays
    // through it.
    #[cfg(target_os = "ios")]
    if let Some(n) = name.filter(|s| !s.is_empty()) {
        if input && !crate::ios_session::prefer_input(n) {
            tracing::warn!(device = n, "daw-audio-io: no such input port; using the route's");
        }
        let def = if input { host.default_input_device() } else { host.default_output_device() };
        return def.ok_or_else(|| format!("no default audio {} device", if input { "input" } else { "output" }));
    }
    if let Some(n) = name.filter(|s| !s.is_empty()) {
        // A pro interface can surface under one name as several nodes — e.g. a
        // PipeWire `Yamaha TF` appears as both a Duplex *sink* (whose monitor is
        // playback, not the mic inputs) and an Input *source* (the real capture
        // channels). Score every name match and take the best for the requested
        // direction, so an input pick lands on the capture source — not the sink
        // monitor — and grabs the variant exposing the most channels.
        let devs = if input {
            host.input_devices()
        } else {
            host.output_devices()
        };
        let best = devs.ok().and_then(|it| {
            it.filter(|d| device_name(d).contains(n))
                .max_by_key(|d| device_score(d, input))
        });
        if let Some(d) = best {
            return Ok(d);
        }
        tracing::warn!(
            device = n,
            "daw-audio-io: named audio device not found; using default"
        );
    }
    let def = if input {
        host.default_input_device()
    } else {
        host.default_output_device()
    };
    def.ok_or_else(|| {
        format!(
            "no default audio {} device",
            if input { "input" } else { "output" }
        )
    })
}

/// What a device can be asked for: the sample rates and buffer sizes it
/// accepts. Used to validate a requested [`AudioIoPrefs`](crate::AudioIoPrefs)
/// rate/buffer before opening, and to populate a settings picker.
#[derive(Clone, Debug, Default, PartialEq, Facet)]
pub struct DeviceCaps {
    pub name: String,
    /// The device's current (native) rate.
    pub default_sample_rate: u32,
    /// Common rates the device supports, ascending.
    pub sample_rates: Vec<u32>,
    /// Smallest and largest buffer size (frames) the device accepts, when
    /// the driver reports a range. `None` = unknown (let the host decide).
    pub buffer_range: Option<(u32, u32)>,
}

impl DeviceCaps {
    /// Whether `frames` is inside the reported buffer range (true when the
    /// range is unknown — the open itself will then be the judge).
    pub fn accepts_buffer(&self, frames: u32) -> bool {
        self.buffer_range
            .is_none_or(|(min, max)| (min..=max).contains(&frames))
    }

    /// Whether `rate` is one of the supported rates.
    pub fn accepts_rate(&self, rate: u32) -> bool {
        self.sample_rates.contains(&rate)
    }
}

/// The rates a settings picker offers; a device's supported ranges are
/// intersected with these (a range is continuous, a picker is not).
const COMMON_RATES: &[u32] = &[
    22_050, 32_000, 44_100, 48_000, 88_200, 96_000, 176_400, 192_000,
];

/// Capabilities of the input or output device matching `name` (or the
/// default device when `None`).
pub fn device_caps(
    host: &cpal::Host,
    name: Option<&str>,
    input: bool,
) -> Result<DeviceCaps, String> {
    let device = pick_device(host, name, input)?;
    let (default, ranges) = if input {
        (
            device.default_input_config(),
            device
                .supported_input_configs()
                .map(|r| r.collect::<Vec<_>>()),
        )
    } else {
        (
            device.default_output_config(),
            device
                .supported_output_configs()
                .map(|r| r.collect::<Vec<_>>()),
        )
    };
    let default = default.map_err(|e| format!("device default config: {e}"))?;
    let ranges = ranges.map_err(|e| format!("device supported configs: {e}"))?;

    let mut sample_rates: Vec<u32> = COMMON_RATES
        .iter()
        .copied()
        .filter(|&r| {
            ranges
                .iter()
                .any(|c| (c.min_sample_rate()..=c.max_sample_rate()).contains(&r))
        })
        .collect();
    if !sample_rates.contains(&default.sample_rate()) {
        sample_rates.push(default.sample_rate());
        sample_rates.sort_unstable();
    }
    let buffer_range = match default.buffer_size() {
        cpal::SupportedBufferSize::Range { min, max } => Some((*min, *max)),
        cpal::SupportedBufferSize::Unknown => None,
    };
    Ok(DeviceCaps {
        name: device_name(&device),
        default_sample_rate: default.sample_rate(),
        sample_rates,
        buffer_range,
    })
}
