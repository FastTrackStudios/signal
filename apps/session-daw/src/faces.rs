//! Devices with faces of their own: the effect's painted picture over a
//! row of its controls.
//!
//! The rack draws a reverb as its tail and a delay as its repeats, at a
//! strip's width. In the FX view a device has an editor's room, and the
//! effect's own editor already paints what it does — `reverb_ui`'s
//! halls, plates, springs and rooms, `delay_ui`'s tape, analog and
//! digital repeats — so the device draws that, from this track's
//! settings, with the controls that move it underneath.

use anyrender::{PaintScene, Scene};
use vello::kurbo::{Affine, Rect};

use crate::arrangement::Palette;
use crate::macros::{Macro, frac, span};
use crate::text::Font;
use crate::tone::{Panel, Tone, Which};

/// What a face paints.
#[derive(Clone)]
pub enum Picture {
    Reverb(reverb_ui::viz::ReverbView),
    Delay(delay_ui::viz::DelayView),
    Mod(modulation_ui::viz::ModView),
    Sat(saturate_ui::viz::SatView),
    /// A face designed in frame, running live: its controls are its own.
    Frame(FrameFace),
}

/// A frame face: which design (its path under the faces directory,
/// without the extension), and what it shows — values by address in
/// their own units, and its text.
#[derive(Clone, PartialEq)]
pub struct FrameFace {
    pub design: String,
    pub values: Vec<(String, f64)>,
    pub texts: Vec<(String, String)>,
    /// Laid out for its box's proportions (frame's responsive mode)
    /// rather than fitted whole: a lone compressor fills the column a
    /// pair would share.
    pub responsive: bool,
}

/// Where frame's faces are: `FTS_FRAME_DIR`, else frame's generated
/// faces in the frame checkout beside this one — as signal finds them.
pub(crate) fn design_dir() -> std::path::PathBuf {
    std::env::var_os("FTS_FRAME_DIR").map_or_else(
        || {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../../../frame/examples/plugins")
        },
        Into::into,
    )
}

/// A reverb's decay as the Time knob: log seconds over 0.3–20 s.
fn time_knob(seconds: f32) -> f64 {
    (f64::from(seconds.max(0.3)) / 0.3).log(20.0_f64 / 0.3)
}

/// …and back.
fn seconds_of(knob: f64) -> f32 {
    crate::num::narrow(0.3 * (20.0_f64 / 0.3).powf(knob.clamp(0.0, 1.0)))
}

