//! A track's real chain, read back into the Tone the FX row draws.
//!
//! The other half of [`crate::tone_bind`]: that writes a rack's settings
//! into the track's FTS processors in each plugin's own words; this reads
//! the processors on the track — which there are, in order, on or
//! bypassed, and each one's parameters as the plugin itself writes them
//! (`-18.0 dB`, `4.0:1`, `Low Shelf`, `Plate`, `1/2`) — into the same
//! fields, so the row shows what the track has and not a placeholder.
//!
//! Each processor fills its own part of the Tone: FTS EQ the EQ bands,
//! Comp the compressor (and which face it wears, by its Style), Saturate
//! the circuit and its knobs, Gate the gate, Reverb the room (its time as
//! the plugin takes it: a position, seconds, or a note value at the
//! song's tempo), Delay the echo, Modulation the wobble. A processor the
//! row has no face for is listed by name, as it is in the chain.

use daw_proto::fx::FxParameter;

use crate::tone::{Tone, Which};

/// One processor on a track, as the engine has it.
#[derive(Clone, Debug)]
pub struct EngineFx {
    /// Its name in the chain (`FTS Reverb`).
    pub name: String,
    pub enabled: bool,
    pub params: Vec<FxParameter>,
}

impl EngineFx {
    /// A parameter's text as the plugin writes it, by its name.
    fn text(&self, name: &str) -> Option<&str> {
        self.params
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(name))
            .map(|p| p.formatted.as_str())
    }

    /// A parameter's number, from its text (`-18.0 dB`, `1.4 kHz`, `4.0:1`).
    fn number(&self, name: &str) -> Option<f64> {
        self.text(name).and_then(number)
    }

    /// A parameter given in percent (`45%`), as 0..1.
    fn fraction(&self, name: &str) -> Option<f64> {
        self.number(name).map(|n| n / 100.0)
    }
}

/// The row's device for a processor, by its name; `None` for one the row
/// has no face for.
#[must_use]
pub fn which_of(name: &str) -> Option<Which> {
    let bare = name.trim();
    Some(match bare {
        n if n.eq_ignore_ascii_case("FTS EQ") => Which::Eq,
        n if n.eq_ignore_ascii_case("FTS Comp") => Which::Comp,
        n if n.eq_ignore_ascii_case("FTS Saturate") => Which::Sat,
        n if n.eq_ignore_ascii_case("FTS Gate") => Which::Gate,
        n if n.eq_ignore_ascii_case("FTS Reverb") => Which::Reverb,
        n if n.eq_ignore_ascii_case("FTS Delay") => Which::Delay,
        n if n.eq_ignore_ascii_case("FTS Modulation") => Which::Mod,
        n if n.eq_ignore_ascii_case("FTS Pitch") => Which::Pitch,
        n if n.eq_ignore_ascii_case("FTS Unison") => Which::Wide,
        _ => return None,
    })
}

/// The leading number of a plugin's text, its unit's scale applied:
/// `-18.0 dB` → -18, `1.4 kHz` → 1400, `4.0:1` → 4, `1.00×` → 1, `2.00 s`
/// → 2.
#[must_use]
pub fn number(text: &str) -> Option<f64> {
    let text = text.trim().replace('\u{2212}', "-");
    let end = text
        .char_indices()
        .find(|(i, c)| !(c.is_ascii_digit() || *c == '.' || (*i == 0 && (*c == '-' || *c == '+'))))
        .map_or(text.len(), |(i, _)| i);
    let value: f64 = text.get(..end)?.parse().ok()?;
    let rest = text.get(end..).unwrap_or("").trim_start();
    Some(if rest.starts_with("kHz") {
        value * 1_000.0
    } else {
        value
    })
}

