//! A playhead at a known instant, in a known clock.

/// Where a transport was, and when, in some clock (microseconds).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Position {
    /// The instant, in the clock this position is stamped in.
    pub host_micros: f64,
    /// The playhead at that instant, seconds.
    pub playhead_seconds: f64,
    /// Playhead seconds per clock second (1.0 = nominal).
    pub playrate: f64,
    pub is_playing: bool,
}

impl Position {
    /// Where the playhead is at `now_micros` (same clock): moved on by the
    /// time since, at its rate, if it is playing.
    #[must_use]
    pub fn at(&self, now_micros: f64) -> f64 {
        if self.is_playing {
            ((now_micros - self.host_micros) * 1e-6).mul_add(self.playrate, self.playhead_seconds)
        } else {
            self.playhead_seconds
        }
    }

    /// This position stamped in another clock, `offset_micros` ahead of
    /// this one (`other = this + offset`).
    #[must_use]
    pub fn shifted(self, offset_micros: f64) -> Self {
        Self {
            host_micros: self.host_micros + offset_micros,
            ..self
        }
    }
}
