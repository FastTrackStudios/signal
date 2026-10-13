//! The params and feeds the visualizers bind to, by namespace — for
//! engines to publish (or mock engines to serve) exactly what a placed
//! `viz.*` component expects.

use surface_proto::{FeedInfo, FeedKind, ParamInfo, Range, ScalarKind, Unit};

/// EQ band shapes, in the option order `eq.response` reads (and signal's
/// `b{n}_shape` wire index).
pub const SHAPES: [&str; 10] = [
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

/// Cut slopes, by `b{n}_slope` index (signal's wire encoding).
pub const SLOPES: [&str; 11] = [
    "0 dB/oct",
    "6 dB/oct",
    "12 dB/oct",
    "18 dB/oct",
    "24 dB/oct",
    "30 dB/oct",
    "36 dB/oct",
    "48 dB/oct",
    "72 dB/oct",
    "96 dB/oct",
    "Brickwall",
];

/// Stereo placements, by `b{n}_placement` index.
pub const PLACEMENTS: [&str; 5] = ["ST", "L", "R", "M", "S"];

/// The display ranges an EQ graph zooms between (± dB), by `<ns>/range`.
pub const RANGES: [f64; 6] = [3.0, 6.0, 12.0, 18.0, 24.0, 30.0];

/// A starting point for the first bands: (freq Hz, gain dB, Q, shape index).
/// Further bands start unused.
const BAND_DEFAULTS: [(f64, f64, f64, usize); 5] = [
    (40.0, 0.0, 0.71, 3),
    (120.0, 3.0, 0.71, 1),
    (420.0, -4.0, 1.4, 0),
    (2400.0, 2.5, 1.0, 0),
    (9000.0, 2.5, 0.71, 2),
];

/// Bands 1..=bands of an EQ, signal's per-band wire family:
/// `<ns>/b{n}_{used,on,freq,gain,q,shape,slope,name,placement,focus,dyn_range}`.
/// The first bands (up to five) start in use.
#[must_use]
pub fn eq_params(ns: &str, bands: usize) -> Vec<ParamInfo> {
    let mut out = Vec::new();
    for n in 1..=bands {
        let used = n <= BAND_DEFAULTS.len().min(bands);
        let (f, g, q, shape) = BAND_DEFAULTS
            .get(n - 1)
            .copied()
            .filter(|_| used)
            .unwrap_or((1000.0, 0.0, 0.707, 0));
        let group = format!("Band {n}");
        let p = |k: &str, name: &str, range: Range| {
            ParamInfo::new(format!("{ns}/b{n}_{k}"), format!("{group} {name}"), range)
                .in_group(group.clone())
        };
        let flag = |k: &str, name: &str, on: bool| {
            let mut t = p(k, name, Range::default()).toggle().short(name);
            t.default = if on { 1.0 } else { 0.0 };
            t
        };
        let choice = |k: &str, name: &str, options: &[&str], at: usize| {
            let mut c = p(k, name, Range::default()).choice(options).short(name);
            c.default = at as f64 / (options.len() - 1) as f64;
            c
        };
        out.push(flag("used", "Used", used));
        out.push(flag("on", "On", true));
        out.push(
            p("freq", "Freq", Range::log(10.0, 30_000.0, Unit::Hz))
                .with_default_real(f)
                .short("Freq"),
        );
        out.push(
            p("gain", "Gain", Range::linear(-30.0, 30.0, Unit::Db))
                .with_default_real(g)
                .short("Gain")
                .bipolar(),
        );
        out.push(
            p("q", "Q", Range::log(0.025, 40.0, Unit::None))
                .with_default_real(q)
                .short("Q"),
        );
        out.push(choice("shape", "Shape", &SHAPES, shape));
        out.push(choice("slope", "Slope", &SLOPES, 2));
        out.push(p("name", "Name", Range::default()).text().short("Name"));
        out.push(choice("placement", "Placement", &PLACEMENTS, 0));
        out.push(flag("focus", "Focus", false));
        out.push(
            p(
                "dyn_range",
                "Dyn Range",
                Range::linear(-30.0, 30.0, Unit::Db),
            )
            .with_default_real(0.0)
            .short("Dyn")
            .bipolar(),
        );
    }
    out
}

/// An EQ graph's own params: `<ns>/range` (the display range, ±12 dB to
/// start) and the analyser layers `<ns>/show_{pre,post,ext}`.
#[must_use]
pub fn eq_view_params(ns: &str) -> Vec<ParamInfo> {
    let labels: Vec<String> = RANGES.iter().map(|r| format!("±{r:.0} dB")).collect();
    let labels: Vec<&str> = labels.iter().map(String::as_str).collect();
    let mut range = ParamInfo::new(format!("{ns}/range"), "Display Range", Range::default())
        .choice(&labels)
        .in_group("View")
        .short("Range");
    range.default = 2.0 / (RANGES.len() - 1) as f64;
    let show = |k: &str, name: &str, on: bool| {
        let mut t = ParamInfo::new(format!("{ns}/show_{k}"), name, Range::default())
            .toggle()
            .in_group("View")
            .short(k.to_uppercase());
        t.default = if on { 1.0 } else { 0.0 };
        t
    };
    vec![
        range,
        show("pre", "Pre-EQ Analyser", true),
        show("post", "Post-EQ Analyser", true),
        show("ext", "External Analyser", false),
    ]
}

/// An EQ graph's analyser feeds: `<ns>/spectrum` (post-EQ),
/// `<ns>/spectrum_pre` and `<ns>/sidechain`.
#[must_use]
pub fn eq_streams(ns: &str, bins: u16) -> Vec<FeedInfo> {
    vec![
        spectrum_stream(ns, bins),
        FeedInfo::new(
            format!("{ns}/spectrum_pre"),
            "Spectrum (pre)",
            FeedKind::spectrum(bins),
        ),
        FeedInfo::new(
            format!("{ns}/sidechain"),
            "Sidechain Spectrum",
            FeedKind::spectrum(bins),
        ),
    ]
}

/// `<ns>/spectrum`: `bins` log-spaced bins in dB over 20 Hz–20 kHz.
#[must_use]
pub fn spectrum_stream(ns: &str, bins: u16) -> FeedInfo {
    FeedInfo::new(
        format!("{ns}/spectrum"),
        "Spectrum",
        FeedKind::spectrum(bins),
    )
}

/// `<ns>/{threshold,ratio,knee,makeup}`.
#[must_use]
pub fn comp_params(ns: &str) -> Vec<ParamInfo> {
    let p = |k: &str, name: &str, range: Range, v: f64| {
        ParamInfo::new(format!("{ns}/{k}"), name, range)
            .in_group("Compressor")
            .with_default_real(v)
    };
    vec![
        p(
            "threshold",
            "Threshold",
            Range::linear(-60.0, 0.0, Unit::Db),
            -24.0,
        )
        .featured(),
        p("ratio", "Ratio", Range::log(1.0, 20.0, Unit::Ratio), 4.0).featured(),
        p("knee", "Knee", Range::linear(0.0, 24.0, Unit::Db), 6.0),
        p("makeup", "Makeup", Range::linear(0.0, 24.0, Unit::Db), 0.0),
    ]
}

/// A compressor's feeds: `<ns>/in` (input level) and `<ns>/gr` (gain
/// reduction) as readings, and `<ns>/in_history` / `<ns>/gr_history`, the
/// last `len` of each.
#[must_use]
pub fn comp_streams(ns: &str, len: u32) -> Vec<FeedInfo> {
    vec![
        FeedInfo::new(format!("{ns}/in"), "Input", FeedKind::Level { channels: 1 }),
        FeedInfo::new(
            format!("{ns}/gr"),
            "Gain Reduction",
            FeedKind::GainReduction,
        ),
        FeedInfo::new(
            format!("{ns}/in_history"),
            "Input History",
            FeedKind::History {
                of: ScalarKind::Level,
                len,
            },
        )
        .rate(60.0),
        FeedInfo::new(
            format!("{ns}/gr_history"),
            "GR History",
            FeedKind::gr_history(len),
        )
        .rate(60.0),
    ]
}

/// `<ns>/{attack,decay,sustain,release}`: seconds, and a 0..1 level.
#[must_use]
pub fn adsr_params(ns: &str) -> Vec<ParamInfo> {
    let p = |k: &str, name: &str, range: Range, v: f64| {
        ParamInfo::new(format!("{ns}/{k}"), name, range)
            .in_group("Envelope")
            .with_default_real(v)
    };
    vec![
        p(
            "attack",
            "Attack",
            Range::log(0.001, 2.0, Unit::Seconds),
            0.08,
        ),
        p(
            "decay",
            "Decay",
            Range::log(0.005, 4.0, Unit::Seconds),
            0.45,
        ),
        p(
            "sustain",
            "Sustain",
            Range::linear(0.0, 1.0, Unit::None),
            0.6,
        ),
        p(
            "release",
            "Release",
            Range::log(0.005, 6.0, Unit::Seconds),
            1.1,
        ),
    ]
}

/// `<ns>/voices`: an ADSR playhead per voice (`[stage, position, level]`).
#[must_use]
pub fn adsr_stream(ns: &str, voices: u8) -> FeedInfo {
    FeedInfo::new(format!("{ns}/voices"), "Voices", FeedKind::adsr(voices)).rate(60.0)
}
