//! Safe wrappers for REAPER peak metering and waveform APIs.

use super::ReaperLow;
use reaper_medium::MediaTrack;

/// Get the current peak level for a track channel (linear scale, typically 0.0–1.0+).
pub fn track_get_peak_info(low: &ReaperLow, track: MediaTrack, channel: i32) -> f64 {
    unsafe { low.Track_GetPeakInfo(track.as_ptr(), channel) }
}

/// Get the peak hold level in dB for a track channel.
/// Pass `clear = false` to read without resetting the hold.
pub fn track_get_peak_hold_db(
    low: &ReaperLow,
    track: MediaTrack,
    channel: i32,
    clear: bool,
) -> f64 {
    unsafe { low.Track_GetPeakHoldDB(track.as_ptr(), channel, clear) }
}

/// One `PCM_Source_GetPeaks` request. `peak_rate` is peaks per second
/// of SOURCE time and `start_time` is seconds into the source; the
/// caller maps take/item time onto these.
#[derive(Clone, Copy, Debug)]
pub struct PeakRequest {
    pub source: reaper_medium::PcmSource,
    pub peak_rate: f64,
    pub start_time: f64,
    pub num_channels: i32,
    pub num_samples_per_channel: i32,
}

/// Read peak data from a PCM source (for waveform display).
///
/// The SDK writes the peaks interleaved by channel in TWO blocks —
/// every maximum, then every minimum (`reaper_plugin_functions.h`,
/// `PCM_Source_GetPeaks`) — so `buf` must hold at least
/// `2 × num_channels × num_samples_per_channel` doubles. Returns the
/// raw SDK word: 20 bits of returned count, then 4 bits of output
/// mode, then the extra-type flag. No extra type is requested.
pub fn pcm_source_get_peaks(low: &ReaperLow, req: PeakRequest, buf: &mut [f64]) -> i32 {
    let needed = 2 * req.num_channels.max(0) as usize * req.num_samples_per_channel.max(0) as usize;
    if buf.len() < needed || req.num_channels <= 0 || req.num_samples_per_channel <= 0 {
        return 0;
    }
    unsafe {
        low.PCM_Source_GetPeaks(
            req.source.as_ptr(),
            req.peak_rate,
            req.start_time,
            req.num_channels,
            req.num_samples_per_channel,
            0,
            buf.as_mut_ptr(),
        )
    }
}

/// Make sure the source's peak cache exists before reading it, running
/// the build synchronously on the calling (main) thread. `mode` 0
/// (begin) returning zero means nothing to do; otherwise `1` (run)
/// returns the percent remaining until zero, then `2` (finish) closes
/// the build. Returns `false` if the build was given up on.
pub fn pcm_source_ensure_peaks(low: &ReaperLow, source: reaper_medium::PcmSource) -> bool {
    // A fresh multi-hour recording builds in a few thousand steps; the
    // bound only stops a source that never reports done.
    const MAX_STEPS: u32 = 100_000;
    let ptr = source.as_ptr();
    if unsafe { low.PCM_Source_BuildPeaks(ptr, 0) } == 0 {
        return true;
    }
    let mut steps = 0;
    while unsafe { low.PCM_Source_BuildPeaks(ptr, 1) } != 0 {
        steps += 1;
        if steps >= MAX_STEPS {
            unsafe { low.PCM_Source_BuildPeaks(ptr, 2) };
            return false;
        }
    }
    unsafe { low.PCM_Source_BuildPeaks(ptr, 2) };
    true
}
