//! The processors' own studio faces, over the rack's settings.
//!
//! frame's plugin sets (`comp/`, `eq/`, `saturate/`, `modulation/`) are
//! the effects as their plugins look: a blackface FET limiter, an opto
//! leveler, a four-band console EQ, a tube, a tape machine, a chorus.
//! Each speaks its own controls (`fet/input`, `consoleeq/lf_gain`…), and
//! the rack's settings are generic — a threshold, a list of bands — so
//! each face here is an adapter: the settings as the face's values, and
//! the face's edits back into the settings.

use crate::macros::{frac, span};
use crate::tone::{Tone, Which};

/// Which compressor a track wears.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum CompFace {
    /// The blackface FET limiter (an 1176): fast, for what transients.
    Fet,
    /// The opto leveler (an LA-2A): slow and smooth, for a voice.
    Opto,
    /// A VCA compressor: the plain one.
    #[default]
    Vca,
}

impl CompFace {
    /// The face a track's Tone compressor wears, by what the track is:
    /// a drum an 1176, a voice an LA-2A.
    #[must_use]
    pub fn for_track(name: &str, ancestors: &[String]) -> Self {
        let within = |word: &str| is_named(name, ancestors, word);
        if is_drum(name, ancestors) {
            Self::Fet
        } else if within("vocal") || within("vox") || within("bgv") {
            Self::Opto
        } else {
            Self::Vca
        }
    }

    /// What its header says it is.
    #[must_use]
    pub const fn title(self) -> &'static str {
        match self {
            Self::Fet => "FET 1176",
            Self::Opto => "Opto LA-2A",
            Self::Vca => "VCA",
        }
    }

    const fn design(self) -> &'static str {
        match self {
            Self::Fet => "comp/01-fet-black",
            Self::Opto => "comp/04-opto-leveler",
            Self::Vca => "comp/09-vca-comp",
        }
    }
}

/// Whether `word` is in the track's name or any folder over it.
#[must_use]
pub fn is_named(name: &str, ancestors: &[String], word: &str) -> bool {
    ancestors
        .iter()
        .map(String::as_str)
        .chain(std::iter::once(name))
        .any(|a| a.to_lowercase().contains(word))
}

/// Whether the track is a drum: a kick, a snare, a tom.
#[must_use]
pub fn is_drum(name: &str, ancestors: &[String]) -> bool {
    ["kick", "snare", "tom"]
        .iter()
        .any(|word| is_named(name, ancestors, word))
}

/// `design` in the shape `suffix` names (`-lane`, `-tall`) where that
/// has been designed, else `design` itself.
fn shaped(design: &str, suffix: &str) -> String {
    let wanted = format!("{design}{suffix}");
    if size_of(&wanted).is_some() {
        wanted
    } else {
        design.to_owned()
    }
}

/// Whether `design` is one of the lane-sized shapes.
#[must_use]
pub fn is_shaped(design: &str) -> bool {
    design.ends_with("-lane") || design.ends_with("-tall")
}

/// Designs' sizes, by design, as read (`None`: unreadable).
type Sizes = std::sync::Mutex<std::collections::HashMap<String, Option<(f64, f64)>>>;

/// A design's size, read from its markup's first frame once.
fn size_of(design: &str) -> Option<(f64, f64)> {
    static SIZES: std::sync::OnceLock<Sizes> = std::sync::OnceLock::new();
    let sizes = SIZES.get_or_init(Default::default);
    if let Some(known) = sizes.lock().ok().and_then(|s| s.get(design).copied()) {
        return known;
    }
    let path = crate::faces::design_dir().join(format!("{design}.frame"));
    let found = std::fs::read_to_string(path).ok().and_then(|text| {
        let frame = text.find("<Frame ")?;
        let line = text.get(frame..)?.lines().next()?;
        let number = |key: &str| -> Option<f64> {
            let rest = line.get(line.find(key)?..)?.strip_prefix(key)?;
            let end = rest
                .find(|c: char| !(c.is_ascii_digit() || c == '.'))
                .unwrap_or(rest.len());
            rest.get(..end)?.parse().ok()
        };
        Some((number(" w=")?, number(" h=")?))
    });
    if let Ok(mut s) = sizes.lock() {
        s.insert(design.to_owned(), found);
    }
    found
}

