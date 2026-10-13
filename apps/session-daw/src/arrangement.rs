//! The arrangement as a recorded scene.
//!
//! Built once, in CONTENT space — seconds along the timeline, row
//! indices down it — and replayed every frame with a transform that
//! carries the scroll and the zoom. `anyrender::Scene` records the
//! commands; `PaintScene::append_scene` replays them into whatever
//! backend is drawing, which on the desktop is Vello on the GPU.
//!
//! That split is the whole design. The scene is rebuilt when the DATA
//! changes — a project opening, a folder collapsing — and never on a
//! scroll, a zoom or a playhead tick. Those are a different `Affine`,
//! not a different scene.
//!
//! # One scene for the panel as well as the lanes
//!
//! Because they must not be able to disagree. The web-view version drew
//! the lanes on a canvas while the track panel stayed in the DOM, and
//! they visibly tore apart while scrolling: native scroll moves DOM
//! content on the compositor, a canvas redraws on the main thread, and
//! nothing reconciles the two. Recorded together and replayed under one
//! transform, being out of sync is not a bug that can happen.

use anyrender::recording::RenderCommand;
use anyrender::{PaintScene, Scene};
use vello::kurbo::{Affine, BezPath, Rect};
use vello::peniko::{Color, Fill};

use daw_ui::studio::{ProjectRef, RowsRef};

use crate::profile::Counts;

/// The divider under every row.
///
/// Part of the row's height rather than added to it, so a track set to
/// 24 in REAPER occupies 24 here too. Adding it would make every row a
/// pixel taller than the project says, and two thousand of those is a
/// hundred and forty pixels of drift down the session.
pub const DIVIDER: f64 = 1.0;
/// The panel's width, from the measured REAPER geometry.
pub const TCP_WIDTH: f64 = 343.0;

/// The resolved theme, as the art crate's control palette.
///
/// Started from the art's own defaults and overridden rather than built
/// field by field: `Chrome` carries relationships that were measured
/// together — the hardware face, its edge and its mark — and a palette
/// assembled from scratch would silently lose the ones this theme has
/// nothing to say about.
fn chrome(theme: &daw_ui::theming::Theme) -> daw_theme::Chrome {
    let c = |col: daw_ui::theming::Color| daw_theme::Color {
        r: col.r,
        g: col.g,
        b: col.b,
        a: col.a,
    };
    daw_theme::Chrome {
        surface: c(theme.tokens.surface),
        surface_raised: c(theme.tokens.surface_raised),
        surface_sunken: c(theme.tokens.surface_sunken),
        border: c(theme.tokens.border),
        text: c(theme.tokens.text),
        text_dim: c(theme.tokens.text_dim),
        text_faint: c(theme.tokens.text_faint),
        accent: c(theme.tokens.accent),
        // A control's face, its edge and the ink on it. The buttons read
        // as part of the panel when these come off the same ladder the
        // panel does, and as borrowed art when they do not.
        //
        hardware: c(theme.tokens.surface_raised),
        hardware_edge: c(theme.tokens.border),
        // The art prints on this and rings with it — the record arm's
        // unlit ring is #a6a6a6 in the source, and a control FACE, not a
        // label. Mapped to `text_dim` alone it came out dark enough to
        // vanish against its own housing, which is why the fader's grip
        // needed hand-brightening at the call site.
        hardware_mark: c(theme.tokens.text_dim).mix(c(theme.tokens.text), 0.5),
        ..daw_theme::Theme::default().chrome
    }
}

/// Colours resolved once, so the draw loop never parses a hex string.
#[derive(Clone)]
pub struct Palette {
    pub surface: Color,
    pub row_a: Color,
    pub row_b: Color,
    pub divider: Color,
    pub grid: Color,
    /// The sub-bar lines, dimmer than the bar lines they sit between.
    pub grid_beat: Color,
    /// The grid's lines finer than a beat.
    pub grid_sub: Color,
    pub ruler_bg: Color,
    pub ruler_fg: Color,
    /// The ruler's quieter ink: beats and subdivisions.
    pub ruler_fg2: Color,
    pub item_edge: Color,
    pub text: Color,
    pub text_dim: Color,
    pub text_faint: Color,
    pub accent: Color,
    pub mute: Color,
    pub solo: Color,
    pub rec: Color,
    pub meter_safe: Color,
    pub meter_warn: Color,
    /// Pan's own colour — yellow, so it is not mistaken for volume's
    /// blue in the column beside it.
    pub pan: Color,
    pub meter_danger: Color,
    /// The track panel's own surfaces. Named `tcp_*` because they come
    /// from the theme's TCP context, which a REAPER theme colours
    /// separately from the arrange view — a panel drawn in arrange
    /// colours is the giveaway that a theme was only half applied.
    pub tcp_tint: Color,
    pub tcp_gutter: Color,
    pub tcp_column: Color,
    pub tcp_rule: Color,
    pub tcp_field: Color,
    pub tcp_button: Color,
    pub tcp_combo: Color,
    pub tcp_meter_well: Color,
    /// How strongly a track's colour tints its row, from the theme.
    pub track_tint: f32,
    /// The palette the ported control art is drawn against.
    ///
    /// `daw_theme_art`'s components reach for `Theme::default()`, which
    /// is right for the one theme they were drawn for and wrong here:
    /// this window opens whatever REAPER theme the user has, and taking
    /// the default put the track panel's buttons in a grey the rest of
    /// the window had moved away from. So the drawings take a palette,
    /// and this is it.
    pub chrome: daw_theme::Chrome,
}

impl Palette {
    #[must_use]
    pub fn from_theme(theme: &daw_ui::theming::Theme) -> Self {
        let c = |col: daw_ui::theming::Color| Color::from_rgba8(col.r, col.g, col.b, col.a);
        Self {
            surface: c(theme.arrange.bg),
            row_a: c(theme.arrange.row_bg[0]),
            row_b: c(theme.arrange.row_bg[1]),
            divider: c(theme.arrange.row_divider[0]),
            grid: c(theme.arrange.grid_measure),
            grid_beat: c(theme.arrange.grid_beat),
            grid_sub: c(theme.arrange.grid_sub),
            ruler_bg: c(theme.arrange.ruler_bg),
            ruler_fg: c(theme.arrange.ruler_fg),
            ruler_fg2: c(theme.arrange.ruler_fg2),
            item_edge: c(theme.arrange.item_edge),
            text_dim: c(theme.tokens.text_dim),
            text_faint: c(theme.tokens.text_faint),
            accent: c(theme.tokens.accent),
            mute: c(theme.tokens.mute),
            solo: c(theme.tokens.solo),
            rec: c(theme.tokens.rec),
            meter_safe: c(theme.tokens.meter_safe),
            meter_warn: c(theme.tokens.meter_warn),
            pan: c(theme.tokens.route_send),
            meter_danger: c(theme.tokens.meter_danger),
            tcp_tint: c(theme.tokens.surface_raised),
            tcp_gutter: c(theme.tokens.surface),
            tcp_column: c(theme.tokens.surface_sunken),
            tcp_rule: c(theme.tokens.border),
            tcp_field: c(theme.tokens.surface_sunken),
            tcp_button: c(theme.tokens.surface),
            tcp_combo: c(theme.tokens.surface_sunken),
            tcp_meter_well: c(theme.tokens.surface_sunken),
            track_tint: theme.metrics.track_tint,
            chrome: chrome(theme),
            text: c(theme.tokens.text),
        }
    }
}

/// Everything drawn, recorded in content space.
pub struct Arrangement {
    /// The panel's shape — full, or compact. What it was RECORDED at:
    /// changing it re-cuts the panel, because the rows are drawn to it.
    pub tcp: crate::tcp::Tcp,
    /// The lanes and their items, at x = seconds * 1.0. The zoom is
    /// applied by the replay transform, so this is recorded at one
    /// pixel per second and scaled at draw time.
    pub lanes: Scene,
    /// The track panel. Recorded separately because it does NOT move
    /// horizontally — it is replayed under a transform that carries only
    /// the vertical scroll.
    pub panel: Scene,
    /// What vertical zoom [`Self::panel`] was cut at.
    ///
    /// Read by the window to know whether the cut it is about to replay
    /// is still the right one. Not a `Cell` the replay checks itself,
    /// because re-cutting needs the rows and the theme and the replay
    /// has neither — the decision belongs to whoever is drawing.
    pub panel_zoom: f64,
    /// The same panel as bands: two rectangles a row, no controls.
    ///
    /// The detail a row deserves depends on how tall it is ON SCREEN,
    /// and that is the zoom — which the recorded scene cannot know,
    /// because not depending on the zoom is the whole reason it can be
    /// recorded once. So both are recorded and the replay picks per row.
    ///
    /// Without this, a session zoomed out to fit drew two thousand
    /// complete track panels into a strip a fifth of a pixel tall each:
    /// seventy-five thousand commands to produce a column of coloured
    /// lines, and the one view that most needs to be fast was the
    /// slowest thing the window did.
    pub panel_bar: Scene,
    pub rows: usize,
    pub length_secs: f64,
    /// The project tempo, for the ruler's bar lines.
    pub bpm: f64,
    /// Where the tempo or the signature changes.
    tempo: Vec<daw_ui::studio::project::TempoChange>,
    /// Where the song starts (its `SONGSTART` marker): the bar it falls
    /// in is bar 1 on the ruler, the count-in before it counted down.
    song_start: Option<f64>,
    /// Every item's title, by row, in seconds — drawn per frame in
    /// pixel space over the lanes, because recorded text would stretch
    /// with the zoom.
    titles: Vec<Title>,
    /// What the items say on their faces — see [`Lettering`]. A view
    /// setting rather than part of the recording: the titles are drawn
    /// per frame, so switching it costs no rebuild.
    pub lettering: Lettering,
    /// The song's chords and key changes, for the ruler's CHORDS lane.
    chart: ChartMarks,
    /// Every item, by row, in seconds, with its fades — what a hit
    /// test asks and what the fade handles are drawn from.
    items: Vec<ItemBox>,
    /// The song's shape, for the ruler's lanes.
    sections: Vec<daw_ui::studio::project::Section>,
    markers: Vec<daw_ui::studio::project::Marker>,
    /// How many items were recorded, for reports that want to say what
    /// was actually drawn.
    pub item_count: usize,
    /// Where each row's commands live, so a replay can skip the rows
    /// that are not on screen. See [`Index`].
    pub index: Index,
    /// The top of every row, plus the bottom of the last — so row `i`
    /// occupies `offsets[i]..offsets[i + 1]` and there are `rows + 1`
    /// entries.
    ///
    /// Cumulative rather than a list of heights, because what a scroll
    /// actually asks is "which row is at this y", and a running sum
    /// answers it with a binary search where heights would need a walk.
    offsets: Vec<f64>,
    /// Each row's track, without the automation lanes under it — what its
    /// items and its panel controls sit in. `offsets` spans both.
    track_h: Vec<f64>,
    /// Each row's tracks its lane line speaks for, as the song had them
    /// when recorded ([`crate::lanes::shown_on`] over every track, not only
    /// the rows shown): a shut folder's tracks are under it, off screen.
    lane_tracks: Vec<Vec<daw_proto::Track>>,
    /// The lanes each row shows, when it shows them all ([`row_lanes`]):
    /// what its strips are named by and swiped on.
    laned: Vec<Option<daw_proto::Track>>,
    /// Which of those lanes has a strip, top to bottom ([`lane_slots`]):
    /// empty for a row showing what plays.
    slots: Vec<Vec<u32>>,
}

/// Which commands belong to which row, and how wide each one is.
///
/// The scene is recorded once and replayed under a transform, which is
/// what makes scrolling free of a rebuild — but it also means a naive
/// replay walks the WHOLE session every frame. At 5120x1440 a 71px row
/// pitch puts about twenty rows on screen; with two thousand tracks that
/// is 1% of the scene, and the profiler measured the other 99% costing
/// 70% of the frame.
///
/// Rows are recorded consecutively, so a row's commands are a contiguous
/// range and the index is two `Vec`s of offsets rather than a tree. The
/// x extents are kept per command so a row that IS on screen still skips
/// the items scrolled off the sides of it — the case that matters when
/// zoomed in on bar 400 of 500.
#[derive(Default)]
pub struct Index {
    /// `lanes` command range per row.
    lanes: Vec<std::ops::Range<u32>>,
    /// `panel` command range per row.
    panel: Vec<std::ops::Range<u32>>,
    /// `panel_bar` command range per row.
    panel_bar: Vec<std::ops::Range<u32>>,
    /// Content-space x extent of every `lanes` command, indexed the same
    /// way as `lanes.commands`. In seconds, like the recording.
    x: Vec<(f64, f64)>,
    /// The audio items' waveforms, by the command of the item body they
    /// are drawn over: built at replay ([`lane_wave_paths`]), at about a
    /// point a pixel and only where on screen, never recorded. Recorded,
    /// every envelope's every point was a path element at the lanes' one
    /// pixel a second — tens of megabytes a song, most of a browser
    /// page — for a picture that at any zoom shows a point a pixel.
    waves: std::collections::HashMap<u32, Vec<LaneWave>>,
}

/// An item's waveform, to draw over its body at replay.
struct LaneWave {
    wave: std::sync::Arc<crate::midi::Wave>,
    /// The item's span, seconds, and the lane's band.
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    color: Color,
    /// Display gain: the take's loudest peak reaches the band's edge.
    gain: f64,
    /// The envelope folded for the last zoom drawn — see [`Runs`].
    runs: std::cell::RefCell<Option<std::rc::Rc<Runs>>>,
    /// The real items this waveform is of — the item, or a shut folder's
    /// children under it — read again finer when zoomed in past it
    /// ([`crate::wave_detail`]).
    sources: Vec<crate::wave_detail::Source>,
    /// The drum this is a shut kit's layer of: drawn whole around its own
    /// hits and faint between ([`crate::kit_gate`]).
    role: Option<LaneRole>,
}

/// A waveform folded to one zoom's resolution: every run of `fold` points
/// as its middle (seconds) and its loudest each way, over the whole item.
///
/// Folding is most of what drawing a waveform cost, and it changes only
/// with the zoom: kept, a pan draws from it rather than folding the
/// thousands of points under a long item again every frame.
struct Runs {
    fold: usize,
    /// The hit markers' version its drum's gate was made from.
    gated: u64,
    /// Seconds between one run and the next.
    pitch: f64,
    columns: Vec<(f64, f32, f32)>,
    /// Each [`CHUNK`] of runs as the path it draws, once drawn: a pan
    /// draws the pieces it has rather than building any.
    chunks: std::cell::RefCell<Vec<Option<std::rc::Rc<BezPath>>>>,
}

/// How many runs one kept piece of a waveform's path holds.
const CHUNK: usize = 256;

/// How far, in pixels, a point may sit off the line through its
/// neighbours and still be left out: a flat stretch (the silence between
/// hits) is drawn by its ends, not by a point a pixel.
const FLAT: f64 = 0.25;

/// How many [`CHUNK`]s `runs` makes.
const fn runs_len(runs: &[(f64, f32, f32)]) -> usize {
    runs.len().div_ceil(CHUNK)
}

/// `points` without those on the flat between their neighbours (within
/// [`FLAT`]): the ends of every flat stretch stay, so the outline is the
/// same one.
fn unflat(points: &[(f64, f64)]) -> Vec<(f64, f64)> {
    let mut out: Vec<(f64, f64)> = Vec::with_capacity(points.len());
    for (i, &point) in points.iter().enumerate() {
        let flat = match (out.last(), points.get(i.saturating_add(1))) {
            (Some(&(_, before)), Some(&(_, after))) => {
                (point.1 - before).abs() < FLAT && (after - point.1).abs() < FLAT
            }
            _ => false,
        };
        if !flat {
            out.push(point);
        }
    }
    out
}

impl LaneWave {
    /// Where a run's loudest reaches, above (`max`) and below (`min`)
    /// the band's middle.
    fn ys(&self, max: f32, min: f32) -> (f64, f64) {
        let half = (self.bottom - self.top) / 2.0;
        let mid = f64::midpoint(self.top, self.bottom);
        let edge = half - 1.0;
        let reach = edge * self.gain;
        (
            mid - (f64::from(max.max(0.0)) * reach).clamp(HAIR, edge.max(HAIR)),
            mid + (-f64::from(min.min(0.0)) * reach).clamp(HAIR, edge.max(HAIR)),
        )
    }

    /// Chunk `k` of `folded` as the closed outline it draws: its runs (and
    /// the next chunk's first, so neighbours meet) across the top and back
    /// along the bottom. Built the first time it is asked for.
    fn chunk(&self, folded: &Runs, k: usize) -> Option<std::rc::Rc<BezPath>> {
        if let Some(Some(kept)) = folded.chunks.borrow().get(k) {
            return Some(std::rc::Rc::clone(kept));
        }
        let from = k.checked_mul(CHUNK)?;
        let to = from.saturating_add(CHUNK + 1).min(folded.columns.len());
        let runs = folded.columns.get(from..to).filter(|r| !r.is_empty())?;
        let (tops, bottoms): (Vec<_>, Vec<_>) = runs
            .iter()
            .map(|&(x, max, min)| {
                let (top, bottom) = self.ys(max, min);
                ((x, top), (x, bottom))
            })
            .unzip();
        let mut path = BezPath::new();
        for (i, point) in unflat(&tops).into_iter().enumerate() {
            if i == 0 {
                path.move_to(point);
            } else {
                path.line_to(point);
            }
        }
        for point in unflat(&bottoms).into_iter().rev() {
            path.line_to(point);
        }
        path.close_path();
        let path = std::rc::Rc::new(path);
        if let Some(slot) = folded.chunks.borrow_mut().get_mut(k) {
            *slot = Some(std::rc::Rc::clone(&path));
        }
        Some(path)
    }

    /// The envelope folded `fold` points to a run, folded now if the last
    /// zoom drawn was another.
    fn runs(&self, fold: usize) -> std::rc::Rc<Runs> {
        let gated = crate::kit_gate::version(self.role);
        let mut kept = self.runs.borrow_mut();
        if let Some(runs) = kept.as_ref().filter(|r| r.fold == fold && r.gated == gated) {
            return std::rc::Rc::clone(runs);
        }
        let wave = &self.wave;
        let gate = self.gate();
        let runs: Vec<(f64, f32, f32)> = wave
            .points
            .chunks(fold)
            .enumerate()
            .map(|(r, run)| {
                let first = r.saturating_mul(fold);
                let (max, min) =
                    run.iter()
                        .enumerate()
                        .fold((0.0_f32, 0.0_f32), |(mx, mn), (i, &(a, b))| {
                            let t = (crate::num::coord(first.saturating_add(i)) + 0.5)
                                .mul_add(wave.step, self.x0);
                            let g = shown(gate.as_deref(), t);
                            (mx.max(a * g), mn.min(b * g))
                        });
                let at =
                    crate::num::coord(r.saturating_mul(fold)) + crate::num::coord(run.len()) / 2.0;
                (
                    at.mul_add(wave.step, self.x0).clamp(self.x0, self.x1),
                    max,
                    min,
                )
            })
            .collect();
        let chunks = std::cell::RefCell::new(vec![None; runs_len(&runs)]);
        let runs = std::rc::Rc::new(Runs {
            fold,
            gated,
            pitch: wave.step * crate::num::coord(fold),
            columns: runs,
            chunks,
        });
        *kept = Some(std::rc::Rc::clone(&runs));
        runs
    }
}

impl LaneWave {
    /// Its drum's gate, when it is a shut kit's drum layer.
    fn gate(&self) -> Option<std::rc::Rc<crate::kit_gate::Gate>> {
        self.role.and_then(crate::kit_gate::of)
    }
}

/// How much of a gated layer shows at `t` (all of it, ungated).
fn shown(gate: Option<&crate::kit_gate::Gate>, t: f64) -> f32 {
    gate.map_or(1.0, |g| {
        num_traits::ToPrimitive::to_f32(&g.at(t)).unwrap_or(1.0)
    })
}

/// The display gain for `wave`: its loudest peak to the lane's edge, so a
/// stem mixed at -20 dB draws as a waveform and not a line. What is drawn
/// changes, never what plays. Capped, so a stem that is near-silent all
/// through shows as near-silent rather than as its noise floor blown up
/// to full height.
fn wave_gain(wave: &crate::midi::Wave) -> f64 {
    gain_for(loudest_of(wave))
}

/// The loudest point of `wave`, either way.
fn loudest_of(wave: &crate::midi::Wave) -> f64 {
    wave.points
        .iter()
        .map(|&(max, min)| f64::from(max.abs().max(min.abs())))
        .fold(0.0_f64, f64::max)
}

/// The display gain that brings `loudest` to the band's edge.
fn gain_for(loudest: f64) -> f64 {
    const MOST_GAIN: f64 = 24.0; // ≈ +27.6 dB
    if loudest > 0.0 {
        (1.0 / loudest).min(MOST_GAIN)
    } else {
        1.0
    }
}

/// How far from the middle silence is drawn: nowhere. The waveform is
/// recorded in the session's units and replayed under the vertical zoom,
/// so any floor here was a floor times the zoom — 0.6 a side became a
/// ten-pixel band through every silence on a track framed tall. The item's
/// body and outline still show where a silent item is.
const HAIR: f64 = 0.0;

