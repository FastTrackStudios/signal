//! The full **Nord Stage 4** signal routing, built as a placeholder composition
//! tree ([`crate::rig_node`]). Every block is a placeholder (no DSP yet) — this
//! locks the *routing* per `docs/nord-stage-4-signal-routing.md`; the `Native`
//! DSP for each block type gets implemented one at a time afterwards.
//!
//! Shape:
//! ```text
//! Preset "Nord Stage"
//! ├─ Engines (parallel)
//! │  ├─ Engine Organ   → Voices(parallel: A,B) → shared Organ FX
//! │  ├─ Engine Keys    → Voices(parallel: A,B)  each: source → FX
//! │  └─ Engine Synth   → Voices(parallel: A,B,C) each: osc→filter→amp → FX
//! └─ Global → Rotary (one shared, last)
//! ```
//! Per-layer FX = the 6 stages: Mod1 · Mod2 · Delay · Amp/EQ · Comp · Reverb
//! (Comp is a bare block; the rest are modules). Synth layers carry the
//! modulators (3 envelopes, LFO, vibrato, arp).

use signal_proto::block::BlockType;

use crate::rig_node::{Container, RigNode};

const ROTARY: &str = "Rotary";

// ── Per-layer FX chain (the 6 stages) ────────────────────────────────────────

/// Mod 1 — modulation family (one mode active): A-Pan / Trem / RM / A-Wah / Wah
/// / Pump. Placeholder = a Tremolo block.
fn mod1() -> Container {
    Container::module("Mod 1").block(BlockType::Trem, "Tremolo")
}

/// Mod 2 — modulation family: Phaser / Flanger / Vibe / Chorus / Ensemble / Spin.
fn mod2() -> Container {
    Container::module("Mod 2").block(BlockType::Chorus, "Chorus")
}

/// Delay — a multi-block module: the delay + a feedback-FX sub-module + a
/// feedback filter (nested modules).
fn delay() -> Container {
    Container::module("Delay")
        .block(BlockType::Delay, "Delay")
        .add(Container::module("Feedback FX").block(BlockType::Chorus, "FB Mod"))
        .block(BlockType::Filter, "FB Filter")
}

/// Amp Sim / EQ — amp model + 3-band EQ + resonant LP24/HP24 filter (+ Drive).
/// Carries the `To Rotary` send (any layer can route to the global Rotary).
fn amp_eq() -> Container {
    Container::module("Amp/EQ")
        .block(BlockType::Amp, "Amp Model")
        .block(BlockType::Eq, "3-Band EQ")
        .block(BlockType::Filter, "LP24/HP24")
        .send(ROTARY, "To Rotary")
}

/// Reverb — swappable algorithm (Spring..Cath) + variation/chorale + bright/dark.
fn reverb() -> Container {
    Container::module("Reverb").block(BlockType::Reverb, "Reverb")
}

/// The ordered per-layer FX tail: Mod1 → Mod2 → Delay → Amp/EQ → Comp(block) →
/// Reverb. (Rotary is reached by the Amp/EQ send, not part of the chain.)
fn fx_chain() -> Vec<RigNode> {
    vec![
        mod1().into(),
        mod2().into(),
        delay().into(),
        amp_eq().into(),
        RigNode::Block {
            block: crate::rig::RigBlock::of_type(BlockType::Compressor).named("Comp"),
        },
        reverb().into(),
    ]
}

// ── Engines ──────────────────────────────────────────────────────────────────

/// Organ — 2 layers (Tonewheel source each) that **share one FX chain** living
/// at the engine level (the flexible-tree case). Routes to Rotary by default.
fn organ_engine() -> Container {
    Container::engine("Organ")
        // engine-level menu (B3 system params)
        .param("tonewheel_mode", "Vintage 1")
        .param("click_level", "Normal")
        .param("trigger_point", "High")
        .add(
            Container::parallel("Organ Voices")
                .add(organ_layer("Organ A"))
                .add(organ_layer("Organ B")),
        )
        .add(Container::module("Organ FX").extend(fx_chain()))
}