/// The design `which` wears over `tone`, if a plugin face serves it.
fn design(which: Which, tone: &Tone) -> Option<&'static str> {
    match which {
        // Every EQ panel is frame's EQ graph: its gestures, its keys, its
        // menus, lit by its glow where the GPU is there.
        Which::RescueEq
        | Which::Eq
        | Which::Space
        | Which::PreEq
        | Which::PostEq
        | Which::DecayEq
        | Which::PolishEq => Some(EQ_FACE),
        Which::Comp => Some(tone.comp_face.design()),
        Which::RescueComp if tone.comp_face == CompFace::Opto => Some(CompFace::Fet.design()),
        Which::Sat => Some(match sat_circuit(tone) {
            saturate_ui::viz::Circuit::Valve => "saturate/01-tube",
            saturate_ui::viz::Circuit::Tape => "saturate/02-tape",
            saturate_ui::viz::Circuit::Core => "saturate/03-transformer",
            saturate_ui::viz::Circuit::Solid => "saturate/04-transistor",
            saturate_ui::viz::Circuit::Steps => "saturate/05-digital",
        }),
        Which::Mod => match tone.modulation.engine {
            modulation_ui::viz::Engine::Chorus => Some("modulation/01-chorus"),
            modulation_ui::viz::Engine::Flanger => Some("modulation/02-flanger"),
            modulation_ui::viz::Engine::Vibrato => Some("modulation/03-vibrato"),
            modulation_ui::viz::Engine::Tremolo => Some("modulation/04-tremolo"),
            _ => None,
        },
        _ => None,
    }
}

/// The circuit the saturator's profile is.
fn sat_circuit(tone: &Tone) -> saturate_ui::viz::Circuit {
    saturate_profiles::PROFILES.get(tone.sat_profile).map_or(
        saturate_ui::viz::Circuit::Valve,
        |profile| {
            saturate_ui::viz::SatView::of(*profile, saturate_profiles::Controls::default())
                .circuit()
        },
    )
}

/// The namespace a design's controls are under.
fn ns_of(design: &str) -> &'static str {
    match design {
        EQ_FACE => "eq",
        "comp/01-fet-black" => "fet",
        "comp/04-opto-leveler" => "opto",
        "comp/09-vca-comp" => "vcacomp",
        "eq/02-console-eq" => "consoleeq",
        "saturate/01-tube" => "tube",
        "saturate/02-tape" => "tape_sat",
        "saturate/03-transformer" => "transformer",
        "saturate/04-transistor" => "transistor",
        "saturate/05-digital" => "digital_sat",
        "modulation/01-chorus" => "chorus",
        "modulation/02-flanger" => "flanger",
        "modulation/03-vibrato" => "vibrato",
        "modulation/04-tremolo" => "tremolo",
        _ => "",
    }
}

/// An 1176's attack and release dials run 1 (slow) to 7 (fast): `ms` on
/// a log scale over `lo..hi` ms, the dial the other way round.
fn dial(ms: f32, lo: f32, hi: f32) -> f64 {
    let f = (f64::from(ms.max(lo)) / f64::from(lo)).log(f64::from(hi) / f64::from(lo));
    6.0f64.mul_add(-f.clamp(0.0, 1.0), 7.0)
}

/// …and back.
fn undial(value: f64, lo: f32, hi: f32) -> f32 {
    let f = ((7.0 - value) / 6.0).clamp(0.0, 1.0);
    crate::num::narrow(f64::from(lo) * (f64::from(hi) / f64::from(lo)).powf(f))
}

/// The 1176's ratio buttons.
const FET_RATIOS: [f32; 4] = [4.0, 8.0, 12.0, 20.0];

/// The comp settings `which` stands for.
fn comp_of(tone: &Tone, which: Which) -> crate::tone::Comp {
    if which == Which::RescueComp {
        tone.rescue_comp
    } else {
        tone.comp
    }
}

fn comp_mut(tone: &mut Tone, which: Which) -> &mut crate::tone::Comp {
    if which == Which::RescueComp {
        &mut tone.rescue_comp
    } else {
        &mut tone.comp
    }
}

/// The used EQ bands, in order: a console EQ's four sections are the
/// first four.
/// frame's EQ graph face: 24 bands, `eq/b{n}_*` (n from 1).
const EQ_FACE: &str = "rig-faces/52-amp-eq";
const EQ_SLOTS: usize = 24;

/// frame's choice params take their position, 0..1 over the options.
fn choice(index: usize, options: usize) -> f64 {
    crate::num::coord(index) / crate::num::coord(options.saturating_sub(1).max(1))
}

/// And give it back.
fn choice_index(value: f64, options: usize) -> usize {
    let top = crate::num::coord(options.saturating_sub(1));
    crate::num::index((value.clamp(0.0, 1.0) * top).round())
}

const EQ_SHAPES: [eq_ui::eq_graph_model::EqBandShape; 10] = {
    use eq_ui::eq_graph_model::EqBandShape as S;
    [
        S::Bell,
        S::LowShelf,
        S::HighShelf,
        S::LowCut,
        S::HighCut,
        S::Notch,
        S::BandPass,
        S::TiltShelf,
        S::FlatTilt,
        S::AllPass,
    ]
};