/// A delay's or a pre-delay's time in milliseconds from its text: `375.0
/// ms`, `1.2 s`.
#[must_use]
pub fn millis(text: &str) -> Option<f64> {
    let value = number(text)?;
    let unit = text
        .trim()
        .trim_start_matches(|c: char| c.is_ascii_digit() || "+-.\u{2212}".contains(c))
        .trim();
    Some(if unit == "s" { value * 1_000.0 } else { value })
}

/// The engine's chain on `track` (its processors in order), each with its
/// parameters. Empty with no song, or a track with none.
#[cfg(feature = "native")]
#[must_use]
pub fn chain(track: &str) -> Vec<EngineFx> {
    use daw::service::{Effects, FxChainContext, FxRef, FxTarget, ProjectContext};
    let (Some(daw), Some(song)) = (crate::open::local_engine(), crate::open::current_song()) else {
        return Vec::new();
    };
    let project = ProjectContext::Project(song);
    let context = FxChainContext::Track(track.to_owned());
    Effects::list(daw, project.clone(), context.clone())
        .into_iter()
        .map(|fx| EngineFx {
            params: Effects::parameters(
                daw,
                project.clone(),
                FxTarget {
                    context: context.clone(),
                    fx: FxRef::Guid(fx.guid.clone()),
                },
            ),
            name: fx.name,
            enabled: fx.enabled,
        })
        .collect()
}

/// Put what `chain` holds into `tone`.
///
/// Each processor the row has a face for fills its part, and is on or
/// bypassed as it is; `bpm` reads a note-valued time. Parts with no
/// processor on the track keep what they had (they are not shown:
/// [`devices`]).
pub fn apply(tone: &mut Tone, chain: &[EngineFx], bpm: f64) {
    for fx in chain {
        let Some(which) = which_of(&fx.name) else {
            continue;
        };
        tone.bypass.set(which, !fx.enabled);
        match which {
            Which::Eq => eq(tone, fx),
            Which::Comp => comp(tone, fx),
            Which::Sat => sat(tone, fx),
            Which::Gate => gate(tone, fx),
            Which::Reverb => reverb(tone, fx, bpm),
            Which::Delay => delay(tone, fx, bpm),
            Which::Mod => modulation(tone, fx),
            _ => {}
        }
    }
}

/// The row's devices for a chain, in its order: those the row has a face
/// for, each once.
#[must_use]
pub fn devices(chain: &[EngineFx]) -> Vec<Which> {
    let mut out: Vec<Which> = Vec::new();
    for which in chain.iter().filter_map(|fx| which_of(&fx.name)) {
        if !out.contains(&which) {
            out.push(which);
        }
    }
    out
}

/// The processors on the track the row has no face for, by name.
#[must_use]
pub fn others(chain: &[EngineFx]) -> Vec<(String, bool)> {
    chain
        .iter()
        .filter(|fx| which_of(&fx.name).is_none())
        .map(|fx| (fx.name.clone(), fx.enabled))
        .collect()
}

fn narrow(value: f64) -> f32 {
    crate::num::narrow(value)
}

fn eq(tone: &mut Tone, fx: &EngineFx) {
    use eq_ui::eq_graph_model::{EqBand, EqBandShape as S};
    let shape = |text: &str| match text.trim() {
        "Low Shelf" => S::LowShelf,
        "High Shelf" => S::HighShelf,
        "Low Cut" => S::LowCut,
        "High Cut" => S::HighCut,
        "Notch" => S::Notch,
        "Bandpass" => S::BandPass,
        "Tilt Shelf" => S::TiltShelf,
        "Flat Tilt" => S::FlatTilt,
        "All Pass" => S::AllPass,
        _ => S::Bell,
    };
    let mut bands = Vec::new();
    for n in 1_usize.. {
        let Some(on) = fx.text(&format!("B{n} On")) else {
            break;
        };
        let on = on.trim().eq_ignore_ascii_case("On");
        if !on {
            continue;
        }
        bands.push(EqBand {
            index: n.saturating_sub(1),
            used: true,
            enabled: true,
            frequency: narrow(fx.number(&format!("B{n} Freq")).unwrap_or(1_000.0)),
            gain: narrow(fx.number(&format!("B{n} Gain")).unwrap_or(0.0)),
            q: narrow(fx.number(&format!("B{n} Q")).unwrap_or(0.71)),
            shape: shape(fx.text(&format!("B{n} Type")).unwrap_or("Bell")),
            ..EqBand::default()
        });
    }
    tone.eq = bands;
}

