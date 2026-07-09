//! Composition tree for the Nord-style **Keys rig** — the structure that the
//! flat guitar-rig chain can't express. See `docs/nord-stage-4-signal-routing.md`.
//!
//! Two orthogonal axes (per the design doc):
//!
//! 1. **Containment (the folder tree)** — a [`RigNode`] is either a leaf
//!    [`RigBlock`] or a [`Container`]: an infinitely-nestable "block folder".
//!    A container carries a [`Role`] label (Preset / Engine / Layer / Module —
//!    intent only) and a [`Combine`] rule (Serial = chain children; Parallel =
//!    sum children). `Layer`/`Engine`/`Preset` are just Modules with a role tag;
//!    grouping is always "nest another Module", never "a Layer inside a Layer".
//!
//! 2. **Routing (the signal graph)** — independent of containment. Each container
//!    also holds **modulators** (control-rate [`RigBlock`]s — `Envelope`/`Lfo`/
//!    `Arpeggiator` — that drive params, not audio) and **sends** (cross-tree
//!    audio routes, e.g. a layer's "To Rotary"). The tree groups/owns; these
//!    route. Cross-layer routing lives here, not in the tree shape.
//!
//! This module is **structure only** — every block is a placeholder
//! ([`RigBlock`] with no realization → `has_backend() == false`). DSP gets
//! implemented block-type by block-type later; the routing is locked first.

use facet::Facet;
use signal_proto::block::BlockType;

use crate::SamplerError;
use crate::rig::RigBlock;

/// A node in the composition tree: a leaf processor or a container.
///
/// Facet-derived, so a whole tree round-trips through styx — presets can be
/// authored in code (traceable factories) **or** loaded from `.styx`.
#[derive(Debug, Clone, Facet)]
#[repr(C)]
pub enum RigNode {
    /// A leaf processor. (Struct variant — round-trips cleanly through styx,
    /// unlike a newtype tuple variant.)
    Block { block: RigBlock },
    /// A nested container subtree.
    Container { container: Container },
}

/// Semantic role of a container — a label describing intent. The audio behaviour
/// is set by [`Combine`], not by this; roles drive display + where shared-vs-
/// per-child processing is understood to sit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(C)]
pub enum Role {
    /// The whole program (top of the tree).
    Preset,
    /// An instrument part (Organ / Keys / Synth).
    Engine,
    /// A processing lane; its parallel siblings sum.
    Layer,
    /// A serial folder / signal-chain segment (infinitely nestable).
    Module,
}

impl Role {
    pub const fn tag(self) -> &'static str {
        match self {
            Role::Preset => "Preset",
            Role::Engine => "Engine",
            Role::Layer => "Layer",
            Role::Module => "Module",
        }
    }
}

/// How a container combines its children into its output.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(C)]
pub enum Combine {
    /// Children chained in order: `child[0] → child[1] → … → out`.
    Serial,
    /// Children fed the same input; their outputs summed (parallel lanes).
    Parallel,
}

impl Combine {
    pub const fn tag(self) -> &'static str {
        match self {
            Combine::Serial => "serial",
            Combine::Parallel => "parallel",
        }
    }
}

/// A cross-tree audio send (the routing axis) — this container's output also
/// flows to the node named `target` (e.g. a layer routing "To Rotary").
#[derive(Debug, Clone, Facet)]
pub struct Send {
    /// Name of the destination node (resolved against the tree).
    pub target: String,
    /// Human label for the route, e.g. "To Rotary".
    pub label: String,
}

/// One **control-rate modulation route** (a ModMatrix row): a modulation
/// source drives one parameter of one block, scaled by `depth`.
///
/// - `source` — the name of a modulator block attached to this container or
///   an ancestor (`"Filter Env"`, `"LFO 1"`), or a MIDI performance source
///   (`"Wheel"`, `"Velocity"`, `"Aftertouch"`, `"Bender"`, `"CC74"`).
/// - `target` — `"Block Name.param"`; the block is found by display name in
///   this container's subtree, the param by the backend's parameter name
///   (e.g. `"Filter.cutoff"`, `"Amp.gain"`).
/// - `depth` — −1..+1 scale of the source into the param's normalized range,
///   added to the param's base value each block.
#[derive(Debug, Clone, Facet)]
pub struct ModRoute {
    pub source: String,
    pub target: String,
    pub depth: f32,
}