const EQ_PLACEMENTS: [eq_ui::eq_graph_model::StereoMode; 5] = {
    use eq_ui::eq_graph_model::StereoMode as M;
    [M::Stereo, M::Left, M::Right, M::Mid, M::Side]
};

/// The slopes frame's graph offers (0 dB/oct … brickwall): the band's
/// own slope index.
const EQ_SLOPES: usize = 11;

/// What the EQ graph shows for `bands`: each slot by its band's index.
fn eq_values(bands: &[eq_ui::eq_graph_model::EqBand]) -> Vec<(String, f64)> {
    let mut out = Vec::new();
    for slot in 0..EQ_SLOTS {
        let at = |k: &str| format!("eq/b{}_{k}", slot + 1);
        let Some(b) = bands.iter().find(|b| b.index == slot && b.used) else {
            out.push((at("used"), 0.0));
            continue;
        };
        let shape = EQ_SHAPES.iter().position(|s| *s == b.shape).unwrap_or(0);
        let place = EQ_PLACEMENTS
            .iter()
            .position(|m| *m == b.stereo_mode)
            .unwrap_or(0);
        let slope = b
            .slope
            .map_or(2, |s| crate::num::index(f64::from(s).round()));
        out.extend([
            (at("used"), 1.0),
            (at("on"), if b.enabled { 1.0 } else { 0.0 }),
            (at("freq"), f64::from(b.frequency)),
            (at("gain"), f64::from(b.gain)),
            (at("q"), f64::from(b.q)),
            (at("shape"), choice(shape, EQ_SHAPES.len())),
            (at("slope"), choice(slope, EQ_SLOPES)),
            (at("placement"), choice(place, EQ_PLACEMENTS.len())),
            (at("focus"), if b.focus { 1.0 } else { 0.0 }),
        ]);
    }
    out
}

/// The EQ graph's band names.
fn eq_texts(bands: &[eq_ui::eq_graph_model::EqBand]) -> Vec<(String, String)> {
    bands
        .iter()
        .filter(|b| b.used && b.index < EQ_SLOTS)
        .map(|b| (format!("eq/b{}_name", b.index + 1), b.name.clone()))
        .collect()
}

/// An edit the EQ graph made: `param` (`b{n}_field`) on `which`'s bands.
/// A band added on the graph (`used` on) is made at its slot; one
/// removed is marked unused.
fn eq_edit(which: Which, tone: &mut Tone, param: &str, value: f64) -> bool {
    tone.bands(which)
        .is_some_and(|bands| bands_edit(bands, param, value))
}

/// An edit an EQ graph made on `bands`: `param` is `b{n}_field`.
fn bands_edit(bands: &mut Vec<eq_ui::eq_graph_model::EqBand>, param: &str, value: f64) -> bool {
    let Some((slot, field)) = param
        .strip_prefix('b')
        .and_then(|rest| rest.split_once('_'))
        .and_then(|(n, field)| Some((n.parse::<usize>().ok()?.checked_sub(1)?, field)))
    else {
        return false;
    };
    if slot >= EQ_SLOTS {
        return false;
    }
    let at = if let Some(at) = bands.iter().position(|b| b.index == slot) {
        at
    } else {
        bands.push(eq_ui::eq_graph_model::EqBand {
            index: slot,
            used: false,
            enabled: true,
            frequency: 1000.0,
            gain: 0.0,
            q: 0.707,
            slope: None,
            shape: eq_ui::eq_graph_model::EqBandShape::Bell,
            focus: false,
            stereo_mode: eq_ui::eq_graph_model::StereoMode::Stereo,
            name: String::new(),
        });
        bands.len() - 1
    };
    let Some(b) = bands.get_mut(at) else {
        return false;
    };
    let v = crate::num::narrow(value);
    let on = value >= 0.5;
    match field {
        "used" => b.used = on,
        "on" => b.enabled = on,
        "freq" => b.frequency = v,
        "gain" => b.gain = v,
        "q" => b.q = v,
        "shape" => {
            if let Some(shape) = EQ_SHAPES.get(choice_index(value, EQ_SHAPES.len())) {
                b.shape = *shape;
            }
        }
        "slope" => {
            b.slope = Some(crate::num::narrow(crate::num::coord(choice_index(
                value, EQ_SLOPES,
            ))));
        }
        "placement" => {
            if let Some(m) = EQ_PLACEMENTS.get(choice_index(value, EQ_PLACEMENTS.len())) {
                b.stereo_mode = *m;
            }
        }
        "focus" => b.focus = on,
        _ => return false,
    }
    true
}

/// The sidechain EQ's face: frame's EQ graph in yellow (`sc/b{n}_*`),
/// its SC key (`sc/sc_view`) the way back, its title `sc/title`.
const SC_FACE: &str = "comp/00-sc-eq";