fn comp(tone: &mut Tone, fx: &EngineFx) {
    use crate::plugin_faces::CompFace;
    let c = &mut tone.comp;
    if let Some(v) = fx.number("Threshold") {
        c.threshold = narrow(v);
    }
    if let Some(v) = fx.number("Ratio") {
        c.ratio = narrow(v);
    }
    if let Some(v) = fx.number("Knee") {
        c.knee = narrow(v);
    }
    if let Some(v) = fx.number("Attack") {
        c.attack = narrow(v);
    }
    if let Some(v) = fx.number("Release") {
        c.release = narrow(v);
    }
    // The face it wears is its detector: FET a 1176's, Opto an LA-2A's.
    tone.comp_face = match fx.text("Style").map(str::trim) {
        Some("FET") => CompFace::Fet,
        Some("Opto") => CompFace::Opto,
        Some(_) => CompFace::Vca,
        None => tone.comp_face,
    };
}

fn sat(tone: &mut Tone, fx: &EngineFx) {
    if let Some(index) = fx.text("Space").and_then(|space| {
        saturate_profiles::PROFILES
            .iter()
            .position(|p| p.name.eq_ignore_ascii_case(space.trim()))
    }) {
        tone.set_sat_profile(index);
    }
    let Some(profile) = saturate_profiles::PROFILES.get(tone.sat_profile) else {
        return;
    };
    let v = profile.voicing;
    // `tone_bind::sat` made the knobs from these; these from the knobs.
    if let Some(drive) = fx.fraction("Drive") {
        tone.sat.drive = 15.0_f32.mul_add(v.drive_scale * narrow(drive), 1.0);
    }
    if let Some(bias) = fx.fraction("Bias") {
        tone.sat.q_point = 1.2_f32.mul_add(narrow(bias) - 0.5, v.q_point);
    }
    if let Some(mix) = fx.fraction("Mix") {
        tone.sat.mix = narrow(mix);
    }
}

fn gate(tone: &mut Tone, fx: &EngineFx) {
    let g = &mut tone.gate;
    for (name, field) in [
        ("Threshold", &mut g.threshold),
        ("Range", &mut g.range),
        ("Attack", &mut g.attack),
        ("Hold", &mut g.hold),
        ("Release", &mut g.release),
    ] {
        if let Some(v) = fx.number(name) {
            *field = narrow(v);
        }
    }
}

/// A note value's length in quarter notes (`1/2` → 2, `1/8D` → 0.75,
/// `1/4T` → 2/3, `1/1` → 4).
#[must_use]
pub fn quarters(label: &str) -> Option<f64> {
    let label = label.trim();
    let (body, factor) = label.strip_suffix('D').map_or_else(
        || {
            label
                .strip_suffix('T')
                .map_or((label, 1.0), |body| (body, 2.0 / 3.0))
        },
        |body| (body, 1.5),
    );
    let (n, d) = body.split_once('/')?;
    let (n, d): (f64, f64) = (n.trim().parse().ok()?, d.trim().parse().ok()?);
    (d > 0.0).then(|| 4.0 * n / d * factor)
}

