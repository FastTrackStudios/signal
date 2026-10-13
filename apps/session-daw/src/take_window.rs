//! What the arrangement should be showing while a take is reviewed.
//!
//! The review screen is not a second timeline: it is the arrangement,
//! scrolled and zoomed to the thing being judged. This works out where
//! that is — the section you are in, with two bars of run-up and one of
//! tail — and hands back an ordinary [`Viewport`], because everything
//! that draws the arrangement already takes one.
//!
//! # Why two before and one after
//!
//! Because a take is judged from its entrance. Whether the chorus
//! landed is a question about the bar before it as much as the chorus,
//! and a window that starts exactly on the downbeat shows you the
//! result without the approach. One bar after is enough to see the
//! ending resolve and not so much that the section stops being the
//! subject.
//!
//! Bars, not seconds, because the run-up is musical: two bars is two
//! bars at any tempo, where four seconds is a bar and a half at 90 and
//! three bars at 180.

use daw_ui::studio::project::TempoChange;

use crate::arrangement::Viewport;
use crate::ruler::Bars;

/// How many bars of run-up the window shows.
pub const BEFORE: f64 = 2.0;

/// How many bars of tail.
pub const AFTER: f64 = 1.0;

/// The shortest window, in seconds.
///
/// A four-bar section at 200 bpm is under five seconds, and a window
/// that short is a wall of waveform with no shape to read. Below this
/// the window grows around the section rather than hugging it.
pub const SHORTEST: f64 = 8.0;

/// The span of session the review should show for a section.
///
/// Clamped at zero: a section at the top of the song cannot show two
/// bars of what came before it, and a window that started at a negative
/// time would draw the count-in twice.
#[must_use]
pub fn span(section: (f64, f64), tempo: &[TempoChange]) -> (f64, f64) {
    let (start, end) = (section.0.min(section.1), section.0.max(section.1));
    // The bar length is read at each END of the window rather than once
    // in the middle: a song that changes tempo at the section boundary —
    // which is where songs change tempo — has a different run-up from
    // its tail, and one number for both would be wrong at whichever end
    // it was not measured at.
    let run_up = Bars::at_time(tempo, start).secs_per_bar() * BEFORE;
    let tail = Bars::at_time(tempo, end).secs_per_bar() * AFTER;
    let (mut from, mut to) = ((start - run_up).max(0.0), end + tail);
    if to - from < SHORTEST {
        // Grown from the middle, so a short section stays in the middle
        // of what you are looking at rather than being pinned to an
        // edge by the arithmetic.
        let middle = (from + to) / 2.0;
        from = (middle - SHORTEST / 2.0).max(0.0);
        to = from + SHORTEST;
    }
    (from, to)
}

/// The viewport that shows that span in a frame this big.
///
/// `width` and `height` are the frame's CONTENT — what is inside the
/// rails — because that is what every other viewport in this window is
/// built from. The span is fitted to the LANE, which is that width
/// minus the track panel: fitting it to the whole content would push
/// the last bar of the section under the panel, where it is drawn over
/// rather than visible.
#[must_use]
pub fn viewport(
    section: (f64, f64),
    tempo: &[TempoChange],
    width: f64,
    height: f64,
    zoom_y: f64,
) -> Viewport {
    let (from, to) = span(section, tempo);
    let seconds = (to - from).max(0.001);
    let lane = (width - crate::arrangement::TCP_WIDTH).max(1.0);
    let pps = (lane / seconds).max(0.001);
    Viewport {
        scroll_x: from * pps,
        scroll_y: 0.0,
        pps,
        zoom_y,
        width,
        height,
        panel_w: crate::arrangement::TCP_WIDTH,
    }
}

#[cfg(test)]
mod tests {
    use super::{AFTER, BEFORE, SHORTEST, span, viewport};
    use daw_ui::studio::project::TempoChange;

    fn at(at: f64, bpm: f64) -> TempoChange {
        TempoChange {
            at,
            bpm,
            beats_per_bar: 4,
            beat_unit: 4,
        }
    }