/// The engine's decay knob (0..1) that rings for `room`'s decay on its
/// algorithm — its own law, inverted — or the Time knob's log reading
/// for an algorithm whose decay is not a calibrated time.
fn decay_knob(room: &crate::tone::Room) -> f64 {
    let law = |d: f64| reverb_dsp::algorithm::decay_seconds(room.algorithm, 0, d);
    let want = f64::from(room.decay);
    if law(0.5).is_none() {
        return time_knob(room.decay);
    }
    let (mut lo, mut hi) = (0.0_f64, 1.0_f64);
    for _ in 0..40 {
        let mid = 0.5 * (lo + hi);
        if law(mid).is_some_and(|s| s < want) {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    0.5 * (lo + hi)
}

/// …and the seconds the engine's decay knob `d` rings for.
fn decay_seconds(room: &crate::tone::Room, d: f64) -> f32 {
    reverb_dsp::algorithm::decay_seconds(room.algorithm, 0, d.clamp(0.0, 1.0))
        .map_or_else(|| seconds_of(d), crate::num::narrow)
}

/// A delay's time as the Time knob: log ms over 20–2500.
fn delay_knob(ms: f32) -> f64 {
    (f64::from(ms.max(20.0)) / 20.0).log(125.0)
}

/// The preset `tone` last loaded, or `fallback`.
fn preset_name(tone: &Tone, fallback: &str) -> String {
    tone.preset
        .and_then(|at| tone.presets.get(at))
        .map_or_else(|| fallback.to_owned(), |p| p.name.clone())
}

/// The frame face frame designed for `which`'s machine — a reverb's
/// algorithm, a delay's style — and its back.
fn time_entry(which: Which, tone: &Tone) -> Option<&'static crate::rig::FaceEntry> {
    let faces = crate::rig::faces();
    match which {
        // A machine wears its own face, whatever program it runs.
        Which::Reverb => tone
            .reverb_machine
            .and_then(|m| faces.reverb(m.face()))
            .or_else(|| faces.reverb(tone.reverb.algorithm.name())),
        Which::Delay => faces.delay(
            crate::rig::DELAY_ALGOS
                .get(tone.delay.style.to_index())
                .copied()
                .unwrap_or(""),
        ),
        _ => None,
    }
}

/// The algorithm as a machine's display spells a program.
fn program_name(algorithm: reverb_dsp::algorithm::AlgorithmType) -> String {
    use reverb_dsp::algorithm::AlgorithmType as A;
    match algorithm {
        A::Room => "SMALL ROOM".to_owned(),
        A::Hall => "LARGE HALL".to_owned(),
        A::Random => "RANDOM HALL".to_owned(),
        other => other.name().to_uppercase(),
    }
}

/// What `entry`'s face (its back, with `back`) shows for `which` over
/// `tone`: values by address in their own units, and its text.
fn time_face(which: Which, tone: &Tone, entry: &crate::rig::FaceEntry, back: bool) -> FrameFace {
    let ns = &entry.ns;
    let at = |param: &str| format!("{ns}/{param}");
    let beat = crate::beats::beat(crate::beats::BPM);
    let (values, texts) = if which == Which::Reverb {
        let room = &tone.reverb;
        let free = || {
            (
                time_knob(room.decay),
                0.0,
                crate::beats::seconds_text(f64::from(room.decay)),
            )
        };
        let (knob, sync, text) = room.sync.map_or_else(free, |div| {
            (
                crate::num::coord(div.saturating_sub(1)) / 6.0,
                1.0,
                format!(
                    "{} · {}",
                    crate::beats::REVERB.get(div).map_or("", |d| d.0),
                    crate::beats::seconds_text(f64::from(room.decay))
                ),
            )
        });
        (
            vec![
                (at("time_knob"), knob),
                (at("sync"), sync),
                // The engine's own decay knob: what a face's picture
                // moves with (the plate's damper, the hall's arches).
                (at("decay"), decay_knob(room)),
                (
                    at("algorithm"),
                    crate::num::coord(
                        reverb_dsp::algorithm::AlgorithmType::ALL
                            .iter()
                            .position(|a| *a == room.algorithm)
                            .unwrap_or(0),
                    ),
                ),
                (at("tone"), f64::from(tone.reverb_extra.tone)),
                (at("modulation"), f64::from(tone.reverb_extra.modulation)),
                (at("low_cut"), f64::from(tone.reverb_extra.low_cut)),
                (at("high_cut"), f64::from(tone.reverb_extra.high_cut)),
                (at("predelay"), f64::from(room.predelay)),
                (at("size"), f64::from(room.size)),
                (at("damping"), f64::from(room.damping)),
                (at("level"), 20.0 * f64::from(room.mix.max(0.001)).log10()),
                (at("on"), 1.0),
            ],
            vec![
                (at("preset"), preset_name(tone, room.algorithm.name())),
                (at("time_text"), text),
                // What a machine's display calls the algorithm it runs.
                (at("program"), program_name(room.algorithm)),
            ],
        )
    } else {
        let echo = &tone.delay;
        let (knob, sync, text) = match echo
            .sync
            .and_then(|k| crate::beats::DELAY.get(k).map(|d| (k, d)))
        {
            Some((k, (label, _, beats))) => (
                crate::num::coord(k) / 7.0,
                1.0,
                format!("{label} · {:.0} ms", beats * beat * 1000.0),
            ),
            None => (delay_knob(echo.time), 0.0, format!("{:.0} ms", echo.time)),
        };
        (
            vec![
                (at("time_knob"), knob),
                (at("sync"), sync),
                (at("time"), f64::from(echo.time)),
                (at("feedback"), f64::from(echo.feedback)),
                (at("level"), 20.0 * f64::from(echo.mix.max(0.001)).log10()),
                (at("on"), 1.0),
            ],
            vec![
                (at("preset"), preset_name(tone, "Delay")),
                (at("time_text"), text),
            ],
        )
    };
    FrameFace {
        responsive: false,
        design: crate::rig::design_of(entry, false, back),
        values,
        texts,
    }
}

/// Width over height of `which`'s frame face over `tone`, if it has
/// one: its front over its back, stacked.
#[must_use]
pub fn aspect(which: Which, tone: &Tone) -> Option<f64> {
    if let Some((face, (w, h))) = crate::plugin_faces::face(which, tone) {
        // A compressor's column is a pair's, stacked: as wide as one
        // face at half the height, whether the second is there or the
        // first fills it alone. A lane-shaped face says its own: a
        // `-lane` is half the column, a `-tall` all of it.
        let stacked = if face.design.ends_with("-tall") {
            1.0
        } else if which == Which::Comp {
            2.0
        } else {
            1.0
        };
        return Some(w / (h * stacked).max(1.0));
    }
    time_entry(which, tone).map(|entry| {
        let (w, h) = entry.size;
        let stacked = if entry.back.is_some() { 2.0 } else { 1.0 };
        w / (h * stacked).max(1.0)
    })
}

/// Apply an edit a frame face made on `which` to `tone`; whether it
/// was one of `tone`'s.
pub fn edit(which: Which, tone: &mut Tone, addr: &str, value: f64) -> bool {
    if crate::plugin_faces::edit(which, tone, addr, value) {
        return true;
    }
    let Some((_, param)) = addr.split_once('/') else {
        return false;
    };
    let v = crate::num::narrow(value);
    let gain = || crate::num::narrow(10.0_f64.powf(value / 20.0)).clamp(0.0, 1.0);
    let beat = crate::beats::beat(crate::beats::BPM);
    let pressed = value >= 0.5;
    match (which, param) {
        // The nameplate's arrows: the slot's presets, round.
        (_, "preset_prev" | "preset_next") => {
            if pressed && !tone.presets.is_empty() {
                let count = tone.presets.len();
                let at = tone.preset.unwrap_or(0);
                let step = if param == "preset_next" {
                    1
                } else {
                    count.saturating_sub(1)
                };
                let next = at.saturating_add(step).checked_rem(count).unwrap_or(0);
                tone.load_preset(next);
            }
        }
        // The algorithm button: the next machine.
        (Which::Reverb, "algo_menu") => {
            if pressed {
                let all = reverb_dsp::algorithm::AlgorithmType::ALL;
                let at = all
                    .iter()
                    .position(|a| *a == tone.reverb.algorithm)
                    .unwrap_or(0);
                if let Some(next) =
                    all.get(at.saturating_add(1).checked_rem(all.len()).unwrap_or(0))
                {
                    tone.reverb.algorithm = *next;
                }
            }
        }
        (Which::Delay, "algo_menu") => {
            if pressed {
                tone.delay.cycle_style();
            }
        }
        // Time ↔ Beat: locked to the length nearest what it plays now;
        // free again at what it was playing.
        (Which::Reverb, "sync") => {
            tone.reverb.sync = pressed.then(|| {
                let div =
                    crate::beats::nearest_reverb(f64::from(tone.reverb.decay), crate::beats::BPM);
                tone.reverb.decay =
                    crate::num::narrow(crate::beats::REVERB.get(div).map_or(1.0, |d| d.1) * beat);
                div
            });
        }
        (Which::Delay, "sync") => {
            tone.delay.sync = pressed.then(|| {
                let k = crate::beats::nearest_delay(f64::from(tone.delay.time), crate::beats::BPM);
                tone.delay.time = crate::num::narrow(
                    crate::beats::DELAY.get(k).map_or(1.0, |d| d.2) * beat * 1000.0,
                );
                k
            });
        }
        (Which::Reverb, "time_knob") if tone.reverb.sync.is_some() => {
            let div = crate::num::index((value.clamp(0.0, 1.0) * 6.0).round()).saturating_add(1);
            tone.reverb.sync = Some(div);
            tone.reverb.decay =
                crate::num::narrow(crate::beats::REVERB.get(div).map_or(1.0, |d| d.1) * beat);
        }
        (Which::Delay, "time_knob") if tone.delay.sync.is_some() => {
            let k = crate::num::index((value.clamp(0.0, 1.0) * 7.0).round()).min(7);
            tone.delay.sync = Some(k);
            tone.delay.time =
                crate::num::narrow(crate::beats::DELAY.get(k).map_or(1.0, |d| d.2) * beat * 1000.0);
        }
        (Which::Reverb, "time_knob") => tone.reverb.decay = seconds_of(value),
        // A program key: that algorithm.
        (Which::Reverb, "algorithm") => {
            if let Some(algorithm) =
                reverb_dsp::algorithm::AlgorithmType::ALL.get(crate::num::index(value.round()))
            {
                tone.reverb.algorithm = *algorithm;
            }
        }
        (Which::Reverb, "decay") => {
            tone.reverb.decay = decay_seconds(&tone.reverb, value);
            tone.reverb.sync = None;
        }
        (Which::Reverb, "predelay") => tone.reverb.predelay = v.clamp(0.0, 200.0),
        (Which::Reverb, "modulation") => tone.reverb_extra.modulation = v.clamp(0.0, 1.0),
        (Which::Reverb, "tone") => tone.reverb_extra.tone = v.clamp(-1.0, 1.0),
        (Which::Reverb, "low_cut") => tone.reverb_extra.low_cut = v.clamp(20.0, 2_000.0),
        (Which::Reverb, "high_cut") => tone.reverb_extra.high_cut = v.clamp(1_000.0, 20_000.0),
        (Which::Reverb, "size") => tone.reverb.size = v.clamp(0.0, 1.0),
        (Which::Reverb, "damping") => tone.reverb.damping = v.clamp(0.0, 1.0),
        (Which::Reverb, "level") => tone.reverb.mix = gain(),
        (Which::Delay, "time_knob") => {
            tone.delay.time = crate::num::narrow(20.0 * 125.0_f64.powf(value.clamp(0.0, 1.0)));
        }
        (Which::Delay, "time") => tone.delay.time = v.clamp(20.0, 2500.0),
        (Which::Delay, "feedback") => tone.delay.feedback = v.clamp(0.0, 0.95),
        (Which::Delay, "level") => tone.delay.mix = gain(),
        _ => return false,
    }
    true
}

/// One device's face: what it is called, its picture, its controls.
pub struct Face {
    /// The machine, as its editor names it: Plate, Hall, Tape…
    pub name: String,
    pub picture: Picture,
    pub knobs: &'static [Macro],
    /// A second frame face stacked under it — a Time face's back, its
    /// "More" panel, when it stands alone in its column.
    pub under: Option<FrameFace>,
}

/// How tall the face's name is.
const HEAD_H: f64 = 14.0;

/// Seconds per beat the pictures are measured against, until the
/// device knows the song's tempo: 120 bpm.
const BEAT: f32 = 0.5;

const REVERB_KNOBS: [Macro; 5] = [
    Macro {
        name: "Decay",
        get: |t| frac(t.reverb.decay, 0.1, 10.0),
        set: |t, f| t.reverb.decay = span(f, 0.1, 10.0),
        says: |t| format!("{:.1} s", t.reverb.decay),
    },
    Macro {
        name: "Pre",
        get: |t| frac(t.reverb.predelay, 0.0, 200.0),
        set: |t, f| t.reverb.predelay = span(f, 0.0, 200.0),
        says: |t| format!("{:.0} ms", t.reverb.predelay),
    },
    Macro {
        name: "Size",
        get: |t| f64::from(t.reverb.size.clamp(0.0, 1.0)),
        set: |t, f| t.reverb.size = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.reverb.size * 100.0),
    },
    Macro {
        name: "Damp",
        get: |t| f64::from(t.reverb.damping.clamp(0.0, 1.0)),
        set: |t, f| t.reverb.damping = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.reverb.damping * 100.0),
    },
    Macro {
        name: "Mix",
        get: |t| f64::from(t.reverb.mix.clamp(0.0, 1.0)),
        set: |t, f| t.reverb.mix = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.reverb.mix * 100.0),
    },
];