/// A container-level setting that isn't a block (a `(name, value)` pair) —
/// e.g. a Layer's `voice_mode` / `unison` / `octave`, an Engine's menu options.
#[derive(Debug, Clone, Facet)]
pub struct Param {
    pub name: String,
    pub value: String,
}

/// The **keyboard-routing zone** a container occupies — the central MIDI input
/// router's per-container rule. A note must fall in both the key window and the
/// velocity window to reach this subtree; crossfade edges blend it in/out
/// (Nord-style key splits + Omnisphere-style velocity crossfades).
///
/// The combined gain (`key_gain × vel_gain`, 0..1) scales the note's velocity
/// into the subtree, so a note in a crossfade region plays adjacent layers at
/// partial level — a true blend. Nested zones multiply (a key-split Layer
/// holding velocity-split Modules). The default [`Zone::full`] passes everything.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
pub struct Zone {
    /// Lowest playable key (MIDI note).
    pub key_lo: u8,
    /// Highest playable key.
    pub key_hi: u8,
    /// Crossfade width in semitones at each key edge. 0 = hard split.
    pub key_xfade: u8,
    /// Lowest velocity that sounds.
    pub vel_lo: u8,
    /// Highest velocity that sounds.
    pub vel_hi: u8,
    /// Crossfade width in velocity units at each edge. 0 = hard. (Omnisphere-style.)
    pub vel_xfade: u8,
}

impl Default for Zone {
    fn default() -> Self {
        Zone::full()
    }
}

impl Zone {
    /// The everything-passes zone (full key + velocity range, no crossfade).
    pub const fn full() -> Self {
        Self {
            key_lo: 0,
            key_hi: 127,
            key_xfade: 0,
            vel_lo: 1,
            vel_hi: 127,
            vel_xfade: 0,
        }
    }

    pub fn is_full(&self) -> bool {
        *self == Zone::full()
    }

    /// Key-axis gain for `key` (0 outside the window, ramped across the xfade).
    pub fn key_gain(&self, key: u8) -> f32 {
        ramp(key, self.key_lo, self.key_hi, self.key_xfade)
    }

    /// Velocity-axis gain for `vel`.
    pub fn vel_gain(&self, vel: u8) -> f32 {
        ramp(vel, self.vel_lo, self.vel_hi, self.vel_xfade)
    }

    /// Combined routing gain for a note — `key_gain × vel_gain`, in `0..=1`.
    pub fn note_gain(&self, key: u8, vel: u8) -> f32 {
        self.key_gain(key) * self.vel_gain(vel)
    }
}

/// Trapezoidal window gain: 0 outside `[lo, hi]`, linearly ramped 0→1 over the
/// `xfade` at each edge, 1 in the middle. `xfade == 0` ⇒ a hard 0/1 window.
fn ramp(x: u8, lo: u8, hi: u8, xfade: u8) -> f32 {
    let (x, lo, hi, xf) = (x as f32, lo as f32, hi as f32, xfade as f32);
    let rising = if xf == 0.0 {
        if x >= lo { 1.0 } else { 0.0 }
    } else {
        (x - lo) / xf
    };
    let falling = if xf == 0.0 {
        if x <= hi { 1.0 } else { 0.0 }
    } else {
        (hi - x) / xf
    };
    rising.min(falling).clamp(0.0, 1.0)
}

/// A container node: a named folder of children with a combine rule, plus the
/// routing-axis attachments (modulators + sends).
#[derive(Debug, Clone, Facet)]
pub struct Container {
    pub role: Role,
    pub name: String,
    pub combine: Combine,
    /// Input trim (dB) applied before this container's children. Modules use it
    /// as their input volume; Layers/Engines normally leave it at 0.
    #[facet(default)]
    pub input_db: f32,
    /// Output volume (dB) — the "fader". A Layer's and an Engine's native volume;
    /// a Module's output trim. Paired with [`input_db`](Self::input_db).
    #[facet(default)]
    pub output_db: f32,
    /// Audio children, in order.
    #[facet(default)]
    pub children: Vec<RigNode>,
    /// Control-rate modulators attached here (drive params via routes, not audio).
    #[facet(default)]
    pub modulators: Vec<RigBlock>,
    /// Cross-tree audio sends from this node's output.
    #[facet(default)]
    pub sends: Vec<Send>,
    /// Control-rate modulation routes scoped to this subtree (the ModMatrix).
    #[facet(default)]
    pub mod_routes: Vec<ModRoute>,
    /// Container-level settings that aren't blocks — e.g. a Layer's `voice_mode`,
    /// `unison`, `octave`; an Engine's menu options.
    #[facet(default)]
    pub params: Vec<Param>,
    /// Keyboard-routing zone (key split + velocity crossfade). Default = full
    /// (everything passes); a Layer with a narrower zone only sounds notes in
    /// its window, scaled by the crossfade.
    #[facet(default)]
    pub zone: Zone,
    /// Whether this whole subtree is bypassed.
    #[facet(default)]
    pub bypassed: bool,
}

