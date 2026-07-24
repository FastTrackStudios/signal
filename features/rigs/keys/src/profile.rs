//! The keys **profile** — engines, layers, and the footswitch stacks that
//! recall them.
//!
//! Where the guitar rig's profile is a flat list of patches (one tone at a
//! time), a keys profile is a *mixer*: several engines sound at once, each
//! holding layers, each layer loading a patch (a `.signalpack` into a Sampler
//! block). A **stack** is a named scene over that mixer — press "Verse" and
//! every layer takes that scene's fader / mute / patch state.
//!
//! ```text
//! Profile "Worship"
//! ├─ Engines (parallel)
//! │  ├─ Keys   → Keys A · Keys B          (piano / EP)
//! │  ├─ Synth  → Synth A · Synth B · Synth C
//! │  ├─ Organ  → Organ A · Organ B        (drawbar upper / lower)
//! │  └─ Pad    → Pad                      (the wash under everything)
//! └─ Global    → master FX tail
//!
//! Stacks: Spotlight · Verse · Energy · Hooks · Underscore
//! ```
//!
//! The tree this builds is an ordinary [`Container`] program — the same one
//! the sampler renders, so layers inherit zones (key/velocity splits), sends,
//! modulators and the block/module system for free. Layer faders are live
//! [`GainCells`](signal_sampler::node_render::GainCells), addressed by the
//! layer's container name.

use facet::Facet;
use signal_proto::block::BlockType;
use signal_sampler::rig_node::Container;

/// One layer's slot in a scene: what it plays and how loud.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct SceneSlot {
    /// Layer container name ("Keys A", "Organ B", …).
    pub layer: String,
    /// Patch (pack / library stem) this layer holds in the scene. Empty =
    /// keep whatever is loaded.
    #[facet(default)]
    pub patch: String,
    /// Fader position in dB for this scene.
    #[facet(default)]
    pub gain_db: f32,
    /// Silent in this scene (fader position is remembered).
    #[facet(default)]
    pub muted: bool,
}

impl SceneSlot {
    pub fn new(layer: impl Into<String>, patch: impl Into<String>, gain_db: f32) -> Self {
        Self { layer: layer.into(), patch: patch.into(), gain_db, muted: false }
    }

    /// A slot that silences its layer in this scene.
    pub fn off(layer: impl Into<String>) -> Self {
        Self { layer: layer.into(), patch: String::new(), gain_db: 0.0, muted: true }
    }
}

/// One footswitch stack: a named scene over the whole mixer.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct KeysStackDef {
    /// Footswitch name ("Spotlight", "Verse", …).
    pub name: String,
    /// One-line description of the sound — what the player is reaching for.
    #[facet(default)]
    pub blurb: String,
    /// Per-layer state this stack recalls.
    #[facet(default)]
    pub slots: Vec<SceneSlot>,
}

/// One layer definition inside an engine.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct LayerDef {
    /// Container name — unique across the profile (the fader's address).
    pub name: String,
    /// Patch loaded at profile-build time (module A). Empty = an empty lane.
    #[facet(default)]
    pub patch: String,
    /// Patches for modules B/C/D — a layer holds four modules (Omnisphere's
    /// Quadzone) and `patch` is module A's. Missing entries are empty
    /// modules, so a one-patch lane just leaves this out.
    #[facet(default)]
    pub extra_modules: Vec<String>,
    /// Authored fader position (dB).
    #[facet(default)]
    pub gain_db: f32,
    /// Lowest key this lane sounds (MIDI note).
    #[facet(default)]
    pub key_lo: u8,
    /// Highest key this lane sounds — the default covers the whole keyboard.
    #[facet(default = 127)]
    pub key_hi: u8,
}

impl LayerDef {
    pub fn new(name: impl Into<String>, patch: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            patch: patch.into(),
            extra_modules: Vec::new(),
            gain_db: 0.0,
            key_lo: 0,
            key_hi: 127,
        }
    }

    /// Every module's patch, module A first, padded to the quad.
    pub fn module_patches(&self) -> Vec<String> {
        let mut v = vec![self.patch.clone()];
        v.extend(self.extra_modules.iter().cloned());
        // At least the default quad, but a layer may declare more.
        if v.len() < signal_synth::engine::MODULES_PER_LAYER {
            v.resize(signal_synth::engine::MODULES_PER_LAYER, String::new());
        }
        v
    }

    /// Restrict this lane to a key window (a Nord-style split).
    pub fn split(mut self, lo: u8, hi: u8) -> Self {
        self.key_lo = lo;
        self.key_hi = hi;
        self
    }

    /// Whether the lane covers the whole keyboard.
    pub fn is_full_range(&self) -> bool {
        self.key_lo == 0 && self.key_hi == 127
    }
}

