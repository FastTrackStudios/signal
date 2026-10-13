//! The quantize surface.
//!
//! The engine was complete and tested, and there was **no way to reach
//! any of it from the editor** — the feature was done and unusable. This
//! is the surface, and because quantize is a tool on the seam it works
//! for MIDI notes as well as audio transients rather than only the
//! latter.
//!
//! State and geometry live here so they are assertable without a
//! renderer; the `rsx!` chrome belongs with the rest of the canvas.

use expression_editor_tools::event::Timed;
use expression_editor_tools::quantize::{Plan, QuantizeConfig, plan};

pub use expression_editor_tools::quantize::{HitPreview, hit_previews, swing};

/// How the quantize is written.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum WriteMode {
    /// Cut the audio and move the pieces. Phase-coherent across a mic
    /// group, which is why drum editing is done this way.
    #[default]
    Split,
    /// Bend time between anchors, keeping the material continuous.
    Warp,
}

/// Which tracks are edited, and which one the hits are detected from.
///
/// Separate on purpose: a kit is quantized from the snare or a trigger
/// track, and every mic is cut in the same places. Detecting per-mic
/// would smear the source at every join.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Group {
    /// Track guids being edited.
    pub members: Vec<String>,
    /// The guid hits are detected from. Must be one of `members`, or the
    /// group is editing to a reference nobody can hear.
    pub trigger: Option<String>,
}

impl Group {
    pub fn is_valid(&self) -> bool {
        match &self.trigger {
            Some(t) => self.members.contains(t),
            None => false,
        }
    }
}

/// The detector's controls, as the panel holds them.
///
/// Plain values, not `expression_editor_audio::DetectConfig`: this crate
/// does not depend on the audio crate, so the panel holds what the
/// sliders say and the host's bridge (`expression_editor_audio::
/// panel_bridge`) turns them into engine types. Defaults mirror the
/// engine's own.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DetectSettings {
    /// Absolute floor on the detector's fast envelope, dBFS.
    pub threshold_db: f64,
    /// `0.0..=1.0`. Higher keeps softer hits.
    pub sensitivity: f64,
    /// How struck a sound must be, in dB. Advanced.
    pub crest_db: f64,
    /// High-pass corner in Hz ahead of detection only, or `None`.
    pub high_pass_hz: Option<f64>,
    /// Low-pass corner in Hz ahead of detection only, or `None`.
    pub low_pass_hz: Option<f64>,
    /// Linear make-up gain after the filters.
    pub gain: f64,
    /// Shortest gap between two hits, seconds. Advanced.
    pub retrigger_secs: f64,
    /// Fixed shift on every detected hit, seconds. Advanced.
    pub time_offset_secs: f64,
}

impl Default for DetectSettings {
    fn default() -> Self {
        Self {
            threshold_db: -60.0,
            sensitivity: 0.5,
            crest_db: 3.0,
            high_pass_hz: None,
            low_pass_hz: None,
            gain: 1.0,
            retrigger_secs: 0.050,
            time_offset_secs: 0.0,
        }
    }
}

/// A named detector filter block.
///
/// One pick instead of four sliders: dialling a kick in is choosing
/// "where the kick lives", and that is a preset, not a tweak.
// r[impl drums.quantize.filter-presets]
#[derive(Clone, Debug, PartialEq)]
pub struct FilterPreset {
    pub name: String,
    pub high_pass_hz: Option<f64>,
    pub low_pass_hz: Option<f64>,
    /// Make-up gain for what the band-limiting took.
    pub gain: f64,
}