impl From<RigBlock> for RigNode {
    fn from(b: RigBlock) -> Self {
        RigNode::Block { block: b }
    }
}

impl From<Container> for RigNode {
    fn from(c: Container) -> Self {
        RigNode::Container { container: c }
    }
}

impl RigNode {
    pub fn name(&self) -> &str {
        match self {
            RigNode::Block { block: b } => &b.name,
            RigNode::Container { container: c } => &c.name,
        }
    }

    pub fn as_container(&self) -> Option<&Container> {
        match self {
            RigNode::Container { container: c } => Some(c),
            _ => None,
        }
    }
}

impl Container {
    fn new(role: Role, name: impl Into<String>, combine: Combine) -> Self {
        Self {
            role,
            name: name.into(),
            combine,
            input_db: 0.0,
            output_db: 0.0,
            children: Vec::new(),
            modulators: Vec::new(),
            sends: Vec::new(),
            mod_routes: Vec::new(),
            params: Vec::new(),
            zone: Zone::full(),
            bypassed: false,
        }
    }

    // ── Fluent builders ──────────────────────────────────────────────────

    /// A serial Module (a folder / signal-chain segment).
    pub fn module(name: impl Into<String>) -> Self {
        Self::new(Role::Module, name, Combine::Serial)
    }
    /// A parallel folder (children sum) — e.g. an engine's set of voice Layers.
    pub fn parallel(name: impl Into<String>) -> Self {
        Self::new(Role::Module, name, Combine::Parallel)
    }
    /// A processing Layer (serial inside; sums with its parallel siblings).
    pub fn layer(name: impl Into<String>) -> Self {
        Self::new(Role::Layer, name, Combine::Serial)
    }
    /// An Engine (instrument part).
    pub fn engine(name: impl Into<String>) -> Self {
        Self::new(Role::Engine, name, Combine::Serial)
    }
    /// A Preset (whole program).
    pub fn preset(name: impl Into<String>) -> Self {
        Self::new(Role::Preset, name, Combine::Serial)
    }

    /// Append a child node (block or container).
    #[must_use]
    pub fn add(mut self, child: impl Into<RigNode>) -> Self {
        self.children.push(child.into());
        self
    }

    /// Append several children.
    #[must_use]
    pub fn extend(mut self, children: impl IntoIterator<Item = RigNode>) -> Self {
        self.children.extend(children);
        self
    }

    /// Convenience: append a placeholder leaf block of `block_type`, named.
    #[must_use]
    pub fn block(mut self, block_type: BlockType, name: impl Into<String>) -> Self {
        self.children.push(RigNode::Block {
            block: RigBlock::of_type(block_type).named(name),
        });
        self
    }

    /// Add a **Sampler** block realized by a sample library spec (keys/piano
    /// layers, drum kits, orchestral sections). See [`RigBlock::sample_lib`].
    #[must_use]
    pub fn sample_block(mut self, name: impl Into<String>, spec_path: impl Into<String>) -> Self {
        self.children.push(RigNode::Block {
            block: RigBlock::sample_lib(spec_path).named(name),
        });
        self
    }

    /// Attach a control-rate modulator (placeholder block) to this container.
    #[must_use]
    pub fn modulator(mut self, block_type: BlockType, name: impl Into<String>) -> Self {
        self.modulators
            .push(RigBlock::of_type(block_type).named(name));
        self
    }

    /// Attach a fully-configured modulator block (params set — e.g. an
    /// imported envelope's ADSR times).
    #[must_use]
    pub fn modulator_block(mut self, block: RigBlock) -> Self {
        self.modulators.push(block);
        self
    }

    /// Add a cross-tree send from this node's output to `target`.
    #[must_use]
    pub fn send(mut self, target: impl Into<String>, label: impl Into<String>) -> Self {
        self.sends.push(Send {
            target: target.into(),
            label: label.into(),
        });
        self
    }

