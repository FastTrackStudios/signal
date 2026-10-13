//! What each unit binds: `<unit>/<param>` params and `<unit>/<feed>` feeds.
//!
//! An engine (or session's `Tone` store, through an adapter) publishes
//! these; the panels bind nothing else. Values are engineering units, as
//! session keeps them: dB, Hz, ms, seconds, ratio, 0..1 shares.
//!
//! | unit | params | feeds |
//! |---|---|---|
//! | EQs (`eq`, `rescue_eq`, `polish_eq`, `space`) | `b{n}_{freq,gain,q,shape,on}`, `range` (zoom stop), `bypass` | `spectrum` |
//! | compressors (`comp`, `rescue_comp`) | `threshold`, `ratio`, `knee`, `makeup`, `attack`, `release`, `bypass` | `in_history`, `gr_history`, `gr` |
//! | `gate` | `threshold`, `range`, `attack`, `hold`, `release`, `bypass` | `in_history`, `open_history`, `in` |
//! | `sat` | `drive`, `bias`, `tilt`, `circuit`, `bits`, `bypass` | `peak` (0..1 linear) |
//! | `deess` | `low`, `high`, `threshold`, `depth`, `bypass` | `spectrum`, `level_history`, `gr_history`, `gr` |
//! | `delay` | `time`, `feedback`, `mix`, `family`, `bypass` | `wet` |
//! | `reverb` | `decay`, `predelay`, `mix`, `family`, `bypass` | `wet` |
//! | the rack | `rack/fold_<phase>` | — |

use surface_kit::viz::catalog as viz;
use surface_proto::{Catalog, FeedInfo, FeedKind, ParamInfo, Range, ScalarKind, Unit};

use super::{Phase, Which};
use crate::curves::{CIRCUITS, DELAY_FAMILIES, ROOM_FAMILIES};

/// The EQ graph's zoom stops, ± dB (the plugin's `DB_RANGE_STEPS`).
pub const EQ_RANGES: [f64; 5] = [3.0, 6.0, 12.0, 18.0, 30.0];
/// Where a rack opens (session `DEFAULT_EQ_RANGE`).
pub const DEFAULT_EQ_RANGE: usize = 2;
/// How many readings a history keeps (session: 72 at 30 Hz).
pub const HISTORY: u32 = 72;

fn p(ns: &str, k: &str, name: &str, range: Range, v: f64) -> ParamInfo {
    ParamInfo::new(format!("{ns}/{k}"), name, range)
        .with_default_real(v)
        .short(name)
}

fn choice(ns: &str, k: &str, name: &str, options: &[&str], at: usize) -> ParamInfo {
    let mut c = ParamInfo::new(format!("{ns}/{k}"), name, Range::default())
        .choice(options)
        .short(name);
    c.default = at as f64 / (options.len().max(2) - 1) as f64;
    c
}

fn toggle(ns: &str, k: &str, name: &str, on: bool) -> ParamInfo {
    let mut t = ParamInfo::new(format!("{ns}/{k}"), name, Range::default())
        .toggle()
        .short(name);
    t.default = if on { 1.0 } else { 0.0 };
    t
}