/// The sidechain EQ face for `of`'s compressor, filling the column.
fn sc_face(
    tone: &Tone,
    of: crate::tone::Sidechain,
) -> Option<(crate::faces::FrameFace, (f64, f64))> {
    let size = size_of(SC_FACE)?;
    let bands = tone.sidechain.get(of as usize)?;
    let mut values = eq_values(bands);
    for (addr, _) in &mut values {
        *addr = addr.replacen("eq/", "sc/", 1);
    }
    values.push(("sc/sc_view".to_owned(), 1.0));
    let comp = match of {
        crate::tone::Sidechain::After => tone.comp_after.map_or(tone.comp_face, |(_, f)| f),
        crate::tone::Sidechain::Main => tone.comp_face,
        crate::tone::Sidechain::Rescue => CompFace::Fet,
    };
    Some((
        crate::faces::FrameFace {
            responsive: true,
            design: SC_FACE.to_owned(),
            values,
            texts: vec![(
                "sc/title".to_owned(),
                format!("{} · Sidechain", comp.title()),
            )],
        },
        size,
    ))
}

/// The emphasis EQ's shapes, by the face's `emph/b{n}_shape` index.
const EMPH_SHAPES: [saturate_dsp::emphasis::EmphShape; 3] = [
    saturate_dsp::emphasis::EmphShape::Bell,
    saturate_dsp::emphasis::EmphShape::LowShelf,
    saturate_dsp::emphasis::EmphShape::HighShelf,
];

/// What a saturator's emphasis view shows: whether it is up, and the six
/// bands (`<ns>/emph/b{n}_*`, n from 1).
fn emphasis_values(ns: &str, tone: &Tone) -> Vec<(String, f64)> {
    let mut out = vec![(
        format!("{ns}/emphasis_view"),
        if tone.sat_emphasis_view { 1.0 } else { 0.0 },
    )];
    for (i, b) in tone.sat_emphasis.iter().enumerate() {
        let at = |k: &str| format!("{ns}/emph/b{}_{k}", i + 1);
        let shape = EMPH_SHAPES.iter().position(|s| *s == b.shape).unwrap_or(0);
        out.extend([
            (at("shape"), choice(shape, EMPH_SHAPES.len())),
            (at("freq"), f64::from(b.freq_hz)),
            (at("gain"), f64::from(b.gain_db)),
            (at("q"), f64::from(b.q)),
        ]);
    }
    out
}

/// An edit on a saturator's emphasis view: the toggle, or `emph/b{n}_*`.
fn emphasis_edit(tone: &mut Tone, param: &str, value: f64) -> bool {
    if param == "emphasis_view" {
        tone.sat_emphasis_view = value >= 0.5;
        return true;
    }
    let Some((slot, field)) = param
        .strip_prefix("emph/b")
        .and_then(|rest| rest.split_once('_'))
        .and_then(|(n, field)| Some((n.parse::<usize>().ok()?.checked_sub(1)?, field)))
    else {
        return false;
    };
    let Some(b) = tone.sat_emphasis.get_mut(slot) else {
        return false;
    };
    let v = crate::num::narrow(value);
    match field {
        "shape" => {
            if let Some(shape) = EMPH_SHAPES.get(choice_index(value, EMPH_SHAPES.len())) {
                b.shape = *shape;
            }
        }
        "freq" => b.freq_hz = v.clamp(20.0, 20_000.0),
        "gain" => b.gain_db = v.clamp(-12.0, 12.0),
        "q" => b.q = v.clamp(0.1, 18.0),
        _ => return false,
    }
    true
}

fn band(tone: &Tone, n: usize) -> Option<&eq_ui::eq_graph_model::EqBand> {
    tone.eq.iter().filter(|b| b.used).nth(n)
}

fn band_mut(tone: &mut Tone, n: usize) -> Option<&mut eq_ui::eq_graph_model::EqBand> {
    tone.eq.iter_mut().filter(|b| b.used).nth(n)
}

