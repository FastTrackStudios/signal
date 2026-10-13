//! The play cursor, the edit cursor, and the time selection.
//!
//! Three different things that all look like a vertical line, and
//! confusing them is the classic DAW bug:
//!
//! - the **play cursor** is where the audio is. It moves on its own and
//!   you cannot put it anywhere; you can only ask the transport to.
//! - the **edit cursor** is where YOU are. It moves when you click and
//!   stays where you left it.
//! - the **time selection** is a span, and it is what most commands act
//!   on when it exists.
//!
//! # Why the play cursor extrapolates
//!
//! The engine reports its position once per audio block — every few
//! milliseconds, and not on a schedule the display shares. Drawn
//! straight from that number, the cursor advances in steps of a block
//! however fast the window presents, which reads as stutter on a 144 Hz
//! display and is the whole reason REAPER users install
//! `ReaSmoothPlayhead` to replace the native one.
//!
//! So the position is a line, not a point: the last report, plus the
//! time since it arrived multiplied by the rate. Between blocks the
//! cursor glides; when the next block lands it corrects. The correction
//! is sub-pixel as long as the extrapolation is honest about the rate,
//! which is why [`Playhead::seek`] exists — a jump is told to the
//! cursor rather than inferred from a position that moved too far.

use std::time::Duration;
use web_time::Instant;

/// Where the audio is, as something that can be asked at any moment.
#[derive(Clone, Copy, Debug)]
pub struct Playhead {
    /// The last position the engine reported, in seconds.
    reported: f64,
    /// When it reported it.
    at: Instant,
    /// Seconds of project per second of wall clock. Zero when stopped;
    /// not always one, because a transport can play at half speed.
    rate: f64,
    playing: bool,
}

impl Playhead {
    #[must_use]
    pub fn stopped(at_seconds: f64) -> Self {
        Self {
            reported: at_seconds,
            at: Instant::now(),
            rate: 0.0,
            playing: false,
        }
    }

    /// A position from the engine.
    ///
    /// Called once per audio block. Everything between two of these is
    /// the extrapolation's job.
    pub fn report(&mut self, seconds: f64, rate: f64, now: Instant) {
        self.reported = seconds;
        self.at = now;
        self.rate = if self.playing { rate } else { 0.0 };
    }

    /// Start or stop moving.
    pub fn set_playing(&mut self, playing: bool, rate: f64, now: Instant) {
        // Freeze where the cursor IS, not where it was last reported —
        // stopping otherwise snaps it backwards by up to a block, which
        // is the most visible glitch a transport can have.
        self.reported = self.at_time(now);
        self.at = now;
        self.playing = playing;
        self.rate = if playing { rate } else { 0.0 };
    }

    /// Jump, without the jump being mistaken for playback.
    ///
    /// A seek reports a position far from the extrapolated one. Told
    /// about it, the cursor moves and carries on; left to infer it, the
    /// next frame would show a cursor that had apparently played several
    /// seconds in one frame.
    pub fn seek(&mut self, seconds: f64, now: Instant) {
        self.reported = seconds;
        self.at = now;
    }

    /// Where to draw it, now.
    #[must_use]
    pub fn at_time(&self, now: Instant) -> f64 {
        if !self.playing {
            return self.reported;
        }
        let elapsed = now.saturating_duration_since(self.at).as_secs_f64();
        // Capped, so a stalled engine does not send the cursor off down
        // the timeline. A gap this long is a dropout, and a cursor
        // frozen at the last real position says so where one sailing
        // away says nothing.
        let elapsed = elapsed.min(MAX_EXTRAPOLATION.as_secs_f64());
        self.rate.mul_add(elapsed, self.reported)
    }

    #[must_use]
    pub const fn playing(&self) -> bool {
        self.playing
    }
}

/// How far ahead of the last report the cursor will ever be drawn.
///
/// Generous next to an audio block (a few milliseconds) and short next
/// to a hang. Anything longer is not latency, it is the engine having
/// stopped talking.
pub const MAX_EXTRAPOLATION: Duration = Duration::from_millis(250);