const DELAY_KNOBS: [Macro; 5] = [
    Macro {
        name: "Time",
        get: |t| frac(t.delay.time, 20.0, 2000.0),
        set: |t, f| t.delay.time = span(f, 20.0, 2000.0),
        says: |t| format!("{:.0} ms", t.delay.time),
    },
    Macro {
        name: "Repeats",
        get: |t| frac(t.delay.feedback, 0.0, 0.95),
        set: |t, f| t.delay.feedback = span(f, 0.0, 0.95),
        says: |t| format!("{:.0}%", t.delay.feedback * 100.0),
    },
    Macro {
        name: "Tone",
        get: |t| f64::from(t.delay.tone.clamp(0.0, 1.0)),
        set: |t, f| t.delay.tone = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.delay.tone * 100.0),
    },
    Macro {
        name: "Width",
        get: |t| f64::from(t.delay.width.clamp(0.0, 1.0)),
        set: |t, f| t.delay.width = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.delay.width * 100.0),
    },
    Macro {
        name: "Mix",
        get: |t| f64::from(t.delay.mix.clamp(0.0, 1.0)),
        set: |t, f| t.delay.mix = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.delay.mix * 100.0),
    },
];

const MOD_KNOBS: [Macro; 4] = [
    Macro {
        name: "Type",
        get: |t| {
            let all = modulation_ui::viz::Engine::ALL;
            let at = all
                .iter()
                .position(|e| *e == t.modulation.engine)
                .unwrap_or(0);
            crate::num::coord(at) / crate::num::coord(all.len().saturating_sub(1).max(1))
        },
        set: |t, f| {
            let all = modulation_ui::viz::Engine::ALL;
            let steps = crate::num::coord(all.len().saturating_sub(1));
            let at = crate::num::index((f.clamp(0.0, 1.0) * steps).round());
            if let Some(engine) = all.get(at) {
                t.modulation.engine = *engine;
            }
        },
        says: |t| format!("{:?}", t.modulation.engine),
    },
    Macro {
        name: "Rate",
        get: |t| frac(t.modulation.rate, 0.05, 10.0),
        set: |t, f| t.modulation.rate = span(f, 0.05, 10.0),
        says: |t| format!("{:.2} Hz", t.modulation.rate),
    },
    Macro {
        name: "Depth",
        get: |t| f64::from(t.modulation.depth.clamp(0.0, 1.0)),
        set: |t, f| t.modulation.depth = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.modulation.depth * 100.0),
    },
    Macro {
        name: "Mix",
        get: |t| f64::from(t.modulation.mix.clamp(0.0, 1.0)),
        set: |t, f| t.modulation.mix = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.modulation.mix * 100.0),
    },
];