/// The part of `lane`'s waveform between `left` and `right` (seconds), at
/// about a point for each of `pps` pixels a second: the envelope's points
/// folded (loudest of each run kept) until there is one per pixel. `None`
/// when none of it is in view, or the band is too thin to show one.
fn lane_wave_paths(lane: &LaneWave, left: f64, right: f64, pps: f64) -> Vec<std::rc::Rc<BezPath>> {
    let wave = &lane.wave;
    let half = (lane.bottom - lane.top) / 2.0;
    let (from, to) = (lane.x0.max(left), lane.x1.min(right));
    if half < 1.5 || wave.step <= 0.0 || from >= to || wave.points.is_empty() {
        return Vec::new();
    }
    // Points folded per drawn point — a power of two, so a zoom moving a
    // little keeps the runs it has — at most a point a pixel. The runs
    // are the whole item's, aligned to its start, so a scroll does not
    // shimmer.
    let fold = crate::num::index((1.0 / (wave.step * pps.max(1e-9))).floor().max(1.0))
        .max(1)
        .next_power_of_two();
    let folded = lane.runs(fold);
    let at = |t: f64| crate::num::index(((t - lane.x0) / folded.pitch).floor().max(0.0));
    let last = folded.columns.len().saturating_sub(1);
    let (first, end) = (at(from).min(last), at(to).min(last));
    (first / CHUNK..=end / CHUNK)
        .filter_map(|k| lane.chunk(&folded, k))
        .collect()
}

/// Zoomed in until a lane's own points are wider than this many pixels,
/// its waveform is drawn from detail ([`crate::wave_detail`]) instead.
const DETAIL_PAST: f64 = 2.0;

/// Past this many pixels a sample, each sample is marked with a dot.
const SAMPLE_DOTS: f64 = 6.0;

/// `(x, y)` through `transform`, as a point on the surface.
fn place(transform: Affine, x: f64, y: f64) -> vello::kurbo::Point {
    let [a, b, c, d, e, f] = transform.as_coeffs();
    vello::kurbo::Point::new(a.mul_add(x, c.mul_add(y, e)), b.mul_add(x, d.mul_add(y, f)))
}

/// A waveform drawn from detail.
enum Fine {
    /// A column a pixel, in the lanes' own coordinates.
    Envelope(BezPath),
    /// Zoomed in past a pixel a sample: a line through the samples of
    /// each item, and a dot at each when they are far enough apart — in
    /// pixels, since the lanes' transform would stretch a stroke.
    Samples(Vec<BezPath>, Vec<Rect>),
}

/// `lane`'s waveform from `left` to `right` seconds from detail, when
/// zoomed in past its own points — as fine as has been read, asked for
/// where it is not: a column no detail reaches yet is drawn from the
/// lane's own points, so the waveform never drops back whole to the
/// coarse one while a read is under way. `None` until any detail of it
/// has been read (the coarse one drawn).
fn fine(lane: &LaneWave, left: f64, right: f64, pps: f64, transform: Affine) -> Option<Fine> {
    if lane.sources.is_empty() || lane.wave.step * pps <= DETAIL_PAST {
        return None;
    }
    let (a, b) = (lane.x0.max(left), lane.x1.min(right));
    if a >= b || pps <= 0.0 {
        return None;
    }
    let mut details = Vec::new();
    let mut missing = false;
    for source in &lane.sources {
        let (sa, sb) = (a.max(source.at), b.min(source.at + source.length));
        if sa >= sb {
            continue;
        }
        // A track is asked for over the view, the same for every item of
        // it on screen: one read, not one an item.
        let (ask_a, ask_b) = if source.track.is_some() {
            (left.max(0.0), right)
        } else {
            (sa, sb)
        };
        match crate::wave_detail::get(source, ask_a, ask_b, 1.0 / pps) {
            Some(detail) => details.push((detail, sa, sb, source.scale)),
            None => missing = true,
        }
    }
    if details.is_empty() {
        return None;
    }
    let coarsest = details.iter().map(|(d, ..)| d.step).fold(0.0, f64::max);
    let gate = lane.gate();
    let scaled = |scale: f64, v: f32| v * num_traits::ToPrimitive::to_f32(&scale).unwrap_or(1.0);
    // A line through the samples only once every item under it has them
    // across the span: a line can have no gaps.
    let every_sample = !missing
        && details
            .iter()
            .all(|(d, sa, sb, _)| d.samples && d.from <= *sa && d.to() >= *sb);
    if every_sample && coarsest * pps >= 1.0 {
        let half = (lane.bottom - lane.top) / 2.0;
        let mid = f64::midpoint(lane.top, lane.bottom);
        let edge = (half - 1.0).max(HAIR);
        let reach = edge * lane.gain;
        let mut lines = Vec::new();
        let mut dots = Vec::new();
        for (detail, sa, sb, scale) in &details {
            let mut trace = BezPath::new();
            for (i, (t, (v, _))) in detail.between(*sa, *sb).enumerate() {
                let v = scaled(*scale, v) * shown(gate.as_deref(), t);
                let y = mid - (f64::from(v) * reach).clamp(-edge, edge);
                let at = place(transform, t, y);
                if i == 0 {
                    trace.move_to(at);
                } else {
                    trace.line_to(at);
                }
                if detail.step * pps >= SAMPLE_DOTS {
                    dots.push(Rect::from_center_size(at, (3.0, 3.0)));
                }
            }
            lines.push(trace);
        }
        return Some(Fine::Samples(lines, dots));
    }
    let columns = crate::num::index(((b - a) * pps).ceil()).max(1);
    let mut cols: Vec<Option<(f32, f32)>> = vec![None; columns];
    let column = |t: f64| crate::num::index(((t - a) * pps).floor());
    // Each point into every column its span covers: a point coarser than a
    // column (detail read for the zoom before) leaves none of them empty.
    let take = |cols: &mut Vec<Option<(f32, f32)>>, t: f64, step: f64, (max, min)| {
        let (from, to) = (t.max(a), (t + step).min(b));
        if from >= to {
            return;
        }
        let last = column(to - 1e-12).min(columns.saturating_sub(1));
        for col in cols
            .iter_mut()
            .take(last.saturating_add(1))
            .skip(column(from))
        {
            let (mx, mn) = col.unwrap_or((0.0, 0.0));
            *col = Some((mx.max(max), mn.min(min)));
        }
    };
    for (detail, sa, sb, scale) in &details {
        for (t, (max, min)) in detail.between(*sa, *sb) {
            let g = shown(gate.as_deref(), t);
            take(
                &mut cols,
                t,
                detail.step,
                (scaled(*scale, max) * g, scaled(*scale, min) * g),
            );
        }
    }
    // Where no detail reaches yet, the lane's own points.
    if cols.iter().any(Option::is_none) {
        let wave = &lane.wave;
        let first = crate::num::index(((a - lane.x0) / wave.step).floor());
        for (i, point) in wave.points.iter().enumerate().skip(first) {
            let t = crate::num::coord(i).mul_add(wave.step, lane.x0);
            if t >= b {
                break;
            }
            let (from, to) = (t.max(a), (t + wave.step).min(b));
            if from >= to {
                continue;
            }
            let last = column(to - 1e-12).min(columns.saturating_sub(1));
            for col in cols
                .iter_mut()
                .take(last.saturating_add(1))
                .skip(column(from))
            {
                if col.is_none() {
                    let g = shown(gate.as_deref(), t);
                    *col = Some((point.0 * g, point.1 * g));
                }
            }
        }
    }
    let filled: Vec<(f64, f32, f32)> = cols
        .iter()
        .enumerate()
        .filter_map(|(c, v)| {
            v.map(|(max, min)| ((crate::num::coord(c) + 0.5).mul_add(1.0 / pps, a), max, min))
        })
        .collect();
    // No column with anything in it (a sliver of an item, say): no
    // envelope — a path of nothing but its close is no shape to fill.
    if filled.is_empty() {
        return None;
    }
    let mut path = BezPath::new();
    for (i, &(x, max, min)) in filled.iter().enumerate() {
        let (top, _) = lane.ys(max, min);
        if i == 0 {
            path.move_to((x, top));
        } else {
            path.line_to((x, top));
        }
    }
    for &(x, max, min) in filled.iter().rev() {
        path.line_to((x, lane.ys(max, min).1));
    }
    path.close_path();
    Some(Fine::Envelope(path))
}

impl Arrangement {
    /// How tall the whole session is, in content pixels.
    #[must_use]
    pub fn content_height(&self) -> f64 {
        self.offsets.last().copied().unwrap_or(0.0)
    }

    /// How tall row `i`'s track is (its automation lanes not counted), in
    /// content pixels.
    #[must_use]
    fn row_height(&self, row: usize) -> f64 {
        if let Some(h) = self.track_h.get(row) {
            return *h;
        }
        let top = self.offsets.get(row).copied().unwrap_or(0.0);
        let bottom = self
            .offsets
            .get(row.saturating_add(1))
            .copied()
            .unwrap_or(top);
        bottom - top
    }

    /// A row's top and height, for laying its controls out.
    ///
    /// The overlay and the hit test need this for the same reason the
    /// mixer's `strip_box` exists: the panel is recorded, so anything
    /// live has to be told where the recorded thing is.
    #[must_use]
    pub fn row_box(&self, row: usize) -> Option<(f64, f64)> {
        let top = *self.offsets.get(row)?;
        let bottom = *self.offsets.get(row.checked_add(1)?)?;
        let track = self.track_h.get(row).copied().unwrap_or(bottom - top);
        Some((top, (track.min(bottom - top) - DIVIDER).max(0.5)))
    }

    /// The tracks row `row`'s lane line speaks for, as recorded — a shut
    /// folder's included, which are not rows ([`crate::lanes::speaks_for`]).
    #[must_use]
    pub fn lane_tracks(&self, row: usize) -> &[daw_proto::Track] {
        self.lane_tracks.get(row).map_or(&[], Vec::as_slice)
    }

    /// The lanes row `row` shows, as recorded, when it shows them all: the
    /// track's own, or a shut folder's ([`row_lanes`]).
    #[must_use]
    pub fn lanes_shown(&self, row: usize) -> Option<&daw_proto::Track> {
        self.laned.get(row).and_then(Option::as_ref)
    }

    /// The lanes row `row` gives a strip, top to bottom ([`lane_slots`]);
    /// empty when it shows what plays.
    #[must_use]
    pub fn lane_slots(&self, row: usize) -> &[u32] {
        self.slots.get(row).map_or(&[], Vec::as_slice)
    }

    /// Where row `row`'s automation lanes start and how much room they
    /// have together, in session coordinates: under its track, to the
    /// bottom of its slot. `None` for a row with none.
    #[must_use]
    pub fn lanes_box(&self, row: usize) -> Option<(f64, f64)> {
        let top = *self.offsets.get(row)?;
        let bottom = *self.offsets.get(row.checked_add(1)?)?;
        let track = self.track_h.get(row).copied()?;
        (bottom - top - track > 0.5).then_some((top + track, bottom - top - track))
    }

    /// An item's band in session coordinates — its row's, narrowed to its
    /// lane on a track showing fixed lanes: `(top, height)`.
    #[must_use]
    pub fn item_box(&self, item: &ItemBox) -> Option<(f64, f64)> {
        let (top, height) = self.row_box(item.row)?;
        let (from, to) = item.band;
        Some((from.mul_add(height, top), ((to - from) * height).max(0.5)))
    }

    /// A row's band ON SCREEN: where [`Self::row_box`] puts it, after
    /// the vertical zoom.
    ///
    /// The difference matters to everything drawn OVER the panel
    /// rather than recorded into it. The recording is replayed under a
    /// transform that already carries `zoom_y`, so session coordinates
    /// are all it needs. The live controls — the knob, the arm, the
    /// name, mute and solo — are drawn fresh every frame under a plain
    /// translate, because they must not stretch when the rows get
    /// taller. That leaves them to do the placing themselves, and not
    /// doing it is why they walked off their own rows the moment
    /// anybody zoomed.
    ///
    /// The height is scaled too, and deliberately: it is what decides
    /// which tier of controls a row shows, and the answer has to be
    /// about the row on screen rather than the row in the session.
    /// What does NOT scale is the art inside the band — see
    /// `crate::art::squashed`, which clamps at 1.
    #[must_use]
    pub fn row_band(&self, row: usize, view: Viewport) -> Option<(f64, f64)> {
        let (top, height) = self.row_box(row)?;
        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        Some((top * zoom, height * zoom))
    }

    /// Which row is at a content y, if any.
    ///
    /// `content_y` is in SESSION units — divide a screen offset by the
    /// vertical zoom before asking, or use [`Self::row_at_screen`],
    /// which does it for you.
    ///
    /// The same binary search `visible_rows` uses, over the same
    /// cumulative offsets — so a hit and a draw cannot disagree about
    /// which row a pixel belongs to unless the offsets themselves are
    /// wrong.
    #[must_use]
    pub fn row_at(&self, content_y: f64) -> Option<usize> {
        if content_y < 0.0 || self.rows == 0 {
            return None;
        }
        let row = self
            .offsets
            .partition_point(|&y| y <= content_y)
            .checked_sub(1)?;
        (row < self.rows).then_some(row)
    }

    /// Which row is under a y measured in SCREEN pixels from the top
    /// of the lanes.
    ///
    /// The pair to [`Self::row_band`], and the reason both exist: a
    /// hit arrives in screen pixels and the offsets are in session
    /// units, so something has to divide — and for a while nothing
    /// did, which meant that at any zoom but 1 the pointer reported a
    /// row it was not over.
    #[must_use]
    pub fn row_at_screen(&self, screen_y: f64, view: Viewport) -> Option<usize> {
        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        self.row_at(screen_y / zoom)
    }

    /// The panel row and control under a point, in the SCREEN
    /// coordinates the lanes start from.
    ///
    /// `x` is measured from the left of the panel and `y` from the top
    /// of the lanes, which is the frame every caller already has to put
    /// the panel in. Lives here rather than in a window because the
    /// answer has to come from the same geometry the row was DRAWN
    /// from, and two windows working it out separately is two chances
    /// to disagree about which button the pointer is on.
    #[must_use]
    pub fn row_spot_at(
        &self,
        view: Viewport,
        rows: &[(daw_proto::Track, u32)],
        tracks: &[daw_proto::Track],
        x: f64,
        y: f64,
    ) -> Option<(usize, crate::row::Control)> {
        if x < 0.0 || x >= self.tcp.width() {
            return None;
        }
        let index = self.row_at_screen(y, view)?;
        let (top, height) = self.row_band(index, view)?;
        let (track, depth) = rows.get(index)?;
        let depth = i32::try_from(*depth).unwrap_or(0);
        // The lane steps, drawn over a theme's row as over the built-in
        // one, are hit the same way on both.
        let row = crate::row::Row::new(top, height, depth, track.is_folder, self.tcp).with_lanes(
            !crate::lanes::speaks_for(tracks, track, self.lane_tracks(index)).is_empty(),
        );
        if let Some(
            step @ (crate::row::Control::LanePrev
            | crate::row::Control::LaneNext
            | crate::row::Control::LaneName
            | crate::row::Control::LaneShow
            | crate::row::Control::Automation),
        ) = row.control_at(x, y)
        {
            return Some((index, step));
        }
        if let Some(theme) = crate::skin::loaded()
            .as_deref()
            .filter(|t| t.has_role(crate::skin::ROW) && !self.tcp.compact)
        {
            let hit = theme.hit(
                crate::skin::ROW,
                (self.tcp.width(), height),
                &crate::skin::row_hit_fields(depth),
                vello::kurbo::Point::new(x, y - top),
            )?;
            return Some((index, crate::skin::row_control(&hit)?));
        }
        // In the band's own frame, which is what `Row` measures from.
        Some((index, row.control_at(x, y)?))
    }

    /// The rows that intersect `view`, clamped to what exists.
    ///
    /// A binary search over [`Arrangement::offsets`] rather than a
    /// division by a row pitch: there is no pitch any more. Rows are
    /// whatever height the project says, so "which row is at this y" is
    /// a lookup, not arithmetic.
    ///
    /// One row of bleed on each side: a row scrolled half off the top
    /// still paints its visible half, and dropping it would tear the
    /// edge of the screen during exactly the fast scroll this is for.
    #[must_use]
    pub fn visible_rows(&self, view: Viewport) -> std::ops::Range<usize> {
        let rows = self.index.lanes.len();
        if rows == 0 {
            return 0..0;
        }
        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        let top = view.scroll_y / zoom;
        let bottom = (view.scroll_y + view.height) / zoom;
        // `partition_point` gives the first row whose TOP is past the
        // edge; the row before it is the one the edge falls inside.
        let first = self
            .offsets
            .partition_point(|&y| y <= top)
            .saturating_sub(1);
        let last = self.offsets.partition_point(|&y| y < bottom);
        first.min(rows)..last.min(rows)
    }
}

/// What part of the session a frame can actually see.
///
/// Held in the same content space the scene is recorded in — seconds
/// across, row pitch down — so the visibility test is the inverse of the
/// replay transform and nothing here has to know about `Affine`.
#[derive(Clone, Copy)]
pub struct Viewport {
    pub scroll_x: f64,
    pub scroll_y: f64,
    /// Pixels per second, zoom included.
    pub pps: f64,
    pub zoom_y: f64,
    pub width: f64,
    pub height: f64,
    /// Where the lanes start: the track panel's width, in whichever
    /// shape it is in. The ONE number every pass that places something
    /// against time reads — the lanes, the ruler, the cursors, the hit
    /// test — so a narrower panel cannot move some of them and not
    /// others.
    pub panel_w: f64,
}

impl Viewport {
    /// The span of the session visible across, in seconds, with a screen
    /// of bleed either side for the same reason.
    #[must_use]
    pub fn secs(self) -> (f64, f64) {
        let pps = self.pps.max(1e-9);
        let left = (self.scroll_x - self.panel_w) / pps;
        let right = (self.scroll_x + self.width) / pps;
        (left - 1.0, right + 1.0)
    }

    /// The seconds on screen, with only a few pixels to spare — for what
    /// is drawn per pixel ([`fine`]), where [`Self::secs`]'s second either
    /// side would be half a million pixels zoomed in to the samples.
    #[must_use]
    pub fn on_screen(self) -> (f64, f64) {
        let pps = self.pps.max(1e-9);
        let spare = 8.0 / pps;
        (
            (self.scroll_x - self.panel_w) / pps - spare,
            (self.scroll_x + self.width) / pps + spare,
        )
    }
}

/// The track panel, recorded at a vertical zoom.
///
/// Its own function, and re-runnable, because the panel is the one
/// surface that must NOT be scaled into place. The lanes can be: a
/// waveform stretched twice as tall is a waveform twice as tall, which
/// is what zooming a lane means. A track panel stretched twice as tall
/// is a name plate with the lettering pulled out of shape and a knob
/// turned into an ellipse — every one of which was on screen the
/// moment anybody zoomed.
///
/// So the row art is cut fresh at the heights it will be seen at, and
/// replayed under a translate. What does not scale with it is
/// [`DIVIDER`]: a hairline between rows is a hairline at every zoom,
/// and multiplying it was the other half of the same bug.
fn record_panel(
    palette: &Palette,
    font: &crate::text::Font,
    rows: &[(daw_proto::Track, u32)],
    layout: crate::layout::Layout,
    zoom: f64,
    tcp: crate::tcp::Tcp,
) -> Panels {
    // The theme's row, when it draws one — the full panel's; the compact
    // panel's second line is still the painters'.
    let theme = crate::skin::loaded().filter(|t| t.has_role(crate::skin::ROW) && !tcp.compact);
    let mut panel = Scene::new();
    let mut bar = Scene::new();
    let mut spans = Vec::with_capacity(rows.len());
    let mut bar_spans = Vec::with_capacity(rows.len());
    let mut lineage: Vec<Color> = Vec::new();
    let mut y = 0.0_f64;
    for (index, (track, depth)) in rows.iter().enumerate() {
        let collapsed = crate::folds::shut(rows, index);
        let level = usize::try_from(*depth).unwrap_or(0);
        lineage.truncate(level);
        let ancestors = lineage.clone();
        lineage.push(crate::tcp::folder_band(palette, track));

        let from = command_index(&panel);
        let bar_from = command_index(&bar);
        let h = layout.height_of(track.height) * zoom;
        let body = (h - DIVIDER).max(0.5);

        let depth = i32::try_from(*depth).unwrap_or(0);
        if let Some(theme) = theme.as_deref() {
            let fields = crate::skin::row_fields(
                palette,
                track,
                crate::skin::RowShown {
                    depth,
                    height: body,
                    font: Some(font),
                    idle: !tcp.recording,
                    collapsed,
                    ..crate::skin::RowShown::default()
                },
            );
            theme.record_with(
                crate::skin::ROW,
                (tcp.width(), body),
                &fields,
                &mut crate::skin::canvas::Host {
                    scene: &mut panel,
                    font,
                },
                Affine::translate((0.0, y)),
                &mut |host, slot, at| match slot {
                    "steps" => crate::tcp::folder_steps(host.scene, &ancestors, y, body),
                    "icon" => crate::track_icon::paint(
                        host.scene,
                        crate::track_icon::Icon::of(&track.name, track.is_folder),
                        at + vello::kurbo::Vec2::new(0.0, y),
                        crate::tcp::ink_on(crate::tcp::folder_band(palette, track)),
                    ),
                    _ => {}
                },
            );
            // The automation button over a theme's row as over the
            // built-in one (hit the same way on both: `row_spot_at`).
            crate::tcp::automation_button(&mut panel, palette, y, body, tcp);
        } else {
            crate::tcp::draw_row(
                &mut panel, palette, font, track, depth, y, body, &ancestors, tcp,
            );
        }
        panel.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.divider,
            None,
            &Rect::new(0.0, y + body, tcp.width(), y + h),
        );

        // The same row as a band, for when it is too short on screen to
        // be worth more. Its tint and its gutter and nothing else.
        bar.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            crate::tcp::row_tint(palette, track),
            None,
            &Rect::new(0.0, y, tcp.width(), y + body),
        );
        bar.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.divider,
            None,
            &Rect::new(0.0, y + body, tcp.width(), y + h),
        );

        spans.push(from..command_index(&panel));
        bar_spans.push(bar_from..command_index(&bar));
        // Its automation lanes under it: their headers are drawn live
        // ([`crate::automation_draw`]), so the panel only leaves room.
        y += crate::automation::lanes_height(&track.guid).mul_add(zoom, h);
    }
    Panels {
        panel,
        bar,
        spans,
        bar_spans,
        zoom,
    }
}