/// A span of the timeline, in seconds.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Span {
    pub start: f64,
    pub end: f64,
}

impl Span {
    /// A span from two points, in either order.
    ///
    /// Dragging right-to-left is the same selection as left-to-right,
    /// and every consumer wants `start <= end`.
    #[must_use]
    pub fn between(a: f64, b: f64) -> Self {
        Self {
            start: a.min(b).max(0.0),
            end: a.max(b).max(0.0),
        }
    }

    #[must_use]
    pub fn length(self) -> f64 {
        self.end - self.start
    }

    /// Whether this is a real selection or an accidental click.
    ///
    /// A click that moves two pixels is a click, not a drag, and a
    /// selection of four milliseconds is one nobody meant to make —
    /// commands that act on the time selection would act on nothing.
    #[must_use]
    pub fn is_meaningful(self) -> bool {
        self.length() > MIN_SELECTION
    }
}

/// The shortest span treated as a selection rather than a click.
pub const MIN_SELECTION: f64 = 0.01;

/// Where the user is on the timeline.
#[derive(Clone, Copy, Debug, Default)]
pub struct Edit {
    /// The edit cursor, in seconds.
    pub at: f64,
    /// The time selection, when there is one.
    pub selection: Option<Span>,
}

impl Edit {
    /// Click: move the cursor, drop the selection.
    ///
    /// REAPER's behaviour, and the one that makes a stale selection
    /// impossible to leave lying around — a command acting on a
    /// selection you had forgotten is worse than one acting on nothing.
    pub fn click(&mut self, seconds: f64) {
        self.at = seconds.max(0.0);
        self.selection = None;
    }