/// The saturator's drive as its knob, 0..1: the gain back through the
/// profile's own drive scale (`Tone::set_sat_profile`'s rule).
fn drive_knob(tone: &Tone) -> f32 {
    let scale = saturate_profiles::PROFILES
        .get(tone.sat_profile)
        .map_or(1.0, |p| p.voicing.drive_scale)
        .max(f32::EPSILON);
    ((tone.sat.drive - 1.0) / (15.0 * scale)).clamp(0.0, 1.0)
}

const SAT_KNOBS: [Macro; 3] = [
    Macro {
        name: "Type",
        get: |t| {
            let last = saturate_profiles::PROFILES.len().saturating_sub(1).max(1);
            crate::num::coord(t.sat_profile) / crate::num::coord(last)
        },
        set: |t, f| {
            let last = crate::num::coord(saturate_profiles::PROFILES.len().saturating_sub(1));
            let at = crate::num::index((f.clamp(0.0, 1.0) * last).round());
            if at != t.sat_profile {
                t.set_sat_profile(at);
            }
        },
        says: |t| {
            saturate_profiles::PROFILES
                .get(t.sat_profile)
                .map_or_else(String::new, |p| p.name.to_owned())
        },
    },
    Macro {
        name: "Drive",
        get: |t| f64::from(drive_knob(t)),
        set: |t, f| {
            let scale = saturate_profiles::PROFILES
                .get(t.sat_profile)
                .map_or(1.0, |p| p.voicing.drive_scale);
            t.sat.drive = (15.0 * scale).mul_add(span(f, 0.0, 1.0), 1.0);
        },
        says: |t| format!("{:.0}%", drive_knob(t) * 100.0),
    },
    Macro {
        name: "Mix",
        get: |t| f64::from(t.sat.mix.clamp(0.0, 1.0)),
        set: |t, f| t.sat.mix = span(f, 0.0, 1.0),
        says: |t| format!("{:.0}%", t.sat.mix * 100.0),
    },
];

/// What the saturation editor's picture draws, for `tone`: its profile
/// voiced at this drive and mix.
fn sat_view(tone: &Tone) -> Option<saturate_ui::viz::SatView> {
    let profile = *saturate_profiles::PROFILES.get(tone.sat_profile)?;
    let controls = saturate_profiles::Controls {
        drive: drive_knob(tone),
        mix: tone.sat.mix,
        ..saturate_profiles::Controls::default()
    };
    Some(saturate_ui::viz::SatView::of(profile, controls))
}

/// What the modulation editor's picture draws, for `tone`.
#[must_use]
pub const fn mod_view(tone: &Tone) -> modulation_ui::viz::ModView {
    let wobble = tone.modulation;
    modulation_ui::viz::ModView {
        engine: wobble.engine,
        rate: wobble.rate,
        depth: wobble.depth,
        mix: wobble.mix,
        on: true,
        // Modulation is cyan-led and motion pink-led, as its editor has it.
        color: match wobble.engine.group() {
            modulation_ui::viz::Group::Colouring => [34, 211, 238],
            modulation_ui::viz::Group::Moving => [244, 114, 182],
        },
        time: 0.0,
    }
}

/// The modulation's vector picture in `at` — the rack's, at a strip's
/// width, where there is no GPU surface to hand it.
pub fn paint_mod_in(scene: &mut Scene, tone: &Tone, at: Panel) {
    if at.width < 2.0 || at.height < 2.0 {
        return;
    }
    let mut picture = Scene::new();
    modulation_ui::viz::paint_mod(&mut picture, &mod_view(tone), at.width, at.height);
    let place = Affine::translate((at.x, at.y));
    scene.push_clip_layer(place, &Rect::new(0.0, 0.0, at.width, at.height));
    scene.append_scene(picture, place);
    scene.pop_layer();
}

/// The repeats `tone`'s delay makes: every tap until it has faded under
/// a fiftieth, alternating sides as wide as the delay is.
fn taps(tone: &Tone) -> Vec<delay_ui::viz::Tap> {
    let echo = &tone.delay;
    let gap = (echo.time / 1000.0).max(0.001);
    let pan = echo.width.clamp(0.0, 1.0) * 0.8;
    std::iter::successors(Some((1.0_f32, 1.0_f32)), |(n, level)| {
        let next = level * echo.feedback.clamp(0.0, 0.95);
        (next > 0.02 && *n < 32.0).then_some((n + 1.0, next))
    })
    .map(|(n, level)| delay_ui::viz::Tap {
        at: gap * n,
        level,
        pan: if n % 2.0 < 1.0 { pan } else { -pan },
    })
    .collect()
}

