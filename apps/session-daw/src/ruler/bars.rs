//! The bar grid down the lanes and the bar numbers along the ruler's foot:
//! which divisions a zoom shows, and how a measure is written.

use super::*;

/// The project's bar grid.
///
/// One tempo and one time signature: true of most sessions and wrong for
/// some. A tempo map belongs here when the project model grows one — the
/// shape of this struct would change, not its callers.
#[derive(Clone, Copy, Debug)]
pub struct Bars {
    pub secs_per_beat: f64,
    pub beats_per_bar: f64,
}

impl Bars {
    /// From a tempo, in 4/4.
    #[must_use]
    pub fn at(bpm: f64) -> Self {
        Self::in_time(bpm, 4)
    }

    /// From a tempo and a signature's top number.
    ///
    /// The top number is what a bar is counted in: three beats to a bar
    /// in three four, seven in seven eight. A ruler that assumed four
    /// put every bar line after the first in the wrong place for
    /// anything else — and a grid that disagrees with the music is one
    /// you edit against at your peril.
    #[must_use]
    pub fn in_time(bpm: f64, beats_per_bar: u32) -> Self {
        Self {
            secs_per_beat: 60.0 / if bpm > 0.0 { bpm } else { 120.0 },
            beats_per_bar: f64::from(beats_per_bar.max(1)),
        }
    }

    /// The bars at a time, from the project's tempo map.
    ///
    /// The last change at or before `seconds` wins, which is what a
    /// tempo map means.
    #[must_use]
    pub fn at_time(tempo: &[daw_ui::studio::project::TempoChange], seconds: f64) -> Self {
        tempo
            .iter()
            .take_while(|change| change.at <= seconds + 1e-9)
            .last()
            .map_or_else(
                || Self::at(120.0),
                |change| Self::in_time(change.bpm, change.beats_per_bar),
            )
    }

    #[must_use]
    pub fn secs_per_bar(self) -> f64 {
        self.secs_per_beat * self.beats_per_bar
    }
}

/// Draw the grid lines across the lanes.
///
/// Drawn over the lanes rather than under them, at a low alpha. Under
/// would mean splitting the recorded scene into a layer below the items
/// and one above, and a grid that reads THROUGH an item is what REAPER's
/// looks like anyway.
pub fn grid(
    painter: &mut impl PaintScene,
    palette: &Palette,
    view: Viewport,
    bars: Bars,
    adaptive: &Adaptive,
    finest: f64,
    origin: (f64, f64),
) {
    // The main toolbar's grid-lines switch.
    if !crate::options::GRID.get() {
        return;
    }
    let measure_px = bars.secs_per_bar() * view.pps;
    if measure_px <= 0.0 {
        return;
    }
    let (from, to) = view.secs();
    let from = from.max(0.0);

    // Three weights, so a bar's start, its beats and what is between them
    // each read for what they are: the subdivisions faint, the beats
    // plainer, the bars strongest and wider.
    let beat_px = bars.secs_per_beat * view.pps;
    // A Grid region's stretch is drawn on its own grid: its lines, at a
    // beat's weight, in place of the beats and the subdivisions there.
    let regions = grid_regions();
    if let Some(in_force) = crate::grid_in_force::current() {
        // Editing: the quantize's grid where it is in force, not what the
        // zoom fits — four bars across fit 32nds no part plays.
        for (start, end, division) in in_force.between(from, to) {
            let step = bars.secs_per_beat * 4.0 / f64::from(division.max(1));
            if step < bars.secs_per_beat - 1e-9 && step * view.pps >= GRID_SUB_ROOM {
                line_every(
                    painter,
                    view,
                    (step, start, end),
                    (palette.grid_sub, 1.0),
                    origin,
                    &regions,
                );
            }
        }
    } else if let Some(division) = adaptive.fit(finest, measure_px) {
        let step = division * 4.0 * bars.secs_per_beat;
        // Finer than a beat: its own lines, under the beats'.
        if step > 0.0 && step < bars.secs_per_beat - 1e-9 {
            line_every(
                painter,
                view,
                (step, from, to),
                (palette.grid_sub, 1.0),
                origin,
                &regions,
            );
        }
    }
    if beat_px >= GRID_BEAT_ROOM {
        line_every(
            painter,
            view,
            (bars.secs_per_beat, from, to),
            (palette.grid_beat, 1.0),
            origin,
            &regions,
        );
    }
    for &(start, end, division) in &regions {
        let step = bars.secs_per_beat * 4.0 / f64::from(division.max(1));
        if step * view.pps >= GRID_BEAT_ROOM / 2.0 {
            line_every(
                painter,
                view,
                (step, from.max(start), to.min(end)),
                (palette.grid_beat, 1.0),
                origin,
                &[],
            );
        }
    }
    // Bar lines always, and brightest: they are what the numbers above
    // count, so they have to survive whatever the division does.
    let bar_width = if measure_px >= GRID_WIDE_BAR {
        2.0
    } else {
        1.0
    };
    line_every(
        painter,
        view,
        (bars.secs_per_bar(), from, to),
        (palette.grid, bar_width),
        origin,
        &[],
    );
}