    /// Add a control-rate modulation route (a ModMatrix row) scoped to this
    /// subtree. See [`ModRoute`].
    #[must_use]
    pub fn route(
        mut self,
        source: impl Into<String>,
        target: impl Into<String>,
        depth: f32,
    ) -> Self {
        self.mod_routes.push(ModRoute {
            source: source.into(),
            target: target.into(),
            depth,
        });
        self
    }

    /// Set a container-level setting (e.g. `voice_mode`, `unison`, `octave`).
    #[must_use]
    pub fn param(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.params.push(Param {
            name: name.into(),
            value: value.into(),
        });
        self
    }

    // ── styx (de)serialization ───────────────────────────────────────────

    /// Parse a composition tree from a `.styx` string.
    pub fn from_styx_str(text: &str) -> Result<Self, SamplerError> {
        facet_styx::from_str(text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Parse a composition tree from a `.styx` file.
    pub fn from_styx_file(path: &std::path::Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_styx_str(&text)
    }

    /// Serialize this tree to a `.styx` string.
    pub fn to_styx_string(&self) -> Result<String, SamplerError> {
        facet_styx::to_string(self).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Set the output volume (dB) — the Layer/Engine fader, or a Module's output
    /// trim.
    #[must_use]
    pub fn volume(mut self, db: f32) -> Self {
        self.output_db = db;
        self
    }

    /// Set the input trim (dB) — a Module's input volume.
    #[must_use]
    pub fn input_db(mut self, db: f32) -> Self {
        self.input_db = db;
        self
    }

    // ── Keyboard routing (central-MIDI-input zone) ───────────────────────

    /// Restrict this subtree to a key range (MIDI notes `lo..=hi`).
    #[must_use]
    pub fn keys(mut self, lo: u8, hi: u8) -> Self {
        self.zone.key_lo = lo;
        self.zone.key_hi = hi;
        self
    }

    /// Crossfade width (semitones) at the key-split edges.
    #[must_use]
    pub fn key_xfade(mut self, semitones: u8) -> Self {
        self.zone.key_xfade = semitones;
        self
    }

    /// Restrict this subtree to a velocity range (`lo..=hi`).
    #[must_use]
    pub fn velocity(mut self, lo: u8, hi: u8) -> Self {
        self.zone.vel_lo = lo;
        self.zone.vel_hi = hi;
        self
    }

    /// Crossfade width (velocity units) at the velocity edges (Omnisphere-style).
    #[must_use]
    pub fn vel_xfade(mut self, width: u8) -> Self {
        self.zone.vel_xfade = width;
        self
    }

    /// Set the whole routing zone at once.
    #[must_use]
    pub fn zone(mut self, zone: Zone) -> Self {
        self.zone = zone;
        self
    }

    // ── Queries ──────────────────────────────────────────────────────────

    /// Every leaf block in this subtree (recursive, audio tree only — excludes
    /// modulators).
    pub fn blocks(&self) -> Vec<&RigBlock> {
        let mut out = Vec::new();
        self.collect_blocks(&mut out);
        out
    }

    fn collect_blocks<'a>(&'a self, out: &mut Vec<&'a RigBlock>) {
        for child in &self.children {
            match child {
                RigNode::Block { block: b } => out.push(b),
                RigNode::Container { container: c } => c.collect_blocks(out),
            }
        }
    }

    /// All modulator blocks in this subtree (recursive).
    pub fn modulators_recursive(&self) -> Vec<&RigBlock> {
        let mut out: Vec<&RigBlock> = self.modulators.iter().collect();
        for child in &self.children {
            if let RigNode::Container { container: c } = child {
                out.extend(c.modulators_recursive());
            }
        }
        out
    }

    /// Find the first descendant container named `name` (depth-first, incl self).
    pub fn find(&self, name: &str) -> Option<&Container> {
        if self.name == name {
            return Some(self);
        }
        for child in &self.children {
            if let RigNode::Container { container: c } = child {
                if let Some(found) = c.find(name) {
                    return Some(found);
                }
            }
        }
        None
    }

    /// Containers of a given role anywhere in the subtree (incl self).
    pub fn of_role(&self, role: Role) -> Vec<&Container> {
        let mut out = Vec::new();
        self.collect_role(role, &mut out);
        out
    }

    fn collect_role<'a>(&'a self, role: Role, out: &mut Vec<&'a Container>) {
        if self.role == role {
            out.push(self);
        }
        for child in &self.children {
            if let RigNode::Container { container: c } = child {
                c.collect_role(role, out);
            }
        }
    }

    /// All cross-tree sends in this subtree, as `(from_name, Send)`.
    pub fn sends_recursive(&self) -> Vec<(&str, &Send)> {
        let mut out: Vec<(&str, &Send)> =
            self.sends.iter().map(|s| (self.name.as_str(), s)).collect();
        for child in &self.children {
            if let RigNode::Container { container: c } = child {
                out.extend(c.sends_recursive());
            }
        }
        out
    }

    /// Render the subtree as an indented routing diagram (for inspection/tests).
    pub fn dump(&self) -> String {
        let mut s = String::new();
        self.dump_into(&mut s, "", true, true);
        s
    }

    fn dump_into(&self, out: &mut String, prefix: &str, last: bool, root: bool) {
        let (branch, child_prefix) = if root {
            ("", String::new())
        } else if last {
            ("└─ ", format!("{prefix}   "))
        } else {
            ("├─ ", format!("{prefix}│  "))
        };
        out.push_str(prefix);
        out.push_str(branch);
        out.push_str(&format!(
            "{} \"{}\" [{}]",
            self.role.tag(),
            self.name,
            self.combine.tag()
        ));
        match self.role {
            // Layers/Engines/Presets have one native volume (the fader).
            Role::Layer | Role::Engine | Role::Preset => {
                out.push_str(&format!("  vol {:+.0}dB", self.output_db));
            }
            // Modules show in/out trim only when set.
            Role::Module if self.input_db != 0.0 || self.output_db != 0.0 => {
                out.push_str(&format!(
                    "  trim {:+.0}/{:+.0}dB",
                    self.input_db, self.output_db
                ));
            }
            Role::Module => {}
        }
        if !self.zone.is_full() {
            let z = &self.zone;
            out.push_str(&format!(
                "  ⌨ keys {}-{}{}  vel {}-{}{}",
                z.key_lo,
                z.key_hi,
                if z.key_xfade > 0 {
                    format!("~{}", z.key_xfade)
                } else {
                    String::new()
                },
                z.vel_lo,
                z.vel_hi,
                if z.vel_xfade > 0 {
                    format!("~{}", z.vel_xfade)
                } else {
                    String::new()
                },
            ));
        }
        if !self.params.is_empty() {
            let ps: Vec<String> = self
                .params
                .iter()
                .map(|p| format!("{}={}", p.name, p.value))
                .collect();
            out.push_str(&format!("  {{{}}}", ps.join(", ")));
        }
        for m in &self.modulators {
            out.push_str(&format!("  ~{}:{}", m.block_type_tag(), m.display_name()));
        }
        for snd in &self.sends {
            out.push_str(&format!("  ⟿ {}→{}", snd.label, snd.target));
        }
        out.push('\n');

        let n = self.children.len();
        for (i, child) in self.children.iter().enumerate() {
            let is_last = i + 1 == n;
            match child {
                RigNode::Container { container: c } => {
                    c.dump_into(out, &child_prefix, is_last, false)
                }
                RigNode::Block { block: b } => {
                    let bb = if is_last { "└─ " } else { "├─ " };
                    out.push_str(&child_prefix);
                    out.push_str(bb);
                    out.push_str(&format!(
                        "Block {} \"{}\"{}\n",
                        b.block_type_tag(),
                        b.display_name(),
                        if b.has_backend() {
                            ""
                        } else {
                            " (placeholder)"
                        }
                    ));
                }
            }
        }
    }
}

impl RigBlock {
    /// Lowercase tag of the block's type (e.g. "amp", "delay") for display/dump.
    pub fn block_type_tag(&self) -> &'static str {
        self.block_type.as_str()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_a_nested_tree_and_finds_nodes() {
        let preset = Container::preset("P")
            .add(
                Container::parallel("Voices")
                    .add(Container::layer("A").block(BlockType::Filter, "Filter"))
                    .add(Container::layer("B").block(BlockType::Filter, "Filter")),
            )
            .add(Container::module("Global").block(BlockType::Rotary, "Rotary"));

        // Two layers, three blocks (2 filters + rotary).
        assert_eq!(preset.of_role(Role::Layer).len(), 2);
        assert_eq!(preset.blocks().len(), 3);
        assert!(preset.find("Voices").is_some());
        assert!(preset.find("Rotary").is_none()); // Rotary is a Block, not a container
        // Filters have Native DSP; the Rotary is still a placeholder.
        assert!(
            preset
                .blocks()
                .iter()
                .all(|b| b.has_backend() == (b.block_type_tag() == "filter"))
        );
    }