fn reverb(tone: &mut Tone, fx: &EngineFx, bpm: f64) {
    let beat = 60.0 / bpm.max(1.0);
    let room = &mut tone.reverb;
    let profile = fx.text("Space").and_then(|space| {
        reverb_profiles::PROFILES
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(space.trim()))
    });
    if let Some(p) = profile {
        room.algorithm = p.algorithm;
    }
    // Its time as the plugin takes it.
    let mode = fx.text("Decay Mode").map_or("%", str::trim);
    let seconds = match mode {
        "Time" => fx.number("Decay Time"),
        "Sync" => fx.text("Decay Div").and_then(quarters).map(|q| q * beat),
        _ => profile.and_then(|p| {
            reverb_dsp::algorithm::decay_seconds(p.algorithm, p.variant, fx.fraction("Decay")?)
        }),
    };
    if let Some(seconds) = seconds {
        room.decay = narrow(seconds);
    }
    room.sync = (mode == "Sync")
        .then(|| fx.text("Decay Div").and_then(quarters))
        .flatten()
        .and_then(|q| {
            crate::beats::REVERB
                .iter()
                .position(|(_, beats)| (beats - q).abs() < 1e-6)
        });
    let predelay = match fx.text("Pre-Delay Sync").map(str::trim) {
        Some("Sync") => fx
            .text("Pre-Delay Div")
            .and_then(quarters)
            .map(|q| q * beat * 1_000.0),
        _ => fx.text("Pre-Delay").and_then(millis),
    };
    if let Some(ms) = predelay {
        room.predelay = narrow(ms);
    }
    for (name, field) in [
        ("Size", &mut room.size),
        ("Damping", &mut room.damping),
        ("Diffusion", &mut room.diffusion),
        ("Mix", &mut room.mix),
    ] {
        if let Some(v) = fx.fraction(name) {
            *field = narrow(v);
        }
    }
    let extra = &mut tone.reverb_extra;
    if let Some(v) = fx.fraction("Modulation") {
        extra.modulation = narrow(v);
    }
    if let Some(v) = fx.fraction("Tone") {
        extra.tone = narrow(v.mul_add(2.0, -1.0));
    }
    // The cuts: the post-EQ's bands shaped as one, wherever they are.
    let (mut low, mut high) = (20.0_f64, 20_000.0_f64);
    let shapes = fx.params.iter().filter(|p| p.name == "Post Shape");
    let freqs: Vec<&FxParameter> = fx.params.iter().filter(|p| p.name == "Post Freq").collect();
    for (shape, freq) in shapes.zip(freqs) {
        let hz = number(&freq.formatted).unwrap_or(0.0);
        match shape.formatted.trim() {
            "Low Cut" => low = low.max(hz),
            "High Cut" => high = high.min(hz),
            _ => {}
        }
    }
    extra.low_cut = narrow(low);
    extra.high_cut = narrow(high);
}

fn delay(tone: &mut Tone, fx: &EngineFx, bpm: f64) {
    let beat = 60.0 / bpm.max(1.0);
    let echo = &mut tone.delay;
    let synced = fx.text("Time Sync").map(str::trim) == Some("Sync");
    let ms = if synced {
        fx.text("Time L Div")
            .and_then(quarters)
            .map(|q| q * beat * 1_000.0)
    } else {
        fx.text("Time L").and_then(millis)
    };
    if let Some(ms) = ms {
        echo.time = narrow(ms);
    }
    echo.sync = synced
        .then(|| fx.text("Time L Div").and_then(quarters))
        .flatten()
        .and_then(|q| {
            crate::beats::DELAY
                .iter()
                .position(|(_, _, beats)| (beats - q).abs() < 1e-6)
        });
    for (name, field) in [
        ("Feedback", &mut echo.feedback),
        ("Mix", &mut echo.mix),
        ("Tone", &mut echo.tone),
    ] {
        if let Some(v) = fx.fraction(name) {
            *field = narrow(v);
        }
    }
    if let Some(profile) = fx.text("Space").and_then(|space| {
        delay_profiles::PROFILES
            .iter()
            .find(|p| p.name.eq_ignore_ascii_case(space.trim()))
    }) {
        echo.style = profile.style;
    }
}