/// One engine: an instrument part holding parallel layers.
#[derive(Debug, Clone, PartialEq, Facet)]
pub struct EngineDef {
    /// Engine container name ("Keys", "Synth", "Organ", "Pad").
    pub name: String,
    /// The engine's own fader (dB) — rides all its layers.
    #[facet(default)]
    pub gain_db: f32,
    pub layers: Vec<LayerDef>,
}

/// A complete keys profile: the mixer shape plus the stacks that recall it.
#[derive(Debug, Clone, PartialEq, Default, Facet)]
pub struct KeysProfile {
    pub name: String,
    pub engines: Vec<EngineDef>,
    #[facet(default)]
    pub stacks: Vec<KeysStackDef>,
}

impl KeysProfile {
    /// Parse a profile from `.styx` (the on-disk authoring format).
    pub fn from_styx_str(text: &str) -> Result<Self, String> {
        facet_styx::from_str(text).map_err(|e| e.to_string())
    }

    /// Serialize to `.styx`.
    pub fn to_styx_string(&self) -> Result<String, String> {
        facet_styx::to_string(self).map_err(|e| e.to_string())
    }

    /// Find an engine by name.
    pub fn engine(&self, name: &str) -> Option<&EngineDef> {
        self.engines.iter().find(|e| e.name == name)
    }

    /// Find a layer (and its engine) by layer name.
    pub fn layer(&self, name: &str) -> Option<(&EngineDef, &LayerDef)> {
        self.engines
            .iter()
            .find_map(|e| e.layers.iter().find(|l| l.name == name).map(|l| (e, l)))
    }

    pub fn layer_mut(&mut self, name: &str) -> Option<&mut LayerDef> {
        self.engines
            .iter_mut()
            .find_map(|e| e.layers.iter_mut().find(|l| l.name == name))
    }

    pub fn engine_mut(&mut self, name: &str) -> Option<&mut EngineDef> {
        self.engines.iter_mut().find(|e| e.name == name)
    }

    /// Every layer name, in engine order — the mixer's column order.
    pub fn layer_names(&self) -> Vec<String> {
        self.engines
            .iter()
            .flat_map(|e| e.layers.iter().map(|l| l.name.clone()))
            .collect()
    }

    /// Build the playable composition tree for this profile.
    ///
    /// `resolve` maps a patch name to the spec path a Sampler block loads
    /// (a `.signalpack`, or a `library.styx`); layers whose patch is empty or
    /// unresolvable render as silent lanes, so a profile is playable before
    /// every pack is downloaded.
    pub fn build_tree(&self, resolve: impl Fn(&str) -> Option<String>) -> Container {
        let mut engines = Container::parallel("Engines");
        for engine in &self.engines {
            let eng = Container::engine(&engine.name).volume(engine.gain_db);
            // Layers SUM — an engine's lanes are voices played together, not
            // a chain (a serial engine would let the last lane overwrite the
            // ones before it). Same shape as the Nord reference program.
            let mut voices = Container::parallel(format!("{} Voices", engine.name));
            for layer in &engine.layers {
                // EVERY lane is a Signal Engine layer — the same source →
                // filters → amp → FX program whether the patch is a Keyscape
                // piano, an Omnisphere soundsource or a wavetable. That's why
                // one layer-zoom surface can control all of them.
                // Each of the lane's four modules realizes its own source.
                let sources: Vec<signal_synth::Source> = layer
                    .module_patches()
                    .into_iter()
                    .map(|patch| {
                        let spec = (!patch.is_empty()).then(|| resolve(&patch)).flatten();
                        signal_synth::Source::sample(spec)
                    })
                    .collect();
                let mut lane = signal_synth::engine::signal_layer(&layer.name, &sources)
                    .volume(layer.gain_db);
                if !layer.is_full_range() {
                    lane = lane.zone(signal_sampler::rig_node::Zone {
                        key_lo: layer.key_lo,
                        key_hi: layer.key_hi,
                        ..signal_sampler::rig_node::Zone::full()
                    });
                }
                voices = voices.add(lane);
            }
            engines = engines.add(eng.add(voices));
        }
        Container::preset(&self.name).add(engines).add(
            // Global tail — one shared rotary for the organ, master reverb.
            Container::module("Global")
                .add(Container::module("Master Reverb").block(BlockType::Reverb, "Reverb")),
        )
    }

    /// Scene lookup for a stack index.
    pub fn stack(&self, index: usize) -> Option<&KeysStackDef> {
        self.stacks.get(index)
    }
}

