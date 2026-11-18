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

use std::fmt::Write;

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

/// What a container means, and what its audio does.
///
/// Both come from [`signal_proto::node`] — the same two axes
/// [`Node`](signal_proto::node::Node) uses, because they are the same two
/// axes. There were two `Role`s and two `Combine`s, one on each side of the
/// domain/audio line, defined identically and kept in step by hand.
pub use signal_proto::node::{Combine, Role};

/// The routing axis: sends, modulation routes, and how they name what they
/// point at.
///
/// From [`signal_proto::node_routing`], because a route is domain — the
/// player drew it. The change that came with the move is the addressing:
/// a target used to be a display name, resolved afresh every time the tree
/// rendered, so renaming a block silently stopped every route into it. Now a
/// name is what *authoring* produces and an id is what is stored:
/// [`Container::resolve_refs`] converts one to the other once the tree is
/// complete, and after that a rename cannot miss.
pub use signal_proto::node_routing::{
    AudioSend as Send, ModRoute, ModSource as RouteSource, NodeRef,
};

/// A node-level setting that isn't a block, and the keyboard window a node
/// occupies.
///
/// Both now live in [`signal_proto::node_routing`] and are re-exported here:
/// they are domain, not audio. A Layer's key split is something a *player*
/// sets, so it belongs in the wire contract beside
/// [`Node`](signal_proto::node::Node) rather than in the crate that renders
/// it — there was one definition of a key split on each side of that line,
/// and they had to agree by hand.
pub use signal_proto::node_routing::{Setting as Param, Zone};

/// A container node: a named folder of children with a combine rule, plus the
/// routing-axis attachments (modulators + sends).
#[derive(Debug, Clone, Facet)]
pub struct Container {
    /// Stable identity — a UUIDv7, minted once and persisted.
    ///
    /// What everything *else* points at: a variant selecting this node, an
    /// override reaching into it, a scene recalling it. Addressing by `name`
    /// cannot do that job. Two siblings may share a name, and renaming one
    /// silently breaks every reference to it — an override that misses does
    /// not error at edit time or at switch time, it just stops applying, on
    /// stage.
    ///
    /// v7 rather than a hash or a counter because a rig is distributable:
    /// two people building a "Pad" on two machines must not mint the same
    /// id. (`seed_id`'s deterministic v5 is the opposite tool, for content
    /// that *must* agree everywhere — the shipped defaults.)
    ///
    /// Empty only between loading a tree written before ids and
    /// [`ensure_ids`](Self::ensure_ids) running over it.
    #[facet(default)]
    pub id: String,
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
    /// Control-rate modulation routes scoped to this subtree (the `ModMatrix`).
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
        Self::Block { block: b }
    }
}

impl From<Container> for RigNode {
    fn from(c: Container) -> Self {
        Self::Container { container: c }
    }
}

impl RigNode {
    #[must_use]
    pub fn name(&self) -> &str {
        match self {
            Self::Block { block: b } => &b.name,
            Self::Container { container: c } => &c.name,
        }
    }

    #[must_use]
    pub fn as_container(&self) -> Option<&Container> {
        match self {
            Self::Container { container: c } => Some(c),
            _ => None,
        }
    }
}

/// Resolve one reference, reporting whether it changed.
fn resolve_ref(
    reference: &mut NodeRef,
    lookup: &impl Fn(&str) -> Option<signal_proto::node::NodeId>,
) -> bool {
    if !reference.is_unresolved() {
        return false;
    }
    reference.resolve(lookup);
    !reference.is_unresolved()
}

/// A fresh node identity: UUIDv7, so ids from different machines never
/// collide and ids from one machine sort by when they were made.
#[must_use]
pub fn new_node_id() -> String {
    uuid::Uuid::now_v7().to_string()
}