fn modulation(tone: &mut Tone, fx: &EngineFx) {
    use modulation_ui::viz::Engine;
    let w = &mut tone.modulation;
    if let Some(circuit) = fx.text("Circuit").map(str::trim) {
        // The circuit's family is what the face draws: delay modulation
        // (chorus, flanger, vibrato), amplitude (tremolo), or a filter
        // swept (a wah, drawn as the phaser — the face's filter engine).
        w.engine = match circuit {
            "Flanger" | "Analog Flanger" => Engine::Flanger,
            "Vibrato" | "CE-1 Vibrato" | "Juno Vibrato" => Engine::Vibrato,
            "Opto" | "Stereo" | "Harmonic" => Engine::Tremolo,
            "Auto Wah" | "Pedal" | "Rhythmic" => Engine::Phaser,
            _ => Engine::Chorus,
        };
    }
    if let Some(rate) = fx.number("Rate") {
        w.rate = narrow(rate);
    }
    // Depth and Mix are written 0–100, without a percent sign.
    if let Some(depth) = fx.number("Depth") {
        w.depth = narrow(depth / 100.0);
    }
    if let Some(mix) = fx.number("Mix") {
        w.mix = narrow(mix / 100.0);
    }
}

#[cfg(test)]
mod tests {

    /// A real FTS processor set up through its own parameters, read back
    /// as the engine reports it: names, values and the plugin's texts.
    fn engine_fx(name: &str, settings: &[(&str, &str)]) -> EngineFx {
        use daw_standalone::plugin::{FxFactory, PluginEvents};
        let mut fx = builtin_fx::BuiltinFx
            .create(name, 48_000.0)
            .expect("a built-in");
        fx.prepare(48_000.0, 64).expect("prepared");
        let changes: Vec<(u32, f64)> = settings
            .iter()
            .map(|(param, text)| {
                let base = param
                    .trim_end_matches(|c: char| c.is_ascii_digit())
                    .trim_end();
                builtin_fx::setting(name, base, text).unwrap_or_else(|| panic!("{param}={text}"))
            })
            .collect();
        let (silence, mut l, mut r) = (vec![0.0_f32; 64], vec![0.0_f32; 64], vec![0.0_f32; 64]);
        let events = PluginEvents {
            params: &changes,
            ..PluginEvents::EMPTY
        };
        fx.process_block(&silence, &silence, &mut l, &mut r, &events)
            .expect("a block");
        let params = fx
            .params()
            .into_iter()
            .map(|info| {
                let value = fx.param_value(info.id).unwrap_or(0.0);
                let mut p = FxParameter::new(info.id, info.name, value);
                p.formatted = fx.value_to_text(info.id, value).unwrap_or_default();
                p
            })
            .collect();
        EngineFx {
            name: name.to_owned(),
            enabled: true,
            params,
        }
    }