/// The **Worship** profile: the five stacks a worship keys player lives in,
/// over four engines.
///
/// Stack intents (why these five):
/// - **Spotlight** — the solo moment: one grand piano, nothing under it.
/// - **Verse** — piano with a soft pad bed; room for the vocal.
/// - **Energy** — full band: piano + EP + bright synth + pad, organ ready.
/// - **Hooks** — the signature line: lead synth on top of the piano bed.
/// - **Underscore** — pad and swell only, under speaking / prayer.
pub fn worship_profile() -> KeysProfile {
    KeysProfile {
        name: "Worship".into(),
        engines: vec![
            EngineDef {
                name: "Keys".into(),
                gain_db: 0.0,
                layers: vec![
                    LayerDef::new("Keys A", "LA Custom C7 Grand"),
                    LayerDef::new("Keys B", "Rhodes - LA Custom"),
                ],
            },
            EngineDef {
                name: "Synth".into(),
                gain_db: 0.0,
                layers: vec![
                    LayerDef::new("Synth A", ""),
                    LayerDef::new("Synth B", ""),
                    LayerDef::new("Synth C", ""),
                ],
            },
            EngineDef {
                name: "Organ".into(),
                gain_db: 0.0,
                layers: vec![
                    LayerDef::new("Organ A", ""),
                    LayerDef::new("Organ B", ""),
                ],
            },
            EngineDef {
                name: "Pad".into(),
                gain_db: 0.0,
                // The wash under everything: "American Obesity" (Live
                // Keyboardist), rebuilt as one layer of the Signal Engine.
                // The patch stacks two soundsources — OB-8 PWM Big Strings
                // over a Juno 60 sub — so it lands as module A + module B,
                // which is exactly what the quad is for.
                layers: vec![LayerDef {
                    name: "Pad".into(),
                    patch: "OB-8 PWM Big Strings".into(),
                    extra_modules: vec!["Juno 60 Raw Sub".into()],
                    gain_db: 0.0,
                    key_lo: 0,
                    key_hi: 127,
                }],
            },
        ],
        stacks: vec![
            KeysStackDef {
                name: "Spotlight".into(),
                blurb: "Solo grand — nothing under it".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", 0.0),
                    SceneSlot::off("Keys B"),
                    SceneSlot::off("Synth A"),
                    SceneSlot::off("Synth B"),
                    SceneSlot::off("Synth C"),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::off("Pad"),
                ],
            },
            KeysStackDef {
                name: "Verse".into(),
                blurb: "Piano + soft pad bed".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", -1.0),
                    SceneSlot::off("Keys B"),
                    SceneSlot::off("Synth A"),
                    SceneSlot::off("Synth B"),
                    SceneSlot::off("Synth C"),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -8.0),
                ],
            },
            KeysStackDef {
                name: "Energy".into(),
                blurb: "Full band — piano, EP, synth, pad".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", 0.0),
                    SceneSlot::new("Keys B", "", -4.0),
                    SceneSlot::new("Synth A", "", -6.0),
                    SceneSlot::off("Synth B"),
                    SceneSlot::off("Synth C"),
                    SceneSlot::new("Organ A", "", -10.0),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -6.0),
                ],
            },
            KeysStackDef {
                name: "Hooks".into(),
                blurb: "Lead synth over the piano bed".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", -4.0),
                    SceneSlot::off("Keys B"),
                    SceneSlot::new("Synth A", "", 0.0),
                    SceneSlot::new("Synth B", "", -6.0),
                    SceneSlot::off("Synth C"),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -10.0),
                ],
            },
            KeysStackDef {
                name: "Underscore".into(),
                blurb: "Pad + swell under speaking".into(),
                slots: vec![
                    SceneSlot::off("Keys A"),
                    SceneSlot::off("Keys B"),
                    SceneSlot::off("Synth A"),
                    SceneSlot::off("Synth B"),
                    SceneSlot::new("Synth C", "", -12.0),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -4.0),
                ],
            },
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn worship_shape() {
        let p = worship_profile();
        assert_eq!(p.engines.len(), 4);
        assert_eq!(p.engine("Keys").unwrap().layers.len(), 2);
        assert_eq!(p.engine("Synth").unwrap().layers.len(), 3);
        assert_eq!(p.engine("Organ").unwrap().layers.len(), 2);
        assert_eq!(p.stacks.len(), 5);
        // Every stack addresses every layer — no lane is left undefined.
        let layers = p.layer_names();
        for stack in &p.stacks {
            for layer in &layers {
                assert!(
                    stack.slots.iter().any(|s| &s.layer == layer),
                    "stack {} misses layer {layer}",
                    stack.name
                );
            }
        }
    }

    #[test]
    fn tree_has_a_fader_per_lane() {
        let p = worship_profile();
        let tree = p.build_tree(|_| None);
        let (_, cells) =
            signal_sampler::node_render::RenderNode::compile_with_cells(&tree, 48_000);
        for name in p.layer_names() {
            assert!(cells.get(&name).is_some(), "no gain cell for {name}");
        }
        for engine in &p.engines {
            assert!(cells.get(&engine.name).is_some(), "no gain cell for {}", engine.name);
        }
    }

    #[test]
    fn styx_roundtrip() {
        let p = worship_profile();
        let text = p.to_styx_string().expect("serialize");
        let back = KeysProfile::from_styx_str(&text).expect("parse");
        assert_eq!(p, back);
    }
}
