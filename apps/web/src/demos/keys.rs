//! Keys rig demo — the **real** `signal-keys-ui` mixer, showing Worship.
//!
//! Like the guitar demo, this is the shipped component rather than a
//! picture of it: [`ControlView`] from `signal-keys-ui`, the same mixer
//! the desktop and browser remotes render.
//!
//! # How it runs with no engine
//!
//! `use_keys_state` says it outright — it returns the command client as an
//! `Option`, and "`None` when the host provided no context — the views
//! then render read-only". So the keys UI is *designed* to render without
//! a rig behind it. The demo supplies a [`KeysMixer`] and no client.
//!
//! # Why these names
//!
//! The mixer below is the shape of the actual Worship profile
//! (`features/rigs/keys/src/profile.rs::worship_profile`): four engines —
//! Keys, Pad, Organ, Bass, plus Aux — with the lanes and patches that
//! profile really defines, down to the fader trims (`Pad` at −7.1 dB,
//! `Shimmer` at −10.5). It is the UI and the *layout* of a real rig with
//! none of its content: no samples, no packs, nothing loaded. `live` is
//! false on every lane for exactly that reason, so the mixer draws the
//! lanes as present-but-not-sounding rather than claiming a library is
//! resident.
//!
//! Reading the profile crate directly would be better still, but
//! `signal-keys` is the backend — it pulls the sampler and the audio host,
//! neither of which belongs in a landing page. The names are duplicated
//! here deliberately, and if the profile changes this drifts; that is the
//! trade for not shipping an engine to draw a mixer.

use std::time::Duration;

use dioxus::prelude::*;
use signal_keys_ui::proto::{
    KeysEngineModel, KeysEnv, KeysLayerModel, KeysMacro, KeysMeter, KeysMixer, KeysModule,
    KeysNode, KeysPerform, KeysStatus,
};
use signal_keys_ui::{ControlView, KeysViewState};

/// (engine, gain dB, [(lane, patch, gain dB)]) — the Worship profile's
/// mixer shape.
/// One lane of the Worship profile.
///
/// The first four fields are the profile's own — name, module A's patch,
/// the authored fader position, and module B where `extra_modules` gives
/// one. A layer holds four modules (Omnisphere's Quadzone) and `patch` is
/// module A's; the profile only fills B on four of these lanes.
///
/// The last two are NOT in the profile. Cutoff and resonance live in the
/// Omnisphere patch, which is content this demo deliberately does not load
/// — so they are plausible values chosen per sound (a pad filtered low, a
/// piano open) purely so the mixer's filter overlay has a curve to draw.
/// Everything else on screen is the profile's.
struct LaneSpec {
    name: &'static str,
    patch: &'static str,
    module_b: &'static str,
    gain_db: f32,
    cutoff_hz: f32,
    resonance: f32,
}

const fn lane(
    name: &'static str,
    patch: &'static str,
    module_b: &'static str,
    gain_db: f32,
    cutoff_hz: f32,
    resonance: f32,
) -> LaneSpec {
    LaneSpec {
        name,
        patch,
        module_b,
        gain_db,
        cutoff_hz,
        resonance,
    }
}

type EngineSpec = (&'static str, f32, &'static [LaneSpec]);

