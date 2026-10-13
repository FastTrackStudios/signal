//! The strip's Tone rack bound to the FTS processors on the track.
//!
//! The rack edits settings the window owns ([`crate::tone::Store`]); this
//! is what turns them into the track's chain — FTS EQ, Comp and Saturate,
//! put on bypassed by [`crate::mix_chains`] — as the plugins' own
//! parameters, each written in the plugin's own words (`-18.0 dB`, `3.0:1`,
//! `Low Shelf`, `Triode`) and resolved by the plugin
//! ([`builtin_fx::setting`]), so no plugin's ranges are assumed here.
//!
//! - EQ: every band the plugin has — the rack's bands, on or off as they
//!   are, and the rest off (the plugin's own defaults have bands on).
//! - Comp: threshold, ratio, knee, attack, release.
//! - Saturate: the profile (`Space`), and the knobs the rack's stage was
//!   made from by the plugin's own `apply` — drive and bias, inverted from
//!   it — and the mix.

use crate::tone::Tone;

/// One FX's wanted parameters: its name, and `(index, normalized value)`.
pub type Stage = (&'static str, Vec<(u32, f64)>);

/// The plugin's name for a band type.
const fn shape(shape: eq_ui::eq_graph_model::EqBandShape) -> &'static str {
    use eq_ui::eq_graph_model::EqBandShape as S;
    match shape {
        S::Bell => "Bell",
        S::LowShelf => "Low Shelf",
        S::HighShelf => "High Shelf",
        S::LowCut => "Low Cut",
        S::HighCut => "High Cut",
        S::Notch => "Notch",
        S::BandPass => "Bandpass",
        S::TiltShelf => "Tilt Shelf",
        S::FlatTilt => "Flat Tilt",
        S::AllPass => "All Pass",
    }
}

/// What `tone` asks of the track's processors. A processor the track does
/// not have takes nothing (`Edit::SetFxParam` finds no FX by its name).
#[must_use]
pub fn stages(tone: &Tone) -> Vec<Stage> {
    vec![
        ("FTS EQ", eq(tone)),
        ("FTS Comp", comp(tone)),
        ("FTS Saturate", sat(tone)),
        ("FTS Gate", gate(tone)),
        ("FTS Reverb", reverb(tone)),
        ("FTS Delay", delay(tone)),
        ("FTS Modulation", modulation(tone)),
    ]
}

/// `(index, value)` for each of `fx`'s `(param, text)` the plugin knows.
fn all(fx: &str, settings: &[(&str, String)]) -> Vec<(u32, f64)> {
    settings
        .iter()
        .filter_map(|(param, text)| at(fx, param, text))
        .collect()
}

fn gate(tone: &Tone) -> Vec<(u32, f64)> {
    let g = tone.gate;
    all(
        "FTS Gate",
        &[
            ("Threshold", format!("{:.1} dB", g.threshold)),
            ("Range", format!("{:.1} dB", g.range)),
            ("Attack", format!("{:.2} ms", g.attack)),
            ("Hold", format!("{:.1} ms", g.hold)),
            ("Release", format!("{:.1} ms", g.release)),
        ],
    )
}

/// A frequency as the plugins write one.
fn hz_text(hz: f32) -> String {
    if hz >= 1_000.0 {
        format!("{:.1} kHz", hz / 1_000.0)
    } else {
        format!("{hz:.1} Hz")
    }
}

/// The plugin's note value for a length in quarter notes, if it has one.
fn note(quarters: f64) -> Option<&'static str> {
    [
        (4.0, "1/1"),
        (3.0, "1/2D"),
        (2.0, "1/2"),
        (1.5, "1/4D"),
        (1.0, "1/4"),
        (2.0 / 3.0, "1/4T"),
        (0.75, "1/8D"),
        (0.5, "1/8"),
        (1.0 / 3.0, "1/8T"),
        (0.25, "1/16"),
    ]
    .iter()
    .find(|(q, _)| (q - quarters).abs() < 1e-6)
    .map(|(_, label)| *label)
}

