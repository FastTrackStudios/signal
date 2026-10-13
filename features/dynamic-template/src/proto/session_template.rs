//! The Ideal Full Session Template — the opinionated "golden session".
//!
//! This is the canonical full track/folder layout `FastTrackStudio` organizes
//! every session toward. It is a **schema**, not a flat track list: fixed
//! structural folders (instrument groups and sub-groups) followed by a chain
//! of *dimension* nodes — Performer, Arrangement, Layers, Channel, Multi-mic —
//! each carrying the configured [`vocabulary`](TemplateNode::vocabulary) of
//! descriptor names it may take. The engine expands and collapses that schema
//! per project, so the same template renders a one-off DI as `GTR E / DI` and
//! a fully tracked, multi-performer part as the full nested tree.
//!
//! These are **pure data** types. Categories are string paths (top-level
//! first, e.g. `["Guitars", "Electric"]`) matching the canonical taxonomy in
//! `music_catalog` — there is no typed instrument enum, so other repos can
//! describe layouts without coupling to a closed category set. The builder
//! that *derives* a golden template from the classification configuration
//! lives in the `dynamic-template` engine crate (it needs the `Config`),
//! not here.

use facet::Facet;

/// The opinionated full-session template: the ideal track/folder layout a
/// session is organized toward.
#[derive(Clone, Debug, Facet)]
pub struct IdealFullSessionTemplate {
    /// Human name for this template, e.g. `"FTS Golden Session"`.
    pub name: String,
    /// Schema version, bumped when the layout's shape changes.
    pub version: u32,
    /// Top-level nodes (folders / tracks), in display order.
    pub root: Vec<TemplateNode>,
    /// Routing destinations the layout sends into (mix buses, stems, etc.).
    pub buses: Vec<TemplateBus>,
}

/// What a [`TemplateNode`] represents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum NodeKind {
    /// A structural folder: an instrument group or sub-group (Drums, Drum Kit,
    /// Kick, Electric, …). Materializes whenever it has any content.
    Folder,
    /// A concrete leaf track.
    Track,
    /// A per-project deepening axis (Performer / Arrangement / Layers / Channel
    /// / Multi-mic). The engine instantiates one instance per project value
    /// drawn from [`vocabulary`](TemplateNode::vocabulary) and collapses the
    /// level when it resolves to a single value.
    Dimension,
    /// A *conditional* tagged collection (e.g. drum `SUM`): a folder that only
    /// materializes when a group has both items matching the collection's
    /// [`vocabulary`](TemplateNode::vocabulary) patterns and items outside it —
    /// the matching ones go inside, the rest sit alongside.
    Collection,
}

/// One node in the template tree; see [`NodeKind`] for the variants.
#[derive(Clone, Debug, Facet)]
pub struct TemplateNode {
    /// Display name, e.g. `"Drums"`, `"Kick"`, `"Arrangement"`, `"SUM"`.
    pub name: String,
    /// What this node represents.
    pub kind: NodeKind,
    /// Canonical group path this node belongs to (top-level first), matching
    /// the music-catalog taxonomy / classification-engine names. Empty for
    /// structural-only or dimension nodes that inherit. e.g. `["Drums", "Kick"]`.
    pub group_path: Vec<String>,
    /// Child nodes, in display order. For a dimension node these are the inner
    /// dimensions / captures; for a structural folder, its sub-folders.
    pub children: Vec<Self>,
    /// Per-track defaults (ignored for pure folders).
    pub defaults: TrackDefaults,
    /// Group-slot membership for this node (the canonical 128-slot partition).
    pub group_membership: Option<GroupMembership>,
    /// How this node routes its output.
    pub routing: NodeRouting,
    /// For a [`Dimension`](NodeKind::Dimension) or [`Collection`](NodeKind::Collection)
    /// node, the configured descriptor / pattern names it covers (e.g.
    /// Arrangement `["Clean", "Crunch", …]`, Multi-mic `["In", "Out", …]`, SUM
    /// `["In", "Out", "Trig"]`). Empty means free-form — named per take.
    pub vocabulary: Vec<String>,
}

impl TemplateNode {
    /// A structural folder carrying a canonical group path.
    pub fn folder(name: impl Into<String>, group_path: Vec<String>) -> Self {
        Self {
            name: name.into(),
            kind: NodeKind::Folder,
            group_path,
            children: Vec::new(),
            defaults: TrackDefaults::default(),
            group_membership: None,
            routing: NodeRouting::default(),
            vocabulary: Vec::new(),
        }
    }