/// What a compressor face shows for `c`, by address.
fn comp_values(ns: &str, c: crate::tone::Comp) -> Vec<(String, f64)> {
    let at = |param: &str| format!("{ns}/{param}");
    match ns {
        "fet" => {
            let ratio = FET_RATIOS
                .iter()
                .enumerate()
                .min_by(|(_, a), (_, b)| (*a - c.ratio).abs().total_cmp(&(*b - c.ratio).abs()))
                .map_or(0, |(i, _)| i);
            vec![
                // The input drives into a fixed threshold: the lower the
                // threshold, the further the input is turned up.
                (
                    at("input"),
                    f64::from(c.threshold.mul_add(-1.2, -48.0)).clamp(-48.0, 0.0),
                ),
                (at("output"), -24.0),
                (at("attack"), dial(c.attack, 0.1, 100.0)),
                (at("release"), dial(c.release, 50.0, 1100.0)),
                (at("ratio"), crate::num::coord(ratio) / 3.0),
                (at("on"), 1.0),
            ]
        }
        "opto" => vec![
            (at("peak_reduction"), frac(-c.threshold, 0.0, 40.0) * 100.0),
            (at("gain"), 50.0),
            (at("limit"), if c.ratio > 6.0 { 1.0 } else { 0.0 }),
            (at("power"), 1.0),
        ],
        _ => vec![
            (at("threshold"), f64::from(c.threshold)),
            (at("ratio"), f64::from(c.ratio)),
            (at("output"), 0.0),
        ],
    }
}

/// A compressor face's edit `param` = `value`, onto `c`; whether it was
/// one of its controls.
fn comp_edit(ns: &str, param: &str, c: &mut crate::tone::Comp, value: f64) -> bool {
    let v = crate::num::narrow(value);
    match (ns, param) {
        ("fet", "input") => c.threshold = ((-48.0 - v) / 1.2).clamp(-60.0, 0.0),
        ("fet", "attack") => c.attack = undial(value, 0.1, 100.0),
        ("fet", "release") => c.release = undial(value, 50.0, 1100.0),
        ("fet", "ratio") => {
            let at = crate::num::index((value.clamp(0.0, 1.0) * 3.0).round());
            c.ratio = FET_RATIOS.get(at).copied().unwrap_or(4.0);
        }
        ("opto", "peak_reduction") => c.threshold = -span(value / 100.0, 0.0, 40.0),
        ("opto", "limit") => c.ratio = if value >= 0.5 { 10.0 } else { 3.0 },
        ("vcacomp", "threshold") => c.threshold = v.clamp(-60.0, 0.0),
        ("vcacomp", "ratio") => c.ratio = v.clamp(1.0, 60.0),
        _ => return false,
    }
    true
}

/// The second compressor's face, after the first (`Tone::comp_after`),
/// and its size.
#[must_use]
pub fn face_after(tone: &Tone) -> Option<(crate::faces::FrameFace, (f64, f64))> {
    // The pair's sidechain EQ fills both faces' column.
    if tone.sc_view.is_some_and(|of| of.panel() == Which::Comp) && size_of(SC_FACE).is_some() {
        return None;
    }
    let (comp, face) = tone.comp_after?;
    let design = face.design();
    let shaped = shaped(design, "-lane");
    let size = size_of(&shaped)?;
    Some((
        crate::faces::FrameFace {
            responsive: false,
            design: shaped,
            values: comp_values(ns_of(design), comp),
            texts: Vec::new(),
        },
        size,
    ))
}

/// An edit the second compressor's face made.
pub fn edit_after(tone: &mut Tone, addr: &str, value: f64) -> bool {
    let Some((ns, param)) = addr.split_once('/') else {
        return false;
    };
    if param == "sc_view" {
        if value >= 0.5 {
            tone.sc_view = Some(crate::tone::Sidechain::After);
        }
        return true;
    }
    tone.comp_after
        .as_mut()
        .is_some_and(|(comp, _)| comp_edit(ns, param, comp, value))
}

/// What a device's header calls it: the face it wears, where it wears
/// one.
#[must_use]
pub fn title(which: Which, tone: &Tone) -> String {
    match which {
        Which::Comp if tone.comp_after.is_some() => {
            let after = tone.comp_after.map_or("", |(_, f)| f.title());
            format!("{} → {after}", tone.comp_face.title())
        }
        Which::Comp => tone.comp_face.title().to_owned(),
        Which::Sat => saturate_profiles::PROFILES
            .get(tone.sat_profile)
            .map_or_else(|| "Saturation".to_owned(), |p| p.name.to_owned()),
        Which::Mod => format!("{:?}", tone.modulation.engine),
        Which::Reverb => tone.reverb.algorithm.name().to_owned(),
        _ => which.name().to_owned(),
    }
}