/// One cut of the panel, at one zoom.
struct Panels {
    panel: Scene,
    bar: Scene,
    spans: Vec<std::ops::Range<u32>>,
    bar_spans: Vec<std::ops::Range<u32>>,
    zoom: f64,
}

impl Arrangement {
    /// Record the whole project, once.
    #[must_use]
    pub fn build(
        palette: &Palette,
        font: &crate::text::Font,
        project: &ProjectRef,
        rows: &RowsRef,
        layout: crate::layout::Layout,
        previews: &crate::midi::Previews,
        tcp: crate::tcp::Tcp,
    ) -> Self {
        let mut lanes = Scene::new();
        let mut index = Index::default();
        let mut titles = Vec::with_capacity(project.item_count);
        let mut boxes = Vec::with_capacity(project.item_count);
        let mut offsets = Vec::with_capacity(rows.len().saturating_add(1));
        let mut track_h = Vec::with_capacity(rows.len());
        let mut lane_tracks = Vec::with_capacity(rows.len());
        let mut laned_rows = Vec::with_capacity(rows.len());
        let mut row_slots = Vec::with_capacity(rows.len());
        let mut y = 0.0_f64;
        let keys = keys_of(project);
        let roles = kit_roles(project);
        let versions = track_versions(project);
        // Every item by guid, for the shut folders' rows to find what is
        // under them — built only when some folder is shut.
        let by_guid: std::collections::HashMap<&str, &daw_proto::Item> = if project.folds.is_empty()
        {
            std::collections::HashMap::new()
        } else {
            project
                .items
                .values()
                .flatten()
                .map(|item| (item.guid.as_str(), item))
                .collect()
        };
        let loudest = folder_loudest(project, &by_guid, previews);
        for (row, (track, depth)) in rows.iter().enumerate() {
            // A folder closes simply by the next row being shallower,
            // so the truncate IS the close.
            // Command indices are `u32`: two per row plus one per item,
            // so four billion of them is a project nothing could open.
            // Saturating rather than wrapping, because a wrapped index
            // would cull the wrong rows rather than fail.
            let lanes_from = command_index(&lanes);
            offsets.push(y);
            // The track's own height, or the user's default, floored at
            // something far below REAPER's minimum — see `layout`. The
            // divider lives INSIDE the row, so a 24 track is 24 tall.
            let h = layout.height_of(track.height);
            let body = (h - DIVIDER).max(0.5);
            let stripe = if row % 2 == 0 {
                palette.row_a
            } else {
                palette.row_b
            };

            // The lane's background and the divider under it. Recorded
            // absurdly wide so a replay at any zoom still covers the
            // viewport; the backend clips.
            lanes.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                stripe,
                None,
                &Rect::new(0.0, y, project.length_secs.max(1.0), y + body),
            );
            index.x.push((f64::MIN, f64::MAX));
            lanes.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.divider,
                None,
                &Rect::new(0.0, y + body, project.length_secs.max(1.0), y + h),
            );
            index.x.push((f64::MIN, f64::MAX));

            // The panel row is NOT recorded here — see `record_panel`,
            // which cuts it fresh at whatever height the zoom asks for
            // rather than letting a transform stretch it.

            // The items on this lane. An item with no colour of its own
            // takes its TRACK's, which is what makes a session read by
            // section when it is zoomed out far enough that names are
            // gone — the same rule the panel's row tint follows.
            let track_color = crate::tcp::track_color(palette, track);
            // Items sit inside their lane, but a two-pixel inset on a
            // three-pixel row leaves nothing to see. Scaled down as the
            // row shrinks, so a collapsed session still shows its items
            // as bands rather than as empty lanes.
            let inset = (body * 0.05).clamp(0.0, 2.0);
            // A track showing its fixed lanes (REAPER 7's, as REAPER draws
            // them): every lane's items, each in its lane's strip, the
            // lanes that do not play dimmed — the takes, the comp and the
            // edit side by side. A shut folder's row is a summary — of
            // every lane of the tracks under it, lane by lane, once its
            // `+` has opened them — and a track showing one lane shows the
            // lane that plays.
            let opened = row_lanes(project, track);
            let lanes_shown = opened.is_some();
            let lane_track = opened.as_ref().unwrap_or(track);
            let row_items: Vec<&daw_proto::Item> = if lanes_shown {
                project.lane(&track.guid).iter().collect()
            } else {
                project.sounding(track)
            };
            // A strip each for the lanes holding something and the one
            // that plays — not one for every lane number the song has
            // used, most of them empty.
            let slots = if lanes_shown {
                lane_slots(
                    lane_track,
                    row_items.iter().map(|i| i.fixed_lane.unwrap_or(0)),
                )
            } else {
                Vec::new()
            };
            if lanes_shown {
                for strip in 1..slots.len() {
                    let at = crate::num::coord(strip) / crate::num::coord(slots.len());
                    let line = at.mul_add(body, y);
                    lanes.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        palette.divider.multiply_alpha(0.6),
                        None,
                        &Rect::new(0.0, line - 0.25, project.length_secs.max(1.0), line + 0.25),
                    );
                    index.x.push((f64::MIN, f64::MAX));
                }
            }
            for item in row_items {
                let band = if lanes_shown {
                    slot_band(item.fixed_lane.unwrap_or(0), &slots)
                } else {
                    (0.0, 1.0)
                };
                let plays =
                    !lanes_shown || daw_ui::studio::project::on_playing_lane(lane_track, item);
                let (y, body) = (band.0.mul_add(body, y), (band.1 - band.0) * body);
                let x0 = item.position.as_seconds();
                let x1 = x0 + item.length.as_seconds().max(0.001);
                // How far into the source this item starts. Zero for
                // everything that has not been slipped, which is why
                // adding it moves no picture that was right before.
                let slip = item.start_offset.as_seconds();
                let color = item_color(item, track_color, tcp.mixing);
                // A lane that does not play is there to be chosen from,
                // not heard: drawn faint.
                let color = if plays {
                    color
                } else {
                    color.multiply_alpha(0.45)
                };
                // The body, dimmed, and the waveform over it in the
                // full colour: the item is read by its waveform, and a
                // solid block of colour was a waveform you could not
                // see through.
                // A shut folder's body fainter still: its waveform is a
                // summary of every track under it, read against the lane.
                let alpha = if daw_ui::studio::folded::parse_guid(&item.guid).is_some() {
                    body_alpha() * FOLDER_BODY
                } else {
                    body_alpha()
                };
                lanes.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    color.multiply_alpha(alpha),
                    None,
                    &Rect::new(x0, y + inset, x1, y + body - inset),
                );
                index.x.push((x0, x1));
                let top = y + inset;
                let bottom = y + body - inset;
                // What the item CONTAINS: notes if it holds MIDI, the
                // envelope otherwise. An item drawn from a waveform it
                // does not have is why a chord track looked like a
                // shaker.
                //
                // Notes only once they have been read. Until then the
                // block is drawn plain rather than filled with a fake
                // shape, because a wrong picture that later corrects
                // itself is worse than an honest empty one.
                //
                // A shut folder's item holds what is under it: the
                // children's audio as one envelope, their notes over it.
                let folded =
                    folded_contents(project, (&by_guid, &roles, &versions), previews, &item.guid);
                let is_folder = folded.is_some();
                // An item that plays is read as its track plays: one read
                // of the track for all its items, kept through an edit.
                let source =
                    if daw_ui::studio::project::on_playing_lane(lane_track, item) && !item.muted {
                        let version = versions.get(item.track_guid.as_str()).copied().unwrap_or(0);
                        let scale = if item.volume > 1e-6 {
                            item.volume.recip()
                        } else {
                            1.0
                        };
                        crate::wave_detail::Source::track(&item.track_guid, x0, x1 - x0, version)
                            .scaled(scale)
                    } else {
                        source_of(item)
                    };
                let held = folded.unwrap_or_else(|| Held {
                    notes: previews.get(&item.guid),
                    wave: previews.wave(&item.guid),
                    layers: Vec::new(),
                    sources: vec![source],
                });
                let (notes, wave) = (held.notes, held.wave);
                // Audio: the take's own peaks, once they have been read —
                // plain until then, for the same reason.
                if let Some(whole) = wave
                    .clone()
                    .filter(|w| w.step > 0.0 && !w.points.is_empty())
                {
                    // One gain for every layer: the loudest anywhere reaches
                    // the edge, and the layers keep their loudness to each
                    // other.
                    // An item at its true level through its item gain, as
                    // REAPER draws it — so gain staging shows: a staged
                    // kick's hits near the edge, the hats under them, a
                    // track recorded low small until it is staged. A shut
                    // folder's row is a summary, as loud as the lane allows.
                    let shared_loudest = daw_ui::studio::folded::parse_guid(&item.guid)
                        .and_then(|(folder, _)| loudest.get(folder));
                    let gain = if held.layers.is_empty() && !is_folder {
                        item.volume.max(0.0)
                    } else if let Some(&loudest) = shared_loudest {
                        // The folder's loudest — but never past this
                        // piece's own: right after an edit the new slices'
                        // peaks are still being read, and the loudest of
                        // what has arrived can be a quiet one, every piece
                        // then blown up past the band.
                        gain_for(loudest.max(loudest_of(&whole)))
                    } else {
                        wave_gain(&whole)
                    };
                    let lane = |wave, color, sources, role| LaneWave {
                        runs: std::cell::RefCell::default(),
                        sources,
                        role,
                        gain,
                        wave,
                        x0,
                        x1,
                        top,
                        bottom,
                        color,
                    };
                    let drawn: Vec<LaneWave> = if held.layers.is_empty() {
                        vec![lane(whole, color, held.sources, None)]
                    } else {
                        held.layers
                            .into_iter()
                            .map(|(role, wave, sources)| {
                                lane(wave, kit_color(role).unwrap_or(color), sources, Some(role))
                            })
                            .collect()
                    };
                    // Over the body just recorded, drawn at replay.
                    let body = command_index(&lanes).saturating_sub(1);
                    index.waves.insert(body, drawn);
                }
                // On a folder of recorded audio (a kit, a piece of it) the
                // notes under it are a hint — the drum MIDI, a sample's
                // triggers — so faint, in a strip along the bottom, under
                // the audio's own shape rather than over it.
                let hint = is_folder && wave.is_some();
                let (roll_top, roll_ink) = if hint {
                    (
                        (bottom - top).mul_add(-0.22, bottom),
                        color.multiply_alpha(0.35),
                    )
                } else {
                    (top, color)
                };
                if let Some(roll) = notes
                    .as_deref()
                    .and_then(|notes| midi_preview(notes, x0, x1, roll_top, bottom))
                {
                    lanes.fill(Fill::NonZero, Affine::IDENTITY, roll_ink, None, &roll);
                    index.x.push((x0, x1));
                }
                // The fades, as the part of the item they take away:
                // the region over the gain curve, darkened, from each
                // end. Recorded in seconds like the item, so the zoom
                // stretches them with it.
                let fades = Fades::of(item);
                if let Some(path) = fades.path(x0, x1, top, bottom, 1.0) {
                    lanes.fill(Fill::NonZero, Affine::IDENTITY, FADE_SHADE, None, &path);
                    index.x.push((x0, x1));
                }
                boxes.push(ItemBox {
                    row,
                    guid: item.guid.clone(),
                    wave,
                    color,
                    slip,
                    x0,
                    x1,
                    fades,
                    // A shut folder's row stands for many items: no one
                    // item volume to show or turn.
                    volume: (!is_folder).then_some(item.volume),
                    band,
                    outline: project.title(item).map_or(color, take_color),
                });
                if let Some(name) = project.title(item) {
                    titles.push(Title {
                        row,
                        x0,
                        x1,
                        name: name.to_owned(),
                        spelled: spell(&track.name, name, x0, &keys),
                    });
                }
            }

            index.lanes.push(lanes_from..command_index(&lanes));
            track_h.push(h);
            lane_tracks.push(
                crate::lanes::shown_on(&project.tracks, track)
                    .into_iter()
                    .cloned()
                    .collect(),
            );
            laned_rows.push(opened);
            row_slots.push(slots);
            y += h + crate::automation::lanes_height(&track.guid);
        }
        // The bottom of the last row, so every row has a `..end`.
        offsets.push(y);

        debug_assert_eq!(
            index.x.len(),
            lanes.commands.len(),
            "one x extent per lane command"
        );

        // The panel, cut at zoom 1 to start with. `repanel` re-cuts it
        // whenever the vertical zoom moves.
        let cut = record_panel(palette, font, rows.as_slice(), layout, 1.0, tcp);
        index.panel = cut.spans;
        index.panel_bar = cut.bar_spans;

        Self {
            tcp,
            lanes,
            panel: cut.panel,
            panel_bar: cut.bar,
            panel_zoom: cut.zoom,
            index,
            offsets,
            track_h,
            lane_tracks,
            laned: laned_rows,
            slots: row_slots,
            rows: rows.len(),
            length_secs: project.length_secs,
            bpm: project.bpm,
            tempo: project.tempo.clone(),
            song_start: crate::ruler::song_start_of(project),
            item_count: project.item_count,
            titles,
            items: boxes,
            sections: project.sections.clone(),
            markers: project.markers.clone(),
            lettering: Lettering::from_env(),
            chart: chart_marks(project, &keys),
        }
    }

    /// The item under a point in the lanes, and which part of it.
    ///
    /// `x` is the content x in pixels from the lanes' left edge (the
    /// TCP's right), `content_y` the content y the row was found at.
    /// The parts are REAPER's: the fade handles in the top corners,
    /// the edges, and the body — checked in that order, because a
    /// handle sits on an edge and an edge sits on the body.
    #[must_use]
    pub fn item_at(
        &self,
        view: Viewport,
        row: usize,
        x: f64,
        content_y: f64,
    ) -> Option<(usize, ItemZone)> {
        let scale = view.pps;
        // Last drawn is on top, so the last match wins.
        let mut found = None;
        for (index, item) in self.items.iter().enumerate() {
            if item.row != row {
                continue;
            }
            // Its lane's strip, on a track showing its lanes: a click in
            // another lane is not on it.
            let Some((top, height)) = self.item_box(item) else {
                continue;
            };
            if content_y < top - 1.0 || content_y > top + height + 1.0 {
                continue;
            }
            let x0 = item.x0.mul_add(scale, -view.scroll_x);
            let x1 = item.x1.mul_add(scale, -view.scroll_x);
            if x < x0 - GRAB || x > x1 + GRAB {
                continue;
            }
            // The dial first: it sits in the corner the fade-out handle
            // shares, and is the smaller target.
            // At the right edge of what is on screen of it: a take that
            // runs the song has its corner far off to the right.
            if item.volume.is_some()
                && let Some((cx, cy, r)) = item_dial(
                    x0,
                    x1.min(view.width - view.panel_w - DIAL_EDGE),
                    height * view.zoom_y,
                )
                && (x - cx).hypot((content_y - top).mul_add(view.zoom_y, -cy)) <= r + 2.0
            {
                found = Some((index, ItemZone::Volume));
                continue;
            }
            let in_handle_band = content_y - top <= HANDLE_BAND * view.zoom_y.max(0.1) + 2.0;
            let fade_in_x = item.fades.fade_in.mul_add(scale, x0);
            let fade_out_x = x1 - item.fades.fade_out * scale;
            let zone = if in_handle_band && (x - fade_in_x).abs() <= GRAB {
                ItemZone::FadeIn
            } else if in_handle_band && (x - fade_out_x).abs() <= GRAB {
                ItemZone::FadeOut
            } else if (x - x0).abs() <= EDGE {
                ItemZone::LeftEdge
            } else if (x - x1).abs() <= EDGE {
                ItemZone::RightEdge
            } else {
                ItemZone::Body
            };
            let _ = height;
            found = Some((index, zone));
        }
        found
    }

    /// An item's box, by the index `item_at` gave.
    #[must_use]
    pub fn item(&self, index: usize) -> Option<&ItemBox> {
        self.items.get(index)
    }

    /// Every item's box, in the order they were recorded.
    ///
    /// For the operations that are about a REGION rather than about a
    /// thing the pointer is on — a razor area asks "what is inside this
    /// rectangle", which no amount of hit testing answers.
    ///
    /// Named for the boxes and not for the items because `items()`
    /// already answers how many the scene draws, which is a different
    /// question with the same noun in it.
    #[must_use]
    pub fn item_boxes(&self) -> &[ItemBox] {
        &self.items
    }

    /// Every other edge sitting at `at` on the same row.
    ///
    /// Two items butted together share a boundary: one ends where the
    /// next begins. Grabbing it and moving only one of them opens a gap
    /// or an overlap, which is never what was meant — what the hand is
    /// on is the SEAM, and a seam moves as one thing.
    ///
    /// Answered in seconds with a tolerance, because the times came
    /// from a file and two edges written to be equal are equal to
    /// within a rounding. Returns each neighbour's guid and which of
    /// its own edges is the one that touches.
    #[must_use]
    pub fn edges_at(&self, row: usize, at: f64, except: &str) -> Vec<(String, ItemZone)> {
        /// A millisecond. Below a millisecond two edges are the same
        /// edge however they were written down, and above it they are
        /// two edges somebody meant to put near each other.
        const SAME: f64 = 0.001;
        self.items
            .iter()
            .filter(|item| item.row == row && item.guid != except)
            .filter_map(|item| {
                if (item.x0 - at).abs() <= SAME {
                    Some((item.guid.clone(), ItemZone::LeftEdge))
                } else if (item.x1 - at).abs() <= SAME {
                    Some((item.guid.clone(), ItemZone::RightEdge))
                } else {
                    None
                }
            })
            .collect()
    }

    /// Every region edge and marker sitting at `at`, except the one
    /// being dragged.
    ///
    /// The ruler's [`Self::edges_at`], and the same reason for
    /// existing: two regions that meet share a boundary, and a marker
    /// written on that boundary is part of it. Dragging one and leaving
    /// the others behind opens a gap between sections that were meant
    /// to be contiguous — a song with a hole between the verse and the
    /// chorus.
    ///
    /// The same millisecond, too. Below it two marks are the same
    /// moment however they were written down; above it they are two
    /// moments somebody meant to put near each other.
    ///
    /// `except` is what the hand has hold of. A region is excluded
    /// whole rather than by the edge pressed: a zero-length region has
    /// both its bounds at one time, and dragging its end must not drag
    /// its own start along.
    #[must_use]
    pub fn marks_at(
        &self,
        at: f64,
        except: Option<crate::ruler::On>,
    ) -> Vec<crate::ruler::MarkEdge> {
        use crate::ruler::{MarkEdge, On};
        /// A millisecond — see [`Self::edges_at`].
        const SAME: f64 = 0.001;
        let (skip_region, skip_marker) = match except {
            Some(On::Region { id, .. }) => (Some(id), None),
            Some(On::Marker { id }) => (None, Some(id)),
            _ => (None, None),
        };
        let mut found = Vec::new();
        for section in &self.sections {
            if skip_region == Some(section.id) {
                continue;
            }
            if (section.start - at).abs() <= SAME {
                found.push(MarkEdge::RegionStart {
                    id: section.id,
                    end: section.end,
                });
            }
            if (section.end - at).abs() <= SAME {
                found.push(MarkEdge::RegionEnd {
                    id: section.id,
                    start: section.start,
                });
            }
        }
        for marker in &self.markers {
            if skip_marker != Some(marker.idx) && (marker.at - at).abs() <= SAME {
                found.push(MarkEdge::Marker { id: marker.idx });
            }
        }
        found
    }

    /// An item's box, by its guid.
    #[must_use]
    pub fn item_by_guid(&self, guid: &str) -> Option<&ItemBox> {
        self.items.iter().find(|b| b.guid == guid)
    }

    /// The regions, for the ruler's lanes.
    #[must_use]
    pub fn sections(&self) -> &[daw_ui::studio::project::Section] {
        &self.sections
    }

    /// The markers, for the ruler's lanes.
    #[must_use]
    pub fn markers(&self) -> &[daw_ui::studio::project::Marker] {
        &self.markers
    }

    /// Where the tempo and the signature change, for the ruler's strip
    /// and for the bar grid that has to count through them.
    #[must_use]
    pub fn tempo(&self) -> &[daw_ui::studio::project::TempoChange] {
        &self.tempo
    }

    /// Where the song starts, its `SONGSTART`: the ruler's bar 1.
    #[must_use]
    pub const fn song_start(&self) -> Option<f64> {
        self.song_start
    }

    /// The item titles on the rows a viewport shows, with where each
    /// sits — for the per-frame pass that writes them in pixel space.
    pub fn titles_in(&self, view: Viewport) -> impl Iterator<Item = (&Title, f64, f64)> + '_ {
        let rows = self.visible_rows(view);
        self.titles.iter().filter_map(move |title| {
            if !rows.contains(&title.row) {
                return None;
            }
            let (top, height) = self.row_box(title.row)?;
            Some((title, top, height))
        })
    }
}

/// How far from a fade handle or an item edge a hit still takes it.
const GRAB: f64 = 6.0;

/// How far from an item's end a hit is the edge rather than the body.
const EDGE: f64 = 5.0;

/// How deep the band along an item's top is where the fade handles
/// live, in pixels at zoom 1.
const HANDLE_BAND: f64 = 10.0;

/// The shade a fade takes off the item: what is faded out is darker.
const FADE_SHADE: Color = Color::from_rgba8(0x00, 0x00, 0x00, 0x5c);

/// Which part of an item a point is on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ItemZone {
    Body,
    LeftEdge,
    RightEdge,
    /// The fade-in's handle: the top corner where the fade ends.
    FadeIn,
    /// The fade-out's handle: the top corner where the fade starts.
    FadeOut,
    /// The item volume dial in the top right corner ([`item_dial`]).
    Volume,
}