impl Container {
    /// Mint an id for this node and everything under it that lacks one,
    /// returning how many were minted.
    ///
    /// Call after loading a tree, and **persist the result**. An id that is
    /// regenerated on each load is not an identity: every reference to it
    /// would break on the next start, which is worse than having none. The
    /// non-zero return is the caller's signal that the tree must be saved.
    pub fn ensure_ids(&mut self) -> usize {
        let mut minted = 0;
        if self.id.is_empty() {
            self.id = new_node_id();
            minted += 1;
        }
        for block in &mut self.modulators {
            if block.id.is_empty() {
                block.id = new_node_id();
                minted += 1;
            }
        }
        for child in &mut self.children {
            minted += match child {
                RigNode::Container { container } => container.ensure_ids(),
                RigNode::Block { block } => {
                    if block.id.is_empty() {
                        block.id = new_node_id();
                        1
                    } else {
                        0
                    }
                }
            };
        }
        minted
    }

    /// Make this tree canonical: mint every missing id, then resolve every
    /// authored route onto those ids. Returns `(ids minted, refs resolved)`.
    ///
    /// The order matters — a reference cannot resolve to an id that does not
    /// exist yet — which is the whole reason this exists rather than leaving
    /// callers to run the two passes themselves.
    ///
    /// This is what a caller runs **before persisting** a tree. Both halves
    /// warn about the same thing: an identity that is regenerated on each
    /// load is not an identity, so a tree that was canonicalized and not
    /// saved has gained nothing.
    pub fn canonicalize(&mut self) -> (usize, usize) {
        let minted = self.ensure_ids();
        let resolved = self.resolve_refs();
        (minted, resolved)
    }

    /// Turn every authored route in this tree into one that points by id,
    /// returning how many references were resolved.
    ///
    /// A builder can only name what it points at — a lane routes "To Rotary"
    /// before the Rotary exists, and a modulator's name is all
    /// [`route`](Self::route) is given. This is the pass that makes those
    /// names permanent, and it is the whole reason the routing axis is safe
    /// from a rename: after it runs there is no name left to miss.
    ///
    /// Call it after the tree is complete and after
    /// [`ensure_ids`](Self::ensure_ids) (a reference cannot resolve to an
    /// id that has not been minted), and **persist the result** for the same
    /// reason `ensure_ids` says to. A name matching nothing is left alone.
    ///
    /// **A name is resolved in the scope it was written in.** A mod route's
    /// target is looked up in the node's own subtree and its source among
    /// the modulators that node can reach — its own, then its ancestors'. A
    /// send's target is looked up across the whole tree, because a send is
    /// cross-tree by definition.
    ///
    /// Resolving globally instead is wrong in a way nothing reports: a keys
    /// layer holds two identical modules, each with its own "Filter Env"
    /// driving its own "Filter 1", and a global lookup points both modules'
    /// routes at the first module's pair — one envelope driving both
    /// filters, the other driving nothing.
    pub fn resolve_refs(&mut self) -> usize {
        let everything = self.name_index();
        self.resolve_refs_in(&everything, &[])
    }

    /// `everything` is the whole tree, for sends; `modulators` is this
    /// node's inherited modulator scope, nearest last.
    fn resolve_refs_in(
        &mut self,
        everything: &[(String, String)],
        modulators: &[(String, String)],
    ) -> usize {
        // This node's modulators, then its ancestors'. A route's source is
        // looked up nearest-first so an inner "Filter Env" wins.
        let mut scope: Vec<(String, String)> = self
            .modulators
            .iter()
            .map(|m| (m.id.to_lowercase(), m.display_name().to_string()))
            .collect();
        scope.extend(modulators.iter().cloned());

        // A mod route's target is a block in this node's own subtree.
        let subtree = self.name_index();

        let find = |index: &[(String, String)], name: &str| {
            index
                .iter()
                .find(|(_, n)| n.to_lowercase() == name)
                .map(|(id, _)| signal_proto::node::NodeId::from(id.clone()))
        };

        let mut resolved = 0;
        for send in &mut self.sends {
            resolved += usize::from(resolve_ref(&mut send.target, &|name| {
                find(everything, name)
            }));
        }
        for route in &mut self.mod_routes {
            resolved += usize::from(resolve_ref(&mut route.target, &|name| find(&subtree, name)));
            if let RouteSource::Node { node } = &mut route.source {
                resolved += usize::from(resolve_ref(node, &|name| find(&scope, name)));
            }
        }
        for child in &mut self.children {
            if let RigNode::Container { container } = child {
                resolved += container.resolve_refs_in(everything, &scope);
            }
        }
        resolved
    }