/// `which`'s face over `tone`, if it has one.
#[must_use]
pub fn of(which: Which, tone: &Tone) -> Option<Face> {
    if let Some((mut frame, _)) = crate::plugin_faces::face(which, tone) {
        // A compressor with a second after it: the two stacked, sharing
        // the height; alone, it fills the column the pair would.
        let under = (which == Which::Comp)
            .then(|| crate::plugin_faces::face_after(tone).map(|(f, _)| f))
            .flatten();
        // Alone, a rack face is laid out for the column; a `-tall` was
        // designed for it.
        frame.responsive = frame.responsive
            || (which == Which::Comp
                && under.is_none()
                && !crate::plugin_faces::is_shaped(&frame.design));
        return Some(Face {
            name: frame.design.clone(),
            picture: Picture::Frame(frame),
            knobs: &[],
            under,
        });
    }
    match which {
        // A machine with a face of its own in frame draws that.
        // A machine frame has a face for draws that, over its back.
        // Alone, a Time face with a tall layout is that one device.
        Which::Reverb | Which::Delay
            if time_entry(which, tone).is_some_and(|e| e.tall.is_some()) =>
        {
            let entry = time_entry(which, tone)?;
            let mut frame = time_face(which, tone, entry, false);
            frame.design = crate::rig::pick_design(entry, false, false, true);
            Some(Face {
                name: entry.face.clone(),
                picture: Picture::Frame(frame),
                knobs: &[],
                under: None,
            })
        }
        Which::Reverb | Which::Delay if time_entry(which, tone).is_some() => {
            let entry = time_entry(which, tone)?;
            Some(Face {
                name: entry.face.clone(),
                picture: Picture::Frame(time_face(which, tone, entry, false)),
                knobs: &[],
                under: entry
                    .back
                    .is_some()
                    .then(|| time_face(which, tone, entry, true)),
            })
        }
        Which::Reverb => {
            let room = &tone.reverb;
            let family = room.algorithm.family();
            Some(Face {
                name: format!("{family:?}"),
                picture: Picture::Reverb(reverb_ui::viz::ReverbView {
                    decay: room.decay,
                    density: room.diffusion,
                    damp: room.damping,
                    predelay: room.predelay / 1000.0,
                    mix: room.mix,
                    on: true,
                    beat: BEAT,
                    family,
                    color: [167, 139, 250],
                    time: 0.0,
                }),
                knobs: &REVERB_KNOBS,
                under: None,
            })
        }
        Which::Delay => {
            let family = tone.delay.style.family();
            let taps = taps(tone);
            let window = taps
                .last()
                .map_or(1.0, |last| last.at + tone.delay.time / 1000.0);
            Some(Face {
                name: format!("{family:?}"),
                picture: Picture::Delay(delay_ui::viz::DelayView {
                    taps,
                    window,
                    mix: 1.0,
                    on: true,
                    beat: BEAT,
                    division: String::new(),
                    family,
                    color: [96, 165, 250],
                    time: 0.0,
                }),
                knobs: &DELAY_KNOBS,
                under: None,
            })
        }
        Which::Sat => sat_view(tone).map(|view| Face {
            name: saturate_profiles::PROFILES
                .get(tone.sat_profile)
                .map_or_else(String::new, |p| p.name.to_owned()),
            picture: Picture::Sat(view),
            knobs: &SAT_KNOBS,
            under: None,
        }),
        Which::Mod => Some(Face {
            name: format!("{:?}", tone.modulation.engine),
            picture: Picture::Mod(mod_view(tone)),
            knobs: &MOD_KNOBS,
            under: None,
        }),
        _ => None,
    }
}

/// Where a face's controls sit in `panel`: a row along its bottom.
#[must_use]
pub fn controls(panel: Panel) -> Panel {
    Panel {
        x: panel.x,
        y: panel.y + panel.height - crate::macros::CELL_H,
        width: panel.width,
        height: crate::macros::CELL_H,
    }
}

/// Draw `face`'s ground and name in `panel` — its picture goes in
/// [`picture_at`], painted by [`Painters`]; its controls in [`controls`].
pub fn chrome(scene: &mut Scene, palette: &Palette, font: &Font, face: &Face, panel: Panel) {
    let ground = Rect::new(
        panel.x,
        panel.y,
        panel.x + panel.width,
        panel.y + panel.height,
    );
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        palette.row_a,
        None,
        &ground,
    );
    crate::tcp::glyphs(
        scene,
        font,
        palette.text_dim,
        &face.name,
        panel.x + 6.0,
        panel.y + 10.5,
        8.0,
    );
}

/// Where a face's picture sits in `panel`: between its name and its
/// controls.
#[must_use]
pub fn picture_at(panel: Panel) -> Panel {
    Panel {
        x: panel.x,
        y: panel.y + HEAD_H,
        width: panel.width,
        height: (panel.height - HEAD_H - crate::macros::CELL_H).max(0.0),
    }
}

/// How often a frame face that moves by itself renders on the GPU.
#[cfg(feature = "native")]
const GPU_FPS: u32 = 30;

/// A registered texture, `width` × `height`, as a scene.
#[cfg(feature = "native")]
fn texture_fill(id: anyrender::ResourceId, width: u32, height: u32) -> Scene {
    let mut scene = Scene::new();
    scene.fill(
        vello::peniko::Fill::NonZero,
        Affine::IDENTITY,
        anyrender::Paint::Resource(vello::peniko::ImageBrush {
            image: id,
            sampler: vello::peniko::ImageSampler::default(),
        }),
        None,
        &Rect::new(0.0, 0.0, f64::from(width), f64::from(height)),
    );
    scene
}