    /// A leaf track (inherits color / membership from its parent folder).
    pub fn track(name: impl Into<String>) -> Self {
        Self {
            kind: NodeKind::Track,
            ..Self::folder(name, Vec::new())
        }
    }

    /// A per-project dimension node named `name` with the configured
    /// `vocabulary` of descriptor names (empty = free-form, named per take).
    pub fn dimension(name: impl Into<String>, vocabulary: Vec<String>) -> Self {
        Self {
            kind: NodeKind::Dimension,
            vocabulary,
            ..Self::folder(name, Vec::new())
        }
    }

    /// A conditional tagged-collection node (e.g. drum `SUM`) named `name`,
    /// covering the given `vocabulary` of pattern names. See
    /// [`NodeKind::Collection`].
    pub fn collection(name: impl Into<String>, vocabulary: Vec<String>) -> Self {
        Self {
            kind: NodeKind::Collection,
            vocabulary,
            ..Self::folder(name, Vec::new())
        }
    }

    /// Add a child node (builder-style).
    #[must_use]
    pub fn with_child(mut self, child: Self) -> Self {
        self.children.push(child);
        self
    }

    /// Append several child nodes (builder-style).
    #[must_use]
    pub fn with_children(mut self, children: impl IntoIterator<Item = Self>) -> Self {
        self.children.extend(children);
        self
    }

    /// Set the canonical group path from string slices (builder-style).
    #[must_use]
    pub fn with_path(mut self, path: &[&str]) -> Self {
        self.group_path = path.iter().map(|s| (*s).to_string()).collect();
        self
    }

    /// Assign this node to a canonical group-slot category (builder-style).
    #[must_use]
    pub fn in_group(mut self, category: impl Into<String>) -> Self {
        self.group_membership = Some(GroupMembership {
            category: category.into(),
        });
        self
    }
}

/// Per-track default settings the template seeds a track with.
#[derive(Clone, Debug, Facet)]
pub struct TrackDefaults {
    /// Track color as `#RRGGBB`, or `None` to inherit from the group path.
    pub color_hex: Option<String>,
    /// Input monitoring mode.
    pub monitor: MonitorMode,
    /// Hardware/track input, or `None` to leave unassigned.
    pub input: Option<TrackInput>,
    /// Whether the track is record-armed by default.
    pub record_armed: bool,
    /// Default fader volume in dB (0.0 = unity).
    pub volume_db: f64,
    /// Default pan, -1.0 (hard left) … 1.0 (hard right).
    pub pan: f64,
}

impl Default for TrackDefaults {
    fn default() -> Self {
        Self {
            color_hex: None,
            monitor: MonitorMode::Auto,
            input: None,
            record_armed: false,
            volume_db: 0.0,
            pan: 0.0,
        }
    }
}

/// Input-monitoring mode for a track.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum MonitorMode {
    /// Never monitor input.
    Off,
    /// Always monitor input.
    On,
    /// Monitor only while record-armed (REAPER "auto").
    Auto,
}

/// A track's input source.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct TrackInput {
    /// 1-based hardware channel.
    pub channel: u32,
    /// Stereo input (channel + channel+1) when true, else mono.
    pub stereo: bool,
}

/// Canonical group-slot membership: which slot band this node joins.
#[derive(Clone, Debug, PartialEq, Eq, Facet)]
pub struct GroupMembership {
    /// Canonical category whose slot band this node joins, resolved against
    /// `music_catalog::groups`. e.g. `"Guitars/Electric"`, `"Drums"`.
    pub category: String,
}

/// How a node routes its output.
#[derive(Clone, Debug, Facet)]
pub struct NodeRouting {
    /// Send post-fader to the parent folder (the usual case).
    pub parent_send: bool,
    /// Name of a [`TemplateBus`] this node additionally sends to, if any.
    pub bus: Option<String>,
}

impl Default for NodeRouting {
    fn default() -> Self {
        Self {
            parent_send: true,
            bus: None,
        }
    }
}

