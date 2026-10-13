//! Rubber Band Library integration via raw FFI.
//!
//! Requires the `rubberband` system library to be installed.
//! On NixOS: available via `pkgs.rubberband`.
//!
//! This module provides a sample-by-sample interface (`tick`) on top of
//! Rubber Band's block-based real-time API, buffering input into blocks
//! and retrieving output into a ring buffer.

use std::collections::VecDeque;

// ---------------------------------------------------------------------------
// FFI declarations
// ---------------------------------------------------------------------------

#[allow(non_camel_case_types, dead_code)]
mod ffi {
    use std::os::raw::{c_double, c_float, c_int, c_uint};

    pub type RubberBandOptions = c_int;
    pub type RubberBandState = *mut std::ffi::c_void;

    // Option flags (from rubberband-c.h).
    pub const OPTION_PROCESS_REALTIME: RubberBandOptions = 0x00000001;
    pub const OPTION_ENGINE_FINER: RubberBandOptions = 0x20000000;
    pub const OPTION_PITCH_HIGH_QUALITY: RubberBandOptions = 0x02000000;
    pub const OPTION_FORMANT_PRESERVED: RubberBandOptions = 0x01000000;
    pub const OPTION_THREADING_NEVER: RubberBandOptions = 0x00010000;

    #[link(name = "rubberband")]
    extern "C" {
        pub fn rubberband_new(
            sample_rate: c_uint,
            channels: c_uint,
            options: RubberBandOptions,
            initial_time_ratio: c_double,
            initial_pitch_scale: c_double,
        ) -> RubberBandState;
        pub fn rubberband_delete(state: RubberBandState);
        pub fn rubberband_reset(state: RubberBandState);
        pub fn rubberband_set_pitch_scale(state: RubberBandState, scale: c_double);
        pub fn rubberband_get_pitch_scale(state: RubberBandState) -> c_double;
        pub fn rubberband_set_formant_scale(state: RubberBandState, scale: c_double);
        pub fn rubberband_set_formant_option(state: RubberBandState, options: RubberBandOptions);
        pub fn rubberband_set_max_process_size(state: RubberBandState, samples: c_uint);
        pub fn rubberband_get_samples_required(state: RubberBandState) -> c_uint;
        pub fn rubberband_get_latency(state: RubberBandState) -> c_uint;
        pub fn rubberband_process(
            state: RubberBandState,
            input: *const *const c_float,
            samples: c_uint,
            final_: c_int,
        );
        pub fn rubberband_available(state: RubberBandState) -> c_int;
        pub fn rubberband_retrieve(
            state: RubberBandState,
            output: *const *mut c_float,
            samples: c_uint,
        ) -> c_uint;
    }
}

// ---------------------------------------------------------------------------
// Public interface
// ---------------------------------------------------------------------------

/// Block size for standard quality mode.
const BLOCK_SIZE: usize = 512;
/// Block size for live (low-latency) mode.
const BLOCK_SIZE_LIVE: usize = 128;

/// Rubber Band-based pitch shifter with sample-by-sample interface.
///
/// Internally buffers input into blocks, feeds them to the Rubber Band
/// real-time engine, and serves output from a ring buffer.
pub struct RubberbandShifter {
    /// Pitch ratio: 0.5 = octave down, 2.0 = octave up.
    pub speed: f64,
    /// Mix: 0.0 = dry only, 1.0 = wet only.
    pub mix: f64,
    /// Enable formant preservation.
    pub preserve_formants: bool,
    /// Live mode: use R2 engine and smaller blocks for lower latency.
    pub live: bool,

    /// Raw FFI handle (null when uninitialised).
    state: ffi::RubberBandState,
    /// Input accumulation buffer (f32 for rubberband).
    in_buf: Vec<f32>,
    /// Number of samples currently in `in_buf`.
    in_count: usize,
    /// Output ring buffer.
    out_buf: VecDeque<f32>,
    /// Retrieve scratch buffer.
    retrieve_buf: Vec<f32>,

    /// Currently applied pitch scale (to detect changes).
    current_pitch_scale: f64,
    /// Currently applied formant preservation flag.
    current_formants: bool,

    sample_rate: f64,
    /// Latency reported by rubberband at creation time.
    reported_latency: usize,
    /// Current block size (depends on live mode).
    block_size: usize,
    /// Track previous live state to detect changes.
    prev_live: bool,
}