/// The effects' own widgets, one per device, kept so each keeps its GPU
/// surface and its animation's clock: the shader where the renderer
/// hands over a device, the vector painter where it does not.
#[derive(Default)]
pub struct Painters {
    reverbs: std::collections::HashMap<
        String,
        (
            reverb_ui::viz::Shared<reverb_ui::viz::ReverbView>,
            reverb_ui::viz::ReverbWidget,
        ),
    >,
    delays: std::collections::HashMap<
        String,
        (
            delay_ui::viz::Shared<delay_ui::viz::DelayView>,
            delay_ui::viz::DelayWidget,
        ),
    >,
    frames: std::collections::HashMap<String, Live>,
    /// The first-time work this frame may still do: opening a design,
    /// attaching it to the GPU, its first render — ~30 ms a face, which a
    /// panel of sixteen paid all in its opening frame. Spread instead:
    /// each frame does what fits ([`FIRST_TIME_BUDGET`]), the faces not
    /// yet ready wait a frame or two, and the panel opens at once.
    first_time: Option<web_time::Instant>,
    /// Some face waited this frame: draw again.
    waiting: bool,
    /// This frame is its panel's first: a face shown in it is simply
    /// there; one shown later fades in.
    opening: bool,
    /// When this frame's first re-render (of a face already up) began:
    /// they stop at [`RERENDER_BUDGET`].
    rerender: Option<web_time::Instant>,
    mods: std::collections::HashMap<
        String,
        (
            modulation_ui::viz::Shared<modulation_ui::viz::ModView>,
            modulation_ui::viz::ModWidget,
        ),
    >,
}

/// One frame face, open: what it was last shown, so a value it is being
/// dragged through is not pushed back at it every frame.
pub struct Live {
    pub surface: frame_live::LiveSurface,
    shown: Option<FrameFace>,
    /// Its GPU path, once tried: the texture the renderer knows (and
    /// the size it was registered at), or `Failed` — vectors from then.
    #[cfg(feature = "native")]
    gpu: Gpu,
    /// When it was last rendered on the GPU: a face is re-rendered at
    /// most [`GPU_FPS`] times a second however often its values move.
    rendered: Option<web_time::Instant>,
    /// When it was first shown, and whether it fades in — see
    /// [`Painters::opacity`].
    appeared: Option<(web_time::Instant, bool)>,
}

/// A frame face's GPU state.
#[cfg(feature = "native")]
#[derive(Default)]
enum Gpu {
    /// Not tried yet.
    #[default]
    Untried,
    /// Attached; the texture registered with the renderer, if one has
    /// been rendered.
    Attached(Option<(anyrender::ResourceId, (u32, u32))>),
    /// The renderer handed no device, or frame-live could not use it.
    Failed,
}

/// How long a face ready after its panel opened takes to fade in.
const FADE: std::time::Duration = std::time::Duration::from_millis(180);

/// How long a frame may spend re-rendering faces already up.
const RERENDER_BUDGET: std::time::Duration = std::time::Duration::from_millis(8);

/// How long a frame may spend getting faces ready for the first time.
const FIRST_TIME_BUDGET: std::time::Duration = std::time::Duration::from_millis(8);

thread_local! {
    static SHARED: std::rc::Rc<std::cell::RefCell<Painters>> = std::rc::Rc::default();
}

/// The window's painters: one set for every panel that shows faces.
///
/// A face is opened, attached to the GPU and rendered once, ~20 ms of
/// work; owned by the panel, all of it was thrown away when the panel
/// closed and paid again — sixteen faces' worth — when it opened. Shared,
/// the FX opens a second time with every face already up, and the docked
/// strip and the FX view show the same faces rather than two copies.
#[must_use]
pub fn shared() -> std::rc::Rc<std::cell::RefCell<Painters>> {
    SHARED.with(std::rc::Rc::clone)
}

impl Painters {
    /// A new frame — `opening`, its panel's first: its budget for
    /// first-time work starts now.
    pub const fn begin_frame(&mut self, opening: bool) {
        self.first_time = None;
        self.rerender = None;
        self.waiting = false;
        self.opening = opening;
    }

    /// How opaque device `key`'s face is drawn this frame.
    ///
    /// A face ready after its panel opened fades in over [`FADE`] rather
    /// than appearing whole in one frame beside ones already up: the panel
    /// fills in, it does not flicker in. One there from the first frame,
    /// or still open from the last time the panel was, is opaque.
    pub fn opacity(&mut self, key: &str) -> f32 {
        let Some((at, true)) = self.frames.get(key).and_then(|live| live.appeared) else {
            return 1.0;
        };
        let t = at.elapsed().as_secs_f32() / FADE.as_secs_f32();
        if t >= 1.0 {
            return 1.0;
        }
        self.waiting = true;
        // Smoothstep: no snap at either end.
        t * t * 2.0f32.mul_add(-t, 3.0)
    }

    /// Note that device `key`'s face has been shown, the first time.
    fn shown(&mut self, key: &str) {
        let opening = self.opening;
        if let Some(live) = self.frames.get_mut(key)
            && live.appeared.is_none()
        {
            live.appeared = Some((web_time::Instant::now(), !opening));
        }
    }

    /// Whether a face is still waiting to be got ready — the host draws
    /// again until none is.
    #[must_use]
    pub const fn waiting(&self) -> bool {
        self.waiting
    }

    /// Whether this frame may do first-time work for one more face. The
    /// first face of a frame always may (so every frame makes headway);
    /// the rest while the budget lasts.
    fn may_start(&mut self) -> bool {
        match self.first_time {
            None => {
                self.first_time = Some(web_time::Instant::now());
                true
            }
            Some(began) if began.elapsed() < FIRST_TIME_BUDGET => true,
            Some(_) => {
                self.waiting = true;
                false
            }
        }
    }