/// How far apart beats must be for the grid to draw their lines, px.
const GRID_BEAT_ROOM: f64 = 6.0;

/// How wide a bar must be for its line to be drawn two pixels wide, px.
const GRID_WIDE_BAR: f64 = 80.0;

/// The ruler strip: its ground, its ticks and its bar numbers.
///
/// Painted last and opaque, so the lanes scrolled under it are covered
/// rather than clipped — one strip of overdraw against a clip layer per
/// frame, and the strip has to be drawn either way.
pub fn ruler(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &Font,
    view: Viewport,
    (tempo, song_start): (&[daw_ui::studio::project::TempoChange], Option<f64>),
    origin: (f64, f64),
) {
    // The rails frame the view, so the ruler starts where they leave
    // off. Drawn from the window's own corner it sat under the top
    // rail, and the strip of it that showed below read as a seam.
    let (ox, oy) = origin;
    fill(
        painter,
        palette.ruler_bg,
        Rect::new(ox, oy, ox + view.width, oy + ruler_h()),
    );
    fill(
        painter,
        palette.tcp_rule,
        Rect::new(ox, oy + ruler_h() - 1.0, ox + view.width, oy + ruler_h()),
    );
    // The bars are the bottom of the strip; the tempo sits just above
    // them and the lanes over that.
    let oy = oy + ruler_h() - bars_h();
    // A slim ruler's numbers are small, close under its sections.
    let (number_y, number_size) = if slim() { (10.0, 8.5) } else { (14.0, 11.0) };

    let (from, to) = view.secs();
    // Every beat up to the right edge, counted through the tempo map
    // rather than multiplied from one tempo. A number has to sit on the
    // bar line it names, and after a tempo change a multiplied grid
    // does not.
    // How wide a bar is where the view starts. Asked BEFORE the walk,
    // because a zoom far enough out makes a bar narrower than a pixel
    // and there is then nothing to draw — and `view.secs()` at that
    // zoom asks for a range measured in centuries, which is a walk
    // that never returns.
    let start_bars = Bars::at_time(tempo, from.max(0.0));
    let bar_px = start_bars.secs_per_bar() * view.pps;
    if bar_px < 2.0 {
        return;
    }
    // Far enough to reach the song's start, wherever the view is.
    let beats = Timeline::new(tempo).beats(to.max(song_start.unwrap_or(0.0)), MAX_BEATS);
    let Some(first) = beats.first() else {
        return;
    };
    // The bar the song starts in is bar 1.
    let bar_one = song_start
        .and_then(|at| beats.iter().rev().find(|b| b.at <= at + 1e-6))
        .map_or(1, |b| b.measure);
    // Measured at the start rather than averaged: a ruler whose
    // spacing changed mid-screen would be harder to read than one
    // slightly too dense at one end.
    let beat_px = bar_px / f64::from(first.per_bar.max(1));
    let bars_apart = bars_between_numbers(bar_px);
    let beat_numbers = beat_px >= BEAT_ROOM;
    let subs = subdivisions(beat_px);
    // Three weights, so where a bar starts reads at a glance: the bar's
    // line the full height and its number bright and large, each beat a
    // shorter line and a small number, the subdivisions short faint ticks
    // with no number at all.
    let height = bars_h();
    let x_of = |t: f64| t.mul_add(view.pps, view.panel_w - view.scroll_x);
    let shown = |x: f64| x >= view.panel_w - 1.0 && x <= view.width;
    let tick = |painter: &mut _, x: f64, tall: f64, color: Color, width: f64| {
        fill(
            painter,
            color,
            Rect::new(ox + x, oy + height - tall, ox + x + width, oy + height),
        );
    };
    // A Grid region's stretch ticked on its own grid, its beats' and
    // subdivisions' ticks left out (its bars kept).
    let regions = grid_regions();
    for beat in &beats {
        if beat.at > to {
            break;
        }
        if beat.at + beat.secs_per_beat < from {
            continue;
        }
        if let Some(n) = subs {
            for k in 1..n {
                let t = (f64::from(k) / f64::from(n)).mul_add(beat.secs_per_beat, beat.at);
                let x = x_of(t);
                if shown(x) && !in_grid_region(&regions, t) {
                    tick(
                        painter,
                        x,
                        height * SUB_TICK,
                        palette.ruler_fg2.multiply_alpha(0.6),
                        1.0,
                    );
                }
            }
        }
        let x = x_of(beat.at);
        if !shown(x) {
            continue;
        }
        let bar = from_song(beat.measure, bar_one);
        if beat.beat == 1 {
            let numbered = i64::from(beat.measure)
                .saturating_sub(i64::from(bar_one))
                .rem_euclid(bars_apart)
                == 0;
            let tall = if numbered { height } else { height * BEAT_TICK };
            tick(
                painter,
                x,
                tall,
                palette.ruler_fg,
                if numbered { 2.0 } else { 1.0 },
            );
            if numbered {
                crate::tcp::glyphs(
                    painter,
                    font,
                    palette.ruler_fg,
                    &bar.to_string(),
                    // Close to its own line, not floating between two.
                    ox + x + 3.0,
                    oy + number_y,
                    number_size + 1.5,
                );
            }
        } else if beat_px >= 4.0 && !in_grid_region(&regions, beat.at) {
            tick(painter, x, height * BEAT_TICK, palette.ruler_fg2, 1.0);
            if beat_numbers {
                crate::tcp::glyphs(
                    painter,
                    font,
                    palette.ruler_fg2,
                    &beat.beat.to_string(),
                    ox + x + 2.0,
                    oy + number_y,
                    number_size - 1.5,
                );
            }
        }
    }
    for &(start, end, division) in &regions {
        let step = start_bars.secs_per_beat * 4.0 / f64::from(division.max(1));
        if step * view.pps < SUB_ROOM {
            continue;
        }
        let first = (start.max(from) / step).ceil();
        let lines = ((end.min(to) / step).ceil() - first).max(0.0);
        for n in counts(
            num_traits::ToPrimitive::to_usize(&lines)
                .unwrap_or(0)
                .min(MAX_TICKS),
        ) {
            let t = (first + n) * step;
            let x = x_of(t);
            if !shown(x) {
                continue;
            }
            tick(painter, x, height * BEAT_TICK, palette.accent, 1.0);
            // Numbered as the ruler numbers a position — `78.3`,
            // `78.3.667` — where the lines leave room, a bar's own line
            // already numbered above.
            let at = beats.partition_point(|b| b.at <= t + 1e-6);
            let Some(beat) = at.checked_sub(1).and_then(|i| beats.get(i)) else {
                continue;
            };
            let fraction = (t - beat.at) / beat.secs_per_beat.max(f64::EPSILON);
            let on_the_bar = beat.beat == 1 && fraction < 1e-3;
            if step * view.pps >= REGION_NUMBER_ROOM && !on_the_bar {
                crate::tcp::glyphs(
                    painter,
                    font,
                    palette.accent,
                    &position(from_song(beat.measure, bar_one), beat.beat, fraction),
                    ox + x + 2.0,
                    oy + number_y,
                    number_size - 1.5,
                );
            }
        }
    }
}

