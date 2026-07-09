//! Audio-engine telemetry surfaced to control-side consumers.
//!
//! [`AudioStatsSnapshot`] is the public stats type returned by
//! [`SamplerRig::audio_stats`](crate::SamplerRig::audio_stats). It used to live
//! in the retired `SamplerPlayer` engine (`player.rs`); it now has a stable home
//! here so consumers keep importing `signal_sampler::AudioStatsSnapshot`.
//!
//! Fields the daw-backed renderer doesn't yet source (stream errors, callback
//! intervals, MIDI-to-callback latency) stay at their `Default` of `0` —
//! daw's [`AudioEngine`](daw::standalone::audio_engine::AudioEngine) owns those
//! counters and they aren't exposed yet.

/// A point-in-time snapshot of audio-engine + sample-cache telemetry.
#[derive(Clone, Debug, Default)]
pub struct AudioStatsSnapshot {
    pub stream_errors: u64,
    pub callback_overruns: u64,
    pub lock_misses: u64,
    pub callbacks: u64,
    pub max_render_us: u64,
    pub last_render_us: u64,
    pub buffer_budget_us: u64,
    pub midi_messages: u64,
    pub last_midi_to_callback_us: u64,
    pub max_midi_to_callback_us: u64,
    pub last_callback_interval_us: u64,
    pub max_callback_interval_us: u64,
    pub dropped_events: u64,
    pub pending_events: usize,
    pub stolen_voices: usize,
    pub cache_misses: usize,
    pub sample_misses: usize,
    pub loaded_sample_bytes: usize,
    pub cache_budget_bytes: Option<usize>,
    pub cache_over_budget_bytes: usize,
    pub recent_cache_misses: Vec<String>,
    pub recent_sample_misses: Vec<String>,
    pub resize_events: u64,
}
