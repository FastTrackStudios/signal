//! Per-stage frame timing, shared by the bench and the window.
//!
//! A frame time on its own says a gesture is slow. It never says WHICH
//! part is slow, so every optimization argued from one is a guess — and
//! this session has already paid for three of those (a CSS gradient, a
//! TCP recycling pool, a per-frame scene clone), each of which was
//! reasoned about convincingly and measured at zero.
//!
//! So the rule here is that work gets a stage before it gets an opinion.
//! A frame splits into:
//!
//! - **paint** — our replay loop plus Vello's encoding of what it pushes.
//!   CPU, ours, and the only part we write.
//! - **gpu** — Vello's compute pipeline and rasterisation. Measured by
//!   subtraction, because `anyrender` hands out no device to put
//!   timestamp queries on.
//! - **readback** — the `ImageRenderer` copying the surface back to CPU
//!   memory. A property of the HARNESS: 29 MB a frame at 5120x1440, and
//!   the windowed app never pays it. Measured once against an empty
//!   scene and subtracted, which is the difference between quoting 141
//!   fps and quoting 66.
//!
//! [`Samples`] is the summary used for both stages and whole phases, so
//! a stage table and a phase table cannot drift into reporting
//! percentiles two different ways.

use web_time::Instant;

/// A set of frame timings in milliseconds, summarised the same way
/// everywhere.
///
/// Sorting happens in [`Samples::summary`] rather than on push: a frame
/// loop should never pay to keep an ordering nobody reads until the
/// phase ends.
#[derive(Default, Clone)]
pub struct Samples {
    ms: Vec<f64>,
}

/// What a set of frame times is worth quoting as.
///
/// The mean is what the gesture costs on average; `p99` and `worst` are
/// whether it ever misses. A mean inside budget with a p99 at triple it
/// is a gesture that visibly hitches, so both are always printed —
/// reporting the mean alone is how a stutter gets called smooth.
#[derive(Clone, Copy)]
pub struct Summary {
    pub mean: f64,
    pub p50: f64,
    pub p99: f64,
    pub worst: f64,
    pub frames: usize,
}

impl Samples {
    /// Room for a phase's worth of frames, so the loop never reallocates
    /// inside a measurement.
    #[must_use]
    pub fn with_capacity(frames: usize) -> Self {
        Self {
            ms: Vec::with_capacity(frames),
        }
    }

    /// Record one frame, in milliseconds.
    pub fn push_ms(&mut self, ms: f64) {
        self.ms.push(ms);
    }

    /// Record the time since `start`.
    pub fn push_since(&mut self, start: Instant) {
        self.push_ms(start.elapsed().as_secs_f64() * 1000.0);
    }

    /// Drop the first `n` frames.
    ///
    /// The opening frames of a phase warm up geometry the previous one
    /// never touched — that is the cost of CHANGING gesture, not of
    /// performing it, and leaving it in makes every phase look like its
    /// predecessor.
    pub fn drop_warmup(&mut self, n: usize) {
        self.ms.drain(..n.min(self.ms.len()));
    }

    /// Subtract a constant per-frame cost from every sample — the
    /// harness's readback, which the real app does not perform.
    ///
    /// Clamped at zero: a sample below the floor means the floor was
    /// measured a little high, not that a frame took negative time.
    #[must_use]
    pub fn less(&self, floor: f64) -> Self {
        Self {
            ms: self.ms.iter().map(|ms| (ms - floor).max(0.0)).collect(),
        }
    }

    #[must_use]
    pub const fn is_empty(&self) -> bool {
        self.ms.is_empty()
    }

