//! The params and feeds the lit visualizers ([`super::lit`]) bind to, by
//! namespace — for engines to publish and mock catalogs to serve. The
//! names are signal's rig params where it has them (`time`, `feedback`,
//! `tap_div_l`, `decay`, `predelay`, `rate`, `depth`, `mix`, …).

use crate::surface::proto::{FeedInfo, ParamInfo, Range, Unit};
/// Tap divisions, as `delay.taps` reads them (option index; `Free` = the
/// delay's own time).
pub const DIVISIONS: [&str; 11] = [
    "1/4", "1/8.", "1/8", "1/8T", "1/16", "Golden", "Silver", "Free", "1/4.", "1/2", "1/4T",
];
/// Delay machines, in `viz.delay`'s order.
pub const DELAY_FAMILIES: [&str; 6] = ["Digital", "Tape", "Analog", "Pitch", "Rhythmic", "Special"];
/// Reverb spaces, in `viz.reverb`'s order.
pub const REVERB_FAMILIES: [&str; 8] = [
    "Room",
    "Hall",
    "Plate",
    "Spring",
    "Ambient",
    "Random",
    "Special",
    "Convolution",
];
/// Modulation engines, in `viz.modulation`'s order.
pub const MOD_ENGINES: [&str; 6] = [
    "Chorus", "Flanger", "Phaser", "Tremolo", "Vibrato", "Rotary",
];

fn choice(
    ns: &str,
    key: &str,
    name: &str,
    group: &str,
    options: &[&str],
    default: usize,
) -> ParamInfo {
    let mut p = ParamInfo::new(format!("{ns}/{key}"), name, Range::default())
        .choice(options)
        .in_group(group)
        .short(name);
    p.default = default as f64 / (options.len().max(2) - 1) as f64;
    p
}

fn toggle(ns: &str, key: &str, name: &str, group: &str) -> ParamInfo {
    let mut p = ParamInfo::new(format!("{ns}/{key}"), name, Range::default())
        .toggle()
        .in_group(group)
        .short(name);
    p.default = 1.0;
    p
}

fn real(ns: &str, key: &str, name: &str, group: &str, range: Range, v: f64) -> ParamInfo {
    ParamInfo::new(format!("{ns}/{key}"), name, range)
        .in_group(group)
        .with_default_real(v)
        .short(name)
}

fn bpm(ns: &str, group: &str) -> ParamInfo {
    real(
        ns,
        "bpm",
        "Tempo",
        group,
        Range::linear(40.0, 240.0, Unit::Bpm),
        120.0,
    )
}

/// `viz.comp_scope`: [`super::catalog::comp_params`] and
/// [`super::catalog::comp_streams`] (`len`-point histories), plus
/// `<ns>/on`.
#[must_use]
pub fn comp_scope(ns: &str, len: u32) -> (Vec<ParamInfo>, Vec<FeedInfo>) {
    let mut params = super::catalog::comp_params(ns);
    params.push(toggle(ns, "on", "On", "Compressor"));
    (params, super::catalog::comp_streams(ns, len))
}

/// `viz.delay_taps`: `<ns>/{time,feedback,mix,tap_div_l,tap_div_r,family,bpm,on}`.
#[must_use]
pub fn delay_params(ns: &str) -> Vec<ParamInfo> {
    let g = "Delay";
    vec![
        real(
            ns,
            "time",
            "Time",
            g,
            Range::log(10.0, 2000.0, Unit::Milliseconds),
            350.0,
        )
        .featured(),
        real(
            ns,
            "feedback",
            "Feedback",
            g,
            Range::linear(0.0, 0.98, Unit::None),
            0.55,
        )
        .featured(),
        real(
            ns,
            "mix",
            "Mix",
            g,
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        choice(ns, "tap_div_l", "Left", g, &DIVISIONS, 0),
        choice(ns, "tap_div_r", "Right", g, &DIVISIONS, 1),
        choice(ns, "family", "Machine", g, &DELAY_FAMILIES, 1),
        bpm(ns, g),
        toggle(ns, "on", "On", g),
    ]
}

/// `viz.reverb_space`: `<ns>/{decay,predelay,density,damp,mix,family,bpm,on}`.
#[must_use]
pub fn reverb_params(ns: &str) -> Vec<ParamInfo> {
    let g = "Reverb";
    vec![
        real(
            ns,
            "decay",
            "Decay",
            g,
            Range::log(0.1, 20.0, Unit::Seconds),
            2.6,
        )
        .featured(),
        real(
            ns,
            "predelay",
            "Pre-delay",
            g,
            Range::linear(0.0, 250.0, Unit::Milliseconds),
            35.0,
        ),
        real(
            ns,
            "density",
            "Density",
            g,
            Range::linear(0.0, 1.0, Unit::None),
            0.55,
        ),
        real(
            ns,
            "damp",
            "Damping",
            g,
            Range::linear(0.0, 1.0, Unit::None),
            0.35,
        ),
        real(
            ns,
            "mix",
            "Mix",
            g,
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        choice(ns, "family", "Space", g, &REVERB_FAMILIES, 1),
        bpm(ns, g),
        toggle(ns, "on", "On", g),
    ]
}

/// `viz.mod_lfo`: `<ns>/{rate,depth,mix,engine,on}`.
#[must_use]
pub fn mod_params(ns: &str) -> Vec<ParamInfo> {
    let g = "Modulation";
    vec![
        real(ns, "rate", "Rate", g, Range::log(0.05, 10.0, Unit::Hz), 0.9).featured(),
        real(
            ns,
            "depth",
            "Depth",
            g,
            Range::linear(0.0, 1.0, Unit::None),
            0.75,
        )
        .featured(),
        real(
            ns,
            "mix",
            "Mix",
            g,
            Range::linear(0.0, 1.0, Unit::None),
            0.5,
        ),
        choice(ns, "engine", "Engine", g, &MOD_ENGINES, 0),
        toggle(ns, "on", "On", g),
    ]
}

/// `viz.eq_glow`: [`super::catalog::eq_params`] and the
/// [`super::catalog::spectrum_stream`].
#[must_use]
pub fn eq_glow(ns: &str, bands: usize, bins: u16) -> (Vec<ParamInfo>, Vec<FeedInfo>) {
    (
        super::catalog::eq_params(ns, bands),
        vec![super::catalog::spectrum_stream(ns, bins)],
    )
}