/// An item as the hit test and the handles see it.
#[derive(Clone, Debug, PartialEq)]
pub struct ItemBox {
    pub row: usize,
    pub guid: String,
    /// The waveform it was recorded with, if its peaks had been read.
    /// Carried so a live pass can draw the same waveform the recorded one
    /// drew — see `slip_overlay`.
    pub wave: Option<std::sync::Arc<crate::midi::Wave>>,
    /// The colour the item was recorded in, for the same reason.
    pub color: Color,
    /// How far into its source the item starts, in seconds — the active
    /// take's offset. What a slip edit changes, and what the waveform
    /// is drawn from.
    pub slip: f64,
    /// Its span, in seconds.
    pub x0: f64,
    pub x1: f64,
    pub fades: Fades,
    /// Its item volume (linear), shown and turned on its dial
    /// ([`item_dial`]). `None` for a shut folder's row.
    pub volume: Option<f64>,
    /// Which part of its row it fills, top to bottom, as fractions: all
    /// of it, or — on a track showing its fixed lanes — its lane's
    /// strip ([`lane_band`]).
    pub band: (f64, f64),
    /// Its outline's colour: the take it was cut from ([`take_color`]),
    /// so every mic of one take — and every slice of it — is ringed alike
    /// while the waveform keeps its track's colour.
    pub outline: Color,
}

/// The colour a take is ringed in, from its name.
///
/// What a recording's takes share across mics (`65-In-260907_1321-01.wav`
/// and `66-Out-260907_1321-01.wav` are one take — the mic's number and
/// name before it set aside), as a hue of its own.
#[must_use]
pub fn take_color(name: &str) -> Color {
    let take = name
        .split_once('-')
        .filter(|(n, _)| !n.is_empty() && n.chars().all(|c| c.is_ascii_digit()))
        .and_then(|(_, rest)| rest.split_once('-'))
        .map_or(name, |(_, take)| take);
    let hash = take.bytes().fold(0xcbf2_9ce4_8422_2325_u64, |h, b| {
        (h ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3)
    });
    // A hue from the hash, at one saturation and lightness: takes told
    // apart by hue alone, none darker or louder than another.
    let hue = f64::from(u16::try_from(hash % 360).unwrap_or(0));
    let (saturation, lightness) = (0.65, 0.62);
    let chroma = (1.0 - 2.0f64.mul_add(lightness, -1.0).abs()) * saturation;
    let second = chroma * (1.0 - ((hue / 60.0) % 2.0 - 1.0).abs());
    let (red, green, blue) = match hue {
        at if at < 60.0 => (chroma, second, 0.0),
        at if at < 120.0 => (second, chroma, 0.0),
        at if at < 180.0 => (0.0, chroma, second),
        at if at < 240.0 => (0.0, second, chroma),
        at if at < 300.0 => (second, 0.0, chroma),
        _ => (chroma, 0.0, second),
    };
    let floor = lightness - chroma / 2.0;
    let byte =
        |part: f64| num_traits::ToPrimitive::to_u8(&((part + floor) * 255.0).round()).unwrap_or(0);
    Color::from_rgb8(byte(red), byte(green), byte(blue))
}

/// An item's two fades: how long, and what shape.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Fades {
    pub fade_in: f64,
    pub fade_out: f64,
    pub in_shape: daw_proto::item::FadeShape,
    pub out_shape: daw_proto::item::FadeShape,
}

impl Fades {
    #[must_use]
    pub fn of(item: &daw_proto::Item) -> Self {
        Self {
            fade_in: item.fade_in_length.as_seconds().max(0.0),
            fade_out: item.fade_out_length.as_seconds().max(0.0),
            in_shape: item.fade_in_shape,
            out_shape: item.fade_out_shape,
        }
    }

    /// The gain a fade shape has reached at `t` of its length, 0..1.
    ///
    /// The plugin's own seven shapes: linear; fast start (the gain is
    /// up quickly) and fast end (it is up late), each with a steeper
    /// cousin; and the slow-start-and-end S, with its steeper cousin.
    #[must_use]
    pub fn gain(shape: daw_proto::item::FadeShape, t: f64) -> f64 {
        use daw_proto::item::FadeShape as S;
        let t = t.clamp(0.0, 1.0);
        match shape {
            S::Linear => t,
            S::FastStart => 1.0 - (1.0 - t).powi(2),
            S::FastStartSteep => 1.0 - (1.0 - t).powi(3),
            S::FastEnd => t.powi(2),
            S::FastEndSteep => t.powi(3),
            S::SlowStartEnd => t * t * (3.0 - 2.0 * t),
            S::SlowStartEndSteep => {
                let s = t * t * (3.0 - 2.0 * t);
                s * s * (3.0 - 2.0 * s)
            }
        }
    }

    /// Both fades as one path: the region over each gain curve, which
    /// is the part of the item the fade takes away.
    ///
    /// `x0..x1` and the fades are in the same unit — seconds for the
    /// recording, pixels for the live overlay — and `scale` turns that
    /// unit into the x axis; `top..bottom` are pixels either way.
    /// `None` when there is no fade to draw.
    #[must_use]
    pub fn path(self, x0: f64, x1: f64, top: f64, bottom: f64, scale: f64) -> Option<BezPath> {
        const STEPS: usize = 12;
        let height = bottom - top;
        if height <= 0.0 || (self.fade_in <= 0.0 && self.fade_out <= 0.0) {
            return None;
        }
        let span = (x1 - x0).max(0.0);
        let fade_in = self.fade_in.min(span);
        let fade_out = self.fade_out.min(span);
        let at = |x: f64| x * scale;
        let y_of = |gain: f64| gain.mul_add(-height, bottom);
        let mut path = BezPath::new();
        if fade_in > 0.0 {
            path.move_to((at(x0), top));
            for k in 0..=STEPS {
                let t = crate::num::coord(k) / crate::num::coord(STEPS);
                let x = fade_in.mul_add(t, x0);
                path.line_to((at(x), y_of(Self::gain(self.in_shape, t))));
            }
            path.close_path();
        }
        if fade_out > 0.0 {
            path.move_to((at(x1), top));
            for k in 0..=STEPS {
                let t = crate::num::coord(k) / crate::num::coord(STEPS);
                let x = x1 - fade_out * t;
                path.line_to((at(x), y_of(Self::gain(self.out_shape, t))));
            }
            path.close_path();
        }
        Some(path)
    }
}

/// The fade handles and the fade being dragged, in pixel space over
/// the lanes: the per-frame pass.
///
/// A handle is drawn on the hovered item — a small square at the top
/// corner where each fade ends — so a fade you can grab is a fade you
/// can see the grab for, and nothing is drawn on the others: twenty
/// thousand handles would be a texture. A fade in flight is drawn
/// whole over its recorded self, so the drag is seen before the
/// recording catches up.
pub fn fade_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    hovered: Option<usize>,
    in_flight: Option<(usize, Fades)>,
) {
    let (ox, oy) = origin;
    let x_of = |t: f64| t.mul_add(view.pps, ox);
    if let Some((index, fades)) = in_flight
        && let Some(item) = scene.item(index)
        && let Some((top, height)) = scene.item_box(item)
    {
        let inset = (height * 0.05).clamp(0.0, 2.0);
        let (top, bottom) = (
            top.mul_add(view.zoom_y, oy) + inset,
            (top + height).mul_add(view.zoom_y, oy) - inset,
        );
        let px = Fades {
            fade_in: fades.fade_in * view.pps,
            fade_out: fades.fade_out * view.pps,
            ..fades
        };
        if let Some(path) = px.path(x_of(item.x0), x_of(item.x1), top, bottom, 1.0) {
            painter.fill(Fill::NonZero, Affine::IDENTITY, FADE_SHADE, None, &path);
            painter.stroke(
                &vello::kurbo::Stroke::new(1.2),
                Affine::IDENTITY,
                palette.text,
                None,
                &path,
            );
        }
    }
    let Some(index) = hovered.or(in_flight.map(|(i, _)| i)) else {
        return;
    };
    let Some(item) = scene.item(index) else {
        return;
    };
    let Some((top, _)) = scene.item_box(item) else {
        return;
    };
    let fades = in_flight.map_or(item.fades, |(_, f)| f);
    let top = top.mul_add(view.zoom_y, oy) + 2.0;
    for x in [
        x_of(item.x0 + fades.fade_in),
        x_of(item.x1 - fades.fade_out),
    ] {
        let r = Rect::new(x - 3.0, top, x + 3.0, top + 6.0);
        painter.fill(Fill::NonZero, Affine::IDENTITY, palette.text, None, &r);
    }
}

/// How far in from the lanes' right edge a dial on an item that runs off
/// it is kept: clear of the scrollbar.
const DIAL_EDGE: f64 = 10.0;

/// Where an item's volume dial sits, in pixels: REAPER's corner knob.
///
/// Its centre and radius, from the item's left and right edges and its
/// lane's height, the centre measured from the lane's top. `None` for an
/// item too small to hold one.
#[must_use]
pub fn item_dial(x0: f64, x1: f64, lane_h: f64) -> Option<(f64, f64, f64)> {
    const R: f64 = 6.0;
    const MARGIN: f64 = 3.0;
    // Room for four radii across and a dial with its margins down.
    const WIDE: f64 = 30.0;
    const TALL: f64 = 18.0;
    let fits = x1 - x0 >= WIDE && lane_h >= TALL;
    fits.then_some((x1 - MARGIN - R, MARGIN + R, R))
}

/// The angle a dial points at for `db`: 0 dB straight up, ±24 dB at
/// ±135°, quieter than −24 at the stop. Radians, clockwise from up.
#[must_use]
pub fn dial_angle(db: f64) -> f64 {
    const SWEEP: f64 = 135.0;
    (db / 24.0).clamp(-1.0, 1.0) * SWEEP.to_radians()
}

/// The item volume dials, in pixel space over the lanes.
///
/// On every item whose volume is not 0 dB — a gain-staged kit shows its
/// gains — and on the one under the pointer; the one being turned at its
/// new value. Only what is on screen.
pub fn volume_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    hovered: Option<usize>,
    turning: Option<(usize, f64)>,
) {
    use vello::kurbo::{Arc, Circle, Line, Point, Stroke};
    let (ox, oy) = origin;
    let x_of = |t: f64| t.mul_add(view.pps, ox);
    for (index, item) in scene.items.iter().enumerate() {
        let Some(volume) = item.volume else { continue };
        let db = turning
            .filter(|(at, _)| *at == index)
            .map_or_else(|| 20.0 * volume.max(1e-9).log10(), |(_, db)| db);
        let shown =
            db.abs() > 0.05 || hovered == Some(index) || turning.is_some_and(|(at, _)| at == index);
        if !shown {
            continue;
        }
        let (x0, x1) = (x_of(item.x0), x_of(item.x1));
        if x1 < 0.0 || x0 > view.width {
            continue;
        }
        let Some((top, height)) = scene.item_box(item) else {
            continue;
        };
        let top = top.mul_add(view.zoom_y, oy);
        if top > view.height || height.mul_add(view.zoom_y, top) < 0.0 {
            continue;
        }
        let Some((cx, cy, r)) = item_dial(
            x0.max(0.0),
            x1.min(view.width - DIAL_EDGE),
            height * view.zoom_y,
        ) else {
            continue;
        };
        let centre = Point::new(cx, top + cy);
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.surface.multiply_alpha(0.85),
            None,
            &Circle::new(centre, r),
        );
        let track = Arc::new(
            centre,
            (r - 1.5, r - 1.5),
            (-225.0_f64).to_radians(),
            270.0_f64.to_radians(),
            0.0,
        );
        painter.stroke(
            &Stroke::new(1.0),
            Affine::IDENTITY,
            palette.text_faint,
            None,
            &track,
        );
        // From 0 dB (up) to the value: the gain at a glance.
        let up = -std::f64::consts::FRAC_PI_2;
        let angle = dial_angle(db);
        let value = Arc::new(centre, (r - 1.5, r - 1.5), up, angle, 0.0);
        painter.stroke(
            &Stroke::new(1.8),
            Affine::IDENTITY,
            palette.text,
            None,
            &value,
        );
        let tip = Point::new(
            (r - 2.0).mul_add((up + angle).cos(), cx),
            (r - 2.0).mul_add((up + angle).sin(), top + cy),
        );
        painter.stroke(
            &Stroke::new(1.2),
            Affine::IDENTITY,
            palette.text,
            None,
            &Line::new(centre, tip),
        );
    }
}

/// The selection and the ghosts, in pixel space over the lanes.
///
/// A selected item wears an outline; an item being moved or trimmed
/// shows where it will land as an outline at the new place, over the
/// recorded item where it still is — the recording catches up on the
/// release.
/// The item being slipped, redrawn at the offset the drag has reached.
///
/// A slip changes which part of the source is under the item, and the
/// source lives in the RECORDED scene — so following the pointer would
/// otherwise mean re-cutting that scene every frame, which is the cost
/// the zoom-y gesture already pays and the one this window is trying
/// not to add to. One item's worth of path a frame instead: the lane
/// under it, the body over that, and the waveform at the new offset,
/// which together are exactly what the recorded pass drew and so cover
/// it completely.
///
/// Drawn under the LANES transform rather than in pixel space, because
/// the waveform is generated from the source's own time — handing it
/// pixels would sample the simulation at pixel numbers.
pub fn slip_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    in_flight: Option<(usize, f64)>,
) {
    let Some((index, slip)) = in_flight else {
        return;
    };
    let Some(item) = scene.item(index) else {
        return;
    };
    let Some((top, height)) = scene.item_box(item) else {
        return;
    };
    // The same three numbers the record pass works from, so the cover
    // lands on the recorded item rather than near it.
    let body = (height - DIVIDER).max(0.5);
    let inset = (body * 0.05).clamp(0.0, 2.0);
    let stripe = if item.row % 2 == 0 {
        palette.row_a
    } else {
        palette.row_b
    };
    let at = Affine::scale_non_uniform(view.pps, view.zoom_y).then_translate(origin.into());
    let lane = Rect::new(item.x0, top, item.x1, top + body);
    painter.fill(Fill::NonZero, at, stripe, None, &lane);
    let (y0, y1) = (top + inset, top + body - inset);
    painter.fill(
        Fill::NonZero,
        at,
        item.color.multiply_alpha(body_alpha()),
        None,
        &Rect::new(item.x0, y0, item.x1, y1),
    );
    // The recorded peaks, moved by how far the drag has slipped the
    // source under the item: past the audio's end they run out, and the
    // item shows empty there until the release re-reads them.
    if let Some(path) = item
        .wave
        .as_deref()
        .and_then(|wave| waveform(wave, item.x0, item.x1, y0, y1, slip - item.slip))
    {
        painter.fill(Fill::NonZero, at, item.color, None, &path);
    }
    // And the fades back over it. The cover took them with the rest of
    // the recorded item, and an item that lost its fade shading for the
    // length of a drag would be telling you the fades had gone.
    if let Some(path) = item.fades.path(item.x0, item.x1, y0, y1, 1.0) {
        painter.fill(Fill::NonZero, at, FADE_SHADE, None, &path);
    }
}

/// The razor areas, over the lanes.
///
/// Drawn as a filled rectangle with a hard edge down each side,
/// because the EDGES are the operative part: an area slices there, and
/// a soft-edged wash would be a marquee — a thing that selects what it
/// covers rather than cutting where it stops. The fill is the accent
/// at low alpha so the waveform under it stays readable; you are
/// choosing a piece of that waveform and need to see which piece.
///
/// `in_flight` is the area being drawn right now, which is not in the
/// set yet — a drag that added to the set every frame would merge with
/// itself and grow in one direction only.
pub fn razor_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    set: &razor::RazorSet,
    in_flight: Option<razor::RazorArea>,
) {
    let (ox, oy) = origin;
    let x_of = |t: f64| t.mul_add(view.pps, ox);
    let visible = scene.visible_rows(view);
    for area in set.areas.iter().copied().chain(in_flight) {
        if area.is_empty() {
            continue;
        }
        // Row by row rather than one rectangle from the first row's top
        // to the last row's bottom: rows are not all the same height,
        // and a rectangle drawn between two of them would cover the
        // ones between at whatever height the maths happened to give.
        let (lo, hi) = (area.row_lo.max(0), area.row_hi.max(0));
        for row in lo..=hi {
            let Ok(row) = usize::try_from(row) else {
                continue;
            };
            if !visible.contains(&row) {
                continue;
            }
            let Some((top, height)) = scene.row_box(row) else {
                continue;
            };
            let r = Rect::new(
                x_of(area.t0),
                top.mul_add(view.zoom_y, oy),
                x_of(area.t1),
                (top + height).mul_add(view.zoom_y, oy),
            );
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                palette.accent.multiply_alpha(0.18),
                None,
                &r,
            );
            for edge in [r.x0, r.x1] {
                painter.fill(
                    Fill::NonZero,
                    Affine::IDENTITY,
                    palette.accent,
                    None,
                    &Rect::new(edge - 0.5, r.y0, edge + 0.5, r.y1),
                );
            }
        }
    }
}

/// The marquee being swept: `(from, to)` in seconds and session-unit
/// heights from the top of the lanes. Drawn as the Editor draws its own —
/// a wash of the accent, edged in it.
pub fn marquee_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    view: Viewport,
    origin: (f64, f64),
    marquee: Option<((f64, f64), (f64, f64))>,
) {
    let Some(((t0, y0), (t1, y1))) = marquee else {
        return;
    };
    let (ox, oy) = origin;
    let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
    let r = Rect::new(
        t0.min(t1).mul_add(view.pps, ox),
        y0.min(y1).mul_add(zoom, oy),
        t0.max(t1).mul_add(view.pps, ox),
        y0.max(y1).mul_add(zoom, oy),
    );
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.accent.multiply_alpha(0.15),
        None,
        &r,
    );
    painter.stroke(
        &vello::kurbo::Stroke::new(1.0),
        Affine::IDENTITY,
        palette.accent,
        None,
        &r,
    );
}

/// The kit's hit markers ([`crate::hit_overlay`]), each on its own piece.
///
/// The fill zones are shaded over the kit's rows `kit`, and each hit is a
/// line on its own piece's rows — `rows` pairs a row with the piece it is
/// ([`Overlay::pieces`] index) — so the kick's hits are on the kick and
/// nowhere else.
///
/// A line in its drum's colour — the kick's red, the snare's blue, the
/// toms' purple, as their waveforms are drawn — or the meters' warn for
/// one to look at (bleed, or found unsure); a ghost note drawn to half the
/// row. A flag at its head, where the hits are far enough apart to read:
/// solid when placed by hand or checked, hollow when found and unchecked.
///
/// [`Overlay::pieces`]: crate::hit_overlay::Overlay::pieces
pub fn hits_overlay(
    painter: &mut impl PaintScene,
    (palette, font): (&Palette, &crate::text::Font),
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    (kit, rows): (std::ops::Range<usize>, &[(usize, usize)]),
    (overlay, selected): (
        &crate::hit_overlay::Overlay,
        &std::collections::HashSet<String, impl std::hash::BuildHasher>,
    ),
) {
    use crate::hit_overlay::Who;
    /// Hits nearer than this many pixels draw no flag, and fainter.
    const CLEAR: f64 = 6.0;
    let (ox, oy) = origin;
    let top = crate::ruler::ruler_h();
    let y_span = |row: usize| {
        let (y, h) = scene.row_box(row)?;
        let y0 = y.mul_add(view.zoom_y, oy).max(top);
        let y1 = (y + h).mul_add(view.zoom_y, oy).min(view.height);
        (y1 > y0).then_some((y0, y1))
    };
    let x_of = |t: f64| t.mul_add(view.pps, ox);
    let pps = view.pps.max(f64::EPSILON);
    let (from, to) = ((view.panel_w - ox) / pps, (view.width - ox) / pps);
    if let (Some((y0, _)), Some((_, y1))) =
        (kit.clone().find_map(y_span), kit.rev().find_map(y_span))
    {
        for zone in &overlay.zones {
            if zone.end < from || zone.start > to {
                continue;
            }
            let tint = if zone.found {
                palette.meter_warn
            } else {
                palette.accent
            };
            let r = Rect::new(
                x_of(zone.start).max(view.panel_w),
                y0,
                x_of(zone.end).min(view.width),
                y1,
            );
            painter.fill(
                Fill::NonZero,
                Affine::IDENTITY,
                tint.multiply_alpha(0.10),
                None,
                &r,
            );
        }
    }
    let marks = overlay.between(from, to);
    let thin = vello::kurbo::Stroke::new(1.0);
    let heavy = vello::kurbo::Stroke::new(1.75);
    // A row showing its lanes: the hits are where they play, on the lane
    // that plays — its strip, not every take's.
    let marks_span = |row: usize| {
        let slots = scene.lane_slots(row);
        let playing = scene.lanes_shown(row).and_then(crate::lanes::playing);
        let (Some(lane), false) = (playing, slots.is_empty()) else {
            return y_span(row);
        };
        let (y, h) = scene.row_box(row)?;
        let (b0, b1) = slot_band(lane, slots);
        let y0 = b0.mul_add(h, y).mul_add(view.zoom_y, oy).max(top);
        let y1 = b1.mul_add(h, y).mul_add(view.zoom_y, oy).min(view.height);
        (y1 > y0).then_some((y0, y1))
    };
    // Each row's stretched stretches along its foot, once a row: a shut
    // kit's row is every piece's, and their slices are stretched as one.
    let mut shown_rows: Vec<usize> = Vec::new();
    for &(row, _) in rows {
        if shown_rows.contains(&row) {
            continue;
        }
        shown_rows.push(row);
        let Some((y0, y1)) = marks_span(row) else {
            continue;
        };
        let pieces: Vec<usize> = rows
            .iter()
            .filter(|(r, _)| *r == row)
            .map(|(_, p)| *p)
            .collect();
        stretch_strip(
            painter,
            font,
            (y0, y1),
            (x_of, (from, to)),
            overlay
                .stretches
                .iter()
                .filter(|s| pieces.contains(&s.piece)),
        );
    }
    for &(row, piece) in rows {
        let Some((y0, y1)) = marks_span(row) else {
            continue;
        };
        let mine: Vec<&crate::hit_overlay::Mark> =
            marks.iter().filter(|m| m.piece == piece).collect();
        let drum = overlay
            .pieces
            .get(piece)
            .and_then(|name| kit_color(expression_editor_core::kit::kit_role(name, &[])))
            .unwrap_or(palette.text);
        let spaced = |k: usize| {
            let x = mine.get(k).map_or(0.0, |m| x_of(m.time));
            let gap = |j: Option<usize>| {
                j.and_then(|j| mine.get(j))
                    .map_or(f64::INFINITY, |m| (x_of(m.time) - x).abs())
            };
            gap(k.checked_sub(1)).min(gap(k.checked_add(1))) >= CLEAR
        };
        for (k, m) in mine.iter().enumerate() {
            use crate::hit_overlay::Edited;
            let colour = if m.doubt { palette.meter_warn } else { drum };
            // What has been done to it, at a glance: as played, the drum's
            // colour a little dimmed; quantized, brighter; placed by hand,
            // brighter still and heavier — its flag white or amber.
            let white = Color::from_rgba8(0xff, 0xff, 0xff, 0xff);
            let (colour, line, flag_colour) = match m.edited {
                Edited::AsPlayed => (colour.multiply_alpha(0.7), &thin, None),
                Edited::Quantized => (crate::tcp::mix(colour, white, 0.3), &thin, Some(white)),
                Edited::ByHand => (
                    crate::tcp::mix(colour, white, 0.55),
                    &heavy,
                    Some(Color::from_rgba8(0xfb, 0xbf, 0x24, 0xff)),
                ),
            };
            // Selected (to drag together): the accent, whatever it is.
            let (colour, line, flag_colour) = if selected.contains(&m.id) {
                (palette.accent, &heavy, Some(palette.accent))
            } else {
                (colour, line, flag_colour)
            };
            let clear = spaced(k);
            let colour = colour.multiply_alpha(if clear { 0.85 } else { 0.45 });
            let x = x_of(m.time).round() + 0.5;
            let foot = if m.ghost { f64::midpoint(y0, y1) } else { y1 };
            painter.stroke(
                line,
                Affine::IDENTITY,
                colour,
                None,
                &vello::kurbo::Line::new((x, y0), (x, foot)),
            );
            let colour =
                flag_colour.map_or(colour, |c| c.multiply_alpha(if clear { 0.95 } else { 0.5 }));
            if clear && y1 - y0 > 10.0 {
                let mut flag = vello::kurbo::BezPath::new();
                flag.move_to((x - 3.5, y0));
                flag.line_to((x + 3.5, y0));
                flag.line_to((x, y0 + 5.0));
                flag.close_path();
                if m.who == Who::Auto {
                    painter.stroke(&thin, Affine::IDENTITY, colour, None, &flag);
                } else {
                    painter.fill(Fill::NonZero, Affine::IDENTITY, colour, None, &flag);
                }
            }
        }
    }
}