    /// Two bars before and one after, measured in bars rather than
    /// seconds: at 120 in four four a bar is two seconds.
    #[test]
    fn the_window_is_two_bars_of_run_up_and_one_of_tail() {
        let tempo = vec![at(0.0, 120.0)];
        let (from, to) = span((40.0, 72.0), &tempo);
        assert!((from - (40.0 - 4.0)).abs() < 1e-9, "from {from}");
        assert!((to - (72.0 + 2.0)).abs() < 1e-9, "to {to}");
    }

    /// The same window at half the tempo is twice the run-up, because
    /// two bars is two bars.
    #[test]
    fn the_run_up_is_musical_and_not_a_number_of_seconds() {
        let fast = span((40.0, 72.0), &vec![at(0.0, 120.0)]);
        let slow = span((40.0, 72.0), &vec![at(0.0, 60.0)]);
        assert!(
            slow.0 < fast.0,
            "a slower song did not get a longer run-up: {slow:?} vs {fast:?}"
        );
        assert!((fast.0 - slow.0 - 4.0).abs() < 1e-9);
    }

    /// A tempo change at the section boundary gives the run-up and the
    /// tail different bar lengths — which is the case this exists for,
    /// since that is where songs change tempo.
    #[test]
    fn each_end_is_measured_in_its_own_tempo() {
        let tempo = vec![at(0.0, 120.0), at(72.0, 60.0)];
        let (from, to) = span((40.0, 72.0), &tempo);
        // Before the change: 2 bars at 120 = 4s. After: 1 bar at 60 = 4s.
        assert!((from - 36.0).abs() < 1e-9, "from {from}");
        assert!((to - 76.0).abs() < 1e-9, "to {to}");
    }

    /// The first section cannot show what came before it, and must not
    /// ask for a negative time.
    #[test]
    fn the_first_section_starts_at_the_start() {
        let (from, to) = span((0.0, 30.0), &vec![at(0.0, 120.0)]);
        assert!(from >= 0.0, "from {from}");
        assert!(to > 30.0);
    }

    /// A very short section is grown from the middle rather than being
    /// shown as a sliver.
    #[test]
    fn a_short_section_is_grown_around_its_middle() {
        let (from, to) = span((60.0, 61.0), &vec![at(0.0, 200.0)]);
        assert!(to - from >= SHORTEST - 1e-9, "{}", to - from);
        let middle = (from + to) / 2.0;
        assert!(
            (middle - 60.5).abs() < 1.5,
            "it drifted off centre: {middle}"
        );
    }

    /// The window fills the LANE — from its left edge to the right of
    /// the frame — with the track panel accounted for rather than
    /// eating the last bar.
    #[test]
    fn the_viewport_fits_the_window_to_the_lane() {
        let tempo = vec![at(0.0, 120.0)];
        let view = viewport((40.0, 72.0), &tempo, 1200.0, 800.0, 1.0);
        let (from, to) = span((40.0, 72.0), &tempo);
        // The lane's left edge shows the start of the window…
        let left = view.scroll_x / view.pps;
        assert!((left - from).abs() < 1e-6, "left {left} vs {from}");
        // …and the frame's right edge shows its end. The panel is
        // drawn over the first `TCP_WIDTH` of content, so the lane runs
        // from there to `width`.
        let lane = view.width - crate::arrangement::TCP_WIDTH;
        let right = (view.scroll_x + lane) / view.pps;
        assert!((right - to).abs() < 1e-6, "right {right} vs {to}");
        assert!(view.pps > 0.0);
        assert!(
            (view.width - 1200.0).abs() < f64::EPSILON,
            "the viewport stopped reporting the content width"
        );
    }

    /// A zero-width lane is a window being laid out, not a division by
    /// zero.
    #[test]
    fn a_lane_with_no_width_does_not_divide_by_it() {
        let view = viewport((0.0, 10.0), &vec![at(0.0, 120.0)], 0.0, 0.0, 1.0);
        assert!(view.pps.is_finite() && view.pps > 0.0);
    }

    /// The constants are what the doc says they are — two and one, not
    /// whatever they drifted to.
    #[test]
    fn the_window_is_the_one_that_was_asked_for() {
        assert!((BEFORE - 2.0).abs() < f64::EPSILON);
        assert!((AFTER - 1.0).abs() < f64::EPSILON);
    }
}