fn reverb(tone: &Tone) -> Vec<(u32, f64)> {
    let room = &tone.reverb;
    let space = reverb_profiles::PROFILES
        .iter()
        .find(|p| p.algorithm == room.algorithm)
        .map_or("Concert Hall", |p| p.name);
    // Its time as the plugin takes it: a note value where it is locked to
    // one the plugin has, else seconds.
    let synced = room
        .sync
        .and_then(|i| crate::beats::REVERB.get(i))
        .and_then(|(_, quarters)| note(*quarters));
    let mut settings = vec![("Space", space.to_owned())];
    if let Some(div) = synced {
        settings.push(("Decay Mode", "Sync".to_owned()));
        settings.push(("Decay Div", div.to_owned()));
    } else {
        settings.push(("Decay Mode", "Time".to_owned()));
        settings.push(("Decay Time", format!("{:.2}", room.decay)));
    }
    settings.extend([
        ("Pre-Delay", format!("{:.1} ms", room.predelay)),
        ("Size", format!("{:.0}%", room.size * 100.0)),
        ("Damping", format!("{:.0}%", room.damping * 100.0)),
        ("Diffusion", format!("{:.0}%", room.diffusion * 100.0)),
        ("Mix", format!("{:.0}%", room.mix * 100.0)),
    ]);
    let extra = tone.reverb_extra;
    settings.extend([
        ("Modulation", format!("{:.0}%", extra.modulation * 100.0)),
        ("Tone", format!("{:.0}%", (extra.tone + 1.0) * 50.0)),
    ]);
    let mut out = all("FTS Reverb", &settings);
    // The cuts as the post-EQ's first band and its last ([`crate::mix_chains`]
    // puts them there): a cut at the edge of hearing is no cut, the band a bell.
    let band = |n: u32, cut: &str, hz: f32, none: bool| -> Vec<(u32, f64)> {
        const BAND: u32 = 4;
        let shape = if none { "Bell" } else { cut };
        [("Post Shape", shape.to_owned()), ("Post Freq", hz_text(hz))]
            .iter()
            .filter_map(|(param, text)| at("FTS Reverb", param, text))
            .map(|(index, value)| {
                (
                    index.saturating_add(n.saturating_sub(1).saturating_mul(BAND)),
                    value,
                )
            })
            .collect()
    };
    out.extend(band(1, "Low Cut", extra.low_cut, extra.low_cut <= 20.0));
    out.extend(band(
        6,
        "High Cut",
        extra.high_cut,
        extra.high_cut >= 20_000.0,
    ));
    out
}

fn delay(tone: &Tone) -> Vec<(u32, f64)> {
    let echo = &tone.delay;
    let space = delay_profiles::PROFILES
        .iter()
        .find(|p| p.style == echo.style)
        .map(|p| p.name);
    let synced = echo
        .sync
        .and_then(|i| crate::beats::DELAY.get(i))
        .and_then(|(_, _, quarters)| note(*quarters));
    let mut settings: Vec<(&str, String)> =
        space.map(|s| ("Space", s.to_owned())).into_iter().collect();
    if let Some(div) = synced {
        settings.push(("Time Sync", "Sync".to_owned()));
        settings.push(("Time L Div", div.to_owned()));
    } else {
        settings.push(("Time Sync", "Free".to_owned()));
        settings.push(("Time L", format!("{:.1} ms", echo.time)));
    }
    settings.extend([
        ("Feedback", format!("{:.0}%", echo.feedback * 100.0)),
        ("Tone", format!("{:.0}%", echo.tone * 100.0)),
        ("Mix", format!("{:.0}%", echo.mix * 100.0)),
    ]);
    all("FTS Delay", &settings)
}

fn modulation(tone: &Tone) -> Vec<(u32, f64)> {
    use modulation_ui::viz::Engine;
    let w = &tone.modulation;
    let circuit = match w.engine {
        Engine::Flanger => "Flanger",
        Engine::Vibrato => "Vibrato",
        Engine::Tremolo => "Opto",
        Engine::Phaser => "Auto Wah",
        Engine::Chorus | Engine::Rotary => "Juno",
    };
    all(
        "FTS Modulation",
        &[
            ("Circuit", circuit.to_owned()),
            ("Rate", format!("{:.2} Hz", w.rate)),
            ("Depth", format!("{:.0}", w.depth * 100.0)),
            ("Mix", format!("{:.0}", w.mix * 100.0)),
        ],
    )
}

/// `(index, value)` for `fx`'s `param` at `text`, if the plugin knows it.
fn at(fx: &str, param: &str, text: &str) -> Option<(u32, f64)> {
    builtin_fx::setting(fx, param, text)
}

fn eq(tone: &Tone) -> Vec<(u32, f64)> {
    const FX: &str = "FTS EQ";
    let mut out = Vec::new();
    // Every band the plugin has: `B1 On` … until there is no such band.
    let mut bands = tone.eq.iter().filter(|b| b.used);
    for n in 1_u32.. {
        let on = format!("B{n} On");
        let band = bands.next();
        let Some(switch) = at(
            FX,
            &on,
            if band.is_some_and(|b| b.enabled) {
                "On"
            } else {
                "Off"
            },
        ) else {
            break;
        };
        out.push(switch);
        let Some(band) = band else { continue };
        for (param, text) in [
            ("Type", shape(band.shape).to_owned()),
            ("Freq", format!("{:.1} Hz", band.frequency)),
            ("Gain", format!("{:.1} dB", band.gain)),
            ("Q", format!("{:.2}", band.q)),
        ] {
            out.extend(at(FX, &format!("B{n} {param}"), &text));
        }
    }
    out
}