impl RubberbandShifter {
    pub fn new() -> Self {
        Self {
            speed: 0.5,
            mix: 1.0,
            preserve_formants: false,
            live: false,
            state: std::ptr::null_mut(),
            in_buf: vec![0.0f32; BLOCK_SIZE],
            in_count: 0,
            out_buf: VecDeque::with_capacity(BLOCK_SIZE * 4),
            retrieve_buf: vec![0.0f32; BLOCK_SIZE * 4],
            current_pitch_scale: 0.5,
            current_formants: false,
            sample_rate: 48000.0,
            reported_latency: 0,
            block_size: BLOCK_SIZE,
            prev_live: false,
        }
    }

    /// (Re-)initialise the Rubber Band engine for the given sample rate.
    ///
    /// Must be called before `tick()` will produce output.
    pub fn update(&mut self, sample_rate: f64) {
        self.sample_rate = sample_rate;
        self.block_size = if self.live {
            BLOCK_SIZE_LIVE
        } else {
            BLOCK_SIZE
        };
        self.prev_live = self.live;

        // Tear down any previous instance.
        self.destroy_state();

        // Live mode: R2 engine (default, no ENGINE_FINER) for lower latency.
        // Standard mode: R3 engine (ENGINE_FINER) for higher quality.
        let mut options = ffi::OPTION_PROCESS_REALTIME
            | ffi::OPTION_PITCH_HIGH_QUALITY
            | ffi::OPTION_THREADING_NEVER;
        if !self.live {
            options |= ffi::OPTION_ENGINE_FINER;
        }

        let pitch_scale = self.speed.clamp(0.01, 100.0);

        let state = unsafe {
            ffi::rubberband_new(
                sample_rate as u32,
                1, // mono
                options,
                1.0, // time ratio (no time stretch)
                pitch_scale,
            )
        };
        assert!(!state.is_null(), "rubberband_new returned null");

        unsafe {
            ffi::rubberband_set_max_process_size(state, self.block_size as u32);
        }

        // Resize input buffer for new block size.
        self.in_buf.resize(self.block_size, 0.0);

        self.state = state;
        self.current_pitch_scale = pitch_scale;
        self.current_formants = self.preserve_formants;

        if self.preserve_formants {
            unsafe {
                ffi::rubberband_set_formant_option(state, ffi::OPTION_FORMANT_PRESERVED);
            }
        }

        self.reported_latency = unsafe { ffi::rubberband_get_latency(state) } as usize;

        // Clear buffers.
        self.in_count = 0;
        self.out_buf.clear();

        // Prime the engine: feed silence equal to the latency so that
        // subsequent output is aligned.
        let prime_samples = self.reported_latency + self.block_size;
        let silence = vec![0.0f32; self.block_size];
        let mut fed = 0;
        while fed < prime_samples {
            let n = self.block_size.min(prime_samples - fed);
            let slice = &silence[..n];
            let ptr: *const f32 = slice.as_ptr();
            unsafe {
                ffi::rubberband_process(self.state, &ptr, n as u32, 0);
            }
            fed += n;
            self.drain_available();
        }
        // Discard priming output — it corresponds to the silent prime input.
        self.out_buf.clear();
    }

    pub fn reset(&mut self) {
        if !self.state.is_null() {
            unsafe {
                ffi::rubberband_reset(self.state);
            }
        }
        self.in_count = 0;
        self.out_buf.clear();
    }

    /// Process one sample. Returns the pitch-shifted (and mixed) output.
    #[inline]
    pub fn tick(&mut self, input: f64) -> f64 {
        if self.state.is_null() {
            return input;
        }

        // Update pitch scale if it changed.
        let desired_pitch = self.speed.clamp(0.01, 100.0);
        if (desired_pitch - self.current_pitch_scale).abs() > 1e-9 {
            unsafe {
                ffi::rubberband_set_pitch_scale(self.state, desired_pitch);
            }
            self.current_pitch_scale = desired_pitch;
        }

        // Update formant preservation if it changed.
        if self.preserve_formants != self.current_formants {
            let opt = if self.preserve_formants {
                ffi::OPTION_FORMANT_PRESERVED
            } else {
                0 // RubberBandOptionFormantShifted = 0
            };
            unsafe {
                ffi::rubberband_set_formant_option(self.state, opt);
            }
            self.current_formants = self.preserve_formants;
        }

        // Accumulate input.
        self.in_buf[self.in_count] = input as f32;
        self.in_count += 1;

        // When we have a full block, push it into rubberband.
        if self.in_count >= self.block_size {
            let ptr: *const f32 = self.in_buf.as_ptr();
            unsafe {
                ffi::rubberband_process(self.state, &ptr, self.block_size as u32, 0);
            }
            self.in_count = 0;
            self.drain_available();
        }

        // Read one sample from the output ring buffer.
        let wet = self.out_buf.pop_front().unwrap_or(0.0) as f64;

        input * (1.0 - self.mix) + wet * self.mix
    }