impl FilterPreset {
    /// The presets that ship: where each drum lives, plus flat.
    pub fn builtins() -> Vec<FilterPreset> {
        vec![
            FilterPreset {
                name: "Full kit".into(),
                high_pass_hz: None,
                low_pass_hz: None,
                gain: 1.0,
            },
            FilterPreset {
                name: "Kick".into(),
                high_pass_hz: Some(30.0),
                low_pass_hz: Some(800.0),
                gain: 1.5,
            },
            FilterPreset {
                name: "Snare".into(),
                high_pass_hz: Some(120.0),
                low_pass_hz: Some(5_000.0),
                gain: 1.5,
            },
            FilterPreset {
                name: "Toms".into(),
                high_pass_hz: Some(60.0),
                low_pass_hz: Some(2_000.0),
                gain: 1.5,
            },
        ]
    }
}

/// The grid divisions the target section offers, 1/4 to 1/64.
// r[impl drums.quantize.grid-options]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridDivision {
    Quarter,
    Eighth,
    Sixteenth,
    ThirtySecond,
    SixtyFourth,
}

impl GridDivision {
    pub const ALL: [GridDivision; 5] = [
        GridDivision::Quarter,
        GridDivision::Eighth,
        GridDivision::Sixteenth,
        GridDivision::ThirtySecond,
        GridDivision::SixtyFourth,
    ];

    pub fn label(self) -> &'static str {
        match self {
            GridDivision::Quarter => "1/4",
            GridDivision::Eighth => "1/8",
            GridDivision::Sixteenth => "1/16",
            GridDivision::ThirtySecond => "1/32",
            GridDivision::SixtyFourth => "1/64",
        }
    }

    /// Length in beats (a quarter note is one beat).
    pub fn beats(self) -> f64 {
        match self {
            GridDivision::Quarter => 1.0,
            GridDivision::Eighth => 0.5,
            GridDivision::Sixteenth => 0.25,
            GridDivision::ThirtySecond => 0.125,
            GridDivision::SixtyFourth => 0.0625,
        }
    }
}

/// Straight, triplet or dotted — the multiplier on the division.
// r[impl drums.quantize.grid-options]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum GridFeel {
    #[default]
    Straight,
    Triplet,
    Dotted,
}

impl GridFeel {
    pub const ALL: [GridFeel; 3] = [GridFeel::Straight, GridFeel::Triplet, GridFeel::Dotted];

    pub fn label(self) -> &'static str {
        match self {
            GridFeel::Straight => "straight",
            GridFeel::Triplet => "triplet",
            GridFeel::Dotted => "dotted",
        }
    }

    pub fn factor(self) -> f64 {
        match self {
            GridFeel::Straight => 1.0,
            GridFeel::Triplet => 2.0 / 3.0,
            GridFeel::Dotted => 1.5,
        }
    }
}

/// Per-slider "my default" values, keyed by the slider's name.
///
/// Right-click stores, Alt-click resets — the affordance Perfect Timing
/// ships, because a drum editor re-dials the same kit for every song.
/// Plain data (`Vec<(String, f64)>`) so a host can serialize it with the
/// rest of its settings.
///
/// TODO: the editor has no settings persistence seam yet — hosts hold
/// this in memory today and should write it wherever their settings land
/// once one exists.
// r[impl drums.quantize.slider-defaults]
#[derive(Clone, Debug, Default, PartialEq)]
pub struct SliderDefaults {
    stored: Vec<(String, f64)>,
}

impl SliderDefaults {
    /// Right-click → store `value` as the default for `name`.
    pub fn store(&mut self, name: &str, value: f64) {
        match self.stored.iter_mut().find(|(n, _)| n == name) {
            Some((_, v)) => *v = value,
            None => self.stored.push((name.to_string(), value)),
        }
    }

    /// The stored default for `name`, when there is one.
    pub fn get(&self, name: &str) -> Option<f64> {
        self.stored.iter().find(|(n, _)| n == name).map(|&(_, v)| v)
    }

    /// Alt-click → what the slider resets to: the user's stored default,
    /// or the built-in one when nothing was stored.
    pub fn reset_value(&self, name: &str, built_in: f64) -> f64 {
        self.get(name).unwrap_or(built_in)
    }
}