/// A stretch's colour by its rate: red at half speed or slower (or twice
/// as fast), amber past [`crate::arrange_edit::STRETCH_LEAST_RATE`] —
/// a stretch to listen to — and a faint blue within it.
fn stretch_tint(rate: f64) -> Color {
    if rate <= 0.5 || rate >= 2.0 {
        Color::from_rgba8(0xef, 0x44, 0x44, 0xff)
    } else if !(crate::arrange_edit::STRETCH_LEAST_RATE - 1e-6
        ..=1.0 / crate::arrange_edit::STRETCH_LEAST_RATE + 1e-6)
        .contains(&rate)
    {
        Color::from_rgba8(0xf5, 0x9e, 0x0b, 0xff)
    } else {
        Color::from_rgba8(0x60, 0xa5, 0xfa, 0xff)
    }
}

/// A row's stretched stretches along its foot, between `y0` and `y1`: a
/// bar over each — a faint blue played near its speed (0.85 or faster),
/// amber slower, red at half speed or slower, where the stretch has been
/// heard as a stretch. Each drawn-out slice (its stretches end to end)
/// says its slowest rate just above them, in that rate's colour
/// (`0.12×`), where there is room before the next. The same stretch on
/// several pieces' slices drawn once.
fn stretch_strip<'a>(
    painter: &mut impl PaintScene,
    font: &crate::text::Font,
    (y0, y1): (f64, f64),
    (x_of, (from, to)): (impl Fn(f64) -> f64, (f64, f64)),
    stretches: impl Iterator<Item = &'a crate::hit_overlay::Stretch>,
) {
    /// The strip's height, pixels; the least row it is drawn on is three.
    const STRIP: f64 = 8.0;
    /// The rate's size, and the room it takes, pixels.
    const SIZE: f32 = 9.0;
    const ROOM: f64 = 30.0;
    /// Stretches nearer than this end to end are one slice's, seconds.
    const JOINED: f64 = 0.000_5;
    if y1 - y0 < STRIP * 3.0 {
        return;
    }
    let (strip_top, base) = (y1 - STRIP - 1.0, y1 - 1.0);
    // In view, each once, by start.
    let mut shown: Vec<&crate::hit_overlay::Stretch> = Vec::new();
    for s in stretches.filter(|s| s.end >= from && s.start <= to) {
        if !shown
            .iter()
            .any(|d| (d.start - s.start).abs() < JOINED && (d.end - s.end).abs() < JOINED)
        {
            shown.push(s);
        }
    }
    shown.sort_by(|a, b| a.start.total_cmp(&b.start));
    for s in &shown {
        let tint = stretch_tint(s.rate);
        let (x0, x1) = (x_of(s.start), x_of(s.end).max(x_of(s.start) + 1.0));
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            tint.multiply_alpha(0.45),
            None,
            &Rect::new(x0, strip_top, x1, base),
        );
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            tint,
            None,
            &Rect::new(x0, base - 2.0, x1, base),
        );
    }
    // Each slice's slowest, at its start, above the strip.
    let mut k = 0_usize;
    while let Some(first) = shown.get(k) {
        let mut end = k.saturating_add(1);
        while let (Some(prev), Some(next)) = (shown.get(end.saturating_sub(1)), shown.get(end)) {
            if next.start - prev.end > JOINED {
                break;
            }
            end = end.saturating_add(1);
        }
        let slowest = shown
            .get(k..end)
            .unwrap_or_default()
            .iter()
            .map(|s| s.rate)
            .min_by(f64::total_cmp)
            .unwrap_or(1.0);
        let room = shown
            .get(end)
            .map_or(f64::INFINITY, |next| x_of(next.start) - x_of(first.start));
        if room >= ROOM {
            crate::tcp::glyphs(
                painter,
                font,
                stretch_tint(slowest),
                &format!("{slowest:.2}×"),
                x_of(first.start),
                strip_top - 2.0,
                SIZE,
            );
        }
        k = end;
    }
}

/// A marquee being drawn over the kit's rows, from `a` to `b`.
pub fn hit_marquee_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    (a, b): ((f64, f64), (f64, f64)),
) {
    let rect = Rect::new(a.0.min(b.0), a.1.min(b.1), a.0.max(b.0), a.1.max(b.1));
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        palette.accent.multiply_alpha(0.12),
        None,
        &rect,
    );
    painter.stroke(
        &vello::kurbo::Stroke::new(1.0),
        Affine::IDENTITY,
        palette.accent.multiply_alpha(0.8),
        None,
        &rect,
    );
}

/// A hit marker's flag being dragged: where it will go, a line in the
/// accent from the ruler down.
pub fn hit_drag_overlay(painter: &mut impl PaintScene, palette: &Palette, view: Viewport, x: f64) {
    if x < view.panel_w || x > view.width {
        return;
    }
    let top = crate::ruler::ruler_h();
    let x = x.round() + 0.5;
    painter.stroke(
        &vello::kurbo::Stroke::new(1.5),
        Affine::IDENTITY,
        palette.accent,
        None,
        &vello::kurbo::Line::new((x, top), (x, view.height)),
    );
    let mut flag = vello::kurbo::BezPath::new();
    flag.move_to((x - 4.0, top));
    flag.line_to((x + 4.0, top));
    flag.line_to((x, top + 6.0));
    flag.close_path();
    painter.fill(Fill::NonZero, Affine::IDENTITY, palette.accent, None, &flag);
}

/// An item body's strength: faint when items are drawn outlined
/// ([`crate::options::OUTLINED`]), so the waveform carries the colour.
/// How much of an item's body alpha a shut folder's item has.
const FOLDER_BODY: f32 = 0.4;

fn body_alpha() -> f32 {
    if crate::options::OUTLINED.get() {
        0.12
    } else {
        0.42
    }
}

/// Every item's outline on screen ([`crate::options::OUTLINED`]).
///
/// Drawn per frame, a pixel wide in each item's own colour. Where a box
/// starts as the one before ends — a cut on a shut folder's row, the
/// slices of an edited take — the line between them is the cut.
///
/// `quiet` (the kit's hit markers shown over it): a cut is a faint
/// neutral line, so the markers — what is being worked on — read over a
/// kit of a thousand slices.
pub fn outlines_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    quiet: bool,
) {
    /// Item edges this close are one edge: a cut.
    const TOUCH: f64 = 1e-3;
    /// Cuts this many pixels apart or more draw at their strongest; closer
    /// (zoomed out over an edited kit) fainter, down to a tint.
    const CLEAR: f64 = 24.0;
    let (ox, oy) = origin;
    let pps = view.pps.max(f64::EPSILON);
    let (from, to) = ((view.panel_w - ox) / pps, (view.width - ox) / pps);
    let rows = scene.visible_rows(view);
    let thin = vello::kurbo::Stroke::new(1.0);
    let top_of_lanes = crate::ruler::ruler_h();
    // The items on screen, each row's in order along it.
    let mut shown: Vec<&ItemBox> = scene
        .items
        .iter()
        .filter(|i| rows.contains(&i.row) && i.x1 >= from && i.x0 <= to)
        .collect();
    shown.sort_by(|a, b| a.row.cmp(&b.row).then(a.x0.total_cmp(&b.x0)));
    let mut last_cut: Option<(usize, f64)> = None;
    for (k, item) in shown.iter().enumerate() {
        let Some((top, height)) = scene.item_box(item) else {
            continue;
        };
        let y0 = top.mul_add(view.zoom_y, oy).max(top_of_lanes);
        let y1 = (top + height).mul_add(view.zoom_y, oy).min(view.height);
        if y1 <= y0 {
            continue;
        }
        let x0 = item.x0.mul_add(pps, ox).max(view.panel_w);
        let x1 = item.x1.mul_add(pps, ox).min(view.width);
        if x1 - x0 < 1.0 {
            continue;
        }
        let line = |painter: &mut _, ink: Color, a: (f64, f64), b: (f64, f64)| {
            PaintScene::stroke(
                painter,
                &thin,
                Affine::IDENTITY,
                ink,
                None,
                &vello::kurbo::Line::new(a, b),
            );
        };
        let ink = item.outline.multiply_alpha(0.9);
        // Its top and bottom in its take's colour.
        line(painter, ink, (x0, y0 + 0.5), (x1, y0 + 0.5));
        line(painter, ink, (x0, y1 - 0.5), (x1, y1 - 0.5));
        // Its start: a cut when the one before on its row ends there —
        // red, fainter the closer the cuts sit — else its take's edge.
        let before = k.checked_sub(1).and_then(|b| shown.get(b));
        let cut = before.is_some_and(|b| b.row == item.row && b.x1 + TOUCH >= item.x0);
        if cut {
            let gap = last_cut
                .filter(|(row, _)| *row == item.row)
                .map_or(CLEAR, |(_, x)| x0 - x);
            let strength = (gap / CLEAR).clamp(0.12, 1.0);
            let (cut_ink, most) = if quiet {
                (palette.divider, 0.35)
            } else {
                (palette.meter_danger, 0.7)
            };
            line(
                painter,
                cut_ink.multiply_alpha(
                    num_traits::ToPrimitive::to_f32(&(most * strength)).unwrap_or(0.5),
                ),
                (x0 + 0.5, y0),
                (x0 + 0.5, y1),
            );
            last_cut = Some((item.row, x0));
        } else {
            line(painter, ink, (x0 + 0.5, y0), (x0 + 0.5, y1));
        }
        // Its end, unless the next one starts there (whose cut it is).
        let after = shown.get(k.saturating_add(1));
        if !after.is_some_and(|a| a.row == item.row && item.x1 + TOUCH >= a.x0) {
            line(painter, ink, (x1 - 0.5, y0), (x1 - 0.5, y1));
        }
    }
}

/// Every cut on a shut folder's row (Edit mode).
///
/// A thin line where one of its boxes starts as the one before it ends.
/// The boxes are the mics' slices folded, the same colour edge to edge, so
/// without a line a kit edited into a thousand slices reads as one take.
/// Drawn per frame, a pixel wide at any zoom, for the rows and the stretch
/// on screen only.
pub fn cuts_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    rows: &[(daw_proto::Track, u32)],
    project: &daw_ui::studio::project::Project,
    quiet: bool,
) {
    /// Ends this close are one edge.
    const TOUCH: f64 = 1e-3;
    let (ox, oy) = origin;
    let pps = view.pps.max(f64::EPSILON);
    let (from, to) = ((view.panel_w - ox) / pps, (view.width - ox) / pps);
    let thin = vello::kurbo::Stroke::new(1.0);
    // Under the hit markers, fainter: the markers are what is read.
    let ink = if quiet {
        palette.divider.multiply_alpha(0.4)
    } else {
        palette.divider
    };
    for index in scene.visible_rows(view) {
        let Some((track, _)) = rows.get(index) else {
            continue;
        };
        let Some(fold) = project.folds.get(&track.guid) else {
            continue;
        };
        let Some((top, h)) = scene.row_box(index) else {
            continue;
        };
        // A folder showing its lanes: each lane's cuts in its own strip,
        // between its own items — every lane's are by start, not all of
        // them together.
        let slots = scene.lane_slots(index);
        let laned = !slots.is_empty();
        let whole = [0_u32];
        for &lane in if laned { slots } else { &whole } {
            let band = if laned {
                slot_band(lane, slots)
            } else {
                (0.0, 1.0)
            };
            let y0 = band
                .0
                .mul_add(h, top)
                .mul_add(view.zoom_y, oy)
                .max(crate::ruler::ruler_h());
            let y1 = band
                .1
                .mul_add(h, top)
                .mul_add(view.zoom_y, oy)
                .min(view.height);
            if y1 <= y0 {
                continue;
            }
            let owned: Vec<daw_proto::Item>;
            let items: &[daw_proto::Item] = if laned {
                owned = fold
                    .items
                    .iter()
                    .filter(|i| i.fixed_lane.unwrap_or(0) == lane)
                    .cloned()
                    .collect();
                &owned
            } else {
                &fold.items
            };
            // Sorted by start, so the stretch on screen is found, not walked.
            let first =
                items.partition_point(|i| i.position.as_seconds() + i.length.as_seconds() < from);
            let mut last_x = f64::NEG_INFINITY;
            for pair in items
                .get(first.saturating_sub(1)..)
                .unwrap_or(&[])
                .windows(2)
            {
                let [before, item] = pair else { continue };
                let at = item.position.as_seconds();
                if at > to {
                    break;
                }
                let ends = before.position.as_seconds() + before.length.as_seconds();
                if ends + TOUCH < at {
                    continue; // a gap: the boxes part on their own
                }
                let x = at.mul_add(pps, ox);
                // Zoomed out, one line a pixel.
                if x - last_x < 1.0 {
                    continue;
                }
                last_x = x;
                painter.stroke(
                    &thin,
                    Affine::IDENTITY,
                    ink,
                    None,
                    &vello::kurbo::Line::new((x, y0), (x, y1)),
                );
            }
        }
    }
}

/// Every gap under a shut folder's row (Edit mode): where a track under
/// it plays nothing between one of its slices and the next, shaded.
///
/// The row folds every track under it into one: a mic dropping out for
/// 20 ms between two slices hides under the others still playing, and is
/// heard — the kick's close mic cutting out where a quantize pulled two
/// strokes apart. A slice's crossfade into the next is no gap, and not
/// drawn; a gap is shaded at least a couple of pixels wide, so it shows at
/// any zoom.
pub fn gaps_overlay(
    painter: &mut impl PaintScene,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    rows: &[(daw_proto::Track, u32)],
    project: &daw_ui::studio::project::Project,
) {
    /// Ends this close are one edge.
    const TOUCH: f64 = 1e-3;
    /// The least a gap is drawn, pixels.
    const LEAST: f64 = 2.0;
    let ink = Color::from_rgba8(0xef, 0x44, 0x44, 0x8c);
    let (ox, oy) = origin;
    let pps = view.pps.max(f64::EPSILON);
    let (from, to) = ((view.panel_w - ox) / pps, (view.width - ox) / pps);
    for index in scene.visible_rows(view) {
        let Some((track, _)) = rows.get(index) else {
            continue;
        };
        let Some(fold) = project.folds.get(&track.guid) else {
            continue;
        };
        let Some((top, h)) = scene.row_box(index) else {
            continue;
        };
        let y0 = top.mul_add(view.zoom_y, oy).max(crate::ruler::ruler_h());
        let y1 = (top + h).mul_add(view.zoom_y, oy).min(view.height);
        if y1 <= y0 {
            continue;
        }
        // What the row is folded from, where it is on screen.
        let children: std::collections::HashSet<&str> = fold
            .spans
            .iter()
            .filter(|s| s.end >= from && s.start <= to)
            .flat_map(|s| s.from.iter().map(String::as_str))
            .collect();
        if children.is_empty() {
            continue;
        }
        for items in project.items.values() {
            let mut mine: Vec<(f64, f64)> = items
                .iter()
                .filter(|i| children.contains(i.guid.as_str()))
                .map(|i| {
                    let at = i.position.as_seconds();
                    (at, at + i.length.as_seconds())
                })
                .collect();
            mine.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut reach = f64::NEG_INFINITY;
            for (start, end) in mine {
                if start > reach + TOUCH && reach.is_finite() {
                    let (x0, x1) = (reach.mul_add(pps, ox), start.mul_add(pps, ox));
                    let mid = f64::midpoint(x0, x1);
                    let half = ((x1 - x0) / 2.0).max(LEAST / 2.0);
                    painter.fill(
                        Fill::NonZero,
                        Affine::IDENTITY,
                        ink,
                        None,
                        &Rect::new(mid - half, y0, mid + half, y1),
                    );
                }
                reach = reach.max(end);
            }
        }
    }
}

pub fn selection_overlay(
    painter: &mut impl PaintScene,
    palette: &Palette,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
    selected: &std::collections::HashSet<String>,
    ghost: Option<(usize, f64, f64)>,
) {
    let (ox, oy) = origin;
    let x_of = |t: f64| t.mul_add(view.pps, ox);
    let rows = scene.visible_rows(view);
    for item in scene
        .items
        .iter()
        .filter(|i| rows.contains(&i.row) && selected.contains(&i.guid))
    {
        let Some((top, height)) = scene.item_box(item) else {
            continue;
        };
        let inset = (height * 0.05).clamp(0.0, 2.0);
        let r = Rect::new(
            x_of(item.x0),
            top.mul_add(view.zoom_y, oy) + inset,
            x_of(item.x1),
            (top + height).mul_add(view.zoom_y, oy) - inset,
        );
        painter.stroke(
            &vello::kurbo::Stroke::new(1.5),
            Affine::IDENTITY,
            palette.text,
            None,
            &r.inset(-0.75),
        );
    }
    if let Some((index, x0, x1)) = ghost
        && let Some(item) = scene.item(index)
        && let Some((top, height)) = scene.item_box(item)
    {
        let inset = (height * 0.05).clamp(0.0, 2.0);
        let r = Rect::new(
            x_of(x0),
            top.mul_add(view.zoom_y, oy) + inset,
            x_of(x1),
            (top + height).mul_add(view.zoom_y, oy) - inset,
        );
        painter.fill(
            Fill::NonZero,
            Affine::IDENTITY,
            palette.text.multiply_alpha(0.12),
            None,
            &r,
        );
        painter.stroke(
            &vello::kurbo::Stroke::new(1.0),
            Affine::IDENTITY,
            palette.text,
            None,
            &r,
        );
    }
}

/// An item's name and where it sits: the row, and its span in seconds.
#[derive(Clone, Debug, PartialEq)]
pub struct Title {
    pub row: usize,
    pub x0: f64,
    pub x1: f64,
    pub name: String,
    /// A Keyflow chord or key item's words both ways, for the lettered
    /// views. `None` for every other item.
    pub spelled: Option<Spelled>,
}

/// What the items say on their faces.
///
/// `Titles` is every item's name, small, in its corner. The other two
/// are for reading the song off the Keyflow folder from across a room:
/// its chord items (and its key) lettered as large as the item holds —
/// the chords as NUMBERS in the song's key, or as chord NAMES. Items
/// that are not chords keep their titles either way.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Lettering {
    #[default]
    Titles,
    Numbers,
    Chords,
}

impl Lettering {
    /// `FTS_LETTERING=numbers|chords` — how a window opens. Anything
    /// else is the titles.
    #[must_use]
    pub fn from_env() -> Self {
        match std::env::var("FTS_LETTERING")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "numbers" | "nashville" => Self::Numbers,
            "chords" | "names" => Self::Chords,
            _ => Self::Titles,
        }
    }

    /// The next view round: titles, numbers, chords, titles.
    #[must_use]
    pub const fn next(self) -> Self {
        match self {
            Self::Titles => Self::Numbers,
            Self::Numbers => Self::Chords,
            Self::Chords => Self::Titles,
        }
    }
}