/// The values `design` shows for `which` over `tone`, by address.
fn values(design: &str, which: Which, tone: &Tone) -> Vec<(String, f64)> {
    let ns = ns_of(design);
    let at = |param: &str| format!("{ns}/{param}");
    let pct = |v: f32| f64::from(v.clamp(0.0, 1.0)) * 100.0;
    match ns {
        "eq" => eq_values(tone.bands_ref(which)),
        "fet" | "opto" | "vcacomp" => comp_values(ns, comp_of(tone, which)),
        "consoleeq" => {
            let mut out = vec![
                (at("eq_in"), 1.0),
                (at("filters_in"), 0.0),
                (at("hpf"), 16.0),
                (at("lpf"), 22_000.0),
            ];
            for (n, (gain, freq, q)) in [
                ("lf_gain", "lf_freq", None),
                ("lmf_gain", "lmf_freq", Some("lmf_q")),
                ("hmf_gain", "hmf_freq", Some("hmf_q")),
                ("hf_gain", "hf_freq", None),
            ]
            .iter()
            .enumerate()
            {
                if let Some(b) = band(tone, n) {
                    out.push((at(gain), f64::from(b.gain)));
                    out.push((at(freq), f64::from(b.frequency)));
                    if let Some(q) = q {
                        out.push((at(q), f64::from(b.q)));
                    }
                }
            }
            out
        }
        "tube" | "tape_sat" | "transformer" | "transistor" | "digital_sat" => {
            let scale = saturate_profiles::PROFILES
                .get(tone.sat_profile)
                .map_or(1.0, |p| p.voicing.drive_scale)
                .max(f32::EPSILON);
            let drive = ((tone.sat.drive - 1.0) / (15.0 * scale)).clamp(0.0, 1.0);
            let mut out = vec![
                (at("drive"), pct(drive)),
                (at("mix"), pct(tone.sat.mix)),
                (at("output"), 0.0),
                (
                    at("bias"),
                    f64::from(tone.sat.q_point.clamp(-1.0, 1.0)) * 50.0,
                ),
                (at("power"), 1.0),
                (at("on"), 1.0),
            ];
            out.extend(emphasis_values(ns, tone));
            out
        }
        "chorus" | "flanger" | "vibrato" | "tremolo" => {
            let w = tone.modulation;
            vec![
                (at("rate"), f64::from(w.rate)),
                (at("depth"), pct(w.depth)),
                (at("mix"), pct(w.mix)),
                (at("on"), 1.0),
            ]
        }
        _ => Vec::new(),
    }
}

/// The plugin face `which` wears over `tone`, and its size.
#[must_use]
pub fn face(which: Which, tone: &Tone) -> Option<(crate::faces::FrameFace, (f64, f64))> {
    // A compressor whose sidechain EQ is open is that EQ.
    if let Some(sc) = tone
        .sc_view
        .filter(|of| of.panel() == which)
        .and_then(|of| sc_face(tone, of))
    {
        return Some(sc);
    }
    let base = design(which, tone)?;
    // A compressor wears the shape designed for where it stands: one of
    // a stacked pair (`-lane`), or alone filling the column (`-tall`); a
    // saturator its `-tall` — the plugin's own face where those are not
    // designed yet.
    let shaped = match which {
        Which::Comp | Which::RescueComp => {
            let paired = which == Which::Comp && tone.comp_after.is_some();
            shaped(base, if paired { "-lane" } else { "-tall" })
        }
        // A saturator stands alone in its column, the compressors' size:
        // its 960-wide face fitted to the lane's height ran half as wide
        // again as they do.
        Which::Sat => shaped(base, "-tall"),
        _ => base.to_owned(),
    };
    let size = size_of(&shaped)?;
    let eq = base == EQ_FACE;
    Some((
        crate::faces::FrameFace {
            // The graph fills its box, whatever the box's proportions.
            responsive: eq,
            design: shaped,
            values: values(base, which, tone),
            texts: if eq {
                eq_texts(tone.bands_ref(which))
            } else {
                Vec::new()
            },
        },
        size,
    ))
}