fn organ_layer(name: &str) -> Container {
    Container::layer(name)
        .param("model", "B3") // B3 | B3Bass | Vox | Farfisa | Pipe1 | Pipe2
        .param("octave", "0")
        .param("vc", "Off") // V1..V3 | C1..C3 | Off
        .param("percussion", "Off")
        .add(Container::module("Organ Source").block(BlockType::Tonewheel, name))
}

/// Keys (piano) — 2 layers, each a self-contained Sampler source → its own FX.
fn keys_engine() -> Container {
    Container::engine("Keys").add(
        Container::parallel("Keys Voices")
            .add(keys_layer("Keys A", None))
            .add(keys_layer("Keys B", None)),
    )
}

/// `sample_spec` = a library spec path realizing the Sampler
/// ([`BlockImpl::Sample`](crate::rig::BlockImpl)); `None` keeps the placeholder
/// (structure only, no DSP).
fn keys_layer(name: &str, sample_spec: Option<String>) -> Container {
    let source = match sample_spec {
        Some(spec) => Container::module("Piano Source").sample_block("Piano", spec),
        None => Container::module("Piano Source").block(BlockType::Sampler, "Piano"),
    };
    Container::layer(name)
        .param("octave", "0")
        .add(source)
        .extend(fx_chain())
}

// ── Sample-realized Keys (machine-local libraries) ───────────────────────────

/// Root of the local Keyscape extraction — per-instrument dirs each holding a
/// `library.styx` + FLAC samples. Override with `FTS_KEYSCAPE_ROOT`.
const KEYSCAPE_ROOT: &str = "/run/media/AudioHaven/Sampled/Keys/Keyscape";

/// Spec path for one Keyscape instrument, if the local extraction has it.
fn keyscape_spec(instrument: &str) -> Option<String> {
    let root = std::env::var("FTS_KEYSCAPE_ROOT").unwrap_or_else(|_| KEYSCAPE_ROOT.into());
    let p = std::path::Path::new(&root)
        .join(instrument)
        .join("library.styx");
    p.exists().then(|| p.to_string_lossy().into_owned())
}

/// The Nord Stage program with its Keys layers realized by real Keyscape
/// libraries (Layer A = LA Custom C7 Grand, Layer B = Classic Rhodes).
/// `None` on machines without the extraction — callers register it
/// conditionally so [`nord_stage_preset`] stays deterministic for tests.
pub fn nord_stage_piano_preset() -> Option<Container> {
    let grand = keyscape_spec("LA Custom C7 Grand")?;
    let rhodes = keyscape_spec("Rhodes - Classic");
    let keys = Container::engine("Keys").add(
        Container::parallel("Keys Voices")
            .add(keys_layer("Keys A", Some(grand)))
            .add(keys_layer("Keys B", rhodes)),
    );
    Some(build_program("Nord Stage (Piano)", keys))
}

/// Synth — 3 layers, each the full voice (Osc → Filter → Amp) + FX, with the
/// control-rate modulators attached.
fn synth_engine() -> Container {
    Container::engine("Synth").add(
        Container::parallel("Synth Voices")
            .add(synth_layer("Synth A"))
            .add(synth_layer("Synth B"))
            .add(synth_layer("Synth C")),
    )
}

fn synth_layer(name: &str) -> Container {
    Container::layer(name)
        // per-layer settings (not blocks)
        .param("octave", "0")
        .param("voice_mode", "Poly") // Poly | Mono | Legato
        .param("unison", "Off") // Off | 1 | 2 | 3
        .param("glide", "0")
        // voice: oscillator → filter → amp
        .add(
            Container::module("Osc")
                .block(BlockType::Oscillator, "Oscillator")
                .block(BlockType::Unison, "Unison"),
        )
        .add(Container::module("Filter").block(BlockType::Filter, "Filter"))
        .add(Container::module("Amp").block(BlockType::Amp, "Amp"))
        // per-layer FX
        .extend(fx_chain())
        // control-rate modulators (routing axis — drive params, not audio)
        .modulator(BlockType::Envelope, "Osc Env")
        .modulator(BlockType::Envelope, "Filter Env")
        .modulator(BlockType::Envelope, "Amp Env")
        .modulator(BlockType::Lfo, "LFO")
        .modulator(BlockType::Lfo, "Vibrato")
        .modulator(BlockType::Arpeggiator, "Arp")
        // The classic NS4 voice routes (design doc §6/§7). Depths are
        // placeholder-musical until per-block param import lands.
        .route("Filter Env", "Filter.cutoff", -0.45)
        .route("LFO", "Filter.cutoff", -0.2)
}

