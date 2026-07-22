//! nice_plug parameter definitions and shared UI state.
//!
//! Lives in `trigger-ui` (not `trigger-plugin`) so the
//! [`crate::control_view`] component can render against the param tree
//! without forcing a circular dep — same split as `comp-ui::params`. The
//! param ids and ranges are the plugin's originals (moved here verbatim),
//! so existing host sessions keep loading.

use atomic_float::AtomicF32;
use audiocore_core::prelude::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};

/// Samples kept in the waveform history ring (one per processed block —
/// ~2.7 s at 512-sample blocks / 48 kHz).
pub const WAVE_HISTORY_LEN: usize = 256;

/// Slots in the hit ring — plenty for every hit still visible in the
/// [`WAVE_HISTORY_LEN`]-block window (the 5 ms minimum retrigger guard
/// bounds hits to well under one per block on average).
pub const HIT_RING_LEN: usize = 64;

/// Lock-free single-writer history ring for the display's rolling peaks.
///
/// The audio thread [`push`](WaveRing::push)es one value per block (no
/// allocation, relaxed atomics); the UI thread
/// [`snapshot`](WaveRing::snapshot)s the whole window oldest → newest. A
/// torn read across the head is at worst one stale sample — invisible in a
/// scrolling display — so no synchronization beyond the atomics is needed.
pub struct WaveRing {
    buf: [AtomicF32; WAVE_HISTORY_LEN],
    /// Next write slot (monotonically increasing, wrapped on use).
    head: AtomicUsize,
}

impl Default for WaveRing {
    fn default() -> Self {
        Self::new()
    }
}

impl WaveRing {
    pub fn new() -> Self {
        Self {
            buf: std::array::from_fn(|_| AtomicF32::new(0.0)),
            head: AtomicUsize::new(0),
        }
    }

    /// Audio thread: append one value. Lock-free, allocation-free.
    pub fn push(&self, v: f32) {
        let i = self.head.load(Ordering::Relaxed);
        self.buf[i % WAVE_HISTORY_LEN].store(v, Ordering::Relaxed);
        self.head.store(i.wrapping_add(1), Ordering::Relaxed);
    }

    /// Monotonic count of pushed blocks — the current block's index while a
    /// block is being processed (the push happens at block end). Hit markers
    /// are timestamped with this so they stay glued to their column as the
    /// window scrolls.
    pub fn head(&self) -> u64 {
        self.head.load(Ordering::Relaxed) as u64
    }

    /// UI thread: copy the window out, oldest → newest.
    pub fn snapshot(&self) -> Vec<f32> {
        let head = self.head.load(Ordering::Relaxed);
        (0..WAVE_HISTORY_LEN)
            .map(|k| self.buf[(head.wrapping_add(k)) % WAVE_HISTORY_LEN].load(Ordering::Relaxed))
            .collect()
    }
}

/// Lock-free single-writer ring of detected hits, so trigger markers
/// survive the scroll instead of flashing for one frame.
///
/// Each slot packs one hit into an `AtomicU64`:
/// `(block_index << 8) | (midi_velocity + 1)` — the `+ 1` keeps a stored
/// hit nonzero so zero unambiguously means "empty slot". `block_index` is
/// [`WaveRing::head`] at the time of the hit; the UI matches it against the
/// current head to place the marker in the scrolling window
/// ([`crate::trigger_waveform_svg::marker_columns`]).
pub struct HitRing {
    buf: [AtomicU64; HIT_RING_LEN],
    head: AtomicUsize,
}

impl Default for HitRing {
    fn default() -> Self {
        Self::new()
    }
}

impl HitRing {
    pub fn new() -> Self {
        Self {
            buf: std::array::from_fn(|_| AtomicU64::new(0)),
            head: AtomicUsize::new(0),
        }
    }

    /// Audio thread: record one hit. Lock-free, allocation-free.
    pub fn push(&self, block_index: u64, velocity: f32) {
        let vel7 = (velocity.clamp(0.0, 1.0) * 127.0).round() as u64;
        let packed = (block_index << 8) | (vel7 + 1);
        let i = self.head.load(Ordering::Relaxed);
        self.buf[i % HIT_RING_LEN].store(packed, Ordering::Relaxed);
        self.head.store(i.wrapping_add(1), Ordering::Relaxed);
    }

    /// UI thread: every stored hit as `(block_index, velocity 0..1)`.
    /// Order is not meaningful (the window placement comes from the block
    /// index); empty slots are skipped.
    pub fn snapshot(&self) -> Vec<(u64, f32)> {
        self.buf
            .iter()
            .filter_map(|slot| {
                let packed = slot.load(Ordering::Relaxed);
                if packed == 0 {
                    return None;
                }
                let vel = ((packed & 0xFF) - 1) as f32 / 127.0;
                Some((packed >> 8, vel))
            })
            .collect()
    }
}