    #[test]
    fn modules_nest_infinitely() {
        let m = Container::module("Delay")
            .block(BlockType::Delay, "Delay")
            .add(Container::module("Feedback FX").block(BlockType::Chorus, "fb"))
            .block(BlockType::Filter, "fb-filter");
        // Delay + fb + fb-filter = 3 leaf blocks across the nested modules.
        assert_eq!(m.blocks().len(), 3);
    }

    #[test]
    fn containers_carry_volume_and_trim() {
        let layer = Container::layer("A").volume(-3.0);
        assert_eq!(layer.output_db, -3.0);
        assert_eq!(layer.input_db, 0.0);
        // Layers show their fader in the dump.
        assert!(layer.dump().contains("vol -3dB"));

        // A Module has both input and output volume.
        let m = Container::module("Amp/EQ").input_db(2.0).volume(-1.0);
        assert_eq!(m.input_db, 2.0);
        assert_eq!(m.output_db, -1.0);
        assert!(m.dump().contains("trim +2/-1dB"));
    }

    #[test]
    fn tree_round_trips_through_styx() {
        let tree = Container::preset("Demo")
            .volume(-1.0)
            .add(
                Container::parallel("Voices")
                    .add(
                        Container::layer("A")
                            .param("voice_mode", "Poly")
                            .volume(-3.0)
                            .add(Container::module("Osc").block(BlockType::Oscillator, "Osc")),
                    )
                    .add(Container::layer("B").block(BlockType::Sampler, "Smp")),
            )
            .add(
                Container::module("Global")
                    .input_db(1.0)
                    .block(BlockType::Rotary, "Rotary"),
            );

        let styx = tree.to_styx_string().expect("serialize");
        let back = Container::from_styx_str(&styx).expect("parse");

        // Structure survives the round trip.
        assert_eq!(back.name, "Demo");
        assert_eq!(back.output_db, -1.0);
        assert_eq!(back.of_role(Role::Layer).len(), 2);
        assert_eq!(back.blocks().len(), 3);
        let a = back.find("A").unwrap();
        assert_eq!(a.output_db, -3.0);
        assert_eq!(a.params[0].value, "Poly");
        assert_eq!(back.find("Global").unwrap().input_db, 1.0);
    }

