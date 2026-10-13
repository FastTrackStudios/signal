//! What the processing LOOKED like, recorded and played back.
//!
//! Every processor graph in this module takes two things: its parameters
//! and its metering. The parameters are settings; the metering is what
//! the processor is doing right now — gain reduction, sibilance, levels
//! — and it is what makes a graph move.
//!
//! Metering normally comes from DSP running in the audio thread, which
//! ties the picture to the processing. That tie is the problem this
//! module removes, because it is wrong in three situations that matter:
//!
//! - **Offline steps.** De-clip, de-click, Melodyne tuning and time
//!   alignment are renders written back to the item. There is no plugin
//!   instance, so there is nothing to meter — and yet what those steps
//!   did to the audio is exactly what you want to see.
//! - **Frozen tracks.** The audio is already rendered and the DSP is
//!   gone. Today that means the graphs go still, which reads as "this
//!   track is doing nothing" rather than "this track's work is already
//!   done".
//! - **A session too big to run.** Two thousand tracks of live
//!   processing is not playable; two thousand tracks of RECORDED
//!   processing is a few megabytes.
//!
//! A metering frame is a handful of `f32`s, so recording one per UI
//! frame costs almost nothing: three floats at 60 Hz over five minutes
//! is about 200 KB. The picture is cheap. The DSP is what is expensive.
//!
//! # One interface, two sources
//!
//! A widget asks [`Metering::sample`] and gets a frame. It does not know
//! whether something is running. That is the whole design — a graph fed
//! by a trace and a graph fed by a live processor are the same graph,
//! which is what lets a track be frozen, thawed, or rendered offline
//! without the strip changing at all.

use std::sync::Arc;

/// How often a trace is sampled, in frames per second.
///
/// UI rate, not audio rate. A meter that moves faster than the screen
/// refreshes is showing something nobody can see, and sampling at audio
/// rate would make a trace four orders of magnitude larger for no
/// visible gain. Peaks are held by the metering itself, so a fast
/// transient still registers between samples.
pub const RATE: f32 = 60.0;

/// A processor's visual state over time.
///
/// Generic over the metering type, because every processor has its own —
/// a de-esser reports sibilance, a compressor reports gain reduction,
/// and flattening them into a common shape would lose exactly the
/// information the graphs are drawn from.
#[derive(Clone, Debug, PartialEq)]
pub struct Trace<M> {
    rate: f32,
    frames: Vec<M>,
}

impl<M> Trace<M> {
    /// An empty trace at the default [`RATE`].
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rate: RATE,
            frames: Vec::new(),
        }
    }

    /// An empty trace sampled at `rate` frames per second.
    ///
    /// A render that produces frames at its own cadence records that
    /// cadence rather than resampling to 60: the trace knows its own
    /// rate, so playback lands on the right frame either way.
    #[must_use]
    pub const fn at_rate(rate: f32) -> Self {
        Self {
            rate,
            frames: Vec::new(),
        }
    }

    /// Record one frame.
    pub fn push(&mut self, frame: M) {
        self.frames.push(frame);
    }

    #[must_use]
    pub const fn len(&self) -> usize {
        self.frames.len()
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }

    /// How long the trace runs, in seconds.
    #[must_use]
    pub fn duration(&self) -> f64 {
        if self.rate <= 0.0 {
            return 0.0;
        }
        f64::from(u32::try_from(self.frames.len()).unwrap_or(u32::MAX)) / f64::from(self.rate)
    }

    /// The frame at `seconds`, or `None` outside the trace.
    ///
    /// Nearest frame rather than interpolated: these are meters, and a
    /// meter reading halfway between two gain-reduction values is a
    /// number the processor never produced. Interpolation would also
    /// round the peaks off, which is the one thing a meter must not do.
    #[must_use]
    pub fn at(&self, seconds: f64) -> Option<&M> {
        if seconds < 0.0 || self.rate <= 0.0 {
            return None;
        }
        let index = (seconds * f64::from(self.rate)).round();
        // Bounds-checked before the conversion rather than after: a
        // transport seeking past the end of a trace is ordinary, and
        // `index` is only known to be a finite non-negative f64 here.
        let frames = u32::try_from(self.frames.len()).unwrap_or(u32::MAX);
        if !index.is_finite() || index >= f64::from(frames) {
            return None;
        }
        #[expect(
            clippy::cast_possible_truncation,
            clippy::cast_sign_loss,
            clippy::as_conversions,
            reason = "checked above: index is finite, >= 0 and < the frame count, so it is exact"
        )]
        let index = index as usize;
        self.frames.get(index)
    }
}