// ── The Program ──────────────────────────────────────────────────────────────

/// The complete Nord Stage 4 program as a placeholder routing tree. Inspect with
/// [`Container::dump`].
pub fn nord_stage_preset() -> Container {
    build_program("Nord Stage", keys_engine())
}

/// The program shell shared by the placeholder and sample-realized variants:
/// same Organ/Synth engines and global tail, parameterized Keys engine.
fn build_program(name: &str, keys: Container) -> Container {
    Container::preset(name)
        .add(
            Container::parallel("Engines")
                .add(organ_engine())
                .add(keys)
                .add(synth_engine()),
        )
        // Global tail. The Global-mode Delay/Comp/Reverb instances live here when
        // a layer promotes that effect to Global (Shift+On); the single shared
        // Rotary is always last. All disabled by default (per-layer FX is the norm).
        .add(
            Container::module("Global")
                .param("delay_scope", "PerLayer") // PerLayer | Global
                .param("comp_scope", "PerLayer")
                .param("reverb_scope", "PerLayer")
                .add(
                    Container::module("Global Delay")
                        .param("enabled", "false")
                        .block(BlockType::Delay, "Global Delay"),
                )
                .add(
                    Container::module("Global Comp")
                        .param("enabled", "false")
                        .block(BlockType::Compressor, "Global Comp"),
                )
                .add(
                    Container::module("Global Reverb")
                        .param("enabled", "false")
                        .block(BlockType::Reverb, "Global Reverb"),
                )
                .block(BlockType::Rotary, ROTARY),
        )
}

/// A small, fully-traceable **layering demo** preset showing the central-MIDI
/// router: a keyboard split (left-hand bass / right-hand) plus an Omnisphere-
/// style **velocity crossfade** on the right hand (a soft pad fading into a
/// bright lead). Demonstrates **nested zones** — the right-hand key split holds
/// two velocity-windowed layers, so gains multiply (key × velocity).
pub fn layering_demo() -> Container {
    Container::preset("Layering Demo").add(
        Container::parallel("Keys")
            // Left hand: bass below the split, with a short key crossfade.
            .add(
                Container::layer("Bass")
                    .keys(0, 59)
                    .key_xfade(3)
                    .block(BlockType::Oscillator, "Bass Osc"),
            )
            // Right hand: above the split, a velocity crossfade between two voices.
            .add(
                Container::parallel("Right Hand")
                    .keys(60, 127)
                    .key_xfade(3)
                    .add(
                        Container::layer("Soft Pad")
                            .velocity(1, 90)
                            .vel_xfade(40)
                            .block(BlockType::Oscillator, "Pad Osc"),
                    )
                    .add(
                        Container::layer("Bright Lead")
                            .velocity(50, 127)
                            .vel_xfade(40)
                            .block(BlockType::Oscillator, "Lead Osc"),
                    ),
            ),
    )
}

/// The **City Grand** physically-modeled piano across the full 88 keys — a
/// single `Harmonic` (modal synthesis) native block. The voice loads its
/// measured parameter table at runtime (see `native::modal`).
pub fn city_grand_preset() -> Container {
    Container::preset("City Grand").add(
        Container::layer("Grand Piano")
            .keys(21, 108)
            .block(BlockType::Harmonic, "City Grand"),
    )
}