    /// Device `key`'s last picture, kept up rather than rendered again:
    /// when it was rendered within the pace — its values move every frame
    /// while a meter does, and a face re-rendered sixteen times a frame
    /// held the panel at 25 fps — or when this frame's re-renders are
    /// spent. `None` to render it now (and that render is counted).
    #[cfg(feature = "native")]
    fn hold_frame(&mut self, key: &str, face: &FrameFace, size: (u32, u32)) -> Option<Scene> {
        // Its values brought up to date first, which is what says whether
        // it has anything new to show.
        self.live(key, face)?;
        let live = self.frames.get_mut(key)?;
        let Gpu::Attached(Some((id, at))) = live.gpu else {
            return None;
        };
        if at != size || !live.surface.needs_redraw() {
            return None;
        }
        let pace = std::time::Duration::from_secs_f64(1.0 / f64::from(GPU_FPS));
        let paced = live.rendered.is_some_and(|at| at.elapsed() < pace);
        let spent = self
            .rerender
            .is_some_and(|began| began.elapsed() >= RERENDER_BUDGET);
        if paced || spent {
            self.waiting = true;
            return Some(texture_fill(id, size.0, size.1));
        }
        self.rerender.get_or_insert_with(web_time::Instant::now);
        None
    }

    /// Whether device `key`'s face has first-time work still to do.
    #[cfg(feature = "native")]
    fn fresh(&self, key: &str) -> bool {
        self.frames
            .get(key)
            .is_none_or(|live| !matches!(live.gpu, Gpu::Attached(Some(_)) | Gpu::Failed))
    }

    /// Device `key`'s frame face, open, for the pointer.
    pub fn frame_mut(&mut self, key: &str) -> Option<&mut frame_live::LiveSurface> {
        self.frames.get_mut(key).map(|live| &mut live.surface)
    }

    /// `face`, `width` × `height` device pixels at `scale`, as device
    /// `key`'s live surface paints it — opened the first time.
    fn paint_frame(
        &mut self,
        key: &str,
        face: &FrameFace,
        (width, height): (u32, u32),
        scale: f64,
    ) -> Scene {
        let mut scene = Scene::new();
        if let Some(live) = self.live(key, face) {
            live.surface.paint_vectors(&mut scene, width, height, scale);
        }
        scene
    }

    /// `face` as device `key`'s live surface renders it on the GPU — its
    /// effects stage too (the EQ's glow), which the vector path leaves
    /// out — into a texture the renderer composites. The vector path
    /// where the renderer hands over no device.
    #[cfg(feature = "native")]
    fn paint_frame_gpu(
        &mut self,
        ctx: &mut dyn anyrender::RenderContext,
        key: &str,
        face: &FrameFace,
        (width, height): (u32, u32),
        scale: f64,
    ) -> Scene {
        let Some(live) = self.live(key, face) else {
            return Scene::new();
        };
        if matches!(live.gpu, Gpu::Untried) {
            let handle = ctx
                .renderer_specific_context()
                .and_then(|any| any.downcast::<wgpu_context_07::DeviceHandle>().ok());
            live.gpu = match handle {
                Some(h) => match live.surface.attach_gpu(h.device.clone(), h.queue.clone()) {
                    Ok(()) => {
                        // A face that moves by itself (a scope, a reverb's
                        // field, a needle settling) is paced; one that does
                        // not renders only when it changes.
                        live.surface.set_max_fps(GPU_FPS);
                        Gpu::Attached(None)
                    }
                    Err(error) => {
                        tracing::warn!(target: "frame", face = %face.design, %error, "frame face: no GPU, vectors");
                        Gpu::Failed
                    }
                },
                None => Gpu::Failed,
            };
        }
        let Gpu::Attached(registered) = &mut live.gpu else {
            let mut scene = Scene::new();
            live.surface.paint_vectors(&mut scene, width, height, scale);
            return scene;
        };
        // Nothing new and the texture the right size: the last render is
        // still the face — composite it again, render nothing.
        if let Some((id, size)) = *registered
            && size == (width, height)
            && !live.surface.needs_redraw()
        {
            return texture_fill(id, width, height);
        }
        live.rendered = Some(web_time::Instant::now());

        let (texture, fresh) = match live.surface.render(width, height, scale) {
            Ok(rendered) => rendered,
            Err(error) => {
                tracing::warn!(target: "frame", face = %face.design, %error, "frame face: GPU render failed, vectors");
                live.gpu = Gpu::Failed;
                let mut scene = Scene::new();
                live.surface.paint_vectors(&mut scene, width, height, scale);
                return scene;
            }
        };
        // A new texture (a new size) is registered again, the old one
        // given back; the same one is re-rendered in place.
        if fresh || registered.is_none_or(|(_, size)| size != (width, height)) {
            if let Some((old, _)) = registered.take() {
                ctx.unregister_resource(old);
            }
            match ctx.try_register_custom_resource(Box::new(texture)) {
                Ok(id) => *registered = Some((id, (width, height))),
                Err(_) => {
                    live.gpu = Gpu::Failed;
                    let mut scene = Scene::new();
                    live.surface.paint_vectors(&mut scene, width, height, scale);
                    return scene;
                }
            }
        }
        registered.map_or_else(Scene::new, |(id, _)| texture_fill(id, width, height))
    }

