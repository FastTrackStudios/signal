//! The full **Omnisphere 3** per-Part signal chain, built as a composition
//! tree ([`crate::rig_node`]) — the routing experiment companion to
//! [`crate::nord`]. If the engine can express both the Nord Stage 4 and the
//! Omnisphere 3 routing, there aren't many synth architectures it can't.
//!
//! Source: the official Omnisphere 3 Reference Guide (v3.0.2c). One Part:
//!
//! ```text
//! Preset "Omnisphere"
//! ├─ Quadzone (parallel)             ← the v3 layer grid (Fader/Notes/Velo)
//! │  ├─ Layer A..D, each:
//! │  │  ├─ Oscillator module (serial, all pre-filter; UI tab order):
//! │  │  │    Soundsource|Wavetable → Unison → Harmonia (4 extra oscs) →
//! │  │  │    FM → Ring Mod → Dual Freq Shifter → Waveshaper → Granular
//! │  │  ├─ Filters module — 2 filters, SERIES (or parallel), 70 types each
//! │  │  ├─ Amp module
//! │  │  ├─ Layer FX rack (4 slots, Independent mode)
//! │  │  ├─ → send to the Part Aux rack
//! │  │  └─ modulators: Amp Env, Filter Env, Mod Env (4 of each per Part,
//! │  │       one per layer here), MSEG-capable
//! │  ├─ …
//! ├─ Common FX rack (4 slots, insert)  → Part output
//! ├─ Aux FX rack (4 slots, send/return)
//! ├─ Master FX rack (4 slots — Multi-level in Omnisphere; kept in the
//! │    preset tail here, like the Nord global tail)
//! └─ Part modulators: 8 LFOs, 48-slot Mod Matrix, Arpeggiator (32-step)
//! ```
//!
//! Everything is a placeholder until its DSP lands, except Sampler blocks —
//! [`omnisphere_soundsource_preset`] realizes Layer A/B with real extracted
//! Omnisphere soundsources ([`BlockImpl::Sample`](crate::rig::BlockImpl)).
//! The Multi level (8 Parts, mixer, 4 shared Aux racks) is a Rig concern and
//! comes later.

use signal_proto::block::BlockType;

use crate::rig_node::Container;

/// Send target for every layer's aux route (the Part's 4-slot Aux rack).
const AUX_RACK: &str = "Aux Rack";

// ── Layers ───────────────────────────────────────────────────────────────────

/// One Omnisphere Layer: oscillator stack → dual filters → amp → layer FX.
/// `soundsource` = a library spec path realizing the Sample-mode source;
/// `None` keeps a placeholder Soundsource block (structure only).
fn layer(name: &str, soundsource: Option<String>) -> Container {
    // Oscillator module — Sample or Synth (wavetable) mode source, then the
    // Oscillator-Zoom sub-modules, all pre-filter (manual UI tab order).
    let mut osc = Container::module("Oscillator");
    osc = match soundsource {
        Some(spec) => osc.sample_block("Soundsource", spec),
        None => osc.block(BlockType::Sampler, "Soundsource"),
    };
    let osc = osc
        .block(BlockType::Wavetable, "Synth Osc") // Synth mode alternative (638 wavetables)
        .block(BlockType::Unison, "Unison") // ≤8 voices
        .block(BlockType::Harmonic, "Harmonia") // 4 extra oscillators
        .block(BlockType::FmOperator, "FM")
        .block(BlockType::RingModulator, "Ring Mod")
        .block(BlockType::Dfs, "Dual Freq Shifter") // NEW v3 (serial/parallel pair)
        .block(BlockType::Waveshaper, "Waveshaper")
        .block(BlockType::Granular, "Granular"); // ≤8 grain voices, WILD mode

    Container::layer(name)
        .param("layer_level", "0")
        .param("filter_routing", "Series") // Series | Parallel
        .add(osc)
        .add(
            Container::module("Filters")
                .block(BlockType::Filter, "Filter 1") // 70 types each (v3)
                .block(BlockType::Filter, "Filter 2"),
        )
        .add(Container::module("Amp").block(BlockType::Amp, "Amp"))
        .add(fx_rack("Layer FX")) // Independent mode: one 4-slot rack per layer
        .send(AUX_RACK, "To Aux")
        // Per-layer share of the Part's 12 envelopes (4 Amp + 4 Filter + 4 Mod,
        // ADSR or Complex MSEG).
        .modulator(BlockType::Envelope, "Amp Env")
        .modulator(BlockType::Envelope, "Filter Env")
        .modulator(BlockType::MultisegEnvelope, "Mod Env")
}