/// How far apart a Grid region's lines must be for each to be numbered,
/// px.
const REGION_NUMBER_ROOM: f64 = 56.0;

/// A position as the ruler writes one: `78.3`, a fraction of a beat on in
/// thousandths, `78.3.667`.
fn position(bar: i64, beat: u32, fraction: f64) -> String {
    let sub = (fraction * 1000.0).round();
    if (0.5..999.5).contains(&sub) {
        format!("{bar}.{beat}.{sub:03.0}")
    } else {
        format!("{bar}.{beat}")
    }
}

/// The most ticks a Grid region draws: a screen's worth.
const MAX_TICKS: usize = 2048;

/// How tall a beat's line is on the ruler, of its height.
const BEAT_TICK: f64 = 0.45;

/// How tall a subdivision's tick is, of the ruler's height.
const SUB_TICK: f64 = 0.22;

/// How wide a beat must be for its number, px.
pub(super) const BEAT_ROOM: f64 = 26.0;

/// How wide a bar must be between numbered bars, px.
pub(super) const BAR_ROOM: f64 = 36.0;

/// How far apart a subdivision's ticks must be, px.
pub(super) const SUB_ROOM: f64 = 7.0;

/// The least room between the grid's lines in force, pixels.
const GRID_SUB_ROOM: f64 = 4.0;