    /// Reported latency in samples.
    pub fn latency(&self) -> usize {
        self.reported_latency
    }

    // -- internal helpers ---------------------------------------------------

    /// Pull all available output from Rubber Band into `out_buf`.
    fn drain_available(&mut self) {
        if self.state.is_null() {
            return;
        }
        loop {
            let avail = unsafe { ffi::rubberband_available(self.state) };
            if avail <= 0 {
                break;
            }
            let n = (avail as usize).min(self.retrieve_buf.len());
            let mut ptr: *mut f32 = self.retrieve_buf.as_mut_ptr();
            let retrieved = unsafe {
                ffi::rubberband_retrieve(self.state, &mut ptr as *mut *mut f32, n as u32)
            };
            for i in 0..retrieved as usize {
                self.out_buf.push_back(self.retrieve_buf[i]);
            }
            if (retrieved as usize) < n {
                break;
            }
        }
    }

    /// Free the rubberband state if allocated.
    fn destroy_state(&mut self) {
        if !self.state.is_null() {
            unsafe {
                ffi::rubberband_delete(self.state);
            }
            self.state = std::ptr::null_mut();
        }
    }
}

impl Default for RubberbandShifter {
    fn default() -> Self {
        Self::new()
    }
}

impl Drop for RubberbandShifter {
    fn drop(&mut self) {
        self.destroy_state();
    }
}

// RubberBandState is a raw pointer, but our wrapper owns it exclusively and
// is not shared across threads without external synchronisation.
unsafe impl Send for RubberbandShifter {}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::f64::consts::PI;

    const SR: f64 = 48000.0;

    fn make_shifter() -> RubberbandShifter {
        let mut s = RubberbandShifter::new();
        s.speed = 0.5;
        s.mix = 1.0;
        s.update(SR);
        s
    }

    #[test]
    fn silence_in_silence_out() {
        let mut s = make_shifter();
        for _ in 0..4800 {
            let out = s.tick(0.0);
            assert!(out.abs() < 1e-6, "Should be silent: {out}");
        }
    }

    #[test]
    fn produces_output_on_sine() {
        let mut s = make_shifter();
        let freq = 220.0;
        let n = 48000;

        let mut energy = 0.0;
        for i in 0..n {
            let input = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
            let out = s.tick(input);
            if i > 4096 {
                energy += out * out;
            }
        }
        assert!(energy > 0.1, "Should produce output: energy={energy}");
    }

    #[test]
    fn no_nan() {
        let mut s = make_shifter();
        for i in 0..48000 {
            let input = (2.0 * PI * 82.0 * i as f64 / SR).sin() * 0.9;
            let out = s.tick(input);
            assert!(out.is_finite(), "NaN/Inf at sample {i}");
        }
    }

    #[test]
    fn different_speeds_differ() {
        let freq = 220.0;
        let n = 9600;

        let collect = |speed: f64| -> Vec<f64> {
            let mut s = RubberbandShifter::new();
            s.speed = speed;
            s.mix = 1.0;
            s.update(SR);
            let mut out = Vec::with_capacity(n);
            for i in 0..n {
                let v = (2.0 * PI * freq * i as f64 / SR).sin() * 0.5;
                out.push(s.tick(v));
            }
            out
        };

        let down = collect(0.5);
        let up = collect(2.0);

        let diff: f64 = down
            .iter()
            .zip(up.iter())
            .map(|(a, b)| (a - b).abs())
            .sum::<f64>()
            / n as f64;

        assert!(
            diff > 0.001,
            "Different speeds should produce different output: {diff}"
        );
    }

    #[test]
    fn dry_wet_mix() {
        let mut s = make_shifter();
        s.mix = 0.0;

        for i in 0..4800 {
            let input = (2.0 * PI * 440.0 * i as f64 / SR).sin() * 0.5;
            let out = s.tick(input);
            assert!((out - input).abs() < 1e-10, "Mix=0 should pass dry");
        }
    }
}