/// A unit's params.
#[must_use]
pub fn params(which: Which, bands: usize) -> Vec<ParamInfo> {
    let ns = which.ns();
    let mut out = match which {
        Which::RescueEq | Which::Eq | Which::PolishEq | Which::Space => {
            let mut v = viz::eq_params(ns, bands);
            let names: Vec<String> = EQ_RANGES.iter().map(|r| format!("±{r:.0}")).collect();
            let names: Vec<&str> = names.iter().map(String::as_str).collect();
            v.push(choice(ns, "range", "Range", &names, DEFAULT_EQ_RANGE));
            v
        }
        Which::RescueComp | Which::Comp => vec![
            p(
                ns,
                "threshold",
                "Threshold",
                Range::linear(-60.0, 0.0, Unit::Db),
                -18.0,
            )
            .featured(),
            p(
                ns,
                "ratio",
                "Ratio",
                Range::log(1.0, 20.0, Unit::Ratio),
                4.0,
            )
            .featured(),
            p(ns, "knee", "Knee", Range::linear(0.0, 24.0, Unit::Db), 6.0),
            p(
                ns,
                "makeup",
                "Makeup",
                Range::linear(0.0, 24.0, Unit::Db),
                0.0,
            ),
            p(
                ns,
                "attack",
                "Attack",
                Range::log(0.1, 200.0, Unit::Milliseconds),
                10.0,
            ),
            p(
                ns,
                "release",
                "Release",
                Range::log(5.0, 3000.0, Unit::Milliseconds),
                150.0,
            ),
        ],
        Which::Gate => vec![
            p(
                ns,
                "threshold",
                "Threshold",
                Range::linear(-60.0, 0.0, Unit::Db),
                -40.0,
            )
            .featured(),
            p(
                ns,
                "range",
                "Range",
                Range::linear(-60.0, 0.0, Unit::Db),
                -18.0,
            )
            .featured(),
            p(
                ns,
                "attack",
                "Attack",
                Range::log(0.05, 50.0, Unit::Milliseconds),
                1.0,
            ),
            p(
                ns,
                "hold",
                "Hold",
                Range::log(1.0, 500.0, Unit::Milliseconds),
                30.0,
            ),
            p(
                ns,
                "release",
                "Release",
                Range::log(5.0, 2000.0, Unit::Milliseconds),
                120.0,
            ),
        ],
        Which::Sat => vec![
            p(ns, "drive", "Drive", Range::log(0.5, 12.0, Unit::None), 2.0).featured(),
            p(
                ns,
                "bias",
                "Bias",
                Range::linear(-0.6, 0.6, Unit::None),
                0.15,
            )
            .bipolar(),
            p(
                ns,
                "tilt",
                "Tilt",
                Range::linear(-12.0, 12.0, Unit::Db),
                0.0,
            )
            .bipolar(),
            choice(ns, "circuit", "Circuit", &CIRCUITS, 0),
            p(
                ns,
                "bits",
                "Bits",
                Range::linear(0.0, 16.0, Unit::None),
                0.0,
            ),
            p(
                ns,
                "mix",
                "Mix",
                Range::linear(0.0, 1.0, Unit::Percent),
                1.0,
            ),
        ],
        Which::DeEss => vec![
            p(
                ns,
                "low",
                "Low",
                Range::log(800.0, 20_000.0, Unit::Hz),
                5000.0,
            ),
            p(
                ns,
                "high",
                "High",
                Range::log(800.0, 20_000.0, Unit::Hz),
                10_000.0,
            ),
            p(
                ns,
                "threshold",
                "Threshold",
                Range::linear(-40.0, 0.0, Unit::Db),
                -24.0,
            )
            .featured(),
            p(
                ns,
                "depth",
                "Depth",
                Range::linear(0.0, 24.0, Unit::Db),
                6.0,
            )
            .featured(),
        ],
        Which::Delay => vec![
            p(
                ns,
                "time",
                "Time",
                Range::log(10.0, 2000.0, Unit::Milliseconds),
                300.0,
            )
            .featured(),
            p(
                ns,
                "feedback",
                "Feedback",
                Range::linear(0.0, 0.99, Unit::Percent),
                0.38,
            )
            .featured(),
            p(
                ns,
                "mix",
                "Mix",
                Range::linear(0.0, 1.0, Unit::Percent),
                0.3,
            ),
            choice(ns, "family", "Family", &DELAY_FAMILIES, 0),
        ],
        Which::Reverb => vec![
            p(
                ns,
                "decay",
                "Decay",
                Range::log(0.1, 20.0, Unit::Seconds),
                1.8,
            )
            .featured(),
            p(
                ns,
                "predelay",
                "Predelay",
                Range::linear(0.0, 250.0, Unit::Milliseconds),
                20.0,
            ),
            p(
                ns,
                "mix",
                "Mix",
                Range::linear(0.0, 1.0, Unit::Percent),
                0.18,
            )
            .featured(),
            choice(ns, "family", "Family", &ROOM_FAMILIES, 1),
        ],
    };
    out.push(toggle(ns, "bypass", "Bypass", false));
    for q in &mut out {
        q.group = Some(which.name().to_string());
    }
    out
}