/// A chord item's symbol as a number and as a chord name, in the key
/// that holds where it starts. A key item says its key both ways.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Spelled {
    pub numbers: String,
    pub chords: String,
}

/// A chord on the ruler's CHORDS lane: its span, and its symbol both ways.
#[derive(Clone, Debug, PartialEq)]
pub struct ChordMark {
    pub x0: f64,
    pub x1: f64,
    pub spelled: Spelled,
}

/// A key change on the CHORDS lane: where, and the key as its item names
/// it.
#[derive(Clone, Debug, PartialEq)]
pub struct KeyMark {
    pub at: f64,
    pub name: String,
}

/// Everything the CHORDS lane draws.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct ChartMarks {
    pub chords: Vec<ChordMark>,
    pub keys: Vec<KeyMark>,
}

/// The Keyflow folder's KEY and CHORD items as the ruler shows them.
///
/// From every track, not the visible rows: the folder is hidden from the
/// panel once a session is prepared, and the lane is where it is read.
/// Built with the recording, so an edit to the folder — made with it
/// opened — is in the lane on the next cut.
fn chart_marks(
    project: &daw_ui::studio::project::Project,
    keys: &[(f64, keyflow::key::Key)],
) -> ChartMarks {
    let mut marks = ChartMarks::default();
    for track in &project.tracks {
        let name = track.name.trim();
        let is_key = name.eq_ignore_ascii_case(session::key::KEY_TRACK);
        if !is_key && !name.eq_ignore_ascii_case(CHORD_TRACK) {
            continue;
        }
        for item in project.lane(&track.guid) {
            let Some(written) = project.title(item) else {
                continue;
            };
            let x0 = item.position.as_seconds();
            if is_key {
                if session::key::parse_key(written).is_some() {
                    marks.keys.push(KeyMark {
                        at: x0,
                        name: written.to_owned(),
                    });
                }
            } else if let Some(spelled) = spell(name, written, x0, keys) {
                marks.chords.push(ChordMark {
                    x0,
                    x1: x0 + item.length.as_seconds().max(0.001),
                    spelled,
                });
            }
        }
    }
    marks.chords.sort_by(|a, b| a.x0.total_cmp(&b.x0));
    marks.keys.sort_by(|a, b| a.at.total_cmp(&b.at));
    marks
}

/// The track the chart's chords are stamped on, one item a chord,
/// named as written (`session::keyflow::from_chart`).
const CHORD_TRACK: &str = "CHORD";

/// Every key change on the KEY track, by where it starts, in order.
///
/// Read from every track rather than the visible rows, so a collapsed
/// Keyflow folder still spells its chords in the right key.
fn keys_of(project: &daw_ui::studio::project::Project) -> Vec<(f64, keyflow::key::Key)> {
    let mut keys: Vec<(f64, keyflow::key::Key)> = project
        .tracks
        .iter()
        .filter(|t| t.name.trim().eq_ignore_ascii_case(session::key::KEY_TRACK))
        .flat_map(|t| project.lane(&t.guid))
        .filter_map(|item| {
            let key = session::key::parse_key(project.title(item)?)?;
            Some((item.position.as_seconds(), key))
        })
        .collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    keys
}

/// The key in force at `at`, as the KEY track's item names it ("F major"):
/// the last change at or before it, or the first if `at` comes before them
/// all. `None` for a session with no key items.
#[must_use]
pub fn key_at(project: &daw_ui::studio::project::Project, at: f64) -> Option<String> {
    let mut keys: Vec<(f64, &str)> = project
        .tracks
        .iter()
        .filter(|t| t.name.trim().eq_ignore_ascii_case(session::key::KEY_TRACK))
        .flat_map(|t| project.lane(&t.guid))
        .filter_map(|item| {
            let name = project.title(item)?;
            session::key::parse_key(name).map(|_| (item.position.as_seconds(), name))
        })
        .collect();
    keys.sort_by(|a, b| a.0.total_cmp(&b.0));
    keys.iter()
        .rev()
        .find(|(from, _)| *from <= at + 1e-6)
        .or_else(|| keys.first())
        .map(|(_, name)| (*name).to_owned())
}

/// A title's two spellings, if its track is the chord or the key track.
///
/// A chord takes the key in force where it starts — the last change at
/// or before it, or the first one if it comes before them all. A symbol
/// that is not a chord, or a song with no key, reads as written.
fn spell(
    track: &str,
    written: &str,
    at: f64,
    keys: &[(f64, keyflow::key::Key)],
) -> Option<Spelled> {
    let track = track.trim();
    if track.eq_ignore_ascii_case(session::key::KEY_TRACK) {
        return Some(Spelled {
            numbers: written.to_owned(),
            chords: written.to_owned(),
        });
    }
    if !track.eq_ignore_ascii_case(CHORD_TRACK) {
        return None;
    }
    // A hair of slack: a chord stamped on the key change's own beat
    // must not read in the key before it for a rounding error.
    let key = keys
        .iter()
        .rev()
        .find(|(from, _)| *from <= at + 1e-6)
        .or_else(|| keys.first())
        .map(|(_, key)| key);
    let as_ = |notation| {
        key.and_then(|key| keyflow::renotate_symbol(written, key, notation))
            .unwrap_or_else(|| written.to_owned())
    };
    Some(Spelled {
        numbers: as_(keyflow::NotationSystem::Nashville),
        chords: as_(keyflow::NotationSystem::Letters),
    })
}

/// The least a note is drawn as, in seconds — the recording's unit.
const MIN_NOTE_SECS: f64 = 0.02;

/// An item's notes as one path: a block per note, stacked by pitch.
///
/// A piano roll squeezed into a lane. The pitch range is the item's
/// own rather than the full 0..127, so a bass part fills its block
/// instead of hugging the floor of a scale it never plays in — what a
/// preview is for is the SHAPE of the part, and a shape pressed into
/// the bottom eighth of the block has none.
///
/// One path for every note, because a lane of a hundred notes is a
/// hundred fills otherwise, and the index has to carry an extent per
/// command.
/// What an item holds to draw: its notes, its envelope, and — a shut
/// kit's — that envelope by drum, in the order they are drawn.
struct Held {
    notes: Option<Vec<crate::midi::Note>>,
    wave: Option<std::sync::Arc<crate::midi::Wave>>,
    layers: Vec<(
        LaneRole,
        std::sync::Arc<crate::midi::Wave>,
        Vec<crate::wave_detail::Source>,
    )>,
    /// The real items the waveform is of.
    sources: Vec<crate::wave_detail::Source>,
}

/// An item as a source of detail for the waveform drawn of it.
fn source_of(item: &daw_proto::Item) -> crate::wave_detail::Source {
    crate::wave_detail::Source::item(
        &item.guid,
        item.position.as_seconds(),
        item.length.as_seconds(),
        item.start_offset.as_seconds(),
    )
}

/// Each track's items as they are now, as a number that changes when any
/// of them moves, is cut, slipped, faded or switched lane: what a track's
/// detail is held for ([`crate::wave_detail::Source::version`]).
fn track_versions(project: &ProjectRef) -> std::collections::HashMap<&str, u64> {
    use std::hash::{Hash as _, Hasher as _};
    project
        .items
        .iter()
        .map(|(track, items)| {
            let mut hash = std::collections::hash_map::DefaultHasher::new();
            for item in items {
                item.position.as_seconds().to_bits().hash(&mut hash);
                item.length.as_seconds().to_bits().hash(&mut hash);
                item.start_offset.as_seconds().to_bits().hash(&mut hash);
                item.volume.to_bits().hash(&mut hash);
                item.fade_in_length.as_seconds().to_bits().hash(&mut hash);
                item.fade_out_length.as_seconds().to_bits().hash(&mut hash);
                item.fixed_lane.hash(&mut hash);
                item.muted.hash(&mut hash);
            }
            (track.as_str(), hash.finish())
        })
        .collect()
}

use expression_editor_core::kit::LaneRole;

/// The drums a shut kit's item is layered by, under to over: the kit's
/// own colour for the rest (cymbals, rooms) at the bottom, then toms,
/// snare, and the kick on top — the parts you read a groove by drawn
/// over the wash, each in its own colour, so the pattern still shows.
const KIT_LAYERS: [LaneRole; 4] = [
    LaneRole::Other,
    LaneRole::Toms,
    LaneRole::Snare,
    LaneRole::Kick,
];

/// A drum's waveform colour on a shut kit; `None` for the rest, which
/// takes the kit's own.
const fn kit_color(role: LaneRole) -> Option<Color> {
    match role {
        LaneRole::Kick => Some(Color::from_rgba8(0xe5, 0x48, 0x4d, 0xff)),
        LaneRole::Snare => Some(Color::from_rgba8(0x3e, 0x8e, 0xf7, 0xff)),
        LaneRole::Toms => Some(Color::from_rgba8(0x9b, 0x6b, 0xe0, 0xff)),
        LaneRole::Other => None,
    }
}

/// Every track's drum role, by guid: from its name and the folders it
/// sits in (`kit_role`) — a mic called `In` inside `Kick` is the kick.
fn kit_roles(project: &ProjectRef) -> std::collections::HashMap<&str, LaneRole> {
    let by_guid: std::collections::HashMap<&str, &daw_proto::Track> = project
        .tracks
        .iter()
        .map(|t| (t.guid.as_str(), t))
        .collect();
    project
        .tracks
        .iter()
        .map(|track| {
            let mut folders: Vec<&str> = Vec::new();
            let mut at = track.parent_guid.as_deref();
            while let Some(parent) = at.and_then(|g| by_guid.get(g)) {
                folders.push(parent.name.as_str());
                at = parent.parent_guid.as_deref();
            }
            (
                track.guid.as_str(),
                expression_editor_core::kit::kit_role(&track.name, &folders),
            )
        })
        .collect()
}

/// Each shut folder's loudest point anywhere under it: one display gain
/// for all of its items. Each its own, an edit that cuts the folder into
/// a piece a seam blew every quiet stretch up to the band's edge.
fn folder_loudest<'a>(
    project: &'a ProjectRef,
    by_guid: &std::collections::HashMap<&str, &daw_proto::Item>,
    previews: &crate::midi::Previews,
) -> std::collections::HashMap<&'a str, f64> {
    project
        .folds
        .iter()
        .map(|(folder, fold)| {
            let read = fold
                .spans
                .iter()
                .flat_map(|span| span.from.iter())
                .filter(|child| by_guid.contains_key(child.as_str()))
                .filter_map(|child| previews.wave(child))
                .map(|wave| loudest_of(&wave))
                .fold(0.0_f64, f64::max);
            (folder.as_str(), remembered_loudest(folder, read))
        })
        .collect()
}

thread_local! {
    /// Each shut folder's loudest as last read whole, by folder guid.
    static FOLDER_LOUDEST: std::cell::RefCell<std::collections::HashMap<String, f64>> =
        std::cell::RefCell::default();
}

/// A shut folder's loudest: what is `read` now, or what was read before
/// when that was louder.
///
/// An edit leaves a kit hundreds of new slices whose peaks are read one by
/// one; the loudest of the few read was a quiet one, the folder's gain at
/// its cap, every drum blown up past the lane until the last was read — the
/// waveforms "going crazy" after every edit. An edit moves audio, it does
/// not make it quieter: the loudest once read holds.
fn remembered_loudest(folder: &str, read: f64) -> f64 {
    FOLDER_LOUDEST.with_borrow_mut(|kept| {
        let loudest = kept.get(folder).copied().unwrap_or(0.0).max(read);
        if loudest > 0.0 {
            kept.insert(folder.to_owned(), loudest);
        }
        loudest
    })
}

/// What a shut folder's row looks its children up in: every item by guid,
/// each track's drum, each track's items' version.
type Lookups<'m, 'k> = (
    &'m std::collections::HashMap<&'k str, &'k daw_proto::Item>,
    &'m std::collections::HashMap<&'k str, LaneRole>,
    &'m std::collections::HashMap<&'k str, u64>,
);

/// What a shut folder's item `guid` holds — the notes and the envelope of
/// the children under its span — or `None` when `guid` is not one.
fn folded_contents(
    project: &ProjectRef,
    (by_guid, roles, versions): Lookups<'_, '_>,
    previews: &crate::midi::Previews,
    guid: &str,
) -> Option<Held> {
    let (folder, index) = daw_ui::studio::folded::parse_guid(guid)?;
    let span = project.folds.get(folder)?.spans.get(index)?;
    // A track hidden from the panel (a DI kept to reamp from) is left out
    // of the picture; its items still go with an edit to the span.
    let shown = |item: &&&daw_proto::Item| {
        project
            .tracks
            .iter()
            .find(|t| t.guid == item.track_guid)
            .is_none_or(|t| t.visible_in_tcp)
    };
    let held: Vec<_> = span
        .from
        .iter()
        .filter_map(|child| by_guid.get(child.as_str()))
        .filter(shown)
        .map(|item| (item, previews.wave(&item.guid), previews.get(&item.guid)))
        .collect();
    let children: Vec<crate::midi::Child<'_>> = held
        .iter()
        .map(|(item, wave, notes)| crate::midi::Child {
            at: item.position.as_seconds(),
            length: item.length.as_seconds(),
            wave: wave.as_deref(),
            notes: notes.as_deref(),
        })
        .collect();
    let role_of = |item: &daw_proto::Item| {
        roles
            .get(item.track_guid.as_str())
            .copied()
            .unwrap_or(LaneRole::Other)
    };
    // A kit — anything under it a drum — is layered by drum.
    let kit = held
        .iter()
        .any(|(item, ..)| role_of(item) != LaneRole::Other);
    let layers = if kit {
        KIT_LAYERS
            .iter()
            .filter_map(|role| {
                let these: Vec<crate::midi::Child<'_>> = held
                    .iter()
                    .zip(&children)
                    .filter(|((item, ..), _)| role_of(item) == *role)
                    .map(|(_, child)| crate::midi::Child { ..*child })
                    .collect();
                // Its tracks, each read once over the span as it plays —
                // not its slices one by one.
                let sources = tracks_over(
                    held.iter()
                        .filter(|(item, ..)| role_of(item) == *role)
                        .map(|(item, ..)| item.track_guid.as_str()),
                    (span.start, span.end),
                    versions,
                );
                crate::midi::fold_waves(span.start, span.end, &these)
                    .map(|wave| (*role, std::sync::Arc::new(wave), sources))
            })
            .collect()
    } else {
        Vec::new()
    };
    Some(Held {
        notes: crate::midi::fold_notes(span.start, span.end, &children),
        wave: crate::midi::fold_waves(span.start, span.end, &children).map(std::sync::Arc::new),
        layers,
        sources: tracks_over(
            held.iter().map(|(item, ..)| item.track_guid.as_str()),
            (span.start, span.end),
            versions,
        ),
    })
}

/// Each of `tracks` once, read over `start..end` as it plays
/// ([`crate::wave_detail::Source::track`]).
fn tracks_over<'a>(
    tracks: impl Iterator<Item = &'a str>,
    (start, end): (f64, f64),
    versions: &std::collections::HashMap<&str, u64>,
) -> Vec<crate::wave_detail::Source> {
    let mut seen: Vec<&str> = Vec::new();
    for track in tracks {
        if !seen.contains(&track) {
            seen.push(track);
        }
    }
    seen.into_iter()
        .map(|track| {
            let version = versions.get(track).copied().unwrap_or(0);
            crate::wave_detail::Source::track(track, start, end - start, version)
        })
        .collect()
}

fn midi_preview(
    notes: &[crate::midi::Note],
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
) -> Option<BezPath> {
    if notes.is_empty() || bottom - top < 2.0 {
        return None;
    }
    let (low, high) = notes.iter().fold((u8::MAX, u8::MIN), |(lo, hi), note| {
        (lo.min(note.pitch), hi.max(note.pitch))
    });
    // A part on one pitch has no range to spread over, so it is drawn
    // down the middle rather than divided by zero.
    let span = f64::from(high.saturating_sub(low)).max(1.0);
    let width = x1 - x0;
    let height = bottom - top;
    // Tall enough to see, short enough that neighbouring pitches do not
    // merge into a block.
    let note_h = (height / span.min(24.0)).clamp(1.0, height / 3.0);

    let mut path = BezPath::new();
    for note in notes {
        let at = x0 + width * f64::from(note.at);
        // Every note gets a width, however short: a preview of a
        // sixteenth-note part at this zoom is otherwise nothing at all.
        // A fixed time, not a share of the item: a share of a whole-song
        // click item was a second, and every click drew a beat long.
        let len = (width * f64::from(note.len)).max(MIN_NOTE_SECS);
        let from_top = f64::from(high.saturating_sub(note.pitch)) / span;
        let y = (height - note_h).mul_add(from_top, top);
        // Written out rather than built from a Rect: one path holding
        // every note keeps this to a single fill, and the index carries
        // one extent per command.
        let (x_from, x_to) = (at, (at + len).min(x1));
        path.move_to((x_from, y));
        path.line_to((x_to, y));
        path.line_to((x_to, y + note_h));
        path.line_to((x_from, y + note_h));
        path.close_path();
    }
    Some(path)
}

/// An item's waveform as one closed path: the maxima forward along the
/// top, the minima back along the bottom, about the lane's middle.
///
/// Drawn from the take's own peaks ([`crate::midi::Wave`]), so point `i`
/// sits at item time `i * step`. `shift` moves the audio under the item —
/// a slip in flight, in seconds into the source — without moving the item.
/// A hair of height at silence, so a quiet stretch still reads as audio
/// rather than as a gap. `None` for a lane too short to show one, or for
/// no audio inside the item.
fn waveform(
    wave: &crate::midi::Wave,
    x0: f64,
    x1: f64,
    top: f64,
    bottom: f64,
    shift: f64,
) -> Option<BezPath> {
    let half = (bottom - top) / 2.0;
    if half < 1.5 || wave.step <= 0.0 || x1 <= x0 {
        return None;
    }
    let mid = (top + bottom) / 2.0;
    // The points that land inside the item once shifted.
    let first = crate::num::index((shift / wave.step).floor().max(0.0));
    let last =
        crate::num::index(((x1 - x0 + shift) / wave.step).ceil().max(0.0)).min(wave.points.len());
    if first >= last {
        return None;
    }
    let span = &wave.points[first..last];
    // Each point at the middle of the stretch it stands for.
    let x_of = |i: usize| {
        (crate::num::coord(first + i) + 0.5)
            .mul_add(wave.step, x0 - shift)
            .clamp(x0, x1)
    };
    let reach = (half - 1.0) * wave_gain(wave);
    let mut path = BezPath::new();
    let edge = half - 1.0;
    for (i, &(max, _)) in span.iter().enumerate() {
        let y = mid - (f64::from(max.max(0.0)) * reach).clamp(HAIR, edge.max(HAIR));
        if i == 0 {
            path.move_to((x_of(i), y));
        } else {
            path.line_to((x_of(i), y));
        }
    }
    for (i, &(_, min)) in span.iter().enumerate().rev() {
        path.line_to((
            x_of(i),
            mid + (-f64::from(min.min(0.0)) * reach).clamp(HAIR, edge.max(HAIR)),
        ));
    }
    path.close_path();
    Some(path)
}

/// The item titles, in pixel space, over the lanes: the per-frame pass.
///
/// `origin` is where the lanes' (0 s, row 0) lands on the surface —
/// the same translation the lanes are replayed under, so a title sits
/// on its item at every scroll and zoom. Written only where there is
/// room to read one: a row shorter than a line, or an item narrower
/// than a few characters, gets none.
pub fn titles(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &crate::text::Font,
    scene: &Arrangement,
    view: Viewport,
    origin: (f64, f64),
) {
    const SIZE: f32 = 8.0;
    const PAD: f64 = 3.0;
    for (title, top, height) in scene.titles_in(view) {
        let row_h = height * view.zoom_y;
        if row_h < 12.0 {
            continue;
        }
        let left = title.x0.mul_add(view.pps, origin.0);
        let right = title.x1.mul_add(view.pps, origin.0);
        let lettered = match (scene.lettering, &title.spelled) {
            (Lettering::Numbers, Some(spelled)) => Some(spelled.numbers.as_str()),
            (Lettering::Chords, Some(spelled)) => Some(spelled.chords.as_str()),
            _ => None,
        };
        if let Some(text) = lettered {
            let row_top = top.mul_add(view.zoom_y, origin.1);
            // The lanes' own left edge, so an item that began off
            // screen still says what it is where it can be seen.
            let seen = origin.0 + view.scroll_x;
            letter(
                painter,
                palette,
                font,
                text,
                (left.max(seen), right),
                (row_top, row_h),
            );
            continue;
        }
        // Held at the lanes' left edge while the item's start is off
        // screen, as the lettering is: a long stem scrolled into its
        // middle still says what it is.
        let left = left.max(origin.0 + view.scroll_x);
        let room = right - left - PAD * 2.0;
        if room < 12.0 {
            continue;
        }
        // Cut the name to what fits, so a title never runs off its item
        // onto the next one.
        let mut name: &str = &title.name;
        while !name.is_empty() && font.width(name, SIZE) > room {
            let mut end = name.len().saturating_sub(1);
            while end > 0 && !name.is_char_boundary(end) {
                end = end.saturating_sub(1);
            }
            name = name.get(..end).unwrap_or("");
        }
        if name.is_empty() {
            continue;
        }
        let y = top.mul_add(view.zoom_y, origin.1) + f64::from(SIZE) + 2.0;
        crate::tcp::glyphs(painter, font, palette.text, name, left + PAD, y, SIZE);
    }
}

