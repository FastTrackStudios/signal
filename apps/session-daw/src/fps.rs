//! A frame-time readout drawn into the frame it is measuring.
//!
//! Modelled on Vello's own `stats.rs` overlay, and for its reasons: a
//! single number is not enough to tell a slow window from a window with
//! a hitch in it. The mean says 4 ms while every twentieth frame takes
//! 40 and the picture stutters; the max says 40 while 99 frames in a
//! hundred were fine. A hundred bars says which.
//!
//! Two things are ours rather than Vello's.
//!
//! The budget. Vello's bars turn amber at 16.67 ms and red at 33.33,
//! because its examples are aimed at 60 Hz. This window is aimed at 240,
//! so the line that matters is [`BUDGET_MS`] — 4.17 — and a frame that
//! would be a comfortable green on a 60 Hz graph is already over.
//!
//! And where the samples come from. Not the interval between frames,
//! which during a gesture is the input's cadence and while idle is
//! nothing at all, but what the shell reports one frame COST: resolve,
//! encode and present. See `blitz_traits::LAST_FRAME_MICROS`.

use std::collections::VecDeque;

use anyrender::PaintScene;
use vello::kurbo::{Affine, Rect};
use vello::peniko::{Color, Fill};

use crate::text::Font;

/// How many frames the graph remembers.
const WINDOW: usize = 100;

/// The frame time this window is built for: 240 Hz.
pub const BUDGET_MS: f64 = 1000.0 / 240.0;

/// And the one it is allowed to fall back to before the bar goes red.
const AMBER_MS: f64 = 1000.0 / 120.0;

/// The lines drawn across the graph, in milliseconds.
const MARKS: [f64; 3] = [BUDGET_MS, AMBER_MS, 1000.0 / 60.0];

const INK: Color = Color::from_rgb8(0xF2, 0xF2, 0xF2);
const UNDER: Color = Color::from_rgb8(0x5B, 0xC8, 0x8C);
const OVER: Color = Color::from_rgb8(0xE3, 0xB3, 0x41);
const WAY_OVER: Color = Color::from_rgb8(0xDC, 0x26, 0x7F);

/// What the window looked like over the last [`WINDOW`] frames.
#[derive(Clone, Copy, Debug)]
pub struct Snapshot {
    pub fps: f64,
    pub mean_ms: f64,
    pub min_ms: f64,
    pub max_ms: f64,
}

/// The sliding window itself.
#[derive(Debug, Default)]
pub struct Stats {
    sum: u64,
    samples: VecDeque<u64>,
}

impl Stats {
    #[must_use]
    pub fn new() -> Self {
        Self {
            sum: 0,
            samples: VecDeque::with_capacity(WINDOW),
        }
    }

    /// Record what one frame cost, in microseconds.
    ///
    /// Anything past a fifth of a second is dropped rather than
    /// recorded. A gap that long is the shell having done something
    /// other than draw this window — the first frame, a resize, a wake
    /// from idle — and letting one into the window rescales the whole
    /// graph around an event that is not a frame.
    pub fn add(&mut self, micros: u64) {
        const IDLE: u64 = 200_000;
        if micros == 0 || micros >= IDLE {
            return;
        }
        if self.samples.len() == WINDOW {
            if let Some(oldest) = self.samples.pop_front() {
                self.sum = self.sum.saturating_sub(oldest);
            }
        }
        self.sum = self.sum.saturating_add(micros);
        self.samples.push_back(micros);
    }

    /// The frames in the window, oldest first.
    pub fn samples(&self) -> impl ExactSizeIterator<Item = &u64> {
        self.samples.iter()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.samples.is_empty()
    }

    /// The window, summarised. `None` until there is anything in it.
    #[must_use]
    pub fn snapshot(&self) -> Option<Snapshot> {
        let count = count(self.samples.len());
        if count == 0.0 {
            return None;
        }
        let mean_ms = millis(self.sum) / count;
        Some(Snapshot {
            fps: 1000.0 / mean_ms.max(f64::EPSILON),
            mean_ms,
            min_ms: millis(self.samples.iter().copied().min().unwrap_or_default()),
            max_ms: millis(self.samples.iter().copied().max().unwrap_or_default()),
        })
    }
}

/// Microseconds as milliseconds, without an `as`.
fn millis(us: u64) -> f64 {
    u32::try_from(us).map_or(f64::from(u32::MAX), f64::from) / 1000.0
}

/// A count as a float, without an `as`.
fn count(n: usize) -> f64 {
    u32::try_from(n).map_or(f64::from(u32::MAX), f64::from)
}

/// Round `n` up to the next multiple of `step`.
fn round_up(n: f64, step: f64) -> f64 {
    (n / step).ceil() * step
}