    /// Find a node anywhere in this subtree by **id** — how a selection or
    /// an override resolves its target.
    ///
    /// Distinct from [`find`](Self::find), which matches on `name`. That one
    /// is the older, fragile lookup: names are not unique among siblings and
    /// do not survive a rename. New references should use this.
    #[must_use]
    pub fn find_by_id(&self, id: &str) -> Option<&Self> {
        if self.id == id {
            return Some(self);
        }
        self.children.iter().find_map(|child| match child {
            RigNode::Container { container } => container.find_by_id(id),
            RigNode::Block { .. } => None,
        })
    }

    fn new(role: Role, name: impl Into<String>, combine: Combine) -> Self {
        Self {
            id: new_node_id(),
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
    #[allow(
        clippy::should_implement_trait,
        reason = "builder-DSL method, not std::ops::Add; renaming would touch ~40 call sites across nord.rs/node_render for no behavioral gain"
    )]
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
    /// The target is taken by name because that is all a builder has: a lane
    /// routes "To Rotary" before the Rotary exists. [`resolve_refs`] turns it
    /// into an id once the tree is whole.
    ///
    /// [`resolve_refs`]: Self::resolve_refs
    pub fn send(mut self, target: impl Into<String>, label: impl Into<String>) -> Self {
        self.sends.push(Send {
            target: NodeRef::name(target),
            label: label.into(),
        });
        self
    }