    /// Drag: select, and take the cursor to the start.
    ///
    /// The cursor goes to the START rather than to where the mouse
    /// ended, because what you do next is usually play what you just
    /// selected.
    pub fn drag(&mut self, from: f64, to: f64) {
        let span = Span::between(from, to);
        if span.is_meaningful() {
            self.at = span.start;
            self.selection = Some(span);
        } else {
            self.click(from);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    /// The point of the whole module: between two engine reports the
    /// cursor keeps moving, so a 144 Hz window does not draw the same
    /// position several frames running.
    #[test]
    fn the_cursor_glides_between_reports() {
        let start = Instant::now();
        let mut head = Playhead::stopped(0.0);
        head.set_playing(true, 1.0, start);
        head.report(10.0, 1.0, start);

        let a = head.at_time(start + ms(3));
        let b = head.at_time(start + ms(6));
        assert!(a > 10.0 && b > a, "the cursor stalled: {a} then {b}");
        // One second of project per second of wall clock.
        assert!((b - 10.006).abs() < 1e-9, "drifted: {b}");
    }

    /// And it respects the rate — a transport at half speed moves the
    /// cursor at half speed, which a naive "advance by frame time" does
    /// not.
    #[test]
    fn it_follows_the_transport_rate() {
        let start = Instant::now();
        let mut head = Playhead::stopped(0.0);
        head.set_playing(true, 0.5, start);
        head.report(4.0, 0.5, start);
        assert!((head.at_time(start + ms(100)) - 4.05).abs() < 1e-9);
    }

    /// A stopped cursor does not move, however long the frame took.
    #[test]
    fn a_stopped_cursor_stays_put() {
        let start = Instant::now();
        let mut head = Playhead::stopped(7.5);
        assert!((head.at_time(start + ms(500)) - 7.5).abs() < f64::EPSILON);
        head.set_playing(true, 1.0, start);
        head.set_playing(false, 1.0, start + ms(200));
        let stopped_at = head.at_time(start + ms(200));
        assert!(
            (head.at_time(start + ms(900)) - stopped_at).abs() < f64::EPSILON,
            "a stopped cursor drifted"
        );
    }

    /// Stopping freezes the cursor where it was DRAWN, not where it was
    /// last reported — otherwise it jumps backwards by up to a block
    /// every time you hit stop.
    #[test]
    fn stopping_does_not_snap_backwards() {
        let start = Instant::now();
        let mut head = Playhead::stopped(0.0);
        head.set_playing(true, 1.0, start);
        head.report(2.0, 1.0, start);

        let shown = head.at_time(start + ms(40));
        head.set_playing(false, 1.0, start + ms(40));
        assert!(
            (head.at_time(start + ms(40)) - shown).abs() < 1e-9,
            "stop moved the cursor from {shown} to {}",
            head.at_time(start + ms(40))
        );
        assert!(shown > 2.0, "nothing was extrapolated to preserve");
    }

    /// An engine that stops reporting freezes the cursor rather than
    /// sending it down the timeline — a dropout should look like one.
    #[test]
    fn a_silent_engine_does_not_run_away() {
        let start = Instant::now();
        let mut head = Playhead::stopped(0.0);
        head.set_playing(true, 1.0, start);
        head.report(1.0, 1.0, start);

        let far = head.at_time(start + Duration::from_secs(30));
        assert!(
            far <= 1.0 + MAX_EXTRAPOLATION.as_secs_f64() + 1e-9,
            "ran away to {far}"
        );
    }

    /// A seek is told, not inferred.
    #[test]
    fn a_seek_moves_without_being_taken_for_playback() {
        let start = Instant::now();
        let mut head = Playhead::stopped(0.0);
        head.set_playing(true, 1.0, start);
        head.report(1.0, 1.0, start);
        head.seek(90.0, start + ms(5));
        assert!((head.at_time(start + ms(5)) - 90.0).abs() < 1e-9);
    }

    /// A span is the same span dragged either way.
    #[test]
    fn a_selection_has_no_direction() {
        assert_eq!(Span::between(9.0, 2.0), Span::between(2.0, 9.0));
        assert!((Span::between(9.0, 2.0).length() - 7.0).abs() < f64::EPSILON);
    }

    /// And it never starts before the project does.
    #[test]
    fn a_selection_cannot_be_negative() {
        let span = Span::between(-5.0, 3.0);
        assert!(span.start >= 0.0 && span.end >= 0.0);
    }

    /// A click clears the selection; a drag makes one and parks the
    /// cursor at its start, because what you do next is play it.
    #[test]
    fn clicking_clears_and_dragging_selects() {
        let mut edit = Edit::default();
        edit.drag(4.0, 9.0);
        assert_eq!(edit.selection, Some(Span::between(4.0, 9.0)));
        assert!((edit.at - 4.0).abs() < f64::EPSILON);

        edit.click(6.0);
        assert_eq!(edit.selection, None);
        assert!((edit.at - 6.0).abs() < f64::EPSILON);
    }

    /// A drag too short to mean anything is a click — otherwise every
    /// click leaves a four-millisecond selection behind for the next
    /// command to act on.
    #[test]
    fn a_twitch_is_a_click() {
        let mut edit = Edit::default();
        edit.drag(3.0, 3.0 + MIN_SELECTION / 2.0);
        assert_eq!(edit.selection, None);
        assert!((edit.at - 3.0).abs() < f64::EPSILON);
    }
}

/// Draw the edit cursor and the time selection.
///
/// Under the play cursor, always. The play cursor is where the audio
/// IS and the edit cursor is where you left off — when they coincide,
/// which is every time you press play from the cursor, the one that
/// moves has to be the one you can see.
pub fn paint_edit(
    painter: &mut impl anyrender::PaintScene,
    palette: &crate::arrangement::Palette,
    edit: &Edit,
    view: crate::arrangement::Viewport,
    origin: (f64, f64),
    top: f64,
    bottom: f64,
) {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::Fill;

    let (ox, _) = origin;
    // The lanes' left edge. Nothing here draws past it: scrolled right,
    // the cursor and the selection are off the left of the lanes, and
    // without the clamp they painted over the track panel instead.
    let left = ox + view.panel_w;
    let at = |seconds: f64| seconds.mul_add(view.pps, left - view.scroll_x);

    // The selection first, as a wash — it is a region, and a region
    // drawn over its own edges hides them.
    if let Some(span) = edit.selection {
        let (edge0, edge1) = (at(span.start), at(span.end));
        let (x0, x1) = (edge0.max(left), edge1.max(left));
        if x1 > x0 {
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                // Strong enough to see at a glance. 0.14 was
                // invisible against the lane backgrounds — a selection
                // you cannot see is one you forget you made, and the
                // next command acts on it.
                palette.accent.with_alpha(0.22),
                None,
                &Rect::new(x0, top, x1, bottom),
            );
            for edge in [edge0, edge1].into_iter().filter(|&edge| edge >= left) {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    palette.accent.with_alpha(0.85),
                    None,
                    &Rect::new(edge, top, edge + 1.0, bottom),
                );
            }
        }
    }