/// A unit's feeds.
#[must_use]
pub fn feeds(which: Which) -> Vec<FeedInfo> {
    let ns = which.ns();
    let f = |k: &str, name: &str, kind: FeedKind| FeedInfo::new(format!("{ns}/{k}"), name, kind);
    let history = |k: &str, name: &str, of: ScalarKind| {
        f(k, name, FeedKind::History { of, len: HISTORY }).rate(30.0)
    };
    match which {
        Which::RescueEq | Which::Eq | Which::PolishEq | Which::Space => {
            vec![viz::spectrum_stream(ns, 96)]
        }
        Which::RescueComp | Which::Comp => vec![
            history("in_history", "Input History", ScalarKind::Level),
            f("gr_history", "GR History", FeedKind::gr_history(HISTORY)).rate(30.0),
            f("gr", "Gain Reduction", FeedKind::GainReduction),
        ],
        Which::Gate => vec![
            history("in_history", "Input History", ScalarKind::Level),
            history(
                "open_history",
                "Open History",
                ScalarKind::Scalar {
                    range: Range::linear(0.0, 1.0, Unit::None),
                },
            ),
            f("in", "Input", FeedKind::Level { channels: 1 }),
        ],
        Which::Sat => vec![f(
            "peak",
            "Peak",
            FeedKind::Scalar {
                range: Range::linear(0.0, 1.0, Unit::None),
            },
        )],
        Which::DeEss => vec![
            FeedInfo::new(format!("{ns}/spectrum"), "Spectrum", FeedKind::spectrum(96)),
            history("level_history", "Band Level", ScalarKind::Level),
            f("gr_history", "GR History", FeedKind::gr_history(HISTORY)).rate(30.0),
            f("gr", "Gain Reduction", FeedKind::GainReduction),
        ],
        Which::Delay | Which::Reverb => vec![f(
            "wet",
            "Wet",
            FeedKind::Scalar {
                range: Range::linear(0.0, 1.0, Unit::None),
            },
        )],
    }
}

/// The params a unit's *layout* depends on (a rebuild when they move).
#[must_use]
pub fn layout_params(which: Which) -> Vec<String> {
    let ns = which.ns();
    match which {
        w if w.is_eq() => vec![format!("{ns}/range")],
        Which::Sat => vec![format!("{ns}/circuit")],
        Which::Delay | Which::Reverb => vec![format!("{ns}/family")],
        _ => Vec::new(),
    }
}

/// The catalog of a rack of `units`.
#[must_use]
pub fn rack(units: &[Which], bands: usize) -> Catalog {
    let mut c = Catalog {
        id: "frame.tone".into(),
        version: 1,
        ..Catalog::default()
    };
    for u in units {
        c.params.extend(params(*u, bands));
        c.streams.extend(feeds(*u));
    }
    for phase in Phase::ALL {
        if units.iter().any(|u| u.phase() == phase) {
            // Where a mixer opens (session `Folded::rest`): Rescue is dialled
            // in once and then done, so it rests shut.
            c.params.push(toggle(
                "rack",
                &format!("fold_{}", phase.name().to_lowercase()),
                &format!("Fold {}", phase.name()),
                phase == Phase::Rescue,
            ));
        }
    }
    c
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_address_is_namespaced_and_unique() {
        let c = rack(&super::super::chains::CHANNEL, 4);
        let mut seen = std::collections::HashSet::new();
        for p in &c.params {
            assert!(seen.insert(p.addr.0.clone()), "duplicate {}", p.addr.0);
            assert!(p.addr.0.contains('/'), "{}", p.addr.0);
        }
        assert!(
            c.param(&surface_proto::ParamAddr::new("comp/threshold"))
                .is_some()
        );
        assert!(
            c.stream(&surface_proto::EntryId::new("comp/gr_history"))
                .is_some()
        );
    }
}