impl<M> Default for Trace<M> {
    fn default() -> Self {
        Self::new()
    }
}

/// Where a graph's metering comes from.
///
/// The three states a processor can be in from the UI's point of view,
/// which are not the same as the three states it can be in from the
/// audio engine's.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum Metering<M> {
    /// Something is running, and this is its current frame.
    Live(M),
    /// Nothing is running, but a recording exists — a frozen track, or
    /// an offline step that was rendered. `at` is the playback position
    /// in seconds, relative to the trace's start.
    Replay { trace: Arc<Trace<M>>, at: f64 },
    /// Nothing is running and nothing was recorded.
    ///
    /// A distinct state on purpose: a processor that has never been run
    /// is not the same as one sitting at zero, and a graph should be
    /// able to say "no data" rather than draw a flat line that looks
    /// like silence.
    ///
    /// The default, so a widget that is given no source shows nothing
    /// rather than a processor that isn't there.
    #[default]
    Idle,
}

impl<M: Clone + Default> Metering<M> {
    /// The frame to draw.
    ///
    /// `Idle`, and a `Replay` seeked past its end, both give the
    /// default — which for every metering type in this module is the
    /// resting state. The distinction between them is preserved in
    /// [`Metering::has_data`] for a graph that wants to say so.
    #[must_use]
    pub fn sample(&self) -> M {
        match self {
            Self::Live(frame) => frame.clone(),
            Self::Replay { trace, at } => trace.at(*at).cloned().unwrap_or_default(),
            Self::Idle => M::default(),
        }
    }
}

impl<M> Metering<M> {
    /// Whether there is anything to show — live or recorded.
    #[must_use]
    pub const fn has_data(&self) -> bool {
        !matches!(self, Self::Idle)
    }

    /// Whether this is a recording rather than a running processor.
    ///
    /// Worth surfacing: a graph may want to mark itself as showing
    /// something that already happened, and a strip may want to say a
    /// track is frozen rather than quiet.
    #[must_use]
    pub const fn is_replay(&self) -> bool {
        matches!(self, Self::Replay { .. })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy, Debug, Default, PartialEq)]
    struct Gr(f32);

    fn ramp(n: u8) -> Trace<Gr> {
        let mut trace = Trace::at_rate(10.0);
        for i in 0..n {
            trace.push(Gr(f32::from(i)));
        }
        trace
    }

    #[test]
    fn samples_the_nearest_frame() {
        let trace = ramp(5);
        assert_eq!(trace.at(0.0), Some(&Gr(0.0)));
        assert_eq!(trace.at(0.3), Some(&Gr(3.0)));
        // Nearest, not floor: 0.24 is closer to frame 2 than frame 3.
        assert_eq!(trace.at(0.24), Some(&Gr(2.0)));
    }

    #[test]
    fn outside_the_trace_is_none() {
        let trace = ramp(5);
        assert_eq!(trace.at(-1.0), None);
        assert_eq!(trace.at(99.0), None);
    }

    #[test]
    fn duration_follows_the_rate() {
        assert!((ramp(20).duration() - 2.0).abs() < 1e-9);
    }

    /// The point of the whole module: a widget asks for a frame and does
    /// not learn whether a processor is running.
    #[test]
    fn live_and_replay_answer_the_same_question() {
        let live = Metering::Live(Gr(6.0));
        let replay = Metering::Replay {
            trace: Arc::new(ramp(10)),
            at: 0.6,
        };
        assert_eq!(live.sample(), replay.sample());
        assert!(live.has_data() && replay.has_data());
        assert!(replay.is_replay() && !live.is_replay());
    }

    /// Seeking past the end rests rather than holding the last frame: a
    /// meter stuck at whatever it read when the audio stopped is worse
    /// than one at zero.
    #[test]
    fn past_the_end_rests() {
        let replay = Metering::Replay {
            trace: Arc::new(ramp(4)),
            at: 10.0,
        };
        assert_eq!(replay.sample(), Gr::default());
        assert!(replay.has_data());
    }

    /// Idle is not zero — it is "nothing has run".
    #[test]
    fn idle_has_no_data() {
        let idle: Metering<Gr> = Metering::Idle;
        assert_eq!(idle.sample(), Gr::default());
        assert!(!idle.has_data());
    }
}