    /// `None` when nothing was recorded, so a stage that never ran
    /// prints as absent rather than as zero — a stage reporting 0.00ms
    /// reads like a free one.
    ///
    /// A NaN sample sorts as equal to everything, which would scramble
    /// the ordering rather than fail; the clock cannot produce one, and
    /// `total_cmp` makes that assumption unnecessary either way.
    #[must_use]
    pub fn summary(&self) -> Option<Summary> {
        let mut sorted = self.ms.clone();
        sorted.sort_by(f64::total_cmp);
        let last = sorted.len().checked_sub(1)?;
        // Nearest-rank, on the sorted index rather than on a float
        // fraction of it, so the quantile cannot land off the end.
        let at = |percent: usize| {
            let rank = last.saturating_mul(percent) / 100;
            sorted.get(rank).copied().unwrap_or_default()
        };
        let frames = sorted.len();
        // `as` is banned here, and rightly: a count that did not fit an
        // f64 exactly would make the mean quietly wrong. A phase is a
        // few hundred frames, so this conversion is exact — and if it
        // ever were not, this returns `None` rather than a bad number.
        let mean = sorted.iter().sum::<f64>() / f64::from(u32::try_from(frames).ok()?);
        Some(Summary {
            mean,
            p50: at(50),
            p99: at(99),
            worst: sorted.get(last).copied().unwrap_or_default(),
            frames,
        })
    }
}

impl Summary {
    /// Frames per second at the p99 — the rate this gesture SUSTAINS.
    ///
    /// Deliberately not computed from the mean. A mean-derived fps is
    /// the number a gesture hits between its stutters.
    #[must_use]
    pub const fn fps(self) -> f64 {
        1000.0 / self.p99.max(0.001)
    }
}

/// The stages of one frame, timed together.
///
/// `paint` is measured directly; `gpu` is what is left of the frame once
/// paint and the harness floor are taken out. Keeping the subtraction
/// here rather than at each call site is what stops the three numbers
/// from failing to add up to the frame.
#[derive(Default)]
pub struct Stages {
    pub frame: Samples,
    pub paint: Samples,
}

impl Stages {
    #[must_use]
    pub fn with_capacity(frames: usize) -> Self {
        Self {
            frame: Samples::with_capacity(frames),
            paint: Samples::with_capacity(frames),
        }
    }

    /// GPU time: the frame minus our paint work, minus the harness's
    /// readback floor. Pass `0.0` for `floor` from a windowed caller,
    /// which presents instead of copying back.
    #[must_use]
    pub fn gpu(&self, floor: f64) -> Samples {
        let mut gpu = Samples::with_capacity(self.frame.ms.len());
        for (frame, paint) in self.frame.ms.iter().zip(&self.paint.ms) {
            gpu.push_ms((frame - paint - floor).max(0.0));
        }
        gpu
    }
}

/// How much of the scene a frame actually drew.
///
/// A timing says a frame was slow; these say whether it was slow because
/// the work is expensive or because there was too much of it. The pair
/// `replayed` / `submitted` is the one that matters for culling: they
/// are equal today, because every command in the scene is pushed
/// regardless of where the viewport is.
#[derive(Default, Clone, Copy)]
pub struct Counts {
    /// Commands walked in the replay loop.
    pub replayed: u64,
    /// Commands actually pushed to the painter.
    pub submitted: u64,
}

impl Counts {
    /// The share of walked commands that reached the painter, as a
    /// percentage. 100% means nothing is being culled WITHIN the rows
    /// that were walked.
    ///
    /// `None` when nothing was walked at all — a viewport scrolled past
    /// the end of the session is an empty frame, not a frame that culled
    /// everything, and folding the two together reads as a bug in the
    /// culling every time.
    #[must_use]
    pub fn kept_pct(self) -> Option<f64> {
        // Same reasoning as `Samples::summary`: counts are in the
        // thousands, so the conversion is exact, and a count too large
        // to convert reports nothing rather than a wrong percentage.
        let submitted = f64::from(u32::try_from(self.submitted).ok()?);
        let replayed = f64::from(u32::try_from(self.replayed).ok()?);
        (replayed > 0.0).then_some(submitted * 100.0 / replayed)
    }
}
