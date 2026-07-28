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
//! │  ├─ Pad    → Pad · Shimmer            (the wash, and its sparkle)
//! │  ├─ Organ  → Organ A · Organ B        (drawbar upper / lower)
//! │  ├─ Aux    → Aux A · Aux B · Aux C    (whatever the song needs)
//! │  ├─ Drone  → Drone                    (the bed a moment sits on)
//! │  └─ SFX    → SFX A · SFX B            (risers, impacts — fired)
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
    /// Keep this lane OUT of the engine and rig Global Controls.
    ///
    /// A global filter sweep or envelope change is a performance move over
    /// the sound as a whole, and there is usually one lane you don't want it
    /// touching — the piano under everything else. An excluded lane still has
    /// its own macros; it just isn't in the scope the globals drive.
    #[facet(default)]
    pub exclude_global: bool,
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
            exclude_global: false,
        }
    }

    /// Keep this lane out of the engine and rig Global Controls.
    #[must_use]
    pub fn excluded_from_globals(mut self) -> Self {
        self.exclude_global = true;
        self
    }

    /// Every module's patch, module A first, padded to the quad.
    pub fn module_patches(&self) -> Vec<String> {
        // Exactly what the layer declares — a patch that uses two modules
        // gets two, and more can be added at any time.
        let mut v = vec![self.patch.clone()];
        v.extend(self.extra_modules.iter().cloned());
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
    /// Engine container name ("Keys", "Aux", "Organ", "Pad").
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

    /// Reorder the engines: named ones take the given order, engines the list
    /// does not mention keep their relative place behind them (a stable sort),
    /// so a partial order is a promotion rather than a truncation.
    pub fn apply_order(&mut self, order: &[String]) {
        let rank = |name: &str| order.iter().position(|n| n == name).unwrap_or(usize::MAX);
        self.engines.sort_by_key(|e| rank(&e.name));
    }

    /// The current engine order, left to right.
    pub fn engine_order(&self) -> Vec<String> {
        self.engines.iter().map(|e| e.name.clone()).collect()
    }

    /// Where this profile is saved: `$FTS_KEYS_PROFILE` when the rig was
    /// pointed at a file, else `~/.config/signal/keys/profiles/<name>.styx`.
    pub fn config_path(name: &str) -> Option<std::path::PathBuf> {
        if let Ok(p) = std::env::var("FTS_KEYS_PROFILE") {
            return Some(std::path::PathBuf::from(p));
        }
        let base = std::env::var("XDG_CONFIG_HOME").map(std::path::PathBuf::from).ok().or_else(
            || std::env::var("HOME").ok().map(|h| std::path::PathBuf::from(h).join(".config")),
        )?;
        let file = name.trim();
        let file = if file.is_empty() { "Worship" } else { file };
        Some(base.join("signal/keys/profiles").join(format!("{file}.styx")))
    }

    /// Write the profile back to disk — the edits a player makes to their
    /// mixer (engine order today; lanes and scenes as they become editable)
    /// belong to the profile, not to the session.
    ///
    /// Best-effort: a rig that cannot write its config still plays, and losing
    /// a reorder is not worth failing a service for.
    pub fn save(&self) {
        let Some(path) = Self::config_path(&self.name) else { return };
        if let Some(dir) = path.parent() {
            if let Err(e) = std::fs::create_dir_all(dir) {
                tracing::warn!(?path, "keys profile not saved: {e}");
                return;
            }
        }
        match self.to_styx_string() {
            Ok(text) => match std::fs::write(&path, text) {
                Ok(()) => tracing::info!(?path, profile = %self.name, "keys profile saved"),
                Err(e) => tracing::warn!(?path, "keys profile not saved: {e}"),
            },
            Err(e) => tracing::warn!("keys profile not serializable: {e}"),
        }
    }

    /// Read a saved profile by name, if one has been written.
    pub fn load_saved(name: &str) -> Option<Self> {
        let path = Self::config_path(name)?;
        let text = std::fs::read_to_string(&path).ok()?;
        match Self::from_styx_str(&text) {
            Ok(p) => {
                tracing::info!(?path, profile = %p.name, "keys profile loaded from disk");
                Some(p)
            }
            Err(e) => {
                tracing::error!(?path, "saved keys profile is unreadable ({e}); using the built-in");
                None
            }
        }
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
        self.build_tree_with(resolve, |_, _| signal_synth::engine::ModuleSettings::default())
    }

    /// As [`build_tree`](Self::build_tree), with the live macro values for
    /// each `(layer, module)` — what makes the Filter block and the envelopes
    /// carry the rig's actual settings.
    pub fn build_tree_with(
        &self,
        resolve: impl Fn(&str) -> Option<String>,
        module_set: impl Fn(&str, usize) -> signal_synth::engine::ModuleSettings,
    ) -> Container {
        let mut engines = Container::parallel("Engines");
        for engine in &self.engines {
            let eng = Container::engine(&engine.name).volume(engine.gain_db);
            // Layers SUM — an engine's lanes are voices played together, not
            // a chain (a serial engine would let the last lane overwrite the
            // ones before it). Same shape as the Nord reference program.
            let mut voices = Container::parallel(format!("{} Voices", engine.name));
            for layer in &engine.layers {
                // The authored fader rides in as the tree default (the live
                // mixer's cells overwrite it once running).
                let lane =
                    Self::lane_container(layer, &resolve, &module_set).volume(layer.gain_db);
                voices = voices.add(lane);
            }
            engines = engines.add(eng.add(voices));
        }
        Container::preset(&self.name)
            .add(engines)
            .add(Self::global_tail())
    }

    /// One layer's composition subtree — the same lane whether it renders
    /// inside the single program tree or as its own daw track.
    ///
    /// EVERY lane is a Signal Engine layer — the same source → filters → amp
    /// → FX program whether the patch is a Keyscape piano, an Omnisphere
    /// soundsource or a wavetable. That's why one layer-zoom surface can
    /// control all of them. Each of the lane's four modules realizes its own
    /// source; the lane's macro values ride along when the rig has them.
    fn lane_container(
        layer: &LayerDef,
        resolve: &impl Fn(&str) -> Option<String>,
        module_set: &impl Fn(&str, usize) -> signal_synth::engine::ModuleSettings,
    ) -> Container {
        let sources: Vec<signal_synth::Source> = layer
            .module_patches()
            .into_iter()
            .map(|patch| {
                let spec = (!patch.is_empty()).then(|| resolve(&patch)).flatten();
                signal_synth::Source::sample(spec)
            })
            .collect();
        let settings: Vec<signal_synth::engine::ModuleSettings> = sources
            .iter()
            .enumerate()
            .map(|(i, source)| {
                let mut set = module_set(&layer.name, i);
                set.source = source.clone();
                set
            })
            .collect();
        let mut lane = signal_synth::engine::signal_layer_with(&layer.name, &settings);
        if !layer.is_full_range() {
            lane = lane.zone(signal_sampler::rig_node::Zone {
                key_lo: layer.key_lo,
                key_hi: layer.key_hi,
                ..signal_sampler::rig_node::Zone::full()
            });
        }
        lane
    }

    /// The rig's global tail — one shared rotary for the organ, master reverb.
    fn global_tail() -> Container {
        Container::module("Global")
            .add(Container::module("Master Reverb").block(BlockType::Reverb, "Reverb"))
    }

    /// Build the profile as a per-lane daw-track program: one subtree per
    /// layer (its fader/mute/solo become daw track ops on its own track),
    /// engines as folder tracks, and the global tail as rig-folder FX.
    ///
    /// Lane and engine faders deliberately do NOT ride into the subtrees —
    /// in lane mode they are daw track volumes, applied by the backend.
    pub fn build_lane_program(
        &self,
        resolve: impl Fn(&str) -> Option<String>,
        module_set: impl Fn(&str, usize) -> signal_synth::engine::ModuleSettings,
    ) -> signal_sampler::keys_rig::LaneProgram {
        use signal_sampler::keys_rig::{LaneEngine, LaneLayer, LaneProgram};
        LaneProgram {
            name: self.name.clone(),
            engines: self
                .engines
                .iter()
                .map(|engine| LaneEngine {
                    name: engine.name.clone(),
                    layers: engine
                        .layers
                        .iter()
                        .map(|layer| LaneLayer {
                            name: layer.name.clone(),
                            tree: Self::lane_container(layer, &resolve, &module_set),
                        })
                        .collect(),
                })
                .collect(),
            tail: Some(Self::global_tail()),
        }
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
                    // The piano under everything: excluded from the engine
                    // and rig globals by default, so a filter sweep or an
                    // envelope change over the rig leaves it alone.
                    LayerDef::new("Keys A", "LA Custom C7 Grand").excluded_from_globals(),
                    LayerDef::new("Keys B", "Rhodes - LA Custom"),
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
                layers: vec![
                    LayerDef {
                        name: "Pad".into(),
                        patch: "OB-8 PWM Big Strings".into(),
                        extra_modules: vec!["Juno 60 Raw Sub".into()],
                        gain_db: 0.0,
                        key_lo: 0,
                        key_hi: 127,
                        exclude_global: false,
                    },
                    // The bright half of the wash — the octave-up sparkle that
                    // sits over the pad rather than inside it, so it can be
                    // ridden (or dropped) on its own.
                    LayerDef::new("Shimmer", ""),
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
                name: "Aux".into(),
                gain_db: 0.0,
                layers: vec![
                    LayerDef::new("Aux A", ""),
                    LayerDef::new("Aux B", ""),
                    LayerDef::new("Aux C", ""),
                ],
            },
            EngineDef {
                name: "Drone".into(),
                gain_db: 0.0,
                // The bed under a moment — a Pad-Player-style drone that
                // holds a key while the band moves over it. One lane, because
                // a drone is one sustained thing; the key it drones on is a
                // performance decision, not a patch one.
                //
                // Empty by default: a drone is chosen for a moment, not left
                // loaded — and preloading a pack nobody asked for costs the
                // budget a piano could have used. Pick a sound in the browser
                // and a key on the card.
                layers: vec![LayerDef::new("Drone", "")],
            },
            EngineDef {
                name: "SFX".into(),
                gain_db: 0.0,
                // Risers, impacts, swells — fired, not played, so the lanes
                // start empty and get filled from the browser for the song at
                // hand.
                layers: vec![LayerDef::new("SFX A", ""), LayerDef::new("SFX B", "")],
            },
        ],
        stacks: vec![
            KeysStackDef {
                name: "Spotlight".into(),
                blurb: "Solo grand — nothing under it".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", 0.0),
                    SceneSlot::off("Keys B"),
                    SceneSlot::off("Aux A"),
                    SceneSlot::off("Aux B"),
                    SceneSlot::off("Aux C"),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::off("Pad"),
                    SceneSlot::off("Shimmer"),
                    SceneSlot::off("Drone"),
                    SceneSlot::off("SFX A"),
                    SceneSlot::off("SFX B"),
                ],
            },
            KeysStackDef {
                name: "Verse".into(),
                blurb: "Piano + soft pad bed".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", -1.0),
                    SceneSlot::off("Keys B"),
                    SceneSlot::off("Aux A"),
                    SceneSlot::off("Aux B"),
                    SceneSlot::off("Aux C"),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -8.0),
                    SceneSlot::off("Shimmer"),
                    SceneSlot::off("Drone"),
                    SceneSlot::off("SFX A"),
                    SceneSlot::off("SFX B"),
                ],
            },
            KeysStackDef {
                name: "Energy".into(),
                blurb: "Full band — piano, EP, synth, pad".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", 0.0),
                    SceneSlot::new("Keys B", "", -4.0),
                    SceneSlot::new("Aux A", "", -6.0),
                    SceneSlot::off("Aux B"),
                    SceneSlot::off("Aux C"),
                    SceneSlot::new("Organ A", "", -10.0),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -6.0),
                    SceneSlot::off("Shimmer"),
                    SceneSlot::off("Drone"),
                    SceneSlot::off("SFX A"),
                    SceneSlot::off("SFX B"),
                ],
            },
            KeysStackDef {
                name: "Hooks".into(),
                blurb: "Lead synth over the piano bed".into(),
                slots: vec![
                    SceneSlot::new("Keys A", "", -4.0),
                    SceneSlot::off("Keys B"),
                    SceneSlot::new("Aux A", "", 0.0),
                    SceneSlot::new("Aux B", "", -6.0),
                    SceneSlot::off("Aux C"),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -10.0),
                    SceneSlot::off("Shimmer"),
                    SceneSlot::off("Drone"),
                    SceneSlot::off("SFX A"),
                    SceneSlot::off("SFX B"),
                ],
            },
            KeysStackDef {
                name: "Underscore".into(),
                blurb: "Pad + swell under speaking".into(),
                slots: vec![
                    SceneSlot::off("Keys A"),
                    SceneSlot::off("Keys B"),
                    SceneSlot::off("Aux A"),
                    SceneSlot::off("Aux B"),
                    SceneSlot::new("Aux C", "", -12.0),
                    SceneSlot::off("Organ A"),
                    SceneSlot::off("Organ B"),
                    SceneSlot::new("Pad", "", -4.0),
                    SceneSlot::off("Shimmer"),
                    SceneSlot::new("Drone", "", -8.0),
                    SceneSlot::off("SFX A"),
                    SceneSlot::off("SFX B"),
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
        // Keys · Pad · Organ · Aux · Drone · SFX.
        assert_eq!(p.engines.len(), 6);
        assert_eq!(p.engine("Keys").unwrap().layers.len(), 2);
        assert_eq!(p.engine("Aux").unwrap().layers.len(), 3);
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
        use signal_sampler::rig_node::Role;
        use std::sync::Arc;
        for name in p.layer_names() {
            assert!(
                cells.get(Role::Layer, &name).is_some(),
                "no gain cell for lane {name}"
            );
        }
        for engine in &p.engines {
            assert!(
                cells.get(Role::Engine, &engine.name).is_some(),
                "no gain cell for engine {}",
                engine.name
            );
        }
        // A one-lane engine named after its lane ("Pad" holding "Pad") must
        // still be two cells — see the render tree's like-named test.
        for engine in p.engines.iter().filter(|e| e.layers.iter().any(|l| l.name == e.name)) {
            assert!(
                !Arc::ptr_eq(
                    cells.get(Role::Engine, &engine.name).expect("engine cell"),
                    cells.get(Role::Layer, &engine.name).expect("lane cell"),
                ),
                "{} shares one cell with its lane",
                engine.name
            );
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

#[cfg(test)]
mod order_tests {
    use super::*;

    /// A saved order survives a round-trip through styx, and an engine the
    /// saved file never heard of still shows up (behind the ones it names).
    #[test]
    fn order_round_trips_and_tolerates_new_engines() {
        let mut p = worship_profile();
        p.apply_order(&["SFX".into(), "Drone".into()]);
        assert_eq!(&p.engine_order()[..2], &["SFX".to_string(), "Drone".to_string()]);

        let text = p.to_styx_string().expect("serialize");
        let back = KeysProfile::from_styx_str(&text).expect("parse");
        assert_eq!(back.engine_order(), p.engine_order());

        // A profile that gains an engine later keeps the saved order and
        // appends the newcomer rather than dropping it.
        let saved_order = vec!["SFX".to_string(), "Drone".to_string()];
        let mut fresh = worship_profile();
        fresh.engines.push(EngineDef {
            name: "Brass".into(),
            gain_db: 0.0,
            layers: vec![LayerDef::new("Brass A", "")],
        });
        fresh.apply_order(&saved_order);
        assert_eq!(&fresh.engine_order()[..2], &["SFX".to_string(), "Drone".to_string()]);
        assert!(fresh.engine("Brass").is_some());
    }
}