const WORSHIP: &[EngineSpec] = &[
    (
        "Keys",
        0.0,
        &[
            lane("Keys 1", "The Grandeur - Piano", "", 0.0, 18_000.0, 0.10),
            lane("Keys 2", "Double Felt Grand", "", -2.5, 12_000.0, 0.14),
            // The profile's third lane is genuinely empty. Kept, because an
            // empty lane is part of what the rig looks like.
            lane("Keys 3", "", "", 0.0, 0.0, 0.0),
        ],
    ),
    (
        "Pad",
        0.0,
        &[
            lane(
                "Pad",
                "OB-8 PWM Big Strings",
                "Prophet 5 Classic",
                -7.1,
                2_400.0,
                0.32,
            ),
            lane(
                "Shimmer",
                "Choir Men Ohs - mf",
                "Choir Women Oos - mf",
                -10.5,
                6_800.0,
                0.18,
            ),
        ],
    ),
    (
        "Organ",
        0.0,
        &[
            lane("Organ A", "", "", 0.0, 0.0, 0.0),
            lane("Organ B", "", "", 0.0, 0.0, 0.0),
        ],
    ),
    // The one User patch in the rig — it exists nowhere else, per the
    // profile's own comment.
    (
        "Bass",
        0.0,
        &[lane("Bass", "Worship PHAT Bass", "", 0.0, 1_100.0, 0.45)],
    ),
    (
        "Aux",
        0.0,
        &[
            lane(
                "Synth 1",
                "Dolceola ^ RR Lite",
                "Clavichord a ^ RR",
                -9.9,
                4_200.0,
                0.26,
            ),
            lane("Synth 2", "Big Berthas Lead", "", -9.4, 3_100.0, 0.30),
        ],
    ),
];

/// Module A, and module B where the profile defines one.
fn modules_for(l: &LaneSpec) -> Vec<KeysModule> {
    if l.patch.is_empty() {
        return Vec::new();
    }
    let make = |index: u32, slot: &str, patch: &str, gain_db: f32, cutoff: f32, resonance: f32| {
        KeysModule {
            index,
            slot: slot.into(),
            patch: patch.into(),
            // `rig_curves` draws one filter and envelope curve per LIVE module,
            // and silently skips the rest — a module left dead renders the
            // Filter and Envelope panels with their knobs but an empty graph.
            live: true,
            gain_db,
            enabled: true,
            // Module B is the slower sound underneath: a pad under a piano.
            // Giving both modules the same envelope draws one curve twice and
            // hides the point, which is that a lane is several sounds stacked.
            amp_env: KeysEnv {
                attack_ms: if index == 0 { 8.0 } else { 180.0 },
                decay_ms: if index == 0 { 420.0 } else { 900.0 },
                sustain: 0.72,
                release_ms: if index == 0 { 640.0 } else { 1_400.0 },
            },
            filter_env: KeysEnv {
                attack_ms: if index == 0 { 24.0 } else { 320.0 },
                decay_ms: 900.0,
                sustain: 0.45,
                release_ms: 800.0,
            },
            cutoff_hz: cutoff,
            resonance: resonance,
            ..KeysModule::default()
        }
    };
    let mut modules = vec![make(0, "A", l.patch, 0.0, l.cutoff_hz, l.resonance)];
    if !l.module_b.is_empty() {
        // Module B sits under A in the profile's stacked patches: trimmed,
        // and filtered differently. Giving it A's filter would draw the same
        // curve twice and hide the thing worth showing — that a lane is
        // several sounds with their own filters stacked.
        modules.push(make(
            1,
            "B",
            l.module_b,
            -3.0,
            l.cutoff_hz * 0.45,
            (l.resonance + 0.12).min(1.0),
        ));
    }
    modules
}

/// Build the Worship mixer.
fn worship_mixer() -> KeysMixer {
    KeysMixer {
        profile: "Worship".into(),
        master_db: 0.0,
        engines: WORSHIP
            .iter()
            .map(|(engine, gain_db, lanes)| KeysEngineModel {
                name: (*engine).into(),
                drone: None,
                gain_db: *gain_db,
                muted: false,
                layers: lanes
                    .iter()
                    .map(|l| KeysLayerModel {
                        name: l.name.into(),
                        engine: (*engine).into(),
                        patch: l.patch.into(),
                        preset: l.patch.into(),
                        gain_db: l.gain_db,
                        muted: false,
                        soloed: false,
                        // Loaded. `rig_curves` draws a filter and envelope
                        // curve per LIVE module, so a lane marked dead shows
                        // the mixer with an empty overlay — which is the rig
                        // before you press play, not the rig.
                        live: !l.patch.is_empty(),
                        key_lo: 0,
                        key_hi: 127,
                        // Without modules the mixer's module strip, filter
                        // overlay and envelope graphs render as an empty
                        // band — which is what the first version showed.
                        modules: modules_for(l),
                        ..KeysLayerModel::default()
                    })
                    .collect(),
            })
            .collect(),
    }
}