/// One word as large as its item: as tall as the row allows, shrunk
/// only if it would run past the item's end, over a shade that keeps
/// the notes under it from reading as part of the letters.
fn letter(
    painter: &mut impl PaintScene,
    palette: &Palette,
    font: &crate::text::Font,
    text: &str,
    (left, right): (f64, f64),
    (top, row_h): (f64, f64),
) {
    const PAD: f64 = 4.0;
    // How much of the size the capitals stand, near enough for the
    // embedded face — what centres the word in its row.
    const CAP: f64 = 0.72;
    // Measured at one size and scaled: a face's advance is linear in
    // its size.
    const PROBE: f32 = 100.0;
    // Below this the word is not the thing to read any more; the
    // title's own size, from the same face.
    const FLOOR: f64 = 8.0;
    let inset = 2.0;
    let room = right - left - PAD * 2.0;
    let tall = (row_h - inset * 2.0 - PAD) / CAP;
    let wide = font.width(text, PROBE);
    if room <= 0.0 || wide <= 0.0 {
        return;
    }
    let size = tall.min(room * f64::from(PROBE) / wide);
    if size < FLOOR {
        return;
    }
    painter.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgba8(0x00, 0x00, 0x00, 0x73),
        None,
        &Rect::new(left, top + inset, right, top + row_h - inset),
    );
    #[expect(
        clippy::cast_possible_truncation,
        reason = "a font size, well inside f32"
    )]
    let size_f32 = size as f32;
    let baseline = top + (row_h + size * CAP) / 2.0;
    crate::tcp::glyphs(
        painter,
        font,
        palette.text,
        text,
        left + PAD,
        baseline,
        size_f32,
    );
}

impl Arrangement {
    /// The song's chords and key changes, for the ruler.
    #[must_use]
    pub const fn chart(&self) -> &ChartMarks {
        &self.chart
    }

    /// How many items this scene draws.
    #[must_use]
    pub const fn items(&self) -> usize {
        self.item_count
    }

    /// Replay what `view` can see, under `transform`.
    ///
    /// This is THE replay: the window and the bench both call it, so a
    /// benchmark cannot measure work the app does not do, and an
    /// optimization cannot land in one and miss the other. Returns what
    /// it walked and what it submitted, which is the pair that says
    /// whether the culling is working.
    pub fn replay_lanes(
        &self,
        painter: &mut impl PaintScene,
        view: Viewport,
        transform: Affine,
    ) -> Counts {
        let (left, right) = view.secs();
        let shown = view.on_screen();
        let mut counts = Counts::default();
        for row in self.visible_rows(view) {
            let Some(span) = self.index.lanes.get(row) else {
                continue;
            };
            for (i, cmd) in commands(&self.lanes, span) {
                counts.replayed = counts.replayed.saturating_add(1);
                // Off the sides of a row that IS on screen — the case
                // that matters when zoomed in on bar 400 of 500.
                match self.index.x.get(i) {
                    Some(&(x0, x1)) if x1 < left || x0 > right => continue,
                    _ => {}
                }
                // A rectangle far wider than the view (a song-long take, a
                // lane's stripe) cut to it first: the renderer sizes its
                // buffers by how long every path is on screen, and zoomed
                // in to the samples a 4-minute take is a hundred million
                // pixels of it.
                let cut = match cmd {
                    RenderCommand::Fill(fill) => {
                        cut_to_view(&fill.shape, fill.transform, left, right).map(|shape| {
                            RenderCommand::Fill(anyrender::recording::FillCommand {
                                shape,
                                ..fill.clone()
                            })
                        })
                    }
                    _ => None,
                };
                if submit_command(painter, cut.as_ref().unwrap_or(cmd), transform) {
                    counts.submitted = counts.submitted.saturating_add(1);
                }
                // An item body with a waveform over it: the part on
                // screen, a point a pixel.
                // Layers in order: a shut kit's are drawn under to over.
                for lane in u32::try_from(i)
                    .ok()
                    .and_then(|i| self.index.waves.get(&i))
                    .into_iter()
                    .flatten()
                {
                    match fine(lane, shown.0, shown.1, view.pps, transform) {
                        Some(Fine::Envelope(path)) => {
                            painter.fill(Fill::NonZero, transform, lane.color, None, &path);
                            counts.submitted = counts.submitted.saturating_add(1);
                        }
                        Some(Fine::Samples(lines, dots)) => {
                            let stroke = vello::kurbo::Stroke::new(1.25);
                            for line in &lines {
                                painter.stroke(&stroke, Affine::IDENTITY, lane.color, None, line);
                            }
                            for dot in &dots {
                                painter.fill(
                                    Fill::NonZero,
                                    Affine::IDENTITY,
                                    lane.color,
                                    None,
                                    dot,
                                );
                            }
                            counts.submitted = counts.submitted.saturating_add(
                                u64::try_from(lines.len().saturating_add(dots.len()))
                                    .unwrap_or(u64::MAX),
                            );
                        }
                        None => {
                            for path in lane_wave_paths(lane, left, right, view.pps) {
                                painter.fill(Fill::NonZero, transform, lane.color, None, &*path);
                                counts.submitted = counts.submitted.saturating_add(1);
                            }
                        }
                    }
                }
            }
        }
        counts
    }

    /// Cut the panel again, for a new vertical zoom.
    ///
    /// Cheap enough to do while a zoom is in flight — it is forty rows
    /// of vector art, not the whole session — and the only way to zoom
    /// a panel without deforming it. See [`record_panel`].
    ///
    /// A no-op when the zoom has not moved, so the caller can ask every
    /// frame and pay only on the frames that changed.
    pub fn repanel(
        &mut self,
        palette: &Palette,
        font: &crate::text::Font,
        rows: &[(daw_proto::Track, u32)],
        layout: crate::layout::Layout,
        zoom: f64,
    ) -> bool {
        let zoom = if zoom > 0.0 { zoom } else { 1.0 };
        if (zoom - self.panel_zoom).abs() < f64::EPSILON {
            return false;
        }
        let cut = record_panel(palette, font, rows, layout, zoom, self.tcp);
        self.panel = cut.panel;
        self.panel_bar = cut.bar;
        self.index.panel = cut.spans;
        self.index.panel_bar = cut.bar_spans;
        self.panel_zoom = cut.zoom;
        true
    }

    /// Forget which zoom the panel was cut at, so the next
    /// [`Self::repanel`] re-cuts whatever the zoom is doing.
    ///
    /// For the things a cut BAKES IN that are not the zoom — the track
    /// name above all. A name is recorded chrome, not a live value, so
    /// renaming a track changes nothing on screen until the row it is
    /// written into is drawn again.
    pub fn forget_panel(&mut self) {
        // NaN, because every comparison against it is false — which is
        // exactly "this cut matches nothing".
        self.panel_zoom = f64::NAN;
    }

    /// The same for the track panel, which scrolls vertically only and
    /// therefore needs no horizontal test.
    ///
    /// `transform` must be a TRANSLATE. The cut being replayed was made
    /// at `panel_zoom` and already carries the vertical zoom in its own
    /// geometry, so scaling it here would apply the zoom twice — and
    /// deform it, which is the thing [`Self::repanel`] exists to stop.
    pub fn replay_panel(
        &self,
        painter: &mut impl PaintScene,
        view: Viewport,
        transform: Affine,
    ) -> Counts {
        let mut counts = Counts::default();
        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        for row in self.visible_rows(view) {
            // How tall this row lands ON SCREEN, which is what decides
            // whether its controls are worth drawing. A row at its full
            // height under a zoom that shrinks it to a fifth of a pixel
            // is a band, whatever the project says about it.
            let on_screen = self.row_height(row) * zoom;
            let (scene, index) = if on_screen >= crate::tcp::BAND_BELOW {
                (&self.panel, &self.index.panel)
            } else {
                (&self.panel_bar, &self.index.panel_bar)
            };
            let Some(span) = index.get(row) else {
                continue;
            };
            for (_, cmd) in commands(scene, span) {
                counts.replayed = counts.replayed.saturating_add(1);
                if submit_command(painter, cmd, transform) {
                    counts.submitted = counts.submitted.saturating_add(1);
                }
            }
        }
        counts
    }
}

/// One row's commands, with their indices.
///
/// The indices are what the x-extent table is keyed by, so they come out
/// alongside rather than being recomputed. Bounds are the slice's, which
/// is what makes an index recorded against a scene that has since been
/// rebuilt yield nothing instead of panicking.
fn commands<'a>(
    scene: &'a Scene,
    span: &std::ops::Range<u32>,
) -> impl Iterator<Item = (usize, &'a RenderCommand)> {
    let start = usize::try_from(span.start).unwrap_or(usize::MAX);
    let end = usize::try_from(span.end).unwrap_or(usize::MAX);
    scene
        .commands
        .get(start..end.min(scene.commands.len()))
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(move |(offset, cmd)| (start.saturating_add(offset), cmd))
}

/// How many commands a scene holds, as an index.
///
/// Saturating: a wrapped index would quietly cull the wrong rows, where
/// a saturated one draws too much. Four billion commands is a project
/// that could not be opened in the first place.
fn command_index(scene: &Scene) -> u32 {
    u32::try_from(scene.commands.len()).unwrap_or(u32::MAX)
}

/// A packed `0xRRGGBB` with an alpha.
fn rgb24(rgb: u32, alpha: u8) -> Color {
    let byte = |shift: u32| u8::try_from((rgb >> shift) & 0xff).unwrap_or(0);
    Color::from_rgba8(byte(16), byte(8), byte(0), alpha)
}

/// Replay EVERYTHING, ignoring the viewport.
///
/// The control for [`Arrangement::replay_lanes`]. Culling is only ever a
/// win if the frame it produces is identical to the frame without it, so
/// `just daw-verify` renders both and compares the pixels — a claim
/// about what is off screen is exactly the kind that is easy to argue
/// and easy to get subtly wrong at the edges.
pub fn replay_all(painter: &mut impl PaintScene, scene: &Scene, transform: Affine) -> Counts {
    let mut counts = Counts::default();
    for cmd in &scene.commands {
        counts.replayed = counts.replayed.saturating_add(1);
        if submit_command(painter, cmd, transform) {
            counts.submitted = counts.submitted.saturating_add(1);
        }
    }
    counts
}

impl Arrangement {
    /// Every row of the panel, at the detail the viewport's ZOOM asks
    /// for — but with no culling.
    ///
    /// The control for [`Arrangement::replay_panel`]. It has to make the
    /// same level-of-detail choice, because that choice is not culling:
    /// substituting a band for a row a fifth of a pixel tall changes the
    /// picture on purpose, while skipping a row that is off screen must
    /// not change it at all. Comparing against a control that drew full
    /// panels everywhere would be testing the two together and failing
    /// on the one that is behaving.
    pub fn replay_all_panel(
        &self,
        painter: &mut impl PaintScene,
        view: Viewport,
        transform: Affine,
    ) -> Counts {
        let zoom = if view.zoom_y > 0.0 { view.zoom_y } else { 1.0 };
        let mut counts = Counts::default();
        for row in 0..self.index.panel.len() {
            let on_screen = self.row_height(row) * zoom;
            let (scene, index) = if on_screen >= crate::tcp::BAND_BELOW {
                (&self.panel, &self.index.panel)
            } else {
                (&self.panel_bar, &self.index.panel_bar)
            };
            let Some(span) = index.get(row) else {
                continue;
            };
            for (_, cmd) in commands(scene, span) {
                counts.replayed = counts.replayed.saturating_add(1);
                if submit_command(painter, cmd, transform) {
                    counts.submitted = counts.submitted.saturating_add(1);
                }
            }
        }
        counts
    }
}

/// `outer` applied after `inner`.
///
/// Named rather than written as `*` because multiplying two affines is
/// composition, not arithmetic: there is nothing here to overflow, and
/// the lint that flags the operator is right to flag it everywhere else.
fn compose(outer: Affine, inner: Affine) -> Affine {
    #[expect(
        clippy::arithmetic_side_effects,
        reason = "affine composition; the operator is matrix multiplication"
    )]
    let composed = outer * inner;
    composed
}

/// `shape` as the rectangle it is, if it is an upright one: a move,
/// lines along its edges, a close — every point on a corner.
fn upright(shape: &BezPath) -> Option<Rect> {
    use vello::kurbo::{PathEl, Shape as _};
    let bounds = shape.bounding_box();
    let corner = |p: vello::kurbo::Point| {
        ((p.x - bounds.x0).abs() < 1e-9 || (p.x - bounds.x1).abs() < 1e-9)
            && ((p.y - bounds.y0).abs() < 1e-9 || (p.y - bounds.y1).abs() < 1e-9)
    };
    let elements = shape.elements();
    (elements.len() <= 6
        && elements.iter().all(|el| match el {
            PathEl::MoveTo(p) | PathEl::LineTo(p) => corner(*p),
            PathEl::ClosePath => true,
            _ => false,
        }))
    .then_some(bounds)
}

/// `shape` cut to `left..right` (and as much again either side), when it
/// is an upright rectangle reaching further than that — in its own
/// coordinates, so only for an untransformed shape. `None` to leave it as
/// it is.
fn cut_to_view(shape: &BezPath, transform: Affine, left: f64, right: f64) -> Option<BezPath> {
    use vello::kurbo::Shape as _;
    if transform != Affine::IDENTITY {
        return None;
    }
    let margin = (right - left).max(0.0);
    let (from, to) = (left - margin, right + margin);
    let bounds = shape.bounding_box();
    if bounds.x0 >= from && bounds.x1 <= to {
        return None;
    }
    let bounds = upright(shape)?;
    let (x0, x1) = (bounds.x0.max(from), bounds.x1.min(to));
    (x1 > x0).then(|| Rect::new(x0, bounds.y0, x1, bounds.y1).to_path(0.1))
}

/// Every solid fill in a finished frame `width` by `height` that reaches
/// far past it, cut to it (and as much again around).
///
/// Zoomed in to the
/// samples, a section's band on the ruler, a take's body, a stretch of
/// its coarse waveform are millions of pixels long, and the renderer
/// sizes its buffers by the length of what it is given, on screen or
/// not — past its limit it stops the window, or quietly draws nothing.
pub fn cut_to_screen(scene: &mut anyrender::recording::Scene, width: f64, height: f64) {
    use vello::kurbo::Shape as _;
    let screen = Rect::new(-width, -height, 2.0 * width, 2.0 * height);
    for cmd in &mut scene.commands {
        let RenderCommand::Fill(fill) = cmd else {
            continue;
        };
        if !matches!(fill.brush, anyrender::Paint::Solid(_)) {
            continue;
        }
        let mut placed = fill.shape.clone();
        placed.apply_affine(fill.transform);
        let bounds = placed.bounding_box();
        if screen.contains(bounds.origin())
            && screen.contains(vello::kurbo::Point::new(bounds.x1, bounds.y1))
        {
            continue;
        }
        fill.shape = clip_to(&placed, screen);
        fill.transform = Affine::IDENTITY;
    }
}

/// `path`'s area inside `rect`: each of its outlines, flattened to lines,
/// cut against the rectangle's four sides (Sutherland–Hodgman), as a
/// fill sees them — an outline left open is closed.
fn clip_to(path: &BezPath, rect: Rect) -> BezPath {
    let mut outlines: Vec<Vec<vello::kurbo::Point>> = Vec::new();
    vello::kurbo::flatten(path.iter(), 0.25, |el| match el {
        vello::kurbo::PathEl::MoveTo(p) => outlines.push(vec![p]),
        vello::kurbo::PathEl::LineTo(p) => {
            if let Some(outline) = outlines.last_mut() {
                outline.push(p);
            }
        }
        _ => {}
    });
    let mut out = BezPath::new();
    for outline in outlines {
        let cut = [
            Side::Left(rect.x0),
            Side::Right(rect.x1),
            Side::Top(rect.y0),
            Side::Bottom(rect.y1),
        ]
        .iter()
        .fold(outline, |points, side| side.clip(&points));
        let mut points = cut.into_iter();
        if let Some(first) = points.next() {
            out.move_to(first);
            for p in points {
                out.line_to(p);
            }
            out.close_path();
        }
    }
    out
}

/// One side of a rectangle, to cut an outline against.
#[derive(Clone, Copy)]
enum Side {
    Left(f64),
    Right(f64),
    Top(f64),
    Bottom(f64),
}

impl Side {
    fn inside(self, p: vello::kurbo::Point) -> bool {
        match self {
            Self::Left(x) => p.x >= x,
            Self::Right(x) => p.x <= x,
            Self::Top(y) => p.y >= y,
            Self::Bottom(y) => p.y <= y,
        }
    }

    /// Where the edge from `a` to `b` crosses this side.
    fn cross(self, a: vello::kurbo::Point, b: vello::kurbo::Point) -> vello::kurbo::Point {
        let along = |from: f64, to: f64, at: f64| {
            let span = to - from;
            if span.abs() < f64::EPSILON {
                0.0
            } else {
                (at - from) / span
            }
        };
        let t = match self {
            Self::Left(x) | Self::Right(x) => along(a.x, b.x, x),
            Self::Top(y) | Self::Bottom(y) => along(a.y, b.y, y),
        };
        a.lerp(b, t)
    }

    /// The part of the closed outline `points` on the inside of this side.
    fn clip(self, points: &[vello::kurbo::Point]) -> Vec<vello::kurbo::Point> {
        let mut out = Vec::with_capacity(points.len());
        let Some(&last) = points.last() else {
            return out;
        };
        let mut previous = last;
        for &p in points {
            match (self.inside(previous), self.inside(p)) {
                (true, true) => out.push(p),
                (true, false) => out.push(self.cross(previous, p)),
                (false, true) => {
                    out.push(self.cross(previous, p));
                    out.push(p);
                }
                (false, false) => {}
            }
            previous = p;
        }
        out
    }
}

/// Push one recorded command through `transform`.
///
/// The brush is forwarded WHOLE rather than unwrapped.
///
/// This used to match `Paint::Solid` and return false for anything else,
/// which silently dropped every gradient that reached a recorded scene:
/// the fader cap's moulding, the volume knob's drop shadow and body, the
/// record arm's ring. They were not mis-drawn, they were not drawn at
/// all — whatever had been painted underneath showed through instead,
/// which is why the result looked merely flat rather than broken. A
/// direct `painter.fill` with the same gradient worked, which is what
/// finally separated "gradients do not work" from "this replay discards
/// them".
pub fn submit_command(
    painter: &mut impl PaintScene,
    cmd: &RenderCommand,
    transform: Affine,
) -> bool {
    match cmd {
        RenderCommand::Fill(fill) => {
            painter.fill(
                fill.fill,
                compose(transform, fill.transform),
                &fill.brush,
                fill.brush_transform,
                &fill.shape,
            );
            true
        }
        RenderCommand::Stroke(stroke) => {
            painter.stroke(
                &stroke.style,
                compose(transform, stroke.transform),
                &stroke.brush,
                stroke.brush_transform,
                &stroke.shape,
            );
            true
        }
        // Text. Dropping this arm is not a silent degradation of
        // quality, it is a track panel with no names in it, so it is
        // handled here rather than defaulted.
        RenderCommand::GlyphRun(run) => {
            painter.draw_glyphs(
                &run.font_data,
                run.font_size,
                run.hint,
                &run.normalized_coords,
                run.embolden,
                &run.style,
                &run.brush,
                run.brush_alpha,
                compose(transform, run.transform),
                run.glyph_transform,
                run.glyphs.iter().copied(),
            );
            true
        }
        // Clipping. The rack scrolls, which means it draws past its own
        // box in both directions — above into the rails, below over the
        // strip's own controls — and a clip is the only thing that
        // stops it. Dropped, as these were, a scrolled rack silently
        // painted over its neighbours.
        //
        // Balanced by construction: whoever pushes one pops it inside
        // the same scene, and the overlay submits a scene whole. The
        // RECORDED scenes are replayed in per-strip ranges and must
        // therefore never carry a layer, since a range could cut
        // between a push and its pop.
        RenderCommand::PushClipLayer(clip) => {
            painter.push_clip_layer(compose(transform, clip.transform), &clip.clip);
            true
        }
        RenderCommand::PopLayer => {
            painter.pop_layer();
            true
        }
        _ => false,
    }
}

#[cfg(test)]
mod chrome_tests {
    /// The fader cap's ring is drawn in `chrome.hardware`, and a ring
    /// painted in a colour that is itself translucent is a ring you can
    /// see the meter through — which is the one thing the cap's window
    /// exists to be, and the one thing its ring must not be.
    #[test]
    fn the_hardware_the_cap_is_moulded_from_is_opaque() {
        let chrome = super::chrome(&daw_ui::theming::Theme::dark());
        assert_eq!(chrome.hardware.a, 255, "the cap's body is translucent");
        assert_eq!(
            chrome.hardware_edge.a, 255,
            "the cap's border is translucent"
        );
        // And it has to be TELLABLE from the strip it sits on. An
        // opaque ring the same grey as the background reads as no ring
        // at all, which looks exactly like a transparent one.
        // It is the strip's own grey, deliberately: these controls are
        // moulded out of the panel rather than sitting on it, and the
        // bevel and border are what give them their edges.
        //
        // Chasing a "transparent" cap once led here, and this was the
        // wrong suspect — the cap was solid all along. What made it
        // look see-through was the level being painted over the whole
        // cap instead of only the pane cut in it. See `CAP_PANE_Y0`.
        assert_eq!(chrome.hardware, chrome.surface_raised);
    }
}

#[cfg(test)]
mod lettering_tests {
    use super::{Spelled, spell};

    fn key(label: &str) -> keyflow::key::Key {
        session::key::parse_key(label).expect("a key")
    }