/// Draw the overlay into the bottom-right corner of a `width`x`height`
/// box, plus however many extra lines the caller wants under the
/// standard ones.
///
/// The extra lines are how the widget says what it actually drew —
/// commands replayed, commands submitted — which is the number that
/// says whether a slow frame is the renderer's fault or the culling's.
pub fn draw(
    scene: &mut impl PaintScene,
    font: &Font,
    stats: &Stats,
    size: (f64, f64),
    extra: &[String],
) {
    let Some(at) = stats.snapshot() else {
        return;
    };
    let (width, height) = size;
    let panel_w = (width * 0.4).clamp(200.0, 520.0);
    let panel_h = panel_w * 0.7;
    let (ox, oy) = (width - panel_w, height - panel_h);

    scene.fill(
        Fill::NonZero,
        Affine::translate((ox, oy)),
        Color::from_rgb8(0, 0, 0).with_alpha(0.78),
        None,
        &Rect::new(0.0, 0.0, panel_w, panel_h),
    );

    let mut labels = vec![
        format!("{:.2} ms   {:.0} fps", at.mean_ms, at.fps),
        format!("min {:.2}   max {:.2} ms", at.min_ms, at.max_ms),
        format!("budget {BUDGET_MS:.2} ms"),
    ];
    labels.extend_from_slice(extra);

    // The top half is lettering and the bottom half is the graph, so a
    // line the caller adds makes the text smaller rather than pushing
    // the bars off the panel.
    let line = panel_h * 0.5 / count(labels.len().saturating_add(1));
    let margin = panel_w * 0.02;
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a font size inside a panel of at most 520 points"
    )]
    let text_size = (line * 0.8) as f32;
    for (i, label) in labels.iter().enumerate() {
        let baseline = oy + count(i.saturating_add(1)) * line;
        crate::tcp::glyphs(scene, font, INK, label, ox + margin, baseline, text_size);
    }

    let plot = Plot {
        base: (ox + margin + panel_w * 0.06, oy + panel_h),
        size: (
            panel_w - 2.0 * margin - panel_w * 0.06,
            panel_h * 0.5 - line,
        ),
        gutter: ox + margin,
        text_size,
    };
    graph(scene, font, stats, at, plot);
}

/// Where the bars go, so `graph` takes one argument for it rather than
/// six.
#[derive(Clone, Copy)]
struct Plot {
    /// The bottom-left of the plot, in scene coordinates.
    base: (f64, f64),
    size: (f64, f64),
    /// Where the threshold numbers are lettered, to the left of it.
    gutter: f64,
    text_size: f32,
}

/// The bars, and the lines they are read against.
fn graph(scene: &mut impl PaintScene, font: &Font, stats: &Stats, at: Snapshot, plot: Plot) {
    let (bx, by) = plot.base;
    let (plot_w, plot_h) = plot.size;

    // What the tallest bar means. The observed max, unless a single
    // spike would flatten everything else into the floor — then the
    // scale follows the mean instead, so a graph of ordinary frames
    // stays readable through a hitch.
    //
    // With a floor a little ABOVE the budget rather than at it. At it,
    // the budget line sits exactly on the top of the plot and is
    // clipped away by the `< ceiling` filter below — so the one line
    // worth seeing disappeared precisely when every frame was inside
    // it, which is the case this readout exists to confirm.
    let ceiling = if at.max_ms > 3.0 * at.mean_ms {
        round_up(1.334 * at.mean_ms, BUDGET_MS)
    } else {
        at.max_ms.max(BUDGET_MS * 1.25)
    };

    let step = plot_w / count(WINDOW);
    let bar = Rect::new(0.0, 0.0, step * 0.6, plot_h);
    for (i, sample) in stats.samples().enumerate() {
        let ms = millis(*sample);
        let colour = if ms <= BUDGET_MS {
            UNDER
        } else if ms <= AMBER_MS {
            OVER
        } else {
            WAY_OVER
        };
        let tall = (ms / ceiling).clamp(0.0, 1.0);
        scene.fill(
            Fill::NonZero,
            Affine::translate((bx + count(i) * step, by)) * Affine::scale_non_uniform(1.0, -tall),
            colour,
            None,
            &bar,
        );
    }

    // And the lines to read them against, each with its own number in
    // the gutter the bars were inset to leave.
    let rule = Rect::new(0.0, 0.0, plot_w, (plot_h * 0.012).max(0.75));
    for ms in MARKS.into_iter().filter(|ms| *ms < ceiling) {
        let y = by - plot_h * (ms / ceiling);
        scene.fill(
            Fill::NonZero,
            Affine::translate((bx, y)),
            INK.with_alpha(0.45),
            None,
            &rule,
        );
        crate::tcp::glyphs(
            scene,
            font,
            INK.with_alpha(0.7),
            &format!("{ms:.2}"),
            plot.gutter,
            y + f64::from(plot.text_size) * 0.3,
            plot.text_size * 0.8,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{BUDGET_MS, Stats, WINDOW};

    #[test]
    fn an_empty_window_has_nothing_to_say() {
        assert!(Stats::new().snapshot().is_none());
    }

    #[test]
    fn the_window_slides_and_the_mean_follows_it() {
        let mut stats = Stats::new();
        for _ in 0..WINDOW {
            stats.add(10_000);
        }
        let at = stats.snapshot().expect("a full window");
        assert!((at.mean_ms - 10.0).abs() < 1e-9, "{}", at.mean_ms);

        // Fill it again with faster frames: the old ones fall out the
        // front, so the mean is the NEW rate and not an average of both.
        for _ in 0..WINDOW {
            stats.add(4_000);
        }
        let at = stats.snapshot().expect("a full window");
        assert!((at.mean_ms - 4.0).abs() < 1e-9, "{}", at.mean_ms);
        assert!((at.min_ms - 4.0).abs() < 1e-9);
        assert!((at.max_ms - 4.0).abs() < 1e-9);
        assert!(at.fps > 240.0);
        assert!(at.mean_ms < BUDGET_MS);
    }

    #[test]
    fn a_wake_from_idle_is_not_a_frame() {
        let mut stats = Stats::new();
        stats.add(4_000);
        stats.add(0);
        stats.add(5_000_000);
        assert_eq!(stats.samples().len(), 1);
        let at = stats.snapshot().expect("one frame");
        assert!((at.max_ms - 4.0).abs() < 1e-9, "{}", at.max_ms);
    }
}
