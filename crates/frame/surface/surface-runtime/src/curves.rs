//! Curve providers: named functions of params that `Curve` controls draw.
//!
//! A provider takes its arguments by name ([`CurveInputs`]) and returns one
//! or more [`Series`] of `(x, y)` points in engineering units (Hz and dB,
//! seconds and level). The runtime evaluates a curve only when one of its
//! inputs changes and shares the result between every control asking for
//! the same thing (an EQ graph's combined curve and its per-band curves are
//! one evaluation).
//!
//! frame ships builtins ([`CurveRegistry::builtin`]) so surfaces work
//! standalone; an engine registers its exact implementation under the same
//! name to override one.
//!
//! | provider | inputs | series |
//! |---|---|---|
//! | `eq.response` | `b{n}_freq`, `b{n}_gain`, `b{n}_q`, `b{n}_shape`, `b{n}_on`, `b{n}_used`, `b{n}_slope` (dB/oct) or `b{n}_slope_index` (0‥10: 0, 6, 12, 18, 24, 30, 36, 48, 72, 96 dB/oct, brickwall), `b{n}_dyn_range`, `stems`, `sample_rate` | `sum`, then `b1`, `b2`, …; with `stems` on, `b{n}_stem` (the band's node down to 0 dB); with a `b{n}_dyn_range`, `b{n}_dyn` (the curve at gain + range) |
//! | `comp.transfer` | `threshold`, `ratio`, `knee`, `makeup` | `transfer`, `unity` |
//! | `adsr.envelope` | `attack`, `decay`, `sustain` (0..1), `release`, `hold`, `curve`; playheads `v{i}_stage`, `v{i}_pos` | `envelope`, `playheads` |
//! | `filter.response` | `freq`, `q`, `kind`, `gain`, `slope`, `sample_rate` | `response` |

use std::collections::{BTreeMap, HashMap};
use std::f64::consts::PI;
use std::sync::Arc;

use surface_proto::Axis;

/// One named series of `(x, y)` points.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Series {
    pub name: String,
    pub points: Vec<(f64, f64)>,
}

impl Series {
    #[must_use]
    pub fn new(name: impl Into<String>, points: Vec<(f64, f64)>) -> Self {
        Self {
            name: name.into(),
            points,
        }
    }
}

/// What a provider is evaluated with: its named arguments and the x axis
/// it is drawn over.
#[derive(Debug, Clone, PartialEq)]
pub struct CurveInputs {
    pub values: BTreeMap<String, f64>,
    pub x: Axis,
    /// How many points across the axis a smooth provider should return.
    pub samples: usize,
}

impl CurveInputs {
    #[must_use]
    pub fn new(x: Axis) -> Self {
        Self {
            values: BTreeMap::new(),
            x,
            samples: 256,
        }
    }

    #[must_use]
    pub fn with(mut self, name: &str, v: f64) -> Self {
        self.values.insert(name.to_string(), v);
        self
    }

    #[must_use]
    pub fn get(&self, name: &str) -> Option<f64> {
        self.values.get(name).copied().filter(|v| v.is_finite())
    }

    #[must_use]
    pub fn or(&self, name: &str, default: f64) -> f64 {
        self.get(name).unwrap_or(default)
    }

    /// `samples` x values evenly across the axis (in its own spacing: log
    /// for frequency).
    #[must_use]
    pub fn xs(&self) -> Vec<f64> {
        let n = self.samples.max(2);
        (0..n)
            .map(|i| self.x.from_unit(i as f64 / (n - 1) as f64))
            .collect()
    }
}

/// A curve provider.
pub type Provider = Arc<dyn Fn(&CurveInputs) -> Vec<Series> + Send + Sync>;

/// Named curve providers.
#[derive(Clone)]
pub struct CurveRegistry {
    providers: HashMap<String, Provider>,
    /// Providers that also read the clock (`time`, seconds): re-evaluated
    /// every frame (a visualizer's playhead, an LFO).
    animated: std::collections::HashSet<String>,
}

impl std::fmt::Debug for CurveRegistry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut names: Vec<&String> = self.providers.keys().collect();
        names.sort();
        f.debug_struct("CurveRegistry")
            .field("providers", &names)
            .finish()
    }
}

impl Default for CurveRegistry {
    fn default() -> Self {
        Self::builtin()
    }
}

impl CurveRegistry {
    /// No providers at all.
    #[must_use]
    pub fn empty() -> Self {
        Self {
            providers: HashMap::new(),
            animated: std::collections::HashSet::new(),
        }
    }