/// The macro panels the mixer draws under the engine band: Filter, Filter
/// Envelope, Amp Envelope, Unison, Vibrato.
///
/// These are the engine's macro groups, not the profile's — the profile
/// names lanes and patches; the macro values live in the Omnisphere patch,
/// which is content this demo does not load. The numbers are a plausible
/// pad-ish setting so each panel draws a real control rather than an empty
/// box. Group strings must match the engine's panel names exactly or the
/// macro lands in no panel at all — note the envelope panels are titled
/// "Filter Envelope" / "Amp Envelope" but keyed "Filter Env" / "Amp Env",
/// which is what left both of them empty the first time.
///
/// (id, name, group, value, min, max, unit)
type MacroSpec = (
    &'static str,
    &'static str,
    &'static str,
    f32,
    f32,
    f32,
    &'static str,
);

const MACROS: &[MacroSpec] = &[
    (
        "filter.cutoff",
        "Cutoff",
        "Filter",
        2_400.0,
        20.0,
        20_000.0,
        "Hz",
    ),
    ("filter.reso", "Resonance", "Filter", 0.32, 0.0, 1.0, ""),
    ("filter.drive", "Drive", "Filter", 0.18, 0.0, 1.0, ""),
    (
        "fenv.attack",
        "Attack",
        "Filter Env",
        24.0,
        0.0,
        4_000.0,
        "ms",
    ),
    (
        "fenv.decay",
        "Decay",
        "Filter Env",
        900.0,
        0.0,
        8_000.0,
        "ms",
    ),
    ("fenv.sustain", "Sustain", "Filter Env", 0.45, 0.0, 1.0, ""),
    (
        "fenv.release",
        "Release",
        "Filter Env",
        800.0,
        0.0,
        8_000.0,
        "ms",
    ),
    ("aenv.attack", "Attack", "Amp Env", 8.0, 0.0, 4_000.0, "ms"),
    ("aenv.decay", "Decay", "Amp Env", 420.0, 0.0, 8_000.0, "ms"),
    ("aenv.sustain", "Sustain", "Amp Env", 0.72, 0.0, 1.0, ""),
    (
        "aenv.release",
        "Release",
        "Amp Env",
        640.0,
        0.0,
        8_000.0,
        "ms",
    ),
    ("unison.voices", "Voices", "Unison", 4.0, 1.0, 8.0, ""),
    ("unison.detune", "Detune", "Unison", 0.22, 0.0, 1.0, ""),
    ("vibrato.rate", "Rate", "Vibrato", 5.2, 0.1, 12.0, "Hz"),
    ("vibrato.depth", "Depth", "Vibrato", 0.14, 0.0, 1.0, ""),
];

/// The macro band's contents, for a surface with no rig behind it.
fn worship_macros() -> Vec<KeysMacro> {
    MACROS
        .iter()
        .map(|(id, name, group, value, min, max, unit)| KeysMacro {
            id: (*id).into(),
            name: (*name).into(),
            group: (*group).into(),
            value: *value,
            min: *min,
            max: *max,
            unit: (*unit).into(),
            // These reach real DSP in the shipped rig; nothing is dimmed.
            live: true,
            ..KeysMacro::default()
        })
        .collect()
}

/// A four-chord progression, as MIDI notes. One bar each.
///
/// I–V–vi–IV in F, voiced with the root down where a bass lane would sit
/// and the triad in the middle of the keyboard — roughly how the Worship
/// profile's key window is actually played.
const PROGRESSION: &[&[u8]] = &[
    &[41, 60, 65, 69, 72], // F
    &[36, 60, 64, 67, 72], // C
    &[45, 60, 64, 69, 72], // Dm
    &[46, 62, 65, 70, 74], // Bb
];