    /// Device `key`'s live surface, opened the first time and shown
    /// `face`'s values — `None` where the design does not open.
    fn live(&mut self, key: &str, face: &FrameFace) -> Option<&mut Live> {
        if !self.frames.contains_key(key) {
            let path = design_dir().join(&face.design);
            let dir = path
                .parent()
                .map(std::path::Path::to_path_buf)
                .unwrap_or_default();
            let stem = path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default();
            let (markup, catalog) = frame_live::face_paths(&dir, &stem);
            match frame_live::LiveSurface::open(&markup, catalog) {
                Ok(mut surface) => {
                    surface.set_animate(true);
                    self.frames.insert(
                        key.to_owned(),
                        Live {
                            surface,
                            shown: None,
                            #[cfg(feature = "native")]
                            gpu: Gpu::Untried,
                            rendered: None,
                            appeared: None,
                        },
                    );
                }
                Err(error) => {
                    tracing::warn!(target: "frame", face = %face.design, %error, "frame face did not open");
                    return None;
                }
            }
        }
        let live = self.frames.get_mut(key)?;
        if live.shown.as_ref() != Some(face) {
            live.surface.set_responsive(face.responsive);
            live.surface.apply_real(face.values.clone());
            live.surface.apply_texts(face.texts.clone());
            live.shown = Some(face.clone());
        }
        live.surface.poll_reload();
        Some(live)
    }

    /// `picture`, `width` × `height` device pixels, as device `key`'s
    /// widget paints it — the vector painter when there is no `ctx`.
    pub fn paint(
        &mut self,
        ctx: Option<&mut dyn anyrender::RenderContext>,
        key: &str,
        picture: &Picture,
        (width, height, scale): (u32, u32, f64),
    ) -> Scene {
        if let Picture::Frame(face) = picture {
            #[cfg(feature = "native")]
            if let Some(ctx) = ctx {
                // Not ready yet and the frame's budget spent: next frame.
                if self.fresh(key) && !self.may_start() {
                    return Scene::new();
                }
                if let Some(held) = self.hold_frame(key, face, (width, height)) {
                    return held;
                }
                let scene = self.paint_frame_gpu(ctx, key, face, (width, height), scale);
                self.shown(key);
                return scene;
            }
            if !self.frames.contains_key(key) && !self.may_start() {
                return Scene::new();
            }
            let scene = self.paint_frame(key, face, (width, height), scale);
            self.shown(key);
            return scene;
        }
        let Some(ctx) = ctx else {
            let mut scene = Scene::new();
            let (w, h) = (f64::from(width), f64::from(height));
            match picture {
                Picture::Reverb(view) => reverb_ui::viz::paint_reverb(&mut scene, view, w, h),
                Picture::Delay(view) => delay_ui::viz::paint_delay(&mut scene, view, w, h),
                Picture::Mod(view) => modulation_ui::viz::paint_mod(&mut scene, view, w, h),
                Picture::Sat(view) => saturate_ui::viz::paint_sat(&mut scene, view, w, h),
                Picture::Frame(_) => {}
            }
            return scene;
        };
        match picture {
            Picture::Reverb(view) => {
                let (shared, widget) = self.reverbs.entry(key.to_owned()).or_insert_with(|| {
                    let shared = std::rc::Rc::new(std::cell::RefCell::new(view.clone()));
                    let mut widget = reverb_ui::viz::ReverbWidget::new(std::rc::Rc::clone(&shared));
                    widget.create_surfaces(ctx);
                    (shared, widget)
                });
                shared.replace(view.clone());
                widget.paint_frame(ctx, width, height, 1.0)
            }
            Picture::Delay(view) => {
                let (shared, widget) = self.delays.entry(key.to_owned()).or_insert_with(|| {
                    let shared = std::rc::Rc::new(std::cell::RefCell::new(view.clone()));
                    let mut widget = delay_ui::viz::DelayWidget::new(std::rc::Rc::clone(&shared));
                    widget.create_surfaces(ctx);
                    (shared, widget)
                });
                shared.replace(view.clone());
                widget.paint_frame(ctx, width, height, 1.0)
            }
            // No widget of its own: its picture is measured, not
            // animated, so the vector painter is the whole of it.
            Picture::Frame(_) => Scene::new(),
            Picture::Sat(view) => {
                let mut scene = Scene::new();
                let (w, h) = (f64::from(width), f64::from(height));
                saturate_ui::viz::paint_sat(&mut scene, view, w, h);
                scene
            }
            Picture::Mod(view) => {
                let (shared, widget) = self.mods.entry(key.to_owned()).or_insert_with(|| {
                    let shared = std::rc::Rc::new(std::cell::RefCell::new(view.clone()));
                    let mut widget =
                        modulation_ui::viz::ModWidget::new(std::rc::Rc::clone(&shared));
                    widget.create_surfaces(ctx);
                    (shared, widget)
                });
                shared.replace(view.clone());
                widget.paint_frame(ctx, width, height, 1.0)
            }
        }
    }
}

#[cfg(test)]
mod fade_tests {
    use super::{FrameFace, Painters, Picture};

    fn face() -> Picture {
        Picture::Frame(FrameFace {
            design: "comp/01-fet-black".to_owned(),
            values: Vec::new(),
            texts: Vec::new(),
            responsive: false,
        })
    }

    /// A face shown in its panel's first frame is simply there; one shown
    /// later fades in, and keeps the host drawing until it has; a face
    /// still open from before is there at once when the panel reopens.
    #[test]
    fn only_a_face_that_arrives_late_fades_in() {
        let mut painters = Painters::default();
        painters.begin_frame(true);
        let _ = painters.paint(None, "early", &face(), (120, 80, 1.0));
        assert!((painters.opacity("early") - 1.0).abs() < f32::EPSILON);
        assert!(!painters.waiting(), "nothing fading in the opening frame");

        painters.begin_frame(false);
        let _ = painters.paint(None, "late", &face(), (120, 80, 1.0));
        let alpha = painters.opacity("late");
        assert!(alpha < 0.5, "a late face starts faint, got {alpha}");
        assert!(painters.waiting(), "the host draws again while it fades");

        // Reopened: both were shown before, neither fades again.
        painters.begin_frame(true);
        let _ = painters.paint(None, "early", &face(), (120, 80, 1.0));
        assert!((painters.opacity("early") - 1.0).abs() < f32::EPSILON);
        std::thread::sleep(super::FADE);
        assert!(
            (painters.opacity("late") - 1.0).abs() < f32::EPSILON,
            "faded in by now"
        );
    }
}