/// Apply an edit a plugin face made on `which` to `tone`; whether it
/// was one this adapter knows.
pub fn edit(which: Which, tone: &mut Tone, addr: &str, value: f64) -> bool {
    let Some((ns, param)) = addr.split_once('/') else {
        return false;
    };
    let v = crate::num::narrow(value);
    match (ns, param) {
        ("eq", p) => return eq_edit(which, tone, p, value),
        // The sidechain EQ: its SC key closes it, its bands are its
        // compressor's.
        ("sc", p) => {
            let Some(of) = tone.sc_view else {
                return false;
            };
            if p == "sc_view" {
                if value < 0.5 {
                    tone.sc_view = None;
                }
                return true;
            }
            return tone
                .sidechain
                .get_mut(of as usize)
                .is_some_and(|bands| bands_edit(bands, p, value));
        }
        ("fet" | "opto" | "vcacomp", _) => {
            // The second compressor's face speaks its own controls: an
            // edit in its namespace (and not the first's) is its.
            let after = tone.comp_after.map(|(_, f)| ns_of(f.design()));
            if which == Which::Comp && after == Some(ns) && ns_of(tone.comp_face.design()) != ns {
                return edit_after(tone, addr, value);
            }
            // Its SC key opens its sidechain EQ in the column.
            if param == "sc_view" {
                if value >= 0.5 {
                    tone.sc_view = Some(if which == Which::RescueComp {
                        crate::tone::Sidechain::Rescue
                    } else {
                        crate::tone::Sidechain::Main
                    });
                }
                return true;
            }
            return comp_edit(ns, param, comp_mut(tone, which), value);
        }
        ("consoleeq", p) => {
            let (n, field) = match p {
                "lf_gain" => (0, 'g'),
                "lf_freq" => (0, 'f'),
                "lmf_gain" => (1, 'g'),
                "lmf_freq" => (1, 'f'),
                "lmf_q" => (1, 'q'),
                "hmf_gain" => (2, 'g'),
                "hmf_freq" => (2, 'f'),
                "hmf_q" => (2, 'q'),
                "hf_gain" => (3, 'g'),
                "hf_freq" => (3, 'f'),
                _ => return false,
            };
            let Some(b) = band_mut(tone, n) else {
                return false;
            };
            match field {
                'g' => b.gain = v,
                'f' => b.frequency = v,
                _ => b.q = v,
            }
        }
        ("tube" | "tape_sat" | "transformer" | "transistor" | "digital_sat", p)
            if p == "emphasis_view" || p.starts_with("emph/") =>
        {
            return emphasis_edit(tone, p, value);
        }
        ("tube" | "tape_sat" | "transformer" | "transistor" | "digital_sat", "drive") => {
            let scale = saturate_profiles::PROFILES
                .get(tone.sat_profile)
                .map_or(1.0, |p| p.voicing.drive_scale);
            tone.sat.drive = (15.0 * scale).mul_add(span(value / 100.0, 0.0, 1.0), 1.0);
        }
        ("tube" | "tape_sat" | "transformer" | "transistor" | "digital_sat", "mix") => {
            tone.sat.mix = span(value / 100.0, 0.0, 1.0);
        }
        ("tube" | "tape_sat" | "transformer" | "transistor" | "digital_sat", "bias") => {
            tone.sat.q_point = (v / 50.0).clamp(-1.0, 1.0);
        }
        ("chorus" | "flanger" | "vibrato" | "tremolo", "rate") => {
            tone.modulation.rate = v.clamp(0.05, 20.0);
        }
        ("chorus" | "flanger" | "vibrato" | "tremolo", "depth") => {
            tone.modulation.depth = span(value / 100.0, 0.0, 1.0);
        }
        ("chorus" | "flanger" | "vibrato" | "tremolo", "mix") => {
            tone.modulation.mix = span(value / 100.0, 0.0, 1.0);
        }
        _ => return false,
    }
    true
}

#[cfg(test)]
mod eq_face_tests {
    use super::*;

    fn value(values: &[(String, f64)], addr: &str) -> Option<f64> {
        values.iter().find(|(a, _)| a == addr).map(|(_, v)| *v)
    }

    /// The EQ panels wear frame's EQ graph, each band at its own slot,
    /// and what the graph edits comes back to the band.
    #[test]
    fn the_eq_graph_shows_and_edits_the_bands() {
        let mut tone = crate::tone::placeholder(0);
        let (face, _) = face(Which::Eq, &tone).expect("the EQ wears a face");
        assert_eq!(face.design, EQ_FACE);
        let first = tone
            .eq
            .iter()
            .find(|b| b.used)
            .cloned()
            .expect("a used band");
        let slot = first.index + 1;
        assert_eq!(
            value(&face.values, &format!("eq/b{slot}_freq")),
            Some(f64::from(first.frequency))
        );
        assert_eq!(value(&face.values, &format!("eq/b{slot}_used")), Some(1.0));

        // A drag on the graph moves the band.
        assert!(edit(
            Which::Eq,
            &mut tone,
            &format!("eq/b{slot}_gain"),
            -4.5
        ));
        let moved = tone
            .eq
            .iter()
            .find(|b| b.index == first.index)
            .expect("still there");
        assert!((moved.gain + 4.5).abs() < 1e-6);
        // A shape is its position over the ten: Low Cut is the fourth.
        assert!(edit(
            Which::Eq,
            &mut tone,
            &format!("eq/b{slot}_shape"),
            3.0 / 9.0
        ));
        let shaped = tone
            .eq
            .iter()
            .find(|b| b.index == first.index)
            .expect("still there");
        assert_eq!(shaped.shape, eq_ui::eq_graph_model::EqBandShape::LowCut);

        // A band added on the graph's last slot is made there.
        assert!(edit(Which::Eq, &mut tone, "eq/b24_used", 1.0));
        assert!(edit(Which::Eq, &mut tone, "eq/b24_freq", 8000.0));
        let added = tone.eq.iter().find(|b| b.index == 23).expect("made");
        assert!(added.used && (added.frequency - 8000.0).abs() < 1e-3);
        // Negative control: a slot past the graph's is no band's.
        assert!(!edit(Which::Eq, &mut tone, "eq/b25_used", 1.0));
    }