/// A 4-slot FX rack — every Omnisphere rack (Layer / Common / Aux / Master)
/// is exactly 4 slots; each slot picks from the 93 internal FX units.
fn fx_rack(name: &str) -> Container {
    let mut rack = Container::module(name);
    for slot in 1..=4 {
        rack = rack.block(BlockType::Custom, format!("{name} Slot {slot}"));
    }
    rack
}

// ── The Part ─────────────────────────────────────────────────────────────────

/// Build the Part shell shared by the placeholder and realized variants.
fn build_part(name: &str, sources: [Option<String>; 4]) -> Container {
    let [a, b, c, d] = sources;
    let mut preset = Container::preset(name)
        // The v3 Quadzone layer grid: Fader (modulatable scan), Notes (splits)
        // and Velo (velocity switch) modes — Notes/Velo map onto each layer's
        // Zone; the Fader scan needs the mod matrix (control-rate, later).
        .add(
            Container::parallel("Quadzone")
                .param("mode", "Fader") // Fader | Notes | Velo
                .add(layer("Layer A", a))
                .add(layer("Layer B", b))
                .add(layer("Layer C", c))
                .add(layer("Layer D", d)),
        )
        // Insert rack on the Part output.
        .add(fx_rack("Common FX"))
        // Send/return rack (layers route here via their To-Aux sends; the
        // module name matches the send target).
        .add(fx_rack(AUX_RACK))
        // Multi-level in Omnisphere proper; kept as the preset tail here.
        .add(fx_rack("Master FX"))
        // Part-level modulators: 8 LFOs, the 48-slot Mod Matrix, the Arp.
        .modulator(BlockType::ModMatrix, "Mod Matrix")
        .modulator(BlockType::Arpeggiator, "Arp");
    for n in 1..=8 {
        preset = preset.modulator(BlockType::Lfo, format!("LFO {n}"));
    }
    preset
}

/// The complete Omnisphere 3 Part as a placeholder routing tree. Inspect with
/// [`Container::dump`].
pub fn omnisphere_preset() -> Container {
    build_part("Omnisphere", [None, None, None, None])
}

// ── Sample-realized variant (machine-local soundsource extraction) ──────────

/// Root of the local Omnisphere soundsource extraction — family dirs holding
/// per-soundsource `library.styx` dirs (multisample) or flat `<name>.styx` +
/// FLAC pairs (one-shots). Override with `FTS_OMNISPHERE_ROOT`.
pub(crate) const OMNISPHERE_ROOT: &str = "/run/media/AudioHaven/Sampled/Keys/Omnisphere";

/// Spec path for one extracted soundsource (relative to the Core
/// Soundsources dir), if present.
fn soundsource_spec(relative: &str) -> Option<String> {
    let root = std::env::var("FTS_OMNISPHERE_ROOT").unwrap_or_else(|_| OMNISPHERE_ROOT.into());
    let p = std::path::Path::new(&root)
        .join("Core Soundsources")
        .join(relative);
    p.exists().then(|| p.to_string_lossy().into_owned())
}

