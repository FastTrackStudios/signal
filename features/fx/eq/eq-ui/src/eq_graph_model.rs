//! Shared data model for the EQ graph.
//!
//! These types are used by both the Dioxus interaction layer and the vello
//! painter. Keeping them separate from the component keeps the rendering and
//! interaction modules from depending on the monolithic widget implementation.

use std::sync::Arc;

use parking_lot::RwLock;
use spectrum_analyzer::dsp::AnalyzerSnapshot;

/// Maximum number of EQ bands supported.
pub const MAX_BANDS: usize = 24;

/// Stereo placement mode for EQ bands.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum StereoMode {
    #[default]
    Stereo,
    Left,
    Right,
    Mid,
    Side,
}

impl StereoMode {
    /// Get display label for the stereo mode.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Stereo => "Stereo",
            Self::Left => "Left",
            Self::Right => "Right",
            Self::Mid => "Mid",
            Self::Side => "Side",
        }
    }

    /// Get short label for the stereo mode.
    pub fn short_label(&self) -> &'static str {
        match self {
            Self::Stereo => "ST",
            Self::Left => "L",
            Self::Right => "R",
            Self::Mid => "M",
            Self::Side => "S",
        }
    }
}

/// EQ graph band data for rendering.
///
/// A simplified band representation for the EQ graph when
/// the full audiocore-dsp types aren't needed.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct EqBand {
    /// Band index (0-23).
    pub index: usize,
    /// Whether this band slot is in use.
    pub used: bool,
    /// Whether the band is enabled (bypassed when false).
    pub enabled: bool,
    /// Center frequency in Hz (10-30000).
    pub frequency: f32,
    /// Gain in dB (-30 to +30).
    pub gain: f32,
    /// Q factor (0.025 to 40). For cut filters, this represents slope order.
    pub q: f32,
    /// Filter shape (bell, shelf, cut, etc.).
    pub shape: EqBandShape,
    /// Whether this band is soloed (only this band audible).
    pub solo: bool,
    /// Stereo placement mode.
    pub stereo_mode: StereoMode,
    /// User-assigned short name (e.g. "Honk"). Empty if unnamed. Snapshot of the
    /// persisted `BandParams::name` — shown on the node label / popup.
    pub name: String,
}

/// EQ band filter shape.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum EqBandShape {
    #[default]
    Bell,
    LowShelf,
    HighShelf,
    LowCut,
    HighCut,
    Notch,
    BandPass,
    TiltShelf,
    FlatTilt,
    AllPass,
}

impl EqBandShape {
    /// Get display label for the filter shape.
    pub fn label(&self) -> &'static str {
        match self {
            Self::Bell => "Bell",
            Self::LowShelf => "Low Shelf",
            Self::HighShelf => "High Shelf",
            Self::LowCut => "Low Cut",
            Self::HighCut => "High Cut",
            Self::Notch => "Notch",
            Self::BandPass => "Band Pass",
            Self::TiltShelf => "Tilt Shelf",
            Self::FlatTilt => "Flat Tilt",
            Self::AllPass => "All Pass",
        }
    }

    /// Whether this filter type uses slope (dB/oct) instead of Q.
    pub fn uses_slope(&self) -> bool {
        matches!(self, Self::LowCut | Self::HighCut)
    }

    /// Whether this filter type uses gain.
    pub fn uses_gain(&self) -> bool {
        !matches!(
            self,
            Self::LowCut | Self::HighCut | Self::Notch | Self::AllPass
        )
    }

    /// All available filter shapes.
    pub fn all() -> &'static [EqBandShape] {
        &[
            Self::Bell,
            Self::LowShelf,
            Self::HighShelf,
            Self::LowCut,
            Self::HighCut,
            Self::Notch,
            Self::BandPass,
            Self::TiltShelf,
            Self::FlatTilt,
            Self::AllPass,
        ]
    }
}