    let x = at(edit.at);
    if x < left {
        return;
    }
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        EDIT_CURSOR,
        None,
        &Rect::new(x, top, x + 1.0, bottom),
    );
}

/// The edit cursor's colour: blue (blue-500), where YOU are — against the
/// play cursor's yellow, where the audio is.
pub const EDIT_CURSOR: vello::peniko::Color =
    vello::peniko::Color::from_rgba8(0x3b, 0x82, 0xf6, 0xff);

/// How the play cursor looks.
///
/// The feature set `ReaSmoothPlayhead` documents — line, trail, shadow,
/// glow — implemented here rather than ported: that extension is closed
/// source and ships as Windows and macOS binaries, so there is nothing
/// to take but the description of what it does.
///
/// The smoothness is not in this struct. It is in [`Playhead`], because
/// it is not a look — a cursor that glides is telling the truth about
/// where the audio is between two reports, and one that steps is
/// rounding that down to the last block.
#[derive(Clone, Copy, Debug)]
pub struct Look {
    pub line: vello::peniko::Color,
    pub width: f64,
    /// How far the trail reaches BEHIND the cursor, in pixels.
    ///
    /// Behind, because the trail is where the audio has been. A trail
    /// ahead of the playhead would be claiming to know what has not
    /// been played.
    pub trail: f64,
    /// How far the shadow reaches, under everything else.
    pub shadow: f64,
    /// How far the glow bleeds behind the line.
    pub glow: f64,
    /// Opacity at the head of the trail (it falls off from there).
    pub trail_strength: f32,
    /// Opacity at the centre of the glow.
    pub glow_strength: f32,
}

impl Default for Look {
    fn default() -> Self {
        Self {
            // Yellow: where the audio is. The edit cursor is blue — see
            // `EDIT_CURSOR` — so the two never read as one another.
            line: vello::peniko::Color::from_rgba8(0xfa, 0xcc, 0x15, 0xff),
            width: 2.0,
            // A hint of where the audio has been, not a bar following
            // the line around.
            trail: 90.0,
            shadow: 0.0,
            glow: 6.0,
            trail_strength: 0.28,
            glow_strength: 0.5,
        }
    }
}

/// The play cursor's trail, from where it has actually been.
///
/// A trail drawn at a fixed length sits beside the line whether or not
/// anything moves — which is what this replaced. This one remembers the
/// play position against the wall clock and draws back to where the
/// cursor was a moment ago: so it drags out behind the cursor from where
/// it set off, reaches its full length once it has travelled that far,
/// and after a stop the tail keeps coming until it has caught the head.
/// A cursor at rest has no trail.
///
/// How far back "a moment" is depends on the zoom — the time it takes
/// the cursor to cover [`Look::trail`] pixels playing at speed — so the
/// trail is the same length on screen at any zoom, capped at
/// [`Trail::MAX_LAG`] so that far out it does not linger for seconds.
///
/// In project seconds rather than pixels, so a scroll or a zoom while
/// playing does not read as the cursor having moved.
#[derive(Clone, Debug, Default)]
pub struct Trail {
    /// (when, where), oldest first — back as far as `MAX_LAG` and no
    /// further.
    seen: std::collections::VecDeque<(Instant, f64)>,
}