    /// frame's builtins: `eq.response`, `comp.transfer`, `adsr.envelope`,
    /// `filter.response`.
    #[must_use]
    pub fn builtin() -> Self {
        let mut r = Self::empty();
        r.register("eq.response", eq_response);
        r.register("comp.transfer", comp_transfer);
        r.register("adsr.envelope", adsr_envelope);
        r.register("filter.response", filter_response);
        crate::viz_curves::register(&mut r);
        r
    }

    /// Register (or override) `name`.
    pub fn register(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&CurveInputs) -> Vec<Series> + Send + Sync + 'static,
    ) {
        self.providers.insert(name.into(), Arc::new(f));
    }

    /// Register (or override) `name` as a provider that reads the clock:
    /// the runtime passes `time` (seconds, [`crate::Runtime::set_clock`])
    /// and evaluates it every frame.
    pub fn register_animated(
        &mut self,
        name: impl Into<String>,
        f: impl Fn(&CurveInputs) -> Vec<Series> + Send + Sync + 'static,
    ) {
        let name = name.into();
        self.animated.insert(name.clone());
        self.register(name, f);
    }

    /// Whether `name` reads the clock.
    #[must_use]
    pub fn is_animated(&self, name: &str) -> bool {
        self.animated.contains(name)
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.providers.contains_key(name)
    }

    /// Evaluate `name`; `None` when no such provider is registered.
    #[must_use]
    pub fn eval(&self, name: &str, inputs: &CurveInputs) -> Option<Vec<Series>> {
        self.providers.get(name).map(|f| f(inputs))
    }
}

// ── biquads (RBJ audio EQ cookbook) ──────────────────────────────────────

/// Normalised biquad coefficients (`a0` = 1).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Biquad {
    pub b0: f64,
    pub b1: f64,
    pub b2: f64,
    pub a1: f64,
    pub a2: f64,
}

/// The EQ band shapes `eq.response` knows, by option index of a band's
/// `shape` choice param.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Shape {
    Bell,
    LowShelf,
    HighShelf,
    LowCut,
    HighCut,
    Notch,
    BandPass,
    /// Tilts about its frequency: half the gain down below, half up above.
    TiltShelf,
    /// A gentle whole-spectrum tilt through its frequency.
    FlatTilt,
    /// Flat magnitude (phase only).
    AllPass,
}

impl Shape {
    /// Option order of a band's shape param: Bell, Low Shelf, High Shelf,
    /// Low Cut, High Cut, Notch, Band Pass, Tilt Shelf, Flat Tilt, All Pass.
    pub const NAMES: [&'static str; 10] = [
        "Bell",
        "Low Shelf",
        "High Shelf",
        "Low Cut",
        "High Cut",
        "Notch",
        "Band Pass",
        "Tilt Shelf",
        "Flat Tilt",
        "All Pass",
    ];

    /// Whether the shape has a gain (cuts, notch and all-pass don't).
    #[must_use]
    pub fn uses_gain(self) -> bool {
        !matches!(
            self,
            Self::LowCut | Self::HighCut | Self::Notch | Self::AllPass
        )
    }

    #[must_use]
    pub fn from_index(i: f64) -> Self {
        match i.round() as i64 {
            1 => Self::LowShelf,
            2 => Self::HighShelf,
            3 => Self::LowCut,
            4 => Self::HighCut,
            5 => Self::Notch,
            6 => Self::BandPass,
            7 => Self::TiltShelf,
            8 => Self::FlatTilt,
            9 => Self::AllPass,
            _ => Self::Bell,
        }
    }
}

impl Biquad {
    fn normalized(b0: f64, b1: f64, b2: f64, a0: f64, a1: f64, a2: f64) -> Self {
        Self {
            b0: b0 / a0,
            b1: b1 / a0,
            b2: b2 / a0,
            a1: a1 / a0,
            a2: a2 / a0,
        }
    }

