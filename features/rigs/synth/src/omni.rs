//! The full **Omnisphere 3** per-Part signal chain, built as a composition
//! tree ([`signal_sampler::rig_node`]) — the routing experiment companion to
//! [`signal_sampler::nord`]. If the engine can express both the Nord Stage 4 and the
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
//! Omnisphere soundsources ([`BlockImpl::Sample`](signal_sampler::rig::BlockImpl)).
//! The Multi level (8 Parts, mixer, 4 shared Aux racks) is a Rig concern and
//! comes later.

use signal_proto::block::BlockType;

use signal_sampler::rig_node::Container;

/// Send target for every layer's aux route (the Part's 4-slot Aux rack).
use crate::engine::AUX_RACK;

// ── Layers ───────────────────────────────────────────────────────────────────

/// One Omnisphere Layer: oscillator stack → dual filters → amp → layer FX.
/// `soundsource` = a library spec path realizing the Sample-mode source;
/// `None` keeps a placeholder Soundsource block (structure only).
fn layer(name: &str, soundsource: Option<String>) -> Container {
    // Omnisphere's "layer" is what this engine calls a MODULE: one Source
    // Block into filters → amp → FX. Four of them make the Quadzone, which is
    // exactly one `crate::engine::signal_layer`.
    crate::engine::signal_module(name, crate::engine::Source::sample(soundsource))
}

use crate::engine::fx_rack;

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
    use signal_sampler::rig_node::Role;

    #[test]
    fn part_has_four_modules_each_with_one_source() {
        let p = omnisphere_preset();
        for l in ["Layer A", "Layer B", "Layer C", "Layer D"] {
            let layer = p.find(l).expect(l);
            let osc = layer.find("Source").expect("source module");
            // ONE generator per module — a second would overwrite the first.
            assert_eq!(osc.blocks().len(), 1, "{l} has a single Source Block");
            assert_eq!(osc.blocks()[0].display_name(), "Soundsource");
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
        // 12 envelopes: 3 per module × the Part's 4 modules.
        let envs: usize = ["Layer A", "Layer B", "Layer C", "Layer D"]
            .iter()
            .filter_map(|n| p.find(n))
            .map(|m| m.modulators.len())
            .sum();
        assert_eq!(envs, 12, "12 envelopes across the 4 modules");
    }

    /// The placeholder Part renders silence-safely (native Filter/Amp are
    /// live pass-through-ish; everything else placeholder).
    #[test]
    fn placeholder_part_compiles_to_a_render_tree() {
        let p = omnisphere_preset();
        let mut rn = signal_sampler::node_render::RenderNode::compile(&p, 48_000);
        rn.prepare(48_000.0, 256);
        // 4 modules × (2 filters + amp) = 12 live native leaves; the source
        // blocks are placeholders here and everything else passes through.
        assert_eq!(rn.live_leaves(), 12);
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
        let mut rn = signal_sampler::node_render::RenderNode::compile(&p, 48_000);
        rn.prepare(48_000.0, 512);

        let (mut l, mut r) = (vec![0.0; 512], vec![0.0; 512]);
        let midi = [PluginMidiEvent {
            offset: 0,
            message: daw::service::MidiEvent::NoteOn {
                channel: daw::service::Channel::new(0),
                key: daw::service::KeyNumber::new(60),
                velocity: daw::service::Velocity::new(100),
            },
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
