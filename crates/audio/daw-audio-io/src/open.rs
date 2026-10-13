//! Resolve [`AudioIoPrefs`] into opened cpal devices + stream configs.
//!
//! This is the single place the engine's "open the chosen device" logic lives,
//! so both the output mixer and the duplex monitor honor the same JACK
//! sample-rate/buffer rules and channel-count handling.

use cpal::traits::DeviceTrait;
use cpal::{BufferSize, SampleFormat, StreamConfig};

use crate::device::pick_device;
use crate::host::{host_is_graph, host_is_jack};
use crate::prefs::AudioIoPrefs;

/// An opened output device + the config to build its stream with.
pub struct OpenedOutput {
    pub device: cpal::Device,
    pub config: StreamConfig,
    pub sample_format: SampleFormat,
    pub channels: u16,
    pub sample_rate: u32,
    pub is_jack: bool,
}

/// Resolve the output device from `prefs`. Opens the device's **full** channel
/// count (so tracks can route to any output channel). Under JACK the server
/// dictates rate + buffer, so a requested rate/buffer is ignored there.
pub fn open_output(host: &cpal::Host, prefs: &AudioIoPrefs) -> Result<OpenedOutput, String> {
    let is_jack = host_is_jack(host);
    let device = pick_device(host, prefs.output_name(), false)?;
    let default = device
        .default_output_config()
        .map_err(|e| format!("output device default config: {e}"))?;
    let server_rate = default.sample_rate();
    let sample_rate = if is_jack {
        server_rate
    } else {
        prefs.sample_rate_opt().unwrap_or(server_rate)
    };
    let channels = default.channels().max(1);
    let buffer_size = if is_jack {
        BufferSize::Default
    } else {
        prefs
            .buffer_size_opt()
            .map(BufferSize::Fixed)
            .unwrap_or(BufferSize::Default)
    };
    Ok(OpenedOutput {
        sample_format: default.sample_format(),
        config: StreamConfig {
            channels,
            sample_rate,
            buffer_size,
        },
        device,
        channels,
        sample_rate,
        is_jack,
    })
}

/// An opened input device + the config to build its stream with.
pub struct OpenedInput {
    pub device: cpal::Device,
    pub config: StreamConfig,
    /// Number of channels the stream opens (enough to reach the chosen channel).
    pub channels: usize,
    pub is_jack: bool,
}

/// Resolve the input device from `prefs`, opening enough channels to reach
/// `max_channel` (the highest track input channel that needs to be tapped). The
/// stream runs at `sample_rate` (it must match the output stream's rate).
pub fn open_input(
    host: &cpal::Host,
    prefs: &AudioIoPrefs,
    sample_rate: u32,
    max_channel: usize,
) -> Result<OpenedInput, String> {
    let is_jack = host_is_jack(host);
    let device = pick_device(host, prefs.input_name(), true)?;
    // Under JACK/PipeWire the cpal device is the graph, not a microphone; the
    // duplex linker guards the capture node it links instead.
    if !host_is_graph(host) {
        crate::input_guard::check_input(
            &crate::device::device_name(&device),
            prefs.allow_builtin_mic,
        )?;
    }
    let channels = if host_is_graph(host) {
        // JACK / native PipeWire: open just enough channels to reach the one we
        // tap. The host targets the chosen device and the graph maps its
        // capture_1..N onto our in_0..N-1 in order, so the tapped channel index
        // lands on the matching hardware input — no manual patching needed.
        (max_channel + 1).max(1)
    } else {
        // ALSA/CoreAudio: pro interfaces (e.g. Yamaha TF) need the full channel
        // count opened so the channel index is reachable in one device stream.
        let default_ch = device
            .default_input_config()
            .map(|c| c.channels())
            .unwrap_or(0);
        let max_ch = device
            .supported_input_configs()
            .ok()
            .and_then(|cfgs| cfgs.map(|c| c.channels()).max())
            .unwrap_or(0);
        (default_ch.max(max_ch) as usize)
            .max(max_channel + 1)
            .max(2)
    };
    let buffer_size = if is_jack {
        BufferSize::Default
    } else {
        prefs
            .buffer_size_opt()
            .map(BufferSize::Fixed)
            .unwrap_or(BufferSize::Default)
    };
    Ok(OpenedInput {
        config: StreamConfig {
            channels: channels as u16,
            sample_rate,
            buffer_size,
        },
        device,
        channels,
        is_jack,
    })
}