    /// A compressor's SC key opens its sidechain EQ; the pair's second
    /// opens its own; the EQ's bands are that compressor's, and its SC
    /// key closes it.
    #[test]
    fn the_sc_key_opens_the_compressors_sidechain_eq() {
        use crate::tone::Sidechain;
        let mut tone = crate::tone::placeholder(0);
        tone.comp_face = CompFace::Fet;
        tone.comp_after = Some((tone.comp, CompFace::Opto));
        assert!(edit(Which::Comp, &mut tone, "opto/sc_view", 1.0));
        assert_eq!(tone.sc_view, Some(Sidechain::After));
        assert!(edit(Which::Comp, &mut tone, "sc/b1_used", 1.0));
        assert!(edit(Which::Comp, &mut tone, "sc/b1_freq", 120.0));
        let band = tone.sidechain[Sidechain::After as usize]
            .iter()
            .find(|b| b.index == 0)
            .expect("made on the second's");
        assert!((band.frequency - 120.0).abs() < 1e-3);
        assert!(tone.sidechain[Sidechain::Main as usize].is_empty());
        assert!(edit(Which::Comp, &mut tone, "sc/sc_view", 0.0));
        assert_eq!(tone.sc_view, None);
        assert!(edit(Which::Comp, &mut tone, "fet/sc_view", 1.0));
        assert_eq!(tone.sc_view, Some(Sidechain::Main));
        // Negative control: closed, the sidechain EQ takes no edits.
        tone.sc_view = None;
        assert!(!edit(Which::Comp, &mut tone, "sc/b1_gain", 3.0));
    }

    /// A saturator's face shows its emphasis view and bands, and what it
    /// edits there comes back to the tone.
    #[test]
    fn the_saturator_face_carries_the_emphasis_eq() {
        let mut tone = crate::tone::placeholder(0);
        let (face, _) = face(Which::Sat, &tone).expect("the saturator wears a face");
        let ns = ns_of(design(Which::Sat, &tone).expect("a design"));
        assert_eq!(
            value(&face.values, &format!("{ns}/emphasis_view")),
            Some(0.0)
        );
        assert_eq!(
            value(&face.values, &format!("{ns}/emph/b6_freq")),
            Some(10_000.0)
        );

        assert!(edit(
            Which::Sat,
            &mut tone,
            &format!("{ns}/emphasis_view"),
            1.0
        ));
        assert!(tone.sat_emphasis_view);
        assert!(edit(
            Which::Sat,
            &mut tone,
            &format!("{ns}/emph/b3_gain"),
            6.0
        ));
        assert!(edit(
            Which::Sat,
            &mut tone,
            &format!("{ns}/emph/b3_shape"),
            1.0
        ));
        assert!((tone.sat_emphasis[2].gain_db - 6.0).abs() < 1e-6);
        assert_eq!(
            tone.sat_emphasis[2].shape,
            saturate_dsp::emphasis::EmphShape::HighShelf
        );
        // Negative control: there is no seventh band.
        assert!(!edit(
            Which::Sat,
            &mut tone,
            &format!("{ns}/emph/b7_gain"),
            6.0
        ));
    }
}

#[cfg(test)]
mod machine_tests {
    use crate::tone::{ReverbMachine, Role, placeholder_for};

    /// The voices' reverbs run in the 480; a drum's does not; until frame
    /// has the 480's face, the algorithm's own is worn.
    #[test]
    fn the_vocal_reverbs_run_in_the_480() {
        let ancestors = |p: &[&str]| p.iter().map(|s| (*s).to_owned()).collect::<Vec<_>>();
        let vocal = placeholder_for(Role::Reverb, 0, "Hall", &ancestors(&["Vocals", "Vox FX"]));
        assert_eq!(vocal.reverb_machine, Some(ReverbMachine::Digital480));
        let choir = placeholder_for(Role::Reverb, 0, "Plate", &ancestors(&["Choir"]));
        assert_eq!(choir.reverb_machine, Some(ReverbMachine::Digital480));
        // Negative control.
        let snare = placeholder_for(
            Role::Reverb,
            0,
            "Snare Plate",
            &ancestors(&["Drum Kit", "Snare"]),
        );
        assert_eq!(snare.reverb_machine, None);
        let face = crate::faces::of(crate::tone::Which::Reverb, &vocal).expect("a face");
        assert!(
            face.name.contains("96-verb-480"),
            "the vocal reverb wears the 480: {}",
            face.name
        );
        let snare_face = crate::faces::of(crate::tone::Which::Reverb, &snare).expect("a face");
        assert!(!snare_face.name.contains("480"), "{}", snare_face.name);
    }
}