/// Convert Q value to slope in dB/octave for cut filters.
pub fn q_to_slope_db(q: f32) -> f32 {
    // Q represents filter order: 0.5 = 6dB/oct, 1.0 = 12dB/oct, etc.
    (q * 2.0).round().max(1.0) * 6.0
}

/// Convert slope in dB/octave to Q value for cut filters.
pub fn slope_db_to_q(slope_db: f32) -> f32 {
    // 6dB/oct = 0.5, 12dB/oct = 1.0, etc.
    (slope_db / 6.0).round().max(1.0) / 2.0
}

/// Band colors matching Pro-Q / ZL Equalizer style.
/// Colors cycle through for bands 0-23, matching the screenshot reference.
pub const BAND_COLORS: &[&str] = &[
    "#4ade80", // 1: Green
    "#60a5fa", // 2: Blue
    "#c084fc", // 3: Purple
    "#f472b6", // 4: Pink
    "#fb7185", // 5: Red/Rose
    "#fb923c", // 6: Orange
    "#facc15", // 7: Yellow
    "#a3e635", // 8: Lime
    "#34d399", // 9: Emerald
    "#22d3d8", // 10: Cyan
    "#818cf8", // 11: Indigo
    "#e879f9", // 12: Fuchsia
    "#f87171", // 13: Red
    "#fdba74", // 14: Light Orange
    "#fde047", // 15: Light Yellow
    "#bef264", // 16: Light Lime
    "#6ee7b7", // 17: Light Emerald
    "#67e8f9", // 18: Light Cyan
    "#a5b4fc", // 19: Light Indigo
    "#f0abfc", // 20: Light Fuchsia
    "#fca5a5", // 21: Light Red
    "#fed7aa", // 22: Peach
    "#fef08a", // 23: Pale Yellow
    "#d9f99d", // 24: Pale Lime
];

/// Get the color for a band by index.
pub fn get_band_color(index: usize) -> &'static str {
    BAND_COLORS[index % BAND_COLORS.len()]
}

/// Map a frequency (Hz) to a color across the audible spectrum, low → high as
/// red → violet — the visible-light analogy (higher audio frequency reads as
/// "bluer"). Used to color band nodes by their center frequency (ReJJ-style) so
/// the graph is readable at a glance: bass nodes warm, treble nodes cool.
/// Returns a `#rrggbb` hex string.
pub fn freq_to_color(hz: f64) -> String {
    let lo = 20.0_f64.log10();
    let hi = 20_000.0_f64.log10();
    let t = ((hz.max(1.0).log10() - lo) / (hi - lo)).clamp(0.0, 1.0);
    // Hue sweeps red (0°) → violet (~290°) across the spectrum.
    hsl_to_hex(t * 290.0, 0.68, 0.58)
}

/// HSL (h in degrees 0–360, s/l in 0–1) → `#rrggbb` hex.
pub fn hsl_to_hex(h: f64, s: f64, l: f64) -> String {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r1, g1, b1) = match hp as i32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = l - c / 2.0;
    let to_u8 = |v: f64| ((v + m) * 255.0).round().clamp(0.0, 255.0) as u8;
    format!("#{:02x}{:02x}{:02x}", to_u8(r1), to_u8(g1), to_u8(b1))
}

/// Get the fill color (semi-transparent) for a band by index.
pub fn get_band_fill_color(index: usize) -> String {
    let hex = get_band_color(index);
    if let (Ok(r), Ok(g), Ok(b)) = (
        u8::from_str_radix(&hex[1..3], 16),
        u8::from_str_radix(&hex[3..5], 16),
        u8::from_str_radix(&hex[5..7], 16),
    ) {
        format!("rgba({r}, {g}, {b}, 0.30)")
    } else {
        "rgba(100, 100, 100, 0.30)".to_string()
    }
}