    /// A chord is spelled in the key that holds where it starts — the
    /// change at its own beat included — and reads as written before
    /// any key or when it is not a chord.
    #[test]
    fn chords_take_the_key_in_force() {
        let keys = [(0.0, key("F major")), (10.0, key("G major"))];
        let at = |written, t| spell("CHORD", written, t, &keys);
        assert_eq!(
            at("5/7", 2.0),
            Some(Spelled {
                numbers: "5/7".into(),
                chords: "C/E".into()
            })
        );
        assert_eq!(at("4", 10.0).map(|s| s.chords), Some("C".into()));
        assert_eq!(at("Bb", 2.0).map(|s| s.numbers), Some("4".into()));
        assert_eq!(at("N.C.", 2.0).map(|s| s.chords), Some("N.C.".into()));
        assert_eq!(
            spell("CHORD", "1", 0.0, &[]).map(|s| s.chords),
            Some("1".into())
        );
        assert_eq!(spell("Bass", "1", 0.0, &keys), None);
        assert_eq!(
            spell("KEY", "F major", 0.0, &keys).map(|s| s.numbers),
            Some("F major".into())
        );
    }
}

#[cfg(test)]
mod waveform_tests {
    use super::waveform;
    use crate::midi::Wave;

    fn wave() -> Wave {
        Wave {
            step: 0.5,
            points: vec![(1.0, -1.0), (0.0, 0.0), (0.5, -0.25), (1.0, -1.0)],
        }
    }

    /// Full scale reaches the lane's edges less a pixel; silence keeps a
    /// hair of height; and nothing is drawn outside the item.
    #[test]
    fn the_envelope_fills_the_lane_and_stays_in_the_item() {
        use vello::kurbo::Shape as _;
        let path = waveform(&wave(), 10.0, 12.0, 0.0, 20.0, 0.0).expect("a path");
        let b = path.bounding_box();
        assert!(
            (b.y0 - 1.0).abs() < 1e-9 && (b.y1 - 19.0).abs() < 1e-9,
            "{b:?}"
        );
        assert!(b.x0 >= 10.0 && b.x1 <= 12.0, "{b:?}");
    }

    /// A quiet take is drawn normalized: its loudest peak reaches the lane
    /// edge as a full-scale one does — and the gain is capped, so a take of
    /// nothing but noise stays small.
    #[test]
    fn a_quiet_take_fills_its_lane() {
        use vello::kurbo::Shape as _;
        let quiet = Wave {
            step: 0.5,
            points: vec![(0.1, -0.1), (0.05, -0.02), (0.1, -0.1)],
        };
        let b = waveform(&quiet, 0.0, 1.5, 0.0, 20.0, 0.0)
            .expect("a path")
            .bounding_box();
        assert!(
            (b.y0 - 1.0).abs() < 1e-9 && (b.y1 - 19.0).abs() < 1e-9,
            "{b:?}"
        );
        let noise = Wave {
            step: 0.5,
            points: vec![(0.001, -0.001), (0.001, -0.001)],
        };
        let b = waveform(&noise, 0.0, 1.0, 0.0, 20.0, 0.0)
            .expect("a path")
            .bounding_box();
        assert!(b.height() < 4.0, "noise stays small: {b:?}");
    }

    /// A slip moves the audio, not the item: shifted a whole second, the
    /// first two points have gone out of the item's left edge.
    #[test]
    fn a_slip_moves_the_audio_under_the_item() {
        let still = waveform(&wave(), 0.0, 2.0, 0.0, 20.0, 0.0).expect("a path");
        let slipped = waveform(&wave(), 0.0, 2.0, 0.0, 20.0, 1.0).expect("a path");
        assert!(slipped.elements().len() < still.elements().len());
        // Past the end of the audio there is nothing left to draw.
        assert!(waveform(&wave(), 0.0, 2.0, 0.0, 20.0, 5.0).is_none());
    }
}

#[cfg(test)]
mod clip_tests {
    use super::{BezPath, Rect, clip_to};
    use vello::kurbo::Shape as _;

    /// A take a hundred million pixels long is cut to the screen; what is
    /// on screen is untouched.
    #[test]
    fn a_long_shape_is_cut_to_the_screen() {
        let screen = Rect::new(0.0, 0.0, 100.0, 50.0);
        let long = Rect::new(-1e8, 10.0, 1e8, 20.0).to_path(0.1);
        let cut = clip_to(&long, screen).bounding_box();
        assert!(
            (cut.x0 - 0.0).abs() < 1e-9 && (cut.x1 - 100.0).abs() < 1e-9,
            "{cut:?}"
        );
        assert!(
            (cut.y0 - 10.0).abs() < 1e-9 && (cut.y1 - 20.0).abs() < 1e-9,
            "{cut:?}"
        );
    }

    /// A waveform's outline crossing the edge keeps its shape inside: the
    /// cut points lie on the edge, at the height the line crossed it.
    #[test]
    fn an_outline_is_cut_where_it_crosses() {
        let mut wave = BezPath::new();
        wave.move_to((-100.0, 0.0));
        wave.line_to((100.0, 20.0));
        wave.line_to((-100.0, 20.0));
        wave.close_path();
        let cut = clip_to(&wave, Rect::new(0.0, -10.0, 1000.0, 100.0));
        let b = cut.bounding_box();
        assert!(
            (b.x0 - 0.0).abs() < 1e-9 && (b.x1 - 100.0).abs() < 1e-9,
            "{b:?}"
        );
        assert!(
            (b.y0 - 10.0).abs() < 1e-9,
            "crossed the left edge halfway up: {b:?}"
        );
    }

    /// Wholly outside is nothing.
    #[test]
    fn outside_is_nothing() {
        let off = Rect::new(500.0, 0.0, 600.0, 10.0).to_path(0.1);
        assert!(
            clip_to(&off, Rect::new(0.0, 0.0, 100.0, 100.0))
                .elements()
                .is_empty()
        );
    }
}

#[cfg(test)]
mod lane_wave_tests {
    use super::{Affine, CHUNK, Fine, LaneWave, fine, lane_wave_paths};
    use crate::midi::Wave;
    use vello::peniko::Color;

    /// Forty points a second over 100 s, a spike at 50 s; `busy`, every
    /// point a little different from the last (as audio is), else flat.
    fn lane(points: usize, busy: bool) -> LaneWave {
        let wave = Wave {
            step: 0.025,
            points: (0..points)
                .map(|i| {
                    let level = if busy && i % 2 == 0 { 0.3 } else { 0.1 };
                    if i == 2000 {
                        (0.9, -0.9)
                    } else {
                        (level, -level)
                    }
                })
                .collect(),
        };
        LaneWave {
            wave: std::sync::Arc::new(wave),
            x0: 0.0,
            x1: 100.0,
            top: 0.0,
            bottom: 40.0,
            color: Color::WHITE,
            gain: 1.0,
            runs: std::cell::RefCell::default(),
            sources: Vec::new(),
            role: None,
        }
    }

    /// Zoomed in past a lane's own points, it is drawn from the detail
    /// read for it — a column a pixel, and past a pixel a sample a line
    /// through the samples; until the detail is here, not at all (the
    /// coarse one is).
    #[test]
    fn zoomed_in_the_waveform_is_drawn_from_detail() {
        let source = crate::wave_detail::Source::item("fine-test-item", 0.0, 100.0, 0.0);
        let mut lane = lane(100, false);
        lane.sources = vec![source.clone()];
        let at = Affine::IDENTITY;
        // At a pixel a second its points (a second each) are a pixel wide:
        // coarse is fine.
        assert!(fine(&lane, 10.0, 20.0, 1.0, at).is_none());
        // Zoomed in, and nothing read yet: asked for, coarse meanwhile.
        assert!(fine(&lane, 10.0, 11.0, 1_000.0, at).is_none());
        let samples: Vec<f64> = (0..4_800).map(|k| f64::from(k % 7) / 7.0 - 0.5).collect();
        crate::wave_detail::hold(
            &source,
            crate::wave_detail::reduce(&samples, 1, 4_800.0, 10.0, 2),
        );
        assert!(matches!(
            fine(&lane, 10.0, 11.0, 1_000.0, at),
            Some(Fine::Envelope(_))
        ));
        crate::wave_detail::hold(
            &source,
            crate::wave_detail::reduce(&samples, 1, 4_800.0, 10.0, 1),
        );
        let Some(Fine::Samples(lines, dots)) = fine(&lane, 10.0, 10.01, 48_000.0, at) else {
            panic!("a line through the samples");
        };
        assert_eq!(lines.len(), 1);
        assert!(!dots.is_empty(), "ten pixels a sample: each marked");
    }

    /// A sliver between two of the detail's points — a split's edge — has
    /// no column with anything in it: no envelope, never a path that is
    /// only its close (which panics the moment it is filled).
    #[test]
    fn a_sliver_between_detail_points_draws_no_envelope() {
        let source = crate::wave_detail::Source::item("fine-sliver-item", 0.0, 100.0, 0.0);
        let mut lane = lane(100, false);
        lane.sources = vec![source.clone()];
        let samples: Vec<f64> = (0..4_800).map(|k| f64::from(k % 7) / 7.0 - 0.5).collect();
        crate::wave_detail::hold(
            &source,
            crate::wave_detail::reduce(&samples, 1, 4_800.0, 10.0, 2),
        );
        for (from, to) in [(10.0002, 10.0003), (10.0, 11.0)] {
            if let Some(Fine::Envelope(path)) = fine(&lane, from, to, 1_000.0, Affine::IDENTITY) {
                assert!(
                    matches!(
                        path.elements().first(),
                        Some(vello::kurbo::PathEl::MoveTo(_))
                    ),
                    "{from}–{to}: {:?}",
                    path.elements().first()
                );
            }
        }
    }

    fn elements(lane: &LaneWave, left: f64, right: f64, pps: f64) -> usize {
        lane_wave_paths(lane, left, right, pps)
            .iter()
            .map(|p| p.elements().len())
            .sum()
    }

    #[test]
    fn a_waveform_is_drawn_at_most_a_point_a_pixel_and_near_the_screen() {
        let lane = lane(4000, true);
        // Fitted: 100 s across 200 px — two pixels a second, ~200 points
        // each way at most, not 4000.
        let fitted = elements(&lane, 0.0, 100.0, 2.0);
        assert!(fitted < 2 * 210, "{fitted} elements");
        // Zoomed in on 10 s at 100 px a second: those seconds at the
        // envelope's own resolution (40 a second is under a pixel's
        // worth), and at most a kept piece past either edge.
        let near = elements(&lane, 45.0, 55.0, 100.0);
        assert!(
            near > 2 * 380 && near < 2 * (400 + 2 * CHUNK) + 8,
            "{near} elements"
        );
        // Out of view: nothing.
        assert!(lane_wave_paths(&lane, 200.0, 300.0, 2.0).is_empty());
    }

    /// A flat stretch is drawn by its ends: the quiet between hits costs
    /// nothing a pixel. (The busy lane above is the control: the same
    /// span keeps its points.)
    #[test]
    fn a_flat_stretch_is_drawn_by_its_ends() {
        let flat = elements(&lane(4000, false), 45.0, 55.0, 100.0);
        assert!(flat < 40, "{flat} elements");
    }

    /// A pan draws the pieces it already has: the same path, not a new one.
    #[test]
    fn a_pan_reuses_the_pieces_it_has() {
        let lane = lane(4000, true);
        let before = lane_wave_paths(&lane, 40.0, 50.0, 100.0);
        let after = lane_wave_paths(&lane, 41.0, 51.0, 100.0);
        assert!(std::rc::Rc::ptr_eq(&before[0], &after[0]));
    }

    #[test]
    fn a_fold_keeps_a_peak_it_passes_over() {
        let lane = lane(4000, false);
        // The spike at point 2000 (50 s) reaches near the band's top edge.
        let top = lane_wave_paths(&lane, 0.0, 100.0, 2.0)
            .iter()
            .flat_map(|p| {
                p.elements()
                    .iter()
                    .filter_map(vello::kurbo::PathEl::end_point)
                    .collect::<Vec<_>>()
            })
            .map(|p| p.y)
            .fold(f64::MAX, f64::min);
        assert!(top < 5.0, "the spike survives the fold: top at {top}");
    }
}

#[cfg(test)]
mod kit_layer_tests {
    use super::{LaneRole, folded_contents, kit_roles};
    use daw_proto::{Duration, Item, PositionInSeconds, Track};
    use daw_ui::studio::{ProjectRef, folded, project::Project};

    fn track(guid: &str, name: &str, parent: Option<&str>) -> Track {
        Track {
            guid: guid.into(),
            name: name.into(),
            parent_guid: parent.map(Into::into),
            is_folder: parent.is_none() || name == "Kick",
            ..Track::default()
        }
    }

    fn item(guid: &str, track: &str) -> Item {
        Item {
            guid: guid.into(),
            track_guid: track.into(),
            position: PositionInSeconds::from_seconds(0.0),
            length: Duration::from_seconds(1.0),
            ..Item::default()
        }
    }

    /// A shut kit's item is its drums layered under to over — the rest,
    /// then toms, snare, kick — each its own envelope; a mic is a kick by
    /// the folder it sits in. A folder of no drums is one envelope.
    #[test]
    fn a_shut_kit_is_layered_by_drum_and_a_shut_folder_of_no_drums_is_not() {
        let tracks = vec![
            track("drums", "Drums", None),
            track("kick", "Kick", Some("drums")),
            track("in", "In", Some("kick")),
            track("snare", "Snare Top", Some("drums")),
            track("oh", "Overheads", Some("drums")),
            track("gtrs", "Guitars", None),
            track("amp", "Amp 1", Some("gtrs")),
        ];
        let mut project = Project {
            tracks,
            ..Project::default()
        };
        for (guid, on) in [
            ("i-in", "in"),
            ("i-snare", "snare"),
            ("i-oh", "oh"),
            ("i-amp", "amp"),
        ] {
            project
                .items
                .entry(on.to_string())
                .or_default()
                .push(item(guid, on));
        }
        let shown = vec![project.tracks[0].clone(), project.tracks[5].clone()];
        folded::refold(&mut project, &shown, true);
        let project = ProjectRef(std::sync::Arc::new(project));
        let previews = crate::midi::Previews::default();
        for guid in ["i-in", "i-snare", "i-oh", "i-amp"] {
            previews.insert_wave(
                guid,
                crate::midi::Wave {
                    step: 0.1,
                    points: vec![(0.5, -0.5); 10],
                },
            );
        }
        let by_guid = project
            .items
            .values()
            .flatten()
            .map(|i| (i.guid.as_str(), i))
            .collect();
        let roles = kit_roles(&project);
        assert_eq!(
            roles.get("in"),
            Some(&LaneRole::Kick),
            "a mic in the kick folder"
        );

        let kit = folded_contents(
            &project,
            (&by_guid, &roles, &std::collections::HashMap::new()),
            &previews,
            &folded::guid_of("drums", 0),
        )
        .expect("the kit's item");
        let order: Vec<LaneRole> = kit.layers.iter().map(|(role, ..)| *role).collect();
        assert_eq!(order, [LaneRole::Other, LaneRole::Snare, LaneRole::Kick]);

        let guitars = folded_contents(
            &project,
            (&by_guid, &roles, &std::collections::HashMap::new()),
            &previews,
            &folded::guid_of("gtrs", 0),
        )
        .expect("the guitars' item");
        assert!(
            guitars.layers.is_empty() && guitars.wave.is_some(),
            "one envelope"
        );
    }
}

/// An item's colour: its own, which take it is — dimmed when it is muted —
/// or with none of its own, its track's. Mixing, always its track's: a
/// take's colour means nothing in a mix ([`crate::tcp::Tcp::mixing`]).
fn item_color(item: &daw_proto::Item, track_color: Color, mixing: bool) -> Color {
    item.color.filter(|_| !mixing).map_or(track_color, |rgb| {
        rgb24(rgb, if item.muted { 0x66 } else { 0xff })
    })
}

#[cfg(test)]
mod item_color_tests {
    /// A take's colour shows in every mode but Mix; mixing, the track's.
    #[test]
    fn mixing_draws_every_item_in_its_tracks_colour() {
        let track = super::Color::from_rgb8(0x20, 0x40, 0x60);
        let take = daw_proto::Item {
            color: Some(0x00_ff_00_00),
            ..daw_proto::Item::default()
        };
        assert_ne!(
            super::item_color(&take, track, false),
            track,
            "a take's own colour"
        );
        assert_eq!(
            super::item_color(&take, track, true),
            track,
            "mixing: the track's"
        );
        let plain = daw_proto::Item::default();
        assert_eq!(super::item_color(&plain, track, false), track);
    }
}

#[cfg(test)]
mod dial_tests {
    /// A hit marker is drawn in its drum's colour, found from the piece's
    /// name as the hit overlay names it: the kick red, the snare blue, a
    /// tom purple, as their waveforms are.
    /// Blue within the floor, amber past it, red at half speed.
    #[test]
    fn a_stretch_is_coloured_by_how_far_it_is_from_its_speed() {
        let (blue, amber, red) = (
            super::stretch_tint(0.9),
            super::stretch_tint(0.6),
            super::stretch_tint(0.3),
        );
        assert!(blue != amber && amber != red && blue != red);
        assert_eq!(
            super::stretch_tint(super::super::arrange_edit::STRETCH_LEAST_RATE),
            blue
        );
        assert_eq!(super::stretch_tint(0.84), amber);
        assert_eq!(super::stretch_tint(0.5), red);
    }

    #[test]
    fn a_hit_marker_wears_its_drums_colour() {
        let colour =
            |piece: &str| super::kit_color(expression_editor_core::kit::kit_role(piece, &[]));
        assert_eq!(colour("Kick"), super::kit_color(super::LaneRole::Kick));
        assert_eq!(colour("Snare"), super::kit_color(super::LaneRole::Snare));
        assert_eq!(colour("Tom 1"), super::kit_color(super::LaneRole::Toms));
        assert!(colour("Kick").is_some() && colour("Kick") != colour("Snare"));
    }

    use super::{dial_angle, item_dial};

    #[test]
    fn a_dial_fits_in_the_top_right_corner_of_an_item_that_holds_one() {
        let (cx, cy, r) = item_dial(100.0, 300.0, 40.0).expect("room for one");
        assert!(cx + r < 300.0 && cx - r > 100.0 && cy - r > 0.0);
        assert!(item_dial(100.0, 110.0, 40.0).is_none(), "too narrow");
        assert!(item_dial(100.0, 300.0, 8.0).is_none(), "too short");
    }

    #[test]
    fn zero_db_points_up_and_the_ends_stop() {
        assert!(dial_angle(0.0).abs() < 1e-12);
        assert!((dial_angle(24.0) - 135_f64.to_radians()).abs() < 1e-12);
        assert!(
            (dial_angle(-96.0) + 135_f64.to_radians()).abs() < 1e-12,
            "the stop"
        );
    }
}

/// Whether `track`'s row shows its fixed lanes: it has more than one,
/// and the window has opened them (its `+`, [`crate::lanes::expanded`]) —
/// over REAPER's own lane display, which a song saved showing one lane
/// would otherwise keep shut against the hand that opened it.
#[must_use]
pub fn shows_lanes(track: &daw_proto::Track) -> bool {
    track.lane_count > 1 && crate::lanes::expanded(&track.guid)
}

/// The lanes `track`'s row shows, when it shows them all.
///
/// The track's own ([`shows_lanes`]), or — on a shut folder whose `+` is open — the lanes
/// of the tracks under it ([`crate::lanes::folder_view`]), each folded on
/// its own (`folded::refold_lanes`). `None` for a row showing what plays.
#[must_use]
pub fn row_lanes(
    project: &daw_ui::studio::project::Project,
    track: &daw_proto::Track,
) -> Option<daw_proto::Track> {
    project.folds.get(&track.guid).map_or_else(
        || shows_lanes(track).then(|| track.clone()),
        // Folded lane by lane only while the folder's lanes were open
        // when the rows were planned: its items say so.
        |fold| {
            if fold.items.iter().any(|item| item.fixed_lane.is_some()) {
                crate::lanes::folder_view(&project.tracks, track)
            } else {
                None
            }
        },
    )
}

/// The lanes a row showing its lanes gives a strip, top to bottom: those
/// `lanes` holds something on (each item's lane), and the one that plays.
///
/// A kit's EDIT lane is lane 35 of a take recorded over and over: a strip
/// for every lane number was 36 strips, each a sliver.
#[must_use]
pub fn lane_slots(track: &daw_proto::Track, lanes: impl Iterator<Item = u32>) -> Vec<u32> {
    let mut held: Vec<u32> = lanes.chain(crate::lanes::playing(track)).collect();
    held.sort_unstable();
    held.dedup();
    if held.is_empty() {
        held.push(0);
    }
    held
}

/// Lane `lane`'s strip of a row showing `slots` ([`lane_slots`]), as
/// fractions of the row; the last strip for a lane it gives none.
#[must_use]
pub fn slot_band(lane: u32, slots: &[u32]) -> (f64, f64) {
    let strip = slots
        .iter()
        .position(|s| *s == lane)
        .unwrap_or_else(|| slots.len().saturating_sub(1));
    let count = crate::num::coord(slots.len().max(1));
    let at = crate::num::coord(strip);
    (at / count, (at + 1.0) / count)
}

/// Lane `lane` of `count`'s strip of its row, top to bottom, as
/// fractions — the first lane at the top, as REAPER stacks them.
#[must_use]
pub fn lane_band(lane: u32, count: u32) -> (f64, f64) {
    let count = f64::from(count.max(1));
    let lane = f64::from(lane);
    (lane / count, (lane + 1.0) / count)
}

#[cfg(test)]
mod take_color_tests {
    use super::take_color;

    /// Every mic of one take is ringed alike; another take otherwise.
    #[test]
    fn a_takes_mics_share_its_colour() {
        let one = take_color("65-In-260907_1321-01.wav");
        assert_eq!(one, take_color("66-Out-260907_1321-01.wav"));
        assert_ne!(one, take_color("65-In-260907_1321-02.wav"));
        // A name not numbered by mic is a take of its own.
        assert_eq!(take_color("Guide 2"), take_color("Guide 2"));
    }
}