/// Everything the panel holds.
#[derive(Clone, Debug)]
pub struct QuantizePanel {
    pub config: QuantizeConfig,
    pub mode: WriteMode,
    /// Leading pad before each cut, in the events' unit. SPLIT only —
    /// a cut exactly on a transient clips its attack.
    pub pad: f64,
    /// Crossfade length at each join. SPLIT only.
    pub crossfade: f64,
    pub group: Group,
    /// The detector's controls. The host bridge turns these into a
    /// `DetectConfig`.
    pub detect: DetectSettings,
    /// The advanced disclosure in the Detect section.
    pub advanced: bool,
    /// Grid division and feel. `config.grid` is derived from these via
    /// [`QuantizePanel::grid_in`] whenever the host knows the beat
    /// length; a host without a tempo map can still set `config.grid`
    /// directly.
    pub division: GridDivision,
    pub feel: GridFeel,
    /// Swing on the off-beat divisions, `0.0..=1.0`.
    pub swing: f64,
    /// Whether each division scans a window (`config.tolerance =
    /// Some(..)`) or every hit snaps to its nearest division.
    pub grid_scan: bool,
    /// The window half-width used while `grid_scan` is on, kept even
    /// while it is off so toggling does not forget the dialled value.
    pub tolerance: f64,
    /// Filter presets: the built-ins plus whatever the user saved.
    pub presets: Vec<FilterPreset>,
    /// Which preset the Detect combo shows.
    pub preset: usize,
    /// Per-slider stored defaults.
    pub defaults: SliderDefaults,
}

impl Default for QuantizePanel {
    fn default() -> Self {
        let config = QuantizeConfig::default();
        Self {
            config,
            mode: WriteMode::default(),
            pad: 0.007,
            crossfade: 0.007,
            group: Group::default(),
            detect: DetectSettings::default(),
            advanced: false,
            division: GridDivision::Sixteenth,
            feel: GridFeel::Straight,
            swing: 0.0,
            grid_scan: config.tolerance.is_some(),
            tolerance: config.tolerance.unwrap_or(0.05),
            presets: FilterPreset::builtins(),
            preset: 0,
            defaults: SliderDefaults::default(),
        }
    }
}

impl QuantizePanel {
    /// The grid length in the events' unit, given one beat's length in
    /// that unit — seconds per quarter for audio, PPQ for MIDI.
    // r[impl drums.quantize.grid-options]
    pub fn grid_in(&self, beat_len: f64) -> f64 {
        beat_len * self.division.beats() * self.feel.factor()
    }

    /// Keep `config` in step with the target controls. Called by the
    /// view after every change; hosts that set `config.grid` from a
    /// tempo map call [`QuantizePanel::grid_in`] themselves.
    pub fn sync_config(&mut self) {
        self.config.tolerance = self.grid_scan.then_some(self.tolerance);
    }

    /// Apply the selected filter preset to the detect settings.
    // r[impl drums.quantize.filter-presets]
    pub fn apply_preset(&mut self, index: usize) {
        let Some(p) = self.presets.get(index) else {
            return;
        };
        self.detect.high_pass_hz = p.high_pass_hz;
        self.detect.low_pass_hz = p.low_pass_hz;
        self.detect.gain = p.gain;
        self.preset = index;
    }

    /// Save the current filter block as a named user preset and select
    /// it.
    // r[impl drums.quantize.filter-presets]
    pub fn save_preset(&mut self, name: impl Into<String>) {
        self.presets.push(FilterPreset {
            name: name.into(),
            high_pass_hz: self.detect.high_pass_hz,
            low_pass_hz: self.detect.low_pass_hz,
            gain: self.detect.gain,
        });
        self.preset = self.presets.len() - 1;
    }
}

/// A vertical line over the waveform: where a hit is, and where it goes.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TriggerLine {
    pub x: f64,
    /// Where it will move to, in pixels. Equal to `x` when it does not
    /// move.
    pub to_x: f64,
    /// True when the planner deliberately left it alone.
    ///
    /// Surfaced rather than hidden: `Plan::unmatched` exists precisely
    /// so "why did that hit not move" is answerable, and a line that is
    /// simply absent answers nothing.
    pub unmatched: bool,
}