/// Drive the demo the way a played rig would: notes under the hands, and
/// meters that answer them.
///
/// The mixer itself never changes — faders do not move on their own — so
/// only `held` and `status.meters` are written. 20 Hz: meters read as
/// continuous, and the keybed only changes on a chord boundary anyway.
fn play_feed(mut held: Signal<Vec<u8>>, mut state: KeysViewState) {
    spawn(async move {
        let mut frame: u32 = 0;
        let mut last_bar = u32::MAX;
        let mut chords = PROGRESSION.iter().copied().cycle();
        loop {
            architect::platform::sleep(Duration::from_millis(50)).await;
            frame = frame.wrapping_add(1);
            // 96 BPM, one chord per bar: 2.5 s a chord at 20 Hz = 50 frames.
            let bar = frame.wrapping_div(50);
            let into_bar = f32::from(u16::try_from(frame % 50).unwrap_or(0)) / 50.0;

            // Stepped from a cycling iterator kept across ticks rather than
            // indexed per frame: no slice index to panic on and no
            // remainder to overflow. `cycle` over a non-empty slice is
            // endless, so `next` is always Some.
            if bar != last_bar {
                last_bar = bar;
                if let Some(chord) = chords.next() {
                    held.set(chord.to_vec());
                }
            }

            // Each chord is struck and decays. Lanes answer at their own
            // rate: a pad swells where a piano is already fading, which is
            // the thing worth showing about a layered rig.
            let strike = (-into_bar * 3.4).exp();
            let swell = (into_bar * 2.2).min(1.0);
            let meter = |lane: &str, kind: &str, level: f32| KeysMeter {
                kind: kind.into(),
                name: lane.into(),
                peak: level.clamp(0.0, 1.0),
            };

            let mut meters = Vec::new();
            for (engine, _, lanes) in WORSHIP {
                let shape = match *engine {
                    // Pads and strings swell; keys and bass are struck.
                    "Pad" => swell * 0.55,
                    "Organ" => 0.0,
                    "Aux" => swell * 0.34,
                    _ => strike * 0.78,
                };
                meters.push(meter(engine, "engine", shape));
                for l in *lanes {
                    if l.patch.is_empty() {
                        continue;
                    }
                    // Trim each lane by its own fader so the meters agree
                    // with the mixer they sit under.
                    let trim = 10.0f32.powf(l.gain_db / 20.0);
                    meters.push(meter(l.name, "layer", shape * trim));
                }
            }
            state.status.write().meters = meters;
        }
    });
}

#[component]
pub fn KeysDemo() -> Element {
    let mixer = use_hook(worship_mixer);

    // The mixer is passed as a prop, but the stacked filter/envelope overlay
    // does NOT read the prop — `rig_curves` and `rig_time_fx` read the mixer
    // out of a `KeysViewState` in context, because on a live rig that is the
    // signal the event stream folds into. With no context they return empty
    // and the overlay draws nothing, which is why the filter block was blank.
    //
    // So the demo provides the same state a rig would, holding the same
    // mixer. No client goes with it: `use_keys_state`'s contract is that the
    // views render read-only when the command client is absent, and that is
    // still what happens here.
    let held = use_signal(|| {
        PROGRESSION
            .first()
            .map(|c| (*c).to_vec())
            .unwrap_or_default()
    });
    let state = use_context_provider(|| KeysViewState {
        status: Signal::new(KeysStatus::default()),
        presets: Signal::new(Vec::new()),
        tree: Signal::new(KeysNode::default()),
        midi: Signal::new(Vec::new()),
        mixer: Signal::new(worship_mixer()),
        perform: Signal::new(KeysPerform::default()),
    });

    // Started once per mount, like the guitar demo's feed.
    use_hook(|| play_feed(held, state.clone()));

    rsx! {
        // `inert` for the same reason as the guitar demo: with no client in
        // context a fader drag would move locally and reach nothing, which
        // reads as broken rather than as a demo.
        div { class: "sg-demo sg-demo-live sg-demo-keys", inert: true, aria_hidden: "true",
            ControlView { mixer, held: held(), macro_seed: worship_macros() }
        }
    }
}