/// City Wurli — the vendored physically-modeled Wurlitzer 200A
/// (`openwurli-dsp`) on the Formant block. The engine's native range is MIDI
/// 33–96; the layer spans the full keyboard and the engine clamps internally.
pub fn city_wurli_preset() -> Container {
    Container::preset("City Wurli").add(
        Container::layer("Wurli")
            .keys(21, 108)
            .block(BlockType::Formant, "City Wurli"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig_node::Role;

    #[test]
    fn city_grand_preset_spans_the_keyboard() {
        let p = city_grand_preset();
        assert_eq!(p.name, "City Grand");
        let layer = p.find("Grand Piano").unwrap();
        assert_eq!((layer.zone.key_lo, layer.zone.key_hi), (21, 108));
    }

    #[test]
    fn layering_demo_has_nested_split_and_velocity_zones() {
        let p = layering_demo();
        assert_eq!(
            p.of_role(Role::Layer).len(),
            3,
            "Bass + Soft Pad + Bright Lead"
        );
        // Left-hand key split.
        let bass = p.find("Bass").unwrap();
        assert_eq!((bass.zone.key_lo, bass.zone.key_hi), (0, 59));
        // Right hand is a key-split container holding velocity-windowed layers.
        let rh = p.find("Right Hand").unwrap();
        assert_eq!(rh.zone.key_lo, 60);
        let pad = p.find("Soft Pad").unwrap();
        assert_eq!(
            (pad.zone.vel_lo, pad.zone.vel_hi, pad.zone.vel_xfade),
            (1, 90, 40)
        );
        let lead = p.find("Bright Lead").unwrap();
        assert_eq!(lead.zone.vel_lo, 50);
    }

    #[test]
    fn nord_stage_has_the_full_layer_topology() {
        let p = nord_stage_preset();
        // 3 engines, 7 sound layers.
        assert_eq!(p.of_role(Role::Engine).len(), 3);
        assert_eq!(p.of_role(Role::Layer).len(), 7);
        assert!(p.find("Organ").is_some());
        assert!(p.find("Keys").is_some());
        assert!(p.find("Synth").is_some());
        assert!(p.find("Synth C").is_some());
    }

    #[test]
    fn sources_and_global_rotary_present_as_placeholders() {
        let p = nord_stage_preset();
        let types: Vec<&str> = p.blocks().iter().map(|b| b.block_type_tag()).collect();
        assert!(types.contains(&"tonewheel"), "organ source");
        assert!(types.contains(&"sampler"), "piano source");
        assert!(types.contains(&"oscillator"), "synth source");
        assert!(types.contains(&"rotary"), "global rotary");
        // Oscillator / Filter / Amp have Native DSP today; everything else is
        // still a placeholder. (More blocks gain a backend as DSP lands.)
        for b in p.blocks() {
            let expected = matches!(b.block_type_tag(), "oscillator" | "filter" | "amp");
            assert_eq!(
                b.has_backend(),
                expected,
                "{} backend expectation",
                b.block_type_tag()
            );
        }
    }

    #[test]
    fn organ_fx_is_shared_at_engine_level() {
        let p = nord_stage_preset();
        let organ = p.find("Organ").unwrap();
        // The Organ engine holds its parallel Voices AND the shared FX module.
        assert!(organ.find("Organ FX").is_some());
        assert!(organ.find("Organ Voices").is_some());
        // Keys/Synth engines have NO engine-level FX module (FX is per-layer).
        assert!(p.find("Keys").unwrap().find("Organ FX").is_none());
    }

    #[test]
    fn synth_layers_carry_modulators_and_to_rotary_sends() {
        let p = nord_stage_preset();
        let synth = p.find("Synth").unwrap();
        // 3 envelopes + 2 LFOs + 1 arp per synth layer × 3 layers = 18.
        assert_eq!(synth.modulators_recursive().len(), 18);
        // Every layer's Amp/EQ has a To-Rotary send; many across the tree.
        let to_rotary = p
            .sends_recursive()
            .into_iter()
            .filter(|(_, s)| s.target == "Rotary")
            .count();
        assert!(
            to_rotary >= 6,
            "each FX chain routes To Rotary, got {to_rotary}"
        );
    }

    #[test]
    fn dump_renders_the_routing() {
        let p = nord_stage_preset();
        let dump = p.dump();
        assert!(dump.contains("Preset \"Nord Stage\""));
        assert!(dump.contains("Engine \"Organ\""));
        assert!(dump.contains("Block tonewheel \"Organ A\" (placeholder)"));
        assert!(dump.contains("To Rotary→Rotary"));
        // Uncomment to eyeball the full routing:
        // println!("{dump}");
    }
}
