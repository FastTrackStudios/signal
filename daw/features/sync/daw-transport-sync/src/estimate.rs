//! How far another clock is from this one.
//!
//! One exchange gives four stamps: `t1` sent here, `t2` received there,
//! `t3` answered there, `t4` received here. If the network took as long
//! each way,
//!
//! ```text
//! offset = ((t2 − t1) + (t3 − t4)) / 2     // there − here
//! delay  = ((t4 − t1) − (t3 − t2)) / 2     // one way
//! ```
//!
//! The error of one exchange is at most half its round trip's asymmetry,
//! so a slow round trip is a less trustworthy one — and no maths can tell
//! a path slower one way than the other from a clock offset. What can be
//! done is to trust the exchanges that met the least delay each way: the
//! fastest round trips are the ones nearest both paths' minimum, so
//! nearest symmetric. [`ClockEstimator`] keeps a window of exchanges and
//! averages the offsets of the fastest quarter — a scheduler hiccup or a
//! congested packet moves the estimate by nothing.

/// Fixed-size window with an interquartile mean: the top and bottom
/// quarter are dropped before averaging, so one wild value cannot swing
/// it.
#[derive(Clone, Debug)]
pub struct RollingWindow {
    values: Vec<f64>,
    cap: usize,
    cursor: usize,
    len: usize,
}

impl RollingWindow {
    #[must_use]
    pub fn new(cap: usize) -> Self {
        let cap = cap.max(1);
        Self {
            values: vec![0.0; cap],
            cap,
            cursor: 0,
            len: 0,
        }
    }

    pub fn push(&mut self, v: f64) {
        if let Some(slot) = self.values.get_mut(self.cursor) {
            *slot = v;
        }
        self.cursor = (self.cursor + 1) % self.cap;
        self.len = (self.len + 1).min(self.cap);
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.len
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// The interquartile mean; 0 when empty.
    #[must_use]
    pub fn average(&self) -> f64 {
        let Some(values) = self.values.get(..self.len) else {
            return 0.0;
        };
        iqm(values)
    }
}

#[allow(clippy::cast_precision_loss)]
fn iqm(values: &[f64]) -> f64 {
    if values.is_empty() {
        return 0.0;
    }
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let q = if sorted.len() < 4 {
        0
    } else {
        sorted.len() / 4
    };
    let kept = sorted.get(q..sorted.len() - q).unwrap_or(&sorted);
    kept.iter().sum::<f64>() / kept.len() as f64
}

/// One exchange, as recorded.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Exchange {
    offset: f64,
    round_trip: f64,
}

/// The offset to one other clock, from a window of exchanges.
#[derive(Clone, Debug)]
pub struct ClockEstimator {
    window: Vec<Exchange>,
    cap: usize,
    cursor: usize,
    /// The estimate as followers use it: the window's answer, smoothed, so
    /// a change in which exchanges are fastest does not jump it.
    smoothed: Option<f64>,
}

/// How much of each new window answer the smoothed offset takes (per
/// exchange; at 10 Hz, about a second's time constant).
const SMOOTHING: f64 = 0.1;

/// Exchanges before which the window's answer is taken as it is.
const WARMUP: usize = 4;

impl Default for ClockEstimator {
    fn default() -> Self {
        Self::new(32)
    }
}

impl ClockEstimator {
    /// Keep the last `cap` exchanges (32 at 10 Hz is ~3 s).
    #[must_use]
    pub fn new(cap: usize) -> Self {
        Self {
            window: Vec::with_capacity(cap.max(1)),
            cap: cap.max(1),
            cursor: 0,
            smoothed: None,
        }
    }

    /// Record one exchange: `t1` sent here, `t2` received there, `t3`
    /// answered there, `t4` received here — microseconds, each in its own
    /// clock. An exchange whose stamps cannot be right (a negative round
    /// trip) is ignored.
    pub fn record(&mut self, t1: f64, t2: f64, t3: f64, t4: f64) {
        let round_trip = (t4 - t1) - (t3 - t2);
        if !round_trip.is_finite() || round_trip < 0.0 {
            return;
        }
        let exchange = Exchange {
            offset: ((t2 - t1) + (t3 - t4)) / 2.0,
            round_trip,
        };
        if self.window.len() < self.cap {
            self.window.push(exchange);
        } else if let Some(slot) = self.window.get_mut(self.cursor) {
            *slot = exchange;
        }
        self.cursor = (self.cursor + 1) % self.cap;
        if let Some(raw) = self.window_offset() {
            self.smoothed = Some(match self.smoothed {
                Some(was) if self.window.len() > WARMUP => (raw - was).mul_add(SMOOTHING, was),
                _ => raw,
            });
        }
    }

    /// How many exchanges the estimate rests on.
    #[must_use]
    pub fn samples(&self) -> usize {
        self.window.len()
    }

    /// The other clock minus this one, microseconds — `None` before the
    /// first exchange. The fastest quarter of the window's round trips,
    /// their offsets averaged, then smoothed over the last second or so.
    #[must_use]
    pub const fn offset_micros(&self) -> Option<f64> {
        self.smoothed
    }

    /// The window's own answer, unsmoothed.
    #[must_use]
    pub fn window_offset(&self) -> Option<f64> {
        if self.window.is_empty() {
            return None;
        }
        let mut by_speed = self.window.clone();
        by_speed.sort_by(|a, b| a.round_trip.total_cmp(&b.round_trip));
        let fastest = by_speed.len().div_ceil(4);
        let offsets: Vec<f64> = by_speed.iter().take(fastest).map(|e| e.offset).collect();
        Some(iqm(&offsets))
    }

    /// The typical round trip, microseconds (how much to trust the offset:
    /// its error is at most half the asymmetry of this).
    #[must_use]
    pub fn round_trip_micros(&self) -> Option<f64> {
        if self.window.is_empty() {
            return None;
        }
        let trips: Vec<f64> = self.window.iter().map(|e| e.round_trip).collect();
        Some(iqm(&trips))
    }

    /// A time in this clock, in the other one.
    #[must_use]
    pub fn to_remote(&self, local_micros: f64) -> Option<f64> {
        self.offset_micros().map(|o| local_micros + o)
    }

    /// A time in the other clock, in this one.
    #[must_use]
    pub fn to_local(&self, remote_micros: f64) -> Option<f64> {
        self.offset_micros().map(|o| remote_micros - o)
    }
}