    #[test]
    fn zone_split_and_crossfade_gains() {
        // Key crossfade window 60..72, 4-semitone edges.
        let z = Zone {
            key_lo: 60,
            key_hi: 72,
            key_xfade: 4,
            vel_lo: 1,
            vel_hi: 127,
            vel_xfade: 0,
        };
        assert_eq!(z.key_gain(59), 0.0, "below window");
        assert_eq!(z.key_gain(60), 0.0, "edge starts the fade at 0");
        assert!((z.key_gain(62) - 0.5).abs() < 1e-6, "half through the fade");
        assert_eq!(z.key_gain(66), 1.0, "centre is full");
        assert!((z.key_gain(70) - 0.5).abs() < 1e-6, "fading out");
        assert_eq!(z.key_gain(72), 0.0, "top edge");
        assert_eq!(z.key_gain(80), 0.0, "above window");

        // Hard split (no crossfade): full inside, zero outside.
        let hard = Zone {
            key_lo: 0,
            key_hi: 59,
            key_xfade: 0,
            ..Zone::full()
        };
        assert_eq!(hard.key_gain(48), 1.0);
        assert_eq!(hard.key_gain(60), 0.0);

        // Velocity crossfade (Omnisphere-style) — soft layer fading out by 80.
        let v = Zone {
            vel_lo: 1,
            vel_hi: 80,
            vel_xfade: 20,
            ..Zone::full()
        };
        assert!((v.vel_gain(70) - 0.5).abs() < 1e-6);
        assert_eq!(v.vel_gain(100), 0.0);
        // Combined gain multiplies key × velocity.
        assert!((z.note_gain(66, 64) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn sends_and_modulators_are_collected() {
        let layer = Container::layer("Synth A")
            .modulator(BlockType::Envelope, "Amp Env")
            .modulator(BlockType::Lfo, "LFO")
            .add(Container::module("Amp/EQ").send("Rotary", "To Rotary"));
        assert_eq!(layer.modulators_recursive().len(), 2);
        let sends = layer.sends_recursive();
        assert_eq!(sends.len(), 1);
        assert_eq!(sends[0].1.label, "To Rotary");
    }
}