    /// Add a control-rate modulation route (a `ModMatrix` row) scoped to this
    /// subtree. See [`ModRoute`].
    #[must_use]
    /// `source` is a modulator's name or a performance gesture (`"Wheel"`,
    /// `"Velocity"`, `"CC74"`); `target` is `"Block Name.param"`. Both are
    /// names for the same reason [`send`](Self::send) takes one.
    pub fn route(
        mut self,
        source: impl Into<String>,
        target: impl Into<String>,
        depth: f32,
    ) -> Self {
        let target = target.into();
        // `"Filter.cutoff"` — the block on the left, the parameter on the
        // right. A target with no parameter addresses nothing, and is kept
        // whole so the warning names what was written.
        let (block, parameter) = target
            .rsplit_once('.')
            .map_or((target.as_str(), ""), |(b, p)| (b, p));
        let source = source.into();
        // A source is either a modulator in the tree or a gesture the player
        // makes, and only this crate knows the gesture vocabulary — so the
        // choice is made here, at authoring time, rather than left ambiguous
        // in the stored route.
        let source = if crate::native::ModSource::midi_by_name(&source).is_some() {
            RouteSource::performance(source)
        } else {
            RouteSource::node(NodeRef::name(source))
        };
        self.mod_routes.push(ModRoute {
            source,
            target: NodeRef::name(block),
            parameter: parameter.to_string(),
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
    ///
    /// # Errors
    ///
    /// Returns `SamplerError::SpecParse` if the string is not valid styx syntax.
    pub fn from_styx_str(text: &str) -> Result<Self, SamplerError> {
        facet_styx::from_str(text).map_err(|e| SamplerError::SpecParse(e.to_string()))
    }

    /// Parse a composition tree from a `.styx` file.
    ///
    /// # Errors
    ///
    /// Returns `SamplerError::SpecParse` if the file is not valid styx syntax,
    /// or a `std::io::Error` wrapped in `SamplerError` if the file cannot be read.
    pub fn from_styx_file(path: &std::path::Path) -> Result<Self, SamplerError> {
        let text = std::fs::read_to_string(path)?;
        Self::from_styx_str(&text)
    }

    /// Serialize this tree to a `.styx` string.
    ///
    /// # Errors
    ///
    /// Returns `SamplerError::SpecParse` if the tree cannot be serialized to styx.
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
    #[must_use]
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
    #[must_use]
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
    #[must_use]
    pub fn find(&self, name: &str) -> Option<&Self> {
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
    #[must_use]
    pub fn of_role(&self, role: Role) -> Vec<&Self> {
        let mut out = Vec::new();
        self.collect_role(role, &mut out);
        out
    }

    fn collect_role<'a>(&'a self, role: Role, out: &mut Vec<&'a Self>) {
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
    #[must_use]
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
    #[must_use]
    pub fn dump(&self) -> String {
        let mut s = String::new();
        // The diagram shows what a send *points at*, so a resolved send has
        // to be looked up: its reference holds an id, and only the node it
        // names knows what to call itself.
        let names = self.name_index();
        self.dump_into(&mut s, "", true, true, &names);
        s
    }

    /// Every `(lower-cased id, name)` in this subtree, including modulators
    /// — a mod route's source is one of those, and they hang off a node
    /// rather than sitting among its children.
    fn name_index(&self) -> Vec<(String, String)> {
        let mut out = vec![(self.id.to_lowercase(), self.name.clone())];
        for m in &self.modulators {
            out.push((m.id.to_lowercase(), m.display_name().to_string()));
        }
        for child in &self.children {
            match child {
                RigNode::Container { container: c } => out.extend(c.name_index()),
                RigNode::Block { block: b } => {
                    out.push((b.id.to_lowercase(), b.display_name().to_string()));
                }
            }
        }
        out
    }

    fn dump_into(
        &self,
        out: &mut String,
        prefix: &str,
        last: bool,
        root: bool,
        names: &[(String, String)],
    ) {
        let (branch, child_prefix) = if root {
            ("", String::new())
        } else if last {
            ("└─ ", format!("{prefix}   "))
        } else {
            ("├─ ", format!("{prefix}│  "))
        };
        out.push_str(prefix);
        out.push_str(branch);
        let _ = write!(
            out,
            "{} \"{}\" [{}]",
            self.role.tag(),
            self.name,
            self.combine.tag()
        );
        match self.role {
            // Layers/Engines/Presets have one native volume (the fader).
            Role::Layer | Role::Engine | Role::Preset => {
                let _ = write!(out, "  vol {:+.0}dB", self.output_db);
            }
            // Modules show in/out trim only when set.
            Role::Module if self.input_db != 0.0 || self.output_db != 0.0 => {
                let _ = write!(out, "  trim {:+.0}/{:+.0}dB", self.input_db, self.output_db);
            }
            Role::Module => {}
        }
        if !self.zone.is_full() {
            let z = &self.zone;
            let _ = write!(
                out,
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
            );
        }
        if !self.params.is_empty() {
            let ps: Vec<String> = self
                .params
                .iter()
                .map(|p| format!("{}={}", p.name, p.value))
                .collect();
            let _ = write!(out, "  {{{}}}", ps.join(", "));
        }
        for m in &self.modulators {
            let _ = write!(out, "  ~{}:{}", m.block_type_tag(), m.display_name());
        }
        for snd in &self.sends {
            let key = snd.target.key();
            let target = names
                .iter()
                .find(|(id, _)| *id == key)
                .map_or_else(|| snd.target.to_string(), |(_, name)| name.clone());
            let _ = write!(out, "  ⟿ {}→{}", snd.label, target);
        }
        out.push('\n');

        let n = self.children.len();
        for (i, child) in self.children.iter().enumerate() {
            let is_last = i + 1 == n;
            match child {
                RigNode::Container { container: c } => {
                    c.dump_into(out, &child_prefix, is_last, false, names);
                }
                RigNode::Block { block: b } => {
                    let bb = if is_last { "└─ " } else { "├─ " };
                    out.push_str(&child_prefix);
                    out.push_str(bb);
                    let _ = write!(
                        out,
                        "Block {} \"{}\"{}\n",
                        b.block_type_tag(),
                        b.display_name(),
                        if b.has_backend() {
                            ""
                        } else {
                            " (placeholder)"
                        }
                    );
                }
            }
        }
    }
}

impl RigBlock {
    /// Lowercase tag of the block's type (e.g. "amp", "delay") for display/dump.
    #[must_use]
    pub fn block_type_tag(&self) -> &'static str {
        self.block_type.as_str()
    }
}