fn comp(tone: &Tone) -> Vec<(u32, f64)> {
    use crate::plugin_faces::CompFace;
    const FX: &str = "FTS Comp";
    let c = tone.comp;
    let style = match tone.comp_face {
        CompFace::Fet => "FET",
        CompFace::Opto => "Opto",
        CompFace::Vca => "VCA",
    };
    [
        ("Style", style.to_owned()),
        ("Threshold", format!("{:.1} dB", c.threshold)),
        ("Ratio", format!("{:.1}:1", c.ratio)),
        ("Knee", format!("{:.1} dB", c.knee)),
        ("Attack", format!("{:.1} ms", c.attack)),
        ("Release", format!("{:.0} ms", c.release)),
    ]
    .iter()
    .filter_map(|(param, text)| at(FX, param, text))
    .collect()
}

fn sat(tone: &Tone) -> Vec<(u32, f64)> {
    const FX: &str = "FTS Saturate";
    let Some(profile) = saturate_profiles::PROFILES.get(tone.sat_profile) else {
        return Vec::new();
    };
    let v = profile.voicing;
    // `apply` made the stage from the knobs; these are the knobs again.
    let drive = ((tone.sat.drive - 1.0) / (15.0 * v.drive_scale.max(f32::EPSILON))).clamp(0.0, 1.0);
    let bias = ((tone.sat.q_point - v.q_point) / 1.2 + 0.5).clamp(0.0, 1.0);
    [
        ("Space", profile.name.to_owned()),
        ("Drive", format!("{:.0}%", drive * 100.0)),
        ("Bias", format!("{:.0}%", bias * 100.0)),
        (
            "Mix",
            format!("{:.0}%", tone.sat.mix.clamp(0.0, 1.0) * 100.0),
        ),
    ]
    .iter()
    .filter_map(|(param, text)| at(FX, param, text))
    .collect()
}

#[cfg(test)]
mod tests {
    use crate::engine::Edit;
    use crate::tone::Store;

    fn store() -> Store {
        let track = daw_proto::Track {
            guid: "t".into(),
            name: "Kick In".into(),
            ..daw_proto::Track::default()
        };
        let mut store = Store::default();
        store.seed(&[(track, 0)]);
        store
    }

    /// A stage turned for the first time is switched on and sent whole —
    /// in the plugin's own terms: the threshold set reads back as set —
    /// and after that only what moved goes; an untouched stage stays off.
    #[test]
    fn the_rack_turned_sets_the_tracks_processor() {
        let mut store = store();
        store.edit("t").expect("t").comp.threshold = -30.0;
        let first = store.flush();
        let comp: Vec<&Edit> = first.iter().filter(|e| matches!(e, Edit::SetFxParam(_, fx, ..) | Edit::SetFxEnabled(_, fx, _) if fx == "FTS Comp")).collect();
        assert!(first.contains(&Edit::SetFxEnabled("t".into(), "FTS Comp".into(), true)));
        assert_eq!(
            comp.len(),
            7,
            "its style, five parameters and the switch: {comp:?}"
        );
        assert!(first.iter().all(|e| !matches!(e, Edit::SetFxParam(_, fx, ..) | Edit::SetFxEnabled(_, fx, _) if fx != "FTS Comp")), "EQ and Saturate untouched: {first:?}");
        let (index, value) =
            builtin_fx::setting("FTS Comp", "Threshold", "-30.0 dB").expect("a threshold");
        assert!(first.contains(&Edit::SetFxParam(
            "t".into(),
            "FTS Comp".into(),
            index,
            value
        )));

        store.edit("t").expect("t").comp.release = 300.0;
        let second = store.flush();
        assert_eq!(second.len(), 1, "only the release: {second:?}");
        assert!(matches!(&second[0], Edit::SetFxParam(_, fx, ..) if fx == "FTS Comp"));
        assert!(store.flush().is_empty(), "nothing moved since");
    }

    /// The EQ's first change sends every band the plugin has: the rack's,
    /// and the rest off — the plugin's own defaults have bands on.
    #[test]
    fn the_eq_sends_every_band() {
        let mut store = store();
        if let Some(tone) = store.edit("t") {
            tone.eq.clear();
        }
        let edits = store.flush();
        let off = builtin_fx::setting("FTS EQ", "B2 On", "Off").expect("B2 On");
        assert!(
            edits.contains(&Edit::SetFxParam("t".into(), "FTS EQ".into(), off.0, off.1)),
            "{edits:?}"
        );
        assert!(edits.contains(&Edit::SetFxEnabled("t".into(), "FTS EQ".into(), true)));
    }
}