/// Audio-thread → UI display data.
pub struct TriggerUiState {
    pub params: Arc<TriggerParams>,
    pub sample_rate: AtomicF32,
    /// Per-block input peaks (linear 0..1, mono sum) for the scrolling bars.
    pub input_wave: WaveRing,
    /// Detected hits — `(block_index, velocity)` markers over the window.
    pub hits: HitRing,
}

impl TriggerUiState {
    pub fn new(params: Arc<TriggerParams>) -> Self {
        Self {
            params,
            sample_rate: AtomicF32::new(48_000.0),
            input_wave: WaveRing::new(),
            hits: HitRing::new(),
        }
    }
}

#[derive(Params)]
pub struct TriggerParams {
    /// Absolute onset threshold.
    #[id = "threshold"]
    pub threshold_db: FloatParam,
    /// Detection confirmation window (the legacy engine's "sensitivity"):
    /// the level must hold above the threshold this long before the trigger
    /// fires. 0 = fire immediately; longer = fewer false triggers from spikes.
    #[id = "sensitivity"]
    pub sensitivity_ms: FloatParam,
    /// Retrigger-guard window.
    #[id = "retrigger"]
    pub retrigger_ms: FloatParam,
    /// MIDI note to emit (default 36 = C1 kick).
    #[id = "note"]
    pub note: IntParam,
    /// Velocity floor (output clamp, 0-1).
    #[id = "vel_min"]
    pub vel_min: FloatParam,
    /// Velocity ceiling (output clamp, 0-1).
    #[id = "vel_max"]
    pub vel_max: FloatParam,
    /// Mute passthrough and click on every hit (threshold tuning).
    #[id = "listen"]
    pub listen: BoolParam,
    /// Sidechain HPF frequency; 0 = off. Isolates the drum from low bleed.
    #[id = "sc_hpf"]
    pub sc_hpf_hz: FloatParam,
    /// Sidechain LPF frequency; 0 = off. Rejects cymbal/hat bleed.
    #[id = "sc_lpf"]
    pub sc_lpf_hz: FloatParam,
    /// Detection algorithm: peak envelope (zero latency) or an FFT onset
    /// detection function.
    #[id = "algorithm"]
    pub algorithm: IntParam,
    /// Velocity curve.
    #[id = "vel_curve"]
    pub vel_curve: IntParam,
    /// Velocity dynamics: 0 = fixed velocity, 1 = full dynamic range.
    #[id = "dynamics"]
    pub dynamics: FloatParam,
}

impl Default for TriggerParams {
    fn default() -> Self {
        Self {
            threshold_db: FloatParam::new(
                "Threshold",
                -30.0,
                FloatRange::Linear { min: -60.0, max: 0.0 },
            )
            .with_unit(" dB")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            sensitivity_ms: FloatParam::new(
                "Sensitivity",
                1.0,
                FloatRange::Linear { min: 0.0, max: 10.0 },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(1)),
            retrigger_ms: FloatParam::new(
                "Retrigger",
                40.0,
                FloatRange::Skewed {
                    min: 5.0,
                    max: 200.0,
                    factor: FloatRange::skew_factor(-1.0),
                },
            )
            .with_unit(" ms")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            note: IntParam::new("Note", 36, IntRange::Linear { min: 0, max: 127 })
                .with_value_to_string(formatters::v2s_i32_note_formatter()),
            vel_min: FloatParam::new("Vel Min", 0.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            vel_max: FloatParam::new("Vel Max", 1.0, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
            listen: BoolParam::new("Listen", false),
            sc_hpf_hz: FloatParam::new(
                "SC HPF",
                0.0,
                FloatRange::Linear { min: 0.0, max: 1_000.0 },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            sc_lpf_hz: FloatParam::new(
                "SC LPF",
                0.0,
                FloatRange::Linear { min: 0.0, max: 20_000.0 },
            )
            .with_unit(" Hz")
            .with_value_to_string(formatters::v2s_f32_rounded(0)),
            algorithm: IntParam::new("Algorithm", 0, IntRange::Linear { min: 0, max: 6 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "Spectral Flux",
                        2 => "SuperFlux",
                        3 => "HFC",
                        4 => "Complex",
                        5 => "Rect Complex",
                        6 => "Mod KL",
                        _ => "Peak Env",
                    }
                    .to_string()
                })),
            vel_curve: IntParam::new("Curve", 0, IntRange::Linear { min: 0, max: 3 })
                .with_value_to_string(Arc::new(|v| {
                    match v {
                        1 => "Log",
                        2 => "Exp",
                        3 => "Fixed",
                        _ => "Linear",
                    }
                    .to_string()
                })),
            dynamics: FloatParam::new("Dynamics", 0.5, FloatRange::Linear { min: 0.0, max: 1.0 })
                .with_value_to_string(formatters::v2s_f32_rounded(2)),
        }
    }
}
