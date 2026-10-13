//! When each buffer really started, from callbacks that arrive late by a
//! varying amount.
//!
//! An audio callback runs when the OS gets to it — a fraction of a
//! millisecond after the device asked, more under load — so reading the
//! clock on entry stamps each buffer with that jitter. The device itself
//! is steady: buffers start exactly `frames / sample_rate` apart by its
//! crystal. [`BufferClock`] is the delay-locked loop JACK uses (after Fons
//! Adriaensen, "Using a DLL to filter time"): a second-order loop that
//! tracks the true buffer period and start times, so each stamp is where
//! the steady device clock puts it, not where the scheduler happened to.
//!
//! Real-time safe: no allocation, a few multiplies per buffer.

/// A delay-locked loop over buffer start times.
#[derive(Clone, Copy, Debug)]
pub struct BufferClock {
    bandwidth_hz: f64,
    /// Filtered start of the current buffer, µs.
    t0: f64,
    /// Predicted start of the next buffer, µs.
    t1: f64,
    /// Filtered buffer period, µs.
    period: f64,
    b: f64,
    c: f64,
    frames: u32,
    sample_rate: f64,
    started: bool,
}

impl Default for BufferClock {
    fn default() -> Self {
        Self::new(1.0)
    }
}

impl BufferClock {
    /// A loop of `bandwidth_hz` (about 1 Hz: slow enough to ignore
    /// scheduler jitter, fast enough to follow a device's crystal).
    #[must_use]
    pub const fn new(bandwidth_hz: f64) -> Self {
        Self {
            bandwidth_hz,
            t0: 0.0,
            t1: 0.0,
            period: 0.0,
            b: 0.0,
            c: 0.0,
            frames: 0,
            sample_rate: 0.0,
            started: false,
        }
    }

    /// Start over (the device restarted, or the stream stalled).
    pub fn reset(&mut self) {
        self.started = false;
    }

    /// One buffer: its callback ran at `now_micros` (clock read on entry)
    /// and it is `frames` long at `sample_rate`. Returns the filtered start
    /// of this buffer, µs.
    pub fn tick(&mut self, now_micros: f64, frames: u32, sample_rate: f64) -> f64 {
        let nominal = f64::from(frames) / sample_rate * 1e6;
        let reshaped =
            frames != self.frames || (sample_rate - self.sample_rate).abs() > f64::EPSILON;
        // A gap of several buffers (a stall, a stop and start): the loop's
        // prediction means nothing, begin again from this callback.
        let stalled = self.started && (now_micros - self.t1).abs() > 4.0 * self.period.max(nominal);
        if !self.started || reshaped || stalled {
            let omega = 2.0 * core::f64::consts::PI * self.bandwidth_hz * nominal * 1e-6;
            self.b = core::f64::consts::SQRT_2 * omega;
            self.c = omega * omega;
            self.frames = frames;
            self.sample_rate = sample_rate;
            self.period = nominal;
            self.t0 = now_micros;
            self.t1 = now_micros + nominal;
            self.started = true;
            return self.t0;
        }
        let error = now_micros - self.t1;
        self.t0 = self.t1;
        self.t1 += self.b.mul_add(error, self.period);
        self.period += self.c * error;
        self.t0
    }

    /// The filtered buffer period, µs (the device's true rate, in this
    /// clock).
    #[must_use]
    pub const fn period_micros(&self) -> f64 {
        self.period
    }
}