impl NodeRouting {
    /// Route this node into `bus` instead of its parent folder.
    ///
    /// The bus attachment point is where the instrument-folder tree hands off
    /// to the bus tree, so the parent send is dropped: a `Vocals / Lead`
    /// folder attached to `LEAD VOX BUS` feeds the bus, not the `Vocals`
    /// folder above it, which stays a pure organizational container. Nodes
    /// above and below the attachment point keep the default parent send.
    pub fn to_bus(bus: impl Into<String>) -> Self {
        Self {
            parent_send: false,
            bus: Some(bus.into()),
        }
    }
}

/// Whether a bus is emitted unconditionally or only when something routes
/// into it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum BusMaterialization {
    /// Emit the bus in every session, populated or not. Reserved for the
    /// anchors of the bus tree (`MIX BUS`).
    Always,
    /// Emit the bus only when at least one node routes into it, directly or
    /// through a descendant bus. This is the default and mirrors how the
    /// engine already collapses empty folders.
    WhenPopulated,
}

/// A routing destination (mix bus / stem) the layout sends into.
///
/// Buses form their own small tree alongside the instrument folders: a bus
/// feeds [`parent`](TemplateBus::parent), or the project master when that is
/// `None`. Instrument folders attach to a bus via
/// [`NodeRouting::bus`]; [`sources`](TemplateBus::sources) is the same
/// mapping read from the bus side, so a consumer can size the bus tree
/// without walking every node.
#[derive(Clone, Debug, Facet)]
pub struct TemplateBus {
    /// Bus name, e.g. `"DRUM BUS"`, `"MIX BUS"`.
    pub name: String,
    /// Channel count (2 = stereo).
    pub channels: u32,
    /// Bus this one feeds. `None` routes straight to the project master —
    /// that is how `MIX BUS`, `GUIDE BUS` and `UTILITY BUS` sit in parallel,
    /// so monitor-only material never prints into a bounce of the mix.
    pub parent: Option<String>,
    /// Canonical group paths that attach here (top-level first), e.g.
    /// `[["Vocals", "Lead"]]`. Empty for a bus that only sums other buses.
    pub sources: Vec<Vec<String>>,
    /// Bus color as `#RRGGBB`, or `None` to leave it unset.
    pub color_hex: Option<String>,
    /// Whether this bus is emitted when nothing routes into it.
    pub materialize: BusMaterialization,
}

impl TemplateBus {
    /// A stereo bus feeding the project master, emitted only when populated.
    pub fn stereo(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            channels: 2,
            parent: None,
            sources: Vec::new(),
            color_hex: None,
            materialize: BusMaterialization::WhenPopulated,
        }
    }

    /// Feed this bus into `parent` rather than the master (builder-style).
    #[must_use]
    pub fn into_bus(mut self, parent: impl Into<String>) -> Self {
        self.parent = Some(parent.into());
        self
    }

    /// Attach a canonical group path as a source of this bus (builder-style).
    #[must_use]
    pub fn from_path(mut self, path: &[&str]) -> Self {
        self.sources
            .push(path.iter().map(|s| (*s).to_string()).collect());
        self
    }

    /// Set the bus color (builder-style).
    #[must_use]
    pub fn colored(mut self, hex: impl Into<String>) -> Self {
        self.color_hex = Some(hex.into());
        self
    }

    /// Emit this bus unconditionally (builder-style).
    #[must_use]
    pub const fn always(mut self) -> Self {
        self.materialize = BusMaterialization::Always;
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builders_shape_nodes() {
        let kick = TemplateNode::folder("Kick", vec!["Drums".into(), "Kick".into()])
            .with_child(TemplateNode::dimension(
                "MultiMic",
                vec!["In".into(), "Out".into()],
            ))
            .in_group("Drums");

        assert_eq!(kick.group_path, vec!["Drums", "Kick"]);
        assert_eq!(kick.kind, NodeKind::Folder);
        assert_eq!(
            kick.group_membership.as_ref().map(|g| g.category.as_str()),
            Some("Drums")
        );

        let mics = &kick.children[0];
        assert_eq!(mics.name, "MultiMic");
        assert_eq!(mics.kind, NodeKind::Dimension);
        assert_eq!(mics.vocabulary, vec!["In", "Out"]);
        assert!(mics.children.is_empty());

        let sum = TemplateNode::collection("SUM", vec!["In".into(), "Out".into(), "Trig".into()]);
        assert_eq!(sum.kind, NodeKind::Collection);
    }
}