    /// Each processor's settings on the track are what the row then shows.
    #[test]
    fn a_tracks_processors_are_what_the_row_shows() {
        let chain = [
            engine_fx(
                "FTS Comp",
                &[
                    ("Threshold", "-24.0 dB"),
                    ("Ratio", "6.0:1"),
                    ("Attack", "10.0 ms"),
                    ("Style", "Opto"),
                ],
            ),
            engine_fx(
                "FTS Gate",
                &[
                    ("Threshold", "-33.0 dB"),
                    ("Range", "20.0 dB"),
                    ("Release", "150.0 ms"),
                ],
            ),
            engine_fx(
                "FTS Reverb",
                &[
                    ("Space", "Plate"),
                    ("Decay Mode", "Sync"),
                    ("Decay Div", "1/2"),
                    ("Pre-Delay", "40.0 ms"),
                    ("Mix", "100%"),
                ],
            ),
            engine_fx(
                "FTS Delay",
                &[
                    ("Time Sync", "Sync"),
                    ("Time L Div", "1/4D"),
                    ("Feedback", "35%"),
                    ("Mix", "100%"),
                ],
            ),
            engine_fx(
                "FTS Saturate",
                &[("Space", "Tape"), ("Drive", "60%"), ("Mix", "80%")],
            ),
            engine_fx(
                "FTS EQ",
                &[
                    ("B1 On", "On"),
                    ("B1 Type", "Low Cut"),
                    ("B1 Freq", "120.0 Hz"),
                    ("B2 On", "On"),
                    ("B2 Type", "Bell"),
                    ("B2 Freq", "3.0 kHz"),
                    ("B2 Gain", "4.0 dB"),
                ],
            ),
        ];
        let mut tone = crate::tone::placeholder(0);
        apply(&mut tone, &chain, 120.0);

        assert!((tone.comp.threshold + 24.0).abs() < 0.1 && (tone.comp.ratio - 6.0).abs() < 0.1);
        assert!((tone.comp.attack - 10.0).abs() < 0.1);
        assert_eq!(tone.comp_face, crate::plugin_faces::CompFace::Opto);
        assert!((tone.gate.threshold + 33.0).abs() < 0.1 && (tone.gate.range - 20.0).abs() < 0.1);
        assert_eq!(tone.reverb.algorithm, reverb_dsp::AlgorithmType::Plate);
        assert!(
            (tone.reverb.decay - 1.0).abs() < 1e-3,
            "a half note at 120: {}",
            tone.reverb.decay
        );
        assert!((tone.reverb.predelay - 40.0).abs() < 0.5 && (tone.reverb.mix - 1.0).abs() < 1e-3);
        assert!(
            (tone.delay.time - 750.0).abs() < 1.0,
            "a dotted quarter at 120: {}",
            tone.delay.time
        );
        assert!(tone.delay.sync.is_some() && (tone.delay.feedback - 0.35).abs() < 0.01);
        let tape = saturate_profiles::PROFILES
            .iter()
            .position(|p| p.name == "Tape");
        assert_eq!(Some(tone.sat_profile), tape);
        assert!((tone.sat.mix - 0.8).abs() < 0.01);
        assert_eq!(tone.eq.len(), 2, "the bands that are on");
        assert!(
            (tone.eq[1].frequency - 3_000.0).abs() < 5.0 && (tone.eq[1].gain - 4.0).abs() < 0.1
        );
        assert_eq!(
            devices(&chain),
            vec![
                Which::Comp,
                Which::Gate,
                Which::Reverb,
                Which::Delay,
                Which::Sat,
                Which::Eq
            ]
        );
        assert!(!tone.bypass.any(), "all on");
    }