    /// One RBJ cookbook section of `shape` at `freq` Hz, `gain` dB, `q`.
    #[must_use]
    pub fn rbj(shape: Shape, freq: f64, gain: f64, q: f64, sample_rate: f64) -> Self {
        let w0 = 2.0 * PI * (freq / sample_rate).clamp(1e-6, 0.4999);
        let (sn, cs) = w0.sin_cos();
        let q = q.max(1e-3);
        let alpha = sn / (2.0 * q);
        let a = 10f64.powf(gain / 40.0);
        let sa = 2.0 * a.sqrt() * alpha;
        match shape {
            Shape::Bell => Self::normalized(
                1.0 + alpha * a,
                -2.0 * cs,
                1.0 - alpha * a,
                1.0 + alpha / a,
                -2.0 * cs,
                1.0 - alpha / a,
            ),
            Shape::LowShelf => Self::normalized(
                a * ((a + 1.0) - (a - 1.0) * cs + sa),
                2.0 * a * ((a - 1.0) - (a + 1.0) * cs),
                a * ((a + 1.0) - (a - 1.0) * cs - sa),
                (a + 1.0) + (a - 1.0) * cs + sa,
                -2.0 * ((a - 1.0) + (a + 1.0) * cs),
                (a + 1.0) + (a - 1.0) * cs - sa,
            ),
            Shape::HighShelf => Self::normalized(
                a * ((a + 1.0) + (a - 1.0) * cs + sa),
                -2.0 * a * ((a - 1.0) + (a + 1.0) * cs),
                a * ((a + 1.0) + (a - 1.0) * cs - sa),
                (a + 1.0) - (a - 1.0) * cs + sa,
                2.0 * ((a - 1.0) - (a + 1.0) * cs),
                (a + 1.0) - (a - 1.0) * cs - sa,
            ),
            Shape::LowCut => Self::normalized(
                (1.0 + cs) / 2.0,
                -(1.0 + cs),
                (1.0 + cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Shape::HighCut => Self::normalized(
                (1.0 - cs) / 2.0,
                1.0 - cs,
                (1.0 - cs) / 2.0,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            Shape::Notch => {
                Self::normalized(1.0, -2.0 * cs, 1.0, 1.0 + alpha, -2.0 * cs, 1.0 - alpha)
            }
            Shape::BandPass => {
                Self::normalized(alpha, 0.0, -alpha, 1.0 + alpha, -2.0 * cs, 1.0 - alpha)
            }
            Shape::AllPass => Self::normalized(
                1.0 - alpha,
                -2.0 * cs,
                1.0 + alpha,
                1.0 + alpha,
                -2.0 * cs,
                1.0 - alpha,
            ),
            // Tilts are built from shelves in `band_sections`.
            Shape::TiltShelf | Shape::FlatTilt => {
                Self::rbj(Shape::HighShelf, freq, gain, q, sample_rate)
            }
        }
    }

    /// A first-order (6 dB/oct) low-pass (`high = false`) or high-pass
    /// section, bilinear-transformed.
    #[must_use]
    pub fn first_order(high: bool, freq: f64, sample_rate: f64) -> Self {
        let k = (PI * (freq / sample_rate).clamp(1e-6, 0.4999)).tan();
        let a1 = (k - 1.0) / (k + 1.0);
        if high {
            Self {
                b0: 1.0 / (1.0 + k),
                b1: -1.0 / (1.0 + k),
                b2: 0.0,
                a1,
                a2: 0.0,
            }
        } else {
            Self {
                b0: k / (1.0 + k),
                b1: k / (1.0 + k),
                b2: 0.0,
                a1,
                a2: 0.0,
            }
        }
    }

    /// Magnitude in dB at `freq` Hz.
    #[must_use]
    pub fn db_at(&self, freq: f64, sample_rate: f64) -> f64 {
        let w = 2.0 * PI * freq / sample_rate;
        let phi = (w / 2.0).sin().powi(2);
        let Self { b0, b1, b2, a1, a2 } = *self;
        let num = (b0 + b1 + b2).powi(2) - 4.0 * (b0 * b1 + 4.0 * b0 * b2 + b1 * b2) * phi
            + 16.0 * b0 * b2 * phi * phi;
        let den =
            (1.0 + a1 + a2).powi(2) - 4.0 * (a1 + 4.0 * a2 + a1 * a2) * phi + 16.0 * a2 * phi * phi;
        if den <= 0.0 || num <= 0.0 {
            return -120.0;
        }
        (10.0 * (num / den).log10()).max(-120.0)
    }
}

/// The sections of one filter band, by the textbook (cuts cascade
/// Butterworth sections to their slope; a 12 dB/oct cut keeps the band's own
/// Q as its resonance; bells and notches take their Q as the RBJ Q) — what
/// `filter.response` draws. An EQ band is [`eq_band_sections`].
#[must_use]
pub fn band_sections(
    shape: Shape,
    freq: f64,
    gain: f64,
    q: f64,
    slope_db: f64,
    sample_rate: f64,
) -> Vec<Biquad> {
    match shape {
        Shape::LowCut | Shape::HighCut => {
            let high = shape == Shape::LowCut;
            let order = ((slope_db / 6.0).round() as i64).clamp(1, 16) as usize;
            if order == 2 {
                return vec![Biquad::rbj(shape, freq, 0.0, q, sample_rate)];
            }
            let mut out: Vec<Biquad> = (0..order / 2)
                .map(|k| {
                    let theta = (order - 2 * k - 1) as f64 * PI / (2 * order) as f64;
                    Biquad::rbj(shape, freq, 0.0, 1.0 / (2.0 * theta.cos()), sample_rate)
                })
                .collect();
            if order % 2 == 1 {
                out.push(Biquad::first_order(high, freq, sample_rate));
            }
            out
        }
        // Half the gain down below the frequency, half up above.
        Shape::TiltShelf => vec![
            Biquad::rbj(Shape::LowShelf, freq, -gain / 2.0, q, sample_rate),
            Biquad::rbj(Shape::HighShelf, freq, gain / 2.0, q, sample_rate),
        ],
        // The same, spread across the spectrum.
        Shape::FlatTilt => vec![
            Biquad::rbj(Shape::LowShelf, freq, -gain / 2.0, 0.3, sample_rate),
            Biquad::rbj(Shape::HighShelf, freq, gain / 2.0, 0.3, sample_rate),
        ],
        _ => vec![Biquad::rbj(shape, freq, gain, q, sample_rate)],
    }
}

/// The sections of one EQ band with the band's Q in Pro-Q's sense — which is
/// processor's `eq-dsp`, what signal's rig (and FTS EQ) designs and draws:
///
/// - a cut's Q scales its most resonant Butterworth section, and `Q = 1` is
///   Butterworth (`cascade_qs`: analog damping `√2 / (Q_natural · Q)`), so
///   the default 0.707 is a gentler knee, 6 dB down at the corner;
/// - a bell's and a notch's section runs at `Q / √2` (`bell_s2_proq4`,
///   `notch`), so 0.707 is a broad Q-0.5 bell.
///
/// Shelves and the rest are [`band_sections`]'s.
#[must_use]
pub fn eq_band_sections(
    shape: Shape,
    freq: f64,
    gain: f64,
    q: f64,
    slope_db: f64,
    sample_rate: f64,
) -> Vec<Biquad> {
    match shape {
        Shape::LowCut | Shape::HighCut => {
            let high = shape == Shape::LowCut;
            let order = ((slope_db / 6.0).round() as i64).clamp(1, 16) as usize;
            let mut out: Vec<Biquad> = (0..order / 2)
                .map(|k| {
                    let theta = (order - 2 * k - 1) as f64 * PI / (2 * order) as f64;
                    // The first section is the most resonant (θ largest).
                    let user = if k == 0 { q.max(1e-3) } else { 1.0 };
                    Biquad::rbj(shape, freq, 0.0, user / (2.0 * theta.cos()), sample_rate)
                })
                .collect();
            if order % 2 == 1 {
                out.push(Biquad::first_order(high, freq, sample_rate));
            }
            out
        }
        Shape::Bell | Shape::Notch => vec![Biquad::rbj(
            shape,
            freq,
            gain,
            q * std::f64::consts::FRAC_1_SQRT_2,
            sample_rate,
        )],
        _ => band_sections(shape, freq, gain, q, slope_db, sample_rate),
    }
}

/// dB/oct of slope index `i` (a cut's slope choice): 0, 6, 12, 18, 24, 30,
/// 36, 48, 72, 96, and brickwall (drawn as 96 steeper still).
#[must_use]
pub fn slope_db(index: f64) -> f64 {
    const DB: [f64; 11] = [
        6.0, 6.0, 12.0, 18.0, 24.0, 30.0, 36.0, 48.0, 72.0, 96.0, 96.0,
    ];
    DB[(index.max(0.0).round() as usize).min(10)]
}

/// The band numbers `b{n}_…` inputs name, ascending.
fn bands(inputs: &CurveInputs) -> Vec<u32> {
    let mut out: Vec<u32> = inputs
        .values
        .keys()
        .filter_map(|k| {
            k.strip_prefix('b')?
                .split_once('_')
                .and_then(|(n, _)| n.parse().ok())
        })
        .collect();
    out.sort_unstable();
    out.dedup();
    out
}

/// `eq.response`: every band's magnitude (dB) over the x axis, and their
/// sum. Series `sum`, then `b{n}` per band — empty for a band that's off.
#[must_use]
pub fn eq_response(inputs: &CurveInputs) -> Vec<Series> {
    let sr = inputs.or("sample_rate", 48_000.0);
    let xs = inputs.xs();
    let mut sum = vec![0.0; xs.len()];
    let mut out = vec![Series::default()];
    let mut extra = Vec::new();
    let stems = inputs.or("stems", 0.0) >= 0.5;
    for n in bands(inputs) {
        let arg = |k: &str| inputs.get(&format!("b{n}_{k}"));
        let on = arg("on").is_none_or(|v| v >= 0.5) && arg("used").is_none_or(|v| v >= 0.5);
        let name = format!("b{n}");
        let Some(freq) = arg("freq").filter(|_| on) else {
            out.push(Series::new(name.clone(), Vec::new()));
            if stems {
                extra.push(Series::new(format!("{name}_stem"), Vec::new()));
            }
            if arg("dyn_range").is_some() {
                extra.push(Series::new(format!("{name}_dyn"), Vec::new()));
            }
            continue;
        };
        let shape = Shape::from_index(arg("shape").unwrap_or(0.0));
        let slope = arg("slope_index").map_or_else(|| arg("slope").unwrap_or(12.0), slope_db);
        let (gain, q) = (arg("gain").unwrap_or(0.0), arg("q").unwrap_or(0.707));
        let sections = eq_band_sections(shape, freq, gain, q, slope, sr);
        let db = |f: f64| sections.iter().map(|s| s.db_at(f, sr)).sum::<f64>();
        let pts: Vec<(f64, f64)> = xs.iter().map(|&f| (f, db(f))).collect();
        for (acc, (_, db)) in sum.iter_mut().zip(&pts) {
            *acc += db;
        }
        out.push(Series::new(name.clone(), pts));
        if stems {
            let at = db(freq);
            // From the node (its gain) to 0 dB, as processor's graph draws it
            // (`paint_connecting_lines`), when the band does anything there.
            let stem = if at.abs() > 0.1 && gain.abs() > 1e-9 {
                vec![(freq, 0.0), (freq, gain)]
            } else {
                Vec::new()
            };
            extra.push(Series::new(format!("{name}_stem"), stem));
        }
        if let Some(range) = arg("dyn_range") {
            let pts = if range.abs() >= 0.05 && shape.uses_gain() {
                let far = eq_band_sections(shape, freq, gain + range, q, slope, sr);
                xs.iter()
                    .map(|&f| (f, far.iter().map(|s| s.db_at(f, sr)).sum::<f64>()))
                    .collect()
            } else {
                Vec::new()
            };
            extra.push(Series::new(format!("{name}_dyn"), pts));
        }
    }
    out[0] = Series::new("sum", xs.iter().copied().zip(sum).collect());
    out.extend(extra);
    out
}

/// A compressor's static curve: output dB for input `x` dB with a soft
/// knee `knee` dB wide (Giannoulis, Massberg & Reiss), plus `makeup`.
#[must_use]
pub fn comp_gain(x: f64, threshold: f64, ratio: f64, knee: f64, makeup: f64) -> f64 {
    let r = ratio.max(1.0);
    let over = x - threshold;
    let y = if knee > 0.0 && (2.0 * over).abs() <= knee {
        x + (1.0 / r - 1.0) * (over + knee / 2.0).powi(2) / (2.0 * knee)
    } else if 2.0 * over > knee.max(0.0) {
        threshold + over / r
    } else {
        x
    };
    y + makeup
}

/// `comp.transfer`: output level against input level. Series `transfer`
/// and `unity` (the 1:1 diagonal, for reference).
#[must_use]
pub fn comp_transfer(inputs: &CurveInputs) -> Vec<Series> {
    let (t, r, k, m) = (
        inputs.or("threshold", -18.0),
        inputs.or("ratio", 4.0),
        inputs.or("knee", 6.0),
        inputs.or("makeup", 0.0),
    );
    let xs = inputs.xs();
    vec![
        Series::new(
            "transfer",
            xs.iter().map(|&x| (x, comp_gain(x, t, r, k, m))).collect(),
        ),
        Series::new("unity", xs.iter().map(|&x| (x, x)).collect()),
    ]
}

/// Progress `p` (0..1) through an envelope segment, bent by `curve`: 0 is a
/// straight line, positive moves fast first (an analog RC charge), negative
/// slow first.
#[must_use]
pub fn bend(p: f64, curve: f64) -> f64 {
    const K: f64 = 5.0;
    let p = p.clamp(0.0, 1.0);
    let c = -curve.clamp(-1.0, 1.0) * K;
    if c.abs() < 1e-6 {
        p
    } else {
        (c * p).exp_m1() / c.exp_m1()
    }
}

/// `adsr.envelope`: level (0..1) over time (s) — attack to full, decay to
/// `sustain`, held for `hold` s (drawn only; default 0.5), release to 0.
/// `curve` bends every segment (default 0.5).
#[must_use]
pub fn adsr_envelope(inputs: &CurveInputs) -> Vec<Series> {
    let a = inputs.or("attack", 0.01).max(0.0);
    let d = inputs.or("decay", 0.2).max(0.0);
    let s = inputs.or("sustain", 0.7).clamp(0.0, 1.0);
    let h = inputs.or("hold", 0.5).max(0.0);
    let r = inputs.or("release", 0.3).max(0.0);
    let c = inputs.or("curve", 0.5);
    let per = (inputs.samples / 3).max(8);
    let mut pts = vec![(0.0, 0.0)];
    let seg = |pts: &mut Vec<(f64, f64)>, t0: f64, len: f64, from: f64, to: f64| {
        for i in 1..=per {
            let p = i as f64 / per as f64;
            pts.push((t0 + len * p, from + (to - from) * bend(p, c)));
        }
    };
    seg(&mut pts, 0.0, a, 0.0, 1.0);
    seg(&mut pts, a, d, 1.0, s);
    pts.push((a + d + h, s));
    seg(&mut pts, a + d + h, r, s, 0.0);
    // Playheads: voice `i` in stage `v{i}_stage` (0 A, 1 D, 2 S, 3 R; −1
    // idle), `v{i}_pos` of the way through it — on the curve.
    let starts = [(0.0, a), (a, d), (a + d, h), (a + d + h, r)];
    let mut heads: Vec<(f64, f64)> = (0..64)
        .map_while(|i| {
            Some((
                inputs.get(&format!("v{i}_stage"))?,
                inputs.or(&format!("v{i}_pos"), 0.0),
            ))
        })
        .filter(|(stage, _)| *stage >= -0.5)
        .filter_map(|(stage, pos)| {
            let (t0, len) = *starts.get(stage.round() as usize)?;
            let t = t0 + len * pos.clamp(0.0, 1.0);
            Some((t, crate::plot::sample(&pts, t)?))
        })
        .collect();
    heads.sort_by(|p, q| p.0.total_cmp(&q.0));
    vec![
        Series::new("envelope", pts),
        Series::new("playheads", heads),
    ]
}

/// `filter.response`: one filter's magnitude (dB). `kind` 0 low-pass,
/// 1 high-pass, 2 band-pass, 3 notch, 4 bell (`gain`); `slope` dB/oct for
/// the low/high-pass (default 12).
#[must_use]
pub fn filter_response(inputs: &CurveInputs) -> Vec<Series> {
    let sr = inputs.or("sample_rate", 48_000.0);
    let freq = inputs.or("freq", 1000.0);
    let q = inputs.or("q", std::f64::consts::FRAC_1_SQRT_2);
    let shape = match inputs.or("kind", 0.0).round() as i64 {
        1 => Shape::LowCut,
        2 => Shape::BandPass,
        3 => Shape::Notch,
        4 => Shape::Bell,
        _ => Shape::HighCut,
    };
    let sections = band_sections(
        shape,
        freq,
        inputs.or("gain", 0.0),
        q,
        inputs.or("slope", 12.0),
        sr,
    );
    let pts = inputs
        .xs()
        .into_iter()
        .map(|f| (f, sections.iter().map(|s| s.db_at(f, sr)).sum()))
        .collect();
    vec![Series::new("response", pts)]
}

#[cfg(test)]
mod tests {
    use super::*;

    const LOG: Axis = Axis::Log {
        f_min: 20.0,
        f_max: 20_000.0,
    };

    fn at(series: &Series, x: f64) -> f64 {
        series
            .points
            .iter()
            .min_by(|a, b| (a.0 - x).abs().total_cmp(&(b.0 - x).abs()))
            .unwrap()
            .1
    }

    #[test]
    fn a_bell_peaks_at_its_gain_and_is_flat_far_away() {
        let b = Biquad::rbj(Shape::Bell, 1000.0, 6.0, 1.0, 48_000.0);
        assert!((b.db_at(1000.0, 48_000.0) - 6.0).abs() < 1e-6);
        assert!(b.db_at(20.0, 48_000.0).abs() < 0.05);
        let cut = Biquad::rbj(Shape::Bell, 1000.0, -9.0, 4.0, 48_000.0);
        assert!((cut.db_at(1000.0, 48_000.0) + 9.0).abs() < 1e-6);
    }

    #[test]
    fn shelves_reach_their_gain_on_their_side() {
        let lo = Biquad::rbj(Shape::LowShelf, 200.0, 5.0, 0.707, 48_000.0);
        assert!((lo.db_at(20.0, 48_000.0) - 5.0).abs() < 0.2);
        assert!(lo.db_at(10_000.0, 48_000.0).abs() < 0.1);
        let hi = Biquad::rbj(Shape::HighShelf, 5000.0, -4.0, 0.707, 48_000.0);
        assert!((hi.db_at(18_000.0, 48_000.0) + 4.0).abs() < 0.3);
        assert!(hi.db_at(100.0, 48_000.0).abs() < 0.1);
    }

    #[test]
    fn cuts_fall_at_their_slope() {
        for slope in [6.0, 12.0, 18.0, 24.0, 48.0] {
            // Q 1 is Butterworth in Pro-Q's sense (and eq-dsp's).
            let s = eq_band_sections(Shape::LowCut, 1000.0, 0.0, 1.0, slope, 96_000.0);
            let db = |f: f64| s.iter().map(|b| b.db_at(f, 96_000.0)).sum::<f64>();
            // Between one and two octaves down: close to the slope per octave.
            let per_oct = db(500.0) - db(250.0);
            assert!(
                (per_oct - slope).abs() < 1.2,
                "{slope} dB/oct cut falls {per_oct} dB/oct"
            );
            assert!(db(10_000.0).abs() < 0.1, "passband flat");
            // Butterworth: −3 dB at the corner.
            assert!(
                (db(1000.0) + 3.01).abs() < 0.1,
                "{slope}: {} at the corner",
                db(1000.0)
            );
        }
    }

    /// The default Q (0.707) is eq-dsp's `highpass_s2_proq4` at analog
    /// damping √2/Q = 2: critically damped, 6 dB down at the corner — what
    /// signal's rig draws for a 12 dB/oct cut.
    #[test]
    fn a_default_q_cut_is_six_db_down_at_the_corner() {
        let s = eq_band_sections(Shape::LowCut, 80.0, 0.0, 0.707, 12.0, 48_000.0);
        let db = s.iter().map(|b| b.db_at(80.0, 48_000.0)).sum::<f64>();
        assert!((db + 6.0).abs() < 0.1, "{db} at the corner");
    }

    #[test]
    fn a_notch_is_deep_at_its_frequency() {
        let n = Biquad::rbj(Shape::Notch, 3000.0, 0.0, 2.0, 48_000.0);
        assert!(n.db_at(3000.0, 48_000.0) < -60.0);
    }

    #[test]
    fn eq_response_sums_its_bands() {
        let inputs = CurveInputs::new(LOG)
            .with("b1_freq", 100.0)
            .with("b1_gain", 4.0)
            .with("b1_q", 1.0)
            .with("b2_freq", 4000.0)
            .with("b2_gain", -6.0)
            .with("b2_q", 2.0)
            .with("b3_freq", 8000.0)
            .with("b3_gain", 12.0)
            .with("b3_on", 0.0);
        let s = eq_response(&inputs);
        assert_eq!(
            s.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(),
            ["sum", "b1", "b2", "b3"]
        );
        assert!(s[3].points.is_empty(), "band 3 is off");
        for i in 0..s[0].points.len() {
            let sum = s[1].points[i].1 + s[2].points[i].1;
            assert!((s[0].points[i].1 - sum).abs() < 1e-9);
        }
        assert!((at(&s[0], 100.0) - 4.0).abs() < 0.2);
        assert!((at(&s[0], 4000.0) + 6.0).abs() < 0.2);
        assert_eq!(s[0].points.len(), 256);
        assert!(
            (s[0].points[0].0 - 20.0).abs() < 1e-9 && (s[0].points[255].0 - 20_000.0).abs() < 1e-6
        );
    }

    #[test]
    fn unused_bands_stems_slopes_and_dynamics() {
        let inputs = CurveInputs::new(LOG)
            .with("stems", 1.0)
            .with("b1_freq", 1000.0)
            .with("b1_gain", 6.0)
            .with("b1_dyn_range", -4.0)
            .with("b2_freq", 100.0)
            .with("b2_shape", 3.0)
            .with("b2_slope_index", 4.0)
            .with("b3_freq", 5000.0)
            .with("b3_gain", 9.0)
            .with("b3_used", 0.0);
        let s = eq_response(&inputs);
        let names: Vec<&str> = s.iter().map(|s| s.name.as_str()).collect();
        assert_eq!(
            names,
            [
                "sum", "b1", "b2", "b3", "b1_stem", "b1_dyn", "b2_stem", "b3_stem"
            ]
        );
        assert!(s[3].points.is_empty(), "an unused band draws nothing");
        assert_eq!(s[4].points, [(1000.0, 0.0), (1000.0, s[4].points[1].1)]);
        assert!(
            (s[4].points[1].1 - 6.0).abs() < 1e-6,
            "the stem reaches the node"
        );
        assert!((at(&s[5], 1000.0) - 2.0).abs() < 0.2, "gain + range = 2 dB");
        let per_oct = at(&s[2], 25.0) - at(&s[2], 12.5).max(-200.0);
        assert!(
            at(&s[2], 50.0) < -6.0 && per_oct.is_finite(),
            "a 24 dB/oct low cut"
        );
        assert_eq!(slope_db(4.0), 24.0);
        assert_eq!(Shape::from_index(9.0), Shape::AllPass);
        let ap = band_sections(Shape::AllPass, 1000.0, 0.0, 0.7, 12.0, 48_000.0);
        assert!(
            ap[0].db_at(300.0, 48_000.0).abs() < 1e-6,
            "all-pass is flat"
        );
        let tilt = band_sections(Shape::TiltShelf, 1000.0, 6.0, 0.7, 12.0, 48_000.0);
        let db = |f: f64| tilt.iter().map(|b| b.db_at(f, 48_000.0)).sum::<f64>();
        assert!(
            db(20.0) < -2.5 && db(18_000.0) > 2.5,
            "tilts down below, up above"
        );
    }

    #[test]
    fn transfer_is_unity_below_threshold_and_ratio_above() {
        let inputs = CurveInputs::new(Axis::Linear {
            min: -60.0,
            max: 0.0,
        })
        .with("threshold", -20.0)
        .with("ratio", 4.0)
        .with("knee", 0.0);
        let s = comp_transfer(&inputs);
        assert!((at(&s[0], -40.0) + 40.0).abs() < 0.3);
        // 20 dB over at 4:1 → 5 dB over.
        assert!((comp_gain(0.0, -20.0, 4.0, 0.0, 0.0) + 15.0).abs() < 1e-9);
        // The soft knee is continuous and meets both lines at its edges.
        for x in [-23.0, -20.0, -17.0] {
            let hard = comp_gain(x, -20.0, 4.0, 0.0, 0.0);
            let soft = comp_gain(x, -20.0, 4.0, 6.0, 0.0);
            assert!(
                soft <= hard + 1e-9 && hard <= x + 1e-9,
                "{x}: soft {soft}, hard {hard}"
            );
        }
        assert!((comp_gain(-23.0, -20.0, 4.0, 6.0, 0.0) + 23.0).abs() < 1e-9);
        assert!(
            (comp_gain(-17.0, -20.0, 4.0, 6.0, 0.0) - comp_gain(-17.0, -20.0, 4.0, 0.0, 0.0)).abs()
                < 1e-9
        );
        assert!(
            (comp_gain(-40.0, -20.0, 4.0, 6.0, 3.0) + 37.0).abs() < 1e-9,
            "makeup"
        );
    }

    #[test]
    fn adsr_passes_through_its_breakpoints() {
        let inputs = CurveInputs::new(Axis::Linear { min: 0.0, max: 4.0 })
            .with("attack", 0.1)
            .with("decay", 0.4)
            .with("sustain", 0.6)
            .with("release", 1.0)
            .with("hold", 0.5)
            .with("curve", 0.0);
        let p = &adsr_envelope(&inputs)[0].points;
        let find = |t: f64| p.iter().find(|q| (q.0 - t).abs() < 1e-9).map(|q| q.1);
        assert_eq!(p[0], (0.0, 0.0));
        assert!((find(0.1).unwrap() - 1.0).abs() < 1e-9);
        assert!((find(0.5).unwrap() - 0.6).abs() < 1e-9);
        assert!((find(1.0).unwrap() - 0.6).abs() < 1e-9);
        assert!(find(2.0).unwrap().abs() < 1e-9);
        assert!(p.windows(2).all(|w| w[1].0 >= w[0].0), "time runs forward");
        // Bent segments move fast first.
        assert!(
            bend(0.25, 0.5) > 0.25
                && bend(0.25, -0.5) < 0.25
                && (bend(0.25, 0.0) - 0.25).abs() < 1e-12
        );
    }

    #[test]
    fn filter_response_is_minus_3_db_at_a_butterworth_corner() {
        let inputs = CurveInputs::new(LOG).with("freq", 1000.0).with("kind", 0.0);
        let s = filter_response(&inputs);
        assert!((at(&s[0], 1000.0) + 3.0).abs() < 0.3);
        assert!(at(&s[0], 100.0).abs() < 0.1);
    }

    #[test]
    fn engines_override_builtins() {
        let mut r = CurveRegistry::builtin();
        assert!(r.contains("eq.response"));
        r.register("eq.response", |_| {
            vec![Series::new("sum", vec![(20.0, 1.0)])]
        });
        assert_eq!(
            r.eval("eq.response", &CurveInputs::new(LOG)).unwrap()[0].points,
            [(20.0, 1.0)]
        );
        assert!(r.eval("nope", &CurveInputs::new(LOG)).is_none());
        for (name, _) in surface_proto::BUILTIN_CURVES {
            assert!(CurveRegistry::builtin().contains(name), "{name}");
        }
    }
}