impl Trail {
    /// The longest the tail ever lags the head, in wall-clock time.
    pub const MAX_LAG: Duration = Duration::from_millis(1500);

    /// Where the cursor is now. A jump — a seek, a loop back, a play
    /// from somewhere else — is not motion, so it starts the trail over
    /// rather than smearing it across the gap.
    ///
    /// The same position twice is NOT a stop: the engine moves the
    /// position once an audio block, and a frame can land between two.
    pub fn record(&mut self, now: Instant, at: f64) {
        if let Some(&(then, was)) = self.seen.back() {
            let elapsed = now.saturating_duration_since(then).as_secs_f64();
            // Faster than real time by more than a frame's slack, or
            // backwards at all: not the transport rolling.
            if at < was - 1e-4 || at - was > elapsed + 0.25 {
                self.seen.clear();
            }
        }
        self.seen.push_back((now, at));
        while self.seen.len() > 2
            && self
                .seen
                .get(1)
                .is_some_and(|&(t, _)| now.saturating_duration_since(t) >= Self::MAX_LAG)
        {
            self.seen.pop_front();
        }
    }

    /// Where the cursor was `lag` before `now` — or where the trail's
    /// memory starts, which is where the cursor set off from.
    #[must_use]
    pub fn tail(&self, now: Instant, lag: Duration) -> Option<f64> {
        let (&(t0, x0), rest) = (self.seen.front()?, self.seen.iter().skip(1));
        let Some(horizon) = now.checked_sub(lag) else {
            return Some(x0);
        };
        if t0 >= horizon {
            return Some(x0);
        }
        let mut before = (t0, x0);
        for &(t, x) in rest {
            if t >= horizon {
                let span = t.saturating_duration_since(before.0).as_secs_f64();
                let into = horizon.saturating_duration_since(before.0).as_secs_f64();
                let f = if span > 0.0 { into / span } else { 1.0 };
                return Some(f.mul_add(x - before.1, before.1));
            }
            before = (t, x);
        }
        Some(before.1)
    }

    /// How long the trail is on screen, in pixels, for a full trail of
    /// `full` pixels at `pps`.
    #[must_use]
    pub fn length(&self, now: Instant, pps: f64, full: f64) -> f64 {
        if pps <= 0.0 || full <= 0.0 {
            return 0.0;
        }
        let lag = Duration::from_secs_f64((full / pps).min(Self::MAX_LAG.as_secs_f64()));
        let (Some(&(_, head)), Some(tail)) = (self.seen.back(), self.tail(now, lag)) else {
            return 0.0;
        };
        ((head - tail) * pps).clamp(0.0, full)
    }

    /// Whether there is still a trail to draw — what keeps frames coming
    /// while the tail catches up after a stop.
    #[must_use]
    pub fn alive(&self, now: Instant) -> bool {
        let (Some(&(_, head)), Some(tail)) = (self.seen.back(), self.tail(now, Self::MAX_LAG))
        else {
            return false;
        };
        head - tail > 1e-6
    }
}

impl Look {
    /// This look with its trail `length` pixels long — what
    /// [`Trail::length`] says the cursor has dragged out behind it. The
    /// glow on the line is the line's and stays as it is.
    #[must_use]
    pub fn trailing(self, length: f64) -> Self {
        Self {
            trail: length.clamp(0.0, self.trail),
            ..self
        }
    }
}

/// How many stops a gradient is built from.
///
/// A quadratic falloff is a curve and a gradient is a polyline through
/// it, so this is how round the corner looks. Eight is past the point
/// where more of them changes the picture at these sizes.
const FALLOFF_STOPS: usize = 8;