    /// Written by the row (`tone_bind`) and read back here, a setting is
    /// the setting: the two halves agree on every word.
    #[cfg(feature = "native")]
    #[test]
    fn written_and_read_back_is_the_same() {
        use daw_standalone::plugin::{FxFactory, PluginEvents};
        let mut tone = crate::tone::placeholder(0);
        tone.reverb.algorithm = reverb_dsp::AlgorithmType::Hall;
        tone.reverb.decay = 2.4;
        tone.reverb.sync = None;
        tone.reverb.predelay = 35.0;
        tone.reverb.mix = 0.6;
        tone.reverb_extra.low_cut = 250.0;
        tone.reverb_extra.high_cut = 7_000.0;
        tone.reverb_extra.modulation = 0.35;
        tone.delay.time = 420.0;
        tone.delay.sync = None;
        tone.delay.feedback = 0.4;
        tone.gate.threshold = -36.0;
        tone.gate.release = 120.0;
        let chain: Vec<EngineFx> = crate::tone_bind::stages(&tone)
            .into_iter()
            .filter(|(name, _)| ["FTS Reverb", "FTS Delay", "FTS Gate"].contains(name))
            .map(|(name, changes)| {
                let mut fx = builtin_fx::BuiltinFx
                    .create(name, 48_000.0)
                    .expect("a built-in");
                fx.prepare(48_000.0, 64).expect("prepared");
                let (silence, mut l, mut r) =
                    (vec![0.0_f32; 64], vec![0.0_f32; 64], vec![0.0_f32; 64]);
                let events = PluginEvents {
                    params: &changes,
                    ..PluginEvents::EMPTY
                };
                fx.process_block(&silence, &silence, &mut l, &mut r, &events)
                    .expect("a block");
                let params = fx
                    .params()
                    .into_iter()
                    .map(|info| {
                        let value = fx.param_value(info.id).unwrap_or(0.0);
                        let mut p = FxParameter::new(info.id, info.name, value);
                        p.formatted = fx.value_to_text(info.id, value).unwrap_or_default();
                        p
                    })
                    .collect();
                EngineFx {
                    name: name.to_owned(),
                    enabled: true,
                    params,
                }
            })
            .collect();
        let mut back = crate::tone::placeholder(3);
        apply(&mut back, &chain, 120.0);
        assert_eq!(back.reverb.algorithm, tone.reverb.algorithm);
        assert!(
            (back.reverb.decay - 2.4).abs() < 0.02,
            "{}",
            back.reverb.decay
        );
        assert!((back.reverb.predelay - 35.0).abs() < 0.2 && (back.reverb.mix - 0.6).abs() < 0.01);
        assert!(
            (back.reverb_extra.low_cut - 250.0).abs() < 2.0,
            "{}",
            back.reverb_extra.low_cut
        );
        assert!(
            (back.reverb_extra.high_cut - 7_000.0).abs() < 60.0,
            "{}",
            back.reverb_extra.high_cut
        );
        assert!((back.reverb_extra.modulation - 0.35).abs() < 0.01);
        assert!((back.delay.time - 420.0).abs() < 0.5 && (back.delay.feedback - 0.4).abs() < 0.01);
        assert!(
            (back.gate.threshold + 36.0).abs() < 0.1 && (back.gate.release - 120.0).abs() < 0.5
        );
    }

    /// A bypassed processor is shown bypassed; one with no face is named.
    #[test]
    fn a_bypassed_processor_and_one_with_no_face() {
        let mut comp = engine_fx("FTS Comp", &[]);
        comp.enabled = false;
        let rig = EngineFx {
            name: "FTS Bass Rig".into(),
            enabled: true,
            params: Vec::new(),
        };
        let mut tone = crate::tone::placeholder(0);
        apply(&mut tone, &[comp.clone(), rig.clone()], 120.0);
        assert!(tone.bypass.is(Which::Comp));
        assert_eq!(
            others(&[comp, rig]),
            vec![("FTS Bass Rig".to_owned(), true)]
        );
    }

    use super::*;

    #[test]
    fn a_plugins_text_is_its_number() {
        assert_eq!(number("-18.0 dB"), Some(-18.0));
        assert_eq!(number("1.4 kHz"), Some(1_400.0));
        assert_eq!(number("4.0:1"), Some(4.0));
        assert_eq!(number("375.0 ms"), Some(375.0));
        assert_eq!(number("2.00 s"), Some(2.0));
        assert_eq!(millis("1.2 s"), Some(1_200.0));
        assert_eq!(millis("375.0 ms"), Some(375.0));
        assert_eq!(number("12 st"), Some(12.0));
        assert_eq!(number("45%"), Some(45.0));
        assert_eq!(number("Plate"), None);
    }

    #[test]
    fn a_note_value_is_its_quarters() {
        assert_eq!(quarters("1/4"), Some(1.0));
        assert_eq!(quarters("1/2"), Some(2.0));
        assert_eq!(quarters("1/1"), Some(4.0));
        assert_eq!(quarters("1/8D"), Some(0.75));
        assert!(quarters("1/4T").is_some_and(|q| (q - 2.0 / 3.0).abs() < 1e-9));
        assert_eq!(quarters("Free"), None);
    }
}