impl TriggerLine {
    pub fn moves(&self) -> bool {
        (self.to_x - self.x).abs() > f64::EPSILON
    }
}

/// The panel's preview of a plan, in pixels.
pub struct Preview {
    pub lines: Vec<TriggerLine>,
    /// Grid divisions in view, so the target is visible and not implied.
    pub divisions: Vec<f64>,
}

/// Plan and lay out, in one step.
pub fn preview<E: Timed>(
    events: &[E],
    panel: &QuantizePanel,
    width: f64,
    span: (f64, f64),
) -> (Plan, Preview) {
    let mut p = plan(events, panel.config);
    // Swing is target placement only — a pass over the plan, so the
    // planner itself never knows the grid is swung.
    swing(&mut p, panel.config, panel.swing);
    let view = lay_out(&p, width, span, panel.config);
    (p, view)
}

fn x_of(t: f64, width: f64, span: (f64, f64)) -> f64 {
    let (from, to) = span;
    let len = (to - from).max(1e-9);
    ((t - from) / len).clamp(0.0, 1.0) * width
}

/// Turn a plan into lines and divisions.
pub fn lay_out(p: &Plan, width: f64, span: (f64, f64), cfg: QuantizeConfig) -> Preview {
    let mut lines: Vec<TriggerLine> = p
        .moves
        .iter()
        .map(|m| TriggerLine {
            x: x_of(m.from, width, span),
            to_x: x_of(m.to, width, span),
            unmatched: false,
        })
        .collect();

    lines.extend(p.unmatched.iter().map(|&at| {
        let x = x_of(at, width, span);
        TriggerLine {
            x,
            to_x: x,
            unmatched: true,
        }
    }));
    lines.sort_by(|a, b| a.x.partial_cmp(&b.x).unwrap_or(core::cmp::Ordering::Equal));

    let mut divisions = Vec::new();
    if cfg.grid > 0.0 {
        let (from, to) = span;
        let first = ((from - cfg.grid_offset) / cfg.grid).ceil();
        let mut d = cfg.grid_offset + first * cfg.grid;
        while d <= to {
            divisions.push(x_of(d, width, span));
            d += cfg.grid;
        }
    }

    Preview { lines, divisions }
}

/// One bar of the sensitivity histogram.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bin {
    /// Lower edge of the bin, in event strength.
    pub from: f64,
    pub to: f64,
    pub count: usize,
}

/// Distribution of event strengths.
///
/// Perfect Timing has one, and it is the difference between guessing at
/// a slider and seeing where the hits actually sit. It also shows the
/// threshold against the material's own noise floor, which is what makes
/// the crest reading below threshold interpretable rather than
/// mysterious.
pub fn histogram<E: Timed>(events: &[E], bins: usize) -> Vec<Bin> {
    let bins = bins.max(1);
    let mut out: Vec<Bin> = (0..bins)
        .map(|i| Bin {
            from: i as f64 / bins as f64,
            to: (i + 1) as f64 / bins as f64,
            count: 0,
        })
        .collect();

    for e in events {
        let s = e.strength().clamp(0.0, 1.0);
        // The top edge belongs to the last bin rather than falling off
        // the end.
        let idx = ((s * bins as f64) as usize).min(bins - 1);
        out[idx].count += 1;
    }
    out
}

/// Where the sensitivity threshold sits on the histogram, 0..1.
pub fn threshold_position(cfg: &QuantizeConfig) -> f64 {
    cfg.min_strength.clamp(0.0, 1.0)
}

/// How many events the filter is currently excluding.
///
/// The number a user actually wants next to the slider: "how much am I
/// about to leave alone".
pub fn excluded_count<E: Timed>(events: &[E], cfg: &QuantizeConfig) -> usize {
    events
        .iter()
        .filter(|e| e.strength() < cfg.min_strength)
        .count()
}