/// How many bars between numbered bars, for a bar this wide.
#[must_use]
pub fn bars_between_numbers(bar_px: f64) -> i64 {
    BAR_STEPS
        .into_iter()
        .find(|n| bar_px * f64::from(*n) >= BAR_ROOM)
        .map_or(256, i64::from)
}

/// How many parts a beat is ticked in — sixteenths (4) or eighths (2) of
/// a quarter-note beat, the finest whose ticks stay apart — `None` when
/// even eighths would crowd.
#[must_use]
pub fn subdivisions(beat_px: f64) -> Option<u32> {
    [4_u32, 2]
        .into_iter()
        .find(|n| beat_px / f64::from(*n) >= SUB_ROOM)
}

/// A measure counted from the song's: `bar_one` (a measure counted from
/// the project's start) is bar 1, the ones after it 2, 3 …, the ones
/// before it −1, −2 … — the count-in counting down to the song, with no
/// bar 0, as REAPER numbers a measure offset.
#[must_use]
pub fn from_song(measure: u32, bar_one: u32) -> i64 {
    let shown = i64::from(measure) - i64::from(bar_one) + 1;
    if shown <= 0 { shown - 1 } else { shown }
}

/// Where `project`'s song starts: its `SONGSTART` marker, the ruler's
/// bar 1.
#[must_use]
pub fn song_start_of(project: &daw_ui::studio::project::Project) -> Option<f64> {
    project
        .markers
        .iter()
        .find(|m| m.name == "SONGSTART")
        .map(|m| m.at)
}

/// The bar and beat at `at`, counted from the song as the ruler counts
/// them ([`from_song`]).
#[must_use]
pub fn song_bar_beat(
    tempo: &[daw_ui::studio::project::TempoChange],
    song_start: Option<f64>,
    at: f64,
) -> (i64, u32) {
    let beats = Timeline::new(tempo).beats(at.max(song_start.unwrap_or(0.0)) + 1e-6, MAX_BEATS);
    let bar_one = song_start
        .and_then(|start| beats.iter().rev().find(|b| b.at <= start + 1e-6))
        .map_or(1, |b| b.measure);
    beats
        .iter()
        .rev()
        .find(|b| b.at <= at + 1e-6)
        .map_or((1, 1), |b| (from_song(b.measure, bar_one), b.beat))
}

/// How many beats a ruler will count before it gives up.
///
/// A tempo map cannot be trusted to be sane — a zero or a negative bpm
/// would make the walk stand still — and a ruler is not the place to
/// find that out by hanging.
///
/// Well above any real session: an hour of sixteenth notes at 200 bpm
/// is under fifty thousand. The guard above is what keeps an absurd
/// zoom from asking for them at all.
pub(super) const MAX_BEATS: usize = 100_000;

/// How many bars between numbers once a beat is too fine to label.
pub(super) const BAR_STEPS: [i32; 8] = [1, 2, 4, 8, 16, 32, 64, 128];