/// The Omnisphere Part with Layers A/B realized by real extracted
/// soundsources (Dream Piano + CS-80 Strings). `None` on machines without
/// the extraction — callers register it conditionally.
pub fn omnisphere_soundsource_preset() -> Option<Container> {
    let a = soundsource_spec("Keyboards/Dream Piano/library.styx")?;
    let b = soundsource_spec("Synth Classic/CS-80 Strings/library.styx");
    Some(build_part(
        "Omnisphere (Soundsources)",
        [Some(a), b, None, None],
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rig_node::Role;

    #[test]
    fn part_has_four_layers_with_the_full_oscillator_stack() {
        let p = omnisphere_preset();
        assert_eq!(p.of_role(Role::Layer).len(), 4, "Quadzone holds 4 layers");
        for l in ["Layer A", "Layer B", "Layer C", "Layer D"] {
            let layer = p.find(l).expect(l);
            let osc = layer.find("Oscillator").expect("oscillator module");
            let names: Vec<_> = osc.blocks().iter().map(|b| b.display_name()).collect();
            for sub in [
                "Soundsource",
                "Synth Osc",
                "Unison",
                "Harmonia",
                "FM",
                "Ring Mod",
                "Dual Freq Shifter",
                "Waveshaper",
                "Granular",
            ] {
                assert!(names.iter().any(|n| n == sub), "{l} osc stack has {sub}");
            }
            let filters = layer.find("Filters").expect("filters module");
            assert_eq!(filters.blocks().len(), 2, "dual filters");
        }
    }

    #[test]
    fn racks_are_four_slots_and_layers_send_to_aux() {
        let p = omnisphere_preset();
        for rack in ["Common FX", AUX_RACK, "Master FX"] {
            let r = p.find(rack).expect(rack);
            assert_eq!(r.blocks().len(), 4, "{rack} has 4 slots");
        }
        // Every layer carries a To-Aux send targeting the Aux rack.
        let sends = p.sends_recursive();
        let aux_sends = sends.iter().filter(|(_, s)| s.target == AUX_RACK).count();
        assert_eq!(aux_sends, 4, "each of the 4 layers sends to the aux rack");
    }

    #[test]
    fn part_modulators_match_the_manual_counts() {
        let p = omnisphere_preset();
        let lfos = p
            .modulators
            .iter()
            .filter(|m| m.block_type == BlockType::Lfo)
            .count();
        assert_eq!(lfos, 8, "8 independent LFOs");
        assert!(
            p.modulators
                .iter()
                .any(|m| m.block_type == BlockType::ModMatrix)
        );
        assert!(
            p.modulators
                .iter()
                .any(|m| m.block_type == BlockType::Arpeggiator)
        );
        // 12 envelopes: 3 per layer × 4 layers (4 Amp + 4 Filter + 4 Mod).
        let envs: usize = p
            .of_role(Role::Layer)
            .iter()
            .map(|l| l.modulators.len())
            .sum();
        assert_eq!(envs, 12, "12 envelopes across the 4 layers");
    }

    /// The placeholder Part renders silence-safely (native Filter/Amp are
    /// live pass-through-ish; everything else placeholder).
    #[test]
    fn placeholder_part_compiles_to_a_render_tree() {
        let p = omnisphere_preset();
        let mut rn = crate::node_render::RenderNode::compile(&p, 48_000);
        rn.prepare(48_000.0, 256);
        // 4 layers × (2 filters + amp + wavetable + waveshaper + dfs) = 24.
        assert_eq!(rn.live_leaves(), 24);
    }

    /// Machine-local: the realized Part plays a real Omnisphere soundsource.
    /// `cargo test -p signal-sampler --lib omnisphere_soundsource -- --ignored`
    #[test]
    #[ignore = "requires the local Omnisphere extraction on AudioHaven"]
    fn omnisphere_soundsource_part_is_audible() {
        use signal_plugin_host::{PluginEvents, PluginMidiEvent};
        let Some(p) = omnisphere_soundsource_preset() else {
            eprintln!("skipping: Omnisphere extraction not present");
            return;
        };
        let mut rn = crate::node_render::RenderNode::compile(&p, 48_000);
        rn.prepare(48_000.0, 512);

        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiMessage::note_on(0, 60, 100),
        }];
        let mut heard = 0.0f32;
        for _ in 0..600 {
            let ev = PluginEvents {
                params: &[],
                midi: &midi,
                note_expressions: &[],
            };
            rn.render(&mut l, &mut r, &ev);
            let rms = (l.iter().map(|s| s * s).sum::<f32>() / l.len() as f32).sqrt();
            heard = heard.max(rms);
            if heard > 1e-3 {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(10));
        }
        assert!(
            heard > 1e-3,
            "soundsource part should be audible, rms={heard}"
        );
    }
}