/// The opacity of a trail `t` of the way from the cursor to its end.
///
/// **Quadratic**, not linear: `(1 - t)²`. A linear trail reads as a
/// solid bar with a hard end, because the eye tracks the end of a ramp
/// rather than its middle. Squaring puts most of the falloff near the
/// cursor, which is what makes it read as motion instead of as a
/// rectangle following the line around.
#[must_use]
pub fn falloff(t: f64) -> f32 {
    let t = t.clamp(0.0, 1.0);
    let remaining = 1.0 - t;
    #[expect(
        clippy::cast_possible_truncation,
        clippy::as_conversions,
        reason = "an opacity in 0..1, which every gradient API takes as f32"
    )]
    let alpha = (remaining * remaining) as f32;
    alpha
}

/// Draw the play cursor at `x`, from `top` to `bottom`.
///
/// Order matters and is the reverse of how it is read: shadow, then
/// trail, then glow, then the line. Each is wider than the one over it,
/// so drawing them the other way round would bury the line under its
/// own halo.
pub fn paint(
    painter: &mut impl anyrender::PaintScene,
    look: Look,
    x: f64,
    top: f64,
    bottom: f64,
    // `left_bound` is the left edge of the lanes; nothing draws past
    // it. The trail reaches backwards, and at the start of a project
    // that is straight over the track panel — a red wash across the
    // names and faders, which reads as damage rather than as motion.
    // The cursor belongs to the timeline, so it stops where the
    // timeline does.
    left_bound: f64,
) {
    use vello::kurbo::{Affine, Rect};
    use vello::peniko::{ColorStop, ColorStops, Fill, Gradient};

    if bottom <= top {
        return;
    }

    // A horizontal ramp from the line's colour at the cursor to nothing
    // at the far end, sampled off the quadratic.
    let mut ramp = |from: f64, to: f64, peak: f32| {
        let (lo, hi) = (from.min(to).max(left_bound), from.max(to).max(left_bound));
        if hi - lo < 0.5 {
            return;
        }
        let mut stops = ColorStops::new();
        for i in 0..=FALLOFF_STOPS {
            let t = crate::num::coord(i) / crate::num::coord(FALLOFF_STOPS);
            #[expect(
                clippy::cast_possible_truncation,
                clippy::as_conversions,
                reason = "a gradient offset in 0..1, which peniko takes as f32"
            )]
            let offset = t as f32;
            stops.push(ColorStop {
                offset,
                color: look.line.with_alpha(falloff(t) * peak).into(),
            });
        }
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            // The gradient keeps its ORIGINAL span so the falloff is
            // the same shape when it is clipped — anchoring it to the
            // clipped rect instead would compress the whole trail into
            // whatever was left, and the cursor would look different
            // at the start of a project than in the middle of one.
            &Gradient::new_linear((from, top), (to, top)).with_stops(stops),
            None,
            &Rect::new(lo, top, hi, bottom),
        );
    };

    // The shadow first and widest, then the trail over it.
    if look.shadow > 0.0 {
        ramp(x, x - look.shadow, 0.35);
    }
    if look.trail > 0.0 {
        ramp(x, x - look.trail, look.trail_strength);
    }
    // The glow bleeds behind the line only: ahead of the cursor is
    // audio that has not played, and a halo there reads as the cursor
    // being further on than it is.
    if look.glow > 0.0 {
        ramp(x, x - look.glow, look.glow_strength);
    }

    let line_left = (x - look.width / 2.0).max(left_bound);
    let line_right = (x + look.width / 2.0).max(left_bound);
    if line_right > line_left {
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            look.line,
            None,
            &Rect::new(line_left, top, line_right, bottom),
        );
    }
}

#[cfg(test)]
mod look_tests {
    use super::{FALLOFF_STOPS, Look, falloff, paint};

    /// Quadratic, and anchored: full at the cursor, nothing at the end.
    #[test]
    fn the_trail_falls_off_quadratically() {
        assert!((falloff(0.0) - 1.0).abs() < 1e-6);
        assert!(falloff(1.0).abs() < 1e-6);
        // Half way along, a linear ramp would be 0.5; squared it is
        // 0.25 — which is the whole difference between a bar and a
        // trail.
        assert!((falloff(0.5) - 0.25).abs() < 1e-6);
    }

