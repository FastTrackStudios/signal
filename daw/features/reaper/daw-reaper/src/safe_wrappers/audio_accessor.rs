//! Safe wrappers for REAPER AudioAccessor APIs.

use super::ReaperLow;
use reaper_medium::{MediaItemTake, MediaTrack};
use std::ffi::c_void;

/// Opaque audio accessor pointer from REAPER.
/// The actual type is `reaper_functions::AudioAccessor` (opaque struct),
/// but since the bindings module is private, we use `c_void` and cast.
pub type AudioAccessorPtr = *mut c_void;

/// Send-safe wrapper around an AudioAccessor raw pointer.
///
/// REAPER AudioAccessors are only accessed from the main thread via
/// `main_thread::query`, making cross-thread transfer of the pointer safe.
#[derive(Copy, Clone)]
pub struct SendableAccessorPtr(AudioAccessorPtr);

// SAFETY: The pointer is only dereferenced on the main thread (via main_thread::query).
// We store it across threads but never dereference it off the main thread.
unsafe impl Send for SendableAccessorPtr {}
unsafe impl Sync for SendableAccessorPtr {}

impl SendableAccessorPtr {
    pub fn new(ptr: AudioAccessorPtr) -> Self {
        Self(ptr)
    }

    pub fn get(self) -> AudioAccessorPtr {
        self.0
    }

    pub fn is_null(self) -> bool {
        self.0.is_null()
    }
}

/// Create an audio accessor for a track.
pub fn create_track_audio_accessor(low: &ReaperLow, track: MediaTrack) -> AudioAccessorPtr {
    unsafe { low.CreateTrackAudioAccessor(track.as_ptr()) as AudioAccessorPtr }
}

/// Create an audio accessor for a take.
pub fn create_take_audio_accessor(low: &ReaperLow, take: MediaItemTake) -> AudioAccessorPtr {
    unsafe { low.CreateTakeAudioAccessor(take.as_ptr()) as AudioAccessorPtr }
}

/// Check if an accessor's state has changed (source audio was modified).
pub fn audio_accessor_state_changed(low: &ReaperLow, accessor: AudioAccessorPtr) -> bool {
    if accessor.is_null() {
        return false;
    }
    unsafe { low.AudioAccessorStateChanged(accessor.cast()) }
}

/// Read interleaved sample data from an accessor.
///
/// Returns the number of samples actually read per channel (0 on error).
/// `buf` must be pre-allocated with at least `num_channels * num_samples_per_channel` elements.
pub fn get_audio_accessor_samples(
    low: &ReaperLow,
    accessor: AudioAccessorPtr,
    sample_rate: i32,
    num_channels: i32,
    start_time: f64,
    num_samples_per_channel: i32,
    buf: &mut [f64],
) -> i32 {
    if accessor.is_null() {
        return 0;
    }
    unsafe {
        low.GetAudioAccessorSamples(
            accessor.cast(),
            sample_rate,
            num_channels,
            start_time,
            num_samples_per_channel,
            buf.as_mut_ptr(),
        )
    }
}

/// Destroy an audio accessor and free its resources.
pub fn destroy_audio_accessor(low: &ReaperLow, accessor: AudioAccessorPtr) {
    if !accessor.is_null() {
        unsafe {
            low.DestroyAudioAccessor(accessor.cast());
        }
    }
}