/// Shared state between the Dioxus component and paint backends.
pub struct EqGraphRenderState {
    pub bands: RwLock<Vec<EqBand>>,
    pub spectrum_db: RwLock<Vec<f32>>,
    pub model_response_db: RwLock<Vec<f32>>,
    /// Full analyzer snapshot (pre/post/external/collision). When it carries
    /// data the painter uses it; otherwise it falls back to `spectrum_db`.
    pub analyzer: RwLock<AnalyzerSnapshot>,
    pub config: RwLock<GraphConfig>,
    pub interaction: RwLock<InteractionState>,
    /// Active EQ cheat-sheet overlay (instrument / general profile) shaded behind
    /// the curves, or `None` for off. Set by the component from the track name or
    /// a manual selection.
    pub overlay: RwLock<Option<&'static crate::cheatsheet::InstrumentProfile>>,
}

impl EqGraphRenderState {
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            bands: RwLock::new(Vec::new()),
            spectrum_db: RwLock::new(Vec::new()),
            model_response_db: RwLock::new(Vec::new()),
            analyzer: RwLock::new(AnalyzerSnapshot::default()),
            config: RwLock::new(GraphConfig::default()),
            interaction: RwLock::new(InteractionState::default()),
            overlay: RwLock::new(None),
        })
    }
}

/// The selectable display ranges, in `db_range` param index order
/// (`fx.eq.display.range`). THE source of truth for the display range —
/// the graph component, the painter, this model and the param formatter all
/// map through it; none carries its own default
/// (`fx.eq.display.defaults-agree`).
// r[impl fx.eq.display.range]
// r[impl fx.eq.display.defaults-agree]
pub const DB_RANGE_STEPS: [f64; 6] = [3.0, 6.0, 12.0, 18.0, 24.0, 30.0];

/// Default display range: the `db_range` param default (index 0 = ±3 dB —
/// a tight range suits the subtle, shelf-first baseline moves).
pub const DEFAULT_DB_RANGE: f64 = DB_RANGE_STEPS[0];

/// The dB range a `db_range` param index selects.
pub fn db_range_for_index(index: i32) -> f64 {
    DB_RANGE_STEPS
        .get(index.max(0) as usize)
        .copied()
        .unwrap_or(30.0)
}

/// The smallest index whose range shows `db` without clipping, when that is
/// larger than `from` — the auto-range expansion step
/// (`fx.eq.display.auto-range`).
// r[impl fx.eq.display.auto-range]
pub fn db_range_index_containing(db: f64, from: i32) -> Option<i32> {
    DB_RANGE_STEPS
        .iter()
        .position(|&r| r >= db.abs())
        .map(|i| i as i32)
        .filter(|&i| i > from)
}

#[derive(Clone)]
pub struct GraphConfig {
    pub db_range: f64,
    pub min_freq: f64,
    pub max_freq: f64,
    pub sample_rate: f64,
    pub show_grid: bool,
    pub show_freq_labels: bool,
    pub show_db_labels: bool,
    pub fill_curve: bool,
    /// Position of the graph area in the window (physical pixels).
    /// Set by the Dioxus component so the overlay paints in the right place.
    pub rect_x: f64,
    pub rect_y: f64,
    pub rect_w: f64,
    pub rect_h: f64,
    /// Display scale factor (DPR). The paint source receives the canvas
    /// content_box in physical pixels, so `rect_w` / `rect_h` are
    /// physical too — divide by `scale` to convert to CSS pixels for
    /// hit-testing (where `evt.element_coordinates()` lives).
    pub scale: f64,
}

impl Default for GraphConfig {
    fn default() -> Self {
        Self {
            db_range: DEFAULT_DB_RANGE,
            min_freq: 20.0,
            max_freq: 20000.0,
            sample_rate: 48000.0,
            show_grid: true,
            show_freq_labels: true,
            show_db_labels: true,
            fill_curve: true,
            rect_x: 0.0,
            rect_y: 0.0,
            rect_w: 800.0,
            rect_h: 350.0,
            scale: 1.0,
        }
    }
}

#[derive(Clone, Default)]
pub struct InteractionState {
    pub hovered_band: Option<usize>,
    pub dragging_band: Option<usize>,
    pub focused_band: Option<usize>,
    pub selected_bands: Vec<usize>,
}