    /// And it is monotonic, so the trail never brightens as it recedes.
    #[test]
    fn the_trail_only_fades() {
        let mut last = f32::INFINITY;
        for i in 0..=20 {
            let a = falloff(f64::from(i) / 20.0);
            assert!(a <= last + 1e-6, "brightened at {i}: {a} after {last}");
            last = a;
        }
    }

    /// Out-of-range inputs are clamped rather than producing a negative
    /// or greater-than-one alpha, which peniko would take literally.
    #[test]
    fn the_falloff_is_clamped() {
        assert!((falloff(-3.0) - 1.0).abs() < 1e-6);
        assert!(falloff(9.0).abs() < 1e-6);
    }

    /// A zero-height cursor draws nothing rather than an inverted rect.
    #[test]
    fn an_empty_span_draws_nothing() {
        let mut scene = anyrender::Scene::new();
        paint(&mut scene, Look::default(), 100.0, 500.0, 500.0, 0.0);
        assert!(scene.commands.is_empty());
    }

    /// Everything switched off still draws the line — the one part
    /// that is not decoration.
    #[test]
    fn the_line_survives_every_effect_being_off() {
        let mut scene = anyrender::Scene::new();
        paint(
            &mut scene,
            Look {
                trail: 0.0,
                shadow: 0.0,
                glow: 0.0,
                ..Look::default()
            },
            100.0,
            0.0,
            900.0,
            0.0,
        );
        assert_eq!(scene.commands.len(), 1, "expected just the line");
    }

    /// And with them on, each is its own pass.
    #[test]
    fn each_effect_is_drawn() {
        let mut scene = anyrender::Scene::new();
        paint(
            &mut scene,
            Look {
                shadow: 200.0,
                ..Look::default()
            },
            300.0,
            0.0,
            900.0,
            0.0,
        );
        // shadow + trail + the glow behind + the line.
        assert_eq!(scene.commands.len(), 4);
        assert!(FALLOFF_STOPS >= 4, "a curve needs stops to be a curve");
    }

    /// The trail stops at the lanes. At the start of a project it
    /// would otherwise wash backwards over the track panel, which
    /// reads as damage rather than as motion.
    #[test]
    fn nothing_draws_left_of_the_lanes() {
        let mut scene = anyrender::Scene::new();
        // The cursor AT the boundary: every effect reaches backwards
        // from here, and all of it is out of bounds.
        paint(&mut scene, Look::default(), 400.0, 0.0, 900.0, 400.0);
        // The line straddles the boundary, so half of it survives;
        // the trail, shadow and left glow have nowhere to go.
        assert!(
            scene.commands.len() <= 2,
            "something drew past the lane edge: {} commands",
            scene.commands.len()
        );
    }

    /// And well inside the lanes it draws everything.
    #[test]
    fn a_cursor_in_the_lanes_keeps_its_trail() {
        let mut scene = anyrender::Scene::new();
        paint(
            &mut scene,
            Look {
                shadow: 200.0,
                ..Look::default()
            },
            900.0,
            0.0,
            900.0,
            400.0,
        );
        // shadow + trail + the glow behind + the line.
        assert_eq!(scene.commands.len(), 4);
    }
}

#[cfg(test)]
mod trail_tests {
    use super::Trail;
    use std::time::Duration;
    use web_time::Instant;

    const PPS: f64 = 100.0;
    const FULL: f64 = 90.0;

    /// A transport rolling at real time for `secs`, a frame at a time —
    /// its position moving once per audio block, so some frames see the
    /// same position twice, as the window does.
    fn roll(trail: &mut Trail, from: Instant, at: f64, secs: f64) -> (Instant, f64) {
        let frame = 1.0 / 120.0;
        let block = 512.0 / 48_000.0;
        let (mut now, mut clock) = (from, 0.0_f64);
        for _ in 0..(secs / frame).round() as usize {
            now += Duration::from_secs_f64(frame);
            clock += frame;
            trail.record(now, at + (clock / block).floor() * block);
        }
        (now, at + (clock / block).floor() * block)
    }

    #[test]
    fn drags_out_from_where_it_set_off() {
        let start = Instant::now();
        let mut trail = Trail::default();
        trail.record(start, 4.0);
        let rest = start + Duration::from_secs(1);
        trail.record(rest, 4.0);
        assert!(trail.length(rest, PPS, FULL) < 1e-9, "a trail at rest");
        let (now, pos) = roll(&mut trail, rest, 4.0, 0.3);
        let early = trail.length(now, PPS, FULL);
        // As far as it has come, and no further: it was never behind 4.0.
        assert!(
            (early - (pos - 4.0) * PPS).abs() < 1.5,
            "{early} vs {}",
            (pos - 4.0) * PPS
        );
        let (now, _) = roll(&mut trail, now, pos, 1.0);
        assert!(
            (trail.length(now, PPS, FULL) - FULL).abs() < 1.5,
            "not at full length"
        );
    }

    #[test]
    fn the_tail_catches_up_after_a_stop() {
        let start = Instant::now();
        let mut trail = Trail::default();
        trail.record(start, 0.0);
        let (mut now, pos) = roll(&mut trail, start, 0.0, 2.0);
        let playing = trail.length(now, PPS, FULL);
        now += Duration::from_millis(300);
        trail.record(now, pos);
        let shrinking = trail.length(now, PPS, FULL);
        assert!(
            shrinking > 0.0 && shrinking < playing,
            "{shrinking} of {playing}"
        );
        now += Trail::MAX_LAG;
        trail.record(now, pos);
        assert!(trail.length(now, PPS, FULL) < 1e-9);
        assert!(!trail.alive(now), "still a trail long after the stop");
    }

    #[test]
    fn a_seek_is_not_motion() {
        let start = Instant::now();
        let mut trail = Trail::default();
        trail.record(start, 0.0);
        let (now, _) = roll(&mut trail, start, 0.0, 1.0);
        let now = now + Duration::from_millis(8);
        trail.record(now, 60.0);
        assert!(trail.length(now, PPS, FULL) < 1e-9);
        let now = now + Duration::from_millis(8);
        trail.record(now, 10.0);
        assert!(trail.length(now, PPS, FULL) < 1e-9, "a jump back smeared");
    }
}

/// The arrangement's edit cursor and time selection, as last drawn — for
/// what acts on them from outside the arrangement (the Organize toolbar's
/// inserts, which go through the engine and read ITS cursor and
/// selection, so they are handed these first).
static CURRENT: std::sync::Mutex<Option<Edit>> = std::sync::Mutex::new(None);

/// Publish the arrangement's edit state (every frame it draws).
pub fn publish(edit: Edit) {
    if let Ok(mut slot) = CURRENT.lock() {
        *slot = Some(edit);
    }
}

/// The arrangement's edit state as last drawn, if it has drawn.
#[must_use]
pub fn current() -> Option<Edit> {
    CURRENT.lock().ok().and_then(|slot| *slot)
}

/// An edit state asked for from outside the arrangement, until it takes
/// it ([`take_asked`]).
static ASKED: std::sync::Mutex<Option<Edit>> = std::sync::Mutex::new(None);

/// Put the arrangement's edit cursor and time selection at `edit`.
///
/// What an action that sets them (`TIME_SELECTION_SET`, an insert that moves
/// the cursor on) asks. It is [`current`] at once, and the arrangement
/// takes it on its next frame.
pub fn ask(edit: Edit) {
    publish(edit);
    if let Ok(mut slot) = ASKED.lock() {
        *slot = Some(edit);
    }
}

/// The edit state asked for since the arrangement last looked, once.
#[must_use]
pub fn take_asked() -> Option<Edit> {
    ASKED.lock().ok().and_then(|mut slot| slot.take())
}
